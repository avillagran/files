#!/usr/bin/env python3
"""Owned SFTP form/profile evidence; production handlers/framebuffer, no OS input or resource sampling."""
import argparse,fcntl,hashlib,json,os,re,signal,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
OUT=ROOT/'docs/evidence/sftp-preparation';OUT.mkdir(parents=True,exist_ok=True)
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
report=dict(functional_gate='IN_PROGRESS',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',method='owned filesystem fixtures and independent Files processes; painted handlers/private command pipes and real FLTK framebuffer; no native input',states=[],captures=[],processes=[])
apps=[]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
class App:
    def __init__(self,label,folder,base,config,place,size='1000x650'):
        self.label=label;self.seq=0;self.raw=base/(label+'.ppm');self.log=base/(label+'.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_BOOKMARK_PLACE':str(place),'FILES_CLOSE_TRACE':'1'}
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
                    form=state.get('sftp');snapshot=state.get('sftp_report');painted=snapshot['painted'] if snapshot else form is None or (form['hit_count']==8+min(len(state['sftp_profiles']['rows']),max(1,(state['height']-336)//36)) and form['profile_input_painted'] if form.get('profiles') else form['hit_count']==14)
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


def preparation():
    with tempfile.TemporaryDirectory(prefix='sftp-form-',dir=BASE) as temp:
        base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir();(source/'a.txt').write_bytes(b'owned source unchanged')
        digest=sha(source/'a.txt')
        def ready(s):return s['sftp']is not None and s['sftp']['capability_done'] and s['sftp']['hit_count']==14
        try:
            a=App('sftp-preparation',source,base,config,source);path=os.fsencode(source)
            a.act('clip-actions');s=a.act('sftp-open',ready)
            assert s['sftp']['capability_error']is None and s['sftp']['sshfs']is False
            assert 'sshfs'in s['sftp']['summary'];a.capture('sftp-missing-capability')
            s=a.act('sftp-review');assert s['sftp']['review']is None and 'Host'in s['sftp']['error']
            a.act('sftp-input:Work-Alias');a.act('sftp-key:ctrl2');a.act('sftp-input:alice');a.act('sftp-key:ctrl3');a.act('sftp-input:2222');a.act('sftp-key:ctrl4');a.act('sftp-input:/Fotos ñ/%20/$()')
            s=a.act('sftp-key:review');assert s['sftp']['review']=='alice@Work-Alias:/Fotos ñ/%20/$()'
            assert bytes(s['path_raw'])==path;a.capture('sftp-reviewed-literal-target')
            a.act('sftp-key:end');a.act('sftp-key:left');s=a.act('sftp-input:X');assert s['sftp']['fields'][3]=='/Fotos ñ/%20/$(X)' and s['sftp']['review']is None
            s=a.act('sftp-key:undo');assert s['sftp']['fields'][3]=='/Fotos ñ/%20/$()';a.act('sftp-key:review')
            s=a.act('sftp-connect',lambda s:not s['sftp_transport']['busy'] and 'sshfs' in s['sftp_transport']['diagnostic'])
            assert not s['sftp_transport']['open'] and not (config/'sftp').exists()
            a.capture('sftp-connect-missing-tools-no-mutation')
            a.act('sftp-request-paste');a.act('sftp-key:ctrl1');s=a.act('sftp-late-paste');assert s['sftp']['fields'][0]=='Work-Alias'
            a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);a.capture('sftp-review-controls-440')
            a.act('sftp-focus:2');a.act('sftp-key:all');a.act('sftp-input:65536');s=a.act('sftp-review');assert s['sftp']['review']is None and 'Puerto'in s['sftp']['error'];a.capture('sftp-invalid-port-440')
            old=s['sftp']['stamp'];a.act('sftp-request-paste');a.act('sftp-close');a.act('commands-shortcut');a.act('commands-query:sftp');s=a.act('commands-run-hit',ready)
            assert s['sftp']['stamp']!=old and s['sftp']['fields']==['','','',''];a.act('sftp-late-paste')
            s=a.act('sftp-old-close:'+str(old));assert s['sftp']is not None
            a.act('sftp-check',ready);a.capture('sftp-catalog-reopen-empty-440');a.close()
            assert sha(source/'a.txt')==digest and sorted(p.name for p in source.iterdir())==['a.txt']
            report.update(functional_gate='PASS',mount_gate='NOT_VALIDATED',normal_exits=1,scope='SFTP controls negative-capability scope: connect reports missing tools without creating registry; real AuxIo read-only tool/FUSE access probe reports sshfs missing;13paintedcontrols;invalidemptyhost/port65536 reject; host/user/port/Unicodepercent-shellliteral path reviewed without dispatch; cursor/middleedit/undo; delayedpaste acrossfield/reopen rejected; oldstamp ignored; recheck; Actions+catalog routes;440controls/sourcebytes/root0mutationdrainACK. No SSH/FUSE/authentication/mount/readonlysyscall-drain/nativeinput/clipboardbytes/resources proof.')
        except BaseException as error:report.update(functional_gate='FAIL',mount_gate='NOT_VALIDATED',error=repr(error));raise
        finally:
            report['logs']={app.label:app.log.read_text()for app in apps}
            for app in apps:
                if app.process.poll()is None:
                    os.killpg(app.process.pid,signal.SIGTERM)
                    try:app.process.wait(timeout=3)
                    except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
            for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
            path=OUT/('sftp-preparation-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)

def profiles():
    with tempfile.TemporaryDirectory(prefix='sftp-profiles-',dir=BASE) as temp:
        base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir();(source/'owned.txt').write_bytes(b'unchanged owned fixture')
        digest=sha(source/'owned.txt');path=os.fsencode(source);file=config/'sftp-profiles.bin'
        def open_form(a):
            a.act('clip-actions');return a.act('sftp-open',lambda s:s['sftp']is not None and s['sftp']['capability_done'])
        def idle(s):return s['sftp_profiles']['loaded'] and not s['sftp_profiles']['busy']
        def fill(a,host):
            for i,value in enumerate([host,'tester','0022','/Fotos ñ/%41/$()']):
                a.act('sftp-key:ctrl'+str(i+1));a.act('sftp-key:all');a.act('sftp-input:'+value)
        def label(a,value):a.act('sftp-key:all');a.act('sftp-input:'+value)
        try:
            a=App('profiles-create',source,base,config,source)
            open_form(a);fill(a,'Work-Alias');a.act('sftp-review')
            a.act('sftp-key:profiles',idle);label(a,'Trabajo ñ')
            s=a.act('sftp-key:save',lambda s:idle(s) and len(s['sftp_profiles']['rows'])==1)
            first=s['sftp_profiles']['rows'][0];assert first['fields']==['Work-Alias','tester','0022','/Fotos ñ/%41/$()']
            assert s['sftp']['review']=='tester@Work-Alias:/Fotos ñ/%41/$()' and bytes(s['path_raw'])==path
            assert not s['sftp_transport']['open'] and s['sftp_transport']['point']is None
            a.capture('sftp-profiles-saved-no-connect')
            a.act('sftp-profile-save',idle);assert len(a.state()['sftp_profiles']['rows'])==1
            a.act('sftp-profile-back');fill(a,'Other-Alias');a.act('sftp-profiles',idle);label(a,'Otro servidor')
            a.act('sftp-profile-save',lambda s:idle(s) and len(s['sftp_profiles']['rows'])==2)
            a.act('sftp-key:down');a.act('sftp-request-paste');s=a.act('sftp-profile-load')
            assert s['sftp']['fields'][0]=='Other-Alias' and s['sftp']['review']is None and not s['sftp']['profiles']
            a.act('sftp-late-paste');a.capture('sftp-profile-load-requires-review')
            a.act('sftp-profiles',idle);a.act('sftp-key:up');a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
            a.capture('sftp-profiles-initial-440')
            a.close();stored=file.read_bytes();assert file.stat().st_mode&0o777==0o600
            b=App('profiles-restore',source,base,config,source,size='440x440');open_form(b)
            assert b.state()['sftp']['fields']==['','','','']
            b.act('sftp-profiles',idle);s=b.act('sftp-key:review')
            assert s['sftp']['fields']==first['fields'] and s['sftp']['review']is None and not s['sftp_transport']['open']
            b.capture('sftp-profile-restored-draft-440');b.act('sftp-profiles',idle)
            lock=(config/'sftp-profiles.lock').open('rb+')
            fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            s=b.act('sftp-profile-remove',lambda s:not s['sftp_profiles']['busy'] and bool(s['sftp_profiles']['error']))
            assert len(s['sftp_profiles']['rows'])==2 and file.read_bytes()==stored
            b.capture('sftp-profiles-busy-preserves-data-440')
            fcntl.flock(lock,fcntl.LOCK_UN);lock.close()
            s=b.act('sftp-profile-refresh',lambda s:idle(s) and len(s['sftp_profiles']['rows'])==1)
            assert s['sftp_profiles']['rows'][0]['fields'][0]=='Other-Alias'
            assert s['sftp']['fields']==first['fields'] # ACK did not apply other draft
            b.capture('sftp-profiles-exact-retry-440');b.close()
            valid=file.read_bytes();future=valid[:8]+b'\x02\x00'+valid[10:];file.write_bytes(future)
            c=App('profiles-future',source,base,config,source,size='440x440');open_form(c)
            s=c.act('sftp-profiles',lambda s:not s['sftp_profiles']['busy'] and bool(s['sftp_profiles']['error']))
            assert not s['sftp_profiles']['loaded'] and file.read_bytes()==future
            c.capture('sftp-profiles-future-preserved-440');c.close();assert file.read_bytes()==future
            assert sha(source/'owned.txt')==digest and sorted(p.name for p in source.iterdir())==['owned.txt']
            assert not (config/'sftp').exists() # no actor connection registry or network request
            report.update(functional_gate='PASS',normal_exits=3,network_gate='NOT_EXERCISED',scope='Local connection drafts only: save/reopen/load explicit, exact fields/Unicode/port spelling, no auto-review/connect, duplicate target, shared file0600, lock conflict preserves file/explicit retry removes exact entry without changing draft, future file untouched, stale paste rejected, source intact,440 controls,3root0ACK. Painted handlers/keys/private command pipe and real FLTK framebuffer; no OS input, SSH host, credentials, mount, resources or startup experiment.')
        except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
        finally:
            report['logs']={app.label:app.log.read_text()for app in apps}
            for app in apps:
                if app.process.poll()is None:
                    os.killpg(app.process.pid,signal.SIGTERM)
                    try:app.process.wait(timeout=3)
                    except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
            for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
            path=OUT/('sftp-profiles-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)


def reports():
    with tempfile.TemporaryDirectory(prefix='sftp-report-',dir=BASE) as temp:
        base=Path(temp);config=base/'config';config.mkdir();source=base/'Source';source.mkdir();(source/'owned.txt').write_bytes(b'unchanged owned report fixture')
        digest=sha(source/'owned.txt');path=os.fsencode(source)
        def open_form(a):
            a.act('clip-actions');return a.act('sftp-open',lambda s:s['sftp']is not None and s['sftp']['capability_done'] and not s['sftp_transport']['busy'])
        try:
            a=App('sftp-report',source,base,config,source);open_form(a)
            for i,value in enumerate(['Draft-Alias','tester','0022','/Fotos ñ/%41/$()']):
                a.act('sftp-key:ctrl'+str(i+1));a.act('sftp-key:all');a.act('sftp-input:'+value)
            s=a.act('sftp-review');fields=s['sftp']['fields'];review=s['sftp']['review']
            a.act('sftp-fixture-long-diagnostic');a.act('sftp-request-paste');s=a.act('sftp-key:report')
            r=s['sftp_report'];assert r and r['painted'] and not r['truncated'] and r['total']>r['rows']
            text=r['text'];assert 'BEGIN-OWNED'in text and 'END-OWNED'in text and chr(92)+'u{1b}'in text and chr(27)not in text
            assert 'no identifica una conexión activa'in text and '0022'in text and '/Fotos ñ/%41/$()'in text
            assert not s['sftp_transport']['busy'] and bytes(s['path_raw'])==path
            a.act('sftp-late-paste');a.capture('sftp-report-overview')
            a.act('sftp-input:MUST-NOT-EDIT');s=a.state();assert s['sftp']['fields']==fields
            s=a.act('sftp-key:page-down');assert s['sftp_report']['scroll']>0;a.capture('sftp-report-page-down')
            a.act('sftp-key:home');seen=[];previous=-1
            for page in range(64):
                s=a.state();r=s['sftp_report'];seen.extend(r['visible']);assert r['scroll']>previous;previous=r['scroll']
                if r['scroll']+r['rows']>=r['total']:break
                a.act('sftp-report-next')
            else:raise AssertionError('bounded report traversal did not reach end')
            assert 'BEGIN-OWNED'in '\n'.join(seen) and 'END-OWNED'in '\n'.join(seen)
            assert r['scroll']+r['rows']==r['total'];a.capture('sftp-report-end')
            s=a.act('sftp-report-copy');assert s['sftp_report']['copy_requests']==1 and s['sftp_report']['text']==text
            old=s['sftp_report']['id'];s=a.act('sftp-fixture-new-diagnostic');assert s['sftp_report']['text']==text
            s=a.act('sftp-report-refresh');assert s['sftp_report']['id']!=old and 'NEW-OBSERVATION-OWNED'in s['sftp_report']['text'] and 'END-OWNED'not in s['sftp_report']['text']
            assert not s['sftp_transport']['busy'] and s['sftp']['fields']==fields and s['sftp']['review']==review
            s=a.act('sftp-report-old-copy:'+str(old));assert s['sftp_report']['copy_requests']==0
            a.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440);a.capture('sftp-report-refreshed-440')
            s=a.act('sftp-key:end');assert s['sftp_report']['scroll']+s['sftp_report']['rows']==s['sftp_report']['total']
            s=a.act('sftp-key:copy-report');assert s['sftp_report']['copy_requests']==1;a.capture('sftp-report-copy-request-440')
            s=a.act('sftp-report-back');assert s['sftp_report']is None and s['sftp']['fields']==fields and s['sftp']['review']==review
            a.capture('sftp-report-return-preserves-form-440');s=a.act('sftp-report');assert s['sftp_report']['copy_requests']==0;a.close()
            b=App('sftp-report-initial-small',source,base,config,source,size='440x440');open_form(b)
            b.act('sftp-report');b.capture('sftp-report-initial-440');b.close()
            assert sha(source/'owned.txt')==digest and sorted(p.name for p in source.iterdir())==['owned.txt']
            assert not (config/'sftp').exists()
            report.update(functional_gate='PASS',normal_exits=2,network_gate='NOT_EXERCISED',copy_gate='REQUEST_HANDLER_ONLY_OS_BYTES_UNVALIDATED',scope='SFTP report capture only: long own diagnostic including Unicode/controls wrapped and all BEGIN-END content reached by pages, fixed source despite transport change, explicit refresh with no transport dispatch, exact request body retained, oldcopyid rejected, form/review/raw literal preserved, stale paste/read-only edits rejected,440 controls/return/reopen/root0ACK.7realFLTKframebuffers/painted handlers/private pipe, no nativeinput/clipboardbytes/SSH/mount/resources/startup/active-helper-close/lifetimes.')
        except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
        finally:
            report['logs']={app.label:app.log.read_text()for app in apps}
            for app in apps:
                if app.process.poll()is None:
                    os.killpg(app.process.pid,signal.SIGTERM)
                    try:app.process.wait(timeout=3)
                    except subprocess.TimeoutExpired:os.killpg(app.process.pid,signal.SIGKILL);app.process.wait()
            for key,name in [('binary_sha256','files'),('helper_sha256','files-helper'),('html_helper_sha256','files-html-helper')]:report[key]=sha(ROOT/'target/release'/name)
            path=OUT/('sftp-report-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)

parser=argparse.ArgumentParser(description=__doc__);group=parser.add_mutually_exclusive_group();group.add_argument('--profiles',action='store_true');group.add_argument('--report',action='store_true');args=parser.parse_args()
reports() if args.report else profiles() if args.profiles else preparation()
