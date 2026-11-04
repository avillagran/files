#!/usr/bin/env python3
"""Functional video frame/seek capture only. Resource/startup phase is explicitly suspended.
Internal handlers + FLTK framebuffer; no native input, clipboard or PSS measurement.
"""
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

ROOT=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
report={'method':'internal handlers, actual ffprobe/ffmpeg local decode and actual FLTK framebuffer; no native OS input or clipboard',
        'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','reason':'User phase: integration/captures only; no resource/startup measurements until released',
        'captures':[],'known_pending':['playback/audio','hardware codecs','resource budget','cross-platform verification','anamorphic/rotation and unusual metadata coverage']}
with tempfile.TemporaryDirectory(prefix='video-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    # Two generated colour segments exercise seek against actual decode bytes.
    subprocess.run(['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','color=c=red:s=640x360:r=5:d=4','-f','lavfi','-i','color=c=blue:s=640x360:r=5:d=4','-filter_complex','[0:v][1:v]concat=n=2:v=1:a=0[out]','-map','[out]','-c:v','ffv1','-threads','1',str(folder/'clip.mkv')],check=True,timeout=15)
    (folder/'bad.mkv').write_text('not a video')
    (folder/'symlink.mkv').symlink_to('clip.mkv')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1'}
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
            return row if '/clip.mkv"' in row and f'revision={revision} ' in row and match and abs(float(match[1])-position)<.02 else False
        return wait(check,'fresh video Ready absent: '+action)
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'framebuffer incomplete')
        path=evidence/(name+'-'+str(time.time_ns())+'.png')
        subprocess.run(['magick',str(raw),str(path)],check=True)
        report['captures'].append({'path':str(path),'framebuffer_dimensions':[int(v) for v in raw.read_bytes().split(b'\n',3)[1].split()]})
    try:
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'UI smoke not ready')
        wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry unvalidated')
        report['geometry']=observed_geometry(process.pid,1000,650)
        ready('video',0.0);command('video-assert-red');capture('video-first-frame')
        ready('video-forward',5.0);command('video-assert-blue');capture('video-seek-blue')
        ready('video-end',7.8);command('video-assert-blue')
        ready('video-quarter',2.0);command('video-assert-red');capture('video-timeline-red')
        ready('video-rapid',6.0);command('video-assert-blue')
        _,after=command('video-bad')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/bad.mkv"' in row for row in log.read_text()[after:].splitlines()),'fresh bad video error absent')
        capture('video-error')
        _,after=command('video-symlink')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/symlink.mkv"' in row for row in log.read_text()[after:].splitlines()),'fresh symlink video error absent')
        ready('video',0.0)
        command('close-video-pending')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'normal drained exit absent'
        normal=True
        report.update(functional_gate='PASS',normal_exit_after_preview_drain=True,cleanup_scope='root exit0 plus common worker drain ACK; no descendant sampler in capture phase',log=log.read_text())
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error),log=log.read_text())
        path=evidence/('video-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n');print(str(path));raise
    finally:
        if not normal:stop_tree(process,None)
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['external_tools']={name:{'path':shutil.which(name),'sha256':hashlib.sha256(Path(shutil.which(name)).read_bytes()).hexdigest(),'version':subprocess.run([name,'-version'],capture_output=True,text=True,timeout=5,check=True).stdout.splitlines()[0]} for name in ('ffprobe','ffmpeg')}
    path=evidence/('video-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
