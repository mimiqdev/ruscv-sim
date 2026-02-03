# Test: SRL - Shift Right Logical instruction test
# Tests: SRL

.section .text
.globl _start

_start:
    # === Test 1: Basic shift right (16 >> 2 = 4) ===
    li      x1, 16          # x1 = 16
    li      x2, 2           # x2 = 2
    srl     x3, x1, x2      # x3 = x1 >> x2 = 16 >> 2 = 4
    
    li      x7, 4           # x7 = expected value (4)
    bne     x3, x7, fail    # if x3 != 4, test failed

    # === Test 2: Shift right by zero (0xAB >> 0 = 0xAB) ===
    li      x1, 0xAB        # x1 = 0xAB
    li      x2, 0           # x2 = 0
    srl     x3, x1, x2      # x3 = x1 >> 0 = 0xAB
    
    li      x7, 0xAB        # x7 = expected value (0xAB)
    bne     x3, x7, fail    # if x3 != 0xAB, test failed

    # === Test 3: Shift right 8 bits (0x1234 >> 8 = 0x12) ===
    li      x1, 0x1234      # x1 = 0x1234
    li      x2, 8           # x2 = 8
    srl     x3, x1, x2      # x3 = x1 >> 8 = 0x12
    
    li      x7, 0x12        # x7 = expected value (0x12)
    bne     x3, x7, fail    # if x3 != 0x12, test failed

    # === Test 4: 64-bit shift right (0x100000000 >> 32 = 1) ===
    li      x1, 0x100000000 # x1 = 0x100000000
    li      x2, 32          # x2 = 32
    srl     x3, x1, x2      # x3 = 0x100000000 >> 32 = 1
    
    li      x7, 1           # x7 = expected value (1)
    bne     x3, x7, fail    # if x3 != 1, test failed

    # === Test 5: Logical shift right - zero fill (0x8000000000000000 >> 63 = 1) ===
    li      x1, 0x8000000000000000  # x1 = 0x8000000000000000 (sign bit set)
    li      x2, 63                  # x2 = 63
    srl     x3, x1, x2              # x3 = logical shift, should be 1
    
    li      x7, 1                   # x7 = expected value (1, not -1)
    bne     x3, x7, fail            # if x3 != 1, test failed

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
