# Project-authored RV64A guest: all four aq/rl encodings are accepted.
# AMOs cover none/aq/rl/aqrl; LR and SC also carry ordering bits.

.section .text
.globl _start
_start:
    la      t0, atomic_word
    li      t1, 1
    amoswap.d t2, t1, (t0)       # aq=0, rl=0; value becomes 1
    li      t1, 2
    amoadd.d.aq t3, t1, (t0)     # aq=1, rl=0; value becomes 3
    li      t1, 3
    amoadd.d.rl t4, t1, (t0)     # aq=0, rl=1; value becomes 6
    li      t1, 4
    amoadd.d.aqrl t5, t1, (t0)   # aq=1, rl=1; value becomes 10

    lr.d.aq t6, (t0)
    li      t1, 11
    sc.d.rl t2, t1, (t0)
    bnez    t2, fail
    ld      t3, (t0)
    bne     t3, t1, fail
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
