#!/usr/bin/env python3
"""Fresh-process first-paint gate with an effective monotonic stderr deadline.

POSIX_FADV_DONTNEED requests eviction for the executable only, not global caches.
Run serialized, without compilation, captures or other benchmarks.
"""
import argparse
import json
import math
import os
from pathlib import Path
import re
import selectors
import statistics
import subprocess
import time

from memcheck import Metrics, ROOT, SAMPLE_INTERVAL, stop_tree
from process_metrics import TreeSampler


def percentile(values, percent):
    """Nearest-rank percentile, documented and conservative for small samples."""
    if not values:
        return None
    return sorted(values)[max(0, math.ceil(len(values) * percent / 100) - 1)]


def consume_stderr(process, begin_ns, timeout=3, sampler=None):
    """Consume a byte pipe while checking a deadline even if child never closes it."""
    deadline = time.monotonic() + timeout
    internal = external = None
    tail = b''
    metrics = Metrics()
    next_sample = time.monotonic()
    terminal_samples = []
    with selectors.DefaultSelector() as selector:
        os.set_blocking(process.stderr.fileno(), False)
        selector.register(process.stderr, selectors.EVENT_READ)
        while True:
            now = time.monotonic()
            if now >= deadline:
                raise TimeoutError(f'monotonic startup timeout after {timeout:g} seconds')
            events = selector.select(min(.01, deadline - now))
            for key, _ in events:
                try:
                    chunk = os.read(key.fileobj.fileno(), 65536)
                except BlockingIOError:
                    continue
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                tail = (tail + chunk)[-65536:]
                match = re.search(rb'FILES_FIRST_PAINT_US=(\d+)(?:\r?\n)', tail)
                if match and internal is None:
                    internal = int(match[1]) / 1000
                    external = (time.perf_counter_ns() - begin_ns) / 1_000_000
            if sampler and time.monotonic() >= next_sample and process.poll() is None:
                observed = sampler.sample()
                terminal = (not observed['root_alive'] and internal is not None and not observed['errors']
                            and all(row['pid'] == process.pid for row in observed['processes']))
                if terminal:
                    # Await an actual status-0 exit below. Never convert an
                    # unreadable live identity/descendant into an expected tail.
                    terminal_samples.append(observed)
                else:
                    for prior in terminal_samples:
                        metrics.add(prior)  # A revived/nonterminal root cannot hide a gap.
                    terminal_samples.clear()
                    metrics.add(observed)
                next_sample = time.monotonic() + SAMPLE_INTERVAL
            if process.poll() is not None and not selector.get_map():
                break
    if process.returncode != 0 or internal is None:
        raise RuntimeError(f'first paint absent or app exited {process.returncode}; stderr={tail.decode(errors="replace")!r}')
    first_time = metrics.samples[0]['time_monotonic'] if metrics.samples else float('inf')
    return {'main_to_paint_ms': internal, 'spawn_to_paint_ms': external,
            'metrics': metrics.report(first_time) if sampler else None, 'timing_only': sampler is None,
            'normal_exit_tail_observations': [{'time_monotonic': s['time_monotonic'], 'exited_during_read': s.get('exited_during_read', [])} for s in terminal_samples],
            'phase_markers_us': {key.decode(): int(value) for key, value in re.findall(rb'FILES_([A-Z_]+)_US=(\d+)', tail)},
            'phase_marker_scope': 'main-relative stages except FONT (duration); only available with FILES_TRACE=1'}


def startup_gate(runs, limit_ms=100, errors=None):
    errors = errors or []
    values = [run['spawn_to_paint_ms'] for run in runs]
    internal = [run['main_to_paint_ms'] for run in runs]
    exceeded = any(value >= limit_ms for value in values + internal) or any(error.get('failure') for error in errors)
    incomplete = not runs or bool(errors) or any(not run.get('timing_only') and not (run.get('metrics') or {}).get('complete', False) for run in runs)
    return {'gate_status': 'FAIL' if exceeded else ('UNVALIDATED' if incomplete else 'PASS'),
            'limit_ms_strictly_below': limit_ms, 'runs': runs, 'errors': errors,
            'p50_spawn_to_paint_ms': statistics.median(values) if values else None,
            'p95_spawn_to_paint_ms': percentile(values, 95),
            'max_spawn_to_paint_ms': max(values) if values else None,
            'p50_main_to_paint_ms': statistics.median(internal) if internal else None,
            'p95_main_to_paint_ms': percentile(internal, 95),
            'max_main_to_paint_ms': max(internal) if internal else None,
            'under_limit': sum(value < limit_ms for value in values),
            'percentile_method': 'nearest rank for p95; median for p50',
            'method': 'fresh process; POSIX_FADV_DONTNEED requested for executable only; system/font/shared-library caches not globally evicted; internal marker ends at completed draw callback, not compositor presentation; external marker receipt is an upper bound affected by scheduling and instrumentation',
            'run_policy': 'serialized: no builds, captures or other benchmarks; caller must enforce',
            'instrumentation_cost': 'process-tree /proc collection delays marker receipt between event polls; use --timing-only as a separate external timing run, and retain a separate measured memory run',
            'process_metrics_measured': bool(runs) and all(not run.get('timing_only') for run in runs),
            'gpu_memory': 'not measured; process-tree memory sampling is a lower bound'}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=ROOT / 'target/release/files')
    parser.add_argument('--directory', type=Path, default=ROOT)
    parser.add_argument('--runs', type=int, default=10)
    parser.add_argument('--timeout', type=float, default=3)
    parser.add_argument('--timing-only', action='store_true', help='separate external timing run without /proc sampling; does not validate memory')
    parser.add_argument('--limit-ms', type=float, default=100)
    args = parser.parse_args(argv)
    if args.runs < 1 or args.timeout <= 0 or args.limit_ms <= 0 or args.limit_ms > 100:
        parser.error('positive runs/timeout/limit required; the 100 ms limit cannot be relaxed')
    results, errors = [], []
    for run in range(args.runs):
        process = sampler = None
        try:
            with args.binary.open('rb') as executable:
                os.posix_fadvise(executable.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
            begin = time.perf_counter_ns()
            process = subprocess.Popen([str(args.binary), str(args.directory), '--size', '960x540', '--scale', '1', '--exit-after', '1'],
                                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                       env={**os.environ, 'FLTK_BACKEND': 'wayland'}, start_new_session=True)
            sampler = None if args.timing_only else TreeSampler(process.pid)
            result = consume_stderr(process, begin, args.timeout, sampler)
            results.append(result)
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError, TimeoutError) as exc:
            errors.append({'run': run, 'error': str(exc), 'failure': isinstance(exc, (TimeoutError, RuntimeError))})
        finally:
            if process:
                cleanup = stop_tree(process, sampler)
                if not cleanup['complete']:
                    errors.append({'run': run, 'cleanup': cleanup})
                process.stderr.close()
        if errors:
            break
    report = startup_gate(results, args.limit_ms, errors)
    print(json.dumps(report, indent=2))
    return 0 if report['gate_status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
