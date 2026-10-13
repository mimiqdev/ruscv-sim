# Stage 4 privilege-transition guest (dev-plan §4 "Privilege transition", D1).
#
# satp stays 0 (Bare), so only the privilege changes. Every trap is taken
# in M (M1). The handler checks mcause, mtval, and mepc exactly, keeps the
# trap-entry mstatus in a5, and returns to the address and mstatus the case
# put in s7 and s8.
#
#   A. mret to U. U-mode `csrr sstatus` is illegal, so the CSR file checks
#      at U. ECALL in U is cause 8.
#   B. mret to S. S-mode `csrr sstatus` retires and `csrr mstatus` is
#      illegal. ECALL in S is cause 9.
#   C. SRET from M to S, with MPRV set and sepc written with bits 1:0 set.
#      sepc reads back with bits 1:0 clear and SRET lands there. SRET sets
#      SPIE, moves SPIE into SIE, clears SPP, and clears MPRV.
#   D. SRET from S to U, again with sepc bits 1:0 written as 1. U-mode
#      `csrr sstatus` is illegal, so the CSR file checks at U.
#   E. SRET in U is illegal (mtval = the SRET encoding) and changes none
#      of SIE, SPIE, SPP, or the privilege.
#
# mtval is 0 for ECALL. For an illegal instruction it is the instruction.

    .equ ANY, -2
    .equ CSRR_SSTATUS, 0x10002373       # csrr t1, sstatus
    .equ CSRR_MSTATUS, 0x30002373       # csrr t1, mstatus
    .equ SRET_BITS, 0x10200073

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1

    # ===== A: mret to U =====
    la      t0, u_a
    csrw    mepc, t0
    li      t0, 0x80                    # MPIE, MPP = U
    csrw    mstatus, t0
    mret

u_a:
    li      s5, 2
    li      s6, CSRR_SSTATUS
    la      s7, u_a2
    li      s8, 0                       # resume in U
    la      s9, u_a_csr
u_a_csr:
    csrr    t1, sstatus                 # illegal in U
    j       fail
u_a2:
    li      s5, 8                       # ECALL from U
    li      s6, 0
    la      s7, m_b
    li      s8, 0x1800                  # resume in M
    la      s9, u_a_ecall
u_a_ecall:
    ecall
    j       fail

m_b:
    # The ECALL came from U: MPP = 0.
    srli    t1, a5, 11
    andi    t1, t1, 3
    bnez    t1, fail

    # ===== B: mret to S =====
    la      t0, s_b
    csrw    mepc, t0
    li      t0, 0x880                   # MPIE, MPP = S
    csrw    mstatus, t0
    mret

s_b:
    csrr    t1, sstatus                 # retires in S
    li      s5, 2
    li      s6, CSRR_MSTATUS
    la      s7, s_b2
    li      s8, 0x800                   # resume in S
    la      s9, s_b_csr
s_b_csr:
    csrr    t1, mstatus                 # illegal in S
    j       fail
s_b2:
    li      s5, 9                       # ECALL from S
    li      s6, 0
    la      s7, m_c
    li      s8, 0x1800
    la      s9, s_b_ecall
s_b_ecall:
    ecall
    j       fail

m_c:
    # The ECALL came from S: MPP = 1.
    srli    t1, a5, 11
    andi    t1, t1, 3
    li      t2, 1
    bne     t1, t2, fail

    # ===== C: SRET from M to S =====
    # MPP = M keeps M's own accesses untranslated while MPRV is set.
    li      t0, 0x21902                 # MPRV | MPP = M | SPP = S | SIE
    csrw    mstatus, t0
    la      t0, s_c
    addi    t0, t0, 3                   # sepc bits 1:0 written as 1
    csrw    sepc, t0
    csrr    t1, sepc
    la      t2, s_c
    bne     t1, t2, fail                # bits 1:0 read 0
    sret
    j       fail

