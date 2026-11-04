#!/usr/bin/env python3
"""Grid functional framebuffer fixtures. No resource/startup sampling/native input."""
import hashlib,json,os,re,signal,subprocess,tempfile,time,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/grid';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'visible grid/zoom/context hits, shared selection/navigation handlers, fixture own-window size constraints and FLTK callback; framebuffer real, no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],'thumbnails':'PENDING; Nerd glyphs only'}
with tempfile.TemporaryDirectory(prefix='grid-ui-',dir=BASE) as temp:
    base=Path(temp);source=base/'Colección';source.mkdir();config=base/'config';config.mkdir();(config/'config.toml').write_text('fixture_note="preserve"\nshow_hidden=false\n')
    child=source/'Álbum';child.mkdir();(child/'único.txt').write_text('fixture\n')
    for i in range(160):
        ext=['png','pdf','mp4','flac','md','zip'][i%6];(source/f'archivo{i}.{ext}').write_text('fixture bytes\n')
    (source/'.secreto').write_text('hidden fixture\n')
    for name in [b'.\xff',b'raw\xff']:
        with open(os.fsencode(source)+b'/'+name,'wb') as f:f.write(b'fixture raw\n')
    expected={os.fsencode(n).hex() for n in os.listdir(source) if not os.fsencode(n).startswith(b'.')}
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
            s=wait(fresh,'painted state absent, seq='+str(seq));report['states'].append(s);assert not s['error'] and s['native_frame_label']=='';return s
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
        state();s=act('grid');assert s['view']=='grid' and set(s['order'])==expected and len(s['visible'])<len(expected)
        act('inspector-close');initial=state();assert capture('grid-default')==(1000,560)
        s=act('grid-first');assert s['selected']==s['order'][0]
        s=act('grid-range');assert len(s['selection'])==s['grid_columns']+1;capture('grid-range-selection')
        act('grid-zoom-in');s=act('grid-zoom-in');assert s['grid_zoom']>initial['grid_zoom'] and s['grid_columns']<initial['grid_columns'];capture('grid-zoom')
        s=act('grid-last');assert s['selected']==s['order'][-1] and s['selected'] in s['visible'] and s['scroll']>0;capture('grid-last-visible')
        last=s;s=act('grid-context');assert s['context']['target']=='Entries' and len(s['selection'])==1;capture('grid-context')
        act('escape');act('grid-first');s=act('grid-open-folder');assert s['path']==str(child) and len(s['order'])==1 and s['scroll']==0;capture('grid-folder')
        s=act('back');assert s['path']==str(source) and set(s['order'])==expected and s['scroll']==0
        s=act('toggle-shortcut');assert len(s['order'])==len(expected)+2
        s=act('toggle-shortcut');assert set(s['order'])==expected and all(not bytes.fromhex(n).startswith(b'.') for n in s['entry_hits']+s['selection'])
        before=state();s=act('fixture-narrow',lambda s:s['width']==800 and s['height']==500);assert s['grid_columns']<before['grid_columns'];capture('grid-resize-narrow')
        s=act('fixture-wide',lambda s:s['width']==before['width'] and s['height']==before['height']);assert s['grid_columns']==before['grid_columns'];capture('grid-return-wide')
        act('list');s=act('grid');assert s['view']=='grid' and s['grid_zoom']==last['grid_zoom'] and len(s['visible'])<len(s['order'])
        close=command('callback');wait(lambda:process.poll() is not None,'normal close absent',True)
        tail=log.read_text()[close:];tokens=['FILES_CLOSE_CALLBACK source=fltk_callback','FILES_CLOSE_REQUEST callback=true','FILES_CLOSE_DRAINED'];positions=[tail.find(t) for t in tokens]
        assert all(p>=0 for p in positions) and positions==sorted(positions) and process.returncode==0,tail
        assert (child/'único.txt').read_text()=='fixture\n' and len(os.listdir(source))==len(expected)+2
        saved=tomllib.loads((config/'config.toml').read_text());assert saved['fixture_note']=='preserve' and saved['show_hidden'] is False and saved['window_mode']=='normal'
        report.update(functional_gate='PASS',visible_rows_only=True,zoom_controls=True,keyboard_selection_handlers=True,range_selection=True,raw_hidden_filter=True,context_menu=True,navigation_back=True,fixture_native_viewport_resize=True,callback_drain_exit0=True,visible_entry_count=len(expected),scope='Owned fixture with two hidden names; runtime/framebuffer handler fixture, not 100k app resource gate or native input. No preview/du/space helper requested; root lifecycle only. Zoom per running app, thumbnails/preferences per folder pending.');normal=True
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['log_before_cleanup']=log.read_text() if log.exists() else ''
        if process is not None and not normal and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        target=OUT/('grid-ui-'+str(time.time_ns())+'.json')
        with target.open('x') as out:json.dump(report,out,ensure_ascii=False,indent=2)
        print(target)
