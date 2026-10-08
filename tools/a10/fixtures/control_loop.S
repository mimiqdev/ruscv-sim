# public-v1 bounded integer/control fixture: 32 additions, checksum 528.
.option norvc
.option norelax
.section .text
.globl _start
_start:
    li t0, 32
    li t1, 0
    li t2, 0
loop:
    addi t1, t1, 1
    add t2, t2, t1
    addi t0, t0, -1
    bnez t0, loop
    li t3, 528
    bne t2, t3, fail
    bne t0, x0, fail
    addi x0, x0, 9
    la s0, signature
    sd t1, 0(s0)
    sd t2, 8(s0)
sample_checks:
    la t6, neighbors
    ld t0, 0(t6)
    li t4, 0x123
    bne t0, t4, fail
    ld t0, 8(t6)
    li t4, 0
    bne t0, t4, fail
    ld t0, 16(t6)
    bnez t0, fail
counter_read:
    csrr t5, minstret
    sd t5, 16(s0)
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
