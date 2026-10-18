//! CSR basic functionality tests
//!
//! Tests basic CSR read/write operations and reset values

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::csr::{machine, supervisor, virtualization, CsrError, CsrFile};

// Machine Mode CSR Tests

#[test]
fn test_mstatus_reset_value() {
    let csr = CsrFile::new(0);
    // SXL and UXL read 2; every other field resets to 0.
    assert_eq!(csr.read(machine::MSTATUS).unwrap(), 0x0000_000A_0000_0000);
}

#[test]
fn test_misa_reset_value() {
    let csr = CsrFile::new(0);
    let misa = csr.read(machine::MISA).unwrap();
    // RV64IMA plus S and U; C=0 keeps IALIGN at 32.
    assert_eq!(misa, 0x8000_0000_0014_1101_u64);
    assert_eq!(misa & (1 << 2), 0);
}

#[test]
fn test_minstret_reset_and_machine_read_write() {
    let mut csr = CsrFile::new(0);
    assert_eq!(csr.read(machine::MINSTRET).unwrap(), 0);

    let value = 0xFEDC_BA98_7654_3210;
    csr.write(machine::MINSTRET, value).unwrap();
    assert_eq!(csr.read(machine::MINSTRET).unwrap(), value);
}

#[test]
fn test_minstret_is_machine_only_and_failed_access_has_no_side_effect() {
    let mut csr = CsrFile::new(0);
    let value = 0x0123_4567_89AB_CDEF;
    csr.write(machine::MINSTRET, value).unwrap();

    csr.set_privilege(PrivilegeMode::Supervisor);
    assert!(matches!(
        csr.read(machine::MINSTRET),
        Err(CsrError::PrivilegeViolation(machine::MINSTRET))
    ));
    assert!(matches!(
        csr.write(machine::MINSTRET, 0),
        Err(CsrError::PrivilegeViolation(machine::MINSTRET))
    ));

    csr.set_privilege(PrivilegeMode::Machine);
    assert_eq!(csr.read(machine::MINSTRET).unwrap(), value);
}

#[test]
fn test_misa_c_is_warl_fixed_zero_for_direct_writes_and_helpers() {
    let mut csr = CsrFile::new(0);
    let base = csr.read(machine::MISA).unwrap();

    csr.write(machine::MISA, base | (1 << 2)).unwrap();
    assert_eq!(csr.read(machine::MISA).unwrap() & (1 << 2), 0);

    csr.read_set(machine::MISA, 1 << 2).unwrap();
    assert_eq!(csr.read(machine::MISA).unwrap() & (1 << 2), 0);

    csr.read_clear(machine::MISA, 1 << 2).unwrap();
    assert_eq!(csr.read(machine::MISA).unwrap() & (1 << 2), 0);
    assert_eq!(csr.read(machine::MISA).unwrap(), base);
}

#[test]
fn test_misa_extension_bits_are_read_only() {
    let mut csr = CsrFile::new(0);
    let base = csr.read(machine::MISA).unwrap();
    // Setting F, D, C, H or clearing M, A, I, S, U changes nothing.
    csr.write(machine::MISA, base | 0b1010_1100).unwrap();
    assert_eq!(csr.read(machine::MISA).unwrap(), base);
    csr.write(machine::MISA, 0).unwrap();
    assert_eq!(csr.read(machine::MISA).unwrap(), base);
}

#[test]
fn test_mepc_low_bits_are_warl_zero_for_direct_writes_and_helpers() {
    let mut csr = CsrFile::new(0);

    csr.write(machine::MEPC, 0x1234_5678_9ABC_DEF3).unwrap();
    assert_eq!(csr.read(machine::MEPC).unwrap(), 0x1234_5678_9ABC_DEF0);

    csr.read_set(machine::MEPC, 0b11).unwrap();
    assert_eq!(csr.read(machine::MEPC).unwrap() & 0b11, 0);

    csr.read_clear(machine::MEPC, 0b01).unwrap();
    assert_eq!(csr.read(machine::MEPC).unwrap() & 0b11, 0);
}

#[test]
fn test_mhartid_read() {
    let csr = CsrFile::new(123);
    assert_eq!(csr.read(machine::MHARTID).unwrap(), 123);
}

#[test]
fn test_mhartid_write_fails() {
    let mut csr = CsrFile::new(0);
    let result = csr.write(machine::MHARTID, 999);
    assert!(matches!(result, Err(CsrError::ReadOnly(_))));
}

