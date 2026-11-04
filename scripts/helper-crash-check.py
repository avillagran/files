#!/usr/bin/env python3
"""Linux owned-helper death on root SIGKILL; no PSS/CPU/startup sampling or OS input."""
import ctypes, hashlib, json, os, re, signal, subprocess, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/0.0.4';OUT.mkdir(parents=True,exist_ok=True)
report={'method':'private source fixtures, shared handlers, root-only SIGKILL (not process group), Linux process-local subreaper and PID/start checks','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','native_input':'NOT_VALIDATED','cases':[],'captures':[],'limitations':['Direct owned children only; no promise about kernel D-state/setuid exec/grandchildren','No mutations in flight; not task-journal recovery or power-loss proof','No audible verification; audio fixture is zero PCM','No PSS/Swap/CPU/GPU/startup/lifetime sampling']}
libc=ctypes.CDLL(None,use_errno=True)
assert libc.prctl(36,1,0,0,0)==0,'private subreaper unavailable' # Only this script process.
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(matches)==1
ENV={**os.environ,'HYPRLAND_INSTANCE_SIGNATURE':matches[0]['instance']}
def fields(pid,start=None):
    proc=Path(f'/proc/{pid}')
    for attempt in range(2):
        try:f=(proc/'stat').read_text().rsplit(')',1)[1].split()
        except (FileNotFoundError,ProcessLookupError):
            if not proc.exists():return None
            if attempt==0:continue
            raise AssertionError('known proc directory survives unreadable stat')
        return f if start is None or int(f[19])==start else None

def reap_known(pid,start):
    deadline=time.monotonic()+5
    while time.monotonic()<deadline:
        f=fields(pid,start)
        if f is None:return {'status':'absent_or_reused','signal':None}
        try:got,status=os.waitpid(pid,os.WNOHANG)
        except ChildProcessError:got=0
        if got:
            return {'status':'adopted_and_reaped','signal':os.WTERMSIG(status) if os.WIFSIGNALED(status) else None,'exit':os.WEXITSTATUS(status) if os.WIFEXITED(status) else None}
        time.sleep(.005)
    raise AssertionError('owned helper survived root death or stayed unreaped')

