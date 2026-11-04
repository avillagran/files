#!/usr/bin/env python3
"""Owned functional fixtures + in-app handlers/framebuffer only; performance frozen."""
import hashlib,json,os,re,signal,subprocess,tempfile,time,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/hidden-context';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'shared visible hit/context/selection handlers; shortcut action and callback injected internally; actual FLTK framebuffer; no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[],'runs':[]}
def hexname(s):return os.fsencode(s).hex()
def identity(pid):
    try:return Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19]
    except (FileNotFoundError,ProcessLookupError):return None
with tempfile.TemporaryDirectory(prefix='hidden-context-',dir=BASE) as temp:
    base=Path(temp);folder=base/'Archivo';folder.mkdir();config=base/'config';config.mkdir()
    (config/'config.toml').write_text('corners="square"\nfixture_note="preserve"\n')
    sizes={}
    def file(parent,name,size):
        path=parent/name
        with path.open('wb') as f:f.truncate(size)
    mib=1024*1024
    for name,amount in [('Fotos',8),('Documentos',4),('Aplicaciones',6),('.cache',2)]:
        d=folder/name;d.mkdir();file(d,'contenido',amount*mib-(mib if name=='Fotos' else 0))
        if name=='Fotos':file(d,'.original',mib)
        sizes[hexname(name)]=amount*mib
    for name,size in [('video.mp4',2*mib),('README.md',mib),('música.flac',mib),('archivo.zip',mib//2),('.secreto',mib)]:
        file(folder,name,size);sizes[hexname(name)]=size
    for raw in [b'raw\xff',b'.\xff']:
        path=os.fsencode(folder)+b'/'+raw
        with open(path,'wb') as f:f.truncate(mib//4)
        sizes[raw.hex()]=mib//4
    visible={n for n in sizes if not bytes.fromhex(n).startswith(b'.')};total=sum(sizes.values())
    try:
        for run in range(2):
            raw=base/f'capture-{run}.ppm';log=base/f'app-{run}.log';process=None;normal=False;seq=0
            try:
                env={**os.environ,'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
                with log.open('w') as output:
                    process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size','1000x560','--scale','1','--exit-after','120'],env=env,stderr=output,stdout=subprocess.DEVNULL,stdin=subprocess.DEVNULL,start_new_session=True)
                def wait(check,reason,allow_exit=False):
                    end=time.monotonic()+25
                    while time.monotonic()<end:
                        result=check()
                        if result:return result
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
                    s=wait(fresh,'fresh painted state absent, seq='+str(seq));report['states'].append(s)
                    assert s['native_frame_label']=='' and not s['error'],s
                    return s
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
                    dims=wait(full,'framebuffer incomplete');target=OUT/(name+'-'+str(time.time_ns())+'.png')
                    subprocess.run(['magick',str(raw),str(target)],check=True,timeout=15)
                    report['captures'].append({'path':str(target),'dimensions':dims,'theme':'current Omarchy','input':'internal handlers'})
                    return dims
                def verify_hidden(s,show):
                    assert s['show_hidden'] is show,s
                    assert set(s['order'])==(set(sizes) if show else visible),s
                    assert all(show or not bytes.fromhex(n).startswith(b'.') for n in s['entry_hits']+s['selection']),s
                def verify_map(s,show):
                    verify_hidden(s,show);m=s['map'];assert m and m['state']=='ready' and m['total']==total and m['complete'],s
                    wanted=set(sizes) if show else visible
                    # Lossy display names are not used for identity; raw order/hits above remain lossless.
                    assert len(m['areas'])==len(wanted),s
                    area=sum(a['w']*a['h'] for a in m['areas']);denom=sum(sizes[n] for n in wanted)
                    for a in m['areas']:
                        assert abs(a['w']*a['h']/area-a['bytes']/denom)<1e-8,a
                initial=state();verify_hidden(initial,run==1)
                if run==0:
                    for view in ['groups','list','spatial','map']:
                        s=act(view);verify_hidden(s,False)
                        if view=='map':verify_map(s,False)
                    act('inspector-close');s=state();verify_map(s,False)
                    dims=capture('space-v3-hidden-default');report['comparison_dimensions_match_v3']=dims==(1000,560)
                    before_pixels=raw.read_bytes().split(b'\n',3)[3]
                    moved=act('tip-advance');assert moved['tip_offset']>0,moved
                    capture('space-bottom-marquee')
                    after_pixels=raw.read_bytes().split(b'\n',3)[3]
                    assert before_pixels[:dims[0]*(dims[1]-32)*3]==after_pixels[:dims[0]*(dims[1]-32)*3]
                    assert before_pixels!=after_pixels
                    report['marquee_clock_injection_changes_only_bottom_bar']=True
                    s=act('toggle-hit');verify_map(s,True);capture('space-show-hidden')
                    s=act('preferences');assert s['preferences'];capture('preferences-hidden-global')
                    s=act('toggle-hit');verify_map(s,False)
                    act('menus-close');s=act('toggle-shortcut');verify_map(s,True)
                    act('list');act('select-hidden');s=act('toggle-shortcut');verify_hidden(s,False);assert hexname('.secreto') not in s['selection'] and s['selected']!=hexname('.secreto')
                    act('select-batch');s=act('right-selected');assert s['context']['target']=='Entries' and set(s['selection'])=={hexname('README.md'),hexname('video.mp4')};capture('context-selected-batch')
                    s=act('outside');assert s['context'] is None and s['dialog'] is None
                    act('select-batch');s=act('right-outside');assert s['selection']==[hexname('música.flac')]
                    s=act('menu-rename');assert s['dialog']=='rename';capture('context-rename-wired')
                    act('dialog-cancel',lambda s:s['dialog'] is None)
                    act('select-batch');s=act('right-background');assert s['context']['target']=='Background' and not s['selection'] and s['selected'] is None;capture('context-background')
                    s=act('menu-create');assert s['dialog']=='file';act('dialog-cancel',lambda s:s['dialog'] is None)
                    act('select-batch');s=act('menu-key');assert s['context']['target']=='Entries';s=act('escape');assert s['context'] is None
                    s=act('toggle-shortcut');verify_hidden(s,True)
                else:
                    # A new app reads the same global explicit true preference.
                    for view in ['groups','list','spatial','map']:
                        s=act(view);verify_hidden(s,True)
                    s=act('toggle-hit');verify_map(s,False)
                    act('inspector-close');capture('space-global-reopened-hidden')
                before=command('callback');wait(lambda:process.poll() is not None,'normal close failed',True)
                tail=log.read_text()[before:];ordered=['FILES_CLOSE_CALLBACK source=fltk_callback','FILES_CLOSE_REQUEST callback=true','FILES_CLOSE_DRAINED']
                positions=[tail.find(x) for x in ordered];assert all(p>=0 for p in positions) and positions==sorted(positions) and process.returncode==0,tail
                helpers=set((int(p),s) for p,s in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=space_map',log.read_text()))
                assert helpers,'helper launch identity logging absent'
                wait(lambda:all(identity(p)!=s for p,s in helpers),'known helper survived',True)
                saved=tomllib.loads((config/'config.toml').read_text());assert saved['show_hidden'] is (run==0) and saved['fixture_note']=='preserve',saved
                report['runs'].append({'root_exit':0,'callback_drain':True,'known_helper_identities':len(helpers),'known_helper_cleanup':True,'persisted_show_hidden':saved['show_hidden'],'log':log.read_text()});normal=True
            finally:
                if process is not None and not normal:
                    report['failed_log']=log.read_text() if log.exists() else ''
                    if process.poll() is None:
                        os.killpg(process.pid,signal.SIGTERM)
                        try:process.wait(timeout=3)
                        except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        report.update(functional_gate='PASS',global_preference_reopened=True,hidden_default_raw_all_views=True,selection_pruned_before_operations=True,context_batch_background_existing_action_wiring=True,frame_label_removed_at_source=True,treemap_visible_area_denominator_and_total_including_hidden=True,scope='two owned instances, same global setting; internal handlers/framebuffer, no native right click/keyboard/minimize or resource validation')
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
        report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        path=OUT/('hidden-context-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(report,out,ensure_ascii=False,indent=2)
        print(path)
