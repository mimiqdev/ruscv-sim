# Stage 6 U1 guest. Sdext CSRs 0x7b0-0x7b3 are legal only in Debug Mode, and
# this hart never executes there. Sdtrig is absent, so 0x7a0 (tselect) is an
# illegal instruction as well. Each access must trap with cause 2.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    li      s0, 0

    csrr    t0, 0x7b0
    csrr    t0, 0x7b1
    csrr    t0, 0x7b2
    csrr    t0, 0x7b3
    csrr    t0, 0x7a0

    li      t0, 5
    bne     s0, t0, fail
    j       pass

trap_handler:
    csrr    t0, mcause
    li      t1, 2
    bne     t0, t1, fail
    addi    s0, s0, 1
    csrr    t0, mepc
    addi    t0, t0, 4
    csrw    mepc, t0
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
