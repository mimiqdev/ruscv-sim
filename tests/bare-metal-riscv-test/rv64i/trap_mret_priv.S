# A6 bare-metal trap regression: MRET executed outside Machine mode.
# First, a legal MRET returns to User mode.  The User-mode MRET must trap as
# IllegalInstruction without restoring MEPC/privilege or applying MRET status
# side effects.  The Machine handler checks the standard illegal trap state,
# repairs MEPC/MPP for a legal return, and the resumed code checks that return.

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0

    # Return to User mode and set MPRV so the legal MRET's clearance is also
    # exercised.  MIE and MPIE are zero, making an illegal User MRET's
    # MPIE=0 distinguishable from a legal MRET's MPIE=1 side effect.
    li      t0, 0x20000
    csrw    mstatus, t0
    la      t0, user_entry
    csrw    mepc, t0
    mret

user_entry:
user_mret_site:
    .word   0x30200073

# The handler changes MEPC and MPP, then MRET returns here in Machine mode.
after_failed_mret:
    li      t0, 1
    bne     s0, t0, fail

    csrr    t0, mstatus
    andi    t1, t0, 8
    bnez    t1, fail
    srli    t1, t0, 7
    andi    t1, t1, 1
    li      t2, 1
    bne     t1, t2, fail
    srli    t1, t0, 11
    andi    t1, t1, 3
    bnez    t1, fail
    srli    t1, t0, 17
    andi    t1, t1, 1
    bnez    t1, fail
    j       pass

trap_handler:
    li      s0, 1

    # Standard illegal-instruction entry from User mode.
    csrr    t0, mcause
    li      t1, 2
    bne     t0, t1, fail

    csrr    t0, mtval
    li      t1, 0x30200073
    bne     t0, t1, fail

    csrr    t0, mepc
    la      t1, user_mret_site
    bne     t0, t1, fail

    # MRET side effects must not have run: MIE remains cleared, MPIE receives
    # the old User-mode MIE=0, and MPP records User mode.
    csrr    t0, mstatus
    andi    t1, t0, 8
    bnez    t1, fail
    srli    t1, t0, 7
    andi    t1, t1, 1
    bnez    t1, fail
    srli    t1, t0, 11
    andi    t1, t1, 3
    bnez    t1, fail
    srli    t1, t0, 17
    andi    t1, t1, 1
    bnez    t1, fail

    # Select Machine mode for the handler's legal return and skip the trapped
    # User-mode MRET.  These are handler-owned recovery effects, not effects
    # of the rejected instruction.
    li      t1, 0x1800
    csrrs   x0, mstatus, t1
    la      t0, after_failed_mret
    csrw    mepc, t0
    mret

pass:
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1

write_exit:
    li      t0, 1
    slli    t0, t0, 63
    or      a0, a0, t0
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

.section .tohost
.align 3
tohost:
    .dword  0
