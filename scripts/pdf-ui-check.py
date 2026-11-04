#!/usr/bin/env python3
"""Functional PDF multipage/zoom capture only. Resource/startup phase is explicitly suspended.
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

ROOT=Path(__file__).resolve().parent.parent
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
parser=argparse.ArgumentParser();parser.add_argument('--source-lifecycle',action='store_true');parser.add_argument('--drop-events',action='store_true');parser.add_argument('--text',action='store_true');args=parser.parse_args()
report={'text_mode':args.text,'source_lifecycle':args.source_lifecycle,'notify_callbacks_suppressed':args.drop_events,'checks':[],'method':'internal handlers, actual pdfinfo/pdftoppm local raster and actual FLTK framebuffer; no native OS input or clipboard',
        'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','reason':'User phase: integration/captures only; no resource/startup measurements until released',
        'captures':[],'known_pending':['selection mapped to original PDF page geometry and Unicode PDF font coverage','complex/password/hostile PDF coverage','resource budget','cross-platform verification']}
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
if len(matches)!=1:raise RuntimeError('No unique compositor for fixture Wayland socket')
os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance'];report['compositor_binding']={'selected':matches[0],'scope':'fixture-only environment, no focus/input/global configuration'}
with tempfile.TemporaryDirectory(prefix='pdf-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    def pdf_fixture(path,blue=False):
        streams=[b'1 0 0 rg 0 0 600 800 re f 1 1 1 rg BT /F1 30 Tf 60 700 Td (PDF pagina 1) Tj ET',b'0 0 1 rg 0 0 600 800 re f 1 1 1 rg BT /F1 30 Tf 60 700 Td (PDF pagina 2) Tj ET']
        if blue:streams[0]=streams[0].replace(b'1 0 0 rg',b'0 0 1 rg',1)
        objects=[b'<< /Type /Catalog /Pages 2 0 R >>',b'<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>',b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 7 0 R >> >> /Contents 4 0 R >>',b'<< /Length '+str(len(streams[0])).encode()+b' >>\nstream\n'+streams[0]+b'\nendstream',b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>',b'<< /Length '+str(len(streams[1])).encode()+b' >>\nstream\n'+streams[1]+b'\nendstream',b'<< /Type /Font /Subtype /Type1 /BaseFont /Courier >>']
        data=b'%PDF-1.4\n';offsets=[0]
        for index,obj in enumerate(objects,1):
            offsets.append(len(data));data+=str(index).encode()+b' 0 obj\n'+obj+b'\nendobj\n'
        xref=len(data);data+=b'xref\n0 8\n0000000000 65535 f \n'+b''.join(f'{offset:010} 00000 n \n'.encode() for offset in offsets[1:])+b'trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n'+str(xref).encode()+b'\n%%EOF\n';path.write_bytes(data)
    pdf_fixture(folder/'pages.pdf')
    (folder/'large.html').write_bytes(b'x'*32769)
    (folder/'bad.pdf').write_text('not a pdf')
    (folder/'symlink.pdf').symlink_to('pages.pdf')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_FIXED_SIZE':'1','FILES_CLOSE_TRACE':'1'}
    if args.drop_events:env['FILES_TEST_DROP_MEDIA_EVENTS']='1'
    with log.open('w') as output:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','pdf','--exit-after','45'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
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
    def proof(row):
        match=re.search(r'(?:source|stamp)=Some\(Revision \{ (.*?) \}\)',row)
        assert match,'PDF FD proof absent'
        actual={k:int(v) for k,v in (part.split(': ') for part in match[1].split(', '))}
        st=(folder/'pages.pdf').stat()
        expected=dict(device=st.st_dev,inode=st.st_ino,bytes=st.st_size,modified_seconds=st.st_mtime_ns//10**9,modified_nanoseconds=st.st_mtime_ns%10**9,changed_seconds=st.st_ctime_ns//10**9,changed_nanoseconds=st.st_ctime_ns%10**9)
        assert actual==expected,('PDF FD proof mismatch',actual,expected)
        return actual
    def fresh(after,page,revision=None):
        row=wait(lambda:next((row for row in reversed(log.read_text()[after:].splitlines()) if row.startswith('FILES_PREVIEW_PDF_READY ') and '/pages.pdf"' in row and (revision is None or f'revision={revision} ' in row) and f'page={page} pages=2 ' in row),False),'fresh PDF Ready absent')
        proof(row)
        return row
    def ready(action,page):
        revision,after=command(action)
        return fresh(after,page,revision)
    def ready_text(action,page):
        revision,after=command(action)
        row=wait(lambda:next((row for row in reversed(log.read_text()[after:].splitlines()) if row.startswith('FILES_PREVIEW_PDF_TEXT_READY ') and '/pages.pdf"' in row and f'revision={revision} ' in row and f'page={page} pages=Some(2) ' in row),False),'fresh extracted PDF text absent')
        proof(row)
        return row
    def unavailable(after):
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/pages.pdf"' in row for row in log.read_text()[after:].splitlines()),'PDF source invalidation absent')
        command('pdf-assert-unavailable')
    def gone(pid,start):
        try:return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])!=start
        except FileNotFoundError:return True
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
        ready('pdf',1);command('pdf-assert-first');capture('pdf-page-first')
        ready('pdf-next',2);command('pdf-assert-second');capture('pdf-page-second')
        command('pdf-zoom');command('pdf-pan');capture('pdf-zoom-pan');command('pdf-assert-pan')
        ready('pdf-rapid',2);command('pdf-assert-second')
        if args.text:
            ready_text('pdf-text',2);capture('pdf-extracted-text-page2');command('pdf-text-select');capture('pdf-extracted-text-selected')
            ready_text('pdf-prev',1);command('pdf-text-select');capture('pdf-extracted-first-page')
            ready('pdf-text',1);command('pdf-assert-first');capture('pdf-return-to-page')
            report['checks'].append('on-demand text mode page2 and page1, effective source proof, pointer selection equals extracted page text; Copy handler invoked, OS clipboard NOT_VALIDATED; return to raster releases extracted text')
            # Terminal source caps must not spin new worker tokens for the same
            # stable unsupported file. This is lifecycle observation, not CPU/memory sampling.
            with (folder/'bad.pdf').open('wb') as file:file.truncate(256*1024*1024+1)
            _,after=command('pdf-bad')
            wait(lambda:any(r.startswith('FILES_PREVIEW_ERROR ') and '/bad.pdf"' in r and 'hasta256MiB' in r for r in log.read_text()[after:].splitlines()),'PDF source cap not visible')
            revision,_=command('pdf-assert-unavailable');helpers=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2)
            later,_=command('pdf-assert-unavailable');assert revision==later and helpers==log.read_text().count('FILES_PREVIEW_HELPER '),'unsupported PDF retried same revision'
            (folder/'bad.pdf').write_text('not a pdf')
            _,after=command('html-large')
            wait(lambda:any(r.startswith('FILES_PREVIEW_ERROR ') and '/large.html"' in r and 'límite32KiB' in r for r in log.read_text()[after:].splitlines()),'HTML source cap not visible')
            revision,_=command('pdf-assert-unavailable');helpers=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2)
            later,_=command('pdf-assert-unavailable');assert revision==later and helpers==log.read_text().count('FILES_PREVIEW_HELPER '),'unsupported HTML retried same revision'
            report['checks'].append('stable unsupported PDF256MiB+1 and HTML32KiB+1 remain errors with unchanged revision and no new child over reconcile interval; not a CPU/resource metric')

        if args.source_lifecycle:
            ready('pdf',1);source=folder/'pages.pdf';original=source.read_bytes();old=source.stat()
            replacement=test/'replace.pdf';pdf_fixture(replacement,blue=True);assert replacement.stat().st_size==old.st_size
            os.utime(replacement,ns=(old.st_atime_ns,old.st_mtime_ns));before=len(log.read_text());os.replace(replacement,source)
            fresh(before,1);command('pdf-assert-blue-first');capture('pdf-source-replaced-blue')
            current=source.stat();before=len(log.read_text());source.write_bytes(original);os.utime(source,ns=(current.st_atime_ns,current.st_mtime_ns))
            fresh(before,1);command('pdf-assert-first');capture('pdf-source-modified-red')
            before=len(log.read_text());source.unlink();unavailable(before);capture('pdf-source-deleted')
            before=len(log.read_text());source.write_bytes(original);fresh(before,1);command('pdf-assert-first')
            before=len(log.read_text());os.rename(source,test/'away.pdf');unavailable(before)
            before=len(log.read_text());os.rename(test/'away.pdf',source);fresh(before,1)
            before=len(log.read_text());os.rename(folder,test/'old-parent');unavailable(before)
            folder.mkdir();before=len(log.read_text());source.write_bytes(original);fresh(before,1);command('pdf-assert-first');capture('pdf-parent-recovered')
            (folder/'bad.pdf').write_text('not a pdf');(folder/'symlink.pdf').symlink_to('pages.pdf')
            report['checks'].append('effective FD proof after same-size/mtime replacement, same inode in-place mutation restoring mtime, delete/recreate, file rename and parent replacement; stale pixels hidden before recovery')
            command('media-hide');count=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2)
            assert log.read_text().count('FILES_PREVIEW_HELPER ')==count,'hidden PDF restarted a helper'
            report['checks'].append('hidden preview launches no helper during one reconciliation interval')

        _,after=command('pdf-bad')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/bad.pdf"' in row for row in log.read_text()[after:].splitlines()),'fresh bad PDF error absent')
        capture('pdf-error')
        _,after=command('pdf-symlink')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/symlink.pdf"' in row for row in log.read_text()[after:].splitlines()),'fresh symlink PDF error absent')
        ready('pdf',1);ready('pdf-next',2)
        command('close-pdf-pending')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'normal drained exit absent'
        records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=',log.read_text());assert records and all(start.isdigit() for _,start in records)
        identities=sorted(set((int(pid),int(start)) for pid,start in records));assert all(gone(pid,start) for pid,start in identities),'logged helper survived exit (zombies not accepted)'
        report['logged_helper_cleanup']={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start identities after root exit0, not lifetime or resource sampling'}
        normal=True
        report.update(functional_gate='PASS',normal_exit_after_preview_drain=True,cleanup_scope='root exit0 plus common worker drain ACK; no descendant sampler in capture phase',log=log.read_text())
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error),log=log.read_text())
        path=evidence/('pdf-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n');print(str(path));raise
    finally:
        if not normal:stop_tree(process,None)
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['external_tools']={name:{'path':shutil.which(name),'sha256':hashlib.sha256(Path(shutil.which(name)).read_bytes()).hexdigest(),'version':subprocess.run([name,'-v'],capture_output=True,text=True,timeout=5,check=True).stderr.splitlines()[0]} for name in ('pdfinfo','pdftoppm')}
    path=evidence/('pdf-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
