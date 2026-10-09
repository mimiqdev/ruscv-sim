#!/usr/bin/env python3
"""Baseline profile controls: scope, floor, classification; not measurements."""
import unittest
import baseline as b
from report_schema import quantile


class BaselineControls(unittest.TestCase):
    def test_manifest_scope_is_exactly_three_representative_workloads(self):
        data = b.manifest()
        self.assertEqual([w['id'] for w in data['workloads']],
                         ['alu-branch-loop', 'ram-store-load-loop', 'device-cli-hello'])
        self.assertEqual(data['version'], 1)
        self.assertEqual(bytes.fromhex(data['workloads'][2]['expected_stdout_hex']), b'Hello!\n')

    def test_verification_floor_is_exit_plus_exact_stdout(self):
        good = {'exit_code': 3, 'expected_exit': 3, 'stdout_hex': '', 'expected_stdout_hex': ''}
        self.assertTrue(b.verified(good))
        self.assertFalse(b.verified(dict(good, exit_code=0)))
        self.assertFalse(b.verified(dict(good, exit_code=0, stdout_hex='2e', expected_stdout_hex='2e')))
        self.assertFalse(b.verified(dict(good, stdout_hex='2f', expected_stdout_hex='2e')))

    def test_quantile_and_classification(self):
        self.assertEqual(quantile([10, 20, 30], 0.5), 20)
        self.assertEqual(quantile([10, 20], 0.05), 10.5)
        d = b.distribution([5, 10, 15])
        self.assertEqual((d['count'], d['median_ns'], d['p05_ns'], d['p95_ns']), (3, 10.0, 5.5, 14.5))
        self.assertEqual(b.distribution([])['median_ns'], None)


if __name__ == '__main__':
    unittest.main()
