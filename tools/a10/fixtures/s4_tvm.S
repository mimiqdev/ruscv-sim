# public-v1 s4_tvm: TVM=1 makes an S-mode satp read raise illegal-instruction
# with mtval equal to the faulting csrr bits. MPP is set to S after the TVM
# write so the access is the TVM trap, not a U-mode CSR privilege trap.
# The handler checks cause 2 and the exact instruction bits, returns to M,
# and exits 0. work=1, checksum=0x77.
.option norvc
.option norelax
.section .text
.globl _start
_start:
    la s0, scratch
    la s1, signature
    la t0, handler
    csrw mtvec, t0
    la t0, s_code
    csrw mepc, t0
    lui t0, 0x100                  # TVM
    csrw mstatus, t0
    addi t0, x0, 1
    slli t0, t0, 11                # MPP = S
    csrs mstatus, t0
    la t0, m_cont
    csrw mscratch, t0
    mret
s_code:
    csrr t1, satp                  # traps: TVM=1 in S; does not retire
    j fail
m_cont:
    li t0, 0x77
    sd t0, 0(s0)
    li t0, 1
    sd t0, 0(s1)
    li t0, 119
    sd t0, 8(s1)
counter_read:
    csrr t5, minstret
    sd t5, 16(s1)
    li a0, 1                       # HTIF payload (0 << 1) | 1 = exit 0
    la t6, tohost
    sd a0, 0(t6)
1:  j 1b
fail:
    li a0, 41
    la t6, tohost
    sd a0, 0(t6)
2:  j 2b
handler:
    csrr t0, mcause
    li t1, 2
    bne t0, t1, fail
    csrr t0, mtval
    li t1, 0x18002373              # csrr t1, satp
    bne t0, t1, fail
    csrr t0, mscratch
    csrw mepc, t0
    addi t0, x0, 0x188             # 0x188 << 4 = 0x1880: MPIE | MPP = M
    slli t0, t0, 4
    csrw mstatus, t0
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
