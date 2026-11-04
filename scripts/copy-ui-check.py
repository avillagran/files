#!/usr/bin/env python3
"""Own fixtures, UI handlers/framebuffer; capture and measured tree runs separate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from memcheck import Metrics, geometry, observed_geometry, stop_tree
from process_metrics import TreeSampler, read_stat

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--capture', action='store_true')
parser.add_argument('--moves', action='store_true')
parser.add_argument('--exdev', action='store_true')
parser.add_argument('--archives', action='store_true')
parser.add_argument('--archive-recovery', action='store_true')
parser.add_argument('--create-archives', action='store_true')
args = parser.parse_args()
evidence = ROOT/'docs/evidence/0.0.4'
evidence.mkdir(parents=True, exist_ok=True)
base = Path.home()/'.cache/files-test'
base.mkdir(parents=True, exist_ok=True)
def exdev_check():
    """Extends the existing copy UI harness; no resource/startup sampling."""
    if not args.capture:raise SystemExit('--exdev requires --capture; resource/startup freeze remains')
    report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED_FREEZE','startup_gate':'NOT_MEASURED_FREEZE','captures':[],'cases':[],
            'scope':'single regular file nlink1, real EXDEV and explicit recovery in app after process exit73',
            'method':'existing internal Actions/handlers + real FLTK framebuffer; no native OS input; no powerloss/SIGKILL/failed fsync syscall'}
    runtime=Path(os.environ['XDG_RUNTIME_DIR'])
    core_tests=sorted((ROOT/'target/debug/deps').glob('files_core-*'),key=lambda p:p.stat().st_mtime_ns,reverse=True)
    core_test=next(p for p in core_tests if p.is_file() and os.access(p,os.X_OK))
    process=None;normal=False;case=None;log=None
    def fingerprint(path):
        st=path.lstat();fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NOATIME)
        try:
            digest=hashlib.sha256()
            while True:
                chunk=os.read(fd,65536)
                if not chunk:break
                digest.update(chunk)
        finally:os.close(fd)
        return {'mode':st.st_mode,'uid':st.st_uid,'gid':st.st_gid,'mtime_ns':st.st_mtime_ns,'atime_ns':st.st_atime_ns,'sha256':digest.hexdigest(),
                'xattrs':{n:os.getxattr(path,n).hex() for n in os.listxattr(path)}}
    work_tmp=tempfile.TemporaryDirectory(prefix='exdev-ui-',dir=base)
    remote_tmp=tempfile.TemporaryDirectory(prefix='files-exdev-ui-',dir=runtime)
    try:
        work=Path(work_tmp.name);remote=Path(remote_tmp.name)
        for name in ['normal','restore-gap','publish-gap']:
            root=work/name;source=root/'source';target=remote/name;config=root/'config';journals=config/'operations'
            for directory in [source,target,journals,root/'tmp']:directory.mkdir(parents=True,mode=0o700)
            journals.chmod(0o700)
            (source/'item').write_bytes(('fixture de movimiento ñ\n'*1024).encode());(source/'item').chmod(0o640)
            stamp=1_700_000_000_000_000_000;os.utime(source/'item',ns=(stamp,stamp))
            try:os.setxattr(source/'item','user.files-exdev',b'preserve-exact')
            except OSError:pass
            (source/'untouched.txt').write_bytes(b'untouched own fixture')
            original=fingerprint(source/'item')
            source_dev=source.stat().st_dev;target_dev=target.stat().st_dev;assert source_dev!=target_dev
            case={'name':name,'source_st_dev':source_dev,'destination_st_dev':target_dev,'states':[],'original':original}
            report['cases'].append(case)
            if name!='normal':
                seeded=subprocess.run([str(core_test),'--exact','exdev_move::linux::tests::recovery_subprocess_actor','--ignored','--nocapture'],
                    env={**os.environ,'TMPDIR':str(root/'tmp'),'FILES_EXDEV_FIXTURE_SOURCE':str(source),'FILES_EXDEV_FIXTURE_DESTINATION':str(target),
                    'FILES_EXDEV_FIXTURE_JOURNALS':str(journals),'FILES_EXDEV_FIXTURE_ACTION':name},stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=20)
                case['fixture_process_exit']=seeded.returncode;case['fixture_process_log']=(seeded.stdout+seeded.stderr).decode(errors='replace');assert seeded.returncode==73
            raw=root/'frame.ppm';log=root/'app.log';env={**os.environ,'TMPDIR':str(root/'tmp'),'MAGICK_TEMPORARY_PATH':str(root/'tmp'),'FILES_CONFIG_DIR':str(config),
                'FILES_TEST_COPY':'1','FILES_TEST_MOVE':'1','FILES_TEST_MOVE_DESTINATION':str(target),'FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_CAPTURE':str(raw),
                'FILES_CLOSE_TRACE':'1','FILES_TEST_FIXED_SIZE':'1'}
            initial_size=(440,440) if name=='restore-gap' else (1000,650)
            case['initial_window_size']=list(initial_size);case['live_resize_gate']='UNVALIDATED_NOT_CLAIMED'
            with log.open('w') as out:process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--size',f'{initial_size[0]}x{initial_size[1]}','--scale','1','--no-session','--ui-smoke','--ui-scene','copy','--exit-after','100'],stdout=subprocess.DEVNULL,stderr=out,env=env,start_new_session=True)
            normal=False;seq=0
            def wait(check,why,allow_exit=False):
                deadline=time.monotonic()+20
                while time.monotonic()<deadline:
                    value=check()
                    if value:return value
                    if process.poll() is not None and not allow_exit:raise AssertionError(why+' app exit='+str(process.returncode))
                    time.sleep(.01)
                raise AssertionError(why)
            def command(action):
                nonlocal seq
                after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
                wait(lambda:'FILES_COPY_COMMAND_OK '+action+'\n' in log.read_text()[after:],'command '+action,action=='close');seq+=1;return after
            def state(after=0,condition=lambda s:True):
                def read():
                    for line in reversed(log.read_text()[after:].splitlines()):
                        if line.startswith('FILES_MOVE_STATE '):
                            s=json.loads(line[len('FILES_MOVE_STATE '):])
                            if s['seq']==seq and condition(s):return s
                    return False
                s=wait(read,'fresh state seq='+str(seq));case['states'].append(s);return s
            def act(action,condition=lambda s:True):return state(command(action),condition)
            def capture(label,expected=None):
                if expected is None:expected=initial_size
                raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
                def ready():
                    if not raw.exists():return False
                    p=raw.read_bytes().split(b'\n',3)
                    if len(p)!=4:return False
                    dimensions=tuple(map(int,p[1].split()))
                    if len(p[3])!=dimensions[0]*dimensions[1]*3:return False
                    assert dimensions==expected,('framebuffer size',dimensions,'expected',expected)
                    return True
                wait(ready,'framebuffer '+label);p=evidence/(label+'-'+str(time.time_ns())+'.png')
                subprocess.run(['magick',str(raw),str(p)],env=env,check=True,timeout=10);report['captures'].append(str(p));case.setdefault('capture_dimensions',{})[str(p)]=list(expected)
            wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'smoke');wait(lambda:geometry(process.pid,*initial_size) and observed_geometry(process.pid,*initial_size),'geometry');state(condition=lambda s:s['window']==list(initial_size) and s['canvas']==list(initial_size) and s['painted']==list(initial_size))
            if name=='normal':
                act('move-exdev-form');act('move-review',lambda s:s['form'] and s['form']['prepared'] and s['form']['exdev'] and not s['review_pending'])
                assert fingerprint(source/'item')==original and not (target/'item').exists();capture('exdev-file-review')
                act('move-confirm',lambda s:s['phase']=='Paused');assert fingerprint(source/'item')==original and not (target/'item').exists();capture('exdev-paused-global-queue')
                done=act('resume',lambda s:s['outcome'] and s['outcome']['success'] and s['outcome']['exdev'] and s['outcome']['completed']==1)
                assert not (source/'item').exists() and fingerprint(target/'item')==original
                assert done['recovery']['actions']==[False,False]
                act('move-exdev-target-tab',lambda s:s['path']==str(target) and 'item'.encode().hex() in s['entries']);capture('exdev-result-target')
                act('move-source-tab',lambda s:s['path']==str(source) and 'item'.encode().hex() not in s['entries'])
            else:
                act('move-recovery-list',lambda s:s['recovery']['open'] and not s['recovery']['loading'] and s['recovery']['entries']==1);capture('exdev-interrupted-'+name+'-catalog')
                selected=act('move-recovery-select',lambda s:s['notice'] and not s['recovery']['open'])
                assert selected['recovery']['actions']==([False,True] if name=='restore-gap' else [True,True]);capture('exdev-interrupted-'+name+'-details')
                if name=='restore-gap':
                    case['narrow_paint']=selected
                    assert selected['window']==selected['canvas']==selected['painted']==[440,440]
                    capture('exdev-restore-controls-initial-440',(440,440))
                action='move-recovery-restore' if name=='restore-gap' else 'move-recovery-finish'
                act(action,lambda s:s['outcome'] and s['outcome']['success'] and s['outcome']['exdev'])
                if name=='restore-gap':assert fingerprint(source/'item')==original
                else:assert not (source/'item').exists()
                assert fingerprint(target/'item')==original;capture('exdev-recovered-'+name+'-result',(440,440) if name=='restore-gap' else (1000,650))
            assert not any(p.name.startswith('.files-move-') for p in source.iterdir())
            assert not any(p.name.startswith('.files-move-') for p in target.iterdir())
            after=command('close');wait(lambda:process.poll() is not None,'normal close',True)
            assert process.returncode==0 and 'FILES_CLOSE_DRAINED' in log.read_text()[after:]
            case['normal_exit0_after_ack']=True;case['stages_absent']=True;case['log']=log.read_text();normal=True;process=None
        report['functional_gate']='PASS'
    except BaseException as error:
        report['error']=repr(error)
        if case is not None and log and log.exists():case['log']=log.read_text()
        raise
    finally:
        if process and not normal:stop_tree(process,None)
        work_tmp.cleanup();remote_tmp.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
        p=evidence/('exdev-ui-'+str(time.time_ns())+'.json')
        with p.open('x') as out:json.dump(report,out,indent=2)
        print(json.dumps({'report':str(p),'functional_gate':report['functional_gate']}))

if args.exdev:
    exdev_check()
    raise SystemExit(0)


def archive_check():
    """ZIP32 review/apply over the existing worker; no resource experiments."""
    if not args.capture:raise SystemExit('--archives requires --capture; resource/startup freeze remains')
    import io,stat,zipfile
    report=dict(functional_gate='FAIL',resource_gate='NOT_MEASURED',startup_gate='NOT_MEASURED',captures=[],states=[],
        method='existing copy/move fixture, painted pointer handlers and real FLTK framebuffer; no native input',
        scope='ZIP32 regular files/directories into a new exclusive child; no links/ZIP64/TAR/creation/crash journal')
    process=None;log=None;normal=False
    temporary=tempfile.TemporaryDirectory(prefix='archive-ui-',dir=base)
    try:
        root=Path(temporary.name);source=root/'source';target=root/'target'
        for name in ['source','target','tmp','config']:(root/name).mkdir()
        def package(entries):
            stream=io.BytesIO()
            with zipfile.ZipFile(stream,'w',compression=zipfile.ZIP_DEFLATED)as z:
                for name,data,mode in entries:
                    info=zipfile.ZipInfo(name);info.create_system=3;info.external_attr=mode<<16
                    z.writestr(info,data,compress_type=zipfile.ZIP_DEFLATED)
            return stream.getvalue()
        contents={'docs/message.txt':'Hola, teclado y mouse. 漢字\r\n'.encode(),'run.sh':b'#!/bin/sh\necho owned\n','raw-X':b'raw fixture'}
        data=package([(n,b,0o100755 if n=='run.sh'else 0o100600)for n,b in contents.items()]+[('empty/',b'',0o040755)])
        data=data.replace(b'raw-X',b'raw-\xff');(source/'source.zip').write_bytes(data)
        bad=bytearray(package([('bad.txt',b'crc must reject',0o100600)]));central=bad.find(b'PK\x01\x02');crc=int.from_bytes(bad[14:18],'little')^1;bad[14:18]=crc.to_bytes(4,'little');bad[central+16:central+20]=crc.to_bytes(4,'little');(source/'corrupt.zip').write_bytes(bad)
        (source/'unsafe.zip').write_bytes(package([('../escape',b'no escape',0o100600)]));(target/'untouched.txt').write_bytes(b'untouched')
        if args.create_archives:
            (source/'Pack/empty').mkdir(parents=True)
            (source/'Pack/message-漢字.txt').write_bytes(('Hola, archivos. 漢字\r\n'*12000).encode())
            (source/'Pack/run.sh').write_bytes(b'#!/bin/sh\necho owned\n');(source/'Pack/run.sh').chmod(0o751);os.utime(source/'Pack/run.sh',ns=(1700000000000000000,1700000000000000000))
            (source/'zero').write_bytes(b'');(source/os.fsdecode(b'raw-\xff')).write_bytes(b'raw bytes')
            (source/'link').symlink_to('../target/untouched.txt')
        def source_fingerprints():
            return {str(p.relative_to(source)):('link',os.readlink(p))if p.is_symlink()else('file',hashlib.sha256(p.read_bytes()).hexdigest(),p.stat().st_mode&0o777)for p in source.rglob('*')if p.is_symlink()or p.is_file()}
        original=source_fingerprints()
        raw=root/'frame.ppm';log=root/'app.log'
        env={**os.environ,'TMPDIR':str(root/'tmp'),'MAGICK_TEMPORARY_PATH':str(root/'tmp'),'FILES_CONFIG_DIR':str(root/'config'),'FILES_TEST_COPY':'1','FILES_TEST_MOVE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_CAPTURE':str(raw),'FILES_CLOSE_TRACE':'1'}
        with log.open('w')as output:process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','copy','--exit-after','180'],env=env,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        report['root_pid']=process.pid;seq=0
        def wait(check,why,exits=False):
            end=time.monotonic()+20
            while time.monotonic()<end:
                result=check()
                if result:return result
                if process.poll()is not None and not exits:raise AssertionError(why+' exit '+str(process.returncode))
                time.sleep(.015)
            raise AssertionError(why)
        def command(action,exits=False):
            nonlocal seq
            after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);pending.replace(raw.with_suffix('.command'))
            wait(lambda:'FILES_COPY_COMMAND_OK '+action+'\n'in log.read_text()[after:],'command absent '+action,exits);seq+=1;return after
        def state(after=0,condition=lambda s:True):
            def current():
                for line in reversed(log.read_text()[after:].splitlines()):
                    if line.startswith('FILES_MOVE_STATE '):
                        value=json.loads(line[len('FILES_MOVE_STATE '):])
                        if value['seq']==seq and (not value['form']or not value['form'].get('extracting')or value['archive_apply_painted']) and condition(value):return value
            value=wait(current,'painted state absent seq '+str(seq));report['states'].append(value);return value
        def act(action,condition=lambda s:True):return state(command(action),condition)
        def capture(name,dimensions=(1000,650)):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return False
                w,h=map(int,parts[1].split());return(w,h)if len(parts[3])==w*h*3 else False
            actual=wait(full,'framebuffer incomplete');assert actual==dimensions
            path=evidence/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=env,check=True,timeout=15)
            assert path.read_bytes()[16:24]==dimensions[0].to_bytes(4,'big')+dimensions[1].to_bytes(4,'big');report['captures'].append(dict(path=str(path),dimensions=dimensions))
        wait(lambda:'FILES_UI_SMOKE_OK'in log.read_text(),'initial smoke absent');state(condition=lambda s:s['paint_revision']>0)
        if args.create_archives:
            report['scope']='ZIP32 streaming DEFLATE creation; files/directories/literal links, raw names, existing queue; no ZIP64/TAR/ACL/xattrs/durable journal/native input'
            act('create-form');assert sorted(p.name for p in target.iterdir())==['untouched.txt']
            act('extract-review',lambda s:s['form']and s['form']['prepared']);assert not(target/'bundle.zip').exists();capture('archive-create-review')
            act('move-resize-440x440',lambda s:s['painted']==[440,440]);capture('archive-create-review-440',(440,440))
            act('extract-cancel');assert not(target/'bundle.zip').exists()
            act('move-resize-1000x650',lambda s:s['painted']==[1000,650]);act('create-form');act('extract-review',lambda s:s['form']and s['form']['prepared'])
            act('extract-apply',lambda s:s['phase']=='Paused');assert not(target/'bundle.zip').exists()
            act('extract-target',lambda s:s['path']==str(target));act('move-focus-untouched')
            s=act('resume',lambda s:s['outcome']and s['outcome'].get('creating')and s['outcome']['success']and b'bundle.zip'.hex()in s['entries'])
            assert s['selection']==[b'untouched.txt'.hex()]and s['outcome']['links']==1;capture('archive-create-result-selection-preserved')
            with zipfile.ZipFile(target/'bundle.zip')as z:
                assert z.testzip()is None
                assert z.read('Pack/message-漢字.txt')==(source/'Pack/message-漢字.txt').read_bytes()
                assert z.read('Pack/run.sh')==(source/'Pack/run.sh').read_bytes()
                assert z.read('zero')==b'' and z.read('link')==b'../target/untouched.txt'
                assert stat.S_ISLNK(z.getinfo('link').external_attr>>16)
                assert (z.getinfo('Pack/run.sh').external_attr>>16)&0o7777==0o751
                from datetime import datetime,timezone
                stamp=datetime.fromtimestamp((source/'Pack/run.sh').stat().st_mtime,timezone.utc)
                assert z.getinfo('Pack/run.sh').date_time==(stamp.year,stamp.month,stamp.day,stamp.hour,stamp.minute,stamp.second//2*2)
                assert z.getinfo('Pack/empty/').is_dir()
                rawinfo=next(i for i in z.infolist()if i.filename.startswith('raw-'))
                assert rawinfo.filename.encode('cp437')==b'raw-\xff'and z.read(rawinfo)==b'raw bytes'
            act('extract-source',lambda s:s['path']==str(source));act('create-form')
            s=act('extract-review',lambda s:s['form']and s['form']['error']and not s['form']['preparing']);assert not s['form']['prepared'];capture('archive-create-occupied-rejected');act('extract-cancel')
            act('create-cancel-form');act('extract-review',lambda s:s['form']and s['form']['prepared']);act('extract-fixture-pause')
            before=log.read_text().count('FILES_ARCHIVE_FIXTURE_PAUSED')
            act('extract-apply',lambda s:s['phase']=='Paused');act('resume')
            wait(lambda:log.read_text().count('FILES_ARCHIVE_FIXTURE_PAUSED')>before,'creation positive progress pause absent')
            assert len(list(target.glob('.files-zip-*')))==1 and not(target/'cancelled.zip').exists()
            capture('archive-create-active-before-cancel')
            act('cancel-active',lambda s:s['outcome']and s['outcome'].get('creating')and s['outcome']['cancelled']and s['notice'])
            assert not list(target.glob('.files-zip-*'))and not(target/'cancelled.zip').exists();capture('archive-create-cancelled-cleaned');act('continue')
            act('create-stale-form');act('extract-review',lambda s:s['form']and s['form']['prepared'])
            actor=source/'Pack/actor-added';actor.write_bytes(b'changed after review')
            act('extract-apply',lambda s:s['phase']=='Paused');act('resume',lambda s:s['outcome']and s['outcome'].get('creating')and not s['outcome']['success']and s['notice'])
            assert not(target/'stale.zip').exists()and not list(target.glob('.files-zip-*'));capture('archive-create-source-changed-rejected');actor.unlink();act('continue')
            act('create-close-form');act('extract-review',lambda s:s['form']and s['form']['prepared']);act('extract-apply',lambda s:s['phase']=='Paused')
            command('callback',True);wait(lambda:process.poll()is not None,'normal close absent',True)
            assert process.returncode==0 and 'FILES_CLOSE_DRAINED'in log.read_text()and not(target/'close.zip').exists()
            assert source_fingerprints()==original and(target/'untouched.txt').read_bytes()==b'untouched'and not list(target.glob('.files-zip-*'))
            report.update(independent_zipfile_crc_and_content=True,independent_utc_dos_timestamp=True,literal_links_and_raw_names=True,source_fingerprints_unchanged=True,active_cancel_cleans_owned_stage=True,paused_close_no_publication=True)
        elif args.archive_recovery:
            (root/'queue').mkdir()
            def reviewed(kind):
                act('extract-'+kind+'-review',lambda s:s['archive_recovery']['confirmation'] is not None)
            def outcome(action):
                return lambda s:s['outcome'] and s['outcome']['archive_action']==action and (s['notice'] or s['outcome']['success'])
            act('extract-cleanup-seed');act('extract-review',lambda s:s['form']and s['form']['prepared'])
            act('extract-fixture-pause');act('extract-apply',lambda s:s['phase']=='Paused');act('resume')
            wait(lambda:'FILES_ARCHIVE_FIXTURE_PAUSED'in log.read_text(),'positive progress pause absent')
            stages=list(target.glob('.files-extract-*'));assert len(stages)==1
            stage=stages[0];foreign=stage/'foreign';foreign.write_bytes(b'foreign fixture must survive')
            inode=foreign.stat().st_ino
            act('cancel-active',lambda s:s['outcome']and s['notice']and s['archive_recovery']['actions'][0])
            reviewed('cleanup');capture('archive-cleanup-review')
            act('extract-recovery-back',lambda s:s['archive_recovery']['confirmation']is None);assert foreign.stat().st_ino==inode
            reviewed('cleanup');s=act('extract-recovery-confirm',outcome('Cleanup'));assert not s['outcome']['success']and not s['outcome']['cancelled'];assert foreign.read_bytes()==b'foreign fixture must survive'
            foreign.rename(root/'foreign-preserved')
            reviewed('cleanup');act('extract-recovery-pause');act('extract-recovery-confirm',lambda s:s['phase']=='Paused')
            act('cancel-active',outcome('Cleanup'));assert stage.exists()
            act('extract-fixture-fill-queue',lambda s:s['queued']==8)
            reviewed('cleanup');before=state()['outcome']['id']
            s=act('extract-recovery-confirm',lambda s:s['archive_recovery']['confirmation']is None);assert s['outcome']['id']==before and stage.exists()
            reviewed('cleanup');stamp=state()['archive_recovery']['confirmation']['stamp']
            act('extract-recovery-clear',lambda s:s['queued']==0)
            act('extract-old-confirm:'+str(stamp),lambda s:s['archive_recovery']['confirmation']is not None)
            act('move-resize-440x440',lambda s:s['painted']==[440,440]);capture('archive-cleanup-confirm-440',(440,440))
            s=act('extract-recovery-confirm',outcome('Cleanup'));assert s['outcome']['success']and s['outcome']['completed']==0 and not stage.exists();assert(root/'foreign-preserved').stat().st_ino==inode
            capture('archive-cleanup-success-440',(440,440));act('continue')
            act('move-resize-1000x650',lambda s:s['painted']==[1000,650])
            act('extract-sync-seed');act('extract-review',lambda s:s['form']and s['form']['prepared']);act('extract-fixture-sync');act('extract-apply',lambda s:s['phase']=='Paused')
            act('resume',lambda s:s['outcome']and s['notice']and s['archive_recovery']['actions'][1])
            published=target/'sync-output';published_inode=published.stat().st_ino
            (source/'source.zip').unlink()
            reviewed('sync');capture('archive-sync-review');act('extract-recovery-pause');act('extract-recovery-confirm',lambda s:s['phase']=='Paused')
            held=target/'held-published';published.rename(held);published.mkdir();(published/'foreign').write_bytes(b'new occupant')
            s=act('resume',outcome('Sync'));assert not s['outcome']['success'];assert(published/'foreign').read_bytes()==b'new occupant'
            published.rename(root/'published-foreign-preserved');held.rename(published)
            reviewed('sync');s=act('extract-recovery-confirm',outcome('Sync'));assert s['outcome']['success']and s['outcome']['completed']==1
            assert published.stat().st_ino==published_inode
            for name,data in contents.items():
                rawname=b'raw-\xff'if name=='raw-X'else os.fsencode(name)
                assert Path(os.fsdecode(os.path.join(os.fsencode(published),rawname))).read_bytes()==data
            capture('archive-sync-confirmed-without-source')
            command('callback',True);wait(lambda:process.poll()is not None,'close absent',True)
            assert process.returncode==0 and 'FILES_CLOSE_DRAINED'in log.read_text()
            report.update(scope='Explicit retained ZIP cleanup and sync-only recovery; private observer failure before fsync, NOT failed syscall/powerloss; private foreign actor',source_zip_removed_before_sync_retry=True,foreign_occupants_preserved=True,queue_rejection_returns_capability=True)
        else:
            act('extract-form');act('extract-review',lambda s:s['form']and s['form']['prepared']and not s['review_pending']);assert sorted(p.name for p in target.iterdir())==['untouched.txt'];capture('archive-review-before-mutation')
            act('move-resize-440x440',lambda s:s['window']==[440,440]and s['painted']==[440,440]);capture('archive-review-initial-controls-440',(440,440))
            act('extract-cancel');assert not(target/'unpacked').exists()
            act('move-resize-1000x650',lambda s:s['window']==[1000,650]and s['painted']==[1000,650]);act('extract-form');act('extract-review',lambda s:s['form']and s['form']['prepared']);act('extract-apply',lambda s:s['phase']=='Paused');assert not(target/'unpacked').exists();capture('archive-paused-no-output')
            act('extract-target',lambda s:s['path']==str(target));act('move-focus-untouched')
            s=act('resume',lambda s:s['outcome']and s['outcome']['extracting']and s['outcome']['success']and 'unpacked'.encode().hex()in s['entries'])
            assert s['selection']==['untouched.txt'.encode().hex()];capture('archive-result-other-tab-preserved')
            out=os.fsencode(target/'unpacked');wanted={os.fsencode(n):b for n,b in contents.items()};wanted[b'raw-\xff']=wanted.pop(b'raw-X')
            for name,data in wanted.items():
                path=os.path.join(out,name);assert open(path,'rb').read()==data;assert stat.S_IMODE(os.stat(path).st_mode)==(0o700 if name==b'run.sh'else 0o600)
            assert stat.S_IMODE(os.stat(out).st_mode)==0o700 and os.path.isdir(os.path.join(out,b'empty'))
            act('extract-source',lambda s:s['path']==str(source));act('extract-form');s=act('extract-review',lambda s:s['form']and not s['form']['preparing']and s['form']['error']);assert not s['form']['prepared'];capture('archive-occupied-rejected');act('extract-cancel')
            act('extract-corrupt');act('extract-review',lambda s:s['form']and s['form']['prepared']);act('extract-apply',lambda s:s['phase']=='Paused');s=act('resume',lambda s:s['outcome']and not s['outcome']['success']and s['notice']);assert not(target/'bad').exists();assert not any(p.name.startswith('.files-extract-')for p in target.iterdir());capture('archive-crc-error-no-published-output');act('continue')
            act('extract-unsafe');s=act('extract-review',lambda s:s['form']and not s['form']['preparing']and s['form']['error']);assert not s['form']['prepared'];assert not(target/'unsafe').exists()and not(root/'escape').exists();capture('archive-traversal-rejected');act('extract-cancel')
            act('extract-close-form');act('extract-review',lambda s:s['form']and s['form']['prepared']);act('extract-apply',lambda s:s['phase']=='Paused');command('callback',True);wait(lambda:process.poll()is not None,'normal close absent',True)
            assert process.returncode==0 and 'FILES_CLOSE_DRAINED'in log.read_text()and 'FILES_CLOSE_CALLBACK source=fltk_callback'in log.read_text();assert not(target/'close').exists()
            assert source_fingerprints()==original
            assert (target/'untouched.txt').read_bytes()==b'untouched';assert not any(p.name.startswith('.files-extract-')for p in target.iterdir())
        normal=True;report.update(functional_gate='PASS',normal_exit0_after_ack=True,source_hashes_unchanged=not args.archive_recovery,staging_absent=True,paused_close_not_active_syscall_proof=not args.archive_recovery,tree_bytes_and_modes=not args.archive_recovery and not args.create_archives)
    except BaseException as error:report['error']=repr(error);raise
    finally:
        if log and log.exists():report['log']=log.read_text()
        if process and process.poll()is None:
            process.terminate()
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:process.kill();process.wait()
        temporary.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
        path=evidence/(('archive-create-ui-'if args.create_archives else'archive-recovery-ui-'if args.archive_recovery else'archive-extract-ui-')+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n');print(path)

if args.archives or args.archive_recovery or args.create_archives:
    archive_check()
    raise SystemExit(0)

def move_check():
    """Native moves over the existing queue. Functional capture only."""
    if not args.capture:raise SystemExit('--moves requires --capture; resource/startup freeze remains active')
    report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],
            'scope':'native same-FS move only; EXDEV refused without copy/delete; no crash journal/undo',
            'method':'existing shared Actions/pointer handlers and real FLTK framebuffer; no native OS input or resource sampling'}
    process=None;normal=False;temporary=None;log=None
    try:
        instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));match=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(match)==1;os.environ['HYPRLAND_INSTANCE_SIGNATURE']=match[0]['instance']
        temporary=tempfile.TemporaryDirectory(prefix='move-ui-',dir=base);root=Path(temporary.name);source=root/'source';target=root/'target'
        for name in ['source','target','queued','close','tmp']:(root/name).mkdir()
        payload=bytes(range(256))*256;(source/'payload.bin').write_bytes(payload);nested=source/'Nested';nested.mkdir();(nested/'inside.txt').write_text('nested 漢字\r\n');(source/'link.txt').symlink_to('payload.bin');rawname=os.fsdecode(b'raw-\xff.txt');(source/rawname).write_bytes(b'raw bytes')
        for name in ['collision.txt','cancel.txt','close.txt']:(source/name).write_text('source '+name)
        (target/'collision.txt').write_bytes(b'existing target');(target/'untouched.txt').write_bytes(b'untouched target')
        stamp=1700000000123456789
        for p in [source/'payload.bin',nested,nested/'inside.txt',source/rawname]:p.chmod(0o750 if p.is_dir() else 0o640);os.utime(p,ns=(stamp,stamp))
        xattrs=False
        try:os.setxattr(source/'payload.bin','user.files-move-fixture',b'preserved');xattrs=True
        except OSError:pass
        def fingerprint(p):
            st=p.lstat();record={'dev':st.st_dev,'ino':st.st_ino,'mode':st.st_mode,'uid':st.st_uid,'gid':st.st_gid,'mtime_ns':st.st_mtime_ns}
            if p.is_symlink():record['link']=os.readlink(p)
            elif p.is_file():record['sha256']=hashlib.sha256(p.read_bytes()).hexdigest()
            if not p.is_symlink():record['xattrs']={n:os.getxattr(p,n).hex() for n in os.listxattr(p)}
            return record
        roots=['payload.bin','Nested','link.txt',rawname];original={os.fsencode(n).hex():fingerprint(source/n) for n in roots};inside=fingerprint(nested/'inside.txt')
        def assert_unpublished():
            for name in roots:
                assert fingerprint(source/name)==original[os.fsencode(name).hex()]
                assert not (target/name).exists() and not (target/name).is_symlink()
            assert fingerprint(source/'Nested/inside.txt')==inside
            assert (target/'collision.txt').read_bytes()==b'existing target' and (target/'untouched.txt').read_bytes()==b'untouched target'
        raw=root/'frame.ppm';log=root/'app.log';env={**os.environ,'TMPDIR':str(root/'tmp'),'MAGICK_TEMPORARY_PATH':str(root/'tmp'),'FILES_CONFIG_DIR':str(root/'config'),'FILES_TEST_COPY':'1','FILES_TEST_MOVE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_CAPTURE':str(raw),'FILES_CLOSE_TRACE':'1','FILES_TEST_FIXED_SIZE':'1'}
        with log.open('w') as out:process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','copy','--exit-after','90'],stdout=subprocess.DEVNULL,stderr=out,env=env,start_new_session=True)
        seq=0
        def wait(check,why,allow_exit=False):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(why+' app exit='+str(process.returncode))
                time.sleep(.01)
            raise AssertionError(why)
        def command(action):
            nonlocal seq
            after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            wait(lambda:'FILES_COPY_COMMAND_OK '+action+'\n' in log.read_text()[after:],'command '+action,action=='close');seq+=1;return after
        def state(after=0,condition=lambda s:True):
            def read():
                for line in reversed(log.read_text()[after:].splitlines()):
                    if line.startswith('FILES_MOVE_STATE '):
                        s=json.loads(line[len('FILES_MOVE_STATE '):])
                        if s['seq']==seq and condition(s):return s
                return False
            s=wait(read,'fresh move state seq='+str(seq));report['states'].append(s);return s
        def act(action,condition=lambda s:True):return state(command(action),condition)
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def ready():
                if not raw.exists():return False
                p=raw.read_bytes().split(b'\n',3);return len(p)==4 and len(p[3])==int(p[1].split()[0])*int(p[1].split()[1])*3
            wait(ready,'framebuffer');p=evidence/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(p)],env=env,check=True,timeout=10);report['captures'].append(str(p))
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'smoke');wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry');report['geometry']=observed_geometry(process.pid,1000,650);state()
        act('move-form');act('move-review',lambda s:s['form'] and s['form']['prepared'] and not s['review_pending']);assert_unpublished();capture('move-native-review')
        act('move-confirm',lambda s:s['phase']=='Paused');assert_unpublished();act('move-target-tab');act('move-focus-untouched');capture('move-paused-target-tab')
        done=act('resume',lambda s:s['outcome'] and s['outcome']['success'] and s['outcome']['moving'] and s['outcome']['completed']==4 and '7061796c6f61642e62696e' in s['entries'])
        assert done['selection']==['untouched.txt'.encode().hex()] and done['path']==str(target)
        for name in roots:assert not (source/name).exists() and not (source/name).is_symlink();assert fingerprint(target/name)==original[os.fsencode(name).hex()]
        assert fingerprint(target/'Nested/inside.txt')==inside;capture('move-native-result-target')
        a=act('move-source-tab');assert not any(os.fsencode(n).hex() in a['entries'] for n in roots) and not a['selection'];capture('move-native-source-refreshed')
        act('move-collision');act('move-review',lambda s:s['form'] and not s['form']['preparing'] and bool(s['form']['error']));assert (source/'collision.txt').read_text()=='source collision.txt' and (target/'collision.txt').read_bytes()==b'existing target';capture('move-collision-source-preserved');act('move-dialog-cancel')
        assert source.stat().st_dev!=Path('/proc').stat().st_dev
        act('move-exdev');exdev=act('move-review',lambda s:s['form'] and not s['form']['preparing'] and 'EXDEV' in s['form']['error']);assert (source/'collision.txt').read_text()=='source collision.txt';capture('move-exdev-explicit-pending');act('move-dialog-cancel')
        act('move-cancel-form');act('move-review',lambda s:s['form'] and s['form']['prepared']);act('move-confirm',lambda s:s['phase']=='Paused');cancelled=act('cancel-active',lambda s:s['outcome'] and s['outcome']['cancelled'] and s['outcome']['completed']==0);assert (source/'cancel.txt').read_text()=='source cancel.txt' and not (root/'queued/cancel.txt').exists();act('continue')
        act('move-close-form');act('move-review',lambda s:s['form'] and s['form']['prepared']);act('move-confirm',lambda s:s['phase']=='Paused');after=command('close');wait(lambda:process.poll() is not None,'normal close',True)
        text=log.read_text();assert process.returncode==0 and 'FILES_CLOSE_DRAINED' in text[after:];assert (source/'close.txt').read_text()=='source close.txt' and not (root/'close/close.txt').exists()
        assert (target/'untouched.txt').read_bytes()==b'untouched target';normal=True;report.update(functional_gate='PASS',log=text,normal_exit0_after_ack=True,native_roots=4,raw_path_and_symlink_literal=True,identity_bytes_metadata_preserved=True,xattr_fixture_supported=xattrs,refresh_source_and_destination=True,collision_no_clobber=True,cancel_before_publication=True,close_active_paused=True,exdev_scope='real cross-device preflight /proc refused, no copy/delete; no EXDEV transfer or simulated move success',cancel_after_some_roots='core callback test only; not native UI cancellation timing',original_roots=original)
    except BaseException as error:
        report['error']=repr(error)
        if log and log.exists():report['log']=log.read_text()
        raise
    finally:
        if process and not normal:stop_tree(process,None)
        if temporary:temporary.cleanup()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
        p=evidence/('move-ui-'+str(time.time_ns())+'.json')
        with p.open('x') as out:json.dump(report,out,indent=2)
        print(json.dumps({'report':str(p),'functional_gate':report['functional_gate']}))

if args.moves:
    move_check()
    raise SystemExit(0)

report = {'method': 'in-process UI actions and real FLTK framebuffer, no native OS input',
          'scenario': 'three roots: 16MiB file, directory with10000 small files, symlink; queue, pause, cancellation, no-clobber, normal close',
          'captures': [], 'functional_gate': 'FAIL', 'resource_gate': 'NOT_MEASURED' if args.capture else 'UNVALIDATED'}
with tempfile.TemporaryDirectory(prefix='copy-ui-', dir=base) as tmp:
    root = Path(tmp)
    source = root/'source'
    for name in ['source','target','queued','close','cancelled']:
        (root/name).mkdir()
    block = bytes(range(256))*4096
    with (source/'payload.bin').open('wb') as f:
        for _ in range(16): f.write(block)
    nested = source/'Nested'
    nested.mkdir()
    for n in range(10000): (nested/f'file{n:04}.txt').write_bytes(f'line {n}\r\n漢字\n'.encode())
    (source/'cancel.txt').write_bytes(b'do not copy this queued file')
    (source/'link.txt').symlink_to('payload.bin')
    stamp = 1700000000123456789
    for path in [source/'payload.bin', *nested.iterdir(), nested]:
        path.chmod(0o750 if path.is_dir() else 0o640)
        os.utime(path, ns=(stamp,stamp))
    def fingerprint(folder):
        result = {}
        for p in sorted(folder.rglob('*')):
            if p.is_symlink(): result[str(p.relative_to(folder))] = ['symlink',os.readlink(p)]
            elif p.is_dir(): result[str(p.relative_to(folder))] = ['directory',p.stat().st_mode & 0o777,p.stat().st_mtime_ns]
            else:
                with p.open('rb') as f: digest = hashlib.file_digest(f,'sha256').hexdigest()
                st=p.stat(); result[str(p.relative_to(folder))] = [st.st_size,st.st_mode & 0o777,st.st_mtime_ns,digest]
        return result
    original = fingerprint(source)
    raw = root/'frame.ppm'
    log = root/'app.log'
    env = {**os.environ,'FILES_TEST_COPY':'1','FILES_TEST_CAPTURE':str(raw),
           'FILES_CONFIG_DIR':str(root/'config'),'FILES_METRICS_READY':'1'}
    with log.open('w') as stream:
        process = subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--size','1000x650',
                                    '--scale','1','--ui-smoke','--ui-scene','copy','--exit-after','40'],
                                   stdout=subprocess.DEVNULL,stderr=stream,env=env,start_new_session=True)
    sampler = None if args.capture else TreeSampler(process.pid)
    metrics = Metrics()
    exit_observations = []
    stable_since = stable_until = None
    normal_exit = False
    def sample():
        if sampler and process.poll() is None:
            value = sampler.sample()
            if not value['root_alive'] and not value['errors'] and not value['processes']:
                exit_observations.append(value)
            else: metrics.add(value)
    def wait(check, reason, timeout=12, allow_exit=False):
        until=time.monotonic()+timeout
        while time.monotonic()<until:
            sample()
            value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; app exited\n'+log.read_text())
            time.sleep(.01)
        raise AssertionError(reason+'\n'+log.read_text())
    def command(action):
        before=log.read_text().count('FILES_COPY_COMMAND_OK '+action+'\n')
        pending=raw.with_suffix('.pending');pending.write_text(action)
        os.replace(pending,raw.with_suffix('.command'))
        wait(lambda:log.read_text().count('FILES_COPY_COMMAND_OK '+action+'\n')>before,'command failed: '+action,allow_exit=action=='close')
    def state():
        values=re.findall(r'FILES_COPY_STATE (.*)',log.read_text())
        return values[-1] if values else ''
    def capture(name):
        assert args.capture
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def ready():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            if len(parts)!=4:return False
            w,h=map(int,parts[1].split())
            return len(parts[3])==w*h*3
        wait(ready,'framebuffer missing')
        path=evidence/(name+'-controls.png')
        if path.exists():path=evidence/(name+'-controls-'+str(time.time_ns())+'.png')
        subprocess.run(['magick',str(raw),str(path)],check=True)
        report['captures'].append(str(path))
    def finished(id): return f'FILES_COPY_FINISHED id={id} ' in log.read_text()
    def ack(action,id):
        command(action)
        wait(lambda:finished(id),'completion ACK missing')
    def resting(seconds):
        until=time.monotonic()+seconds
        while time.monotonic()<until:sample();time.sleep(.01)
    try:
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'initial UI missing')
        wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry not validated')
        report['geometry']=observed_geometry(process.pid,1000,650)
        command('form')
        if args.capture:capture('copy-destination')
        command('enqueue')
        wait(lambda:'phase=Paused' in state() and 'active=Some(1)' in state(),'paused worker missing')
        command('extra');command('enqueue')
        wait(lambda:'queued=1' in state(),'second job not queued')
        if args.capture:capture('copy-paused-queue')
        command('cancel-pending')
        wait(lambda:'queued=0' in state(),'pending cancellation missing')
        command('target')
        command('resume')
        wait(lambda:finished(1),'copy ACK missing')
        wait(lambda:'loaded=true entries=3 snapshot=false' in state(),'destination refresh missing')
        if args.capture:capture('copy-result')
        # No-clobber from the same visible destination: fail without losing bytes.
        command('source')
        wait(lambda:'loaded=true entries=4 snapshot=false' in state(),'source reload missing')
        command('collision');ack('enqueue',3)
        wait(lambda:'notice=true' in state(),'no-clobber diagnostic not visible')
        if args.capture:capture('copy-no-clobber')
        command('continue')
        wait(lambda:'notice=false' in state(),'diagnostic not dismissed')
        command('target')
        wait(lambda:'loaded=true entries=3 snapshot=false' in state(),'target reload missing before cancellation')
        stable_since=time.monotonic()
        resting(1.5)
        stable_until=time.monotonic()
        # Separate active cancellation: wait for actual file progress before cancel.
        # The directory root must remain unpublished and its staging cleaned.
        command('cancel-form');command('enqueue')
        wait(lambda:re.search(r'FILES_COPY_PROGRESS id=4 bytes=[1-9][0-9]* entries=[1-9][0-9]* ',log.read_text()),'no real progress before cancellation')
        command('cancel-active')
        wait(lambda:finished(4),'active cancellation ACK absent')
        cancelled=re.search(r'FILES_COPY_FINISHED id=4 .*cancelled=true report_bytes=([0-9]+) report_files=([0-9]+) completed=0',log.read_text())
        assert cancelled and int(cancelled[2])>0,'cancellation did not interrupt a partially copied directory'
        assert not list((root/'cancelled').iterdir()),'partial cancelled root/staging remains'
        report['active_cancel_after_progress']={'id':4,'copied_files':int(cancelled[2]),'logical_bytes':int(cancelled[1]),'unpublished_root_cleaned':True,
                                               'scope':'directory copying after observed per-file progress; auto strategy may clone data, not necessarily userspace transfer'}
        if args.capture:capture('copy-cancelled-progress')
        command('continue')
        # Close a genuinely active paused job; require its own ACK before exit0.
        command('close-form');command('enqueue')
        wait(lambda:'phase=Paused' in state() and 'active=Some(5)' in state(),'close paused job missing')
        before=log.read_text().count('FILES_COPY_FINISHED')
        command('close')
        wait(lambda:process.poll() is not None,'normal close timed out',allow_exit=True)
        assert process.returncode==0
        assert finished(5) and log.read_text().count('FILES_COPY_FINISHED')==before+1
        assert 'FILES_COPY_DRAINED' in log.read_text()
        normal_exit=True
        expected={k:v for k,v in original.items() if k!='cancel.txt'}
        assert fingerprint(root/'target')==expected,'destination data/metadata mismatch'
        assert fingerprint(source)==original,'source changed'
        assert not list((root/'queued').iterdir()),'cancelled pending job wrote data'
        assert not list((root/'close').iterdir()),'close cancellation published data'
        assert not any(p.name.startswith('.files-copy-') for p in root.rglob('*')),'staging remains'
        survivors=[];errors=[]
        for pid,start in sorted(sampler.known if sampler else []):
            try:
                st=read_stat(Path(f'/proc/{pid}/stat').read_text())
                if st['start']==start and st['state']!='Z':survivors.append([pid,start])
            except FileNotFoundError:
                if Path(f'/proc/{pid}').exists():errors.append(f'{pid}/{start}: stat missing while directory exists')
            except (OSError,ValueError,IndexError) as exc:errors.append(f'{pid}/{start}: {exc}')
        cleanup={'complete':not survivors and not errors,'survivors':survivors,'errors':errors,
                 'scope':'observed PID/starttime identities' if sampler else 'root exit0 only; descendants not sampled'}
        report.update(functional_gate='PASS',normal_exit_after_exact_ack=normal_exit,cleanup=cleanup,
                      pending_cancelled=1,close_cancelled_id=5,source_intact=True,metadata_sha256_symlink_verified=True,
                      stable_since=stable_since,stable_until=stable_until,stable_scope='post-copy/no-clobber window after loaded listing, not universal idle',
                      stdout_log=log.read_text(),terminal_exit_observations=exit_observations)
        if sampler:
            values=metrics.report(stable_since,stable_until)
            peak=values['sampled_maxima']['memory_kib']
            values['sample_interval_requested_ms']=10
            report['metrics']=values
            report['resource_gate']='UNVALIDATED' if not values['complete'] or not cleanup['complete'] else ('FAIL' if peak*1024>150_000_000 else 'PASS')
            report['goal_met']=peak*1024<=50_000_000 if report['resource_gate']!='UNVALIDATED' else None
            report['resource_scope']='sampled complete observed tree PSS+SwapPss; not lifetime peak, GPU excluded; explicit scenario, goal50MB decimal/ceiling150MB'
    except Exception as exc:
        report['error']=str(exc)
        report['stdout_log']=log.read_text()
    finally:
        if process.poll() is None:
            report['emergency_cleanup']=stop_tree(process,sampler)
        for binary in ['files','files-helper']:
            path=ROOT/'target/release'/binary
            report[binary+'_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
        output=evidence/('copy-ui.json' if args.capture else 'copy-resources.json')
        if output.exists():output=evidence/(output.stem+'-'+str(time.time_ns())+'.json')
        output.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({'report':str(output),'functional':report['functional_gate'],'resources':report['resource_gate']}))
    if report['functional_gate']!='PASS' or report['resource_gate'] in ['FAIL','UNVALIDATED'] and not args.capture:raise SystemExit(1)