s_c:
    # SRET landed at s_c in S (sstatus is readable). SPP = 0, SPIE = 1,
    # SIE = the old SPIE = 0.
    csrr    t1, sstatus
    andi    t1, t1, 0x122               # SIE | SPIE | SPP
    li      t2, 0x20
    bne     t1, t2, fail
    li      s5, 9
    li      s6, 0
    la      s7, m_d
    li      s8, 0x1800
    la      s9, s_c_ecall
s_c_ecall:
    ecall
    j       fail

m_d:
    # SRET cleared MPRV, and the ECALL that followed came from S.
    li      t2, 0x20000
    and     t1, a5, t2
    bnez    t1, fail
    srli    t1, a5, 11
    andi    t1, t1, 3
    li      t2, 1
    bne     t1, t2, fail

    # ===== D: SRET from S to U =====
    la      t0, s_d
    csrw    mepc, t0
    li      t0, 0x800                   # MPP = S
    csrw    mstatus, t0
    mret

s_d:
    li      t0, 0x2                     # SIE = 1
    csrs    sstatus, t0
    li      t0, 0x120                   # SPIE = 0, SPP = U
    csrc    sstatus, t0
    la      t0, u_d
    addi    t0, t0, 1                   # sepc bit 0 written as 1
    csrw    sepc, t0
    li      s5, 2
    li      s6, CSRR_SSTATUS
    la      s7, u_d2
    li      s8, 0                       # resume in U
    la      s9, u_d
    sret
    j       fail

u_d:
    csrr    t1, sstatus                 # first instruction; illegal in U
    j       fail
u_d2:
    # a5 is the mstatus SRET left: SIE = old SPIE = 0, SPIE = 1, SPP = 0,
    # and the trap came from U.
    mv      a6, a5

    # ===== E: SRET in U =====
    # The handler returned with mstatus = 0, so SIE, SPIE, and SPP are all
    # 0 here. An SRET that applied any effect would set SPIE.
    li      s5, 2
    li      s6, SRET_BITS
    la      s7, u_e
    li      s8, 0
    la      s9, u_e_sret
u_e_sret:
    sret
    j       fail
u_e:
    mv      a7, a5
    li      s5, 8                       # still U: ECALL is cause 8
    li      s6, 0
    la      s7, m_end
    li      s8, 0x1800
    la      s9, u_e_ecall
u_e_ecall:
    ecall
    j       fail

m_end:
    # Case D: SIE | SPIE | SPP = SPIE only, MPP = U.
    andi    t1, a6, 0x122
    li      t2, 0x20
    bne     t1, t2, fail
    srli    t1, a6, 11
    andi    t1, t1, 3
    bnez    t1, fail
    # Case E: the illegal SRET left SIE, SPIE, and SPP at 0, MPP = U.
    andi    t1, a7, 0x122
    bnez    t1, fail
    srli    t1, a7, 11
    andi    t1, t1, 3
    bnez    t1, fail
    # The last ECALL also came from U.
    srli    t1, a5, 11
    andi    t1, t1, 3
    bnez    t1, fail

    li      a0, 0
    j       write_exit

fail:
    li      a0, 1
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

# (mcause << 16 | mtval) << 8 | 0x7f: the low byte is never 0, so the CLI
# process status also fails when mtval's low byte is 0 (ECALL).
mismatch:
    slli    a0, t0, 16
    or      a0, a0, t1
    slli    a0, a0, 8
    ori     a0, a0, 0x7f
    j       write_exit

epc_mismatch:
    li      a0, 62
    j       write_exit

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    sd      a0, 0(s1)
hang:
    j       hang

# Expected mcause in s5, mtval in s6, mepc in s9; recovery address in s7,
# the mstatus value for the return in s8. The trap-entry mstatus goes to a5.
trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    li      t4, -1
    beq     s5, t4, unexpected
    bne     t0, s5, mismatch
    li      t5, ANY
    beq     s6, t5, mtval_ok
    bne     t1, s6, mismatch
mtval_ok:
    csrr    t2, mepc
    bne     t2, s9, epc_mismatch
    csrr    a5, mstatus
    li      s5, -1
    csrw    mepc, s7
    csrw    mstatus, s8
    mret

.section .tohost
.align 3
tohost:
    .dword 0
