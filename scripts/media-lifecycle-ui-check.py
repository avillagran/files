#!/usr/bin/env python3
"""Own media fixtures, watcher/checked-FD publication and framebuffer. NO metrics."""
import hashlib,json,os,re,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/media-lifecycle';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','method':'shared preview handlers + real FLTK framebuffer; filesystem changes in private fixtures; no native input','captures':[],'checks':[],'cache':'no product disk cache; RAM/pipe preview only; Grid thumbnails/LRU pending','source_scope':'standalone image/video, not embedded Markdown/PDF/audio','capture_dimensions':{},'footer_scope':'Existing footer is main-process PSS only, not a tree+Swap budget. Not used for acceptance.','disk_cache_scope':'No product media cache under isolated source/config/data; RAM/pipe implementation, not an audit of all filesystem paths.'}
def png(path,color):
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    w,h=320,200;data=zlib.compress((b'\0'+bytes(color)*w)*h)
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',data)+chunk(b'IEND',b''))
with tempfile.TemporaryDirectory(prefix='media-life-',dir=BASE) as temp:
    base=Path(temp);folder=base/'media';folder.mkdir();config=base/'config';config.mkdir();data=base/'data';data.mkdir()
    source=folder/'alpha.png';png(source,(220,30,40))
    subprocess.run(['magick',str(source),str(folder/'oriented.jpg')],check=True)
    subprocess.run(['magick',str(source),str(folder/'tiny.webp')],check=True)
    subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=c=blue:s=320x180:r=12:d=4','-c:v','ffv1','-threads','1',str(folder/'clip.mkv')],check=True)
    raw=base/'capture.ppm';log=base/'app.log';process=None;normal=False
    try:
        env={**os.environ,'XDG_DATA_HOME':str(data),'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_MUTATIONS':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
        with log.open('w') as output:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','image','--exit-after','90'],env=env,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
        def wait(check,reason,allow_exit=False):
            end=time.monotonic()+15
            while time.monotonic()<end:
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode))
                time.sleep(.02)
            raise AssertionError(reason)
        def command(action):
            before=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            wait(lambda:'FILES_PREVIEW_COMMAND_OK '+action+'\n' in log.read_text()[before:],'command failed: '+action,action=='close')
            assert 'FILES_PREVIEW_COMMAND_ERROR' not in log.read_text()[before:]
            return before
        def ready(after,name='alpha.png',kind='IMAGE'):
            def marker():
                lines=[s for s in log.read_text()[after:].splitlines() if s.startswith('FILES_PREVIEW_'+kind+'_READY ') and '/'+name+'"' in s]
                return lines[-1] if lines else None
            line=wait(marker,'fresh '+kind+' ready absent: '+name);report['checks'].append(line);return line
        def error(after):
            return wait(lambda:next((s for s in log.read_text()[after:].splitlines() if s.startswith('FILES_PREVIEW_ERROR ') and '/alpha.png"' in s),None),'fresh original-unavailable absent')
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def complete():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(complete,'framebuffer missing');path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],check=True);report['captures'].append(str(path));report['capture_dimensions'][str(path)]=list(struct.unpack('>II',path.read_bytes()[16:24]))
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'scene not loaded')
        ready(command('png'));command('media-red');capture('source-red')
        # Same path/mtime, distinct inode. Replacement is intentionally after Ready(A).
        old=source.stat();replacement=folder/'replacement';png(replacement,(20,50,230));os.utime(replacement,ns=(old.st_atime_ns,old.st_mtime_ns));before=len(log.read_text());os.replace(replacement,source)
        ready(before);command('media-blue');capture('source-replaced-blue');report['checks'].append('same-path replacement: inode changes; checked FD/Ready stamp and all RGB blue')
        before=len(log.read_text());png(source,(30,200,70));ready(before);command('media-green');report['checks'].append('in-place modification: old pixels replaced')
        before=len(log.read_text());source.unlink();error(before);capture('source-deleted');report['checks'].append('external deletion: Error, bitmap released')
        before=len(log.read_text());png(source,(220,30,40));ready(before);command('media-red')
        before=len(log.read_text());source.rename(folder/'moved.png');error(before);before=len(log.read_text());(folder/'moved.png').rename(source);ready(before);command('media-red');report['checks'].append('rename away/back: unavailable then fresh source')
        # Parent identity changes without changing absolute pathname. Watch must reattach.
        before=len(log.read_text());folder.rename(base/'old-media');folder.mkdir();png(source,(20,50,230));ready(before);command('media-blue');report['checks'].append('parent rename/replacement: fresh path revision, watcher recovered')
        # Rapid destination changes drain one helper before the last decode.
        for name in ['oriented.jpg','tiny.webp','clip.mkv']:(folder/name).write_bytes((base/'old-media'/name).read_bytes())
        ready(command('image-rapid'),'oriented.jpg');ready(command('png'));command('media-blue')
        ready(command('video'),'clip.mkv','VIDEO');command('video-play')
        wait(lambda:'playing=true sequence=Some(' in log.read_text(),'video did not publish a checked stream frame')
        before=len(log.read_text());(folder/'clip.mkv').unlink()
        wait(lambda:any(s.startswith('FILES_PREVIEW_ERROR ') and '/clip.mkv"' in s for s in log.read_text()[before:].splitlines()),'deleted playing video not invalidated')
        capture('playing-source-deleted');report['checks'].append('video snapshot/stream carry FD stamp; deleting playing source cancels/releases media')
        ready(command('png'));command('media-blue');command('media-hide');
        count=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2);assert log.read_text().count('FILES_PREVIEW_HELPER ')==count
        report['checks'].append('hidden preview: no replacement helper launched during reconciliation interval')
        ready(command('png'));command('media-blue');before=command('media-trash');wait(lambda:not source.exists(),'own Trash did not delete fixture');report['checks'].append('own Trash hides preview before file mutation ACK')
        # Let ACK/refresh finish before normal close; no subprocess metrics are sampled.
        wait(lambda:re.search(r'^FILES_MUTATION_FINISHED id=\d+ success=true undo=true redo=false$',log.read_text()[before:],re.M),'own Trash success/Undo ACK absent')
        command('close');wait(lambda:process.poll() is not None,'normal close absent',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text() and 'FILES_CLOSE_DRAINED' in log.read_text()
        normal=True
        text=log.read_text();records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=',text);assert records and all(start.isdigit() for _,start in records)
        helpers={(int(pid),int(start)) for pid,start in records}
        survivors=[]
        for pid,start in helpers:
            try:
                status=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()
                if int(status[19])==start and status[0]!='Z':survivors.append([pid,start])
            except FileNotFoundError:
                assert not Path(f'/proc/{pid}').exists(), "PID still exists but stat unreadable"
        assert helpers and not survivors
        report['cleanup']={'normal_exit':0,'preview_ack':True,'logged_helper_identities':sorted(helpers),'survivors':survivors,'complete':True,'scope':'logged PID/starttime helpers after root exit0; not lifetime/metrics coverage','readonly_probe_stop':re.findall(r'FILES_MEDIA_PROBE_STOPPED=([^ ]+)',text)}
        report['functional_gate']='PASS'
        report['product_disk_cache_created']=False
        report['remaining_paths']=[str(p.relative_to(base)) for p in base.rglob('*') if p.is_file()]
        # Only known fixture/config/trash/capture/log outputs permitted, no product media cache.
        assert all(p.split('/')[0] in {'media','old-media','config','data','capture.ppm','app.log'} for p in report['remaining_paths'])
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error))
        raise
    finally:
        if process and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        report['log']=log.read_text() if log.exists() else ''
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
        report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        path=OUT/('media-lifecycle-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(report,out,indent=2)
        print(path)
