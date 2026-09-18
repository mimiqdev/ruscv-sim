"""Focused tests for the A6 retained-evidence replay helpers."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).parent))
from replay_act4 import (  # noqa: E402
    FAILURE,
    FOOTER,
    HEADER,
    classify,
    generation_errors,
    result_errors,
)


PASS = f"""{HEADER}
Exit Code:  0
Cycles:     12
Final PC:   0x0000000080000030
Status:     SUCCESS
{FOOTER}
"""
FAIL = f"""{HEADER}
Exit Code:  1
Cycles:     12
Final PC:   0x0000000080000030
Status:     FAILED
{FOOTER}
"""


def plan_fixture():
    return {
        "variants": [
            {
                "source": "tests/rv64i/I/I-add-00.S",
                "variant": "rv64-selfcheck",
                "elf": "suite/a.elf",
                "signature_elf": "suite/a.sig.elf",
                "signature": "suite/a.sig",
                "expected_results": "suite/a.results",
            }
        ]
    }


def generated_fixture():
    return {
        "generation_returncode": 0,
        "errors": [],
        "variants": [
            {
                **plan_fixture()["variants"][0],
                "artifacts": {
                    "elf": {"sha256": "a" * 64, "bytes": 3},
                    "signature_elf": {"sha256": "b" * 64, "bytes": 4},
                    "signature": {"sha256": "c" * 64, "bytes": 5},
                    "expected_results": {"sha256": "d" * 64, "bytes": 6},
                },
            }
        ],
    }


def names_and_sizes():
    names = {
        "work/suite/a.elf",
        "work/suite/a.sig.elf",
        "work/suite/a.sig",
        "work/suite/a.results",
    }
    return names, {
        "work/suite/a.elf": 3,
        "work/suite/a.sig.elf": 4,
        "work/suite/a.sig": 5,
        "work/suite/a.results": 6,
    }


class ClassifierTests(unittest.TestCase):
    def test_strict_pass_and_guest_fail(self):
        self.assertEqual(classify(PASS, "", 0)["classification"], "guest-pass")
        self.assertEqual(classify(FAIL, "", 1)["classification"], "guest-fail")

    def test_rejects_ambiguous_or_contradictory_output(self):
        self.assertEqual(classify(PASS + PASS, "", 0)["classification"], FAILURE)
        self.assertEqual(classify(PASS.replace("SUCCESS", "FAILED"), "", 0)["classification"], FAILURE)
        self.assertEqual(classify(PASS, "stderr", 0)["reason"], "simulator-stderr")
        self.assertEqual(classify(PASS, "", 1)["reason"], "exit-code-returncode-mismatch")


class AccountingMutationTests(unittest.TestCase):
    def test_result_identity_mutations_fail_closed(self):
        plan = plan_fixture()
        generated = generated_fixture()
        good = [{
            "source": "tests/rv64i/I/I-add-00.S",
            "variant": "rv64-selfcheck",
            "elf": "suite/a.elf",
            "classification": "guest-pass",
            "sha256": "a" * 64,
        }]
        self.assertEqual(result_errors(plan, generated, good), [])
        for mutated in ([], good[:-1], good + [copy.deepcopy(good[0])], good + [{"elf": "extra.elf"}]):
            with self.subTest(size=len(mutated)):
                self.assertTrue(result_errors(plan, generated, mutated))

    def test_generation_mutations_fail_closed(self):
        plan = plan_fixture()
        generated = generated_fixture()
        names, sizes = names_and_sizes()
        self.assertEqual(generation_errors(plan, generated, names, sizes), [])
        for removed in names:
            with self.subTest(removed=removed):
                self.assertTrue(generation_errors(plan, generated, names - {removed}, sizes))
        for suffix in (".elf", ".sig", ".results"):
            extra = f"work/extra{suffix}"
            with self.subTest(extra=extra):
                self.assertTrue(
                    generation_errors(plan, generated, names | {extra}, {**sizes, extra: 1})
                )
        failed = copy.deepcopy(generated)
        failed["generation_returncode"] = 1
        self.assertTrue(generation_errors(plan, failed, names, sizes))


class ArchiveSafetyTests(unittest.TestCase):
    def test_zip_path_traversal_is_not_an_offline_input(self):
        from replay_act4 import archive_members

        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / "bad.zip"
            with zipfile.ZipFile(archive, "w") as bundle:
                bundle.writestr("../outside", b"bad")
            with zipfile.ZipFile(archive) as bundle:
                with self.assertRaises(ValueError):
                    archive_members(bundle)


if __name__ == "__main__":
    unittest.main()
