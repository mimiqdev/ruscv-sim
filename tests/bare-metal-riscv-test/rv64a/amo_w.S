# Project-authored RV64A guest: AMOSWAP/ADD/MIN/MAX at word width.
# Values discriminate old-result sign extension and signed MIN/MAX behavior.

.section .text
.globl _start
_start:
    la      t0, atomic_word
    li      t1, -1
    amoswap.w t2, t1, (t0)
    li      t3, 0x7fffffff
    bne     t2, t3, fail

    li      t1, 2
    amoadd.w t4, t1, (t0)
    li      t3, -1
    bne     t4, t3, fail

    li      t1, -5
    amomin.w t5, t1, (t0)
    li      t3, 1
    bne     t5, t3, fail

    li      t1, 6
    amomax.w t6, t1, (t0)
    li      t3, -5
    bne     t6, t3, fail

    lw      t4, (t0)
    li      t3, 6
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
    .word   0x7fffffff

.section .tohost
.align 3
tohost:
    .dword  0
