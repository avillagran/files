#!/usr/bin/env python3
"""Sidebar devices and bookmarks in a real Files window.

FILES_TEST_DEVICES_JSON replaces `lsblk` with a fixture (one mounted USB
stick, one unmounted partition); FILES_TEST_UDISKSCTL is a recorder, so no
real disk is ever mounted or powered off. Internal handlers, no OS input.
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
EVIDENCE = ROOT / 'docs/evidence/devices-20261104'


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    ap.add_argument('--label', default='source')
    args = ap.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    report = {'functional_gate': 'FAIL', 'label': args.label, 'binary_sha256': digest(args.binary),
              'helper_sha256': digest(args.binary.parent / 'files-helper'),
              'html_helper_sha256': digest(args.binary.parent / 'files-html-helper') if (args.binary.parent / 'files-html-helper').is_file() else None,
              'method': 'fixture lsblk JSON + recorder udisksctl; real window; no disk touched',
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
        with tempfile.TemporaryDirectory(prefix='devices-', dir=base_root) as tmp:
            base = Path(tmp)
            listing = base / 'listing'
            listing.mkdir()
            (listing / 'a.txt').write_text('a')
            pen = base / 'mnt/PENDRIVE'
            datos = base / 'mnt/DATOS'
            pen.mkdir(parents=True)
            datos.mkdir(parents=True)
            (pen / 'foto.jpg').write_bytes(b'x')
            (datos / 'informe.txt').write_text('d')
            devices = {'blockdevices': [
                {'name': 'sdz', 'path': '/dev/sdz', 'type': 'disk', 'fstype': None, 'size': 32000000000, 'rm': True, 'hotplug': True, 'mountpoints': [None], 'children': [
                    {'name': 'sdz1', 'path': '/dev/sdz1', 'type': 'part', 'fstype': 'exfat', 'label': 'PENDRIVE', 'size': 31000000000, 'rm': True, 'hotplug': True, 'mountpoints': [str(pen)]}]},
                {'name': 'sdy', 'path': '/dev/sdy', 'type': 'disk', 'fstype': None, 'size': 500000000000, 'rm': False, 'hotplug': False, 'mountpoints': [None], 'children': [
                    {'name': 'sdy2', 'path': '/dev/sdy2', 'type': 'part', 'fstype': 'ext4', 'label': 'DATOS', 'size': 499000000000, 'rm': False, 'hotplug': False, 'mountpoints': [None]}]}]}
            dj = base / 'lsblk.json'
            dj.write_text(json.dumps(devices))
            calls = base / 'udisks-calls'
            tool = base / 'udisksctl'
            tool.write_text(f'#!/bin/sh\necho "$@" >> {calls}\n[ "$1" = mount ] && echo "Mounted $3 at {datos}."\nexit 0\n')
            tool.chmod(tool.stat().st_mode | stat.S_IEXEC)
            capture = base / 'frame.ppm'
            env = {**os.environ, 'FILES_CONFIG_DIR': str(base / 'config'), 'XDG_DATA_HOME': str(base / 'data'),
                   'FILES_TEST_DEVICES_JSON': str(dj), 'FILES_TEST_UDISKSCTL': str(tool),
                   'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_PREVIEW': '1', 'FILES_TEST_FIXED_SIZE': '1',
                   'FILES_TEST_ISOLATE_INPUT': '1', 'TMPDIR': str(base), 'MAGICK_TEMPORARY_PATH': str(base)}
            with log.open('w') as output:
                process = subprocess.Popen([str(args.binary), str(listing), '--size', '1000x650', '--scale', '1',
                                            '--no-session', '--ui-smoke', '--ui-scene', 'folder', '--exit-after', '60'],
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
                after = len(text())
                pending = capture.with_suffix('.pending')
                pending.write_text(action)
                os.replace(pending, capture.with_suffix('.command'))
                done = wait(lambda: re.search(r'FILES_PREVIEW_COMMAND_(REV action=' + re.escape(action) + r' |ERROR ' + re.escape(action) + r': (.*))', text()[after:]), action)
                if done.group(0).startswith('FILES_PREVIEW_COMMAND_ERROR'):
                    raise AssertionError(f'{action}: {done.group(2)}')
                return after
            def state():
                after = command('nf-devices')
                return wait(lambda: re.search(r'FILES_NF_DEVICES .*', text()[after:]), 'devices').group(0)
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
            command('nf-rail')
            s = wait(lambda: (x := state()) and 'PENDRIVE' in x and 'DATOS' in x and 'device:/dev/sdz1:None' in x and x, 'devices listed')
            assert 'device:/dev/sdz1:Some(Eject)' in s and 'device:/dev/sdy2:None' in s and 'device:/dev/sdy2:Some' not in s, s
            shot('rail-devices')
            # Click the unmounted partition: mounts, then opens it.
            command('nf-device:/dev/sdy2:')
            wait(lambda: 'FILES_DEVICE_DONE op=Mount node=/dev/sdy2' in text(), 'mount done')
            wait(lambda: f'path={datos} ' in state(), 'navigated to mount')
            assert calls.read_text().splitlines()[0] == 'mount -b /dev/sdy2 --no-user-interaction'
            report['checks']['mount_then_open'] = str(datos)
            # Bookmark it: appears in the sidebar.
            command('nf-bookmark-add')
            wait(lambda: f'nav:{datos}' in state(), 'bookmark in sidebar')
            report['checks']['bookmark_in_sidebar'] = True
            # Click the mounted stick: opens it; eject leaves it and powers off.
            command('nf-device:/dev/sdz1:')
            wait(lambda: f'path={pen} ' in state(), 'open mounted')
            command('nf-device:/dev/sdz1:eject')
            wait(lambda: 'FILES_DEVICE_DONE op=Eject node=/dev/sdz1' in text(), 'eject done')
            s = state()
            assert f'path={pen} ' not in s, 'still inside the ejected device'
            assert 'expulsado' in s, s
            lines = calls.read_text().splitlines()
            assert lines[1:] == ['unmount -b /dev/sdz1 --no-user-interaction', 'power-off -b /dev/sdz --no-user-interaction'], lines
            shot('after-eject')
            report['checks']['eject'] = lines[1:]
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
