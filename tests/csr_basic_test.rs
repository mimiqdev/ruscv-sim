//! CSR basic functionality tests
//!
//! Tests basic CSR read/write operations and reset values

use ruscv_sim::csr::{machine, supervisor, virtualization, CsrError, CsrFile};

// Machine Mode CSR Tests

#[test]
fn test_mstatus_reset_value() {
    let csr = CsrFile::new(0);
    assert_eq!(csr.read(machine::MSTATUS).unwrap(), 0);
}

#[test]
fn test_misa_reset_value() {
    let csr = CsrFile::new(0);
    let misa = csr.read(machine::MISA).unwrap();
    // RV64IMAC - MXL=10 (64-bit), I=1, M=1, A=1, C=1
    assert_eq!(misa, 0x8000_0000_0010_0100_u64);
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
    csr.write(machine::MIE, 0x888).unwrap();
    assert_eq!(csr.read(machine::MIE).unwrap(), 0x888);
}

#[test]
fn test_mip_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MIP, 0xAAA).unwrap();
    assert_eq!(csr.read(machine::MIP).unwrap(), 0xAAA);
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
    csr.write(machine::MEDELEG, 0x0000_B1FF).unwrap();
    assert_eq!(csr.read(machine::MEDELEG).unwrap(), 0x0000_B1FF);
}

#[test]
fn test_mideleg_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MIDELEG, 0x0222).unwrap();
    assert_eq!(csr.read(machine::MIDELEG).unwrap(), 0x0222);
}

#[test]
fn test_mcounteren_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MCOUNTEREN, 0x0000_0007).unwrap();
    assert_eq!(csr.read(machine::MCOUNTEREN).unwrap(), 0x0000_0007);
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
    assert_eq!(csr.read(supervisor::SSTATUS).unwrap(), 0);
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
    csr.write(supervisor::SIE, 0x222).unwrap();
    assert_eq!(csr.read(supervisor::SIE).unwrap(), 0x222);
}

#[test]
fn test_sip_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SIP, 0x020).unwrap();
    assert_eq!(csr.read(supervisor::SIP).unwrap(), 0x020);
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
    csr.write(supervisor::SCOUNTEREN, 0x7).unwrap();
    assert_eq!(csr.read(supervisor::SCOUNTEREN).unwrap(), 0x7);
}

#[test]
fn test_sscratch_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(supervisor::SSCRATCH, 0x9999_9999).unwrap();
    assert_eq!(csr.read(supervisor::SSCRATCH).unwrap(), 0x9999_9999);
}

// Virtualization CSR Tests

#[test]
fn test_vsstatus_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSSTATUS, 0x1234).unwrap();
    assert_eq!(csr.read(virtualization::VSSTATUS).unwrap(), 0x1234);
}

#[test]
fn test_vsie_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSIE, 0x100).unwrap();
    assert_eq!(csr.read(virtualization::VSIE).unwrap(), 0x100);
}

#[test]
fn test_vstvec_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSTVEC, 0x8010_0000).unwrap();
    assert_eq!(csr.read(virtualization::VSTVEC).unwrap(), 0x8010_0000);
}

#[test]
fn test_vsepc_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSEPC, 0x8020_0000).unwrap();
    assert_eq!(csr.read(virtualization::VSEPC).unwrap(), 0x8020_0000);
}

#[test]
fn test_vscause_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSCAUSE, 0x8000_000D).unwrap();
    assert_eq!(csr.read(virtualization::VSCAUSE).unwrap(), 0x8000_000D);
}

#[test]
fn test_vstval_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSTVAL, 0xABCD_EF01).unwrap();
    assert_eq!(csr.read(virtualization::VSTVAL).unwrap(), 0xABCD_EF01);
}

#[test]
fn test_vsip_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSIP, 0x080).unwrap();
    assert_eq!(csr.read(virtualization::VSIP).unwrap(), 0x080);
}

#[test]
fn test_vsatp_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSATP, 0x8000_5678).unwrap();
    assert_eq!(csr.read(virtualization::VSATP).unwrap(), 0x8000_5678);
}

#[test]
fn test_vsscratch_read_write() {
    let mut csr = CsrFile::new(0);
    csr.write(virtualization::VSSCRATCH, 0x7777_7777).unwrap();
    assert_eq!(csr.read(virtualization::VSSCRATCH).unwrap(), 0x7777_7777);
}

// Read-only bit tests

#[test]
fn test_mstatus_reserved_bits_masked() {
    let mut csr = CsrFile::new(0);
    csr.write(machine::MSTATUS, 0xFFFF_FFFF_FFFF_FFFF).unwrap();
    let value = csr.read(machine::MSTATUS).unwrap();
    // Check that value is masked according to RV64 mstatus writable bits
    // The actual mask is 0x8000_0003_000D_FFEA for RV64
    let rv64_mstatus_mask = 0x8000_0003_000D_FFEA_u64;
    assert_eq!(value, 0xFFFF_FFFF_FFFF_FFFF & rv64_mstatus_mask);
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