#[test]
fn test_mie_read_write() {
    let mut csr = CsrFile::new(0);
    // SSIE, STIE, SEIE, MSIE, and MTIE are writable. MEIE stays 0.
    csr.write(machine::MIE, 0x888).unwrap();
    assert_eq!(csr.read(machine::MIE).unwrap(), 0x88);
    csr.write(machine::MIE, u64::MAX).unwrap();
    assert_eq!(csr.read(machine::MIE).unwrap(), 0x2AA);
}

#[test]
fn test_mip_read_write() {
    let mut csr = CsrFile::new(0);
    // SSIP, STIP, and SEIP are writable. MSIP, MTIP, and MEIP stay 0
    // while no machine line is pending.
    csr.write(machine::MIP, u64::MAX).unwrap();
    assert_eq!(csr.read(machine::MIP).unwrap(), 0x222);
}

#[test]
fn test_mtvec_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MTVEC, 0x8000_0000).unwrap();
    assert_eq!(csr.read(machine::MTVEC).unwrap(), 0x8000_0000);
}

#[test]
fn test_mepc_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MEPC, 0x1000_0000).unwrap();
    assert_eq!(csr.read(machine::MEPC).unwrap(), 0x1000_0000);
}

#[test]
fn test_mcause_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MCAUSE, 0x8000_0001).unwrap();
    assert_eq!(csr.read(machine::MCAUSE).unwrap(), 0x8000_0001);
}

#[test]
fn test_mtval_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MTVAL, 0xDEAD_BEEF).unwrap();
    assert_eq!(csr.read(machine::MTVAL).unwrap(), 0xDEAD_BEEF);
}

#[test]
fn test_medeleg_read_write() {
    let mut csr = CsrFile::new(0);
    // No trap delegation: the register is read-only zero.
    csr.write(machine::MEDELEG, 0x0000_B1FF).unwrap();
    assert_eq!(csr.read(machine::MEDELEG).unwrap(), 0);
}

#[test]
fn test_mideleg_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MIDELEG, 0x0222).unwrap();
    assert_eq!(csr.read(machine::MIDELEG).unwrap(), 0);
}

#[test]
fn test_mcounteren_read_write() {
    let mut csr = CsrFile::new(0);
    // Read-only zero in this profile, including bits 63:32.
    csr.write(machine::MCOUNTEREN, u64::MAX).unwrap();
    assert_eq!(csr.read(machine::MCOUNTEREN).unwrap(), 0);
}

#[test]
fn test_mscratch_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MSCRATCH, 0xCAFE_BABE).unwrap();
    assert_eq!(csr.read(machine::MSCRATCH).unwrap(), 0xCAFE_BABE);
}

// Supervisor Mode CSR Tests

#[test]
fn test_sstatus_reset_value() {
    let csr = CsrFile::new(0);
    // UXL is part of the SXLEN-64 sstatus layout and reads 2.
    assert_eq!(
        csr.read(supervisor::SSTATUS).unwrap(),
        0x0000_0002_0000_0000
    );
}

#[test]
fn test_stvec_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::STVEC, 0x8000_0100).unwrap();
    assert_eq!(csr.read(supervisor::STVEC).unwrap(), 0x8000_0100);
}

#[test]
fn test_sepc_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SEPC, 0x8000_2000).unwrap();
    assert_eq!(csr.read(supervisor::SEPC).unwrap(), 0x8000_2000);
}

#[test]
fn test_scause_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SCAUSE, 0x8000_0009).unwrap();
    assert_eq!(csr.read(supervisor::SCAUSE).unwrap(), 0x8000_0009);
}

#[test]
fn test_stval_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::STVAL, 0x1234_5678).unwrap();
    assert_eq!(csr.read(supervisor::STVAL).unwrap(), 0x1234_5678);
}

#[test]
fn test_sie_read_write() {
    let mut csr = CsrFile::new(0);
    // mideleg is 0, so sie is a read-only-zero view of mie.
    csr.write(supervisor::SIE, 0x222).unwrap();
    assert_eq!(csr.read(supervisor::SIE).unwrap(), 0);
    assert_eq!(csr.read(machine::MIE).unwrap(), 0);
    csr.write(machine::MIE, 0x222).unwrap();
    assert_eq!(csr.read(supervisor::SIE).unwrap(), 0);
}

#[test]
fn test_sip_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SIP, 0x020).unwrap();
    assert_eq!(csr.read(supervisor::SIP).unwrap(), 0);
    assert_eq!(csr.read(machine::MIP).unwrap(), 0);
    csr.write(machine::MIP, 0x222).unwrap();
    assert_eq!(csr.read(supervisor::SIP).unwrap(), 0);
}

