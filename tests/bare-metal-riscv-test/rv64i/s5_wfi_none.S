# Stage 5 U4. No locally enabled interrupt and no future platform event.
#
# mie stays 0, so a future mtimecmp is not a wake. The run stops with
# NoProgress. The instruction after WFI does not retire.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    addi    t0, zero, 0                 # enables stay clear
    csrw    mie, t0
    li      s8, 0x22
    wfi
    addi    s8, s8, 1
    j       fail

trap_handler:
    li      a0, 1
    j       write_exit

fail:
    li      a0, 1

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)
hang:
    j       hang

.section .tohost
.align 3
tohost:
    .dword 0
