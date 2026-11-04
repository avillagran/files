#!/usr/bin/env python3
"""Real local-trash fixture operations via internal visible hit targets/framebuffer.
No OS input, resource/startup sampling, external trash or personal data.
"""
import hashlib,json,os,re,subprocess,tempfile,time
from pathlib import Path
from memcheck import stop_tree
ROOT=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
report={'method':'internal visible hit targets/handlers + actual filesystem/Trash metadata + FLTK framebuffer; no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'known_pending':['per-volume/cross-device trash','redo, browse/import persisted trash receipts and crash journal','batch beyond64 and resource budget','other platforms']}
with tempfile.TemporaryDirectory(prefix='trash-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir();data=test/'data';data.mkdir();trash=data/'Trash'
    (folder/'uno.txt').write_bytes(b'original uno\x00\xff');(folder/'dos.txt').write_text('dos 漢字\r\n',newline='')
    (folder/'carpeta').mkdir();(folder/'carpeta'/'inside.txt').write_bytes(b'nested preserved')
    outside=test/'outside.txt';outside.write_bytes(b'outside remains unchanged');(folder/'enlace').symlink_to(outside)
    raw_name=os.fsdecode(b'raw\xff.bin');(folder/raw_name).write_bytes(b'raw name preserved');(folder/'untouched.txt').write_bytes(b'not selected')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'XDG_DATA_HOME':str(data),'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_TRASH':'1','FILES_TEST_MUTATIONS':'1'}
    with log.open('w') as output:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','trash','--exit-after','90'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
    normal=False;seq=0
    def wait(check,reason,allow_exit=False,timeout=15):
        until=time.monotonic()+timeout
        while time.monotonic()<until:
            value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode)+'\n'+log.read_text())
            time.sleep(.025)
        raise AssertionError(reason+'\n'+log.read_text())
    def ready():
        return wait(lambda:next((line for line in log.read_text().splitlines() if line.startswith(f'FILES_TRASH_READY seq={seq} ')),False),'fresh loaded/worker-idle marker absent')
    def command(action):
        global seq
        before=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
        match=wait(lambda:re.search(r'FILES_TRASH_COMMAND seq=(\d+) action='+re.escape(action)+r' id=(\d+) applied=(true|false) undo=(true|false) notice=(true|false)',log.read_text()[before:]),'command missing: '+action,action=='close')
        seq=int(match[1]);return int(match[2]),before,match[3]=='true'
    def finished(command_result,success):
        request,before,applied=command_result;assert applied,log.read_text()
        return wait(lambda:next((line for line in log.read_text()[before:].splitlines() if line.startswith(f'FILES_MUTATION_FINISHED id={request} success={str(success).lower()} ')),False),'fresh matching mutation ACK absent')
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'framebuffer incomplete');path=evidence/(name+'-'+str(time.time_ns())+'.png')
        subprocess.run(['magick',str(raw),str(path)],check=True)
        report['captures'].append({'path':str(path),'framebuffer_dimensions':[int(v) for v in raw.read_bytes().split(b'\n',3)[1].split()]})
    def trash_count():return len(list((trash/'files').iterdir()))
    try:
        ready();command('actions');ready();capture('trash-menu')
        finished(command('trash-basic'),True);line=ready();assert 'undo=true' in line
        for name in ['uno.txt','dos.txt','carpeta','enlace']:assert not (folder/name).exists() and not (folder/name).is_symlink()
        assert trash_count()==4 and len(list((trash/'info').iterdir()))==4
        assert outside.read_bytes()==b'outside remains unchanged' and (folder/'untouched.txt').read_bytes()==b'not selected'
        for info in (trash/'info').iterdir():
            assert info.stat().st_mode&0o777==0o600
            text=info.read_text();assert text.startswith('[Trash Info]\nPath=') and '\nDeletionDate=' in text
        capture('trash-result')
        (folder/'uno.txt').write_bytes(b'new occupant must survive')
        finished(command('restore'),False);line=ready();assert 'undo=true notice=true' in line
        assert (folder/'uno.txt').read_bytes()==b'new occupant must survive'
        assert trash_count()==1
        assert (folder/'carpeta'/'inside.txt').read_bytes()==b'nested preserved' and (folder/'enlace').is_symlink()
        capture('trash-restore-conflict')
        command('report-pan');ready();capture('trash-restore-conflict-pan')
        (folder/'uno.txt').unlink();command('dismiss');ready();finished(command('restore'),True);ready()
        assert (folder/'uno.txt').read_bytes()==b'original uno\x00\xff'
        assert (folder/'dos.txt').read_bytes()=='dos 漢字\r\n'.encode()
        assert trash_count()==0 and len(list((trash/'info').iterdir()))==0
        capture('trash-restored')
        command('actions');ready();finished(command('trash-raw'),True);ready()
        assert not (folder/raw_name).exists()
        metadata=next((trash/'info').iterdir()).read_bytes();assert b'raw%FF.bin' in metadata
        finished(command('restore'),True);ready();assert (folder/raw_name).read_bytes()==b'raw name preserved'
        for i in range(65):(folder/f'many-{i:03d}').write_bytes(str(i).encode())
        command('reload');ready();command('actions');ready();result=command('trash-many');assert not result[2]
        line=ready();assert 'notice=true' in line and 'máximo 64' in line,line
        assert trash_count()==0 and len(list((trash/'info').iterdir()))==0
        assert all((folder/f'many-{i:03d}').read_bytes()==str(i).encode() for i in range(65))
        capture('trash-batch-limit');command('dismiss');ready()
        command('close');wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_TRASH_DRAINED' in log.read_text(),log.read_text();normal=True
        report.update({'functional_gate':'PASS','selected_batch':4,'raw_name_roundtrip':True,'no_clobber_conflict_and_retry':True,'symlink_target_untouched':True,'nonempty_directory_roundtrip':True,'metadata_encoding_and_modes':True,'65_selection_rejected_before_mutation':True,'normal_exit_after_ack':True,'root_exit':process.returncode,'log':log.read_text(),'binary_sha256':hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest(),'helper_used':False})
    except Exception as error:
        report.update({'functional_gate':'FAIL','error':repr(error),'log':log.read_text()});raise
    finally:
        if not normal:stop_tree(process)
        path=evidence/('trash-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2));print(path)
