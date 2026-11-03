# Zca U5 guest. Contract D10: one mixed-length rv64imac image for the
# OpenOCD/GDB compressed session.
#
# Every instruction below has a fixed length, so each named symbol keeps its
# length and its `pc % 4` residue. The compressed forms are written with their
# C mnemonics (the assembler here does not shorten the 32-bit spellings), and
# the 4-byte forms sit inside `.option arch, -c` where relaxation could
# otherwise shorten them. The offsets are pinned by the assertions in
# scripts/debug_attach.sh.
#
#   _start + 0  2-byte  c.li a0, 0x1f
#   _start + 2  2-byte  c.addi a1, 1
#   _start + 4  2-byte  c.nop
#   _start + 6  2-byte  c.addi a0, 1        Z5 2-byte software breakpoint
#   _start + 8  2-byte  c.nop
#   _start + a  4-byte  jal x0, zca_link    Z5 4-byte breakpoint at pc % 4 == 2
#   _start + e  4-byte  jal s0, zca_hint    link value zca_second_link
#   _start + 12 2-byte  c.nop               zca_second_link; not reached
#   _start + 14 2-byte  c.nop               padding, not reached
#   _start + 16 2-byte  c.nop               padding, not reached
#   _start + 18 2-byte  c.nop               padding, not reached
#   _start + 1a 4-byte  addi x0, x0, 0x5a   zca_hint, a HINT at pc % 4 == 2
#   _start + 1e 4-byte  jal x0, zca_exit
#
# A debugger plants a software breakpoint on the 2-byte instruction, then on
# the 4-byte instruction at pc % 4 == 2, continues and single-steps across
# both, then single-steps the 4-byte link and the 4-byte HINT before parking
# the hart on zca_spin. After `monitor reset halt` the image runs from _start
# to the tohost word without a breakpoint.

.section .text
.globl _start
.globl zca_len2
.globl zca_len4
.globl zca_link
.globl zca_second_link
.globl zca_hint
.globl zca_spin
.globl zca_exit

.option arch, +c
_start:
    c.li    a0, 0x1f
    c.addi  a1, 1
    c.nop
zca_len2:
    c.addi  a0, 1
    c.nop
.option arch, -c
zca_len4:
    jal     x0, zca_link
zca_link:
    jal     s0, zca_hint
.option arch, +c
zca_second_link:
    c.nop
    c.nop
    c.nop
    c.nop
.option arch, -c
zca_hint:
    addi    x0, x0, 0x5a
    jal     x0, zca_exit
.option arch, +c

zca_spin:
    c.j     zca_spin
zca_exit:
    c.li    t0, 1
    la      t1, tohost
    sd      t0, 0(t1)
    c.j     zca_exit

.section .tohost
.align 3
.globl tohost
.globl fromhost
tohost:
    .dword  0
fromhost:
    .dword  0
