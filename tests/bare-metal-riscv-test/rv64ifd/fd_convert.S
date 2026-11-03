# fd_convert.S — conversions, classification and NaN compares under rv64imafd.
#
# Contract D4: the manual's saturation table, RV64 sign extension of 32-bit
# results, the signaling-NaN rules for FEQ/FLT/FLE and FCVT, and FCLASS bits 8
# and 9 distinguished. Each check increments s2 first, so a failure reports
# its own number.

.macro CHECK_I reg, value
    addi    s2, s2, 1
    li      t6, \value
    bne     \reg, t6, fail
.endm

# Compare the low 32 bits of the sign-extended FMV.X.W result.
.macro CHECK_XW reg, value
    addi    s2, s2, 1
    slli    t5, \reg, 32
    srli    t5, t5, 32
    li      t6, \value
    bne     t5, t6, fail
.endm

# Compare the full 64 bits of an FPR through memory.
.macro CHECK_F64 freg, value
    addi    s2, s2, 1
    fsd     \freg, 0(s3)
    ld      t1, 0(s3)
    li      t6, \value
    bne     t1, t6, fail
.endm

    .equ FS_DIRTY, 0x6000
    .equ NX, 0x01
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

    # ---- float to integer saturation table ----
    li      t0, 0x7f800000             # +inf
    fmv.w.x f1, t0
    csrw    fflags, x0
    fcvt.w.s t1, f1, rne
    CHECK_I t1, 0x000000007fffffff
    csrr    t1, fflags
    CHECK_I t1, NV

    li      t0, 0xff800000             # -inf
    fmv.w.x f2, t0
    csrw    fflags, x0
    fcvt.w.s t1, f2, rne
    CHECK_I t1, 0xffffffff80000000
    csrr    t1, fflags
    CHECK_I t1, NV

    li      t0, 0x7fc00000             # canonical quiet NaN
    fmv.w.x f3, t0
    csrw    fflags, x0
    fcvt.w.s t1, f3, rne
    CHECK_I t1, 0x000000007fffffff
    csrr    t1, fflags
    CHECK_I t1, NV

    # ---- the required negative control: a negative value saturates to zero ----
    li      t0, 0xbf800000             # -1.0
    fmv.w.x f4, t0
    csrw    fflags, x0
    fcvt.wu.s t1, f4, rne
    CHECK_I t1, 0
    csrr    t1, fflags
    CHECK_I t1, NV

    li      t0, 0xbf000000             # -0.5 rounds toward zero to 0
    fmv.w.x f5, t0
    csrw    fflags, x0
    fcvt.wu.s t1, f5, rne
    CHECK_I t1, 0
    csrr    t1, fflags
    CHECK_I t1, NX

    csrw    fflags, x0
    fcvt.wu.s t1, f3, rne
    CHECK_I t1, 0xffffffffffffffff
    csrr    t1, fflags
    CHECK_I t1, NV

    # ---- RV64 sign extension of a 32-bit result ----
    csrw    fflags, x0
    fcvt.w.s t1, f4, rtz
    CHECK_I t1, 0xffffffffffffffff
    csrr    t1, fflags
    CHECK_I t1, 0

    # ---- rounding applies to the integer destination too ----
    li      t0, 0x40200000             # 2.5
    fmv.w.x f6, t0
    fcvt.l.s t1, f6, rne
    CHECK_I t1, 2
    fcvt.l.s t1, f6, rup
    CHECK_I t1, 3
    fcvt.l.s t1, f6, rtz
    CHECK_I t1, 2

    # ---- float to float: narrowing rounds, widening is exact ----
    li      t0, 0x3ff8000000000000     # 1.5
    fmv.d.x f7, t0
    csrw    fflags, x0
    fcvt.s.d f8, f7, rne
    fsd     f8, 0(s3)
    ld      t1, 0(s3)
    CHECK_XW t1, 0x3fc00000
    csrr    t1, fflags
    CHECK_I t1, 0

    li      t0, 0x3f000000             # 0.5f
    fmv.w.x f9, t0
    csrw    fflags, x0
    fcvt.d.s f10, f9
    fsd     f10, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x3fe0000000000000
    csrr    t1, fflags
    CHECK_I t1, 0
    # Widening then narrowing returns the same single, exactly.
    fcvt.s.d f11, f10, rne
    fsd     f11, 0(s3)
    ld      t1, 0(s3)
    CHECK_XW t1, 0x3f000000
    csrr    t1, fflags
    CHECK_I t1, 0

    # ---- integer to float ----
    li      t0, -1
    fcvt.s.w f12, t0, rne
    fsd     f12, 0(s3)
    ld      t1, 0(s3)
    CHECK_XW t1, 0xbf800000

    li      t0, 0x10000000000          # 2^40
    fcvt.d.l f13, t0, rne
    CHECK_F64 f13, 0x4270000000000000

    li      t0, -1
    fcvt.d.w f14, t0
    CHECK_F64 f14, 0xbff0000000000000

    # ---- FCLASS distinguishes all ten classes ----
    li      t0, 0xff800000
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x001                 # -inf
    li      t0, 0xbf800000
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x002                 # negative normal
    li      t0, 0x80000001
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x004                 # negative subnormal
    li      t0, 0x80000000
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x008                 # -0
    li      t0, 0x00000000
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x010                 # +0
    li      t0, 0x00000001
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x020                 # positive subnormal
    li      t0, 0x3f800000
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x040                 # positive normal
    li      t0, 0x7f800000
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x080                 # +inf
    li      t0, 0x7f800001
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x100                 # signaling NaN
    li      t0, 0x7fc00001
    fmv.w.x f15, t0
    fclass.s t1, f15
    CHECK_I t1, 0x200                 # quiet NaN

    # ---- FEQ is quiet on a quiet NaN and raises NV on a signaling NaN ----
    li      t0, 0x7fc00000
    fmv.w.x f16, t0
    li      t0, 0x3f800000
    fmv.w.x f17, t0
    csrw    fflags, x0
    feq.s   t1, f16, f17
    CHECK_I t1, 0
    csrr    t1, fflags
    CHECK_I t1, 0

    li      t0, 0x7f800001             # signaling NaN
    fmv.w.x f18, t0
    csrw    fflags, x0
    feq.s   t1, f18, f17
    CHECK_I t1, 0
    csrr    t1, fflags
    CHECK_I t1, NV

    # FLT is signaling for any NaN.
    csrw    fflags, x0
    flt.s   t1, f16, f17
    CHECK_I t1, 0
    csrr    t1, fflags
    CHECK_I t1, NV

    # ---- +0 == -0 and -0 < +0 is false ----
    li      t0, 0x80000000
    fmv.w.x f19, t0
    li      t0, 0x00000000
    fmv.w.x f20, t0
    csrw    fflags, x0
    feq.s   t1, f19, f20
    CHECK_I t1, 1
    flt.s   t1, f19, f20
    CHECK_I t1, 0
    fle.s   t1, f19, f20
    CHECK_I t1, 1
    csrr    t1, fflags
    CHECK_I t1, 0

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
