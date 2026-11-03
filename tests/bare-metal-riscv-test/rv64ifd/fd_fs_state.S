# fd_fs_state.S — mstatus.FS Off traps and Dirty marking under rv64imafd.
#
# Contract D6. FS and SD reset to Off/0. While FS is Off every F/D and Zcd
# instruction, and every fflags/frm/fcsr CSR instruction, raises
# illegal-instruction with mtval equal to the encoding and no architectural FP
# effect. FS becomes Dirty when a retired instruction writes an FPR, writes
# fflags/frm/fcsr through CSRRW/CSRRWI, or accrues a nonzero flag set. FSW and
# an exception-free FEQ/FLT/FLE/FCLASS/FCVT-to-integer leave FS unchanged, and
# SD reads as FS == Dirty in both mstatus and the sstatus view.
#
# Each check increments s2 first, so a failure reports its own number.

.macro CHECK_I reg, value
    addi    s2, s2, 1
    li      t6, \value
    bne     \reg, t6, fail
.endm

.macro CHECK_FS value
    addi    s2, s2, 1
    csrr    t1, mstatus
    srli    t2, t1, 13
    andi    t2, t2, 3
    li      t6, \value
    bne     t2, t6, fail
.endm

.macro CHECK_SD reg, value
    addi    s2, s2, 1
    csrr    t1, \reg
    srli    t2, t1, 63
    li      t6, \value
    bne     t2, t6, fail
.endm

# The instruction traps with illegal-instruction and mtval equal to `word`,
# then execution resumes at `resume`. Reaching `j unexpected` means it did not
# trap, which is a failure.
.macro EXPECT_ILLEGAL word, resume
    addi    s2, s2, 1
    li      s5, CAUSE_ILLEGAL
    li      s6, \word
    la      s7, \resume
    .word   \word
    j       unexpected
\resume:
.endm

    .equ CAUSE_ILLEGAL, 2
    .equ FS_OFF, 0
    .equ FS_INITIAL, 1
    .equ FS_CLEAN, 2
    .equ FS_DIRTY_STATE, 3
    .equ MSTATUS_FS_INITIAL, 0x2000
    .equ MSTATUS_FS_CLEAN, 0x4000

.section .text
.globl _start

_start:
    li      s2, 0
    la      s1, tohost
    la      t0, trap_handler
    csrw    mtvec, t0
    li      s5, -1
    li      s6, -1

    # ---- reset state: FS and SD are Off ----
    CHECK_FS FS_OFF
    CHECK_SD mstatus, 0

    # ---- with FS Off every FP instruction class traps ----
    EXPECT_ILLEGAL 0x003170d3, off_fadd       # fadd.s f1,f2,f3,dyn
    EXPECT_ILLEGAL 0x0002a087, off_flw        # flw f1,0(t0)
    EXPECT_ILLEGAL 0x0012a027, off_fsw        # fsw f1,0(t0)
    EXPECT_ILLEGAL 0x0002b087, off_fld        # fld f1,0(t0)
    EXPECT_ILLEGAL 0x0012b027, off_fsd        # fsd f1,0(t0)
    EXPECT_ILLEGAL 0xe0008353, off_fmv_x      # fmv.x.w t1,f1
    EXPECT_ILLEGAL 0xf20280d3, off_fmv_dx     # fmv.d.x f1,t0
    EXPECT_ILLEGAL 0xa020a353, off_feq        # feq.s t1,f1,f2
    EXPECT_ILLEGAL 0xe0009353, off_fclass     # fclass.s t1,f1
    EXPECT_ILLEGAL 0xc000f353, off_fcvt       # fcvt.w.s t1,f1
    EXPECT_ILLEGAL 0x00102373, off_csrr_flags # csrr t1,fflags
    EXPECT_ILLEGAL 0x00229073, off_csrw_frm   # csrw frm,t0
    EXPECT_ILLEGAL 0x00329073, off_csrw_fcsr  # csrw fcsr,t0

    # A trapping instruction is not a retired FP instruction: FS stays Off.
    CHECK_FS FS_OFF
    CHECK_SD mstatus, 0

    # ---- FS = Initial: a retired FPR write makes FS Dirty ----
    li      t0, MSTATUS_FS_INITIAL
    csrw    mstatus, t0
    CHECK_FS FS_INITIAL
    CHECK_SD mstatus, 0

    li      t0, 0x3ff0000000000000     # 1.0
    fmv.d.x f1, t0
    fadd.d  f2, f1, f1                 # exact, no flags
    CHECK_FS FS_DIRTY_STATE
    CHECK_SD mstatus, 1
    # The sstatus view agrees.
    CHECK_SD sstatus, 1

    # ---- FS = Clean: transfers and exception-free compares leave it clean ----
    li      t0, MSTATUS_FS_CLEAN
    csrw    mstatus, t0
    CHECK_FS FS_CLEAN

    la      s3, scratch
    fsd     f2, 0(s3)                  # no FPR write, no flags
    CHECK_FS FS_CLEAN

    li      t0, 0x3ff0000000000000
    fmv.d.x f3, t0                     # this one does write an FPR: Dirty
    li      t0, MSTATUS_FS_CLEAN
    csrw    mstatus, t0

    li      t0, 0x3f800000
    fmv.w.x f4, t0                     # Dirty again, reset below
    li      t0, MSTATUS_FS_CLEAN
    csrw    mstatus, t0
    fsw     f4, 8(s3)                  # FSW: no FPR write, no flags
    CHECK_FS FS_CLEAN

    feq.s   t1, f4, f4                 # true, no exception
    CHECK_I t1, 1
    CHECK_FS FS_CLEAN

    fclass.s t1, f4                    # no exception
    CHECK_I t1, 0x040
    CHECK_FS FS_CLEAN

    fcvt.w.s t1, f4, rtz               # exact conversion
    CHECK_I t1, 1
    CHECK_FS FS_CLEAN

    # A read of fflags through CSRRS with rs1 = x0 does not write it.
    csrr    t1, fflags
    CHECK_FS FS_CLEAN

    # ---- FS = Clean: a nonzero flag set makes FS Dirty ----
    li      t0, 0x40400000             # 3.0f
    fmv.w.x f5, t0
    li      t0, MSTATUS_FS_CLEAN
    csrw    mstatus, t0
    fdiv.s  f6, f4, f5                 # 1/3 accrues NX
    CHECK_FS FS_DIRTY_STATE
    CHECK_SD mstatus, 1

    # An exception-free FCVT to integer stays clean even with flags cleared.
    li      t0, MSTATUS_FS_CLEAN
    csrw    mstatus, t0
    csrwi   fflags, 0
    li      t0, MSTATUS_FS_CLEAN
    csrw    mstatus, t0
    fcvt.w.s t1, f4, rtz
    CHECK_FS FS_CLEAN

    # ---- FS = Clean: CSRRW to fflags always writes it ----
    csrw    fflags, x0
    CHECK_FS FS_DIRTY_STATE

    li      a0, 0
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

fail:
    mv      a0, s2
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
    csrr    t0, mcause
    csrr    t1, mtval
    li      t4, -1
    beq     s5, t4, unexpected
    bne     t0, s5, mismatch
    bne     t1, s6, mismatch
    li      s5, -1
    csrw    mepc, s7
    mret

mismatch:
    csrr    a0, mcause
    ori     a0, a0, 0x100
    j       write_exit

.section .data
.align 3
scratch:
    .dword 0, 0

.section .tohost
.align 3
tohost:
    .dword 0
