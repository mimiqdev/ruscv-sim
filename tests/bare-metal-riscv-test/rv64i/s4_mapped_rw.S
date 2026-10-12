# Stage 4 mapped read/write guest (dev-plan §4 "Mapped read and write").
#
# Machine mode builds an Sv39 page table in RAM, turns translation on for
# Supervisor mode, and returns through mret into the identity-mapped boot
# code, which jumps into the translated program page at VA 0x1000.  The
# supervisor code loads and stores through a translated VA (a 4 KiB page
# and a 1 GiB superpage), checks the bytes and the untouched neighbor,
# checks the A/D bits the hardware set in the leaf PTEs, executes all four
# SFENCE.VMA forms, and reports pass/fail through tohost.
#
# The whole image is one contiguous .text region: the page tables, the
# translated pages, and tohost are page-aligned data and code inside it,
# discovered with `la` (auipc-relative, so no lui sign-extension traps).
# The S-mode program receives the composed superpage data VA in s0, which
# mret preserves, because S mode cannot recompute the physical offset.

.section .text
.globl _start

_start:
    # ---- Machine-mode boot: build the page tables ----
    # root[vpn2=0] -> mid table (pointer, V only)
    la      t0, root_table
    la      t1, mid_table
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 1
    sd      t1, 0(t0)

    # mid[vpn1=0] -> leaf table (pointer, V only)
    la      t0, mid_table
    la      t2, leaf_table
    srli    t2, t2, 12
    slli    t2, t2, 10
    ori     t2, t2, 1
    sd      t2, 0(t0)

    # leaf[vpn0=1]: VA 0x1000 -> program page, R|X
    la      t0, leaf_table
    la      t3, s4_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x0b                # V | R | X
    sd      t3, 8(t0)

    # leaf[vpn0=2]: VA 0x2000 -> leaf table page, R|W (self-inspection)
    la      t3, leaf_table
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x07                # V | R | W
    sd      t3, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> root table page, R|W (self-inspection)
    la      t3, root_table
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x07
    sd      t3, 24(t0)

    # root[vpn2=1]: 1 GiB superpage, VA 0x40000000 -> PA 0x80000000, R|W.
    la      t0, root_table
    li      t3, 0x80000                 # 1 GiB-aligned PPN of 0x80000000
    slli    t3, t3, 10
    ori     t3, t3, 0x07                # V | R | W
    sd      t3, 8(t0)

    # root[vpn2=2]: identity 1 GiB superpage for VA 0x80000000, R|W|X:
    # the boot trampoline, the trap handler, and tohost all live here, and
    # the S-mode fetch of the trampoline needs X on this mapping.
    li      t3, 0x80000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f
    sd      t3, 16(t0)

    # Data value at the translated data page, neighbor zero.
    la      t0, data_page
    li      t1, 0x1234567890abcdef
    sd      t1, 0(t0)
    sd      x0, 8(t0)

    # Compose the S-mode superpage VA for the data page into s0: the 1 GiB
    # leaf maps VA 0x40000000 + (PA - 0x80000000).  mret preserves s0.
    la      t0, data_page
    li      t1, 0x80000000
    sub     t0, t0, t1
    li      t1, 0x40000000
    add     s0, t0, t1

    # The tohost word sits on the identity superpage, so its S-mode VA is
    # its physical address.  mret preserves s1.  `la` inside the translated
    # page would compute auipc-relative garbage (the page runs at VA 0x1000,
    # not at its link address), so every address the translated page needs
    # arrives in a register.
    la      s1, tohost

    # ---- Enter Supervisor with Sv39 on ----
    la      t0, trap_handler
    csrw    mtvec, t0
    la      t0, s_trampoline
    csrw    mepc, t0
    li      t0, 0x800                   # MPP = S (01 << 11)
    csrw    mstatus, t0
    la      t0, root_table
    srli    t0, t0, 12                  # satp PPN
    li      t1, 8
    slli    t1, t1, 60                  # Sv39 mode
    or      t0, t0, t1
    csrw    satp, t0
    mret

# Runs in S mode through the identity superpage; jumps into the translated
# program page so the 4 KiB instruction mapping is exercised.
s_trampoline:
    li      t0, 0x1000
    jr      t0

trap_handler:
    # Any trap lands here in M mode: report the cause as the exit code.
    csrr    a0, mcause
    j       write_exit

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

# ---- Page-aligned regions inside .text ----
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
data_page:
    .zero 4096

# ---- The translated program page (VA 0x1000) ----
    .align 12
    .globl s4_page
s4_page:
    # Load through the superpage at the composed data VA (s0).
    ld      a1, 0(s0)
    li      a3, 0x1234567890abcdef
    bne     a1, a3, prog_fail

    # Store through the superpage, read back, check the neighbor.
    li      a4, 0xdeadbeefcafef00d
    sd      a4, 0(s0)
    ld      a5, 0(s0)
    bne     a5, a4, prog_fail
    ld      a6, 8(s0)
    bnez    a6, prog_fail

    # The superpage leaf (root[1], seen at VA 0x3008) has A and D set.
    li      a2, 0x3008                  # VA 0x3000 + 8, root[1]
    ld      a3, 0(a2)
    li      a4, 0xc0                    # A | D
    and     a3, a3, a4
    bne     a3, a4, prog_fail

    # The program page leaf (leaf[1], seen at VA 0x2008) has A set.  This
    # very fetch set it.  D must be clear: nothing stores to VA 0x1000.
    li      a2, 0x2008                  # VA 0x2000 + 8, leaf[1]
    ld      a3, 0(a2)
    li      a4, 0x40                    # A
    and     a3, a3, a4
    bne     a3, a4, prog_fail
    andi    a3, a3, 0x80                # D
    bnez    a3, prog_fail

    # ---- SFENCE.VMA: all four forms (D8) ----
    sfence.vma                          # rs1=x0, rs2=x0 (global)
    li      t0, 1
    sfence.vma x0, t0                   # rs1=x0 (one ASID)
    sfence.vma t0, x0                   # rs2=x0 (one VA, all ASIDs)
    li      t1, 2
    sfence.vma t0, t1                   # one VA in one ASID

    # Translation still works after the fences.
    ld      a5, 0(s0)
    li      a4, 0xdeadbeefcafef00d
    bne     a5, a4, prog_fail

    # Pass: write the exit word through the identity superpage.
    li      a0, 0
prog_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    sd      a0, 0(s1)                   # tohost, identity mapped, from s1
prog_hang:
    j       prog_hang

prog_fail:
    li      a0, 1
    j       prog_exit

.section .tohost
.align 3
tohost:
    .dword 0
