# Test: LH - Load Halfword (16-bit signed load)
# Tests: LH, address alignment, sign extension to 64-bit

.section .data
.align 4
test_data:
    .half 0x1234             # +ve 16-bit value
    .half 0x8000             # -ve in signed 16-bit (sign-extend to 0xFFFFFFFFFFFF8000)
    .half 0xFFFF             # -1 in signed 16-bit (sign-extend to 0xFFFFFFFFFFFFFFFF)
    .half 0x0000             # zero
    .word 0xAABBCCDD         # for unaligned test

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Load positive halfword (0x1234) ===
    # lh should load 0x1234, sign-extend to 0x0000000000001234
    lh      x1, 0(x10)          # x1 = *(x10 + 0)
    li      x7, 0x1234          # x7 = expected value
    bne     x1, x7, fail        # if x1 != expected, fail

    # === Test 2: Load negative halfword (0x8000) ===
    # lh should load 0x8000, sign-extend to 0xFFFFFFFFFFFFFFFF8000
    lh      x2, 2(x10)          # x2 = *(x10 + 2)
    li      x7, 0x8000          # x7 = 0x8000
    # Sign-extend to 64-bit: 0x8000 with sign bit set -> 0xFFFFFFFFFFFF8000
    slli    x8, x7, 48          # shift left to bit 63
    srai    x8, x8, 48          # arithmetic shift right to sign-extend
    bne     x2, x8, fail        # if x2 != expected, fail

    # === Test 3: Load -1 (0xFFFF) ===
    # lh should load 0xFFFF, sign-extend to 0xFFFFFFFFFFFFFFFF
    lh      x3, 4(x10)          # x3 = *(x10 + 4)
    li      x7, -1              # x7 = -1 (0xFFFFFFFFFFFFFFFF)
    bne     x3, x7, fail        # if x3 != -1, fail

    # === Test 4: Load zero (0x0000) ===
    lh      x4, 6(x10)          # x4 = *(x10 + 6)
    li      x7, 0               # x7 = 0
    bne     x4, x7, fail        # if x4 != 0, fail

    # === Test 5: SH then LH (verify store/load pair) ===
    addi    x11, x10, 16        # x11 = scratch area after test_data
    li      x5, 0x5A5A          # x5 = test value
    sh      x5, 0(x11)          # store halfword
    lh      x6, 0(x11)          # load halfword back
    li      x7, 0x5A5A          # x7 = expected
    bne     x6, x7, fail        # verify

    # === Test 6: LH with negative offset ===
    addi    x12, x10, 4         # x12 = test_data + 4
    lh      x6, -2(x12)         # load from (x12 - 2) = test_data + 2
    li      x7, 0x8000          # should load 0x8000
    slli    x8, x7, 48
    srai    x8, x8, 48          # sign-extend
    bne     x6, x8, fail

    # === Test 7: Verify only lower 16 bits are loaded ===
    # Load from word 0xAABBCCDD at offset 8
    lh      x6, 8(x10)          # load lower 16 bits (0xCCDD)
    li      x7, 0xCCDD          # expect 0xCCDD
    # Sign-extend: 0xCCDD has bit 15 set (0x8000), so should be negative
    slli    x8, x7, 48
    srai    x8, x8, 48          # sign-extend
    bne     x6, x8, fail

    lh      x6, 10(x10)         # load upper 16 bits (0xAABB)
    li      x7, 0xAABB          # expect 0xAABB
    slli    x8, x7, 48
    srai    x8, x8, 48          # sign-extend
    bne     x6, x8, fail

    # === Test 8: Load small negative value ===
    li      x1, 0xFF80          # -128 in 16-bit signed
    sh      x1, 0(x11)
    lh      x2, 0(x11)
    li      x7, -128            # should be -128 as 64-bit
    bne     x2, x7, fail

pass:
    li      x5, 0               # exit_code = 0 (pass)
    j       write_exit

fail:
    li      x5, 1               # exit_code = 1 (fail)

write_exit:
    la      x4, tohost
    li      x6, 1
    slli    x6, x6, 63          # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6          # x5 = WRITE_MARKER | exit_code
    sd      x5, 0(x4)           # write to tohost

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
