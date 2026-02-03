# Test: SUB - Subtraction instruction test
# Tests: SUB

.section .text
.globl _start

_start:
    # === Test 1: Basic subtraction (10 - 3 = 7) ===
    li      x1, 10          # x1 = 10
    li      x2, 3           # x2 = 3
    sub     x3, x1, x2      # x3 = x1 - x2 = 10 - 3 = 7
    
    li      x7, 7           # x7 = expected value (7)
    bne     x3, x7, fail    # if x3 != 7, test failed

    # === Test 2: Subtract from zero (0 - 5 = -5) ===
    li      x1, 0           # x1 = 0
    li      x2, 5           # x2 = 5
    sub     x3, x1, x2      # x3 = 0 - 5 = -5
    
    li      x7, -5          # x7 = expected value (-5)
    bne     x3, x7, fail    # if x3 != -5, test failed

    # === Test 3: Subtract zero (100 - 0 = 100) ===
    li      x1, 100         # x1 = 100
    li      x2, 0           # x2 = 0
    sub     x3, x1, x2      # x3 = 100 - 0 = 100
    
    li      x7, 100         # x7 = expected value (100)
    bne     x3, x7, fail    # if x3 != 100, test failed

    # === Test 4: Negative subtraction (5 - (-3) = 8) ===
    li      x1, 5           # x1 = 5
    li      x2, -3          # x2 = -3
    sub     x3, x1, x2      # x3 = 5 - (-3) = 8
    
    li      x7, 8           # x7 = expected value (8)
    bne     x3, x7, fail    # if x3 != 8, test failed

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
