# Test: XORI - XOR Immediate instruction test
# Tests: XORI

.section .text
.globl _start

_start:
    # === Test 1: Basic XORI (0xFF ^ 0x0F = 0xF0) ===
    li      x1, 0xFF        # x1 = 0xFF
    xori    x3, x1, 0x0F    # x3 = x1 ^ 0x0F = 0xF0
    
    li      x7, 0xF0        # x7 = expected value (0xF0)
    bne     x3, x7, fail    # if x3 != 0xF0, test failed

    # === Test 2: XORI with zero (0xAB ^ 0 = 0xAB) ===
    li      x1, 0xAB        # x1 = 0xAB
    xori    x3, x1, 0       # x3 = x1 ^ 0 = 0xAB
    
    li      x7, 0xAB        # x7 = expected value (0xAB)
    bne     x3, x7, fail    # if x3 != 0xAB, test failed

    # === Test 3: XORI with same value (0x55 ^ 0x55 = 0x00) ===
    li      x1, 0x55        # x1 = 0x55
    xori    x3, x1, 0x55    # x3 = x1 ^ 0x55 = 0x00
    
    li      x7, 0x00        # x7 = expected value (0x00)
    bne     x3, x7, fail    # if x3 != 0x00, test failed

    # === Test 4: XORI all ones immediate (bitwise NOT) (0x0F ^ 0xFFF = 0xFFFFFFFFFFFFFFF0) ===
    li      x1, 0x0F        # x1 = 0x0F
    xori    x3, x1, -1      # x3 = x1 ^ -1 = ~x1 = 0xFFFFFFFFFFFFFFF0
    
    li      x7, -16         # x7 = expected value (-16 = 0xFFFFFFFFFFFFFFF0)
    bne     x3, x7, fail    # if x3 != -16, test failed

    # === Test 5: XORI with 64-bit value ===
    li      x1, 0xF0F0F0F0F0F0F0F0  # x1 = 0xF0F0F0F0F0F0F0F0
    xori    x3, x1, 0xFF            # x3 = x1 ^ 0xFF = 0xF0F0F0F0F0F0F00F
    
    li      x7, 0xF0F0F0F0F0F0F00F  # x7 = expected value
    bne     x3, x7, fail            # if x3 != expected, test failed

    # === Test 6: XORI toggle bits (0x12345678 ^ 0x7FF = 0x12345087) ===
    li      x1, 0x12345678  # x1 = 0x12345678
    xori    x3, x1, 0x7FF   # x3 = x1 ^ 0x7FF = 0x12345087
    
    li      x7, 0x12345087  # x7 = expected value (0x12345087)
    bne     x3, x7, fail    # if x3 != 0x12345087, test failed

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
