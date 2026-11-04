#!/usr/bin/env python3
"""SHA-256 real: fixtures propias, referencia hashlib y handlers/framebuffer; sin métricas."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--installed', action='store_true', help='Validar el binario y helpers instalados para este usuario')
parser.add_argument('--wayland', action='store_true', help='Usar el socket Hyprland NODE verificado, sin input SO')
args = parser.parse_args()
ROOT = Path(__file__).resolve().parent.parent
BINARY = Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
RUN = str(time.time_ns())
BASE = Path.home() / '.cache/files-test' / ('checksums-node-' + RUN)
OUT = ROOT / 'docs/evidence/checksums' / ('node-functional-' + RUN)
BASE.mkdir(parents=True)
OUT.mkdir(parents=True)
report = dict(gate='RUNNING', backend='Hyprland NODE wayland-1; own windows only' if args.wayland else 'owned Xvfb; isolated runtime directory',
              scope='production handlers and painted targets; own FLTK framebuffer; no native input',
              resource_gate='NOT_MEASURED', startup_gate='NOT_MEASURED', states=[], captures=[])
apps = []
xvfb = None

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class App:
    def __init__(self, label, root, config, env, size='1000x650'):
        self.label = label
        self.seq = 0
        self.raw = BASE / (label + '.ppm')
        self.log = OUT / (label + '.txt')
        environment = {**env, 'FILES_CONFIG_DIR': str(config), 'FILES_TEST_CAPTURE': str(self.raw),
                       'FILES_TEST_PREVIEW': '1', 'FILES_TEST_HIDDEN_CONTEXT': '1', 'FILES_TEST_SESSION': '1', 'FILES_TEST_TABS': '1',
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
    config = BASE / 'config'
    source.mkdir(); config.mkdir()
    (source/'folder').mkdir()
    (config/'config.toml').write_text('show_hidden=false\nfixture_note="preserve"\n')
    contents = {b'a.txt': b'abc', b'b.txt': b'abc', b'c.txt': bytes(range(256))*400, b'raw-\xfe\n.txt': b'abc'}
    for name,data in contents.items():
        with open(os.fsencode(source)+b'/'+name,'wb') as stream: stream.write(data)
    sparse = source/'cancel.bin'
    with sparse.open('wb') as stream: stream.truncate(64*1024**3)
    sparse_before = (sparse.stat().st_ino, sparse.stat().st_size, sparse.stat().st_mtime_ns, sparse.stat().st_ctime_ns)
    before = {name.hex(): hashlib.sha256(data).hexdigest() for name,data in contents.items()}
    def check_files():
        actual = {}
        for name in contents:
            with open(os.fsencode(source)+b'/'+name,'rb') as stream: actual[name.hex()] = hashlib.sha256(stream.read()).hexdigest()
        assert actual == before
        assert (sparse.stat().st_ino, sparse.stat().st_size, sparse.stat().st_mtime_ns, sparse.stat().st_ctime_ns) == sparse_before
        assert sorted(os.fsencode(n) for n in os.listdir(source)) == sorted(list(contents)+[b'cancel.bin',b'folder'])
    def wire(path):
        value = os.fsencode(path)
        try: return value.decode('utf-8')
        except UnicodeDecodeError: return list(value)
    request = dict(command='hash_files',id=42,root=wire(source),names=[dict(path=wire(n)) for n in contents])
    def helper(req):
        return subprocess.run([str(BINARY.parent/'files-helper'),'hash_files'],input=(json.dumps(req)+'\n').encode(),capture_output=True,timeout=20)
    result = helper(request)
    assert result.returncode == 0, result.stderr
    frames = [json.loads(line) for line in result.stdout.splitlines()]
    assert frames[-1] == dict(id=42,kind='done',count=4)
    observed = [frame for frame in frames if frame['kind']=='item']
    assert len(observed)==4 and all(f['id']==42 for f in frames)
    for frame,(name,data) in zip(observed,contents.items()):
        row=frame['result']['Ok'];assert bytes(row['sha256']).hex()==before[name.hex()] and row['proof']['file']['bytes']==len(data)
    duplicate = {**request, 'names': [dict(path='a.txt'),dict(path='a.txt')]}
    bad = helper(duplicate);assert bad.returncode != 0 and not bad.stdout
    report['wire'] = dict(reference='Python hashlib.sha256',valid_frames=frames,duplicate_rejected=True,duplicate_stderr=bad.stderr.decode('utf-8','replace'))
    a = App('A', source, config, env)
    a.act('list');a.act('selection-clear')
    empty=a.act('checksums-open');assert empty['checksums'] is None and 'Marca' in empty['error']
    a.act('select-name:a.txt'); focused=a.act('checksums-toggle-focused-mark')
    assert not focused['selection'] and focused['selected'] is not None
    focused=a.palette('checksums',lambda s:s['checksums'] is not None)
    assert len(focused['checksums']['names'])==1 and bytes(focused['checksums']['names'][0])==b'a.txt'
    a.act('checksums-close-hit',lambda s:s['checksums'] is None)
    a.act('select-name:folder')
    unsupported=a.act('checksums-open');assert unsupported['checksums'] is None and 'regulares' in unsupported['error']
    assert 'command=hash_files' not in a.log.read_text()
    a.act('filter-query:ext:txt type:file');a.act('selection-filtered')
    panel=a.palette('checksums',lambda s:s['checksums'] is not None)
    assert not panel['error'] and not panel['checksums']['busy'] and not panel['checksums']['rows']
    assert 'command=hash_files' not in a.log.read_text(), 'abrir la revisión no debe leer archivos'
    assert {bytes(n) for n in panel['checksums']['names']}==set(contents)
    a.capture('selected-files-review')
    calculated=a.act('checksums-start-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'] and len(s['checksums']['rows'])==4)
    tool=calculated['checksums'];assert not tool['error'] and '1 grupo coincidente' in tool['comparison'],tool
    for name,result in zip(tool['names'],tool['rows']):
        assert bytes(result['Ok']['sha256']).hex()==before[bytes(name).hex()]
    a.capture('sha256-comparison-wide')
    a.act('checksums-last');a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
    a.capture('sha256-full-digest-minimum')
    copied=a.act('checksums-copy-hit');assert 'solicitada' in copied['checksums']['note']
    a.act('checksums-close-hit',lambda s:s['checksums'] is None)
    def gone(pid,start):
        try: return int(Path('/proc',str(pid),'stat').read_text().rsplit(')',1)[1].split()[19])!=start
        except FileNotFoundError: return True
    def start_large():
        a.act('select-name:cancel.bin');a.act('clip-actions');a.act('checksums-menu-hit')
        offset=a.command('checksums-start-hit')
        state=a.state(offset,lambda s:s['checksums'] is not None and s['checksums']['busy'] and not s['checksums']['pending'] and s['checksums']['progress']['total']==64*1024**3)
        records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=hash_files',a.log.read_text()[offset:])
        assert records
        pid,start=map(int,records[-1]);assert not gone(pid,start)
        return pid,start
    pid,start=start_large();a.capture('sha256-reading-cancellable')
    cancelled=a.act('checksums-cancel-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'])
    assert 'cancelado' in cancelled['checksums']['error'] and not cancelled['checksums']['rows']
    assert gone(pid,start);a.capture('sha256-cancelled-after-ack')
    a.act('checksums-close-hit',lambda s:s['checksums'] is None)
    start_large();a.close()
    records={(int(pid),int(start)) for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=hash_files',a.log.read_text())}
    assert len(records)==3 and all(gone(pid,start) for pid,start in records)
    b = App('B-toolkit', source, config, {**env, 'FILES_TEST_TOOLKIT_POPUP': '1'}, size='440x440')
    b.act('select-name:a.txt');b.popup('actions');b.capture('toolkit-checksum-action-minimum')
    offset=b.popup('pick');b.state(offset,lambda s:s['checksums'] is not None and not s['checksums']['busy'])
    single=b.act('checksums-start-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'] and len(s['checksums']['rows'])==1)
    assert bytes(single['checksums']['rows'][0]['Ok']['sha256']).hex()==before[b'a.txt'.hex()]
    b.capture('toolkit-checksum-result-minimum');b.act('checksums-close-hit',lambda s:s['checksums'] is None);b.close()
    extra={(int(pid),int(start)) for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=hash_files',b.log.read_text())}
    assert len(extra)==1 and all(gone(pid,start) for pid,start in extra)
    records |= extra
    check_files()
    report.update(gate='PASS',normal_exits=2,checks=['independent hashlib wire reference and raw names','zero selection and directory rejected without helper','explicit review before IO','one helper for4 files','SHA256 matching group and full digest','copy request only; no OS clipboard claim','440 and1000 layout','actual sparse-file cancellation after reader opened','root callback while helper active, exact drain ACK','production toolkit Actions popup and SHA-256 result at440','all4 logged helper identities gone/reused; strict noZ'],small_fixture_sha256=before,sparse_stat_unchanged=True,logged_helper_identities=sorted(records))
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
    if 'sparse' in globals():
        sparse.unlink(missing_ok=True)
        report['sparse_cleanup'] = 'own sparse file removed by harness after own-root shutdown; not an app mutation'
    report['functional_gate'] = report['gate']
    report['helper_sha256'] = sha(BINARY.parent/'files-helper')
    if (BINARY.parent/'files-html-helper').is_file(): report['html_helper_sha256'] = sha(BINARY.parent/'files-html-helper')
    report['binary_sha256'] = sha(BINARY)
    report['binary_path'] = str(BINARY)
    report['processes'] = [dict(label=app.label, pid=app.process.pid, exit=app.process.returncode) for app in apps]
    report['fixture_root'] = str(BASE)
    (OUT/'receipt.json').write_text(json.dumps(report, indent=2, ensure_ascii=True) + '\n')
    print(OUT/'receipt.json', flush=True)
