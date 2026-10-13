# Stage 4 identity and counters guest (dev-plan §4 "Identity and counters",
# M2 and D10).
#
# mvendorid, marchid, mimpid, and mconfigptr read 0 and reject a write with
# illegal-instruction; per the §7.8 ruling mtval then holds the exact 32-bit
# faulting instruction bits, which the guest asserts.  mcycle advances by
# the completed cycles of a retired instruction and minstret by 1; a value
# written to either counter reads back after that instruction.  cycle and
# instret reads in S and in U raise illegal-instruction while mcounteren is
# 0.  mhpmcounter3/31 and mhpmevent3/31 read 0 and ignore writes.
# hpmcounter3 and time do not exist.
#
# The handler returns to s7 with the s4-selected MPP (0x800 = S,
# 0x1800 = M), set per case so an M-phase continuation stays in M.

    .equ CAUSE_ILLEGAL, 2
    .equ ANY, -2

.section .text
.globl _start

_start:
    la      t0, trap_handler
    csrw    mtvec, t0
    la      s1, tohost
    li      s5, -1
    li      s4, 0x1800                  # default: stay in M

    # Identity registers read 0.
    csrr    t1, mvendorid
    bnez    t1, fail
    csrr    t1, marchid
    bnez    t1, fail
    csrr    t1, mimpid
    bnez    t1, fail
    csrr    t1, mconfigptr
    bnez    t1, fail

    # A write to each raises illegal-instruction with mtval = the csrw bits.
    # The encodings are asserted, not wildcarded (§7.8 ruling).
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xf1129073              # csrw mvendorid,t0
    la      s7, id_marchid
    csrw    mvendorid, t0
    j       fail
id_marchid:
    csrr    t1, mvendorid
    bnez    t1, fail
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xf1229073              # csrw marchid,t0
    la      s7, id_mimpid
    csrw    marchid, t0
    j       fail
id_mimpid:
    csrr    t1, marchid
    bnez    t1, fail
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xf1329073              # csrw mimpid,t0
    la      s7, id_mconfigptr
    csrw    mimpid, t0
    j       fail
id_mconfigptr:
    csrr    t1, mimpid
    bnez    t1, fail
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xf1529073              # csrw mconfigptr,t0
    la      s7, counters
    csrw    mconfigptr, t0
    j       fail

counters:
    csrr    t1, mconfigptr
    bnez    t1, fail

    # mcycle exists and reads back a written value after the writing
    # instruction retires; the write itself retires (no trap).
    li      t0, 0x1111
    csrw    mcycle, t0
    csrr    t1, mcycle
    blt     t1, t0, fail                # counter kept the written value and
                                        # only advanced by completed turns
    li      t2, 0x1111 + 64
    bge     t1, t2, fail

    # minstret: two reads differ by exactly the retired instructions
    # between them (the addi; both reads themselves retire, so 3 reads
    # apart in retirement count across two csrr reads plus one addi).
    csrr    s2, minstret
    addi    t1, x0, 0
    csrr    s3, minstret
    sub     t1, s3, s2
    li      t2, 2                       # csrr + addi retired between reads
    bne     t1, t2, fail

    # A written minstret value is the value read back after the writing
    # instruction: the csrr that follows sees the write plus its own
    # retirement is not yet counted (write takes effect before the next
    # instruction retires, and the read itself has not retired).
    li      t0, 0x2222
    csrw    minstret, t0
    csrr    t1, minstret
    li      t2, 0x2222
    bne     t1, t2, fail

    # The 29 extra counters and their event selectors read 0 and ignore a
    # write: both ends of each range, no trap.
    li      s5, -1
    li      t0, -1
    csrw    0xB03, t0
    csrr    t1, 0xB03
    bnez    t1, fail
    csrw    0xB1F, t0
    csrr    t1, 0xB1F
    bnez    t1, fail
    csrw    0x323, t0
    csrr    t1, 0x323
    bnez    t1, fail
    csrw    0x33F, t0
    csrr    t1, 0x33F
    bnez    t1, fail

    # hpmcounter3 (0xC03) and time (0xC01) do not exist in any mode.
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xc0302373              # csrr t1,hpmcounter3
    la      s7, no_time
    csrr    t1, 0xC03
    j       fail
no_time:
    li      s5, CAUSE_ILLEGAL
    li      s6, 0xc0102373              # csrr t1,time (rdtime)
    la      s7, to_s
    csrr    t1, 0xC01
    j       fail

to_s:
    # S-mode reads of cycle (0xC00) and instret (0xC02) trap.
    la      t0, s_reads
    csrw    mepc, t0
    li      t0, 0x880                   # MPIE | MPP = S
    csrw    mstatus, t0
    mret
s_reads:
    li      s5, CAUSE_ILLEGAL
    li      s4, 0x800                   # next continuation returns to S
    li      s6, 0xc0002373              # csrr t1,cycle (rdcycle)
    la      s7, s_instret
    csrr    t1, cycle
    j       fail
s_instret:
    li      s5, CAUSE_ILLEGAL
    li      s4, 0x800
    li      s6, 0xc0202373              # csrr t1,instret (rdinstret)
    la      s7, to_u
    csrr    t1, instret
    j       fail
to_u:
    # Drop to U via sret; U-mode reads of cycle and instret also trap.
    la      t0, u_reads
    csrw    sepc, t0
    sret
u_reads:
    li      s5, CAUSE_ILLEGAL
    li      s4, 0x800
    li      s6, 0xc0002373              # csrr t1,cycle
    la      s7, u_instret
    csrr    t1, cycle
    j       fail
u_instret:
    li      s5, CAUSE_ILLEGAL
    li      s4, 0x1800                  # back to M for the exit word
    li      s6, 0xc0202373              # csrr t1,instret
    la      s7, done_ok
    csrr    t1, instret
    j       fail
done_ok:
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
    mv      t2, s5
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
    csrw    mstatus, s4
    mret

.section .tohost
.align 3
tohost:
    .dword 0
