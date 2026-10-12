# Stage 4 A/D observation guest (dev-plan §4 "A/D observation", D4).
#
# Machine mode builds an Sv39 table and returns to Supervisor mode. The
# supervisor code checks two cases a guest can raise:
#
#   1. A fresh page is read, then written. A is set after the read. A and
#      D are set after the write. The leaf is read back through a second
#      mapping whose A bit is already set, so the inspection does not
#      change the leaf under test.
#   2. The write-back commits and the following access faults. The leaf is
#      writable and its PPN is below RAM, so the A/D update commits and the
#      explicit load is then rejected. The handler reads the leaf back and
#      checks that A stayed set.
#
# The rejected write-back itself is not a guest case. The public machine
# has no target that serves the leaf's bytes and then refuses the
# conditional store. The driver suite covers that row.
#
# The image is one contiguous .text region (see linker-s4.ld). The
# translated program page runs at VA 0x1000, so every address it needs
# arrives in a register that mret preserves.

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

    # leaf[vpn0=1]: VA 0x1000 -> program page, R|X|A. The fetch must not
    # write this PTE back.
    la      t0, leaf_table
    la      t3, s4_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x4b                # V | R | X | A
    sd      t3, 8(t0)

    # leaf[vpn0=2]: VA 0x2000 -> fresh data page, R|W, A and D clear.
    la      t3, data_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x07                # V | R | W
    sd      t3, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> the leaf table itself, R|A. The program
    # reads the fresh leaf through this mapping. A is already set and the
    # access is a load, so the inspection does not write the table back.
    la      t3, leaf_table
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x43                # V | R | A
    sd      t3, 24(t0)

    # leaf[vpn0=5]: VA 0x5000 -> PPN 1, below RAM, R|W, A and D clear.
    li      t3, 1
    slli    t3, t3, 10
    ori     t3, t3, 0x07                # V | R | W, PPN 1
    sd      t3, 40(t0)

    # root[vpn2=2]: identity 1 GiB superpage, R|W|X (boot, handler, tohost).
    la      t0, root_table
    li      t3, 0x80000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f
    sd      t3, 16(t0)

    la      t0, data_page
    li      t1, 0x1111222233334444
    sd      t1, 0(t0)

    li      s0, 0x2000                  # fresh data page
    li      s2, 0x3010                  # leaf[2] seen through VA 0x3000
    li      s4, 0x5000                  # PPN below RAM
    la      s1, tohost

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
    # The fresh page has A and D clear before the read.
    ld      a1, 0(s2)
    andi    a1, a1, 0xc0
    bnez    a1, prog_fail

    # The read sets A and leaves D clear.
    ld      a1, 0(s0)
    li      a2, 0x1111222233334444
    bne     a1, a2, prog_fail
    ld      a1, 0(s2)
    andi    a3, a1, 0xc0
    li      a4, 0x40
    bne     a3, a4, prog_fail

    # The write sets D as well.
    li      a1, 0x5555666677778888
    sd      a1, 0(s0)
    ld      a1, 0(s2)
    andi    a3, a1, 0xc0
    li      a4, 0xc0
    bne     a3, a4, prog_fail

    # The write-back commits and the explicit load is then rejected,
    # because the translated page sits below RAM. The handler checks that
    # A stayed set in the leaf.
    li      s5, 5
    mv      s6, s4
    la      s7, case2_check
    ld      a1, 0(s4)
    j       prog_fail
case2_check:
    li      a2, 0x3028                  # leaf[5] through VA 0x3000
    ld      a1, 0(a2)
    andi    a3, a1, 0xc0
    li      a4, 0x40                    # A set, D clear: the access was a load
    bne     a3, a4, prog_fail

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
