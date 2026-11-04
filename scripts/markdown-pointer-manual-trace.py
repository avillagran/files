#!/usr/bin/env python3
"""Manual Markdown pointer trace: own fixture, no clipboard or injected input."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
BASE = Path.home() / '.cache/files-test'
OUT = ROOT / 'docs/evidence/0.0.4'
BASE.mkdir(parents=True, exist_ok=True)
OUT.mkdir(parents=True, exist_ok=True)
run_id = str(time.time_ns())
report = {'method': 'user mouse on private Markdown fixture; Wayland pointer event counts only',
          'functional_gate': 'UNVALIDATED', 'clipboard': 'NOT_READ',
          'resource_gate': 'NOT_MEASURED', 'startup_gate': 'NOT_MEASURED',
          'raw_wayland_log': 'temporary and deleted; no keyboard or clipboard contents retained'}
app = Path.home() / '.local/bin/files'
report['binary_sha256'] = hashlib.sha256(app.read_bytes()).hexdigest()
process = None
with tempfile.TemporaryDirectory(prefix='markdown-pointer-', dir=BASE) as tmp:
    base = Path(tmp)
    folder = base / 'files'
    shutil.copytree(ROOT / 'scripts/fixtures/markdown', folder)
    raw = base / 'capture.ppm'
    log = base / 'wayland.log'
    config = base / 'config'
    config.mkdir()
    env = {**os.environ, 'FILES_CONFIG_DIR': str(config), 'FILES_TEST_CAPTURE': str(raw),
           'FILES_TEST_PREVIEW': '1', 'FILES_TRACE': '1', 'WAYLAND_DEBUG': 'client',
           'TMPDIR': str(ROOT / '.cargo/tmp')}
    def wait(check, reason, timeout=20):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            value = check()
            if value:
                return value
            if process.poll() is not None:
                raise RuntimeError(reason + ': app exited')
            time.sleep(.025)
        raise TimeoutError(reason)
    def command(value):
        previous = log.read_text(errors='replace').count('FILES_PREVIEW_COMMAND_OK ' + value + '\n')
        pending = raw.with_suffix('.pending')
        pending.write_text(value)
        os.replace(pending, raw.with_suffix('.command'))
        wait(lambda: log.read_text(errors='replace').count('FILES_PREVIEW_COMMAND_OK ' + value + '\n') > previous, 'command ' + value)
    try:
        with log.open('w') as stream:
            process = subprocess.Popen([str(app), str(folder), '--size', '1000x650', '--scale', '1',
                                        '--ui-smoke', '--ui-scene', 'markdown', '--exit-after', '120'],
                                       env=env, stdout=subprocess.DEVNULL, stderr=stream, start_new_session=True)
        wait(lambda: 'FILES_UI_SMOKE_OK' in log.read_text(errors='replace'), 'smoke')
        command('markdown')
        wait(lambda: any('layout_ready=true' in line and 'overview.md' in line
                         for line in log.read_text(errors='replace').splitlines()
                         if line.startswith('FILES_PREVIEW_MARKDOWN_READY')), 'markdown layout')
        start = log.stat().st_size
        print('Fixture Markdown lista. Arrastra sobre texto visible; no copies ni escribas. Pulsa Enter aquí al terminar.', flush=True)
        input()
        raw.unlink(missing_ok=True)
        raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():
                return False
            parts = raw.read_bytes().split(b'\n', 3)
            if len(parts) != 4 or parts[0] != b'P6':
                return False
            width, height = map(int, parts[1].split())
            return len(parts[3]) == width * height * 3
        wait(complete, 'framebuffer')
        image = OUT / ('markdown-pointer-manual-' + run_id + '.png')
        subprocess.run(['magick', str(raw), str(image)], check=True, timeout=10, env=env)
        report['capture'] = str(image)
        with log.open('rb') as stream:
            stream.seek(start)
            lines = stream.read().decode(errors='replace').splitlines()
        report['wayland_pointer_events'] = {name: sum(bool(re.search(r'wl_pointer@\d+\.' + name + r'\(', line)) for line in lines)
                                             for name in ('enter', 'leave', 'motion', 'button')}
        report['scope'] = 'Counts show events on this Wayland client, not dispatch into the Markdown handler or selection offsets.'
        command('close')
        process.wait(timeout=15)
        text = log.read_text(errors='replace')
        report['normal_exit_after_drain'] = process.returncode == 0 and 'FILES_PREVIEW_DRAINED' in text
    except BaseException as error:
        report['error'] = repr(error)
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        output = OUT / ('markdown-pointer-manual-' + run_id + '.json')
        with output.open('x') as stream:
            json.dump(report, stream, indent=2)
        print('Informe:', output, flush=True)
