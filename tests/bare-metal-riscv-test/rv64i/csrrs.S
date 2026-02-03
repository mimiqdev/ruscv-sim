# Test: CSRRS - CSR Read Set
# Tests: CSRRS (read CSR, set bits specified by rs1)

.section .text
.globl _start

_start:
    # Using mscratch (0x340) for testing

    # Initialize mscratch to 0
    li      x1, 0
    csrrw   x0, 0x340, x1

    # Test 1: Set single bit
    li      x1, 0x1                 # Bit 0
    csrrs   x2, 0x340, x1           # x2 = old mscratch (0), mscratch |= 0x1
    li      x7, 0                   # Expected old value
    bne     x2, x7, fail
    csrrs   x3, 0x340, x0           # Read mscratch
    li      x7, 0x1
    bne     x3, x7, fail            # mscratch should be 0x1

    # Test 2: Set another bit (should OR with existing)
    li      x1, 0x2                 # Bit 1
    csrrs   x4, 0x340, x1           # x4 = old mscratch (0x1), mscratch |= 0x2
    li      x7, 0x1
    bne     x4, x7, fail
    csrrs   x5, 0x340, x0           # Read mscratch
    li      x7, 0x3                 # Should be 0x1 | 0x2 = 0x3
    bne     x5, x7, fail

    # Test 3: Set multiple bits at once
    li      x1, 0xFF00
    csrrs   x0, 0x340, x1           # mscratch |= 0xFF00
    csrrs   x6, 0x340, x0           # Read mscratch
    li      x7, 0xFF03              # Should be 0x3 | 0xFF00 = 0xFF03
    bne     x6, x7, fail

    # Test 4: CSRRS with x0 as rs1 (read only, no modify)
    li      x1, 0xFFFFFFFF          # This should not be used
    csrrs   x7, 0x340, x0           # x7 = mscratch, no modification
    li      x8, 0xFF03
    bne     x7, x8, fail
    csrrs   x9, 0x340, x0           # Read again
    bne     x9, x8, fail            # Should be unchanged

    # Test 5: Set already set bits (no change)
    li      x1, 0x1                 # Bit 0 already set
    csrrs   x0, 0x340, x1
    csrrs   x10, 0x340, x0          # Read
    li      x7, 0xFF03
    bne     x10, x7, fail

    # Reset mscratch
    li      x1, 0
    csrrw   x0, 0x340, x1

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
