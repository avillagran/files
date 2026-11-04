"""UI extension for the existing explicit loopback laboratory; inert on import.
Painted production handlers and real framebuffer, never native input or a resource benchmark.
"""
import hashlib, json, os, re, shlex, subprocess, time
from pathlib import Path

def copy_policy_ui(app,local,remote,mount,expect):
    """Own local/remote fixtures; invokes production review/options/queue only."""
    def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
    external=remote/'ui-existing-link-target';external.write_bytes(b'existing remote target')
    os.utime(external,ns=(1000000000000,2000000000000));external_before=external.stat().st_mtime_ns
    (local/'a.txt').write_bytes(b'GUI SFTP payload\x00\xff');(local/'a.txt').chmod(0o640)
    os.utime(local/'a.txt',ns=(3000000000000,4000000000000))
    raw=b'raw-\xff.txt';raw_path=Path(os.fsdecode(os.fsencode(local)+b'/'+raw));raw_path.write_bytes(b'opaque filename payload')
    (local/'tree').mkdir();(local/'tree/child').write_bytes(b'nested GUI payload')
    (local/'link').symlink_to(external)
    originals={name:digest(local/name) for name in ['a.txt','tree/child',os.fsdecode(raw)]}
    for name in ['ui-default','ui-link-default','ui-link-blocked','ui-visible','ui-cancel']:(remote/name).mkdir()
    app.act('refresh',lambda s:s['bookmarks']['browser_loaded'] and len(s['order'])==4)
    def selected(name):app.act('clip-select:'+os.fsencode(name).hex());app.act('dest-copy-selected')
    def destination(name):app.act('dest-text:'+str(mount/name))
    def choose(omit):
        app.act('dest-options',lambda s:s['destination_editor']['options_open'])
        app.act('dest-visible');
        if omit:app.act('dest-key:symlink-times')
    def review():app.act('dest-options',lambda s:not s['destination_editor']['options_open'])
    def complete(old,ok):return lambda s:s['destination_queue_idle'] and s['copy_result'] is not None and s['copy_result']['id']>old and s['copy_result']['ok']==ok
    def start_done(ok):
        state=app.state();previous=(state.get('copy_result')or{}).get('id',0)
        return app.act('dest-start',complete(previous,ok))
    def dismiss():app.act('dest-dismiss')
    selected('a.txt');destination('ui-default')
    value=start_done(False)['copy_result'];expect(value['publication']=='StagedRename'and value['completed']==0,'default copy silently changed publication')
    expect(not list((remote/'ui-default').iterdir()),'default failure left unpublished root')
    app.capture('sftp-ui-default-publication-rejected');dismiss()
    selected('link');destination('ui-link-default')
    value=start_done(False)['copy_result'];expect('FUSE symlink' in value['cause'] and value['symlink_times_omitted']==0,'unsafe FUSE symlink setter not blocked')
    expect(not list((remote/'ui-link-default').iterdir())and external.stat().st_mtime_ns==external_before,'default link mutated target')
    dismiss()
    selected('link');destination('ui-link-blocked');choose(False);review()
    value=start_done(False)['copy_result'];expect(value['publication']=='VisibleExclusive'and value['symlink_times_omitted']==0,'visible link silently omitted timestamps')
    expect(not list((remote/'ui-link-blocked').iterdir())and external.stat().st_mtime_ns==external_before,'blocked visible link mutated target')
    dismiss()
    app.act('dest-copy-all');destination('ui-visible');choose(True)
    app.capture('sftp-copy-options-visible-omission')
    app.act('fixture-minimum',lambda s:s['width']==440 and s['height']==440)
    app.capture('sftp-copy-options-initial-minimum');review();app.capture('sftp-copy-review-visible-minimum')
    app.act('fixture-wide',lambda s:s['width']==1000 and s['height']==560)
    previous=app.state()['copy_result']['id'];app.act('dest-start-paused')
    app.act('dest-start',lambda s:s['destination_editor'] is None and s['copy_progress']['phase']=='Paused')
    expect(not list((remote/'ui-visible').iterdir()),'paused review mutated destination')
    app.capture('sftp-copy-visible-paused')
    value=app.act('dest-resume',complete(previous,True))['copy_result']
    expect(value['publication']=='VisibleExclusive'and value['completed']==4 and value['visible_completed']==4 and value['symlink_times_omitted']==1,'queue lost reviewed policies')
    destination_root=remote/'ui-visible'
    for name,checksum in originals.items():expect(digest(destination_root/name)==checksum and digest(local/name)==checksum,'GUI copy byte mismatch')
    expect(os.readlink(destination_root/'link')==str(external),'GUI link payload changed')
    expect((destination_root/'a.txt').stat().st_mode&0o777==0o640,'GUI rwx not preserved')
    expect((destination_root/'a.txt').stat().st_mtime_ns==(local/'a.txt').stat().st_mtime_ns,'GUI mtime not preserved')
    expect(external.stat().st_mtime_ns==external_before,'omitted link dates changed existing target')
    app.act('dest-details');app.capture('sftp-copy-visible-result-report');dismiss()
    selected('a.txt');destination('ui-visible');choose(True);review()
    collision=start_done(False)['copy_result']
    expect(collision['completed']==0 and digest(destination_root/'a.txt')==originals['a.txt'],'collision overwrote destination')
    app.capture('sftp-copy-visible-no-clobber');dismiss()
    bulk=local/'bulk';bulk.mkdir()
    for i in range(3000):(bulk/f'{i:05d}').write_bytes(i.to_bytes(4,'little')+b'x'*508)
    app.act('refresh',lambda s:s['bookmarks']['browser_loaded'] and len(s['order'])==5)
    selected('bulk');destination('ui-cancel');choose(True);review()
    previous=app.state()['copy_result']['id']
    app.act('dest-start',lambda s:s['copy_progress']['phase']=='Running'and (s['copy_progress']['entries']or 0)>0)
    value=app.act('dest-cancel-active',complete(previous,False))['copy_result']
    expect(value['cancelled']and value['publication']=='VisibleExclusive'and value['visible_started']==1,'active visible copy not cancelled')
    partial=remote/'ui-cancel/bulk';expect(partial.is_dir(),'visible partial was removed automatically')
    copied=list(partial.iterdir());expect(0<len(copied)<3000,'partial coverage not observed')
    for path in copied:expect(path.read_bytes()==(bulk/path.name).read_bytes(),'retained partial bytes differ')
    expect(len(list(bulk.iterdir()))==3000 and all((bulk/f'{i:05d}').read_bytes()==i.to_bytes(4,'little')+b'x'*508 for i in range(3000)),'cancel changed source')
    expect(not any(p.name.startswith('.files-copy-')for p in (remote/'ui-cancel').iterdir()),'visible cancel left hidden staging')
    app.capture('sftp-copy-visible-cancel-partial');dismiss()
    return {'gate':'PASS','successful_roots':4,'raw_filename':raw.hex(),'symlink_dates_omitted':1,'external_target_mtime_unchanged':True,'cancelled_partial_entries':len(copied),'source_bulk_entries':3000,'staged_default_rejected':True,'unsafe_fuse_link_dates_rejected':True,'visible_link_dates_not_silently_omitted':True,'collision_no_clobber':True}

