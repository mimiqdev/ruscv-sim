# Project-authored RV64A guest: the AMO itself writes the tohost exit signal.
# Public runners must first retire this AMO, then observe and clear the signal.

.section .text
.globl _start
_start:
    la      t0, tohost
    li      t1, 1                # standard HTIF payload: exit code 0
    amoadd.d t2, t1, (t0)
1:  j       1b

.section .tohost
.align 3
tohost:
    .dword  0
