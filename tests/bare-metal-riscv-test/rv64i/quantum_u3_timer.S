# quantum U3 (quantum D6/D8/D10). The image of the live debug-attach session
# that runs under `run --transport tlm --quantum 1024`, and an ordinary suite
# guest when no debugger is attached.
#
# The guest arms a time-derived timer crossing: it samples `time`, stores
# `mtimecmp` = that sample plus DELTA, enables MTIE and `mstatus.MIE`, then
# spins. MTIP therefore rises only because committed time reached the compare,
# with no CLINT store in that turn. The trap handler reads `time` first and
# records the D8 lateness `R - X`: local time passes through every tick, so the
# reference point `X` is the compare itself, and the boundary that takes the
# trap completes one turn before the handler's first instruction reads `time`.
#
# Unattached, the guest reaches its own tohost exit through that trap. The
# debug session halts it after the compare is armed and again after the
# lateness word is written, and reads `mtime` and the recorded words through
# the system bus, so no fact here depends on the debugger.

.equ DELTA, 64

.section .text
.globl _start
.globl q3_spin
.globl q3_report

_start:
    la      t0, q3_trap
    csrw    mtvec, t0
    rdtime  t0
    la      t1, q3_arm_time
    sd      t0, 0(t1)
    li      t2, DELTA
    add     t3, t0, t2
    la      t4, q3_compare
    sd      t3, 0(t4)
    lui     t4, 0x02004                 # mtimecmp hart 0 at 0x0200_4000
    sd      t3, 0(t4)
    li      t5, 0x80                    # MTIE
    csrs    mie, t5
    csrsi   mstatus, 8
q3_spin:
    j       q3_spin

.align 2
q3_trap:
    rdtime  t0
    la      t1, q3_trap_time
    sd      t0, 0(t1)
    la      t1, q3_traps
    ld      t2, 0(t1)
    addi    t2, t2, 1
    sd      t2, 0(t1)
    csrr    t2, mcause
    la      t1, q3_mcause
    sd      t2, 0(t1)
    csrr    t2, mepc
    la      t1, q3_mepc
    sd      t2, 0(t1)
    la      t1, q3_compare
    ld      t2, 0(t1)
    addi    t0, t0, -1
    sub     t0, t0, t2
    la      t1, q3_lateness
    sd      t0, 0(t1)
    csrci   mie, 8

q3_report:
    li      t0, 1
    slli    t0, t0, 63
    la      t1, tohost
    sd      t0, 0(t1)
q3_hang:
    j       q3_hang

.section .data
.align 3
.globl q3_arm_time
q3_arm_time:
    .dword  0
.globl q3_compare
q3_compare:
    .dword  0
.globl q3_trap_time
q3_trap_time:
    .dword  0
.globl q3_lateness
q3_lateness:
    .dword  0
.globl q3_traps
q3_traps:
    .dword  0
.globl q3_mcause
q3_mcause:
    .dword  0
.globl q3_mepc
q3_mepc:
    .dword  0

.section .tohost
.align 3
.globl tohost
.globl fromhost
tohost:
    .dword  0
fromhost:
    .dword  0
