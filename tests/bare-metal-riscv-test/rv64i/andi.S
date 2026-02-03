# Test: ANDI - AND Immediate instruction test
# Tests: ANDI

.section .text
.globl _start

_start:
    # === Test 1: Basic ANDI (0xFF & 0x0F = 0x0F) ===
    li      x1, 0xFF        # x1 = 0xFF
    andi    x3, x1, 0x0F    # x3 = x1 & 0x0F = 0x0F
    
    li      x7, 0x0F        # x7 = expected value (0x0F)
    bne     x3, x7, fail    # if x3 != 0x0F, test failed

    # === Test 2: ANDI with zero (0xAB & 0 = 0x00) ===
    li      x1, 0xAB        # x1 = 0xAB
    andi    x3, x1, 0       # x3 = x1 & 0 = 0x00
    
    li      x7, 0x00        # x7 = expected value (0x00)
    bne     x3, x7, fail    # if x3 != 0x00, test failed

    # === Test 3: ANDI with all ones (0x1234 & 0x7FF = 0x234) ===
    li      x1, 0x1234      # x1 = 0x1234
    andi    x3, x1, 0x7FF   # x3 = x1 & 0x7FF = 0x234
    
    li      x7, 0x234       # x7 = expected value (0x234)
    bne     x3, x7, fail    # if x3 != 0x234, test failed

    # === Test 4: ANDI with negative immediate - sign extended (0xFFFFFFFF & 0x7FF = 0x7FF) ===
    li      x1, 0xFFFFFFFF  # x1 = 0xFFFFFFFF
    andi    x3, x1, 0x7FF   # x3 = x1 & 0x7FF = 0x7FF
    
    li      x7, 0x7FF       # x7 = expected value (0x7FF)
    bne     x3, x7, fail    # if x3 != 0x7FF, test failed

    # === Test 5: ANDI with 64-bit value ===
    li      x1, 0xF0F0F0F0F0F0F0F0  # x1 = 0xF0F0F0F0F0F0F0F0
    andi    x3, x1, 0xF0            # x3 = x1 & 0xF0 = 0xF0
    
    li      x7, 0xF0                # x7 = expected value (0xF0)
    bne     x3, x7, fail            # if x3 != 0xF0, test failed

    # === Test 6: ANDI with maximum positive immediate (0xFFFFFFFF & 2047 = 2047) ===
    li      x1, 0xFFFFFFFF  # x1 = 0xFFFFFFFF
    andi    x3, x1, 2047    # x3 = x1 & 2047 = 2047 (0x7FF)
    
    li      x7, 2047        # x7 = expected value (2047)
    bne     x3, x7, fail    # if x3 != 2047, test failed

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
