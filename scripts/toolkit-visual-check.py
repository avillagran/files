#!/usr/bin/env python3
"""Toolkit 1ff1adf in Files: Omarchy tooltip on the settings button and the
Preferences menu fade (OMARCHY_FLTK_ANIM=on) versus no fade (off).

Real window on a hidden workspace (or private Xvfb), toolkit popup fixture,
no OS input: the tooltip is shown through `fltk_omarchy::tooltip::show` on
an invisible probe over the button, the menu through the settings Action.
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
EVIDENCE = ROOT / 'docs/evidence/toolkit-1ff1adf-20261104'


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def session(args, base, anim, report):
    listing = base / f'listing-{anim}'
    listing.mkdir()
    for i in range(12):
        (listing / f'item-{i:02d}.txt').write_text('x')
    capture = base / f'frame-{anim}.ppm'
    log = EVIDENCE / f'{args.label}-{anim}-{time.time_ns()}-app.log'
    env = {**os.environ, 'FILES_CONFIG_DIR': str(base / f'config-{anim}'), 'XDG_DATA_HOME': str(base / 'data'),
           'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_FIXED_SIZE': '1', 'FILES_TEST_ISOLATE_INPUT': '1',
           'FILES_TEST_TOOLKIT_POPUP': '1', 'OMARCHY_FLTK_ANIM': anim, 'TMPDIR': str(base), 'MAGICK_TEMPORARY_PATH': str(base)}
    with log.open('w') as output:
        process = subprocess.Popen([str(args.binary), str(listing), '--size', '900x520', '--scale', '1', '--no-session',
                                    '--ui-smoke', '--exit-after', '40'],
                                   env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
    try:
        def text():
            return log.read_text()
        def wait(check, reason, timeout=12):
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                v = check()
                if v:
                    return v
                if process.poll() is not None:
                    raise AssertionError(f'{reason}; exit {process.returncode}')
                time.sleep(.01)
            raise AssertionError(reason)
        def command(action, marker=None):
            after = len(text())
            pending = capture.with_suffix('.pending')
            pending.write_text(action)
            pending.replace(capture.with_suffix('.popup-command'))
            wait(lambda: (marker or f'FILES_TOOLKIT_FIXTURE action={action}') in text()[after:], action)
            return text()[after:]
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
            parts = capture.read_bytes().split(b'\n', 3)
            dims = [int(v) for v in parts[1].split()]
            png = EVIDENCE / f'{args.label}-{anim}-{label}-{time.time_ns()}.png'
            subprocess.run(['magick', str(capture), str(png)], check=True, env=env, timeout=20)
            report['captures'].append(str(png))
            return dims, parts[3]
        def roi(frame, rect):
            (w, _), raw = frame
            x, y, rw, rh = rect
            return b''.join(raw[((y + r) * w + x) * 3:((y + r) * w + x + rw) * 3] for r in range(rh))

        wait(lambda: 'FILES_UI_SMOKE_OK' in text(), 'smoke')
        time.sleep(.8)
        out = command('tooltip-settings')
        m = re.search(r'FILES_TOOLTIP shown=(\w+) (.*)@(\d+),(\d+),(\d+),(\d+)', out)
        assert m and m.group(1) == 'true', out
        tip_text, tip = m.group(2), tuple(int(v) for v in m.group(3, 4, 5, 6))
        tframe = shot('tooltip')
        command('tooltip-hide')
        time.sleep(.3)
        base_frame = shot('no-tooltip')
        tooltip_drawn = roi(tframe, tip) != roi(base_frame, tip)
        assert tooltip_drawn, 'tooltip surface not painted'
        out = command('settings', 'FILES_TOOLKIT_POPUP_OPEN')
        early = shot('menu-early')
        rect = re.search(r'rect=Some\(\((\d+), (\d+), (\d+), (\d+)\)\)', text())
        time.sleep(.6)
        final = shot('menu-final')
        assert rect, 'menu rect'
        r = tuple(int(v) for v in rect.groups())
        faded = roi(early, r) != roi(final, r)
        command('escape', 'reason=')
        return {'tooltip_text': tip_text, 'tooltip_rect': tip, 'tooltip_painted': tooltip_drawn,
                'menu_rect': r, 'early_differs_from_final': faded}
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    ap.add_argument('--label', default='source')
    args = ap.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    report = {'functional_gate': 'FAIL', 'label': args.label, 'binary_sha256': digest(args.binary),
              'helper_sha256': digest(args.binary.parent / 'files-helper'),
              'html_helper_sha256': digest(args.binary.parent / 'files-html-helper') if (args.binary.parent / 'files-html-helper').is_file() else None,
              'backend': 'wayland' if os.environ.get('WAYLAND_DISPLAY') else 'x11',
              'method': 'toolkit popup fixture, tooltip::show probe, framebuffer captures; no OS input',
              'sessions': {}, 'captures': [], 'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED'}
    out = EVIDENCE / f'{args.label}-{time.time_ns()}.json'
    try:
        base_root = Path.home() / '.cache/files-test'
        base_root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='toolkit-visual-', dir=base_root) as tmp:
            for anim in ['on', 'off']:
                report['sessions'][anim] = session(args, Path(tmp), anim, report)
        on, off = report['sessions']['on'], report['sessions']['off']
        assert on['early_differs_from_final'], 'no fade with animations on'
        assert not off['early_differs_from_final'], 'menu changed after opening with animations off'
        assert 'Preferencias' in on['tooltip_text'] or on['tooltip_text'], on
        report['functional_gate'] = 'PASS'
    except Exception as e:
        report['error'] = repr(e)
    if report['functional_gate'] == 'PASS':
        for f in EVIDENCE.glob(f'{args.label}-*-app.log'):
            f.unlink()
    out.write_text(json.dumps(report, indent=2) + '\n')
    print(report['functional_gate'], out)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
