#!/usr/bin/env python3
"""Owned destination editor evidence; production handlers/framebuffer, no OS input or resource sampling."""
import fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/destination';OUT.mkdir(parents=True,exist_ok=True)
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

with tempfile.TemporaryDirectory(prefix='destination-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir()
    target=base/'Destino ñ';target.mkdir();other=base/'Otro';other.mkdir()
    (source/'note.txt').write_bytes(b'owned source bytes')
    original=sha(source/'note.txt')
    try:
        a=App('A',source,base,config,target)
        a.act('dest-copy');s=a.act('dest-text:'+str(other));assert s['destination_editor']['text']==str(other)
        a.act('dest-key:home');a.act('dest-key:end');a.act('dest-key:shift-home')
        s=a.act('dest-text:'+str(target));assert s['destination_editor']['text']==str(target)
        a.capture('copy-destination-unicode-selected')
        a.act('dest-key:undo');s=a.act('dest-key:redo');assert s['destination_editor']['text']==str(target)
        # A previous caret receipt cannot edit at a new caret.
        a.act('dest-request-paste');a.act('dest-key:left');s=a.act('dest-late-paste');assert s['destination_editor']['text']==str(target)
        # Mouse click/drag through shared production geometry; insertion at the end.
        a.act('dest-point-end');s=a.act('dest-text:/');assert s['destination_editor']['text']==str(target)+'/'
        a.capture('copy-destination-caret-and-viewport')
        # Preflight occurs before any input/history mutation; injected 1MiB text isn't stored.
        s=a.act('dest-oversize');assert s['destination_editor']['text']==str(target)+'/' and s['destination_editor']['error']
        a.capture('copy-destination-oversize-retained')
        a.act('dest-key:undo');s=a.act('dest-key:redo');assert s['destination_editor']['text']==str(target)+'/'
        a.act('dest-start',lambda s:s['destination_editor'] is None and s['destination_queue_idle'])
        a.wait(lambda:(target/'note.txt').exists(),'copy result absent');assert (target/'note.txt').read_bytes()==b'owned source bytes'
        a.capture('copy-edited-destination-result')
        a.act('dest-move');a.act('dest-text:'+str(other));s=a.act('dest-start',lambda s:s['destination_editor']is not None and s['destination_editor']['prepared'])
        a.capture('move-destination-review')
        a.act('dest-key:left');assert a.act('dest-key:right')['destination_editor']['prepared']
        s=a.act('dest-text:/');assert not s['destination_editor']['prepared']
        a.capture('move-edit-invalidates-reviewed-plan')
        a.act('dest-key:escape')
        # Captured opaque destination uses exact bytes; presentation can't become a path.
        a.act('dest-copy');a.act('dest-raw');a.act('dest-key:end');s=a.act('dest-text:bad');assert s['destination_editor']['captured'] and s['destination_editor']['error']
        a.capture('copy-opaque-destination-protected')
        a.act('dest-key:all');a.act('dest-text:'+str(other));s=a.act('dest-key:undo');assert s['destination_editor']['text']==str(other)and not s['destination_editor']['captured']
        a.act('fixture-minimum',lambda s:s['width']==440);a.capture('copy-destination-initial-440')
        a.act('dest-key:all');s=a.act('dest-limit');assert len(s['destination_editor']['text'])==4096 and s['destination_editor']['caret']==4096
        a.capture('copy-destination-4096-viewport-440')
        a.act('dest-key:escape');a.close()
        assert sha(source/'note.txt')==original and not list(other.iterdir())
        report.update(functional_gate='PASS',normal_exits=1,scope='owned production destination editor/handlers and real framebuffer; Unicode selected replacement/undo-redo/caret-stale paste/pointer end/1MiB preflight/copy bytes to edited destination/move review caret retained then edit invalidates/raw partial rejects/full replacement undo boundary/440/cancel/exit0ACK; no native input/clipboard protocol/resources')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll() is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
        path=OUT/('destination-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
