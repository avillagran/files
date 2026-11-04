"""Pure aggregation fixtures: no app launch or resource measurements."""
import unittest
from memcheck import Metrics


def sample(at, memory, cpu=0, complete=True):
    return {
        'time_monotonic': at, 'pss_kib': memory, 'swap_pss_kib': 0,
        'memory_kib': memory, 'fds': 8, 'threads': 2, 'process_count': 1,
        'cpu_percent_observed': cpu, 'complete': complete,
        'errors': [] if complete else ['unreadable live process'],
        'root_alive': True, 'processes': [{'pid': 10, 'start': 20}],
    }


class StableWindowTests(unittest.TestCase):
    def fixture(self, closing_complete=True):
        metrics = Metrics()
        for value in [sample(1, 200, 30), sample(2, 100, 0),
                      sample(3, 120, 2), sample(4, 400, 90, closing_complete)]:
            metrics.add(value)
        return metrics

    def test_shutdown_excluded_from_idle_but_retained_in_peak(self):
        report = self.fixture().report(2, 4)
        self.assertTrue(report['complete'])
        self.assertEqual(report['sample_count'], 4)
        self.assertEqual(report['stable_sample_count'], 2)
        self.assertEqual(report['stable_medians']['memory_kib'], 110)
        self.assertEqual(report['stable_cpu_percent_observed_median'], 1)
        self.assertEqual(report['stable_cpu_percent_observed_max'], 2)
        self.assertEqual(report['sampled_maxima']['memory_kib'], 400)
        self.assertEqual(report['sample_at_memory_max']['time_monotonic'], 4)

    def test_incomplete_shutdown_still_invalidates_whole_run(self):
        report = self.fixture(False).report(2, 4)
        self.assertFalse(report['complete'])
        self.assertEqual(report['stable_sample_count'], 2)
        self.assertEqual(report['errors'][0]['sample'], 3)
        self.assertEqual(report['sampled_maxima']['memory_kib'], 400)

    def test_legacy_open_ended_call_preserves_behavior(self):
        report = self.fixture().report(2)
        self.assertEqual(report['stable_sample_count'], 3)
        self.assertEqual(report['stable_cpu_percent_observed_max'], 90)
        self.assertEqual(report['stable_medians']['memory_kib'], 120)

    def test_empty_or_reversed_window_cannot_pass(self):
        for start, end in [(3, 3), (4, 2), (8, 9)]:
            report = self.fixture().report(start, end)
            self.assertFalse(report['complete'])
            self.assertEqual(report['stable_sample_count'], 0)
            self.assertIsNone(report['stable_medians']['memory_kib'])
            self.assertIsNone(report['stable_cpu_percent_observed_max'])
            self.assertEqual(report['sampled_maxima']['memory_kib'], 400)


if __name__ == '__main__':
    unittest.main()
