# fdc_u6_attach.S — OpenOCD/GDB floating-point session guest (contract D12).
#
# Built for rv64imafdc and linked with linker-s4.ld. Unlike the other debug
# guests, this image needs the debugger: the session writes f0-f31 and fcsr,
# then continues, and the guest verifies those values. It also parks on three
# FP instructions that the debugger breaks on and single-steps, with an
# mstatus.FS check at each stop.
#
# The debugger sets fd_u6_marker before it continues. Without that marker the
# image is a fresh-guest-suite control: it verifies the reset FP state (FS
# Off, fcsr zero, every FPR zero) and exits 0 without executing FP arithmetic.
#
# Fixed FP stops, stepped by scripts/debug/fd-u6-session.gdb:
#   fd_u6_bp_load   FLD    f8, 0(a0)       FS Clean -> Dirty (FPR write)
#   fd_u6_bp_add    FADD.D f10, f8, f9     FS Clean -> Dirty (FPR write)
#   fd_u6_bp_store  FSW    f11, 12(a0)     FS Clean -> Clean (no FPR write)
#
# Each check increments s2 first, so a failure reports its own exit code
# through the standard HTIF payload ((code << 1) | 1). tohost = 1 is pass.

.macro CHECK_FS value
    addi    s2, s2, 1
    csrr    t1, mstatus
    srli    t2, t1, 13
    andi    t2, t2, 3
    li      t6, \value
    bne     t2, t6, fail
.endm

.macro CHECK_SD value
    addi    s2, s2, 1
    csrr    t1, mstatus
    srli    t1, t1, 63
    li      t6, \value
    bne     t1, t6, fail
.endm

# Compare an FPR with the double at \offset of fd_u6_expected.
.macro CHECK_FPR reg, offset
    addi    s2, s2, 1
    fmv.x.d t0, \reg
    ld      t1, \offset(s0)
    bne     t0, t1, fail
.endm

# The reset state of an FPR is zero.
.macro CHECK_ZERO_FPR reg
    addi    s2, s2, 1
    fmv.x.d t0, \reg
    bnez    t0, fail
.endm

    .equ FS_INITIAL, 0x2000
    .equ FS_CLEAN, 0x4000
    .equ U6_MARKER, 0x55433644

.section .text
.option push
.option norvc
.option norelax
.globl _start

_start:
    li      s2, 0
    la      s0, fd_u6_expected
    la      t0, fd_u6_marker
    ld      t1, 0(t0)
    li      t2, U6_MARKER
    bne     t1, t2, unattached

    # The debugger wrote f0-f31 and fcsr through DM abstract access while FS
    # was Off. Contract D10: that path leaves FS unchanged.
    CHECK_FS 0
    li      t0, FS_INITIAL
    csrs    mstatus, t0
    CHECK_FS 1

    # The debugger wrote fcsr = 0x7b: fflags = 0x1b, frm = 3.
    addi    s2, s2, 1
    csrr    t0, fcsr
    li      t1, 0x7b
    bne     t0, t1, fail

    # The debugger wrote f_i = i + 0.5 for every register. Reading an FPR is a
    # transfer out, so these reads must leave FS Initial.
    CHECK_FPR ft0, 0
    CHECK_FPR ft1, 8
    CHECK_FPR ft2, 16
    CHECK_FPR ft3, 24
    CHECK_FPR ft4, 32
    CHECK_FPR ft5, 40
    CHECK_FPR ft6, 48
    CHECK_FPR ft7, 56
    CHECK_FPR fs0, 64
    CHECK_FPR fs1, 72
    CHECK_FPR fa0, 80
    CHECK_FPR fa1, 88
    CHECK_FPR fa2, 96
    CHECK_FPR fa3, 104
    CHECK_FPR fa4, 112
    CHECK_FPR fa5, 120
    CHECK_FPR fa6, 128
    CHECK_FPR fa7, 136
    CHECK_FPR fs2, 144
    CHECK_FPR fs3, 152
    CHECK_FPR fs4, 160
    CHECK_FPR fs5, 168
    CHECK_FPR fs6, 176
    CHECK_FPR fs7, 184
    CHECK_FPR fs8, 192
    CHECK_FPR fs9, 200
    CHECK_FPR fs10, 208
    CHECK_FPR fs11, 216
    CHECK_FPR ft8, 224
    CHECK_FPR ft9, 232
    CHECK_FPR ft10, 240
    CHECK_FPR ft11, 248
    CHECK_FS 1

    # ---- FLD: an FPR write turns FS from Clean to Dirty ----
    li      t0, FS_CLEAN
    csrw    mstatus, t0
    CHECK_FS 2
    la      a0, fd_u6_vals
