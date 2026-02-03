# Test: SLL - Shift Left Logical instruction test
# Tests: SLL

.section .text
.globl _start

_start:
    # === Test 1: Basic shift left (1 << 3 = 8) ===
    li      x1, 1           # x1 = 1
    li      x2, 3           # x2 = 3
    sll     x3, x1, x2      # x3 = x1 << x2 = 1 << 3 = 8
    
    li      x7, 8           # x7 = expected value (8)
    bne     x3, x7, fail    # if x3 != 8, test failed

    # === Test 2: Shift left by zero (0xAB << 0 = 0xAB) ===
    li      x1, 0xAB        # x1 = 0xAB
    li      x2, 0           # x2 = 0
    sll     x3, x1, x2      # x3 = x1 << 0 = 0xAB
    
    li      x7, 0xAB        # x7 = expected value (0xAB)
    bne     x3, x7, fail    # if x3 != 0xAB, test failed

    # === Test 3: Shift left 8 bits (0x12 << 8 = 0x1200) ===
    li      x1, 0x12        # x1 = 0x12
    li      x2, 8           # x2 = 8
    sll     x3, x1, x2      # x3 = x1 << 8 = 0x1200
    
    li      x7, 0x1200      # x7 = expected value (0x1200)
    bne     x3, x7, fail    # if x3 != 0x1200, test failed

    # === Test 4: 64-bit shift left (0x1 << 32 = 0x100000000) ===
    li      x1, 1           # x1 = 1
    li      x2, 32          # x2 = 32
    sll     x3, x1, x2      # x3 = 1 << 32 = 0x100000000
    
    li      x7, 0x100000000 # x7 = expected value (0x100000000)
    bne     x3, x7, fail    # if x3 != 0x100000000, test failed

    # === Test 5: Shift with bits shifted out (0xFF << 4 = 0xFF0) ===
    li      x1, 0xFF        # x1 = 0xFF
    li      x2, 4           # x2 = 4
    sll     x3, x1, x2      # x3 = 0xFF << 4 = 0xFF0
    
    li      x7, 0xFF0       # x7 = expected value (0xFF0)
    bne     x3, x7, fail    # if x3 != 0xFF0, test failed

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
