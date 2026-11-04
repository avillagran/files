#!/usr/bin/env python3
"""Open (Enter) and Open with (Ctrl+Shift+O) in a real Files window.

Private XDG_DATA_HOME/XDG_DATA_DIRS (fake applications) and
XDG_CONFIG_HOME (mimeapps.list): the user's associations are never read or
written. FILES_TEST_OPEN_LAUNCHER points to a recorder that stores argv
instead of launching anything. Internal handlers, no OS input.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'docs/evidence/open-with-20261104'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    parser.add_argument('--label', default='source')
    args = parser.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    report = {'functional_gate': 'FAIL', 'label': args.label, 'binary_sha256': digest(args.binary),
              'helper_sha256': digest(args.binary.parent / 'files-helper'),
              'html_helper_sha256': digest(args.binary.parent / 'files-html-helper') if (args.binary.parent / 'files-html-helper').is_file() else None,
              'method': 'internal Actions, real window, recorder launcher, private XDG dirs; user mimeapps untouched',
              'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED', 'checks': {}, 'captures': []}
    if os.environ.get('WAYLAND_DISPLAY'):
        inst = json.loads(subprocess.check_output(['hyprctl', 'instances', '-j'], timeout=2))
        match = [i for i in inst if i.get('wl_socket') == os.environ.get('WAYLAND_DISPLAY')]
        if len(match) == 1:
            os.environ['HYPRLAND_INSTANCE_SIGNATURE'] = match[0]['instance']
    stamp = time.time_ns()
    out = EVIDENCE / f'{args.label}-{stamp}.json'
    log = EVIDENCE / f'{args.label}-{stamp}-app.log'
    process = None
    base_root = Path.home() / '.cache/files-test'
    base_root.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix='open-with-', dir=base_root) as tmp:
            base = Path(tmp)
            listing = base / 'listing'
            listing.mkdir()
            (listing / 'photo.png').write_bytes(b'\x89PNG\r\n\x1a\n')
            (listing / 'notes').write_bytes(b'plain text without extension\n')
            (listing / 'blob.bin').write_bytes(bytes(range(256)))
            data, sysdata, config = base / 'data', base / 'sys', base / 'config-home'
            (data / 'applications').mkdir(parents=True)
            (sysdata / 'applications').mkdir(parents=True)
            config.mkdir()
            def app(where, id, name, mimes, extra=''):
                (where / 'applications' / id).write_text(f'[Desktop Entry]\nType=Application\nName={name}\nExec=true %F\nMimeType={mimes}\n{extra}')
            app(sysdata, 'viewer.desktop', 'Visor', 'image/png;')
            app(sysdata, 'painter.desktop', 'Pintor', 'image/png;image/jpeg;')
            app(sysdata, 'editor.desktop', 'Editor', 'text/plain;')
            app(sysdata, 'other.desktop', 'Otra', 'application/zip;')
            app(sysdata, 'hidden.desktop', 'Oculta', 'image/png;', 'NoDisplay=true\n')
            (config / 'mimeapps.list').write_text('[Default Applications]\nimage/png=viewer.desktop\n')
            record = base / 'launched.txt'
            launcher = base / 'launcher'
            launcher.write_text(f'#!/bin/sh\nprintf "%s\\n" "$@" >> {record}\nprintf "=END=\\n" >> {record}\n')
            launcher.chmod(launcher.stat().st_mode | stat.S_IEXEC)
            capture = base / 'frame.ppm'
            env = {**os.environ, 'FILES_CONFIG_DIR': str(base / 'files-config'), 'XDG_DATA_HOME': str(data),
                   'XDG_DATA_DIRS': str(sysdata), 'XDG_CONFIG_HOME': str(config), 'XDG_CONFIG_DIRS': str(base / 'none'),
                   'FILES_TEST_OPEN_LAUNCHER': str(launcher), 'FILES_TEST_OPEN': '1', 'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_PREVIEW': '1',
                   'FILES_TEST_FIXED_SIZE': '1', 'FILES_TEST_ISOLATE_INPUT': '1', 'TMPDIR': str(base), 'MAGICK_TEMPORARY_PATH': str(base)}
            with log.open('w') as output:
                process = subprocess.Popen([str(args.binary), str(listing), '--size', '1000x650', '--scale', '1',
                                            '--no-session', '--ui-smoke', '--ui-scene', 'folder', '--exit-after', '90'],
                                           env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
            def text():
                return log.read_text()
            def wait(check, reason, timeout=15):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    v = check()
                    if v:
                        return v
                    if process.poll() is not None:
                        raise AssertionError(f'{reason}; exit {process.returncode}')
                    time.sleep(.03)
                raise AssertionError(reason)
            def command(action):
                for _ in range(40):
                    after = len(text())
                    pending = capture.with_suffix('.pending')
                    pending.write_text(action)
                    os.replace(pending, capture.with_suffix('.command'))
                    done = wait(lambda: re.search(r'FILES_PREVIEW_COMMAND_(REV action=' + re.escape(action) + r' |ERROR ' + re.escape(action) + r': (.*))', text()[after:]), action)
                    if 'REV' in done.group(0):
                        return after
                    if 'falta' not in (done.group(2) or ''):
                        raise AssertionError(f'{action}: {done.group(2)}')
                    time.sleep(.2)
                raise AssertionError('listing never settled for ' + action)
            def ow():
                after = command('nf-ow-state')
                return wait(lambda: re.search(r'FILES_NF_OPEN_WITH .*', text()[after:]), 'ow state').group(0)
            def launched():
                if not record.exists():
                    return []
                return [b.strip().split('\n') for b in record.read_text().split('=END=\n') if b.strip()]
            def shot(label):
                capture.unlink(missing_ok=True)
                capture.with_suffix('.request').touch()
                def complete():
                    if not capture.exists():
                        return False
                    parts = capture.read_bytes().split(b'\n', 3)
                    if len(parts) != 4:
                        return False
                    dims = [int(v) for v in parts[1].split()]
                    return len(dims) == 2 and len(parts[3]) == dims[0] * dims[1] * 3
                wait(complete, 'capture ' + label)
                png = EVIDENCE / f'{args.label}-{label}-{time.time_ns()}.png'
                subprocess.run(['magick', str(capture), str(png)], check=True, env=env, timeout=20)
                report['captures'].append(str(png))

            wait(lambda: 'FILES_UI_SMOKE_OK' in text(), 'smoke')
            # Enter on a PNG: default from mimeapps.list, launched through the launcher.
            command('nf-select:photo.png')
            command('nf-open')
            wait(lambda: len(launched()) == 1, 'default open not launched')
            assert launched()[0] == ['-t', 'service', '--', 'viewer.desktop', str(listing / 'photo.png')], launched()
            report['checks']['open_default'] = launched()[0]
            # Unknown name with text content: text/plain handler.
            command('nf-select:notes')
            wait(lambda: 'FILES_OPEN_OUTCOME=' in text(), 'first outcome')
            command('nf-open')
            wait(lambda: len(launched()) == 2, 'text open')
            assert launched()[1][3] == 'editor.desktop', launched()
            # Binary with no handler: visible error, nothing launched.
            command('nf-select:blob.bin')
            time.sleep(.3)
            command('nf-open')
            wait(lambda: 'Sin aplicación para application/x-executable' in ow(), 'no-handler error')
            assert len(launched()) == 2
            report['checks']['no_handler'] = 'error shown, nothing launched'
            # Open with: recommended (default first, hidden excluded), all, set default.
            command('nf-select:photo.png')
            command('nf-open-with')
            state = wait(lambda: (s := ow()) and 'rows=' in s and 'rows= ' not in s and s, 'chooser loaded')
            assert 'mime=Some("image/png")' in state and 'default=Some("viewer.desktop")' in state, state
            assert 'rows=viewer.desktop,painter.desktop ' in state, state
            shot('open-with-recommended')
            command('nf-ow-all')
            assert 'other.desktop' in ow() and 'all=true' in ow()
            command('nf-ow-all')
            command('nf-ow-select:1')
            command('nf-ow-default')
            wait(lambda: 'FILES_OPEN_WITH_DEFAULT mime=image/png app=painter.desktop' in text(), 'default saved')
            mimeapps = (config / 'mimeapps.list').read_text()
            assert '[Default Applications]\nimage/png=painter.desktop\n' in mimeapps and '[Added Associations]\nimage/png=painter.desktop;' in mimeapps, mimeapps
            shot('open-with-default-saved')
            command('nf-ow-select:0')
            command('nf-ow-open')
            wait(lambda: len(launched()) == 3, 'open with launch')
            assert launched()[2][3] == 'painter.desktop', launched()
            # Enter now uses the new default.
            command('nf-open')
            wait(lambda: len(launched()) == 4, 'open after default change')
            assert launched()[3][3] == 'painter.desktop', launched()
            report['checks']['open_with'] = {'default_saved': 'painter.desktop', 'launches': launched()}
            report['functional_gate'] = 'PASS'
    except Exception as e:
        report['error'] = repr(e)
    finally:
        if process and process.poll() is None:
            process.kill()
            process.wait()
    if report['functional_gate'] == 'PASS':
        log.unlink(missing_ok=True)
    out.write_text(json.dumps(report, indent=2) + '\n')
    print(report['functional_gate'], out)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
