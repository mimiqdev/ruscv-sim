# Simple RISC-V test: BLT branch instruction test matching user's description
# Tests: BLT, ADD, ADDI, done label at 0x8000001c

.section .text
.globl _start

_start:
    # Initialize gp=10, sp=1
    li      x3, 10          # gp = x3 = 10
    li      x2, 1           # sp = x2 = 1
    li      x1, 0           # sum = 0

loop:
    # blt gp, sp, 0x8000001c (this is what user describes)
    blt     x3, x2, done    # if gp < sp, jump to done (should be 0x8000001c)
    
    # add instruction (user mentions "add")
    add     x1, x1, x2      # sum += sp
    
    # addi instruction (user mentions "addi")
    addi    x2, x2, 1       # sp++
    
    # Jump back to loop
    j       loop

done:
    # Exit with code 55
    la      x5, tohost
    li      x6, 55          # Expected exit code
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
