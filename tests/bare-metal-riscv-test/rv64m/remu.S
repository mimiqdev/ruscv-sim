# Test: REMU - Remainder Unsigned
# Tests: REMU (unsigned 64-bit remainder)

.section .text
.globl _start

_start:
    # Test 1: Simple remainder: 20 % 6 = 2
    li      x1, 20
    li      x2, 6
    remu    x3, x1, x2      # x3 = 20 % 6 = 2
    li      x7, 2
    bne     x3, x7, fail

    # Test 2: Exact division: 20 % 5 = 0
    li      x1, 20
    li      x2, 5
    remu    x3, x1, x2      # x3 = 20 % 5 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 3: Zero dividend: 0 % 5 = 0
    li      x1, 0
    li      x2, 5
    remu    x3, x1, x2      # x3 = 0 % 5 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 4: Remainder by 1: 42 % 1 = 0
    li      x1, 42
    li      x2, 1
    remu    x3, x1, x2      # x3 = 42 % 1 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 5: Large unsigned values: 0xFFFFFFFFFFFFFFFF % 100
    li      x1, 0xFFFFFFFFFFFFFFFF
    li      x2, 100
    remu    x3, x1, x2      # x3 = 63
    li      x7, 63
    bne     x3, x7, fail

    # Test 6: Maximum % (max-1) = 1
    li      x1, 0xFFFFFFFFFFFFFFFF
    li      x2, 0xFFFFFFFFFFFFFFFE
    remu    x3, x1, x2      # x3 = 1
    li      x7, 1
    bne     x3, x7, fail

    # Test 7: Power of 2 remainder: 0xFFFFFFFFFFFFFFFF % 256 = 0xFF
    li      x1, 0xFFFFFFFFFFFFFFFF
    li      x2, 256
    remu    x3, x1, x2      # x3 = 0xFF
    li      x7, 0xFF
    bne     x3, x7, fail

    # Test 8: Small remainder: 0x8000000000000000 % 3 = 2
    li      x1, 0x8000000000000000
    li      x2, 3
    remu    x3, x1, x2      # x3 = 2
    li      x7, 2
    bne     x3, x7, fail

    # Test 9: Remainder < divisor
    li      x1, 7
    li      x2, 10
    remu    x3, x1, x2      # x3 = 7
    li      x7, 7
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
