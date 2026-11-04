#!/usr/bin/env python3
"""Functional-only rename regex fixtures. No performance/resource sampling."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from memcheck import stop_tree

parser = argparse.ArgumentParser()
parser.add_argument('--operation-labels', action='store_true')
args = parser.parse_args()
ROOT = Path(__file__).resolve().parent.parent
BASE = Path.home() / '.cache/files-test'
BASE.mkdir(parents=True, exist_ok=True)
EVIDENCE = ROOT / 'docs/evidence/0.0.4'
EVIDENCE.mkdir(parents=True, exist_ok=True)
report = {'operation_labels': args.operation_labels, 'method': 'internal UI handlers, real filesystem fixtures and FLTK framebuffer; no native OS input',
          'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED', 'captures': [], 'batch': 2}
with tempfile.TemporaryDirectory(prefix='rename-regex-', dir=BASE) as tmp:
    test = Path(tmp)
    folder = test / os.fsdecode(b'files-\xff\n' if args.operation_labels else b'files')
    folder.mkdir()
    payloads = [b'first\x00\xff', 'Unicode 漢字\t\r\n'.encode()]
    for i, value in enumerate(payloads, 1):
        (folder / f'photo{i}.txt').write_bytes(value)
    (folder / 'untouched.txt').write_bytes(b'unchanged')
    raw, log = test / 'capture.ppm', test / 'app.log'
    env = {**os.environ, 'FILES_CONFIG_DIR': str(test/'config'), 'FILES_TEST_CAPTURE': str(raw),
           'FILES_TEST_MUTATIONS': '1', 'FILES_METRICS_READY': '1'}
    with log.open('w') as out:
        process = subprocess.Popen([str(ROOT/'target/release/files'), str(folder), '--size', '1000x650',
                                    '--ui-smoke', '--ui-scene', 'rename', '--exit-after', '60'],
                                   env=env, stderr=out, stdout=subprocess.DEVNULL, start_new_session=True)
    normal = False

    def wait(check, reason, allow_exit=False):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            value = check()
            if value:
                return value
            if process.poll() is not None and not allow_exit:
                raise AssertionError(reason + '; premature exit\n' + log.read_text())
            time.sleep(.02)
        raise AssertionError(reason + '\n' + log.read_text())

    def command(action):
        offset = len(log.read_text())
        pending = raw.with_suffix('.pending')
        pending.write_text(action)
        os.replace(pending, raw.with_suffix('.command'))
        wait(lambda: 'FILES_MUTATION_COMMAND_OK '+action+'\n' in log.read_text()[offset:], 'command missing: '+action,
             allow_exit=action == 'close')
        return offset

    def preview(action, valid):
        offset = command(action)
        prefix = r'FILES_RENAME_STATE id=([1-9][0-9]*) regex=true ready='+str(valid).lower()
        suffix = r' changes=(\d+) idle=true error=("[^\n]*")'

        def fresh():
            for match in re.finditer(prefix+suffix, log.read_text()[offset:]):
                if valid and match[2] == '2' and match[3] == '""':
                    return match[1]
                if not valid and match[2] == '0' and match[3] != '""':
                    return match[1]
            return False
        return wait(fresh, 'fresh preview/error revision missing: '+action)

    def unchanged(renamed):
        for i, value in enumerate(payloads, 1):
            old, new = folder/f'photo{i}.txt', folder/f'imagen-{i}.txt'
            expected, absent = (new, old) if renamed else (old, new)
            assert expected.read_bytes() == value and not absent.exists()
        assert (folder/'untouched.txt').read_bytes() == b'unchanged'
        assert not any(p.name.startswith('.files-rename-') for p in folder.iterdir())

    def finish(action, renamed):
        offset = command(action)
        wait(lambda: re.search(r'FILES_MUTATION_FINISHED id=\d+ success=true ', log.read_text()[offset:]),
             'fresh mutation ACK absent')
        unchanged(renamed)
        wait(lambda: re.search(r'FILES_SCENE_READY view=groups loaded=true entries=3 stats_ready=true',
                               log.read_text()[offset:]), 'fresh reloaded listing absent')

    def capture(name):
        raw.unlink(missing_ok=True)
        raw.with_suffix('.request').touch()
        def full():
            if not raw.exists():
                return False
            parts = raw.read_bytes().split(b'\n', 3)
            return len(parts) == 4 and len(parts[3]) == int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(full, 'incomplete framebuffer')
        target = EVIDENCE / (name+'-'+str(time.time_ns())+'.png')
        subprocess.run(['magick', str(raw), str(target)], check=True)
        report['captures'].append(str(target))

    try:
        wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'initial scene absent')
        valid_id = preview('regex-preview', True)
        unchanged(False)
        capture('rename-regex-preview')
        invalid_id = preview('regex-invalid', False)
        assert invalid_id != valid_id
        unchanged(False)
        command('apply-invalid')
        capture('rename-regex-invalid')
        expansion_id = preview('regex-expand', False)
        assert expansion_id != invalid_id
        unchanged(False)
        command('apply-invalid')
        capture('rename-regex-expansion-limit')
        preview('regex-preview', True)
        finish('apply', True)
        capture('rename-regex-result')
        finish('undo', False)
        finish('redo', True)
        finish('undo', False)
        command('close')
        wait(lambda: process.poll() is not None, 'normal close absent', True)
        assert process.returncode == 0 and 'FILES_MUTATIONS_DRAINED' in log.read_text()
        normal = True
        report.update(functional_gate='PASS', preview_keeps_originals=True, invalid_and_expansion_rejected=True,
                      byte_exact_apply_undo_redo=True, no_staging_left=True, root_exit=process.returncode,
                      normal_exit_after_ack=True, cleanup_scope='root exit0; no helper used or tree sampler')
    except Exception as error:
        report.update(functional_gate='FAIL', error=repr(error))
        raise
    finally:
        report['log'] = log.read_text()
        report['binary_sha256'] = hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
        if not normal:
            stop_tree(process)
        path = EVIDENCE / ('rename-regex-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as output:
            json.dump(report, output, ensure_ascii=False, indent=2)
        print(path)
