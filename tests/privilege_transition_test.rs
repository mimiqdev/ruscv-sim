//! Privilege mode transition tests
//!
//! Tests privilege mode changes and CSR access control

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::csr::{machine, supervisor, CsrError, CsrFile};

// Privilege mode tests

#[test]
fn test_initial_privilege_mode() {
    let csr = CsrFile::new(0);
    assert_eq!(csr.get_privilege(), PrivilegeMode::Machine);
}

#[test]
fn test_set_privilege_mode() {
    let mut csr = CsrFile::new(0);

    csr.set_privilege(PrivilegeMode::Supervisor);
    assert_eq!(csr.get_privilege(), PrivilegeMode::Supervisor);

    csr.set_privilege(PrivilegeMode::User);
    assert_eq!(csr.get_privilege(), PrivilegeMode::User);

    csr.set_privilege(PrivilegeMode::Machine);
    assert_eq!(csr.get_privilege(), PrivilegeMode::Machine);
}

// Machine mode can access all CSRs

#[test]
fn test_machine_mode_access_machine_csr() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::Machine);

    // 0x1220 sets SPIE (5), VS bit 9, and MPP = 2.  VS is read-only 0, the
    // reserved MPP = 2 keeps the old MPP, and SXL and UXL read 2.
    csr.write(machine::MSTATUS, 0x1220).unwrap();
    assert_eq!(csr.read(machine::MSTATUS).unwrap(), 0x0000_000A_0000_0020);
}

#[test]
fn test_machine_mode_access_supervisor_csr() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::Machine);

    // Only SPIE (5) of 0x5678 is a writable sstatus field; UXL reads 2.
    csr.write(supervisor::SSTATUS, 0x5678).unwrap();
    assert_eq!(
        csr.read(supervisor::SSTATUS).unwrap(),
        0x0000_0002_0000_0020
    );
}

// Supervisor mode can access supervisor CSRs but not machine CSRs

#[test]
fn test_supervisor_mode_access_supervisor_csr() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::Supervisor);

    // Only SPP (8) of 0xABCD is a writable sstatus field; UXL reads 2.
    csr.write(supervisor::SSTATUS, 0xABCD).unwrap();
    assert_eq!(
        csr.read(supervisor::SSTATUS).unwrap(),
        0x0000_0002_0000_0100
    );
}

#[test]
fn test_supervisor_mode_cannot_access_machine_csr_read() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::Supervisor);

    let result = csr.read(machine::MSTATUS);
    assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
}

#[test]
fn test_supervisor_mode_cannot_access_machine_csr_write() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::Supervisor);

    let result = csr.write(machine::MSTATUS, 0x1234);
    assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
}

// User mode cannot access supervisor or machine CSRs

#[test]
fn test_user_mode_cannot_access_supervisor_csr_read() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::User);

    let result = csr.read(supervisor::SSTATUS);
    assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
}

#[test]
fn test_user_mode_cannot_access_supervisor_csr_write() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::User);

    let result = csr.write(supervisor::SSTATUS, 0x5678);
    assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
}

#[test]
fn test_user_mode_cannot_access_machine_csr_read() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::User);

    let result = csr.read(machine::MSTATUS);
    assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
}

#[test]
fn test_user_mode_cannot_access_machine_csr_write() {
    let mut csr = CsrFile::new(0);
    csr.set_privilege(PrivilegeMode::User);

    let result = csr.write(machine::MSTATUS, 0xABCD);
    assert!(matches!(result, Err(CsrError::PrivilegeViolation(_))));
}

// Privilege escalation tests

#[test]
fn test_escalate_from_user_to_supervisor() {
    let mut csr = CsrFile::new(0);

    // Start in user mode
    csr.set_privilege(PrivilegeMode::User);
    assert!(csr.read(supervisor::SSTATUS).is_err());

    // Escalate to supervisor (simulating trap handler)
    csr.set_privilege(PrivilegeMode::Supervisor);
    assert!(csr.read(supervisor::SSTATUS).is_ok());
}

#[test]
fn test_escalate_from_supervisor_to_machine() {
    let mut csr = CsrFile::new(0);

    // Start in supervisor mode
    csr.set_privilege(PrivilegeMode::Supervisor);
    assert!(csr.read(machine::MSTATUS).is_err());

    // Escalate to machine (simulating trap handler)
    csr.set_privilege(PrivilegeMode::Machine);
    assert!(csr.read(machine::MSTATUS).is_ok());
}

// Delegation tests (conceptual - actual delegation logic would be in trap handler)

#[test]
fn test_medeleg_delegation_register() {
    let mut csr = CsrFile::new(0);

    // No trap delegation in this profile: medeleg is read-only zero, and
    // writing ones then reading back is how software discovers that.
    csr.write(machine::MEDELEG, 0x0000_B1FF).unwrap();
    assert_eq!(csr.read(machine::MEDELEG).unwrap(), 0);

    let old = csr.read_set(machine::MEDELEG, 0x00FF).unwrap();
    assert_eq!(old, 0);
    assert_eq!(csr.read(machine::MEDELEG).unwrap(), 0);
}

#[test]
fn test_mideleg_delegation_register() {
    let mut csr = CsrFile::new(0);

    csr.write(machine::MIDELEG, 0x0222).unwrap();
    assert_eq!(csr.read(machine::MIDELEG).unwrap(), 0);
}

// Counter enable tests

#[test]
fn test_mcounteren() {
    let mut csr = CsrFile::new(0);

    // Read-only zero in this profile.
    csr.write(machine::MCOUNTEREN, 0x7).unwrap();
    assert_eq!(csr.read(machine::MCOUNTEREN).unwrap(), 0);
}

#[test]
fn test_scounteren() {
    let mut csr = CsrFile::new(0);

    csr.write(supervisor::SCOUNTEREN, 0x7).unwrap();
    assert_eq!(csr.read(supervisor::SCOUNTEREN).unwrap(), 0);
}

// Trap vector tests

#[test]
fn test_mtvec_configuration() {
    let mut csr = CsrFile::new(0);

    // Direct mode (bit [1:0] = 00)
    csr.write(machine::MTVEC, 0x8000_0000).unwrap();
    assert_eq!(csr.read(machine::MTVEC).unwrap() & 0b11, 0);

    // Vectored mode (bit [1:0] = 01)
    csr.write(machine::MTVEC, 0x8000_0001).unwrap();
    assert_eq!(csr.read(machine::MTVEC).unwrap() & 0b11, 1);
}

#[test]
fn test_stvec_configuration() {
    let mut csr = CsrFile::new(0);

    csr.write(supervisor::STVEC, 0x8000_1000).unwrap();
    assert_eq!(csr.read(supervisor::STVEC).unwrap(), 0x8000_1000);
}
