#!/usr/bin/env python3
"""PopupMenu in Files; fixture commands use toolkit handlers, not OS input."""
import hashlib, json, os, re, subprocess, tempfile, time
from pathlib import Path
from memcheck import stop_tree
ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'docs/evidence/0.0.4'
BASE = Path.home() / '.cache/files-test'
OUT.mkdir(parents=True, exist_ok=True); BASE.mkdir(parents=True, exist_ok=True)

def main():
    result = {'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED',
              'method':'Files window framebuffer + toolkit focus::press/click_at fixture; no native input', 'captures':[]}
    process = None
    temporary = tempfile.TemporaryDirectory(prefix='toolkit-menu-', dir=BASE)
    base = Path(temporary.name); log = base/'app.log'; raw = base/'frame.ppm'
    try:
        directory=base/'files';directory.mkdir();(directory/'example.txt').write_text('fixture\n')
        config=base/'config';config.mkdir()
        shell=base/'shell.toml';shell.write_text('[font]\nbase-size = 18\n')
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),
             'FILES_TEST_FIXED_SIZE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_TOOLKIT_POPUP':'1','FILES_CLOSE_TRACE':'1',
             'OMARCHY_FLTK_USER_SHELL':str(shell),'TMPDIR':str(base),'MAGICK_TEMPORARY_PATH':str(base)}
        with log.open('w') as stream:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(directory),'--size','800x440',
                '--scale','1','--ui-smoke','--exit-after','30'],stderr=stream,stdout=subprocess.DEVNULL,env=env,start_new_session=True)
        def wait(check, reason, timeout=12):
            until=time.monotonic()+timeout
            while time.monotonic()<until:
                if check(): return
                if process.poll() is not None: raise RuntimeError(f'{reason}: exit {process.returncode}')
                time.sleep(.03)
            raise RuntimeError(reason)
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'smoke')
        def command(action, marker):
            before=len(log.read_text()); tmp=raw.with_suffix('.pending');tmp.write_text(action)
            os.replace(tmp,raw.with_suffix('.popup-command'))
            wait(lambda:marker in log.read_text()[before:],f'{action}: {marker}')
            return log.read_text()[before:]
        def opened(action, kind):
            text=command(action, f'FILES_TOOLKIT_POPUP_OPEN kind={kind}')
            match=re.search(r'rect=Some\(\((\d+), (\d+), (\d+), (\d+)\)\)',text)
            assert match, text
            x,y,w,h=map(int,match.groups()); assert x>=0 and y>=0 and x+w<=800 and y+h<=440, match.groups()
            return [x,y,w,h]
        def capture(label):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def ready():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and parts[1]==b'800 440' and len(parts[3])==800*440*3
            wait(ready,'framebuffer dimensions')
            target=OUT/f'toolkit-menu-{label}-{time.time_ns()}.png'
            subprocess.run(['magick',str(raw),str(target)],env=env,check=True,timeout=10)
            result['captures'].append(str(target))
        result['settings_rect']=opened('settings','Settings');capture('settings')
        command('help-key','FILES_TOOLKIT_HELP open=true')
        wait(lambda:'reason=Closed valid=true focus_returned=true' in log.read_text(),'F1 dismiss')
        command('help-close','FILES_TOOLKIT_FIXTURE action=help-close')
        opened('settings','Settings')
        command('actions-key','FILES_TOOLKIT_POPUP_OPEN kind=Actions')
        command('settings-key','FILES_TOOLKIT_POPUP_OPEN kind=Settings')
        command('ctrl-j','reason=Closed valid=true focus_returned=true')
        opened('settings','Settings')
        command('escape','reason=Escape valid=true focus_returned=true')
        result['actions_rect']=opened('actions','Actions');capture('actions-start')
        command('end','FILES_TOOLKIT_FIXTURE action=end');capture('actions-end')
        assert Path(result['captures'][-1]).read_bytes()!=Path(result['captures'][-2]).read_bytes(), 'End no cambió el framebuffer del menú'
        command('outside','reason=ClickOutside valid=true focus_returned=true')
        opened('settings','Settings');command('pick','reason=Picked(0) valid=true focus_returned=true')
        result['context_rect']=opened('context','Context');capture('context-edge')
        command('closed','reason=Closed valid=true focus_returned=true')
        opened('context','Context')
        command('actions-key','FILES_TOOLKIT_POPUP_OPEN kind=Actions')
        command('escape','reason=Escape valid=true focus_returned=true')
        opened('context','Context')
        # Entries gained Invert/Clear before Open/Preview/Rename. Two Down
        # selects Open (index2), not Rename; exercise the actual Rename row.
        for _ in range(4):command('down','FILES_TOOLKIT_FIXTURE action=down')
        command('pick','reason=Picked(4) valid=true focus_returned=true rename_open=true')
        command('cancel-rename','FILES_TOOLKIT_FIXTURE action=cancel-rename')
        opened('actions','Actions')
        command('close','FILES_TOOLKIT_FIXTURE action=close')
        wait(lambda:process.poll() is not None,'normal close',timeout=15)
        assert process.returncode==0 and 'kind=Actions reason=Closed valid=false' in log.read_text() and 'FILES_CLOSE_DRAINED' in log.read_text()
        result.update(functional_gate='PASS', normal_exit=True, dismissal_reasons=['Escape','ClickOutside','Picked(0)','Closed'], viewport=[800,440])
    except BaseException as error:
        result['error']=repr(error)
    finally:
        if process is not None and process.poll() is None:stop_tree(process,None)
        if log.exists():result['log']=log.read_text()
        for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:
            result[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
        report=OUT/f'toolkit-menu-ui-{time.time_ns()}.json'
        with report.open('x') as stream:json.dump(result,stream,indent=2)
        temporary.cleanup();print(json.dumps({'report':str(report),'functional_gate':result['functional_gate']}))
    return 0 if result['functional_gate']=='PASS' else 1
if __name__=='__main__':raise SystemExit(main())
