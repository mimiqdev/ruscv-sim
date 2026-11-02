#!/usr/bin/env python3
"""Independent Zca expansion-decoder reference generator (issue #132).

Classifies every 16-bit parcel as legal expansion / HINT / reserved /
FP-compressed illegal and emits one `parcel class expansion` TSV row each,
deterministically, on stdout. Written from the RISC-V ISA Unprivileged
Specification version 20250508 Chapter "C" and the Selectable Zca
Compressed-Instruction Profile contract (docs/dev-plan.md D2/D6, merge
9db1bed). Independent of src/isa/rv64c/decoder_16bit.rs.

Contract-D2 reading: C.ADDI4SPN with rd'=x0 and a nonzero immediate is not
one of the listed HINT classes and not listed as reserved, so it is a legal
expansion (addi x0, x2, nzuimm) here and in the Rust decoder.
"""

# 32-bit opcode and funct3 encodings (not shared with the Rust tables).
OP_LOAD, OP_LOAD_FP = 0b0000011, 0b0000111
OP_STORE, OP_STORE_FP = 0b0100011, 0b0100111
OP_OP_IMM, OP_OP_IMM_32 = 0b0010011, 0b0011011
OP_OP, OP_OP_32 = 0b0110011, 0b0111011
OP_LUI, OP_AUIPC = 0b0110111, 0b0010111
OP_BRANCH, OP_JALR, OP_JAL = 0b1100011, 0b1100111, 0b1101111
OP_SYSTEM = 0b1110011
F3_ADDSUB, F3_SLL, F3_SLT, F3_SLTU = 0, 1, 2, 3
F3_XOR, F3_SRLSRA, F3_OR, F3_AND = 4, 5, 6, 7


def sext(value: int, bits: int) -> int:
    if value & (1 << (bits - 1)):
        return value | ~((1 << bits) - 1)
    return value


