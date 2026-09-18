# A6 bare-metal trap regression: an illegal instruction.
# The handler verifies the raw instruction in MTVAL, advances MEPC, and
# returns.  A handler failure always writes a nonzero guest exit code.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0

    li      t0, 8
    csrrs   x0, mstatus, t0

    illegal_site:
    .word   0xffffffff

resumed:
    li      t0, 1
    bne     s0, t0, fail
    j       pass

trap_handler:
    li      s0, 1

    csrr    t0, mcause
    li      t1, 2
    bne     t0, t1, fail

    csrr    t0, mtval
    # MTVAL carries the zero-extended 32-bit illegal instruction in RV64.
    li      t1, 1
    slli    t1, t1, 32
    addi    t1, t1, -1
    bne     t0, t1, fail

    csrr    t0, mepc
    la      t1, illegal_site
    bne     t0, t1, fail
    addi    t0, t0, 4
    csrw    mepc, t0

    csrr    t0, mstatus
    andi    t1, t0, 8
    bnez    t1, fail
    srli    t1, t0, 7
    andi    t1, t1, 1
    li      t2, 1
    bne     t1, t2, fail
    srli    t1, t0, 11
    andi    t1, t1, 3
    li      t2, 3
    bne     t1, t2, fail

    mret

pass:
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1

write_exit:
    li      t0, 1
    slli    t0, t0, 63
    or      a0, a0, t0
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

.section .tohost
.align 3
tohost:
    .dword  0
