#!/usr/bin/env python3
"""Independent Zca expansion-decoder reference generator (issue #132).

Classifies every 16-bit parcel as legal expansion / HINT / reserved /
FP-compressed illegal and emits one `parcel class expansion` TSV row each,
deterministically, on stdout.

This generator is deliberately written in a different style from
src/isa/rv64c/decoder_16bit.rs: each compressed encoding is described by a
declarative table row (which parcel bits select it, its class predicate,
and a generic immediate field map), and one evaluator walks the table,
instead of per-quadrant hand-written classify functions with custom shift
chains. Written from the RISC-V ISA Unprivileged Specification version
20250508 Chapter "C" and the Selectable Zca Compressed-Instruction Profile
contract (docs/dev-plan.md D2/D6, merge 9db1bed).
"""

from dataclasses import dataclass
from typing import Callable, Optional

OP_LOAD, OP_STORE = 0b0000011, 0b0100011
OP_OP_IMM, OP_OP_IMM_32 = 0b0010011, 0b0011011
OP_OP, OP_OP_32 = 0b0110011, 0b0111011
OP_LUI = 0b0110111
OP_BRANCH, OP_JALR, OP_JAL = 0b1100011, 0b1100111, 0b1101111
OP_SYSTEM = 0b1110011
F3_ADDSUB, F3_SLL, F3_SLT, F3_SLTU = 0, 1, 2, 3
F3_XOR, F3_SRLSRA, F3_OR, F3_AND = 4, 5, 6, 7

E, H, R, F = "E", "H", "R", "F"


def sext(value: int, bits: int) -> int:
    """Sign-extend `value` (assembled bits) from `bits` wide to 32 bits."""
    return (value | ~((1 << bits) - 1)) & 0xFFFFFFFF if value & (1 << (bits - 1)) else value


@dataclass
class ImmField:
    """imm[dest_hi:dest_lo] <- parcel[src_hi:src_lo]."""

    dest_hi: int
    dest_lo: int
    src_hi: int
    src_lo: int


def gather(parcel: int, fields: tuple) -> int:
    imm = 0
    for f in fields:
        width = f.src_hi - f.src_lo + 1
        imm |= (((parcel >> f.src_lo) & ((1 << width) - 1)) << f.dest_lo)
    return imm


# Immediate layouts as destination-bit <- parcel-bit maps from the spec's
# per-format diagrams.
I_ADDI4SPN = (  # nzuimm[5:4|9:6|3|2] <- p[12:11|10:7|5|6]
    ImmField(5, 4, 12, 11),
    ImmField(9, 6, 10, 7),
    ImmField(3, 3, 5, 5),
    ImmField(2, 2, 6, 6),
)
I_LW = (  # uimm[5:3|2|6] <- p[12:10|6|5]
    ImmField(5, 3, 12, 10),
    ImmField(2, 2, 6, 6),
    ImmField(6, 6, 5, 5),
)
I_LD = (  # uimm[5:3|7:6] <- p[12:10|6:5]
    ImmField(5, 3, 12, 10),
    ImmField(7, 6, 6, 5),
)
I_CI = (  # imm[5|4:0] <- p[12|6:2]
    ImmField(5, 5, 12, 12),
    ImmField(4, 0, 6, 2),
)
I_LUI = (  # nzimm[17|16:12] <- p[12|6:2]
    ImmField(17, 17, 12, 12),
    ImmField(16, 12, 6, 2),
)
I_ADDI16SP = (  # nzimm[9|4|6|8:7|5] <- p[12|6|5|4:3|2]
    ImmField(9, 9, 12, 12),
    ImmField(4, 4, 6, 6),
    ImmField(6, 6, 5, 5),
    ImmField(8, 7, 4, 3),
    ImmField(5, 5, 2, 2),
)
I_J = (  # imm[11|4|9:8|10|6|7|3:1|5] <- p[12|11|10:9|8|7|6|5:3|2]
    ImmField(11, 11, 12, 12),
    ImmField(4, 4, 11, 11),
    ImmField(9, 8, 10, 9),
    ImmField(10, 10, 8, 8),
    ImmField(6, 6, 7, 7),
    ImmField(7, 7, 6, 6),
    ImmField(3, 1, 5, 3),
    ImmField(5, 5, 2, 2),
)
I_B = (  # imm[8|4:3|7:6|2:1|5] <- p[12|11:10|6:5|4:3|2]
    ImmField(8, 8, 12, 12),
    ImmField(4, 3, 11, 10),
    ImmField(7, 6, 6, 5),
    ImmField(2, 1, 4, 3),
    ImmField(5, 5, 2, 2),
)
I_LWSP = (  # uimm[5|4:2|7:6] <- p[12|6:4|3:2]
    ImmField(5, 5, 12, 12),
    ImmField(4, 2, 6, 4),
    ImmField(7, 6, 3, 2),
)
I_LDSP = (  # uimm[5|4:3|8:6] <- p[12|6:5|4:2]
    ImmField(5, 5, 12, 12),
    ImmField(4, 3, 6, 5),
    ImmField(8, 6, 4, 2),
)
I_SWSP = (  # uimm[5:2|7:6] <- p[12:9|8:7]
    ImmField(5, 2, 12, 9),
    ImmField(7, 6, 8, 7),
)
I_SDSP = (  # uimm[5:3|8:6] <- p[12:10|9:7]
    ImmField(5, 3, 12, 10),
    ImmField(8, 6, 9, 7),
)


