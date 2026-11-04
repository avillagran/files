#!/usr/bin/env python3
"""Owned external changes -> actual Model/watch/paint; no OS input/resource claims."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/listing-model/space-map/navigation';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','states':[],'captures':[],'logs':{}}
apps=[]
class App:
    def __init__(self,label,folder,base):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        config=base/(label+'-config');config.mkdir();(config/'owned-data').mkdir()
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'XDG_DATA_HOME':str(config/'owned-data'),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_SPACE_MAP':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
        with self.log.open('w')as out:self.p=subprocess.Popen([str(BINARY),str(folder),'--ui-smoke','--ui-scene','space-map','--size','1000x650','--scale','1','--exit-after','180'],env=env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
        apps.append(self);self.state(lambda s:s['loaded'] and s['watch_settled'])
    def wait(self,check):
        deadline=time.monotonic()+25
        while time.monotonic()<deadline:
            value=check()
            if value:return value
            if self.p.poll()is not None:raise AssertionError(self.label+' exited '+str(self.p.returncode))
            time.sleep(.02)
        raise AssertionError(self.label+' state/command deadline')
    def state(self,predicate=lambda s:True):
        def check():
            found=[]
            for line in self.log.read_text().splitlines(keepends=True):
                if not line.endswith('\n'):continue
                if line.startswith('FILES_SPACE_STATE '):
                    s=json.loads(line[len('FILES_SPACE_STATE '):])
                    if s['seq']==self.seq:found.append(s)
            return found[-1]if found and predicate(found[-1])else None
        s=self.wait(check);report['states'].append({'app':self.label,'state':s});return s
    def act(self,command,predicate=lambda s:True):
        offset=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(command);pending.replace(self.raw.with_suffix('.command'))
        match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(command)+r'$',self.log.read_text()[offset:],re.M));self.seq=int(match[1]);return self.state(predicate)
    def ready(self,predicate=lambda s:True):return self.state(lambda s:s['view']=='space_map' and s['state']=='ready' and s['loaded'] and s['drained'] and s['watch_settled'] and predicate(s))
    def capture(self,label):
        self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
        def complete():
            if not self.raw.exists():return False
            parts=self.raw.read_bytes().split(b'\n',3)
            if len(parts)!=4:return False
            w,h=map(int,parts[1].split());return len(parts[3])==w*h*3
        self.wait(complete);path=OUT/(label+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(path)],check=True,timeout=15);report['captures'].append(str(path))
    def close(self):
        offset=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text('callback');pending.replace(self.raw.with_suffix('.command'))
        self.p.wait(timeout=25);assert self.p.returncode==0 and 'FILES_CLOSE_DRAINED'in self.log.read_text()[offset:]

def helpers(app):return set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=space_map',app.log.read_text()))
def row_names(s):return {r['raw']for r in s['row_hits']}
def shown(s):
    assert s['selected_raw']in s['navigation_raw']
    if s['selected_raw']in s['no_area_raw']:
        assert s['selected_raw']in row_names(s)
        x,y,w,h=s['list_rect']
        assert h>=26 and all(rx>=x and ry>=y and rx+rw<=x+w and ry+rh<=y+h for rx,ry,rw,rh in [r['rect']for r in s['row_hits']])
def go(app,index):
    app.act('key:home');s=app.ready()
    for _ in range(index):app.act('key:down');s=app.ready()
    return s
with tempfile.TemporaryDirectory(prefix='space-map-nav-ui-',dir=BASE)as tmp:
    base=Path(tmp);folder=base/'Positive';folder.mkdir()
    with (folder/'huge.bin').open('wb')as f:f.truncate(1<<40)
    for n in range(12):(folder/f'tiny-{n:02}.bin').write_bytes(b'x')
    raw=os.fsencode(folder)+b'/tiny-\xff.bin'
    with open(raw,'wb')as f:f.write(b'x')
    (folder/'.hidden-zero').touch()
    try:
        a=App('positive',folder,base);a.act('map');a.ready(lambda s:s['observed']==(1<<40)+13)
        a.act('fixture-minimum');s=a.ready(lambda s:s['width']<500)
        nav=s['navigation_raw'];assert len(nav)==14 and len(set(nav))==14 and len(s['no_area_raw'])==13
        assert s['max_scroll']>0 and s['page_rows']>=1 and len(s['row_hits'])>=1
        assert {c['action']for c in s['controls']}>= {'recalculate','list-up','list-down'}
        a.capture('space-map-positive-only-tiny-rows-440');ids=helpers(a)
        a.act('marks:clear');s=a.ready();assert not s['selection_raw'] and s['selected_raw'] is None
        a.act('marks-menu');s=a.ready();assert any(not i['enabled'] and 'Filtro no aplicado' in i['label'] for i in s['selection_menu'])
        a.capture('space-map-marks-menu-440')
        a.act('menu-all');s=a.ready();assert set(s['selection_raw'])==set(nav)
        a.capture('space-map-mark-all-projection')
        a.act('marks:invert');s=a.ready();assert not s['selection_raw'] and s['selected_raw'] is None
        a.act('palette:select-all');s=a.ready();assert set(s['selection_raw'])==set(nav)
        a.act('palette:clear');s=a.ready();assert not s['selection_raw'] and s['selected_raw'] is None
        a.act('key:home');a.ready(lambda s:s['selected_raw']==nav[0])
        a.act('insert');s=a.ready();assert s['selected_raw']==nav[1] and nav[0] not in s['selection_raw']
        a.capture('space-map-insert-toggle-advance')
        assert helpers(a)==ids

        assert len(ids)==1
        for i,name in enumerate(nav):
            a.act('key:home'if i==0 else'key:down');s=a.ready(lambda s:s['selected_raw']==name);shown(s)
            if b'\xff'.hex()in name:a.capture('space-map-raw-tiny-row-keyboard-visible')
        a.capture('space-map-end-row-revealed-440')
        a.act('key:home');a.ready(lambda s:s['selected_raw']==nav[0])
        a.act('key:shift-end');s=a.ready(lambda s:s['selected_raw']==nav[-1]);shown(s)
        assert set(s['selection_raw'])==set(nav);a.capture('space-map-shift-range-map-order')
        a.act('key:ctrl-home');s=a.ready(lambda s:s['selected_raw']==nav[0]);shown(s);assert set(s['selection_raw'])==set(nav)
        a.act('key:ctrl-down');s=a.ready(lambda s:s['selected_raw']==nav[1]);shown(s);assert set(s['selection_raw'])==set(nav)
        a.capture('space-map-control-focus-keeps-marks')
        a.act('key:home');a.ready(lambda s:s['selected_raw']==nav[0]);step=s['page_rows']
        a.act('key:page-down');s=a.ready(lambda s:s['selected_raw']==nav[min(step,len(nav)-1)]);shown(s)
        a.act('key:page-up');s=a.ready(lambda s:s['selected_raw']==nav[0]);shown(s)
        a.act('key:end');before=a.ready(lambda s:s['selected_raw']==nav[-1]);shown(before)
        a.act('list-up');s=a.ready();assert s['scroll']<before['scroll']and s['selected_raw']==before['selected_raw']and s['selection_raw']==before['selection_raw']
        a.act('wheel-down');after=a.ready();assert after['scroll']>=s['scroll']and after['scroll']<=after['max_scroll']
        a.act('list-down');s=a.ready();assert s['scroll']<=s['max_scroll'];a.capture('space-map-mouse-list-controls')
        name=next(n for n in row_names(s)if 'ff'not in n);a.act('select:'+bytes.fromhex(name).decode());s=a.ready(lambda s:s['selected_raw']==name);shown(s)
        a.act('fixture-wide');s=a.ready(lambda s:s['width']>500);shown(s);a.capture('space-map-resize-reveals-focused-tiny-row')
        a.act('hidden');s=a.ready(lambda s:b'.hidden-zero'.hex()in s['navigation_raw']);assert len(s['navigation_raw'])==15
        shown(go(a,s['navigation_raw'].index(b'.hidden-zero'.hex())));a.capture('space-map-hidden-zero-navigation')
        a.act('marks:all');s=a.ready();assert set(s['selection_raw'])==set(s['navigation_raw']) and b'.hidden-zero'.hex() in s['selection_raw']
        a.act('hidden');s=a.ready(lambda s:b'.hidden-zero'.hex()not in s['navigation_raw']);assert b'.hidden-zero'.hex()not in s['selection_raw']
        assert helpers(a)==ids and not s['changed']and not s['watch_gap']
        a.act('fixture-minimum');a.ready(lambda s:s['width']<500)
        a.act('bar-toggle');s=a.ready(lambda s:s['operation_bar']);shown(s)
        assert any(c['index']==3 for c in s['bar_rects']);a.capture('space-map-mc-bar-context-recalculate')
        assert set(s['navigation_raw'])<=set(s['no_area_raw']), 'all24px-high tiles need named rows with MC strip'
        before=s['revision'];a.act('f5');s=a.ready(lambda s:s['revision']!=before);assert s['observed']==(1<<40)+13
        before=s['revision'];a.act('bar-recalculate');s=a.ready(lambda s:s['revision']!=before)
        assert s['observed']==(1<<40)+13 and len(helpers(a))==len(ids)+2
        a.capture('space-map-mc-mouse-recalculate')
        a.act('fixture-wide');a.ready(lambda s:s['width']>500)
        a.act('palette-open:recalcular');s=a.ready(lambda s:s['palette'] is not None)
        item=next(i for i in s['palette']['matches'] if i['id']=='refresh')
        assert item['label']=='Recalcular Espacio y actualizar carpeta' and item['shortcut']=='F5 / Ctrl+R' and item['reason'] is None
        a.capture('space-map-palette-recalculate-context')
        a.act('palette-close');s=a.ready(lambda s:s['palette'] is None)
        before=s['revision'];a.act('palette:refresh');s=a.ready(lambda s:s['revision']!=before)
        assert s['observed']==(1<<40)+13 and len(helpers(a))==len(ids)+3
        a.close()
        zeros=base/'Zeros';zeros.mkdir()
        for n in range(18):(zeros/f'zero-{n:02}').touch()
        (zeros/'ZeroFolder').mkdir()
        b=App('zeros',zeros,base);b.act('map');b.ready(lambda s:s['total']==0 and s['nodes']==19)
        b.act('fixture-minimum');s=b.ready(lambda s:s['width']<500)
        assert not s['areas']and len(s['no_area_raw'])==19 and len(s['navigation_raw'])==19
        b.act('key:end');s=b.ready();shown(s);assert s['scroll']>0;b.capture('space-map-all-zero-end-revealed')
        s=go(b,s['navigation_raw'].index(b'ZeroFolder'.hex()));assert s['selected_raw']==b'ZeroFolder'.hex();shown(s)
        b.act('enter-selected');s=b.ready(lambda s:s['path']==str(zeros/'ZeroFolder')and s['nodes']==0)
        assert not s['navigation_raw']and not s['row_hits']and s['max_scroll']==0
        b.capture('space-map-zero-folder-keyboard-navigation');b.close()
        partial=base/'Partial';partial.mkdir();(partial/'known').write_bytes(b'known')
        restricted=partial/'restricted';restricted.mkdir();(restricted/'unread').write_bytes(b'not observed');restricted.chmod(0)
        try:
            c=App('partial',partial,base);c.act('map');s=c.ready(lambda s:s['complete'] is False)
            assert s['observed']==5 and len(s['navigation_raw'])==2 and len(s['no_area_raw'])==1
            ids=helpers(c);c.act('marks:all');s=c.ready();assert set(s['selection_raw'])==set(s['navigation_raw'])
            c.capture('space-map-partial-unknown-row-marked')
            c.act('entry-menu');s=c.ready();copy=next(i for i in s['selection_menu'] if 'Copiar selección' in i['label'])
            assert copy['enabled'] and 'F5' not in copy['label']
            c.capture('space-map-entry-menu-copy-review-caption')
            c.act('menu-close');c.ready(lambda s:s['selection_menu'] is None)
            c.act('copy-inspector');s=c.ready(lambda s:s['copy_dialog'] is not None)
            assert not s['copy_dialog']['moving'] and set(s['copy_dialog']['names_raw'])==set(s['navigation_raw'])
            c.capture('space-map-inspector-copy-opens-review')
            c.act('copy-close');s=c.ready(lambda s:s['copy_dialog'] is None)

            c.act('marks:invert');s=c.ready();assert not s['selection_raw'] and s['selected_raw'] is None
            assert helpers(c)==ids;c.close()
        finally:restricted.chmod(0o700)
        ops=base/'Operations';ops.mkdir();(ops/'out').mkdir();(ops/'sample.txt').write_bytes(b'owned payload');(ops/'other.txt').write_bytes(b'other')
        with zipfile.ZipFile(ops/'archive.zip','w')as archive:archive.writestr('inside.txt',b'owned archive payload')
        d=App('operations',ops,base);d.act('map');d.ready()
        d.act('select:sample.txt');d.ready(lambda s:s['selected_raw']==b'sample.txt'.hex())
        d.act('bar-toggle');s=d.ready(lambda s:s['operation_bar'])
        assert all(b['reason'] is None for b in s['mc']['buttons'] if b['index'] in [1,2,3,4,5,6])
        d.capture('space-map-mc-actions-enabled')
        d.act('palette:create-zip');s=d.ready(lambda s:s['copy_dialog'] is not None)
        assert s['copy_dialog']['creating'] and s['copy_dialog']['names_raw']==[b'sample.txt'.hex()]
        d.capture('space-map-palette-create-zip-review');d.act('copy-close');d.ready(lambda s:s['copy_dialog'] is None)
        d.act('select:archive.zip');d.ready();d.act('palette:extract');s=d.ready(lambda s:s['copy_dialog'] is not None)
        assert s['copy_dialog']['extracting'] and not s['copy_dialog']['creating'] and s['copy_dialog']['names_raw']==[b'archive.zip'.hex()]
        d.capture('space-map-palette-extract-review');d.act('copy-close');d.ready(lambda s:s['copy_dialog'] is None)
        d.act('select:sample.txt');d.ready()

        d.act('palette:properties');d.ready(lambda s:s['properties'] is not None and not s['properties']['pending'] and s['properties']['observed'])
        d.capture('space-map-palette-properties');d.act('properties-close');d.ready(lambda s:s['properties'] is None and not s['mutation_busy'])
        d.act('bar:2',lambda s:s['preview_visible']);d.state(lambda s:s['preview_ready'] and s['preview_drained'])
        d.capture('space-map-mc-existing-text-preview');d.act('preview-close');d.ready(lambda s:not s['preview_visible'])
        for command,kind in [('clipboard-copy','Copy'),('clipboard-cut','Cut')]:
            d.act('palette:'+command);d.ready(lambda s:s['clipboard_count']==1 and not s['clipboard_busy'] and kind in s['clipboard_kind'])
            d.act('palette:clipboard-paste');s=d.ready(lambda s:s['copy_dialog'] is not None)
            assert s['copy_dialog']['names_raw']==[b'sample.txt'.hex()] and s['copy_dialog']['moving']==(kind=='Cut')
            assert (ops/'sample.txt').read_bytes()==b'owned payload'
            d.capture('space-map-private-buffer-'+kind.lower()+'-review');d.act('copy-close');d.ready(lambda s:s['copy_dialog'] is None)
        d.act('palette:copy');d.ready(lambda s:s['copy_dialog'] is not None)
        d.act('copy-dest:out');d.ready();d.act('copy-start');d.wait(lambda:(ops/'out/sample.txt').exists())
        d.ready(lambda s:not s['copy_busy']);assert (ops/'out/sample.txt').read_bytes()==b'owned payload'
        d.act('notice-close');d.ready();d.act('select:sample.txt');d.ready()
        d.act('bar:1');d.ready(lambda s:s['rename_dialog'] is not None)
        d.act('rename-text:renamed.txt');d.ready(lambda s:s['rename_dialog'] is not None and s['rename_dialog']['prepared'])
        d.capture('space-map-mc-rename-reviewed');d.act('rename-apply');d.wait(lambda:(ops/'renamed.txt').exists() and not (ops/'sample.txt').exists())
        d.ready(lambda s:not s['mutation_applied'] and not s['mutation_busy']);d.act('notice-close');d.ready()
        d.act('bar:5');d.ready(lambda s:s['rename_dialog'] is not None and s['rename_dialog']['mode']=='folder')
        d.act('rename-text:Created');d.ready();d.act('rename-apply');d.wait(lambda:(ops/'Created').is_dir())
        d.ready(lambda s:not s['mutation_applied'] and not s['mutation_busy']);d.act('notice-close');d.ready()
        d.act('select:renamed.txt');d.ready();d.act('bar:4');d.ready(lambda s:s['copy_dialog'] is not None and s['copy_dialog']['moving'])
        d.act('copy-dest:out');d.ready();d.act('copy-start');d.ready(lambda s:s['copy_dialog'] is not None and s['copy_dialog']['reviewed'])
        d.capture('space-map-mc-move-reviewed');d.act('copy-start');d.wait(lambda:(ops/'out/renamed.txt').exists() and not (ops/'renamed.txt').exists())
        d.ready(lambda s:not s['copy_busy']);assert (ops/'out/renamed.txt').read_bytes()==b'owned payload'
        d.act('notice-close');d.ready();d.act('select:other.txt');d.ready();d.act('bar:6');d.wait(lambda:not (ops/'other.txt').exists())
        d.ready(lambda s:not s['mutation_applied'] and not s['mutation_busy'] and s['can_undo']);d.act('notice-close');d.ready()
        d.act('palette:undo');d.wait(lambda:(ops/'other.txt').exists());d.ready(lambda s:not s['mutation_applied'] and not s['mutation_busy'])
        assert (ops/'other.txt').read_bytes()==b'other';d.act('notice-close');d.ready();d.capture('space-map-owned-operations-trash-undo-restored');d.close()
        identities=set().union(*(helpers(app)for app in apps))
        for pid,start in identities:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'known helper identity survived: '+pid
        all_helpers=set().union(*(set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=\w+',app.log.read_text()))for app in apps))
        for pid,start in all_helpers:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'known helper identity survived: '+pid
        assert (folder/'huge.bin').stat().st_size==1<<40 and Path(os.fsdecode(raw)).read_bytes()==b'x'
        report.update(functional_gate='PASS',normal_exits=4,logged_space_helpers=sorted(identities),all_logged_helpers=sorted(all_helpers),scope='owned apparent-size fixture, no throughput/resource experiment; map bulk marks/invert/explicit empty/palette/visible menu/Insert/raw names/hidden projection without rescans; MC and palette owned copy/rename/create/move/trash-undo/properties/existing text preview/private buffer CopyCut review; inspector Copy opens captured-name review, entry-menu copy has no false F5; actual map layout/paint/shared key-wheel handlers/visible mouse controls: positive-only tiny blocks,rawnames,HomeEnd/Pages/CtrlShift/ranges,overflow buttons,wheel,resize,hidden projection,allzero and folder navigation; no extra helper for navigation/projection; explicit F5/MC scans verified, private handlers not OS input/global lifetime')
    except BaseException as e:
        report.update(functional_gate='FAIL',error=repr(e))
        for app in apps:
            if app.p.poll() is None:
                try:app.capture('failure-'+app.label)
                except BaseException as capture_error:report.setdefault('capture_errors',[]).append(repr(capture_error))
        raise
    finally:
        for app in apps:
            report['logs'][app.label]=app.log.read_text()
            if app.p.poll()is None:
                os.killpg(app.p.pid,signal.SIGTERM)
                try:app.p.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.p.pid,signal.SIGKILL);app.p.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:
            file=(Path.home()/'.local/bin'/name)if args.installed else ROOT/'target/release'/name;report[key]=hashlib.sha256(file.read_bytes()).hexdigest()
        path=OUT/('space-map-nav-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
