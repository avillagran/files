#!/usr/bin/env python3
"""Diagnostic only: sampled tree plus bounded /proc mapping snapshots. Not a gate.
Run serialized without builds/captures/other benchmarks.
"""
import argparse, hashlib, json, os, re, subprocess, tempfile, time
from pathlib import Path
from memcheck import ROOT, TreeSampler, geometry, observed_geometry, scene_readiness, stop_tree, hypr_json


def mappings(pid):
    groups = {}
    name = None
    for line in Path(f'/proc/{pid}/smaps').read_text().splitlines():
        if re.match(r'^[0-9a-f]+-[0-9a-f]+ ', line):
            parts = line.split(None, 5)
            name = parts[5] if len(parts) > 5 else '[anonymous]'
            groups.setdefault(name, {'mappings': 0, 'Size': 0, 'Pss': 0, 'SwapPss': 0, 'Anonymous': 0, 'Private_Dirty': 0, 'Shared_Dirty': 0})['mappings'] += 1
        elif name:
            key, _, rest = line.partition(':')
            if key in groups[name]:
                groups[name][key] += int(rest.split()[0])
    return sorted(({'name': key, **values, 'memory_kib': values['Pss'] + values['SwapPss']} for key, values in groups.items()), key=lambda row: row['memory_kib'], reverse=True)


def main():
    args = argparse.ArgumentParser()
    args.add_argument('--mmap-threshold', type=int)
    options = args.parse_args()
    app_env = dict(os.environ)
    if options.mmap_threshold:
        app_env['MALLOC_MMAP_THRESHOLD_'] = str(options.mmap_threshold)
    base = Path.home()/'.cache/files-test'
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='mapping-', dir=base) as d:
        directory = Path(d)/'files'; directory.mkdir()
        for i in range(10000): (directory/f'file-{i:06}.txt').touch()
        with tempfile.TemporaryFile(dir=base) as log:
            process = subprocess.Popen([str(ROOT/'target/release/files'), str(directory), '--size','960x540','--scale','1'], stdout=subprocess.DEVNULL, stderr=log, env={**app_env,'FILES_CONFIG_DIR':str(Path(d)/'config'), 'FILES_METRICS_READY':'1'}, start_new_session=True)
            sampler = TreeSampler(process.pid)
            snapshots = []
            trace = []
            start = time.monotonic(); last_peak = -1024; sized = None; ready = None; next_geometry = 0; geometries = []
            try:
                while time.monotonic()-start < 6:
                    tick = time.monotonic(); sample = sampler.sample()
                    trace.append({'elapsed_ms':1000*(tick-start), **sample})
                    if sample['memory_kib'] > last_peak+512 and len(snapshots)<32:
                        last_peak = sample['memory_kib']
                        snapshots.append({'phase':'new sampled maximum','elapsed_ms':1000*(tick-start),'tree':sample,'root_mappings':mappings(process.pid), 'client_after_mapping_read':next((c for c in hypr_json('clients') if c['pid']==process.pid), None), 'mapping_read_completed_ms':1000*(time.monotonic()-start)})
                    if tick >= next_geometry:
                        before = next((c for c in hypr_json('clients') if c['pid']==process.pid), None)
                        geometries.append({'elapsed_ms':1000*(time.monotonic()-start), 'before_request':before})
                        geometry(process.pid,960,540)
                        sized=observed_geometry(process.pid,960,540)
                        next_geometry = time.monotonic()+.2
                    ready=scene_readiness(os.pread(log.fileno(),8192,0).decode(errors='replace'),'groups',10000)
                    if process.poll() is not None: raise RuntimeError('app exited')
                    time.sleep(max(0,.025-(time.monotonic()-tick)))
                snapshots.append({'phase':'stable end','elapsed_ms':1000*(time.monotonic()-start),'tree':sampler.sample(),'root_mappings':mappings(process.pid), 'client_after_mapping_read':next((c for c in hypr_json('clients') if c['pid']==process.pid), None), 'mapping_read_completed_ms':1000*(time.monotonic()-start)})
            finally:
                cleanup=stop_tree(process,sampler)
            print(json.dumps({'acceptance':False,'scope':'diagnostic smaps reads alter sampling cadence; peaks lower bounds; no lifetime or GPU guarantee','geometry':sized,'readiness':ready,'snapshots':snapshots,'samples':trace,'cleanup':cleanup, 'mmap_threshold_env':options.mmap_threshold, 'geometry_history':geometries, 'binary_sha256_after_cleanup':hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()},indent=2))

if __name__=='__main__': main()
