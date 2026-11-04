#!/usr/bin/env python3
"""Owned external changes -> actual Model/watch/paint; no OS input/resource claims."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/listing-model/space-map';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','states':[],'captures':[],'logs':{}}
apps=[]
class App:
    def __init__(self,label,folder,base):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        config=base/(label+'-config');config.mkdir()
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_SPACE_MAP':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
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
def area_names(s):return {a['name']for a in s['areas']}
with tempfile.TemporaryDirectory(prefix='space-map-watch-ui-',dir=BASE)as tmp:
    base=Path(tmp);folder=base/'Files';folder.mkdir();photos=folder/'Fotos';photos.mkdir()
    (photos/'nested.bin').write_bytes(b'n'*16);(folder/'base.bin').write_bytes(b'b'*8)
    try:
        a=App('map',folder,base);a.act('map');first=a.ready(lambda s:s['observed']==24)
        assert not first['changed']and not first['watch_gap'];a.capture('space-map-observation-and-scope')
        a.act('select-photos');before=a.ready(lambda s:s['selected']=='Fotos');ids=helpers(a)
        (folder/'extra.bin').write_bytes(b'e'*12)
        s=a.ready(lambda s:s['changed']);assert s['observed']==24 and s['revision']==before['revision']and s['generation']==before['generation']
        assert s['selected']=='Fotos'and helpers(a)==ids;a.capture('space-map-root-change-retains-observation-no-rescan')
        a.act('refresh');s=a.ready(lambda s:s['observed']==36 and not s['changed'])
        assert s['revision']!=before['revision']and s['generation']!=before['generation']
        assert 'extra.bin'in area_names(s);a.act('select:extra.bin');a.ready(lambda s:s['selected']=='extra.bin')
        assert len(helpers(a))==len(ids)+1;a.capture('space-map-recalculate-renews-listing-and-hits')
        before=a.ready();ids=helpers(a);(photos/'nested.bin').write_bytes(b'n'*26)
        a.act('state');s=a.ready();assert s['observed']==36 and s['revision']==before['revision']and helpers(a)==ids
        # This observation is deliberately not a proof of recursive monitoring.
        a.act('f5');s=a.ready(lambda s:s['observed']==46 and not s['changed']);a.capture('space-map-nested-change-explicit-f5')
        (folder/'extra.bin').rename(folder/'renamed.bin');(folder/'base.bin').unlink()
        a.ready(lambda s:s['changed']);a.act('refresh');s=a.ready(lambda s:s['observed']==38 and not s['changed'])
        assert area_names(s)=={'Fotos','renamed.bin'};a.act('select:renamed.bin');a.ready(lambda s:s['selected']=='renamed.bin')
        a.act('fixture-minimum');s=a.ready(lambda s:s['width']<500)
        controls=s['controls'];assert any(c['action']=='recalculate'and c['rect'][2]>=100 for c in controls)
        assert all(x>=0 and y>=0 and w>0 and h>0 and x+w<=440 and y+h<=440 for x,y,w,h in [c['rect']for c in controls])
        a.capture('space-map-scope-and-recalculate-fit-440')
        old=base/'Moved';folder.rename(old)
        a.state(lambda s:s['state']=='ready'and s['watch_gap']);before=a.state(lambda s:s['state']=='ready'and s['watch_gap']);ids=helpers(a)
        assert before['observed']==38;a.capture('space-map-watch-gap-retains-report')
        folder.mkdir();(folder/'replacement.bin').write_bytes(b'r'*5)
        a.ready(lambda s:s['watch_settled']);s=a.ready();assert s['watch_gap']and s['observed']==38 and helpers(a)==ids
        a.act('refresh');s=a.ready(lambda s:s['observed']==5 and not s['changed']and not s['watch_gap'])
        assert area_names(s)=={'replacement.bin'};a.act('select:replacement.bin');a.ready(lambda s:s['selected']=='replacement.bin')
        a.capture('space-map-root-replacement-explicit-recovery');a.close()
        identities=helpers(a)
        for pid,start in identities:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'known helper identity survived: '+pid
        assert (old/'Fotos/nested.bin').read_bytes()==b'n'*26 and (old/'renamed.bin').read_bytes()==b'e'*12
        report.update(functional_gate='PASS',normal_exits=1,logged_space_helpers=sorted(identities),scope='actual nonrecursive watch -> sticky observation invalidation -> explicit button/F5 listing+map reload -> real painted hits; direct create/rename/delete, nested explicit recalculation, root replacement/recovery,440 controls; no automatic scan, private handlers/FB, no OS input/resource/global lifetime claim')
    except BaseException as e:report.update(functional_gate='FAIL',error=repr(e));raise
    finally:
        for app in apps:
            report['logs'][app.label]=app.log.read_text()
            if app.p.poll()is None:
                os.killpg(app.p.pid,signal.SIGTERM)
                try:app.p.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.p.pid,signal.SIGKILL);app.p.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:
            file=(Path.home()/'.local/bin'/name)if args.installed else ROOT/'target/release'/name;report[key]=hashlib.sha256(file.read_bytes()).hexdigest()
        path=OUT/('space-map-watch-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
