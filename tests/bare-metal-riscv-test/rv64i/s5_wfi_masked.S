# Stage 5 U4. M-mode WFI with MIE clear.
#
# Privileged specification 20250508 §3.3.3: a locally enabled pending
# interrupt wakes the hart even when MIE is clear. The trap is not taken.
# Execution continues at the WFI address plus 4 and MTIP stays pending.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    lui     s0, 0x0200C
    addi    s0, s0, -8
    lui     s1, 0x02004
    ld      t0, 0(s0)
    li      t1, 0x1000
    add     t0, t0, t1
    sd      t0, 0(s1)
    mv      s3, t0
    li      t0, 0x80                    # MTIE, MIE stays 0
    csrw    mie, t0
    li      s8, 0x22
    wfi
    addi    s8, s8, 1                   # must run
    csrr    t0, time
    bltu    t0, s3, fail
    csrr    t0, mip
    andi    t0, t0, 0x80
    beqz    t0, fail
    csrr    t0, mcause
    bnez    t0, fail
    csrr    t0, mepc
    bnez    t0, fail
    li      t1, 0x23
    bne     s8, t1, fail
    li      a0, 0
    j       write_exit

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
