#!/usr/bin/env python3
"""Functional-only properties/chmod checks on owned fixtures; no native input or metrics."""
import hashlib,json,os,re,subprocess,tempfile,time
from pathlib import Path
from memcheck import stop_tree
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
EVIDENCE=ROOT/'docs/evidence/0.0.4';EVIDENCE.mkdir(parents=True,exist_ok=True)
report={'method':'internal visible hit handlers + FLTK framebuffer + actual fixture modes/bytes; no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'runs':[]}
with tempfile.TemporaryDirectory(prefix='properties-ui-',dir=BASE) as temp:
    test=Path(temp)
    for closing in [False,True]:
        folder=test/('closing' if closing else 'normal');folder.mkdir()
        source=folder/'example-日本語.txt';source.write_bytes('contenido 漢字\t\r\n'.encode());os.chmod(source,0o600)
        original=source.read_bytes();outside=test/('target-close' if closing else 'target-normal');outside.write_bytes(b'outside preserved');os.chmod(outside,0o640)
        (folder/'enlace').symlink_to(outside)
        (folder/'carpeta').mkdir();(folder/'carpeta'/'inside').write_bytes(b'nested untouched')
        raw=test/'capture.ppm';log=test/('close.log' if closing else 'normal.log')
        env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PROPERTIES':'1','FILES_METRICS_READY':'1'}
        with log.open('w') as out:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','properties','--exit-after','60'],env=env,stderr=out,stdout=subprocess.DEVNULL,start_new_session=True)
        normal=False;seq=0;run={'close_during_request':closing}
        def wait(check,reason,allow_exit=False):
            until=time.monotonic()+12
            while time.monotonic()<until:
                result=check()
                if result:return result
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode)+'\n'+log.read_text())
                time.sleep(.02)
            raise AssertionError(reason+'\n'+log.read_text())
        def ready():
            return wait(lambda:next((line for line in log.read_text().splitlines() if line.startswith(f'FILES_PROPERTIES_READY seq={seq} ')),False),'fresh properties ready absent')
        def command(action):
            global seq
            before=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            match=wait(lambda:re.search(r'FILES_PROPERTIES_COMMAND seq=(\d+) action='+re.escape(action)+r' id=(\d+) applied=(true|false)',log.read_text()[before:]),'command failed: '+action,action in ['close','apply-close'])
            seq=int(match[1]);return int(match[2]),before
        def applied(command_result,success,allow_exit=False):
            request,before=command_result
            return wait(lambda:re.search(r'FILES_PROPERTIES_ACK id='+str(request)+r' success='+str(success).lower()+r' changed=(true|false) snapshot=(true|false)',log.read_text()[before:]),'matching post-command ACK absent',allow_exit)
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(full,'framebuffer incomplete');target=EVIDENCE/(name+'-'+str(time.time_ns())+'.png')
            subprocess.run(['magick',str(raw),str(target)],check=True);report['captures'].append(str(target))
            report.setdefault('capture_geometry',[]).append({'path':str(target),'framebuffer_dimensions':[int(v) for v in raw.read_bytes().split(b'\n',3)[1].split()]})
        try:
            ready();command('actions');ready();capture('properties-actions');command('properties-file');line=ready()
            assert 'observed=0600 requested=0600 stale=false editable=true' in line,line
            if not closing:capture('properties-file')
            command('focus-group-read');ready();command('group-read');assert 'requested=0640' in ready()
            if closing:
                request,before=command('apply-close')
                wait(lambda:process.poll() is not None,'close did not finish',True)
                ack=re.search(r'FILES_PROPERTIES_ACK id='+str(request)+r' success=(true|false) changed=(true|false) snapshot=(true|false)',log.read_text()[before:])
                assert ack,'normal exit without matching ACK'
                assert bool(ack[1]=='true')==bool(ack[2]=='true')
                expected=0o640 if ack[2]=='true' else 0o600
                assert source.stat().st_mode&0o777==expected
                run.update(ack_after_apply_close=True,changed=ack[2]=='true',final_mode=oct(expected),scope='close immediately after request submission; syscall-active cancellation not claimed')
            else:
                ack=applied(command('apply'),True);assert ack[1]=='true';line=ready()
                assert 'observed=0640 requested=0640 stale=false editable=true' in line,line
                assert source.stat().st_mode&0o777==0o640 and source.read_bytes()==original
                capture('properties-permissions-applied')
                # Simulate an actor while the confirmation remains based on640.
                os.chmod(source,0o600);command('focus-other-read');ready();command('other-read');assert 'requested=0644' in ready()
                ack=applied(command('apply'),False);assert ack[1]=='false';line=ready()
                assert 'requested=0644 stale=true editable=false' in line and source.stat().st_mode&0o777==0o600
                capture('properties-stale-rejected')
                command('reload');assert 'observed=0600 requested=0600 stale=false editable=true' in ready()
                command('dismiss');ready();command('actions');ready();command('properties-link');line=ready()
                assert 'editable=false' in line,line
                command('readonly-apply');ready()
                assert outside.read_bytes()==b'outside preserved' and outside.stat().st_mode&0o777==0o640
                capture('properties-symlink-readonly')
                command('dismiss');ready();command('actions');ready();command('properties-folder');line=ready()
                assert 'stale=false editable=true' in line,line
                assert (folder/'carpeta'/'inside').read_bytes()==b'nested untouched'
                capture('properties-directory')
                command('close');wait(lambda:process.poll() is not None,'normal close absent',True)
                run.update(read_apparent_size=True,chmod0600_to0640=True,external_mode_change_preserved=True,reload_required=True,symlink_readonly=True,directory_read=True)
            assert process.returncode==0 and 'FILES_PROPERTIES_DRAINED' in log.read_text()
            assert source.read_bytes()==original and (folder/'carpeta'/'inside').read_bytes()==b'nested untouched'
            normal=True;run.update(functional_gate='PASS',normal_exit_after_ack=True,root_exit=process.returncode,cleanup_scope='root exit0; same-process worker, no helper used')
        except Exception as error:
            run.update(functional_gate='FAIL',error=repr(error));raise
        finally:
            run['log']=log.read_text();report['runs'].append(run)
            if not normal:stop_tree(process)
            report['functional_gate']='PASS' if len(report['runs'])==2 and all(r['functional_gate']=='PASS' for r in report['runs']) else ('FAIL' if any(r['functional_gate']=='FAIL' for r in report['runs']) else 'IN_PROGRESS')
            report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
            # Save every run, including failed/incomplete evidence; never overwrite.
            target=EVIDENCE/('properties-ui-'+str(time.time_ns())+'.json')
            with target.open('x') as out:json.dump(report,out,ensure_ascii=False,indent=2)
            print(target)
