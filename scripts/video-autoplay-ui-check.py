#!/usr/bin/env python3
"""Space on a video plays at once, at the source's pixels and rate, with a
global saved mute (sound on by default). Internal handlers and the real FLTK
framebuffer; no OS input. The audio track is digital silence (zero PCM), so
the default output plays nothing audible; audibility is NOT validated here.

Run on a hidden Hyprland workspace, e.g.
  hyprctl dispatch 'hl.dsp.exec_cmd("python3 scripts/video-autoplay-ui-check.py", { workspace = "special:files-video silent" })'
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
EVIDENCE = ROOT / 'docs/evidence/video-autoplay-20261104'
READY = re.compile(r'^FILES_PREVIEW_VIDEO_READY .*$', re.M)


def fields(row):
    out = {}
    for key in ['width', 'height', 'playing', 'sequence', 'audio_enabled', 'muted', 'audio_failed', 'revision']:
        m = re.search(rf' {key}=(\S+)', row)
        out[key] = m and m.group(1)
    m = re.search(r' target=\((\d+), (\d+)\)', row)
    out['target'] = (int(m[1]), int(m[2])) if m else None
    m = re.search(r' device_scale=([0-9.]+)', row)
    out['device_scale'] = float(m[1]) if m else None
    m = re.search(r' rate=Some\(\((\d+), (\d+)\)\)', row)
    out['rate'] = (int(m[1]), int(m[2])) if m else None
    seq = re.search(r'sequence=Some\((\d+)\)', row)
    out['seq'] = int(seq[1]) if seq else None
    return out


def fit(dw, dh, bw, bh, side=3840, area=3840 * 2160):
    bw, bh = min(bw, side), min(bh, side)
    div = max(dw / bw, dh / bh, 1.0)
    w, h = max(int(dw / div), 1), max(int(dh / div), 1)
    if w * h > area:
        f = (area / (w * h)) ** .5
        div = max(dw / int(w * f), dh / int(h * f), 1.0)
        w, h = max(int(dw / div), 1), max(int(dh / div), 1)
    return w, h


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    parser.add_argument('--label', default='source')
    parser.add_argument('--scale', default='2', help="FLTK --scale; '1' on a HiDPI output checks the real device scale")
    args = parser.parse_args()
    EVIDENCE.mkdir(parents=True, exist_ok=True)
    report = {'functional_gate': 'FAIL', 'label': args.label, 'fltk_scale_arg': args.scale,
              'binary_sha256': hashlib.sha256(args.binary.read_bytes()).hexdigest(),
              'helper_sha256': hashlib.sha256((args.binary.parent / 'files-helper').read_bytes()).hexdigest(),
              'html_helper_sha256': hashlib.sha256((args.binary.parent / 'files-html-helper').read_bytes()).hexdigest() if (args.binary.parent / 'files-html-helper').is_file() else None,
              'method': 'internal preview commands, real ffprobe/ffmpeg decode, real FLTK window; no OS input',
              'audible_verification': 'NOT_VALIDATED: zero PCM track', 'resource_gate': 'NOT_MEASURED',
              'startup_gate': 'NOT_MEASURED', 'sessions': {}, 'captures': []}
    if os.environ.get('WAYLAND_DISPLAY'):
        instances = json.loads(subprocess.check_output(['hyprctl', 'instances', '-j'], timeout=2))
        matches = [i for i in instances if i.get('wl_socket') == os.environ.get('WAYLAND_DISPLAY')]
        if len(matches) == 1:
            os.environ['HYPRLAND_INSTANCE_SIGNATURE'] = matches[0]['instance']
    out = EVIDENCE / f'{args.label}-{time.time_ns()}.json'
    base = Path.home() / '.cache/files-test'
    base.mkdir(parents=True, exist_ok=True)
    try:
        with tempfile.TemporaryDirectory(prefix='video-autoplay-', dir=base) as tmp:
            test = Path(tmp)
            folder = test / 'files'
            folder.mkdir()
            clip = folder / 'clip.mkv'
            subprocess.run(['ffmpeg', '-v', 'error', '-nostdin', '-f', 'lavfi', '-i', 'testsrc2=s=1920x1080:r=30:d=8',
                            '-f', 'lavfi', '-i', 'anullsrc=r=48000:cl=stereo:d=8', '-c:v', 'libx264', '-preset', 'ultrafast',
                            '-pix_fmt', 'yuv420p', '-c:a', 'aac', '-shortest', str(clip)], check=True, timeout=60)
            report['source'] = {'codec': 'h264', 'size': [1920, 1080], 'rate': '30/1', 'audio': 'aac zero PCM', 'bytes': clip.stat().st_size}
            config = test / 'config'

            def session(name, extra, steps):
                raw = test / f'{name}.ppm'
                log = test / f'{name}.log'
                env = {**os.environ, 'FILES_CONFIG_DIR': str(config), 'FILES_TEST_CAPTURE': str(raw), 'FILES_TEST_PREVIEW': '1',
                       'FILES_TEST_ISOLATE_INPUT': '1', 'FILES_TEST_FIXED_SIZE': '1', **extra}
                with log.open('w') as output:
                    process = subprocess.Popen([str(args.binary), str(folder), '--size', '1200x760', '--scale', args.scale, '--ui-smoke',
                                                '--ui-scene', 'video', '--exit-after', '40'],
                                               env=env, stderr=output, stdout=subprocess.DEVNULL, start_new_session=True)
                try:
                    def wait(check, reason, timeout=15):
                        deadline = time.monotonic() + timeout
                        while time.monotonic() < deadline:
                            value = check()
                            if value:
                                return value
                            if process.poll() is not None:
                                raise AssertionError(f'{name}: {reason}; exited')
                            time.sleep(.025)
                        raise AssertionError(f'{name}: {reason}')

                    def command(action):
                        # The folder watch's initial reload can briefly empty the
                        # listing; a "missing entry" there is retried, not a pass.
                        deadline = time.monotonic() + 8
                        while True:
                            after = len(log.read_text())
                            pending = raw.with_suffix('.pending')
                            pending.write_text(action)
                            os.replace(pending, raw.with_suffix('.command'))
                            done = wait(lambda: re.search(r'FILES_PREVIEW_COMMAND_(REV action=' + re.escape(action) + r' |ERROR ' + re.escape(action) + r': )',
                                                          log.read_text()[after:]), 'command ' + action)
                            if 'REV' in done.group(0):
                                return after
                            report.setdefault('retried_commands', []).append(action)
                            assert time.monotonic() < deadline, 'command kept failing: ' + action
                            time.sleep(.2)

                    def rows(after=0):
                        return [fields(r) for r in READY.findall(log.read_text()[after:])]

                    def shot(label):
                        raw.unlink(missing_ok=True)
                        raw.with_suffix('.request').touch()

                        def complete():
                            if not raw.exists():
                                return False
                            parts = raw.read_bytes().split(b'\n', 3)
                            if len(parts) != 4:
                                return False
                            dims = [int(v) for v in parts[1].split()]
                            return len(dims) == 2 and len(parts[3]) == dims[0] * dims[1] * 3
                        wait(complete, 'capture ' + label)
                        png = EVIDENCE / f'{args.label}-{name}-{label}-{time.time_ns()}.png'
                        subprocess.run(['magick', str(raw), str(png)], check=True, timeout=20)
                        report['captures'].append(str(png))

                    wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'smoke ready')
                    result = steps(command, rows, wait, shot)
                    report['sessions'][name] = result
                    return result
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait()
                    (EVIDENCE / f'{args.label}-{name}.log').write_text(log.read_text() if log.exists() else '')

            def first_open(command, rows, wait, shot):
                after = command('video')
                # No play command: it must start by itself.
                playing = wait(lambda: next((r for r in rows(after) if r['playing'] == 'true' and r['seq'] is not None), None),
                               'autoplay produced no stream frame')
                t0, s0 = time.monotonic(), playing['seq']
                time.sleep(2.0)
                last = [r for r in rows(after) if r['seq'] is not None][-1]
                rate = (last['seq'] - s0) / (time.monotonic() - t0)
                expected = fit(1920, 1080, *last['target'])
                assert (int(last['width']), int(last['height'])) == expected, (last, expected)
                assert last['rate'] == (30, 1), last
                assert last['audio_enabled'] == 'true' and last['muted'] == 'false', last
                assert last['audio_failed'] == 'false', last
                assert rate >= 20, f'stream rate {rate:.1f} fps'
                shot('playing-native')
                # Mute: global and saved.
                after = command('video-audio')
                muted = wait(lambda: next((r for r in rows(after) if r['muted'] == 'true'), None), 'mute not applied')
                wait(lambda: 'preview_muted = true' in (config / 'config.toml').read_text() if (config / 'config.toml').exists() else False,
                     'mute not saved')
                command('preview-close')
                after = command('video')
                reopened = wait(lambda: next((r for r in rows(after) if r['playing'] == 'true' and r['seq'] is not None), None),
                                'reopened video did not autoplay')
                assert reopened['muted'] == 'true' and reopened['audio_enabled'] == 'false', reopened
                return {'autoplay_first_stream_frame': True, 'stream_size': [int(last['width']), int(last['height'])],
                        'device_target': list(last['target']), 'device_scale': last['device_scale'],
                        'declared_rate': list(last['rate']), 'observed_stream_fps_2s': round(rate, 1),
                        'sound_by_default': True, 'mute_saved': True, 'reopen_starts_muted': True}
            session('first', {'FILES_TEST_VIDEO_AUTOPLAY': '1'}, first_open)

            def restart(command, rows, wait, shot):
                after = command('video')
                r = wait(lambda: next((r for r in rows(after) if r['playing'] == 'true' and r['seq'] is not None), None),
                         'autoplay after restart')
                assert r['muted'] == 'true' and r['audio_enabled'] == 'false', r
                after = command('video-audio')
                wait(lambda: next((r for r in rows(after) if r['muted'] == 'false' and r['audio_enabled'] == 'true'), None), 'unmute')
                wait(lambda: 'preview_muted = false' in (config / 'config.toml').read_text(), 'unmute not saved')
                return {'restart_keeps_mute': True, 'unmute_saved': True}
            session('restart', {'FILES_TEST_VIDEO_AUTOPLAY': '1'}, restart)

            def legacy(command, rows, wait, shot):
                after = command('video')
                r = wait(lambda: next((r for r in rows(after)), None), 'legacy snapshot')
                time.sleep(1.0)
                last = rows(after)[-1]
                assert last['playing'] == 'false', last
                return {'scripted_checks_keep_paused_first_frame': True}
            session('legacy', {}, legacy)
            report['functional_gate'] = 'PASS'
    except Exception as e:
        report['error'] = repr(e)
    out.write_text(json.dumps(report, indent=2) + '\n')
    print(report['functional_gate'], out)
    return 0 if report['functional_gate'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
