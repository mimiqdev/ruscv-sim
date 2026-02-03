# Test: BLTU - Branch Less Than Unsigned
# Tests: BLTU (Branch if Less Than, unsigned comparison)
# Test cases: branch taken (rs1 < rs2 as unsigned), branch not taken (rs1 >= rs2 as unsigned)

.section .text
.globl _start

_start:
    # === Test 1: BLTU branch taken (positive, rs1 < rs2) ===
    li      x1, 5           # x1 = 5
    li      x2, 10          # x2 = 10
    bltu    x1, x2, test1_pass  # should branch (5 < 10)
    j       fail            # branch not taken - fail

test1_pass:
    # === Test 2: BLTU branch not taken (positive, rs1 > rs2) ===
    li      x3, 20          # x3 = 20
    li      x4, 10          # x4 = 10
    bltu    x3, x4, fail    # should NOT branch (20 > 10)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 3: BLTU branch not taken (equal) ===
    li      x5, 7           # x5 = 7
    li      x6, 7           # x6 = 7
    bltu    x5, x6, fail    # should NOT branch (7 == 7)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 4: BLTU branch taken (large positive vs negative as unsigned) ===
    # In unsigned comparison: 0xFFFFFFFFFFFFFFFB (-5) is a very large positive number
    li      x7, 5           # x7 = 5
    li      x8, -1          # x8 = -1 = 0xFFFFFFFFFFFFFFFF (as unsigned: very large)
    bltu    x7, x8, test4_pass  # should branch (5 < 0xFFFFFFFFFFFFFFFF as unsigned)
    j       fail            # branch not taken - fail

test4_pass:
    # === Test 5: BLTU branch not taken (negative as unsigned vs small positive) ===
    li      x9, -5          # x9 = -5 = 0xFFFFFFFFFFFFFFFB (as unsigned: large)
    li      x10, 10         # x10 = 10
    bltu    x9, x10, fail   # should NOT branch (0xFFFFFFFFFFFFFFFB > 10 as unsigned)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 6: BLTU branch taken (negative values compared as unsigned) ===
    # -10 = 0xFFFFFFFFFFFFFFF6, -5 = 0xFFFFFFFFFFFFFFFB
    # As unsigned: 0xFFFFFFFFFFFFFFF6 < 0xFFFFFFFFFFFFFFFB
    li      x11, -10        # x11 = -10 = 0xFFFFFFFFFFFFFFF6
    li      x12, -5         # x12 = -5 = 0xFFFFFFFFFFFFFFFB
    bltu    x11, x12, test6_pass # should branch (-10 < -5 as unsigned)
    j       fail            # branch not taken - fail

test6_pass:
    # === Test 7: BLTU branch not taken (larger negative as unsigned) ===
    li      x13, -5         # x13 = -5 = 0xFFFFFFFFFFFFFFFB
    li      x14, -10        # x14 = -10 = 0xFFFFFFFFFFFFFFF6
    bltu    x13, x14, fail  # should NOT branch (-5 > -10 as unsigned)
    
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
