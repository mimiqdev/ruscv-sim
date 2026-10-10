# calibration-workloads/1: hand-declared deterministic family work.
.option norvc
.option norelax
.section .text
.globl _start
_start:
p0:
    auipc x8, 589824
p4:
    addi x8, x8, 0
p8:
    auipc x9, 2
p12:
    addi x9, x9, -8
p16:
    addi x18, x0, 64
p20:
    addi x19, x0, 0
p24:
    addi x20, x0, 0
p28:
    addi x6, x0, 65
p32:
    sb x6, 0(x8)
p36:
    add x19, x19, x6
p40:
    addi x20, x20, 1
p44:
    addi x18, x18, -1
p48:
    bne x18, x0, p32
p52:
    lui x29, 1
p56:
    addi x29, x29, 64
p60:
    bne x19, x29, fail
p64:
    addi x0, x0, 9
p68:
    sd x20, 0(x9)
p72:
    sd x19, 8(x9)
p76:
    auipc x31, 3
p80:
    addi x31, x31, -76
p84:
    ld x5, 0(x31)
p88:
    addi x29, x0, 291
p92:
    bne x5, x29, fail
p96:
    ld x5, 8(x31)
p100:
    addi x29, x0, 0
p104:
    bne x5, x29, fail
p108:
    ld x5, 16(x31)
p112:
    bne x5, x0, fail
p116:
    csrr x30, minstret
p120:
    sd x30, 16(x9)
p124:
    addi x10, x0, 7
p128:
    auipc x31, 1
p132:
    addi x31, x31, -128
p136:
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
