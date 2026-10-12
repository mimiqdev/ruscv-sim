# Stage 4 permission-fault guest (dev-plan §4 "Permission fault").
#
# Machine mode builds an Sv39 table with deliberately wrong permissions and
# returns to Supervisor mode.  The supervisor code provokes each fault in
# the contract's list; the M-mode handler checks the exact cause and the
# original virtual address in mtval, then resumes at the next case.  A
# final exit code 0 means every fault matched the contract exactly.
#
# Cases (all in Supervisor mode):
#   1. Store to a read-only PTE        -> store page fault (15), mtval VA
#   2. Fetch from a non-X PTE          -> instr page fault (12), mtval VA
#   3. Load of U=1 page with SUM=0     -> load page fault (13), mtval VA
#   4. Fetch of a U=1 page             -> instruction page fault (12), mtval VA
#   5. Load of X-only page, MXR=0      -> load page fault (13), mtval VA
#   6. Same load with MXR=1            -> retires, correct value
#
# The whole image is one contiguous .text region (see linker-s4.ld).  The
# translated program page runs at VA 0x1000, so `la` inside it would
# compute auipc-relative garbage; every address it needs arrives in a
# register (s0..s3) that mret preserves.

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

    # leaf[vpn0=1]: VA 0x1000 -> program page, R|X
    la      t0, leaf_table
    la      t3, s4_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x0b                # V | R | X
    sd      t3, 8(t0)

    # leaf[vpn0=2]: VA 0x2000 -> data page, R only: case 1's store faults.
    la      t3, data_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x03                # V | R
    sd      t3, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> read-only code page, R only: case 2's
    # fetch faults.
    la      t3, ro_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x03                # V | R
    sd      t3, 24(t0)

    # leaf[vpn0=4]: VA 0x4000 -> data page, R|W|U: case 3's S load faults
    # (SUM=0).
    la      t3, data_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x17                # V | R | W | U
    sd      t3, 32(t0)

    # leaf[vpn0=6]: VA 0x6000 -> U page, R|X|U: case 4's S fetch faults
    # regardless of SUM.  X is set, so only the U=1 rule can fault it.
    la      t3, ufetch_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x1b                # V | R | X | U
    sd      t3, 48(t0)

    # leaf[vpn0=5]: VA 0x5000 -> execute-only page, X only: cases 4 and 5.
    la      t3, xonly_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x09                # V | X
    sd      t3, 40(t0)

    # root[vpn2=2]: identity 1 GiB superpage, R|W|X (boot, handler, tohost).
    la      t0, root_table
    li      t3, 0x80000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f
    sd      t3, 16(t0)

    # The execute-only page holds a known word for the MXR load.
    la      t0, xonly_page
    li      t1, 0x13579bdf02468ace
    sd      t1, 0(t0)
    # The read-only data page holds a readable value.
    la      t0, data_page
    li      t1, 0x2468ace13579bdf0
    sd      t1, 0(t0)

    #   s0 = VA 0x2000 (read-only data page)
    #   s1 = tohost address (identity mapped)
    #   s2 = VA 0x5000 (execute-only page)
    #   s3 = identity-mapped resume after the U-page fetch fault
    li      s0, 0x2000
    la      s1, tohost
    li      s2, 0x5000
    la      s3, ufetch_resume
    la      t0, case5_arm
    la      t1, s4_page
    sub     s4, t0, t1              # case5_arm offset inside the VA 0x1000 page

    # ---- Enter Supervisor with Sv39 on ----
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

# The M-mode trap handler: check cause and mtval against the expected pair
# for this case, then resume after the faulting instruction.  A mismatch
# writes the observed cause as the exit code and hangs.
trap_handler:
    csrr    t0, mcause
    csrr    t1, mtval
    # Expected values are kept by the program in s5 (cause) and s6 (mtval)
    # when it arms a case, or s5 = -1 when the instruction must retire.
    li      t4, -1
    beq     s5, t4, trap_unexpected
    bne     t0, s5, trap_mismatch
    bne     t1, s6, trap_mismatch
    # Resume at the case's recovery address in s7: a fetch fault's mepc
    # is the unmapped VA, so +4 would loop on the faulting page.
    csrw    mepc, s7
    li      t4, 0x800
    csrw    mstatus, t4
    mret
trap_mismatch:
    csrr    a0, mcause
    j       write_exit
trap_unexpected:
    # An unexpected trap while a case expected retirement.
    li      a0, 63
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
    .align 12
ro_page:
    # Never executed; only fetched by case 2's jump.  R-only mapping.
    nop
    nop
    .zero 4088
    .align 12
xonly_page:
    .zero 4096
    .align 12
ufetch_page:
    # Never executed.  R|X|U, so only the S-mode U=1 fetch rule faults it.
    nop
    .zero 4092

# Case 4 resumes here, on the identity superpage, then returns to the
# translated page for the MXR cases.
ufetch_resume:
    li      t0, 0x1000
    add     t0, t0, s4
    jr      t0

# ---- The translated program page (VA 0x1000) ----
    .align 12
    .globl s4_page
s4_page:
    # Case 1: store to the read-only page at VA 0x2000 (s0).
    li      s5, 15                      # expect store page fault
    li      s6, 0x2000                  # expect mtval VA 0x2000
    la      s7, case1_resume
    li      a1, 0x77
    sd      a1, 0(s0)
    j       prog_fail
case1_resume:
    # The handler resumed here.  The stored byte must not have landed.
    ld      a2, 0(s0)
    li      a3, 0x2468ace13579bdf0
    bne     a2, a3, prog_fail

    # Case 2: fetch from the non-executable page at VA 0x3000.
    li      s5, 12                      # expect instruction page fault
    li      s6, 0x3000                  # expect mtval VA 0x3000
    la      s7, case2_resume
    li      t0, 0x3000
    jr      t0
    j       prog_fail
case2_resume:
    # Case 3: S-mode load of the U=1 page at VA 0x4000 with SUM=0.
    li      s5, 13                      # expect load page fault
    li      s6, 0x4000                  # expect mtval VA 0x4000
    la      s7, case3_resume
    li      t0, 0x4000
    ld      a1, 0(t0)
    j       prog_fail
case3_resume:
    # Case 4: S-mode fetch of the U=1 page at VA 0x6000, with SUM set.
    # D9 faults this fetch regardless of SUM, so the case has to run with
    # SUM=1: under SUM=0 the ordinary user-page rule would fault it too,
    # and the test could not tell the two apart.  The recovery address
    # was computed in M mode (s3).
    li      t0, 0x40000                 # SUM (bit 18)
    csrs    sstatus, t0
    li      s5, 12
    li      s6, 0x6000
    mv      s7, s3
    li      t0, 0x6000
    jr      t0
    j       prog_fail
case5_arm:
    # Case 5: load of the execute-only page at VA 0x5000 (s2) with MXR=0.
    li      s5, 13
    li      s6, 0x5000
    la      s7, case5_resume
    ld      a1, 0(s2)
    j       prog_fail
case5_resume:
    # Case 6: the same load with MXR=1 retires with the known value.
    li      s5, -1                      # must retire, no trap
    li      t0, 0x80000                 # MXR (bit 19)
    csrs    sstatus, t0
    ld      a1, 0(s2)
    li      a2, 0x13579bdf02468ace
    bne     a1, a2, prog_fail

    # All cases matched.  Exit 0.
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
