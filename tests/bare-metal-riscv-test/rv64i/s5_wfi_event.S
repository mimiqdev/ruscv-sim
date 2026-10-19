# Stage 5 U4. A legal WFI retires, then waits.
#
# Privileged specification 20250508 §3.3.3: wake is mip & mie, not MIE.
# The following boundary takes the timer interrupt. mepc is the WFI
# address plus 4. The continue/run jumps mtime to mtimecmp and does not
# add that distance to minstret.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    lui     s0, 0x0200C
    addi    s0, s0, -8                  # mtime at 0x0200_BFF8
    lui     s1, 0x02004                 # mtimecmp at 0x0200_4000
    ld      t0, 0(s0)
    li      t1, 0x1000
    add     t0, t0, t1
    sd      t0, 0(s1)
    mv      s3, t0                      # compare the jump must reach
    li      t0, 0x80                    # MTIE
    csrw    mie, t0
    li      t0, 8
    csrs    mstatus, t0                 # MIE
    li      s8, 0x22
    la      s7, victim
    csrr    s4, minstret
    csrr    s5, time
    csrr    s6, mcycle
    wfi
victim:
    addi    s8, zero, 0x99
    j       fail

trap_handler:
    csrr    t0, mcause
    li      t1, 1
    slli    t1, t1, 63
    addi    t1, t1, 7
    bne     t0, t1, mismatch
    csrr    t0, mepc
    bne     t0, s7, mismatch
    csrr    t0, mtval
    bnez    t0, mismatch
    li      t1, 0x22
    bne     s8, t1, mismatch
    csrr    t0, time
    bltu    t0, s3, mismatch
    sub     t2, t0, s5
    li      t3, 0x800
    bltu    t2, t3, mismatch            # the idle jump, not the trap tick
    csrr    t1, minstret
    sub     t1, t1, s4
    li      t3, 0x100
    bgeu    t1, t3, mismatch            # minstret did not move by the jump
    csrr    t1, mcycle
    sub     t1, t1, s6
    bgeu    t1, t3, mismatch            # mcycle did not move by the jump
    li      a0, 0
    j       write_exit

mismatch:
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
