#!/usr/bin/env python3
"""Aceptación aislada de vistas por carpeta; handlers/framebuffer, sin input SO."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
import tomllib

parser = argparse.ArgumentParser()
parser.add_argument('--installed', action='store_true', help='Validar el binario y helpers instalados para este usuario')
parser.add_argument('--wayland', action='store_true', help='Usar el socket Hyprland NODE verificado, sin input SO')
args = parser.parse_args()
ROOT = Path(__file__).resolve().parent.parent
BINARY = Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
RUN = str(time.time_ns())
BASE = Path.home() / '.cache/files-test' / ('folder-views-node-' + RUN)
OUT = ROOT / 'docs/evidence/folder-views' / ('node-functional-' + RUN)
BASE.mkdir(parents=True)
OUT.mkdir(parents=True)
report = dict(gate='RUNNING', backend='Hyprland NODE wayland-1; own windows only' if args.wayland else 'owned Xvfb; isolated runtime directory',
              scope='production handlers and painted targets; own FLTK framebuffer; no native input',
              resource_gate='NOT_MEASURED', startup_gate='NOT_MEASURED', states=[], captures=[])
apps = []
xvfb = None

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def fingerprint(root):
    return {os.fsencode(p.relative_to(root)).hex(): sha(p)
            for p in root.rglob('*') if p.is_file()}

class App:
    def __init__(self, label, root, config, env, size='1000x650'):
        self.label = label
        self.seq = 0
        self.raw = BASE / (label + '.ppm')
        self.log = OUT / (label + '.txt')
        environment = {**env, 'FILES_CONFIG_DIR': str(config), 'FILES_TEST_CAPTURE': str(self.raw),
                       'FILES_TEST_HIDDEN_CONTEXT': '1', 'FILES_TEST_SESSION': '1', 'FILES_TEST_TABS': '1',
                       'FILES_TEST_FIXED_SIZE': '1', 'FILES_TEST_ISOLATE_INPUT': '1', 'FILES_CLOSE_TRACE': '1'}
        with self.log.open('w') as output:
            self.process = subprocess.Popen([str(BINARY), str(root), '--no-session',
                '--ui-smoke', '--ui-scene', 'hidden-context', '--size', size, '--scale', '1',
                '--exit-after', '180'], env=environment, stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL, stderr=output, start_new_session=True)
        apps.append(self)
        self.state(predicate=lambda s: s['folder_views']['loaded'] and not s['folder_views']['busy'])

    def wait(self, check, why, exits=False):
        end = time.monotonic() + 25
        while time.monotonic() < end:
            value = check()
            if value:
                return value
            if self.process.poll() is not None and not exits:
                raise AssertionError(self.label + ': ' + why + '; exit=' + str(self.process.returncode))
            time.sleep(.02)
        raise AssertionError(self.label + ': ' + why)

    def state(self, after=0, predicate=lambda s: True):
        def current():
            rows = []
            for line in self.log.read_text()[after:].splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    state = json.loads(line.removeprefix('FILES_HIDDEN_STATE '))
                    if state['seq'] == self.seq and not state['config_dirty'] and predicate(state):
                        rows.append(state)
            return rows[-1] if rows else None
        result = self.wait(current, 'estado pintado ausente, seq=' + str(self.seq))
        report['states'].append(dict(app=self.label, state=result))
        assert not result['error'], result
        return result

    def command(self, action, exits=False):
        offset = len(self.log.read_text())
        pending = self.raw.with_suffix('.pending')
        pending.write_text(action)
        pending.replace(self.raw.with_suffix('.command'))
        match = self.wait(lambda: re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action=' +
            re.escape(action) + r'$', self.log.read_text()[offset:], re.M), 'ACK comando: ' + action, exits)
        self.seq = int(match[1])
        return offset

    def act(self, action, predicate=lambda s: True):
        return self.state(self.command(action), predicate)

    def palette(self, ident, predicate=lambda s: True):
        self.act('commands-shortcut', lambda s: s['commands'] is not None)
        self.act('commands-query:' + ident)
        self.act('commands-row:' + ident)
        return self.act('commands-run-hit', lambda s: s['commands'] is None and predicate(s))

    def capture(self, name):
        self.raw.unlink(missing_ok=True)
        self.raw.with_suffix('.request').touch()
        def full():
            if not self.raw.exists():
                return None
            parts = self.raw.read_bytes().split(b'\n', 3)
            if len(parts) != 4:
                return None
            width, height = map(int, parts[1].split())
            return (width, height) if len(parts[3]) == width * height * 3 else None
        dimensions = self.wait(full, 'framebuffer incompleto')
        target = OUT / (name + '.png')
        subprocess.run(['magick', str(self.raw), str(target)], check=True, timeout=15)
        report['captures'].append(dict(app=self.label, path=str(target), dimensions=dimensions, sha256=sha(target)))

    def popup(self, action):
        offset = len(self.log.read_text())
        pending = self.raw.with_suffix('.popup-pending')
        pending.write_text(action)
        pending.replace(self.raw.with_suffix('.popup-command'))
        self.wait(lambda: 'FILES_TOOLKIT_FIXTURE action=' + action in self.log.read_text()[offset:],
                  'ACK popup: ' + action)
        return offset

    def close(self):
        self.command('callback', True)
        self.wait(lambda: self.process.poll() is not None, 'cierre drenado ausente', True)
        assert self.process.returncode == 0 and 'FILES_CLOSE_DRAINED' in self.log.read_text()

def idle(s):
    return not s['folder_views']['busy']

def good(s):
    return idle(s) and not s['folder_views']['error']

try:
    runtime = BASE / 'runtime'
    runtime.mkdir(mode=0o700)
    if args.wayland:
        socket = Path('/run/user/1000/wayland-1')
        assert socket.is_socket(), 'Socket Wayland NODE ausente'
        env = {**os.environ, 'WAYLAND_DISPLAY': socket.name, 'XDG_RUNTIME_DIR': str(socket.parent)}
        env.pop('DISPLAY', None)
    else:
        readfd, writefd = os.pipe()
        with (OUT/'xvfb.txt').open('w') as output:
            xvfb = subprocess.Popen(['Xvfb', '-displayfd', str(writefd), '-screen', '0', '1400x900x24',
                                     '-nolisten', 'tcp'], pass_fds=(writefd,), stdout=output, stderr=output)
        os.close(writefd)
        # Xvfb publica el display solamente cuando ya escucha conexiones.
        with os.fdopen(readfd) as stream:
            display = ':' + stream.readline().strip()
        assert display != ':' and xvfb.poll() is None
        env = {**os.environ, 'DISPLAY': display, 'XDG_RUNTIME_DIR': str(runtime)}
        env.pop('WAYLAND_DISPLAY', None)
    source = BASE / os.fsdecode(b'A-\xff')
    other = BASE / 'B'
    config = BASE / 'config'
    for folder in (source, other, config):
        folder.mkdir()
    (config/'config.toml').write_text('show_hidden=false\nfixture_note="preserve"\n')
    for folder in (source, other):
        for i in range(80):
            (folder/f'item-{i:03}.txt').write_bytes(b'owned ' + str(i).encode())
        (folder/'.hidden').write_bytes(b'private fixture')
    before = {os.fsencode(p).hex(): fingerprint(p) for p in (source, other)}
    a = App('A', source, config, env)
    assert not (config/'folder-views.bin').exists(), 'Read no debe crear el archivo'
    a.act('list')
    a.act('inspector-close')
    a.act('grid')
    a.act('grid-zoom-in')
    a.act('sizes-sort')
    a.act('toggle-shortcut', lambda s: s['show_hidden'])
    original = a.act('grid-last', lambda s: s['scroll'] > 0)
    a.act('preferences')
    saved = a.act('folder-save-hit', lambda s: good(s) and len(s['folder_views']['rows']) == 1)
    row = saved['folder_views']['rows'][0]
    assert bytes(row['path_raw']) == os.fsencode(source)
    assert row['view'] == 3 and row['sort'] == 1 and row['zoom'] == 115 and row['scroll'] == original['scroll'], row
    a.capture('saved-preferences-wide')
    a.act('menus-close')
    digest = sha(config/'folder-views.bin')
    a.act('tab-b-location', lambda s: bytes(s['path_raw']) == os.fsencode(other))
    a.act('list')
    restored = a.act('tab-a-location', lambda s: s['view'] == 'grid' and s['scroll'] == row['scroll'])
    assert abs(restored['grid_zoom'] - 1.15) < 0.00001 and restored['size_sort']['column'] == 'Size' and restored['show_hidden']
    a.capture('restored-grid-scroll-wide')
    assert sha(config/'folder-views.bin') == digest, 'Navegar no guarda automáticamente'
    # Dos pestañas del mismo path mantienen sus propias elecciones.
    a.act('tab-new', lambda s: len(s['tabs']) == 2)
    a.act('list')
    a.act('tab-a', lambda s: s['view'] == 'grid')
    a.act('tab-b', lambda s: s['view'] == 'list')
    assert sha(config/'folder-views.bin') == digest
    a.capture('same-path-independent-tabs')
    a.act('fixture-minimum', lambda s: s['width'] == 440 and s['height'] == 440)
    a.act('preferences')
    a.capture('preferences-minimum')
    a.act('folder-forget-hit', lambda s: good(s) and not s['folder_views']['rows'])
    a.act('menus-close')
    # Restituir por paleta y conservar la captura al fallar el lock.
    a.act('tab-a', lambda s: s['view'] == 'grid')
    with (config/'folder-views.lock').open('a+b') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        a.palette('folder-view-save', lambda s: idle(s) and s['folder_views']['unresolved'])
        a.command('callback', True)
        a.wait(lambda: 'FILES_CLOSE_WAIT saves_ready=false' in a.log.read_text(), 'fallo no bloqueó el cierre')
        assert a.process.poll() is None
        a.capture('failed-save-blocks-close-minimum')
        a.act('folder-cancel-close', lambda s: not s['folder_views']['closing'])
        a.act('tab-b-location', lambda s: bytes(s['path_raw']) == os.fsencode(other))
    retry = a.palette('folder-view-retry', lambda s: good(s) and len(s['folder_views']['rows']) == 1)
    assert bytes(retry['folder_views']['rows'][0]['path_raw']) == os.fsencode(source)
    a.capture('retry-keeps-captured-folder')
    a.close()
    b = App('B', source, config, env)
    restarted = b.state(predicate=lambda s: s['view'] == 'grid' and not s['folder_views']['restoring'])
    assert abs(restarted['grid_zoom'] - 1.15) < 0.00001 and restarted['show_hidden']
    b.capture('new-process-restored-folder-default')
    b.close()
    c = App('C-toolkit', source, config, {**env, 'FILES_TEST_TOOLKIT_POPUP': '1'}, size='440x440')
    c.popup('settings')
    c.capture('production-toolkit-preferences-minimum')
    offset = c.popup('pick')
    c.state(offset, lambda s: good(s) and 'guardada' in s['folder_views']['notice'])
    c.popup('settings')
    c.popup('down')
    offset = c.popup('pick')
    c.state(offset, lambda s: good(s) and not s['folder_views']['rows'])
    c.capture('production-toolkit-forgot-folder')
    c.close()
    assert before == {os.fsencode(p).hex(): fingerprint(p) for p in (source, other)}
    assert tomllib.loads((config/'config.toml').read_text())['fixture_note'] == 'preserve'
    report.update(gate='PASS', normal_exits=3, fixture_unchanged=True,
                  checks=['read-only absence', 'raw path', 'explicit save and grid scroll restore',
                          'global hidden', 'two tabs same path independent', 'forget painted target',
                          'busy lock close gate', 'retry exact captured folder after navigation',
                          'new process restore', 'production toolkit popup save/forget at440', 'minimum/wide framebuffers', 'owned fixture unchanged'])
except BaseException as error:
    report.update(gate='FAIL', error=repr(error))
    raise
finally:
    for app in apps:
        if app.process.poll() is None:
            os.killpg(app.process.pid, signal.SIGTERM)
            try:
                app.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(app.process.pid, signal.SIGKILL)
                app.process.wait()
    if xvfb is not None:
        xvfb.terminate()
        xvfb.wait(timeout=5)
    report['binary_sha256'] = sha(BINARY)
    report['binary_path'] = str(BINARY)
    report['processes'] = [dict(label=app.label, pid=app.process.pid, exit=app.process.returncode) for app in apps]
    report['fixture_root'] = str(BASE)
    (OUT/'receipt.json').write_text(json.dumps(report, indent=2, ensure_ascii=True) + '\n')
    print(OUT/'receipt.json', flush=True)
