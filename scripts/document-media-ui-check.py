#!/usr/bin/env python3
"""Bounded own-fixture document media lifecycle; functional only, NO metrics."""
import hashlib,json,os,re,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
from memcheck import geometry,observed_geometry
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/document-media';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','method':'internal shared Markdown commands + own filesystem mutations + actual FLTK framebuffer; no native input','captures':[],'cases':[],'footer_scope':'main-process PSS footer is not a tree/Swap budget and is not used','known_pending':['HTML resources','cross-document LRU','native input/clipboard','other platforms','performance gates remain frozen']}
def png(path,color):
    def chunk(k,d):return struct.pack('>I',len(d))+k+d+struct.pack('>I',zlib.crc32(k+d)&0xffffffff)
    w,h=320,160
    payload=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes(color)*w)*h))+chunk(b'IEND',b'')
    path.write_bytes(payload.ljust(2048,b'\0'))
def gone(pid,start):
    p=Path('/proc')/str(pid)
    for attempt in range(2):
        try:
            fields=(p/'stat').read_text().rsplit(')',1)[1].split()
            return int(fields[19])!=start
        except (FileNotFoundError,ProcessLookupError):
            if not p.exists():return True
            if attempt:raise AssertionError('known PID still present without readable stat')
    return False
try:
    for drop_events in [False,True]:
        case={'notify_callbacks_suppressed':drop_events,'checks':[]}
        report['cases'].append(case)
        with tempfile.TemporaryDirectory(prefix='document-media-',dir=BASE) as temp:
            base=Path(temp);folder=base/'files';folder.mkdir();(folder/'sub/deep').mkdir(parents=True)
            private=base/'tmp';private.mkdir();raw=base/'capture.ppm';log=base/'app.log';normal=False;process=None
            source=folder/'sub/deep/media.png'
            document='# Documento vivo\n\nLa imagen cambia con su original.\n\n![Recurso local](sub/deep/media.png)\n'
            (folder/'overview.md').write_text(document);png(source,(220,30,40))
            try:
                env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(base/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1','RUST_BACKTRACE':'1'}
                if drop_events:env['FILES_TEST_DROP_MEDIA_EVENTS']='1'
                with log.open('w') as out:
                    process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','markdown','--exit-after','90'],env=env,stderr=out,stdout=subprocess.DEVNULL,stdin=subprocess.DEVNULL,start_new_session=True)
                def text():return log.read_text()
                def wait(check,reason,allow_exit=False):
                    end=time.monotonic()+15
                    while time.monotonic()<end:
                        value=check()
                        if value:return value
                        if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; root exit '+str(process.returncode))
                        time.sleep(.02)
                    raise AssertionError(reason)
                def command(action):
                    before=len(text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
                    wait(lambda:('FILES_PREVIEW_COMMAND_OK '+action+'\n') in text()[before:],'command '+action,action=='callback')
                    assert 'FILES_PREVIEW_COMMAND_ERROR' not in text()[before:]
                    return before
                def ready(after,loaded=1):
                    def latest():
                        lines=[line for line in text()[after:].splitlines() if line.startswith('FILES_PREVIEW_MARKDOWN_READY ') and '/overview.md"' in line and 'layout_ready=true ' in line and ('images_loaded='+str(loaded)+' ') in line]
                        return lines[-1] if lines else None
                    line=wait(latest,'fresh published/reflowed document absent loaded='+str(loaded))
                    case['checks'].append(line);return line
                def proof():
                    before=command('markdown-source-proof')
                    values=[json.loads(line[len('FILES_DOCUMENT_PROOF '):]) for line in text()[before:].splitlines() if line.startswith('FILES_DOCUMENT_PROOF ')]
                    assert values and values[-1]['published_after_watch_probe'];return values[-1]
                def actual_proof():
                    value=proof();entries=value['resources'];assert len(entries)==1
                    assert bytes(entries[0]['relative_raw'])==b'sub/deep/media.png'
                    stat=source.stat();assert entries[0]['proof']['file']['inode']==stat.st_ino and entries[0]['proof']['file']['device']==stat.st_dev
                    return value
                def frame(name,color=None,absent=None,save=False):
                    raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
                    def complete():
                        if not raw.exists():return False
                        parts=raw.read_bytes().split(b'\n',3)
                        return parts if len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3 else None
                    parts=wait(complete,'framebuffer absent');pixels=parts[3]
                    assert list(map(int,parts[1].split()))==[1000,650]
                    if color:assert sum(pixels[i:i+3]==bytes(color) for i in range(0,len(pixels),3))>10000,'expected image pixels absent'
                    if absent:assert sum(pixels[i:i+3]==bytes(absent) for i in range(0,len(pixels),3))<100,'stale image pixels still visible'
                    if save:
                        path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=env,check=True);report['captures'].append(str(path))
                wait(lambda:'FILES_UI_SMOKE_OK' in text(),'UI smoke absent')
                wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry absent')
                ready(command('markdown'));a=actual_proof();frame('embedded-red',(220,30,40),save=not drop_events)
                old=source.stat();png(folder/'next',(20,50,230));assert (folder/'next').stat().st_size==old.st_size;os.utime(folder/'next',ns=(old.st_atime_ns,old.st_mtime_ns));before=len(text());os.replace(folder/'next',source)
                ready(before);b=actual_proof();assert b['revision']>a['revision'] and b['resources'][0]['proof']['file']['inode']!=a['resources'][0]['proof']['file']['inode']
                frame('embedded-replaced-blue',(20,50,230),(220,30,40),not drop_events)
                case['checks'].append('same path/size/mtime replacement: old red gone, blue shown, fresh effective FD inode/revision')
                inode=source.stat().st_ino;before=len(text());png(source,(180,140,20));assert source.stat().st_ino==inode
                ready(before);modified=actual_proof();assert modified['resources'][0]['proof']['file']!=b['resources'][0]['proof']['file']
                frame('embedded-inplace-yellow',(180,140,20),(20,50,230))
                case['checks'].append('in-place modification preserves inode but changes revision and replaces displayed RGB')
                before=len(text());source.unlink();ready(before,0);assert proof()['resources']==[]
                frame('embedded-deleted-placeholder',absent=(180,140,20),save=not drop_events)
                case['checks'].append('external delete: published placeholder has no old bitmap/proof')
                before=len(text());png(source,(30,200,70));ready(before);actual_proof();frame('embedded-recreated-green',(30,200,70),(20,50,230),not drop_events)
                case['checks'].append('missing image intent detects recreation without retaining old RGB')
                before=len(text());source.rename(folder/'sub/deep/away.png');ready(before,0)
                before=len(text());(folder/'sub/deep/away.png').rename(source);ready(before);actual_proof()
                case['checks'].append('rename away/back invalidates and reacquires the current path')
                before=len(text());folder.rename(base/'old-files')
                wait(lambda:any(line.startswith('FILES_PREVIEW_ERROR ') for line in text()[before:].splitlines()),'root removal error absent')
                folder.mkdir();(folder/'sub/deep').mkdir(parents=True);(folder/'overview.md').write_text(document);before=len(text());png(source,(20,50,230))
                ready(before);current=actual_proof();assert current['document']['root']['inode']==folder.stat().st_ino
                frame('embedded-root-replaced',(20,50,230),save=not drop_events)
                case['checks'].append('root rename/replacement recovers through fresh rootFD and watches; no old Ready')
                before=command('media-hide');helpers_before=text().count('FILES_PREVIEW_HELPER ')
                end=time.monotonic()+1.2
                while time.monotonic()<end:
                    assert text().count('FILES_PREVIEW_HELPER ')==helpers_before,'hidden document launched decode'
                    time.sleep(.03)
                # Close/reopen Space, not an OS minimize/restore claim.
                ready(command('markdown'));actual_proof()
                case['checks'].append('closed preview launches no helpers during wait; reopening demands fresh document proof')
                command('callback');wait(lambda:process.poll() is not None,'normal callback close absent',True)
                assert process.returncode==0 and 'FILES_CLOSE_CALLBACK' in text() and 'FILES_CLOSE_DRAINED' in text() and 'FILES_PREVIEW_DRAINED' in text() and 'panicked at' not in text()
                records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=([^\s]+)',text())
                assert records and all(start.isdigit() for _,start,_ in records)
                identities={(int(pid),int(start)) for pid,start,_ in records};assert all(gone(pid,start) for pid,start in identities)
                case.update(functional_gate='PASS',normal_exit=0,helpers=records,cleanup={'logged_identities_absent':True,'zombies_accepted':False,'scope':'logged PID/start identities after exit0; no lifetime sampling or metrics'},readonly_probe_stop=re.findall(r'FILES_MEDIA_PROBE_STOPPED=([^ ]+)',text()))
                normal=True
            finally:
                if process and process.poll() is None:
                    os.killpg(process.pid,signal.SIGTERM)
                    try:process.wait(timeout=3)
                    except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
                case['log']=text() if log.exists() else ''
                case['normal']=normal
    report['functional_gate']='PASS'
except BaseException as e:
    report.update(functional_gate='FAIL',error=repr(e))
finally:
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    path=OUT/('document-media-ui-'+str(time.time_ns())+'.json')
    with path.open('x') as out:json.dump(report,out,indent=2)
    print(path)
raise SystemExit(0 if report['functional_gate']=='PASS' else 1)
