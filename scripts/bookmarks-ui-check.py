#!/usr/bin/env python3
"""Owned two-window favourites evidence; production handlers/framebuffer, no OS input or resource sampling."""
import argparse,fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BIN_DIR=Path.home()/'.local/bin'if args.installed else ROOT/'target/release'
OUT=ROOT/'docs/evidence/bookmarks';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
apps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1'}
        with self.log.open('w') as output:
            self.process=subprocess.Popen([str(BIN_DIR/'files'),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size',size,'--scale','1','--exit-after','180'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        apps.append(self);report['processes'].append(dict(label=label,pid=self.process.pid,root_raw=list(os.fsencode(folder))))
        self.state(predicate=lambda s:s['bookmarks']['browser_loaded']);self.act('list');self.act('book-fixture-places')
    def wait(self,check,why,exits=False):
        end=time.monotonic()+25
        while time.monotonic()<end:
            value=check()
            if value:return value
            if self.process.poll() is not None and not exits:raise AssertionError(self.label+': '+why+' exit '+str(self.process.returncode))
            time.sleep(.02)
        raise AssertionError(self.label+': '+why)
    def state(self,after=0,predicate=lambda s:True):
        def current():
            states=[]
            for line in self.log.read_text()[after:].splitlines(keepends=True):
                if not line.endswith('\n'):continue
                if line.startswith('FILES_HIDDEN_STATE '):
                    state=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    panel=state['bookmarks']['panel']
                    painted=panel is None or all(b['stamp']==panel['stamp'] for b in state['bookmarks']['buttons'])
                    if state['seq']==self.seq and painted and predicate(state):states.append(state)
            return states[-1] if states else None
        s=self.wait(current,'painted state absent seq '+str(self.seq));report['states'].append(dict(app=self.label,state=s));return s
    def command(self,action,exits=False):
        offset=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(action);pending.replace(self.raw.with_suffix('.command'))
        match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',self.log.read_text()[offset:],re.M),'command absent '+action,exits)
        self.seq=int(match[1]);return offset
    def act(self,action,predicate=lambda s:True):return self.state(self.command(action),predicate)
    def capture(self,name):
        self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
        def full():
            if not self.raw.exists():return None
            parts=self.raw.read_bytes().split(b'\n',3)
            if len(parts)!=4:return None
            w,h=map(int,parts[1].split());return (w,h)if len(parts[3])==w*h*3 else None
        dims=self.wait(full,'incomplete framebuffer');path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(path)],check=True,timeout=15);report['captures'].append(dict(app=self.label,path=str(path),dimensions=dims))
    def close(self):
        self.command('callback',True);self.wait(lambda:self.process.poll() is not None,'normal close absent',True);assert self.process.returncode==0 and 'FILES_CLOSE_DRAINED' in self.log.read_text()
