# public-v1 s4 csr_fields: M-mode CSR field policy on one compact path.
# mstatus rejects MPP=2 (keeps 0) and FS/SD read 0 with SXL/UXL=2; misa
# ignores an all-ones write; mepc keeps bits 1:0 clear; mtvec MODE stays in
# {0,1}; satp ignores an unsupported MODE in full; mip/mie keep only 0x222.
# work=6, checksum=0xB0B, exit 0.
.option norvc
.option norelax
.section .text
.globl _start
_start:
    la s0, scratch
    la s1, signature
    li t0, (2 << 11) | (1 << 14) | (1 << 63)
    csrw mstatus, t0
    csrr t1, mstatus
    li t2, 0xA00000000
    bne t1, t2, fail
    li t0, -1
    csrw misa, t0
    csrr t1, misa
    li t2, 0x8000000000141101
    bne t1, t2, fail
    li t0, -1
    csrw mepc, t0
    csrr t1, mepc
    andi t3, t1, 3
    bnez t3, fail
    la t0, vect
    ori t0, t0, 1
    csrw mtvec, t0
    li t0, 3
    csrw mtvec, t0
    csrr t1, mtvec
    andi t3, t1, 3
    li t2, 1
    bne t3, t2, fail
    li t0, 9 << 60
    csrw satp, t0
    csrr t1, satp
    bnez t1, fail
    li t0, -1
    csrw mie, t0
    csrw mip, t0
    csrw mip, x0
    csrw mie, x0
    csrr t1, mie
    bnez t1, fail
    li t0, 0xB0B
    sd t0, 0(s0)
    sd x0, 8(s0)
sample_checks:
    ld t0, 0(s0)
    li t4, 0xB0B
    bne t0, t4, fail
    ld t0, 8(s0)
    bnez t0, fail
    li t0, 6
    sd t0, 0(s1)
    li t0, 0xB0B
    sd t0, 8(s1)
counter_read:
    csrr t5, minstret
    sd t5, 16(s1)
    li a0, 1
    la t6, tohost
    sd a0, 0(t6)
1:  j 1b
fail:
    li a0, 41
    la t6, tohost
    sd a0, 0(t6)
2:  j 2b
vect:
    j fail
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
