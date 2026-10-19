# Stage 5 U1 clock and counters.
#
# Privileged specification 20250508 §3.2.1: time is a read-only shadow of
# mtime, and mtime is not mcycle. §3.1.11: mcounteren is WARL and stays 0,
# so S-mode and U-mode reads of time are illegal. ACLINT 1.0-rc4 resets
# MTIME to 0 (line 198) and MSIP to 0 with only bit 0 writable (line 344).
# MTIMECMP reset is unknown (line 213). This profile uses all-ones.
# One completed Hart turn adds one to mtime after the instruction, so the
# next load sees the stored value plus one and the following time read
# sees that load's value plus one.

    .equ CAUSE_ILLEGAL, 2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    li      s4, 0x1800
    li      s5, -1

    lui     s0, 0x0200C
    addi    s0, s0, -8                  # mtime at 0x0200_BFF8
    lui     s1, 0x02004                 # mtimecmp at 0x0200_4000
    lui     s2, 0x02000                 # msip at 0x0200_0000

    ld      t1, 0(s1)
    li      t2, -1
    bne     t1, t2, fail
    li      t0, 0x1122334455667788
    sd      t0, 0(s1)
    ld      t1, 0(s1)
    bne     t1, t0, fail

    li      t0, -1
    sw      t0, 0(s2)
    lw      t1, 0(s2)
    li      t2, 1
    bne     t1, t2, fail
    sw      zero, 0(s2)
    lw      t1, 0(s2)
    bnez    t1, fail

    li      t0, 0x5000
    sd      t0, 0(s0)                   # mtime becomes 0x5000, then the turn ticks
    ld      t1, 0(s0)                   # reads 0x5001
    csrr    t3, time                    # reads 0x5002
    addi    t2, t0, 1
    bne     t1, t2, fail
    addi    t2, t1, 1
    bne     t3, t2, fail

    li      t0, 1
    slli    t0, t0, 32
    csrw    mcycle, t0
    ld      t1, 0(s0)
    li      t2, 0x8000
    bgeu    t1, t2, fail

    li      t0, 1
    slli    t0, t0, 60
    csrr    s10, mcycle
    csrr    s11, minstret
    sd      t0, 0(s0)
    csrr    t1, mcycle
    csrr    t2, minstret
    sub     t3, t1, s10
    li      t4, 4
    bgeu    t3, t4, fail
    sub     t3, t2, s11
    bgeu    t3, t4, fail

    li      s5, CAUSE_ILLEGAL
    li      s6, 0xc0129073              # csrw time, t0
    la      s7, to_s
    csrw    time, t0
    j       fail

to_s:
    la      t0, s_time
    csrw    mepc, t0
    li      t0, 0x880                   # MPIE | MPP = S
    csrw    mstatus, t0
    mret
s_time:
    li      s5, CAUSE_ILLEGAL
    li      s4, 0x800                   # return to S
    li      s6, 0xc0102373              # csrr t1, time
    la      s7, to_u
    csrr    t1, time
    j       fail
to_u:
    la      t0, u_time
    csrw    sepc, t0
    sret
u_time:
    li      s5, CAUSE_ILLEGAL
    li      s4, 0x1800
    li      s6, 0xc0102373
    la      s7, done_ok
    csrr    t1, time
    j       fail

done_ok:
    li      a0, 0
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

trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    bne     t0, s5, mismatch
    bne     t1, s6, mismatch
    li      s5, -1
    csrw    mepc, s7
    csrw    mstatus, s4
    mret
mismatch:
    csrr    a0, mcause
    j       write_exit

.section .tohost
.align 3
tohost:
    .dword 0
