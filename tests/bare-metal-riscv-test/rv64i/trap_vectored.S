# A6 bare-metal trap regression: synchronous traps with mtvec.MODE=Vectored.
# The first vector slot jumps over sentinels.  If synchronous ECALL were
# incorrectly sent to BASE + 4*cause, cause 11 would enter the failure slot at
# BASE+44 instead of the handler body.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    ori     t0, t0, 1
    csrw    mtvec, t0

    li      t0, 8
    csrrs   x0, mstatus, t0

    ecall_site:
    ecall

resumed:
    li      t0, 1
    bne     s0, t0, fail
    j       pass

.align 2
trap_handler:
    j       trap_handler_body
    # BASE+44 is deliberately a failure path.  Correct synchronous vectoring
    # enters at BASE; a mistaken BASE+4*11 calculation reaches this slot.
    .rept   11
    j       fail
    .endr

trap_handler_body:
    li      s0, 1

    csrr    t0, mcause
    li      t1, 11
    bne     t0, t1, fail

    csrr    t0, mepc
    la      t1, ecall_site
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
