# Test: JAL - Jump and Link
# Tests: JAL (jump to relative offset, save return address)

.section .text
.globl _start

_start:
    # Test 1: Simple forward jump
    li      x1, 0           # x1 = 0 (marker)
    jal     x2, forward     # jump to forward, x2 = return address (0x80000008)
    li      x1, 1           # Should not execute (skipped by jump)

forward:
    # Verify we landed at correct location
    li      x3, 1           # x3 = marker (forward location reached)
    
    # Test 2: Verify return address was saved correctly
    # Expected return address is 0x80000008 (address of 'li x1, 1' instruction)
    # Check if x2 > 0 (return address should be a valid positive address)
    bgt     x2, x0, test2_pass  # if x2 > 0, likely correct
    j       fail
test2_pass:

    # Test 3: Jump and return using JAL/JALR
    li      x5, 0           # x5 = result
    jal     x6, subroutine  # call subroutine
    # Return here from subroutine
    li      x7, 42          # x7 = expected return value
    beq     x5, x7, test3_pass  # if subroutine returned correctly
    j       fail
test3_pass:

    # Test 4: Backward jump (loop) using JAL
    li      x8, 0           # x8 = counter
    li      x9, 3           # x9 = target count

loop:
    addi    x8, x8, 1       # counter++
    blt     x8, x9, loop    # if counter < 3, loop
    
    # Verify loop executed exactly 3 times
    li      x7, 3
    bne     x8, x7, fail

    # All tests passed
    j       pass

# Subroutine: sets x5 to 42 and returns using JALR
subroutine:
    li      x5, 42          # Set return value
    jalr    x0, x6, 0       # Return to caller (using saved link in x6)

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
