import unittest
from memcheck import summarize_exits

class ExitSummaryTests(unittest.TestCase):
    def test_repeated_exit_is_one_group_without_mutating_raw_observations(self):
        raw=[{'sample':i,'pid':42,'start':7,'phase':'stat','reason':'gone','memory_available':False} for i in [1,4,5]]
        result=summarize_exits(raw)
        self.assertEqual(len(result),1)
        self.assertEqual(result[0]['observation_count'],3)
        self.assertEqual((result[0]['first_sample'],result[0]['last_sample']),(1,5))
        self.assertEqual([row['sample'] for row in raw],[1,4,5])
        self.assertFalse(any('observation_count' in row for row in raw))

    def test_identity_phase_and_unknown_start_are_not_merged(self):
        raw=[{'sample':0,'pid':42,'start':start,'phase':phase,'reason':'gone'} for start,phase in [(7,'stat'),(8,'stat'),(7,'smaps'),(None,'stat')]]
        self.assertEqual(len(summarize_exits(raw)),4)
        self.assertIsNone(summarize_exits(raw)[-1]['start'])

if __name__=='__main__':unittest.main()
