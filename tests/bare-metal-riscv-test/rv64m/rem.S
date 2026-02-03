# Test: REM - Remainder (Signed)
# Tests: REM (signed 64-bit remainder)

.section .text
.globl _start

_start:
    # Test 1: Simple remainder: 20 % 6 = 2
    li      x1, 20
    li      x2, 6
    rem     x3, x1, x2      # x3 = 20 % 6 = 2
    li      x7, 2
    bne     x3, x7, fail

    # Test 2: Exact division: 20 % 5 = 0
    li      x1, 20
    li      x2, 5
    rem     x3, x1, x2      # x3 = 20 % 5 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 3: Zero dividend: 0 % 5 = 0
    li      x1, 0
    li      x2, 5
    rem     x3, x1, x2      # x3 = 0 % 5 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 4: Remainder by 1: 42 % 1 = 0
    li      x1, 42
    li      x2, 1
    rem     x3, x1, x2      # x3 = 42 % 1 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 5: Negative dividend: -20 % 6 = -2 (sign follows dividend)
    li      x1, -20
    li      x2, 6
    rem     x3, x1, x2      # x3 = -20 % 6 = -2
    li      x7, -2
    bne     x3, x7, fail

    # Test 6: Negative divisor: 20 % -6 = 2 (sign follows dividend)
    li      x1, 20
    li      x2, -6
    rem     x3, x1, x2      # x3 = 20 % -6 = 2
    li      x7, 2
    bne     x3, x7, fail

    # Test 7: Both negative: -20 % -6 = -2 (sign follows dividend)
    li      x1, -20
    li      x2, -6
    rem     x3, x1, x2      # x3 = -20 % -6 = -2
    li      x7, -2
    bne     x3, x7, fail

    # Test 8: Large positive remainder
    li      x1, 0x7FFFFFFFFFFFFFFF  # Max positive
    li      x2, 100
    rem     x3, x1, x2
    li      x7, 7                   # 0x7FFFFFFFFFFFFFFF % 100 = 7
    bne     x3, x7, fail

    # Test 9: Large negative remainder
    li      x1, 0x8000000000000000  # Min negative (-2^63)
    li      x2, 100
    rem     x3, x1, x2
    li      x7, -8                  # -2^63 % 100 = -8 (truncated)
    bne     x3, x7, fail

    # Test 10: Overflow case: -2^63 % -1 = 0
    li      x1, 0x8000000000000000  # -2^63
    li      x2, -1
    rem     x3, x1, x2      # x3 = 0 (overflow, but remainder is 0)
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
