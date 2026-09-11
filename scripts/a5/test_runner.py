"""Local stdlib tests for experiment classification, audit and byte mutation."""
import json
import os
import pathlib
import struct
import subprocess
import sys
import tempfile
import unittest

from cli_result import classify, FAILURE, HEADER, FOOTER

RUNNER = pathlib.Path(__file__).with_name("run.py").resolve()

# Verbatim host blocks from artifact 10271109348 intact.txt / corrupt.txt.
PASS = f"""{HEADER}
Exit Code:  0
Cycles:     4327
Final PC:   0x0000000080015038
Status:     SUCCESS
{FOOTER}
"""
FAIL = f"""{HEADER}
Exit Code:  1
Cycles:     1130
Final PC:   0x0000000080015050
Status:     FAILED
{FOOTER}
"""


class ClassificationTests(unittest.TestCase):
    def test_actual_pass_and_fail(self):
        for output, rc, expected in [(PASS, 0, "guest-pass"), (FAIL, 1, "guest-fail")]:
            with self.subTest(expected=expected):
                self.assertEqual(classify(output, "", rc)["classification"], expected)

    def test_guest_console_is_not_host_result(self):
        console = "Status:     FAILED\nError: guest diagnostic\nCycles: nonsense\n"
        self.assertEqual(classify(console + PASS, "", 0)["classification"], "guest-pass")
        self.assertEqual(classify(console, "", 0)["classification"], FAILURE)

    def test_missing_duplicate_and_malformed_fields(self):
        for output, rc in [(PASS, 0), (FAIL, 1)]:
            for line in output.splitlines():
                for replacement in ["", line + "\n" + line, line + " junk"]:
                    with self.subTest(line=line, replacement=replacement, rc=rc):
                        changed = output.replace(line + "\n", replacement + "\n")
                        self.assertEqual(classify(changed, "", rc)["classification"], FAILURE)

    def test_contradictions_and_consistency(self):
        cases = [
            (PASS.replace("SUCCESS", "SUCCESS\nStatus:     FAILED"), "", 0),
            (FAIL.replace("FAILED", "FAILED\nStatus:     SUCCESS"), "", 1),
            (PASS.replace("SUCCESS", "FAILED"), "", 0),
            (FAIL.replace("FAILED", "SUCCESS"), "", 1),
            (PASS, "", 1), (FAIL, "", 0), (PASS, "", -9),
            (PASS, "", None), (PASS, "Error: load failure\n", 0),
            ("", "Error: Execution failed: invalid ELF\n", 1),
            (FAIL.replace(FOOTER, "Error:      decode failure\n" + FOOTER), "", 1),
            (PASS.replace(FOOTER, "Error:      decode failure\n" + FOOTER), "", 0),
            (FAIL.replace("FAILED", "TIMEOUT"), "", 1),
            (PASS + PASS, "", 0), (PASS + FAIL, "", 1),
            (PASS + "Status:     FAILED\n", "", 0),
            (PASS.replace("4327", "-1"), "", 0),
            (PASS.replace("4327", "1.0"), "", 0),
            (PASS.replace("4327", "1000001"), "", 0),
            (PASS.replace("4327", str(2**64)), "", 0),
            (PASS.replace("4327", "9" * 5000), "", 0),
            (FAIL.replace("Code:  1", "Code:  4294967296"), "", 1),
            (FAIL.replace("Code:  1", "Code:  256"), "", 0),
            (PASS.replace("0x0000000080015038", "0x10000000080015038"), "", 0),
            (PASS.replace("0x0000000080015038", "not-a-pc"), "", 0),
            (PASS.replace("SUCCESS", "UNKNOWN"), "", 0),
        ]
        for output, stderr, rc in cases:
            with self.subTest(output=output[:200], stderr=stderr, rc=rc):
                self.assertEqual(classify(output, stderr, rc)["classification"], FAILURE)
        self.assertEqual(classify(PASS, "", 0, timed_out=True)["reason"], "host-timeout")

    def test_optional_signature_and_other_guest_exit_code(self):
        signature = "Signature:  0x0000000080013970 (8 bytes)\n"
        self.assertEqual(classify(PASS.replace(FOOTER, signature + FOOTER), "", 0)
                         ["classification"], "guest-pass")
        self.assertEqual(classify(FAIL.replace("Code:  1", "Code:  2"), "", 2)
                         ["classification"], "guest-fail")
        for value in [signature + signature, signature.replace("8 bytes", "-1 bytes")]:
            self.assertEqual(classify(PASS.replace(FOOTER, value + FOOTER), "", 0)
                             ["classification"], FAILURE)


class RunnerTests(unittest.TestCase):
    def probe(self, failure=False, instruction="00000013 addi", mappings="", output_edit=""):
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
                "bin/riscv64-unknown-elf-objdump": f"print({('80000000: ' + instruction)!r})",
                "bin/riscv64-unknown-elf-nm": f"print({('0000000080001000 d signature_base' + mappings)!r})",
                "target/release/ruscv-sim": (
                    "import pathlib,sys\n"
                    "bad = pathlib.Path(sys.argv[2]).read_bytes()[200] != 0\n"
                    f"output = {FAIL!r} if bad else {PASS!r}\n"
                    + (f"output = output.replace({FOOTER!r}, 'Error:      decode failure\\n' + {FOOTER!r})\n"
                       if failure else "")
                    + output_edit
                    + "print(output)\n"
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

    def test_runner_rejects_contradictory_and_incomplete_results(self):
        for edit in [
            f"output = output.replace({FOOTER!r}, 'Status:     SUCCESS\\nStatus:     FAILED\\n' + {FOOTER!r})\n",
            "output = '\\n'.join(line for line in output.splitlines() if not line.startswith('Cycles:'))\n",
        ]:
            with self.subTest(edit=edit):
                result, report, _ = self.probe(output_edit=edit)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual([r["classification"] for r in report["results"]],
                                 [FAILURE, FAILURE])

    def test_m_instruction_not_accepted_as_base_opcode(self):
        result, report, audit = self.probe(instruction="02000033 mul")
        self.assertNotEqual(result.returncode, 0)
        self.assertIsNone(report)
        self.assertTrue(audit["unsupported"])

    def test_compressed_instruction_rejected(self):
        result, _, audit = self.probe(instruction="0001 c.nop")
        self.assertNotEqual(result.returncode, 0)
        self.assertTrue(audit["unsupported"])

    def test_embedded_data_requires_mapping_symbol(self):
        instruction = "00000013 addi\n80000004: deadbeef .word\n80000008: 00000013 addi"
        result, _, _ = self.probe(instruction=instruction)
        self.assertNotEqual(result.returncode, 0)
        result, _, audit = self.probe(
            instruction=instruction,
            mappings="\n80000000 t $x\n80000004 t $d\n80000008 t $x",
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(audit["mapped_inline_data_words"], 1)
        self.assertEqual(audit["instructions"], 2)


if __name__ == "__main__":
    unittest.main()
