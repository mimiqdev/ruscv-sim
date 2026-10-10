# calibration-workloads/1: hand-declared deterministic family work.
.option norvc
.option norelax
.section .text
.globl _start
_start:
p0:
    auipc x8, 3
p4:
    addi x8, x8, 8
p8:
    auipc x9, 2
p12:
    addi x9, x9, -8
p16:
    addi x18, x0, 24
p20:
    addi x19, x0, 0
p24:
    addi x20, x0, 0
p28:
    addi x6, x0, -2
p32:
    addi x7, x0, 5
p36:
    addi x28, x0, 3
p40:
    sw x6, 0(x8)
p44:
    amoadd.w x11, x7, (x8)
p48:
    bne x11, x6, fail
p52:
    lr.w x12, (x8)
p56:
    bne x12, x28, fail
p60:
    sc.w x13, x6, (x8)
p64:
    bne x13, x0, fail
p68:
    lr.w x14, (x8)
p72:
    bne x14, x6, fail
p76:
    sw x28, 0(x8)
p80:
    sc.w x15, x7, (x8)
p84:
    addi x29, x0, 1
p88:
    bne x15, x29, fail
p92:
    sc.w x0, x7, (x8)
p96:
    lw x16, 0(x8)
p100:
    bne x16, x28, fail
p104:
    add x19, x19, x16
p108:
    addi x20, x20, 1
p112:
    addi x18, x18, -1
p116:
    bne x18, x0, p40
p120:
    addi x29, x0, 72
p124:
    bne x19, x29, fail
p128:
    addi x0, x0, 9
p132:
    sd x20, 0(x9)
p136:
    sd x19, 8(x9)
p140:
    auipc x31, 3
p144:
    addi x31, x31, -140
p148:
    ld x5, 0(x31)
p152:
    addi x29, x0, 291
p156:
    bne x5, x29, fail
p160:
    ld x5, 8(x31)
p164:
    addi x29, x0, 3
p168:
    bne x5, x29, fail
p172:
    ld x5, 16(x31)
p176:
    bne x5, x0, fail
p180:
    csrr x30, minstret
p184:
    sd x30, 16(x9)
p188:
    addi x10, x0, 7
p192:
    auipc x31, 1
p196:
    addi x31, x31, -192
p200:
    sd x10, 0(x31)
halt: j halt
fail:
    addi a0, x0, 41
    la t6, tohost
    sd a0, 0(t6)
failed: j failed
.section .tohost,"aw",@progbits
.balign 8
tohost: .dword 0
.section .signature,"aw",@progbits
.balign 8
signature: .dword 0,0,0
.section .data
.balign 8
neighbors: .dword 0x123
.section .work_bss,"aw",@nobits
.balign 8
scratch: .space 16
