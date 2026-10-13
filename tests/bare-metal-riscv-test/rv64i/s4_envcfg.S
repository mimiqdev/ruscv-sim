# Stage 4 environment-configuration guest (dev-plan §4 "Environment
# configuration", M2).
#
# menvcfg and senvcfg exist.  FIOM (bit 0) reads back the written value;
# every other bit reads 0.  Access raises no illegal-instruction trap.
# On this single in-order hart a FENCE retires identically with FIOM set
# and clear; the guest checks both retire and that a known register is
# unchanged across them.

    .equ CAUSE_ILLEGAL, 2
    .equ ANY, -2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1

    # menvcfg (0x30A): all-ones write keeps only FIOM.
    li      t0, -1
    csrw    0x30A, t0
    csrr    t1, 0x30A
    li      t2, 1
    bne     t1, t2, fail
    csrw    0x30A, x0
    csrr    t1, 0x30A
    bnez    t1, fail

    # senvcfg (0x10A): same rule, from M.
    csrw    0x10A, t0
    csrr    t1, 0x10A
    li      t2, 1
    bne     t1, t2, fail
    csrw    0x10A, x0
    csrr    t1, 0x10A
    bnez    t1, fail

    # FENCE retires with FIOM clear and set, in M, S, and U, and changes
    # no register.  s2 carries the witness value.
    li      s2, 0x1234
    fence
    li      t1, 0x1234
    bne     s2, t1, fail

    li      t0, 1
    csrw    0x30A, t0                   # FIOM = 1
    fence
    bne     s2, t1, fail

    # Enter S and fence both ways.
    la      t0, s_fence
    csrw    mepc, t0
    li      t0, 0x880                   # MPIE | MPP = S
    csrw    mstatus, t0
    mret
s_fence:
    fence                               # FIOM set
    csrw    senvcfg, x0                 # FIOM clear through senvcfg
    fence
    # Drop to U through sret (SPP = 0).
    la      t0, u_fence
    csrw    sepc, t0
    sret
u_fence:
    fence
    # U cannot write senvcfg; the read of an M CSR proves the mode and
    # returns to M to finish.
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x30102373              # csrr t1,misa (§7.8 ruling)
    la      s7, done_ok
    li      s4, 0x1800                  # return to M
    csrr    t1, misa
    j       fail
done_ok:
    csrr    t1, 0x30A
    li      t2, 1
    bne     t1, t2, fail                # menvcfg FIOM still set
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

mismatch:
    csrr    a0, mcause
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
    li      t5, ANY
    beq     s6, t5, mtval_ok
    bne     t1, s6, mismatch
mtval_ok:
    li      s5, -1
    csrw    mepc, s7
    csrw    mstatus, s4
    mret

.section .tohost
.align 3
tohost:
    .dword 0
