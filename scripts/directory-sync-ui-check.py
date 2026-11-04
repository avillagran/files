#!/usr/bin/env python3
"""Owned comparison panel/reader lifecycle; handlers/framebuffer, not OS input."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/directory-sync-ui';OUT.mkdir(parents=True,exist_ok=True)
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

def open_sync(a,right):
    a.act('palette:review-sync-folders');comparison(a,lambda p:p['mode']=={'update':'left'});root(a,1,right)

def select(a,name):
    a.act('fixture-compare-select:'+os.fsencode(name).hex());return comparison(a,lambda p:p['selected_raw']==os.fsencode(name).hex())

with tempfile.TemporaryDirectory(prefix='directory-sync-ui-',dir=BASE)as tmp:
    base=Path(tmp)
    try:
        left,right=owned_pair(base,'main');right.rmdir();right=Path(os.fsdecode(os.fsencode(base)+b'/main-b-'+bytes([255,10])));right.mkdir()
        for folder in [left,right]:
            for n in range(150):put(folder,('file-%03d.dat'%n).encode(),b'x')
            put(folder,b'raw\xff\n',b'raw');put(folder,b'metadata',b'same')
        put(left,b'content',b'left');put(right,b'content',b'rite')
        put(left,b'only-a',b'a');put(right,b'only-b',b'b')
        put(left,b'.hidden-a',b'hidden');(left/'empty-a').mkdir();(right/'empty-b').mkdir()
        (left/'source-dir/deep').mkdir(parents=True);put(left,b'source-dir/deep/file',b'x')
        (right/'extra-dir/deep').mkdir(parents=True);put(right,b'extra-dir/deep/file',b'y')
        (left/'type-dir').mkdir();put(left,b'type-dir/child',b'x');put(right,b'type-dir',b'file')
        os.chmod(left/'metadata',0o644);os.chmod(right/'metadata',0o600)
        os.symlink('target-a',left/'link');os.symlink('target-b',right/'link')
        put(left,b'raw\\xFF',b'literal');put(right,'raw�\n'.encode(),b'shadow')
        before=[snapshot(left),snapshot(right)]
        a=App('main',left,base);a.act('palette:view-list');a.ready();open_sync(a,right)
        modes=[{'update':'left'},{'update':'right'},{'mirror':'left'},{'mirror':'right'},'merge']
        for n,mode in enumerate(modes):
            if n:a.act('fixture-compare-mode');s=comparison(a);assert s['comparison']['rows']==0 and s['comparison']['mode']==mode
            a.act('fixture-compare-start');s=finish(a);assert s['comparison']['mode']==mode and not s['comparison']['blocked'] and s['comparison']['rows']>150
            p=select(a,'content')['comparison'];assert p['selected_proposal']=='Review(Content)'and p['selected_choice']=='Pending'
            a.act('fixture-compare-choice');a.act('fixture-compare-choice');p=comparison(a)['comparison'];expected='Right'if mode in [{'update':'right'},{'mirror':'right'}]else'Left';assert p['selected_choice']=='PreserveAndReplaceFrom('+expected+')'
            a.capture('mode-'+str(n)+'-review-choice')
            a.act('fixture-compare-pending');p=comparison(a)['comparison'];assert p['selected_choice']=='Pending'
            p=select(a,'metadata')['comparison'];assert p['selected_proposal']=='Review(Metadata)'
            a.act('fixture-compare-choice');a.act('fixture-compare-choice');p=comparison(a)['comparison'];assert p['selected_choice']=='PreserveAndSetMetadataFrom('+expected+')'
            p=select(a,'.hidden-a')['comparison'];assert p['selected_proposal']==('Retain'if mode=={'update':'right'}else'Review(DestinationOnly)'if mode=={'mirror':'right'}else'CopyFrom(Left)')
            p=select(a,'empty-a')['comparison'];assert p['selected_proposal']==('Retain'if mode=={'update':'right'}else'Review(DestinationOnly)'if mode=={'mirror':'right'}else'CopyFrom(Left)')
            if mode in [{'mirror':'left'},{'mirror':'right'}]:
                name='extra-dir'if mode=={'mirror':'left'}else'source-dir';p=select(a,name)['comparison'];assert p['selected_proposal']=='Review(DestinationOnly)'
                a.act('fixture-compare-choice');a.act('fixture-compare-choice');p=comparison(a)['comparison'];assert p['selected_choice']=='PreserveAndRemove';a.capture('recoverable-retirement-'+str(n))
            p=select(a,'type-dir/child')['comparison'];assert p['selected_proposal'].startswith('CoveredBy(')and not any(b['command'].startswith('Choice(')for b in p['buttons'])
        # Merge allows either winner explicitly, but performs no effects.
        select(a,'content');a.act('fixture-compare-choice');a.act('fixture-compare-choice');a.act('fixture-compare-choice');p=comparison(a)['comparison'];assert p['selected_choice']=='PreserveAndReplaceFrom(Right)'
        a.act('fixture-minimum');comparison(a);assert a.capture('compact-explicit-choice')==(440,440)
        for _ in range(3):a.act('fixture-compare-filter')
        p=comparison(a)['comparison'];assert p['filter']==3;count=p['visible'];select(a,'link');a.act('fixture-compare-choice');p=comparison(a)['comparison'];assert p['visible']==count-1;a.capture('compact-pending-filter')
        a.act('fixture-compare-filter');p=comparison(a)['comparison'];assert p['filter']==4;a.capture('compact-proposals-filter')
        offset=len(a.log.read_text(errors='replace'));a.act('fixture-compare-report');comparison(a)
        lines=[line for line in a.log.read_text(errors='replace')[offset:].splitlines()if line.startswith('FILES_COMPARE_REPORT ')];assert len(lines)==1;text=json.loads(lines[0][len('FILES_COMPARE_REPORT '):])['text'];assert 'NO aplica cambios'in text and 'PreserveAndReplaceFrom(Right)'in text and 'CoveredBy'in text and 'raw\\xFF\\n'in text
        # Switching to comparison invalidates the review; switching back is fresh.
        a.act('fixture-compare-mode');p=comparison(a)['comparison'];assert p['mode']is None and p['rows']==0
        a.act('fixture-compare-start');finish(a);a.capture('comparison-mode-retained')
        a.act('fixture-compare-mode');comparison(a);a.act('fixture-compare-start');finish(a)
        for _ in range(2):a.act('fixture-compare-filter')
        select(a,b'raw\xff\n');a.act('fixture-compare-b');s=a.ready(lambda s:s['comparison']is None and s['path_raw']==os.fsencode(right).hex());assert s['selected_raw']==b'raw\xff\n'.hex();a.capture('raw-locate-b');a.close()
        # Dual has a captured owner; identical roots are rejected before a helper.
        a=App('dual',left,base);a.act('palette:dual');a.ready(lambda s:s['dual']);a.act('palette:review-sync-folders');comparison(a);a.act('fixture-compare-start');p=comparison(a,lambda p:not p['busy'])['comparison'];assert p['error']and p['rows']==0
        root(a,1,right);a.act('fixture-compare-start');s=finish(a);owner=s['pane_owner'];a.act('pane-next');s=comparison(a);assert s['pane_owner']==owner;a.capture('dual-owner-and-review');a.act('fixture-compare-close');a.ready(lambda s:s['comparison']is None);a.close()
        # Map reader releases before admission and resumes after panel close.
        a=App('map',left,base);a.act('palette:view-space');a.state(lambda s:s['state']=='ready'and s['watch_settled']);open_sync(a,right);a.act('fixture-compare-start');finish(a);a.capture('map-review-overlay');a.act('fixture-compare-close');a.state(lambda s:s['comparison']is None and s['state']=='ready');a.capture('map-resumed');a.close()
        empty_a,empty_b=owned_pair(base,'empty');a=App('empty',empty_a,base);a.act('palette:view-list');a.ready();open_sync(a,empty_b);a.act('fixture-compare-start');p=finish(a)['comparison'];assert p['rows']==0 and p['unresolved']==0 and not p['blocked'];a.capture('empty-review-not-execution');a.close()
        partial_a,partial_b=owned_pair(base,'partial')
        for folder in [partial_a,partial_b]:
            with open(folder/'large.bin','wb')as f:f.truncate(1024**3)
        put(partial_a,b'unique',b'x');a=App('partial',partial_a,base);a.act('palette:view-list');a.ready();open_sync(a,partial_b);a.act('fixture-compare-start');p=finish(a)['comparison'];assert p['blocked']and p['progress']['reasons']&8
        select(a,'unique');p=comparison(a)['comparison'];assert p['selected_proposal']=='Blocked'and not any(b['command'].startswith('Choice(')for b in p['buttons']);a.act('fixture-minimum');comparison(a);a.capture('compact-all-blocked')
        a.act('fixture-compare-budget');p=comparison(a)['comparison'];assert p['rows']==0 and p['blocked']is None;a.close()
        cancel_a,cancel_b=owned_pair(base,'cancel')
        for folder in [cancel_a,cancel_b]:
            with open(folder/'large.bin','wb')as f:f.truncate(4*1024**3)
        a=App('cancel',cancel_a,base);a.act('palette:view-list');a.ready();open_sync(a,cancel_b);a.act('fixture-compare-budget');comparison(a);a.act('fixture-compare-start');comparison(a,lambda p:p['busy']and p['progress']['bytes_read']>0);a.capture('sync-helper-active');a.act('fixture-compare-cancel');p=comparison(a,lambda p:not p['busy'])['comparison'];assert p['rows']==0 and p['error'];a.capture('cancel-after-ack')
        a.act('fixture-compare-start');comparison(a,lambda p:p['busy']and p['progress']['bytes_read']>0);a.close();log=a.log.read_text(errors='replace');acks=[m.start()for m in re.finditer(r'FILES_COMPARE_ACK .*root_closing=true',log)];assert acks and acks[-1]<log.rfind('FILES_CLOSE_DRAINED')
        assert [snapshot(left),snapshot(right)]==before
        tracked=[]
        for app in apps:
            for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=prepare_directory_sync',app.log.read_text(errors='replace')):
                stat=Path('/proc')/pid/'stat'
                if stat.exists():assert stat.read_text().rsplit(')',1)[1].split()[19]!=start
                tracked.append({'pid':int(pid),'start':start})
        assert tracked;report['sync_helper_identities']=tracked
        report.update(functional_gate='PASS',normal_exits=len(apps),scope='Owned visible sync dry-run all five modes, explicit typed per-row pending/skip/replace/remove review, compact controls/pending/proposal filters, ancestor coverage, raw root/report/Locate, mode/budget invalidation, comparison retained, Dual owner/identical-root rejection, SpaceMap stop/resume, empty/partial-block-all, actual helper hashing then cancel ACK and rootclose ACK-before-drain; namespace/content/link/mode/mtime unchanged. No Apply/queue/recovery implemented. Internal handlers/framebuffer not native input/clipboard/AT-SPI/2x/global lifetime proof. Sparse files functional only, no resource experiment.')
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
        path=OUT/('directory-sync-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+chr(10));print(path)
