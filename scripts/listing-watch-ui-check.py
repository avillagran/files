#!/usr/bin/env python3
"""Owned external changes -> actual Model/watch/paint; no OS input/resource claims."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/listing-model';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','states':[],'captures':[],'logs':{}}
apps=[]
class App:
    def __init__(self,label,folder,base):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        config=base/(label+'-config');config.mkdir()
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
        with self.log.open('w')as out:self.p=subprocess.Popen([str(BINARY),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','180'],env=env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
        apps.append(self);self.state(lambda s:s['bookmarks']['browser_loaded']);self.act('list');self.ready()
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
            for line in self.log.read_text().splitlines():
                if line.startswith('FILES_HIDDEN_STATE '):
                    s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    if s['seq']==self.seq and predicate(s):found.append(s)
            return found[-1]if found else None
        s=self.wait(check);report['states'].append({'app':self.label,'state':s});return s
    def act(self,command,predicate=lambda s:True):
        offset=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(command);pending.replace(self.raw.with_suffix('.command'))
        match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(command)+r'$',self.log.read_text()[offset:],re.M));self.seq=int(match[1]);return self.state(predicate)
    def ready(self,predicate=lambda s:True):return self.state(lambda s:s['bookmarks']['browser_loaded'] and s['listing_watch']['settled'] and predicate(s))
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

def names(s):return set(s['visible'])
with tempfile.TemporaryDirectory(prefix='listing-ui-',dir=BASE)as tmp:
    base=Path(tmp);folder=base/'Files';folder.mkdir()
    for name in ['a.txt','b.txt','c.txt']: (folder/name).write_bytes(b'owned')
    raw=os.fsencode(folder)+b'/raw-\xff.txt'
    with open(raw,'wb')as f:f.write(b'raw')
    try:
        a=App('listing',folder,base);a.act('selection-clear');a.act('select-name:b.txt');before=a.act('toggle-raw-name');a.capture('listing-before-external-change')
        (folder/'0-created.txt').write_bytes(b'created')
        s=a.ready(lambda s:b'0-created.txt'.hex()in names(s));assert set(s['selection'])==set(before['selection'])and s['selected']==before['selected']
        a.capture('listing-created-preserves-raw-marks-focus')
        (folder/'c.txt').rename(folder/'renamed.txt');(folder/'a.txt').unlink()
        a.ready(lambda s:b'renamed.txt'.hex()in names(s)and b'c.txt'.hex()not in names(s)and b'a.txt'.hex()not in names(s));a.capture('listing-renamed-and-deleted-automatically')
        a.act('omni');before=a.state();count=before['listing_watch']['refreshes'];(folder/'during-edit.txt').write_bytes(b'deferred')
        s=a.state(lambda s:s['listing_watch']['pending']);assert s['listing_watch']['refreshes']==count and b'during-edit.txt'.hex()not in names(s)
        a.capture('listing-edit-defers-pending-change');a.act('omni-cancel');a.ready(lambda s:b'during-edit.txt'.hex()in names(s));a.capture('listing-edit-cancel-applies-pending')
        assert (folder/'b.txt').read_bytes()==b'owned'and Path(os.fsdecode(raw)).read_bytes()==b'raw';a.close()
        one=base/'One';two=base/'Two';one.mkdir();two.mkdir();(one/'from-one.txt').write_bytes(b'one');(two/'from-two.txt').write_bytes(b'two')
        alias=base/'Alias';alias.symlink_to(one,target_is_directory=True);b=App('alias',alias,base)
        assert bytes(b.ready()['path_raw'])==os.fsencode(alias)
        replacement=base/'replacement';replacement.symlink_to(two,target_is_directory=True);replacement.replace(alias)
        b.ready(lambda s:b'from-two.txt'.hex()in names(s)and b'from-one.txt'.hex()not in names(s));b.capture('listing-alias-replacement-follows-open-path');b.close()
        report.update(functional_gate='PASS',normal_exits=2,scope='actual watch -> Model -> async listing -> painted state; create/rename/delete/raw marks+focus, path edit deferral then apply, root alias replacement; private handlers/real framebuffer, no OS input/resource/full-lifetime claim')
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
        path=OUT/('listing-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
