#!/usr/bin/env python3
"""Duplicados por carpeta: grupos, límites y cancelación reales; sin métricas."""
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
parser.add_argument('--properties-layout', action='store_true', help='Validar rutas completas y permisos mínimos en el flujo search-labels')
parser.add_argument('--search-labels', action='store_true', help='Verificar resultados y destinos raw en búsqueda/historial/favoritos')
parser.add_argument('--capture-audit', action='store_true', help='IDs/ACK de captura y par observado/repintado en doble panel')
parser.add_argument('--name-labels', action='store_true', help='Capturar etiquetas raw en vistas normales tras ubicar')
parser.add_argument('--installed', action='store_true', help='Validar el binario y helpers instalados para este usuario')
parser.add_argument('--wayland', action='store_true', help='Usar el socket Hyprland NODE verificado, sin input SO')
args = parser.parse_args()
if args.properties_layout and not args.search_labels:parser.error("--properties-layout requiere --search-labels")
ROOT = Path(__file__).resolve().parent.parent
BINARY = Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
RUN = str(time.time_ns())
BASE = Path.home() / '.cache/files-test' / ('duplicates-node-' + RUN)
OUT = ROOT / 'docs/evidence/duplicates' / ('node-functional-' + RUN)
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
        self.capture_seq = 0
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

    def frame(self, name, redraw=False):
        self.capture_seq += 1
        ident=self.capture_seq
        offset=len(self.log.read_text())
        self.raw.unlink(missing_ok=True)
        request=self.raw.with_suffix('.request')
        if args.capture_audit:
            pending=request.with_suffix('.capture-pending')
            pending.write_text(json.dumps(dict(id=ident,redraw=redraw)))
            pending.replace(request)
            match=self.wait(lambda:re.search(r'FILES_CAPTURE_OK id='+str(ident)+r' redraw=(true|false) before=(\d+) paint=(\d+) width=(\d+) height=(\d+)',self.log.read_text()[offset:]),'ACK captura '+str(ident))
            before,paint,width,height=map(int,match.groups()[1:])
            assert (match[1]=='true')==redraw
            if redraw:assert paint>before
        else:
            assert not redraw
            request.touch()
            def full():
                if not self.raw.exists():return None
                parts=self.raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return None
                w,h=map(int,parts[1].split())
                return (w,h) if len(parts[3])==w*h*3 else None
            width,height=self.wait(full,'framebuffer incompleto')
            before=paint=None
        data=self.raw.read_bytes();parts=data.split(b'\n',3)
        assert len(parts)==4 and parts[0]==b'P6' and tuple(map(int,parts[1].split()))==(width,height) and len(parts[3])==width*height*3
        target=OUT/(name+'.png')
        subprocess.run(['magick','ppm:-',str(target)],input=data,check=True,timeout=15)
        report['captures'].append(dict(app=self.label,path=str(target),dimensions=[width,height],sha256=sha(target),request_id=ident if args.capture_audit else None,explicit_redraw=redraw,paint_before=before,paint_after=paint))
        return width,height,parts[3]

    def capture(self, name):
        observed=self.frame(name)
        if args.capture_audit and name=='raw-path-dual-title':
            repeated=self.frame(name+'-observed-again')
            fresh=self.frame(name+'-explicit-redraw',True)
            def region(frame):
                w,h,pixels=frame;x0,x1=70,min(350,w//2-80);y0,y1=160,min(420,h-80)
                assert x1>x0 and y1>y0
                return hashlib.sha256(b''.join(pixels[(y*w+x0)*3:(y*w+x1)*3] for y in range(y0,y1))).hexdigest()
            hashes=[region(frame) for frame in [observed,repeated,fresh]]
            report.setdefault('capture_audit',[]).append(dict(app=self.label,scope='left names/icons ROI x70..350 y160..420, excludes header/footer/metadata',observed_again_fresh_sha256=hashes,equal=len(set(hashes))==1))
            assert len(set(hashes))==1,'Región nombres/iconos cambió entre observado y repintado explícito; conservar todas las capturas'

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

sparse_files = []
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
    source=BASE/os.fsdecode(b'A-\xff\n');config=BASE/'config';overflow=BASE/'Overflow'
    for path in [source,config,overflow]:path.mkdir()
    (source/'sub').mkdir();(source/'.hidden').mkdir()
    (config/'config.toml').write_text('show_hidden=false\nfixture_note="preserve"\n')
    payloads={b'a.txt':b'abc',b'sub/b.txt':b'abc',b'.hidden/raw-\xfe\n.txt':b'abc',b'unique.bin':b'unique-length'}
    for name,data in payloads.items():
        with open(os.fsencode(source)+b'/'+name,'wb') as stream:stream.write(data)
    os.link(source/'a.txt',source/'alias.txt');os.symlink('sub',source/'link');os.mkfifo(source/'fifo')
    sparse=source/'large.bin'
    with sparse.open('wb') as stream:stream.truncate(64*1024**3)
    sparse_files.append(sparse)
    before={name.hex():hashlib.sha256(data).hexdigest() for name,data in payloads.items()}
    stat_fields=lambda p:(p.stat().st_ino,p.stat().st_size,p.stat().st_mtime_ns,p.stat().st_ctime_ns)
    sparse_before=stat_fields(sparse)
    for i in range(129):(overflow/f'copy-{i:03}.txt').write_bytes(b'abc')
    def wire(path):
        value=os.fsencode(path)
        try:return value.decode('utf-8')
        except UnicodeDecodeError:return list(value)
    def raw(path):return os.fsencode(path) if isinstance(path,str) else bytes(path)
    def run_helper(path):
        r=subprocess.run([str(BINARY.parent/'files-helper'),'find_duplicates'],input=(json.dumps(dict(command='find_duplicates',id=42,root=wire(path)))+'\n').encode(),capture_output=True,timeout=25)
        assert r.returncode==0,r.stderr
        report.setdefault('wire_bytes',[]).append(dict(root=repr(str(path)),bytes=len(r.stdout)))
        frames=[json.loads(line) for line in r.stdout.splitlines()]
        rows=[]
        for frame in frames:
            if frame['kind']=='page':
                assert frame['offset']==len(rows) and 0<len(frame['rows'])<=128
                assert sum(len(raw(row['path'])) for row in frame['rows'])<=8192
                rows.extend(frame['rows'])
        if frames[-1]['kind']=='done':
            assert not frames[-1]['report']['rows']
            frames[-1]['report']['rows']=rows
        return frames
    frames=run_helper(source);assert frames[-1]['kind']=='done';reference=frames[-1]['report'];p=reference['progress']
    assert p['phase']==2 and p['reasons']==0 and p['errors']==0 and p['groups']==1 and p['matches']==3 and p['aliases']==1 and p['hashed']==3 and p['bytes_read']==9 and p['bytes_total']==9,p
    ids=set()
    for row in reference['rows']:
        assert bytes(row['observation']['sha256']).hex()==hashlib.sha256(b'abc').hexdigest()
        f=row['observation']['proof']['file'];ids.add((f['device'],f['inode']))
    assert len(ids)==3 and any(raw(r['path']).startswith(b'.hidden/') for r in reference['rows'])
    large_wire=BASE/'Wire4096';large_wire.mkdir()
    for i in range(4096):
        name=b'raw-'+b'\xfe'*230+f'-{i:04}'.encode()
        with open(os.fsencode(large_wire)+b'/'+name,'wb') as stream:stream.write(b'abc')
    wire_frames=run_helper(large_wire);wire_report=wire_frames[-1]['report']
    assert report['wire_bytes'][-1]['bytes']>4*1024*1024
    assert len(wire_report['rows'])==4096 and wire_report['progress']['matches']==4096 and wire_report['progress']['groups']==1 and wire_report['progress']['bytes_read']==4096*3 and wire_report['progress']['reasons']==0
    assert len({raw(r['path']) for r in wire_report['rows']})==4096
    assert all(bytes(r['observation']['sha256']).hex()==hashlib.sha256(b'abc').hexdigest() for r in wire_report['rows'])
    report['large_wire_reference']=dict(rows=4096,groups=1,content_bytes=4096*3,wire_bytes=report['wire_bytes'][-1]['bytes'])
    error=run_helper(BASE/'missing');assert error[-1]['kind']=='error' and error[-1]['message']
    report['wire_reference']=reference;report['missing_root_error']=error
    a=App('A',source,config,env)
    a.act('list');a.act('selection-clear')
    if args.name_labels:
        a.capture('raw-path-list-breadcrumb-and-tab')
        a.act('columns',lambda s:s['view']=='columns');a.capture('raw-path-column-title')
        a.act('list',lambda s:s['view']=='list')
        a.palette('dual',lambda s:s['dual']['enabled']);a.capture('raw-path-dual-title')
        a.palette('dual',lambda s:not s['dual']['enabled'])
        a.act('list');a.act('selection-clear')
    opened=a.palette('duplicates',lambda s:s['checksums'] is not None)
    assert opened['checksums']['scan_mode'] and not opened['checksums']['busy'] and opened['checksums']['scan_report'] is None and not opened['selection']
    assert 'command=find_duplicates' not in a.log.read_text()
    a.capture('folder-scan-review')
    result=a.act('checksums-start-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'] and s['checksums']['scan_report'] is not None)
    p=result['checksums']['scan_report']['progress'];assert p['groups']==1 and p['matches']==3 and p['aliases']==1 and p['bytes_read']==9 and not result['checksums']['error'],result['checksums']
    a.capture('nested-hidden-distinct-inode-group')
    a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);a.act('checksums-last');a.capture('duplicate-full-digest-minimum')
    a.act('checksums-copy-hit');a.act('checksums-first')
    located=a.act('duplicates-locate-hit',lambda s:s['checksums'] is None and s['path_raw']==list(os.fsencode(source/'.hidden')) and s['selected']==b'raw-\xfe\n.txt'.hex())
    assert not located['show_hidden'] and located['selection']==[b'raw-\xfe\n.txt'.hex()]
    a.capture('located-raw-name-in-hidden-parent-without-changing-hidden-policy')
    if args.name_labels:
        a.act('groups',lambda s:s['view']=='groups');a.capture('raw-label-groups')
        a.palette('view-grid',lambda s:s['view']=='grid');a.capture('raw-label-grid')
        a.act('columns',lambda s:s['view']=='columns');a.capture('raw-label-columns')
        a.act('list',lambda s:s['view']=='list')
        a.palette('dual',lambda s:s['dual']['enabled']);a.capture('raw-label-dual')
        a.palette('dual',lambda s:not s['dual']['enabled'])
        a.act('list',lambda s:s['view']=='list')
        assert a.state()['path_raw']==list(os.fsencode(source/'.hidden'))
    a.act('back',lambda s:s['path_raw']==list(os.fsencode(source)) and len(s['order'])>1)
    second=source/'large-copy.bin'
    with second.open('wb') as stream:stream.truncate(64*1024**3)
    sparse_files.append(second);second_before=stat_fields(second)
    def gone(pid,start):
        try:return int(Path('/proc',str(pid),'stat').read_text().rsplit(')',1)[1].split()[19])!=start
        except FileNotFoundError:return True
    def start_large():
        a.act('duplicates-open');offset=a.command('checksums-start-hit')
        a.state(offset,lambda s:s['checksums'] is not None and s['checksums']['busy'] and s['checksums']['scan_progress']['phase']==1 and s['checksums']['scan_progress']['bytes_read']>9)
        ids=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=find_duplicates',a.log.read_text()[offset:]);assert ids
        pid,start=map(int,ids[-1]);assert not gone(pid,start);return pid,start
    pid,start=start_large();a.capture('folder-scan-active-reader')
    cancelled=a.act('checksums-cancel-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'])
    assert cancelled['checksums']['scan_report'] is None and 'cancelada' in cancelled['checksums']['error'];assert gone(pid,start)
    a.capture('folder-scan-cancelled-not-no-duplicates');a.act('checksums-close-hit',lambda s:s['checksums'] is None)
    start_large();a.close()
    b=App('B-toolkit',overflow,config,{**env,'FILES_TEST_TOOLKIT_POPUP':'1'},size='440x440')
    b.act('selection-clear');b.popup('actions');b.popup('down');b.capture('folder-scan-toolkit-action')
    offset=b.popup('pick');b.state(offset,lambda s:s['checksums'] is not None and s['checksums']['scan_mode'])
    partial=b.act('checksums-start-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'] and s['checksums']['scan_report'] is not None)
    tool=partial['checksums'];p=tool['scan_report']['progress'];assert p['groups']==1 and p['matches']==129 and p['reasons']==0 and not tool['error'] and len(tool['names'])==128 and tool['scan_pages']==[0,128],tool
    helper_count=b.log.read_text().count('command=find_duplicates')
    b.capture('large-group-first-page')
    last=b.act('duplicates-next-hit',lambda s:s['checksums']['scan_page']==1)
    assert last['checksums']['names']==[list(b'copy-128.txt')] and len(last['checksums']['rows'])==1
    b.capture('large-group-last-page-single-member-with-peer-in-other-page')
    b.act('checksums-copy-hit')
    first=b.act('duplicates-prev-hit',lambda s:s['checksums']['scan_page']==0)
    assert len(first['checksums']['names'])==128 and b.log.read_text().count('command=find_duplicates')==helper_count
    assert 'Sin coincidencias' not in tool['comparison']
    assert any(x['command']=='Copy' for x in first['checksums']['buttons'])
    b.act('checksums-copy-hit')
    for name in ('other-0.txt','other-1.txt'):(overflow/name).write_bytes(b'xyz')
    grouped=b.act('checksums-start-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'] and s['checksums']['scan_report'] is not None and s['checksums']['scan_report']['progress']['groups']==2)
    assert grouped['checksums']['scan_report']['progress']['matches']==131
    second_group=b.act('duplicates-group-next-hit',lambda s:s['checksums']['selected_group']['index']==1)
    assert second_group['checksums']['selected_group']['count']==129
    b.capture('group-identity-count-and-navigation-minimum')
    b.act('duplicates-next-hit',lambda s:s['checksums']['scan_page']==1)
    back_group=b.act('duplicates-group-prev-hit',lambda s:s['checksums']['selected_group']['index']==0 and s['checksums']['scan_page']==0)
    assert back_group['checksums']['selected_group']['count']==2
    located=b.act('duplicates-locate-hit',lambda s:s['checksums'] is None and s['selected']==b'other-0.txt'.hex())
    assert located['path_raw']==list(os.fsencode(overflow))
    b.capture('located-candidate-after-group-navigation')
    assert b.log.read_text().count('command=find_duplicates')==helper_count+1
    (overflow/'.invisible').write_bytes(b'xyz')
    b.act('duplicates-open')
    b.act('checksums-start-hit',lambda s:s['checksums'] is not None and not s['checksums']['busy'] and s['checksums']['scan_report'] is not None)
    b.act('checksums-first')
    hidden=b.act('duplicates-locate-hit',lambda s:s['checksums'] is None and not s['selection'] and s['selected'] is None and bool(s['error']))
    assert not hidden['show_hidden'] and hidden['path_raw']==list(os.fsencode(overflow))
    b.capture('hidden-candidate-preserves-policy-and-clears-unrelated-selection')
    b.close()
    if args.search_labels:
        search_root=BASE/'SearchLabels';search_root.mkdir()
        raw_dir=b'dir-\xff\n';raw_file=b'raw-'+(b'long-'*24 if args.properties_layout else b'')+b'\xfe\t.txt'
        raw_parent=os.fsencode(search_root)+b'/'+raw_dir;os.mkdir(raw_parent)
        with open(raw_parent+b'/'+raw_file,'wb') as stream:stream.write(b'candidate bytes')
        if args.properties_layout:os.chmod(raw_parent+b'/'+raw_file,0o600)
        c=App('C-search-labels',search_root,config,env)
        c.act('list');c.act('recursive-open')
        found=c.act('recursive-query:raw',lambda s:not s['search']['running'] and s['search']['report'] is not None and len(s['search']['report']['hits'])==1)
        assert raw(found['search']['report']['hits'][0]['relative'])==raw_dir+b'/'+raw_file
        c.capture('search-raw-leaf-and-parent-wide')
        c.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);c.capture('search-raw-leaf-and-parent-minimum')
        located=c.act('recursive-locate',lambda s:not s['search']['visible'] and s['path_raw']==list(raw_parent) and s['selected']==raw_file.hex())
        assert located['selection']==[raw_file.hex()]
        c.act('book-open',lambda s:s['bookmarks']['loaded'] and not s['bookmarks']['busy'])
        added=c.act('book-add',lambda s:not s['bookmarks']['busy'] and len(s['bookmarks']['rows'])==1)
        assert added['bookmarks']['rows'][0]['path']==list(raw_parent)
        c.act('book-row:1');c.act('book-edit');c.act('book-text:Candidato')
        c.act('book-save',lambda s:not s['bookmarks']['busy'] and s['bookmarks']['rows'][0]['label']=='Candidato')
        c.act('fixture-wide',lambda s:s['width']==1000);c.capture('favourite-original-path-with-user-label')
        c.act('book-close');c.act('back',lambda s:s['path_raw']==list(os.fsencode(search_root)) and len(s['order'])==1)
        c.act('book-open',lambda s:s['bookmarks']['loaded'] and not s['bookmarks']['busy'])
        c.act('book-key:ctrlf');history=c.act('book-text:'+chr(92)+'xFF')
        assert history['bookmarks']['panel']['query']==chr(92)+'xFF'
        assert 'Back(' in history['bookmarks']['panel']['selected']
        c.capture('history-escaped-caption-filter')
        jumped=c.act('book-jump',lambda s:s['bookmarks']['panel'] is None and s['path_raw']==list(raw_parent) and s['bookmarks']['browser_loaded'])
        assert not jumped['error']
        if args.properties_layout:
            def escaped(value):
                out=''
                for ch in os.fsdecode(value):
                    code=ord(ch)
                    if 0xdc80<=code<=0xdcff:out+=chr(92)+'x'+format(code-0xdc00,'02X')
                    elif ch==chr(92):out+=chr(92)*2
                    elif code in (10,13,9):out+=chr(92)+{10:'n',13:'r',9:'t'}[code]
                    else:out+=ch
                return out
            original=raw_parent+b'/'+raw_file;before_mode=os.stat(original)
            c.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
            shown=c.act('prop-open',lambda s:s['properties'] is not None and not s['properties']['pending'] and s['properties']['observed']==0o600)
            prop=shown['properties'];assert prop['editable'] and not prop['changed'] and prop['path_raw']==list(original)
            assert ''.join(prop['paths']['requested'])=='Ruta solicitada: '+escaped(original)
            assert ''.join(prop['paths']['verified'])=='Ruta verificada: '+escaped(bytes(prop['current_raw']))
            assert len(prop['paths']['requested'])>1
            c.capture('properties-complete-requested-path-440')
            c.act('prop-page-down');c.capture('properties-verified-path-page-440')
            focused=c.act('prop-last-bit',lambda s:s['properties']['focus']==8)
            prop=focused['properties'];x,y,w,h=prop['body'];bits=[b for b in prop['buttons'] if b['command']=='Bit']
            assert len(bits)==9
            assert all(b['rect'][0]>=x and b['rect'][1]>=y and b['rect'][0]+b['rect'][2]<=x+w and b['rect'][1]+b['rect'][3]<=y+h for b in bits)
            c.capture('properties-nine-permission-targets-fit-440')
            c.act('prop-group-read-focus');changed=c.act('prop-toggle-focused',lambda s:s['properties']['requested']==0o640 and s['properties']['changed'])
            buttons=[b['rect'] for b in changed['properties']['buttons'] if b['command']!='Bit'];assert len(buttons)==3
            assert all(a[1]+a[3]<=440-32 for a in buttons)
            for i,a in enumerate(buttons):
                for b in buttons[i+1:]:assert a[0]+a[2]<=b[0] or b[0]+b[2]<=a[0]
            c.capture('properties-nonoverlapping-apply-controls-440')
            applied=c.act('prop-apply',lambda s:s['properties'] is not None and not s['properties']['pending'] and s['properties']['observed']==0o640 and not s['properties']['stale'])
            assert os.stat(original).st_mode&0o777==0o640 and os.stat(original).st_ino==before_mode.st_ino
            c.act('prop-home',lambda s:s['properties']['scroll']==0);c.capture('properties-applied-to-original-raw-file')
            c.act('prop-close',lambda s:s['properties'] is None)
            link=raw_parent+b'/reference-link';os.symlink(raw_file,link)
            c.act('refresh',lambda s:'7265666572656e63652d6c696e6b' in s['order'])
            c.act('select-name:reference-link');readonly=c.act('prop-open',lambda s:s['properties'] is not None and not s['properties']['pending'] and s['properties']['observed'] is not None)
            prop=readonly['properties'];assert not prop['editable'] and not prop['stale'] and ''.join(prop['paths']['link'])=='Destino raw (no abierto): '+escaped(raw_file)
            assert not any(b['command'] in ['Bit','Apply'] for b in prop['buttons'])
            c.act('prop-page-down');c.capture('properties-raw-symlink-target-readonly')
            attempted=c.act('prop-readonly-attempt');assert not attempted['properties']['changed'] and not attempted['properties']['pending']
            assert os.stat(original).st_mode&0o777==0o640 and os.path.islink(link)
            c.act('prop-close',lambda s:s['properties'] is None)
            report['properties_layout_checks']=['complete cached requested/verified raw paths','PageDown/Home navigation','nine bit hits inside clipped body at440','three action buttons do not overlap and remain above footer','chmod600to640 on exact original inode','symlink raw target shown read-only and target mode unchanged']
        c.close()
        search_helpers={(int(pid),int(start)) for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_batches',c.log.read_text())}
        assert len(search_helpers)==1 and all(gone(pid,start) for pid,start in search_helpers)
        report['search_label_helpers']=sorted(search_helpers)
        with open(raw_parent+b'/'+raw_file,'rb') as stream:assert stream.read()==b'candidate bytes'
        report['search_label_checks']=['raw result leaf/parent','wide and minimum captures','locate exact raw bytes','persisted favourite original path and explicit user alias','history visible-escape query and exact jump','fixture content unchanged']
    identities=set()
    for app in apps:
        identities|={(int(pid),int(start)) for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=find_duplicates',app.log.read_text())}
    assert len(identities)==6 and all(gone(pid,start) for pid,start in identities)
    for name,digest in before.items():
        with open(os.fsencode(source)+b'/'+bytes.fromhex(name),'rb') as stream:assert hashlib.sha256(stream.read()).hexdigest()==digest
    assert stat_fields(sparse)==sparse_before and stat_fields(second)==second_before
    assert (source/'alias.txt').stat().st_ino==(source/'a.txt').stat().st_ino and (source/'link').is_symlink()
    assert all((overflow/f'copy-{i:03}.txt').read_bytes()==b'abc' for i in range(129))
    report.update(gate='PASS',normal_exits=len(apps),checks=['recursive hidden/raw names','unique64GiB size skipped: only9bytes hashed','hardlink alias excluded','independent hashlib reference','4096 raw-name matches over4MiB wire without omission','missing root error not empty success','review without IO and explicit empty selection','live-reader cancel and root-close ACK','129 matching files retained across two bounded pages without rehashing','real toolkit popup at440','group count and cross-page navigation without helper','locate raw name after async listing with hidden policy retained','small fixture hashes and sparse stats unchanged'],logged_helper_identities=sorted(identities))
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
    for sparse in sparse_files:
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
