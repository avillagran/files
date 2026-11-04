#!/usr/bin/env python3
"""Owned copy conflict policy evidence; production handlers/framebuffer, no OS input or resource sampling."""
import fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/conflicts';OUT.mkdir(parents=True,exist_ok=True)
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


with tempfile.TemporaryDirectory(prefix='conflicts-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir()
    stop=base/'Default';stop.mkdir();target=base/'Skip';target.mkdir()
    names=[b'a.txt',b'note.txt',b'raw-\xff.txt',b'z.txt']
    for n in names:
        with open(os.fsencode(source)+b'/'+n,'wb')as out:out.write(b'source:'+n)
    original={n:sha(source/os.fsdecode(n))for n in names}
    (stop/'note.txt').write_bytes(b'unchanged default occupant')
    actor=base/'Actor';actor.write_bytes(b'outside actor');(target/'note.txt').symlink_to(actor)
    (target/'z.txt').mkdir();(target/'z.txt/keep').write_bytes(b'existing directory')
    def finished(job,ok):return lambda s:s['destination_queue_idle']and s['copy_result']is not None and s['copy_result']['id']==job and s['copy_result']['ok']==ok
    try:
        a=App('A',source,base,config,target)
        s=a.act('dest-copy-all');assert not s['destination_editor']['skip_existing']
        a.act('dest-text:'+str(stop));a.capture('copy-stop-default-review')
        s=a.act('dest-start',finished(1,False));assert (s['copy_result']['completed'],s['copy_result']['skipped'])==(1,0)
        assert (stop/'note.txt').read_bytes()==b'unchanged default occupant'and not (stop/'z.txt').exists()
        a.capture('copy-stop-default-conflict');a.act('dest-dismiss')
        a.act('dest-copy-all');a.act('dest-text:'+str(target));s=a.act('dest-key:skip');assert s['destination_editor']['skip_existing']
        a.capture('copy-skip-explicit-review')
        s=a.act('dest-start',finished(2,True));assert (s['copy_result']['completed'],s['copy_result']['skipped'])==(2,2)
        assert len(s['copy_result']['skipped_paths'])==2
        assert (target/'note.txt').is_symlink()and actor.read_bytes()==b'outside actor'and (target/'z.txt/keep').read_bytes()==b'existing directory'
        for n in [names[0],names[2]]:assert sha(target/os.fsdecode(n))==original[n]
        a.capture('copy-skip-completed-summary');a.act('dest-details');a.capture('copy-skip-root-details');a.act('dest-dismiss')
        a.act('fixture-minimum',lambda s:s['width']==440)
        s=a.act('dest-copy-all');assert not s['destination_editor']['skip_existing']
        a.act('dest-text:'+str(target));s=a.act('dest-skip');assert s['destination_editor']['skip_existing'];a.capture('copy-skip-mouse-440')
        s=a.act('dest-start',finished(3,True));assert (s['copy_result']['completed'],s['copy_result']['skipped'])==(0,4)
        a.capture('copy-all-skipped-summary-440');a.act('dest-details');a.capture('copy-all-skipped-details-440');a.act('dest-dismiss')
        a.act('dest-move');s=a.act('dest-key:skip');assert not s['destination_editor']['skip_existing'];a.act('dest-key:escape')
        a.close()
        assert all(sha(source/os.fsdecode(n))==original[n]for n in names)
        assert not any(p.name.startswith('.files-copy-')for folder in [stop,target]for p in folder.iterdir())
        report.update(functional_gate='PASS',normal_exits=1,scope='default Stop publishes first then fails at collision, explicit keyboard Skip publishes2/skips2 with raw filename and symlink/directory occupant preserved, next dialog defaults Stop, mouse Skip at440 all4 skipped/0published, observed skipped details/counters, Move F4 refuses policy, source bytes intact/root0drainACK; production handlers/framebuffer, no OS input/publication race/lifetimes/resources')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll() is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
        path=OUT/('copy-conflicts-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
