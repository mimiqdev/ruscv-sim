# Test: MULHSU - Multiply High Signed/Unsigned
# Tests: MULHSU (multiply signed x unsigned 64-bit numbers, return upper 64 bits)

.section .text
.globl _start

_start:
    # Test 1: Small positive numbers (same as mulhu): 3 * 4 = 12, upper = 0
    li      x1, 3
    li      x2, 4
    mulhsu  x3, x1, x2      # x3 = upper 64 bits of (3 * 4) = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 2: Large positive * positive
    # 0x1000000000000000 * 2 = 0x2000000000000000
    li      x1, 0x1000000000000000
    li      x2, 2
    mulhsu  x3, x1, x2
    li      x7, 0
    bne     x3, x7, fail

    # Test 3: Negative signed * positive unsigned
    # -1 * 1 = 0xFFFFFFFFFFFFFFFF (128-bit: all 1s)
    # Upper 64 bits = 0xFFFFFFFFFFFFFFFF
    li      x1, -1
    li      x2, 1
    mulhsu  x3, x1, x2      # x3 = 0xFFFFFFFFFFFFFFFF (sign extension of -1)
    li      x7, 0xFFFFFFFFFFFFFFFF
    bne     x3, x7, fail

    # Test 4: Large negative signed * large unsigned
    # -2^63 * 2 = -2^64
    # Upper 64 bits = 0xFFFFFFFFFFFFFFFF (sign extension of negative)
    li      x1, 0x8000000000000000  # -2^63 (signed)
    li      x2, 2
    mulhsu  x3, x1, x2      # x3 = upper 64 bits of (-2^64)
    li      x7, 0xFFFFFFFFFFFFFFFF
    bne     x3, x7, fail

    # Test 5: Multiply by zero: 12345 * 0 = 0
    li      x1, 12345
    li      x2, 0
    mulhsu  x3, x1, x2      # x3 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 6: Mixed large values
    # -2 (0xFFFFFFFFFFFFFFFE) * 2 = -4
    # Upper bits = sign extension
    li      x1, -2
    li      x2, 2
    mulhsu  x3, x1, x2      # x3 = upper bits of (-4) = 0xFFFFFFFFFFFFFFFF
    li      x7, 0xFFFFFFFFFFFFFFFF
    bne     x3, x7, fail

    # Test 7: 2^63 * 2 (signed positive * unsigned)
    # 0x7FFFFFFFFFFFFFFF * 2 = 0xFFFFFFFFFFFFFFFE
    # Upper bits = 0
    li      x1, 0x7FFFFFFFFFFFFFFF  # 2^63 - 1 (signed positive)
    li      x2, 2
    mulhsu  x3, x1, x2      # x3 = upper bits
    li      x7, 0
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
