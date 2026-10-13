# Stage 4 TVM/TSR/TW guest (dev-plan §4 "TVM, TSR, and TW").
#
# TVM, TW, and TSR are M-only fields of mstatus, so S-mode code cannot set
# them.  Every case ecalls to M with a request: s2 = bits to arm, s3 =
# S-mode case entry, s8 = 1 (arm request).  The shared M handler arms the
# bits and mrets into the case.  Plain traps (s8 = 0) verify s5 (expected
# cause, -1 = none), s6 (expected mtval, -2 = any) and return to s7,
# preserving the armed bits; s4 selects the return MPP (0x800 = S,
# 0x1800 = M) for the one M-mode continuation.
#
#   TVM=1, S: csrr satp / csrw satp / sfence.vma -> illegal (2)
#   TVM=0, S: the same three operations retire; satp reads 0 (never on)
#   S-mode read of the M-only minstret -> illegal (2) at TVM=0
#   TSR=1, S: sret -> illegal (2);  TSR=0: sret retires into U
#   TW=0, S: wfi retires;  TW=1, S: wfi -> illegal (2)
#   U: wfi -> illegal (2) at TW=0 and TW=1, mtval = the WFI encoding
#   U: read of the M-only misa -> illegal (2), proving the mode
#   M with TVM|TW|TSR set: satp r/w, sfence, wfi, sret all retire
#
# Any trap while s5 = -1, or a wrong cause/mtval, exits 63 / mcause.

    .equ CAUSE_ILLEGAL, 2
    .equ CAUSE_ECALL_S, 9
    .equ ANY, -2
    .equ TVM, 0x100000
    .equ TW,  0x200000
    .equ TSR, 0x400000
    .equ WFI_ENC, 0x10500073

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1                      # no trap expected yet
    li      s4, 0x800                   # plain returns go to S

    # ---- Enter Supervisor ----
    la      t0, s_entry
    csrw    mepc, t0
    li      t0, 0x880                   # MPIE | MPP = S
    csrw    mstatus, t0
    mret

s_entry:
    # ===== TVM = 1: satp read, write, and sfence.vma all trap =====
    li      s2, TVM
    la      s3, tvm_read_case
    j       request_bits
tvm_read_case:                           # S, TVM=1
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x18002373              # csrr t1,satp (§7.8 ruling)
    la      s7, tvm_write_case
    csrr    t1, satp
    j       fail
tvm_write_case:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x18001073              # csrw satp,x0 (§7.8 ruling)
    la      s7, tvm_sfence_case
    csrw    satp, x0
    j       fail
tvm_sfence_case:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x12000073              # sfence.vma (§7.8 ruling)
    la      s7, tvm_off
    sfence.vma
    j       fail
tvm_off:
    # ===== TVM = 0: the same operations retire =====
    li      s2, 0
    la      s3, tvm0_case
    j       request_bits
tvm0_case:                              # S, TVM=0
    li      s5, -1
    csrr    t1, satp                    # translation never on: reads 0
    bnez    t1, fail
    csrw    satp, x0                    # Bare write retires
    sfence.vma                          # retires

    # S-mode read of the M-only minstret traps at TVM=0.
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xb0202373              # csrr t1,minstret (§7.8 ruling)
    la      s7, tsr_on
    csrr    t1, minstret
    j       fail

tsr_on:
    # ===== TSR = 1: S-mode sret traps =====
    li      s2, TSR
    la      s3, tsr1_case
    j       request_bits
tsr1_case:                              # S, TSR=1
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x10200073              # sret (§7.8 ruling)
    la      s7, wfi_tw0
    sret
    j       fail

    # ===== WFI: TW=0 retires, TW=1 traps =====
wfi_tw0:                                # S, all bits clear
    li      s2, 0
    la      s3, wfi0_case
    j       request_bits
wfi0_case:                              # S, TW=0
    li      s5, -1
    wfi                                 # retires
    li      s2, TW
    la      s3, wfi1_case
    j       request_bits
