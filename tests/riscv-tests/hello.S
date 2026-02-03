# RISC-V test: Hello World via UART
# Outputs characters to UART base address
# Tests: LUI, ADDI, SW, SB, loop

#define UART_BASE     0x10000000
#define UART_TX       0x00
#define UART_LSR      0x05

.section .text
.globl _start

_start:
    # Load UART base address
    lui     x1, 0x10000      # UART_BASE = 0x10000000
    
    # Output 'H'
    li      x2, 'H'
    sb      x2, UART_TX(x1)
    
    # Wait for TX ready (read LSR)
wait_h:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20     # Check THR empty bit
    beqz    x3, wait_h
    
    # Output 'e'
    li      x2, 'e'
    sb      x2, UART_TX(x1)

wait_e:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20
    beqz    x3, wait_e
    
    # Output 'l'
    li      x2, 'l'
    sb      x2, UART_TX(x1)

wait_l:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20
    beqz    x3, wait_l
    
    # Output 'l'
    li      x2, 'l'
    sb      x2, UART_TX(x1)

wait_l2:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20
    beqz    x3, wait_l2
    
    # Output 'o'
    li      x2, 'o'
    sb      x2, UART_TX(x1)

wait_o:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20
    beqz    x3, wait_o
    
    # Output '!'
    li      x2, '!'
    sb      x2, UART_TX(x1)

wait_excl:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20
    beqz    x3, wait_excl
    
    # Output newline
    li      x2, '\n'
    sb      x2, UART_TX(x1)

wait_nl:
    lbu     x3, UART_LSR(x1)
    andi    x3, x3, 0x20
    beqz    x3, wait_nl
    
    # Exit with code 0 (success)
    la      x4, tohost
    li      x5, 0
    sw      x5, 0(x4)
    
    # Loop forever
inf:
    j       inf

# Tohost address
.section .data
.align 3
tohost:
    .word   0x00
    .word   0x80001000
