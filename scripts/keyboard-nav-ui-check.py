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
OUT = ROOT / 'docs/evidence/keyboard-nav'
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
    now=time.time()
    for i in range(72):
        p=source/f'file-{i:03}.txt';p.write_bytes(f'owned-{i}'.encode())
        age=[0,2,20][i%3];os.utime(p,(now-age*86400,now-age*86400))
    for i in range(12):
        p=source/f'folder-{i:02}';p.mkdir();(p/'inside.txt').write_bytes(b'nested')
    (source/'.hidden.txt').write_bytes(b'hidden')
    rawname=b'zz-raw-\xff.txt'
    with open(os.fsencode(source)+b'/'+rawname,'wb') as output:output.write(b'opaque source')
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
        def focused_visible(s):return s['selected'] is not None and s['selected'] in s['visible']
        state();act('list')
        s=act('nav:plain:end',focused_visible);assert bytes.fromhex(s['selected'])==rawname and s['scroll']>0
        s=act('nav:shift:up',focused_visible);expected={b'file-071.txt',rawname};assert selected(s)==expected
        capture('list-end-range')
        s=act('nav:ctrl:home',focused_visible);assert selected(s)==expected and s['selected']==b'folder-00'.hex()
        s=act('nav:ctrlshift:down',focused_visible);assert selected(s)==expected|{b'folder-00',b'folder-01'}
        capture('ctrl-focus-and-additive-range')
        s=act('nav:plain:home');s=act('nav:plain:page-down',focused_visible);assert s['selected']==s['order'][12]
        s=act('nav:shift:page-down',focused_visible);assert len(selected(s))==13
        capture('list-page-range')
        act('nav:plain:end');s=act('nav:shift:up');act('dual-copy')
        s=state();assert {bytes.fromhex(n) for n in s['dual']['copy_form']['names']}==expected
        capture('keyboard-range-copy-review');act('clip-cancel-dialog')
        act('selection-clear');s=act('nav:shift:down');assert selected(s)=={b'folder-00'}
        act('omni');s=act('nav:plain:page-down');assert s['editing'] and selected(s)=={b'folder-00'};act('omni-cancel')
        s=act('groups',lambda s:s.get('group_ends')==[12,37,61,85,85])
        assert bytes.fromhex(s['order'][12])==b'file-000.txt' and bytes.fromhex(s['order'][-1])==b'file-071.txt'
        act('select-name:folder-11');s=act('nav:plain:down',focused_visible);assert s['selected']==b'file-000.txt'.hex()
        s=act('nav:plain:end',focused_visible);assert s['selected']==b'file-071.txt'.hex();capture('groups-visual-last-and-reveal')
        s=act('nav:shift:home',focused_visible);assert len(selected(s))==85;capture('groups-visual-range')
        act('back',lambda s:s['path']==str(base));act('history-back',lambda s:s['path']==str(source))
        act('columns');s=act('nav:plain:end',focused_visible);assert bytes.fromhex(s['selected'])==rawname
        s=act('nav:shift:up',focused_visible);assert selected(s)==expected;capture('columns-keyboard-range')
        act('select-name:folder-00');s=act('nav:plain:right',lambda s:s['path']==str(source/'folder-00'))
        s=act('nav:plain:left',lambda s:s['path']==str(source))
        act('grid');act('nav:plain:home');s=act('nav:ctrl:end',focused_visible);assert selected(s)=={b'folder-00'} and bytes.fromhex(s['selected'])==rawname
        capture('grid-independent-focus')
        act('dual-on');s=act('nav:plain:home');s=act('nav:shift:down',focused_visible);assert selected(s)=={b'folder-00',b'folder-01'}
        act('dual-switch');s=act('dual-b-location',lambda s:bytes(s['path_raw'])==os.fsencode(destination) and not s['order'])
        s=act('nav:plain:end');assert s['selected'] is None and not selected(s)
        s=act('dual-switch');assert selected(s)=={b'folder-00',b'folder-01'}
        act('fixture-short',lambda s:s['width']==800 and s['height']==440);s=act('nav:ctrl:end',focused_visible);assert selected(s)=={b'folder-00',b'folder-01'}
        capture('dual-focused-end-small-window')
        s=act('map',lambda s:s['map'] and s['map']['state']=='ready');assert s['map']['complete']
        s=act('nav:plain:home');assert len(selected(s))==1
        first=s['selected'];s=act('nav:ctrlshift:end');assert len(selected(s))==1 and s['selected']!=first
        capture('treemap-distinct-single-selection')
        command('callback')
        wait(lambda: process.poll() is not None, 'normal close absent', True)
        text = log.read_text()
        assert process.returncode == 0 and 'FILES_CLOSE_DRAINED' in text
        for name, digest in before.items():
            assert hashlib.sha256((source / name).read_bytes()).hexdigest() == digest
        report.update(functional_gate='PASS', scope='Home/End/Page/Shift ranges/Ctrl independent focus/additive ranges, exact raw copy review, empty focus and editing guard, Groups visual age order and folder boundary, Columns parent/child, Grid and dual-pane owner isolation, 800x440 and callback drain; shared production navigation handlers, no native OS key input/resource/startup claim')
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
        target = OUT / ('keyboard-nav-ui-' + str(time.time_ns()) + '.json')
        with target.open('x') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
        print(target)
