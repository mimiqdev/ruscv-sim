# Test: CSRRWI - CSR Read Write Immediate
# Tests: CSRRWI (read CSR, write 5-bit immediate to CSR)

.section .text
.globl _start

_start:
    # Using mscratch (0x340) for testing

    # Initialize mscratch to 0
    csrrwi  x0, 0x340, 0

    # Test 1: Write immediate, read old value
    # uimm=5 (0x5), should read old value (0) into x1
    csrrwi  x1, 0x340, 5            # x1 = old mscratch (0), mscratch = 5
    bne     x1, x0, fail
    
    # Verify write
    csrrs   x2, 0x340, x0           # Read mscratch
    li      x7, 5
    bne     x2, x7, fail

    # Test 2: CSRRWI with x0 as rd (write only)
    csrrwi  x0, 0x340, 10           # mscratch = 10
    csrrs   x3, 0x340, x0           # Read mscratch
    li      x7, 10
    bne     x3, x7, fail

    # Test 3: Write max 5-bit immediate (31)
    csrrwi  x4, 0x340, 31           # x4 = old value (10), mscratch = 31
    li      x7, 10
    bne     x4, x7, fail
    csrrs   x5, 0x340, x0
    li      x7, 31
    bne     x5, x7, fail

    # Test 4: Write zero immediate
    csrrwi  x0, 0x340, 0            # mscratch = 0
    csrrs   x6, 0x340, x0
    bne     x6, x0, fail

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
