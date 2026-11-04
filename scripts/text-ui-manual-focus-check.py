#!/usr/bin/env python3
"""Space text UI with real helper; capture/clipboard and resource runs are separate."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from memcheck import Metrics, geometry, observed_geometry, stop_tree
from process_metrics import TreeSampler, read_stat

ROOT=Path(__file__).resolve().parent.parent
parser=argparse.ArgumentParser()
parser.add_argument('--capture',action='store_true')
parser.add_argument('--clipboard-trace',action='store_true')
parser.add_argument('--manual-focus',action='store_true',required=True)
args=parser.parse_args()
evidence=ROOT/'docs/evidence/0.0.4';evidence.mkdir(parents=True,exist_ok=True)
base=Path.home()/'.cache/files-test';base.mkdir(parents=True,exist_ok=True)
run_id=str(time.time_ns())
report={'focus_authorization':'focus by user; fixture selection/copy through internal handlers; no synthetic OS keys or clicks', 'method':'Space UI handlers/pointer mapping and FLTK framebuffer; no native OS input','captures':[],'clipboard':'not read in resource mode','resource_gate':'NOT_MEASURED' if args.capture else 'UNVALIDATED'}
with tempfile.TemporaryDirectory(prefix='preview-ui-',dir=base) as tmp:
    test=Path(tmp);folder=test/'files';folder.mkdir()
    source='área\t漢字\r\n@x😀 código\r\nfinal\r\n'
    (folder/'A.txt').write_bytes(source.encode())
    (folder/'B.txt').write_bytes(b'\xff\xfe'+'texto B UTF16\r\n'.encode('utf-16le'))
    (folder/'C.txt').write_text('latest C\n')
    (folder/'binary.bin').write_bytes(b'\x00'*128)
    (folder/'long.txt').write_text('línea larga de ejemplo\n'*2500)
    raw=test/'capture.ppm';log=test/'app.log'
    env={**os.environ,'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_CAPTURE':str(raw),'FILES_TEST_PREVIEW':'1','FILES_WAIT_TRACE':'1'}
    if args.clipboard_trace:
        assert args.capture,'protocol trace is functional-only, never a resource run'
        env['WAYLAND_DEBUG']='client'
    with log.open('w') as out:
        process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','text','--exit-after','120'],env=env,stderr=out,stdout=subprocess.DEVNULL,start_new_session=True)
    sampler=None if args.capture else TreeSampler(process.pid)
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
        before=log.read_text().count('FILES_PREVIEW_COMMAND_OK '+action+'\n')
        pending=raw.with_suffix('.pending');pending.write_text(action);os.replace(pending,raw.with_suffix('.command'))
        wait(lambda:log.read_text().count('FILES_PREVIEW_COMMAND_OK '+action+'\n')>before,'command failed: '+action,action.startswith('close'))
    def ready(name,truncated=False):
        # Match the latest emitted state, never an old Ready for a different file.
        def check():
            markers=[line for line in log.read_text().splitlines() if line.startswith(('FILES_PREVIEW_READY','FILES_PREVIEW_ERROR'))]
            return markers and markers[-1].startswith('FILES_PREVIEW_READY') and '/'+name+'"' in markers[-1] and ('truncated='+str(truncated).lower()) in markers[-1]
        wait(check,'ready absent for '+name)
    def capture(name):
        raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
        def complete():
            if not raw.exists():return False
            parts=raw.read_bytes().split(b'\n',3)
            return len(parts)==4 and len(parts[3])==int(parts[1].split()[0])*int(parts[1].split()[1])*3
        wait(complete,'capture missing')
        path=evidence/(name+'-manual-focused-'+run_id+'.png');subprocess.run(['magick',str(raw),str(path)],check=True);report['captures'].append(str(path))
    def cleanup():
        survivors=[];errors=[]
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
        command('open');ready('A.txt')
        # Wait for first text draw to establish pointer geometry; command ACK alone
        # does not prove the Ready document has been painted.
        time.sleep(.05)
        if args.capture:
            clients=json.loads(subprocess.check_output(['hyprctl','clients','-j'],timeout=2))
            matches=[c for c in clients if c.get('pid')==process.pid and c.get('address')]
            assert len(matches)==1, 'fixture PID must identify exactly one compositor window'
            address=matches[0]['address']
            assert re.fullmatch(r'0x[0-9a-fA-F]+',address), 'invalid compositor address'
            def focused():
                active=json.loads(subprocess.check_output(['hyprctl','activewindow','-j'],timeout=2))
                return active if active.get('pid')==process.pid and active.get('address')==address else None
            print('FIXTURE_WAITING_FOR_MANUAL_FOCUS pid='+str(process.pid), flush=True)
            active=wait(focused,'fixture not manually focused',timeout=90)
            report['focus_before_copy']={'target_pid':process.pid,'target_address':address,'active_pid':active.get('pid'),'active_address':active.get('address'),'target_is_active':True,'method':'user/compositor focus; no focus IPC issued'}
        command('select')
        selection=re.findall(r'FILES_PREVIEW_SELECTION=(.*)',log.read_text())
        assert selection and json.loads(selection[-1])=='rea\t漢字\r\n@x'
        if args.capture:
            capture('text-selection')
            # Copy ownership/offer becomes visible asynchronously after the app's
            # next Wayland flush. Poll only AFTER our own copy, without logging data.
            transfers=[]
            expected='rea\t漢字\r\n@x'.encode()
            deadline=time.monotonic()+2
            matched=False
            while time.monotonic()<deadline:
                transfer=subprocess.run(['wl-paste','--no-newline'],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=3)
                matched=transfer.returncode==0 and transfer.stdout==expected
                transfers.append({'returncode':transfer.returncode,'exact_match':matched,'stderr':transfer.stderr.decode(errors='replace')[:512]})
                if matched:break
                if process.poll() is not None:break
                time.sleep(.025)
            report['clipboard_transfer']={'attempts':transfers,'app_alive_after_transfer':process.poll() is None,'async_offer_deadline_seconds':2}
            report['clipboard']='PASS exact Wayland clipboard bytes Unicode+TAB+CRLF after own CopyPreview; clipboard not read before copy' if matched else 'FAIL required clipboard transfer: no exact match within deadline (content withheld)'
            report['clipboard_gate']='PASS' if matched else 'FAIL'
        command('rapid');ready('C.txt')
        command('binary')
        wait(lambda:'FILES_PREVIEW_ERROR' in log.read_text(),'binary error absent')
        if args.capture:capture('text-binary-error')
        command('truncated');ready('long.txt',True)
        if args.capture:capture('text-truncated')
        stable_since=time.monotonic()
        until=stable_since+1
        while time.monotonic()<until:sample();time.sleep(.005 if sampler else .025)
        stable_until=time.monotonic()
        command('close-pending')
        wait(lambda:process.poll() is not None,'close did not finish',True)
        assert process.returncode==0 and 'FILES_PREVIEW_DRAINED' in log.read_text(),'exit without drain confirmation'
        normal=True;report['cleanup']=cleanup()
        report['functional_checks_passed']=True
        report['functional_gate']='PASS' if args.capture and matched else 'FAIL'
        report['required_clipboard_gate']=report.get('clipboard_gate','PENDING real Wayland focus/clipboard; resource scenario is partial')
        report['normal_exit_after_preview_drain']=True
        report['selection_bytes_verified']='Unicode+TAB+CRLF source preserved; pointer handlers with measured shaped width'
        report['rapid_navigation']='A started then B→C latest wins; C Ready observed; controller tests cover stale same-path/errors'
        report['log']=log.read_text()
        if sampler:
            report['metrics']=metrics.report(stable_since,stable_until)
            report['metrics']['sample_interval_requested_ms']=5.0
            report['stable_since_monotonic']=stable_since;report['stable_until_monotonic']=stable_until
            report['stable_phase']='truncated preview displayed, no ongoing requests; bounded one-second window'
            report['terminal_exit_observations']=terminal
            report['helper_identities_verified']=sorted(helpers)
            report['sampled_peak_memory_mb_decimal']=report['metrics']['sampled_maxima']['memory_kib']*1024/1e6
            report['resource_gate']=('PASS' if report['sampled_peak_memory_mb_decimal']<=150 else 'FAIL') if report['metrics']['complete'] and report['cleanup']['complete'] and helpers else 'UNVALIDATED'
            report['goal_met']=report['sampled_peak_memory_mb_decimal']<=50 if report['resource_gate']!='UNVALIDATED' else None
            report['resource_scope']='sampled main+observed helpers PSS+Swap; not exact peaks/full lifetimes; GPU excluded. Five-file fixture, not baseline10k or large-operation coverage.'
    except BaseException as e:
        text=log.read_text()
        if 'clipboard_gate' not in report:
            report['clipboard_gate']='NOT_ATTEMPTED'
        report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
        report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
        report['clipboard_protocol_events']=[line for line in text.splitlines() if re.search(r'wl_keyboard.*enter\(|wl_data_device.*set_selection\(|wl_data_source.*cancelled\(',line)]
        if args.clipboard_trace:text='\n'.join(line for line in text.splitlines() if line.startswith(('FILES_','Error real')))
        report.update(functional_gate='FAIL',error=repr(e),log=text)
        (evidence/f'text-manual-focus-failure-{run_id}.json').write_text(json.dumps(report,indent=2)+'\n')
        raise
    finally:
        if not normal:stop_tree(process,sampler)
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    report['helper_sha256']=hashlib.sha256((ROOT/'target/release/files-helper').read_bytes()).hexdigest()
    path=evidence/('text-manual-focus-ui-'+run_id+'.json')
    path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':report['resource_gate']}))
    if report['functional_gate']!='PASS' or not args.capture and report['resource_gate']!='PASS':raise SystemExit(1)
