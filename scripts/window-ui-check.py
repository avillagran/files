#!/usr/bin/env python3
"""Functional window/menu evidence only. No OS input or resource/startup sampling.
The compositor chooses initial tiling/placement. This script never floats/moves or
resizes a window via hyprctl; mode changes use the app's internal handlers.
"""
import hashlib,json,os,re,subprocess,tempfile,time,tomllib
from pathlib import Path
from memcheck import stop_tree
ROOT=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
report={'method':'internal file/menu handlers + actual FLTK framebuffer; compositor modes observed read-only with hyprctl; no native OS input', 'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','native_move_resize_gate':'PENDING_OS_INPUT','captures':[],'runs':[], 'known_pending':['real serial-driven move/resize and grab/grabbing cursor visual proof','screen reader tree and non-Linux validation','resource/startup phase frozen']}
with tempfile.TemporaryDirectory(prefix='window-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir();(folder/'archivo2.txt').write_text('2');(folder/'archivo10.txt').write_text('10');(folder/'carpeta').mkdir()
    config=test/'config';config.mkdir();settings=config/'config.toml';settings.write_text('effects=false\n[future]\nvalue=42\n')
    def observed(pid):
        clients=json.loads(subprocess.run(['hyprctl','clients','-j'],check=True,stdout=subprocess.PIPE,timeout=3).stdout)
        return next((c for c in clients if c.get('pid')==pid),None)
    for run,initial in enumerate(['normal','fullscreen','normal']):
        raw=test/f'capture{run}.ppm';log=test/f'app{run}.log'
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_WINDOW':'1'}
        with log.open('w') as output:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','window','--exit-after','60'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
        normal=False;seq=0
        def wait(check,reason,allow_exit=False,timeout=15):
            until=time.monotonic()+timeout
            while time.monotonic()<until:
                result=check()
                if result:return result
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode)+'\n'+log.read_text())
                time.sleep(.025)
            raise AssertionError(reason+'\n'+log.read_text())
        def ready(expected):
            return wait(lambda:next((line for line in log.read_text().splitlines() if line.startswith(f'FILES_WINDOW_READY seq={seq} ') and f'mode={expected} ' in line),False),'fresh window READY absent')
        def command(action,expected):
            before=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            match=wait(lambda:re.search(r'FILES_WINDOW_COMMAND seq=(\d+) action='+re.escape(action)+r'\n',log.read_text()[before:]),'command missing: '+action)
            return int(match[1])
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def complete():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(complete,'framebuffer incomplete')
            path=evidence/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],check=True)
            report['captures'].append({'path':str(path),'framebuffer_dimensions':[int(v) for v in raw.read_bytes().split(b'\n',3)[1].split()]})
        try:
            line=ready(initial);assert 'border=true' in line,line
            initial_client=wait(lambda:observed(process.pid),'compositor client missing')
            if initial=='fullscreen': wait(lambda:(c:=observed(process.pid)) and c.get('fullscreen')==2,'persisted fullscreen not observed')
            report['runs'].append({'initial_mode':initial,'initial_marker':line,'initial_compositor':initial_client})
            if run==0:
                assert 'fullscreen=false' in line,line
                capture('window-normal-default')
                seq=command('preferences',initial);line=ready(initial);assert 'preferences=true actions=false' in line
                capture('window-preferences')
                seq=command('actions',initial);line=ready(initial);assert 'preferences=false actions=true' in line
                capture('window-actions')
                seq=command('preferences',initial);ready(initial)
                seq=command('fullscreen','fullscreen');ready('fullscreen')
                wait(lambda:(c:=observed(process.pid)) and c.get('fullscreen')==2,'fullscreen mode not observed')
                capture('window-fullscreen-preferences')
            elif run==1:
                seq=command('preferences',initial);ready(initial)
                seq=command('normal','normal');ready('normal')
                wait(lambda:(c:=observed(process.pid)) and c.get('fullscreen')==0,'normal restore not observed')
                seq=command('menus-close','normal');ready('normal');capture('window-normal-restored')
            seq=command('close','normal');wait(lambda:process.poll() is not None,'normal close timeout',True)
            assert process.returncode==0 and 'FILES_WINDOW_DRAINED' in log.read_text(),log.read_text()
            normal=True
            parsed=tomllib.loads(settings.read_text());assert parsed['effects'] is False and parsed['future']['value']==42
            expected='fullscreen' if run==0 else 'normal';assert parsed['window_mode']==expected,parsed
            report['runs'][-1].update({'exit':process.returncode,'drained':True,'persisted_mode':parsed['window_mode'],'unknown_fields_preserved':True,'log':log.read_text()})
        except Exception as error:
            report['functional_gate']='FAIL';report['error']=repr(error);report['log']=log.read_text()
            raise
        finally:
            if not normal:stop_tree(process)
            if 'functional_gate' in report:
                path=evidence/('window-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2));print(path)
    report['functional_gate']='PASS'
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    path=evidence/('window-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,ensure_ascii=False,indent=2));print(path)
