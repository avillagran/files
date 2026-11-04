#!/usr/bin/env python3
"""Owned functional fixture: local filters/marking/folder size; no resource sampling."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import tempfile
import time

parser=argparse.ArgumentParser();parser.add_argument('--editing',action='store_true');args=parser.parse_args()
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'docs/evidence/usability'
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
    for name in ['final-2026.rs', 'draft-2026.rs', 'report-2026.txt', 'annual report.txt', '.hidden-2026.rs']:
        (source / name).write_bytes(b'fixture\n')
    rawname = b'annual-2026-\xff.rs'
    with open(os.fsencode(source) + b'/' + rawname, 'wb') as output:
        output.write(b'raw\n')
    folder = source / 'Archive'
    folder.mkdir()
    (folder / 'sub').mkdir()
    (folder / 'a').write_bytes(b'123')
    (folder / 'sub/b').write_bytes(b'12345')
    (folder / 'sub/cycle').symlink_to(folder)
    (folder / 'broken').symlink_to('missing')
    before = {p.relative_to(source).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in source.rglob('*') if p.is_file() and not p.is_symlink()}
    other=base/'Other';other.mkdir();(other/'separate.txt').write_bytes(b'other private fixture')
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

    def state(after=0, predicate=lambda s: True, allow_error=False):
        def current():
            rows = []
            for line in log.read_text()[after:].splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    s = json.loads(line[len('FILES_HIDDEN_STATE '):])
                    if s['seq'] == seq and predicate(s):
                        rows.append(s)
            return rows[-1] if rows else None
        s = wait(current, 'painted state absent for seq ' + str(seq))
        assert allow_error or not s['error'], s['error']
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
               'FILES_TEST_FIXED_SIZE': '1', 'FILES_CLOSE_TRACE': '1','FILES_TEST_DUAL_B':str(other)}
        if args.editing:env.update(FILES_TEST_PREVIEW='1',FILES_TEST_TABS='1')
        with log.open('w') as output:
            process = subprocess.Popen([str(ROOT / 'target/release/files'), str(source), '--ui-smoke',
                                        '--ui-scene', 'hidden-context', '--size', '1000x650', '--scale', '1', '--exit-after', '90'],
                                       env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
        state()
        act('list')
        if args.editing:
            expr='2026 ext:rx !draft type:file';good=expr.replace('ext:rx','ext:rs')
            s=act('filter-focus');assert s['filter_editor']['active']
            act('filter-text:'+expr)
            act('filter-point:10');s=act('filter-drag:11');assert s['filter_editor']['selected_text']=='x'
            capture('filter-edit-pointer-selection')
            s=act('filter-text:s');assert names(s)=={b'final-2026.rs',rawname} and s['query']==good
            s=act('filter-key:undo');assert s['query']==expr and not names(s)
            s=act('filter-key:redo');assert s['query']==good and len(names(s))==2
            act('filter-point:10');act('filter-drag:11');offset=command('filter-key:copy');s=state(offset)
            assert 'FILES_FILTER_PRIVATE_COPY "s"' in log.read_text()[offset:] and s['clipboard']['count']==0
            for key in ['f2','f5','f6']:
                s=act('filter-key:'+key);assert s['dialog'] is None and s['dual']['copy_form'] is None
            act('path-paste-prepare');act('filter-key:left');offset=command('path-paste-deliver');s=state(offset)
            assert s['query']==good and 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:]
            before_epoch=s['filter_editor']['epoch'];offset=command('filter-huge');s=state(offset,allow_error=True)
            assert s['query']==good and s['filter_editor']['epoch']==before_epoch and '4096' in s['error']
            capture('filter-edit-rejected-paste-keeps-query')
            s=state(command('filter-key:all'),allow_error=True)
            assert '4096' in s['error'] and s['filter_editor']['selected_text']==good
            act('filter-text:ñandú');act('filter-key:home');s=act('filter-key:shiftright')
            assert s['filter_editor']['selected_text']=='ñ';capture('filter-edit-unicode-selection')
            s=act('filter-text:Ñ');assert s['query']=='Ñandú'
            s=act('filter-key:undo');assert s['query']=='ñandú'
            act('filter-key:redo');act('filter-key:all');act('filter-text:'+good)
            s=act('filter-focus');assert s['query']==good and s['filter_editor']['selected_text']==good
            s=act('filter-key:enter');assert not s['filter_editor']['active'] and s['query']==good
            act('path-open-hit');s=act('path-cancel-hit');assert not s['editing'] and s['query']==good
            # Actual pane and tab transitions retain queries, never an old input target/history.
            act('dual-on');act('dual-switch');act('dual-b-location',lambda s:bytes(s['path_raw'])==os.fsencode(other))
            act('filter-query:*.txt');act('path-paste-prepare');act('dual-switch')
            offset=command('path-paste-deliver');s=state(offset)
            assert s['query']==good and not s['filter_editor']['active'] and 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:]
            capture('filter-edit-pane-query-preserved')
            s=act('dual-switch');assert s['query']=='*.txt';act('dual-close-panel')
            s=act('tab-new');assert not s['filter_editor']['active']
            act('filter-query:*.txt');act('path-paste-prepare');act('tab-a')
            offset=command('path-paste-deliver');s=state(offset)
            assert s['query']==good and not s['filter_editor']['active'] and 'FILES_PATH_PASTE accepted=false' in log.read_text()[offset:]
            s=act('tab-b');assert s['query']=='*.txt';act('tab-close')
            act('filter-focus');act('filter-text:'+'a'*4096)
            act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);capture('filter-edit-long-query-end-440')
            act('filter-key:home');capture('filter-edit-long-query-start-440')
            act('filter-key:escape');act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
            act('filter-query:'+good);act('filter-key:enter')
            s=act('filter-list-backspace',lambda s:bytes(s['path_raw'])==os.fsencode(base))
            assert not s['query'] and not s['filter_editor']['active']
            act('tab-a-location',lambda s:bytes(s['path_raw'])==os.fsencode(source))
            assert (other/'separate.txt').read_bytes()==b'other private fixture'
        s = act('filter-query:2026 ext:rs !draft type:file')
        expected = {b'final-2026.rs', rawname}
        assert names(s) == expected and all(bytes.fromhex(n) in expected for n in s['selection'])
        assert capture('filter-combined') == ((1000,560) if args.editing else (1000,650))
        s = act('grid')
        assert names(s) == expected
        s = act('columns')
        assert names(s) == expected
        s = act('filter-query:"annual report"')
        assert names(s) == {b'annual report.txt'}
        capture('filter-literal-columns')
        s = act('filter-query:*.txt !report-2026*')
        assert names(s) == {b'annual report.txt'}
        s = act('filter-clear-hit')
        assert len(s['order']) == 6 and not s['query']
        act('list')
        act('select-name:annual report.txt')
        s = act('mark-next')
        assert not s['selection'] and s['selected'] != b'annual report.txt'.hex()
        s = act('mark-next')
        assert len(s['selection']) == 1
        s = act('mark-next')
        assert len(s['selection']) == 2 and s['selected'] not in s['selection']
        capture('mc-marked-batch-and-focus')
        act('filter-query:type:dir')
        s = act('select-name:Archive')
        assert names(s) == {b'Archive'}
        s = act('size-hit', lambda s: s['folder_usage']['text'] == 'Aparente: 8 B · 5 entradas')
        assert not s['folder_usage']['running'] and not s['folder_usage']['pending']
        assert capture('folder-size-count') == ((1000,560) if args.editing else (1000,650))
        # Recalculation after mutation reads current contents instead of retaining an old result.
        (folder / 'new').write_bytes(b'1234')
        s = act('size-hit', lambda s: s['folder_usage']['text'] == 'Aparente: 12 B · 6 entradas')
        capture('folder-size-recalculated')
        act('filter-query:ext:rs !draft')
        s = act('fixture-short', lambda s: s['width'] == 800 and s['height'] == 440)
        assert names(s) == expected
        assert capture('filter-initial-small') == (800, 440)
        command('callback')
        wait(lambda: process.poll() is not None, 'normal close absent', True)
        text = log.read_text()
        assert process.returncode == 0 and 'FILES_CLOSE_DRAINED' in text
        if args.editing:
            identities=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=measure_directory',text));assert identities
            for pid,start in identities:
                try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
                except FileNotFoundError:continue
                assert parts[19]!=start,'helper still exists, including zombie: '+pid
            report['logged_helper_identities']=sorted(identities)
            report['helper_cleanup']='logged PID/start absent or reused; Z not accepted; not lifetime coverage'
        for name, digest in before.items():
            assert hashlib.sha256((source / name).read_bytes()).hexdigest() == digest
        assert (folder / 'sub/cycle').is_symlink()
        report['filter_editing']=args.editing
        if args.editing:report['editing_scope']='painted byte-position pointer/drag and text-only private copy; F2/F5/F6 suppressed; Unicode middle edit/undo/redo; active huge paste rejected atomically; delayed paste rejected after caret, pane and tab change; query retained by CtrlF and per-pane/tab persistence; path pencil reachable with query; 4096B start/end horizontal viewport440; then original local-filter/MC/du regressions. No native input/clipboard/resource proof'
        report.update(functional_gate='PASS', scope='local name filters, cross-view visible set, raw names, MC marking, directory-only helper 8B/5 entries then 12B/6, bounded initial-small framebuffer and callback drain; no native input/interoperability/resource claim')
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
        target = OUT / (('filter-editing-ui-' if args.editing else 'usability-ui-') + str(time.time_ns()) + '.json')
        with target.open('x') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
        print(target)