.globl fd_u6_bp_load
fd_u6_bp_load:
    fld     f8, 0(a0)
    addi    s2, s2, 1
    fmv.x.d t0, f8
    ld      t1, 0(a0)
    bne     t0, t1, fail
    CHECK_FS 3
    CHECK_SD 1

    # ---- FADD.D: another FPR write, Clean -> Dirty ----
    li      t0, FS_CLEAN
    csrw    mstatus, t0
.globl fd_u6_bp_add
fd_u6_bp_add:
    fadd.d  f10, f8, f9
    addi    s2, s2, 1
    fmv.x.d t0, f10
    ld      t1, 16(a0)
    bne     t0, t1, fail
    CHECK_FS 3
    # 3.0 + 9.5 is exact, so the debugger's fflags bits survive.
    addi    s2, s2, 1
    csrr    t0, fcsr
    li      t1, 0x7b
    bne     t0, t1, fail

    # ---- FSW: a store out of an FPR leaves FS unchanged ----
    flw     f11, 8(a0)
    CHECK_FS 3
    li      t0, FS_CLEAN
    csrw    mstatus, t0
.globl fd_u6_bp_store
fd_u6_bp_store:
    fsw     f11, 12(a0)
    addi    s2, s2, 1
    lw      t0, 12(a0)
    li      t1, 0x3fc00000
    bne     t0, t1, fail
    CHECK_FS 2
    CHECK_SD 0

    # A later reset of the same image runs the unattached path.
    la      t0, fd_u6_marker
    sd      zero, 0(t0)
    j       pass

unattached:
    # Fresh suite run: FS Off at reset, then fcsr and every FPR are zero.
    CHECK_FS 0
    li      t0, FS_INITIAL
    csrs    mstatus, t0
    addi    s2, s2, 1
    csrr    t0, fcsr
    bnez    t0, fail
    CHECK_ZERO_FPR ft0
    CHECK_ZERO_FPR ft1
    CHECK_ZERO_FPR ft2
    CHECK_ZERO_FPR ft3
    CHECK_ZERO_FPR ft4
    CHECK_ZERO_FPR ft5
    CHECK_ZERO_FPR ft6
    CHECK_ZERO_FPR ft7
    CHECK_ZERO_FPR fs0
    CHECK_ZERO_FPR fs1
    CHECK_ZERO_FPR fa0
    CHECK_ZERO_FPR fa1
    CHECK_ZERO_FPR fa2
    CHECK_ZERO_FPR fa3
    CHECK_ZERO_FPR fa4
    CHECK_ZERO_FPR fa5
    CHECK_ZERO_FPR fa6
    CHECK_ZERO_FPR fa7
    CHECK_ZERO_FPR fs2
    CHECK_ZERO_FPR fs3
    CHECK_ZERO_FPR fs4
    CHECK_ZERO_FPR fs5
    CHECK_ZERO_FPR fs6
    CHECK_ZERO_FPR fs7
    CHECK_ZERO_FPR fs8
    CHECK_ZERO_FPR fs9
    CHECK_ZERO_FPR fs10
    CHECK_ZERO_FPR fs11
    CHECK_ZERO_FPR ft8
    CHECK_ZERO_FPR ft9
    CHECK_ZERO_FPR ft10
    CHECK_ZERO_FPR ft11

pass:
    li      t0, 1
    la      t1, tohost
    sd      t0, 0(t1)
1:  j       1b

.option pop

fail:
    slli    t0, s2, 1
    ori     t0, t0, 1
    la      t1, tohost
    sd      t0, 0(t1)
1:  j       1b

.section .data
.align 3
.globl fd_u6_marker
fd_u6_marker:
    .word   0
    .word   0
.globl fd_u6_expected
fd_u6_expected:
    .double 0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.5
    .double 8.5, 9.5, 10.5, 11.5, 12.5, 13.5, 14.5, 15.5
    .double 16.5, 17.5, 18.5, 19.5, 20.5, 21.5, 22.5, 23.5
    .double 24.5, 25.5, 26.5, 27.5, 28.5, 29.5, 30.5, 31.5

.align 3
.globl fd_u6_vals
fd_u6_vals:
    .double 3.0
    .float  1.5
    .word   0
    .double 12.5

.section .tohost
.align 3
.globl tohost
.globl fromhost
tohost:
    .dword  0
fromhost:
    .dword  0
