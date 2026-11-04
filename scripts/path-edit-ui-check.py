#!/usr/bin/env python3
"""Owned functional fixture: local filters/marking/folder size; no resource sampling."""
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
OUT = ROOT / 'docs/evidence/path-edit'
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
    target=source/'beta/é/leaf';target.mkdir(parents=True);(target/'verified.txt').write_bytes(b'content-preserved')
    rawname=b'raw-%2F-\xff';rawpath=os.fsencode(base)+b'/'+rawname;os.mkdir(rawpath)
    with open(rawpath+b'/owned.txt','wb') as output:output.write(b'opaque')
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
               'FILES_TEST_FIXED_SIZE': '1', 'FILES_CLOSE_TRACE': '1'}
        with log.open('w') as output:
            process = subprocess.Popen([str(ROOT / 'target/release/files'), str(source), '--ui-smoke',
                                        '--ui-scene', 'hidden-context', '--size', '1000x650', '--scale', '1', '--exit-after', '90'],
                                       env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
        state();act('list');s=act('omni');assert s['replace_edit'] and bytes.fromhex(s['path_editor']['selected']).decode()==str(source)
        capture('path-initial-selection')
        act('path-text:alpha/日本😀/leaf');act('path-key:plain:home');act('path-key:ctrl:right');act('path-key:plain:right')
        s=act('path-key:ctrlshift:right');assert bytes.fromhex(s['path_editor']['selected']).decode()=='日本😀'
        s=act('path-text:é');assert s['edit']=='alpha/é/leaf' and s['path_editor']['caret']==len('alpha/é'.encode())
        capture('path-midline-unicode')
        s=act('path-key:plain:delete');assert s['edit']=='alpha/éleaf'
        s=act('path-key:ctrl:z');assert s['edit']=='alpha/é/leaf'
        act('path-key:plain:home');s=act('path-key:ctrlshift:right');old=s
        s=act('paste-large');assert s['edit']==old['edit'] and s['path_editor']==old['path_editor'] and s['error'];capture('path-rejected-paste-retains-selection')
        s=act('path-text:beta');assert s['edit']=='beta/é/leaf' and not s['error']
        act('path-key:plain:home');act('path-click:0');s=act('path-drag:4');act('path-release');assert bytes.fromhex(s['path_editor']['selected'])==b'beta';capture('path-mouse-selection')
        s=act('path-key:ctrl:c');assert s['edit']=='beta/é/leaf'
        s=act('path-key:ctrl:x');assert s['edit']=='/é/leaf'
        s=act('path-key:ctrl:z');assert s['edit']=='beta/é/leaf'
        s=act('path-double:5');assert bytes.fromhex(s['path_editor']['selected']).decode()=='é';act('path-release')
        s=act('path-accept-hit',lambda s:s['path']==str(target) and not s['editing']);assert b'verified.txt'.hex() in s['order'];capture('path-committed-unicode-directory')
        act('omni');act('path-text:file://remote/private');s=act('path-key:plain:enter');assert s['editing'] and s['path']==str(target) and s['error']
        from urllib.parse import quote_from_bytes
        uri='file://'+quote_from_bytes(rawpath,safe='/')
        act('path-key:ctrl:a');act('path-text:'+uri);s=act('path-key:plain:enter',lambda s:bytes(s['path_raw'])==rawpath and not s['editing'])
        s=act('omni');assert '%FF' in s['edit'] and '%252F' in s['edit'];capture('path-raw-uri-lossless')
        s=act('path-key:plain:enter',lambda s:not s['editing']);assert bytes(s['path_raw'])==rawpath
        act('omni');act('path-paste-prepare');s=act('path-key:plain:home');old=s['edit']
        s=act('path-paste-deliver');assert s['edit']==old and not s['clipboard']['text_pending']
        act('path-paste-prepare');s=act('path-paste-deliver');assert s['edit'].startswith('changedfile://')
        act('path-key:ctrl:z');act('fixture-short',lambda s:s['width']==800 and s['height']==440)
        act('path-key:ctrl:a');act('path-text:'+('longword-'*80))
        s=act('path-key:plain:end');assert s['path_editor']['scroll']>0;capture('path-small-horizontal-scroll')
        s=act('path-key:plain:home');assert s['path_editor']['scroll']==0;capture('path-small-start-caret')
        act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('path-minimum-editor')
        s=act('path-cancel-hit');assert not s['editing'] and bytes(s['path_raw'])==rawpath
        capture('path-minimum-browse-header')
        s=act('path-open-hit');assert s['editing'] and '%FF' in s['edit'];act('path-cancel-hit')
        s=act('view-menu-hit');assert s['context']['target']=='Views' and len(s['context']['items'])==6;capture('path-minimum-view-menu');act('escape')
        assert open(rawpath+b'/owned.txt','rb').read()==b'opaque'
        command('callback')
        wait(lambda: process.poll() is not None, 'normal close absent', True)
        text = log.read_text()
        assert process.returncode == 0 and 'FILES_CLOSE_DRAINED' in text
        for name, digest in before.items():
            assert hashlib.sha256((source / name).read_bytes()).hexdigest() == digest
        report.update(functional_gate='PASS', scope='path UTF8 midline replacement/delete/undo, pointer drag/component selection through painted geometry, oversized paste atomicity, directory commit, explicit raw URI and exactly-once decode, stale paste after cursor move, 800x440 horizontal scroll, cancel and callback drain; no native OS input/clipboard/IME publication or resource/startup claim')
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
        target = OUT / ('path-edit-ui-' + str(time.time_ns()) + '.json')
        with target.open('x') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
        print(target)
