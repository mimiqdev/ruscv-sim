"""Bounded public-CLI guest controls through accounting.execute; no oracle edits."""
import json
from pathlib import Path
import subprocess

from accounting import execute
from inventory import sha256


def main():
    root = Path.cwd()
    directory = root / ".a5/evidence/controls"
    directory.mkdir(parents=True, exist_ok=True)
    programs = {"pass": "RVMODEL_HALT_PASS", "guest-fail": "RVMODEL_HALT_FAIL",
                "cycle-timeout": "1: j 1b", "unsupported": ".word 0xffffffff"}
    for name, body in programs.items():
        source = directory / f"{name}.S"
        source.write_text('#include "rvmodel_macros.h"\n.section .text.init\n'
                          '.global rvtest_entry_point\nrvtest_entry_point:\n' + body +
                          '\nRVMODEL_DATA_SECTION\n')
        command = ["riscv64-unknown-elf-gcc", "-march=rv64i", "-mabi=lp64", "-nostdlib",
                   "-I", str(root / "scripts/a5"), "-T", str(root / "scripts/a5/link.ld"),
                   str(source), "-o", str(directory / f"{name}.elf")]
        completed = subprocess.run(command, capture_output=True, text=True, timeout=60)
        (directory / f"{name}.build.txt").write_text(json.dumps(command) + "\n" + completed.stdout + completed.stderr)
        completed.check_returncode()
    (directory / "malformed.elf").write_bytes(b"not an ELF")
    results = []
    for name in [*programs, "malformed", "missing"]:
        elf = directory / f"{name}.elf"
        outcome = execute(root / "target/release/ruscv-sim", elf, directory / name, max_cycles=1000)
        results.append({"control": name, "sha256": sha256(elf) if elf.exists() else None, **outcome})
    expected = ["guest-pass", "guest-fail"] + ["simulator-or-runner-failure"] * 4
    success = [r["classification"] for r in results] == expected and results[2]["reason"] == "cycle-timeout"
    (directory / "results.json").write_text(json.dumps({"success": success, "results": results}, indent=2) + "\n")
    if not success:
        raise SystemExit("negative control mismatch")


if __name__ == "__main__":
    main()
