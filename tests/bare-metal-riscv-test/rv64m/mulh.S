# Test: MULH - Multiply High (Signed x Signed)
# Tests: MULH (multiply two signed 64-bit numbers, return upper 64 bits)

.section .text
.globl _start

_start:
    # Test 1: Small numbers: 3 * 4 = 12, upper 64 bits = 0
    li      x1, 3
    li      x2, 4
    mulh    x3, x1, x2      # x3 = upper 64 bits of (3 * 4) = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 2: Result needs > 64 bits: 0x1000000000000000 * 2 = 0x2000000000000000
    # Upper 64 bits = 0
    li      x1, 0x1000000000000000
    li      x2, 2
    mulh    x3, x1, x2      # x3 = upper 64 bits
    li      x7, 0
    bne     x3, x7, fail

    # Test 3: Large multiplication causing overflow to high bits
    # 0x8000000000000000 (-2^63) * 0x8000000000000000 = 2^126
    # Upper 64 bits = 0x4000000000000000
    li      x1, 0x8000000000000000  # -2^63
    li      x2, 0x8000000000000000  # -2^63
    mulh    x3, x1, x2      # x3 = upper 64 bits of 2^126 = 0x4000000000000000
    li      x7, 0x4000000000000000
    bne     x3, x7, fail

    # Test 4: -1 * -1 = 1, upper 64 bits = 0
    li      x1, -1
    li      x2, -1
    mulh    x3, x1, x2      # x3 = upper 64 bits of 1 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 5: Negative * positive: -2^63 * 2 = -2^64
    # Upper 64 bits = -1 (sign extension)
    li      x1, 0x8000000000000000  # -2^63
    li      x2, 2
    mulh    x3, x1, x2      # x3 = upper 64 bits of (-2^64) = -1
    li      x7, -1
    bne     x3, x7, fail

    # Test 6: Multiply by zero: 12345 * 0 = 0, upper bits = 0
    li      x1, 12345
    li      x2, 0
    mulh    x3, x1, x2      # x3 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 7: 2^62 * 2^62 = 2^124
    # Upper 64 bits = 2^60
    li      x1, 0x4000000000000000  # 2^62
    li      x2, 0x4000000000000000  # 2^62
    mulh    x3, x1, x2      # x3 = 2^60
    li      x7, 0x1000000000000000  # 2^60
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
