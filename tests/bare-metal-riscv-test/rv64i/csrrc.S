# Test: CSRRC - CSR Read Clear
# Tests: CSRRC (read CSR, clear bits specified by rs1)

.section .text
.globl _start

_start:
    # Using mscratch (0x340) for testing

    # Initialize mscratch to all 1s (0xFFFFFFFFFFFFFFFF)
    li      x1, 0xFFFFFFFFFFFFFFFF
    csrrw   x0, 0x340, x1

    # Test 1: Clear single bit
    li      x1, 0x1                 # Bit 0
    csrrc   x2, 0x340, x1           # x2 = old mscratch (all 1s), mscratch &= ~0x1
    li      x7, 0xFFFFFFFFFFFFFFFF
    bne     x2, x7, fail
    csrrc   x3, 0x340, x0           # Read mscratch
    li      x7, 0xFFFFFFFFFFFFFFFE  # Bit 0 cleared
    bne     x3, x7, fail

    # Test 2: Clear another bit
    li      x1, 0x2                 # Bit 1
    csrrc   x4, 0x340, x1           # x4 = old mscratch, mscratch &= ~0x2
    li      x7, 0xFFFFFFFFFFFFFFFE
    bne     x4, x7, fail
    csrrc   x5, 0x340, x0           # Read mscratch
    li      x7, 0xFFFFFFFFFFFFFFFC  # Bits 0,1 cleared
    bne     x5, x7, fail

    # Test 3: Clear multiple bits at once
    li      x1, 0xFF00
    csrrc   x0, 0x340, x1           # mscratch &= ~0xFF00
    csrrc   x6, 0x340, x0           # Read mscratch
    li      x7, 0xFFFFFFFFFFFF00FC  # Bits 0,1 and 8-15 cleared
    bne     x6, x7, fail

    # Test 4: CSRRC with x0 as rs1 (read only, no modify)
    csrrc   x7, 0x340, x0           # x7 = mscratch, no modification
    li      x8, 0xFFFFFFFFFFFF00FC
    bne     x7, x8, fail
    csrrc   x9, 0x340, x0           # Read again
    bne     x9, x8, fail            # Should be unchanged

    # Test 5: Clear already cleared bits (no change)
    li      x1, 0x1                 # Bit 0 already cleared
    csrrc   x0, 0x340, x1
    csrrc   x10, 0x340, x0          # Read
    li      x7, 0xFFFFFFFFFFFF00FC
    bne     x10, x7, fail

    # Test 6: Clear all remaining bits
    li      x1, 0xFFFFFFFFFFFFFFFF
    csrrc   x0, 0x340, x1           # Clear all bits
    csrrc   x11, 0x340, x0          # Read
    bne     x11, x0, fail           # Should be 0

    # Reset mscratch
    csrrw   x0, 0x340, x0

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
    sd      x5, 0(x4)

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
