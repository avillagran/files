#!/usr/bin/env python3
"""Functional PCM correctness via a private Pulse null sink, never speakers.
No PSS/Swap/CPU/startup/throughput measurement. Shared UI handlers + framebuffer,
not OS input. Only the fixture-owned monitor/module and Popen processes are closed.
"""
import argparse
import array
import hashlib
import json
import math
import os
from pathlib import Path
import re
import selectors
import signal as process_signal
import stat
import shlex
import shutil
import subprocess
import tempfile
import time
import wave
from memcheck import geometry, observed_geometry, stop_tree

ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
EVIDENCE=ROOT/'docs/evidence/0.0.4'
parser=argparse.ArgumentParser();parser.add_argument('--zero-routing',action='store_true');args=parser.parse_args()
report={'zero_pcm_routing_probe':args.zero_routing,'routing':[],'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','method':'shared UI handlers, actual FLTK framebuffer and private Pulse monitor PCM; no native input/clipboard or physical speaker audibility','captures':[],'checks':[],'pcm':[]}
def terminate_fixture(signum,frame):raise KeyboardInterrupt('fixture terminated')
process_signal.signal(process_signal.SIGTERM,terminate_fixture)
creation_attempted=False
module=None;module_name='files_test_'+str(os.getpid())+'_'+str(time.time_ns())
process=None;recorder=None;normal=False;default_before=None

def pactl(*args):
    return subprocess.check_output(['pactl',*args],text=True,timeout=5).strip()
def owned_module():
    # This host's pactl JSON omits module IDs; short mode supplies ID/name/args
    # separated by TAB. Match BOTH exact module kind and unique sink_name.
    matches=[]
    for row in pactl('list','short','modules').splitlines():
        parts=row.split('\t')
        if len(parts)>=3 and parts[0].isdigit() and parts[1]=='module-null-sink' and 'sink_name='+module_name in shlex.split(parts[2]):
            if module is None or int(parts[0])==module:matches.append({'index':int(parts[0])})
    assert len(matches)<=1,'ambiguous private module ownership'
    return matches[0] if matches else None
def gone(pid,start):
    try:return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])!=start
    except FileNotFoundError:return True

def spectrum(values,freq):
    coefficient=2*math.cos(2*math.pi*freq/48000);a=b=0.0
    for value in values:a,b=value+coefficient*a-b,a
    return max(0,a*a+b*b-coefficient*a*b)
