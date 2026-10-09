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
    addi x18, x0, 128
p20:
    addi x19, x0, 0
p24:
    addi x20, x0, 0
p28:
    addi x20, x20, 1
p32:
    add x19, x19, x20
p36:
    addi x18, x18, -1
p40:
    bne x18, x0, p28
p44:
    lui x29, 2
p48:
    addi x29, x29, 64
p52:
    bne x19, x29, fail
p56:
    addi x0, x0, 9
p60:
    sd x20, 0(x9)
p64:
    sd x19, 8(x9)
p68:
    auipc x31, 3
p72:
    addi x31, x31, -68
p76:
    ld x5, 0(x31)
p80:
    addi x29, x0, 291
p84:
    bne x5, x29, fail
p88:
    ld x5, 8(x31)
p92:
    addi x29, x0, 0
p96:
    bne x5, x29, fail
p100:
    ld x5, 16(x31)
p104:
    bne x5, x0, fail
p108:
    csrr x30, minstret
p112:
    sd x30, 16(x9)
p116:
    addi x10, x0, 7
p120:
    auipc x31, 1
p124:
    addi x31, x31, -120
p128:
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
