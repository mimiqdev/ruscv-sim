# fd_rounding.S — every rounding mode, static and dynamic, under rv64imafd.
#
# Contract D4/D5: all five rounding modes select distinct results, RMM differs
# from RNE on a tie, a static `rm` overrides `frm`, a reserved static `rm`
# (5 or 6) raises illegal-instruction, and a dynamic `rm` while `frm` holds
# 5-7 raises illegal-instruction. Accrued flags only accrue; they never trap.
#
# Two values drive the checks:
#   1/3 in single precision is not representable, so RNE/RUP/RMM round up to
#   0x3EAAAAAB and RTZ/RDN round down to 0x3EAAAAAA.
#   1 + 2^-24 in double precision is exactly halfway between the two nearest
#   singles, so RNE returns 0x3F800000 (even) and RMM returns 0x3F800001.

    .equ CAUSE_ILLEGAL, 2
    .equ FS_DIRTY, 0x6000

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1
    li      s6, -1

    # mstatus.FS = Dirty so F and D instructions are legal (contract D6).
    li      t0, FS_DIRTY
    csrs    mstatus, t0
    csrw    fcsr, x0

    # ---- 1/3 in single precision: RNE, RTZ, RDN, RUP, RMM ----
    li      t0, 0x3f800000         # 1.0f
    fmv.w.x f1, t0
    li      t0, 0x40400000         # 3.0f
    fmv.w.x f2, t0

    csrw    fflags, x0
    fdiv.s  f3, f1, f2, rne
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaab
    bne     t1, t2, fail1
    csrr    t3, fflags
    li      t4, 1                  # NX
    bne     t3, t4, fail2

    fdiv.s  f3, f1, f2, rtz
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaaa
    bne     t1, t2, fail3

    fdiv.s  f3, f1, f2, rdn
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaaa
    bne     t1, t2, fail4

    fdiv.s  f3, f1, f2, rup
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaab
    bne     t1, t2, fail5

    fdiv.s  f3, f1, f2, rmm
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaab
    bne     t1, t2, fail6

    # ---- exact halfway value: RNE rounds to even, RMM away from zero ----
    li      t0, 0x3ff0000010000000 # 1.0 + 2^-24 in double precision
    fmv.d.x f4, t0

    csrw    fflags, x0
    fcvt.s.d f5, f4, rne
    fmv.x.w t1, f5
    li      t2, 0x3f800000
    bne     t1, t2, fail7
    csrr    t3, fflags
    li      t4, 1                  # NX: the tie is inexact
    bne     t3, t4, fail8

    fcvt.s.d f5, f4, rmm
    fmv.x.w t1, f5
    li      t2, 0x3f800001
    bne     t1, t2, fail9
    # RMM and RNE must not be the same rounding (contract D4).
    fcvt.s.d f6, f4, rne
    fmv.x.w t1, f5
    fmv.x.w t2, f6
    beq     t1, t2, fail10

    fcvt.s.d f5, f4, rtz
    fmv.x.w t1, f5
    li      t2, 0x3f800000
    bne     t1, t2, fail11
    fcvt.s.d f5, f4, rdn
    fmv.x.w t1, f5
    li      t2, 0x3f800000
    bne     t1, t2, fail12
    fcvt.s.d f5, f4, rup
    fmv.x.w t1, f5
    li      t2, 0x3f800001
    bne     t1, t2, fail13

    # ---- dynamic rounding reads frm ----
    li      t0, 1                  # RTZ
    csrw    frm, t0
    fdiv.s  f3, f1, f2              # rm = dyn
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaaa
    bne     t1, t2, fail14

    li      t0, 3                  # RUP
    csrw    frm, t0
    fdiv.s  f3, f1, f2
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaab
    bne     t1, t2, fail15

    li      t0, 4                  # RMM
    csrw    frm, t0
    fcvt.s.d f5, f4
    fmv.x.w t1, f5
    li      t2, 0x3f800001
    bne     t1, t2, fail16

    # frm holds 0 (RNE) before the reserved-`frm` illegal-instruction checks.
    csrw    frm, x0

    # ---- a static rm overrides frm ----
    li      t0, 3                  # frm = RUP
    csrw    frm, t0
    fdiv.s  f3, f1, f2, rtz        # static RTZ wins
    fmv.x.w t1, f3
    li      t2, 0x3eaaaaaa
    bne     t1, t2, fail17
    csrw    frm, x0

    # ---- reserved static rm = 5 and 6 raise illegal-instruction ----
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003150d3         # fadd.s f1,f2,f3,rm=5
    la      s7, after_static5
    .word   0x003150d3
    j       fail18
after_static5:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003160d3         # fadd.s f1,f2,f3,rm=6
    la      s7, after_static6
    .word   0x003160d3
    j       fail19
after_static6:

    # ---- reserved dynamic frm = 5, 6, 7 raise illegal-instruction ----
    li      t0, 5
    csrw    frm, t0
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003170d3         # fadd.s f1,f2,f3,dyn
    la      s7, after_dyn5
    .word   0x003170d3
    j       fail20
after_dyn5:
    li      t0, 6
    csrw    frm, t0
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003170d3
    la      s7, after_dyn6
    .word   0x003170d3
    j       fail21
after_dyn6:
    li      t0, 7
    csrw    frm, t0
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003170d3
    la      s7, after_dyn7
    .word   0x003170d3
    j       fail22
after_dyn7:
    csrw    frm, x0

    # ---- fflags survive unrelated instructions and frame the CSR alias ----
    csrw    fflags, x0
    li      t0, 0x1f               # NX|UF|OF|DZ|NV
    csrw    fflags, t0
    csrr    t1, fflags
    bne     t1, t0, fail23
    csrr    t1, fcsr
    andi    t1, t1, 0x1f
    bne     t1, t0, fail24

    li      a0, 0
    j       write_exit

fail1:  li a0, 1
    j   write_exit
fail2:  li a0, 2
    j   write_exit
fail3:  li a0, 3
    j   write_exit
fail4:  li a0, 4
    j   write_exit
fail5:  li a0, 5
    j   write_exit
fail6:  li a0, 6
    j   write_exit
fail7:  li a0, 7
    j   write_exit
fail8:  li a0, 8
    j   write_exit
fail9:  li a0, 9
    j   write_exit
fail10: li a0, 10
    j   write_exit
fail11: li a0, 11
    j   write_exit
fail12: li a0, 12
    j   write_exit
fail13: li a0, 13
    j   write_exit
fail14: li a0, 14
    j   write_exit
fail15: li a0, 15
    j   write_exit
fail16: li a0, 16
    j   write_exit
fail17: li a0, 17
    j   write_exit
fail18: li a0, 18
    j   write_exit
fail19: li a0, 19
    j   write_exit
fail20: li a0, 20
    j   write_exit
fail21: li a0, 21
    j   write_exit
fail22: li a0, 22
    j   write_exit
fail23: li a0, 23
    j   write_exit
fail24: li a0, 24
    j   write_exit

# A trap that was not the expected illegal-instruction is a failure.
unexpected:
    li      a0, 63
    j       write_exit
mismatch:
    csrr    a0, mcause
    ori     a0, a0, 0x100
    j       write_exit

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
    bne     t1, s6, mismatch
    li      s5, -1
    csrw    mepc, s7
    mret

.section .tohost
.align 3
tohost:
    .dword 0
