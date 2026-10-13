# Stage 4 F-and-D-absent guest (dev-plan §4 "F and D absent", M2).
#
# misa reads 0x8000_0000_0014_1101.  An all-ones write to mstatus and to
# sstatus leaves FS and SD 0.  In M, in S, and in U, FLW, a misaligned FSD,
# FADD.S with rm=DYN, FADD.D, FMADD.S, and one LOAD-FP encoding whose
# funct3 is a vector width each raise illegal-instruction with mtval equal
# to that instruction's bits, no memory effect, and no FPR/FCSR change.
# In S and in U an FSD through an Sv39 mapping whose leaf has A=0 faults
# before any data walk.  csrr/csrw on 0x001..0x003 raise illegal.

    .equ CAUSE_ILLEGAL, 2
    .equ ANY, -2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1
    li      s4, 0x1800                  # default: stay in M

    # misa is exactly the Stage 4 value.
    li      t2, 0x8000000000141101
    csrr    t1, misa
    bne     t1, t2, fail

    # All-ones writes leave FS (14:13) and SD (63) at 0.
    li      t0, -1
    csrw    mstatus, t0
    csrr    t1, mstatus
    srl     t3, t1, 63
    bnez    t3, fail
    srl     t3, t1, 13
    andi    t3, t3, 3
    bnez    t3, fail
    csrw    sstatus, t0
    csrr    t1, sstatus
    srl     t3, t1, 63
    bnez    t3, fail
    srl     t3, t1, 13
    andi    t3, t3, 3
    bnez    t3, fail

    # ---- M-mode FP traps, each with mtval = the instruction bits ----
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x0002a087              # FLW f1,0(t0)
    la      s7, m_fsd
    .word   0x0002a087
    j       fail
m_fsd:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x0022b127              # FSD misaligned f2,3(t0)
    la      s7, m_fadds
    .word   0x0022b127
    j       fail
m_fadds:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003170d3              # FADD.S f1,f2,f3,dyn
    la      s7, m_faddd
    .word   0x003170d3
    j       fail
m_faddd:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x023170d3              # FADD.D f1,f2,f3,dyn
    la      s7, m_fmadds
    .word   0x023170d3
    j       fail
m_fmadds:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x083170c3              # FMADD.S f1,f2,f3,f4,dyn
    la      s7, m_vecwidth
    .word   0x083170c3
    j       fail
m_vecwidth:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x00028087              # LOAD-FP funct3=0 (vector width)
    la      s7, m_fcsr
    .word   0x00028087
    j       fail
m_fcsr:
    # fflags/frm/fcsr (0x001..0x003) do not exist.
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x00102ef3              # csrr t4,fflags
    la      s7, m_frm
    csrr    t4, 0x001
    j       fail
m_frm:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x00202ef3              # csrr t4,frm
    la      s7, m_fcsr2
    csrr    t4, 0x002
    j       fail
m_fcsr2:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x00302ef3              # csrr t4,fcsr
    la      s7, to_s
    csrr    t4, 0x003
    j       fail

to_s:
    # ---- S-mode: same FP traps, plus FSD through an A=0 mapped leaf ----
    la      t0, s_code
    csrw    mepc, t0
    li      t0, 0x880
    csrw    mstatus, t0
    mret
s_code:
    li      s4, 0x800
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x0002a087              # FLW in S
    la      s7, s_fadds
    .word   0x0002a087
    j       fail
s_fadds:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x003170d3              # FADD.S in S
    la      s7, s_mapped_fsd
    .word   0x003170d3
    j       fail

s_mapped_fsd:
    # Build a one-level Sv39 map: root[vpn2=1] is a 1 GiB superpage leaf
    # mapping VA 0x4000_0000 -> PA 0x8000_0000 R|W with A=0, D=0.  Enable
    # Sv39, execute FSD to a mapped VA, and then verify the A bit is still
    # 0: the illegal-instruction trap happened before any data walk.
    la      t0, fpd_root
    li      t3, 0x80000                 # PPN of the 1 GiB PA 0x8000_0000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f               # V | R | W | X
    sd      t3, 8(t0)                  # root[vpn2=1]: data map VA 0x4000_0000
    li      t3, 0x80000                # identity superpage for the image
    slli    t3, t3, 10
    ori     t3, t3, 0x0f               # V | R | W | X
    sd      t3, 16(t0)                 # root[vpn2=2]: VA 0x8000_0000 -> PA 0x8000_0000

    la      t0, fpd_root
    srli    t0, t0, 12                  # satp PPN
    li      t1, 8
    slli    t1, t1, 60                  # Sv39 mode
    or      t0, t0, t1
    csrw    satp, t0

    li      a3, 0x40000123              # mapped VA, misaligned for FSD
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x0026b127              # FSD f2,0(a3)
    la      s7, s_check_ad
    .word   0x0026b127                  # FSD f2,0(a3)
    j       fail
s_check_ad:
    # Back to Bare, then read the leaf PTE: A must still be 0.
    csrw    satp, x0
    la      t1, fpd_root
    ld      t2, 8(t1)
    srli    t2, t2, 6
    andi    t2, t2, 1                   # A bit
    bnez    t2, fail

    # ---- U-mode FP trap ----
    la      t0, u_code
    csrw    sepc, t0
    sret
u_code:
    li      s4, 0x1800                  # return to M
    li      s5, CAUSE_ILLEGAL
    li      s6, 0x0002a087              # FLW in U
    la      s7, done_ok
    .word   0x0002a087
    j       fail
done_ok:
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
    mv      t2, s5
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
    csrw    mepc, s7
    csrw    mstatus, s4
    mret

    .align 12
fpd_root:
    .zero 4096

.section .tohost
.align 3
tohost:
    .dword 0
