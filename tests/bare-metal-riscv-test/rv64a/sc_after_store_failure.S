# Project-authored RV64A guest: an overlapping ordinary store breaks SC.

.section .text
.globl _start
_start:
    la      t0, atomic_word
    li      t1, 1
    sd      t1, (t0)
    lr.d    t2, (t0)
    li      t1, 2
    sd      t1, (t0)             # committed overlapping write invalidates LR
    li      t1, 3
    sc.d    t3, t1, (t0)
    beqz    t3, fail             # SC must fail (rd=1)
    ld      t4, (t0)
    li      t5, 2
    bne     t4, t5, fail         # host-visible store must remain
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
