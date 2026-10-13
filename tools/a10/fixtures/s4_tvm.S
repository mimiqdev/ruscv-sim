# public-v1 s4_tvm: TVM=1 makes an S-mode satp read raise illegal-instruction
# with mtval equal to the faulting csrr instruction bits; the handler resumes
# in M and exits 0.  SXL/UXL and MPIE transitions follow the standard M-mode
# trap entry.  work=1, checksum=0x77.
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
    li t0, 0x100000               # TVM
    csrw mstatus, t0
    la t0, m_cont
    csrw mscratch, t0
    mret
s_code:
    csrr t1, satp                  # traps: TVM=1 in S
    j fail
m_cont:
    li t0, 0x77
    sd t0, 0(s0)
    li t0, 1
    slli t0, t0, 63
    la t6, tohost
    sd t0, 0(t6)
2:  j 2b
fail:
    li a0, 41
    la t6, tohost
    sd a0, 0(t6)
3:  j 3b
handler:
    csrr t0, mcause
    li t1, 2
    bne t0, t1, fail
    csrr t0, mtval
    li t1, 0x18002373              # the csrr t1,satp bits
    bne t0, t1, fail
    csrr t0, mscratch
    csrw mepc, t0
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
