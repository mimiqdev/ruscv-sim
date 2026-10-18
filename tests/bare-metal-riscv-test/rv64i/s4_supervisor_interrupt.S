# Stage 4 supervisor-interrupt guest (dev-plan §4 "Supervisor interrupt",
# D11 option (a)).
#
# mie keeps 0xAAA. mip keeps 0x222 while no machine line is pending.
# sie and sip read 0 and ignore writes while mideleg is 0. SSIP with SSIE
# is not taken in M while MIE is 0, is taken
# in M once MIE is 1, is taken in S and in U while MIE is 0.  With SEIP,
# SSIP, and STIP pending and enabled together, SEI is taken first; with
# SSIP and STIP together, SSI is taken.  A vectored mtvec enters at
# BASE + 4*cause (proved by a marker only the cause-1 entry sets).  Each
# delivery is checked for mcause, mepc (the interrupted instruction), and
# mtval = 0.

    .equ CAUSE_SSI, 1
    .equ CAUSE_STI, 5
    .equ CAUSE_SEI, 9

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s3, 0                       # vectored-entry marker

    # ---- mie keeps 0xAAA; mip keeps 0x222 with no machine line ----
    li      t0, -1
    csrw    mie, t0
    csrr    t1, mie
    li      t2, 0xAAA
    bne     t1, t2, fail
    csrw    mip, t0
    csrr    t1, mip
    li      t2, 0x222
    bne     t1, t2, fail

    # ---- sie/sip read 0 and ignore writes (mideleg = 0) ----
    csrw    sie, t0
    csrr    t1, sie
    bnez    t1, fail
    csrw    sip, t0
    csrr    t1, sip
    bnez    t1, fail

    # ---- not taken in M while MIE = 0 ----
    csrw    mip, x0                     # clear pending first
    li      t0, (1 << CAUSE_SSI)        # SSIE
    csrw    mie, t0
    li      t0, (1 << CAUSE_SSI)
    csrw    mip, t0                     # SSIP pending, MIE=0, in M
    nop                                  # must retire: no interrupt
    nop

    # ---- taken in M once MIE = 1 ----
    li      s5, CAUSE_SSI               # expected cause
    li      s6, 1                       # mepc check id
    la      s7, m_after                 # continuation in M
    la      t0, m_take
    li      t0, (1 << 3)                # MIE
    csrs    mstatus, t0                 # delivery at the next boundary
m_take:
    nop                                  # interrupted here
m_after:
    csrr    t1, mip
    srli    t1, t1, CAUSE_SSI
    andi    t1, t1, 1
    bnez    t1, fail                    # handler must clear the taken bit

    # ---- taken in S with MIE clear ----
    li      t0, (1 << CAUSE_SSI)
    csrw    mie, t0                     # SSIE (still set)
    li      s5, CAUSE_SSI
    li      s6, 2
    la      s7, back_m
    la      t0, s_take
    csrw    mepc, t0
    li      t0, 0x800                   # MPP = S; MPIE=0 so MIE=0 in S
    csrw    mstatus, t0
    li      t0, (1 << CAUSE_SSI)
    csrw    mip, t0                     # arm last: fires right after mret
    mret
s_take:                                 # S-mode, MIE=0: interrupted here
    nop
    j       fail

back_m:
    # ---- taken in U with MIE clear ----
    li      t0, 8
    csrc    mstatus, t0                 # MIE off so M does not take it first
    li      t0, (1 << CAUSE_SSI)
    csrw    mie, t0
    li      s5, CAUSE_SSI
    li      s6, 3
    la      s7, back_m2
    la      t0, u_take
    csrw    sepc, t0
    li      t0, (1 << CAUSE_SSI)
    csrw    mip, t0                     # arm last: fires right after sret
    sret
u_take:                                 # U-mode: interrupted here
    nop
    j       fail

back_m2:
    # ---- priority: SEI first with all three pending+enabled ----
    li      t0, 8
    csrs    mstatus, t0                 # M-mode delivery needs MIE = 1
    li      t0, 0x222
    csrw    mie, t0
    li      s5, CAUSE_SEI
    li      s6, 4
    la      s7, back_m3
    li      t0, 0x222
    csrw    mip, t0                     # arm last
prio_take:
    nop                                  # interrupted here
    j       fail

back_m3:
    # ---- SSI beats STI with both pending+enabled ----
    li      t0, (1 << CAUSE_SSI) | (1 << CAUSE_STI)
    csrw    mie, t0
    li      s5, CAUSE_SSI
    li      s6, 5
    la      s7, back_m4
    li      t0, (1 << CAUSE_SSI) | (1 << CAUSE_STI)
    csrw    mip, t0                     # arm last
prio2_take:
    nop                                  # interrupted here
    j       fail

back_m4:
    # ---- vectored mtvec enters at BASE + 4*cause ----
    la      t0, vect_base
    ori     t0, t0, 1                   # MODE = vectored
    csrw    mtvec, t0
    li      t0, (1 << CAUSE_SSI)
    csrw    mie, t0
    li      s5, CAUSE_SSI
    li      s6, 6
    la      s7, back_m5
    li      t0, (1 << CAUSE_SSI)
    csrw    mip, t0                     # arm last
vect_take:
    nop                                  # interrupted here
    j       fail

back_m5:
    bnez    s3, ok                      # cause-1 entry set the marker
    j       fail
ok:
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

mismatch:
    csrr    a0, mcause
    j       write_exit

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

trap_handler:
    # Interrupt entry: mcause top bit set with the cause in the low bits,
    # mepc = the interrupted instruction, mtval = 0.  Clear only the taken
    # pending bit, then continue in M at s7.
    li      t4, 1
    slli    t4, t4, 63
    csrr    t0, mcause
    blt     t0, t4, unexpected          # synchronous trap: not expected
    not     t1, t4
    and     t1, t0, t1                  # low bits = cause code
    bne     t1, s5, mismatch
    beqz    s6, skip_epc
    li      t2, 1
    beq     s6, t2, e_m
    li      t2, 2
    beq     s6, t2, e_s
    li      t2, 3
    beq     s6, t2, e_u
    li      t2, 4
    beq     s6, t2, e_prio
    li      t2, 5
    beq     s6, t2, e_prio2
    j       e_vect
e_m:
    la      t3, m_take
    j       check_epc
e_s:
    la      t3, s_take
    j       check_epc
e_u:
    la      t3, u_take
    j       check_epc
e_prio:
    la      t3, prio_take
    j       check_epc
e_prio2:
    la      t3, prio2_take
    j       check_epc
e_vect:
    la      t3, vect_take
check_epc:
    csrr    t5, mepc
    bne     t5, t3, mismatch
skip_epc:
    csrr    t5, mtval
    bnez    t5, mismatch
    # clear all pending bits: later stages re-arm what they need
    csrw    mip, x0
    # continue in M at s7
    li      t5, 0x1880                  # MIE | MPIE | MPP = M
    csrw    mstatus, t5
    csrw    mepc, s7
    mret

    .align 2
vect_base:
    j       unexpected                  # cause 0: must not be taken
    li      s3, 1                       # cause 1 (SSI): set the marker
    j       trap_handler
    j       unexpected                  # cause 2
    j       unexpected                  # cause 3

.section .tohost
.align 3
tohost:
    .dword 0
