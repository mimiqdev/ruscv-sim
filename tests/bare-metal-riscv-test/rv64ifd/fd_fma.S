# fd_fma.S — fused multiply-add under rv64imafd.
#
# Contract D4: FMADD, FMSUB, FNMSUB and FNMADD are fused: one rounding only,
# with the infinite-precision product added to the addend. Each check
# increments s2 first, so a failure reports its own number.

.macro CHECK_I reg, value
    addi    s2, s2, 1
    li      t6, \value
    bne     \reg, t6, fail
.endm

# Compare the low 32 bits of the sign-extended FMV.X.W result, so negative
# single-precision encodings compare against their raw bit pattern.
.macro CHECK_XW reg, value
    addi    s2, s2, 1
    slli    t5, \reg, 32
    srli    t5, t5, 32
    li      t6, \value
    bne     t5, t6, fail
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

    # ---- fused, not two roundings ----
    # a = 2^24 + 2, b = 1 + 2^-23, c = -(2^24 + 2).
    # The exact product is 2^24 + 4 + 2^-22, so the exact sum is 2 + 2^-22,
    # which is representable: the fused result is 0x40000001 with no flag.
    # Rounding the product first gives 2^24 + 4 and then exactly 2.0 with NX.
    li      t0, 0x4b800001
    fmv.w.x f1, t0
    li      t0, 0x3f800001
    fmv.w.x f2, t0
    li      t0, 0xcb800001
    fmv.w.x f3, t0
    csrw    fflags, x0
    fmadd.s f4, f1, f2, f3, rne
    fmv.x.w t1, f4
    CHECK_XW t1, 0x40000001
    csrr    t1, fflags
    CHECK_I t1, 0

    # ---- FMADD.S 2*3+1 = 7 ----
    li      t0, 0x40000000             # 2.0f
    fmv.w.x f5, t0
    li      t0, 0x40400000             # 3.0f
    fmv.w.x f6, t0
    li      t0, 0x3f800000             # 1.0f
    fmv.w.x f7, t0
    csrw    fflags, x0
    fmadd.s f8, f5, f6, f7
    fmv.x.w t1, f8
    CHECK_XW t1, 0x40e00000             # 7.0f
    csrr    t1, fflags
    CHECK_I t1, 0

    # ---- FMSUB.S 2*3-1 = 5 ----
    fmsub.s f8, f5, f6, f7
    fmv.x.w t1, f8
    CHECK_XW t1, 0x40a00000             # 5.0f

    # ---- FNMSUB.S -(2*3)+1 = -5 ----
    fnmsub.s f8, f5, f6, f7
    fmv.x.w t1, f8
    CHECK_XW t1, 0xc0a00000

    # ---- FNMADD.S -(2*3)-1 = -7 ----
    fnmadd.s f8, f5, f6, f7
    fmv.x.w t1, f8
    CHECK_XW t1, 0xc0e00000

    # ---- an inexact fused result accrues NX once ----
    # (1 + 2^-23)^2 = 1 + 2^-22 + 2^-46 rounds to 1 + 2^-22 with NX.
    li      t0, 0x00000000
    fmv.w.x f9, t0
    csrw    fflags, x0
    fmadd.s f10, f2, f2, f9
    fmv.x.w t1, f10
    CHECK_XW t1, 0x3f800002
    csrr    t1, fflags
    CHECK_I t1, NX

    # ---- -(inf) + inf is invalid and yields the canonical NaN ----
    li      t0, 0x7f800000             # +inf
    fmv.w.x f11, t0
    csrw    fflags, x0
    fnmsub.s f12, f11, f7, f11         # -(inf * 1) + inf
    fmv.x.w t1, f12
    CHECK_XW t1, 0x7fc00000
    csrr    t1, fflags
    CHECK_I t1, NV

    # ---- the double format fuses the same way ----
    li      t0, 0x4004000000000000     # 2.5
    fmv.d.x f13, t0
    li      t0, 0x4010000000000000     # 4.0
    fmv.d.x f14, t0
    li      t0, 0x3fe0000000000000     # 0.5
    fmv.d.x f15, t0
    csrw    fflags, x0
    fmadd.d f16, f13, f14, f15         # 2.5 * 4 + 0.5 = 10.5
    fsd     f16, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x4025000000000000
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
