# public-v1 s4_ssip: SSIP+SSIE taken in M once MIE=1; mcause is
# (1<<63)|1, mepc is the interrupted instruction, mtval is 0; the handler
# clears the pending bit and exits 0.  work=1, checksum=0x55.
.option norvc
.option norelax
.section .text
.globl _start
_start:
    la s0, scratch
    la s1, signature
    la t0, handler
    csrw mtvec, t0
    la t0, m_cont
    csrw mscratch, t0
    li t0, 2                        # SSIE
    csrw mie, t0
    li t0, 2                        # SSIP
    csrw mip, t0
    li t0, 8                        # MIE
    csrs mstatus, t0
int_site:
    addi x0, x0, 7                  # interrupted here
    j fail
m_cont:
    csrr t0, mip
    bnez t0, fail
    li t0, 0x55
    sd t0, 0(s0)
    li t0, 1
    sd t0, 0(s1)
    li t0, 85
    sd t0, 8(s1)
counter_read:
    csrr t5, minstret
    sd t5, 16(s1)
    li t4, 1
    slli t4, t4, 63
    la t6, tohost
    sd t4, 0(t6)
4:  j 4b
fail:
    li a0, 41
    la t6, tohost
    sd a0, 0(t6)
3:  j 3b
handler:
    csrr t0, mcause
    li t1, 1
    slli t1, t1, 63
    addi t1, t1, 1                  # (1<<63) | SSI
    bne t0, t1, fail
    csrr t0, mepc
    la t1, int_site
    bne t0, t1, fail
    csrr t0, mtval
    bnez t0, fail
    csrw mip, x0
    csrr t0, mscratch
    csrw mepc, t0
    li t1, 0x1880                  # MIE | MPIE | MPP = M
    csrw mstatus, t1
    mret
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
