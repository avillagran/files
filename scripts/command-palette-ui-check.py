#!/usr/bin/env python3
"""Command discovery and existing tools through an owned private fixture; no resource sampling."""
import argparse
import tomllib
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

parser=argparse.ArgumentParser();parser.add_argument("--operations",action="store_true");args=parser.parse_args()
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'docs/evidence/commands'
OUT.mkdir(parents=True, exist_ok=True)
BASE = Path.home() / '.cache/files-test'
BASE.mkdir(parents=True, exist_ok=True)
report = dict(functional_gate='IN_PROGRESS', resource_gate='NOT_MEASURED', startup_gate='NOT_MEASURED',
              method='owned filesystem fixture; production actions/painted hit targets and real FLTK framebuffer; no OS input',
              states=[], captures=[])
process = None
normal = False
with tempfile.TemporaryDirectory(prefix='commands-', dir=BASE) as temp:
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
    destination=base/os.fsdecode(b'B-\xfe');destination.mkdir()
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
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_DUAL_B':str(destination)}
        with log.open('w') as output:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','120'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        def selected(s):return {bytes.fromhex(n) for n in s['selection']}
        def commands(s):return s['commands']
        def selected_id(s):return commands(s)['matches'][commands(s)['selected']]['id']
        def open_keys():return act('commands-shortcut',lambda s:commands(s) is not None)
        def query(q):return act('commands-query:'+q)
        def choose(id):return act('commands-row:'+id)
        def execute(id,check=lambda s:s['commands'] is None):
            open_keys();query(id);choose(id);return act('commands-run-hit',check)
        state();act('list');act('selection-clear')
        if args.operations:
            s=act('bar-toggle',lambda s:s['operation_bar']['enabled'] and not s['config_dirty'])
            assert len(s['operation_bar']['buttons'])==8
            s=act('bar-hit:3');assert s['dual']['copy_form'] is None and s['error'].startswith('MC: ')
            capture('operations-no-selection')
            act('filter-focus');s=act('filter-text:alpha');assert s['filter_editor']['active']
            s=act('bar-hit:3');assert s['filter_editor']['active'] and s['query']=='alpha' and s['dual']['copy_form'] is None
            capture('operations-text-focus-blocked')
            act('filter-key:escape');act('select-name:alpha.rs')
            s=act('bar-hit:1',lambda s:s['dialog']=='rename');capture('operations-rename-review');act('dialog-cancel')
            s=act('bar-hit:5',lambda s:s['dialog']=='folder');capture('operations-new-folder-review');act('dialog-cancel')
            s=act('bar-hit:0',lambda s:s['help'] is not None);act('help-close')
            for resize,w,h in [('fixture-short',800,440),('fixture-minimum',440,440),('fixture-wide',1000,560)]:
                s=act(resize,lambda s:s['width']==w and s['height']==h)
                buttons=s['operation_bar']['buttons'];assert len(buttons)==8
                for b in buttons:
                    x,y,bw,bh=b['rect'];assert x>=0 and x+bw<=w and y>=h-68 and y+bh<=h-32
                capture('operations-controls-'+str(w))
            act('selection-clear')
        act('clip-actions');s=act('commands-menu-hit')
        assert len(commands(s)['matches'])==59 and not selected(s) and s['selected'] is None
        capture('commands-discovery')
        s=query('rename');assert not commands(s)['run_hit']
        s=act('commands-key:enter');assert 'Selecciona' in commands(s)['error'] and s['dual']['copy_form'] is None
        capture('commands-disabled-reason')
        s=query('view-grid');assert selected_id(s)=='view-grid'
        s=act('commands-run-hit');assert commands(s) is None and s['view']=='grid'
        # Keyboard selection and list pages, no native input synthesis.
        open_keys();s=query('');s=act('commands-key:end');assert selected_id(s)=='recovery'
        s=act('commands-key:home');assert selected_id(s)=='search'
        s=act('commands-key:pagedown');assert commands(s)['selected']>0
        act('commands-key:escape');s=execute('select-all');assert len(selected(s))==5
        # A stale file-context is rejected, then only an explicit refresh recaptures it.
        open_keys();query('view-list');s=act('commands-context-change');assert not commands(s)['context_current']
        s=act('commands-key:enter');assert commands(s) is not None and s['view']=='grid'
        capture('commands-stale-context')
        act('commands-recapture-hit');s=act('commands-run-hit');assert s['view']=='list' and commands(s) is None
        # Plain-text callback is private; a changed input epoch rejects it.
        open_keys();act('path-paste-prepare');query('permisos');offset=command('path-paste-deliver');s=state(offset)
        assert commands(s)['query']=='permisos' and 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:]
        s=query('no such command zzzz');assert not commands(s)['matches'] and not commands(s)['run_hit'];capture('commands-no-match')
        s=query('');act('fixture-short',lambda s:s['width']==800 and s['height']==440);capture('commands-800x440')
        s=act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);assert commands(s)['row_hits']<=commands(s)['page'];capture('commands-440x440')
        act('commands-key:escape');act('clip-actions');capture('commands-actions-menu-440x440')
        s=act('commands-menu-hit');assert commands(s) is not None
        act('commands-key:escape');act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
        # Dispatch the actual directory calculator, not a duplicate implementation.
        act('select-name:folder');s=execute('sizes-one',lambda s:s['commands'] is None and not s['folder_usage']['queue_work'] and not s['folder_usage']['running'] and bool(s['folder_usage']['rows']))
        measured={bytes(r['name']):r for r in s['folder_usage']['rows']};assert measured[b'folder']['label']=='∑ 6 B'
        capture('commands-directory-size-result')
        # Filter-to-mark command preserves raw names and then uses the global copy queue.
        act('filter-query:ext:rs !beta type:file');s=execute('filtered');assert selected(s)=={b'alpha.rs',rawname} and s['query']==''
        act('dual-on');act('dual-switch');act('dual-b-location',lambda s:bytes(s['path_raw'])==os.fsencode(destination));act('dual-switch')
        if args.operations:
            s=act('bar-hit:4',lambda s:s['dual']['copy_form'] is not None)
            assert s['dual']['copy_form']['moving']
            assert not list(destination.iterdir());capture('operations-move-review')
            act('clip-cancel-dialog');assert not list(destination.iterdir())
            s=act('bar-hit:3',lambda s:s['dual']['copy_form'] is not None)
        else:
            s=execute('copy',lambda s:s['commands'] is None and s['dual']['copy_form'] is not None)
        form=s['dual']['copy_form'];assert {bytes.fromhex(n) for n in form['names']}=={b'alpha.rs',rawname}
        assert bytes.fromhex(form['source'])==os.fsencode(source) and bytes.fromhex(form['destination'])==os.fsencode(destination)
        assert not list(destination.iterdir());capture('commands-raw-copy-review')
        s=act('clip-confirm',lambda s:s['dual']['copy_success'] and not s['dual']['copy_busy'])
        assert {os.fsencode(p.name) for p in destination.iterdir()}=={b'alpha.rs',rawname}
        assert (destination/'alpha.rs').read_bytes()==(source/'alpha.rs').read_bytes()
        assert open(os.fsencode(destination)+b'/'+rawname,'rb').read()==b'opaque source'
        capture('commands-copy-completed')
        # Existing search opens as a new modal after command dispatch, then closes normally.
        s=execute('search',lambda s:s['commands'] is None and s['search']['visible']);capture('commands-to-search')
        act('recursive-close');open_keys();query('help')
        command('callback');wait(lambda:process.poll() is not None,'normal close absent',True)
        text=log.read_text();assert process.returncode==0 and 'FILES_CLOSE_DRAINED' in text
        if args.operations:
            prefs=tomllib.loads((config/'config.toml').read_text());assert prefs['operation_bar'] and prefs['fixture_note']=='preserve' and not prefs['show_hidden']
            start=len(text);seq=0
            with log.open('a') as output:
                process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--ui-smoke','--ui-scene','hidden-context','--size','800x440','--scale','1','--exit-after','120'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
            s=state(start,lambda s:s['operation_bar']['enabled'] and len(s['operation_bar']['buttons'])==8);capture('operations-restored-preference')
            s=act('bar-toggle',lambda s:not s['operation_bar']['enabled'] and not s['config_dirty']);assert not s['operation_bar']['buttons']
            command('callback');wait(lambda:process.poll() is not None,'second normal close absent',True)
            text=log.read_text();assert process.returncode==0 and text.count('FILES_CLOSE_DRAINED')==2
            prefs=tomllib.loads((config/'config.toml').read_text());assert not prefs['operation_bar'] and prefs['fixture_note']=='preserve'
            report['operation_bar_scope']='8 painted buttons, disabled empty/text-focus does not launch or blur, rename/new-folder review cancel, help, geometry440/800/1000, dual Move review cancel then raw Copy completed, true restored in new process then false persisted; no native input'

        identities=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=([^\s]+)',text));assert identities
        for pid,start,role in identities:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'helper identity still exists, including Z: '+pid
        for name,digest in before.items():assert hashlib.sha256((source/name).read_bytes()).hexdigest()==digest
        report.update(functional_gate='PASS',logged_helper_identities=sorted(identities),helper_cleanup='logged identities absent or reused; Z not accepted; no lifetime coverage',scope='59 static command discovery, aliases/shortcut search, disabled/no-match, modal/context/paste guards, selection/pages, 800/440 layout, actual directory calculator, raw filter-to-mark and reviewed dual-panel copy queue with completed bytes, command-to-search and callback close0ACK; private handlers/real FLTK framebuffer, no native input, OS clipboard, performance or process-tree budget claim')
        normal=True
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
        target = OUT / (('operation-bar-ui-' if args.operations else 'command-palette-ui-') + str(time.time_ns()) + '.json')
        with target.open('x') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
        print(target)
