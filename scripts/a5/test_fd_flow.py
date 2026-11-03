"""Offline checks for the frozen F/D and Zcd selections, plans and audit gates.

The Sail derivations are self-asserting: each one exits non-zero unless the
enabled extension set is exactly its selected profile, and the ACT4 runs that
consume the resulting `sail.json` prove the oracle. Nothing here needs the
pinned upstream checkout, so the ordinary unit-test job can run it.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from accounting import (a5_allowed_compressed, a5_allowed_fp, execute, make_plan,
                        summarize, verify_plan)
from linked_audit import audit
from test_runner import PASS

FD = Path("docs/verification/a5-fd-source-inventory.json")
ZCD = Path("docs/verification/a5-zcd-source-inventory.json")
FD_CONFIG = "ruscv-rv64imafd"
ZCD_CONFIG = "ruscv-rv64imafdc-zcd"
PIN = "a7c99303516f4e668f7488f172043392e23b9dfd"


class SelectionEnv(unittest.TestCase):
    """Pin the accounting environment for one test and restore it after."""

    config = FD_CONFIG
    manifest = FD

    def setUp(self):
        self.previous = {key: os.environ.get(key)
                         for key in ("A5_CONFIG", "A5_MANIFEST", "A5_PLAN", "A5_RUN_ISA")}
        os.environ["A5_CONFIG"] = self.config
        os.environ["A5_MANIFEST"] = str(self.manifest)
        os.environ.pop("A5_PLAN", None)
        os.environ.pop("A5_RUN_ISA", None)
        self.addCleanup(self.restore)

    def restore(self):
        for key, value in self.previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


class FrozenSelectionTests(unittest.TestCase):
    def test_fd_selection_shape_and_pins(self):
        manifest = json.loads(FD.read_text())
        self.assertEqual(manifest["status"], "approved-frozen")
        self.assertEqual(manifest["act4_commit"], PIN)
        self.assertEqual(manifest["profile"], "a5-rv64imafd-nontrapping-proposal-v1")
        self.assertEqual(manifest["counts"], {"required-d": 114, "required-f": 82})
        self.assertEqual(len(manifest["sources"]), 196)
        self.assertEqual(len({s["source"] for s in manifest["sources"]}), 196)
        self.assertEqual(len(manifest["environment_headers"]), 11)
        for source in manifest["sources"]:
            parts = Path(source["source"]).parts
            self.assertEqual(parts[:2], ("tests", "rv64i"))
            self.assertIn(parts[2], ("D", "F"))
            self.assertEqual(source["params"], {"MXLEN": 64})
            self.assertEqual(source["disposition"], "include")
            if parts[2] == "F":
                self.assertEqual(source["required_extensions"], ["F", "I"])
                self.assertEqual(source["march"], "rv64if_zicsr")
                self.assertEqual(source["reason"], "required-f")
            else:
                self.assertEqual(source["required_extensions"], ["D", "F", "I"])
                self.assertEqual(source["march"], "rv64ifd_zicsr")
                self.assertEqual(source["reason"], "required-d")

    def test_zcd_selection_shape_and_pins(self):
        manifest = json.loads(ZCD.read_text())
        self.assertEqual(manifest["status"], "approved-frozen")
        self.assertEqual(manifest["act4_commit"], PIN)
        self.assertEqual(manifest["profile"], "a5-rv64imafdc-zcd-nontrapping-proposal-v1")
        self.assertEqual(manifest["counts"], {"required-zcd": 4})
        self.assertEqual(len(manifest["sources"]), 4)
        self.assertEqual(len(manifest["environment_headers"]), 11)
        for source in manifest["sources"]:
            self.assertTrue(source["source"].startswith("tests/rv64i/Zcd/"), source)
            self.assertEqual(source["required_extensions"], ["D", "F", "I", "Zcd"])
            self.assertEqual(source["march"], "rv64ifd_zicsr_zcd")
            self.assertEqual(source["params"], {"MXLEN": 64})
            self.assertEqual(source["disposition"], "include")
            self.assertEqual(source["reason"], "required-zcd")

    def test_excluded_neighbours_are_absent(self):
        names = {s["source"] for s in json.loads(FD.read_text())["sources"]}
        names |= {s["source"] for s in json.loads(ZCD.read_text())["sources"]}
        self.assertFalse([n for n in names if Path(n).parts[2] not in ("D", "F", "Zcd")], sorted(names))
        # The natural-alignment neighbours share REQUIRED_EXTENSIONS with the
        # selected directories, so nothing may carry them in.
        for directory in ("MisalignF", "MisalignD", "ZfaF", "ZfaD", "ZfaZfh", "ZfaZfhD",
                          "ZfaZvfh", "Zfh", "ZfhD", "Zfhmin", "ZfhminD", "Zfbfmin"):
            self.assertFalse([n for n in names if f"/{directory}/" in n], directory)

    def test_udb_configs_declare_only_the_selected_extensions(self):
        fd = Path("scripts/a5/ruscv-rv64imafd.yaml").read_text()
        for line in ('{ name: I, version: "= 2.1.0" }', '{ name: Zicsr, version: "= 2.0.0" }',
                     '{ name: F, version: "= 2.2.0" }', '{ name: D, version: "= 2.2.0" }'):
            self.assertEqual(fd.count(line), 1, line)
        self.assertEqual(fd.count("{ name:"), 4)
        self.assertIn("MXLEN: 64", fd)
        self.assertNotIn("arch_overlay", fd)

        zcd = Path("scripts/a5/ruscv-rv64imafdc-zcd.yaml").read_text()
        for line in ('{ name: I, version: "= 2.1.0" }', '{ name: Zca, version: "= 1.0.0" }',
                     '{ name: Zicsr, version: "= 2.0.0" }', '{ name: F, version: "= 2.2.0" }',
                     '{ name: D, version: "= 2.2.0" }', '{ name: Zcd, version: "= 1.0.0" }'):
            self.assertEqual(zcd.count(line), 1, line)
        self.assertEqual(zcd.count("{ name:"), 6)
        self.assertIn("MXLEN: 64", zcd)
        self.assertNotIn("arch_overlay", zcd)

        # The frozen RV64I and Zca configs and manifests stay untouched.
        self.assertEqual(Path("scripts/a5/ruscv-rv64i-smoke.yaml").read_text().count("{ name:"), 1)
        self.assertEqual(Path("scripts/a5/ruscv-rv64imac-zca.yaml").read_text().count("{ name:"), 2)
        self.assertEqual(len(json.loads(Path("docs/verification/a5-source-inventory.json")
                                        .read_text())["sources"]), 1970)
        self.assertEqual(len(json.loads(Path("docs/verification/a5-zca-source-inventory.json")
                                        .read_text())["sources"]), 32)

    def test_fp_profiles_declare_floating_point_to_act4(self):
        # ACT4 enables mstatus.FS only for a DUT that declares F support, so
        # the FP selections must carry these defines and the non-FP ones must
        # not.
        fp = Path("scripts/a5/rvtest_config_fd.h").read_text()
        self.assertEqual(fp.count("#define F_SUPPORTED"), 1)
        self.assertEqual(fp.count("#define D_SUPPORTED"), 1)
        for name in ("rvtest_config.h",):
            self.assertNotIn("F_SUPPORTED", Path(f"scripts/a5/{name}").read_text())


class FdPlanTests(SelectionEnv):
    def test_fd_plan_is_exact_and_fail_closed(self):
        plan = make_plan(json.loads(FD.read_text()))
        self.assertEqual(plan["profile"], "a5-rv64imafd-nontrapping-proposal-v1")
        self.assertEqual(len(plan["variants"]), 196)
        self.assertTrue(all(v["elf"].startswith(f"{FD_CONFIG}/elfs/rv64i/") for v in plan["variants"]))
        self.assertTrue(all(v["elf"].split("/")[3] in ("D", "F") for v in plan["variants"]))
        self.assertEqual(verify_plan(plan), plan["variants"])
        for mutated in (plan["variants"][:-1], plan["variants"] + plan["variants"][:1],
                        [{**plan["variants"][0], "elf": "other.elf"}] + plan["variants"][1:]):
            with self.subTest(count=len(mutated)), self.assertRaises(ValueError):
                verify_plan({**plan, "variants": mutated})

    def test_fd_plan_requires_every_generated_artifact(self):
        plan = make_plan(json.loads(FD.read_text()))
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            for variant in plan["variants"]:
                for key in ("elf", "signature_elf", "signature", "expected_results"):
                    path = work / variant[key]
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b"fixture")
            results = [{"source": v["source"], "variant": v["variant"], "elf": v["elf"],
                        "classification": "guest-pass", "sha256": "0" * 64} for v in plan["variants"]]
            summary = summarize(plan, {"errors": [], "generated_selfcheck_elfs": 196,
                                       "variants": [{**v, "artifacts": {}} for v in plan["variants"]]},
                                results)
            self.assertFalse(summary["success"])  # missing artifact hashes fail closed


class ZcdPlanTests(SelectionEnv):
    config = ZCD_CONFIG
    manifest = ZCD

    def test_zcd_plan_is_exact_and_fail_closed(self):
        plan = make_plan(json.loads(ZCD.read_text()))
        self.assertEqual(plan["profile"], "a5-rv64imafdc-zcd-nontrapping-proposal-v1")
        self.assertEqual(len(plan["variants"]), 4)
        self.assertTrue(all(v["elf"].startswith(f"{ZCD_CONFIG}/elfs/rv64i/Zcd/")
                            for v in plan["variants"]))
        self.assertEqual(verify_plan(plan), plan["variants"])
        with self.assertRaises(ValueError):
            verify_plan({**plan, "variants": plan["variants"][:-1]})


class ExecutionProfileTests(SelectionEnv):
    def run_isa(self, isa):
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "sim"
            log = Path(directory) / "args.txt"
            script.write_text(f"#!{sys.executable}\nimport pathlib,sys\n"
                              f"pathlib.Path({str(log)!r}).write_text(' '.join(sys.argv[1:]))\n"
                              f"print({PASS!r})\n")
            script.chmod(0o755)
            result = execute(script, Path(directory) / "g.elf", Path(directory) / "d", isa=isa)
            self.assertEqual(result["classification"], "guest-pass")
            self.assertEqual(result["isa"], isa)
            self.assertIn(f"--isa {isa}", log.read_text())

    def test_fd_run_isa_appends_the_selected_profile(self):
        self.run_isa("rv64imafd")

    def test_zcd_run_isa_appends_the_selected_profile(self):
        os.environ["A5_CONFIG"] = ZCD_CONFIG
        os.environ["A5_MANIFEST"] = str(ZCD)
        self.run_isa("rv64imafdc")

    def test_fp_and_compressed_gates_are_profile_specific(self):
        os.environ.pop("A5_CONFIG", None)
        self.assertEqual(a5_allowed_fp(), False)
        self.assertEqual(a5_allowed_compressed(), {})
        os.environ["A5_CONFIG"] = "ruscv-rv64i-smoke"
        self.assertEqual(a5_allowed_fp(), False)
        os.environ["A5_CONFIG"] = "ruscv-rv64imac-zca"
        self.assertEqual(a5_allowed_fp(), False)
        self.assertNotIn("c.fld", a5_allowed_compressed())
        self.assertNotIn("c.fsdsp", a5_allowed_compressed())
        os.environ["A5_CONFIG"] = FD_CONFIG
        self.assertTrue(a5_allowed_fp())
        self.assertEqual(a5_allowed_compressed(), {})
        os.environ["A5_CONFIG"] = ZCD_CONFIG
        self.assertTrue(a5_allowed_fp())
        zcd = a5_allowed_compressed()
        self.assertEqual(zcd["c.fld"], 0)
        self.assertEqual(zcd["c.fsd"], 0)
        self.assertEqual(zcd["c.fldsp"], 2)
        self.assertEqual(zcd["c.fsdsp"], 2)
        # Zca stays allowed under rv64imafdc, Zcmop and Zcb stay out.
        self.assertEqual(zcd["c.addi"], 1)
        self.assertNotIn("c.mop.s", zcd)


class AuditGateTests(unittest.TestCase):
    def audit(self, objdump, **kwargs):
        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            symbols = "0000000080001000 d signature_base"

            def fake_run(command, **_):
                return symbols if "nm" in command[0] else objdump
            original = subprocess.check_output
            subprocess.check_output = fake_run
            try:
                audit("elf", evidence, **kwargs)
            finally:
                subprocess.check_output = original
            return json.loads((evidence / "audit.json").read_text())

    def test_fp_words_require_the_fp_gate(self):
        fp = "80000000: 0002a087 flw\n80000004: 003170d3 fadd.s\n"
        with self.assertRaises(ValueError):
            self.audit(fp)
        report = self.audit(fp, allowed_fp=True)
        self.assertEqual(report["unsupported"], [])
        self.assertEqual(report["mnemonics"], ["fadd.s", "flw"])

    def test_fp_gate_keeps_everything_else_fail_closed(self):
        # An out-of-profile FP format and a non-Zicsr SYSTEM instruction stay
        # unsupported even with the FP gate on.
        for word, op in (("12345053", "fadd.q"), ("00000073", "ecall"),
                         ("00100073", "ebreak"), ("30200073", "mret")):
            with self.subTest(op=op), self.assertRaises(ValueError):
                self.audit(f"80000000: {word} {op}\n", allowed_fp=True)
        report = self.audit("80000000: 34002073 csrrsi\n", allowed_fp=True)
        self.assertEqual(report["unsupported"], [])

    def test_zcd_parcels_require_the_compressed_mapping(self):
        parcels = "80000020: 2000 c.fld\n80000022: a000 c.fsd\n"
        with self.assertRaises(ValueError):
            self.audit(parcels, allowed_fp=True)
        report = self.audit(parcels, allowed_fp=True,
                            allowed_compressed={"c.fld": 0, "c.fsd": 0})
        self.assertEqual(report["compressed_parcels"], 2)
        self.assertEqual(report["unsupported"], [])


if __name__ == "__main__":
    unittest.main()
