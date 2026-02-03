# Test: DIV - Divide (Signed)
# Tests: DIV (signed 64-bit division)

.section .text
.globl _start

_start:
    # Test 1: Simple division: 20 / 4 = 5
    li      x1, 20
    li      x2, 4
    div     x3, x1, x2      # x3 = 20 / 4 = 5
    li      x7, 5
    bne     x3, x7, fail

    # Test 2: Division by one: 42 / 1 = 42
    li      x1, 42
    li      x2, 1
    div     x3, x1, x2      # x3 = 42 / 1 = 42
    li      x7, 42
    bne     x3, x7, fail

    # Test 3: Zero divided: 0 / 5 = 0
    li      x1, 0
    li      x2, 5
    div     x3, x1, x2      # x3 = 0 / 5 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 4: Negative / positive = negative: -20 / 4 = -5
    li      x1, -20
    li      x2, 4
    div     x3, x1, x2      # x3 = -20 / 4 = -5
    li      x7, -5
    bne     x3, x7, fail

    # Test 5: Positive / negative = negative: 20 / -4 = -5
    li      x1, 20
    li      x2, -4
    div     x3, x1, x2      # x3 = 20 / -4 = -5
    li      x7, -5
    bne     x3, x7, fail

    # Test 6: Negative / negative = positive: -20 / -4 = 5
    li      x1, -20
    li      x2, -4
    div     x3, x1, x2      # x3 = -20 / -4 = 5
    li      x7, 5
    bne     x3, x7, fail

    # Test 7: Division with remainder: 22 / 5 = 4 (truncated toward 0)
    li      x1, 22
    li      x2, 5
    div     x3, x1, x2      # x3 = 22 / 5 = 4
    li      x7, 4
    bne     x3, x7, fail

    # Test 8: Negative division with remainder: -22 / 5 = -4 (truncated toward 0)
    li      x1, -22
    li      x2, 5
    div     x3, x1, x2      # x3 = -22 / 5 = -4
    li      x7, -4
    bne     x3, x7, fail

    # Test 9: Large positive numbers
    li      x1, 0x7FFFFFFFFFFFFFFF  # Max signed positive
    li      x2, 1
    div     x3, x1, x2      # x3 = 0x7FFFFFFFFFFFFFFF
    li      x7, 0x7FFFFFFFFFFFFFFF
    bne     x3, x7, fail

    # Test 10: Large negative numbers
    li      x1, 0x8000000000000000  # Min signed negative (-2^63)
    li      x2, 1
    div     x3, x1, x2      # x3 = 0x8000000000000000
    li      x7, 0x8000000000000000
    bne     x3, x7, fail

    # Test 11: Overflow case: -2^63 / -1 = -2^63 (overflow, per spec)
    li      x1, 0x8000000000000000  # -2^63
    li      x2, -1
    div     x3, x1, x2      # x3 = -2^63 (overflow result)
    li      x7, 0x8000000000000000
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
