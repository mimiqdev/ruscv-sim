# Test: MUL - Multiply (Signed)
# Tests: MUL (multiply two signed 64-bit numbers, return lower 64 bits)

.section .text
.globl _start

_start:
    # Test 1: Simple multiplication: 3 * 4 = 12
    li      x1, 3
    li      x2, 4
    mul     x3, x1, x2      # x3 = 3 * 4 = 12
    li      x7, 12
    bne     x3, x7, fail

    # Test 2: Multiply by zero: 42 * 0 = 0
    li      x1, 42
    li      x2, 0
    mul     x3, x1, x2      # x3 = 42 * 0 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 3: Multiply by one: 123 * 1 = 123
    li      x1, 123
    li      x2, 1
    mul     x3, x1, x2      # x3 = 123 * 1 = 123
    li      x7, 123
    bne     x3, x7, fail

    # Test 4: Large positive numbers: 0x10000 * 0x10000 = 0x100000000
    li      x1, 0x10000
    li      x2, 0x10000
    mul     x3, x1, x2      # x3 = lower 64 bits of 0x100000000
    li      x7, 0x100000000
    bne     x3, x7, fail

    # Test 5: Negative * positive = negative: -5 * 3 = -15
    li      x1, -5
    li      x2, 3
    mul     x3, x1, x2      # x3 = -5 * 3 = -15
    li      x7, -15
    bne     x3, x7, fail

    # Test 6: Positive * negative = negative: 5 * -3 = -15
    li      x1, 5
    li      x2, -3
    mul     x3, x1, x2      # x3 = 5 * -3 = -15
    li      x7, -15
    bne     x3, x7, fail

    # Test 7: Negative * negative = positive: -7 * -8 = 56
    li      x1, -7
    li      x2, -8
    mul     x3, x1, x2      # x3 = -7 * -8 = 56
    li      x7, 56
    bne     x3, x7, fail

    # Test 8: Large values that overflow (lower 64 bits)
    li      x1, 0xFFFFFFFFFFFFFFFF  # -1
    li      x2, 2
    mul     x3, x1, x2      # x3 = lower 64 bits of -2 = 0xFFFFFFFFFFFFFFFE
    li      x7, 0xFFFFFFFFFFFFFFFE
    bne     x3, x7, fail

    # All tests passed
    j       pass

fail:
    # Test failed, exit code = 1
    li      x5, 1
    j       write_exit

pass:
    # Test passed, exit code = 0
    li      x5, 0

write_exit:
    la      x4, tohost
    li      x6, 1
    slli    x6, x6, 63      # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6      # x5 = WRITE_MARKER | exit_code
    sd      x5, 0(x4)

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
