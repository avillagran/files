#!/usr/bin/env python3
"""Shortcut help functional framebuffer fixtures. No resource/startup sampling/native input."""
import hashlib,json,os,re,signal,subprocess,tempfile,time,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/help';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'visible Preferences help button, shared F1/help-key handlers, own-window fixture resize and FLTK callback; real framebuffer, no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],'tabs_and_session':'PENDING; per-instance location history only'}
with tempfile.TemporaryDirectory(prefix='help-ui-',dir=BASE) as temp:
    base=Path(temp);source=base/'Colección';source.mkdir();config=base/'config';config.mkdir();(config/'config.toml').write_text('fixture_note="preserve"\nshow_hidden=false\n')
    for name in ['one','two','branch']:
        d=source/name;d.mkdir();(d/'item.txt').write_text('owned fixture\n')
    deeper=source/'two'/'deeper';deeper.mkdir();(deeper/'raw.txt').write_text('safe\n')
    raw=base/'capture.ppm';log=base/'app.log';process=None;normal=False;seq=0
    try:
        env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
        with log.open('w') as output:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(source),'--ui-smoke','--ui-scene','hidden-context','--size','1000x560','--scale','1','--exit-after','90'],env=env,stderr=output,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,start_new_session=True)
        def wait(check,reason,allow_exit=False):
            end=time.monotonic()+25
            while time.monotonic()<end:
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode))
                time.sleep(.01)
            raise AssertionError(reason)
        def state(after=0,check=lambda s:True):
            def fresh():
                rows=[]
                for line in log.read_text()[after:].splitlines():
                    if line.startswith('FILES_HIDDEN_STATE '):
                        s=json.loads(line[len('FILES_HIDDEN_STATE '):])
                        if s['seq']==seq and not s['config_dirty'] and check(s):rows.append(s)
                return rows[-1] if rows else None
            s=wait(fresh,'painted state absent, seq='+str(seq));report['states'].append(s);assert s['native_frame_label']=='';return s
        def command(action):
            global seq
            before=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            match=wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',log.read_text()[before:],re.M),'command absent: '+action,action=='callback')
            seq=int(match[1]);return before
        def act(action,check=lambda s:True):return state(command(action),check)
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists() or raw.stat().st_size>64*1024*1024:return None
                parts=raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return None
                w,h=map(int,parts[1].split());return (w,h) if len(parts[3])==w*h*3 else None
            dims=wait(full,'framebuffer incomplete');target=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(target)],check=True,timeout=15);report['captures'].append({'path':str(target),'dimensions':dims});return dims
        initial=state();act('grid');act('preferences');opened=act('help');assert opened['help'] and opened['help']['top']==0 and not opened['preferences'] and not opened['entry_hits'];capture('help-navigation')
        path=opened['path'];selection=opened['selection'];page=opened['help']['page']
        moved=act('help-page');assert moved['help']['top']==page and moved['path']==path and moved['selection']==selection;capture('help-selection')
        end=act('help-end');assert end['help']['top']>moved['help']['top'];capture('help-operations-preview')
        narrow=act('fixture-narrow',lambda s:s['width']==800 and s['height']==500);assert narrow['help']['page']<page and not narrow['entry_hits'];capture('help-narrow')
        closed=act('help-close');assert closed['help'] is None and closed['entry_hits'] and closed['path']==path
        reopened=act('help-key');assert reopened['help']['top']==0
        closed=act('help-escape');assert closed['help'] is None and closed['selection']==selection
        act('help-key')
        close=command('callback');wait(lambda:process.poll() is not None,'normal close absent',True)
        tail=log.read_text()[close:];tokens=['FILES_CLOSE_CALLBACK source=fltk_callback','FILES_CLOSE_REQUEST callback=true','FILES_CLOSE_DRAINED'];positions=[tail.find(t) for t in tokens]
        assert all(p>=0 for p in positions) and positions==sorted(positions) and process.returncode==0,tail
        assert (deeper/'raw.txt').read_text()=='safe\n' and not (deeper/'missing').exists()
        assert tomllib.loads((config/'config.toml').read_text())['fixture_note']=='preserve'
        report.update(functional_gate='PASS',preferences_visible_help_button=True,help_key_toggle_escape=True,keyboard_pages_and_resize=True,underlying_file_hits_removed=True,selection_and_path_preserved=True,callback_with_help_open_exit0=True,scope='Static implemented-shortcut help. Internal shared handlers and actual framebuffer; no native F1/scroll/clipboard. No preview/du/treemap helpers requested; root lifecycle only. Does not complete all F1-F10/terminal/i18n/accessibility roadmap.');normal=True
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['log_before_cleanup']=log.read_text() if log.exists() else ''
        if process is not None and not normal and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        target=OUT/('help-ui-'+str(time.time_ns())+'.json')
        with target.open('x') as out:json.dump(report,out,ensure_ascii=False,indent=2)
        print(target)
