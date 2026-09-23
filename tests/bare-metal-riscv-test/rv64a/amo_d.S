# Project-authored RV64A guest: AMOSWAP/ADD/MIN/MAX at doubleword width.
# The signed operands exercise each old-value/result path over a full 64 bits.

.section .text
.globl _start
_start:
    la      t0, atomic_word
    li      t1, -9
    amoswap.d t2, t1, (t0)
    li      t3, 10
    bne     t2, t3, fail

    li      t1, 20
    amoadd.d t4, t1, (t0)
    li      t3, -9
    bne     t4, t3, fail

    li      t1, -3
    amomin.d t5, t1, (t0)
    li      t3, 11
    bne     t5, t3, fail

    li      t1, 4
    amomax.d t6, t1, (t0)
    li      t3, -3
    bne     t6, t3, fail

    ld      t4, (t0)
    li      t3, 4
    bne     t4, t3, fail
    li      a0, 0
    j       exit
fail:
    li      a0, 1
exit:
    la      t0, tohost
    slli    a0, a0, 1
    ori     a0, a0, 1
    sd      a0, (t0)
1:  j       1b

.section .data
.align 3
atomic_word:
    .dword  10

.section .tohost
.align 3
tohost:
    .dword  0
