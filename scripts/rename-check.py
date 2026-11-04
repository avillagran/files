#!/usr/bin/env python3
"""Real UI handlers/framebuffer + disk fixtures. Capture and metrics are separate."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import time
from memcheck import Metrics, geometry, observed_geometry, stop_tree
from process_metrics import TreeSampler, read_stat

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--capture', action='store_true')
args = parser.parse_args()
base = Path.home()/'.cache/files-test'
base.mkdir(parents=True, exist_ok=True)
evidence = ROOT/'docs/evidence/0.0.3'
evidence.mkdir(parents=True, exist_ok=True)
report = {'method':'in-process UI handlers and FLTK framebuffer; no native OS input', 'captures':[], 'runs':[], 'resource_gate':'NOT_MEASURED' if args.capture else 'UNVALIDATED'}
with tempfile.TemporaryDirectory(prefix='rename-ui-', dir=base) as tmp:
    test=Path(tmp)
    for closing in [False, True]:
        folder=test/('close' if closing else 'cycle');folder.mkdir()
        original={'photo1.txt':b'first payload\x00\xff', 'photo2.txt':b'second payload\r\n'}
        for name,data in original.items():(folder/name).write_bytes(data)
        (folder/'untouched.txt').write_bytes(b'untouched')
        raw=test/'capture.ppm';log=test/('close.log' if closing else 'cycle.log')
        env={**os.environ,'FILES_TEST_CAPTURE':str(raw),'FILES_CONFIG_DIR':str(test/'config'),'FILES_TEST_MUTATIONS':'1','FILES_METRICS_READY':'1'}
        with log.open('w') as out:
            process=subprocess.Popen([str(ROOT/'target/release/files'),str(folder),'--size','1000x650','--scale','1','--ui-smoke','--ui-scene','rename','--exit-after','30'],stderr=out,stdout=subprocess.DEVNULL,env=env,start_new_session=True)
        sampler=None if args.capture else TreeSampler(process.pid)
        metrics=Metrics();ready=None;stable_since=None;stable_until=None;normal_exit=False;exit_observations=[]
        def sample():
            if sampler and process.poll() is None:
                value=sampler.sample()
                if not value['root_alive'] and not value['errors'] and not value['processes']:
                    exit_observations.append({'reason':'terminal root-dead empty observation deferred; valid only after confirmed exit0 and verified cleanup','sample':value})
                else:metrics.add(value)
        def wait(check, reason, timeout=8, allow_exit=False):
            deadline=time.monotonic()+timeout
            while time.monotonic()<deadline:
                sample()
                value=check()
                if value:return value
                if process.poll() is not None and not allow_exit:raise AssertionError(reason+'; app exited: '+log.read_text())
                time.sleep(.025)
            raise AssertionError(reason+'\n'+log.read_text())
        def command(action):
            before=log.read_text().count('FILES_MUTATION_COMMAND_OK '+action+'\n')
            commandfile=raw.with_suffix('.command');pending=commandfile.with_suffix('.pending');pending.write_text(action);os.replace(pending,commandfile)
            wait(lambda:log.read_text().count('FILES_MUTATION_COMMAND_OK '+action+'\n')>before,'command not accepted: '+action,allow_exit=action=='close')
        def finish(action):
            count=log.read_text().count('FILES_MUTATION_FINISHED')
            command(action)
            wait(lambda:log.read_text().count('FILES_MUTATION_FINISHED')>count,'mutation ACK absent')
        def capture(name,expected_count=None):
            assert sampler is None
            if expected_count is not None:
                wait(lambda:re.search(r'FILES_SCENE_READY view=groups loaded=true entries='+str(expected_count)+r' stats_ready=true',log.read_text().rsplit('FILES_MUTATION_FINISHED',1)[-1]),'result listing not reloaded')
            raw.unlink(missing_ok=True);raw.with_suffix('.request').touch()
            wait(lambda:raw.exists() and raw.stat().st_size>1000,'capture missing')
            # Header + complete expected byte count, avoiding a partially written PPM.
            def complete():
                data=raw.read_bytes();parts=data.split(b'\n',3)
                if len(parts)!=4:return False
                width,height=map(int,parts[1].split());return len(parts[3])==width*height*3
            wait(complete,'incomplete framebuffer')
            path=evidence/(name+'.png');subprocess.run(['magick',str(raw),str(path)],check=True)
            report['captures'].append(str(path))
        def cleanup_check():
            # Read-only verification: no signal can rescue a failed normal close.
            survivors=[];errors=[]
            for pid,start in sorted(sampler.known if sampler else []):
                try:
                    stat=read_stat(Path(f'/proc/{pid}/stat').read_text())
                    if stat['start']==start and stat['state']!='Z':survivors.append([pid,start])
                except FileNotFoundError:
                    if Path(f'/proc/{pid}').exists():errors.append(f'{pid}/{start}: stat missing but process directory exists')
                except (OSError,ValueError,IndexError) as exc:errors.append(f'{pid}/{start}: {exc}')
            return {'complete':not survivors and not errors,'surviving_observed_processes':survivors,'errors':errors,'method':'read-only PID/starttime verification after exit0; no termination signals' if sampler else 'root exit0 only; no sampler/descendant verification','scope':'observed tree identities' if sampler else 'root process only'}
        def assert_names(renamed):
            names=['image-001.txt','image-002.txt'] if renamed else list(original)
            for name,data in zip(names,original.values()):assert (folder/name).read_bytes()==data
            assert not any((folder/name).exists() for name in (list(original) if renamed else ['image-001.txt','image-002.txt']))
            assert (folder/'untouched.txt').read_bytes()==b'untouched'
            assert not any(p.name.startswith('.files-rename-') for p in folder.iterdir())
        try:
            wait(lambda:'FILES_UI_SMOKE_OK' in log.read_text(),'initial UI not ready')
            wait(lambda:geometry(process.pid,1000,650) and observed_geometry(process.pid,1000,650),'native window geometry absent')
            geometry_report=observed_geometry(process.pid,1000,650)
            command('rename-preview')
            wait(lambda:'FILES_RENAME_PREVIEW_READY count=2' in log.read_text(),'preview absent')
            ready=time.monotonic()
            assert_names(False) # Preview has not modified disk.
            if closing:
                stable_since=time.monotonic()
                until=stable_since+1
                while time.monotonic()<until:sample();time.sleep(.025)
                stable_until=time.monotonic()
                ack_before=log.read_text().count('FILES_MUTATION_FINISHED')
                command('apply-close')
                request_ids=wait(lambda:re.findall(r'FILES_MUTATION_REQUEST id=(\d+) command=apply-close',log.read_text()),'Apply request ID missing',allow_exit=True)
                assert len(request_ids)==1,'missing/ambiguous Apply request ID'
                expected_ack='FILES_MUTATION_FINISHED id='+request_ids[0]+' '

                wait(lambda:process.poll() is not None or 'FILES_MUTATION_FINISHED' in log.read_text(),'close did not drain',allow_exit=True)
                assert log.read_text().count('FILES_MUTATION_FINISHED')==ack_before+1 and expected_ack in log.read_text(),'exit before exact Apply ACK'
                if process.poll() is None:
                    # Cancellation is intentionally visible; dismiss it, then close normally.
                    command('continue');command('close')
            else:
                if args.capture:capture('rename-preview-compact')
                finish('apply');assert_names(True)
                if args.capture:capture('rename-result-compact',3)
                command('continue')
                finish('undo');assert_names(False)
                command('continue')
                finish('redo');assert_names(True)
                command('continue')
                command('create-file');finish('apply');assert (folder/'created.txt').read_bytes()==b''
                command('continue')
                command('create-folder');finish('apply');assert (folder/'Created').is_dir()
                if args.capture:capture('create-result-compact',5)
                command('continue')
                # Isolated stable interval, with no capture or compilation in resource mode.
                stable_since=time.monotonic()
                until=stable_since+1
                while time.monotonic()<until:sample();time.sleep(.025)
                stable_until=time.monotonic()
                command('close')
            wait(lambda:process.poll() is not None,'normal close timed out',allow_exit=True)
            assert process.returncode==0,log.read_text()
            if closing:assert expected_ack in log.read_text(),'normal exit missing ACK'
            cleanup=cleanup_check()
            normal_exit=True
            if closing:
                # Cancellation may race completion; either whole result is valid, never partial.
                assert_names((folder/'image-001.txt').exists())
            assert not any(p.name.startswith('.files-rename-') for p in folder.iterdir())
            run={'scenario':'close with pending Apply' if closing else 'preview/apply/undo/redo/create file+folder','exit_code':process.returncode,'normal_exit_after_ack':True,'filesystem_contents_and_no_staging':'PASS','log':log.read_text(),'geometry':geometry_report,'cleanup':cleanup,'preview_ready_monotonic':ready,'stable_since_monotonic':stable_since,'stable_until_monotonic':stable_until,'stable_phase':'preview before pending-Apply close' if closing else 'after final creation and dismissal'}
            if sampler:
                run['metrics']=metrics.report(stable_since,stable_until)
                run['terminal_exit_observations']=exit_observations
            report['runs'].append(run)
        except BaseException as exc:
            failure={**report,'functional_gate':'FAIL','error':repr(exc),'failed_run_log':log.read_text(),'normal_exit':normal_exit}
            if sampler:failure['failed_run_metrics']=metrics.report(stable_since or float('inf'),stable_until)
            (evidence/f'rename-failure-{time.time_ns()}.json').write_text(json.dumps(failure,indent=2)+'\n')
            raise
        finally:
            # Emergency cleanup cannot make a failed lifecycle check pass.
            if not normal_exit:stop_tree(process,sampler)
    # Hash after all app cleanup: does not warm the executable before startup.
    report['binary_sha256']=hashlib.sha256((ROOT/'target/release/files').read_bytes()).hexdigest()
    if not args.capture:
        complete=all(r['metrics']['complete'] and r['cleanup']['complete'] and r['normal_exit_after_ack'] for r in report['runs'])
        peak=max(r['metrics']['sampled_maxima']['memory_kib'] for r in report['runs'])
        report['sampled_peak_memory_mb_decimal']=peak*1024/1e6
        report['sampled_goal_met']=peak*1024<=50_000_000
        report['goal_met']=report['sampled_goal_met'] if complete else None
        report['resource_gate']=('PASS' if peak*1024<=150_000_000 else 'FAIL') if complete else 'UNVALIDATED'
        report['resource_scope']='sampled tree PSS+Swap; lower bound, not full lifetimes, GPU excluded; scenario is 3 files + created entries, not baseline10k/1080p'
    report['functional_gate']='PASS'
    path=evidence/('rename-ui.json' if args.capture else 'rename-resources-normal-exit.json')
    path.write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({'report':str(path),'functional_gate':report['functional_gate'],'resource_gate':report['resource_gate']}))
    if report['resource_gate'] in ('FAIL','UNVALIDATED') and not args.capture:raise SystemExit(1)
