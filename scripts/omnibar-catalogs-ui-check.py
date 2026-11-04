#!/usr/bin/env python3
"""Owned omnibar catalog evidence; production handlers/framebuffer, no OS input or resource sampling."""
import fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/omnibar-catalogs';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
apps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1'}
        with self.log.open('w') as output:
            self.process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size',size,'--scale','1','--exit-after','180'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        apps.append(self);report['processes'].append(dict(label=label,pid=self.process.pid,root_raw=list(os.fsencode(folder))))
        self.state(predicate=lambda s:s['bookmarks']['browser_loaded']);self.act('list')
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
            for line in self.log.read_text()[after:].splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    state=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    painted=True
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



with tempfile.TemporaryDirectory(prefix='omnibar-catalogs-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir();target=base/'Destino privado';target.mkdir()
    (source/'keep.txt').write_bytes(b'original fixture')
    (source/'>literal').mkdir();(source/':literal').mkdir()
    before=sha(source/'keep.txt')
    try:
        a=App('A',source,base,config,target)
        def prefix(text,mouse=False):
            a.act('path-open-hit');a.act('path-text:'+text)
            return a.act('path-accept-hit' if mouse else 'path-key:plain:enter')
        selected=a.state()['selection'];path=str(source)
        s=prefix('> grid');assert s['commands']['query']=='grid' and s['path']==path and s['selection']==selected
        assert s['view']=='list';a.capture('omnibar-command-filter-no-execution')
        a.act('commands-row:view-grid');s=a.act('commands-run-hit');assert s['commands'] is None and s['view']=='grid'
        # Query limit rejects before closing editor; draft and root survive.
        text='>'+('x'*257);s=prefix(text);assert s['editing'] and s['edit']==text and s['path']==path and '256' in s['error']
        a.capture('omnibar-catalog-limit-draft-retained');a.act('path-cancel-hit')
        # Existing place catalog receives the suffix, then navigation is explicit.
        a.act('book-fixture-places');s=prefix(': Destino',True)
        assert s['bookmarks']['panel']['query']=='Destino' and s['path']==path
        a.capture('omnibar-place-filter-no-navigation')
        a.act('book-key:down');s=a.act('book-jump',lambda s:bytes(s['path_raw'])==os.fsencode(target) and s['bookmarks']['panel'] is None)
        a.capture('omnibar-place-explicit-navigation')
        # Prefix-like real directory names stay available through ./.
        a.act('path-open-hit');a.act('path-text:'+str(source));a.act('path-key:plain:enter',lambda s:bytes(s['path_raw'])==os.fsencode(source) and s['bookmarks']['browser_loaded'])
        a.act('path-open-hit');a.act('path-text:./>literal');s=a.act('path-key:plain:enter',lambda s:Path(os.fsdecode(bytes(s['path_raw'])))==source/'>literal')
        assert s['commands'] is None
        a.act('back',lambda s:bytes(s['path_raw'])==os.fsencode(source))
        a.act('path-open-hit');a.act('path-text:./:literal');s=a.act('path-key:plain:enter',lambda s:Path(os.fsdecode(bytes(s['path_raw'])))==source/':literal')
        assert s['bookmarks']['panel'] is None
        a.act('back',lambda s:bytes(s['path_raw'])==os.fsencode(source))
        a.act('fixture-minimum',lambda s:s['width']==440)
        s=prefix('> rename');assert s['commands']['query']=='rename';a.capture('omnibar-catalog-440')
        a.act('commands-key:escape');a.close();assert sha(source/'keep.txt')==before
        report.update(functional_gate='PASS',normal_exits=1,scope='CtrlL prefix >grid opens filtered catalog without execution, explicit grid command; >257 limit preserves draft/root; :Destino opens filtered existing places without navigation, explicit selection/jump; ./ prefix names literal; 440 catalog; source hash/root0ACK; handlers/framebuffer not native input/MIME/resource/interop proof')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll()is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
        path=OUT/('omnibar-catalogs-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
