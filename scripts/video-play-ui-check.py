#!/usr/bin/env python3
"""Functional video playback/pause/EOF capture only. Resource/startup phase is explicitly suspended.
Internal handlers + FLTK framebuffer; no native input, clipboard or PSS measurement.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
from memcheck import geometry, observed_geometry, stop_tree

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--no-clip',action='store_true',help='Functional scene with four normal captures only; no 24-frame readback clip, no memory inference')
parser.add_argument('--zoom',action='store_true',help='Verify zoom/pan across frames, pause/seek/resume and source reset')
parser.add_argument('--gif',action='store_true',help='Animated GIF, infinite source loop ignored, same bounded video engine')
parser.add_argument('--with-audio',action='store_true',help='Silent FLAC track index0 and video index1, opt-in/hot toggle/Pulse output')
parser.add_argument('--pulse-failure',action='store_true',help='Private unavailable Pulse socket: visible error then explicit muted retry')
args=parser.parse_args()
if args.zoom and not args.no_clip:parser.error('--zoom requires --no-clip')
if args.gif and (args.with_audio or not args.no_clip):parser.error('--gif requires --no-clip and excludes --with-audio')
if args.with_audio and not args.no_clip:parser.error('--with-audio requires --no-clip')
if args.pulse_failure and not args.with_audio:parser.error('--pulse-failure requires --with-audio')
ROOT=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
report={'method':'internal handlers, actual ffprobe/ffmpeg local decode and actual FLTK framebuffer; no native OS input or clipboard',
        'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','reason':'User phase: integration/captures only; no resource/startup measurements until released',
        'captures':[],'zoom_fixture':args.zoom,'gif_fixture':args.gif,'audio_fixture':args.with_audio,'pulse_failure_fixture':args.pulse_failure,'audible_verification':'NOT_VALIDATED: zero PCM','clip_enabled':not args.no_clip,'known_pending':['audibility/A-V sync/sample-exact seek/audio track selection','hardware codecs','resource budget','cross-platform verification','VFR/PTS/large/rotated media coverage']}
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
if len(matches)!=1:raise RuntimeError('No unique compositor for fixture Wayland socket')
os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
report['compositor_binding']={'selected':matches[0],'scope':'fixture-only unique Wayland socket, no focus/global changes'}
with tempfile.TemporaryDirectory(prefix='video-play-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    # Two generated colour segments exercise seek against actual decode bytes.
    generator=['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','color=c=red:s=640x360:r=5:d=4','-f','lavfi','-i','color=c=blue:s=640x360:r=5:d=4']
    if args.with_audio:generator+=['-f','lavfi','-i','anullsrc=r=48000:cl=mono:d=8']
    generator+=['-filter_complex','[0:v][1:v]concat=n=2:v=1:a=0[out]']
    if args.with_audio:generator+=['-map','2:a:0'] # Audio FIRST to reject v0/index0 assumptions.
    generator+=['-map','[out]','-c:v','gif' if args.gif else 'ffv1','-threads','1']
    if args.gif:generator+=['-loop','0']
    if args.with_audio:generator+=['-c:a','flac']
    clip_name='clip.gif' if args.gif else 'clip.mkv'
    subprocess.run([*generator,str(folder/clip_name)],check=True,timeout=15)
    if args.gif:
        payload=(folder/clip_name).read_bytes();offset=payload.index(b'NETSCAPE2.0');assert payload[offset+11:offset+16]==b'\x03\x01\x00\x00\x00','infinite-loop GIF fixture marker absent'
        report['gif_source']={'bytes':len(payload),'netscape_loop_extension':True,'ignore_loop':True,'policy':'one pass only; no frame-precise timing/alpha/disposal or resource claim'}
    (folder/'bad.mkv').write_text('not a video')
    (folder/'symlink.mkv').symlink_to(clip_name)
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1'}
    if args.pulse_failure:env['PULSE_SERVER']='unix:'+str(test/'unavailable-pulse.sock')
    with log.open('w') as output:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','video','--exit-after','45'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
    normal=False
    def wait(check,reason,allow_exit=False,timeout=12):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exited\n'+log.read_text())
            time.sleep(.025)
        raise AssertionError(reason+'\n'+log.read_text())
    def command(action):
        if args.gif and action=='video':action='gif'
        after=len(log.read_text())
        temporary=raw.with_suffix('.pending');temporary.write_text(action);os.replace(temporary,raw.with_suffix('.command'))
        match=wait(lambda:re.search(r'FILES_PREVIEW_COMMAND_REV action='+re.escape(action)+r' revision=(\d+)',log.read_text()[after:]),'command failed: '+action,action.startswith('close'))
        return int(match[1]),after
    def ready(action,position,after=0):
        revision,after=command(action)
        def check():
            rows=[line for line in log.read_text()[after:].splitlines() if line.startswith('FILES_PREVIEW_VIDEO_READY ')]
            if not rows:return False
            row=rows[-1]
            match=re.search(r'position=([0-9.]+)',row)
            return row if ('/'+clip_name+'"') in row and f'revision={revision} ' in row and match and abs(float(match[1])-position)<.02 else False
        return wait(check,'fresh video Ready absent: '+action)
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'framebuffer incomplete')
        path=evidence/(('gif-' if args.gif else '')+name+'-'+str(time.time_ns())+'.png')
        subprocess.run(['magick',str(raw),str(path)],check=True)
        report['captures'].append({'path':str(path),'framebuffer_dimensions':[int(v) for v in raw.read_bytes().split(b'\n',3)[1].split()]})
    def observed_helper(after):
        records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=preview_video_stream',log.read_text()[after:])
        assert records,'stream process not logged'
        pid,start=map(int,records[-1]);stat=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
        assert int(stat[19])==start and stat[0]!='Z','stream PID not alive with expected identity'
        return {'pid':pid,'starttime':start}
    def gone(identity):
        proc=Path(f"/proc/{identity['pid']}")
        for attempt in range(2):
            try:stat=(proc/'stat').read_text().rsplit(')',1)[1].split()
            except (FileNotFoundError,ProcessLookupError):
                if not proc.exists():return True
                if attempt==0:continue
                raise AssertionError('known process directory survives unreadable stat')
            return int(stat[19])!=identity['starttime']
    def frame_marker(after,playing,minimum=0,revision=None):
        for row in reversed(log.read_text()[after:].splitlines()):
            if not row.startswith('FILES_PREVIEW_VIDEO_READY '):continue
            seq=re.search(r'sequence=Some\((\d+)\)',row)
            if revision is not None and f'revision={revision} ' not in row:continue
            if f'playing={str(playing).lower()} ' in row and seq and int(seq[1])>=minimum:return row
        return False
    def clip_frame(index):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'clip framebuffer incomplete')
        shutil.copyfile(raw,test/f'frame-{index:04d}.ppm')
        return time.monotonic()
    try:
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'UI smoke not ready')
        wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry unvalidated')
        report['geometry']=observed_geometry(process.pid,1000,650)
        ready('video',0.0)
        if args.gif:command('video-assert-paused');command('video-assert-red');capture('snapshot-first')
        if args.zoom:
            command('video-assert-transform-reset');capture('video-fit-before-zoom')
            command('video-zoom');capture('video-zoom-before-pan')
            command('video-pan');command('video-assert-transform');capture('video-zoom-panned')
        if args.with_audio:command('video-assert-audio-tracks');capture('video-audio-default-muted')
        if args.pulse_failure:
            # Since 2026-11-04 an unavailable audio output no longer stops the
            # video: it keeps playing silently for this file ("Sin audio"),
            # without changing the saved mute preference.
            command('video-audio');revision,after=command('video-play')
            def silent():
                return next((r for r in log.read_text()[after:].splitlines() if r.startswith('FILES_PREVIEW_VIDEO_READY ') and 'playing=true ' in r and 'audio_failed=true ' in r and 'muted=false ' in r and re.search(r'sequence=Some\((\d+)\)',r) and int(re.search(r'sequence=Some\((\d+)\)',r)[1])>=4),False)
            wait(silent,'Pulse failure did not continue silently');capture('video-audio-output-silent-fallback')
            assert 'audio=false ' in log.read_text()[after:],'silent fallback still requested audio'
            identity=observed_helper(after);report['silent_fallback_after_pulse_failure']=True
        else:
            revision,after=command('video-play');wait(lambda:frame_marker(after,True,4,revision),'stream produced no frames')
            identity=observed_helper(after)
            if args.with_audio:
                assert 'audio=false ' in log.read_text()[after:],'default unexpectedly enabled sound'
                old=identity;revision,after=command('video-audio');wait(lambda:frame_marker(after,True,4,revision),'audio opt-in did not resume fresh stream')
                identity=observed_helper(after);assert identity!=old and gone(old),'hot audio toggle did not drain old stream'
                assert 'audio=true video_index=1 audio_index=Some(0)' in log.read_text()[after:],'sound not mapped to actual track indices'
                report['audio_hot_toggle_drained_previous']=True
        report['stream_observed_live']=identity
        command('video-assert-playing');capture('video-playing-red')
        started=time.monotonic();samples=[]
        for index in range(0 if args.no_clip else 24):
            delay=started+index*.25-time.monotonic()
            if delay>0:time.sleep(delay)
            samples.append(clip_frame(index))
        # Blue starts at 4s; frames now come at the source rate (5fps here),
        # not a fixed 12fps: 4.8s of output is sequence 24.
        if args.no_clip:wait(lambda:frame_marker(after,True,24),'blue segment not reached')
        command('video-assert-blue')
        if args.zoom:command('video-assert-transform')
        capture('video-playing-blue')
        _,after=command('video-pause')
        wait(lambda:frame_marker(after,False) and 'drained=true' in frame_marker(after,False),'pause did not reap')
        command('video-assert-paused');assert gone(identity),'paused ffmpeg still exists'
        capture('video-paused-blue')
        if args.zoom:
            command('video-assert-transform');ready('video-mid',2.)
            command('video-assert-transform');command('video-assert-red');capture('video-seek-retains-framing')
        _,after=command('video-play')
        wait(lambda:frame_marker(after,True,2),'resume produced no frame')
        resumed=observed_helper(after)
        wait(lambda:frame_marker(after,False) and 'drained=true' in frame_marker(after,False) and 'notice=None' in frame_marker(after,False),'normal EOF not drained or backend error present')
        command('video-assert-paused');assert gone(resumed),'EOF ffmpeg still exists'
        if args.zoom:command('video-assert-transform')
        capture('video-playback-ended')
        if args.gif:
            command('media-hide');count=log.read_text().count('command=preview_video_stream');time.sleep(1.2);assert log.read_text().count('command=preview_video_stream')==count,'closed GIF relaunched stream'
            report['gif_no_hidden_stream']=True
            for _ in range(80):
                _,check_after=command('gif-thumbnails')
                rows=[r for r in log.read_text()[check_after:].splitlines() if r.startswith('FILES_GIF_THUMBNAILS ')]
                state=json.loads(rows[-1].split(' ',1)[1]) if rows else None
                own=next((r for r in state['ready'] if '/clip.gif"' in r['path']),None) if state else None
                if own and state['painted']>0:break
                time.sleep(.05)
            else:raise AssertionError('visible GIF inspector thumbnail not ready/painted')
            assert own['width']<=160 and own['height']<=160 and own['revision'] is not None
            assert own['pixel'][0]>180 and own['pixel'][2]<70,'GIF thumbnail is not first red frame'
            expected=(folder/clip_name).stat();assert own['revision']['device']==expected.st_dev and own['revision']['inode']==expected.st_ino and own['revision']['bytes']==expected.st_size,'thumbnail FD proof mismatch'
            report['gif_inspector_thumbnail']={'snapshot':state,'scope':'visible inspector static first-frame160px, no playback loop; grid GIF not captured in this scene'}
            capture('inspector-first-frame')
        ready('video',0.0)
        if args.zoom:
            command('video-assert-transform-reset');capture('video-reopen-resets-framing')
            report['zoom_pan_checks']={'frames':True,'pause':True,'seek':True,'resume_eof':True,'new_source_reset':True,'method':'shared Action zoom + Model pointer down/drag, not native input','scope':'single source numeric transform only, viewport raster bounded by existing sampler; no resource claim'}
        if args.with_audio and not args.pulse_failure:command('video-audio')
        _,after=command('video-play')
        wait(lambda:frame_marker(after,True,4),'close fixture did not play')
        closing=observed_helper(after);command('close-video-playing')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'normal drained exit absent'
        assert gone(closing),'ffmpeg survived normal app exit'
        if not args.no_clip:
            clip=evidence/('video-playback-framebuffer-'+str(time.time_ns())+'.mp4')
            subprocess.run(['ffmpeg','-v','error','-nostdin','-framerate','4','-i',str(test/'frame-%04d.ppm'),'-c:v','libx264','-threads','1','-pix_fmt','yuv420p','-movflags','+faststart',str(clip)],check=True,timeout=20)
            report['clip']={'path':str(clip),'method':'24 actual FLTK framebuffer snapshots, sampled ~4fps and assembled at4fps after app exit; no native input or continuous OS recording','sample_times':samples}
        else:
            report['clip']={'status':'OMITTED','reason':'functional four-capture scene; no resource experiment or memory inference'}
        identities=sorted(set((int(pid),int(start)) for pid,start in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=',log.read_text())))
        assert identities and all(gone({'pid':pid,'starttime':start}) for pid,start in identities),'logged helper identity survived close'
        report['logged_helper_cleanup']={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start identities after exit, not lifetime or resource sampling'}
        normal=True
        report.update(functional_gate='PASS',normal_exit_after_preview_drain=True,cleanup_scope='root exit0 plus State ACK and observed stream PID/starttime absent after pause, EOF and active close; no memory sampler',log=log.read_text())
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error),log=log.read_text())
        path=evidence/('video-play-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n');print(str(path));raise
    finally:
        if not normal:stop_tree(process,None)
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['external_tools']={name:{'path':shutil.which(name),'sha256':hashlib.sha256(Path(shutil.which(name)).read_bytes()).hexdigest(),'version':subprocess.run([name,'-version'],capture_output=True,text=True,timeout=5,check=True).stdout.splitlines()[0]} for name in ('ffprobe','ffmpeg')}
    path=evidence/('video-play-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
