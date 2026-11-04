import tempfile
import unittest
import shutil
from unittest import mock
from pathlib import Path
from process_metrics import TreeSampler, read_stat


def stat_text(pid, parent, start, ticks=0):
    fields = ['0'] * 22
    fields[0], fields[1] = 'S', str(parent)
    fields[11], fields[12] = str(ticks), '0'
    fields[17], fields[19] = '2', str(start)
    return f'{pid} (name with ) parentheses) ' + ' '.join(fields)


class MetricsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.proc = Path(self.tmp.name)
        self.now = 1.0

    def process(self, pid, parent, start, pss=100, swap=20, ticks=0):
        base = self.proc / str(pid)
        base.mkdir(exist_ok=True)
        (base / 'stat').write_text(stat_text(pid, parent, start, ticks))
        (base / 'smaps_rollup').write_text(f'Pss: {pss} kB\nSwapPss: {swap} kB\n')
        (base / 'fd').mkdir(exist_ok=True)
        (base / 'fd' / '0').touch()

    def sampler(self):
        return TreeSampler(10, self.proc, lambda: self.now)

    def test_stat_handles_parentheses(self):
        parsed = read_stat(stat_text(10, 1, 900, 12))
        self.assertEqual((parsed['pid'], parsed['ppid'], parsed['start'], parsed['ticks']), (10, 1, 900, 12))

    def test_tree_and_reparented_child(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101, pss=200)
        self.process(12, 11, 102, pss=300)
        self.process(20, 1, 99, pss=9999)
        sampler = self.sampler()
        first = sampler.sample()
        self.assertTrue(first['complete'])
        self.assertEqual(first['memory_kib'], 660)
        self.assertEqual(first['process_count'], 3)
        self.process(11, 1, 101, pss=200)
        self.assertEqual(sampler.sample()['memory_kib'], 660)

    def test_reused_pid_is_not_retained(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        sampler = self.sampler()
        sampler.sample()
        self.process(11, 1, 200, pss=9999)
        self.assertEqual(sampler.sample()['process_count'], 1)
        self.process(10, 1, 300)
        result = sampler.sample()
        self.assertFalse(result['root_alive'])
        self.assertFalse(result['complete'])

    def test_missing_metrics_invalidates_sample(self):
        self.process(10, 1, 100)
        sampler = self.sampler()
        (self.proc / '10' / 'smaps_rollup').write_text('Pss: 100 kB\n')
        result = sampler.sample()
        self.assertFalse(result['complete'])
        self.assertEqual(len(result['errors']), 1)

    def test_cpu_interval(self):
        self.process(10, 1, 100)
        sampler = self.sampler()
        self.assertIsNone(sampler.sample()['cpu_percent_observed'])
        self.now = 3.0
        self.process(10, 1, 100, ticks=sampler.hz)
        self.assertEqual(sampler.sample()['cpu_percent_observed'], 50)

    def test_unreadable_observed_child_invalidates_sample(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        sampler = self.sampler()
        self.assertTrue(sampler.sample()['complete'])
        for broken in ('malformed', ''):
            (self.proc / '11' / 'stat').write_text(broken)
            result = sampler.sample()
            self.assertFalse(result['complete'])
            self.assertEqual(result['errors'][0]['pid'], 11)
        (self.proc / '11' / 'stat').unlink()
        self.assertFalse(sampler.sample()['complete'])

    def test_exited_observed_child_is_not_a_read_failure(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        sampler = self.sampler()
        sampler.sample()
        shutil.rmtree(self.proc / '11')
        result = sampler.sample()
        self.assertTrue(result['complete'])
        self.assertEqual(result['process_count'], 1)

    def interrupted_read(self, action, exception=FileNotFoundError):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        sampler = self.sampler()
        sampler.sample()
        original = Path.read_text
        def read(path, *args, **kwargs):
            if path == self.proc / '11' / 'smaps_rollup':
                action()
                raise exception('metric disappeared during collection')
            return original(path, *args, **kwargs)
        with mock.patch.object(Path, 'read_text', read):
            return sampler.sample()

    def test_exit_between_stat_and_metric_is_explicit_not_a_live_zero(self):
        result = self.interrupted_read(lambda: shutil.rmtree(self.proc / '11'))
        self.assertTrue(result['complete'])
        self.assertEqual(result['memory_kib'], 120)
        self.assertEqual([row['pid'] for row in result['processes']], [10])
        event = result['exited_during_read'][0]
        self.assertEqual((event['pid'], event['start'], event['phase']), (11, 101, 'smaps_rollup'))
        self.assertFalse(event['memory_available'])
        self.assertIn('disappeared', event['reason'])

    def test_missing_metric_of_live_identity_remains_invalid(self):
        result = self.interrupted_read(lambda: None)
        self.assertFalse(result['complete'])
        self.assertEqual(result['exited_during_read'], [])

    def test_unverifiable_stat_after_missing_metric_remains_invalid(self):
        result = self.interrupted_read(lambda: (self.proc / '11' / 'stat').write_text('malformed'))
        self.assertFalse(result['complete'])
        self.assertEqual(result['exited_during_read'], [])

    def test_reuse_between_stat_and_metric_is_reported_as_ended_identity(self):
        result = self.interrupted_read(lambda: self.process(11, 1, 999, pss=9999))
        self.assertTrue(result['complete'])
        self.assertEqual(result['memory_kib'], 120)
        self.assertEqual(result['exited_during_read'][0]['start'], 101)
        self.assertIn('reused', result['exited_during_read'][0]['reason'])

    def test_same_identity_zombie_confirms_ended_address_space(self):
        def zombie():
            path = self.proc / '11' / 'stat'
            path.write_text(stat_text(11, 10, 101).replace(') S ', ') Z '))
        result = self.interrupted_read(zombie)
        self.assertTrue(result['complete'])
        self.assertIn('zombie', result['exited_during_read'][0]['reason'])
        self.assertEqual(result['memory_kib'], 120)

    def test_esrch_metric_requires_confirmed_ended_identity(self):
        import errno
        class Gone(OSError):
            def __init__(self, message):
                super().__init__(errno.ESRCH, message)
        result = self.interrupted_read(lambda: shutil.rmtree(self.proc / '11'), Gone)
        self.assertTrue(result['complete'])
        self.assertEqual(result['exited_during_read'][0]['start'], 101)
        result = self.interrupted_read(lambda: None, Gone)
        self.assertFalse(result['complete'])
        self.assertEqual(result['exited_during_read'], [])

    def test_permission_error_never_relaxes_into_confirmed_exit(self):
        result = self.interrupted_read(lambda: shutil.rmtree(self.proc / '11'), PermissionError)
        self.assertFalse(result['complete'])
        self.assertEqual(result['exited_during_read'], [])


class ChildrenMetricsTests(MetricsTests):
    """Run the unchanged legacy contract in both discovery modes, then races."""
    def process(self, pid, parent, start, pss=100, swap=20, ticks=0):
        super().process(pid, parent, start, pss, swap, ticks)
        self.task(pid, pid, start)
        # Fixture /proc exposes current parentage through each leader's children.
        parents = {}
        for entry in self.proc.iterdir():
            try:
                child = read_stat((entry / 'stat').read_text())
                parents.setdefault(child['ppid'], []).append(child['pid'])
            except (OSError, ValueError, IndexError):
                pass
        for entry in self.proc.iterdir():
            task = entry / 'task' / entry.name / 'children'
            if task.exists():
                task.write_text(' '.join(map(str, parents.get(int(entry.name), []))))

    def task(self, pid, tid, start, children=''):
        task = self.proc / str(pid) / 'task' / str(tid)
        task.mkdir(parents=True, exist_ok=True)
        (task / 'stat').write_text(stat_text(tid, 1, start))
        (task / 'children').write_text(children)
        return task

    def sampler(self):
        return TreeSampler(10, self.proc, lambda: self.now, discovery='children')

    def test_invalid_mode_rejected(self):
        with self.assertRaises(ValueError):
            TreeSampler(10, self.proc, discovery='typo')

    def test_all_threads_and_recursive_retained_children_without_global_scan(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101, pss=200)
        self.process(12, 11, 102, pss=300)
        self.process(20, 1, 99, pss=9999)
        (self.proc / '10' / 'task' / '10' / 'children').write_text('')
        self.task(10, 15, 105, '11')
        sampler = self.sampler()
        original = Path.iterdir
        def no_global(path):
            if path == self.proc:
                raise AssertionError('children mode must not enumerate global /proc')
            return original(path)
        with mock.patch.object(Path, 'iterdir', no_global):
            first = sampler.sample()
        self.assertTrue(first['complete'], first['errors'])
        self.assertEqual(first['process_count'], 3)
        self.assertEqual(first['memory_kib'], 660)
        self.process(11, 1, 101, pss=200)
        self.process(13, 11, 103, pss=400)
        second = sampler.sample()
        self.assertTrue(second['complete'], second['errors'])
        self.assertEqual(second['process_count'], 4)
        self.assertEqual(second['memory_kib'], 1080)

    def test_unknown_live_child_stat_failure_invalidates(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        original = Path.read_text
        for exc in (PermissionError('denied'), ValueError('bad stat'),
                    FileNotFoundError('stat absent'), OSError(5, 'I/O error'), OSError(3, 'no such process')):
            def broken(path, *args, **kwargs):
                if path == self.proc / '11' / 'stat':
                    raise exc
                return original(path, *args, **kwargs)
            with self.subTest(exc=repr(exc)), mock.patch.object(Path, 'read_text', broken):
                result = self.sampler().sample()
            self.assertFalse(result['complete'])
            self.assertEqual(result['errors'][0]['phase'], 'discovery_stat')
            self.assertEqual(result['errors'][0]['pid'], 11)

    def test_new_child_exited_before_stat_has_explicit_unknown_identity_gap(self):
        self.process(10, 1, 100)
        (self.proc / '10' / 'task' / '10' / 'children').write_text('11')
        result = self.sampler().sample()
        self.assertTrue(result['complete'], result['errors'])
        self.assertEqual(result['process_count'], 1)
        event = result['exited_during_read'][0]
        self.assertEqual((event['pid'], event['start'], event['phase']), (11, None, 'discovery_stat'))
        self.assertFalse(event['memory_available'])

    def test_live_task_children_failures_never_become_empty(self):
        self.process(10, 1, 100)
        sampler = self.sampler()
        task = self.proc / '10' / 'task' / '10'
        original = Path.read_text
        for exc in (PermissionError('denied'), FileNotFoundError('gone'),
                    OSError(3, 'no such process'), OSError(5, 'I/O error')):
            def broken(path, *args, **kwargs):
                if path == task / 'children':
                    raise exc
                return original(path, *args, **kwargs)
            with self.subTest(exc=repr(exc)), mock.patch.object(Path, 'read_text', broken):
                result = sampler.sample()
            self.assertFalse(result['complete'])
            self.assertEqual(result['errors'][0]['phase'], 'task_children')
        (task / 'children').write_text('11 invalid')
        self.assertFalse(sampler.sample()['complete'])

    def test_live_task_stat_unreadable_invalidates(self):
        self.process(10, 1, 100)
        task = self.task(10, 15, 105)
        sampler = self.sampler()
        (task / 'stat').write_text('corrupt')
        result = sampler.sample()
        self.assertFalse(result['complete'])
        self.assertEqual(result['errors'][0]['tid'], 15)

    def test_task_disappeared_after_enumeration_is_explicit_lifetime_gap(self):
        self.process(10, 1, 100)
        task = self.task(10, 15, 105)
        sampler = self.sampler()
        original = Path.read_text
        def gone(path, *args, **kwargs):
            if path == task / 'stat':
                shutil.rmtree(task)
                raise FileNotFoundError('vanished task')
            return original(path, *args, **kwargs)
        with mock.patch.object(Path, 'read_text', gone):
            result = sampler.sample()
        self.assertTrue(result['complete'], result['errors'])
        self.assertEqual(result['discovery_events'][0]['tid'], 15)
        self.assertFalse(result['discovery_events'][0]['memory_available'])

    def test_permission_error_does_not_relax_when_task_disappeared(self):
        self.process(10, 1, 100)
        task = self.task(10, 15, 105)
        sampler = self.sampler()
        original = Path.read_text
        def denied(path, *args, **kwargs):
            if path == task / 'stat':
                shutil.rmtree(task)
                raise PermissionError('denied')
            return original(path, *args, **kwargs)
        with mock.patch.object(Path, 'read_text', denied):
            result = sampler.sample()
        self.assertFalse(result['complete'])
        self.assertEqual(result['discovery_events'], [])

    def test_reused_tid_is_explicit_gap(self):
        self.process(10, 1, 100)
        task = self.task(10, 15, 105)
        sampler = self.sampler()
        original = Path.read_text
        def reuse(path, *args, **kwargs):
            result = original(path, *args, **kwargs)
            if path == task / 'children':
                (task / 'stat').write_text(stat_text(15, 1, 999))
            return result
        with mock.patch.object(Path, 'read_text', reuse):
            result = sampler.sample()
        self.assertTrue(result['complete'], result['errors'])
        self.assertIn('TID reused', result['discovery_events'][0]['reason'])

    def test_unreadable_task_directory_invalidates_surviving_owner(self):
        self.process(10, 1, 100)
        sampler = self.sampler()
        original = Path.iterdir
        for exc in (PermissionError('denied'), FileNotFoundError('gone')):
            def denied(path):
                if path == self.proc / '10' / 'task':
                    raise exc
                return original(path)
            with self.subTest(exc=repr(exc)), mock.patch.object(Path, 'iterdir', denied):
                result = sampler.sample()
            self.assertFalse(result['complete'])
            self.assertEqual(result['errors'][0]['phase'], 'discovery_tasks')

    def test_listed_pid_reused_unrelated_is_not_counted_and_invalidates_discovery(self):
        self.process(10, 1, 100)
        self.process(11, 1, 200, pss=9999)
        (self.proc / '10' / 'task' / '10' / 'children').write_text('11')
        result = self.sampler().sample()
        self.assertFalse(result['complete'])
        self.assertEqual(result['process_count'], 1)
        self.assertEqual(result['errors'][0]['phase'], 'discovery_relationship')

    def test_failed_first_identity_stays_invalid_after_reparenting_until_resolved(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        sampler = self.sampler()
        original = Path.read_text
        def denied(path, *args, **kwargs):
            if path == self.proc / '11' / 'stat':
                raise PermissionError('new child unreadable')
            return original(path, *args, **kwargs)
        with mock.patch.object(Path, 'read_text', denied):
            self.assertFalse(sampler.sample()['complete'])
        self.process(11, 1, 101)
        second = sampler.sample()
        self.assertFalse(second['complete'])
        self.assertEqual(second['process_count'], 1)
        self.assertTrue(any(e['phase'] == 'discovery_relationship' for e in second['errors']))
        self.process(11, 10, 101)
        recovered = sampler.sample()
        self.assertTrue(recovered['complete'], recovered['errors'])
        self.assertEqual(recovered['process_count'], 2)

    def test_unresolved_child_is_not_forgotten_when_parent_exits(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101)
        self.process(12, 11, 102)
        sampler = self.sampler()
        original = Path.read_text
        def denied(path, *args, **kwargs):
            if path == self.proc / '12' / 'stat':
                raise PermissionError('new grandchild unreadable')
            return original(path, *args, **kwargs)
        with mock.patch.object(Path, 'read_text', denied):
            self.assertFalse(sampler.sample()['complete'])
        shutil.rmtree(self.proc / '11')
        (self.proc / '10' / 'task' / '10' / 'children').write_text('')
        (self.proc / '12' / 'stat').write_text(stat_text(12, 1, 102))
        self.assertFalse(sampler.sample()['complete'])
        shutil.rmtree(self.proc / '12')
        result = sampler.sample()
        self.assertTrue(result['complete'], result['errors'])
        self.assertTrue(any(e['pid'] == 12 and e['start'] is None for e in result['exited_during_read']))

    def test_empty_tasks_for_live_owner_invalidates(self):
        self.process(10, 1, 100)
        shutil.rmtree(self.proc / '10' / 'task' / '10')
        result = self.sampler().sample()
        self.assertFalse(result['complete'])
        self.assertEqual(result['errors'][0]['phase'], 'discovery_tasks')

    def test_parent_reuse_during_children_read_does_not_admit_foreign_children(self):
        self.process(10, 1, 100)
        self.process(11, 10, 101, pss=9999)
        sampler = self.sampler()
        original = Path.read_text
        def reuse(path, *args, **kwargs):
            result = original(path, *args, **kwargs)
            if path == self.proc / '10' / 'task' / '10' / 'children':
                (self.proc / '10' / 'stat').write_text(stat_text(10, 1, 999))
            return result
        with mock.patch.object(Path, 'read_text', reuse):
            result = sampler.sample()
        self.assertFalse(result['complete'])
        self.assertFalse(result['root_alive'])
        self.assertNotIn(11, [row['pid'] for row in result['processes']])
        self.assertTrue(any(e['phase'] == 'discovery_children_identity' for e in result['errors']))


if __name__ == '__main__':
    unittest.main()