def recovered_ui(root, source, point, server_pid, probe, remember, expect, out, sshfs, ssh_config, alias, port, copy_policy=False):
    repository = Path(__file__).resolve().parent.parent
    result = {'gate':'IN_PROGRESS', 'method':'production transport actor + real SSHFS/FUSE, injected painted handlers/framebuffer; NO OS input', 'captures':[], 'states':[]}
    apps=[];busy=None;reaped=False;allow_reap=False;fresh_point=None
    def service():
        nonlocal reaped
        remember()
        if allow_reap and not reaped:
            pid,status=os.waitpid(server_pid,os.WNOHANG)
            if pid:
                expect(os.waitstatus_to_exitcode(status)==0,'GUI disconnect SSHFS exit failed');reaped=True
    class App:
        def __init__(self,label,path_override=None,folder=None):
            self.seq=0;self.raw=root/(label+'.ppm');self.log=root/(label+'.log');self.label=label
            env={**os.environ,'FILES_CONFIG_DIR':str(root.parent),'FILES_TEST_CAPTURE':str(self.raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
            if path_override is not None:env['PATH']=path_override
            with self.log.open('w') as output:
                self.process=subprocess.Popen([str(repository/'target/release/files'),str(source if folder is None else folder),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','120'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
            apps.append(self)
            self.state(lambda s:s['bookmarks']['browser_loaded'])
            self.act('list');self.act('clip-actions');self.act('sftp-open',lambda s:s['sftp'] is not None and s['sftp']['capability_done'])
        def wait(self,check,why,exits=False):
            end=time.monotonic()+25
            while time.monotonic()<end:
                service();value=check()
                if value:return value
                if self.process.poll() is not None and not exits:raise AssertionError(why+' exit '+str(self.process.returncode))
                time.sleep(.02)
            raise AssertionError(why)
        def state(self,predicate=lambda s:True):
            def check():
                states=[]
                for line in self.log.read_text().splitlines():
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        editor=s.get('destination_editor')
                        policy_painted=editor is None or editor.get('policy_controls')==(3 if editor['options_open'] else 1)
                        if s['seq']==self.seq and policy_painted and (s.get('sftp') is None or s['sftp']['hit_count']==12) and predicate(s):states.append(s)
                return states[-1] if states else None
            value=self.wait(check,'SFTP painted state '+str(self.seq));result['states'].append(value);return value
        def command(self,name,exits=False):
            offset=len(self.log.read_text());pending=self.raw.with_suffix('.pending');pending.write_text(name);pending.replace(self.raw.with_suffix('.command'))
            match=self.wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(name)+r'$',self.log.read_text()[offset:],re.M),'SFTP command '+name,exits)
            self.seq=int(match[1])
        def act(self,name,predicate=lambda s:True):self.command(name);return self.state(predicate)
        def capture(self,label):
            state=self.state();dims=(state['width'],state['height'])
            self.raw.unlink(missing_ok=True);self.raw.with_suffix('.request').touch()
            def check():
                if not self.raw.exists():return False
                parts=self.raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and parts[1]==f'{dims[0]} {dims[1]}'.encode() and len(parts[3])==dims[0]*dims[1]*3
            self.wait(check,'SFTP framebuffer incomplete')
            destination=out/(label+'-'+str(time.time_ns())+'.png')
            subprocess.run(['magick',str(self.raw),str(destination)],check=True,timeout=15)
            result['captures'].append({'path':str(destination),'dimensions':list(dims)})
        def close(self):
            self.command('callback',True);self.wait(lambda:self.process.poll() is not None,'SFTP root close ACK',True)
            expect(self.process.returncode==0 and 'FILES_CLOSE_DRAINED' in self.log.read_text(),'GUI root did not close after ACK')
    try:
        first=App('recovered-ui-one')
        first.state(lambda s:'Registrado' in s['sftp_transport']['phase'] and not s['sftp_transport']['busy'])
        first.capture('sftp-ui-registered')
        first.act('sftp-reconcile',lambda s:s['sftp_transport']['phase']=='Recuperada · Retained' and not s['sftp_transport']['busy'])
        first.capture('sftp-ui-explicit-control')
        first.act('sftp-mount-open',lambda s:s['sftp'] is None and bytes(s['path_raw'])==os.fsencode(point) and s['bookmarks']['browser_loaded'])
        first.capture('sftp-ui-open-verified-mount')
        expect((point/'alpha.txt').read_bytes()==(source/'alpha.txt').read_bytes(),'opened wrong mount')
        first.act('clip-actions');first.act('sftp-open',lambda s:s['sftp'] is not None and s['sftp']['capability_done'])
        before=probe();busy=os.open(point/'alpha.txt',os.O_RDONLY|os.O_CLOEXEC)
        first.act('sftp-disconnect',lambda s:s['sftp_transport']['phase']=='Recuperada · Retained' and bool(s['sftp_transport']['diagnostic']) and not s['sftp_transport']['busy'])
        expect(probe()==before,'GUI busy disconnect changed mount identity')
        first.capture('sftp-ui-busy-retained');first.close()
        expect(probe()==before and (point/'alpha.txt').read_bytes()==(source/'alpha.txt').read_bytes(),'GUI close did not retain serving mount')
        os.close(busy);busy=None
        second=App('recovered-ui-two')
        second.state(lambda s:'Registrado' in s['sftp_transport']['phase'] and not s['sftp_transport']['busy'])
        allow_reap=True
        second.act('sftp-disconnect',lambda s:s['sftp_transport']['phase']=='Recuperada · Absent' and not s['sftp_transport']['busy'])
        expect(reaped and not probe()['present'],'GUI claimed absent before server reap/mount absence')
        second.capture('sftp-ui-disconnected');second.close()
        wrapper=root.parent/'private-tools';wrapper.mkdir(mode=0o700)
        executable=wrapper/'sshfs'
        executable.write_text('#!/bin/sh\nexec '+shlex.quote(str(sshfs))+' -F '+shlex.quote(str(ssh_config))+' "$@"\n');executable.chmod(0o700)
        copy_source=root.parent/'copy-source'
        if copy_policy:copy_source.mkdir(mode=0o700)
        third=App('fresh-ui',str(wrapper)+':/usr/bin:/bin',copy_source if copy_policy else None)
        third.state(lambda s:s['sftp']['tools_ready'] and not s['sftp_transport']['busy'])
        third.act('sftp-input:'+alias);third.act('sftp-focus:2');third.act('sftp-input:'+str(port));third.act('sftp-focus:3');third.act('sftp-input:'+str(source));third.act('sftp-review')
        value=third.act('sftp-connect',lambda s:s['sftp_transport']['phase']=='Ready' and s['sftp_transport']['open'] and not s['sftp_transport']['busy'])
        fresh_point=Path(os.fsdecode(bytes(value['sftp_transport']['point'])))
        expect(fresh_point.parent==root and fresh_point!=point,'GUI new point not generated in private root')
        third.capture('sftp-ui-fresh-connected')
        third.act('sftp-mount-open',lambda s:s['sftp'] is None and bytes(s['path_raw'])==os.fsencode(fresh_point) and s['bookmarks']['browser_loaded'])
        expect((fresh_point/'alpha.txt').read_bytes()==(source/'alpha.txt').read_bytes(),'fresh GUI mount bytes differ')
        third.capture('sftp-ui-fresh-open')
        if copy_policy:
            third.act('history-back',lambda s:bytes(s['path_raw'])==os.fsencode(copy_source) and s['bookmarks']['browser_loaded'])
            result['copy_policy']=copy_policy_ui(third,copy_source,source,fresh_point,expect)
        third.act('clip-actions');third.act('sftp-open',lambda s:s['sftp'] is not None and s['sftp']['capability_done'])
        third.state(lambda s:not s['sftp_transport']['busy'])
        third.act('sftp-disconnect',lambda s:s['sftp_transport']['phase']=='Absent' and not s['sftp_transport']['busy'])
        verified=subprocess.run([str(repository/'target/debug/examples/mount-table-probe'),'--point',str(fresh_point),fresh_point.name],capture_output=True,text=True,check=True,timeout=3)
        expect(not json.loads(verified.stdout)['present'],'fresh GUI mount still present after Absent')
        third.capture('sftp-ui-fresh-disconnected');third.close()
        result['fresh_connect']={'gate':'PASS','point':str(fresh_point),'private_wrapper_only':'exec actual SSHFS with fixture -F then exact product argv; no product override or global install'}
        result.update(gate='PASS',normal_exits=3,original_server_reaped_by='private parent harness, never product controller',server_reaped=reaped)
        return result
    except BaseException as error:
        result.update(gate='FAIL',error=repr(error))
        raise
    finally:
        if busy is not None:os.close(busy)
        result['logs']={a.label:a.log.read_text() for a in apps}
        for a in apps:
            if a.process.poll() is None:
                a.process.terminate()
                try:a.process.wait(timeout=3)
                except subprocess.TimeoutExpired:a.process.kill();a.process.wait(timeout=3)
        if fresh_point is not None:
            check=subprocess.run([str(repository/'target/debug/examples/mount-table-probe'),'--point',str(fresh_point),fresh_point.name],capture_output=True,text=True,timeout=3)
            if check.returncode==0 and json.loads(check.stdout)['present']:
                unmount=subprocess.run(['/usr/bin/fusermount3','-u',str(fresh_point)],capture_output=True,text=True,timeout=5)
                result['fresh_cleanup']={'explicit_ordinary_unmount_exit':unmount.returncode}
        (out/('sftp-ui-control-'+str(time.time_ns())+'.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
