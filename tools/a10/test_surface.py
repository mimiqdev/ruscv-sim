#!/usr/bin/env python3
"""Public-surface controls: deleted calibrated machinery stays deleted."""
import json
import subprocess
import unittest
from pathlib import Path
from collect import ROOT


class PublicSurface(unittest.TestCase):
    def test_schema_has_no_calibrated_branch_or_identities(self):
        schema = json.loads((ROOT / 'tools/a10/ruscv-perf-1.schema.json').read_text())
        self.assertNotIn('oneOf', schema)
        self.assertNotIn('calibrated_report', schema['$defs'])
        self.assertNotIn('calibration-workloads', schema['$defs'].get('smoke_report', schema).get('properties', {}).get('workload', {'type': 'object'}).get('const', ''))
        text = (ROOT / 'tools/a10/ruscv-perf-1.schema.json').read_text()
        self.assertNotIn('calibration-workloads/1', text)
        self.assertNotIn('a10-calibration-oracle', text)
        # Inheritance/caller defs stay: smoke CLI rows carry their provenance.
        self.assertIn('inheritance', schema['$defs'])

    def test_wrapper_header_lists_only_smoke_baseline_validate(self):
        header = (ROOT / 'scripts/perf-test.sh').read_text().splitlines()[:3]
        joined = '\n'.join(header)
        for gone in ('calibrated', 'compare', 'cohort'):
            self.assertNotIn(gone, joined)

    def test_removed_subcommands_fail_inconclusive_without_side_effects(self):
        surface = [
            ['run', '--suite', 'public-v1', '--profile', 'calibrated', '--out', 'target/a10-surface-calibrated'],
            ['compare'],
            ['cohort', '--out', 'target/a10-surface-cohort'],
        ]
        for argv in surface:
            completed = subprocess.run(['./scripts/perf-test.sh'] + argv, cwd=ROOT, capture_output=True, text=True)
            self.assertEqual(completed.returncode, 2, argv)
            self.assertIn('INCONCLUSIVE', completed.stderr + completed.stdout)
        self.assertFalse((ROOT / 'target/a10-surface-calibrated').exists())
        self.assertFalse((ROOT / 'target/a10-surface-cohort').exists())


if __name__ == '__main__':
    unittest.main()
