# Stage 4 walk-fault guest (dev-plan §4 "Walk fault", D3).
#
# Machine mode builds an Sv39 table and returns to Supervisor mode. The
# supervisor code provokes each walk fault in the contract's list. The
# M-mode handler checks the exact cause and the original virtual address
# in mtval, then resumes at the next case. Exit code 0 means every fault
# matched. A case that does not trap falls through to prog_fail.
#
# Cases, all in Supervisor mode:
#   1. Non-canonical VA on a load        -> load page fault (13)
#   2. Non-canonical VA on a fetch       -> instruction page fault (12)
#   3. Invalid PTE (V = 0)               -> load page fault (13)
#   4. Reserved PTE bit 63 set           -> load page fault (13)
#   5. Non-leaf PTE with the U bit set   -> load page fault (13)
#   6. Misaligned 1 GiB superpage        -> load page fault (13)
#   7. Walk read the target rejects      -> load access fault (5)
#
# A host, protocol, or unknown walk completion is a SimulatorFailure. No
# guest can raise one, so the driver suite covers that row.
#
# The image is one contiguous .text region (see linker-s4.ld). The
# translated program page runs at VA 0x1000, so every address it needs
# arrives in a register that mret preserves. A fetch fault's mepc is the
# unmapped VA, so the handler resumes at the recovery address in s7.

.section .text
.globl _start

_start:
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

    # leaf[vpn0=1]: VA 0x1000 -> program page, R|X|A. The fetch of the
    # translated page must not write this PTE back.
    la      t0, leaf_table
    la      t3, s4_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x4b                # V | R | X | A
    sd      t3, 8(t0)

    # leaf[vpn0=2]: VA 0x2000 -> invalid PTE (V = 0). Case 3.
    sd      x0, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> reserved bit 63 set. Case 4.
    la      t3, data_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x07                # V | R | W
    li      t4, 1
    slli    t4, t4, 63
    or      t3, t3, t4
    sd      t3, 24(t0)

    # mid[vpn1=1]: VA 0x00200000 -> non-leaf pointer with U set. Case 5.
    la      t0, mid_table
    la      t3, leaf_table
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x11                # V | U
    sd      t3, 8(t0)

    # root[vpn2=1]: VA 0x40000000 -> misaligned 1 GiB superpage. Case 6.
    # PPN 0x80001 is not a multiple of 2^18.
    la      t0, root_table
    li      t3, 0x80001
    slli    t3, t3, 10
    ori     t3, t3, 0x07                # V | R | W
    sd      t3, 8(t0)

    # root[vpn2=2]: identity 1 GiB superpage, R|W|X (boot, handler, tohost).
    li      t3, 0x80000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f
    sd      t3, 16(t0)

    # root[vpn2=3]: VA 0xc0000000 -> next-level table at PPN 0. Case 7.
    # PPN 0 is below the RAM base, so the target rejects that walk read.
    li      t3, 1                       # V only, PPN 0
    sd      t3, 24(t0)

    # Addresses the translated page cannot recompute with la.
    li      s0, 0x2000                  # invalid PTE
    li      s2, 0x3000                  # reserved bit
    li      s3, 0x200000                # non-leaf with U
    li      s4, 0x40000000              # misaligned superpage
    li      s8, 0xc0000000              # rejected walk read
    la      s1, tohost
    la      s9, case2_resume
    la      t0, case3_arm
    la      t1, s4_page
    sub     s10, t0, t1                 # case3_arm offset inside VA 0x1000

    # Enter Supervisor with Sv39 on.
    la      t0, trap_handler
    csrw    mtvec, t0
    la      t0, s_trampoline
    csrw    mepc, t0
    li      t0, 0x800                   # MPP = S
    csrw    mstatus, t0
    la      t0, root_table
    srli    t0, t0, 12
    li      t1, 8
    slli    t1, t1, 60
    or      t0, t0, t1
    csrw    satp, t0
    mret

s_trampoline:
    li      t0, 0x1000
    jr      t0

# Case 2 resumes here, on the identity superpage. The fetch fault's mepc
# is the non-canonical VA, so the handler cannot add 4 to it.
case2_resume:
    li      t0, 0x1000
    add     t0, t0, s10
    jr      t0

# Expected cause and mtval live in s5 and s6. s7 holds the recovery
# address. A mismatch writes the observed cause as the exit code.
trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    bne     t0, s5, trap_mismatch
    bne     t1, s6, trap_mismatch
    csrw    mepc, s7
    li      t4, 0x800
    csrw    mstatus, t4
    mret
trap_mismatch:
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

    .align 12
    .globl s4_page
s4_page:
    # Case 1: non-canonical load. Bit 38 set, bits 63:39 clear.
    li      s5, 13
    li      t0, 1
    slli    s6, t0, 38
    la      s7, case1_resume
    ld      a1, 0(s6)
    j       prog_fail
case1_resume:
    # Case 2: non-canonical fetch at the same address. The recovery
    # address was computed in M mode (s9).
    li      s5, 12
    li      t0, 1
    slli    s6, t0, 38
    mv      s7, s9
    jr      s6
    j       prog_fail
case3_arm:
    # Case 3: invalid PTE at VA 0x2000.
    li      s5, 13
    li      s6, 0x2000
    la      s7, case3_resume
    ld      a1, 0(s0)
    j       prog_fail
case3_resume:
    # Case 4: reserved bit 63 at VA 0x3000.
    li      s5, 13
    li      s6, 0x3000
    la      s7, case4_resume
    ld      a1, 0(s2)
    j       prog_fail
case4_resume:
    # Case 5: non-leaf PTE with U set, VA 0x200000.
    li      s5, 13
    li      s6, 0x200000
    la      s7, case5_resume
    ld      a1, 0(s3)
    j       prog_fail
case5_resume:
    # Case 6: misaligned 1 GiB superpage, VA 0x40000000.
    li      s5, 13
    li      s6, 0x40000000
    la      s7, case6_resume
    ld      a1, 0(s4)
    j       prog_fail
case6_resume:
    # Case 7: the next-level table sits at PPN 0, below RAM. The target
    # rejects that walk read, so the fault is an access fault and mtval
    # still holds the original virtual address.
    li      s5, 5
    li      s6, 0xc0000000
    la      s7, case7_resume
    ld      a1, 0(s8)
    j       prog_fail
case7_resume:
    li      a0, 0
prog_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    sd      a0, 0(s1)
prog_hang:
    j       prog_hang

prog_fail:
    li      a0, 1
    j       prog_exit

.section .tohost
.align 3
tohost:
    .dword 0
