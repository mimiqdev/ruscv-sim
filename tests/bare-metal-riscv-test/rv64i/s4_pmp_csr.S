# Stage 4 PMP CSR guest (dev-plan §4 "PMP CSR access", D5).
#
# The standard RV64 PMP set is read-only zero: the eight even pmpcfg CSRs
# and pmpaddr0 through pmpaddr63.  A write is ignored and raises nothing.
# Odd pmpcfg addresses do not exist on RV64, so access raises
# illegal-instruction.  The guest covers both ends of each range and the
# odd-address trap, then checks that a write changed no architectural
# state (a known mscratch value survives).

    .equ CAUSE_ILLEGAL, 2
    .equ ANY, -2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1
    li      s4, 0x800                   # handler returns to S? no: stays M
    li      t3, 0x5a5a5a5a5a5a5a5a
    csrw    mscratch, t3

    # Even pmpcfg0 (0x3A0) and pmpcfg14 (0x3AE): write ignored, read 0.
    li      t0, -1
    csrw    0x3A0, t0
    csrr    t1, 0x3A0
    bnez    t1, fail
    csrw    0x3AE, t0
    csrr    t1, 0x3AE
    bnez    t1, fail

    # pmpaddr0 (0x3B0) and pmpaddr63 (0x3EF): write ignored, read 0.
    csrw    0x3B0, t0
    csrr    t1, 0x3B0
    bnez    t1, fail
    csrw    0x3EF, t0
    csrr    t1, 0x3EF
    bnez    t1, fail

    # The writes changed no architectural state.
    csrr    t1, mscratch
    bne     t1, t3, fail

    # Odd pmpcfg1 (0x3A1) and pmpcfg15 (0x3AF) do not exist: illegal.
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x3a102373              # csrr t1,pmpcfg1 (§7.8 ruling)
    la      s7, odd2
    csrr    t1, 0x3A1
    j       fail
odd2:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x3af29073              # csrw pmpcfg15,t0 (§7.8 ruling)
    la      s7, done_ok
    csrw    0x3AF, t0
    j       fail
done_ok:
    # The trap changed nothing either.
    csrr    t1, mscratch
    bne     t1, t3, fail
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
    mret

.section .tohost
.align 3
tohost:
    .dword 0
