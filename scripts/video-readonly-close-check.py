#!/usr/bin/env python3
"""Simulated readonly hold: normal UI callback exit permission is NOT worker ACK.
No kernel-D/FUSE injection, native input, PSS, CPU or startup measurements.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from memcheck import geometry,observed_geometry,stop_tree

ROOT=Path(__file__).resolve().parent.parent
BASE=Path.home()/'.cache/files-test';BASE.mkdir(parents=True,exist_ok=True)
EVIDENCE=ROOT/'docs/evidence/0.0.4'
report={'functional_gate':'FAIL','resource_gate':'NOT_MEASURED','startup_gate':'NOT_MEASURED','method':'shared UI callback and actual FLTK framebuffer, fixture-only30s readonly hold; NOT real kernelD/native OS input','cases':[]}
instances=json.loads(subprocess.check_output(['hyprctl','instances','-j'],timeout=2));matches=[i for i in instances if i.get('wl_socket')==os.environ.get('WAYLAND_DISPLAY')]
assert len(matches)==1
os.environ['HYPRLAND_INSTANCE_SIGNATURE']=matches[0]['instance']
def gone(pid,start):
    try:return int(Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[19])!=start
    except FileNotFoundError:return True
try:
    with tempfile.TemporaryDirectory(prefix='video-readonly-',dir=BASE) as tmp:
        test=Path(tmp);folder=test/'files';folder.mkdir()
        subprocess.run(['ffmpeg','-v','error','-nostdin','-f','lavfi','-i','color=c=red:s=320x180:r=5:d=4','-c:v','gif','-loop','0','-threads','1',str(folder/'clip.gif')],check=True,timeout=10)
        for role in ('video_snapshot','video_stream'):
            raw=test/(role+'.ppm');log=test/(role+'.log');case={'role':role,'simulated_readonly_hold':True,'worker_drained':False};report['cases'].append(case)
            env={**os.environ,'FILES_CONFIG_DIR':str(test/(role+'-config')),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_TEST_FIXED_SIZE':'1','FILES_TEST_ISOLATE_INPUT':'1','FILES_CLOSE_TRACE':'1','FILES_TEST_READONLY_HOLD_ROLE':role}
            with log.open('w') as output:process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','video','--exit-after','20'],env=env,stdout=subprocess.DEVNULL,stderr=output,start_new_session=True)
            normal=False
            def wait(check,reason,allow_exit=False):
                deadline=time.monotonic()+8
                while time.monotonic()<deadline:
                    value=check()
                    if value:return value
                    if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; process exited')
                    time.sleep(.025)
                raise AssertionError(reason)
            def command(action):
                after=len(log.read_text());pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
                row=wait(lambda:re.search(r'FILES_PREVIEW_COMMAND_REV action='+re.escape(action)+r' revision=(\d+)',log.read_text()[after:]),'command missing '+action,action=='callback')
                return int(row[1]),after
            try:
                wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'UI smoke missing')
                wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'geometry absent');case['geometry']=observed_geometry(process.pid,1000,650)
                revision,after=command('gif')
                if role=='video_stream':
                    wait(lambda:any(r.startswith('FILES_PREVIEW_VIDEO_READY ') and f'revision={revision} ' in r for r in log.read_text()[after:].splitlines()),'snapshot not ready')
                    command('video-play')
                held='FILES_READONLY_HELD role='+role+' simulated=true'
                wait(lambda:held in log.read_text(),'readonly hold not reached')
                raw.with_suffix('.request').touch()
                def complete():
                    if not raw.exists():return False
                    parts=raw.read_bytes().split(b'\n',3)
                    return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
                wait(complete,'framebuffer incomplete')
                capture=EVIDENCE/('readonly-'+role+'-'+str(time.time_ns())+'.png');subprocess.run(['magick',str(raw),str(capture)],check=True)
                case['capture']={'path':str(capture),'framebuffer_dimensions':[int(n) for n in raw.read_bytes().split(b'\n',3)[1].split()]}
                command('callback');wait(lambda:process.poll() is not None,'normal callback exit blocked by readonly worker',True)
                text=log.read_text();assert process.returncode==0,'root did not exit0'
                assert 'FILES_PREVIEW_EXIT_ALLOWED drained=false readonly_pending=true no_future_helper=true' in text,'honest readonly permission marker absent'
                assert 'FILES_PREVIEW_DRAINED' not in text,'unacknowledged worker falsely called drained'
                assert 'FILES_VIDEO_CHILD_ADMIT role='+role not in text.split(held,1)[1],'child admission after held readonly close'
                records=re.findall(r'FILES_PREVIEW_HELPER pid=(\d+) start=([^ ]+) command=',text);assert all(start.isdigit() for _,start in records),'unknown helper identity'
                identities=sorted(set((int(pid),int(start)) for pid,start in records));assert all(gone(*i) for i in identities),'observed helper survived exit'
                case.update(functional_gate='PASS',normal_callback_exit0=True,exit_permission=True,worker_drained=False,no_late_child_admission=True,logged_helper_cleanup={'identities':identities,'complete':True,'zombies_accepted':False,'scope':'only logged identities, not lifetime coverage'},log=text)
                normal=True
            except BaseException as error:
                case.update(functional_gate='FAIL',error=repr(error),log=log.read_text());raise
            finally:
                if not normal:stop_tree(process,None)
        report['functional_gate']='PASS'
except BaseException as error:
    report['error']=repr(error);raise
finally:
    for name,key in [('files','binary_sha256'),('files-helper','helper_sha256'),('files-html-helper','html_helper_sha256')]:report[key]=hashlib.sha256((ROOT/'target/release'/name).read_bytes()).hexdigest()
    path=EVIDENCE/('video-readonly-close-'+str(time.time_ns())+'.json')
    with path.open('x') as output:json.dump(report,output,indent=2)
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':'NOT_MEASURED'}))
