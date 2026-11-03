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

Floating-point encodings are likewise opt-in. `allowed_fp` extends the
32-bit opcode set with LOAD-FP, STORE-FP, OP-FP and R4, and the mnemonic set
with exactly the F and D instruction names plus the six Zicsr CSR forms the
ACT4 FP tests use to set `mstatus.FS` and clear `fflags`. The default rejects
all of them, so the RV64I smoke and Zca audits are unchanged.
"""
import bisect
import json
import re
import subprocess

# LOAD-FP, STORE-FP, MADD/MSUB/NMSUB/NMADD and OP-FP.
FP_OPCODES = {0x07, 0x27, 0x43, 0x47, 0x4b, 0x4f, 0x53}
# SYSTEM, restricted below to the six Zicsr CSR instruction mnemonics.
SYSTEM_OPCODE = 0x73
CSR_MNEMONICS = {"csrrw", "csrrs", "csrrc", "csrrwi", "csrrsi", "csrrci"}


def fp_mnemonics():
    """Every F and D instruction name, plus the Zicsr forms FP tests need.

    The set is enumerated rather than inferred from the opcode, so a mnemonic
    outside the selected profile still fails the audit closed.
    """
    names = {"flw", "fsw", "fld", "fsd"}
    for fmt in ("s", "d"):
        for operation in ("fadd", "fsub", "fmul", "fdiv", "fsqrt",
                          "fsgnj", "fsgnjn", "fsgnjx", "fmin", "fmax",
                          "feq", "flt", "fle", "fclass",
                          "fmadd", "fmsub", "fnmsub", "fnmadd"):
            names.add(f"{operation}.{fmt}")
        for width in ("w", "wu", "l", "lu"):
            names.add(f"fcvt.{width}.{fmt}")
            names.add(f"fcvt.{fmt}.{width}")
    names.update({"fcvt.s.d", "fcvt.d.s",
                  "fmv.x.w", "fmv.x.d", "fmv.w.x", "fmv.d.x"})
    return frozenset(names | CSR_MNEMONICS)


def audit(elf, evidence, *, allowed_compressed=None, allowed_fp=False):
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
    if allowed_fp:
        mnemonics |= fp_mnemonics()
        allowed |= FP_OPCODES | {SYSTEM_OPCODE}
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
