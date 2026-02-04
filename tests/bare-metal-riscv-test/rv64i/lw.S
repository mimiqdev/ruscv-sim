# Test: LW - Load Word (32-bit signed load)
# Tests: LW, address alignment, sign extension

.section .data
.align 4
test_data:
    .word 0x12345678       # +ve 32-bit value
    .word 0x80000000       # -ve in signed 32-bit (should sign-extend to 0xFFFFFFFF80000000)
    .word 0xFFFFFFFF       # -1 in signed 32-bit (should sign-extend to 0xFFFFFFFFFFFFFFFF)
    .word 0x00000000       # zero

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Load positive value ===
    # lw should load 0x12345678, sign-extend to 0x0000000012345678
    lw      x1, 0(x10)          # x1 = *(x10 + 0)
    li      x7, 0x12345678      # x7 = expected value
    bne     x1, x7, fail        # if x1 != expected, fail

    # === Test 2: Load negative value (0x80000000) ===
    # lw should load 0x80000000, sign-extend to 0xFFFFFFFF80000000
    lw      x2, 4(x10)          # x2 = *(x10 + 4)
    li      x7, 0x80000000      # x7 = expected 32-bit value
    # Sign-extend x7 to 64-bit for comparison
    addiw   x8, x7, 0           # x8 = sign-extended x7
    bne     x2, x8, fail        # if x2 != expected, fail

    # === Test 3: Load -1 (0xFFFFFFFF) ===
    # lw should load 0xFFFFFFFF, sign-extend to 0xFFFFFFFFFFFFFFFF
    lw      x3, 8(x10)          # x3 = *(x10 + 8)
    li      x7, -1              # x7 = -1 (0xFFFFFFFFFFFFFFFF)
    bne     x3, x7, fail        # if x3 != -1, fail

    # === Test 4: Load zero ===
    lw      x4, 12(x10)         # x4 = *(x10 + 12)
    li      x7, 0               # x7 = 0
    bne     x4, x7, fail        # if x4 != 0, fail

    # === Test 5: Store then Load (verify SW works with LW) ===
    # Create a test area on stack
    addi    x11, x10, 16        # x11 = test_data + 16 (scratch area)
    li      x5, 0xAABBCCDD      # x5 = test value
    sw      x5, 0(x11)          # store word
    lw      x6, 0(x11)          # load word back
    li      x7, 0xAABBCCDD      # x7 = expected
    addiw   x7, x7, 0           # sign-extend expected value
    bne     x6, x7, fail        # verify loaded value

    # === Test 6: LW with negative offset ===
    addi    x12, x10, 8         # x12 = test_data + 8
    lw      x6, -4(x12)         # load from (x12 - 4) = test_data + 4
    li      x7, 0x80000000      # should load 0x80000000
    addiw   x8, x7, 0           # sign-extend expected value
    bne     x6, x8, fail

    # === Test 7: LW with different base registers ===
    mv      x20, x10
    lw      x6, 0(x20)          # use x20 as base
    li      x7, 0x12345678
    bne     x6, x7, fail

pass:
    li      x5, 0               # exit_code = 0 (pass)
    j       write_exit

fail:
    li      x5, 1               # exit_code = 1 (fail)
    j       write_exit

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
