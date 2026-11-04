#!/usr/bin/env python3
"""Owned name/metadata layout and raw targets; handlers/framebuffer, not OS input."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/narrow-names';OUT.mkdir(parents=True,exist_ok=True)
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
            for line in self.log.read_text(errors='replace').splitlines(keepends=True):
                if not line.endswith('\n'):continue
                if line.startswith('FILES_SPACE_STATE '):
                    s=json.loads(line[len('FILES_SPACE_STATE '):])
                    if s['seq']==self.seq:found.append(s)
            return found[-1]if found and predicate(found[-1])else None
        s=self.wait(check);report['states'].append({'app':self.label,'state':s});return s
    def act(self,command,predicate=lambda s:True):
        offset=len(self.log.read_text(errors='replace'));pending=self.raw.with_suffix('.pending');pending.write_text(command);pending.replace(self.raw.with_suffix('.command'))
        match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(command)+r'$',self.log.read_text(errors='replace')[offset:],re.M));self.seq=int(match[1]);return self.state(predicate)
    def ready(self,predicate=lambda s:True):return self.state(lambda s:s['loaded'] and s['scene_ready'] and s['other_loaded'] and s['watch_settled'] and predicate(s))
    def capture(self,label):
        self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
        def complete():
            if not self.raw.exists():return False
            parts=self.raw.read_bytes().split(b'\n',3)
            if len(parts)!=4:return False
            w,h=map(int,parts[1].split());return len(parts[3])==w*h*3
        self.wait(complete);path=OUT/(label+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(self.raw),str(path)],check=True,timeout=15);report['captures'].append(str(path));return tuple(map(int,self.raw.read_bytes().split(b'\n',3)[1].split()))
    def close(self):
        offset=len(self.log.read_text(errors='replace'));pending=self.raw.with_suffix('.pending');pending.write_text('callback');pending.replace(self.raw.with_suffix('.command'))
        self.p.wait(timeout=25);assert self.p.returncode==0 and 'FILES_CLOSE_DRAINED'in self.log.read_text(errors='replace')[offset:]

with tempfile.TemporaryDirectory(prefix='narrow-names-ui-',dir=BASE)as tmp:
    base=Path(tmp)
    try:
        for mode in ['list','dual']:
            folder=base/mode;folder.mkdir();(folder/'Folder').mkdir()
            payloads={b'one.txt':b'one',b'two.txt':b'two-two',bytes([97,45,255,10]):b'raw',bytes([97,92,113]):b'slash'}
            for n,(name,data) in enumerate(payloads.items()):
                path=os.fsencode(folder)+b'/'+name
                with open(path,'wb')as f:f.write(data)
                os.utime(path,(100+n,100+n))
            a=App(mode,folder,base);a.act('palette:'+('dual'if mode=='dual'else'view-list'));a.ready(lambda s:s['dual']==(mode=='dual'))
            a.act('bar-toggle');a.ready(lambda s:s['operation_bar'])
            raw=bytes([97,45,255,10]);raw_hex=raw.hex()
            for layout in ['wide','compact-expanded','compact-collapsed']:
                a.act('fixture-wide'if layout=='wide'else'fixture-minimum');a.ready()
                a.act('fixture-rail:on'if layout=='compact-expanded'else'fixture-rail:off');a.ready()
                a.act('select-raw:'+raw_hex);s=a.ready(lambda s:s['selected_raw']==raw_hex)
                expected=(440,440)if layout!='wide'else(1000,560)
                assert a.capture(mode+'-'+layout+'-raw-name')==expected
                owner=s['pane_owner']
                for criterion,sort in [('size','Size'),('date','Modified'),('name','Name')]:
                    a.act('header-sort:'+criterion);s=a.ready(lambda s:s['sort']==sort)
                    assert s['selected_raw']==raw_hex and s['selection_raw']==[raw_hex] and s['pane_owner']==owner
                    a.capture(mode+'-'+layout+'-sort-'+criterion)
                a.act('palette:copy');s=a.ready(lambda s:s['copy_dialog'] is not None)
                assert s['copy_dialog']['names_raw']==[raw_hex] and not s['copy_busy'] and s['copy_queue']==0
                a.act('copy-close');a.ready(lambda s:s['copy_dialog'] is None)
                if mode=='dual':
                    a.act('pane-next');s=a.ready();assert s['pane_owner']!=owner
                    a.act('select-raw:'+raw_hex);a.ready();a.act('header-sort:size');s=a.ready(lambda s:s['sort']=='Size')
                    assert s['other_sort']=='Name' and s['selected_raw']==raw_hex
                    a.capture(mode+'-'+layout+'-other-pane-name')
                    a.act('header-sort:date');s=a.ready(lambda s:s['sort']=='Modified');assert s['other_sort']=='Name'
                    a.act('pane-next');s=a.ready();assert s['pane_owner']==owner and s['sort']=='Name' and s['other_sort']=='Modified' and s['selected_raw']==raw_hex
            for name,data in payloads.items():
                with open(os.fsencode(folder)+b'/'+name,'rb')as f:assert f.read()==data
            assert set(os.listdir(os.fsencode(folder)))==set(payloads)|{b'Folder'}
            a.close()
        report.update(functional_gate='PASS',normal_exits=len(apps),scope='Owned List/Dual,1000wide and440 collapsed/expanded sidebar with MC: actual name painting inspected separately, raw-byte selection, all visible sort controls preserve raw focus/marks/owner, independent pane sort, copy review captures exact raw name and does not enqueue, unchanged payloads and normal close/drain. Internal handlers/real framebuffer are not OS input, clipboard, AT-SPI,2x or global/resource proof.')
    except BaseException as e:
        report.update(functional_gate='FAIL',error=repr(e))
        for a in apps:
            if a.p.poll() is None:
                try:a.capture('failure-'+a.label)
                except BaseException as ce:report.setdefault('capture_errors',[]).append(repr(ce))
        raise
    finally:
        for a in apps:
            report['logs'][a.label]=a.log.read_text(errors='replace')
            if a.p.poll() is None:
                os.killpg(a.p.pid,signal.SIGTERM)
                try:a.p.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(a.p.pid,signal.SIGKILL);a.p.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:
            file=(Path.home()/'.local/bin'/name)if args.installed else ROOT/'target/release'/name;report[key]=hashlib.sha256(file.read_bytes()).hexdigest()
        path=OUT/('narrow-names-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+chr(10));print(path)
