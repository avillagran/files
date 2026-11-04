"""Independent fixtures for optional per-task process discovery; no live benchmark."""
import tempfile
import unittest
from pathlib import Path
from unittest import mock
from process_metrics import TreeSampler
from test_process_metrics import stat_text


class ChildrenDiscoveryReview(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.proc = Path(self.tmp.name)
        self.process(10, 1, 100)

    def process(self, pid, parent, start, pss=100):
        base = self.proc / str(pid)
        base.mkdir(exist_ok=True)
        (base / 'stat').write_text(stat_text(pid, parent, start))
        (base / 'smaps_rollup').write_text(f'Pss: {pss} kB\nSwapPss: 20 kB\n')
        (base / 'fd').mkdir(exist_ok=True)
        self.task(pid, pid, start)

    def task(self, pid, tid, start, children=''):
        base = self.proc / str(pid) / 'task' / str(tid)
        base.mkdir(parents=True, exist_ok=True)
        (base / 'stat').write_text(stat_text(tid, 1, start))
        (base / 'children').write_text(children)
        return base

    def sampler(self):
        return TreeSampler(10, self.proc, discovery='children')

    def test_nonleader_child_and_grandchild_remain_after_reparent(self):
        self.process(11, 10, 101, 200)
        self.process(12, 11, 102, 300)
        self.task(10, 99, 105, '11')
        self.task(11, 11, 101, '12')
        s = self.sampler()
        first = s.sample()
        self.assertTrue(first['complete'], first['errors'])
        self.assertEqual(first['memory_kib'], 660)
        (self.proc / '11/stat').write_text(stat_text(11, 1, 101))
        self.task(10, 99, 105, '')
        second = s.sample()
        self.assertTrue(second['complete'], second['errors'])
        self.assertEqual(second['memory_kib'], 660)

    def test_missing_children_in_live_task_is_not_empty(self):
        path = self.task(10, 99, 105) / 'children'
        path.unlink()
        result = self.sampler().sample()
        self.assertFalse(result['complete'])
        self.assertTrue(any(e.get('phase') == 'task_children' for e in result['errors']))

    def test_permission_denied_children_invalidates_sample(self):
        path = self.task(10, 99, 105) / 'children'
        original = Path.read_text
        def read(p, *args, **kwargs):
            if p == path:
                raise PermissionError('fixture unreadable children')
            return original(p, *args, **kwargs)
        s = self.sampler()
        with mock.patch.object(Path, 'read_text', read):
            result = s.sample()
        self.assertFalse(result['complete'])
        self.assertTrue(any(e.get('phase') == 'task_children' for e in result['errors']))

    def test_parent_reuse_during_discovery_does_not_adopt_new_child(self):
        self.process(11, 10, 101, 9000)
        path = self.task(10, 99, 105, '11') / 'children'
        original = Path.read_text
        def read(p, *args, **kwargs):
            value = original(p, *args, **kwargs)
            if p == path:
                (self.proc / '10/stat').write_text(stat_text(10, 1, 200))
            return value
        s = self.sampler()
        with mock.patch.object(Path, 'read_text', read):
            result = s.sample()
        self.assertFalse(result['complete'])
        self.assertNotIn((11, 101), s.known)
        self.assertTrue(any(e.get('phase') == 'discovery_children_identity' for e in result['errors']))


if __name__ == '__main__':
    unittest.main()
