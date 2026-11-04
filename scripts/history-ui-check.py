#!/usr/bin/env python3
"""Navigation history and bounded omnibar functional framebuffer fixtures. No resource/startup sampling/native input."""
import hashlib,json,os,re,signal,subprocess,tempfile,time,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/history';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'visible history buttons and folder double-click hits, shared bounded-input handler, fixed own-window fixture and FLTK callback; real framebuffer, no native input or clipboard access','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],'tabs_and_session':'PENDING; per-instance location history only'}
with tempfile.TemporaryDirectory(prefix='history-ui-',dir=BASE) as temp:
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
        first=state();assert not first['history_back'] and not first['history_forward']
        act('grid');act('inspector-close')
        one=act('enter-one');assert one['path']==str(source/'one') and one['history_back'] and not one['history_forward']
        root=act('history-back');assert root['path']==str(source) and root['history_forward']
        two=act('enter-two');assert two['path']==str(source/'two') and not two['history_forward']
        nested=act('enter-deeper');assert nested['path']==str(deeper);capture('history-nested')
        two=act('history-back');assert two['path']==str(source/'two') and two['history_forward'];capture('history-back-forward')
        root=act('history-back');assert root['path']==str(source)
        two=act('history-forward');assert two['path']==str(source/'two')
        nested=act('history-forward');assert nested['path']==str(deeper) and not nested['history_forward']
        failed=act('missing');assert failed['path']==str(deeper/'missing') and failed['error'];capture('history-missing-error')
        recovered=act('history-back');assert recovered['path']==str(deeper) and not recovered['error']
        act('history-back');act('history-back');branched=act('enter-branch');assert branched['path']==str(source/'branch') and not branched['history_forward'];capture('history-new-branch')
        editing=act('omni');assert editing['editing'] and editing['replace_edit']
        rejected=act('paste-large');assert rejected['edit']==editing['edit'] and rejected['replace_edit'] and '4096' in rejected['error'];capture('omnibar-rejected-paste')
        recovered=act('paste-small');assert recovered['edit']=='literal — á' and not recovered['replace_edit'] and not recovered['error'];capture('omnibar-recovered')
        act('omni-cancel')
        close=command('callback');wait(lambda:process.poll() is not None,'normal close absent',True)
        tail=log.read_text()[close:];tokens=['FILES_CLOSE_CALLBACK source=fltk_callback','FILES_CLOSE_REQUEST callback=true','FILES_CLOSE_DRAINED'];positions=[tail.find(t) for t in tokens]
        assert all(p>=0 for p in positions) and positions==sorted(positions) and process.returncode==0,tail
        assert (deeper/'raw.txt').read_text()=='safe\n' and not (deeper/'missing').exists()
        assert tomllib.loads((config/'config.toml').read_text())['fixture_note']=='preserve'
        report.update(functional_gate='PASS',back_forward_visible_controls=True,new_navigation_discards_forward=True,missing_location_can_back=True,oversized_paste_preserves_text_and_replace=True,valid_paste_recovers=True,callback_drain_exit0=True,scope='Owned directories and one missing destination; shared hit/selection/navigation/input handlers, framebuffers, no native keyboard or clipboard access. Per-instance history only; no tabs/session persistence, no resource/startup gate.');normal=True
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['log_before_cleanup']=log.read_text() if log.exists() else ''
        if process is not None and not normal and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        target=OUT/('history-ui-'+str(time.time_ns())+'.json')
        with target.open('x') as out:json.dump(report,out,ensure_ascii=False,indent=2)
        print(target)