try:
 with tempfile.TemporaryDirectory(prefix='helper-crash-',dir=BASE) as tmp:
    t=Path(tmp);tool_tmp=t/'tools';tool_tmp.mkdir();ENV.update(TMPDIR=str(tool_tmp),MAGICK_TEMPORARY_PATH=str(tool_tmp),MAGICK_MEMORY_LIMIT='32MiB',MAGICK_MAP_LIMIT='0');folder=t/'files';folder.mkdir();desc=folder/'folder';desc.mkdir()
    for i in range(20000):(desc/f'f{i:05}').touch()
    subprocess.run(['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','anullsrc=r=48000:cl=mono','-t','8','-c:a','flac','-threads','1',str(folder/'song.flac')],check=True,timeout=10)
    subprocess.run(['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','color=c=blue:s=320x180:r=12:d=8','-c:v','ffv1','-threads','1',str(folder/'clip.mkv')],check=True,timeout=10)
    originals={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.is_file()}
    for case,scene,commands,label in [('audio','audio',['audio','audio-play'],'preview_audio_playback'),('video','video',['video','video-play'],'preview_video_stream'),('folder','folder',['folder'],'preview_directory_du'),('map','space-map',['map'],'space_map')]:
        location=t/case;location.mkdir();raw=location/'capture.ppm';log=location/'app.log';normal=False;record={'case':case,'scene':scene,'target_label':label};report['cases'].append(record)
        env={**ENV,'FILES_CONFIG_DIR':str(location/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1'}
        if case=='map':env['FILES_TEST_SPACE_MAP']='1'
        with log.open('w') as output:p=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene',scene,'--size','1000x650','--exit-after','30'],env=env,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        def text():return log.read_text()
        def wait(check,reason,allow_exit=False):
            deadline=time.monotonic()+12
            while time.monotonic()<deadline:
                result=check()
                if result:return result
                if p.poll() is not None and not allow_exit:raise AssertionError(reason+'; root exited '+str(p.returncode))
                time.sleep(.003)
            raise AssertionError(reason)
        def command(action):
            before=len(text());request=raw.with_suffix('.pending');request.write_text(action);os.replace(request,raw.with_suffix('.command'))
            prefix='FILES_SPACE_COMMAND ' if case=='map' else 'FILES_PREVIEW_COMMAND_OK '
            wait(lambda:prefix in text()[before:] and action in text()[before:],'command missing '+action)
            return before
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def complete():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(complete,'framebuffer incomplete');parts=raw.read_bytes().split(b'\n',3)
            path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=ENV,check=True,timeout=5)
            report['captures'].append({'path':str(path),'dimensions':list(map(int,parts[1].split()))})
        try:
            wait(lambda:'FILES_UI_SMOKE_OK' in text() and 'FILES_FIRST_PAINT_US' in text(),'fixture not painted')
            if case=='map':wait(lambda:'FILES_SPACE_STATE ' in text(),'map toolbar not painted')
            for action in commands:
                before=command(action)
                if action in ('audio','video'):
                    marker='FILES_PREVIEW_READY ' if action=='audio' else 'FILES_PREVIEW_VIDEO_READY '
                    wait(lambda:marker in text()[before:],'metadata/snapshot not ready')
            def alive():
                for pid,start,kind in reversed(re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=(\S+)',text())):
                    if kind==label and (f:=fields(int(pid),int(start))) and f[0]!='Z':return int(pid),int(start)
            pid,start=wait(alive,'target helper not observed alive')
            if case in ('audio','video'):capture('owned-'+case+'-before-root-kill')
            f=fields(pid,start);assert f and f[0]!='Z','helper ended before root kill'
            record['helper_alive_before_root_kill']={'pid':pid,'start':start}
            p.kill();p.wait(timeout=3);assert p.returncode==-signal.SIGKILL
            record['root_exit']=p.returncode;record['helper_death']=reap_known(pid,start)
            assert record['helper_death']['signal']==signal.SIGKILL,'helper exit not confirmed as SIGKILL after parent death'
            assert fields(pid,start) is None,'same helper identity remains, including zombie'
            normal=True;record['functional_gate']='PASS'
        finally:
            record['log']=text()
            if p.poll() is None:p.kill();p.wait(timeout=3)
            if not normal:
                # Emergency cleanup is never used to pass the parent-death gate.
                for pid,start,_ in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=(\S+)',text()):
                    if fields(int(pid),int(start)):
                        try:os.kill(int(pid),signal.SIGKILL)
                        except ProcessLookupError:pass
                        reap_known(int(pid),int(start))
    # Restart the same private config after forced termination. No mutations were
    # in flight, so this is media/launch recovery, not operation-journal proof.
    location=t/'relaunch';location.mkdir();raw=location/'capture.ppm';log=location/'app.log';case='audio';scene='audio'
    env={**ENV,'FILES_CONFIG_DIR':str(t/'audio'/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1'}
    with log.open('w') as output:p=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','audio','--size','1000x650','--exit-after','20'],env=env,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
    try:
        wait(lambda:'FILES_UI_SMOKE_OK' in text(),'restart not ready');before=command('audio');wait(lambda:'FILES_PREVIEW_READY ' in text()[before:] and 'stamp=Some(' in text()[before:],'restart lost verified media')
        capture('owned-media-relaunch');command('callback');p.wait(timeout=8);assert p.returncode==0 and 'FILES_PREVIEW_DRAINED' in text() and 'FILES_CLOSE_DRAINED' in text()
        report['relaunch']={'normal_exit':True,'log':text(),'scope':'same private config/source, no mutations in flight; not durable task recovery'}
    finally:
        if p.poll() is None:p.kill();p.wait(timeout=3)
    assert originals=={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in folder.iterdir() if p.is_file()},'readonly source fixtures changed'
    assert sum(1 for _ in desc.iterdir())==20000
    report['sources_intact']=True;report['functional_gate']='PASS'
except BaseException as error:
 report.update(functional_gate='FAIL',error=repr(error));raise
finally:
 report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
 path=OUT/('helper-crash-'+str(time.time_ns())+'.json')
 with path.open('x') as output:json.dump(report,output,indent=2)
 print(path)
