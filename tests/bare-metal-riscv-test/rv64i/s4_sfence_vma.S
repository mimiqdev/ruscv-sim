# Stage 4 SFENCE.VMA guest (dev-plan §4 "SFENCE.VMA", D8 and M4).
#
# Supervisor mode runs on an identity 1 GiB superpage (U=0), so it can
# rewrite its own leaf PTEs with ordinary stores. VA 0x3000 is remapped
# between two physical pages, and VA 0x4000 starts invalid and is then
# mapped. Each rewrite is followed by one of the four SFENCE.VMA forms and
# a load (or a store) through the newly written mapping:
#
#   sfence.vma x0, x0    global
#   sfence.vma x0, rs2   one ASID (rs2 = 0, the only ASID: ASIDLEN = 0)
#   sfence.vma rs1, x0   one VA, all ASIDs
#   sfence.vma rs1, rs2  one VA in one ASID
#
# The model has no translation cache (M4), so a rewritten leaf is already
# visible before the fence. The guest checks that once and records it as a
# property of this model, not as an architectural requirement: the
# privileged specification only guarantees the new translation after the
# fence. A non-canonical VA in rs1 raises no exception.
#
# Two illegal-instruction cases follow, each with mtval equal to the
# instruction bits: a SYSTEM funct3 = 0 encoding that is not SFENCE.VMA
# (HFENCE.VVMA; the H extension is absent) in S, and SFENCE.VMA in U.

    .equ ANY, -2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1

    # root[vpn2=0] -> mid table, mid[vpn1=0] -> leaf table (pointers).
    la      t0, root_table
    la      t1, mid_table
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 1
    sd      t1, 0(t0)
    la      t0, mid_table
    la      t1, leaf_table
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 1
    sd      t1, 0(t0)

    # root[vpn2=2]: identity 1 GiB superpage, R|W|X|A|D, U=0. Boot code,
    # the handler, the S-mode program, the page tables, and tohost.
    la      t0, root_table
    li      t1, 0x80000
    slli    t1, t1, 10
    ori     t1, t1, 0xcf
    sd      t1, 16(t0)

    # leaf[vpn0=2]: VA 0x2000 -> u_page, R|X|U|A.
    la      t0, leaf_table
    la      t1, u_page
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 0x5b
    sd      t1, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> page_a, R|W|A|D. leaf[vpn0=4] stays 0.
    la      t1, page_a
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 0xc7
    sd      t1, 24(t0)

    # Page contents: page_a and page_b differ, page_c is VA 0x4000's target.
    la      t0, page_a
    li      t1, 0xaaaa0000aaaa0001
    sd      t1, 0(t0)
    la      t0, page_b
    li      t1, 0xbbbb0000bbbb0002
    sd      t1, 0(t0)
    la      t0, page_c
    li      t1, 0xcccc0000cccc0003
    sd      t1, 0(t0)

    # Sv39 on, enter S at s_main on the identity superpage.
    la      t0, root_table
    srli    t0, t0, 12
    li      t1, 8
    slli    t1, t1, 60
    or      t0, t0, t1
    csrw    satp, t0
    la      t0, s_main
    csrw    mepc, t0
    li      t0, 0x800                   # MPP = S, MPRV = 0
    csrw    mstatus, t0
    mret

