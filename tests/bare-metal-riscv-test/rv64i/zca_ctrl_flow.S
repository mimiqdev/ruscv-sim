# zca_ctrl_flow.S — mixed-length control flow under --isa rv64imac.
#
# Compressed branches (c.beqz/c.bnez) and jumps (c.j) drive fall-through and
# taken transfers over a mixed 16/32-bit stream. A 32-bit instruction placed
# at a 2-byte-aligned address proves parcel fetch across pc%4==2. The exit
# path writes HTIF with the accumulated score.
#
# GNU as 2.40 note: the canonical c.* mnemonics use the compressed operand
# forms (c.addi rd, imm; c.add rd, rs2; branches with a label target). The
# assembler keeps them 16-bit when the form is exactly compressed-legal.
#
# Builds with -march=rv64imac_zicsr_zifencei; runs only under --isa rv64imac.

.section .text
.globl _start

_start:
    .option push
    .option norelax
    .option rvc

    # x5 = 0; the taken/not-taken ladder below accumulates a score in x5.
    c.li    x5, 0

    # Test 1: c.beqz taken (a1 = 0) jumps over the poison increment.
    c.li    a1, 0
    c.beqz  a1, t1_taken
    li      x7, 99
    c.add   x5, x7            # must be skipped
t1_taken:
    # Test 2: c.beqz not taken (a1 != 0) falls through and adds 1.
    c.li    a1, 1
    c.beqz  a1, t2_skipped
    c.addi  x5, 1
    c.j     t3_start
t2_skipped:
    addi    x5, x5, 64            # must be skipped
t3_start:
    # Test 3: c.bnez taken.
    c.li    a1, 3
    c.bnez  a1, t4_start
    li      x7, 99
    c.add   x5, x7            # must be skipped
t4_start:
    # Test 4: c.bnez not taken falls through.
    c.li    a1, 0
    c.bnez  a1, t5_skipped
    c.addi  x5, 2
    c.j     t6_start
t5_skipped:
    addi    x5, x5, 64            # must be skipped
t6_start:
    # Test 5: c.j skips a poison word and lands on the next increment.
    c.j     t6_landed
    .word 0x00000000           # illegal parcel must never execute
t6_landed:
    c.addi  x5, 4

    # Test 6: a 32-bit instruction at a 2-byte-aligned address. The stream
    # above leaves pc at pc % 4 == 2 here, so the addiw fetch starts on a
    # parcel boundary and proves non-4-byte-aligned 32-bit fetch.
    .option norvc
    addiw   x6, x0, 8
    .option rvc
    c.add   x5, x6            # x5 += 8

    # Expected score: 0 + 1 + 2 + 4 + 8 = 15.
    c.li    x7, 15
    beq     x5, x7, pass
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

.section .tohost
.align 3
.global tohost
tohost:
    .dword 0
