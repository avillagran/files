#!/usr/bin/env python3
"""Functional audio metadata capture only. Resource/startup phase is explicitly suspended.
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
parser.add_argument('--playback',action='store_true',help='Silent PCM fixture: play/pause/seek/EOF/source deletion/active callback close')
parser.add_argument('--system-silent',action='store_true',help='Use default Pulse audio output with zero PCM; default playback fixture uses null muxer')
parser.add_argument('--source-lifecycle',action='store_true',help='External replace/modify/delete/rename/parent recovery with effective FD proof')
parser.add_argument('--drop-events',action='store_true',help='Fixture suppresses media notify callbacks; fallback reconcile still required')
args=parser.parse_args()
if args.system_silent and not args.playback:parser.error('--system-silent requires --playback')
if args.playback and args.source_lifecycle:parser.error('playback and source-lifecycle are separate functional scopes')
ROOT=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
report={'method':'internal handlers, actual ffprobe local metadata and actual FLTK framebuffer; no native OS input or clipboard',
        'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','reason':'User phase: integration/captures only; no resource/startup measurements until released',
        'captures':[],'playback':args.playback,'audio_output':'default Pulse audio output with zero PCM, not audible verification' if args.system_silent else 'ffmpeg null output (playback fixture only)','source_lifecycle':args.source_lifecycle,'notify_callbacks_suppressed':args.drop_events,'checks':[],'known_pending':['waveform/cover art and audible verification','resource budget','cross-platform verification','additional formats/cover art']}
# The compositor may restart while a long-running development shell survives.
# Bind only this fixture to the unique instance for its actual Wayland socket;
# never choose an unrelated desktop or change global shell/session settings.
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2))
matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
if len(matches)!=1:raise RuntimeError('No unique Hyprland instance for fixture Wayland socket')
previous=os.environ.get('HYPRLAND_INSTANCE_SIGNATURE');os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
report['compositor_binding']={'previous_signature':previous,'selected':matches[0],'scope':'fixture-only environment, unique Wayland socket match; no focus/input/global changes'}
with tempfile.TemporaryDirectory(prefix='audio-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    subprocess.run(['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','sine=frequency=440:sample_rate=48000:duration=6,volume=0' if args.playback else 'sine=frequency=440:sample_rate=48000:duration=2','-c:a','pcm_s16le','-threads','1',str(folder/'sound.wav')],check=True,timeout=10)
    for name,title in [('song.flac','Canción 漢 á'),('long.flac','漢'*1500)]:
        subprocess.run(['ffmpeg','-v','error','-nostdin','-i',str(folder/'sound.wav'),'-c:a','flac','-threads','1','-metadata','title='+title,'-metadata','artist=Autora','-metadata','album=Álbum',str(folder/name)],check=True,timeout=10)
    (folder/'bad.flac').write_text('not an audio file')
    (folder/'symlink.flac').symlink_to('song.flac')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1'}
    if args.drop_events:env['FILES_TEST_DROP_MEDIA_EVENTS']='1'
    env['FILES_CLOSE_TRACE']='1'
    if args.playback:
        if args.system_silent:env.pop('FILES_TEST_AUDIO_NULL',None)
        else:env['FILES_TEST_AUDIO_NULL']='1'
    with log.open('w') as output:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','audio','--exit-after','45'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
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
    def ready(action,name,truncated=False):
        revision,after=command(action)
        return fresh(after,name,truncated)
    def fresh(after,name='song.flac',truncated=False):
        return wait(lambda:next((row for row in log.read_text()[after:].splitlines() if row.startswith('FILES_PREVIEW_READY ') and '/'+name+'"' in row and 'encoding=Audio · metadatos ' in row and 'truncated='+str(truncated).lower() in row and 'stamp=Some(' in row),False),'fresh checked audio Ready absent: '+name)
    def assert_source(action):
        _,after=command(action)
        def proof():
            for line in log.read_text()[after:].splitlines():
                if line.startswith('FILES_AUDIO_SOURCE_OK '):return json.loads(line.split(' ',1)[1])
        actual=wait(proof,'effective audio proof absent')
        st=(folder/'song.flac').stat();expected={'device':st.st_dev,'inode':st.st_ino,'bytes':st.st_size,'modified_seconds':st.st_mtime_ns//10**9,'modified_nanoseconds':st.st_mtime_ns%10**9,'changed_seconds':st.st_ctime_ns//10**9,'changed_nanoseconds':st.st_ctime_ns%10**9}
        assert actual['source']==expected,(actual,expected)
        return actual
    def make_title(path,title):
        subprocess.run(['ffmpeg','-v','error','-nostdin','-i',str(folder/'sound.wav'),'-c:a','flac','-threads','1','-metadata','title='+title,'-metadata','artist=Autora','-metadata','album=Álbum',str(path)],check=True,timeout=10)
    def gone(pid,start):
        proc=Path(f'/proc/{pid}')
        for attempt in range(2):
            try:fields=(proc/'stat').read_text().rsplit(')',1)[1].split()
            except (FileNotFoundError,ProcessLookupError):
                if not proc.exists():return True
                if attempt==0:continue
                raise AssertionError('known process directory survives unreadable stat')
            return int(fields[19])!=start
    def transport(after,revision,requested,running,drained,minimum=0.0):
        def check():
            rows=[r for r in log.read_text()[after:].splitlines() if r.startswith('FILES_PREVIEW_READY ') and '/song.flac"' in r and f'revision={revision} ' in r]
            if not rows:return False
            row=rows[-1]
            if 'audio_error=Some(' in row:raise AssertionError('audio backend error: '+row)
            match=re.search(r'position=([0-9.]+)',row)
            return row if all(f'{key}={str(value).lower()}' in row for key,value in [('audio_requested',requested),('audio_running',running),('drained',drained)]) and match and float(match[1])>=minimum else False
        return wait(check,'audio transport state absent')
    def observed_player(after):
        rows=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=preview_audio_playback',log.read_text()[after:]);assert rows,'audio player not logged'
        pid,start=map(int,rows[-1]);proc=Path(f'/proc/{pid}');fields=(proc/'stat').read_text().rsplit(')',1)[1].split();assert int(fields[19])==start and fields[0]!='Z','audio player not alive with expected identity'
        return pid,start
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
        ready('audio','song.flac');command('audio-assert');capture('audio-metadata')
        command('audio-select');capture('audio-metadata-selection')
        ready('audio-wav','sound.wav');capture('audio-wav-metadata')
        ready('audio-long','long.flac',True);command('audio-assert-truncated');capture('audio-tags-truncated')
        ready('audio-rapid','song.flac');command('audio-assert')
        _,after=command('audio-bad')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/bad.flac"' in row for row in log.read_text()[after:].splitlines()),'fresh bad audio error absent')
        _,after=command('audio-symlink')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/symlink.flac"' in row for row in log.read_text()[after:].splitlines()),'fresh symlink audio error absent')
        if args.source_lifecycle:
            ready('audio','song.flac');a=assert_source('audio-source-a')
            source=folder/'song.flac';old=source.stat()
            replacement=test/'replacement.flac';make_title(replacement,'Canción 漢 ó')
            assert replacement.stat().st_size==old.st_size,'same-size fixture not established'
            os.utime(replacement,ns=(old.st_atime_ns,old.st_mtime_ns));before=len(log.read_text());os.replace(replacement,source)
            fresh(before);b=assert_source('audio-source-b');assert b['source']['inode']!=a['source']['inode']
            assert b['source']['bytes']==a['source']['bytes'] and b['source']['modified_seconds']==a['source']['modified_seconds'] and b['source']['modified_nanoseconds']==a['source']['modified_nanoseconds']
            capture('audio-source-replaced');report['checks'].append('same path/size/mtime replacement publishes different effective FD identity and new title')
            changed=test/'changed.flac';make_title(changed,'Canción 漢 é');before=len(log.read_text());source.write_bytes(changed.read_bytes());os.utime(source,ns=(old.st_atime_ns,old.st_mtime_ns));fresh(before)
            c=assert_source('audio-source-c');assert c['source']['inode']==b['source']['inode'] and c['source']!=b['source']
            capture('audio-source-modified');report['checks'].append('in-place write with restored mtime invalidates old title by revision/ctime')
            before=len(log.read_text());source.unlink();wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/song.flac"' in row for row in log.read_text()[before:].splitlines()),'deleted audio not invalidated')
            command('audio-source-empty');capture('audio-source-deleted')
            before=len(log.read_text());os.replace(changed,source);fresh(before);assert_source('audio-source-c')
            away=folder/'away.flac';before=len(log.read_text());os.rename(source,away);wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/song.flac"' in row for row in log.read_text()[before:].splitlines()),'renamed audio not invalidated')
            before=len(log.read_text());os.rename(away,source);fresh(before);assert_source('audio-source-c')
            # Root replacement recovery: keep fixture generator input before moving parent.
            replacement=test/'recovered.flac';make_title(replacement,'Canción 漢 á')
            before=len(log.read_text());os.rename(folder,test/'old-parent');wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/song.flac"' in row for row in log.read_text()[before:].splitlines()),'renamed parent not invalidated')
            folder.mkdir();before=len(log.read_text());os.replace(replacement,source);fresh(before);assert_source('audio-source-a');capture('audio-parent-recovered')
            report['checks'].append('delete/recreate, file rename away/back and parent replacement clear old metadata and recover through fresh FD proof')
            command('media-hide');count=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2);assert log.read_text().count('FILES_PREVIEW_HELPER ')==count,'hidden audio restarted helper'
            report['checks'].append('closed audio preview does not launch new metadata helper during reconcile interval')
        if args.playback:
            ready('audio','song.flac');command('audio-assert-paused')
            revision,after=command('audio-play');transport(after,revision,True,True,False,1.0);first=observed_player(after);command('audio-assert-playing');capture('audio-playing')
            revision,after=command('audio-pause');transport(after,revision,False,False,True);command('audio-assert-paused');assert gone(*first),'paused player not reaped'
            command('audio-select');revision,after=command('audio-seek');transport(after,revision,False,False,True,2.4);capture('audio-paused-seek')
            revision,after=command('audio-play');transport(after,revision,True,True,False,2.4);second=observed_player(after)
            revision,after=command('audio-timeline');transport(after,revision,True,True,False,4.4);seek=observed_player(after);assert seek!=second and gone(*second),'seek did not drain old player'
            capture('audio-timeline-seek');transport(after,revision,False,False,True,6.0);command('audio-assert-paused');assert gone(*seek),'EOF player not reaped';capture('audio-eof')
            revision,after=command('audio-play');transport(after,revision,True,True,False);hidden=observed_player(after);command('media-hide');wait(lambda:gone(*hidden),'hidden audio player survived')
            count=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2);assert log.read_text().count('FILES_PREVIEW_HELPER ')==count,'hidden audio restarted helper'
            ready('audio','song.flac');revision,after=command('audio-play');transport(after,revision,True,True,False);deleted=observed_player(after)
            backup=test/'song-backup.flac';shutil.copy2(folder/'song.flac',backup);before=len(log.read_text());(folder/'song.flac').unlink()
            wait(lambda:any(r.startswith('FILES_PREVIEW_ERROR ') and '/song.flac"' in r for r in log.read_text()[before:].splitlines()),'playing deleted source not invalidated');wait(lambda:gone(*deleted),'deleted source player survived');command('audio-source-empty');capture('audio-playing-source-deleted')
            before=len(log.read_text());os.replace(backup,folder/'song.flac');fresh(before);assert_source('audio-source-a')
            revision,after=command('audio-play');transport(after,revision,True,True,False);closing=observed_player(after)
            report['active_player_at_callback']={'pid':closing[0],'start':closing[1]}
            command('callback')
            report['checks'].append('actual ffmpeg audio play/pause/seek pointer without stealing selection, resume/EOF, hidden stop/no restart, source deletion kill/reap, callback close while player observed alive; silent PCM, no physical/audible verification')
        else:command('close-audio-pending')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'normal drained exit absent'
        records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=',log.read_text());assert records and all(start.isdigit() for _,start in records)
        identities=sorted(set((int(pid),int(start)) for pid,start in records));assert all(gone(pid,start) for pid,start in identities),'logged helper identity survived close'
        report['logged_helper_cleanup']={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start after exit0, not lifetime or resource sampling'}
        normal=True
        report.update(functional_gate='PASS',normal_exit_after_preview_drain=True,cleanup_scope='root exit0 plus common worker drain ACK; no descendant sampler in capture phase',log=log.read_text())
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error),log=log.read_text())
        path=evidence/('audio-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n');print(str(path));raise
    finally:
        if not normal:stop_tree(process,None)
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['external_tools']={name:{'path':shutil.which(name),'sha256':hashlib.sha256(Path(shutil.which(name)).read_bytes()).hexdigest(),'version':subprocess.run([name,'-version'],capture_output=True,text=True,timeout=5,check=True).stdout.splitlines()[0]} for name in (('ffprobe','ffmpeg') if args.playback else ('ffprobe',))}
    path=evidence/('audio-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