def done(s):return not s['bookmarks']['busy']
def loaded(s):return done(s)and s['bookmarks']['loaded']and not s['bookmarks']['error']
def rows(s):return s['bookmarks']['rows']
with tempfile.TemporaryDirectory(prefix='bookmarks-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();(config/'config.toml').write_text('show_hidden=false\nfixture_note="preserve"\n')
    aroot=base/os.fsdecode(b'A-\xff');broot=base/'B';place=base/'Destino'
    for p in [aroot,broot,place]:p.mkdir();(p/'note.txt').write_bytes(b'owned directory contents')
    before={os.fsencode(p):sha(p/'note.txt') for p in [aroot,broot,place]}
    try:
        a=App('A',aroot,base,config,place);b=App('B',broot,base,config,place)
        assert not rows(a.act('book-open',loaded));assert not rows(b.act('book-open',loaded))
        sa=a.act('book-add',lambda s:loaded(s)and len(rows(s))==1);assert bytes(rows(sa)[0]['path'])==os.fsencode(aroot)
        a.capture('favourite-raw-path-added')
        sb=b.act('book-add',lambda s:loaded(s)and len(rows(s))==2);assert {bytes(r['path'])for r in rows(sb)}=={os.fsencode(aroot),os.fsencode(broot)}
        a.act('book-reload',lambda s:loaded(s)and len(rows(s))==2);a.act('book-row:1');a.act('book-edit');sa=a.act('book-text:Taller α');assert sa['dialog'] is None and sa['bookmarks']['panel']['editing']
        a.capture('favourite-label-editor')
        sa=a.act('book-save',lambda s:loaded(s)and rows(s)[0]['label']=='Taller α');assert rows(sa)[0]['id']==1;a.capture('favourite-label-updated')
        b.act('book-row:1');sb=b.act('book-remove',lambda s:done(s)and s['bookmarks']['unresolved']);assert 'cambió' in sb['bookmarks']['error'] and len(rows(sb))==2
        b.capture('favourite-stale-label-rejected')
        b.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
        b.command('callback',True);b.wait(lambda:'FILES_CLOSE_WAIT saves_ready=false' in b.log.read_text(),'failed write did not block close');assert b.process.poll() is None;b.capture('favourite-save-error-close-440')
        b.act('book-cancel-close',lambda s:not s['bookmarks']['closing']);b.act('book-open');b.act('book-omit');b.act('book-reload',loaded)
        b.act('book-row:1');sb=b.act('book-remove',lambda s:loaded(s)and len(rows(s))==1);assert aroot.is_dir()and (aroot/'note.txt').read_bytes()==b'owned directory contents'
        sa=a.act('book-add',lambda s:loaded(s)and len(rows(s))==2 and any(r['id']==3 for r in rows(s)));assert {r['id']for r in rows(sa)}=={2,3}
        old=b.state()['bookmarks']['panel']['stamp'];b.act('book-reload',loaded);digest=sha(config/'bookmarks.bin');sb=b.act('book-stale-remove:'+str(old));assert not sb['bookmarks']['busy'] and sha(config/'bookmarks.bin')==digest
        b.capture('favourite-remove-does-not-delete-folder')
        # A short lock held by another process fails nonblocking; retry retains A
        # even after the UI navigates to another directory.
        with (config/'bookmarks.lock').open('a+b') as guard:
            fcntl.flock(guard,fcntl.LOCK_EX|fcntl.LOCK_NB)
            a.act('book-add',lambda s:done(s)and s['bookmarks']['unresolved']);a.capture('favourite-writer-busy')
            a.act('book-key:ctrlf');a.act('book-text:Destino');a.act('book-jump',lambda s:s['bookmarks']['panel'] is None and bytes(s['path_raw'])==os.fsencode(place)and s['bookmarks']['browser_loaded'])
            a.act('book-open');fcntl.flock(guard,fcntl.LOCK_UN)
        sa=a.act('book-reload',loaded);assert {bytes(r['path'])for r in rows(sa)}=={os.fsencode(aroot),os.fsencode(broot)};assert sha(config/'bookmarks.bin')==digest
        a.capture('favourite-retry-keeps-original-folder')
        # A missing/non-directory place is a browser error, not an open launcher.
        a.act('book-close');place.rename(base/'Destino-preserved');place.write_bytes(b'actor file')
        a.act('book-open',loaded);a.act('book-key:ctrlf');a.act('book-text:Destino');a.act('book-jump',lambda s:s['bookmarks']['panel'] is None and bool(s['error']));a.capture('favourite-target-is-file-load-error')
        a.act('book-open',loaded);a.act('book-row:3');a.act('book-jump',lambda s:bytes(s['path_raw'])==os.fsencode(aroot)and not s['error']and s['bookmarks']['browser_loaded'])
        a.close();b.close()
        c=App('C',aroot,base,config,base/'Destino-preserved',size='800x440');sc=c.act('book-open',loaded);assert {r['id']for r in rows(sc)}=={2,3};assert bytes(next(r for r in rows(sc)if r['id']==3)['path'])==os.fsencode(aroot);c.capture('favourites-restored-new-process');c.close()
        packet=(config/'bookmarks.bin').read_bytes();future=bytearray(packet);future[8:10]=(2).to_bytes(2,'little');(config/'bookmarks.bin').write_bytes(future)
        d=App('D',aroot,base,config,base/'Destino-preserved',size='440x440');sd=d.act('book-open',lambda s:done(s)and bool(s['bookmarks']['error']));assert not sd['bookmarks']['loaded'];d.act('book-details');d.capture('favourite-future-store-preserved');d.close();assert (config/'bookmarks.bin').read_bytes()==future
        assert sha(aroot/'note.txt')==before[os.fsencode(aroot)]and sha(broot/'note.txt')==before[os.fsencode(broot)]and sha(base/'Destino-preserved/note.txt')==before[os.fsencode(place)]and place.read_bytes()==b'actor file'
        report.update(functional_gate='PASS',normal_exits=4,scope='4 owned processes, stale snapshot additions merge, raw path retained, label edit not directory rename, stale label remove rejects, close blocked until explicit omit/reload at440, fresh remove leaves folder, ID never reused, old panel stamp rejects, short lock busy then retry retains original root after navigation, non-directory error recovery, new-process restore, future format preserved; handlers/framebuffer, no OS input/powerloss/lifetimes/resources')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll() is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(BIN_DIR/name)
        path=OUT/('bookmarks-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
