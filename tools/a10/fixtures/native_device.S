# Existing A9 native-only six-instruction UART/fixed-HTIF sequence.
.option norvc
.section .text
.globl _start
_start:
    lui x1, 0x10000
    addi x2, x0, 65
    sb x2, 0(x1)
    lui x1, 0x40008
    addi x2, x0, 7
    sd x2, 0(x1)
