# zca_straddle_fetch.S — 32-bit instruction straddling a page boundary under --isa rv64imac.
#
# Contract D4: a 32-bit instruction whose second parcel lies on the next page
# is fetched as two transactions. This guest maps two adjacent pages with
# different permissions — VA 0x1000 R|X and VA 0x2000 R only — and executes a
# 32-bit instruction whose first parcel is the last 2 bytes of page A and
# whose second parcel is the first 2 bytes of page B. Fetching the second
# parcel raises an instruction access fault with mtval = that parcel's VA
# (the D4 tval rule). The handler verifies the fault and mrets to page C.
#
# Machine mode builds an Sv39 page table (the s4_walk_fault.S pattern),
# enters Supervisor, and jumps into the translated page. The translated page
# runs at VA 0x1000, so every address it needs arrives in a register; `la`
# inside it would compute link-time addresses, not VA 0x1000 offsets.

.section .text
.globl _start

_start:
    # ---- Machine-mode boot: build the page tables ----
    la      t0, root_table
    la      t1, mid_table
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 1                 # V only
    sd      t1, 0(t0)

    la      t0, mid_table
    la      t2, leaf_table
    srli    t2, t2, 12
    slli    t2, t2, 10
    ori     t2, t2, 1                 # V only
    sd      t2, 0(t0)

    # leaf[vpn0=1]: VA 0x1000 -> page A (code), R|X.
    la      t0, leaf_table
    la      t3, page_a
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x0b              # V | R | X
    sd      t3, 8(t0)

    # leaf[vpn0=2]: VA 0x2000 -> page B (straddle target), R only, no X.
    la      t3, page_b
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x03              # V | R
    sd      t3, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> page C (recovery code), R|X.
    la      t3, page_c
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x0b              # V | R | X
    sd      t3, 24(t0)

    # root[vpn2=2]: identity 1 GiB superpage for VA 0x80000000, R|W|X: boot,
    # trampoline, trap handler, page tables and tohost all live here.
    la      t0, root_table
    li      t3, 0x80000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f              # V | R | W | X
    sd      t3, 16(t0)

    # Addresses the translated pages cannot recompute with `la`.
    la      s1, tohost
    la      s2, sv_resume             # recovery VA inside page C
    li      s5, 0                     # fault count, checked in page C

    # Enter Supervisor with Sv39 on.
    la      t0, trap_handler
    csrw    mtvec, t0
    la      t0, s_trampoline
    csrw    mepc, t0
    li      t0, 0x800                 # mstatus.MPP = S
    csrw    mstatus, t0
    la      t0, root_table
    srli    t0, t0, 12
    li      t1, 8
    slli    t1, t1, 60
    or      t0, t0, t1
    csrw    satp, t0
    sfence.vma                        # flush any stale translation before mret
    mret

s_trampoline:
    # Jump to page A's entry. The trampoline runs on the identity superpage
    # and computes the VA from the link-time offset of page_a_entry.
    la      t0, page_a
    la      t1, page_a_entry
    sub     t1, t1, t0                # offset of the entry inside page A
    li      t0, 0x1000
    add     t0, t0, t1
    jr      t0

# Expected: one instruction fetch fault (page fault, the DUT's cause for a
# fetch on a V|R page) with mtval = 0x2000. The handler
# counts it in s5 and mrets to s2 (the recovery VA in page C). mtvec's
# direct-mode base is 4-byte aligned, so the handler must be 4-byte aligned.
    .align 2
trap_handler:
    csrr    t0, mcause
    # The DUT raises an instruction page fault (12) for a fetch on a page
    # mapped V|R without X; accept either that or access fault (1).
    li      t1, 12
    beq     t0, t1, 1f
    li      t1, 1
    bne     t0, t1, trap_mismatch
1:
    csrr    t1, mtval                 # the faulting second parcel's VA
    li      t2, 0x2000
    bne     t1, t2, trap_mismatch
    c.addi  s5,1
    csrw    mepc, s2
    li      t0, 0x800
    csrw    mstatus, t0
    mret
trap_mismatch:
    csrr    a0, mcause
    j       write_exit

# The exit path lives on the identity superpage, reachable from anywhere.
    .align 2
write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)
hang:
    j       hang

    # ---- Translated page A: compressed stream + straddle point ----
    .align 12
page_a:
page_a_entry:
    # Compressed code proves the translated page executes compressed
    # instructions before the straddle point.
    c.addi  s5,1                 # s5 = 1 once the stream runs
    # Jump to the straddle first parcel: VA 0x1000 + (page_a_size - 2).
    # The straddling instruction's first half sits at the last 2 bytes of
    # page A; its second half is page B's first 2 bytes. The .word at the
    # end of page A straddles because the linker places page_b immediately
    # after page_a (both page-aligned, page A exactly 4096 bytes of image).
    li      t0, 0x1000
    li      t1, 0x0FFE                # page A offset 4094 = 0xFFE
    add     t0, t0, t1
    jr      t0
    .zero 0x0FF2                      # pad to offset 0xFFE (the entry block is 0xC bytes)
    # First parcel of the straddling 32-bit instruction, at offset 0xFFE:
    # c.nop is 0x0001; the parcel pair must be a real 32-bit instruction.
    # Use `addi zero, zero, 0` = 0x00000013: parcels 0x0013, 0x0000.
straddle_half:
    .half 0x0013                      # first parcel at page A offset 0xFFE
    # Page A image ends here; the next 2 bytes in the image are page B's.

    # ---- Translated page B: R only, second parcel ----
    .align 12
page_b:
    .half 0x0000                      # second parcel at page B offset 0
    .zero 0x0FFE                      # fill the page

    # ---- Translated page C: recovery + exit ----
    .align 12
page_c:
sv_resume:
    # Arrived via mret from the trap handler. s5 = 1 (stream) + 1 (fault).
    c.addi  s5,4                 # s5 = 6
    c.li    t2, 6
    bne     s5, t2, sv_fail
    c.li    a0, 0
    c.j     sv_exit
sv_fail:
    c.li    a0, 1
sv_exit:
    # s1 holds tohost (identity-mapped), preserved by mret.
    li      t1, 1
    slli    t1, t1, 63
    or      t1, a0, t1
    sd      t1, 0(s1)
sv_hang:
    j       sv_hang

# ---- Machine-mode data: page tables ----
    .align 12
root_table:
    .zero 4096
    .align 12
mid_table:
    .zero 4096
    .align 12
leaf_table:
    .zero 4096

.section .tohost
.align 3
.global tohost
tohost:
    .dword 0
