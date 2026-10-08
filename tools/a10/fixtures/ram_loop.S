# public-v1 RAM/control/signature+BSS: 16 iterations, checksum sum(1..16)=136.
.option norvc
.option norelax
.section .text
.globl _start
_start:
    la s0, scratch
    la s1, signature
    li t0, 16
    li t1, 0
    li t2, 0
    ld t3, 0(s0)
    bnez t3, fail
loop:
    addi t1, t1, 1
    sw t1, 0(s0)
    lw t3, 0(s0)
    bne t3, t1, fail
    add t2, t2, t3
    addi t0, t0, -1
    bnez t0, loop
    li t4, 136
    bne t2, t4, fail
    addi x0, x0, 9
    sd t1, 0(s1)
    sd t2, 8(s1)
sample_checks:
    la t6, neighbors
    ld t0, 0(t6)
    li t4, 0x123
    bne t0, t4, fail
    ld t0, 8(t6)
    li t4, 16
    bne t0, t4, fail
    ld t0, 16(t6)
    bnez t0, fail
counter_read:
    csrr t5, minstret
    sd t5, 16(s1)
    li a0, 7
    la t6, tohost
    sd a0, 0(t6)
1:  j 1b
fail:
    li a0, 41
    la t6, tohost
    sd a0, 0(t6)
2:  j 2b
.section .tohost,"aw",@progbits
.balign 8
tohost: .dword 0
.section .signature,"aw",@progbits
.balign 8
signature: .dword 0,0,0
.section .data
.balign 8
neighbors: .dword 0x123
.section .bss
.balign 8
scratch: .space 16
