# Test: LWU - Load Word Unsigned (32-bit unsigned load)
# Tests: LWU, zero extension to 64-bit

.section .data
.align 4
test_data:
    .word 0x12345678       # +ve 32-bit value
    .word 0x80000000       # should zero-extend to 0x0000000080000000
    .word 0xFFFFFFFF       # should zero-extend to 0x00000000FFFFFFFF
    .word 0x00000000       # zero

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Load positive value (0x12345678) ===
    # lwu should load 0x12345678, zero-extend to 0x0000000012345678
    lwu     x1, 0(x10)          # x1 = *(x10 + 0)
    li      x7, 0x12345678      # x7 = expected value
    bne     x1, x7, fail        # if x1 != expected, fail

    # === Test 2: Load high bit set value (0x80000000) ===
    # lwu should load 0x80000000, zero-extend to 0x0000000080000000
    # (NOT sign-extend to 0xFFFFFFFF80000000 like LW would do)
    lwu     x2, 4(x10)          # x2 = *(x10 + 4)
    li      x7, 0x80000000      # x7 = 0x80000000 (zero-extended)
    bne     x2, x7, fail        # if x2 != expected, fail
    # Verify upper 32 bits are zero
    srli    x3, x2, 32          # shift right 32 bits
    li      x7, 0               # upper bits should be 0
    bne     x3, x7, fail

    # === Test 3: Load all 1s (0xFFFFFFFF) ===
    # lwu should load 0xFFFFFFFF, zero-extend to 0x00000000FFFFFFFF
    # (NOT sign-extend to 0xFFFFFFFFFFFFFFFF like LW would do)
    lwu     x4, 8(x10)          # x4 = *(x10 + 8)
    li      x7, 0xFFFFFFFF      # x7 = 0xFFFFFFFF (zero-extended)
    bne     x4, x7, fail        # if x4 != expected, fail
    # Verify upper 32 bits are zero
    srli    x3, x4, 32          # shift right 32 bits
    li      x7, 0               # upper bits should be 0
    bne     x3, x7, fail

    # === Test 4: Load zero ===
    lwu     x5, 12(x10)         # x5 = *(x10 + 12)
    li      x7, 0               # x7 = 0
    bne     x5, x7, fail        # if x5 != 0, fail

    # === Test 5: SW then LWU (verify store/load pair) ===
    addi    x11, x10, 16        # x11 = scratch area
    li      x6, 0xAABBCCDD      # x6 = test value
    sw      x6, 0(x11)          # store word
    lwu     x7, 0(x11)          # load word unsigned
    li      x8, 0xAABBCCDD      # x8 = expected
    bne     x7, x8, fail        # verify loaded value
    # Verify upper bits are zero
    srli    x3, x7, 32
    li      x8, 0
    bne     x3, x8, fail

    # === Test 6: LWU with negative offset ===
    addi    x12, x10, 8         # x12 = test_data + 8
    lwu     x6, -4(x12)         # load from (x12 - 4) = test_data + 4
    li      x7, 0x80000000      # should load 0x80000000
    bne     x6, x7, fail
    # Verify zero-extension
    srli    x3, x6, 32
    li      x7, 0
    bne     x3, x7, fail

    # === Test 7: Compare LWU vs LW behavior ===
    # LW should sign-extend 0xFFFFFFFF to -1
    lw      x1, 8(x10)          # load with sign extension
    li      x7, -1              # expect -1
    bne     x1, x7, fail

    # LWU should zero-extend 0xFFFFFFFF to 0xFFFFFFFF
    lwu     x2, 8(x10)          # load with zero extension
    li      x7, 0xFFFFFFFF      # expect 0xFFFFFFFF
    bne     x2, x7, fail

    # They should NOT be equal
    beq     x1, x2, fail        # LW and LWU results should differ for 0xFFFFFFFF

    # === Test 8: Test with different base registers ===
    mv      x20, x10
    lwu     x6, 0(x20)          # use x20 as base
    li      x7, 0x12345678
    bne     x6, x7, fail

    # === Test 9: Store negative, load with LWU ===
    li      x1, -1              # x1 = 0xFFFFFFFFFFFFFFFF
    sw      x1, 0(x11)          # store lower 32 bits (0xFFFFFFFF)
    lwu     x2, 0(x11)          # load with zero extension
    li      x7, 0xFFFFFFFF      # expect 0xFFFFFFFF, not -1
    bne     x2, x7, fail
    # Ensure it's not sign-extended
    bge     x2, x0, lwu_ok      # 0xFFFFFFFF as unsigned is positive
    j       fail
lwu_ok:

    # === Test 10: Large positive 32-bit value ===
    li      x1, 0x7FFFFFFF      # max positive 32-bit signed
    sw      x1, 0(x11)
    lwu     x2, 0(x11)
    li      x7, 0x7FFFFFFF
    bne     x2, x7, fail
    srli    x3, x2, 32          # check upper bits
    li      x7, 0
    bne     x3, x7, fail

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
