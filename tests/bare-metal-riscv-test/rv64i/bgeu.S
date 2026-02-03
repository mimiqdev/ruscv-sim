# Test: BGEU - Branch Greater or Equal Unsigned
# Tests: BGEU (Branch if Greater or Equal, unsigned comparison)
# Test cases: branch taken (rs1 >= rs2 as unsigned), branch not taken (rs1 < rs2 as unsigned)

.section .text
.globl _start

_start:
    # === Test 1: BGEU branch taken (positive, rs1 > rs2) ===
    li      x1, 20          # x1 = 20
    li      x2, 10          # x2 = 10
    bgeu    x1, x2, test1_pass  # should branch (20 >= 10)
    j       fail            # branch not taken - fail

test1_pass:
    # === Test 2: BGEU branch taken (equal values) ===
    li      x3, 15          # x3 = 15
    li      x4, 15          # x4 = 15
    bgeu    x3, x4, test2_pass  # should branch (15 >= 15)
    j       fail            # branch not taken - fail

test2_pass:
    # === Test 3: BGEU branch not taken (positive, rs1 < rs2) ===
    li      x5, 5           # x5 = 5
    li      x6, 10          # x6 = 10
    bgeu    x5, x6, fail    # should NOT branch (5 < 10)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 4: BGEU branch taken (negative as unsigned vs small positive) ===
    # -5 = 0xFFFFFFFFFFFFFFFB is a very large unsigned number
    li      x7, -5          # x7 = -5 = 0xFFFFFFFFFFFFFFFB (large as unsigned)
    li      x8, 10          # x8 = 10
    bgeu    x7, x8, test4_pass  # should branch (0xFFFFFFFFFFFFFFFB >= 10 as unsigned)
    j       fail            # branch not taken - fail

test4_pass:
    # === Test 5: BGEU branch not taken (small positive vs large unsigned) ===
    li      x9, 10          # x9 = 10
    li      x10, -1         # x10 = -1 = 0xFFFFFFFFFFFFFFFF (very large as unsigned)
    bgeu    x9, x10, fail   # should NOT branch (10 < 0xFFFFFFFFFFFFFFFF as unsigned)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 6: BGEU branch taken (both negative as unsigned, rs1 > rs2) ===
    # -5 = 0xFFFFFFFFFFFFFFFB, -10 = 0xFFFFFFFFFFFFFFF6
    # As unsigned: 0xFFFFFFFFFFFFFFFB > 0xFFFFFFFFFFFFFFF6
    li      x11, -5         # x11 = -5 = 0xFFFFFFFFFFFFFFFB
    li      x12, -10        # x12 = -10 = 0xFFFFFFFFFFFFFFF6
    bgeu    x11, x12, test6_pass # should branch (-5 >= -10 as unsigned)
    j       fail            # branch not taken - fail

test6_pass:
    # === Test 7: BGEU branch not taken (both negative as unsigned, rs1 < rs2) ===
    li      x13, -20        # x13 = -20 = 0xFFFFFFFFFFFFFFEC
    li      x14, -5         # x14 = -5 = 0xFFFFFFFFFFFFFFFB
    bgeu    x13, x14, fail  # should NOT branch (-20 < -5 as unsigned)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 8: BGEU branch taken (zero vs zero) ===
    li      x15, 0          # x15 = 0
    li      x16, 0          # x16 = 0
    bgeu    x15, x16, test8_pass # should branch (0 >= 0)
    j       fail            # branch not taken - fail

test8_pass:
    # === Test 9: BGEU branch taken (large unsigned vs zero) ===
    li      x17, -1         # x17 = -1 = 0xFFFFFFFFFFFFFFFF
    li      x18, 0          # x18 = 0
    bgeu    x17, x18, pass  # should branch (large >= 0)
    j       fail            # branch not taken - fail

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
