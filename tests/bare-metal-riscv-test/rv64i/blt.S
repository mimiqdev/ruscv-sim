# Test: BLT - Branch Less Than (Signed)
# Tests: BLT (Branch if Less Than, signed comparison)
# Test cases: branch taken (rs1 < rs2), branch not taken (rs1 >= rs2)

.section .text
.globl _start

_start:
    # === Test 1: BLT branch taken (positive, rs1 < rs2) ===
    li      x1, 5           # x1 = 5
    li      x2, 10          # x2 = 10
    blt     x1, x2, test1_pass  # should branch (5 < 10)
    j       fail            # branch not taken - fail

test1_pass:
    # === Test 2: BLT branch not taken (positive, rs1 > rs2) ===
    li      x3, 20          # x3 = 20
    li      x4, 10          # x4 = 10
    blt     x3, x4, fail    # should NOT branch (20 > 10)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 3: BLT branch not taken (equal) ===
    li      x5, 7           # x5 = 7
    li      x6, 7           # x6 = 7
    blt     x5, x6, fail    # should NOT branch (7 == 7, not less)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 4: BLT branch taken (negative vs positive) ===
    li      x7, -5          # x7 = -5
    li      x8, 5           # x8 = 5
    blt     x7, x8, test4_pass  # should branch (-5 < 5)
    j       fail            # branch not taken - fail

test4_pass:
    # === Test 5: BLT branch not taken (positive vs negative) ===
    li      x9, 10          # x9 = 10
    li      x10, -10        # x10 = -10
    blt     x9, x10, fail   # should NOT branch (10 > -10 in signed)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 6: BLT branch taken (both negative) ===
    li      x11, -20        # x11 = -20
    li      x12, -10        # x12 = -10
    blt     x11, x12, test6_pass # should branch (-20 < -10)
    j       fail            # branch not taken - fail

test6_pass:
    # === Test 7: BLT branch not taken (both negative, greater) ===
    li      x13, -5         # x13 = -5
    li      x14, -100       # x14 = -100
    blt     x13, x14, fail  # should NOT branch (-5 > -100)
    
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
