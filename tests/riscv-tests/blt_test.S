# Simple RISC-V test: BLT branch instruction test (no compressed instructions)
# Tests: BLT, ADDI, ADD, done label

.section .text
.globl _start

_start:
    # Initialize gp=20, sp=1
    li      x3, 20          # gp = x3 = 20
    li      x2, 1           # sp = x2 = 1

loop:
    # if sp > gp, jump to done
    blt     x3, x2, done    # if gp < sp (i.e., sp > gp), jump to done
    
    # sp++
    addi    x2, x2, 1
    
    # Dummy add to show PC movement
    add     x4, x2, x3
    
    j       loop

done:
    # Expected: sp=21 (when sp=21 > gp=20, branch taken)
    # Exit with code 21
    la      x5, tohost
    li      x6, 21
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
