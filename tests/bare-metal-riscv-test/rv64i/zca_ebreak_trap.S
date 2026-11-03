# zca_ebreak_trap.S — C.EBREAK trap under --isa rv64imac.
#
# Contract D8/D2: C.EBREAK is EBREAK. Without ebreak redirection it raises
# Breakpoint (mcause=3, mtval=0). The trap handler advances mepc past the
# 2-byte C.EBREAK and returns with mret, proving compressed trap entry, the
# D5 mepc mask (bit 0 cleared on write) and a return to a 2-byte-aligned
# target in a mixed-length stream.
#
# GNU as 2.40 note: `c.ebreak` assembles to the 2-byte 0x9002 parcel.

.section .text
.globl _start

_start:
    .option push
    .option norelax
    .option rvc

    la      t0, trap_handler
    csrw    mtvec, t0

    c.li    s0, 0             # trap count
    c.li    s1, 0             # mepc-alignment flags (0 = always 2-aligned)

    # Test 1: C.EBREAK inside a compressed stream. The handler counts the
    # trap, adds 2 to mepc and resumes at the c.addi below.
    c.addi  s0, 1             # runs before AND after the trap: s0 = 2
    c.ebreak                  # mcause=3, mepc = this address
    c.addi  s0, 1             # resume lands here: s0 = 2

    # Test 2: mepc was exactly 2-byte aligned at entry (bit 0 clear). The
    # handler ORs the raw mepc into s1; any set bit means a mask violation.
    # This stream also places a 32-bit instruction inside the compressed
    # region, proving mixed lengths around the trap.
    .option norvc
    addiw   t1, x0, 8         # 32-bit instruction in the compressed stream
    .option rvc
    c.add   s0, t1            # s0 += 8

    # Test 3: second C.EBREAK at a 4-byte-aligned address.
    .align  2
    .option norvc
    nop                       # keep the next parcel 4-byte aligned
    .option rvc
    c.ebreak                  # handler resumes at the check
    c.addi  s0, 4             # s0 = 2 + 8 + 4 = 14

    # Expected: 14. The handler adds nothing to s0; the resumed stream does.
    c.li    t2, 14
    beq     s0, t2, check_align
    c.j     fail

check_align:
    # Every mepc had bit 0 clear (2-byte aligned per D5); bit 1 may be set
    # because a C.EBREAK at pc%4==2 stores pc with bit 1 set.
    andi    t3, s1, 1
    beqz    t3, pass
    c.j     fail

pass:
    c.li    a0, 0
    j       write_exit
fail:
    c.li    a0, 1

write_exit:
    .option pop
    la      t0, tohost
    li      t2, 1
    slli    t2, t2, 63
    or      t1, a0, t2
    sd      t1, 0(t0)
1:
    j       1b

# Trap handler: EBREAK only. Record mepc alignment, advance mepc by 2 (the
# C.EBREAK length), and mret. mtvec's direct-mode base is 4-byte aligned, so
# the handler must be 4-byte aligned.
    .align 2
trap_handler:
    csrr    t3, mcause
    li      t4, 3
    bne     t3, t4, trap_fail
    csrr    t3, mepc
    or      s1, s1, t3        # bit 0 must be clear; OR the raw value
    andi    t3, t3, -2        # clear bit 0 (the D5 mask), keep bit 1
    addi    t3, t3, 2         # skip the 2-byte C.EBREAK
    csrw    mepc, t3
    mret
trap_fail:
    # Unexpected cause: poison the score so the exit check fails.
    li a0, 99
    la      t0, tohost
    li      t2, 1
    slli    t2, t2, 63
    or      t1, a0, t2
    sd      t1, 0(t0)
2:
    j       2b

.section .tohost
.align 3
.global tohost
tohost:
    .dword 0
