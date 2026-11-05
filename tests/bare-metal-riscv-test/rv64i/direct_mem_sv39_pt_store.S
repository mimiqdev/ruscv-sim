# TLM direct memory U3 (direct-memory D11): a direct store into a page table.
#
# Supervisor mode runs on an identity 1 GiB superpage (U=0), so it can rewrite
# the leaf PTE that maps VA 0x3000 with an ordinary store.  The guest first
# loads through the mapping (it must return page_a), stores a PTE that points
# at page_b, executes SFENCE.VMA, and loads the same VA again.  The second load
# must return page_b: its walk has to see the PTE the guest just stored, and
# that store went through the direct path (see tests/direct_mem_u3_guests.rs).
#
# The last check reads the leaf PTE back, so a store that landed somewhere
# else, or a walk that read a stale view, fails instead of passing silently.
# Every PTE carries A and D, so no walk takes a step-9 atomic update: the U3
# fixture keeps its accesses non-atomic on purpose.

    .section .text
    .globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0

    # root[0] -> mid table, mid[0] -> leaf table (non-leaf pointers).
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

    # root[2]: identity 1 GiB superpage at PA 0x80000000, R|W|X|A|D, U=0. It
    # covers the boot code, the handler, the S-mode program, the page tables,
    # page_a, page_b and tohost.
    la      t0, root_table
    li      t1, 0x80000
    slli    t1, t1, 10
    ori     t1, t1, 0xcf
    sd      t1, 16(t0)

    # leaf[3]: VA 0x3000 -> page_a, R|W|A|D, U=0.
    la      t0, leaf_table
    la      t1, page_a
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 0xc7
    sd      t1, 24(t0)

    # The two candidate pages hold distinct words, so the load tells them
    # apart even if both are mapped at some point.
    la      t0, page_a
    li      t1, 0xaaaa0000aaaa0001
    sd      t1, 0(t0)
    la      t0, page_b
    li      t1, 0xbbbb0000bbbb0002
    sd      t1, 0(t0)

    # Sv39 on, enter S at pt_main on the identity superpage.
    la      t0, root_table
    srli    t0, t0, 12
    li      t1, 8
    slli    t1, t1, 60
    or      t0, t0, t1
    csrw    satp, t0
    la      t0, pt_main
    csrw    mepc, t0
    li      t0, 0x800                   # MPP = S, MPRV = 0
    csrw    mstatus, t0
    mret

# s2 = &leaf[3], s3 = VA 0x3000, s10 = the new leaf PTE.
pt_main:
    la      s2, leaf_table
    addi    s2, s2, 24
    li      s3, 0x3000
    la      t0, page_b
    srli    t0, t0, 12
    slli    t0, t0, 10
    ori     s10, t0, 0xc7

    # Before the rewrite the walk must report page_a.
    ld      a1, 0(s3)
    li      a2, 0xaaaa0000aaaa0001
    bne     a1, a2, pt_fail

    # The direct store into the page table, then the required ordering.
    sd      s10, 0(s2)
    sfence.vma

    # The next walk must see the stored PTE: only page_b holds this word.
    ld      a1, 0(s3)
    li      a2, 0xbbbb0000bbbb0002
    bne     a1, a2, pt_fail

    # The leaf really holds the new PTE.
    ld      a1, 0(s2)
    bne     a1, s10, pt_fail

    li      a0, 0
    j       pt_exit

pt_fail:
    li      a0, 1
pt_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t2, tohost
    sd      a0, 0(t2)
pt_hang:
    j       pt_hang

# Any trap (a page fault from a stale mapping, an access fault, an illegal
# instruction) is a failure.
trap_handler:
    li      a0, 1
    j       pt_exit

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

    .section .tohost
    .align 3
    .globl tohost
tohost:
    .dword  0
