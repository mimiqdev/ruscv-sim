# public-v1 mixed D: 8 turns of ordinary/AMO/LR/success+failure SC.
# -2 + 5 = 3, then successful SC writes -2, store invalidates LR,
# failed SC and x0 SC leave 3 unchanged. Checksum = 8*3 = 24.
.option norvc
.option norelax
.section .text
.globl _start
_start:
    la s0, scratch
    la s1, signature
    li s2, 8
    li s3, 0
    li s4, 0
    li t1, -2
    li t2, 5
    li t3, 3
loop:
    sd t1, 0(s0)
    amoadd.d a1, t2, (s0)
    bne a1, t1, fail
    lr.d a2, (s0)
    bne a2, t3, fail
    sc.d a3, t1, (s0)
    bnez a3, fail
    lr.d a4, (s0)
    bne a4, t1, fail
    sd t3, 0(s0)
    sc.d a5, t2, (s0)
    li t4, 1
    bne a5, t4, fail
    sc.d x0, t2, (s0)
    ld a6, 0(s0)
    bne a6, t3, fail
    add s3, s3, a6
    addi s4, s4, 1
    addi s2, s2, -1
    bnez s2, loop
    li t4, 24
    bne s3, t4, fail
    addi x0, x0, 9
    sd s4, 0(s1)
    sd s3, 8(s1)
sample_checks:
    la t6, neighbors
    ld t0, 0(t6)
    li t4, 0x123
    bne t0, t4, fail
    ld t0, 8(t6)
    li t4, 3
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
