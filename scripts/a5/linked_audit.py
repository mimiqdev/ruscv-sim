"""Audit executable mapping-symbol regions, including startup and exit code."""
import bisect
import json
import re
import subprocess


def audit(elf, evidence):
    evidence.mkdir(parents=True, exist_ok=True)
    objdump = subprocess.check_output(
        ["riscv64-unknown-elf-objdump", "-d", "-M", "no-aliases", str(elf)], text=True)
    symbols = subprocess.check_output(
        ["riscv64-unknown-elf-nm", "--special-syms", "-n", str(elf)], text=True)
    (evidence / "linked.objdump").write_text(objdump)
    (evidence / "symbols.txt").write_text(symbols)
    mapping = sorted((int(address, 16), name.startswith("$d")) for address, name in
                     re.findall(r"^([0-9a-f]+) \w (\$[dx]\S*)$", symbols, re.M))
    addresses = [address for address, _ in mapping]
    mnemonics = set("""lui auipc jal jalr beq bne blt bge bltu bgeu
lb lh lw ld lbu lhu lwu sb sh sw sd
addi slti sltiu xori ori andi slli srli srai
add sub sll slt sltu xor srl sra or and
addiw slliw srliw sraiw addw subw sllw srlw sraw fence fence.tso""".split())
    allowed = {0x03, 0x0f, 0x13, 0x17, 0x1b, 0x23, 0x33, 0x37, 0x3b, 0x63, 0x67, 0x6f}
    instructions, inline_data = [], []
    for address, word, op in re.findall(r"^\s*([0-9a-f]+):\s+([0-9a-f]{4,8})\s+(\S+)", objdump, re.M):
        index = bisect.bisect_right(addresses, int(address, 16)) - 1
        if index >= 0 and mapping[index][1]:
            inline_data.append((address, word, op))
        else:
            instructions.append((word, op))
    unsupported = [(word, op) for word, op in instructions
                   if len(word) != 8 or int(word, 16) & 0x7f not in allowed or op not in mnemonics]
    report = {"instructions": len(instructions), "unsupported": unsupported,
              "mapped_inline_data_words": len(inline_data),
              "mnemonics": sorted({op for word, op in instructions})}
    (evidence / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
    if not instructions or unsupported:
        raise ValueError(f"empty or unsupported linked instruction audit: {report}")
    return symbols
