#!/usr/bin/env python3
"""Omarchy Glassy windows in mapped Files frames; never OS input or personal data.

Scenes (fixture listing, own process each):
  pinned   scripted checks without OMARCHY_FLTK_GLASS paint opaque
  forced   OMARCHY_FLTK_GLASS=on: surfaces carry the glass opacity
  live     OMARCHY_FLTK_GLASS=system with a private toggles directory:
           creating window-glassy.lua turns glass on, removing it restores
           the opaque frame and the Wayland opaque region.

The capture reads the window buffer without its alpha channel, so a glass
surface appears premultiplied (colour x opacity) while text and glyphs drawn
over it keep their full colour. The raw alpha of the same toolkit code path is
read back by vendor/lib-fltk-omarchy/examples/glass_check.rs.

Run it on a hidden Hyprland workspace, e.g.
  hyprctl dispatch 'hl.dsp.exec_cmd("python3 scripts/glass-ui-check.py", { workspace = "special:files-glass silent" })'
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
EVIDENCE = ROOT / 'docs/evidence/glassy-windows-20261104'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


STATUS_ROWS = 32  # status strip, repainted with live metrics every 2 s


def body(frame, width=800):
    """Frame without the live status strip."""
    return frame[:len(frame) - STATUS_ROWS * width * 3]


def compare(opaque, glass, opacity):
    """Pixels that changed must be the opaque colour times the opacity."""
    changed = matched = unchanged_bright = 0
    for i in range(0, len(opaque), 3):
        a, b = opaque[i:i + 3], glass[i:i + 3]
        if a == b:
            if max(a) >= 128:
                unchanged_bright += 1
            continue
        changed += 1
        if all(abs(round(x * opacity) - y) <= 2 for x, y in zip(a, b)):
            matched += 1
    pixels = len(opaque) // 3
    return {'pixels': pixels, 'changed': changed, 'premultiplied_match': matched,
            'changed_fraction': round(changed / pixels, 4),
            'match_fraction': round(matched / max(changed, 1), 4),
            'unchanged_bright_pixels': unchanged_bright}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    parser.add_argument('--label', default='source')
    args = parser.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    backend = os.environ.get('FLTK_BACKEND', 'wayland' if os.environ.get('WAYLAND_DISPLAY') else 'unspecified')
    report = {'functional_gate': 'FAIL', 'label': args.label, 'backend': backend,
              'binary_path': str(args.binary.resolve()), 'binary_sha256': digest(args.binary),
              'helper_sha256': digest(args.binary.parent / 'files-helper'),
              'html_helper_sha256': digest(args.binary.parent / 'files-html-helper') if (args.binary.parent / 'files-html-helper').is_file() else None,
              'method': 'Mapped Files framebuffer captures and app log; no OS input, user toggles untouched',
              'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED', 'scenes': {}, 'captures': []}
    base_root = Path.home() / '.cache/files-test'
    base_root.mkdir(parents=True, exist_ok=True)
    out = EVIDENCE / f'{args.label}-{backend}-{time.time_ns()}.json'
    try:
        with tempfile.TemporaryDirectory(prefix='glass-', dir=base_root) as tmp:
            base = Path(tmp)
            listing = base / 'listing'
            listing.mkdir()
            for i in range(40):
                (listing / f'document-{i:02d}.txt').write_bytes(b'fixture\n')
            (listing / 'folder').mkdir()
            expected = {p.name: (digest(p) if p.is_file() else 'dir') for p in listing.iterdir()}

            def scene(name, extra, steps):
                capture = base / f'{name}.ppm'
                log = base / f'{name}.log'
                env = {**os.environ, 'FILES_CONFIG_DIR': str(base / f'config-{name}'), 'XDG_DATA_HOME': str(base / 'data'),
                       'FILES_TEST_CAPTURE': str(capture), 'FILES_TEST_FIXED_SIZE': '1', 'FILES_TEST_ISOLATE_INPUT': '1',
                       'TMPDIR': str(base), 'MAGICK_TEMPORARY_PATH': str(base)}
                env.pop('OMARCHY_FLTK_GLASS', None)
                env.update(extra)
                with log.open('w') as output:
                    process = subprocess.Popen([str(args.binary), str(listing), '--size', '800x440', '--scale', '1',
                                                '--no-session', '--ui-smoke', '--exit-after', '14'],
                                               env=env, stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
                try:
                    def wait(check, reason, timeout=10):
                        deadline = time.monotonic() + timeout
                        while time.monotonic() < deadline:
                            value = check()
                            if value:
                                return value
                            if process.poll() is not None:
                                raise AssertionError(f'{name}: {reason}; app exit {process.returncode}')
                            time.sleep(.02)
                        raise AssertionError(f'{name}: {reason}')

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
                        wait(complete, f'complete framebuffer {label}')
                        parts = capture.read_bytes().split(b'\n', 3)
                        png = EVIDENCE / f'{args.label}-{backend}-{name}-{label}-{time.time_ns()}.png'
                        subprocess.run(['magick', str(capture), str(png)], check=True, env=env, timeout=10)
                        report['captures'].append(str(png))
                        return parts[3]

                    wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'smoke ready')
                    time.sleep(.6)  # first listing paint settles
                    result = steps(wait, shot, log)
                    wait(lambda: process.poll() is not None, 'normal exit', timeout=20)
                    text = log.read_text()
                    assert process.returncode == 0, f'{name}: exit {process.returncode}'
                    stats = re.findall(r'FILES_GLASS_STATS (.*)', text)
                    result['stats'] = stats[-1] if stats else None
                    result['startup'] = (re.findall(r'FILES_GLASS mode=.*', text) or [None])[0]
                    result['changes'] = re.findall(r'FILES_GLASS_CHANGED .*', text)
                    report['scenes'][name] = result
                    return result
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
                    (EVIDENCE / f'{args.label}-{backend}-{name}.log').write_text(log.read_text() if log.exists() else '')

            frames = {}

            def pinned(wait, shot, log):
                frames['opaque'] = shot('opaque')
                assert 'FILES_GLASS mode=opaque opacity=1.00 overridden=true' in log.read_text()
                return {'pinned_opaque': True}
            scene('pinned', {}, pinned)

            def forced(wait, shot, log):
                text = log.read_text()
                match = re.search(r'FILES_GLASS mode=glassy opacity=([0-9.]+) overridden=true wayland=(\w+)', text)
                assert match, 'forced glass startup line'
                opacity = float(match.group(1))
                frames['glass'] = shot('glass')
                return {'opacity': opacity, 'wayland': match.group(2) == 'true'}
            f = scene('forced', {'OMARCHY_FLTK_GLASS': 'on'}, forced)
            if f['wayland']:
                cmp = compare(frames['opaque'], frames['glass'], f['opacity'])
                report['opaque_vs_glass'] = cmp
                assert cmp['changed_fraction'] >= 0.5, cmp
                assert cmp['match_fraction'] >= 0.97, cmp
                assert cmp['unchanged_bright_pixels'] > 200, cmp
                assert re.search(r'glass_fills=[1-9]\d* .*regions_cleared=[1-9]', f['stats'] or ''), f['stats']
            else:
                assert body(frames['opaque']) == body(frames['glass']), 'non-Wayland backend must stay opaque'
                assert re.search(r'glass_fills=0 .*regions_cleared=0', f['stats'] or ''), f['stats']
                report['opaque_vs_glass'] = 'identical (backend without per-pixel alpha)'

            toggles = base / 'state/omarchy/toggles/hypr'
            (base / 'state/omarchy').mkdir(parents=True)

            def live(wait, shot, log):
                before = shot('system-transparent')
                assert body(before) == body(frames['opaque']), 'transparent mode must match the opaque frame'
                toggles.mkdir(parents=True)
                (toggles / 'window-glassy.lua').write_text('-- fixture\n')
                wait(lambda: 'FILES_GLASS_CHANGED mode=glassy' in log.read_text(), 'glassy picked up live')
                time.sleep(.3)
                on = shot('system-glassy')
                (toggles / 'window-glassy.lua').unlink()
                wait(lambda: 'FILES_GLASS_CHANGED mode=transparent' in log.read_text(), 'leaving glassy picked up live')
                time.sleep(.3)
                after = shot('system-back')
                assert body(after) == body(frames['opaque']), 'leaving glassy must restore the opaque frame'
                if f['wayland']:
                    assert body(on) == body(frames['glass']), 'live glassy must match the forced glass frame'
                return {'transparent_equals_opaque': True, 'glassy_equals_forced_outside_status': body(on) == body(frames['glass']), 'status_rows_excluded': STATUS_ROWS,
                        'back_equals_opaque': True}
            l = scene('live', {'OMARCHY_FLTK_GLASS': 'system', 'OMARCHY_FLTK_TOGGLES_DIR': str(toggles)}, live)
            if f['wayland']:
                assert re.search(r'regions_restored=[1-9]', l['stats'] or ''), l['stats']
            observed = {p.name: (digest(p) if p.is_file() else 'dir') for p in listing.iterdir()}
            assert observed == expected, 'fixture changed'
            report['fixture_unchanged'] = True
            report['functional_gate'] = 'PASS'
    except Exception as e:  # keep failed evidence
        report['error'] = repr(e)
    out.write_text(json.dumps(report, indent=2) + '\n')
    print(report['functional_gate'], out)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
