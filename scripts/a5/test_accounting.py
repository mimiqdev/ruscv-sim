"""Local negative controls; fake processes test orchestration, not ISA support."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

from accounting import execute, exact, generation, make_plan, summarize, verify_plan
from inventory import MANIFEST, disposition, metadata, selected
from test_runner import FAIL, PASS, FOOTER


class InventoryTests(unittest.TestCase):
    def test_profile_identity_and_pending_decisions(self):
        profile = json.loads(Path("scripts/a5/profile-proposal.json").read_text())
        manifest = json.loads(MANIFEST.read_text())
        self.assertEqual(profile["status"], "proposed-not-frozen")
        self.assertEqual(profile["id"], manifest["profile"])
        self.assertEqual(profile["selection"]["proposed_included_sources"], len(selected(manifest)))
        self.assertEqual(len(profile["selection"]["freeze_decisions_required"]), 2)

    def test_complete_inventory(self):
        inventory = json.loads(MANIFEST.read_text())
        sources = inventory["sources"]
        self.assertEqual(len(sources), 1970)
        self.assertEqual(len({s["source"] for s in sources}), 1970)
        self.assertEqual(sum(inventory["counts"].values()), len(sources))
        self.assertEqual(len(selected(inventory)), 51)
        self.assertIn("tests/rv64i/I/I-fence-00.S", selected(inventory))
        misaligned = [s for s in sources if s["reason"] == "misaligned-success-environment"]
        self.assertEqual(len(misaligned), 8)
        self.assertTrue(all(s["params"]["MISALIGNED_LDST"] is True for s in misaligned))
        for source in sources:
            action, reason, consequence = disposition(source["source"], source["required_extensions"], source["params"])
            self.assertEqual((action, reason, consequence),
                             (source["disposition"], source["reason"], source["coverage_consequence"]))

    def test_header_rejections(self):
        valid = "START_TEST_CONFIG\n# REQUIRED_EXTENSIONS: ['I']\n# params:\n#   MXLEN: 64\n# MARCH: rv64i_zicsr\nEND_TEST_CONFIG"
        self.assertEqual(metadata(valid), (["I"], {"MXLEN": 64}, "rv64i_zicsr"))
        for bad in ["", valid + valid, valid.replace("#   MXLEN: 64", "#   MXLEN: 64\n#   MXLEN: 64"),
                    valid.replace("['I']", "['I', 'I']"), valid.replace("# params:", "# unknown: true")]:
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                metadata(bad)
        with self.assertRaises(ValueError):
            disposition("tests/rv64i/New/new.S", ["I"], {"MXLEN": 64})


class AccountingTests(unittest.TestCase):
    def setUp(self):
        self.plan = make_plan(json.loads(MANIFEST.read_text()))
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.work = Path(self.directory.name)
        for variant in self.plan["variants"]:
            for key in ("elf", "signature_elf", "signature", "expected_results"):
                path = self.work / variant[key]
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"fixture-not-an-actual-ELF")

    def good(self):
        generated = generation(self.plan, self.work, 0)
        results = [{"elf": v["elf"], "classification": "guest-pass",
                    "sha256": v["artifacts"]["elf"]["sha256"]} for v in generated["variants"]]
        return generated, results

    def test_positive_complete_accounting(self):
        generated, results = self.good()
        self.assertTrue(summarize(self.plan, generated, results)["success"])
        self.assertEqual(generated["generated_selfcheck_elfs"], 51)
        self.assertEqual(generated["generated_signature_elfs"], 51)

    def test_empty_missing_duplicate_extra_plan_and_results(self):
        for variants in [[], self.plan["variants"][:-1], self.plan["variants"] * 2,
                         self.plan["variants"] + [{**self.plan["variants"][0], "elf": "extra.elf"}]]:
            changed = {**self.plan, "variants": variants}
            with self.subTest(variants=len(variants)), self.assertRaises(ValueError):
                verify_plan(changed)
        generated, results = self.good()
        for bad in [[], results[:-1], results * 2, results + [{"elf": "extra.elf", "classification": "guest-pass"}]]:
            with self.subTest(results=len(bad)):
                self.assertFalse(summarize(self.plan, generated, bad)["success"])

    def test_generation_failure_cannot_pass_even_with_all_outputs(self):
        _, results = self.good()
        for rc in [1, -9, None, False]:
            with self.subTest(rc=rc):
                generated = generation(self.plan, self.work, rc)
                self.assertFalse(summarize(self.plan, generated, results)["success"])

    def test_missing_empty_extra_generation(self):
        first = self.plan["variants"][0]
        for key in ("elf", "signature_elf", "signature", "expected_results"):
            path = self.work / first[key]
            original = path.read_bytes()
            for operation in ("missing", "empty"):
                with self.subTest(key=key, operation=operation):
                    if operation == "missing":
                        path.unlink()
                    else:
                        path.write_bytes(b"")
                    self.assertTrue(generation(self.plan, self.work, 0)["errors"])
                    path.write_bytes(original)
        (self.work / "extra.elf").write_bytes(b"extra")
        self.assertTrue(generation(self.plan, self.work, 0)["errors"])
        with self.assertRaises(ValueError):
            exact(["a"], ["a", "a"], "duplicate generation")

    def test_failure_and_hash_mismatch_cannot_pass(self):
        generated, results = self.good()
        for classification in ["guest-fail", "simulator-or-runner-failure", None, "unknown"]:
            changed = copy.deepcopy(results)
            changed[0]["classification"] = classification
            self.assertFalse(summarize(self.plan, generated, changed)["success"])
        results[0]["sha256"] = "0" * 64
        self.assertFalse(summarize(self.plan, generated, results)["success"])

    def test_same_execution_path_negative_processes(self):
        script = self.work / "simulator"
        cases = [
            (PASS, "", 0, "guest-pass"),
            (FAIL, "", 1, "guest-fail"),
            (FAIL.replace(FOOTER, "Error:      Unsupported instruction\n" + FOOTER), "", 1,
             "simulator-or-runner-failure"),
            (PASS, "Error: simulator failed", 0, "simulator-or-runner-failure"),
            (PASS.replace("SUCCESS", "TIMEOUT"), "", 0, "simulator-or-runner-failure"),
            ("", "Error: malformed ELF", 1, "simulator-or-runner-failure"),
        ]
        for stdout, stderr, rc, expected in cases:
            script.write_text(f"#!{sys.executable}\nimport sys\nprint({stdout!r})\nsys.stderr.write({stderr!r})\nsys.exit({rc})\n")
            script.chmod(0o755)
            result = execute(script, self.work / "missing.elf", self.work / "diagnostics")
            self.assertEqual(result["classification"], expected)
        script.write_text(f"#!{sys.executable}\nimport time\ntime.sleep(10)\n")
        result = execute(script, self.work / "missing.elf", self.work / "diagnostics", timeout=0.02)
        self.assertEqual(result["reason"], "host-timeout")
        result = execute(self.work / "no-simulator", self.work / "missing.elf", self.work / "diagnostics")
        self.assertEqual(result["classification"], "simulator-or-runner-failure")


if __name__ == "__main__":
    unittest.main()
