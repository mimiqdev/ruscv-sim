# Test: MULHU - Multiply High Unsigned
# Tests: MULHU (multiply two unsigned 64-bit numbers, return upper 64 bits)

.section .text
.globl _start

_start:
    # Test 1: Small numbers: 3 * 4 = 12, upper 64 bits = 0
    li      x1, 3
    li      x2, 4
    mulhu   x3, x1, x2      # x3 = upper 64 bits of (3 * 4) = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 2: Large positive multiplication
    # 0x1000000000000000 * 2 = 0x2000000000000000
    # Upper 64 bits = 0
    li      x1, 0x1000000000000000
    li      x2, 2
    mulhu   x3, x1, x2
    li      x7, 0
    bne     x3, x7, fail

    # Test 3: Multiplication causing overflow to high bits
    # 0x8000000000000000 * 0x8000000000000000 = 0x40000000000000000000000000000000
    # Upper 64 bits = 0x4000000000000000
    li      x1, 0x8000000000000000
    li      x2, 0x8000000000000000
    mulhu   x3, x1, x2      # x3 = 0x4000000000000000
    li      x7, 0x4000000000000000
    bne     x3, x7, fail

    # Test 4: Maximum values: 0xFFFFFFFFFFFFFFFF * 0xFFFFFFFFFFFFFFFF
    # = 0xFFFFFFFFFFFFFFFE0000000000000001
    # Upper 64 bits = 0xFFFFFFFFFFFFFFFE
    li      x1, 0xFFFFFFFFFFFFFFFF
    li      x2, 0xFFFFFFFFFFFFFFFF
    mulhu   x3, x1, x2      # x3 = 0xFFFFFFFFFFFFFFFE
    li      x7, 0xFFFFFFFFFFFFFFFE
    bne     x3, x7, fail

    # Test 5: Multiply by zero: 12345 * 0 = 0
    li      x1, 12345
    li      x2, 0
    mulhu   x3, x1, x2      # x3 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 6: One is max, one is 1: 0xFFFFFFFFFFFFFFFF * 1
    # Upper 64 bits = 0
    li      x1, 0xFFFFFFFFFFFFFFFF
    li      x2, 1
    mulhu   x3, x1, x2      # x3 = 0
    li      x7, 0
    bne     x3, x7, fail

    # Test 7: 2^62 * 2^62 = 2^124
    # Upper 64 bits = 2^60 = 0x1000000000000000
    li      x1, 0x4000000000000000  # 2^62
    li      x2, 0x4000000000000000  # 2^62
    mulhu   x3, x1, x2      # x3 = 2^60
    li      x7, 0x1000000000000000  # 2^60
    bne     x3, x7, fail

    # Test 8: 2^32 * 2^32 = 2^64
    # Upper 64 bits = 1
    li      x1, 0x100000000       # 2^32
    li      x2, 0x100000000       # 2^32
    mulhu   x3, x1, x2      # x3 = 1
    li      x7, 1
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
