#!/usr/bin/env python3
"""Owned alternate copy name evidence; production handlers/framebuffer, no OS input or resource sampling."""
import argparse,fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
parser=argparse.ArgumentParser()
parser.add_argument('--operation-labels',action='store_true')
parser.add_argument('--installed',action='store_true')
args=parser.parse_args()
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/copy-names';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
BINARY=Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
apps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1'}
        with self.log.open('w') as output:
            self.process=subprocess.Popen([str(BINARY),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size',size,'--scale','1','--exit-after','180'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
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



with tempfile.TemporaryDirectory(prefix='copy-names-',dir=BASE) as temp:
    base=Path(temp);config=base/'config';config.mkdir();source=base/os.fsdecode(b'Source-\xff\n' if args.operation_labels else b'Source');source.mkdir();target=base/'Target';target.mkdir()
    raw=b'raw-\xff'+(b'\n\\literal-'+chr(0x202e).encode() if args.operation_labels else b'')+b'.txt'
    for n in [b'a.txt',b'b.txt',raw]:
        with open(os.fsencode(source)+b'/'+n,'wb')as out:out.write(b'owned:'+n)
    (source/'folder').mkdir();(source/'folder/deep.txt').write_bytes(b'nested bytes');(source/'link').symlink_to('a.txt')
    before={n:sha(source/os.fsdecode(n))for n in [b'a.txt',b'b.txt',raw]}
    def field(s):return s['destination_editor']
    def finished(job,ok):return lambda s:s['destination_queue_idle']and s['copy_result']is not None and s['copy_result']['id']==job and s['copy_result']['ok']==ok
    try:
        a=App('A',source,base,config,target)
        a.act('dest-copy-all');a.act('dest-text:'+str(target));a.act('dest-key:name-focus')
        def seek(name):
            s=a.state()
            while field(s)['name_row']>0:s=a.act('dest-name-prev')
            for _ in range(5):
                if bytes(field(s)['name_original'])==name:return s
                s=a.act('dest-name-next')
            raise AssertionError('captured row absent '+repr(name))
        s=seek(b'a.txt');assert field(s)['name_focus']
        a.act('dest-name-text:copie ñ.txt');a.capture('copy-name-first-root')
        a.act('dest-request-paste');s=seek(b'b.txt');a.act('dest-late-paste')
        a.act('dest-name-text:copie ñ.txt');s=a.act('dest-start',lambda s:field(s)is not None and bool(field(s)['error']))
        assert not list(target.iterdir())and s['copy_result']is None;a.capture('copy-name-duplicate-plan-rejected')
        a.act('dest-name-focus');a.act('dest-name-text:b-copy.txt');seek(b'folder')
        a.act('dest-name-text:folder-copy');seek(b'link');a.act('dest-name-text:link-copy');s=seek(raw);assert bytes(field(s)['name_original'])==raw
        if args.operation_labels:
            assert field(s)['name_original_label']==r'raw-\xFF\n\\literal-\u{202E}.txt'
            a.capture('copy-original-escaped-controls-review')
            a.act('dest-options',lambda s:field(s)['options_open']);a.capture('copy-source-escaped-options')
            a.act('dest-options',lambda s:not field(s)['options_open'])
        a.act('dest-key:end');s=a.act('dest-name-text:suffix');assert field(s)['error']
        a.capture('copy-raw-name-partial-protected');a.act('dest-name-reset');a.act('dest-name-text:raw-copy.txt')
        old=field(a.act('dest-name-oversize'))['name_draft'];assert old=='raw-copy.txt'
        a.capture('copy-name-oversize-retained');a.act('dest-name-reset');a.act('dest-name-text:raw-copy.txt')
        a.act('dest-request-paste');s=a.act('dest-key:folder-focus');assert not field(s)['name_focus'];a.act('dest-late-paste')
        assert field(a.state())['names_changed']==5
        a.capture('copy-name-mapped-five-review')
        s=a.act('dest-start',finished(1,True));assert s['copy_result']['completed']==5
        assert sha(target/'copie ñ.txt')==before[b'a.txt']and sha(target/'b-copy.txt')==before[b'b.txt']and sha(target/'raw-copy.txt')==before[raw]
        assert (target/'folder-copy/deep.txt').read_bytes()==b'nested bytes'and (target/'link-copy').is_symlink()and os.readlink(target/'link-copy')=='a.txt'
        a.capture('copy-names-five-root-result')
        # An alternate target still uses the same no-clobber publication.
        a.act('dest-copy-a');a.act('dest-text:'+str(target));a.act('dest-key:name-focus');a.act('dest-name-text:copie ñ.txt')
        s=a.act('dest-start',finished(2,False));assert s['copy_result']['completed']==0 and sha(target/'copie ñ.txt')==before[b'a.txt'];a.capture('copy-name-existing-target-preserved');a.act('dest-dismiss')
        # Literal backslash is a legal Unix basename, never a byte escape decoder.
        a.act('dest-copy-a');a.act('dest-text:'+str(target));a.act('dest-key:name-focus');a.act('dest-name-text:'+r'raw-\xFF.txt')
        a.act('dest-start',finished(3,True));assert sha(target/r'raw-\xFF.txt')==before[b'a.txt']and not(target/os.fsdecode(raw)).exists()
        # Private files clipboard with Source token uses the new checked engine.
        a.act('dest-private-copy-a',lambda s:s['clipboard']['count']==1);a.act('dest-private-paste',lambda s:field(s)is not None)
        a.act('dest-text:'+str(target));a.act('dest-key:name-focus');a.act('dest-name-text:clipboard-copy.txt');a.act('dest-start',finished(4,True));assert sha(target/'clipboard-copy.txt')==before[b'a.txt']
        # Same folder duplication and pointer placement: the source basename never changes.
        a.act('fixture-minimum',lambda s:s['width']==440);a.act('dest-copy-a');a.act('dest-name-focus');a.act('dest-name-text:a copy.txt')
        a.act('dest-name-point-end');a.act('dest-name-text:!');a.act('dest-key:undo');s=a.act('dest-key:redo');assert field(s)['name_draft']=='a copy.txt!'
        a.capture('copy-new-name-same-folder-440');a.act('dest-start',finished(5,True));assert sha(source/'a copy.txt!')==before[b'a.txt']
        a.capture('copy-same-folder-duplicate-result-440')
        if args.operation_labels:
            a.act('fixture-wide');a.act('select-name:a.txt')
            a.act('commands-shortcut');a.act('commands-query:create-zip');a.act('commands-row:create-zip')
            a.act('commands-key:enter',lambda s:field(s)is not None);a.capture('create-zip-literal-two-line-caption')
            a.act('dest-key:escape',lambda s:field(s)is None)
            import zipfile
            with zipfile.ZipFile(source/'review.zip','w') as archive:archive.writestr('inside.txt',b'owned archive')
            a.act('refresh',lambda s:'7265766965772e7a6970' in s['order']);a.act('select-name:review.zip')
            a.act('commands-shortcut');a.act('commands-query:extract');a.act('commands-row:extract')
            a.act('commands-key:enter',lambda s:field(s)is not None);a.capture('extract-zip-literal-two-line-caption')
            a.act('dest-key:escape',lambda s:field(s)is None)
        a.close()
        assert all(sha(source/os.fsdecode(n))==before[n]for n in before)
        assert os.readlink(source/'link')=='a.txt'and (source/'folder/deep.txt').read_bytes()==b'nested bytes'
        report.update(operation_labels=args.operation_labels,installed=args.installed,functional_gate='PASS',normal_exits=1,scope='5 original roots mapped independently to Unicode basenames, duplicate plan refused without mutations, raw partial/oversize/reset, row/folder stale paste rejects, actual bytes/directory/literal symlink copy5, occupied alternate target preserved, literal Unix backslash not decoded, private Source clipboard named copy, pointer/undo/redo and same-folder duplicate440, originals intact/root0ACK; no native input/clipboard MIME/race/powerloss/lifetimes/resources')
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['logs']={app.label:app.log.read_text() for app in apps}
        for app in apps:
            if app.process.poll()is None:
                os.killpg(app.process.pid,signal.SIGTERM)
                try:app.process.wait(timeout=3)
                except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
        for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha((Path.home()/'.local/bin'/name) if args.installed else ROOT/'target/release'/name)
        path=OUT/('copy-names-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)
