"""Audit linked instructions and run intact/corrupt ACT4 ELFs via the public CLI."""
import hashlib
import json
import pathlib
import re
import struct
import subprocess

from cli_result import classify
from linked_audit import audit

root = pathlib.Path.cwd()
evidence = root / ".a5/evidence"
elfs = list((root / ".a5/work").glob("**/elfs/**/*.elf"))
assert len(elfs) == 1, elfs
elf = elfs[0]
symbols = audit(elf, evidence)
# Flip the first actual expected result, after the initial signature canary.
# Find signature_base using the linked symbol table, then map its VA through
# PT_LOAD rather than guessing file offsets. Code and exit hooks stay intact.
match = re.search(r"^([0-9a-f]+) \w signature_base$", symbols, re.M)
assert match, "Missing signature_base"
address = int(match[1], 16) + 8
data = bytearray(elf.read_bytes())
assert data[:6] == b"\x7fELF\x02\x01"
phoff = struct.unpack_from("<Q", data, 32)[0]
phsize, phnum = struct.unpack_from("<HH", data, 54)
offsets = []
for i in range(phnum):
    kind, flags, offset, va, pa, filesz, memsz, align = struct.unpack_from("<IIQQQQQQ", data, phoff + i * phsize)
    if kind == 1 and va <= address < va + filesz:
        offsets.append(offset + address - va)
assert len(offsets) == 1, offsets
offset = offsets[0]
old = data[offset]
data[offset] ^= 1
control = evidence / "I-add-00.corrupt.elf"
control.write_bytes(data)
results = []
for name, path in [("intact", elf), ("corrupt", control)]:
    command = [str(root / "target/release/ruscv-sim"), "run", str(path), "--max-cycles", "1000000"]
    timed_out = False
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        stdout, stderr = result.stdout, result.stderr
        rc = result.returncode
    except subprocess.TimeoutExpired as error:
        # TimeoutExpired may carry bytes even with text=True.
        def text(value):
            return value.decode(errors="replace") if isinstance(value, bytes) else value or ""
        stdout, stderr, rc = text(error.stdout), text(error.stderr), None
        timed_out = True
    except OSError as error:
        stdout, stderr, rc = "", str(error), None
    (evidence / f"{name}.txt").write_text(stdout + stderr)
    (evidence / f"{name}.stdout.txt").write_text(stdout)
    (evidence / f"{name}.stderr.txt").write_text(stderr)
    outcome = classify(stdout, stderr, rc, timed_out=timed_out)
    results.append({"case": name, "command": command, "returncode": rc, **outcome,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
report = {"mutation": {"address": hex(address), "offset": offset, "before": old, "after": data[offset]},
          "results": results}
(evidence / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
assert [r["classification"] for r in results] == ["guest-pass", "guest-fail"], report
