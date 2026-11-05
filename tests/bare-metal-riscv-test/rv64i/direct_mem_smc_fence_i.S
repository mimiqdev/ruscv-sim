# TLM direct memory U3 (direct-memory D11): self-modifying code through FENCE.I.
#
# The guest stores a new instruction into its own instruction stream, orders
# that store with FENCE.I, and then executes the patched word.  The original
# word is `addi a0, zero, 1`; the planted word is `addi a0, zero, 42`.  A lost
# store, a stale fetch or a fetch that reads a partly written word leaves a0
# at 1 and the guest exits with code 1.
#
# Architectural behaviour alone does not distinguish the blocking route from
# the direct one: both are supposed to produce the same bytes.  The proof that
# this guest took the direct path lives in tests/direct_mem_u3_guests.rs, which
# runs this image through a counting target and requires that the store into
# the code page and the fetch of the patched word both leave b_transport when
# direct memory is enabled.  This image is also a member of the fresh suite,
# where it runs a third time under `--transport tlm --dmi`, so a direct pass
# that diverges from native fails the suite.

    .section .text
    .globl _start

_start:
    la      s0, smc_patch           # the word that will be patched
    li      t0, 0x02a00513          # addi a0, zero, 42
    sw      t0, 0(s0)               # store into the code page
    fence.i                         # make the store visible to fetch
    li      a0, 0

    .balign 4
smc_patch:
    addi    a0, zero, 1              # patched to 42 before it executes
smc_after:
    li      t1, 42
    bne     a0, t1, smc_fail

    # The patched word must still be in memory after the fetch of it.
    lw      t2, 0(s0)
    li      t3, 0x02a00513
    bne     t2, t3, smc_fail

    li      a0, 0
smc_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t2, tohost
    sd      a0, 0(t2)
smc_hang:
    j       smc_hang

smc_fail:
    li      a0, 1
    j       smc_exit

    .section .tohost
    .align 3
    .globl tohost
tohost:
    .dword  0