def signal(data,frequency=None):
    values=array.array('h');values.frombytes(data[:len(data)//4*4])
    if __import__('sys').byteorder!='little':values.byteswap()
    left=values[::2];right=values[1::2]
    assert len(left)>=12000,'not enough PCM frames for functional check'
    # Choose a bounded half-second window after the first nonzero signal.
    start=next((i for i,v in enumerate(left) if abs(v)>100),0) if frequency else 0
    window=left[start:start+24000] if frequency else left[-12000:];assert len(window)>=12000,'PCM signal window incomplete'
    rms=math.sqrt(sum(v*v for v in window)/len(window));peak=max(map(abs,window))
    result={'expected_hz':frequency,'pcm_frames':len(left),'window_frames':len(window),'rms_s16':rms,'peak_s16':peak,'kind':'audio sample correctness, not performance metric'}
    if frequency:
        wrong=880 if frequency==440 else 440
        ratio=spectrum(window,frequency)/max(1,spectrum(window,wrong))
        result['target_to_other_tone_energy_ratio']=ratio
        assert 100<rms<20000 and peak>200 and ratio>100,('wrong or missing PCM tone',result)
        assert max(abs(a-b) for a,b in zip(left,right))<=2,'stereo channels differ for mono fixture'
    else:assert rms<3 and peak<10,('muted/paused PCM not silent',result)
    return result
try:
    for tool in ['pactl','parec','ffmpeg','magick']:assert shutil.which(tool),'missing '+tool
    socket=Path(os.environ.get('XDG_RUNTIME_DIR','/run/user/'+str(os.getuid())))/'pulse/native'
    status=socket.stat();assert stat.S_ISSOCK(status.st_mode) and status.st_uid==os.getuid(),'local user Pulse socket unavailable'
    os.environ['PULSE_SERVER']='unix:'+str(socket)
    report['server_transport']='local user Unix socket, fixture-only environment'
    default_before=pactl('get-default-sink')
    assert default_before,'refusing sink creation without an existing default'
    creation_attempted=True
    answer=pactl('load-module','module-null-sink','sink_name='+module_name,'rate=48000','channels=2','sink_properties=device.description=Files_private_fixture')
    assert answer.isdigit(),'module ID unavailable'
    module=int(answer);report['created_module']={'name':module_name,'id':module};assert owned_module(),'private module identity mismatch'
    assert pactl('get-default-sink')==default_before,'default sink changed; fixture stops without changing it'
    sink_rows=[row.split('\t') for row in pactl('list','short','sinks').splitlines()]
    sink=next(s for s in sink_rows if len(s)>=2 and s[1]==module_name)
    sink_index=int(sink[0])
    sink_json=next(r for r in json.loads(pactl('-f','json','list','sinks')) if r['name']==module_name)
    report['sink_identity_crosscheck']={'short_index':sink_index,'json_index':sink_json['index']}
    assert sink_index==int(sink_json['index']),'private sink index differs between short/JSON'
    report['private_sink']={'name':module_name,'module_id':module,'sink_index':sink_index,'explicit_device':True,'global_default_changed':False}
    instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
    assert len(matches)==1,'no unique compositor for fixture'
    previous_signature=os.environ.get('HYPRLAND_INSTANCE_SIGNATURE')
    os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
    report['compositor_binding']={'selected':matches[0],'previous_signature':previous_signature,'scope':'fixture parent and child only, unique Wayland socket, no focus/global configuration'}
    tmp=tempfile.mkdtemp(prefix='audio-pcm-',dir=BASE)
    test=Path(tmp);folder=test/'files';folder.mkdir()
    def wav_fixture(path,frequency):
        tone=array.array('h',(int(4096*math.sin(2*math.pi*frequency*i/48000)) for i in range(48000)))
        if __import__('sys').byteorder!='little':tone.byteswap()
        with wave.open(str(path),'wb') as audio:
            audio.setnchannels(1);audio.setsampwidth(2);audio.setframerate(48000)
            audio.writeframesraw(bytes(2*48000*2))
            for _ in range(10):audio.writeframesraw(bytes(len(tone)*2) if args.zero_routing else tone.tobytes())
    wav_fixture(test/'audio440.wav',440);wav_fixture(test/'audio880.wav',880)
    subprocess.run(['ffmpeg','-v','error','-nostdin','-i',str(test/'audio440.wav'),'-c:a','flac','-threads','1',str(folder/'song.flac')],check=True,timeout=10)
    subprocess.run(['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','color=c=blue:s=320x180:r=12:d=12','-i',str(test/'audio880.wav'),'-map','1:a:0','-map','0:v:0','-c:a','flac','-c:v','ffv1','-threads','1',str(folder/'clip.mkv')],check=True,timeout=15)
    for source in (folder/'song.flac',folder/'clip.mkv'):
        metadata=json.loads(subprocess.check_output(['ffprobe','-v','error','-show_entries','format=duration','-of','json',str(source)],timeout=5))
        assert abs(float(metadata['format']['duration'])-12)<.001,'fixture does not contain exactly12s'
    report['source_fixture']={'method':'WAV built sample by sample: 96000 zero samples then480000 mono sine samples, s16/48000; encoded without timing filters','duration_seconds':12,'silence_preamble_frames':96000,'metadata_crosscheck':True}
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'HYPRLAND_INSTANCE_SIGNATURE':matches[0]['instance'],'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_AUDIO_SINK':module_name}
    env.pop('FILES_TEST_AUDIO_NULL',None)
    with log.open('w') as output:process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','audio','--exit-after','40'],env=env,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
    def wait(check,reason,allow_exit=False,timeout=8):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; app exited')
            time.sleep(.02)
        raise AssertionError(reason)
    def command(action):
        after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
        match=wait(lambda:re.search(r'FILES_PREVIEW_COMMAND_REV action='+re.escape(action)+r' revision=(\d+)',log.read_text()[after:]),'command absent: '+action,action=='callback')
        return int(match[1]),after
    def marker(after,prefix,revision=None,*flags):
        return wait(lambda:next((r for r in reversed(log.read_text()[after:].splitlines()) if r.startswith(prefix) and (revision is None or f'revision={revision} ' in r) and all(f in r for f in flags)),False),'fresh state absent: '+prefix)
    def helper(after,phase):
        rows=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command='+phase,log.read_text()[after:]);assert rows,'helper absent'
        pid,start=map(int,rows[-1]);assert not gone(pid,start),'helper already gone'
        return pid,start
    def routed(pid):
        rows=json.loads(pactl('-f','json','list','sink-inputs'))
        own=[r for r in rows if r.get('properties',{}).get('application.process.id')==str(pid)]
        for r in own:
            entry={'pid':pid,'actual_sink':int(r['sink']),'private_sink':sink_index,'target_object':r.get('properties',{}).get('target.object')}
            if len(report['routing'])<16 and entry not in report['routing']:report['routing'].append(entry)
            if int(r['sink'])==4294967295:return False
            assert int(r['sink'])==sink_index,'own audio routed outside private sink'
        return bool(own)
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'framebuffer incomplete')
        path=EVIDENCE/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],check=True)
        report['captures'].append({'path':str(path),'framebuffer_dimensions':[int(n) for n in raw.read_bytes().split(b'\n',3)[1].split()]})
    wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'UI not ready')
    wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry absent')
    report['geometry']=observed_geometry(process.pid,1000,650)
    revision,after=command('audio');marker(after,'FILES_PREVIEW_READY',revision)
    recorder=subprocess.Popen(['parec','--raw','--format=s16le','--rate=48000','--channels=2','--latency-msec=20','--client-name=files-private-fixture','--device='+module_name+'.monitor'],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,start_new_session=True)
    os.set_blocking(recorder.stdout.fileno(),False);selector=selectors.DefaultSelector();selector.register(recorder.stdout,selectors.EVENT_READ)
    def drain():
        for _ in range(64):
            try:data=os.read(recorder.stdout.fileno(),65536)
            except BlockingIOError:return
            if not data:raise AssertionError('PCM recorder exited')
    def pcm(seconds):
        data=bytearray();deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            if recorder.poll() is not None:raise AssertionError('PCM recorder failed')
            for _,_ in selector.select(min(.05,max(0,deadline-time.monotonic()))):
                try:block=os.read(recorder.stdout.fileno(),65536)
                except BlockingIOError:continue
                if not block:raise AssertionError('PCM EOF')
                assert len(data)+len(block)<=1024*1024,'fixture PCM cap exceeded'
                data.extend(block)
        return data
    revision,after=command('audio-play');marker(after,'FILES_PREVIEW_READY',revision,'audio_running=true')
    player=helper(after,'preview_audio_playback');wait(lambda:routed(player[0]),'audio not routed to private sink');capture('audio-private-pcm-playing')
    report['pcm'].append({'phase':'audio440',**signal(pcm(3.25),None if args.zero_routing else 440)})
    revision,after=command('audio-pause');marker(after,'FILES_PREVIEW_READY',revision,'audio_requested=false','drained=true');assert gone(*player),'paused helper survived'
    pcm(.3);drain();report['pcm'].append({'phase':'paused',**signal(pcm(.6))})
    revision,after=command('video');marker(after,'FILES_PREVIEW_VIDEO_READY',revision)
    revision,after=command('video-play');marker(after,'FILES_PREVIEW_VIDEO_READY',revision,'playing=true','audio_enabled=false','sequence=Some(')
    pcm(2.3);drain();report['pcm'].append({'phase':'video_default_muted',**signal(pcm(.6))})
    revision,after=command('video-pause');marker(after,'FILES_PREVIEW_VIDEO_READY',revision,'playing=false')
    revision,after=command('video-back');marker(after,'FILES_PREVIEW_VIDEO_READY',revision,'playing=false','position=0.000000')
    revision,after=command('video-audio');marker(after,'FILES_PREVIEW_VIDEO_READY',revision,'audio_enabled=true')
    revision,after=command('video-play');marker(after,'FILES_PREVIEW_VIDEO_READY',revision,'playing=true','audio_enabled=true','sequence=Some(')
    video=helper(after,'preview_video_stream');wait(lambda:routed(video[0]),'video audio not routed to private sink');capture('video-private-pcm-playing')
    drain();report['pcm'].append({'phase':'video880',**signal(pcm(3.25),None if args.zero_routing else 880)})
    current=Path(f'/proc/{video[0]}/stat').read_text().rsplit(')',1)[1].split();assert int(current[19])==video[1] and current[0]!='Z','stream not live immediately before callback'
    report['active_stream_at_callback']={'pid':video[0],'start':video[1]}
    command('callback');wait(lambda:process.poll() is not None,'normal close absent',True)
    assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'close ACK missing'
    identities=sorted(set(tuple(map(int,r)) for r in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=',log.read_text())))
    assert identities and all(gone(*r) for r in identities),'logged helper survived close (zombies not accepted)'
    report['logged_helper_cleanup']={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start after root0 ACK, not lifetime/resource coverage'}
    report['log']=log.read_text();normal=True
    report['checks'].append('fixture sources include 2s silence preamble for route confirmation before tone; default-muted video checked after preamble, then seek0 paused before opt-in')
    report['checks'].append(('ZERO PCM ROUTING PROBE ONLY; no tone correctness. ' if args.zero_routing else '')+'actual audio440 PCM, pause silence, video default silence, opt-in audio880 through SAME video child; explicit private device confirmed by own sink-input PID, no default sink change')
    report['functional_gate']='PASS'
except BaseException as error:
    report['error']=repr(error)
    if 'log' in locals() and log.exists():report['log']=log.read_text()
    raise
finally:
    cleanup_errors=[]
    if process is not None and not normal:
        try:stop_tree(process,None)
        except BaseException as error:cleanup_errors.append('root emergency cleanup: '+repr(error))
    if recorder is not None:
        try:
            if recorder.poll() is None:recorder.terminate()
            try:recorder.wait(timeout=3)
            except subprocess.TimeoutExpired:recorder.kill();recorder.wait(timeout=3)
            recorder.stdout.close()
            if 'selector' in locals():selector.close()
            report['recorder_reaped']=True
        except BaseException as error:cleanup_errors.append('private monitor cleanup: '+repr(error))
    if creation_attempted:
        try:
            owned=owned_module()
            if owned is not None:pactl('unload-module',str(owned['index']))
            report['private_module_removed']=owned_module() is None
        except BaseException as error:cleanup_errors.append('private module cleanup: '+repr(error))
    if default_before is not None:
        try:report['default_sink_unchanged']=pactl('get-default-sink')==default_before
        except BaseException as error:cleanup_errors.append('default sink verification: '+repr(error))
    report['cleanup_errors']=cleanup_errors
    if cleanup_errors or not (report.get('private_module_removed') and report.get('default_sink_unchanged') and report.get('recorder_reaped')):report['functional_gate']='FAIL'
    for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
    path=EVIDENCE/('audio-pcm-ui-'+str(time.time_ns())+'.json')
    with path.open('x') as output:json.dump(report,output,indent=2)
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
    if 'tmp' in locals():shutil.rmtree(tmp)
if report['functional_gate']!='PASS':raise SystemExit(1)
