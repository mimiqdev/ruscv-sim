# Test: CSRRSI - CSR Read Set Immediate
# Tests: CSRRSI (read CSR, set bits specified by 5-bit immediate)

.section .text
.globl _start

_start:
    # Using mscratch (0x340) for testing

    # Initialize mscratch to 0
    csrrwi  x0, 0x340, 0

    # Test 1: Set single bit with immediate
    csrrsi  x1, 0x340, 1            # x1 = old mscratch (0), mscratch |= 0x1
    bne     x1, x0, fail
    csrrs   x2, 0x340, x0           # Read mscratch
    li      x7, 0x1
    bne     x2, x7, fail

    # Test 2: Set another bit
    csrrsi  x3, 0x340, 2            # x3 = old mscratch (0x1), mscratch |= 0x2
    li      x7, 0x1
    bne     x3, x7, fail
    csrrs   x4, 0x340, x0
    li      x7, 0x3                 # 0x1 | 0x2 = 0x3
    bne     x4, x7, fail

    # Test 3: Set multiple bits
    csrrsi  x0, 0x340, 0x18         # mscratch |= 0x18 (bits 3,4)
    csrrs   x5, 0x340, x0
    li      x7, 0x1B                # 0x3 | 0x18 = 0x1B
    bne     x5, x7, fail

    # Test 4: Set already set bits
    csrrsi  x0, 0x340, 1            # Set bit 0 again
    csrrs   x6, 0x340, x0
    li      x7, 0x1B
    bne     x6, x7, fail

    # Test 5: CSRRSI with uimm=0 (read only)
    csrrsi  x7, 0x340, 0            # Read mscratch, no change
    li      x8, 0x1B
    bne     x7, x8, fail
    csrrs   x9, 0x340, x0
    bne     x9, x8, fail

    # Test 6: Set max bits (0x1F = bits 0-4)
    csrrwi  x0, 0x340, 0            # Reset to 0
    csrrsi  x10, 0x340, 0x1F        # Set bits 0-4
    csrrs   x11, 0x340, x0
    li      x7, 0x1F
    bne     x11, x7, fail

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
