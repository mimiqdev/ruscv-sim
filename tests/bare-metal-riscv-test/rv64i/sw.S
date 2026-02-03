# Test: SW - Store Word (32-bit store)
# Tests: SW, address alignment, memory write/read back verification

.section .data
.align 4
test_data:
    .word 0x00000000       # scratch area 0
    .word 0x00000000       # scratch area 1
    .word 0x00000000       # scratch area 2
    .word 0x00000000       # scratch area 3

.section .text
.globl _start

_start:
    # Get base address of test data
    la      x10, test_data

    # === Test 1: Store simple positive value ===
    li      x1, 0x12345678      # x1 = test value
    sw      x1, 0(x10)          # store to *(x10 + 0)
    lw      x2, 0(x10)          # load back (to verify)
    li      x7, 0x12345678      # x7 = expected
    bne     x2, x7, fail        # verify

    # === Test 2: Store value that requires sign extension on load ===
    li      x1, 0x80000000      # x1 = test value (high bit set)
    sw      x1, 4(x10)          # store to *(x10 + 4)
    lw      x2, 4(x10)          # load back
    li      x7, 0x80000000      # should be same value
    addiw   x8, x7, 0           # sign-extend expected
    bne     x2, x8, fail

    # === Test 3: Store all 1s (0xFFFFFFFF) ===
    li      x1, -1              # x1 = 0xFFFFFFFFFFFFFFFF
    sw      x1, 8(x10)          # store lower 32 bits
    lw      x2, 8(x10)          # load back
    li      x7, -1              # expect -1 (sign-extended)
    bne     x2, x7, fail

    # === Test 4: Store zero ===
    li      x1, 0               # x1 = 0
    sw      x1, 12(x10)         # store
    lw      x2, 12(x10)         # load back
    li      x7, 0               # expect 0
    bne     x2, x7, fail

    # === Test 5: Store only affects lower 32 bits of memory ===
    # First, set 64-bit value in memory (using SD via manual store)
    li      x3, 0xDEADBEEFCAFEBABE  # large 64-bit value
    sw      x3, 0(x10)          # store only lower 32 bits (0xCAFEBABE)
    lw      x4, 0(x10)          # load back
    li      x7, 0xCAFEBABE      # expect only lower 32 bits
    bne     x4, x7, fail

    # === Test 6: Store with different source registers ===
    li      x20, 0xAABBCCDD
    sw      x20, 0(x10)
    lw      x5, 0(x10)
    li      x7, 0xAABBCCDD
    bne     x5, x7, fail

    li      x21, 0x11223344
    sw      x21, 4(x10)
    lw      x5, 4(x10)
    li      x7, 0x11223344
    bne     x5, x7, fail

    li      x22, 0x55667788
    sw      x22, 8(x10)
    lw      x5, 8(x10)
    li      x7, 0x55667788
    bne     x5, x7, fail

    # === Test 7: Multiple stores to same location ===
    li      x1, 0x11111111
    sw      x1, 0(x10)
    li      x1, 0x22222222
    sw      x1, 0(x10)
    li      x1, 0x33333333
    sw      x1, 0(x10)
    lw      x2, 0(x10)
    li      x7, 0x33333333      # last value should remain
    bne     x2, x7, fail

    # === Test 8: Store with negative offset ===
    addi    x11, x10, 8         # x11 = test_data + 8
    li      x1, 0x99998888
    sw      x1, -4(x11)         # store to (x11 - 4) = test_data + 4
    lw      x2, 4(x10)          # verify at correct location
    li      x7, 0x99998888
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
