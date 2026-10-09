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
    addi x18, x0, 64
p20:
    addi x19, x0, 0
p24:
    addi x20, x0, 0
p28:
    ld x28, 0(x8)
p32:
    bne x28, x0, fail
p36:
    addi x20, x20, 1
p40:
    sw x20, 0(x8)
p44:
    lw x28, 0(x8)
p48:
    bne x28, x20, fail
p52:
    add x19, x19, x28
p56:
    addi x18, x18, -1
p60:
    bne x18, x0, p36
p64:
    lui x29, 1
p68:
    addi x29, x29, -2016
p72:
    bne x19, x29, fail
p76:
    addi x0, x0, 9
p80:
    sd x20, 0(x9)
p84:
    sd x19, 8(x9)
p88:
    auipc x31, 3
p92:
    addi x31, x31, -88
p96:
    ld x5, 0(x31)
p100:
    addi x29, x0, 291
p104:
    bne x5, x29, fail
p108:
    ld x5, 8(x31)
p112:
    addi x29, x0, 64
p116:
    bne x5, x29, fail
p120:
    ld x5, 16(x31)
p124:
    bne x5, x0, fail
p128:
    csrr x30, minstret
p132:
    sd x30, 16(x9)
p136:
    addi x10, x0, 7
p140:
    auipc x31, 1
p144:
    addi x31, x31, -140
p148:
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
