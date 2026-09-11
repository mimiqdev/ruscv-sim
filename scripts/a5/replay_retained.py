"""Replay artifact 10271109348 without regenerating Sail or invoking binutils.

Usage: python3 scripts/a5/replay_retained.py .a5/retained
Build target/release/ruscv-sim first. Retained audit text is substituted only for
the two binutils queries; run.py still mutates the ELF and invokes the real CLI.
"""
import hashlib
import json
import os
import pathlib
import runpy
import subprocess
import sys
import tempfile
from unittest.mock import patch

from cli_result import classify

root = pathlib.Path(__file__).resolve().parents[2]
artifact = pathlib.Path(sys.argv[1]).resolve()
hashes = {
    "evidence/intact.txt": "e9173c66a4ff1bd2ed9b9efae5ecd2e655b20fbda94b2a98e150fc502fab6abf",
    "evidence/corrupt.txt": "68a5ed22ed4e26023713f5838c774a40720768cfc7ce7c03e0d8a71f005d9a1d",
    "evidence/linked.objdump": "aded7a3a009310f9f6f7cd234e5334f48f047c7421020d2c95ca21fada531d33",
    "evidence/symbols.txt": "3298141d7c2e7e4da511b27804692a7d5835d92f32bc51233505d1cfacadb887",
    "work/ruscv-rv64i-smoke/elfs/rv64i/I/I-add-00.elf":
        "56b1efd50a01eb92a4ac8d81e8beb8e90f30f9529c42fdd610dd1ebd4cadedd8",
    "evidence/I-add-00.corrupt.elf":
        "33b913f22dcfd625e1dfeb06c2e253c6531a5be2befee2514b675c66fd936d54",
}
for name, expected in hashes.items():
    assert hashlib.sha256((artifact / name).read_bytes()).hexdigest() == expected, name
elf = artifact / "work/ruscv-rv64i-smoke/elfs/rv64i/I/I-add-00.elf"
control = artifact / "evidence/I-add-00.corrupt.elf"
original, corrupt = elf.read_bytes(), control.read_bytes()
assert len(original) == len(corrupt)
assert [(i, a, b) for i, (a, b) in enumerate(zip(original, corrupt)) if a != b] == [
    (84344, 0xd9, 0xd8)]
for name, rc, expected in [("intact", 0, "guest-pass"), ("corrupt", 1, "guest-fail")]:
    # Original evidence merged streams. Fresh runner below retains them separately.
    result = classify((artifact / f"evidence/{name}.txt").read_text(), "", rc)
    assert result["classification"] == expected, result

with tempfile.TemporaryDirectory(dir=root / ".a5", prefix="replay-") as directory:
    workspace = pathlib.Path(directory)
    evidence = workspace / ".a5/evidence"
    evidence.mkdir(parents=True)
    (workspace / ".a5/work").symlink_to(artifact / "work", target_is_directory=True)
    (workspace / "target").symlink_to(root / "target", target_is_directory=True)

    def retained_binutils(command, **kwargs):
        assert command in [
            ["riscv64-unknown-elf-objdump", "-d", "-M", "no-aliases",
             str(workspace / ".a5/work/ruscv-rv64i-smoke/elfs/rv64i/I/I-add-00.elf")],
            ["riscv64-unknown-elf-nm", "--special-syms", "-n",
             str(workspace / ".a5/work/ruscv-rv64i-smoke/elfs/rv64i/I/I-add-00.elf")],
        ], command
        name = "linked.objdump" if "objdump" in command[0] else "symbols.txt"
        return (artifact / "evidence" / name).read_text()

    previous = pathlib.Path.cwd()
    try:
        os.chdir(workspace)
        with patch("subprocess.check_output", side_effect=retained_binutils):
            runpy.run_path(str(root / "scripts/a5/run.py"), run_name="__main__")
    finally:
        os.chdir(previous)
    report = json.loads((evidence / "results.json").read_text())
    for result, cycles, pc in zip(report["results"], [4327, 1130], ["38", "50"]):
        assert result["execution_result"]["cycles"] == cycles
        assert result["execution_result"]["final_pc"] == "0x00000000800150" + pc
    assert (evidence / "I-add-00.corrupt.elf").read_bytes() == corrupt
    # Retain fresh outputs/report after the temporary replay workspace disappears.
    destination = root / ".a5/replay-evidence"
    destination.mkdir(exist_ok=True)
    for path in evidence.iterdir():
        (destination / path.name).write_bytes(path.read_bytes())

missing = subprocess.run(
    [str(root / "target/release/ruscv-sim"), "run", str(root / ".a5/no-such-elf"),
     "--max-cycles", "1000000"], capture_output=True, text=True, timeout=60)
assert missing.returncode == 1
assert classify(missing.stdout, missing.stderr, missing.returncode)["classification"] == \
    "simulator-or-runner-failure"
(destination / "missing-elf.stdout.txt").write_text(missing.stdout)
(destination / "missing-elf.stderr.txt").write_text(missing.stderr)
print("Retained-output parsing, real-CLI runner replay, exact mutation and missing-ELF control passed.")
