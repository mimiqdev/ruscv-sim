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
    # --- 修复点：使用 x10 而不是 x1 ---
    # x1 (ra) 是函数调用的返回地址寄存器，不能随便用来存全局变量
    lui     x10, 0x10000        # x10 = UART_BASE

    # --- 输出 'H' ---
    call    wait_tx_ready
    li      x2, 'H'
    sb      x2, 0(x10)          # 使用 x10

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

# --- 子程序：等待 UART 发送就绪 ---
wait_tx_ready:
    # 使用 x10 作为基址
    lbu     x3, UART_LSR(x10)
    andi    x3, x3, UART_LSR_EMPTY
    beqz    x3, wait_tx_ready
    ret     # ret 实际上是 jalr x0, 0(x1)，这里用到 x1 是正确的

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
