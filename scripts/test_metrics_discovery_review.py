"""Los huecos de discovery deben sobrevivir al resumen y a la ventana estable."""
import unittest
from memcheck import Metrics
from test_metrics_windows import sample


class DiscoveryAggregationReview(unittest.TestCase):
    def test_discovery_gap_outside_stable_window_remains_visible(self):
        metrics = Metrics()
        first = sample(1, 300)
        first['discovery'] = 'children'
        event = {'pid': 10, 'start': 20, 'tid': 77, 'task_start': 30,
                 'phase': 'task_children', 'reason': 'task disappeared',
                 'memory_available': False}
        first['discovery_events'] = [event]
        metrics.add(first)
        stable = sample(2, 100)
        stable['discovery'] = 'children'
        metrics.add(stable)
        report = metrics.report(2, 3)
        self.assertEqual(report['stable_sample_count'], 1)
        self.assertEqual(report['discovery_event_count'], 1)
        self.assertEqual(report['discovery_events'], [{'sample': 0, **event}])
        # Una identidad de hilo no equivale a un proceso con memoria leída.
        self.assertEqual(report['observed_process_identities'], [[10, 20]])
        self.assertEqual(report['discovery_modes'], ['children'])
        self.assertEqual(report['sampled_maxima']['memory_kib'], 300)

    def test_unreadable_discovery_after_stable_window_invalidates_run(self):
        metrics = Metrics()
        metrics.add(sample(1, 100))
        late = sample(2, 200, complete=False)
        late['errors'] = [{'phase': 'task_children', 'pid': 10,
                           'error': 'permission denied'}]
        metrics.add(late)
        report = metrics.report(1, 2)
        self.assertFalse(report['complete'])
        self.assertEqual(report['errors'][0]['sample'], 1)
        self.assertEqual(report['errors'][0]['errors'], late['errors'])
        self.assertEqual(report['stable_medians']['memory_kib'], 100)


if __name__ == '__main__':
    unittest.main()
