#!/usr/bin/env python3
"""DOCX extraction UI and source lifecycle; handlers/framebuffer, no OS input/metrics."""
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import zipfile
from memcheck import geometry, observed_geometry, stop_tree

ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
EVIDENCE=ROOT/'docs/evidence/0.0.4'
report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','captures':[],
        'method':'shared preview/selection/copy Actions, actual helper and FLTK framebuffer; no native OS input, no external clipboard read or performance sampling'}
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2))
matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')];assert len(matches)==1
os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
WORD='http://schemas.openxmlformats.org/wordprocessingml/2006/main'
def document(label):
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<w:document xmlns:w="{WORD}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>
<w:p><w:r><w:t>Documento {label}</w:t></w:r></w:p>
<w:p><w:r><w:t>niño 文档 🙂</w:t><w:tab/><w:t>fin</w:t></w:r></w:p>
<w:p><w:r><w:t>Texto &amp; símbolos &lt;seguros&gt;</w:t><w:br/><w:t>Segunda línea</w:t></w:r></w:p>
<w:p><w:hyperlink r:id="rId1"><w:r><w:t>Enlace externo inerte</w:t></w:r></w:hyperlink></w:p>
<w:del><w:r><w:delText>BORRADO</w:delText></w:r></w:del><w:r><w:instrText>IGNORAR</w:instrText></w:r>
<w:sectPr/></w:body></w:document>'''.encode()
def package(xml,method=zipfile.ZIP_DEFLATED,padding=0):
    data=io.BytesIO()
    with zipfile.ZipFile(data,'w',compression=method) as z:
        for name,value in [('word/document.xml',xml),('word/_rels/document.xml.rels',b'<external Target="https://invalid.test/no-request"/>'),('../outside-created',b'not extracted')]:
            info=zipfile.ZipInfo(name,date_time=(2020,1,1,0,0,0));info.compress_type=method;z.writestr(info,value)
        z.comment=b'P'*padding
    return data.getvalue()
try:
    with tempfile.TemporaryDirectory(prefix='office-ui-',dir=BASE) as temporary:
        test=Path(temporary);folder=test/'files';folder.mkdir()
        # Storage precondition: earlier ENOSPC replacement failure is not a feature failure.
        (test/'probe-a').write_bytes(b'a');(test/'probe-b').write_bytes(b'b');os.replace(test/'probe-b',test/'probe-a')
        a,b=package(document('ALFA')),package(document('BETA'));size=max(len(a),len(b))
        a=package(document('ALFA'),padding=size-len(a));b=package(document('BETA'),padding=size-len(b));assert len(a)==len(b)
        source=folder/'report.docx';source.write_bytes(a)
        (folder/os.fsdecode(b'raw-\xff.docx')).write_bytes(package(document('RAW')))
        (folder/'link.docx').symlink_to('report.docx')
        (folder/'too-large.docx').write_bytes(package(b'x'*(1024*1024+1)))
        (folder/'dtd.docx').write_bytes(package(b'<!DOCTYPE x SYSTEM "https://invalid.test/external">'+document('ALFA')))
        long_xml=f'<w:document xmlns:w="{WORD}"><w:body><w:p><w:r><w:t>{"é"*(128*1024+1)}</w:t></w:r></w:p></w:body></w:document>'.encode()
        (folder/'truncated.docx').write_bytes(package(long_xml))
        bad=bytearray(package(document('ALFA'),zipfile.ZIP_STORED));at=30+len(b'word/document.xml');bad[at+10]^=1;(folder/'bad.docx').write_bytes(bad)
        raw=test/'capture.ppm';log=test/'app.log'
        private=test/'tmp';private.mkdir()
        env={**os.environ,'TMPDIR':str(private),'MAGICK_TEMPORARY_PATH':str(private),'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_TEST_DROP_MEDIA_EVENTS':'1'}
        with log.open('w') as out:process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','text','--exit-after','45'],env=env,stdout=subprocess.DEVNULL,stderr=out,start_new_session=True)
        normal=False
        def wait(check,reason,allow_exit=False):
            deadline=time.monotonic()+10
            while time.monotonic()<deadline:
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; root exited')
                time.sleep(.025)
            raise AssertionError(reason)
        def command(action):
            after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
            match=wait(lambda:re.search(r'FILES_PREVIEW_COMMAND_REV action='+re.escape(action)+r' revision=(\d+)',log.read_text()[after:]),'command failed '+action,action=='callback')
            return int(match[1]),after
        def fresh(after,revision=None,expected_path=None):
            expected_path=expected_path or source;st=expected_path.stat()
            expected=dict(device=st.st_dev,inode=st.st_ino,bytes=st.st_size,modified_seconds=st.st_mtime_ns//10**9,modified_nanoseconds=st.st_mtime_ns%10**9,changed_seconds=st.st_ctime_ns//10**9,changed_nanoseconds=st.st_ctime_ns%10**9)
            def check():
                for row in reversed(log.read_text()[after:].splitlines()):
                    if not row.startswith('FILES_PREVIEW_READY ') or 'encoding=DOCX · texto extraído ' not in row or revision is not None and f'revision={revision} ' not in row:continue
                    match=re.search(r'stamp=Some\(Revision \{ (.*?) \}\)',row)
                    if match and {key:int(value) for key,value in (part.split(': ') for part in match[1].split(', '))}==expected:return row
                return False
            return wait(check,'fresh checked DOCX Ready absent')
        def ready(action):
            path=folder/os.fsdecode(b'raw-\xff.docx') if action=='office-raw' else folder/'truncated.docx' if action=='office-truncated' else source
            revision,after=command(action);return fresh(after,revision,path)
        def capture(name):
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            def complete():
                if not raw.exists():return False
                parts=raw.read_bytes().split(b'\n',3)
                return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
            wait(complete,'framebuffer incomplete');path=EVIDENCE/(name+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(path)],env=env,check=True,timeout=5);report['captures'].append(str(path))
        def error(action,name):
            _,after=command(action)
            return wait(lambda:next((r for r in log.read_text()[after:].splitlines() if r.startswith('FILES_PREVIEW_ERROR ') and '/'+name+'"' in r),False),'fresh exact error absent '+name)
        try:
            wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'UI smoke absent')
            wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry absent');report['geometry']=observed_geometry(process.pid,1000,650)
            ready('office');command('office-assert-a');capture('office-main-body')
            command('office-select');capture('office-visible-selection')
            st=source.stat();replacement=folder/'replacement.docx';replacement.write_bytes(b);os.utime(replacement,ns=(st.st_atime_ns,st.st_mtime_ns));after=len(log.read_text());os.replace(replacement,source)
            assert source.stat().st_size==st.st_size and source.stat().st_mtime_ns==st.st_mtime_ns
            fresh(after);command('office-assert-b');capture('office-replaced-same-size-mtime')
            after=len(log.read_text());source.unlink();wait(lambda:any(r.startswith('FILES_PREVIEW_ERROR ') and '/report.docx"' in r for r in log.read_text()[after:].splitlines()),'delete did not invalidate')
            after=len(log.read_text());source.write_bytes(b);fresh(after);command('office-assert-b')
            ready('office-raw');command('office-assert-raw');capture('office-raw-source')
            report['errors']=[error('office-bad','bad.docx'),error('office-oversize','too-large.docx'),error('office-dtd','dtd.docx'),error('office-symlink','link.docx')]
            capture('office-symlink-error')
            row=ready('office-truncated');assert 'truncated=true ' in row and 'bytes=262144 ' in row;capture('office-truncated-explicit')
            ready('office-rapid');command('office-assert-b')
            assert not (test/'outside-created').exists(),'ZIP member extracted outside archive'
            command('callback');wait(lambda:process.poll() is not None,'normal close blocked',True)
            text=log.read_text();assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in text,'exit0 with helper ACK required'
            identities=sorted(set((int(p),int(s)) for p,s in re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=(\d+) command=',text)));assert identities
            for pid,start in identities:
                proc=Path(f'/proc/{pid}')
                for attempt in range(2):
                    try:
                        state=(proc/'stat').read_text().rsplit(')',1)[1].split();assert int(state[19])!=start,'known helper alive or zombie';break
                    except (FileNotFoundError,ProcessLookupError):
                        if not proc.exists():break
                        if attempt:raise AssertionError('known PID directory with unreadable stat')
            normal=True
            report.update(functional_gate='PASS',selection='full extracted document, Copy handler only; clipboard contents NOT_VALIDATED',notify_callbacks_suppressed=True,same_size_mtime_replace=True,delete_recreate=True,opaque_source_path=True,latest_wins=True,normal_callback_exit0_after_ack=True,logged_helper_cleanup={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start identities, not lifetime/resource coverage'},no_zip_member_extraction=True,log=text)
        except BaseException:
            report['log']=log.read_text();raise
        finally:
            if not normal:stop_tree(process,None)
except BaseException as error:
    report['error']=repr(error);raise
finally:
    for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
    path=EVIDENCE/('office-ui-'+str(time.time_ns())+'.json')
    with path.open('x') as out:json.dump(report,out,indent=2)
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
