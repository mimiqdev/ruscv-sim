"""Audit linked instructions and run intact/corrupt ACT4 ELFs via the public CLI."""
import hashlib
import json
import pathlib
import re
import struct
import subprocess

root = pathlib.Path.cwd()
evidence = root / ".a5/evidence"
elfs = list((root / ".a5/work").glob("**/elfs/**/*.elf"))
assert len(elfs) == 1, elfs
elf = elfs[0]
objdump = subprocess.check_output(["riscv64-unknown-elf-objdump", "-d", "-M", "no-aliases", str(elf)], text=True)
(evidence / "linked.objdump").write_text(objdump)
# Strict base-I opcode audit includes startup and failure-reporting code.
allowed = {0x03, 0x0f, 0x13, 0x17, 0x1b, 0x23, 0x33, 0x37, 0x3b, 0x63, 0x67, 0x6f}
instructions = re.findall(r"^\s*[0-9a-f]+:\s+([0-9a-f]{4,8})\s+(\S+)", objdump, re.M)
assert instructions, "No disassembly"
unsupported = [(word, op) for word, op in instructions if len(word) != 8 or int(word, 16) & 0x7f not in allowed]
(evidence / "audit.json").write_text(json.dumps({"instructions": len(instructions), "unsupported": unsupported}, indent=2))
assert not unsupported, unsupported
# Flip the first actual expected result, after the initial signature canary.
# Find signature_base using the linked symbol table, then map its VA through
# PT_LOAD rather than guessing file offsets. Code and exit hooks stay intact.
symbols = subprocess.check_output(["riscv64-unknown-elf-nm", str(elf)], text=True)
(evidence / "symbols.txt").write_text(symbols)
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
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        output = result.stdout + result.stderr
        rc = result.returncode
    except subprocess.TimeoutExpired:
        output, rc = "Host timeout", None
    (evidence / f"{name}.txt").write_text(output)
    status = ("guest-pass" if rc == 0 and "Status:     SUCCESS" in output
              else "guest-fail" if rc == 1 and "Status:     FAILED" in output and "Error:" not in output
              else "simulator-or-runner-failure")
    results.append({"case": name, "command": command, "returncode": rc, "classification": status,
                    "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
report = {"mutation": {"address": hex(address), "offset": offset, "before": old, "after": data[offset]},
          "results": results}
(evidence / "results.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))
assert [r["classification"] for r in results] == ["guest-pass", "guest-fail"], report
