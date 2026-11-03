# fd_nanbox.S — NaN boxing and payload preservation under rv64imafd.
#
# Contract D5: every 32-bit write to an FPR sets the upper 32 bits to ones; a
# non-transfer single-precision operand read from an FPR whose upper 32 bits
# are not all ones yields the canonical NaN 0x7FC00000; sign injection
# preserves the payload of a correctly boxed operand; narrow transfers out of
# the register (FSW, FMV.X.W) use the low 32 bits unchanged; arithmetic,
# square root, fused multiply-add, conversions and FMIN/FMAX canonicalize NaN
# results, while FSGNJ*, FMV and loads/stores preserve payloads bit for bit.
#
# Each check increments s2 first, so a failure reports its own number.

.macro CHECK_I reg, value
    addi    s2, s2, 1
    li      t6, \value
    bne     \reg, t6, fail
.endm

.macro CHECK_R a, b
    addi    s2, s2, 1
    bne     \a, \b, fail
.endm

    .equ FS_DIRTY, 0x6000

.section .text
.globl _start

_start:
    li      s2, 0
    la      s1, tohost
    li      t0, FS_DIRTY
    csrs    mstatus, t0
    la      s3, scratch

    # ---- FLW NaN-boxes its result ----
    la      t0, word_qnan_single
    flw     f1, 0(t0)
    fsd     f1, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc12345

    # ---- a load/store round trip preserves the payload bit for bit ----
    fsd     f1, 0(s3)
    fld     f2, 0(s3)
    fsd     f2, 8(s3)
    ld      t1, 8(s3)
    CHECK_I t1, 0xffffffff7fc12345

    # ---- FSGNJ/FSGNJN/FSGNJX preserve a boxed NaN payload ----
    fsgnj.s f3, f1, f1
    fsd     f3, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc12345

    fsgnjn.s f3, f1, f1
    fsd     f3, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffffffc12345      # FSGNJN inverts the second operand sign

    fsgnjx.s f3, f1, f1
    fsd     f3, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc12345

    # ---- an unboxed single operand reads as the canonical NaN ----
    li      t0, 0x3f800000             # upper 32 bits are zero: not boxed
    fmv.d.x f4, t0
    fadd.s  f5, f4, f4
    fsd     f5, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc00000

    fsgnj.s f5, f4, f4
    fsd     f5, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc00000

    fclass.s t1, f4
    CHECK_I t1, 0x200                   # bit 9: the canonical NaN is quiet

    # ---- arithmetic on a boxed NaN is canonical, transfers are not ----
    fadd.s  f5, f1, f1
    fsd     f5, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc00000

    fsqrt.s f5, f1
    fsd     f5, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc00000

    fsgnj.s f5, f1, f1
    fsd     f5, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc12345

    # ---- FMV.X.W returns the low 32 bits, sign-extended ----
    # The upper half deliberately differs from the sign extension, so a full
    # 64-bit copy (FMV.X.D behaviour) fails this check.
    li      t0, 0xaaaaaaaa80000001
    fmv.d.x f6, t0
    fmv.x.w t1, f6
    CHECK_I t1, 0xffffffff80000001

    # 0x7fc12345 has bit 31 clear, so the sign extension is invisible.
    fmv.x.w t1, f1
    CHECK_I t1, 0x000000007fc12345

    # ---- FSW stores the low 32 bits unchanged ----
    li      t0, 0xdeadbeef7fc12345
    fmv.d.x f7, t0
    fsw     f7, 16(s3)
    lwu     t1, 16(s3)
    CHECK_I t1, 0x000000007fc12345

    # ---- FMV.W.X boxes; FMV.D.X does not ----
    li      t0, 0x3f800000
    fmv.w.x f8, t0
    fsd     f8, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff3f800000

    fmv.d.x f8, t0
    fsd     f8, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x000000003f800000

    # ---- FLD and FSD keep a double NaN payload; FADD.D canonicalizes it ----
    la      t0, double_qnan
    fld     f9, 0(t0)
    fsd     f9, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x7ff8000000000001

    fadd.d  f10, f9, f9
    fsd     f10, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x7ff8000000000000

    fsgnjx.d f10, f9, f9
    fsd     f10, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x7ff8000000000001

    # ---- FMIN/FMAX canonicalize NaN results (version 2.2 rules) ----
    fmin.s  f11, f1, f1
    fsd     f11, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0xffffffff7fc00000

    fmax.d  f11, f9, f9
    fsd     f11, 0(s3)
    ld      t1, 0(s3)
    CHECK_I t1, 0x7ff8000000000000

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
    .dword 0, 0, 0
word_qnan_single:
    .word 0x7fc12345
    .align 3
double_qnan:
    .dword 0x7ff8000000000001

.section .tohost
.align 3
tohost:
    .dword 0
