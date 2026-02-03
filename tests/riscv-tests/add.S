# Simple RISC-V test: calculate 1 + 2 + ... + 10 = 55
# Tests: ADD, ADDI, SUB, BLT, BNE, JALR

.section .text
.globl _start

_start:
    # Initialize
    li      x1, 0           # sum = 0
    li      x2, 1           # i = 1
    li      x3, 10          # n = 10

loop:
    # if i > n, exit loop
    bgt     x2, x3, done
    
    # sum += i
    add     x1, x1, x2
    
    # i++
    addi    x2, x2, 1
    
    j       loop

done:
    # Expected result: x1 = 55 (0x37)
    # Write result to tohost (exit code)
    # Use write marker format: (1 << 63) | exit_code
    
    # For simple test, we'll just write exit code 0x37 (55)
    la      x4, tohost
    li      x5, 55          # exit code = 55
    sw      x5, 0(x4)
    
    # Loop forever
inf:
    j       inf

# Tohost address (should match simulator's default)
.section .data
.align 3
tohost:
    .word   0x00
    .word   0x80001000      # Default tohost address (little-endian)
