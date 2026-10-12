# Stage 4 delegation guest (dev-plan §4 "Delegation", M1).
#
# medeleg and mideleg are read-only zero: writing ones reads back zero.
# A page fault taken afterwards is taken in M, with mtval holding the
# original virtual address.  The fault is raised through Sv39: an S-mode
# load of an unmapped canonical address.

    .equ CAUSE_ILLEGAL, 2
    .equ ANY, -2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1

    # medeleg and mideleg ignore writes and read 0.
    li      t0, -1
    csrw    medeleg, t0
    csrr    t1, medeleg
    bnez    t1, fail
    csrw    mideleg, t0
    csrr    t1, mideleg
    bnez    t1, fail

    # Build a minimal Sv39 table: root[0] is a pointer-only entry, so a
    # load at VA 0x1000 walks to a missing leaf and page-faults.
    la      t0, root_table
    la      t1, mid_table
    srli    t1, t1, 12
    slli    t1, t1, 10
    ori     t1, t1, 1                   # V only
    sd      t1, 0(t0)

    # Identity 1 GiB superpage for the boot image so the handler and the
    # S-mode trampoline fetch legally.
    la      t0, root_table
    li      t1, 0x80000                 # PPN of 0x80000000
    slli    t1, t1, 10
    ori     t1, t1, 0x0f                # V | R | W | X
    sd      t1, 16(t0)

    # Enter S with Sv39 on and take the load page fault.
    la      t0, root_table
    srli    t0, t0, 12
    li      t1, 8
    slli    t1, t1, 60
    or      t0, t0, t1
    csrw    satp, t0
    la      t0, s_fault
    csrw    mepc, t0
    li      t0, 0x880                   # MPIE | MPP = S
    csrw    mstatus, t0
    li      s5, 13                      # load page fault
    li      s6, 0x1000                  # mtval = the original VA
    la      s7, done_ok
    mret
s_fault:
    li      t0, 0x1000
    ld      t1, 0(t0)                   # unmapped: load page fault
    j       fail
done_ok:
    # The fault was taken in M: the handler ran at mtvec (M) and the read
    # back of medeleg is still 0.
    csrr    t1, medeleg
    bnez    t1, fail
    li      a0, 0
    j       write_exit

fail:
    li      a0, 1
    j       write_exit

unexpected:
    li      a0, 63
    j       write_exit

mismatch:
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
    li      s5, -1
    csrw    mepc, s7
    li      t4, 0x1800                  # MPP = M: the check runs in M
    csrw    mstatus, t4
    mret

    .align 12
root_table:
    .zero 4096
    .align 12
mid_table:
    .zero 4096

.section .tohost
.align 3
tohost:
    .dword 0
