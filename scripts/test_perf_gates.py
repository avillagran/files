#!/usr/bin/env python3
"""Gate logic/timeout fixtures only: does not launch files or run benchmarks."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest import mock

import memcheck

spec = importlib.util.spec_from_file_location('startup_check', Path(__file__).with_name('startup-check.py'))
startup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(startup)


def sample(pss=20 * 1024, swap=0, complete=True, now=1, processes=None):
    return {'pss_kib': pss, 'swap_pss_kib': swap, 'memory_kib': pss + swap,
            'threads': 2, 'fds': 8, 'process_count': len(processes or [(1, 10)]),
            'processes': [{'pid': pid, 'start': start} for pid, start in (processes or [(1, 10)])],
            'complete': complete, 'root_alive': True, 'errors': [] if complete else [{'pid': 2, 'error': 'unreadable'}],
            'time_monotonic': now, 'cpu_percent_observed': None if now == 1 else 2.0}


def summary(*samples):
    metrics = memcheck.Metrics()
    for value in samples or [sample()]:
        metrics.add(value)
    return metrics.report(0)


GEOMETRY = {'native_wayland': True, 'window_physical': [1920, 1080]}


class PerfGateTests(unittest.TestCase):
    def test_swap_counts_toward_analysis_stop(self):
        report = memcheck.memory_gate(summary(sample(25 * 1024, 17 * 1024)), GEOMETRY)
        self.assertEqual(report['gate_status'], 'FAIL')
        self.assertEqual(report['sampled_peak_memory_mib'], 42)
        self.assertTrue(any('analysis' in reason for reason in report['gate_reasons']))

    def test_no_geometry_never_passes(self):
        self.assertEqual(memcheck.memory_gate(summary(), None)['gate_status'], 'UNVALIDATED')
        wrong = {**GEOMETRY, 'window_physical': [960, 540]}
        self.assertEqual(memcheck.memory_gate(summary(), wrong)['gate_status'], 'UNVALIDATED')

    def test_incomplete_sample_is_not_discarded(self):
        report = summary(sample(complete=False), sample(now=2))
        self.assertFalse(report['complete'])
        self.assertEqual(memcheck.memory_gate(report, GEOMETRY)['gate_status'], 'UNVALIDATED')

    def test_peak_and_retained_exit_identity(self):
        report = summary(sample(20 * 1024, processes=[(1, 10), (2, 11)]),
                         sample(30 * 1024, 5 * 1024, now=2), sample(10 * 1024, now=3))
        self.assertEqual(report['sampled_maxima']['memory_kib'], 35 * 1024)
        self.assertEqual(report['exited_process_identities'], [[2, 11]])
        self.assertEqual(report['stable_medians']['pss_kib'], 20 * 1024)

    def test_baseline_objective_is_distinct_from_stop(self):
        report = memcheck.memory_gate(summary(sample(36 * 1024)), GEOMETRY)
        self.assertEqual(report['gate_status'], 'PASS')
        self.assertFalse(report['baseline_objective_met'])
        report = memcheck.memory_gate(summary(sample(61 * 1024)), GEOMETRY, limit_mib=150)
        self.assertEqual(report['gate_status'], 'FAIL')
        self.assertEqual(report['limits']['baseline_hard_mib'], 60)

    def test_decimal_modern_goal(self):
        report = memcheck.memory_gate(summary(sample(49 * 1024)), GEOMETRY, scenario='preview')
        self.assertEqual(report['gate_status'], 'PASS')  # 49 MiB > 50 decimal MB goal, below ceiling.
        self.assertFalse(report['modern_goal_met'])
        self.assertTrue(report['warnings'])
        self.assertGreater(report['sampled_peak_memory_mb_decimal'], 50)
        explicit = memcheck.memory_gate(summary(sample(49 * 1024)), GEOMETRY, scenario='preview', scenario_budget_mb=50)
        self.assertEqual(explicit['gate_status'], 'FAIL')

    def test_hyprland_geometry_is_observed_not_assumed(self):
        with mock.patch.dict(os.environ, {'HYPRLAND_INSTANCE_SIGNATURE': 'fixture'}):
            with mock.patch.object(memcheck, 'hypr_json', return_value=[]):
                self.assertIsNone(memcheck.observed_geometry(10, 960, 540))
            client = {'pid': 10, 'size': [960, 540], 'floating': True, 'xwayland': False, 'monitor': 1, 'address': 'test'}
            with mock.patch.object(memcheck, 'hypr_json', side_effect=[[client], [{'id': 1, 'scale': 2}]]):
                self.assertEqual(memcheck.observed_geometry(10, 960, 540)['window_physical'], [1920, 1080])

    def test_geometry_does_not_resize_already_correct_window(self):
        client = {'pid': 10, 'size': [960, 540], 'floating': True, 'address': 'fixture'}
        with mock.patch.dict(os.environ, {'HYPRLAND_INSTANCE_SIGNATURE': 'fixture'}), mock.patch.object(memcheck, 'hypr_json', return_value=[client]), mock.patch.object(memcheck.subprocess, 'run') as dispatch:
            self.assertEqual(memcheck.geometry(10, 960, 540), 'fixture')
            dispatch.assert_not_called()

    def test_confirmed_exit_event_is_retained_in_aggregate_with_warning(self):
        value = sample()
        value['exited_during_read'] = [{'pid': 22, 'start': 33, 'phase': 'fd', 'reason': 'directory disappeared', 'memory_available': False}]
        metrics = summary(value)
        self.assertIn([22, 33], metrics['observed_process_identities'])
        self.assertEqual(metrics['exited_during_read_count'], 1)
        report = memcheck.memory_gate(metrics, GEOMETRY)
        self.assertEqual(report['gate_status'], 'PASS')
        self.assertTrue(any('memory unavailable' in warning for warning in report['warnings']))

    def test_scene_readiness_requires_count_and_view_specific_completion(self):
        line = 'FILES_SCENE_READY view=groups loaded=true entries=100000 stats_ready=false layout_ready=false generation=1'
        self.assertIsNone(memcheck.scene_readiness(line, 'groups', 100000))
        ready = line.replace('stats_ready=false', 'stats_ready=true')
        self.assertIsNotNone(memcheck.scene_readiness(ready, 'groups', 100000))
        self.assertIsNone(memcheck.scene_readiness(ready, 'groups', 10000))
        self.assertIsNone(memcheck.scene_readiness(ready + '\n' + ready.replace('view=groups', 'view=list'), 'groups', 100000))
        self.assertIsNone(memcheck.scene_readiness(ready, 'spatial', 100000))
        spatial = line.replace('view=groups', 'view=spatial')
        self.assertIsNone(memcheck.scene_readiness(spatial, 'spatial', 100000))
        self.assertIsNotNone(memcheck.scene_readiness(spatial.replace('layout_ready=false', 'layout_ready=true'), 'spatial', 100000))

    def test_timeout_does_not_wait_for_stderr_eof(self):
        process = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(10)'],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
        start = time.monotonic()
        try:
            with self.assertRaises(TimeoutError):
                startup.consume_stderr(process, time.perf_counter_ns(), timeout=.12)
            self.assertLess(time.monotonic() - start, .8)
        finally:
            cleanup = memcheck.stop_tree(process)
            process.stderr.close()
        self.assertTrue(cleanup['complete'])

    def test_marker_does_not_bypass_hung_exit_deadline(self):
        process = subprocess.Popen([sys.executable, '-c', 'import sys,time; print("FILES_FIRST_PAINT_US=1000",file=sys.stderr,flush=True); time.sleep(10)'],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
        try:
            with self.assertRaises(TimeoutError):
                startup.consume_stderr(process, time.perf_counter_ns(), timeout=.12)
        finally:
            memcheck.stop_tree(process)
            process.stderr.close()

    def test_expected_exit_race_preserves_previous_complete_interval(self):
        process = subprocess.Popen([sys.executable, '-c', 'import sys,time; print("FILES_FIRST_PAINT_US=1000",file=sys.stderr,flush=True); time.sleep(.12)'],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)

        class ExitRaceSampler:
            calls = 0

            def sample(self):
                self.calls += 1
                if self.calls == 1:
                    return sample(now=time.monotonic())
                process.wait(timeout=.5)
                return {**sample(complete=False, now=time.monotonic()), 'root_alive': False, 'errors': [], 'processes': []}

        try:
            result = startup.consume_stderr(process, time.perf_counter_ns(), timeout=1, sampler=ExitRaceSampler())
            self.assertTrue(result['metrics']['complete'])
            self.assertEqual(result['metrics']['sample_count'], 1)
        finally:
            memcheck.stop_tree(process)
            process.stderr.close()

    def test_terminal_root_before_wait_status_is_not_a_live_metric_gap(self):
        process = subprocess.Popen([sys.executable, '-c', 'import sys,time; print("FILES_FIRST_PAINT_US=1000",file=sys.stderr,flush=True); time.sleep(.12)'], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
        class TailSampler:
            calls = 0
            def sample(self):
                self.calls += 1
                if self.calls == 1:
                    return {**sample(now=time.monotonic()), 'processes': [{'pid': process.pid, 'start': 1}]}
                return {**sample(complete=False, now=time.monotonic()), 'root_alive': False, 'errors': [], 'processes': []}
        try:
            result = startup.consume_stderr(process, time.perf_counter_ns(), timeout=1, sampler=TailSampler())
            self.assertTrue(result['metrics']['complete'])
            self.assertTrue(result['normal_exit_tail_observations'])
        finally:
            memcheck.stop_tree(process)
            process.stderr.close()

    def test_normal_exit_does_not_hide_an_unverified_descendant_error(self):
        process = subprocess.Popen([sys.executable, '-c', 'import sys,time; print("FILES_FIRST_PAINT_US=1000",file=sys.stderr,flush=True); time.sleep(.08)'], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True)
        class ErrorSampler:
            calls = 0
            def sample(self):
                self.calls += 1
                if self.calls == 1:
                    return sample(now=time.monotonic())
                process.wait(timeout=.5)
                return {**sample(complete=False, now=time.monotonic()), 'root_alive': False}
        try:
            result = startup.consume_stderr(process, time.perf_counter_ns(), timeout=1, sampler=ErrorSampler())
            self.assertFalse(result['metrics']['complete'])
        finally:
            memcheck.stop_tree(process)
            process.stderr.close()

    def test_startup_percentile_and_strict_limit(self):
        self.assertEqual(startup.percentile(list(range(1, 11)), 95), 10)
        runs = [{'spawn_to_paint_ms': 99, 'main_to_paint_ms': 98, 'metrics': {'complete': True}}]
        self.assertEqual(startup.startup_gate(runs)['gate_status'], 'PASS')
        runs[0]['spawn_to_paint_ms'] = 100
        self.assertEqual(startup.startup_gate(runs)['gate_status'], 'FAIL')
        runs[0]['spawn_to_paint_ms'] = 99
        runs[0]['metrics']['complete'] = False
        self.assertEqual(startup.startup_gate(runs)['gate_status'], 'UNVALIDATED')

    def test_timing_only_does_not_claim_memory(self):
        runs = [{'spawn_to_paint_ms': 50, 'main_to_paint_ms': 45, 'metrics': None, 'timing_only': True}]
        report = startup.startup_gate(runs)
        self.assertEqual(report['gate_status'], 'PASS')
        self.assertFalse(report['process_metrics_measured'])
        self.assertEqual(startup.startup_gate(runs, errors=[{'failure': True}])['gate_status'], 'FAIL')

    def test_stable_sample_required(self):
        metrics = memcheck.Metrics()
        metrics.add(sample())
        self.assertFalse(metrics.report(5)['complete'])


if __name__ == '__main__':
    unittest.main()
