#!/usr/bin/env python3
"""Copy from the text preview with REAL pointer and keyboard input.

Everything runs inside a private nested Hyprland that this script starts on a
hidden special workspace of the current session and stops at the end. Input
goes only to the nested socket (scripts/tools/vpointer.c refuses wayland-0/1;
wtype gets the nested WAYLAND_DISPLAY), and `wl-copy`/`wl-paste` read and write
the NESTED clipboard, never the user's.

Checks: drag + Ctrl+C, double click (word), triple click (line), the Copiar
button, and that Ctrl+C with an empty selection keeps the previous clipboard.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = ROOT / 'docs/evidence/preview-copy-20261105'
TEXT = 'primera línea del archivo de prueba\nsegunda línea con más texto aquí\ntercera línea final\n'


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def hypr(args, env=None):
    return subprocess.run(['hyprctl', *args], env=env, capture_output=True, text=True, timeout=5).stdout


def build_pointer(tmp):
    tools = ROOT / 'scripts/tools'
    xml = tools / 'wlr-virtual-pointer-unstable-v1.xml'
    header = tmp / 'wlr-virtual-pointer-unstable-v1-client.h'
    code = tmp / 'wlr-virtual-pointer-unstable-v1.c'
    subprocess.run(['wayland-scanner', 'client-header', str(xml), str(header)], check=True)
    subprocess.run(['wayland-scanner', 'private-code', str(xml), str(code)], check=True)
    flags = subprocess.check_output(['pkg-config', '--cflags', '--libs', 'wayland-client'], text=True).split()
    out = tmp / 'vpointer'
    subprocess.run(['cc', '-O2', '-Wall', f'-I{tmp}', '-o', str(out), str(tools / 'vpointer.c'), str(code), *flags], check=True)
    return out


def start_nested(tmp, report):
    # The parent is the session that owns WAYLAND_DISPLAY; only its hidden
    # special workspace receives the nested compositor window.
    instances = json.loads(hypr(['instances', '-j']))
    parent = [i for i in instances if i.get('wl_socket') == os.environ.get('WAYLAND_DISPLAY')] or instances
    if len(parent) != 1:
        raise AssertionError('cannot tell which Hyprland session is the parent')
    os.environ['HYPRLAND_INSTANCE_SIGNATURE'] = parent[0]['instance']
    report['parent_socket'] = parent[0]['wl_socket']
    runtime = Path(os.environ['XDG_RUNTIME_DIR'])
    before = {p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')}
    conf = tmp / 'nested.conf'
    conf.write_text('misc {\n    disable_hyprland_logo = true\n    disable_splash_rendering = true\n}\n')
    command = f'Hyprland --config {conf} > {tmp}/nested.log 2>&1'
    lua = f'hl.dsp.exec_cmd("{command}", {{ workspace = "special:files-copy-check silent" }})'
    out = hypr(['dispatch', lua])
    if 'ok' not in out:
        hypr(['dispatch', 'exec', f'[workspace special:files-copy-check silent] {command}'])
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        new = {p.name for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')} - before
        if new:
            socket = sorted(new)[0]
            instances = json.loads(hypr(['instances', '-j']))
            match = [i for i in instances if i.get('wl_socket') == socket]
            if match:
                # Our own process: the only Hyprland started with this config.
                pid = subprocess.run(['pgrep', '-x', '-f', f'Hyprland --config {conf}'], capture_output=True,
                                     text=True).stdout.split()
                report['nested'] = {'socket': socket, 'pid': pid}
                return socket, match[0]['instance'], pid
        time.sleep(.2)
    raise AssertionError('nested Hyprland did not start')


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    ap.add_argument('--label', default='source')
    args = ap.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    html_helper = args.binary.parent / 'files-html-helper'
    report = {'functional_gate': 'FAIL', 'label': args.label, 'binary_sha256': digest(args.binary),
              'helper_sha256': digest(args.binary.parent / 'files-helper'),
              'html_helper_sha256': digest(html_helper) if html_helper.is_file() else None,
              'method': 'private nested Hyprland on a hidden special workspace; virtual pointer + wtype on the '
                        'nested socket; nested clipboard only', 'checks': {}, 'captures': [],
              'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED'}
    out = EVIDENCE / f'{args.label}-{time.time_ns()}.json'
    process = nested_pid = None
    base_root = Path.home() / '.cache/files-test'
    base_root.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix='preview-copy-', dir=base_root) as tmp:
            tmp = Path(tmp)
            pointer = build_pointer(tmp)
            socket, signature, nested_pid = start_nested(tmp, report)
            assert socket != report['parent_socket'], 'refusing the session socket'
            nested = {**os.environ, 'WAYLAND_DISPLAY': socket, 'HYPRLAND_INSTANCE_SIGNATURE': signature}
            monitor = json.loads(hypr(['monitors', '-j'], nested))[0]
            extent = (round(monitor['width'] / monitor['scale']), round(monitor['height'] / monitor['scale']))
            listing = tmp / 'listing'
            listing.mkdir()
            (listing / 'nota.txt').write_text(TEXT)
            capture = tmp / 'frame.ppm'
            log = tmp / 'app.log'
            env = {**nested, 'FILES_CONFIG_DIR': str(tmp / 'config'), 'XDG_DATA_HOME': str(tmp / 'data'),
                   'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_PREVIEW': '1', 'TMPDIR': str(tmp), 'MAGICK_TEMPORARY_PATH': str(tmp)}
            with log.open('w') as output:
                process = subprocess.Popen([str(args.binary), str(listing), '--size', '900x480', '--scale', '1',
                                            '--no-session', '--ui-smoke', '--ui-scene', 'folder', '--exit-after', '90'],
                                           env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)

            def wait(check, reason, timeout=15):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline:
                    v = check()
                    if v:
                        return v
                    if process.poll() is not None:
                        raise AssertionError(f'{reason}; exit {process.returncode}')
                    time.sleep(.05)
                raise AssertionError(reason)

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

            wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'smoke')
            time.sleep(.8)
            window = wait(lambda: [c for c in json.loads(hypr(['clients', '-j'], nested)) if c['class'] == 'files'], 'window')[0]
            (wx, wy), (ww, wh) = window['at'], window['size']

            def ptr(x0, y0, x1=None, y1=None, extra=None):
                x1, y1 = (x0, y0) if x1 is None else (x1, y1)
                subprocess.run([str(pointer), socket, str(extent[0]), str(extent[1]), str(wx + x0), str(wy + y0),
                                str(wx + x1), str(wy + y1)] + ([extra] if extra else []), check=True, timeout=10)
                time.sleep(.4)

            def keys(*k):
                subprocess.run(['wtype', *k], env=nested, check=True, timeout=10)
                time.sleep(.5)

            def paste():
                return subprocess.run(['wl-paste', '--no-newline'], env=nested, capture_output=True, text=True, timeout=5).stdout

            def copy_key():
                keys('-M', 'ctrl', '-k', 'c', '-m', 'ctrl')

            # Focus and select the file, then Space opens the text preview.
            ptr(300, 50)
            keys('-k', 'space')
            wait(lambda: 'FILES_PREVIEW_READY' in log.read_text(), 'preview')
            time.sleep(.5)
            line1, line2, left = (110, 133, 95)
            subprocess.run(['wl-copy', 'previo'], env=nested, check=True, timeout=5)
            time.sleep(.3)
            ptr(115, line2)
            copy_key()
            report['checks']['empty_selection_keeps_clipboard'] = paste()
            assert paste() == 'previo', paste()
            shot('empty-selection-notice')
            ptr(115, line2, extra='c2')
            copy_key()
            report['checks']['double_click_word'] = paste()
            assert paste() == 'segunda', paste()
            ptr(115, line2, extra='c3')
            copy_key()
            report['checks']['triple_click_line'] = paste()
            assert paste() == 'segunda línea con más texto aquí\n', paste()
            ptr(left, line1, 200, line2)
            copy_key()
            report['checks']['drag_ctrl_c'] = paste()
            assert paste() == 'primera línea del archivo de prueba\nsegunda línea', paste()
            shot('copied-notice')
            subprocess.run(['wl-copy', '--clear'], env=nested, timeout=5)
            ptr(115, line1, extra='c2')
            ptr(ww - 104, wh - 44, extra='1500')
            report['checks']['copy_button_after_hover'] = paste()
            assert paste() == 'primera', paste()
            notices = re.findall(r'FILES_PREVIEW_COPY .*', log.read_text())
            report['checks']['notices'] = notices
            assert notices[0].endswith('Ctrl+A') and 'Copiado: 7 caracteres' in notices[1], notices
            report['functional_gate'] = 'PASS'
    except Exception as e:
        report['error'] = repr(e)
    finally:
        if process and process.poll() is None:
            process.kill()
            process.wait()
        for signal in (15, 9):
            alive = []
            for pid in nested_pid or []:
                try:
                    os.kill(int(pid), signal)
                    alive.append(pid)
                except (OSError, ValueError):
                    pass
            deadline = time.monotonic() + 10
            while alive and time.monotonic() < deadline:
                alive = [pid for pid in alive if Path(f'/proc/{pid}').exists()]
                time.sleep(.2)
            if not alive:
                break
        report['nested_stopped'] = not any(Path(f'/proc/{pid}').exists() for pid in nested_pid or [])
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
    print(report['functional_gate'], out)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
