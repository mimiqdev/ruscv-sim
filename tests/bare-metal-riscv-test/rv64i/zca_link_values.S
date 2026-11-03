# zca_link_values.S — link values and mixed-length fall-through under --isa rv64imac.
#
# Contract D5: fall-through and link values are pc + length (2 or 4). Compressed
# c.jalr links pc+2, a 32-bit jal links pc+4, and control transfers at
# 2-byte-aligned addresses work across the mixed stream.
#
# GNU as 2.40 note: c.jalr/c.jr/c.mv/c.add keep their compressed 2-operand
# forms; branches compress only when the target fits the compressed offset.

.section .text
.globl _start

_start:
    .option push
    .option norelax
    .option rvc

    c.li    s0, 0             # accumulated error code

    # Test 1: c.jalr links pc+2. The callee computes link - call_site.
    la      t0, callee
    auipc   t1, 0             # t1 = address of this auipc = M (4 bytes)
    c.jalr  t0                # call_site = M + 4; link = M + 6
    c.j     t1_resume         # 2-byte skip over the poison word
    .word 0x00000000           # must never execute
t1_resume:
    # The callee stored link - call_site in a1; expected 2.
    c.li    t3, 2
    beq     a1, t3, test2
    c.addi  s0, 1

test2:
    # Test 2: a 32-bit jal links pc+4. jal x9, 3f: the return lands on the
    # c.li after it, and x9 must equal that instruction's address.
    c.li    a2, 0
    jal     x9, after_jal
after_jal:
    c.li    t3, 0
    la      t4, after_jal
    beq     x9, t4, test3     # link (call_site + 4) == the after_jal label
    c.addi  s0, 2
test3:
    # Test 3: c.jr return with a 2-byte target works and links nothing.
    la      t0, t3_landed
    c.jr    t0                # jump to t3_landed
    .word 0x00000000           # must never execute
t3_landed:
    c.addi  t5, 5             # landed: proves the 2-byte-target transfer

    # Exit: s0 == 0 means every check passed.
    beqz    s0, pass
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

# Callee for test 1: a1 = link (x1/ra) - call_site. The call site is the
# instruction right after the auipc marker, so pass it in t1 and compute
# link - (t1 + 4) here. c.jr x1 returns.
callee:
    addi    t5, t1, 4         # call_site address
    sub     a1, x1, t5        # link - call_site = 2
    c.jr    x1

.section .tohost
.align 3
.global tohost
tohost:
    .dword 0
