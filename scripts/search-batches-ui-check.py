#!/usr/bin/env python3
"""Owned incremental search evidence; production handlers/framebuffer, no OS input or resource sampling."""
import fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/search-batches';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
apps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_SEARCH_HOLD_FIRST_BATCH':'1'}
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



with tempfile.TemporaryDirectory(prefix='search-batches-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir();target=base/'Target';target.mkdir()
    for i in range(40):(source/f'f{i:03}.txt').write_bytes(b'plain file')
    raw=b'needle-\xff.raw'
    with open(os.fsencode(source)+b'/'+raw,'wb') as f:f.write(b'unique needle text')
    before={os.fsencode(p.name):sha(p)for p in source.iterdir()}
    gate=source/'.files-search-gate'
    def provisional(s):return s['search']['provisional'] is not None and s['search']['running'] and s['search']['report'] is None and s['search']['row_hits']>0
    def final(s):return not s['search']['running'] and s['search']['report'] is not None
    def live(app):
        identities=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_batches',app.log.read_text());assert identities
        pid,start=identities[-1];stat=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split();assert stat[19]==start and stat[0]!='Z'
        return [pid,start]
    try:
        a=App('A',source,base,config,target);a.act('recursive-open');gate.touch()
        s=a.act('recursive-query:ext:txt',provisional);report['live_before_terminal']=live(a);assert s['search']['report'] is None
        a.capture('search-provisional-real-helper-held');live(a)
        selected=bytes(s['search']['provisional'][s['search']['selected']]);gate.unlink()
        s=a.state(predicate=final);assert len(s['search']['report']['hits'])==40 and s['search']['report']['complete'] and s['search']['provisional'] is None
        chosen=s['search']['report']['hits'][s['search']['selected']]['relative'];assert os.fsencode(chosen)==selected
        a.capture('search-final-exact-reconciled')
        gate.touch();a.act('recursive-query:ext:txt',provisional)
        s=a.act('recursive-cancel',lambda s:s['search']['provisional'] is None and not s['search']['running']);assert s['search']['report'] is None
        a.capture('search-cancel-clears-candidates')
        gate.touch();a.act('recursive-query:ext:txt',provisional)
        source.rename(base/'Preserved');source.mkdir();(source/'actor.txt').write_bytes(b'actor unchanged')
        s=a.state(predicate=lambda s:bool(s['search']['error'])and not s['search']['running'])
        assert s['search']['provisional'] is None and s['search']['report'] is None
        a.capture('search-root-replacement-invalidates-candidates')
        assert (source/'actor.txt').read_bytes()==b'actor unchanged'
        (source/'actor.txt').unlink();source.rmdir();(base/'Preserved').rename(source);gate.unlink(missing_ok=True)
        gate.touch();a.act('recursive-query:ext:txt',provisional)
        s=a.act('recursive-query:ext:no-match',final);assert not s['search']['report']['hits'] and s['search']['provisional'] is None
        a.capture('search-new-query-discards-old-prefix')
        # Content/raw wire through actual decoder; raw result can be explicitly located while provisional.
        gate.touch();s=a.act('recursive-query:ext:raw content:"unique needle"',provisional)
        assert bytes(s['search']['provisional'][0])==raw
        snippet=s['search']['provisional_snippets'][0];assert snippet['text'].encode()[snippet['start']:snippet['end']]==b'unique needle'
        a.act('fixture-minimum',lambda s:s['width']==440);a.capture('search-content-raw-provisional-440')
        s=a.act('recursive-locate',lambda s:not s['search']['visible']and not s['search']['running']and s['selected']==raw.hex()and s['selected']in s['visible'])
        a.capture('search-provisional-locate-revalidated-list')
        gate.unlink(missing_ok=True)
        assert all(sha(source/os.fsdecode(n))==digest for n,digest in before.items())
        a.close();text=a.log.read_text();identities=set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=search_batches',text))
        for pid,start in identities:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'helper identity remains including Z'
        report.update(functional_gate='PASS',normal_exits=1,logged_helpers=[list(v)for v in sorted(identities)],scope='real scanner/helper wire held up to5s by fixture gate after first batch; provisional rows before terminal while PID/start live, final40 exact/selection preserved, cancel clears, rootreplace terminalerror removes candidates, newerquery discards old prefix, raw content440 and provisional locate through normal listing, source hashes/root0ACK/strict logged helper cleanup. No natural latency/kernelD/native input/MIME/lifetime/resource/startup proof')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll()is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
        path=OUT/('search-batches-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
