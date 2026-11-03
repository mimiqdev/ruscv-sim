"""Offline checks for the frozen Zca selection, plan and execution profile."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from accounting import (a5_allowed_compressed, execute, make_plan, summarize,
                        verify_plan)
from linked_audit import audit
from test_runner import PASS, FOOTER

MANIFEST = Path("docs/verification/a5-zca-source-inventory.json")
CONFIG = "ruscv-rv64imac-zca"


class ZcaEnv(unittest.TestCase):
    """Pin the accounting environment for one test and restore it after."""

    def setUp(self):
        self.previous = {key: os.environ.get(key)
                         for key in ("A5_CONFIG", "A5_MANIFEST", "A5_PLAN", "A5_RUN_ISA")}
        os.environ["A5_CONFIG"] = CONFIG
        os.environ["A5_MANIFEST"] = str(MANIFEST)
        os.environ.pop("A5_PLAN", None)
        os.environ.pop("A5_RUN_ISA", None)
        self.addCleanup(self.restore)

    def restore(self):
        for key, value in self.previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def zca_plan():
    return make_plan(json.loads(MANIFEST.read_text()))


class FrozenSelectionTests(unittest.TestCase):
    def test_selection_shape_and_pins(self):
        manifest = json.loads(MANIFEST.read_text())
        self.assertEqual(manifest["status"], "approved-frozen")
        self.assertEqual(manifest["act4_commit"], "a7c99303516f4e668f7488f172043392e23b9dfd")
        self.assertEqual(manifest["profile"], "a5-rv64imac-zca-nontrapping-proposal-v1")
        self.assertEqual(manifest["counts"], {"required-zca": 32})
        self.assertEqual(len(manifest["sources"]), 32)
        self.assertEqual(len({s["source"] for s in manifest["sources"]}), 32)
        for source in manifest["sources"]:
            self.assertTrue(source["source"].startswith("tests/rv64i/Zca/"), source)
            self.assertEqual(source["required_extensions"], ["I", "Zca"])
            self.assertEqual(source["params"], {"MXLEN": 64})
            self.assertEqual(source["march"], "rv64i_zicsr_zca")
            self.assertEqual(source["disposition"], "include")
            self.assertEqual(source["reason"], "required-zca")
        # Every other directory stays excluded: the manifest selects only Zca.
        self.assertTrue(all(s["source"].startswith("tests/rv64i/Zca/") for s in manifest["sources"]))
        self.assertEqual(len(manifest["environment_headers"]), 11)

    def test_udb_config_declares_only_i_and_zca(self):
        text = Path("scripts/a5/ruscv-rv64imac-zca.yaml").read_text()
        self.assertIn('{ name: I, version: "= 2.1.0" }', text)
        self.assertIn('{ name: Zca, version: "= 1.0.0" }', text)
        self.assertEqual(text.count("name: Zca"), 1)
        self.assertIn("MXLEN: 64", text)
        self.assertNotIn("arch_overlay", text)
        # The frozen RV64I config and its manifest stay untouched by U4.
        self.assertEqual(len(json.loads(Path("docs/verification/a5-source-inventory.json")
                                        .read_text())["sources"]), 1970)


class PlanTests(ZcaEnv):
    def test_zca_plan_is_exact_and_fail_closed(self):
        plan = zca_plan()
        self.assertEqual(plan["profile"], "a5-rv64imac-zca-nontrapping-proposal-v1")
        self.assertEqual(len(plan["variants"]), 32)
        self.assertTrue(all(v["elf"].startswith(f"{CONFIG}/elfs/rv64i/Zca/") for v in plan["variants"]))
        self.assertEqual(verify_plan(plan), plan["variants"])
        for mutated in (plan["variants"][:-1], plan["variants"] + plan["variants"][:1],
                        [{**plan["variants"][0], "elf": "other.elf"}] + plan["variants"][1:]):
            with self.subTest(count=len(mutated)), self.assertRaises(ValueError):
                verify_plan({**plan, "variants": mutated})

    def test_zca_plan_requires_every_generated_artifact(self):
        plan = zca_plan()
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            for variant in plan["variants"]:
                for key in ("elf", "signature_elf", "signature", "expected_results"):
                    path = work / variant[key]
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"fixture")
            results = [{"source": v["source"], "variant": v["variant"], "elf": v["elf"],
                        "classification": "guest-pass", "sha256": "0" * 64} for v in plan["variants"]]
            summary = summarize(plan, {"errors": [], "generated_selfcheck_elfs": 32,
                                       "variants": [{**v, "artifacts": {}} for v in plan["variants"]]},
                                results)
            self.assertFalse(summary["success"])  # missing artifact hashes fail closed


class ExecutionProfileTests(ZcaEnv):
    def test_run_isa_appends_the_selected_profile(self):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "sim"
            log = Path(directory) / "args.txt"
            script.write_text(f"#!{sys.executable}\nimport pathlib,sys\n"
                               f"pathlib.Path({str(log)!r}).write_text(' '.join(sys.argv[1:]))\n"
                               f"print({PASS!r})\n")
            script.chmod(0o755)
            result = execute(script, Path(directory) / "g.elf", Path(directory) / "d",
                             isa="rv64imac")
            self.assertEqual(result["classification"], "guest-pass")
            self.assertEqual(result["isa"], "rv64imac")
            self.assertIn("--isa rv64imac", log.read_text())
            execute(script, Path(directory) / "g.elf", Path(directory) / "d")
            self.assertNotIn("--isa", log.read_text())

    def test_compressed_audit_gate_is_zca_only(self):
        os.environ.pop("A5_CONFIG", None)
        allowed = a5_allowed_compressed()
        os.environ["A5_CONFIG"] = CONFIG
        zca = a5_allowed_compressed()
        self.assertEqual(allowed, {})
        self.assertEqual(zca["c.addi"], 1)
        self.assertEqual(zca["c.lw"], 0)
        self.assertEqual(zca["c.addi4spn"], 0)
        self.assertEqual(zca["c.addiw"], 1)
        self.assertEqual(zca["c.slli"], 2)
        self.assertNotIn("c.mop.s", zca)
        self.assertNotIn("c.fld", zca)
        self.assertNotIn("c.fsdsp", zca)

    def test_audit_accepts_only_allowed_parcels(self):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            objdump = "80000000: 0505 c.addi\n80000002: 4080 c.lw\n"
            symbols = "0000000080001000 d signature_base"
            def fake_run(command, **_):
                return symbols if "nm" in command[0] else objdump
            original = subprocess.check_output
            subprocess.check_output = fake_run
            try:
                audit("elf", evidence, allowed_compressed={"c.addi": 1, "c.lw": 0})
            finally:
                subprocess.check_output = original
            report = json.loads((evidence / "audit.json").read_text())
            self.assertEqual(report["compressed_parcels"], 2)
            self.assertEqual(report["unsupported"], [])


if __name__ == "__main__":
    unittest.main()
