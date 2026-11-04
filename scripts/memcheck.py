#!/usr/bin/env python3
"""Serialized Linux process-tree memory gate; PSS + SwapPss, never RSS.

Sampled maxima are lower bounds: children/pulses between samples can escape.
Run without builds, screen captures or other benchmarks; the caller owns isolation.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import re
import signal
import statistics
import subprocess
import tempfile
import time

from process_metrics import TreeSampler, read_stat

ROOT = Path(__file__).resolve().parent.parent
SAMPLE_INTERVAL = .025


def hypr_json(command):
    return json.loads(subprocess.check_output(['hyprctl', command, '-j'], timeout=.5))


def geometry(pid, width, height):
    """Request only this test window's size; callers must verify observed_geometry.

    Address return preserved for ui-check.py. Absence is never evidence of success.
    """
    if not os.environ.get('HYPRLAND_INSTANCE_SIGNATURE'):
        return None
    client = next((c for c in hypr_json('clients') if c['pid'] == pid), None)
    if client is None:
        return None
    address = 'address:' + client['address']
    if not client['floating']:
        subprocess.run(['hyprctl', 'dispatch', f'hl.dsp.window.float({{ window = {json.dumps(address)}, action = "set" }})'], check=True, stdout=subprocess.DEVNULL, timeout=.5)
    if not client['floating'] or client.get('size') != [width,height]:
        subprocess.run(['hyprctl', 'dispatch', f'hl.dsp.window.resize({{ window = {json.dumps(address)}, x = {width}, y = {height} }})'], check=True, stdout=subprocess.DEVNULL, timeout=.5)
    return client['address']


def observed_geometry(pid, width, height):
    if not os.environ.get('HYPRLAND_INSTANCE_SIGNATURE'):
        return None
    client = next((c for c in hypr_json('clients') if c['pid'] == pid), None)
    if not client or client.get('size') != [width, height] or not client.get('floating') or client.get('xwayland', True):
        return None
    monitor = next((m for m in hypr_json('monitors') if m['id'] == client['monitor']), None)
    if not monitor or 'scale' not in monitor:
        return None
    return {'window_logical': client['size'], 'output_scale': monitor['scale'],
            'window_physical': [round(n * monitor['scale']) for n in client['size']],
            'native_wayland': True, 'address': client['address']}


def scene_readiness(log, view, count):
    matches = re.findall(r'FILES_SCENE_READY view=(\w+) loaded=(true|false) entries=(\d+) stats_ready=(true|false) layout_ready=(true|false) generation=(\d+)', log)
    for actual, loaded, entries, stats, layout, generation in matches[-1:]:
        if actual == view and loaded == 'true' and int(entries) == count and (view != 'groups' or stats == 'true') and (view != 'spatial' or layout == 'true'):
            return {'view': actual, 'loaded': True, 'entry_count': int(entries), 'stats_ready': stats == 'true', 'layout_ready': layout == 'true', 'generation': int(generation)}
    return None


def summarize_exits(events):
    """Derived groups; retain raw observations and do not count repeats as children."""
    groups = {}
    for event in events:
        key = (event.get('pid'), event.get('start'), event.get('phase'), event.get('reason'))
        if key not in groups:
            groups[key] = {k: v for k, v in event.items() if k != 'sample'}
            groups[key].update(observation_count=0, first_sample=event.get('sample'), last_sample=event.get('sample'))
        groups[key]['observation_count'] += 1
        groups[key]['last_sample'] = event.get('sample')
    return list(groups.values())


class Metrics:
    """Aggregate all samples, retaining errors rather than silently dropping races."""
    def __init__(self):
        self.samples = []
        self.seen = set()
        self.exited = set()
        self.last_live = set()
        self.exited_during_read = []
        self.discovery_events = []

    def add(self, sample):
        live = {(p['pid'], p['start']) for p in sample['processes']}
        self.exited.update(self.last_live - live)
        self.last_live = live
        self.seen.update(live)
        for event in sample.get('exited_during_read', []):
            identity = (event['pid'], event['start'])
            if event['start'] is not None:
                self.seen.add(identity)
                self.exited.add(identity)
            self.exited_during_read.append({'sample': len(self.samples), **event})
        self.discovery_events.extend({'sample':len(self.samples),**event} for event in sample.get('discovery_events', []))
        self.samples.append(sample)

    def report(self, stable_since, stable_until=None):
        """Summarize the whole run, with idle statistics in [since, until).

        The optional end excludes shutdown or a following operation from idle
        statistics. It never excludes those samples from peaks or completeness.
        """
        complete = bool(self.samples) and all(s['complete'] for s in self.samples)
        stable = [s for s in self.samples if s['time_monotonic'] >= stable_since
                  and (stable_until is None or s['time_monotonic'] < stable_until)]
        fields = ('pss_kib', 'swap_pss_kib', 'memory_kib', 'fds', 'threads', 'process_count')
        peak = {key: max((s[key] for s in self.samples), default=0) for key in fields}
        # Independently maximal PSS and SwapPss need not occur in the same sample.
        at_peak = max(self.samples, key=lambda s: s['memory_kib'], default=None)
        stable_values = {key: statistics.median(s[key] for s in stable) if stable else None for key in fields}
        cpu = [s['cpu_percent_observed'] for s in stable if s['cpu_percent_observed'] is not None]
        return {'complete': complete and bool(stable), 'sample_count': len(self.samples),
                'sample_interval_requested_ms': SAMPLE_INTERVAL * 1000,
                'max_observed_interval_ms': max((1000 * (b['time_monotonic'] - a['time_monotonic']) for a, b in zip(self.samples, self.samples[1:])), default=None),
                'stable_sample_count': len(stable), 'sampled_maxima': peak,
                'sample_at_memory_max': at_peak, 'stable_medians': stable_values,
                'stable_cpu_percent_observed_median': statistics.median(cpu) if cpu else None,
                'stable_cpu_percent_observed_max': max(cpu) if cpu else None,
                'observed_process_identities': [list(p) for p in sorted(self.seen)],
                'exited_process_identities': [list(p) for p in sorted(self.exited)],
                'exited_during_read': self.exited_during_read,
                'exited_during_read_count': len(self.exited_during_read),
                'exit_summary_by_identity_phase': summarize_exits(self.exited_during_read),
                'known_exit_identity_count': len({(e['pid'],e['start']) for e in self.exited_during_read if e.get('start') is not None}),
                'unknown_exit_record_count': sum(e.get('start') is None for e in self.exited_during_read),
                'discovery_events': self.discovery_events,
                'discovery_event_count': len(self.discovery_events),
                'discovery_modes': sorted({s.get('discovery','legacy scan') for s in self.samples}),
                'completeness_scope': 'complete verifies surviving readable identities and discovery reads, not lifetimes; confirmed exits and lost/reused TID children can have unavailable process/memory coverage',
                'errors': [{'sample': i, 'errors': s['errors'], 'root_alive': s['root_alive']}
                           for i, s in enumerate(self.samples) if not s['complete']],
                'peak_scope': 'sampled lower bound; not an exact peak; GPU memory excluded',
                'snapshot_scope': 'process reads are not simultaneous; CPU omits first/new and final/exited intervals'}


def stop_tree(process, sampler=None):
    """Terminate own process group plus retained reparented identities; bounded reap."""
    known = set(sampler.known) if sampler else set()
    errors = []

    def alive(identity):
        try:
            stat = read_stat(Path(f'/proc/{identity[0]}/stat').read_text())
            return stat['start'] == identity[1] and stat['state'] != 'Z'
        except (OSError, ValueError, IndexError):
            return False

    def own_group_exists():
        # An unreaped root PID cannot have been reused. After it exits, verify a
        # retained descendant's starttime and group before signalling that group.
        if process.poll() is None:
            return True
        for identity in known:
            try:
                text = Path(f'/proc/{identity[0]}/stat').read_text()
                stat = read_stat(text)
                group = int(text[text.rindex(')') + 2:].split()[2])
                if stat['start'] == identity[1] and group == process.pid:
                    return True
            except (OSError, ValueError, IndexError):
                pass
        return False

    # start_new_session=True is required at launch: this group belongs to this test.
    for sig in (signal.SIGTERM, signal.SIGKILL):
        if own_group_exists():
            try:
                os.killpg(process.pid, sig)
            except ProcessLookupError:
                pass
            except OSError as exc:
                errors.append(str(exc))
        for identity in known:
            if alive(identity):
                try:
                    os.kill(identity[0], sig)
                except ProcessLookupError:
                    pass
                except OSError as exc:
                    errors.append(str(exc))
        try:
            process.wait(timeout=.5)
        except subprocess.TimeoutExpired:
            pass
        until = time.monotonic() + .25
        while any(alive(identity) for identity in known) and time.monotonic() < until:
            time.sleep(.01)
    survivors = [list(identity) for identity in known if alive(identity)]
    if process.poll() is None:
        errors.append('root did not exit after SIGKILL')
    return {'surviving_observed_processes': survivors, 'errors': errors,
            'complete': not survivors and not errors and process.poll() is not None}


def memory_gate(metrics, geometry_report, count=10000, scenario='baseline', limit_mib=60,
                physical=(1920, 1080), error=None, scenario_budget_mb=None):
    reasons = []
    peak = metrics['sampled_maxima']['memory_kib']
    baseline = scenario == 'baseline'
    # Both decimal MB and binary MiB are explicit; no RSS or swap omission.
    limits = {'baseline_objective_mib': 35, 'baseline_analysis_stop_mib': 40,
              'baseline_hard_mib': min(limit_mib, 60), 'modern_goal_mb_decimal': 50,
              'absolute_ceiling_mb_decimal': 150, 'scenario_budget_mb_decimal': scenario_budget_mb}
    fail = False
    if peak * 1024 > 150_000_000:
        reasons.append('absolute 150 MB ceiling exceeded'); fail = True
    if baseline and peak > min(limit_mib, 60) * 1024:
        reasons.append('baseline hard memory gate exceeded'); fail = True
    if baseline and peak > 40 * 1024:
        reasons.append('baseline 40 MiB analysis threshold exceeded: stop and investigate'); fail = True
    goal_met = peak * 1024 <= 50_000_000
    warnings = []
    if metrics.get('exited_during_read_count'):
        warnings.append(f"{metrics.get('known_exit_identity_count', len({(e['pid'],e['start']) for e in metrics.get('exited_during_read',[]) if e.get('start') is not None}))} known process exit identities; {metrics['exited_during_read_count']} observation records (repeated exits are not additional children): memory unavailable in those reads; see raw events and grouped summary")
    if metrics.get('discovery_event_count'):
        warnings.append(f"{metrics['discovery_event_count']} task/children discovery lifetime gap(s): descendants may have escaped observation; see discovery_events")
    if not baseline and not goal_met:
        warnings.append('50 MB decimal goal missed; inspect feature cost')
    if scenario_budget_mb is not None and peak * 1024 > scenario_budget_mb * 1_000_000:
        reasons.append('explicit scenario memory budget exceeded'); fail = True
    valid_geometry = bool(geometry_report and geometry_report.get('native_wayland') is True
                          and geometry_report.get('window_physical') == list(physical))
    if not valid_geometry:
        reasons.append('native Wayland requested geometry was not observed')
    if not metrics['complete']:
        reasons.append('missing or incomplete process-tree/stable samples')
    if error:
        reasons.append(str(error))
    valid = valid_geometry and metrics['complete'] and not error
    status = 'FAIL' if fail else ('PASS' if valid else 'UNVALIDATED')
    return {'gate_status': status, 'gate_reasons': reasons, 'warnings': warnings, 'limits': limits,
            'files': count, 'scenario': scenario, 'geometry': geometry_report,
            'required_physical': list(physical), 'metrics': metrics,
            'sampled_peak_memory_mib': peak / 1024, 'sampled_peak_memory_mb_decimal': peak * 1024 / 1_000_000,
            'baseline_objective_met': peak <= 35 * 1024 if baseline and valid else None,
            'modern_goal_met': goal_met if valid else None,
            'run_policy': 'serialized: no concurrent builds, captures or benchmarks; caller must enforce',
            'gpu_memory': 'not measured by /proc; effects must be off for baseline'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    parser.add_argument('--width', type=int, default=960)
    parser.add_argument('--height', type=int, default=540)
    parser.add_argument('--physical-width', type=int, default=1920)
    parser.add_argument('--physical-height', type=int, default=1080)
    parser.add_argument('--limit-mib', type=float, default=60)
    parser.add_argument('--count', type=int, choices=(10000, 100000), default=10000)
    parser.add_argument('--scenario', default='baseline')
    parser.add_argument('--scenario-budget-mb', type=float, help='explicit hard budget in decimal MB, in addition to global/baseline gates')
    parser.add_argument('--view', choices=('groups', 'list', 'spatial'), default='groups')
    parser.add_argument('--app-arg', action='append', default=[], help='explicit extra app argument; repeat, use = for --flags')
    parser.add_argument('--timeout', type=float, default=12)
    args = parser.parse_args(argv)
    if args.timeout < 8 or args.limit_mib <= 0 or (args.scenario_budget_mb is not None and args.scenario_budget_mb <= 0):
        parser.error('timeout must allow geometry + 5 s scene + 1 s stable; limit must be positive')
    base = Path.home() / '.cache/files-test'
    base.mkdir(parents=True, exist_ok=True)
    metrics, error, client, sampler, process, cleanup = Metrics(), None, None, None, None, None
    readiness = None
    stable_since = float('inf')
    with tempfile.TemporaryDirectory(prefix='memory-', dir=base) as folder:
        listing = Path(folder) / 'files'
        listing.mkdir()
        for i in range(args.count):
            (listing / f'file-{i:06}.txt').touch()
        env = {**os.environ, 'FLTK_BACKEND': 'wayland', 'FILES_CONFIG_DIR': str(Path(folder) / 'config/files'), 'FILES_METRICS_READY': '1'}
        command = [str(args.binary), str(listing), '--size', f'{args.width}x{args.height}', '--scale', '1']
        if args.view == 'list':
            command.append('--list')
        elif args.view == 'spatial':
            command.append('--spatial')
        command.extend(args.app_arg)
        with tempfile.TemporaryFile(dir=base) as log, ThreadPoolExecutor(max_workers=1) as observer:
            try:
                process = subprocess.Popen(command, env=env, stdout=subprocess.DEVNULL, stderr=log, start_new_session=True)
                started = time.monotonic()
                sampler = TreeSampler(process.pid)
                deadline, next_geometry, scene_ready = started + args.timeout, started, None
                observation = None

                def observe():
                    geometry(process.pid, args.width, args.height)
                    return observed_geometry(process.pid, args.width, args.height)
                while time.monotonic() < deadline:
                    tick = time.monotonic()
                    sample = sampler.sample(); metrics.add(sample)
                    if process.poll() is not None or not sample['root_alive']:
                        raise RuntimeError('app exited before stable measurement')
                    if observation is not None and observation.done():
                        observed = observation.result()
                        observation = None
                        if observed:
                            client = observed
                        elif scene_ready is not None:
                            raise RuntimeError('geometry lost after initial observation')
                        next_geometry = time.monotonic() + .2
                    if observation is None and tick >= next_geometry:
                        observation = observer.submit(observe)
                    readiness = scene_readiness(os.pread(log.fileno(), 8192, 0).decode(errors='replace'), args.view, args.count)
                    if scene_ready is not None and readiness is None:
                        raise RuntimeError('scene readiness changed after initial verification')
                    if client is not None and readiness is not None and scene_ready is None:
                        scene_ready = time.monotonic()
                        stable_since = scene_ready + 5
                    if scene_ready is not None and tick >= stable_since + 1:
                        break
                    if client is None and tick - started >= 3:
                        raise RuntimeError('native geometry not verified within 3 seconds')
                    time.sleep(max(0, SAMPLE_INTERVAL - (time.monotonic() - tick)))
                else:
                    raise TimeoutError('measurement deadline: loaded scene/count/readiness or stable window not verified')
            except (OSError, RuntimeError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
                error = str(exc)
            finally:
                if process:
                    cleanup = stop_tree(process, sampler)
                    if not cleanup['complete']:
                        error = (error + '; ' if error else '') + 'cleanup incomplete'
            log.seek(0, os.SEEK_END)
            log.seek(max(0, log.tell() - 8192))
            log_tail = log.read(8192).decode(errors='replace')
        report = memory_gate(metrics.report(stable_since), client, args.count, args.scenario,
                             args.limit_mib, (args.physical_width, args.physical_height), error, args.scenario_budget_mb)
        first = re.search(r'FILES_FIRST_PAINT_US=(\d+)', log_tail)
        report.update({'view_requested': args.view, 'view_validation': 'application readiness marker verified against requested view/count; rendered UI independently validated', 'scene_readiness': readiness, 'font_selected': next((line for line in log_tail.splitlines() if line.startswith('FILES_FONT=')), None), 'first_paint_ms_internal': int(first[1]) / 1000 if first else None,
                       'cleanup': cleanup, 'stderr_tail': log_tail if error else None})
    print(json.dumps(report, indent=2))
    return 0 if report['gate_status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
