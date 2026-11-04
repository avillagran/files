#!/usr/bin/env python3
"""Owned real-helper sync review protocol checks; not UI or Apply acceptance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--helper', type=Path, default=ROOT / 'target/debug/files-helper')
args = parser.parse_args()
helper = args.helper.resolve()
out = ROOT / 'docs/evidence/directory-sync-wire'
out.mkdir(parents=True, exist_ok=True)
base = Path.home() / '.cache/files-test'
base.mkdir(parents=True, exist_ok=True)
MAX_STREAM = 21237760 + 128 * (16384 + 512 + 1)
report = {'functional_gate': 'IN_PROGRESS', 'scope': 'Owned fresh sync-review helper CLI only; no visible review, Apply, queue, recovery or installation', 'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED', 'helper_sha256': hashlib.sha256(helper.read_bytes()).hexdigest(), 'cases': []}

def wire_path(value):
    try:
        return value.decode('utf8')
    except UnicodeDecodeError:
        return list(value)

def raw(value):
    return value.encode('utf8') if isinstance(value, str) else bytes(value)

def invoke(label, intent, command='prepare_directory_sync', exit_ok=True, payload=None):
    data = payload if payload is not None else (json.dumps({'command': 'prepare_directory_sync', 'intent': intent}, ensure_ascii=False) + '\n').encode()
    child = subprocess.run([str(helper), command], input=data, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=25)
    case = {'label': label, 'returncode': child.returncode, 'stderr': child.stderr.decode('utf8', errors='replace'), 'stdout_bytes': len(child.stdout)}
    report['cases'].append(case)
    assert (child.returncode == 0) == exit_ok, (label, case)
    if not exit_ok:
        assert not child.stdout
        return []
    assert child.stdout.endswith(b'\n') and len(child.stdout) <= MAX_STREAM
    frames = []
    for line in child.stdout.splitlines():
        assert len(line) <= 65536
        frame = json.loads(line)
        assert set(frame) == {'version', 'mode', 'observation'}
        assert frame['version'] == 1 and frame['mode'] == intent['mode']
        assert frame['observation']['id'] == intent['id']
        frames.append(frame)
    case['frames'] = frames
    return frames

def reconstruct(frames):
    rows, terminal, pages = [], None, 0
    for frame in frames:
        event = frame['observation']['event']
        assert terminal is None
        if event['kind'] == 'page':
            assert event['offset'] == len(rows) and 0 < len(event['rows']) <= 128
            assert sum(len(raw(r['path'])) for r in event['rows']) <= 4096
            rows.extend(event['rows'])
            pages += 1
        elif event['kind'] == 'done':
            terminal = event
        else:
            assert event['kind'] == 'progress'
    assert terminal is not None and terminal['progress']['compared'] == len(rows)
    names = [raw(r['path']) for r in rows]
    assert len(names) == len(set(names)) and names == sorted(names, key=lambda n: tuple(n.split(b'/')))
    assert all(not n.startswith(b'/') and all(c not in [b'', b'.', b'..'] for c in n.split(b'/')) for n in names)
    return {raw(r['path']): r for r in rows}, terminal, pages

def snapshot(root):
    entries = []
    for parent, dirs, files in os.walk(root):
        for name in sorted(dirs + files):
            path = os.path.join(parent, name)
            st = os.lstat(path)
            value = os.readlink(path) if os.path.islink(path) else hashlib.sha256(Path(os.fsdecode(path)).read_bytes()).hexdigest() if os.path.isfile(path) else None
            entries.append((os.path.relpath(path, root), st.st_mode, st.st_mtime_ns, value))
    return sorted(entries)

try:
    with tempfile.TemporaryDirectory(prefix='directory-sync-wire-', dir=base) as tmp:
        root = os.fsencode(tmp)
        left, right = root + b'/left-\xff', root + b'/right\n'
        for folder in (left, right):
            os.mkdir(folder)
            os.mkdir(folder + b'/nested')
            os.mkdir(folder + b'/empty')
            for n in range(130):
                with open(folder + f'/file-{n:03}'.encode(), 'wb') as stream:
                    stream.write(b'x')
            with open(folder + b'/nested/raw\xff\n', 'wb') as stream:
                stream.write(b'raw')
            os.symlink(b'../outside\xff', folder + b'/literal-link')
        for folder, data in [(left, b'left'), (right, b'rite')]:
            with open(folder + b'/changed', 'wb') as stream:
                stream.write(data)
        with open(left + b'/only-left', 'wb') as stream:
            stream.write(b'left')
        with open(right + b'/shadow\xef\xbf\xbd\n', 'wb') as stream:
            stream.write(b'shadow')
        before = snapshot(root)
        expected = [{'device': os.stat(d).st_dev, 'inode': os.stat(d).st_ino} for d in (left, right)]
        intent = {'version': 1, 'id': 11, 'left': wire_path(left), 'right': wire_path(right), 'expected': expected, 'mode': 'merge', 'read_limit': 1024 * 1024}
        modes = ['merge', {'update': 'left'}, {'update': 'right'}, {'mirror': 'left'}, {'mirror': 'right'}]
        for n, mode in enumerate(modes):
            current = {**intent, 'id': 11 + n, 'mode': mode}
            rows, terminal, pages = reconstruct(invoke(f'fresh-paged-mode-{n}', current))
            assert pages >= 2 and terminal['roots'] == expected and terminal['progress']['reasons'] == 0
            assert rows[b'changed']['difference'] == 'Content' and rows[b'only-left']['difference'] == 'OnlyLeft'
            assert rows[b'nested/raw\xff\n']['difference'] == 'Equal' and rows[b'shadow\xef\xbf\xbd\n']['difference'] == 'OnlyRight'
            assert rows[b'literal-link']['difference'] == 'Equal' and not any(name.startswith(b'literal-link/') for name in rows)
        rows, terminal, _ = reconstruct(invoke('explicit-read-limit', {**intent, 'id': 20, 'mode': {'mirror': 'left'}, 'read_limit': 0}))
        assert terminal['progress']['bytes_read'] == 0 and terminal['progress']['reasons'] & 8
        assert rows[b'nested/raw\xff\n']['difference'] == 'Unknown'
        for label, changes in [('captured-root-replaced', {'expected': [{'device': 0, 'inode': 1}, {'device': 0, 'inode': 2}]}), ('canonical-overlap', {'right': wire_path(left + b'/nested'), 'expected': None}), ('missing-root', {'left': '/does-not-exist', 'expected': None})]:
            frames = invoke(label, {**intent, **changes})
            assert len(frames) >= 1 and frames[-1]['observation']['event']['kind'] == 'error'
            assert all(f['observation']['event']['kind'] != 'page' for f in frames)
        for label, changes in [('over-read-budget', {'read_limit': 1024**4 + 1}), ('foreign-version', {'version': 2}), ('unknown-intent-instruction', {'apply': True}), ('invalid-mode', {'mode': {'mirror': 'newer'}}), ('same-captured-root', {'expected': [expected[0]] * 2}), ('nul-path', {'left': [47, 0, 65]})]:
            invoke(label, {**intent, **changes}, exit_ok=False)
        invoke('mismatched-command', intent, command='compare_directories', exit_ok=False)
        invoke('oversize-request', intent, payload=b'x' * 65537, exit_ok=False)
        assert snapshot(root) == before
        # Exercise both complete retained inventories in the actual limited
        # helper. Empty files test namespace/wire bounds, not throughput or PSS.
        maximum = []
        for side in (b'a', b'b'):
            folder = root + b'/max-' + side
            os.mkdir(folder)
            maximum.append(folder)
            for n in range(8192):
                name = side + f'{n:04x}'.encode() + b'\x01' * 123
                with open(folder + b'/' + name, 'xb'):
                    pass
        before_max = [snapshot(folder) for folder in maximum]
        full_intent = {**intent, 'id': 40, 'left': wire_path(maximum[0]), 'right': wire_path(maximum[1]), 'expected': None, 'mode': {'mirror': 'right'}, 'read_limit': 0}
        full_frames = invoke('both-full-retained-control-inventories', full_intent)
        rows, terminal, pages = reconstruct(full_frames)
        assert len(rows) == 16384 and pages >= 512
        assert terminal['progress']['reasons'] == 0 and terminal['progress']['entries'] == [8192, 8192]
        assert terminal['progress']['different'] == 16384 and terminal['progress']['unknown'] == 0
        assert report['cases'][-1]['stdout_bytes'] > 4 * 1024 * 1024
        for side, folder in zip((b'a', b'b'), maximum):
            assert all(side + f'{n:04x}'.encode() + b'\x01' * 123 in rows for n in range(8192))
            assert before_max[0 if side == b'a' else 1] == snapshot(folder)
        # Preserve exact verification code, result counts and frame digest
        # without storing another multi-megabyte copy of control-name JSON.
        case = report['cases'][-1]
        case.update(rows=len(rows), pages=pages, terminal=terminal, frames_json_sha256=hashlib.sha256(json.dumps(full_frames, ensure_ascii=False, separators=(',', ':')).encode()).hexdigest())
        del case['frames']
        report['functional_gate'] = 'PASS'
        report['scope'] += '; all five modes, raw roots/names, paged rows, captured roots/read limit, malformed intent/command rejection, unchanged namespace/content/link targets/mode/mtime; normal subprocess wait. Local proposal derivation and EOF/exit/reap are covered separately by Rust tests. No native input/clipboard/AT-SPI/2x/global lifetime claim.'
except BaseException as error:
    report.update(functional_gate='FAIL', error=repr(error))
    raise
finally:
    receipt = out / f'helper-wire-{time.time_ns()}.json'
    receipt.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n')
    print(receipt)
