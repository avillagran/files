#!/usr/bin/env python3
"""Visible image/video thumbnails, raw identity, cache invalidation and close.
No PSS/Swap/CPU/startup sampling. No native input; shared handlers + framebuffer.
"""
import argparse,hashlib,json,os,re,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--large-codecs',action='store_true',help='24MP row PNG and progressive JPEG in existing Grid/inspector demand')
parser.add_argument('--normal-only',action='store_true',help='Scope to main functional lifecycle; do not repeat lost-event/backend-loss fixtures')
args=parser.parse_args()
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/thumbnails';OUT.mkdir(parents=True,exist_ok=True)
KNOWN_ISSUES={'original_deleted_widget_panic': 'UNRESOLVED cause; original fail1793067614908477249 had no backtrace. Idle20s did not reproduce. New guarded backend-loss path verified separately, not proof of compositor/SuperW or original trigger.', 'global_cache': 'Only Grid/inspector LRU; full Space releases it. Embedded and other format/frame buffers are not unified under this ledger.'}
report={'known_issues':KNOWN_ISSUES,'functional_gate':'IN_PROGRESS','method':'shared Grid/inspector/preview/FLTK callback handlers + private FS fixtures + real framebuffer; no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'instances':[],'local_budget_scope':'cache RGB capacities + active wire reservation4MiB,32entries; cache keys64KiB/demand paths64KiB; scratch<=76800B, not process-tree memory','footer_scope':'main-only PSS footer not used as a budget','large_codecs_fixture':args.large_codecs,'limitations':['PNG/JPEG40Mipix source; WebP4Mipix; APNG/ICC/color fidelity and resource gate pending','standalone Grid/inspector; embedded/PDF/audio cache integration pending','FLTK mapped-visibility gate; native minimization/input not proved']}
def png(path,color,w=320,h=200):
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    compressor=zlib.compressobj();chunks=[];row=b'\0'+bytes(color)*w
    for _ in range(h):chunks.append(compressor.compress(row))
    data=b''.join(chunks)+compressor.flush()
    path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',data)+chunk(b'IEND',b''))

def run(base,lost_events=False,backend_loss=False):
    tool_tmp=base/"tool-tmp";tool_tmp.mkdir();os.environ["TMPDIR"]=str(tool_tmp);os.environ["MAGICK_TEMPORARY_PATH"]=str(tool_tmp);os.environ["MAGICK_MEMORY_LIMIT"]="32MiB";os.environ["MAGICK_MAP_LIMIT"]="0"
    folder=base/'media';folder.mkdir();config=base/'config';config.mkdir();data=base/'data';data.mkdir();(folder/'one').mkdir()
    png(folder/'red.png',(220,30,40))
    if not lost_events:
        png(folder/'blue.png',(20,50,230),*( (6000,4000) if args.large_codecs else (320,200) ));png(folder/'green.png',(30,200,70));png(folder/'one'/'inside.png',(180,40,200))
        raw_source=Path(os.fsdecode(os.fsencode(folder)+b'/raw\xff.png'));png(raw_source,(180,40,200))
        subprocess.run(['magick','-size','6000x4000' if args.large_codecs else '4000x3000','gradient:#1f4580-#de6d60','-quality','85','-interlace','plane' if args.large_codecs else 'none',str(folder/'photo.jpg')],check=True)
        subprocess.run(['magick',str(folder/'red.png'),str(folder/'tiny.webp')],check=True)
        subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=c=blue:s=640x360:r=12:d=4','-c:v','ffv1','-threads','1',str(folder/'clip.mkv')],check=True)
        (folder/'notes.txt').write_text('not a thumbnail source\n')
    raw=base/'capture.ppm';log=base/'app.log';process=None;normal=False;seq=0;spawned=set();observed_live=0;instance={'backend_widget_loss_fixture':backend_loss,'lost_notify_fixture':lost_events,'checks':[],'states':[],'exit_read_events':[]};exit_seen=set()
    def text():return log.read_text() if log.exists() else ''
    def stat_for(pid,start,phase):
        proc=Path(f'/proc/{pid}');path=proc/'stat'
        for attempt in range(2):
            try:
                fields=path.read_text().rsplit(')',1)[1].split()
                return fields if int(fields[19])==start else None
            except (FileNotFoundError,ProcessLookupError) as error:
                if not proc.exists():
                    key=(pid,start,phase)
                    if key not in exit_seen:instance['exit_read_events'].append({'pid':pid,'start':start,'phase':phase,'errno':error.errno,'reason':'confirmed process directory disappearance'});exit_seen.add(key)
                    return None
                if attempt==1:raise AssertionError(f'known PID {pid}/{start} still exists but stat unreadable') from error
    def live_helpers():
        nonlocal observed_live
        rows=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=([^\s]+)',text())
        assert all(start.isdigit() for _,start,_ in rows),'helper without stable identity'
        spawned.update((int(pid),int(start)) for pid,start,_ in rows)
        live=[]
        for pid,start in spawned:
            fields=stat_for(pid,start,'poll')
            if fields and int(fields[19])==start and fields[0]!='Z':live.append([pid,start])
        observed_live=max(observed_live,len(live));assert len(live)<=1,'concurrent media/du helpers '+repr(live)
        return live
    def wait(check,reason,allow_exit=False):
        end=time.monotonic()+20
        while time.monotonic()<end:
            live_helpers();value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode))
            time.sleep(.015)
        raise AssertionError(reason)
    def command(action):
        nonlocal seq
        before=len(text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
        def response():
            tail=text()[before:]
            assert 'FILES_SPACE_COMMAND_ERROR' not in tail, tail
            return re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',tail,re.M)
        marker=wait(response,'command absent '+action,action in {'callback','fixture-destroy-window'});seq=int(marker[1]);return before
    def state(after=0,check=lambda s:True):
        def fresh():
            rows=[]
            for line in text()[after:].splitlines():
                if line.startswith('FILES_THUMBNAILS_STATE '):
                    value=json.loads(line[len('FILES_THUMBNAILS_STATE '):])
                    if value['seq']==seq and check(value):rows.append(value)
            return rows[-1] if rows else None
        s=wait(fresh,'fresh thumbnail state absent seq='+str(seq));instance['states'].append(s);return s
    def find(s,name):return next((row for row in s['thumbnails']['ready'] if row['path'].endswith('/'+name+'"')),None)
    def color(s,name,rgb):return (row:=find(s,name)) and row['pixel']==list(rgb)
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def full():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(full,'framebuffer absent');path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],check=True);report['captures'].append({'path':str(path),'dimensions':list(struct.unpack('>II',path.read_bytes()[16:24]))})
    try:
        env={**os.environ,'XDG_DATA_HOME':str(data),'FILES_CONFIG_DIR':str(config),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_THUMBNAILS':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_MUTATIONS':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1','RUST_BACKTRACE':'1'}
        if lost_events:env['FILES_TEST_DROP_MEDIA_EVENTS']='1'
        if backend_loss:env['FILES_TEST_ALLOW_WIDGET_DELETE']='1'
        with log.open('w') as out:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','90'],env=env,stderr=out,stdout=subprocess.DEVNULL,stdin=subprocess.DEVNULL,start_new_session=True)
        wait(lambda:'FILES_UI_SMOKE_OK' in text() and 'FILES_HIDDEN_STATE ' in text(),'paint after smoke absent')
        state(command('grid'),lambda s:color(s,'red.png',(220,30,40)) and s['thumbnails']['painted']>=1)
        if backend_loss:
            command('refresh');live=wait(live_helpers,'no live helper before widget loss');instance['helper_alive_before_widget_loss']=live
            command('fixture-destroy-window')
        elif lost_events:
            old=(folder/'red.png').stat();replacement=folder/'next';png(replacement,(20,50,230));os.utime(replacement,ns=(old.st_atime_ns,old.st_mtime_ns));before=len(text());os.replace(replacement,folder/'red.png')
            state(before,lambda s:color(s,'red.png',(20,50,230)) and s['thumbnails']['painted']>=1)
            instance['checks'].append('notify callbacks intentionally ignored; periodic per-file FD reconcile replaced samepath cached red with blue')
            capture('lost-event-reconciled')
        else:
            s=state(check=lambda s:len(s['thumbnails']['ready'])>=7 and s['thumbnails']['painted']>=7)
            assert all(row['width']<=160 and row['height']<=160 and row['revision'] for row in s['thumbnails']['ready'])
            assert s['thumbnails']['rgb_capacity']<=4*1024*1024 and s['thumbnails']['scratch_capacity']<=160*160*3
            if args.large_codecs:
                for name,fmt in [('blue.png','png'),('photo.jpg','jpeg')]:
                    row=find(s,name)
                    assert row and row['source_width']==6000 and row['source_height']==4000 and row['format']==fmt,row
                    assert row['revision'] and max(row['width'],row['height'])==160,row
                instance['checks'].append('24MP row PNG + 24MP progressive DC JPEG decoded at160px from expected/effective FD; visible Grid demand, same cache/inspector')
            capture('grid-large-codecs' if args.large_codecs else 'grid-visible-media')
            before_bytes=s['thumbnails']['rgb_capacity'];s=state(command('thumb-select-red'),lambda s:color(s,'red.png',(220,30,40)) and s['thumbnails']['painted']>=8)
            assert s['thumbnails']['rgb_capacity']==before_bytes,'inspector cloned cache RGB'
            capture('grid-and-inspector-shared')
            state(command('thumb-select-raw'),lambda s:any('\\xFF.png"' in row['path'] and row['pixel']==[180,40,200] for row in s['thumbnails']['ready']))
            instance['checks'].append('nonUTF8 filename decoded via raw byte IPC; source identity preserved')
            state(command('thumb-select-video'),lambda s:(row:=find(s,'clip.mkv')) and row['pixel'][2]>200 and row['pixel'][0]<40)
            count=text().count('FILES_PREVIEW_HELPER ');until=time.monotonic()+1.3
            while time.monotonic()<until:live_helpers();time.sleep(.02)
            assert text().count('FILES_PREVIEW_HELPER ')==count,'stable cache repeatedly launched decoder'
            instance['checks'].append('stable image/video cache revalidated by FD probes without new decoder processes')
            # Space must release thumbnail cache, drain, and use the same video engine.
            state(command('thumb-click-preview'),lambda s:s['preview']=='video' and s['thumbnails']['cached']==0 and not s['thumbnails']['busy'])
            state(command('thumb-preview-close'),lambda s:s['preview']=='closed' and len(s['thumbnails']['ready'])>=7)
            instance['checks'].append('inspector click→Space→close drains one helper and frees cache before full preview')
            old=(folder/'red.png').stat();replacement=folder/'next';png(replacement,(20,50,230));os.utime(replacement,ns=(old.st_atime_ns,old.st_mtime_ns));before=len(text());os.replace(replacement,folder/'red.png')
            state(before,lambda s:color(s,'red.png',(20,50,230)));state(command('thumb-select-red'),lambda s:color(s,'red.png',(20,50,230)) and s['thumbnails']['painted']>=2);capture('replaced-source-shared')
            before=len(text());raw_source.unlink();state(before,lambda s:s['thumbnails']['errors']>=1 and not any('\\xFF.png"' in row['path'] for row in s['thumbnails']['ready']) and s['thumbnails']['cached']<=6)
            instance['checks'].append('external raw-source deletion clears its cached RGB and readiness')
            before=len(text());png(folder/'red.png',(30,200,70));state(before,lambda s:color(s,'red.png',(30,200,70)))
            before=command('thumb-trash');wait(lambda:re.search(r'^FILES_MUTATION_FINISHED id=\d+ success=true undo=true redo=false$',text()[before:],re.M),'own Trash ACK absent');assert not (folder/'red.png').exists()
            state(before,lambda s:not find(s,'red.png') and s['preview']=='closed');instance['checks'].append('own Trash ACK/refresh invalidates selection cache')
            state(command('enter-one'),lambda s:len(s['thumbnails']['ready'])==1 and any('/inside.png"' in r['path'] for r in s['thumbnails']['ready']))
            capture('folder-demand-changed')
            state(command('back'),lambda s:find(s,'photo.jpg') is not None)
            # Clear cache and close while a real logged decoder process is alive.
            command('refresh');live=wait(live_helpers,'no active helper for callback cancellation');instance['helper_alive_at_close_request']=live
        if not backend_loss:command('callback')
        wait(lambda:process.poll() is not None,'close absent',True)
        if backend_loss:
            assert process.returncode==1 and 'FILES_BACKEND_WIDGET_LOST' in text() and 'FILES_FAILURE_DRAINED' in text() and 'panicked at' not in text()
            instance['checks'].append('own C++ window destruction while helper alive: guarded widget loss, cancellation/reap then deliberate exit1; not compositor/SuperW input')
        else:
            assert process.returncode==0 and 'FILES_CLOSE_CALLBACK source=fltk_callback' in text() and 'FILES_CLOSE_DRAINED' in text()
            assert 'FILES_THUMBNAILS_CLOSE allowed=true' in text()
        remaining=[]
        for pid,start in spawned:
            fields=stat_for(pid,start,'cleanup')
            if fields and int(fields[19])==start:remaining.append([pid,start,fields[0]])
        assert not remaining,'helper identities still present '+repr(remaining)
        normal=True;instance.update(expected_exit=1 if backend_loss else 0,actual_exit=process.returncode,helpers=sorted(spawned),cleanup_complete=True,observed_live_helpers=observed_live,concurrency_scope='logged PID/starttime polls + single-flight code; not full lifetime coverage or resource measurement')
    except BaseException as error:
        instance['error']=repr(error);raise
    finally:
        if process and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        instance['completed_expected_lifecycle']=normal;instance['normal_exit']=normal and not backend_loss;instance['log']=text();report['instances'].append(instance)

try:
    with tempfile.TemporaryDirectory(prefix='thumbnails-ui-',dir=BASE) as temp:run(Path(temp))
    if not args.normal_only:
        with tempfile.TemporaryDirectory(prefix='thumbnails-lost-event-',dir=BASE) as temp:run(Path(temp),True)
        with tempfile.TemporaryDirectory(prefix='thumbnails-widget-loss-',dir=BASE) as temp:run(Path(temp),backend_loss=True)
    report['functional_gate']='PASS'
except BaseException as error:
    report.update(functional_gate='FAIL',error=repr(error));raise
finally:
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    output=OUT/('thumbnails-ui-'+str(time.time_ns())+'.json')
    with output.open('x') as out:json.dump(report,out,indent=2)
    print(output)
