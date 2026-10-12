# D11 CLI guest. Sets SSIP and SSIE in M, enables MIE, and checks that the
# next instruction is interrupted into a direct mtvec handler.

.section .text
.globl _start

_start:
    la      t0, handler
    csrw    mtvec, t0
    li      t0, 2
    csrs    mie, t0
    csrs    mip, t0
    li      t0, 8
    csrs    mstatus, t0
interrupted:
    nop
    j       fail

handler:
    csrr    t0, mcause
    li      t1, 1
    slli    t1, t1, 63
    addi    t1, t1, 1
    bne     t0, t1, fail
    csrr    t0, mtval
    bnez    t0, fail
    csrr    t0, mepc
    la      t1, interrupted
    bne     t0, t1, fail
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
