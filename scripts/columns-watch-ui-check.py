#!/usr/bin/env python3
"""Owned external changes -> actual Model/watch/paint; no OS input/resource claims."""
import argparse,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser();parser.add_argument('--installed',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files'if args.installed else ROOT/'target/release/files'
OUT=ROOT/'docs/evidence/listing-model/columns';OUT.mkdir(parents=True,exist_ok=True)
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
            for line in self.log.read_text().splitlines(keepends=True):
                if not line.endswith('\n'):continue
                if line.startswith('FILES_HIDDEN_STATE '):
                    s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                    if s['seq']==self.seq:found.append(s)
            return found[-1]if found and predicate(found[-1])else None
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
def rows(s): return s['columns']['rows']
def all_ready(s): return bool(rows(s)) and all(c['loaded'] for c in rows(s))
def column_names(s,i): return set(rows(s)[i]['order'])
with tempfile.TemporaryDirectory(prefix='columns-listing-ui-',dir=BASE)as tmp:
    base=Path(tmp);folder=base/'Files';folder.mkdir();branch=folder/'A';branch.mkdir()
    (folder/'B').mkdir()
    for name in ['same.txt','alpha.txt','last.txt']:(branch/name).write_bytes(b'owned')
    raw=os.fsencode(branch)+b'/raw-\xff.txt'
    with open(raw,'wb')as f:f.write(b'raw')
    try:
        a=App('columns',folder,base);a.act('columns');a.ready(all_ready)
        a.act('columns-select:A');s=a.ready(lambda s:len(rows(s))==2 and all_ready(s)and rows(s)[1]['path']==str(branch))
        owner=rows(s)[1]['watch_id'];ticket=rows(s)[1]['id'];a.capture('columns-before-external-change')
        (branch/'0-external.txt').write_bytes(b'created')
        s=a.ready(lambda s:len(rows(s))==2 and all_ready(s)and b'0-external.txt'.hex()in column_names(s,1))
        assert rows(s)[1]['watch_id']==owner and rows(s)[1]['id']!=ticket
        assert bytes(s['path_raw'])==os.fsencode(folder);a.capture('columns-lookahead-external-create')
        a.act('columns-right');a.ready(lambda s:s['columns']['active']==1 and all_ready(s));a.act('columns-raw')
        before=a.ready(all_ready);selected=before['selected'];a.capture('columns-active-raw-selected')
        (folder/'external-ancestor.txt').write_bytes(b'ancestor')
        s=a.ready(lambda s:len(rows(s))==2 and all_ready(s)and b'external-ancestor.txt'.hex()in column_names(s,0))
        assert s['selected']==selected and s['selection']==before['selection']
        assert rows(s)[0]['selected']==b'A'.hex()and bytes(s['path_raw'])==os.fsencode(branch)
        a.capture('columns-ancestor-updates-preserves-active-origin')
        a.act('omni');before=a.state();count=before['listing_watch']['refreshes'];(folder/'deferred-ancestor.txt').write_bytes(b'deferred')
        s=a.state(lambda s:s['listing_watch']['pending']);assert s['listing_watch']['refreshes']==count
        assert b'deferred-ancestor.txt'.hex()not in column_names(s,0)
        a.capture('columns-path-edit-defers-ancestor');a.act('omni-cancel')
        a.ready(lambda s:all_ready(s)and b'deferred-ancestor.txt'.hex()in column_names(s,0))
        moved=folder/'moved-A';branch.rename(moved)
        s=a.state(lambda s:len(rows(s))==1 and s['bookmarks']['browser_loaded']and bool(s['error']))
        assert bytes(s['path_raw'])==os.fsencode(branch);a.capture('columns-missing-branch-retains-active-path')
        moved.rename(branch)
        a.ready(lambda s:all_ready(s)and not s['error']and b'raw-\xff.txt'.hex()in names(s))
        a.capture('columns-restored-branch-recovers');a.close()
        report.update(functional_gate='PASS',normal_exits=1,scope='actual watch -> bounded column owner -> async reload -> painted state; external lookahead/ancestor changes, raw active marks, path-edit deferral, missing branch revocation and recovery; private handlers/framebuffer, no OS input/resource/full-lifetime claim')
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
        path=OUT/('columns-listing-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
