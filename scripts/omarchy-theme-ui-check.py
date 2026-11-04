#!/usr/bin/env python3
"""Toolkit theme reload and real framebuffer, without input injection or metrics."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

from memcheck import geometry, stop_tree

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'docs/evidence/0.0.4'
BASE = Path.home() / '.cache/files-test'
BASE.mkdir(parents=True, exist_ok=True)
OUT.mkdir(parents=True, exist_ok=True)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def wait(check, message, timeout=12):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        value = check()
        if value:
            return value
        time.sleep(.03)
    raise RuntimeError(message)

def main():
    result = {'functional_gate': 'FAIL', 'resource_gate': 'NOT_MEASURED',
              'startup_gate': 'NOT_MEASURED', 'captures': [],
              'method': 'FLTK framebuffer and internal smoke actions; no native input'}
    process = None
    temp = tempfile.TemporaryDirectory(prefix='toolkit-theme-', dir=BASE)
    base = Path(temp.name)
    log = base / 'files.log'
    try:
        for name, background, surface in [('one', '#152b38', '#12212a'), ('two', '#243749', '#18232f')]:
            d = base / name
            d.mkdir()
            (d / 'colors.toml').write_text(f'background = "{background}"\nforeground = "#e4e8eb"\naccent = "#83b9ef"\n')
            (d / 'shell.toml').write_text(f'[font]\nbase-size = 12\n[popups]\nbackground = "{surface}"\nbackground-alpha = 1.0\n')
        state = base / 'state'; state.mkdir()
        (state / 'theme').symlink_to(base / 'one')
        shell = base / 'shell.toml'; shell.write_text('[font]\nbase-size = 14\n')
        config = base / 'config'; config.mkdir()
        (config / 'config.toml').write_text('corners = "square"\n')
        directory = base / 'files'; directory.mkdir()
        (directory / 'example.txt').write_text('fixture\n')
        raw = base / 'frame.ppm'
        env = dict(os.environ)
        env.pop('FILES_THEME', None)
        env.update(OMARCHY_FLTK_STATE_DIR=str(state), OMARCHY_FLTK_USER_SHELL=str(shell),
                   FILES_CONFIG_DIR=str(config), FILES_TEST_CAPTURE=str(raw),
                   FILES_TEST_FIXED_SIZE='1', FILES_TEST_ISOLATE_INPUT='1',
                   TMPDIR=str(base), MAGICK_TEMPORARY_PATH=str(base))
        with log.open('w') as stream:
            process = subprocess.Popen([str(ROOT / 'target/release/files'), str(directory),
                '--size', '1000x650', '--scale', '1', '--ui-smoke', '--exit-after', '12'],
                stdout=subprocess.DEVNULL, stderr=stream, env=env, start_new_session=True)
        wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(), 'smoke')
        wait(lambda: geometry(process.pid, 1000, 650), 'window geometry')
        def capture(label, rgb):
            raw.unlink(missing_ok=True)
            raw.with_suffix('.request').touch()
            def complete():
                if not raw.exists(): return False
                data = raw.read_bytes()
                parts = data.split(b'\n', 3)
                return len(parts) == 4 and parts[0] == b'P6' and parts[1] == b'1000 650' and len(parts[3]) == 1000 * 650 * 3
            wait(complete, 'complete framebuffer')
            data = raw.read_bytes().split(b'\n', 3)[3]
            count = sum(data[i:i+3] == rgb for i in range(0, len(data), 3))
            assert count > 1000, (label, count)
            target = OUT / f'toolkit-theme-{label}-{time.time_ns()}.png'
            subprocess.run(['magick', str(raw), str(target)], env=env, check=True, timeout=10)
            result['captures'].append(str(target))
            return count
        result['initial_pixels'] = capture('initial', bytes.fromhex('152b38'))
        (base / 'one' / 'colors.toml').write_text('background = "#bad')
        wait(lambda: 'FILES_THEME_TOOLKIT_REJECTED initial=false' in log.read_text(), 'partial theme rejection')
        result['retained_pixels'] = capture('partial-retained', bytes.fromhex('152b38'))
        before = len(log.read_text())
        (base / 'one' / 'colors.toml').unlink()
        wait(lambda: 'FILES_THEME_TOOLKIT_REJECTED initial=false' in log.read_text()[before:] and 'colors.toml' in log.read_text()[before:], 'missing colors rejection')
        result['missing_pixels'] = capture('missing-retained', bytes.fromhex('152b38'))
        (base / 'one' / 'colors.toml').write_text('background = "#152b38"\nforeground = "#e4e8eb"\naccent = "#83b9ef"\n')
        time.sleep(.3)
        before = len(log.read_text())
        shell.write_text('[font\nbase-size = 14\n')
        wait(lambda: 'FILES_THEME_TOOLKIT_REJECTED initial=false' in log.read_text()[before:] and 'shell.toml' in log.read_text()[before:], 'malformed shell rejection')
        result['malformed_shell_pixels'] = capture('shell-retained', bytes.fromhex('152b38'))
        shell.write_text('[font]\nbase-size = 14\n')
        (state / 'next').symlink_to(base / 'two')
        os.replace(state / 'next', state / 'theme')
        wait(lambda: 'FILES_THEME_TOOLKIT background=#243749 corners=square radius=0 font_base=14' in log.read_text(), 'toolkit theme reload')
        assert 'surface=#18232f' in log.read_text(), 'surface remained from previous theme'
        result['reloaded_pixels'] = capture('reloaded', bytes.fromhex('243749'))
        wait(lambda: process.poll() is not None, 'normal close', timeout=20)
        assert process.returncode == 0
        result.update(functional_gate='PASS', normal_exit=True, symlink_swap=True, partial_write_retained=True,
                      missing_file_retained=True, malformed_shell_retained=True, last_error_visible=True,
                      user_shell_font_read=True, square_survives_reload=True)
    except BaseException as error:
        result['error'] = repr(error)
    finally:
        if process is not None and process.poll() is None:
            stop_tree(process, None)
        if log.exists(): result['log'] = log.read_text()
        for name, key in [('files', 'binary_sha256'), ('files-helper', 'helper_sha256'),
                          ('files-html-helper', 'html_helper_sha256')]:
            result[key] = sha(ROOT / 'target/release' / name)
        report = OUT / f'toolkit-theme-ui-{time.time_ns()}.json'
        with report.open('x') as stream: json.dump(result, stream, indent=2)
        temp.cleanup()
        print(json.dumps({'report': str(report), 'functional_gate': result['functional_gate']}))
    return 0 if result['functional_gate'] == 'PASS' else 1

if __name__ == '__main__':
    raise SystemExit(main())
