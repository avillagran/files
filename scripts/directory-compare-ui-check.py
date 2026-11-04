#!/usr/bin/env python3
"""Owned comparison panel/reader lifecycle; handlers/framebuffer, not OS input."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/directory-compare-ui';OUT.mkdir(parents=True,exist_ok=True)
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


def comparison(a,predicate=lambda p:True):return a.state(lambda s:s.get('comparison')is not None and s['comparison']['buttons']and all(b['stamp']==s['comparison']['stamp']for b in s['comparison']['buttons'])and predicate(s['comparison']))
def root(a,side,path):
    a.act('fixture-compare-root:'+str(side)+':'+os.fsencode(path).hex());return comparison(a)
def finish(a):return comparison(a,lambda p:not p['busy']and p['progress']['phase']==3)
def open_panel(a,right):
    a.act('palette:compare-folders');comparison(a,lambda p:not p['busy']);root(a,1,right)

def owned_pair(base,label):
    left=base/(label+'-a');right=base/(label+'-b');left.mkdir();right.mkdir();return left,right

def put(folder,name,data):
    with open(os.fsencode(folder)+b'/'+name,'wb')as f:f.write(data)
    os.utime(os.fsencode(folder)+b'/'+name,(100,100))

def snapshot(folder):
    result={}
    root=os.fsencode(folder)
    for parent,dirs,files in os.walk(root,followlinks=False):
        for name in dirs+files:
            full=parent+b'/'+name;relative=os.path.relpath(full,root);stat=os.lstat(full)
            if os.path.islink(full):value=('link',os.readlink(full))
            elif os.path.isdir(full):value=('dir',None)
            else:
                with open(full,'rb')as f:value=('file',f.read())
            result[relative]=(value,stat.st_mode&0o7777,stat.st_mtime_ns)
    return result

with tempfile.TemporaryDirectory(prefix='directory-compare-ui-',dir=BASE)as tmp:
    base=Path(tmp)
    try:
        left,right=owned_pair(base,'main');right.rmdir();right=Path(os.fsdecode(os.fsencode(base)+bytes([47])+b'main-b-'+bytes([255,10])));right.mkdir()
        for folder in [left,right]:
            (folder/'nested').mkdir()
            for n in range(150):put(folder,('file-%03d.dat'%n).encode(),b'x')
            put(folder,b'same.txt',b'same');put(folder,b'raw\xff\n',b'raw')
        put(left,b'changed.bin',b'left');put(right,b'changed.bin',b'diff')
        put(left,b'nested/file',b'a');put(right,b'nested/file',b'b')
        put(left,b'.only-a',b'left');put(right,b'only-b',b'right')
        put(left,b'raw\\xFF',b'literal');put(right,'raw�\n'.encode(),b'shadow')
        os.symlink('target-a',left/'link');os.symlink('target-b',right/'link')
        long_path=('z-'+'long'*25)+'/'+('tail'*25)+'/'+('last'*25)
        (left/long_path).parent.mkdir(parents=True);put(left,os.fsencode(long_path),b'long')
        unchanged=[snapshot(left),snapshot(right)]
        a=App('main',left,base);a.act('palette:view-list');a.ready()
        a.act('palette:compare-folders');s=comparison(a);assert s['comparison']['editors'][1]==''
        root(a,1,right);s=comparison(a);assert s['comparison']['editors'][1].startswith('file://')and'%FF%0A'in s['comparison']['editors'][1];a.act('fixture-compare-start');s=finish(a);p=s['comparison'];assert p['rows']>150 and p['progress']['reasons']==0 and p['visible']<p['rows']
        assert a.capture('wide-differences')==(1000,650)
        a.act('fixture-compare-filter');s=comparison(a,lambda p:p['filter']==1);assert s['comparison']['visible']==0;a.capture('complete-empty-unknown-filter')
        a.act('fixture-compare-filter');s=comparison(a,lambda p:p['filter']==2);assert s['comparison']['visible']==s['comparison']['rows']
        for name,kind in [(b'changed.bin','Content'),(b'raw\xff\n','Equal'),(b'raw\\xFF','OnlyLeft'),('raw�\n'.encode(),'OnlyRight')]:
            a.act('fixture-compare-select:'+name.hex());s=comparison(a,lambda p:p['selected_raw']==name.hex());assert s['comparison']['selected_kind']==kind
        offset=len(a.log.read_text(errors='replace'));a.act('fixture-compare-report');s=comparison(a)
        lines=[line for line in a.log.read_text(errors='replace')[offset:].splitlines()if line.startswith('FILES_COMPARE_REPORT ')];assert len(lines)==1
        text=json.loads(lines[0][len('FILES_COMPARE_REPORT '):])['text'];assert 'no instantánea'in text and '"raw\\xFF\\n"'in text and '"raw\\\\xFF"'in text and '"raw�\\n"'in text
        a.act('fixture-minimum');comparison(a);a.act('fixture-compare-select:'+os.fsencode(long_path).hex());s=comparison(a);assert s['comparison']['pan_limit']>0
        assert a.capture('compact-long-path')==(440,440);a.act('fixture-compare-pan');s=comparison(a);assert s['comparison']['pan']==s['comparison']['pan_limit']>0;a.capture('compact-long-path-end')
        a.act('fixture-compare-end');s=comparison(a);assert s['comparison']['selected']==s['comparison']['visible']-1 and s['comparison']['top']>0;a.capture('compact-end')
        a.act('fixture-compare-select:'+bytes([114,97,119,255,10]).hex());a.act('fixture-compare-b');s=a.ready(lambda s:s['comparison']is None and s['path_raw']==os.fsencode(right).hex());assert s['selected_raw']==bytes([114,97,119,255,10]).hex();a.capture('raw-locate-b')
        a.close()
        # Dual captures the active and opposite browser roots, without stealing focus.
        dual=base/'dual';(dual/'B').mkdir(parents=True);put(dual,b'file',b'x');put(dual/'B',b'file',b'x')
        a=App('dual',dual,base);a.act('palette:dual');a.ready(lambda s:s['dual']);a.act('pane-next');a.ready();a.act('select-raw:'+b'B'.hex());a.ready();a.act('enter-selected');a.ready(lambda s:s['path_raw']==os.fsencode(dual/'B').hex());a.act('pane-next');a.ready(lambda s:s['path_raw']==os.fsencode(dual).hex())
        a.act('palette:compare-folders');s=comparison(a);assert s['comparison']['editors']==[str(dual),str(dual/'B')]and s['comparison']['focus']is None
        a.act('fixture-compare-start');s=finish(a);owner=s['pane_owner'];a.act('pane-next');s=comparison(a);assert s['pane_owner']==owner;a.capture('dual-captured-roots');a.act('fixture-compare-close');a.ready(lambda s:s['comparison']is None);a.close()
        # SpaceMap stops its reader and resumes its own projection after panel close.
        a=App('map',left,base);a.act('palette:view-space');a.state(lambda s:s['state']=='ready'and s['watch_settled']);open_panel(a,right);a.act('fixture-compare-start');finish(a);a.capture('map-comparison-overlay');a.act('fixture-compare-close');a.state(lambda s:s['comparison']is None and s['state']=='ready');a.capture('map-restored');a.close()
        # Empty namespace is a complete observation; filter emptiness is not a partial proof.
        empty_a,empty_b=owned_pair(base,'empty');a=App('empty',empty_a,base);a.act('palette:view-list');a.ready();open_panel(a,empty_b);a.act('fixture-compare-start');s=finish(a);assert s['comparison']['rows']==0 and s['comparison']['progress']['reasons']==0;a.capture('empty-complete');a.act('fixture-compare-close');a.ready(lambda s:s['comparison']is None);a.close()
        # A sparse large file exceeds the default read policy without reading its body.
        partial_a,partial_b=owned_pair(base,'partial')
        for folder in [partial_a,partial_b]:
            with open(folder/'large.bin','wb')as f:f.truncate(1024**3)
        a=App('partial',partial_a,base);a.act('palette:view-list');a.ready();open_panel(a,partial_b);a.act('fixture-compare-start');s=finish(a);p=s['comparison'];assert p['progress']['reasons']&8 and p['progress']['bytes_read']==0 and p['progress']['unknown']==1
        a.act('fixture-minimum');comparison(a);a.capture('partial-unknown');a.act('fixture-compare-filter');s=comparison(a);assert s['comparison']['visible']==1;a.capture('partial-unknown-filter')
        a.act('fixture-compare-budget');s=comparison(a);assert s['comparison']['rows']==0 and s['comparison']['progress']['phase']==0
        a.close()
        # Actual helper hashing is cancelled after observed progress; no timing or CPU metric.
        cancel_a,cancel_b=owned_pair(base,'cancel')
        for folder in [cancel_a,cancel_b]:
            with open(folder/'large.bin','wb')as f:f.truncate(4*1024**3)
        a=App('cancel',cancel_a,base);a.act('palette:view-list');a.ready();open_panel(a,cancel_b);a.act('fixture-compare-budget');s=comparison(a);assert s['comparison']['budget']==16*1024**3
        a.act('fixture-compare-start');comparison(a,lambda p:p['busy']and p['progress']['bytes_read']>0);a.capture('real-helper-active');a.act('fixture-compare-cancel');s=comparison(a,lambda p:not p['busy']);assert s['comparison']['rows']==0 and s['comparison']['error'];error=s['comparison']['error'];offset=len(a.log.read_text(errors='replace'));a.act('fixture-compare-report');s=comparison(a);lines=[line for line in a.log.read_text(errors='replace')[offset:].splitlines()if line.startswith('FILES_COMPARE_REPORT ')];assert len(lines)==1;text=json.loads(lines[0][len('FILES_COMPARE_REPORT '):])['text'];assert 'Sin informe final aceptado'in text and error in text and s['comparison']['error']==error;a.capture('cancel-after-ack')
        a.act('fixture-compare-start');comparison(a,lambda p:p['busy']and p['progress']['bytes_read']>0);a.close();log=a.log.read_text(errors='replace');acks=[m.start()for m in re.finditer(r'FILES_COMPARE_ACK .*root_closing=true',log)];assert acks and acks[-1]<log.rfind('FILES_CLOSE_DRAINED')
        for folder in [cancel_a,cancel_b]:assert (folder/'large.bin').stat().st_size==4*1024**3
        assert [snapshot(left),snapshot(right)]==unchanged
        for folder,expected in [(left,b'left'),(right,b'diff')]:
            with open(folder/'changed.bin','rb')as f:assert f.read()==expected
        tracked=[]
        for app in apps:
            for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=compare_directories',app.log.read_text(errors='replace')):
                stat=Path('/proc')/pid/'stat'
                if stat.exists():assert stat.read_text().rsplit(')',1)[1].split()[19]!=start
                tracked.append({'pid':int(pid),'start':start})
        assert tracked;report['comparison_helper_identities']=tracked
        report.update(functional_gate='PASS',normal_exits=len(apps),scope='Owned comparison UI List roots/manual+raw URI, Dual captured roots/modal owner and SpaceMap resume, complete/empty/partial report, raw names vs literal escape/replacement shadow, retained filter navigation/virtualized end/horizontal path pan, full report isolated text sink, exact side-B locate, budget change revokes old report, actual helper progress then cancel ACK and root close/drain. Sparse inputs are cancellation/read-limit fixtures; no throughput/PSS/CPU/startup experiment. Internal handlers/framebuffer are not native input/clipboard/AT-SPI/2x/global lifetime proof.')
    except BaseException as e:
        report.update(functional_gate='FAIL',error=repr(e))
        for a in apps:
            if a.p.poll()is None:
                try:a.capture('failure-'+a.label)
                except BaseException as ce:report.setdefault('capture_errors',[]).append(repr(ce))
        raise
    finally:
        for a in apps:
            report['logs'][a.label]=a.log.read_text(errors='replace')
            if a.p.poll()is None:
                os.killpg(a.p.pid,signal.SIGTERM)
                try:a.p.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(a.p.pid,signal.SIGKILL);a.p.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:
            file=(Path.home()/'.local/bin'/name)if args.installed else ROOT/'target/release'/name;report[key]=hashlib.sha256(file.read_bytes()).hexdigest()
        path=OUT/('directory-compare-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+chr(10));print(path)
