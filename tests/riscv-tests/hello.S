# RISC-V test: Hello World via UART
# Tests: LUI, ADDI, LB, SB, ANDI, BEQZ
# Base Address: 0x10000000

.equ UART_BASE,      0x10000000
.equ UART_TX,        0x00
.equ UART_LSR,       0x05
.equ UART_LSR_EMPTY, 0x20

.section .text
.globl _start

_start:
    # --- Load UART_BASE into x10 ---
    # lui uses 20-bit signed immediate, so we cannot directly use 0x10000
    # Instead: load 0x1000 then shift left by 4 to get 0x10000, then shift by 12 more (lui behavior)
    # Actually: lui places imm[19:0] in bits [31:12], so we need lui with 0x1000 then slli by 4
    lui     x10, 0x1000         # x10 = 0x1000000
    slli    x10, x10, 4         # x10 = 0x10000000 (UART_BASE)

    # --- 输出 'H' ---
    call    wait_tx_ready
    li      x2, 'H'
    sb      x2, 0(x10)          # use x10

    # --- 输出 'e' ---
    call    wait_tx_ready
    li      x2, 'e'
    sb      x2, 0(x10)

    # --- 输出 'l' ---
    call    wait_tx_ready
    li      x2, 'l'
    sb      x2, 0(x10)

    # --- 输出 'l' ---
    call    wait_tx_ready
    li      x2, 'l'
    sb      x2, 0(x10)

    # --- 输出 'o' ---
    call    wait_tx_ready
    li      x2, 'o'
    sb      x2, 0(x10)

    # --- 输出 '!' ---
    call    wait_tx_ready
    li      x2, '!'
    sb      x2, 0(x10)

    # --- 输出 '\n' ---
    call    wait_tx_ready
    li      x2, '\n'
    sb      x2, 0(x10)

    j       pass

# --- Subroutine: wait for UART transmit ready ---
wait_tx_ready:
    # use x10 as base address
    lbu     x3, UART_LSR(x10)
    andi    x3, x3, UART_LSR_EMPTY
    beqz    x3, wait_tx_ready
    ret     # ret is actually jalr x0, 0(x1), using x1 here is correct

pass:
    la      x4, tohost
    li      x5, 0
    li      x6, 1
    slli    x6, x6, 63
    or      x5, x5, x6
    sd      x5, 0(x4)

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
