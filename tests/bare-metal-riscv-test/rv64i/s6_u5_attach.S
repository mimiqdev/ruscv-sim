# Stage 6 U5 guest. Unattached, this falls through to the HTIF exit and does
# not enter the spin. A debugger single-steps the first addi, plants a
# software breakpoint on the second addi, writes a2 and steps that addi, then
# parks the hart on s6_spin. After `monitor reset halt` the same image runs
# from _start to the tohost word.

.section .text
.globl _start
.globl s6_break
.globl s6_spin
.globl s6_exit

_start:
    addi    a0, zero, 0x42
    addi    a1, a0, 1
s6_break:
    addi    a2, a2, 1
    j       s6_exit

s6_spin:
    j       s6_spin

s6_exit:
    li      t0, 1
    la      t1, tohost
    sd      t0, 0(t1)
    j       s6_exit

.section .data
.align 3
.globl s6_scratch
s6_scratch:
    .dword  0

.section .tohost
.align 3
tohost:
    .dword  0
