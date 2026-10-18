# Stage 5 U2 timer interrupt.
#
# Privileged specification 20250508 §3.1.9: MTIP is read-only in mip and
# follows unsigned mtime >= mtimecmp. MTIE is set only after the compare
# write. §3.2.1: that write is visible at the next instruction. This guest
# does not read the mtimecmp reset value. The handler sees mepc and the
# register the interrupted instruction would have written.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    lui     s1, 0x02004                 # mtimecmp at 0x0200_4000

    sd      zero, 0(s1)
    csrr    t1, mip
    li      t2, 0x80
    and     t1, t1, t2
    bne     t1, t2, fail

    li      t0, 0x80                    # MTIE
    csrw    mie, t0
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
    addi    t1, t1, 7
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
