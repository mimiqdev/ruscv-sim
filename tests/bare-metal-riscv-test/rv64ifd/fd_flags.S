# fd_flags.S — exception-flag accrual under rv64imafd.
#
# Contract D4/D5: every exception is detected with tininess after rounding and
# only accrues `fflags`; no floating-point exception ever traps. `fflags` is
# five bits, `frm` is three bits, and `fcsr` aliases them exactly with the
# remaining bits reading zero.

.macro CHECK_I reg, value
    addi    s2, s2, 1
    li      t6, \value
    bne     \reg, t6, fail
.endm

    .equ FS_DIRTY, 0x6000
    .equ NX, 0x01
    .equ UF, 0x02
    .equ OF, 0x04
    .equ DZ, 0x08
    .equ NV, 0x10

.section .text
.globl _start

_start:
    li      s2, 0
    la      s1, tohost
    li      t0, FS_DIRTY
    csrs    mstatus, t0
    csrw    fcsr, x0
    la      s3, scratch

    # ---- an exact result accrues nothing ----
    li      t0, 0x3f800000             # 1.0f
    fmv.w.x f1, t0
    li      t0, 0x40000000             # 2.0f
    fmv.w.x f2, t0
    csrw    fflags, x0
    fadd.s  f3, f1, f2
    fmv.x.w t1, f3
    CHECK_I t1, 0x40400000             # 3.0f
    csrr    t1, fflags
    CHECK_I t1, 0

    # ---- inexact: NX only ----
    li      t0, 0x40400000             # 3.0f
    fmv.w.x f4, t0
    csrw    fflags, x0
    fdiv.s  f5, f1, f4                 # 1/3
    csrr    t1, fflags
    CHECK_I t1, NX

    # ---- tininess after rounding is decided on the unbounded result ----
    # 2^-126 * (1 - 2^-24) = 2^-127 * 1.111...1 (23 ones), which is exactly
    # representable with 24 significand bits but lies strictly below the
    # smallest normal. IEEE 754 tininess after rounding therefore reports it
    # tiny: UF and NX accrue even though the bounded result rounds up to the
    # smallest normal 2^-126.
    li      t0, 0x00800000             # 2^-126, smallest normal
    fmv.w.x f6, t0
    li      t0, 0x3f7fffff             # 1 - 2^-24
    fmv.w.x f7, t0
    csrw    fflags, x0
    fmul.s  f8, f6, f7
    fmv.x.w t1, f8
    CHECK_I t1, 0x00800000
    csrr    t1, fflags
    CHECK_I t1, 0x03                   # UF | NX

    # ---- an exact subnormal result accrues nothing ----
    li      t0, 0x3f000000             # 0.5f
    fmv.w.x f17, t0
    csrw    fflags, x0
    fmul.s  f8, f6, f17
    fmv.x.w t1, f8
    CHECK_I t1, 0x00400000             # 2^-127
    csrr    t1, fflags
    CHECK_I t1, 0

    # ---- an inexact subnormal result sets UF and NX ----
    li      t0, 0x3f400001             # 0.75 + 2^-24
    fmv.w.x f9, t0
    csrw    fflags, x0
    fmul.s  f10, f6, f9
    fmv.x.w t1, f10
    CHECK_I t1, 0x00600000             # 3 * 2^-128
    csrr    t1, fflags
    CHECK_I t1, 0x03                   # UF | NX

    # ---- overflow sets OF and NX and returns infinity ----
    li      t0, 0x7f7fffff             # largest finite single
    fmv.w.x f11, t0
    csrw    fflags, x0
    fmul.s  f12, f11, f2               # * 2.0
    fmv.x.w t1, f12
    CHECK_I t1, 0x7f800000
    csrr    t1, fflags
    CHECK_I t1, 0x05                   # OF | NX

    # ---- division by zero sets DZ and returns infinity ----
    csrw    fflags, x0
    li      t0, 0x00000000             # +0.0f
    fmv.w.x f13, t0
    fdiv.s  f14, f1, f13               # 1.0 / 0.0
    fmv.x.w t1, f14
    CHECK_I t1, 0x7f800000
    csrr    t1, fflags
    CHECK_I t1, DZ

    # ---- invalid operation sets NV and returns the canonical NaN ----
    li      t0, 0x7f800000             # +inf
    fmv.w.x f15, t0
    csrw    fflags, x0
    fsub.s  f16, f15, f15              # inf - inf
    fmv.x.w t1, f16
    CHECK_I t1, 0x7fc00000
    csrr    t1, fflags
    CHECK_I t1, NV

    # ---- accrued flags survive unrelated integer work ----
    li      t0, -1
    addi    t1, t0, 1
    csrr    t2, fflags
    CHECK_I t2, NV

    # ---- fflags is five bits and fcsr aliases it ----
    li      t0, -1
    csrw    fflags, t0
    csrr    t1, fflags
    CHECK_I t1, 0x1f

    li      t0, 3                      # frm = RUP
    csrw    frm, t0
    li      t0, 0x15
    csrw    fflags, t0
    csrr    t1, fcsr
    CHECK_I t1, 0x75                   # (3 << 5) | 0x15

    li      t0, 0xff
    csrw    fcsr, t0
    csrr    t1, frm
    CHECK_I t1, 7
    csrr    t1, fflags
    CHECK_I t1, 0x1f
    csrr    t1, fcsr
    CHECK_I t1, 0xff

    # ---- fcsr bits above the eight defined ones read zero ----
    li      t0, -1
    csrw    fcsr, t0
    csrr    t1, fcsr
    CHECK_I t1, 0xff

    li      a0, 0
    j       write_exit

fail:
    mv      a0, s2

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

.section .data
.align 3
scratch:
    .dword 0

.section .tohost
.align 3
tohost:
    .dword 0
