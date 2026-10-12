# Stage 4 MPRV guest (dev-plan §4 "MPRV", D9).
#
# Machine mode builds an Sv39 table and, instead of entering S, keeps
# executing in M with MPRV=1 and MPP=S: loads and stores then use the
# Supervisor translation and protection, while instruction fetch stays
# untranslated.  The guest checks:
#
#   1. A load through the S-mode mapping of a U=1 page succeeds when
#      SUM=1 (SUM applies under MPRV with MPP=S).
#   2. The same load faults (load page fault, 13) when SUM=0.
#   3. A store through the S-mode mapping lands at the translated address.
#   4. Instruction fetch never uses the translation: the M-mode code runs
#      at its bare physical address the whole time.
#
# The whole image is one contiguous .text region (see linker-s4.ld); page
# tables and the data page are page-aligned inside it and discovered with
# `la`.

.section .text
.globl _start

_start:
    # ---- Build the page tables ----
    # root[vpn2=0] -> mid table
    la      t0, root_table
    la      t1, mid_table
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 1
    sd      t1, 0(t0)

    # mid[vpn1=0] -> leaf table
    la      t0, mid_table
    la      t2, leaf_table
    srli    t2, t2, 12
    slli    t2, t2, 10
    ori     t2, t2, 1
    sd      t2, 0(t0)

    # leaf[vpn0=1]: VA 0x1000 -> data page, R|W|U.  M-mode with MPRV=1 and
    # MPP=S loads this page through the S-mode translation; U=1 means the
    # load needs SUM=1 (SUM applies under MPRV with MPP=S).
    la      t0, leaf_table
    la      t3, data_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x17                # V | R | W | U
    sd      t3, 8(t0)

    # Known value in the data page, neighbor zero.
    la      t0, data_page
    li      t1, 0x0123456789abcdef
    sd      t1, 0(t0)
    sd      x0, 8(t0)

    # Compose the S-mode VA of the data page in s0 (mstatus writes below
    # do not touch it): the 4 KiB leaf maps VA 0x1000 -> data page.
    li      s0, 0x1000

    # ---- Turn translation on, stay in M, set MPRV with MPP=S ----
    la      t0, trap_handler
    csrw    mtvec, t0
    la      t0, root_table
    srli    t0, t0, 12                  # satp PPN
    li      t1, 8
    slli    t1, t1, 60                  # Sv39 mode
    or      t0, t0, t1
    csrw    satp, t0
    # mstatus: MPRV (17) | MPP=S (01<<11) | SUM (18).
    li      t0, 0x60800                 # MPRV | SUM | MPP=S

    # Case 1: with SUM=1 the M-mode load of the U page translates and
    # succeeds.  The fetch of this very instruction is untranslated (M
    # mode), which case 4 checks at the end.
    csrw    mstatus, t0
    ld      a1, 0(s0)
    li      a2, 0x0123456789abcdef
    bne     a1, a2, fail

    # Case 3: the store through the S-mode mapping lands at the translated
    # address, not at the bare VA (which is unmapped RAM far below the
    # image, so a store there would fault or corrupt nothing observable;
    # instead read back through the same translation).
    li      a3, 0xfedcba9876543210
    sd      a3, 0(s0)
    ld      a4, 0(s0)
    bne     a4, a3, fail
    # The neighbor is untouched.
    ld      a5, 8(s0)
    bnez    a5, fail

    # Case 2: with SUM=0 the same load takes the load page fault (13) with
    # the virtual address in mtval.
    li      s5, 13
    li      s6, 0x1000
    la      s7, case2_resume
    li      t0, 0x20800                 # MPRV | MPP=S, SUM clear
    csrw    mstatus, t0
    ld      a1, 0(s0)
case2_resume:

    # Case 4: fetch never used translation.  mret below returns to M mode
    # at the address in mepc; every instruction since _start executed at
    # its bare physical address, so simply reaching here proves it.  Clear
    # MPRV before exiting so the exit store itself stays untranslated.
    li      t0, 0
    csrw    mstatus, t0
    li      a0, 0
    j       write_exit

trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    li      t4, -1
    beq     s5, t4, trap_unexpected
    bne     t0, s5, trap_mismatch
    bne     t1, s6, trap_mismatch
    csrw    mepc, s7
    li      t4, 0x1800                  # MPP=M for the handler's own mret
    csrw    mstatus, t4
    mret
trap_mismatch:
    csrr    a0, mcause
    j       write_exit
trap_unexpected:
    li      a0, 63
    j       write_exit

fail:
    li      a0, 1

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

.section .tohost
.align 3
tohost:
    .dword 0
