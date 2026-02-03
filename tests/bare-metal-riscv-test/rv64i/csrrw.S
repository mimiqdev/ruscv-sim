# Test: CSRRW - CSR Read Write
# Tests: CSRRW (read CSR, write new value to CSR)

.section .text
.globl _start

_start:
    # Note: mscratch is a read-write CSR (0x340)
    # It's safe to modify for testing

    # Test 1: Write new value, read old value
    li      x1, 0x123456789ABCDEF0  # New value to write
    li      x2, 0x0                 # Expected old value (mscratch should be 0 initially)
    
    csrrw   x3, 0x340, x1           # x3 = old mscratch, mscratch = x1
    # Note: We can't reliably check initial value, so just verify write worked
    
    # Read back to verify write
    csrrs   x4, 0x340, x0           # x4 = current mscratch (read without modify)
    bne     x4, x1, fail            # mscratch should equal what we wrote

    # Test 2: CSRRW with x0 as rd (write only, no read)
    li      x5, 0xAABBCCDDEEFF0011
    csrrw   x0, 0x340, x5           # mscratch = x5, don't care about old value
    csrrs   x6, 0x340, x0           # x6 = mscratch
    bne     x6, x5, fail

    # Test 3: CSRRW with same source and destination
    li      x7, 0x1122334455667788
    mv      x8, x7
    csrrw   x8, 0x340, x8           # x8 = old mscratch, mscratch = old mscratch
    bne     x8, x5, fail            # x8 should have previous value

    # Test 4: Reset mscratch to 0
    li      x9, 0
    csrrw   x0, 0x340, x9           # mscratch = 0

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