#[test]
fn test_satp_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SATP, 0x8000_1234).unwrap();
    assert_eq!(csr.read(supervisor::SATP).unwrap(), 0x8000_1234);
}

#[test]
fn test_scounteren_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SCOUNTEREN, u64::MAX).unwrap();
    assert_eq!(csr.read(supervisor::SCOUNTEREN).unwrap(), 0);
}

#[test]
fn test_sscratch_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SSCRATCH, 0x9999_9999).unwrap();
    assert_eq!(csr.read(supervisor::SSCRATCH).unwrap(), 0x9999_9999);
}

// Hypervisor CSRs are absent from this profile.

#[test]
fn test_vs_csrs_are_absent() {
    let mut csr = CsrFile::new(0);
    for addr in [
        virtualization::VSSTATUS,
        virtualization::VSIE,
        virtualization::VSTVEC,
        virtualization::VSSCRATCH,
        virtualization::VSEPC,
        virtualization::VSCAUSE,
        virtualization::VSTVAL,
        virtualization::VSIP,
        virtualization::VSATP,
    ] {
        assert_eq!(csr.read(addr), Err(CsrError::InvalidAddress(addr)));
        assert_eq!(csr.write(addr, 0x1234), Err(CsrError::InvalidAddress(addr)));
    }
}

// Read-only bit tests

#[test]
fn test_mstatus_reserved_bits_masked() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MSTATUS, 0xFFFF_FFFF_FFFF_FFFF).unwrap();
    let value = csr.read(machine::MSTATUS).unwrap();
    // Writable: SIE, MIE, SPIE, MPIE, SPP, MPP, MPRV, SUM, MXR, TVM, TW,
    // TSR.  SXL and UXL read 2.  SD, FS, VS, XS, UBE, SBE, MBE read 0.
    assert_eq!(value, 0x0000_000A_007E_19AA);
}

#[test]
fn test_mstatus_mpp_rejects_reserved_value() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MSTATUS, 1 << 11).unwrap();
    // MPP = 2 is reserved: the field keeps its previous legal value (S).
    csr.write(machine::MSTATUS, 2 << 11).unwrap();
    assert_eq!(csr.read(machine::MSTATUS).unwrap() & (3 << 11), 1 << 11);
    csr.write(machine::MSTATUS, 3 << 11).unwrap();
    assert_eq!(csr.read(machine::MSTATUS).unwrap() & (3 << 11), 3 << 11);
    csr.write(machine::MSTATUS, 0).unwrap();
    assert_eq!(csr.read(machine::MSTATUS).unwrap() & (3 << 11), 0);
}

#[test]
fn test_sstatus_is_a_view_of_mstatus() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SSTATUS, u64::MAX).unwrap();
    // SIE, SPIE, SPP, SUM, MXR, plus read-only UXL = 2.  FS and SD read 0.
    assert_eq!(
        csr.read(supervisor::SSTATUS).unwrap(),
        0x0000_0002_000C_0122
    );
    // The write reached mstatus, and fields outside sstatus did not change.
    assert_eq!(csr.read(machine::MSTATUS).unwrap(), 0x0000_000A_000C_0122);

    csr.write(machine::MSTATUS, 0x0000_000A_0070_19AA).unwrap();
    csr.write(supervisor::SSTATUS, 0).unwrap();
    // TVM, TW, TSR, MPP, MPIE, MIE survive an sstatus write.
    assert_eq!(csr.read(machine::MSTATUS).unwrap(), 0x0000_000A_0070_1888);
    // An mstatus write is visible through sstatus.
    csr.write(machine::MSTATUS, 0x0000_000A_0000_0002).unwrap();
    assert_eq!(
        csr.read(supervisor::SSTATUS).unwrap(),
        0x0000_0002_0000_0002
    );
}

#[test]
fn test_epc_low_bits_and_tvec_mode_are_warl() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SEPC, 0x8000_2003).unwrap();
    assert_eq!(csr.read(supervisor::SEPC).unwrap(), 0x8000_2000);

    for tvec in [machine::MTVEC, supervisor::STVEC] {
        csr.write(tvec, 0x8000_0101).unwrap();
        assert_eq!(csr.read(tvec).unwrap(), 0x8000_0101);
        // MODE 2 and 3 are reserved: MODE keeps the old value, BASE changes.
        csr.write(tvec, 0x8000_0202).unwrap();
        assert_eq!(csr.read(tvec).unwrap(), 0x8000_0201);
        csr.write(tvec, 0x8000_0300).unwrap();
        csr.write(tvec, 0x8000_0403).unwrap();
        assert_eq!(csr.read(tvec).unwrap(), 0x8000_0400);
    }
}

