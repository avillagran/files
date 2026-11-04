#!/usr/bin/env python3
"""Mapped Files pixels and internal handlers; never OS input or personal data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from urllib.parse import quote_from_bytes

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'docs/evidence/toolkit-input-20261104'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    args = parser.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    base_root = Path.home() / '.cache/files-test'
    base_root.mkdir(parents=True, exist_ok=True)
    report = {'functional_gate': 'FAIL', 'method': 'Mapped Files framebuffer, internal Released/controller/catalog fixture; no OS input',
              'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED', 'captures': [],
              'backend': os.environ.get('FLTK_BACKEND', 'wayland' if os.environ.get('WAYLAND_DISPLAY') else 'unspecified'), 'binary_path': str(args.binary.resolve())}
    process = None
    with tempfile.TemporaryDirectory(prefix='toolkit-input-', dir=base_root) as tmp:
        base = Path(tmp)
        listing = base / 'listing'
        listing.mkdir()
        for i in range(300):
            (listing / f'item-{i:03d}.txt').write_bytes(b'fixture\n')
        data = base / 'data'
        files, info = data / 'Trash/files', data / 'Trash/info'
        for path in [data, data / 'Trash', files, info]:
            path.mkdir(mode=0o700)
        for i in range(120):
            name = f'stored-{i:03d}.txt'
            (files / name).write_bytes(f'preserve {i}\n'.encode())
            original = f'/original/file-{i:03d}.txt'.encode()
            (info / (name + '.trashinfo')).write_text('[Trash Info]\nPath=' + quote_from_bytes(original, safe='/') + '\nDeletionDate=2026-11-04T12:34:56\n')
        (files / 'raw').write_bytes(b'raw bytes')
        (info / 'raw.trashinfo').write_text('[Trash Info]\nPath=/original/raw%FF.txt\nDeletionDate=2026-11-04T12:34:56\n')
        (files / 'orphan').write_bytes(b'orphan preserved')
        expected = {str(p.relative_to(base)): digest(p) for d in [listing, files, info] for p in d.iterdir()}
        capture = base / 'frame.ppm'
        log = base / 'app.log'
        env = {**os.environ, 'FILES_CONFIG_DIR': str(base / 'config'), 'XDG_DATA_HOME': str(data),
               'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_FIXED_SIZE': '1', 'FILES_TEST_ISOLATE_INPUT': '1',
               'FILES_TEST_TOOLKIT_POPUP': '1', 'FILES_CLOSE_TRACE': '1', 'TMPDIR': str(base), 'MAGICK_TEMPORARY_PATH': str(base)}
        try:
            with log.open('w') as output:
                process = subprocess.Popen([str(args.binary), str(listing), '--size', '800x440', '--scale', '1', '--no-session', '--ui-smoke', '--exit-after', '45'],
                                           env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            def wait(check, reason, timeout=10):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    value = check()
                    if value:
                        return value
                    if process.poll() is not None:
                        raise AssertionError(f'{reason}; app exit {process.returncode}')
                    time.sleep(.02)
                raise AssertionError(reason)
            wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'smoke ready')
            if os.environ.get('WAYLAND_DISPLAY') and report['backend'] != 'x11':
                def mapped_client():
                    clients = json.loads(subprocess.check_output(['hyprctl', 'clients', '-j'], text=True))
                    return next((c for c in clients if c.get('pid') == process.pid), None)
                own = wait(mapped_client, 'own native Wayland client mapped')
                report['native_window'] = {k: own.get(k) for k in ['pid', 'class', 'xwayland', 'size', 'workspace']}
                assert not own.get('xwayland')
                assert own.get('workspace', {}).get('name', '').startswith('special:files-toolkit-regression')
            viewport = None
            def command(action, marker=None):
                start = len(log.read_text())
                pending = capture.with_suffix('.pending')
                pending.write_text(action)
                pending.replace(capture.with_suffix('.popup-command'))
                wait(lambda: (marker or f'FILES_TOOLKIT_FIXTURE action={action}') in log.read_text()[start:], action)
                return log.read_text()[start:]
            def shot(label):
                nonlocal viewport
                capture.unlink(missing_ok=True)
                capture.with_suffix('.request').touch()
                def complete():
                    if not capture.exists():
                        return False
                    parts = capture.read_bytes().split(b'\n', 3)
                    if len(parts) != 4:
                        return False
                    dims = [int(v) for v in parts[1].split()]
                    return len(dims) == 2 and all(0 < v <= 4000 for v in dims) and len(parts[3]) == dims[0] * dims[1] * 3
                wait(complete, 'complete framebuffer')
                parts = capture.read_bytes().split(b'\n', 3)
                dims = [int(v) for v in parts[1].split()]
                if viewport is None:
                    viewport = dims
                    report['viewport'] = dims
                assert viewport == dims, 'fixture window changed geometry during the checks'
                raw = parts[3]
                path = EVIDENCE / f'{report["backend"]}-{label}-{time.time_ns()}.png'
                subprocess.run(['magick', str(capture), str(path)], check=True, env=env, timeout=10)
                report['captures'].append(str(path))
                return raw
            shot('sidebar')
            text = command('context', 'FILES_TOOLKIT_POPUP_OPEN kind=Context')
            match = re.search(r'rect=Some\(\((\d+), (\d+), (\d+), (\d+)\)\)', text)
            assert match, text
            x, y, w, h = map(int, match.groups())
            before = shot('menu-before-release')
            command('release-redraw')
            after = shot('menu-after-release')
            roi = lambda pixels: b''.join(pixels[((y + row) * viewport[0] + x) * 3:((y + row) * viewport[0] + x + w) * 3] for row in range(h))
            assert roi(before) == roi(after), 'release/canvas redraw changed menu pixels'
            report['release_menu_pixels_preserved'] = True
            command('escape', 'reason=Escape')
            command('wheel-state')
            first = int(re.findall(r'FILES_WHEEL scroll=(\d+)', log.read_text())[-1])
            command('wheel-down')
            time.sleep(.25)
            command('wheel-state')
            second = int(re.findall(r'FILES_WHEEL scroll=(\d+)', log.read_text())[-1])
            assert second - first == 114, (first, second)
            shot('scrolled')
            command('wheel-up')
            time.sleep(.25)
            command('wheel-state')
            assert int(re.findall(r'FILES_WHEEL scroll=(\d+)', log.read_text())[-1]) == first
            report['wheel_three_units_pixels'] = second - first
            command('trash-open')
            for _ in range(30):
                text = command('trash-state')
                if 'open=true count=122 ready=true' in text:
                    break
                time.sleep(.05)
            else:
                raise AssertionError('catalog did not finish with 122 entries')
            start = shot('trash-start')
            command('trash-end')
            end = shot('trash-end')
            assert start != end, 'catalog End did not reveal last entries'
            command('trash-reload')
            for _ in range(30):
                if 'open=true count=122 ready=true' in command('trash-state'):
                    break
                time.sleep(.05)
            else:
                raise AssertionError('catalog reload')
            command('trash-close')
            assert 'open=false' in command('trash-state')
            report['trash_catalog'] = {'count': 122, 'reload': True, 'last_row_revealed': True, 'read_only': True}
            observed = {str(p.relative_to(base)): digest(p) for d in [listing, files, info] for p in d.iterdir()}
            assert observed == expected, 'fixture data or trash metadata changed'
            report['fixture_files_and_metadata_unchanged'] = True
            command('close')
            process.wait(timeout=12)
            assert process.returncode == 0 and 'FILES_CLOSE_DRAINED' in log.read_text()
            report.update(functional_gate='PASS', normal_exit=True)
        except BaseException as error:
            report['error'] = repr(error)
        finally:
            if process is not None and process.poll() is None:
                # Only the child process group created above, never another app.
                import signal
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=10)
            report['log'] = log.read_text() if log.exists() else ''
    report['binary_sha256'] = digest(args.binary)
    for name, key in [('files-helper', 'helper_sha256'), ('files-html-helper', 'html_helper_sha256')]:
        report[key] = digest(args.binary.with_name(name))
    out = EVIDENCE / f'{report["backend"]}-functional-{time.time_ns()}.json'
    out.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({'report': str(out), 'functional_gate': report['functional_gate'], 'error': report.get('error')}))
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
