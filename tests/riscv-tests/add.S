# Simple RISC-V test: calculate 1 + 2 + ... + 10 = 55
# Tests: ADD, ADDI, SUB, BLT, BNE, JALR

.section .text
.globl _start

_start:
    # Initialize
    li      x1, 0           # sum = 0
    li      x2, 1           # i = 1
    li      x3, 10          # n = 10

loop:
    # if i > n, exit loop
    bgt     x2, x3, done

    # sum += i
    add     x1, x1, x2

    # i++
    addi    x2, x2, 1

    j       loop

done:
    # Expected result: x1 = 55 (0x37)
    
    # 验证计算结果是否正确
    li      x7, 55          # x7 = expected value (55)
    beq     x1, x7, pass    # if x1 == 55, test passed

fail:
    # 测试失败，exit code = 1
    li      x5, 1
    j       write_exit

pass:
    # 测试通过，exit code = 0
    li      x5, 0

write_exit:
    # 使用 la 让汇编器自动生成正确的 auipc/addi/ld 序列
    la      x4, tohost

    li      x6, 1
    slli    x6, x6, 63      # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6      # x5 = WRITE_MARKER | exit_code
    
    # 现在 x4 必然是 8 字节对齐的 (因为 .align 3)，不会报错
    sd      x5, 0(x4)       # Write to tohost

    # Loop forever
inf:
    j       inf

# Tohost address for simulation
.section .data
.space 0x100
.align 12
tohost:
    .dword  0
