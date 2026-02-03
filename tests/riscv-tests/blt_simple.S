# Ultra-simple BLT test: branch should be taken immediately
# gp=20, sp=1, so sp < gp, blt should NOT be taken
# Then we add sp until sp > gp, then branch should be taken

.section .text
.globl _start

_start:
    # Initialize: gp = x3 = 20, sp = x2 = 1
    li      x3, 20          # gp = 20
    li      x2, 1           # sp = 1

    # Test 1: blt with sp < gp (should NOT branch)
    # This should print "[BRANCH-TAKEN]" if incorrectly branching
    blt     x3, x2, done    # if 20 < 1? No, so don't branch
    
    # Increment sp
    addi    x2, x2, 1       # sp = 2
    
    # Test 2: blt with sp < gp (should NOT branch)
    blt     x3, x2, done    # if 20 < 2? No
    
    addi    x2, x2, 1       # sp = 3
    blt     x3, x2, done    # if 20 < 3? No
    
    # ... continue until sp > gp ...
    # When sp = 21, 20 < 21 is true, branch to done
    
    addi    x2, x2, 18      # sp = 21 (now sp > gp)
    
    # Now blt should branch to done
    blt     x3, x2, done    # if 20 < 21? YES! Branch to done

loop_here:
    # If we get here, blt didn't branch (bug!)
    j       loop_here

done:
    # Exit with sp value
    la      x5, tohost
    li      x6, 21          # Expected exit code
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