#[test]
fn test_satp_accepts_bare_and_sv39_and_ignores_other_modes() {
    let mut csr = CsrFile::new(0);
    let sv39 = (8u64 << 60) | 0x8_0000;
    csr.write(supervisor::SATP, sv39).unwrap();
    assert_eq!(csr.read(supervisor::SATP).unwrap(), sv39);
    // ASIDLEN is 0, so the ASID field reads 0.
    csr.write(supervisor::SATP, sv39 | (0xFFFF << 44)).unwrap();
    assert_eq!(csr.read(supervisor::SATP).unwrap(), sv39);
    // Sv48, Sv57, and reserved modes leave the whole register unchanged.
    for mode in [1u64, 9, 10, 15] {
        csr.write(supervisor::SATP, (mode << 60) | 0x1234).unwrap();
        assert_eq!(csr.read(supervisor::SATP).unwrap(), sv39);
    }
    csr.write(supervisor::SATP, 0).unwrap();
    assert_eq!(csr.read(supervisor::SATP).unwrap(), 0);
}

#[test]
fn test_envcfg_only_fiom_is_writable() {
    let mut csr = CsrFile::new(0);
    for addr in [machine::MENVCFG, supervisor::SENVCFG] {
        assert_eq!(csr.read(addr).unwrap(), 0);
        csr.write(addr, u64::MAX).unwrap();
        assert_eq!(csr.read(addr).unwrap(), 1);
        csr.write(addr, 0).unwrap();
        assert_eq!(csr.read(addr).unwrap(), 0);
    }
}

#[test]
fn test_pmp_csrs_are_read_only_zero_and_odd_pmpcfg_is_absent() {
    let mut csr = CsrFile::new(0);
    for addr in [0x3A0u16, 0x3AE, 0x3B0, 0x3EF] {
        csr.write(addr, u64::MAX).unwrap();
        assert_eq!(csr.read(addr).unwrap(), 0);
    }
    for addr in [0x3A1u16, 0x3AF] {
        assert_eq!(csr.read(addr), Err(CsrError::InvalidAddress(addr)));
        assert_eq!(csr.write(addr, 1), Err(CsrError::InvalidAddress(addr)));
    }
}

#[test]
fn test_identity_registers_read_zero_and_reject_writes() {
    let mut csr = CsrFile::new(0);
    for addr in [
        machine::MVENDORID,
        machine::MARCHID,
        machine::MIMPID,
        machine::MCONFIGPTR,
    ] {
        assert_eq!(csr.read(addr).unwrap(), 0);
        assert_eq!(csr.write(addr, 1), Err(CsrError::ReadOnly(addr)));
        assert_eq!(csr.read(addr).unwrap(), 0);
    }
}

#[test]
fn test_hpm_counters_and_events_are_read_only_zero() {
    let mut csr = CsrFile::new(0);
    for addr in [0xB03u16, 0xB1F, 0x323, 0x33F] {
        csr.write(addr, u64::MAX).unwrap();
        assert_eq!(csr.read(addr).unwrap(), 0);
    }
    // hpmcounter shadows stay absent. time is the read-only mtime shadow.
    assert_eq!(csr.read(0xC01).unwrap(), 0);
    assert_eq!(csr.write(0xC01, 1), Err(CsrError::ReadOnly(0xC01)));
    for addr in [0xC03u16, 0xC1F] {
        assert_eq!(csr.read(addr), Err(CsrError::InvalidAddress(addr)));
    }
}

#[test]
fn test_fp_csr_addresses_are_absent() {
    let mut csr = CsrFile::new(0);
    for addr in [0x001u16, 0x002, 0x003] {
        assert_eq!(csr.read(addr), Err(CsrError::InvalidAddress(addr)));
        assert_eq!(csr.write(addr, 0), Err(CsrError::InvalidAddress(addr)));
    }
}

// Multiple CSR access tests

#[test]
fn test_multiple_csr_access() {
    let mut csr = CsrFile::new(42);

    // Write multiple CSRs
    csr.write(machine::MEPC, 0x1000).unwrap();
    csr.write(machine::MCAUSE, 0x2000).unwrap();
    csr.write(machine::MTVAL, 0x3000).unwrap();

    // Verify all values
    assert_eq!(csr.read(machine::MEPC).unwrap(), 0x1000);
    assert_eq!(csr.read(machine::MCAUSE).unwrap(), 0x2000);
    assert_eq!(csr.read(machine::MTVAL).unwrap(), 0x3000);
    assert_eq!(csr.read(machine::MHARTID).unwrap(), 42);
}
