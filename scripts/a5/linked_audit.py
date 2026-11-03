"""Audit executable mapping-symbol regions, including startup and exit code.

The audit accepts 8-digit 32-bit words and 4-digit 16-bit parcels. A parcel is
allowed only when its mnemonic maps, in the caller's `allowed_compressed`
mapping, to the parcel's quadrant (bits [1:0] of the 16-bit halfword). The
default rejects every parcel, so the RV64I smoke audit is unchanged. A mapping
keyed by mnemonic, not quadrant alone, is what separates a Zca-only profile
from excluded neighbours: C.ADDI4SPN, C.LW, C.SW, C.LD and C.SD share quadrant
0, the Zcmop instructions share quadrant 1 with C.NOP/C.LI/C.LUI/C.ADDI, and
the FP compressed loads/stores share quadrants 0 and 2. A 4-digit word whose
quadrant is 3 is not a parcel at all and is reported unsupported.
"""
import bisect
import json
import re
import subprocess


def audit(elf, evidence, *, allowed_compressed=None):
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
    compressed = allowed_compressed or {}
    instructions, inline_data = [], []
    parcels = 0
    for address, word, op in re.findall(r"^\s*([0-9a-f]+):\s+([0-9a-f]{4,8})\s+(\S+)", objdump, re.M):
        index = bisect.bisect_right(addresses, int(address, 16)) - 1
        if index >= 0 and mapping[index][1]:
            inline_data.append((address, word, op))
        else:
            if len(word) == 4:
                parcels += 1
            instructions.append((word, op))
    # A word that is neither an 8-digit 32-bit encoding nor a 4-digit parcel
    # is malformed objdump output and fails closed, exactly as before.
    unsupported = [(word, op) for word, op in instructions if len(word) not in (4, 8)]
    unsupported += [(word, op) for word, op in instructions
                    if len(word) == 8 and (int(word, 16) & 0x7f not in allowed or op not in mnemonics)]
    for word, op in instructions:
        if len(word) == 4:
            parcel = int(word, 16)
            if parcel & 3 == 3 or compressed.get(op) != parcel & 3:
                unsupported.append((word, op))
    report = {"instructions": len(instructions), "unsupported": unsupported,
              "compressed_parcels": parcels,
              "mapped_inline_data_words": len(inline_data),
              "mnemonics": sorted({op for word, op in instructions})}
    (evidence / "audit.json").write_text(json.dumps(report, indent=2) + "\n")
    if not instructions or unsupported:
        raise ValueError(f"empty or unsupported linked instruction audit: {report}")
    return symbols
