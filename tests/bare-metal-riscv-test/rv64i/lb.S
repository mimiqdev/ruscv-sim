# Test: LB - Load Byte (8-bit signed load)
# Tests: LB, sign extension to 64-bit

.section .data
.align 4
test_data:
    .byte 0x12               # +ve 8-bit value
    .byte 0x80               # -ve in signed 8-bit (sign-extend to 0xFFFFFFFFFFFFFF80)
    .byte 0xFF               # -1 in signed 8-bit (sign-extend to 0xFFFFFFFFFFFFFFFF)
    .byte 0x00               # zero
    .byte 0x7F               # +127 (max positive)
    .byte 0xAB               # for testing
    .word 0x12345678         # for multi-byte tests

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Load positive byte (0x12) ===
    lb      x1, 0(x10)          # x1 = *(x10 + 0)
    li      x7, 0x12            # x7 = expected value
    bne     x1, x7, fail        # if x1 != expected, fail

    # === Test 2: Load negative byte (0x80) ===
    # lb should load 0x80, sign-extend to 0xFFFFFFFFFFFFFF80
    lb      x2, 1(x10)          # x2 = *(x10 + 1)
    li      x7, 0x80            # x7 = 0x80
    slli    x8, x7, 56          # shift left to bit 63
    srai    x8, x8, 56          # arithmetic shift right to sign-extend
    bne     x2, x8, fail        # if x2 != expected, fail

    # === Test 3: Load -1 (0xFF) ===
    # lb should load 0xFF, sign-extend to 0xFFFFFFFFFFFFFFFF
    lb      x3, 2(x10)          # x3 = *(x10 + 2)
    li      x7, -1              # x7 = -1
    bne     x3, x7, fail        # if x3 != -1, fail

    # === Test 4: Load zero (0x00) ===
    lb      x4, 3(x10)          # x4 = *(x10 + 3)
    li      x7, 0               # x7 = 0
    bne     x4, x7, fail        # if x4 != 0, fail

    # === Test 5: Load max positive byte (0x7F = 127) ===
    lb      x5, 4(x10)          # x5 = *(x10 + 4)
    li      x7, 127             # x7 = 127
    bne     x5, x7, fail        # verify

    # === Test 6: SB then LB (verify store/load pair) ===
    addi    x11, x10, 24        # x11 = scratch area
    li      x6, 0xAB            # x6 = test value
    sb      x6, 0(x11)          # store byte
    lb      x7, 0(x11)          # load byte back
    # 0xAB has bit 7 set, so sign-extend to 64-bit: 0xFFFFFFFFFFFFFFAB
    li      x8, 0xAB
    slli    x8, x8, 56
    srai    x8, x8, 56          # sign-extend
    bne     x7, x8, fail        # verify

    # === Test 7: LB with negative offset ===
    addi    x12, x10, 2         # x12 = test_data + 2
    lb      x6, -1(x12)         # load from (x12 - 1) = test_data + 1
    li      x7, 0x80            # should load 0x80
    slli    x8, x7, 56
    srai    x8, x8, 56          # sign-extend
    bne     x6, x8, fail

    # === Test 8: Load each byte from a word ===
    # Word at offset 6: 0x12345678 (little-endian: 78 56 34 12)
    lb      x6, 6(x10)          # load byte 0: 0x78
    li      x7, 0x78
    bne     x6, x7, fail

    lb      x6, 7(x10)          # load byte 1: 0x56
    li      x7, 0x56
    bne     x6, x7, fail

    lb      x6, 8(x10)          # load byte 2: 0x34
    li      x7, 0x34
    bne     x6, x7, fail

    lb      x6, 9(x10)          # load byte 3: 0x12
    li      x7, 0x12
    bne     x6, x7, fail

    # === Test 9: Load negative byte from word ===
    # If we store 0xDEADBE80, byte at offset 0 should be 0x80 (negative)
    li      x1, 0xDEADBE80
    sw      x1, 0(x11)
    lb      x2, 0(x11)
    li      x7, 0x80
    slli    x8, x7, 56
    srai    x8, x8, 56
    bne     x2, x8, fail

pass:
    li      x5, 0               # exit_code = 0 (pass)
    j       write_exit

fail:
    li      x5, 1               # exit_code = 1 (fail)

write_exit:
    la      x4, tohost
    li      x6, 1
    slli    x6, x6, 63          # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6          # x5 = WRITE_MARKER | exit_code
    sd      x5, 0(x4)           # write to tohost

inf:
    j       inf

.section .tohost
.align 3
tohost:
    .dword  0