def i_type(op: int, rd: int, rs1: int, f3: int, imm: int) -> int:
    return (
        ((imm & 0xFFF) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((f3 & 7) << 12)
        | ((rd & 0x1F) << 7)
        | op
    ) & 0xFFFFFFFF


def r_type(op: int, rd: int, rs1: int, rs2: int, f3: int, f7: int) -> int:
    return (
        ((f7 & 0x7F) << 25)
        | ((rs2 & 0x1F) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((f3 & 7) << 12)
        | ((rd & 0x1F) << 7)
        | op
    ) & 0xFFFFFFFF


def s_type(op: int, rs1: int, rs2: int, f3: int, imm: int) -> int:
    return (
        (((imm >> 5) & 0x7F) << 25)
        | ((rs2 & 0x1F) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((f3 & 7) << 12)
        | ((imm & 0x1F) << 7)
        | op
    ) & 0xFFFFFFFF


def b_type(op: int, rs1: int, rs2: int, f3: int, imm: int) -> int:
    return (
        (((imm >> 12) & 1) << 31)
        | (((imm >> 5) & 0x3F) << 25)
        | ((rs2 & 0x1F) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((f3 & 7) << 12)
        | (((imm >> 1) & 0xF) << 8)
        | (((imm >> 11) & 1) << 7)
        | op
    ) & 0xFFFFFFFF


def u_type(op: int, rd: int, imm: int) -> int:
    return ((imm & 0xFFFFF000) | ((rd & 0x1F) << 7) | op) & 0xFFFFFFFF


def j_type(op: int, rd: int, imm: int) -> int:
    return (
        (((imm >> 20) & 1) << 31)
        | (((imm >> 12) & 0xFF) << 12)
        | (((imm >> 11) & 1) << 20)
        | (((imm >> 1) & 0x3FF) << 21)
        | ((rd & 0x1F) << 7)
        | op
    ) & 0xFFFFFFFF


# Immediate extractors, mirroring the specification tables.
def ci_imm(p: int) -> int:  # C.ADDI/C.LI/C.ANDI/C.ADDIW
    return sext(((p >> 12) & 1) << 5 | ((p >> 2) & 0x1F), 6)


def c_lui_imm(p: int) -> int:  # already positioned at bit 17..12
    return sext(((p >> 12) & 1) << 17 | ((p >> 2) & 0x1F) << 12, 18)


def c_addi16sp_imm(p: int) -> int:
    return sext(
        ((p >> 12) & 1) << 9
        | ((p >> 6) & 1) << 4
        | ((p >> 5) & 1) << 6
        | ((p >> 3) & 3) << 7
        | ((p >> 2) & 1) << 5,
        10,
    )


def c_addi4spn_imm(p: int) -> int:
    return (
        ((p >> 11) & 3) << 4
        | ((p >> 7) & 0xF) << 6
        | ((p >> 5) & 1) << 3
        | ((p >> 6) & 1) << 2
    )


def c_lw_imm(p: int) -> int:  # also C.SW
    return ((p >> 10) & 7) << 3 | ((p >> 6) & 1) << 2 | ((p >> 5) & 1) << 6


def c_ld_imm(p: int) -> int:  # also C.SD
    return ((p >> 10) & 7) << 3 | ((p >> 5) & 3) << 6


def c_lwsp_imm(p: int) -> int:
    return ((p >> 12) & 1) << 5 | ((p >> 4) & 7) << 2 | ((p >> 2) & 3) << 6


def c_ldsp_imm(p: int) -> int:
    return ((p >> 12) & 1) << 5 | ((p >> 5) & 3) << 3 | ((p >> 2) & 7) << 6


def c_swsp_imm(p: int) -> int:
    return ((p >> 9) & 0xF) << 2 | ((p >> 7) & 3) << 6


def c_sdsp_imm(p: int) -> int:
    return ((p >> 10) & 7) << 3 | ((p >> 7) & 7) << 6


def c_j_imm(p: int) -> int:
    return sext(
        ((p >> 12) & 1) << 11
        | ((p >> 11) & 1) << 4
        | ((p >> 9) & 3) << 8
        | ((p >> 8) & 1) << 10
        | ((p >> 7) & 1) << 6
        | ((p >> 6) & 1) << 7
        | ((p >> 3) & 7) << 1
        | ((p >> 2) & 1) << 5,
        12,
    )


def c_b_imm(p: int) -> int:  # C.BEQZ/C.BNEZ
    return sext(
        ((p >> 12) & 1) << 8
        | ((p >> 10) & 3) << 3
        | ((p >> 5) & 3) << 6
        | ((p >> 3) & 3) << 1
        | ((p >> 2) & 1) << 5,
        9,
    )


def c_shamt(p: int) -> int:
    return ((p >> 12) & 1) << 5 | ((p >> 2) & 0x1F)


E, H, R, F = "E", "H", "R", "F"


def classify_c0(p: int):
    funct3 = (p >> 13) & 7
    rd = 8 + ((p >> 2) & 7)
    rs1 = 8 + ((p >> 7) & 7)
    rs2 = 8 + ((p >> 2) & 7)
    if funct3 == 0b000:
        imm = c_addi4spn_imm(p)
        if imm == 0:
            return R, None
        return E, i_type(OP_OP_IMM, rd, 2, F3_ADDSUB, imm)
    if funct3 in (0b001, 0b101):
        return F, None  # C.FLD / C.FSD
    if funct3 == 0b010:
        return E, i_type(OP_LOAD, rd, rs1, F3_SLT, c_lw_imm(p))
    if funct3 == 0b011:
        return E, i_type(OP_LOAD, rd, rs1, F3_SLTU, c_ld_imm(p))  # RV64: C.LD
    if funct3 == 0b100:
        return R, None
    if funct3 == 0b110:
        return E, s_type(OP_STORE, rs1, rs2, F3_SLT, c_lw_imm(p))
    return E, s_type(OP_STORE, rs1, rs2, F3_SLTU, c_ld_imm(p))  # RV64: C.SD


def classify_c1(p: int):
    funct3 = (p >> 13) & 7
    rd = (p >> 7) & 0x1F
    if funct3 == 0b000:
        imm = ci_imm(p)
        if (rd == 0) != (imm == 0):
            return H, i_type(OP_OP_IMM, rd, rd, F3_ADDSUB, imm)
        return E, i_type(OP_OP_IMM, rd, rd, F3_ADDSUB, imm)
    if funct3 == 0b001:
        if rd == 0:
            return R, None
        return E, i_type(OP_OP_IMM_32, rd, rd, F3_ADDSUB, ci_imm(p))
    if funct3 == 0b010:
        if rd == 0:
            return H, i_type(OP_OP_IMM, rd, 0, F3_ADDSUB, ci_imm(p))
        return E, i_type(OP_OP_IMM, rd, 0, F3_ADDSUB, ci_imm(p))
    if funct3 == 0b011:
        if rd == 2:
            imm = c_addi16sp_imm(p)
            if imm == 0:
                return R, None
            return E, i_type(OP_OP_IMM, 2, 2, F3_ADDSUB, imm)
        imm = c_lui_imm(p)
        if imm == 0:
            return R, None
        if rd == 0:
            return H, u_type(OP_LUI, rd, imm)
        return E, u_type(OP_LUI, rd, imm)
    if funct3 == 0b100:
        return classify_c1_alu(p)
    if funct3 == 0b101:
        return E, j_type(OP_JAL, 0, c_j_imm(p))
    if funct3 == 0b110:
        return E, b_type(OP_BRANCH, 8 + ((p >> 7) & 7), 0, F3_ADDSUB, c_b_imm(p))
    return E, b_type(OP_BRANCH, 8 + ((p >> 7) & 7), 0, F3_SLL, c_b_imm(p))


def classify_c1_alu(p: int):
    rd = 8 + ((p >> 7) & 7)
    rs2 = 8 + ((p >> 2) & 7)
    sub = (p >> 10) & 3
    if sub == 0b00:
        shamt = c_shamt(p)
        if shamt == 0:
            return H, i_type(OP_OP_IMM, rd, rd, F3_SRLSRA, shamt)
        return E, i_type(OP_OP_IMM, rd, rd, F3_SRLSRA, shamt)
    if sub == 0b01:
        shamt = c_shamt(p)
        enc = i_type(OP_OP_IMM, rd, rd, F3_SRLSRA, shamt | 0x400)
        return (H, enc) if shamt == 0 else (E, enc)
    if sub == 0b10:
        return E, i_type(OP_OP_IMM, rd, rd, F3_AND, ci_imm(p))
    funct2 = (p >> 5) & 3
    w = (p >> 12) & 1
    if w == 0:
        f3f7 = [
            (F3_ADDSUB, 0x20),
            (F3_XOR, 0),
            (F3_OR, 0),
            (F3_AND, 0),
        ][funct2]
        return E, r_type(OP_OP, rd, rd, rs2, f3f7[0], f3f7[1])
    if funct2 == 0:
        return E, r_type(OP_OP_32, rd, rd, rs2, F3_ADDSUB, 0x20)
    if funct2 == 1:
        return E, r_type(OP_OP_32, rd, rd, rs2, F3_ADDSUB, 0)
    return R, None  # RV64-reserved C.SUBW/C.ADDW-group encodings


def classify_c2(p: int):
    funct3 = (p >> 13) & 7
    rd = (p >> 7) & 0x1F
    rs2 = (p >> 2) & 0x1F
    if funct3 == 0b000:
        shamt = c_shamt(p)
        enc = i_type(OP_OP_IMM, rd, rd, F3_SLL, shamt)
        return (H, enc) if (rd == 0 or shamt == 0) else (E, enc)
    if funct3 == 0b001:
        return F, None  # C.FLDSP
    if funct3 == 0b011:
        if rd == 0:
            return R, None
        return E, i_type(OP_LOAD, rd, 2, F3_SLTU, c_ldsp_imm(p))  # RV64: C.LDSP
    if funct3 == 0b010:
        if rd == 0:
            return R, None
        return E, i_type(OP_LOAD, rd, 2, F3_SLT, c_lwsp_imm(p))
    if funct3 == 0b100:
        return classify_c2_mv_add(p)
    if funct3 == 0b101:
        return F, None  # C.FSDSP
    if funct3 == 0b110:
        return E, s_type(OP_STORE, 2, rs2, F3_SLT, c_swsp_imm(p))
    return E, s_type(OP_STORE, 2, rs2, F3_SLTU, c_sdsp_imm(p))  # RV64: C.SDSP


def classify_c2_mv_add(p: int):
    rd = (p >> 7) & 0x1F
    rs2 = (p >> 2) & 0x1F
    if ((p >> 12) & 1) == 0:
        if rs2 == 0:
            if rd == 0:
                return R, None  # C.JR with rs1=0
            return E, i_type(OP_JALR, 0, rd, F3_ADDSUB, 0)
        if rd == 0:
            return H, r_type(OP_OP, rd, 0, rs2, F3_ADDSUB, 0)
        return E, r_type(OP_OP, rd, 0, rs2, F3_ADDSUB, 0)
    if rs2 == 0 and rd == 0:
        return E, i_type(OP_SYSTEM, 0, 0, F3_ADDSUB, 1)  # C.EBREAK
    if rs2 == 0:
        return E, i_type(OP_JALR, 1, rd, F3_ADDSUB, 0)
    if rd == 0:
        return H, r_type(OP_OP, rd, rd, rs2, F3_ADDSUB, 0)
    return E, r_type(OP_OP, rd, rd, rs2, F3_ADDSUB, 0)


def classify(p: int):
    quadrant = p & 3
    if quadrant == 0:
        return classify_c0(p)
    if quadrant == 1:
        return classify_c1(p)
    if quadrant == 2:
        return classify_c2(p)
    return R, None


def main() -> None:
    for p in range(0x10000):
        cls, enc = classify(p)
        exp = f"0x{enc:08x}" if enc is not None else "-"
        print(f"0x{p:04x}\t{cls}\t{exp}")


if __name__ == "__main__":
    main()
