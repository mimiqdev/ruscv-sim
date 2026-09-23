# Project-authored RV64A guest: retry an LR/SC increment until it succeeds.
# The guest reports success only when the committed word is exactly one.

.section .text
.globl _start
_start:
    la      t0, atomic_word
    li      t1, 1
    lrsc_retry:
    lr.d.aq t2, (t0)
    addi    t2, t2, 1
    sc.d.rl t3, t2, (t0)
    bnez    t3, lrsc_retry
    ld      t4, (t0)
    bne     t4, t1, fail
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
    .dword  0

.section .tohost
.align 3
tohost:
    .dword  0