wfi1_case:                              # S, TW=1
    li      s5, CAUSE_ILLEGAL
    li      s6, WFI_ENC                 # 0x10500073 (§7.8 ruling)
    la      s7, u_wfi_arm
    wfi
    j       fail

u_wfi_arm:                              # S, TW=1 still set
    # ===== U-mode wfi at TW=1, then at TW=0 =====
    li      s5, CAUSE_ILLEGAL
    li      s6, WFI_ENC
    la      s7, u_wfi_tw0_arm
    la      t0, u_wfi_code
    csrw    sepc, t0
    sret                                 # into U (TSR/TVM clear, SPP=0)
    j       fail
u_wfi_code:                             # running in U
    wfi
    j       fail
u_wfi_tw0_arm:
    li      s2, 0                        # clear TW
    la      s3, u_wfi2_case
    j       request_bits
u_wfi2_case:                            # S, TW=0
    li      s5, CAUSE_ILLEGAL
    li      s6, WFI_ENC
    la      s7, u_misa_arm
    la      t0, u_wfi_code
    csrw    sepc, t0
    sret
    j       fail
u_misa_arm:                             # S
    # ===== U-mode read of the M-only misa proves the mode =====
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x30102373              # csrr t1,misa (§7.8 ruling)
    la      s7, tsr0_arm
    la      t0, u_misa_code
    csrw    sepc, t0
    sret
    j       fail
u_misa_code:                            # U
    csrr    t1, misa
    j       fail

tsr0_arm:
    # ===== Machine mode with TVM|TW|TSR set =====
    li      s5, CAUSE_ECALL_S
    li      s6, 0
    la      s7, m_block
    li      s4, 0x1800                  # this continuation runs in M
    li      s2, 0                       # keep the armed bits clear
    ecall
    j       fail
m_block:                                # M
    li      t0, TVM | TW | TSR
    csrs    mstatus, t0
    csrr    t1, satp                    # readable in M with TVM set
    csrw    satp, t0                    # MODE 0 = Bare: value sticks
    csrr    t2, satp
    bne     t2, t0, fail
    sfence.vma                          # legal in M with TVM set
    wfi                                 # retires in M with TW set
    la      t0, m_after_sret
    csrw    sepc, t0
    sret                                 # legal in M with TSR set
    j       fail
m_after_sret:                           # U (SPP was 0)
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x30102373              # csrr t1,misa (§7.8 ruling)
    la      s7, m_exit
    li      s4, 0x1800                  # return to M to write the exit word
    li      s2, 0
    csrr    t1, misa                    # traps: proves we are below M
    j       fail
m_exit:                                 # M again (handler used MPP = M)
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1
    j       write_exit

unexpected:                             # trap while s5 = -1
    li      a0, 63
    j       write_exit

mismatch:                               # wrong cause or mtval
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

# S-mode springboard: leave the arm request and ecall to M.
request_bits:
    li      s8, 1
    li      s5, CAUSE_ECALL_S
    li      s6, 0
    ecall
    j       fail

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
    li      s5, -1
    beqz    s8, plain_return
    # Arm path: set the requested bits and return to the case in S.
    li      s8, 0
    csrr    t5, mstatus
    li      t6, ~(TVM | TW | TSR)
    and     t5, t5, t6
    or      t5, t5, s2
    li      t6, 0x880                   # MPIE | MPP = S
    or      t5, t5, t6
    csrw    mstatus, t5
    csrw    mepc, s3
    mret
plain_return:
    # Preserve the armed bits, return to s7 with the s4-selected MPP.
    csrr    t5, mstatus
    li      t6, ~(TVM | TW | TSR)
    and     t5, t5, t6
    or      t5, t5, s2
    li      t6, 0x80                    # MPIE
    or      t5, t5, t6
    or      t5, t5, s4
    csrw    mstatus, t5
    csrw    mepc, s7
    mret

.section .tohost
.align 3
tohost:
    .dword 0
