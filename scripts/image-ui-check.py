#!/usr/bin/env python3
"""Space bitmap UI, binary helper, zoom and cleanup; no native input or clipboard."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import struct
import zlib
import time
from memcheck import Metrics, geometry, observed_geometry, stop_tree
from process_metrics import TreeSampler, read_stat

ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--capture',action='store_true')
parser.add_argument('--jpeg-large',action='store_true')
parser.add_argument('--png-large',action='store_true')
parser.add_argument('--discovery',choices=['scan','children'],default='scan')
args=parser.parse_args()
if args.png_large and (not args.capture or args.jpeg_large):parser.error("--png-large requires --capture and excludes --jpeg-large; performance phase remains frozen")
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
report={'method':'Space image UI handlers and FLTK framebuffer; no native OS input','captures':[],'clipboard':'not read in resource mode','resource_gate':'NOT_MEASURED' if args.capture else 'UNVALIDATED'}
with tempfile.TemporaryDirectory(prefix='preview-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    private_tmp=test/'tmp';private_tmp.mkdir()
    fixture_env={**os.environ,'TMPDIR':str(private_tmp),'MAGICK_TEMPORARY_PATH':str(private_tmp),'MAGICK_MEMORY_LIMIT':'32MiB','MAGICK_MAP_LIMIT':'0'}
    def png(path,w,h):
        def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data)&0xffffffff)
        compressor=zlib.compressobj();parts=[]
        for y in range(h):
            color=(220,60,80,180) if y<h//2 else (20,180,220,90)
            row=bytes(color)*(w//2)+bytes((60,220,120,255))*(w-w//2)
            parts.append(compressor.compress(b'\0'+row))
        parts.append(compressor.flush());data=b''.join(parts)
        path.write_bytes(b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0))+chunk(b'IDAT',data)+chunk(b'IEND',b''))
    if args.jpeg_large:
        for mp,w,h in [(12,4000,3000),(24,6000,4000)]:
            subprocess.run(['magick','-size',f'{w}x{h}','gradient:#1f4580-#de6d60','-fill','#74ba6f','-draw',f'circle {w//2},{h//2} {w//2},{h//4}','-sampling-factor','2x2','-quality','85','-interlace','none',str(folder/f'baseline{mp}.jpg')],env=fixture_env,check=True)
            subprocess.run(['magick',str(folder/f'baseline{mp}.jpg'),'-interlace','Plane',str(folder/f'progressive{mp}.jpg')],env=fixture_env,check=True)
        jpeg=(folder/'baseline24.jpg').read_bytes()
        exif=b'Exif\0\0'+b'II*\0'+struct.pack('<I',8)+struct.pack('<H',1)+struct.pack('<HHI',274,3,1)+struct.pack('<H',6)+b'\0\0'+b'\0'*4
        (folder/'oriented24.jpg').write_bytes(jpeg[:2]+b'\xff\xe1'+struct.pack('>H',len(exif)+2)+exif+jpeg[2:])
        progressive=(folder/'progressive24.jpg').read_bytes()
        (folder/'progressive-oriented24.jpg').write_bytes(progressive[:2]+b'\xff\xe1'+struct.pack('>H',len(exif)+2)+exif+progressive[2:])
    else:
        png(folder/'alpha.png',2048,2048)
        png(folder/'large.png',3000,3000) # valid9MP, decoded by rows now
        if args.png_large:
            png(folder/'rows12.png',4000,3000);png(folder/'rows24.png',6000,4000)
            exif=b'II*\0'+struct.pack('<I',8)+struct.pack('<H',1)+struct.pack('<HHI',274,3,1)+struct.pack('<H',6)+b'\0\0'+b'\0'*4
            chunk=struct.pack('>I',len(exif))+b'eXIf'+exif+struct.pack('>I',zlib.crc32(b'eXIf'+exif)&0xffffffff)
            original=(folder/'rows24.png').read_bytes();(folder/'rows24-oriented.png').write_bytes(original[:-12]+chunk+original[-12:])
            shutil.copyfile(ROOT/'crates/helper/tests/fixtures/adam7-rgba-31x23.png',folder/'adam7.png')
        subprocess.run(['magick',str(folder/'alpha.png'),'-resize','480x320!','-alpha','off',str(folder/'oriented.jpg')],env=fixture_env,check=True)
        jpeg=(folder/'oriented.jpg').read_bytes()
        exif=b'Exif\0\0'+b'II*\0'+struct.pack('<I',8)+struct.pack('<H',1)+struct.pack('<HHI',274,3,1)+struct.pack('<H',6)+b'\0\0'+b'\0'*4
        (folder/'oriented.jpg').write_bytes(jpeg[:2]+b'\xff\xe1'+struct.pack('>H',len(exif)+2)+exif+jpeg[2:])
        subprocess.run(['magick','-size','1x1','xc:red','-define','webp:lossless=true',str(folder/'tiny.webp')],env=fixture_env,check=True)
        (folder/'broken.png').write_bytes(b'corrupt PNG')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**fixture_env,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_WAIT_TRACE':'1'}
    with log.open('w') as out:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','image','--exit-after','30'],env=env,stderr=out,stdout=subprocess.DEVNULL,start_new_session=True)
    sampler=None if args.capture else TreeSampler(process.pid,discovery=args.discovery) if args.discovery!='scan' else TreeSampler(process.pid)
    metrics=Metrics();helpers=set();terminal=[];stable_since=stable_until=None;normal=False
    def sample():
        if sampler and process.poll() is None:
            value=sampler.sample()
            if not value['root_alive'] and not value['errors'] and not value['processes']:terminal.append(value)
            else:metrics.add(value)
            for item in value['processes']:
                try:
                    if Path(f'/proc/{item["pid"]}/exe').resolve()==ROOT/'target/release/files-helper':helpers.add((item['pid'],item['start']))
                except OSError:pass
    def wait(check,reason,allow_exit=False,timeout=8):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            sample();value=check()
            if value:return value
            if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; exited\n'+log.read_text())
            time.sleep(.005 if sampler else .025)
        raise AssertionError(reason+'\n'+log.read_text())
    def command(action):
        offset=len(log.read_text())
        before=log.read_text().count('FILES_PREVIEW_COMMAND_OK '+action+'\n')
        pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
        wait(lambda:log.read_text().count('FILES_PREVIEW_COMMAND_OK '+action+'\n')>before,'command failed: '+action,action.startswith('close'))
        return offset
    def ready(name,after=0):
        def check():
            markers=[line for line in log.read_text()[after:].splitlines() if line.startswith(('FILES_PREVIEW_IMAGE_READY','FILES_PREVIEW_ERROR'))]
            return markers[-1] if markers and markers[-1].startswith('FILES_PREVIEW_IMAGE_READY') and '/'+name+'"' in markers[-1] else None
        return wait(check,'image absent for '+name)
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'capture missing')
        path=evidence/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=fixture_env,check=True);report['captures'].append(str(path))
    def cleanup():
        survivors=[];errors=[]
        if sampler is None:
            records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=',log.read_text())
            assert records and all(start.isdigit() for _,start in records),'helper identity absent'
            identities={(int(pid),int(start)) for pid,start in records}
            for pid,start in identities:
                p=Path(f'/proc/{pid}')
                for attempt in range(2):
                    try:
                        stat=read_stat((p/'stat').read_text())
                        if stat['start']==start:survivors.append([pid,start,stat['state']])
                        break
                    except (FileNotFoundError,ProcessLookupError):
                        if not p.exists():break
                        if attempt:raise AssertionError('known helper stat disappeared but PID still present')
            return {'complete':not survivors,'survivors':survivors,'logged_identities':sorted(identities),'zombies_accepted':False,'scope':'logged PID/start identities after exit0, no lifetime/resource sampling'}
        for pid,start in sorted(sampler.known if sampler else []):
            try:
                stat=read_stat(Path(f'/proc/{pid}/stat').read_text())
                if stat['start']==start and stat['state']!='Z':survivors.append([pid,start])
            except FileNotFoundError:
                if Path(f'/proc/{pid}').exists():errors.append(f'{pid}: stat missing, process directory present')
            except (OSError,ValueError,IndexError) as e:errors.append(f'{pid}/{start}: {e}')
        return {'complete':not survivors and not errors,'survivors':survivors,'errors':errors,'scope':'observed PID/starttime tree, read-only after exit0' if sampler else 'root exit0 only; no descendant sampler'}
    try:
        wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'UI not ready')
        wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry absent')
        report['geometry']=observed_geometry(process.pid,1000,650)
        if args.jpeg_large:
            ready('baseline12.jpg',command('jpeg12'))
            assert 'width=1000 height=750 source=4000x3000 format=jpeg' in log.read_text()
            if args.capture:capture('jpeg12-idct')
            ready('baseline24.jpg',command('jpeg24'))
            assert 'width=750 height=500 source=6000x4000 format=jpeg' in log.read_text()
            if args.capture:capture('jpeg24-idct')
            if args.capture:
                ready('oriented24.jpg',command('jpeg24-oriented'))
                assert 'width=500 height=750 source=6000x4000 format=jpeg' in log.read_text()
                capture('jpeg24-oriented-idct')
                for mp,w,h,sw,sh in [(12,500,375,4000,3000),(24,750,500,6000,4000)]:
                    after=command(f'progressive{mp}')
                    marker=ready(f'progressive{mp}.jpg',after)
                    assert f'width={w} height={h} source={sw}x{sh} format=jpeg' in marker,marker
                    command('jpeg-progressive-assert')
                    capture(f'jpeg-progressive{mp}-dc-preview')
                after=command('progressive24-oriented');marker=ready('progressive-oriented24.jpg',after)
                assert 'width=500 height=750 source=6000x4000 format=jpeg' in marker,marker
                capture('jpeg-progressive24-dc-oriented')
                ready('baseline24.jpg',command('jpeg24'))
            stable_since=time.monotonic();until=stable_since+1
            while time.monotonic()<until:sample();time.sleep(.005 if sampler else .025)
            stable_until=time.monotonic()
            command('close-jpeg-pending' if args.capture else 'close')
            wait(lambda:process.poll() is not None,'close did not finish',True)
            assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text()
        else:
            ready('alpha.png',command('png'))
            if args.capture:capture('image-alpha')
            ready('alpha.png',command('image-zoom'))
            wait(lambda:'zoom=2 ' in log.read_text(),'zoom not applied')
            if args.capture:capture('image-zoom')
            ready('oriented.jpg',command('jpeg'))
            marker=ready('oriented.jpg')
            assert 'width=320 height=480 source=480x320 format=jpeg' in log.read_text()
            if args.capture:capture('image-oriented')
            ready('tiny.webp',command('webp'))
            assert 'width=1 height=1 source=1x1 format=webp' in log.read_text()
            ready('oriented.jpg',command('image-rapid'))
            after=command('image-corrupt')
            wait(lambda:any(line.startswith('FILES_PREVIEW_ERROR ') and '/broken.png"' in line for line in log.read_text()[after:].splitlines()),'fresh exact corrupt error missing')
            if args.capture:capture('image-corrupt')
            marker=ready('large.png',command('image-large'));assert 'width=1024 height=1024 source=3000x3000 format=png' in marker
            command('png-source-assert')
            if args.capture:capture('png9mp-rows')
            if args.png_large:
                for action,name,w,h,sw,sh in [('png12','rows12.png',1024,768,4000,3000),('png24','rows24.png',1024,682,6000,4000),('png24-oriented','rows24-oriented.png',682,1024,6000,4000)]:
                    marker=ready(name,command(action));assert f'width={w} height={h} source={sw}x{sh} format=png' in marker,marker
                    command('png-source-assert');capture(action+'-row-preview')
                ready('adam7.png',command('png-adam7'));command('image-zoom');capture('png-adam7-zoom')
            ready('alpha.png',command('png'))
            stable_since=time.monotonic()
            until=stable_since+1
            while time.monotonic()<until:sample();time.sleep(.005 if sampler else .025)
            stable_until=time.monotonic()
            command('close-image-pending')
            wait(lambda:process.poll() is not None,'close did not finish',True)
            assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'exit without drain confirmation'
        normal=True;report['cleanup']=cleanup()
        assert report['cleanup']['complete'],'logged helper identity remains after normal exit'
        report['functional_gate']='PASS'
        report['normal_exit_after_preview_drain']=True
        report['fixtures']='baseline JPEG12/24MP RGB-pattern IDCT, EXIF24MP and actual progressive12/24 reducedDC previews/orientation in capture only; resource mode two baseline files only/no pending-close spawn' if args.jpeg_large else ('RGBA PNG4/9/12/24MP by rows, lateEXIF24MP/Adam7, JPEG/WebP/corrupt, rapid/zoom/close' if args.png_large else 'RGBA PNG4/9MP byrows, JPEG EXIF6,WebP,corrupt,rapid/zoom/close')
        report['log']=log.read_text()
        if sampler:
            report['metrics']=metrics.report(stable_since,stable_until)
            report['metrics']['sample_interval_requested_ms']=5.0
            report['stable_since_monotonic']=stable_since;report['stable_until_monotonic']=stable_until
            report['stable_phase']='one-second post-Ready window; can include first software paint, not universal idle'
            report['discovery_mode']=args.discovery
            report['terminal_exit_observations']=terminal
            report['helper_identities_verified']=sorted(helpers)
            records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=preview_image',log.read_text())
            unknown=[(pid,start) for pid,start in records if not start.isdigit()]
            spawned={(int(pid),int(start)) for pid,start in records if start.isdigit()}
            report['helpers_with_unknown_starttime']=unknown
            missing=spawned-helpers
            report['spawned_image_helpers']=sorted(spawned)
            report['helpers_without_memory_sample']=sorted(missing)
            report['sampled_peak_memory_mb_decimal']=report['metrics']['sampled_maxima']['memory_kib']*1024/1e6
            report['resource_gate']=('PASS' if report['sampled_peak_memory_mb_decimal']<=150 else 'FAIL') if report['metrics']['complete'] and report['cleanup']['complete'] and spawned and not missing and not unknown else 'UNVALIDATED'
            report['goal_met']=report['sampled_peak_memory_mb_decimal']<=50 if report['resource_gate']!='UNVALIDATED' else None
            report['observed_goal_exceeded']=report['sampled_peak_memory_mb_decimal']>50
            report['resource_scope']='sampled main+helpers PSS+Swap, all logged helpers require observed memory; not full lifetimes/exact peak; GPU excluded; '+('two baseline12/24MP synthetic-pattern JPEG decodes; five listed fixtures, progressive decoding not measured, not baseline10k' if args.jpeg_large else '5files, not baseline10k or photos12/24MP')
    except BaseException as e:
        text=log.read_text()
        report.update(functional_gate='FAIL',error=repr(e),log=text)
        (evidence/f'image-failure-{time.time_ns()}.json').write_text(json.dumps(report,indent=2)+'\n')
        raise
    finally:
        if not normal:stop_tree(process,sampler)
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    path=evidence/(('jpeg-large-ui-' if args.jpeg_large else 'image-ui-')+str(time.time_ns())+'.json' if args.capture else ('jpeg-large-resources-' if args.jpeg_large else 'image-resources-')+str(time.time_ns())+'.json')
    path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':report['resource_gate']}))
    if report['functional_gate']!='PASS' or not args.capture and report['resource_gate']!='PASS':raise SystemExit(1)
