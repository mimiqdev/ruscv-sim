# zca_hints.S — HINT encodings retire with no architectural effect.
#
# Contract D2: exactly the specification HINT encodings retire with no
# architectural effect. GNU as cannot emit most HINT forms from mnemonics
# (it rejects 3-operand c.* forms and reserves rd=0 aliases), so this guest
# emits the raw 16-bit parcels with .half and checks that the surrounding
# architectural state is untouched.
#
# Parcel bit layouts used below (all in an `.option rvc` region):
#   c.addi:  [15:13]=000 [12]=imm[5] [11:7]=rd [6:2]=imm[4:0] [1:0]=01
#   c.li:    [15:13]=010 [12]=imm[5] [11:7]=rd [6:2]=imm[4:0] [1:0]=01
#   c.lui:   [15:13]=011 [12]=imm[5] [11:7]=rd [6:2]=imm[4:0] [1:0]=01
#   c.slli:  [15:13]=000 [12]=0    [11:7]=rd [6:2]=shamt    [1:0]=10
#   c.srli:  [15:13]=100 [12]=0    [11:7]=rd [6:2]=shamt    [1:0]=01
#   c.srai:  [15:13]=100 [12]=1    [11:7]=rd [6:2]=shamt    [1:0]=01
#   c.mv:    [15:12]=1000     [11:7]=rd [6:2]=rs2          [1:0]=10
#   c.add:   [15:12]=1001     [11:7]=rd [6:2]=rs2          [1:0]=10

.section .text
.globl _start

_start:
    .option push
    .option norelax
    # NOTE: no `.option rvc` here. The guarded registers are set with plain
    # 32-bit `li` because c.li cannot hold 42; the parcels below are emitted
    # as raw .half words, which assemble with or without .option rvc.

    # Guard registers whose HINT forms must stay untouched.
    li x5, 0x2A
    li x6, 0x2A
    li x7, 0x2A
    li x8, 0x2A
    li x9, 0x2A
    li x10, 0x2A
    li x11, 0x2A
    li x12, 0x2A

    # HINT: c.addi x5, 0 (rd != 0, imm = 0). rd=5,imm=0: 0x0281.
    .half 0x0281
    # Real 16-bit instruction between HINT parcels: c.addi x5, 1 = 0x0285.
    .half 0x0285
    # HINT: c.nop 1 (rd = 0, imm = 1). rd=0,imm=1: 0x0005.
    .half 0x0005
    # HINT: c.li x0, 7 (rd = 0). 010_0_00000_00111_01 = 0x401D.
    .half 0x401D
    # HINT: c.lui x0, 1 (rd = 0, nzimm != 0). 011_0_00000_00001_01 = 0x6005.
    .half 0x6005
    # HINT: c.slli x0, 1 (rd = 0). rd=0,shamt=1,q=2: 0x0006.
    .half 0x0006
    # HINT: c.srli x8, 0 (zero shift). f3=100,rd'=x8,shamt=0: 0x8001.
    # (C.SRAI zero-shift is not encodable on RV64: bit 12 is both the SRAI
    # selector and shamt[5], so shamt=0 forces the SRLI encoding. The U1
    # decoder's C.SRAI zero-shift branch stays for the RV32-form contract.)
    .half 0x8001
    # HINT: c.mv x0, x5 (rd = 0, rs2 != 0). rd=0,rs2=5,q=2: 0x8016.
    .half 0x8016
    # HINT: c.add x0, x5 (rd = 0, rs2 != 0). rd=0,rs2=5,q=2: 0x9016.
    .half 0x9016
    # Real 16-bit instruction after the HINTs: c.addi x5, 1. x5 = 44.
    .half 0x0285

    # x5 advanced by exactly the two real c.addi parcels: 42 + 2 = 44.
    li      t2, 44
    beq     x5, t2, check_rest
    c.j     fail

check_rest:
    # Every other guarded register still holds 42.
    li t2, 0x2A
    beq     x6, t2, 1f
    c.j     fail
1:
    beq     x7, t2, 2f
    c.j     fail
2:
    beq     x8, t2, 3f
    c.j     fail
3:
    beq     x9, t2, 4f
    c.j     fail
4:
    beq     x10, t2, 5f
    c.j     fail
5:
    beq     x11, t2, 6f
    c.j     fail
6:
    beq     x12, t2, check_x0
    c.j     fail

check_x0:
    # x0 must still be zero: copy it and compare.
    li t3, 0
    add     t3, t3, x0            # t3 += x0: still 0
    beqz    t3, pass
    c.j     fail

pass:
    c.li    a0, 0
    j       write_exit
fail:
    c.li    a0, 1

write_exit:
    .option pop
    la      t0, tohost
    li      t2, 1
    slli    t2, t2, 63
    or      t1, a0, t2
    sd      t1, 0(t0)
1:
    j       1b

.section .tohost
.align 3
.global tohost
tohost:
    .dword 0
