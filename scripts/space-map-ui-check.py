#!/usr/bin/env python3
"""Functional treemap fixtures/framebuffers only. No resource/startup sampling or OS input."""
import hashlib,json,os,re,signal,subprocess,tempfile,time,tomllib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
EVIDENCE=ROOT/'docs/evidence/0.0.4';EVIDENCE.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'visible hit handlers + shared navigation/mode/callback handlers + actual FLTK framebuffer; no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[]}
def identity(pid):
    try:
        s=Path(f'/proc/{pid}/stat').read_text();return s.rsplit(')',1)[1].split()[19]
    except (FileNotFoundError,ProcessLookupError):return None
with tempfile.TemporaryDirectory(prefix='space-ui-',dir=BASE) as temp:
    base=Path(temp);folder=base/'source';folder.mkdir();partial=base/'partial';partial.mkdir();busy=base/'busy';busy.mkdir()
    restricted=partial/'restringida';restricted.mkdir();(restricted/'hidden').write_bytes(b'not traversed')
    (partial/'known').write_bytes(b'known');os.chmod(restricted,0)
    for name,mib in [('Fotos',8),('Documentos',4)]:
        (folder/name).mkdir()
        with (folder/name/'contenido').open('wb') as f:f.truncate(mib*1024*1024)
    with (folder/'video.bin').open('wb') as f:f.truncate(2*1024*1024)
    (folder/'vacío.txt').touch();(folder/'vacía').mkdir();outside=base/'outside';outside.write_bytes(b'excluded');(folder/'enlace').symlink_to(outside)
    # Bounded owned workload to exercise close/cancellation during a real scan;
    # sizes/counts are fixture inputs, not throughput or performance measurements.
    (busy/'items').mkdir()
    for i in range(20000):(busy/'items'/str(i)).touch()
    config=base/'config';config.mkdir();(config/'config.toml').write_text('corners="square"\nfixture_note="preserve"\n')
    raw=base/'capture.ppm';log=base/'app.log';process=None;normal=False;seq=0
    try:
        env={**os.environ,'FILES_TEST_ISOLATE_INPUT':'1','FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_SPACE_MAP':'1','FILES_TEST_PREVIEW':'1','FILES_CLOSE_TRACE':'1'}
        with log.open('w') as output:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','space-map','--exit-after','90'],env=env,stderr=output,stdout=subprocess.DEVNULL,stdin=subprocess.DEVNULL,start_new_session=True)
        def wait(check,reason,allow_exit=False,timeout=20):
            until=time.monotonic()+timeout
            while time.monotonic()<until:
                result=check()
                if result:return result
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; root exit='+str(process.returncode))
                time.sleep(.01)
            raise AssertionError(reason)
        def state(kind=None,after=0,extra=lambda s:True):
            def fresh():
                rows=[]
                for line in log.read_text()[after:].splitlines(keepends=True):
                    if not line.endswith('\n'):continue
                    if line.startswith('FILES_SPACE_STATE '):
                        s=json.loads(line[len('FILES_SPACE_STATE '):])
                        if s['seq']==seq and (kind is None or s['state']==kind) and extra(s):rows.append(s)
                return rows[-1] if rows else None
            result=wait(fresh,'fresh painted space state absent: '+str(kind));report['states'].append(result);return result
        def command(action):
            global seq
            before=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            match=wait(lambda:re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',log.read_text()[before:],re.M),'command absent: '+action,action=='callback')
            seq=int(match[1]);return before
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists() or raw.stat().st_size>64*1024*1024:return None
                parts=raw.read_bytes().split(b'\n',3)
                if len(parts)!=4:return None
                w,h=map(int,parts[1].split());return (w,h) if len(parts[3])==w*h*3 else None
            dims=wait(full,'framebuffer incomplete');target=EVIDENCE/(name+'-'+str(time.time_ns())+'.png')
            subprocess.run(['magick',str(raw),str(target)],check=True,timeout=15)
            report['captures'].append({'path':str(target),'dimensions':dims})
        def ratios(s):
            areas={a['name']:a for a in s['areas']};assert set(areas)=={'Fotos','Documentos','video.bin'},areas
            total=sum(a['w']*a['h'] for a in areas.values())
            for name,weight in [('Fotos',8),('Documentos',4),('video.bin',2)]:
                a=areas[name];assert a['bytes']==weight*1024*1024
                assert abs(a['w']*a['h']/total-weight/14)<1e-9
            assert s['total']==14*1024*1024 and s['complete'] is True
        state(extra=lambda s:s['view']=='other')
        before=command('map');initial=state('ready',before);ratios(initial)
        before=command('full');wide=state('ready',before,lambda s:s['width']!=initial['width']);ratios(wide);capture('space-map-wide')
        before=command('select-photos');selected=state('ready',before);assert selected['selected']=='Fotos';capture('space-map-inspector')
        before=command('enter-photos');inside=state('ready',before);assert inside['path']==str(folder/'Fotos') and inside['total']==8*1024*1024 and inside['nodes']==1;capture('space-map-folder')
        before=command('back');ratios(state('ready',before))
        before=command('select-zero');assert state('ready',before)['selected']=='vacío.txt';capture('space-map-zero-selection')
        before=command('empty');empty=state('ready',before);assert empty['nodes']==0 and empty['total']==0 and empty['areas']==[] and empty['complete'];capture('space-map-empty')
        (folder/'vacía').rmdir()
        before=command('refresh');error=state('error',before);assert error['error'] and error['drained'];capture('space-map-error')
        before=command('back');ratios(state('ready',before))
        before=command('partial');incomplete=state('ready',before);assert incomplete['complete'] is False and incomplete['total']==5 and incomplete['observed']==5 and len(incomplete['areas'])==1;capture('space-map-partial')
        before=command('root');ratios(state('ready',before))
        before=command('normal');resized=state('ready',before,lambda s:s['width']!=wide['width']);ratios(resized);capture('space-map-resize')
        before=command('list');state('closed',before,lambda s:s['drained'] and s['view']=='other')
        before=command('map');ratios(state('ready',before))
        before=command('busy');state('loading',before)
        before_cancel=command('cancel');assert state('closed',before_cancel,lambda s:s['drained'])['areas']==[];capture('space-map-cancelled')
        before=command('refresh')
        helper=wait(lambda:re.search(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=space_map',log.read_text()[before:]),'busy scan helper not launched')
        pid,start=int(helper[1]),helper[2];report['helper_alive_before_callback']=identity(pid)==start
        before=command('callback');wait(lambda:process.poll() is not None,'normal close failed',True)
        tail=log.read_text()[before:]
        ordered=['FILES_CLOSE_CALLBACK source=fltk_callback','FILES_CLOSE_REQUEST callback=true','FILES_CLOSE_DRAINED']
        positions=[tail.find(s) for s in ordered];assert all(p>=0 for p in positions) and positions==sorted(positions)
        assert process.returncode==0 and 'FILES_SPACE_DRAINED' in tail
        helpers=set((int(p),s) for p,s in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=space_map',log.read_text()))
        wait(lambda:all(identity(p)!=s for p,s in helpers),'known helper identity survived normal close',True)
        assert (folder/'Fotos'/'contenido').stat().st_size==8*1024*1024 and (folder/'Documentos'/'contenido').stat().st_size==4*1024*1024
        assert (folder/'video.bin').stat().st_size==2*1024*1024 and (folder/'enlace').is_symlink() and outside.read_bytes()==b'excluded'
        saved=tomllib.loads((config/'config.toml').read_text());assert saved['window_mode']=='normal' and saved['fixture_note']=='preserve'
        report.update(functional_gate='PASS',proportional_bytes_8_4_2=True,selection_inspector=True,navigation_back=True,empty_partial_error=True,real_viewport_resize=True,cancel_and_leave_release=True,normal_callback_exit_after_drain=True,helper_identities_observed=len(helpers),known_helper_cleanup_complete=True,root_exit=process.returncode,fixture_contents_preserved=True,scope='flat immediate children; apparent bytes by name; geometry areas exact before visual 2px gaps; no snapshot/dedup/native input/resource gate')
        normal=True
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        report['log_before_cleanup']=log.read_text() if log.exists() else ''
        if process is not None and not normal and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        os.chmod(restricted,0o700)
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
        report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        target=EVIDENCE/('space-map-ui-'+str(time.time_ns())+'.json')
        with target.open('x') as out:json.dump(report,out,ensure_ascii=False,indent=2)
        print(target)
