# fd_zcd_absent.S — Zcd stays illegal under rv64imafd.
#
# Contract D8: under `rv64imac` and `rv64imafd` the four Zcd encodings remain
# illegal-instruction, exactly as before Zcd support existed. This guest runs
# under `--isa rv64imafd` with `mstatus.FS` already Dirty, so each trap is an
# encoding trap and not an FS-Off trap. The parcels are emitted with `.half`
# so the assembler never needs compressed double-precision support.
#
# `mtval` for a two-byte illegal encoding carries the faulting fetch word, so
# only its low 16 bits (the parcel itself) are compared here. That keeps the
# check independent of whatever halfword follows the parcel.
#
# Each check increments s2 first, so a failure reports its own number.

    .equ CAUSE_ILLEGAL, 2
    .equ FS_DIRTY, 0x6000

# The parcel traps with illegal-instruction and mtval whose low 16 bits are the
# parcel, then execution resumes at `resume`. Reaching `j unexpected` means it
# did not trap, which is a failure.
#
# The parcel lives in the low half of an aligned 32-bit slot: rv64imafd has no
# C, so IALIGN is 32, the following instruction and the resume address must
# stay 4-byte aligned, and the faulting fetch word then carries the parcel with
# a zero upper half.
.macro EXPECT_ILLEGAL_HALF parcel, resume
    addi    s2, s2, 1
    li      s5, CAUSE_ILLEGAL
    li      s6, \parcel
    la      s7, \resume
    .balign 4
    .word   \parcel
    j       unexpected
\resume:
.endm

.section .text
.globl _start

_start:
    li      s2, 0
    li      s8, 0
    la      s1, tohost
    la      t0, trap_handler
    csrw    mtvec, t0
    li      s5, -1
    li      s6, -1
    li      t0, FS_DIRTY
    csrs    mstatus, t0
    la      a0, scratch
    li      sp, 0

    EXPECT_ILLEGAL_HALF 0x2500, after_c_fld       # c.fld f8, 8(a0)
    EXPECT_ILLEGAL_HALF 0xa580, after_c_fsd       # c.fsd f8, 8(a1)
    EXPECT_ILLEGAL_HALF 0x24a2, after_c_fldsp     # c.fldsp f9, 8(sp)
    EXPECT_ILLEGAL_HALF 0xa426, after_c_fsdsp     # c.fsdsp f9, 8(sp)

    # FS was already Dirty, so the traps changed nothing about it.
    csrr    t1, mstatus
    srli    t1, t1, 13
    andi    t1, t1, 3
    li      t6, 3
    bne     t1, t6, fail_marker

    li      a0, 0
    j       write_exit

fail_marker:
    addi    s2, s2, 1
fail:
    mv      a0, s2
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

mismatch:
    csrr    a0, mcause
    ori     a0, a0, 0x100

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    li      t4, -1
    beq     s5, t4, unexpected
    bne     t0, s5, mismatch
    slli    t1, t1, 48
    srli    t1, t1, 48
    bne     t1, s6, mismatch
    li      s5, -1
    csrw    mepc, s7
    mret

.section .data
.align 3
scratch:
    .dword 0, 0

.section .tohost
.align 3
tohost:
    .dword 0
