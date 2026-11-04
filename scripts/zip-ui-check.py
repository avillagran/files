#!/usr/bin/env python3
"""Functional ZIP catalog capture only. Resource/startup phase is explicitly suspended.
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
report={'method':'internal handlers, actual files-helper ZIP central-directory catalog and actual FLTK framebuffer; no native OS input or clipboard',
        'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','reason':'User phase: integration/captures only; no resource/startup measurements until released',
        'captures':[],'known_pending':['extraction/compression','ZIP64/tar/other formats','payload/local-header/CRC integrity','resource budget','cross-platform verification']}
with tempfile.TemporaryDirectory(prefix='zip-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    import zipfile,warnings
    with warnings.catch_warnings():
        warnings.simplefilter('ignore',UserWarning)
        with zipfile.ZipFile(folder/'catalog.zip','w',compression=zipfile.ZIP_STORED) as archive:
            archive.writestr('normal.txt','hello');archive.writestr('carpeta/niño.txt','texto')
            archive.writestr('../outside.txt','no se extrae')
            link=zipfile.ZipInfo('link');link.create_system=3;link.external_attr=0o120777<<16
            archive.writestr(link,'../outside.txt');archive.writestr('duplicate.txt','first');archive.writestr('duplicate.txt','second');archive.writestr('C:\\escape.txt','no se extrae')
    with zipfile.ZipFile(folder/'empty.zip','w'):pass
    with zipfile.ZipFile(folder/'raw.zip','w') as archive:archive.writestr('rawX.bin',b'body')
    raw_zip=(folder/'raw.zip').read_bytes().replace(b'rawX.bin',b'raw\xff.bin');(folder/'raw.zip').write_bytes(raw_zip)
    for name,count in [('large.zip',2500),('over.zip',10001)]:
        with zipfile.ZipFile(folder/name,'w') as archive:
            for index in range(count):archive.writestr(f'archivo-{index:05d}.txt',b'')
    (folder/'bad.zip').write_text('not a ZIP file')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1'}
    with log.open('w') as output:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','archive','--exit-after','45'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
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
        return wait(lambda:next((row for row in log.read_text()[after:].splitlines() if row.startswith('FILES_PREVIEW_READY ') and '/'+name+'"' in row and 'encoding=ZIP · directorio central ' in row and 'truncated='+str(truncated).lower() in row),False),'fresh ZIP Ready absent: '+action)
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
        ready('archive','catalog.zip');command('archive-assert');capture('zip-catalog-safety-labels')
        ready('archive-raw','raw.zip');command('archive-assert-raw');capture('zip-raw-name')
        ready('archive-empty','empty.zip');capture('zip-empty-catalog')
        ready('archive-large','large.zip',True);command('archive-assert-truncated');capture('zip-catalog-truncated')
        ready('archive-rapid','catalog.zip');command('archive-assert')
        for action,name in [('archive-bad','bad.zip'),('archive-over','over.zip')]:
            _,after=command(action)
            wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/'+name+'"' in row for row in log.read_text()[after:].splitlines()),'fresh ZIP error absent: '+name)
        command('close-archive-pending')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'normal drained exit absent'
        assert not (test/'outside.txt').exists(),'listing extracted an entry'
        normal=True
        report.update(functional_gate='PASS',normal_exit_after_preview_drain=True,cleanup_scope='root exit0 plus common worker drain ACK; no descendant sampler in capture phase',log=log.read_text())
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error),log=log.read_text())
        path=evidence/('zip-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n');print(str(path));raise
    finally:
        if not normal:stop_tree(process,None)
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    path=evidence/('zip-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
