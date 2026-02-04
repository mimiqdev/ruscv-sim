# Test: SB - Store Byte (8-bit store)
# Tests: SB, memory write/read back verification

.section .data
.align 4
test_data:
    .byte 0x00               # scratch area 0
    .byte 0x00               # scratch area 1
    .byte 0x00               # scratch area 2
    .byte 0x00               # scratch area 3

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Store simple positive byte ===
    li      x1, 0x12            # x1 = test value
    sb      x1, 0(x10)          # store to *(x10 + 0)
    lb      x2, 0(x10)          # load back (to verify)
    li      x7, 0x12            # x7 = expected
    bne     x2, x7, fail        # verify

    # === Test 2: Store value with high bit set (0x80) ===
    li      x1, 0x80            # x1 = test value
    sb      x1, 1(x10)          # store to *(x10 + 1)
    lb      x2, 1(x10)          # load back
    li      x7, 0x80            # should be same value
    slli    x8, x7, 56
    srai    x8, x8, 56          # sign-extend expected
    bne     x2, x8, fail

    # === Test 3: Store all 1s (0xFF) ===
    li      x1, -1              # x1 = 0xFFFFFFFFFFFFFFFF
    sb      x1, 2(x10)          # store lower 8 bits
    lb      x2, 2(x10)          # load back
    li      x7, -1              # expect -1 (sign-extended)
    bne     x2, x7, fail

    # === Test 4: Store zero ===
    li      x1, 0               # x1 = 0
    sb      x1, 3(x10)          # store
    lb      x2, 3(x10)          # load back
    li      x7, 0               # expect 0
    bne     x2, x7, fail

    # === Test 5: Store only affects lower 8 bits of memory ===
    # Set 32-bit value in memory, then SB to see if only one byte changes
    li      x3, 0xDEADBEEF
    sw      x3, 0(x10)          # store full word
    li      x4, 0xAA
    sb      x4, 0(x10)          # store only first byte
    lw      x5, 0(x10)          # load back full word
    li      x7, 0xDEADBEAA      # expect other bytes preserved, first changed
    addiw   x7, x7, 0           # sign-extend expected value
    bne     x5, x7, fail

    # === Test 6: Store with different source registers ===
    li      x20, 0x11
    sb      x20, 0(x10)
    lb      x5, 0(x10)
    li      x7, 0x11
    bne     x5, x7, fail

    li      x21, 0x22
    sb      x21, 1(x10)
    lb      x5, 1(x10)
    li      x7, 0x22
    bne     x5, x7, fail

    li      x22, 0x33
    sb      x22, 2(x10)
    lb      x5, 2(x10)
    li      x7, 0x33
    bne     x5, x7, fail

    li      x23, 0x44
    sb      x23, 3(x10)
    lb      x5, 3(x10)
    li      x7, 0x44
    bne     x5, x7, fail

    # === Test 7: Multiple stores to same location ===
    li      x1, 0x11
    sb      x1, 0(x10)
    li      x1, 0x22
    sb      x1, 0(x10)
    li      x1, 0x33
    sb      x1, 0(x10)
    lb      x2, 0(x10)
    li      x7, 0x33            # last value should remain
    bne     x2, x7, fail

    # === Test 8: Store with negative offset ===
    addi    x11, x10, 4         # x11 = test_data + 4
    li      x1, 0x99
    sb      x1, -2(x11)         # store to (x11 - 2) = test_data + 2
    lb      x2, 2(x10)          # verify at correct location
    li      x7, 0x99
    slli    x8, x7, 56
    srai    x8, x8, 56
    bne     x2, x8, fail

    # === Test 9: Verify byte positions in word ===
    li      x1, 0xAABBCCDD
    sw      x1, 0(x10)          # store initial word
    li      x2, 0xFF
    sb      x2, 1(x10)          # store at byte 1
    lw      x3, 0(x10)
    li      x7, 0xAABBFFDD      # only byte 1 changed
    addiw   x7, x7, 0           # sign-extend expected value
    bne     x3, x7, fail

    li      x2, 0xEE
    sb      x2, 2(x10)          # store at byte 2
    lw      x3, 0(x10)
    li      x7, 0xAAEEFFDD      # bytes 1 and 2 changed
    addiw   x7, x7, 0           # sign-extend expected value
    bne     x3, x7, fail

    li      x2, 0xCC
    sb      x2, 3(x10)          # store at byte 3
    lw      x3, 0(x10)
    li      x7, 0xCCEEFFDD      # bytes 1, 2, 3 changed
    addiw   x7, x7, 0           # sign-extend expected value
    bne     x3, x7, fail

    # === Test 10: Store each byte then verify full word ===
    li      x1, 0x12
    sb      x1, 0(x10)
    li      x1, 0x34
    sb      x1, 1(x10)
    li      x1, 0x56
    sb      x1, 2(x10)
    li      x1, 0x78
    sb      x1, 3(x10)
    lw      x2, 0(x10)
    li      x7, 0x78563412      # little-endian result
    bne     x2, x7, fail

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
