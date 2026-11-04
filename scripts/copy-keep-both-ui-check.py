#!/usr/bin/env python3
"""Owned copy conflict policy evidence; production handlers/framebuffer, no OS input or resource sampling."""
import fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/copy-keep-both';OUT.mkdir(parents=True,exist_ok=True)
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


with tempfile.TemporaryDirectory(prefix='keep-both-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir();target=base/'Target';target.mkdir()
    (source/'a.txt').write_bytes(b'original a');(source/'note.txt').write_bytes(b'note')
    (source/'archive.tar.gz').write_bytes(b'archive bytes')
    raw=b'raw-\xff.txt';(source/os.fsdecode(raw)).write_bytes(b'raw bytes')
    (source/'dir.txt').mkdir();(source/'dir.txt/child').write_bytes(b'child bytes')
    (source/'link.txt').symlink_to('a.txt')
    for name in ['a.txt','a (2).txt','note.txt','archive.tar.gz','dir.txt',os.fsdecode(raw)]:
        (target/name).write_bytes(b'occupant:'+os.fsencode(name))
    actor=base/'actor';actor.write_bytes(b'outside owned actor');(target/'link.txt').symlink_to(actor)
    def snapshot(folder):
        return {os.fsencode(p.relative_to(folder)):(p.lstat().st_ino,os.readlink(p) if p.is_symlink() else sha(p) if p.is_file() else 'directory') for p in folder.rglob('*')}
    before_source=snapshot(source);before_target=snapshot(target)
    def finished(job):return lambda s:s['destination_queue_idle']and s['copy_result']is not None and s['copy_result']['id']==job and s['copy_result']['ok']
    try:
        a=App('keep-both',source,base,config,target)
        s=a.act('dest-copy-all');assert s['destination_editor']['conflict_policy']=='Stop'
        a.act('dest-text:'+str(target));s=a.act('dest-key:skip');assert s['destination_editor']['conflict_policy']=='Skip'
        s=a.act('dest-key:skip');assert s['destination_editor']['conflict_policy']=='KeepBoth'
        a.capture('keep-both-review');assert snapshot(source)==before_source and snapshot(target)==before_target
        a.act('dest-start-paused');a.act('dest-start');a.capture('keep-both-paused')
        assert snapshot(source)==before_source and snapshot(target)==before_target
        s=a.act('dest-resume',finished(1));r=s['copy_result'];assert (r['completed'],r['renamed'],r['skipped'])==(6,6,0)
        expected=[b'a (3).txt',b'note (2).txt',b'archive (2).tar.gz',b'dir.txt (2)',b'link (2).txt',b'raw-\xff (2).txt']
        assert set(map(bytes,r['completed_paths']))=={os.fsencode(target)+b'/'+n for n in expected}
        assert (target/'a (3).txt').read_bytes()==b'original a'
        assert (target/'archive (2).tar.gz').read_bytes()==b'archive bytes'
        assert (target/'dir.txt (2)/child').read_bytes()==b'child bytes'
        assert (target/os.fsdecode(expected[-1])).read_bytes()==b'raw bytes'
        assert os.readlink(target/'link (2).txt')=='a.txt'
        assert all(snapshot(target)[k]==v for k,v in before_target.items())
        assert snapshot(source)==before_source and actor.read_bytes()==b'outside owned actor'
        a.capture('keep-both-result');a.act('dest-details');a.capture('keep-both-actual-paths');a.act('dest-dismiss')
        a.act('fixture-minimum',lambda s:s['width']==440)
        s=a.act('dest-copy-a');assert s['destination_editor']['conflict_policy']=='Stop'
        a.act('dest-text:'+str(source));a.act('dest-skip');s=a.act('dest-skip');assert s['destination_editor']['conflict_policy']=='KeepBoth'
        a.capture('keep-both-same-folder-review-440');s=a.act('dest-start',finished(2));assert s['copy_result']['renamed']==1
        assert (source/'a (2).txt').read_bytes()==b'original a';a.capture('keep-both-same-folder-result-440');a.act('dest-details');a.capture('keep-both-same-folder-details-440');a.act('dest-dismiss')
        a.act('dest-move');s=a.act('dest-key:skip');assert s['destination_editor']['conflict_policy']=='Stop';a.act('dest-key:escape')
        a.close()
        assert not any(p.name.startswith('.files-copy-')for folder in [source,target]for p in folder.iterdir())
        report.update(functional_gate='PASS',normal_exits=1,scope='Copy-only Stop/Skip/KeepBoth cycle via keyboard and painted mouse hits; review and paused queue unchanged; 6 roots published with numbered actual paths including raw bytes, compound archive, dotted directory, literal symlink; occupants/source preserved; same-folder copy at440; fresh dialog Stop and Move refuses; root0/drainACK. Private handlers/framebuffer, no native input/protocol/resource/lifetimes/hostile-writer proof.')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll() is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
        path=OUT/('copy-keep-both-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