def i_type(op, rd, rs1, f3, imm):
    return ((imm & 0xFFF) << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | op


def r_type(op, rd, rs1, rs2, f3, f7):
    return (f7 << 25) | (rs2 << 20) | (rs1 << 15) | (f3 << 12) | (rd << 7) | op


def s_type(op, rs1, rs2, f3, imm):
    return (
        (((imm >> 5) & 0x7F) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (f3 << 12)
        | ((imm & 0x1F) << 7)
        | op
    )


def b_type(op, rs1, rs2, f3, imm):
    return (
        (((imm >> 12) & 1) << 31)
        | (((imm >> 5) & 0x3F) << 25)
        | (rs2 << 20)
        | (rs1 << 15)
        | (f3 << 12)
        | (((imm >> 1) & 0xF) << 8)
        | (((imm >> 11) & 1) << 7)
        | op
    )


def u_type(op, rd, imm):
    return (imm & 0xFFFFF000) | (rd << 7) | op


def j_type(op, rd, imm):
    return (
        (((imm >> 20) & 1) << 31)
        | (((imm >> 12) & 0xFF) << 12)
        | (((imm >> 11) & 1) << 20)
        | (((imm >> 1) & 0x3FF) << 21)
        | (rd << 7)
        | op
    )


def _rd(p):
    return (p >> 7) & 0x1F


def _rs2(p):
    return (p >> 2) & 0x1F


def _rdp(p):  # compact rd' field, x8..x15
    return 8 + ((p >> 2) & 7)


def _r1p(p):  # compact rs1' field, x8..x15
    return 8 + ((p >> 7) & 7)


def _r2p(p):
    return 8 + ((p >> 2) & 7)


def _sub(p):
    return (p >> 10) & 3


def _f2(p):
    return (p >> 5) & 3


def _b12(p):
    return (p >> 12) & 1


@dataclass
class Row:
    """One compressed encoding.

    sel:   (quadrant, funct3)
    match: extra parcel-bit conditions identifying this row within the slot
    kind:  (parcel, imm) -> 'E'|'H'|'R'|'F'
    build: (parcel, imm) -> 32-bit expansion, for E/H rows
    imm_layout / imm_bits: generic immediate assembly + sign width
    """

    sel: tuple
    name: str
    match: Callable[[int], bool]
    kind: Callable[[int, int], str]
    build: Optional[Callable[[int, int], int]] = None
    imm_layout: tuple = ()
    imm_bits: int = 0


ROWS = [
    # ---- Quadrant 0 ----
    Row((0b00, 0b000), "c.addi4spn", lambda p: True,
        lambda p, i: R if i == 0 else E,
        lambda p, i: i_type(OP_OP_IMM, _rdp(p), 2, F3_ADDSUB, i), I_ADDI4SPN),
    Row((0b00, 0b001), "c.fld", lambda p: True, lambda p, i: F),
    Row((0b00, 0b010), "c.lw", lambda p: True, lambda p, i: E,
        lambda p, i: i_type(OP_LOAD, _rdp(p), _r1p(p), F3_SLT, i), I_LW),
    Row((0b00, 0b011), "c.ld", lambda p: True, lambda p, i: E,  # RV64 slot
        lambda p, i: i_type(OP_LOAD, _rdp(p), _r1p(p), F3_SLTU, i), I_LD),
    Row((0b00, 0b100), "reserved", lambda p: True, lambda p, i: R),
    Row((0b00, 0b101), "c.fsd", lambda p: True, lambda p, i: F),
    Row((0b00, 0b110), "c.sw", lambda p: True, lambda p, i: E,
        lambda p, i: s_type(OP_STORE, _r1p(p), _r2p(p), F3_SLT, i), I_LW),
    Row((0b00, 0b111), "c.sd", lambda p: True, lambda p, i: E,  # RV64 slot
        lambda p, i: s_type(OP_STORE, _r1p(p), _r2p(p), F3_SLTU, i), I_LD),
    # ---- Quadrant 1 ----
    Row((0b01, 0b000), "c.addi/c.nop", lambda p: True,
        lambda p, i: H if (_rd(p) == 0) != (i == 0) else E,
        lambda p, i: i_type(OP_OP_IMM, _rd(p), _rd(p), F3_ADDSUB, i), I_CI, 6),
    Row((0b01, 0b001), "c.addiw", lambda p: True,
        lambda p, i: R if _rd(p) == 0 else E,
        lambda p, i: i_type(OP_OP_IMM_32, _rd(p), _rd(p), F3_ADDSUB, i), I_CI, 6),
    Row((0b01, 0b010), "c.li", lambda p: True,
        lambda p, i: H if _rd(p) == 0 else E,
        lambda p, i: i_type(OP_OP_IMM, _rd(p), 0, F3_ADDSUB, i), I_CI, 6),
    Row((0b01, 0b011), "c.addi16sp", lambda p: _rd(p) == 2,
        lambda p, i: R if i == 0 else E,
        lambda p, i: i_type(OP_OP_IMM, 2, 2, F3_ADDSUB, i), I_ADDI16SP, 10),
    Row((0b01, 0b011), "c.lui", lambda p: _rd(p) != 2,
        lambda p, i: R if i == 0 else (H if _rd(p) == 0 else E),
        lambda p, i: u_type(OP_LUI, _rd(p), i), I_LUI, 18),
    Row((0b01, 0b100), "c.srli", lambda p: _sub(p) == 0b00,
        lambda p, i: H if gather(p, I_CI) == 0 else E,
        lambda p, i: i_type(OP_OP_IMM, _r1p(p), _r1p(p), F3_SRLSRA, i), I_CI),
    Row((0b01, 0b100), "c.srai", lambda p: _sub(p) == 0b01,
        lambda p, i: H if gather(p, I_CI) == 0 else E,
        lambda p, i: i_type(OP_OP_IMM, _r1p(p), _r1p(p), F3_SRLSRA, i | 0x400), I_CI),
    Row((0b01, 0b100), "c.andi", lambda p: _sub(p) == 0b10, lambda p, i: E,
        lambda p, i: i_type(OP_OP_IMM, _r1p(p), _r1p(p), F3_AND, i), I_CI, 6),
    Row((0b01, 0b100), "c.sub", lambda p: _sub(p) == 0b11 and _b12(p) == 0 and _f2(p) == 0,
        lambda p, i: E,
        lambda p, i: r_type(OP_OP, _r1p(p), _r1p(p), _r2p(p), F3_ADDSUB, 0x20)),
    Row((0b01, 0b100), "c.xor", lambda p: _sub(p) == 0b11 and _b12(p) == 0 and _f2(p) == 1,
        lambda p, i: E,
        lambda p, i: r_type(OP_OP, _r1p(p), _r1p(p), _r2p(p), F3_XOR, 0)),
    Row((0b01, 0b100), "c.or", lambda p: _sub(p) == 0b11 and _b12(p) == 0 and _f2(p) == 2,
        lambda p, i: E,
        lambda p, i: r_type(OP_OP, _r1p(p), _r1p(p), _r2p(p), F3_OR, 0)),
    Row((0b01, 0b100), "c.and", lambda p: _sub(p) == 0b11 and _b12(p) == 0 and _f2(p) == 3,
        lambda p, i: E,
        lambda p, i: r_type(OP_OP, _r1p(p), _r1p(p), _r2p(p), F3_AND, 0)),
    Row((0b01, 0b100), "c.subw", lambda p: _sub(p) == 0b11 and _b12(p) == 1 and _f2(p) == 0,
        lambda p, i: E,
        lambda p, i: r_type(OP_OP_32, _r1p(p), _r1p(p), _r2p(p), F3_ADDSUB, 0x20)),
    Row((0b01, 0b100), "c.addw", lambda p: _sub(p) == 0b11 and _b12(p) == 1 and _f2(p) == 1,
        lambda p, i: E,
        lambda p, i: r_type(OP_OP_32, _r1p(p), _r1p(p), _r2p(p), F3_ADDSUB, 0)),
    Row((0b01, 0b100), "rv64-reserved", lambda p: _sub(p) == 0b11 and _b12(p) == 1 and _f2(p) >= 2,
        lambda p, i: R),
    Row((0b01, 0b101), "c.j", lambda p: True, lambda p, i: E,
        lambda p, i: j_type(OP_JAL, 0, i), I_J, 12),
    Row((0b01, 0b110), "c.beqz", lambda p: True, lambda p, i: E,
        lambda p, i: b_type(OP_BRANCH, _r1p(p), 0, F3_ADDSUB, i), I_B, 9),
    Row((0b01, 0b111), "c.bnez", lambda p: True, lambda p, i: E,
        lambda p, i: b_type(OP_BRANCH, _r1p(p), 0, F3_SLL, i), I_B, 9),
    # ---- Quadrant 2 ----
    Row((0b10, 0b000), "c.slli", lambda p: True,
        lambda p, i: H if _rd(p) == 0 or gather(p, I_CI) == 0 else E,
        lambda p, i: i_type(OP_OP_IMM, _rd(p), _rd(p), F3_SLL, i), I_CI),
    Row((0b10, 0b001), "c.fldsp", lambda p: True, lambda p, i: F),
    Row((0b10, 0b010), "c.lwsp", lambda p: True,
        lambda p, i: R if _rd(p) == 0 else E,
        lambda p, i: i_type(OP_LOAD, _rd(p), 2, F3_SLT, i), I_LWSP),
    Row((0b10, 0b011), "c.ldsp", lambda p: True,  # RV64 slot
        lambda p, i: R if _rd(p) == 0 else E,
        lambda p, i: i_type(OP_LOAD, _rd(p), 2, F3_SLTU, i), I_LDSP),
    Row((0b10, 0b100), "c.jr", lambda p: _b12(p) == 0 and _rs2(p) == 0,
        lambda p, i: R if _rd(p) == 0 else E,
        lambda p, i: i_type(OP_JALR, 0, _rd(p), F3_ADDSUB, 0)),
    Row((0b10, 0b100), "c.mv", lambda p: _b12(p) == 0 and _rs2(p) != 0,
        lambda p, i: H if _rd(p) == 0 else E,
        lambda p, i: r_type(OP_OP, _rd(p), 0, _rs2(p), F3_ADDSUB, 0)),
    Row((0b10, 0b100), "c.ebreak", lambda p: _b12(p) == 1 and _rd(p) == 0 and _rs2(p) == 0,
        lambda p, i: E,
        lambda p, i: i_type(OP_SYSTEM, 0, 0, F3_ADDSUB, 1)),
    Row((0b10, 0b100), "c.jalr", lambda p: _b12(p) == 1 and _rd(p) != 0 and _rs2(p) == 0,
        lambda p, i: E,
        lambda p, i: i_type(OP_JALR, 1, _rd(p), F3_ADDSUB, 0)),
    Row((0b10, 0b100), "c.add", lambda p: _b12(p) == 1 and _rs2(p) != 0,
        lambda p, i: H if _rd(p) == 0 else E,
        lambda p, i: r_type(OP_OP, _rd(p), _rd(p), _rs2(p), F3_ADDSUB, 0)),
    Row((0b10, 0b101), "c.fsdsp", lambda p: True, lambda p, i: F),
    Row((0b10, 0b110), "c.swsp", lambda p: True, lambda p, i: E,
        lambda p, i: s_type(OP_STORE, 2, _rs2(p), F3_SLT, i), I_SWSP),
    Row((0b10, 0b111), "c.sdsp", lambda p: True, lambda p, i: E,  # RV64 slot
        lambda p, i: s_type(OP_STORE, 2, _rs2(p), F3_SLTU, i), I_SDSP),
]

INDEX = {}
for row in ROWS:
    INDEX.setdefault(row.sel, []).append(row)


def classify(p: int):
    rows = INDEX.get((p & 3, (p >> 13) & 7), ())
    for row in rows:
        if not row.match(p):
            continue
        imm = gather(p, row.imm_layout)
        if row.imm_bits:
            imm = sext(imm, row.imm_bits)
        cls = row.kind(p, imm)
        if cls in (R, F):
            return cls, None
        return cls, row.build(p, imm)
    return R, None


def main() -> None:
    for p in range(0x10000):
        cls, enc = classify(p)
        exp = f"0x{enc:08x}" if enc is not None else "-"
        print(f"0x{p:04x}\t{cls}\t{exp}")


if __name__ == "__main__":
    main()
