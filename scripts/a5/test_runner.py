"""Local stdlib tests for experiment classification, audit and byte mutation."""
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest

RUNNER = pathlib.Path(__file__).with_name("run.py").resolve()


class RunnerTests(unittest.TestCase):
    def probe(self, failure=False, instruction="00000013 addi"):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            for name in [".a5/evidence", ".a5/work/smoke/elfs", "bin", "target/release"]:
                (root / name).mkdir(parents=True)
            data = bytearray(256)
            data[:6] = b"\x7fELF\x02\x01"
            struct.pack_into("<Q", data, 32, 64)
            struct.pack_into("<HH", data, 54, 56, 1)
            struct.pack_into("<IIQQQQQQ", data, 64, 1, 6, 192, 0x80001000,
                             0x80001000, 64, 64, 8)
            (root / ".a5/work/smoke/elfs/test.elf").write_bytes(data)
            tools = {
                "bin/riscv64-unknown-elf-objdump": f"print('80000000: {instruction}')",
                "bin/riscv64-unknown-elf-nm": "print('0000000080001000 d signature_base')",
                "target/release/ruscv-sim": (
                    "import pathlib,sys\n"
                    "bad = pathlib.Path(sys.argv[2]).read_bytes()[200] != 0\n"
                    "print('Status:     FAILED' if bad else 'Status:     SUCCESS')\n"
                    + ("print('Error:      decode failure')\n" if failure else "")
                    + "sys.exit(1 if bad else 0)\n"
                ),
            }
            for name, body in tools.items():
                path = root / name
                path.write_text(f"#!{sys.executable}\n{body}\n")
                path.chmod(0o755)
            result = subprocess.run([sys.executable, str(RUNNER)], cwd=root,
                                    env={**os.environ, "PATH": f"{root / 'bin'}:{os.environ['PATH']}"},
                                    capture_output=True, text=True)
            report = root / ".a5/evidence/results.json"
            audit = json.loads((root / ".a5/evidence/audit.json").read_text())
            return result, json.loads(report.read_text()) if report.exists() else None, audit

    def test_pass_fail_and_exact_mutation(self):
        result, report, _ = self.probe()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([r["classification"] for r in report["results"]], ["guest-pass", "guest-fail"])
        self.assertEqual(report["mutation"], {"address": "0x80001008", "offset": 200, "before": 0, "after": 1})

    def test_simulator_failure_is_not_guest_failure(self):
        result, report, _ = self.probe(failure=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(report["results"][1]["classification"], "simulator-or-runner-failure")

    def test_m_instruction_not_accepted_as_base_opcode(self):
        result, report, audit = self.probe(instruction="02000033 mul")
        self.assertNotEqual(result.returncode, 0)
        self.assertIsNone(report)
        self.assertTrue(audit["unsupported"])

    def test_compressed_instruction_rejected(self):
        result, _, audit = self.probe(instruction="0001 c.nop")
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(audit["unsupported"])


if __name__ == "__main__":
    unittest.main()
