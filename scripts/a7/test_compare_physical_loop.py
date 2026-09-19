#!/usr/bin/env python3
"""Focused negative tests for the public-facade comparison assertions."""
from __future__ import annotations

from copy import deepcopy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import compare_physical_loop as harness  # noqa: E402


def parsed_result() -> dict:
    verification = {
        **harness.EXPECTED_RESULT,
        "elapsed_ns": 100,
        "commit_records": harness.EXPECTED_RESULT["cycles"],
    }
    sample = {**harness.EXPECTED_RESULT, "index": 0, "elapsed_ns": 90}
    return {"verification": verification, "samples": [sample]}


class ComparisonAssertionTests(unittest.TestCase):
    def test_each_logged_result_field_mismatch_is_rejected(self) -> None:
        replacements = {
            "exit_code": 1,
            "cycles": harness.EXPECTED_RESULT["cycles"] + 1,
            "final_pc": harness.EXPECTED_RESULT["final_pc"] + 4,
            "timed_out": True,
            "error": "changed",
        }
        for field, value in replacements.items():
            with self.subTest(field=field):
                parsed = parsed_result()
                parsed["verification"][field] = value
                with self.assertRaisesRegex(harness.HarnessError, "known fixture result changed"):
                    harness.validate_run_result(parsed, "fixture", 1)

    def test_each_sample_result_field_mismatch_is_rejected(self) -> None:
        replacements = {
            "exit_code": 1,
            "cycles": harness.EXPECTED_RESULT["cycles"] + 1,
            "final_pc": harness.EXPECTED_RESULT["final_pc"] + 4,
            "timed_out": True,
            "error": "changed",
        }
        for field, value in replacements.items():
            with self.subTest(field=field):
                parsed = parsed_result()
                parsed["samples"][0][field] = value
                with self.assertRaisesRegex(harness.HarnessError, "sample result differs"):
                    harness.validate_run_result(parsed, "fixture", 1)

    def test_cross_revision_result_tuple_mismatch_is_rejected(self) -> None:
        baseline = parsed_result()["verification"]
        for field, value in {
            "exit_code": 1,
            "cycles": harness.EXPECTED_RESULT["cycles"] + 1,
            "final_pc": harness.EXPECTED_RESULT["final_pc"] + 4,
            "timed_out": True,
            "error": "changed",
        }.items():
            with self.subTest(field=field):
                implementation = deepcopy(baseline)
                implementation[field] = value
                observations = [{"verification": baseline}, {"verification": implementation}]
                with self.assertRaisesRegex(harness.HarnessError, "baseline and implementation"):
                    harness.validate_cross_revision_results(observations)

    def test_known_fixture_result_is_accepted(self) -> None:
        parsed = parsed_result()
        self.assertEqual(harness.validate_run_result(parsed, "fixture", 1), parsed["verification"])


if __name__ == "__main__":
    unittest.main()
