# Project-authored HTIF U3 guest: the endpoint's read answer, the standard
# console device's bytes and the standard syscall exit word (htif D8).
#
# The endpoint is the device window 0x4000_8000 with eight bytes. The guest
# reads it, expects zero and exits with code 1 if the answer is not zero. It
# then writes three console bytes (device 1, command 1, payload byte), writes
# the standard syscall exit word for code 0 and self-loops. The self-loop is
# what --fault-htif-drop-event needs: dropping only the exit word's event leaves
# the console events published and makes the run reach its turn bound.

.equ HTIF_BASE,          0x40008000
.equ HTIF_DEVICE_SHIFT,  56
.equ HTIF_CMD_SHIFT,     48
.equ HTIF_CONSOLE,       1
.equ HTIF_CONSOLE_PUT,   1

.section .text
.globl _start
_start:
    lui     t0, 0x40008             # 0x4000_8000, the HTIF endpoint
    ld      t1, 0(t0)               # the endpoint's read answer
    beqz    t1, console
    li      t2, 3                   # standard exit word for code 1: (1 << 1) | 1
    sd      t2, 0(t0)
1:  j       1b

console:
    li      t3, HTIF_CONSOLE
    slli    t3, t3, HTIF_DEVICE_SHIFT
    li      t4, HTIF_CONSOLE_PUT
    slli    t4, t4, HTIF_CMD_SHIFT
    or      t3, t3, t4              # device 1, command 1

    li      t5, 'H'
    or      t6, t3, t5
    sd      t6, 0(t0)
    li      t5, 'i'
    or      t6, t3, t5
    sd      t6, 0(t0)
    li      t5, '\n'
    or      t6, t3, t5
    sd      t6, 0(t0)

    li      t6, 1                   # standard syscall exit word for code 0
    sd      t6, 0(t0)
2:  j       2b

.section .tohost
.align 3
tohost:
    .dword  0
