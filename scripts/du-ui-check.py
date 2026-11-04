#!/usr/bin/env python3
"""Run separately for resource evidence and framebuffer capture (internal UI actions)."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
from memcheck import Metrics, stop_tree
from process_metrics import TreeSampler

parser = argparse.ArgumentParser()
parser.add_argument('--capture', action='store_true')
parser.add_argument('--count', type=int, default=10000)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
evidence = root / 'docs/evidence/du'
evidence.mkdir(parents=True, exist_ok=True)
base = Path.home() / '.cache/files-test'
base.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='files-du-ui-', dir=base) as tmp:
    test = Path(tmp)
    folder = test / 'files' / 'Documents'
    folder.mkdir(parents=True)
    for i in range(args.count):
        (folder / f'note-{i:05}.txt').write_bytes(b'files\n' * 100)
    raw = test / 'capture.ppm'
    env = {**os.environ, 'FILES_CONFIG_DIR': str(test / 'config')}
    if args.capture:
        env['FILES_TEST_CAPTURE'] = str(raw)
    log = test / 'app.log'
    with log.open('w') as output:
        process = subprocess.Popen([str(root / 'target/release/files'), str(folder.parent), '--size', '1000x650', '--scale', '1', '--ui-smoke', '--ui-scene', 'du'], env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
    sampler = None if args.capture else TreeSampler(process.pid)
    metrics = Metrics()
    ready = None
    helpers = set()
    try:
        deadline = time.monotonic() + 25
        while time.monotonic() < deadline:
            if process.poll() is not None:
                raise RuntimeError(log.read_text())
            if sampler:
                sample = sampler.sample()
                metrics.add(sample)
                for item in sample["processes"]:
                    try:
                        if Path(f'/proc/{item["pid"]}/exe').resolve() == root / "target/release/files-helper":
                            helpers.add((item["pid"], item["start"]))
                    except OSError:
                        pass
            text = log.read_text()
            if f'FILES_DU_OK Aparente: {args.count * 600 / 1000000:.1f} MB' in text and ready is None:
                ready = time.monotonic()
            if ready and time.monotonic() - ready > 1:
                break
            time.sleep(.025)
        assert ready is not None, log.read_text()
        if args.capture:
            raw.with_suffix('.request').touch()
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline and not raw.exists():
                time.sleep(.025)
            assert raw.exists(), log.read_text()
            time.sleep(.15)
            subprocess.run(['magick', str(raw), str(evidence / 'inspector.png')], check=True)
        else:
            report = metrics.report(ready)
            report['scenario'] = f'inspector recursive apparent size, {args.count} files'
            report['ui_action_method'] = 'internal action; no native OS input'
            report['window_requested'] = [1000, 650]
            report['resource_gate'] = 'UNVALIDATED'
            report['helper_identities_verified'] = sorted(helpers)
            if report['complete'] and helpers:
                report['resource_gate'] = 'PASS' if report['sampled_maxima']['memory_kib'] * 1024 < 150_000_000 else 'FAIL'
            report['memory_peak_mb_decimal'] = report['sampled_maxima']['memory_kib'] * 1024 / 1e6
            report['functional_result'] = f'PASS: {args.count * 600} bytes'
    finally:
        cleanup = stop_tree(process, sampler)
    if not args.capture:
        report['cleanup'] = cleanup
        if not cleanup['complete']:
            report['resource_gate'] = 'UNVALIDATED'
        (evidence / f'memory-{args.count}.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps({'result':report['functional_result'], 'peak_mb': report['memory_peak_mb_decimal'], 'gate': report['resource_gate']}))
    else:
        print(json.dumps({'capture':str(evidence / 'inspector.png'), 'method':'FLTK framebuffer, internal UI action'}))
