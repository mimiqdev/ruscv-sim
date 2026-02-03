# Test: JALR - Jump and Link Register
# Tests: JALR (jump to address in register, save return address)

.section .text
.globl _start

_start:
    # Test 1: Simple jump to register address
    li      x1, 0           # x1 = 0 (marker)
    la      x2, target1     # x2 = address of target1
    jalr    x3, x2, 0       # jump to x2 + 0, x3 = return address
    li      x1, 1           # Should not execute (skipped by jump)

target1:
    # Verify we landed at correct location
    li      x4, 1           # x4 = marker (target1 reached)
    
    # Test 2: Verify return address was saved (should be positive address)
    bgt     x3, x0, test2_pass
    j       fail
test2_pass:

    # Test 3: Return from subroutine using JALR
    la      x6, target2     # x6 = address of target2
    jalr    x7, x6, 0       # jump to target2
    # Return here after target2
    j       test4           # Continue to test 4

target2:
    # Return using the saved link address
    jalr    x0, x7, 0       # Return to caller (jump to x7 + 0)
    # Should never reach here
    j       fail

test4:
    # Test 4: JALR with offset
    la      x8, target3     # x8 = base address
    addi    x8, x8, -4      # Adjust base back by 4
    jalr    x9, x8, 4       # jump to x8 + 4 = target3
    j       fail            # Should not execute

target3:
    # Verify we landed correctly with offset
    li      x10, 1          # marker
    
    # Test 5: Function call chain using JAL/JALR
    jal     x11, func_a     # Call func_a
    
    # All tests passed
    j       pass

# Function A: calls Function B
func_a:
    li      x12, 10         # x12 = 10
    la      x13, func_b
    jalr    x14, x13, 0     # Call func_b
    
    # Check result from func_b
    li      x7, 20
    bne     x12, x7, fail   # func_b should have set x12 = 20
    
    jalr    x0, x11, 0      # Return to caller

# Function B: modifies x12
func_b:
    addi    x12, x12, 10    # x12 = 20
    jalr    x0, x14, 0      # Return to func_a

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
