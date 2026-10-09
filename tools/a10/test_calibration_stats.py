#!/usr/bin/env python3
import copy
from pathlib import Path
import unittest
from calibration_stats import distribution, ratio
from integrity import loads, Invalid
P=loads((Path(__file__).parent/'calibrated-v1.json').read_bytes())
class Statistics(unittest.TestCase):
    def test_deterministic_full_distribution_no_trimming(self):
        values=[10000000]*29+[100000000]
        a=distribution(values,P)
        self.assertEqual(a,distribution(values,P));self.assertEqual(a['count'],30)
        self.assertEqual(a['median_ns'],10000000);self.assertEqual(a['mad_ns'],0)
        self.assertGreater(a['p95_ns'],a['median_ns']-1)
        self.assertEqual(values[-1],100000000)
    def test_empty_and_zero_are_not_performance_success(self):
        self.assertFalse(distribution([],P)['sufficient_noise'])
        self.assertFalse(distribution([0]*30,P)['sufficient_noise'])
        for values in ([-1]*30,[True]*30):
            with self.assertRaises(Invalid):distribution(values,P)
    def test_ratio_sufficiency_not_regression_threshold(self):
        with self.assertRaises(Invalid):ratio([100]*29,[100]*30,P)
        value=ratio([100]*30,[200]*30,P)
        self.assertEqual(value['candidate_over_baseline'],2)
        self.assertEqual(value['low'],2);self.assertIn('informational',value['classification'])
    def test_wide_ci_is_inconclusive(self):
        values=[10000000]*15+[20000000]*15
        self.assertFalse(distribution(values,P)['sufficient_noise'])
        with self.assertRaises(Invalid):ratio(values,[10000000]*30,P)
if __name__=='__main__':unittest.main()
