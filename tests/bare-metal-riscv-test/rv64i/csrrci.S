# Test: CSRRCI - CSR Read Clear Immediate
# Tests: CSRRCI (read CSR, clear bits specified by 5-bit immediate)

.section .text
.globl _start

_start:
    # Using mscratch (0x340) for testing

    # Initialize mscratch to all 1s (0xFFFFFFFFFFFFFFFF)
    li      x1, 0xFFFFFFFFFFFFFFFF
    csrrw   x0, 0x340, x1

    # Test 1: Clear single bit with immediate
    csrrci  x2, 0x340, 1            # x2 = old mscratch (all 1s), mscratch &= ~0x1
    li      x7, 0xFFFFFFFFFFFFFFFF
    bne     x2, x7, fail
    csrrc   x3, 0x340, x0           # Read mscratch
    li      x7, 0xFFFFFFFFFFFFFFFE  # Bit 0 cleared
    bne     x3, x7, fail

    # Test 2: Clear another bit
    csrrci  x4, 0x340, 2            # x4 = old mscratch, mscratch &= ~0x2
    li      x7, 0xFFFFFFFFFFFFFFFE
    bne     x4, x7, fail
    csrrc   x5, 0x340, x0
    li      x7, 0xFFFFFFFFFFFFFFFC  # Bits 0,1 cleared
    bne     x5, x7, fail

    # Test 3: Clear multiple bits
    csrrci  x0, 0x340, 0x18         # mscratch &= ~0x18 (bits 3,4)
    csrrc   x6, 0x340, x0
    li      x7, 0xFFFFFFFFFFFFFFE4  # Bits 0,1,3,4 cleared (0xFC & ~0x18 = 0xE4)
    bne     x6, x7, fail

    # Test 4: Clear already cleared bits
    csrrci  x0, 0x340, 1            # Clear bit 0 again
    csrrc   x7, 0x340, x0
    li      x8, 0xFFFFFFFFFFFFFFE4
    bne     x7, x8, fail

    # Test 5: CSRRCI with uimm=0 (read only)
    csrrci  x9, 0x340, 0            # Read mscratch, no change
    bne     x9, x8, fail
    csrrc   x10, 0x340, x0
    bne     x10, x8, fail

    # Test 6: Clear all 5-bit representable bits (0x1F = bits 0-4)
    csrrci  x0, 0x340, 0x1F         # Clear bits 0-4
    csrrc   x11, 0x340, x0
    li      x7, 0xFFFFFFFFFFFFFFE0  # Bits 0-4 cleared
    bne     x11, x7, fail

    # Test 7: Clear remaining high bits manually
    li      x1, 0xFFFFFFFFFFFFFFFF
    csrrw   x0, 0x340, x1           # Reset to all 1s
    
    # Clear multiple times with different uimm values
    csrrci  x0, 0x340, 0x1F         # Clear bits 0-4
    csrrci  x0, 0x340, 0x1F         # Clear again (no effect on cleared bits)
    
    csrrc   x12, 0x340, x0
    li      x7, 0xFFFFFFFFFFFFFFE0
    bne     x12, x7, fail

    # Reset mscratch
    csrrwi  x0, 0x340, 0

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
