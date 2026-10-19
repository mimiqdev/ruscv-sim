# Stage 5 U3 external interrupt.
#
# Privileged specification 20250508 §3.1.9: MEIP is read-only in mip and
# follows the machine PLIC context. MEIE is set only after the gateway
# write. PLIC specification 1.0.0: a 4-byte claim returns source 1 and
# clears pending. A 4-byte complete ends service. A level that is still
# high makes the source pending again. Trap entry does not claim.
# Clearing the level and completing leaves MEIP clear.
# The level word at 0x40000000 is a simulator-only test device. It is
# not a PLIC register. The architectural PLIC map runs through 0x0FFFFFFF.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0

    lui     t0, 0x0C000
    li      t1, 1
    sw      t1, 4(t0)                   # source 1 priority 1

    lui     t0, 0x0C002
    li      t1, 2
    sw      t1, 0(t0)                   # context 0 enable, bit 1

    lui     t0, 0x0C200
    sw      zero, 0(t0)                 # machine threshold 0

    lui     t0, 0x40000
    li      t1, 1
    sw      t1, 0(t0)                   # gateway level 1

    lui     t0, 0x0C001
    lw      t1, 0(t0)
    andi    t1, t1, 2
    li      t2, 2
    bne     t1, t2, fail                # pending bit 1

    csrr    t1, mip
    li      t2, 0x800
    and     t1, t1, t2
    bne     t1, t2, fail                # MEIP before any claim

    li      t0, 0x800                   # MEIE
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
    addi    t1, t1, 11
    bne     t0, t1, mismatch
    csrr    t0, mepc
    bne     t0, s7, mismatch
    csrr    t0, mtval
    bnez    t0, mismatch
    li      t1, 0x22
    bne     s8, t1, mismatch

    lui     t0, 0x0C200
    lw      t1, 4(t0)                   # claim
    li      t2, 1
    bne     t1, t2, mismatch

    lui     t0, 0x0C001
    lw      t1, 0(t0)
    andi    t1, t1, 2
    bnez    t1, mismatch                # claim cleared pending

    csrr    t1, mip
    li      t2, 0x800
    and     t1, t1, t2
    bnez    t1, mismatch                # in service, MEIP clear

    lui     t0, 0x0C200
    li      t1, 1
    sw      t1, 4(t0)                   # complete, level still 1

    lui     t0, 0x0C001
    lw      t1, 0(t0)
    andi    t1, t1, 2
    li      t2, 2
    bne     t1, t2, mismatch            # pending returns

    csrr    t1, mip
    li      t2, 0x800
    and     t1, t1, t2
    bne     t1, t2, mismatch

    lui     t0, 0x0C200
    lw      t1, 4(t0)                   # claim again
    li      t2, 1
    bne     t1, t2, mismatch

    lui     t0, 0x40000
    sw      zero, 0(t0)                 # level 0

    lui     t0, 0x0C200
    li      t1, 1
    sw      t1, 4(t0)                   # complete

    lui     t0, 0x0C001
    lw      t1, 0(t0)
    andi    t1, t1, 2
    bnez    t1, mismatch

    csrr    t1, mip
    li      t2, 0x800
    and     t1, t1, t2
    bnez    t1, mismatch                # MEIP stays clear

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
