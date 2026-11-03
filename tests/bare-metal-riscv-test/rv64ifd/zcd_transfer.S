# zcd_transfer.S — Zcd compressed double-precision loads and stores.
#
# Runs under `--isa rv64imafdc`. Contract D2/D8: C.FLD, C.FSD, C.FLDSP and
# C.FSDSP expand to FLD/FSD with the specified scaled offsets. They are
# transfers, so a double NaN payload moves bit for bit and is never
# canonicalized; only arithmetic on the loaded value canonicalizes it.
#
# Each check increments s2 first, so a failure reports its own number.

.macro CHECK_I reg, value
    addi    s2, s2, 1
    li      t6, \value
    bne     \reg, t6, fail
.endm

    .equ FS_DIRTY, 0x6000

.section .text
.globl _start

_start:
    li      s2, 0
    la      s1, tohost
    li      t0, FS_DIRTY
    csrs    mstatus, t0
    la      a0, zdata
    la      sp, zstack

    # ---- C.FLD loads the full 64 bits from a scaled offset ----
    c.fld   f8, 0(a0)
    fsd     f8, 24(a0)
    ld      t1, 24(a0)
    CHECK_I t1, 0x7ff8000000000001

    c.fld   f9, 8(a0)
    fsd     f9, 32(a0)
    ld      t1, 32(a0)
    CHECK_I t1, 0xbff0000000000000

    c.fld   f10, 16(a0)
    fsd     f10, 40(a0)
    ld      t1, 40(a0)
    CHECK_I t1, 0x3ff0000000000000

    # ---- C.FSD stores the full 64 bits at a scaled offset ----
    c.fsd   f8, 48(a0)
    ld      t1, 48(a0)
    CHECK_I t1, 0x7ff8000000000001

    c.fsd   f9, 56(a0)
    ld      t1, 56(a0)
    CHECK_I t1, 0xbff0000000000000

    # ---- the stack-pointer forms round trip the same values ----
    c.fldsp f11, 0(sp)
    c.fldsp f12, 8(sp)
    c.fsdsp f11, 16(sp)
    c.fsdsp f12, 24(sp)
    ld      t1, 16(sp)
    CHECK_I t1, 0x400921fb54442d18
    ld      t1, 24(sp)
    CHECK_I t1, 0xc00921fb54442d18

    # ---- a Zcd transfer never canonicalizes; arithmetic on it does ----
    li      t0, 0x7ff0000000000001     # signaling-NaN payload
    sd      t0, 64(a0)
    c.fld   f13, 64(a0)
    c.fsd   f13, 72(a0)                # transfer: bits unchanged
    ld      t1, 72(a0)
    CHECK_I t1, 0x7ff0000000000001

    c.fld   f14, 16(a0)                # 1.0
    fadd.d  f15, f13, f14              # arithmetic canonicalizes the NaN
    fsd     f15, 80(a0)
    ld      t1, 80(a0)
    CHECK_I t1, 0x7ff8000000000000

    # ---- a Zcd store writes an FPR, so FS stays Dirty ----
    csrr    t1, mstatus
    srli    t1, t1, 13
    andi    t1, t1, 3
    li      t6, 3
    bne     t1, t6, fail_marker

    li      a0, 0
    j       write_exit

fail_marker:
    addi    s2, s2, 1
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
zdata:
    .dword 0x7ff8000000000001          # 0
    .dword 0xbff0000000000000          # 8
    .dword 0x3ff0000000000000          # 16
    .dword 0, 0, 0, 0                  # 24, 32, 40, 48
    .dword 0, 0, 0, 0, 0, 0            # 56 .. 96
zstack:
    .dword 0x400921fb54442d18          # pi
    .dword 0xc00921fb54442d18          # -pi
    .dword 0, 0

.section .tohost
.align 3
tohost:
    .dword 0
