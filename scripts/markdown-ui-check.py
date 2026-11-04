#!/usr/bin/env python3
"""Rendered Markdown acceptance fixtures; captures and resource runs are separate.
Internal handlers + actual FLTK framebuffer, never native OS input or clipboard proof.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
from memcheck import Metrics, geometry, observed_geometry, stop_tree
from process_metrics import TreeSampler, read_stat

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--capture', action='store_true')
parser.add_argument('--embedded', action='store_true')
parser.add_argument('--checked-relative', action='store_true')
args = parser.parse_args()
if args.checked_relative and not (args.capture and args.embedded):
    parser.error("--checked-relative requires --capture --embedded; no resource runs in this phase")
evidence = ROOT/'docs/evidence/0.0.4'
evidence.mkdir(parents=True, exist_ok=True)
base = Path.home()/'.cache/files-test'
base.mkdir(parents=True, exist_ok=True)
report = {'method': 'internal selection/link/scroll handlers + real FLTK framebuffer; compositor geometry requested and verified; no native input',
          'captures': [], 'clipboard': 'NOT_VALIDATED; selection bytes checked inside app, no clipboard read',
          'resource_gate': 'NOT_MEASURED' if args.capture else 'UNVALIDATED',
          'known_pending': ['embedded images', 'grapheme/ZWJ shaping and navigation', 'HTML renderer', 'clipboard without Wayland focus']}
def competing_builds():
    result=[]
    for stat in Path('/proc').glob('[0-9]*/comm'):
        try:
            name=stat.read_text().strip()
            if name in {'cargo','rustc','cmake','cc1plus','cc1','clang','clang++','ninja','make'}:
                identity=read_stat(stat.with_name('stat').read_text())
                if identity['state']!='Z':result.append({'pid':int(stat.parent.name),'start':identity['start'],'comm':name})
        except (FileNotFoundError,PermissionError,ValueError,IndexError):
            pass
    return result

if not args.capture:
    busy=competing_builds()
    if busy:
        report.update(functional_gate='NOT_RUN',resource_gate='UNVALIDATED',interference=busy,reason='Build competitor observed before launch; no app or performance measurement started')
        path=evidence/('markdown-resource-blocked-'+str(time.time_ns())+'.json')
        path.write_text(json.dumps(report,indent=2)+'\n')
        print(str(path))
        raise SystemExit(2)
    report['build_isolation_preflight']='no listed compiler/build process observed; this is not complete host isolation'

with tempfile.TemporaryDirectory(prefix='markdown-ui-', dir=base) as tmp:
    test = Path(tmp)
    folder = test/'files'
    shutil.copytree(ROOT/'scripts/fixtures/markdown', folder)
    # Complete valid document near the source cap; many visual wraps and no I/O
    # except the single bounded text helper. Not a baseline 10k-file scenario.
    large = '# Documento acotado\n\n```text\n' + ('dato_'+'abcdefghijklmnopqrstuvwxyz_'*7+'漢字\n')*1000+'```\n'
    assert len(large.encode()) < 256*1024
    (folder/'large.md').write_text(large)
    (folder/'too-many.md').write_text('x\n\n'*1100)
    (test/'outside.md').write_text('# private fixture sentinel\n')
    (folder/'linked.md').symlink_to(test/'outside.md')
    if args.embedded:
        import struct, zlib
        def png(path, rgb):
            def chunk(kind,data):
                return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
            rows=(b'\0'+bytes(rgb)*1024)*1024
            path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1024,1024,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(rows))+chunk(b'IEND',b''))
        png(folder/'large-a.png',(100,180,230))
        png(folder/'large-b.png',(230,100,180))
        (folder/'linked.png').symlink_to(folder/'palette.png')
        (folder/'embedded.md').write_text('# Imágenes locales acotadas\n\n'+
            '![primera](palette.png)\n\n![repetida](./palette.png)\n\n![alias normalizado](unused/../palette.png)\n\n'+
            '![grande A](large-a.png)\n\n![grande B fuera de presupuesto](large-b.png)\n\n'+
            '![escape](../outside.png)\n\n![symlink](linked.png)\n\n![remota](https://example.invalid/no.png)\n\n![repetida al final](palette.png)\n')
        report['known_pending']=['embedded invalidation after publication and parent watchers','cross-document LRU with safe invalidation','grapheme/ZWJ shaping','clipboard without Wayland focus']
    raw = test/'capture.ppm'
    log = test/'app.log'
    env = {**os.environ, 'FILES_CONFIG_DIR': str(test/'config'), 'FILES_TEST_CAPTURE': str(raw), 'FILES_TEST_PREVIEW': '1'}
    with log.open('w') as out:
        process = subprocess.Popen([str(ROOT/'target/release/files'), str(folder), '--size', '1000x650', '--scale', '1', '--ui-smoke', '--ui-scene', 'markdown', '--exit-after', '45'], env=env, stderr=out, stdout=subprocess.DEVNULL, start_new_session=True)
    sampler = None if args.capture else TreeSampler(process.pid, discovery='children')
    metrics = Metrics()
    helpers = set()
    terminal = []
    stable_since = stable_until = None
    normal = False
    latencies = []

    def sample():
        if sampler and process.poll() is None:
            value = sampler.sample()
            if not value['root_alive'] and not value['errors'] and not value['processes']:
                terminal.append(value)
            else:
                metrics.add(value)
            for item in value['processes']:
                try:
                    if Path(f'/proc/{item["pid"]}/exe').resolve() == ROOT/'target/release/files-helper':
                        helpers.add((item['pid'], item['start']))
                except OSError:
                    pass

    def wait(check, reason, allow_exit=False, timeout=12):
        deadline = time.monotonic()+timeout
        while time.monotonic() < deadline:
            sample()
            value = check()
            if value:
                return value
            if process.poll() is not None and not allow_exit:
                raise AssertionError(reason+'; process exited\n'+log.read_text())
            time.sleep(.005 if sampler else .025)
        raise AssertionError(reason+'\n'+log.read_text())

    def command(action):
        before = len(log.read_text())
        pending = raw.with_suffix('.pending')
        pending.write_text(action)
        os.replace(pending, raw.with_suffix('.command'))
        wait(lambda: 'FILES_PREVIEW_COMMAND_OK '+action+'\n' in log.read_text()[before:], 'command failed: '+action, action.startswith('close'))
        match = wait(lambda: re.search(r'FILES_PREVIEW_COMMAND_REV action='+re.escape(action)+r' revision=(\d+)', log.read_text()[before:]), 'command revision absent', action.startswith('close'))
        return int(match[1]), before

    def ready(name, action=None, width=1000, height=650, after=None, revision=None, truncated=False):
        started = time.monotonic()
        if action:
            revision, after = command(action)
        if after is None:
            after = 0
        def check():
            markers = [line for line in log.read_text()[after:].splitlines() if line.startswith(('FILES_PREVIEW_MARKDOWN_READY', 'FILES_PREVIEW_ERROR'))]
            if not markers:
                return False
            line = markers[-1]
            return line.startswith('FILES_PREVIEW_MARKDOWN_READY') and '/'+name+'"' in line and 'layout_ready=true' in line and 'layout_error=None' in line and ('truncated='+str(truncated).lower()+' ') in line and f'width={min(width-32,1160)-36} height={min(height-44,800)-146} ' in line and (revision is None or f'revision={revision} ' in line)
        wait(check, 'fresh fully laid-out Markdown absent: '+name)
        if action:
            latencies.append({'action': action, 'through_observed_layout_ready_ms': (time.monotonic()-started)*1000})
        return revision

    def capture(name):
        raw.unlink(missing_ok=True)
        raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():
                return False
            parts = raw.read_bytes().split(b'\n', 3)
            return len(parts) == 4 and len(parts[3]) == int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete, 'framebuffer absent')
        parts = raw.read_bytes().split(b'\n', 3)
        path = evidence/(name+'-'+str(time.time_ns())+'.png')
        subprocess.run(['magick', str(raw), str(path)], check=True)
        report['captures'].append({'path': str(path), 'framebuffer_dimensions': [int(n) for n in parts[1].split()]})

    def cleanup():
        survivors, errors = [], []
        for pid, start in sorted(sampler.known if sampler else []):
            try:
                stat = read_stat(Path(f'/proc/{pid}/stat').read_text())
                if stat['start'] == start and stat['state'] != 'Z':
                    survivors.append([pid, start])
            except FileNotFoundError:
                if Path(f'/proc/{pid}').exists():
                    errors.append(f'{pid}: stat absent but directory present')
            except (OSError, ValueError, IndexError) as error:
                errors.append(f'{pid}/{start}: {error}')
        return {'complete': not survivors and not errors, 'survivors': survivors, 'errors': errors,
                'scope': 'observed PID/start tree after normal exit' if sampler else 'root exit0 only; no descendant sampler'}

    try:
        wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'UI not ready')
        wait(lambda: geometry(process.pid, 1000, 650) and observed_geometry(process.pid, 1000, 650), 'geometry unvalidated')
        report['geometry'] = observed_geometry(process.pid, 1000, 650)
        if args.embedded:
            ready('embedded.md','markdown-embedded')
            command('markdown-image-assert')
            marker=[line for line in log.read_text().splitlines() if line.startswith('FILES_PREVIEW_MARKDOWN_READY')][-1]
            loaded=int(re.search(r'images_loaded=(\d+)',marker)[1])
            retained=int(re.search(r'image_bytes=(\d+)',marker)[1])
            assert loaded>=4 and retained<4*1024*1024, 'dedupe or retained image cap failed'
            if args.checked_relative:
                assert 'checked_root=true' in marker and 'checked_resources=2 ' in marker, 'descriptor-bound resource manifest missing'
                report['checked_relative_binding']='actual document-root identity plus 2 unique probe/decoder FD proofs; reused occurrences share RGB; later FS invalidation still pending'
            report['embedded_initial']={'loaded_occurrences':loaded,'retained_capacity_bytes':retained,'source_references':9,'unique_decoded_retained_expected':2,'cross_document_lru':False}
            if args.capture:
                capture('markdown-embedded-palette')
                pixels=raw.read_bytes().split(b'\n',3)[3]
                colors=[(224,96,80),(112,184,112),(56,112,176)]
                counts=[sum(pixels[i:i+3]==bytes(c) for i in range(0,len(pixels),3)) for c in colors]
                assert all(n>=1000 for n in counts),'palette bitmap pixels absent from real framebuffer'
                report['framebuffer_palette_exact_color_counts']=counts
                command('markdown-scroll')
                capture('markdown-embedded-large-bitmap')
                pixels=raw.read_bytes().split(b'\n',3)[3]
                blue=sum(pixels[i:i+3]==bytes((100,180,230)) for i in range(0,len(pixels),3))
                assert blue>5000,'large admitted bitmap not visible'
                report['framebuffer_admitted_large_bitmap_exact_color_pixels']=blue
            command('markdown-bottom')
            if args.capture:capture('markdown-embedded-budget-and-security')
            ready('contextual-links.md','markdown-contextual')
            command('markdown-context-reject')
            ready('guide.md','markdown-context-guide')
            command('markdown-image-assert');command('markdown-context-reject')
            if args.capture:capture('markdown-contextual-guide-image')
            ready('detail.md','markdown-context-detail')
            command('markdown-image-assert');command('markdown-context-reject')
            if args.capture:capture('markdown-contextual-detail-image')
            ready('guide.md','markdown-context-back')
            ready('contextual-links.md','markdown-context-root')
            command('markdown-image-assert')
            report['contextual_parent_navigation']='root→subdir→nested→subdir→root; each escape rejected without revision or helper spawn, local images visible'
        else:
            revision=ready('overview.md', 'markdown')
            if args.capture:
                capture('markdown-overview')
                command('markdown-select')
                assert 'FILES_MARKDOWN_SELECTION' in log.read_text()
                capture('markdown-selection')
                resize_after=len(log.read_text())
                wait(lambda: geometry(process.pid, 800, 500) and observed_geometry(process.pid, 800, 500), 'resize geometry unvalidated')
                ready('overview.md',width=800,height=500,after=resize_after,revision=revision)
                command('markdown-assert-selection')
                capture('markdown-resize')
                command('markdown-bottom')
                capture('markdown-bottom')
                restore_after=len(log.read_text())
                wait(lambda: geometry(process.pid, 1000, 650) and observed_geometry(process.pid, 1000, 650), 'restore geometry unvalidated')
                ready('overview.md',after=restore_after,revision=revision)
                command('markdown-top')
            ready('guide.md', 'markdown-guide')
            if args.capture:
                capture('markdown-local-guide')
            ready('overview.md', 'markdown-return')
            _,error_after=command('markdown-symlink')
            wait(lambda: any(line.startswith('FILES_PREVIEW_ERROR ') and '/linked.md"' in line for line in log.read_text()[error_after:].splitlines()), 'fresh exact symlink error absent')
            if args.capture:
                capture('markdown-symlink-rejected')
            ready('overview.md', 'markdown')
            ready('too-many.md','markdown-too-many',truncated=True)
            report['line_cap_behavior']='fresh revision READY truncated=true after helper2000LF cap, not a renderer rejection'
            if args.capture:
                capture('markdown-helper-truncated')
            ready('large.md', 'markdown-large')
            command('markdown-bottom')
            if args.capture:
                capture('markdown-large-bottom')
            ready('overview.md', 'markdown-rapid')
        stable_since = time.monotonic()
        until = stable_since+1.5
        while time.monotonic() < until:
            sample()
            time.sleep(.005 if sampler else .025)
        stable_until = time.monotonic()
        command('close-markdown-pending' if args.capture else 'close')
        wait(lambda: process.poll() is not None, 'normal exit did not finish', True)
        assert process.returncode == 0 and 'FILES_PREVIEW_DRAINED' in log.read_text(), 'exit without drained ACK'
        normal = True
        report.update(functional_gate='PASS', normal_exit_after_preview_drain=True, cleanup=cleanup(),
                      latencies=latencies, log=log.read_text(),
                      fixtures={'listed_entries': len(list(folder.iterdir())), 'large_markdown_source_bytes': len(large.encode()), 'source_sha256': hashlib.sha256((folder/'overview.md').read_bytes()).hexdigest()})
        if sampler:
            report['metrics'] = metrics.report(stable_since, stable_until)
            report['metrics']['sample_interval_requested_ms'] = 5
            report.update(stable_since_monotonic=stable_since, stable_until_monotonic=stable_until,
                          stable_phase='1.5s post-layout Ready; may include software paints, not settled idle guaranteed', terminal_exit_observations=terminal)
            records = re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=preview_(?:relative_text|relative_image|text)', log.read_text())
            spawned = {(int(pid), int(start)) for pid, start in records if start.isdigit()}
            unknown = [(pid, start) for pid, start in records if not start.isdigit()]
            missing = spawned-helpers
            report.update(spawned_helpers=sorted(spawned), helper_identities_verified=sorted(helpers), helpers_without_memory_sample=sorted(missing), helpers_unknown_starttime=unknown)
            peak = report['metrics']['sampled_maxima']['memory_kib']*1024/1e6
            validated = report['metrics']['complete'] and report['cleanup']['complete'] and spawned and not missing and not unknown
            report['sampled_peak_memory_mb_decimal'] = peak
            report['resource_gate'] = 'FAIL' if peak > 150 else ('PASS' if validated else 'UNVALIDATED')
            report['goal_met'] = peak <= 50 if validated else None
            report['observed_goal_exceeded'] = peak > 50
            report['resource_scope'] = ('embedded/contextual images; RGB retained4MiB + scratch4MiB auxiliary, not process budget; ' if args.embedded else '')+'sampled whole app tree PSS+Swap; all logged helpers need memory/exe identity; not full lifetimes/exact peak; no GPU; rendered Markdown overview/local links+200k code document, not baseline10k'
    except BaseException as error:
        report.update(functional_gate='FAIL', error=repr(error), log=log.read_text(), latencies=latencies)
        if sampler:
            report['metrics'] = metrics.report(stable_since or time.monotonic(), stable_until)
        path = evidence/('markdown-failure-'+str(time.time_ns())+'.json')
        path.write_text(json.dumps(report, indent=2)+'\n')
        print(str(path))
        raise
    finally:
        if not normal:
            stop_tree(process, sampler)
    # Hash after sampling/cleanup, never warm the measured launch beforehand.
    report['binary_sha256'] = hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['helper_sha256'] = hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    path = evidence/((('markdown-embedded-ui-' if args.capture else 'markdown-embedded-resources-') if args.embedded else ('markdown-ui-' if args.capture else 'markdown-resources-'))+str(time.time_ns())+'.json')
    path.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'report': str(path), 'functional_gate': report['functional_gate'], 'resource_gate': report['resource_gate']}))
    if report['functional_gate'] != 'PASS' or not args.capture and report['resource_gate'] != 'PASS':
        raise SystemExit(1)
