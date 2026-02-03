# RISC-V test: Fibonacci sequence
# Calculate Fibonacci numbers up to F10
# Sequence: 0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55
# Goal: Verify that F10 equals 55
# Result: Pass (0) if x2 == 55, Fail (1) otherwise

.section .text
.globl _start

_start:
    # Initialize Fibonacci: F0=0, F1=1
    li      x1, 0           # a = 0 (F0)
    li      x2, 1           # b = 1 (F1)
    
    # Need 9 iterations to calculate from F1 to F10
    # Iter 1: 0+1=1 (F2)
    # ...
    # Iter 9: 34+21=55 (F10)
    li      x3, 9           # count = 9
    li      x4, 0           # i = 0

fib_loop:
    # if i >= count, exit loop
    bge     x4, x3, check_result

    # Compute next: c = a + b
    add     x5, x1, x2

    # Shift: a = b, b = c
    mv      x1, x2          # a = old_b
    mv      x2, x5          # b = new_sum (c)

    # Increment counter
    addi    x4, x4, 1

    j       fib_loop

check_result:
    # At this point x2 should equal 55 (F10)
    li      x7, 55          # Expected value
    beq     x2, x7, pass    # If match, jump to pass

fail:
    # Test failed, exit code = 1
    li      x5, 1
    j       write_exit

pass:
    # Test passed, exit code = 0
    li      x5, 0

write_exit:
    # Load tohost address (assembler handles la pseudo-instruction)
    la      x4, tohost

    # Construct write data: (1 << 63) | exit_code
    li      x6, 1
    slli    x6, x6, 63      # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6      # x5 = WRITE_MARKER | exit_code

    # Write to memory
    sd      x5, 0(x4)

    # Infinite loop waiting for simulator to terminate
inf:
    j       inf

# --- Key change: use .tohost section ---
# This aligns with linker.ld to automatically place at 0x80001000
.section .tohost
.align 3
tohost:
    .dword  0
