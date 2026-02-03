# Test: ADDI - Add Immediate instruction test
# Tests: ADDI

.section .text
.globl _start

_start:
    # === Test 1: Basic add immediate (10 + 5 = 15) ===
    li      x1, 10          # x1 = 10
    addi    x3, x1, 5       # x3 = x1 + 5 = 15
    
    li      x7, 15          # x7 = expected value (15)
    bne     x3, x7, fail    # if x3 != 15, test failed

    # === Test 2: Add immediate zero (0xAB + 0 = 0xAB) ===
    li      x1, 0xAB        # x1 = 0xAB
    addi    x3, x1, 0       # x3 = x1 + 0 = 0xAB
    
    li      x7, 0xAB        # x7 = expected value (0xAB)
    bne     x3, x7, fail    # if x3 != 0xAB, test failed

    # === Test 3: Add negative immediate (10 + (-5) = 5) ===
    li      x1, 10          # x1 = 10
    addi    x3, x1, -5      # x3 = x1 + (-5) = 5
    
    li      x7, 5           # x7 = expected value (5)
    bne     x3, x7, fail    # if x3 != 5, test failed

    # === Test 4: Add large immediate (0 + 2047 = 2047, max positive imm) ===
    li      x1, 0           # x1 = 0
    addi    x3, x1, 2047    # x3 = 0 + 2047 = 2047 (0x7FF)
    
    li      x7, 2047        # x7 = expected value (2047)
    bne     x3, x7, fail    # if x3 != 2047, test failed

    # === Test 5: Add negative large immediate (0 + (-2048) = -2048, min negative imm) ===
    li      x1, 0           # x1 = 0
    addi    x3, x1, -2048   # x3 = 0 + (-2048) = -2048 (0xFFFFFFFFFFFFF800)
    
    li      x7, -2048       # x7 = expected value (-2048)
    bne     x3, x7, fail    # if x3 != -2048, test failed

    # === Test 6: 64-bit add immediate ===
    li      x1, 0xFFFFFFFFFFFFFF00  # x1 = 0xFFFFFFFFFFFFFF00 (-256)
    addi    x3, x1, 100             # x3 = -256 + 100 = -156
    
    li      x7, -156                # x7 = expected value (-156)
    bne     x3, x7, fail            # if x3 != -156, test failed

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
