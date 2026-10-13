# Stage 4 CSR field-policy guest (dev-plan §4 "CSR field policy", M2).
#
# Each row of the M2 table is written with its illegal value and read back:
# mstatus rejects MPP=2 and keeps FS/VS/XS/SD/UBE at 0 with SXL/UXL=2;
# sstatus shows the same S-visible result; misa never changes; sepc and
# mepc keep bits 1:0 clear; stvec and mtvec keep MODE in {0,1};
# mcounteren and scounteren are 32-bit; satp keeps only Bare/Sv39 MODEs
# with ASID read-only 0; menvcfg and senvcfg keep only FIOM; mip and mie
# keep only bits 1, 5, and 9 (mask 0x222).  A write raises no trap.

.section .text
.globl _start

_start:
    la      t1, tohost

    # ---- mstatus ----
    li      t0, (2 << 11) | (1 << 14) | (1 << 63) | (1 << 6) | (3 << 9) | (3 << 15)
    csrw    mstatus, t0                 # MPP=2, FS=1, SD, UBE, VS=3, XS=3
    csrr    t2, mstatus
    # SXL(35:34)=2, UXL(33:32)=2 always present
    li      t3, 0xA00000000
    and     t4, t2, t3
    bne     t4, t3, fail
    # SD(63)=0, UBE(6)=0, VS(9:10)=0, FS(13:14)=0, XS(15:16)=0
    li      t3, 0x8000000000000040 | (3<<9) | (3<<13) | (3<<15)
    and     t4, t2, t3
    bnez    t4, fail
    # MPP must not be 2; write left it at reset (0) since 2 is illegal
    srli    t4, t2, 11
    andi    t4, t4, 3
    li      t5, 2
    beq     t4, t5, fail

    # MPP=1 (S) and MPP=3 (M) are legal and stick.
    li      t0, (1 << 11)
    csrw    mstatus, t0
    csrr    t2, mstatus
    srli    t4, t2, 11
    andi    t4, t4, 3
    li      t5, 1
    bne     t4, t5, fail
    li      t0, (3 << 11)
    csrw    mstatus, t0
    csrr    t2, mstatus
    srli    t4, t2, 11
    andi    t4, t4, 3
    li      t5, 3
    bne     t4, t5, fail

    # ---- sstatus mirrors the S-visible fields and cannot set M-only ones
    li      t0, -1
    csrw    sstatus, t0
    csrr    t2, sstatus
    # UXL=2 present, SPP=1, SIE/SPIE/SUM/MXR set, everything else 0
    li      t3, 0x200000000             # UXL=2 (bits 33:32 = binary 10)
    li      t4, 0xC0122                 # SIE|SPIE|SPP|SUM|MXR
    or      t3, t3, t4
    and     t5, t2, t3
    bne     t5, t3, fail
    # SD, FS, VS, XS, UBE read 0 through sstatus too
    li      t3, 0x8000000000000040 | (3<<9) | (3<<13) | (3<<15)
    and     t5, t2, t3
    bnez    t5, fail
    # M-only fields are unchanged by the sstatus write: MPP is still 3
    # from the mstatus block, and TVM/TSR/TW/MPRV/MPIE/MIE stayed 0.
    csrr    t2, mstatus
    srli    t5, t2, 11
    andi    t5, t5, 3
    li      t4, 3
    bne     t5, t4, fail
    li      t3, (1<<20)|(1<<21)|(1<<22)|(1<<17)|(1<<7)|(1<<3)
    and     t5, t2, t3
    bnez    t5, fail

    # ---- misa: no write changes any bit ----
    li      t0, 0x8000000000141101
    csrr    t2, misa
    bne     t2, t0, fail
    li      t0, -1
    csrw    misa, t0
    li      t0, 0x8000000000141101
    csrr    t2, misa
    bne     t2, t0, fail

    # ---- sepc / mepc bits 1:0 read 0 ----
    li      t0, -1
    csrw    sepc, t0
    csrr    t2, sepc
    andi    t3, t2, 3
    bnez    t3, fail
    csrw    mepc, t0
    csrr    t2, mepc
    andi    t3, t2, 3
    bnez    t3, fail

    # ---- stvec / mtvec MODE keeps 0 or 1, BASE writable ----
    li      t0, 0x123 << 2              # BASE with MODE=0
    csrw    stvec, t0
    csrr    t2, stvec
    li      t3, 0x123 << 2
    bne     t2, t3, fail
    li      t0, (0x123 << 2) | 2        # MODE=2 rejected
    csrw    stvec, t0
    csrr    t2, stvec
    andi    t3, t2, 3
    bnez    t3, fail                    # MODE stays 0
    li      t0, (0x123 << 2) | 1        # MODE=1 legal
    csrw    stvec, t0
    csrr    t2, stvec
    andi    t3, t2, 3
    li      t4, 1
    bne     t3, t4, fail
    li      t0, (0x123 << 2) | 3        # MODE=3 leaves MODE unchanged (1)
    csrw    stvec, t0
    csrr    t2, stvec
    andi    t3, t2, 3
    li      t4, 1
    bne     t3, t4, fail
    # mtvec: same rule, and restore a sane vector before any trap use
    li      t0, (0x100 << 2) | 0
    csrw    mtvec, t0
    csrr    t2, mtvec
    andi    t3, t2, 3
    bnez    t3, fail

    # ---- mcounteren / scounteren are 32-bit ----
    li      t0, -1
    csrw    mcounteren, t0
    csrr    t2, mcounteren
    srli    t3, t2, 32
    bnez    t3, fail
    csrw    scounteren, t0
    csrr    t2, scounteren
    srli    t3, t2, 32
    bnez    t3, fail
    # In this profile both are read-only 0 even in the low 32 bits.
    bnez    t2, fail

    # ---- satp: unsupported MODE ignored in full; ASID reads 0 ----
    li      t0, 9 << 60                 # Sv48
    csrw    satp, t0
    csrr    t2, satp
    bnez    t2, fail                    # whole write ignored, satp stays 0
    li      t0, 0x12345                 # Bare MODE 0 with PPN
    csrw    satp, t0
    csrr    t2, satp
    li      t4, 0x12345
    bne     t2, t4, fail                # Bare write sticks (PPN writable)
    li      t0, (0x123 << 44) | (9 << 60) | 0x54321   # Sv48 + ASID + PPN
    csrw    satp, t0
    csrr    t2, satp
    li      t4, 0x12345
    bne     t2, t4, fail           # ignored in full again
    # ASID is read-only 0 on a legal write: MODE=0, ASID=0xffff, PPN=x
    li      t0, (0xffff << 44) | 0x54321
    csrw    satp, t0
    csrr    t2, satp
    li      t3, 0xffff << 44
    and     t4, t2, t3
    bnez    t4, fail
    li      t4, 0x54321
    bne     t2, t4, fail           # PPN kept
    csrw    satp, x0                    # back to Bare 0

    # ---- menvcfg / senvcfg: FIOM only ----
    li      t0, -1
    csrw    0x30A, t0
    csrr    t2, 0x30A
    li      t3, 1
    bne     t2, t3, fail
    csrw    0x10A, t0
    csrr    t2, 0x10A
    li      t3, 1
    bne     t2, t3, fail

    # ---- mip / mie keep only 0x222 ----
    li      t0, -1
    csrw    mie, t0
    csrr    t2, mie
    li      t3, 0x222
    bne     t2, t3, fail
    csrw    mip, t0
    csrr    t2, mip
    li      t3, 0x222
    bne     t2, t3, fail
    # Clear them again so no interrupt is left pending.
    csrw    mie, x0
    csrw    mip, x0

    li      a0, 0
    j       write_exit

fail:
    li      a0, 1

write_exit:
    li      t1, 1
    slli    t1, t1, 63
    or      a0, a0, t1
    la      t1, tohost
    sd      a0, 0(t1)

hang:
    j       hang

.section .tohost
.align 3
tohost:
    .dword 0
