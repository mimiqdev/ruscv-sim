# Test: BGE - Branch Greater or Equal (Signed)
# Tests: BGE (Branch if Greater or Equal, signed comparison)
# Test cases: branch taken (rs1 >= rs2), branch not taken (rs1 < rs2)

.section .text
.globl _start

_start:
    # === Test 1: BGE branch taken (positive, rs1 > rs2) ===
    li      x1, 20          # x1 = 20
    li      x2, 10          # x2 = 10
    bge     x1, x2, test1_pass  # should branch (20 >= 10)
    j       fail            # branch not taken - fail

test1_pass:
    # === Test 2: BGE branch taken (equal values) ===
    li      x3, 15          # x3 = 15
    li      x4, 15          # x4 = 15
    bge     x3, x4, test2_pass  # should branch (15 >= 15)
    j       fail            # branch not taken - fail

test2_pass:
    # === Test 3: BGE branch not taken (positive, rs1 < rs2) ===
    li      x5, 5           # x5 = 5
    li      x6, 10          # x6 = 10
    bge     x5, x6, fail    # should NOT branch (5 < 10)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 4: BGE branch taken (positive vs negative) ===
    li      x7, 10          # x7 = 10
    li      x8, -10         # x8 = -10
    bge     x7, x8, test4_pass  # should branch (10 >= -10)
    j       fail            # branch not taken - fail

test4_pass:
    # === Test 5: BGE branch not taken (negative vs positive) ===
    li      x9, -5          # x9 = -5
    li      x10, 5          # x10 = 5
    bge     x9, x10, fail   # should NOT branch (-5 < 5)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 6: BGE branch taken (both negative, rs1 > rs2) ===
    li      x11, -5         # x11 = -5
    li      x12, -10        # x12 = -10
    bge     x11, x12, test6_pass # should branch (-5 >= -10)
    j       fail            # branch not taken - fail

test6_pass:
    # === Test 7: BGE branch not taken (both negative, rs1 < rs2) ===
    li      x13, -100       # x13 = -100
    li      x14, -50        # x14 = -50
    bge     x13, x14, fail  # should NOT branch (-100 < -50)
    
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
