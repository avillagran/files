#!/usr/bin/env python3
"""Functional HTML UI capture only. Resource/startup phase is explicitly suspended.
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
parser=argparse.ArgumentParser();parser.add_argument("--installed",action="store_true");parser.add_argument('--source-lifecycle',action='store_true');parser.add_argument('--drop-events',action='store_true');args=parser.parse_args()
BINARY=Path.home()/'.local/bin/files' if args.installed else ROOT/'target/release/files'
report={'source_lifecycle':args.source_lifecycle,'notify_callbacks_suppressed':args.drop_events,'checks':[],'method':'internal handlers, native litehtml document layout and actual FLTK framebuffer; no native OS input or clipboard',
        'resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','reason':'User phase: integration/captures only; no resource/startup measurements until released',
        'captures':[],'known_pending':['real font metrics/shaping','local HTML resources and links','safe DOM allocator/depth budget','cross-platform adapter','resource budget','selection preservation across reflow']}
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
if len(matches)!=1:raise RuntimeError('No unique compositor for fixture Wayland socket')
os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance'];report['compositor_binding']={'selected':matches[0],'scope':'fixture-only, no focus/input/global configuration'}
with tempfile.TemporaryDirectory(prefix='html-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    for name in ('layout.html','blocked.html'):
        shutil.copyfile(ROOT/'scripts/fixtures/html-engine'/name,folder/name)
    # Observable author CSS media-query change within the app's minimum width.
    with (folder/'layout.html').open('a') as out:
        out.write('<style>@media(max-width:800px){h1{font-size:20px}.card{padding:6px}}</style>')
    (folder/'long.html').write_text('<style>body{color:#ddeeee;background:#102018}p{margin:14px}</style><h1>Documento largo</h1>'+''.join('<p>Sección '+str(i)+' con texto visible de la vista HTML.</p>' for i in range(45))+'<p>FINAL_HTML_VISIBLE</p>')
    (folder/'large.html').write_text('a'*32769)
    (folder/'source.html').write_text('<h1>FD_ORIGINAL_A</h1>')
    (folder/'symlink.html').symlink_to('source.html')
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1'}
    if args.drop_events:env['FILES_TEST_DROP_MEDIA_EVENTS']='1'
    with log.open('w') as output:
        process=subprocess.Popen([str(BINARY),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','html','--exit-after','45'],env=env,stderr=output,stdout=subprocess.DEVNULL,start_new_session=True)
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
    def proof(row,name):
        match=re.search(r'source=Revision \{ (.*?) \}',row)
        if not match:return False
        actual={k:int(v) for k,v in (part.split(': ') for part in match[1].split(', '))}
        try:st=(folder/name).stat()
        except FileNotFoundError:return False
        expected=dict(device=st.st_dev,inode=st.st_ino,bytes=st.st_size,modified_seconds=st.st_mtime_ns//10**9,modified_nanoseconds=st.st_mtime_ns%10**9,changed_seconds=st.st_ctime_ns//10**9,changed_nanoseconds=st.st_ctime_ns%10**9)
        return actual==expected
    def unavailable(after):
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/source.html"' in row for row in log.read_text()[after:].splitlines()),'source HTML invalidation absent')
        command('html-source-empty')
    def gone(pid,start):
        try:return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])!=start
        except FileNotFoundError:return True
    def ready(name,action=None,width=1000,height=650,after=0,revision=None):
        if action:revision,after=command(action)
        def check():
            rows=[line for line in log.read_text()[after:].splitlines() if line.startswith(('FILES_PREVIEW_HTML_READY','FILES_PREVIEW_ERROR'))]
            if not rows:return False
            row=rows[-1]
            return row if row.startswith('FILES_PREVIEW_HTML_READY') and '/'+name+'"' in row and f'width={min(width-32,1160)-36} height={min(height-44,800)-146} ' in row and (revision is None or f'revision={revision} ' in row) and proof(row,name) else False
        return wait(check,'fresh HTML Ready absent: '+name)
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
        ready('layout.html','html');capture('html-rendered-css')
        command('html-select')
        assert 'FILES_HTML_SELECTION_OK' in log.read_text()
        capture('html-visible-selection')
        after=len(log.read_text())
        wait(lambda:geometry(process.pid,800,500) and observed_geometry(process.pid,800,500),'resize geometry unvalidated')
        ready('layout.html',width=800,height=500,after=after)
        capture('html-css-resize')
        after=len(log.read_text())
        wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'restore geometry unvalidated')
        ready('layout.html',after=after)
        ready('blocked.html','html-blocked');command('html-assert-blocked')
        capture('html-blocked-resources')
        ready('long.html','html-long');command('html-scroll');command('html-assert-end')
        capture('html-scroll-final')
        _,after=command('html-large')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/large.html"' in row for row in log.read_text()[after:].splitlines()),'fresh large HTML error absent')
        ready('layout.html','html-rapid')
        _,after=command('html-symlink')
        wait(lambda:any(row.startswith('FILES_PREVIEW_ERROR ') and '/symlink.html"' in row for row in log.read_text()[after:].splitlines()),'symlink HTML error absent')
        if args.source_lifecycle:
            ready('source.html','html-source');command('html-source-a');capture('html-source-original')
            source=folder/'source.html';a=source.read_bytes();b=b'<h1>PATH_REPLACEMENT_B</h1>';a=a+b' '*(len(b)-len(a));before=len(log.read_text());source.write_bytes(a)
            ready('source.html',after=before)
            old=source.stat();replacement=test/'replacement.html';replacement.write_bytes(b);assert old.st_size==replacement.stat().st_size
            os.utime(replacement,ns=(old.st_atime_ns,old.st_mtime_ns));before=len(log.read_text());os.replace(replacement,source)
            ready('source.html',after=before);command('html-source-b');capture('html-source-replaced')
            old=source.stat();before=len(log.read_text());source.write_bytes(a);os.utime(source,ns=(old.st_atime_ns,old.st_mtime_ns))
            ready('source.html',after=before);command('html-source-a');capture('html-source-modified')
            before=len(log.read_text());source.unlink();unavailable(before);capture('html-source-deleted')
            before=len(log.read_text());source.write_bytes(a);ready('source.html',after=before);command('html-source-a')
            before=len(log.read_text());os.rename(source,test/'away.html');unavailable(before)
            before=len(log.read_text());os.rename(test/'away.html',source);ready('source.html',after=before)
            before=len(log.read_text());os.rename(folder,test/'old-parent');unavailable(before)
            folder.mkdir();before=len(log.read_text());source.write_bytes(a);ready('source.html',after=before);command('html-source-a');capture('html-parent-recovered')
            shutil.copyfile(test/'old-parent'/'layout.html',folder/'layout.html');shutil.copyfile(test/'old-parent'/'long.html',folder/'long.html')
            report['checks'].append('actual inherited FD proof/text after same-size+mtime replacement, inplace restore mtime, delete/recreate, file rename and parent replacement; stale display list removed')
            command('media-hide');count=log.read_text().count('FILES_PREVIEW_HELPER ');time.sleep(1.2)
            assert log.read_text().count('FILES_PREVIEW_HELPER ')==count,'hidden HTML restarted helper'
            report['checks'].append('hidden HTML starts no helper during reconcile interval')
        ready('layout.html','html')
        command('close-html-pending')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'normal drained exit absent'
        records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=',log.read_text());assert records and all(start.isdigit() for _,start in records)
        identities=sorted(set((int(pid),int(start)) for pid,start in records));assert all(gone(pid,start) for pid,start in identities),'logged helper survived exit (zombies not accepted)'
        report['logged_helper_cleanup']={'complete':True,'identities':identities,'zombies_accepted':False,'scope':'logged PID/start after root exit0, not lifetime or resource sampling'}
        normal=True
        report.update(functional_gate='PASS',normal_exit_after_preview_drain=True,cleanup_scope='root exit0 plus common worker drain ACK; no descendant sampler in capture phase',log=log.read_text())
    except BaseException as error:
        report.update(functional_gate='FAIL',error=repr(error),log=log.read_text())
        path=evidence/('html-ui-failure-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n');print(str(path));raise
    finally:
        if not normal:stop_tree(process,None)
    report['binary_path']=str(BINARY)
    report['helper_sha256']=hashlib.sha256((BINARY.parent/'files-helper').read_bytes()).hexdigest()
    report['binary_sha256']=hashlib.sha256(BINARY.read_bytes()).hexdigest()
    report['html_helper_sha256']=hashlib.sha256((BINARY.parent/'files-html-helper').read_bytes()).hexdigest()
    path=evidence/('html-ui-'+str(time.time_ns())+'.json');path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
