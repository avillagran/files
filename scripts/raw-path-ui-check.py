#!/usr/bin/env python3
"""Opaque Unix directory argv + full Space image via bounded lossless path wire.
Functional handlers/framebuffer only. No process metrics, startup, native input.
"""
import hashlib,json,os,re,signal,struct,subprocess,tempfile,time,zlib
from pathlib import Path
ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
OUT=ROOT/'docs/evidence/raw-path';OUT.mkdir(parents=True,exist_ok=True)
report={'functional_gate':'IN_PROGRESS','method':'args_os/raw directory bytes + shared Grid/inspector/Space handlers and real FLTK framebuffer; no native input','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],'states':[]}
with tempfile.TemporaryDirectory(prefix='raw-path-ui-',dir=BASE) as temp:
    base=Path(temp);folder_bytes=os.fsencode(base)+b'/directorio\xfe';os.mkdir(folder_bytes);folder=Path(os.fsdecode(folder_bytes));tmp=base/'tool-tmp';tmp.mkdir()
    source=Path(os.fsdecode(folder_bytes+b'/raw\xff.png'))
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
    w,h=320,200;source.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,2,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes((180,40,200))*w)*h))+chunk(b'IEND',b''))
    identity=source.stat();report['source_identity']={'dev':identity.st_dev,'ino':identity.st_ino,'bytes':identity.st_size};report['directory_bytes']=list(folder_bytes)
    raw=base/'capture.ppm';log=base/'app.log';process=None;normal=False;seq=0
    try:
        env={**os.environ,'TMPDIR':str(tmp),'MAGICK_TEMPORARY_PATH':str(tmp),'FILES_CONFIG_DIR':str(base/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_HIDDEN_CONTEXT':'1','FILES_TEST_THUMBNAILS':'1','FILES_TEST_PREVIEW':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1','RUST_BACKTRACE':'1'}
        with log.open('w') as out:
            process=subprocess.Popen([str(ROOT/'target/release/files'),'--ui-smoke','--ui-scene','hidden-context','--size','1000x650','--scale','1','--exit-after','45','--',folder_bytes],env=env,stderr=out,stdout=subprocess.DEVNULL,stdin=subprocess.DEVNULL,start_new_session=True)
        def text():return log.read_text()
        def wait(check,reason,allow_exit=False):
            until=time.monotonic()+15
            while time.monotonic()<until:
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exit='+str(process.returncode))
                time.sleep(.02)
            raise AssertionError(reason)
        def command(action):
            global seq
            before=len(text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            def check():
                tail=text()[before:];assert 'FILES_SPACE_COMMAND_ERROR' not in tail,tail
                return re.search(r'^FILES_SPACE_COMMAND seq=(\d+) action='+re.escape(action)+r'$',tail,re.M)
            marker=wait(check,'command absent '+action,action=='callback');seq=int(marker[1]);return before
        def state(after=0,check=lambda s:True):
            def latest():
                values=[]
                for line in text()[after:].splitlines():
                    if line.startswith('FILES_THUMBNAILS_STATE '):
                        s=json.loads(line[len('FILES_THUMBNAILS_STATE '):])
                        if s['seq']==seq and s['path_raw']==list(folder_bytes) and check(s):values.append(s)
                return values[-1] if values else None
            s=wait(latest,'raw state absent');report['states'].append(s);assert s['path'] is None;return s
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def full():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(full,'raw image framebuffer absent');path=OUT/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=env,check=True);report['captures'].append({'path':str(path),'dimensions':list(struct.unpack('>II',path.read_bytes()[16:24]))})
        wait(lambda:'FILES_HIDDEN_STATE ' in text(),'post-smoke paint absent')
        state(command('grid'),lambda s:len(s['thumbnails']['ready'])==1 and s['thumbnails']['painted']>=1)
        state(command('thumb-select-raw'),lambda s:s['thumbnails']['painted']>=2)
        capture('raw-directory-grid-inspector')
        state(command('thumb-click-preview'),lambda s:s['preview']=='image' and s['thumbnails']['cached']==0)
        before=command('thumb-assert-raw-space')
        proof=wait(lambda:re.search(r'FILES_RAW_SPACE_PREVIEW_OK generation=(\d+) source=Some\(Revision \{ device: (\d+), inode: (\d+), bytes: (\d+)',text()[before:]),'raw image source proof absent')
        assert tuple(map(int,proof.group(2,3,4)))==(identity.st_dev,identity.st_ino,identity.st_size)
        capture('raw-directory-and-space')
        state(command('thumb-preview-close'),lambda s:s['preview']=='closed' and len(s['thumbnails']['ready'])==1)
        command('callback');wait(lambda:process.poll() is not None,'normal callback exit absent',True)
        assert process.returncode==0 and 'FILES_CLOSE_DRAINED' in text() and 'panicked at' not in text()
        records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=([^\s]+)',text());assert records and all(start.isdigit() for _,start,_ in records)
        for pid,start,_ in records:
            proc=Path('/proc')/pid
            for attempt in range(2):
                try:
                    fields=(proc/'stat').read_text().rsplit(')',1)[1].split();assert int(fields[19])!=int(start),'helper identity still present';break
                except (FileNotFoundError,ProcessLookupError):
                    if not proc.exists():break
                    if attempt:raise AssertionError('known helper still exists but stat missing')
        report.update(functional_gate='PASS',normal_exit=0,helpers=records,cleanup_scope='logged PID/starttime identities absent/reused after exit0; no lifetime/resource sampling',raw_cli=True,full_image_pixels=[180,40,200],effective_revision_matches_source=True,legacy_path_strings='core roundtrip tests, ordinary thumbnail/preview paths retain existing serialization')
        normal=True
    except BaseException as error:report.update(functional_gate='FAIL',error=repr(error));raise
    finally:
        if process and process.poll() is None:
            os.killpg(process.pid,signal.SIGTERM)
            try:process.wait(timeout=3)
            except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()
        report['log']=log.read_text() if log.exists() else '';report['normal_exit_confirmed']=normal
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest();report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        path=OUT/('raw-path-ui-'+str(time.time_ns())+'.json')
        with path.open('x') as out:json.dump(report,out,indent=2)
        print(path)
