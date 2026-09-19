# Deterministic public-facade workload for A7 migration observations.
# The same bytes are assembled once and passed to both isolated revisions.

.equ ITERATIONS, 4096

.section .text
.globl _start

_start:
    la      t0, scratch
    li      t1, 0x13579bdf
    li      t2, ITERATIONS

loop:
    sd      t1, 0(t0)
    ld      t3, 0(t0)
    addi    t1, t3, 1
    addi    t2, t2, -1
    bnez    t2, loop

    # Exit code 0 in the ELF-declared .tohost RAM location.
    la      t0, tohost
    li      t1, 1
    slli    t1, t1, 63
    sd      t1, 0(t0)

wait:
    j       wait

.section .data
.align 3
scratch:
    .dword 0

.section .tohost
.align 3
tohost:
    .dword 0
