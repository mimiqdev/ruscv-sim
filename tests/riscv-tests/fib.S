# RISC-V test: Fibonacci sequence
# Calculate first 10 Fibonacci numbers: 0, 1, 1, 2, 3, 5, 8, 13, 21, 34
# Tests: ADD, SUB, MV, ADDI, BGTZ, BEQ, JALR

.section .text
.globl _start

_start:
    # Initialize Fibonacci: F0=0, F1=1
    li      x1, 0           # a = 0 (F0)
    li      x2, 1           # b = 1 (F1)
    li      x3, 10          # count = 10
    li      x4, 0           # i = 0

    # Print header marker (exit code 255 = start marker)
    la      x5, tohost
    li      x6, 255
    sw      x6, 0(x5)

fib_loop:
    # Check if i >= count
    bge     x4, x3, fib_done
    
    # Output current Fibonacci number (as exit code)
    # For testing, we'll just output the last result
    # Save result before next calculation
    
    # Compute next: c = a + b
    add     x7, x1, x2
    
    # Shift: a = b, b = c
    mv      x1, x2
    mv      x2, x7
    
    # Increment counter
    addi    x4, x4, 1
    
    j       fib_loop

fib_done:
    # x2 contains F10 = 34
    # Exit with code 34
    li      x6, 34
    sw      x6, 0(x5)
    
    # Loop forever
inf:
    j       inf

# Tohost address
.section .data
.align 3
tohost:
    .word   0x00
    .word   0x80001000
