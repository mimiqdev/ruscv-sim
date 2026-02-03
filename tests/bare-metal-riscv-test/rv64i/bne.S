# Test: BNE - Branch Not Equal
# Tests: BNE (Branch if Not Equal)
# Test cases: branch taken (not equal), branch not taken (equal)

.section .text
.globl _start

_start:
    # === Test 1: BNE branch taken (not equal values) ===
    li      x1, 10          # x1 = 10
    li      x2, 20          # x2 = 20 (not equal to x1)
    bne     x1, x2, test1_pass  # should branch (10 != 20)
    j       fail            # branch not taken - fail

test1_pass:
    # === Test 2: BNE branch not taken (equal values) ===
    li      x3, 100         # x3 = 100
    li      x4, 100         # x4 = 100 (equal to x3)
    bne     x3, x4, fail    # should NOT branch (100 == 100)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 3: BNE with zero and non-zero ===
    li      x5, 0           # x5 = 0
    li      x6, 1           # x6 = 1
    bne     x5, x6, test3_pass  # should branch (0 != 1)
    j       fail            # branch not taken - fail

test3_pass:
    # === Test 4: BNE not taken with both zero ===
    li      x7, 0           # x7 = 0
    li      x8, 0           # x8 = 0 (equal)
    bne     x7, x8, fail    # should NOT branch (0 == 0)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 5: BNE with negative numbers (different values) ===
    li      x9, -5          # x9 = -5
    li      x10, -10        # x10 = -10
    bne     x9, x10, test5_pass # should branch (-5 != -10)
    j       fail            # branch not taken - fail

test5_pass:
    # === Test 6: BNE not taken with same negative values ===
    li      x11, -100       # x11 = -100
    li      x12, -100       # x12 = -100 (equal)
    bne     x11, x12, fail  # should NOT branch (-100 == -100)
    
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
    sd      x5, 0(x4)       # Write to tohost

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
