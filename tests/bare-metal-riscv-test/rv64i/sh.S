# Test: SH - Store Halfword (16-bit store)
# Tests: SH, address alignment, memory write/read back verification

.section .data
.align 4
test_data:
    .half 0x0000             # scratch area 0
    .half 0x0000             # scratch area 1
    .half 0x0000             # scratch area 2
    .half 0x0000             # scratch area 3

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Store simple positive halfword ===
    li      x1, 0x1234          # x1 = test value
    sh      x1, 0(x10)          # store to *(x10 + 0)
    lh      x2, 0(x10)          # load back (to verify)
    li      x7, 0x1234          # x7 = expected
    bne     x2, x7, fail        # verify

    # === Test 2: Store value with high bit set (0x8000) ===
    li      x1, 0x8000          # x1 = test value
    sh      x1, 2(x10)          # store to *(x10 + 2)
    lh      x2, 2(x10)          # load back
    li      x7, 0x8000          # should be same value
    slli    x8, x7, 48
    srai    x8, x8, 48          # sign-extend expected
    bne     x2, x8, fail

    # === Test 3: Store all 1s (0xFFFF) ===
    li      x1, -1              # x1 = 0xFFFFFFFFFFFFFFFF
    sh      x1, 4(x10)          # store lower 16 bits
    lh      x2, 4(x10)          # load back
    li      x7, -1              # expect -1 (sign-extended)
    bne     x2, x7, fail

    # === Test 4: Store zero ===
    li      x1, 0               # x1 = 0
    sh      x1, 6(x10)          # store
    lh      x2, 6(x10)          # load back
    li      x7, 0               # expect 0
    bne     x2, x7, fail

    # === Test 5: Store only affects lower 16 bits of memory ===
    # Set 32-bit value in memory, then SH to see if only lower 16 bits change
    li      x3, 0xDEADBEEF
    sw      x3, 0(x10)          # store full word
    li      x4, 0x1234
    sh      x4, 0(x10)          # store only lower 16 bits
    lw      x5, 0(x10)          # load back full word
    li      x7, 0xDEAD1234      # expect upper 16 bits preserved, lower changed
    addiw   x7, x7, 0           # sign-extend expected value
    bne     x5, x7, fail

    # === Test 6: Store with different source registers ===
    li      x20, 0xAABB
    sh      x20, 0(x10)
    lh      x5, 0(x10)
    li      x7, 0xAABB
    slli    x8, x7, 48
    srai    x8, x8, 48
    bne     x5, x8, fail

    li      x21, 0x1122
    sh      x21, 2(x10)
    lh      x5, 2(x10)
    li      x7, 0x1122
    bne     x5, x7, fail

    li      x22, 0x3344
    sh      x22, 4(x10)
    lh      x5, 4(x10)
    li      x7, 0x3344
    bne     x5, x7, fail

    # === Test 7: Multiple stores to same location ===
    li      x1, 0x1111
    sh      x1, 0(x10)
    li      x1, 0x2222
    sh      x1, 0(x10)
    li      x1, 0x3333
    sh      x1, 0(x10)
    lh      x2, 0(x10)
    li      x7, 0x3333          # last value should remain
    bne     x2, x7, fail

    # === Test 8: Store with negative offset ===
    addi    x11, x10, 8         # x11 = test_data + 8
    li      x1, 0x9999
    sh      x1, -4(x11)         # store to (x11 - 4) = test_data + 4
    lh      x2, 4(x10)          # verify at correct location
    li      x7, 0x9999
    slli    x8, x7, 48
    srai    x8, x8, 48
    bne     x2, x8, fail

    # === Test 9: Verify halfword in word boundary ===
    li      x1, 0xCAFE
    sh      x1, 2(x10)          # store at offset 2 (within first word)
    lw      x2, 0(x10)          # load full word
    li      x7, 0xCAFE3333      # expect: 0xCAFE at upper 16 bits, 0x3333 at lower (from prev test)
    addiw   x7, x7, 0           # sign-extend expected value
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
