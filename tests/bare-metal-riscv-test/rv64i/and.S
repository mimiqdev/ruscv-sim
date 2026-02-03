# Test: AND - Bitwise AND instruction test
# Tests: AND

.section .text
.globl _start

_start:
    # === Test 1: Basic AND (0xFF & 0x0F = 0x0F) ===
    li      x1, 0xFF        # x1 = 0xFF
    li      x2, 0x0F        # x2 = 0x0F
    and     x3, x1, x2      # x3 = x1 & x2 = 0x0F
    
    li      x7, 0x0F        # x7 = expected value (0x0F)
    bne     x3, x7, fail    # if x3 != 0x0F, test failed

    # === Test 2: AND with zero (0xAB & 0x00 = 0x00) ===
    li      x1, 0xAB        # x1 = 0xAB
    li      x2, 0x00        # x2 = 0x00
    and     x3, x1, x2      # x3 = x1 & x2 = 0x00
    
    li      x7, 0x00        # x7 = expected value (0x00)
    bne     x3, x7, fail    # if x3 != 0x00, test failed

    # === Test 3: AND with all ones (0x1234 & 0xFFFF = 0x1234) ===
    li      x1, 0x1234      # x1 = 0x1234
    li      x2, 0xFFFF      # x2 = 0xFFFF
    and     x3, x1, x2      # x3 = x1 & x2 = 0x1234
    
    li      x7, 0x1234      # x7 = expected value (0x1234)
    bne     x3, x7, fail    # if x3 != 0x1234, test failed

    # === Test 4: 64-bit AND (0xF0F0F0F0F0F0F0F0 & 0x0F0F0F0F0F0F0F0F = 0x0000000000000000) ===
    li      x1, 0xF0F0F0F0F0F0F0F0  # x1 = 0xF0F0F0F0F0F0F0F0
    li      x2, 0x0F0F0F0F0F0F0F0F  # x2 = 0x0F0F0F0F0F0F0F0F
    and     x3, x1, x2              # x3 = 0x00
    
    li      x7, 0x00                # x7 = expected value (0x00)
    bne     x3, x7, fail            # if x3 != 0x00, test failed

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
