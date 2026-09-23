# Project-authored RV64A guest: LR/SC at the final aligned RAM dword.
# The linker places .tohost last, so its complete 8-byte span is at the image
# RAM boundary. SC writes zero, then an ordinary store requests successful exit.

.section .text
.globl _start
_start:
    la      t0, tohost
    li      t1, 0
    lr.d    t2, (t0)
    sc.d    t3, t1, (t0)
    bnez    t3, fail
    li      t1, 1
    sd      t1, (t0)
1:  j       1b
fail:
    li      t1, 3
    sd      t1, (t0)
2:  j       2b

.section .tohost
.align 3
tohost:
    .dword  0
