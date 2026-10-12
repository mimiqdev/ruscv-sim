# Stage 4 SRET and vectored-mtvec guest.
#
# Two interactions between a translated trap and the return path:
#
#   1. A load page fault is taken while mtvec MODE is Vectored. A
#      synchronous exception enters at BASE in both modes, so the slots at
#      BASE + 4*cause are failure paths. The handler checks the cause and
#      the original virtual address, then returns with mret.
#   2. Supervisor mode executes SRET to User mode. SRET clears MPRV, sets
#      SPIE, and returns with bits 1:0 of SEPC clear. User mode then loads
#      through its own mapping, so the return really changed the privilege
#      the translation uses. SRET in User mode is an illegal instruction,
#      and the handler checks that SRET applied none of its effects.
#
# The image is one contiguous .text region (see linker-s4.ld). The
# translated pages run at VA 0x1000 and VA 0x2000, so every address they
# need arrives in a register that mret preserves.

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

    # leaf[vpn0=1]: VA 0x1000 -> supervisor page, R|X|A.
    la      t0, leaf_table
    la      t3, s_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x4b                # V | R | X | A
    sd      t3, 8(t0)

    # leaf[vpn0=2]: VA 0x2000 -> user page, R|X|U|A.
    la      t3, u_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x5b                # V | R | X | U | A
    sd      t3, 16(t0)

    # leaf[vpn0=3]: VA 0x3000 -> data page, R|W|U.
    la      t3, data_page
    srli    t3, t3, 12
    slli    t3, t3, 10
    ori     t3, t3, 0x17                # V | R | W | U
    sd      t3, 24(t0)

    # root[vpn2=2]: identity 1 GiB superpage, R|W|X (boot, handler, tohost).
    la      t0, root_table
    li      t3, 0x80000
    slli    t3, t3, 10
    ori     t3, t3, 0x0f
    sd      t3, 16(t0)

    la      t0, data_page
    li      t1, 0x1111222233334444
    sd      t1, 0(t0)

    li      s0, 0x3000                  # user data page
    li      s2, 0x2000                  # user code page
    la      s1, tohost
    la      s9, s_resume                # case 1 recovery, on the identity map
    la      s11, after_illegal_sret     # illegal-SRET recovery, identity map

    # Vectored mtvec. The handler body is at BASE, and the following slots
    # are failure paths.
    la      t0, trap_handler
    ori     t0, t0, 1
    csrw    mtvec, t0

    la      t0, s_trampoline
    csrw    mepc, t0
    li      t0, 0x20800                 # MPRV | MPP = S
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

# Case 1 resumes here. A page fault's mepc is the faulting VA, so the
# handler returns to this address instead of adding 4.
s_resume:
    li      t0, 0x1000
    add     t0, t0, s10
    jr      t0

# Case 3 resumes here, on the identity superpage. The translated page is
# only 4 KiB, and these checks do not fit in what it leaves. The handler
# saved mstatus at trap entry in s3, so every effect of the first SRET is
# observed through that snapshot: the handler's return would have
# overwritten the live sstatus before this code runs.
after_illegal_sret:
    # The first SRET cleared MPRV (mstatus bit 17) while returning to U;
    # the illegal SRET in U must have changed nothing before trapping, so
    # the snapshot still shows it clear.
    srli    t1, s3, 17
    andi    t1, t1, 1
    bnez    t1, prog_fail

    # The first SRET set SPIE and cleared SIE and SPP in mstatus; the
    # illegal SRET must have left all three alone.
    li      t2, 0x20                    # SPIE set
    andi    t1, s3, 0x122               # SIE | SPP | SPIE
    bne     t1, t2, prog_fail

    # The trap that brought us here came from U, so MPP records U.
    srli    t1, s3, 11
    andi    t1, t1, 3
    bnez    t1, prog_fail

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

# Expected cause and mtval live in s5 and s6. s7 holds the recovery
# address and s8 holds the mstatus value for the return. A mismatch
# writes the observed cause and mtval as the exit code.
#
.align 2
trap_handler:
    j       trap_body
    .rept   16
    j       fail_slot
    .endr

trap_body:
    csrr    t0, mcause
    csrr    t1, mtval
    bne     t0, s5, trap_mismatch
    bne     t1, s6, trap_mismatch
    # Record MPRV before the return overwrites mstatus. The resumed code
    # is in S and cannot read that bit itself.
    csrr    s3, mstatus
    csrw    mepc, s7
    mv      t4, s8                      # return privilege, set by the case
    csrw    mstatus, t4
    mret
trap_mismatch:
    slli    a0, t0, 16
    or      a0, a0, t1
    j       write_exit
fail_slot:
    li      a0, 2
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
s_page:
    # Case 1: an unmapped load. mtvec is vectored, so a handler that adds
    # 4 times the cause enters a failure slot instead of trap_body.
    li      s5, 13
    li      s6, 0x4000
    mv      s7, s9
    li      s8, 0x20800                 # return to S with MPRV armed
    la      t0, case2_arm
    la      t1, s_page
    sub     s10, t0, t1
    li      t0, 0x4000
    ld      a1, 0(t0)
    j       prog_fail
case2_arm:
    # Case 2: SRET from S to U. MPRV was set in M mode before the return
    # to S, and SRET must clear it. SPIE must become 1, and the low bits
    # of SEPC must not survive the return.
    li      t0, 0x22                    # SIE | SPIE, both clear
    csrc    sstatus, t0
    li      t0, 3
    add     t0, t0, s2                  # VA 0x2003
    csrw    sepc, t0
    li      t0, 0x100                   # SPP = U
    csrc    sstatus, t0
    sret
    j       prog_fail

    .align 12
u_page:
    # The return landed at VA 0x2000, so SEPC bits 1:0 were cleared, and
    # the privilege is U, so this U=1 page is executable. sstatus is not
    # readable in U, so the status checks happen after the return to S.
    ld      a1, 0(s0)
    li      a2, 0x1111222233334444
    bne     a1, a2, prog_fail

    # SRET in U is illegal. The handler checks that it changed nothing:
    # mtval must carry the SRET encoding itself, not 0x2004.
    li      s5, 2
    li      s6, 0x10200073              # the SRET encoding
    mv      s7, s11
    li      s8, 0x800                   # return to S
    sret
    j       prog_fail

.section .tohost
.align 3
tohost:
    .dword 0
