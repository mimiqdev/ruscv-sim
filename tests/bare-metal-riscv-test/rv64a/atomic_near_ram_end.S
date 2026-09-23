# Project-authored RV64A guest: LR/SC at the final aligned RAM dword.
# linker.ld keeps this image below 0x80001008; load_elf_file rounds that span
# up to its minimum 64-KiB allocation [0x80000000, 0x80010000). Thus the final
# aligned dword is 0x8000fff8. A successful full-width SC is followed by an
# ordinary store to .tohost; failure requests a nonzero exit instead.

.section .text
.globl _start
_start:
    li      t0, 0x8000fff8
    li      t1, 0
    lr.d    t2, (t0)
    sc.d    t3, t1, (t0)
    bnez    t3, fail
    la      t0, tohost
    li      t1, 1
    sd      t1, (t0)
1:  j       1b
fail:
    la      t0, tohost
    li      t1, 3
    sd      t1, (t0)
2:  j       2b

.section .tohost
.align 3
tohost:
    .dword  0
