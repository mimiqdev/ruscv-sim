# public-v1 s4_mapped: Sv39 one-level map, S-mode load+store through a
# translated VA, A/D set by hardware in the leaf PTE, then back to Bare and
# exit 0.  work=1, checksum=0x9E3.
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
    # root[vpn2=1]: 1 GiB superpage VA 0x40000000 -> PA 0x80000000, V|R|W
    la t0, root
    li t1, 0x80000
    slli t1, t1, 10
    ori t1, t1, 0x0f
    sd t1, 8(t0)
    # VA 0x40003008 -> PA 0x80003008 (scratch): compose in s2
    li t2, 0x40000000
    li t3, 0x80000000
    la t4, scratch
    sub t4, t4, t3
    add s2, t4, t2
    # data value in scratch, neighbor zero
    li t1, 0x9E3
    sd t1, 0(s0)
    sd x0, 8(s0)
    # satp = Sv39 | root PPN
    la t0, root
    srli t0, t0, 12
    li t1, 8
    slli t1, t1, 60
    or t0, t0, t1
    csrw mepc, x0
    la t0, s_code
    csrw mepc, t0
    li t0, 0x800                  # MPP = S
    csrw mstatus, t0
    csrw satp, t0
    mret
s_code:
    ld t1, 0(s2)                  # translated load: 0x9E3
    li t2, 0x9E3
    bne t1, t2, fail
    sd t1, 8(s2)                  # translated store
    csrw satp, x0                 # S-mode satp write at TVM=0: retires
    ecall                         # S-mode ecall: taken in M
m_cont:
    csrw satp, x0                 # back to Bare in M
    ld t1, 8(s0)
    li t2, 0x9E3
    bne t1, t2, fail
    la t0, root
    ld t1, 8(t0)
    li t2, 0x60                   # A | D
    and t1, t1, t2
    bne t1, t2, fail
    sd t1, 0(s1)
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
    csrr t0, mscratch
    csrw mepc, t0
    li t0, 0x1880                 # MIE | MPIE | MPP = M
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
.balign 4096
root: .space 4096