# s2 = &leaf[3] (VA 0x3000), s3 = &leaf[4] (VA 0x4000).
# s9 = PTE for page_a, s10 = PTE for page_b, s11 = PTE for page_c.
s_main:
    la      s2, leaf_table
    addi    s3, s2, 32
    addi    s2, s2, 24
    la      t0, page_a
    srli    t0, t0, 12
    slli    t0, t0, 10
    ori     s9, t0, 0xc7                # V | R | W | A | D
    la      t0, page_b
    srli    t0, t0, 12
    slli    t0, t0, 10
    ori     s10, t0, 0xc7
    la      t0, page_c
    srli    t0, t0, 12
    slli    t0, t0, 10
    ori     s11, t0, 0xc7
    li      a2, 0xaaaa0000aaaa0001
    li      a3, 0xbbbb0000bbbb0002
    li      a4, 0xcccc0000cccc0003
    li      s4, 0x3000

    # Initial mapping: VA 0x3000 -> page_a.
    ld      t1, 0(s4)
    bne     t1, a2, fail

    # Remap VA 0x3000 -> page_b. Model property (M4, no translation
    # cache): the new leaf is already visible before any fence.
    sd      s10, 0(s2)
    ld      t1, 0(s4)
    bne     t1, a3, fail

    # Form 1: rs1 = x0, rs2 = x0 (global).
    sfence.vma
    ld      t1, 0(s4)
    bne     t1, a3, fail

    # Form 2: rs1 = x0, rs2 != x0 (one ASID). ASID 0 is the only ASID.
    sd      s9, 0(s2)
    li      t2, 0
    sfence.vma x0, t2
    ld      t1, 0(s4)
    bne     t1, a2, fail

    # Form 3: rs1 != x0, rs2 = x0 (one VA, every ASID).
    sd      s10, 0(s2)
    sfence.vma s4, x0
    ld      t1, 0(s4)
    bne     t1, a3, fail

    # Form 4: rs1 != x0, rs2 != x0 (one VA in one ASID), then a store
    # through the new mapping that lands in page_a, not page_b.
    sd      s9, 0(s2)
    sfence.vma s4, t2
    ld      t1, 0(s4)
    bne     t1, a2, fail
    li      t3, 0x5a5a5a5a12345678
    sd      t3, 8(s4)
    la      t0, page_a
    ld      t1, 8(t0)
    bne     t1, t3, fail
    la      t0, page_b
    ld      t1, 8(t0)
    bnez    t1, fail

    # A previously invalid leaf becomes valid: VA 0x4000 -> page_c, fenced
    # by VA, then loaded.
    li      s6, 0x4000
    sd      s11, 0(s3)
    sfence.vma s6, x0
    ld      t1, 0(s6)
    bne     t1, a4, fail

    # A non-canonical VA in rs1 raises no exception.
    li      t0, 1
    slli    t0, t0, 63
    sfence.vma t0, x0
    sfence.vma t0, t2

    # A SYSTEM funct3 = 0 encoding that is not SFENCE.VMA: HFENCE.VVMA
    # (funct7 0010001). H is absent, so it is illegal-instruction.
    li      s5, 2
    li      s6, 0x22000073
    la      s7, after_hfence
    li      s8, 0x800                   # resume in S
    .word   0x22000073                  # hfence.vvma x0, x0
    j       fail
after_hfence:
    li      t0, -1
    bne     s5, t0, fail                # the handler ran and consumed it

    # Return to U through SRET: SPP = U, sepc = VA 0x2000.
    li      t0, 0x100
    csrc    sstatus, t0
    li      t0, 0x2000
    csrw    sepc, t0
    li      s5, 2
    li      s6, 0x12000073              # sfence.vma x0, x0
    la      s7, done
    li      s8, 0x1800                  # resume in M
    sret
    j       fail

done:
    # Back in M after the U-mode SFENCE.VMA trap. The trap came from U.
    srli    t1, a5, 11
    andi    t1, t1, 3
    bnez    t1, fail
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

# (mcause << 16 | mtval) << 8 | 0x7f: the low byte is never 0, so the CLI
# process status also fails when mtval's low byte is 0 (ECALL).
mismatch:
    slli    a0, t0, 16
    or      a0, a0, t1
    slli    a0, a0, 8
    ori     a0, a0, 0x7f
    j       write_exit

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    sd      a0, 0(s1)
hang:
    j       hang

# Expected mcause in s5, mtval in s6, recovery address in s7, and the
# mstatus value for the return in s8. The trap-entry mstatus is kept in a5.
trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    li      t4, -1
    beq     s5, t4, unexpected
    bne     t0, s5, mismatch
    li      t5, ANY
    beq     s6, t5, mtval_ok
    bne     t1, s6, mismatch
mtval_ok:
    csrr    a5, mstatus
    li      s5, -1
    csrw    mepc, s7
    csrw    mstatus, s8
    mret

    .align 12
root_table:
    .zero 4096
    .align 12
mid_table:
    .zero 4096
    .align 12
leaf_table:
    .zero 4096
    .align 12
page_a:
    .zero 4096
    .align 12
page_b:
    .zero 4096
    .align 12
page_c:
    .zero 4096

    .align 12
u_page:
    # U mode: SFENCE.VMA is illegal here.
    sfence.vma
    j       fail_u
fail_u:
    # U cannot reach tohost (the superpage has U=0); spin and time out.
    j       fail_u

.section .tohost
.align 3
tohost:
    .dword 0
