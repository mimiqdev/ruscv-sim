# A6 bare-metal trap regression: EBREAK.
# EBREAK has cause 3 and an architectural MTVAL of zero.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0

    li      t0, 8
    csrrs   x0, mstatus, t0

    ebreak_site:
    ebreak

resumed:
    li      t0, 1
    bne     s0, t0, fail
    j       pass

trap_handler:
    li      s0, 1

    csrr    t0, mcause
    li      t1, 3
    bne     t0, t1, fail

    csrr    t0, mtval
    bnez    t0, fail

    csrr    t0, mepc
    la      t1, ebreak_site
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
