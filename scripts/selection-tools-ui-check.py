#!/usr/bin/env python3
"""Owned selection tools fixture; no resource sampling."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'docs/evidence/selection-tools'
OUT.mkdir(parents=True, exist_ok=True)
BASE = Path.home() / '.cache/files-test'
BASE.mkdir(parents=True, exist_ok=True)
report = dict(functional_gate='IN_PROGRESS', resource_gate='NOT_MEASURED', startup_gate='NOT_MEASURED',
              method='owned filesystem fixture; production actions/painted hit targets and real FLTK framebuffer; no OS input',
              states=[], captures=[])
process = None
normal = False
with tempfile.TemporaryDirectory(prefix='usability-', dir=BASE) as temp:
    base = Path(temp)
    source = base / 'Documents'
    source.mkdir()
    config = base / 'config'
    config.mkdir()
    (config / 'config.toml').write_text('show_hidden=false\nfixture_note="preserve"\n')
    for name in ['alpha.rs', 'beta.rs', 'notes.txt', '.hidden.rs']:
        (source/name).write_bytes(('owned '+name).encode())
    rawname=b'raw-\xff.rs'
    with open(os.fsencode(source)+b'/'+rawname,'wb') as output:output.write(b'opaque source')
    folder=source/'folder';folder.mkdir();(folder/'inside').write_bytes(b'nested')
    destination=base/'B';destination.mkdir()
    before = {p.relative_to(source).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in source.rglob('*') if p.is_file() and not p.is_symlink()}
    raw = base / 'frame.ppm'
    log = base / 'app.log'
    seq = 0

    def wait(check, reason, allow_exit=False):
        end = time.monotonic() + 25
        while time.monotonic() < end:
            value = check()
            if value:
                return value
            if process.poll() is not None and not allow_exit:
                raise AssertionError(reason + ': app exited ' + str(process.returncode))
            time.sleep(.02)
        raise AssertionError(reason)

    def state(after=0, predicate=lambda s: True):
        def current():
            rows = []
            for line in log.read_text()[after:].splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    s = json.loads(line[len('FILES_HIDDEN_STATE '):])
                    if s['seq'] == seq and predicate(s):
                        rows.append(s)
            return rows[-1] if rows else None
        s = wait(current, 'painted state absent for seq ' + str(seq))
        report['states'].append(s)
        return s

    def command(action):
        global seq
        offset = len(log.read_text())
        pending = raw.with_suffix('.pending')
        pending.write_text(action)
        pending.replace(raw.with_suffix('.command'))
        match = wait(lambda: re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action=' + re.escape(action) + r'$', log.read_text()[offset:], re.M),
                     'command not acknowledged: ' + action, action == 'callback')
        seq = int(match[1])
        return offset

    def act(action, predicate=lambda s: True):
        return state(command(action), predicate)

    def names(s):
        return {bytes.fromhex(name) for name in s['order']}

    def capture(label):
        raw.unlink(missing_ok=True)
        raw.with_suffix('.request').touch()
        def full():
            if not raw.exists():
                return None
            parts = raw.read_bytes().split(b'\n', 3)
            if len(parts) != 4:
                return None
            w, h = map(int, parts[1].split())
            return (w, h) if len(parts[3]) == w * h * 3 else None
        dims = wait(full, 'incomplete framebuffer')
        target = OUT / (label + '-' + str(time.time_ns()) + '.png')
        subprocess.run(['magick', str(raw), str(target)], check=True, timeout=15)
        report['captures'].append(dict(path=str(target), dimensions=dims))
        return dims

    try:
        env = {**os.environ, 'FILES_CONFIG_DIR': str(config), 'FILES_TEST_CAPTURE': str(raw),
               'FILES_TEST_HIDDEN_CONTEXT': '1', 'FILES_TEST_ISOLATE_INPUT': '1',
               'FILES_TEST_FIXED_SIZE': '1', 'FILES_CLOSE_TRACE': '1', 'FILES_TEST_DUAL_B':str(destination)}
        with log.open('w') as output:
            process = subprocess.Popen([str(ROOT / 'target/release/files'), str(source), '--ui-smoke',
                                        '--ui-scene', 'hidden-context', '--size', '1000x650', '--scale', '1', '--exit-after', '90'],
                                       env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
        def selected(s):return {bytes.fromhex(n) for n in s['selection']}
        def pattern():
            act('filter-query:ext:rs !beta type:file')
            act('clip-actions');act('selection-menu')
            s=act('selection-menu-filtered')
            assert s['query']=='' and selected(s)=={b'alpha.rs',rawname} and len(s['order'])==5
            return s
        state();act('list');s=pattern();capture('pattern-marks-show-all')
        act('filter-query:ext:rs !beta type:file');act('clip-actions');act('selection-menu');capture('selection-controls-menu');act('selection-menu-filtered')
        act('right-background');capture('selection-background-menu');s=act('selection-background-all');assert len(selected(s))==5
        s=pattern()
        s=act('selection-invert');assert selected(s)=={b'folder',b'beta.rs',b'notes.txt'}
        capture('inverted-visible-marks')
        s=act('selection-clear');assert not selected(s) and s['selected'] is None and s['anchor'] is None
        s=act('dual-copy');assert s['dual']['copy_form'] is None and not selected(s) and s['selected'] is None
        capture('empty-selection-no-copy')
        act('clip-dismiss')
        act('filter-query:type:invalid');s=act('selection-filtered');assert s['query']=='type:invalid' and s['error'].startswith('Selección:')
        s=pattern();assert not s['error'];act('grid');capture('grid-pattern-marks')
        act('columns');s=pattern();capture('columns-pattern-marks')
        s=act('dual-copy');assert {bytes.fromhex(n) for n in s['dual']['copy_form']['names']}=={b'alpha.rs',rawname}
        act('clip-cancel-dialog');act('list');act('dual-on');act('dual-switch')
        act('dual-b-location',lambda s:bytes(s['path_raw'])==os.fsencode(destination) and s['dual']['panels'] and not s['order'])
        act('selection-clear');act('dual-switch');s=state()
        assert selected(s)=={b'alpha.rs',rawname} and bytes(s['path_raw'])==os.fsencode(source)
        s=act('dual-copy');f=s['dual']['copy_form'];assert {bytes.fromhex(n) for n in f['names']}=={b'alpha.rs',rawname}
        assert bytes.fromhex(f['destination'])==os.fsencode(destination);capture('marked-copy-opposite-panel-review')
        act('clip-confirm',lambda s:s['dual']['copy_success'] and not s['dual']['copy_busy'])
        assert {os.fsencode(p.name) for p in destination.iterdir()}=={b'alpha.rs',rawname}
        assert (destination/'alpha.rs').read_bytes()==(source/'alpha.rs').read_bytes()
        assert open(os.fsencode(destination)+b'/'+rawname,'rb').read()==b'opaque source'
        act('fixture-short',lambda s:s['width']==800 and s['height']==440);capture('dual-selection-small-window')
        act('clip-actions');act('selection-menu');capture('selection-controls-small-window');act('escape')
        act('dual-switch');s=act('selection-all');assert selected(s)=={b'alpha.rs',rawname}
        s=act('selection-invert');assert not selected(s) and s['selected'] is None
        s=act('selection-clear');assert not selected(s)
        command('callback')
        wait(lambda: process.poll() is not None, 'normal close absent', True)
        text = log.read_text()
        assert process.returncode == 0 and 'FILES_CLOSE_DRAINED' in text
        for name, digest in before.items():
            assert hashlib.sha256((source / name).read_bytes()).hexdigest() == digest
        report.update(functional_gate='PASS', scope='pattern filter-to-mark/show-all, invert/clear/empty no fallback, raw names/hidden policy, List/Grid/Columns, dual-pane owner isolation, exact copy review and completed bytes, 800x440 and callback drain; production handlers and framebuffer, no native input/clipboard or resource/startup claim')
        normal = True
    except BaseException as error:
        report.update(functional_gate='FAIL', error=repr(error))
        raise
    finally:
        report['log'] = log.read_text() if log.exists() else ''
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        report['normal_exit'] = normal
        for key, name in [('binary_sha256', 'files'), ('helper_sha256', 'files-helper'), ('html_helper_sha256', 'files-html-helper')]:
            report[key] = hashlib.sha256((ROOT / 'target/release' / name).read_bytes()).hexdigest()
        target = OUT / ('selection-tools-ui-' + str(time.time_ns()) + '.json')
        with target.open('x') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
        print(target)
