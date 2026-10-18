# Stage 5 U2 software interrupt.
#
# Privileged specification 20250508 §3.1.9: MSIP is read-only in mip and
# follows msip bit 0. MSIE is writable. §3.2.1: the msip store is visible
# at the next instruction. With MIE clear in M the pending interrupt is
# not taken. After MSIE and MIE are both set, the next instruction does
# not retire. The handler sees mepc and the register that instruction
# would have written.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    lui     s2, 0x02000                 # msip at 0x0200_0000

    li      t0, -1
    sw      t0, 0(s2)
    lw      t1, 0(s2)
    li      t2, 1
    bne     t1, t2, fail

    csrr    t1, mip
    andi    t1, t1, 8
    beqz    t1, fail
    csrw    mip, zero
    csrr    t1, mip
    andi    t1, t1, 8
    beqz    t1, fail

    li      t0, 8                       # MSIE, mstatus.MIE still 0
    csrw    mie, t0
    addi    s8, zero, 0x55
    li      t1, 0x55
    bne     s8, t1, fail

    li      s8, 0x22
    la      s7, victim
    li      t0, 8
    csrs    mstatus, t0
victim:
    addi    s8, zero, 0x99
    j       fail

trap_handler:
    csrr    t0, mcause
    li      t1, 1
    slli    t1, t1, 63
    addi    t1, t1, 3
    bne     t0, t1, mismatch
    csrr    t0, mepc
    bne     t0, s7, mismatch
    csrr    t0, mtval
    bnez    t0, mismatch
    li      t1, 0x22
    bne     s8, t1, mismatch
    li      a0, 0
    j       write_exit

mismatch:
    li      a0, 1
    j       write_exit

fail:
    li      a0, 1

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)
hang:
    j       hang

.section .tohost
.align 3
tohost:
    .dword 0
