#!/usr/bin/env python3
"""Owned paused queue and shared controls; no OS input or resource experiment."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time,zipfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/queue-review';OUT.mkdir(parents=True,exist_ok=True)
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

def rects_fit(s,w,h):
    controls=[c['rect']for c in s['copy_controls']];assert len(controls)==3
    bars=[b['rect']for b in s['mc']['buttons']];assert len(bars)==8
    for i,(x,y,cw,ch) in enumerate(controls):
        assert x>=0 and y>=90 and x+cw<=w and y+ch<=min(b[1]for b in bars)
        for bx,by,bw,bh in controls[i+1:]+bars:
            assert x+cw<=bx or bx+bw<=x or y+ch<=by or by+bh<=y
with tempfile.TemporaryDirectory(prefix='queue-review-ui-',dir=BASE)as tmp:
    base=Path(tmp)
    try:
        for mode,view in [('view-list','List'),('view-groups','Groups'),('view-grid','Grid'),('view-columns','Columns'),('dual','List')]:
            folder=base/mode;folder.mkdir();(folder/'out').mkdir()
            for name in ['one','two','three','move']:(folder/(name+'.txt')).write_bytes(('owned '+name).encode())
            with zipfile.ZipFile(folder/'archive.zip','w')as archive:archive.writestr('inside.txt',b'owned archive')
            a=App(mode,folder,base);a.act('palette:'+mode);a.ready(lambda s:s['view_kind']==view and s['dual']==(mode=='dual'))
            a.act('bar-toggle');a.ready(lambda s:s['operation_bar'])
            a.act('select:one.txt');a.ready();a.act('f5');a.ready(lambda s:s['copy_dialog'] is not None)
            a.act('copy-dest:out');a.ready();a.act('copy-start-paused');a.ready();a.act('copy-start')
            first=a.ready(lambda s:s['copy_active'] is not None and s['copy_phase']=='Paused')['copy_active']
            assert not (folder/'out/one.txt').exists()
            a.capture(mode+'-first-paused')
            a.act('select:two.txt');s=a.ready();assert all(b['reason'] is None for b in s['mc']['buttons'] if b['index'] in [3,4])
            a.act('bar:3');s=a.ready(lambda s:s['copy_dialog'] is not None)
            assert s['copy_dialog']['names_raw']==[b'two.txt'.hex()] and s['copy_active']==first and s['copy_phase']=='Paused'
            a.capture(mode+'-mc-next-review')
            a.act('copy-dest:out');a.ready();a.act('copy-start');s=a.ready(lambda s:s['copy_queue']==1 and s['copy_active']==first)
            second=s['copy_queued_ids'][0];assert second!=first and not (folder/'out/two.txt').exists()
            a.act('select:three.txt');a.ready();a.act('palette:copy');s=a.ready(lambda s:s['copy_dialog'] is not None)
            assert s['copy_queue']==1 and s['copy_dialog']['names_raw']==[b'three.txt'.hex()]
            a.capture(mode+'-palette-next-review');a.act('copy-close');a.ready(lambda s:s['copy_dialog'] is None)
            a.act('entry-menu');s=a.ready();assert all(i['enabled'] for i in s['selection_menu'] if 'Copiar selección' in i['label'] or 'Mover selección' in i['label'])
            a.capture(mode+'-context-review-available');a.act('menu-close');a.ready()
            a.act('select:move.txt');a.ready();a.act('palette:move');a.ready(lambda s:s['copy_dialog'] is not None and s['copy_dialog']['moving'])
            a.act('copy-dest:out');a.ready();a.act('copy-start');s=a.ready(lambda s:s['copy_dialog'] is not None and s['copy_dialog']['reviewed'])
            assert s['copy_active']==first and s['copy_queued_ids']==[second] and (folder/'move.txt').exists() and not (folder/'out/move.txt').exists()
            a.capture(mode+'-move-token-review');a.act('copy-close');a.ready(lambda s:s['copy_dialog'] is None)
            a.act('select:three.txt');a.ready();a.act('palette:create-zip');a.ready(lambda s:s['copy_dialog'] is not None and s['copy_dialog']['creating'])
            a.act('copy-close');a.ready();a.act('select:archive.zip');a.ready();a.act('palette:extract');a.ready(lambda s:s['copy_dialog'] is not None and s['copy_dialog']['extracting'])
            a.act('copy-close');a.ready();a.act('fixture-minimum');a.ready();a.act('fixture-rail:on');s=a.ready()
            size=a.capture(mode+'-compact-queue-controls');assert size==(440,440);rects_fit(s,*size)
            a.act('copy-clear');s=a.ready(lambda s:s['copy_queue']==0);assert s['copy_active']==first and s['copy_phase']=='Paused'
            assert not (folder/'out/two.txt').exists()
            a.act('fixture-wide');a.ready();a.act('fixture-rail:off');a.ready();a.act('select:two.txt');a.ready();a.act('bar:3');a.ready(lambda s:s['copy_dialog'] is not None)
            a.act('copy-dest:out');a.ready();a.act('copy-start');s=a.ready(lambda s:s['copy_queue']==1)
            assert second not in s['copy_queued_ids'] and s['copy_active']==first and s['copy_phase']=='Paused'
            a.act('copy-resume');a.wait(lambda:(folder/'out/one.txt').exists() and (folder/'out/two.txt').exists());s=a.ready(lambda s:not s['copy_busy'])
            for name in ['one','two']:
                expected=('owned '+name).encode();assert (folder/(name+'.txt')).read_bytes()==expected and (folder/'out'/(name+'.txt')).read_bytes()==expected
            assert sorted(p.name for p in (folder/'out').iterdir())==['one.txt','two.txt']
            assert (folder/'three.txt').read_bytes()==b'owned three' and (folder/'move.txt').read_bytes()==b'owned move'
            a.act('fixture-minimum');a.ready();a.act('fixture-rail:on');s=a.ready()
            size=a.capture(mode+'-completed-retained-sources');assert size==(440,440)
            detail=next(c['rect']for c in s['copy_controls']if c['action']=='details');x,y,dw,dh=detail
            assert x>=186 and x+dw<=440 and y>=90 and y+dh<=min(b['rect'][1]for b in s['mc']['buttons'])-48
            a.close()
        identities=set().union(*(set(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=\w+',a.log.read_text(errors='replace')))for a in apps))
        for pid,start in identities:
            try:parts=Path('/proc',pid,'stat').read_text().rsplit(')',1)[1].split()
            except FileNotFoundError:continue
            assert parts[19]!=start,'known child survived: '+pid
        report.update(functional_gate='PASS',normal_exits=len(apps),logged_helpers=sorted(identities),scope='Owned small functional fixtures in List/Groups/Grid/Columns/Dual: first copy paused, next via MC queued, palette/context/Move FD token/ZIP reviews while active, cancellation preserves jobs and sources; compact440 expanded sidebar controls within canvas and above MC; clear pending preserves active; resume copies exactly retained requests; normal close drain. Private handlers/real framebuffer are not OS input, clipboard, AT-SPI,2x or global lifetime evidence; no resource/startup experiment.')
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
        path=OUT/('queue-review-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
