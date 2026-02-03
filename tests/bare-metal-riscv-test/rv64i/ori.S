# Test: ORI - OR Immediate instruction test
# Tests: ORI

.section .text
.globl _start

_start:
    # === Test 1: Basic ORI (0xF0 | 0x0F = 0xFF) ===
    li      x1, 0xF0        # x1 = 0xF0
    ori     x3, x1, 0x0F    # x3 = x1 | 0x0F = 0xFF
    
    li      x7, 0xFF        # x7 = expected value (0xFF)
    bne     x3, x7, fail    # if x3 != 0xFF, test failed

    # === Test 2: ORI with zero (0xAB | 0 = 0xAB) ===
    li      x1, 0xAB        # x1 = 0xAB
    ori     x3, x1, 0       # x3 = x1 | 0 = 0xAB
    
    li      x7, 0xAB        # x7 = expected value (0xAB)
    bne     x3, x7, fail    # if x3 != 0xAB, test failed

    # === Test 3: ORI with large immediate (0x12340000 | 0x7FF = 0x123407FF) ===
    li      x1, 0x12340000  # x1 = 0x12340000
    ori     x3, x1, 0x7FF   # x3 = x1 | 0x7FF = 0x123407FF
    
    li      x7, 0x123407FF  # x7 = expected value (0x123407FF)
    bne     x3, x7, fail    # if x3 != 0x123407FF, test failed

    # === Test 4: ORI with negative immediate - sign extended ===
    li      x1, 0x00000000  # x1 = 0x00000000
    ori     x3, x1, -1      # x3 = x1 | 0xFFF = 0xFFFFFFFFFFFFFFFF
    
    li      x7, -1          # x7 = expected value (-1 = all 1s)
    bne     x3, x7, fail    # if x3 != -1, test failed

    # === Test 5: ORI with 64-bit value ===
    li      x1, 0x0F0F0F0F0F0F0F00  # x1 = 0x0F0F0F0F0F0F0F00
    ori     x3, x1, 0xF0            # x3 = x1 | 0xF0 = 0x0F0F0F0F0F0F0FF0
    
    li      x7, 0x0F0F0F0F0F0F0FF0  # x7 = expected value
    bne     x3, x7, fail            # if x3 != expected, test failed

    # === Test 6: ORI with same value (0x55 | 0x55 = 0x55) ===
    li      x1, 0x55        # x1 = 0x55
    ori     x3, x1, 0x55    # x3 = x1 | 0x55 = 0x55
    
    li      x7, 0x55        # x7 = expected value (0x55)
    bne     x3, x7, fail    # if x3 != 0x55, test failed

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
