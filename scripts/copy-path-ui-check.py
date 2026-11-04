#!/usr/bin/env python3
"""Owned raw path text export/import; no native clipboard or OS input."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/copy-path';OUT.mkdir(parents=True,exist_ok=True)
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


with tempfile.TemporaryDirectory(prefix='copy-path-ui-',dir=BASE)as tmp:
    base=Path(tmp)
    try:
        for mode in ['list','dual']:
            folder=base/mode;folder.mkdir()
            names=[b'raw\xff\n%2F\\xFF', '日本-100%20literal'.encode(), b'literal\\xFF']
            for name in names:
                os.mkdir(os.fsencode(folder)+b'/'+name)
                with open(os.fsencode(folder)+b'/'+name+b'/owned.txt','wb')as f:f.write(name)
            a=App(mode,folder,base);a.act('palette:'+('dual'if mode=='dual'else'view-list'));a.ready()
            for index,name in enumerate(names):
                a.act('select-raw:'+name.hex());a.ready(lambda s:s['selected_raw']==name.hex())
                offset=len(a.log.read_text(errors='replace'))
                a.act('fixture-copy-path-edit');a.state(lambda s:s['loaded'] and s['watch_settled'])
                lines=[line for line in a.log.read_text(errors='replace')[offset:].splitlines()if line.startswith('FILES_PATH_TEXT ')]
                assert len(lines)==1
                exported=json.loads(lines[0][len('FILES_PATH_TEXT '):]);text=exported['text']
                if index==0:
                    assert text.startswith('file://')and'%FF%0A%252F%5CxFF'in text and'URI file://'in exported['status']
                else:assert text==os.fsdecode(os.fsencode(folder)+b'/'+name)
                a.capture(mode+'-editor-'+str(index))
                expected=(os.fsencode(folder)+b'/'+name).hex()
                a.act('fixture-path-commit');s=a.ready(lambda s:s['path_raw']==expected)
                assert s['selection_error']==''
                a.capture(mode+'-destination-'+str(index))
                a.act('back');a.ready(lambda s:s['path_raw']==os.fsencode(folder).hex())
                with open(os.fsencode(folder)+b'/'+name+b'/owned.txt','rb')as f:assert f.read()==name
            assert set(os.listdir(os.fsencode(folder)))==set(names)
            a.close()
        report.update(functional_gate='PASS',normal_exits=len(apps),scope='Owned List/Dual: shared copy-path sink exports exact raw name as explicit local URI, Unicode/percent/backslash remain literal; same text enters actual address editor and commits navigation to exact raw directory, returns to parent, payloads unchanged, normal exit/drain. Sink bypasses native clipboard ownership/delivery; handlers/framebuffer are not OS input/AT-SPI/2x/resource evidence.')
    except BaseException as e:
        report.update(functional_gate='FAIL',error=repr(e));raise
    finally:
        for a in apps:
            report['logs'][a.label]=a.log.read_text(errors='replace')
            if a.p.poll()is None:
                os.killpg(a.p.pid,signal.SIGTERM)
                try:a.p.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(a.p.pid,signal.SIGKILL);a.p.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:
            file=(Path.home()/'.local/bin'/name)if args.installed else ROOT/'target/release'/name;report[key]=hashlib.sha256(file.read_bytes()).hexdigest()
        path=OUT/('copy-path-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+chr(10));print(path)
