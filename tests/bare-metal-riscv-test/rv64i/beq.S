# Test: BEQ - Branch Equal
# Tests: BEQ (Branch if Equal)
# Test cases: branch taken (equal), branch not taken (not equal)

.section .text
.globl _start

_start:
    # === Test 1: BEQ branch taken (equal values) ===
    li      x1, 42          # x1 = 42
    li      x2, 42          # x2 = 42 (equal to x1)
    beq     x1, x2, test1_pass  # should branch (42 == 42)
    j       fail            # branch not taken - fail

test1_pass:
    # === Test 2: BEQ branch not taken (not equal values) ===
    li      x3, 10          # x3 = 10
    li      x4, 20          # x4 = 20 (not equal to x3)
    beq     x3, x4, fail    # should NOT branch (10 != 20)
    
    # If we reach here, branch was not taken - continue to next test
    
    # === Test 3: BEQ with zero ===
    li      x5, 0           # x5 = 0
    li      x6, 0           # x6 = 0 (equal)
    beq     x5, x6, test3_pass  # should branch (0 == 0)
    j       fail            # branch not taken - fail

test3_pass:
    # === Test 4: BEQ with negative numbers ===
    li      x7, -5          # x7 = -5
    li      x8, -5          # x8 = -5 (equal)
    beq     x7, x8, test4_pass  # should branch (-5 == -5)
    j       fail            # branch not taken - fail

test4_pass:
    # === Test 5: BEQ not taken with negative vs positive ===
    li      x9, -1          # x9 = -1 (0xFFFFFFFFFFFFFFFF in 64-bit)
    li      x10, 1          # x10 = 1
    beq     x9, x10, fail   # should NOT branch (-1 != 1)
    
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
