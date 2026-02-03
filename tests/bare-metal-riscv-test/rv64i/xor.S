# Test: XOR - Bitwise XOR instruction test
# Tests: XOR

.section .text
.globl _start

_start:
    # === Test 1: Basic XOR (0xFF ^ 0x0F = 0xF0) ===
    li      x1, 0xFF        # x1 = 0xFF
    li      x2, 0x0F        # x2 = 0x0F
    xor     x3, x1, x2      # x3 = x1 ^ x2 = 0xF0
    
    li      x7, 0xF0        # x7 = expected value (0xF0)
    bne     x3, x7, fail    # if x3 != 0xF0, test failed

    # === Test 2: XOR with zero (0xAB ^ 0x00 = 0xAB) ===
    li      x1, 0xAB        # x1 = 0xAB
    li      x2, 0x00        # x2 = 0x00
    xor     x3, x1, x2      # x3 = x1 ^ x2 = 0xAB
    
    li      x7, 0xAB        # x7 = expected value (0xAB)
    bne     x3, x7, fail    # if x3 != 0xAB, test failed

    # === Test 3: XOR same value (0x1234 ^ 0x1234 = 0x00) ===
    li      x1, 0x1234      # x1 = 0x1234
    li      x2, 0x1234      # x2 = 0x1234
    xor     x3, x1, x2      # x3 = x1 ^ x2 = 0x00
    
    li      x7, 0x00        # x7 = expected value (0x00)
    bne     x3, x7, fail    # if x3 != 0x00, test failed

    # === Test 4: XOR complement (0xF0F0F0F0F0F0F0F0 ^ 0x0F0F0F0F0F0F0F0F = 0xFFFFFFFFFFFFFFFF) ===
    li      x1, 0xF0F0F0F0F0F0F0F0  # x1 = 0xF0F0F0F0F0F0F0F0
    li      x2, 0x0F0F0F0F0F0F0F0F  # x2 = 0x0F0F0F0F0F0F0F0F
    xor     x3, x1, x2              # x3 = 0xFFFFFFFFFFFFFFFF
    
    li      x7, -1                  # x7 = expected value (-1, which is 0xFFFFFFFFFFFFFFFF)
    bne     x3, x7, fail            # if x3 != 0xFFFFFFFFFFFFFFFF, test failed

pass:
    li      x5, 0           # exit_code = 0 (pass)
    j       write_exit

fail:
    li      x5, 1           # exit_code = 1 (fail)

write_exit:
    la      x4, tohost
    li      x6, 1
    slli    x6, x6, 63      # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6      # x5 = WRITE_MARKER | exit_code
    sd      x5, 0(x4)       # Write to tohost

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
