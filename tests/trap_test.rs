//! Trap handling tests
//!
//! Tests for trap handling framework including:
//! - Exception handling
//! - Interrupt handling
//! - MRET/SRET instructions
//! - CSR trap register interactions

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::core::{ExceptionCause, InterruptCause, Trap, TrapDelegation, TrapHandler};
use ruscv_sim::csr::machine;
use ruscv_sim::csr::supervisor;
use ruscv_sim::csr::CsrFile;

// Helper function to create a trap context wrapper
struct TestTrapContext {
    csr: CsrFile,
    privilege: PrivilegeMode,
}

impl TestTrapContext {
    fn new() -> Self {
        Self {
            csr: CsrFile::new(0),
            privilege: PrivilegeMode::Machine,
        }
    }

    fn privilege(&self) -> PrivilegeMode {
        self.privilege
    }
}

// ========================================
// Exception Cause Tests
// ========================================

#[test]
fn test_exception_cause_codes() {
    assert_eq!(ExceptionCause::InstructionAddressMisaligned.code(), 0);
    assert_eq!(ExceptionCause::IllegalInstruction.code(), 2);
    assert_eq!(ExceptionCause::Breakpoint.code(), 3);
    assert_eq!(ExceptionCause::LoadAddressMisaligned.code(), 4);
    assert_eq!(ExceptionCause::EcallM.code(), 11);
    assert_eq!(ExceptionCause::InstructionPageFault.code(), 12);
}

#[test]
fn test_exception_is_access_fault() {
    assert!(ExceptionCause::InstructionAccessFault.is_access_fault());
    assert!(ExceptionCause::LoadAccessFault.is_access_fault());
    assert!(ExceptionCause::StoreAccessFault.is_access_fault());
    assert!(!ExceptionCause::IllegalInstruction.is_access_fault());
    assert!(!ExceptionCause::EcallM.is_access_fault());
}

#[test]
fn test_exception_is_page_fault() {
    assert!(ExceptionCause::InstructionPageFault.is_page_fault());
    assert!(ExceptionCause::LoadPageFault.is_page_fault());
    assert!(ExceptionCause::StorePageFault.is_page_fault());
    assert!(!ExceptionCause::IllegalInstruction.is_page_fault());
}

#[test]
fn test_exception_is_ecall() {
    assert!(ExceptionCause::EcallU.is_ecall());
    assert!(ExceptionCause::EcallS.is_ecall());
    assert!(ExceptionCause::EcallM.is_ecall());
    assert!(!ExceptionCause::IllegalInstruction.is_ecall());
}

// ========================================
// Interrupt Cause Tests
// ========================================

#[test]
fn test_interrupt_cause_codes() {
    // Interrupt codes have bit 63 set (sign bit in 64-bit representation)
    assert_eq!(
        InterruptCause::MachineSoftware.code(),
        0x8000_0000_0000_0003
    );
    assert_eq!(InterruptCause::MachineTimer.code(), 0x8000_0000_0000_0007);
    assert_eq!(
        InterruptCause::MachineExternal.code(),
        0x8000_0000_0000_000B
    );
    assert_eq!(
        InterruptCause::SupervisorSoftware.code(),
        0x8000_0000_0000_0001
    );
    assert_eq!(
        InterruptCause::SupervisorTimer.code(),
        0x8000_0000_0000_0005
    );
}

#[test]
fn test_interrupt_is_machine_mode() {
    assert!(InterruptCause::MachineSoftware.is_machine_mode());
    assert!(InterruptCause::MachineTimer.is_machine_mode());
    assert!(InterruptCause::MachineExternal.is_machine_mode());
    assert!(!InterruptCause::SupervisorSoftware.is_machine_mode());
}

#[test]
fn test_interrupt_is_supervisor_mode() {
    assert!(InterruptCause::SupervisorSoftware.is_supervisor_mode());
    assert!(InterruptCause::SupervisorTimer.is_supervisor_mode());
    assert!(InterruptCause::SupervisorExternal.is_supervisor_mode());
    assert!(!InterruptCause::MachineSoftware.is_supervisor_mode());
}

// ========================================
// Trap Delegation Tests
// ========================================

#[test]
fn test_trap_delegation_default() {
    let delegation = TrapDelegation::new();
    assert!(!delegation.should_delegate_exception(ExceptionCause::IllegalInstruction));
    assert!(!delegation.should_delegate_interrupt(InterruptCause::SupervisorSoftware));
}

#[test]
fn test_trap_delegation_exception() {
    let mut delegation = TrapDelegation::new();

    delegation.delegate_exception(ExceptionCause::IllegalInstruction);
    delegation.delegate_exception(ExceptionCause::EcallU);

    assert!(delegation.should_delegate_exception(ExceptionCause::IllegalInstruction));
    assert!(delegation.should_delegate_exception(ExceptionCause::EcallU));
    assert!(!delegation.should_delegate_exception(ExceptionCause::StoreAddressMisaligned));
}

#[test]
fn test_trap_delegation_interrupt() {
    let mut delegation = TrapDelegation::new();

    delegation.delegate_interrupt(InterruptCause::SupervisorSoftware);
    delegation.delegate_interrupt(InterruptCause::SupervisorTimer);

    assert!(delegation.should_delegate_interrupt(InterruptCause::SupervisorSoftware));
    assert!(delegation.should_delegate_interrupt(InterruptCause::SupervisorTimer));
    assert!(!delegation.should_delegate_interrupt(InterruptCause::MachineSoftware));
}

#[test]
fn test_trap_delegation_bits() {
    let mut delegation = TrapDelegation::new();

    // Medeleg: bits 0-15 for different exception types
    delegation.delegate_exception(ExceptionCause::IllegalInstruction);
    assert_eq!(delegation.medeleg, 1 << 2);

    delegation.delegate_exception(ExceptionCause::EcallU);
    assert_eq!(delegation.medeleg, (1 << 2) | (1 << 8));
}

// ========================================
// Trap Handler Tests
// ========================================

#[test]
fn test_trap_handler_creation() {
    let handler = TrapHandler::new();
    assert!(!handler
        .delegation()
        .should_delegate_exception(ExceptionCause::IllegalInstruction));
}

#[test]
fn test_vector_trap_direct_mode() {
    let handler = TrapHandler::new();
    let tvec = 0x8000_0000; // Direct mode (bits [1:0] = 00)

    // Any cause should go to base address
    assert_eq!(handler.vector_trap(tvec, 0), 0x8000_0000);
    assert_eq!(handler.vector_trap(tvec, 7), 0x8000_0000);
    assert_eq!(
        handler.vector_trap(tvec, 0x8000_0000_0000_0003),
        0x8000_0000
    );
}

#[test]
fn test_vector_trap_vectored_mode() {
    let handler = TrapHandler::new();
    let tvec = 0x8000_0001; // Vectored mode (bits [1:0] = 01)

    // Cause should be masked to 7 bits and multiplied by 4
    assert_eq!(handler.vector_trap(tvec, 0), 0x8000_0000);
    assert_eq!(handler.vector_trap(tvec, 1), 0x8000_0004);
    assert_eq!(handler.vector_trap(tvec, 7), 0x8000_001C);
    assert_eq!(
        handler.vector_trap(tvec, 0x8000_0000_0000_0003),
        0x8000_000C
    );
}

#[test]
fn test_vector_trap_vectored_mode_wrapping() {
    let handler = TrapHandler::new();
    let tvec = 0xFFFF_FFFC; // Vectored mode near end of address space

    // Test wrapping behavior
    let result = handler.vector_trap(tvec, 100);
    // Should wrap around
    assert!(result < 0xFFFF_FFFC);
}

// ========================================
// Trap Exception Handling Tests
// ========================================

#[test]
fn test_handle_illegal_instruction_exception() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    let new_pc = handler.handle_trap(trap, 0x1000, 0xBAD0_1234, &mut context);

    // Should vector to mtvec base (0 by default)
    assert_eq!(new_pc, 0);

    // Check that MEPC was set
    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x1000);

    // Check that MCAUSE was set
    assert_eq!(context.csr.read(machine::MCAUSE).unwrap(), 2);

    // Check that MTVAL was set
    assert_eq!(context.csr.read(machine::MTVAL).unwrap(), 0xBAD0_1234);
}

#[test]
fn test_handle_ecall_exception() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Exception(ExceptionCause::EcallM);
    let new_pc = handler.handle_trap(trap, 0x2000, 0, &mut context);

    assert_eq!(new_pc, 0);
    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x2000);
    assert_eq!(context.csr.read(machine::MCAUSE).unwrap(), 11);
}

#[test]
fn test_handle_exception_updates_mstatus() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    // Set up initial mstatus
    context.csr.write(machine::MSTATUS, 0x0000_0008).unwrap(); // MIE = 1

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    handler.handle_trap(trap, 0x1000, 0, &mut context);

    // Check that mstatus was updated: MPIE = old MIE, MIE = 0, MPP = Machine
    let mstatus = context.csr.read(machine::MSTATUS).unwrap();
    assert_eq!((mstatus >> 7) & 1, 1); // MPIE = 1
    assert_eq!((mstatus >> 3) & 1, 0); // MIE = 0
    assert_eq!((mstatus >> 11) & 0b11, 3); // MPP = 11 (Machine)
}

// ========================================
// Trap Interrupt Handling Tests
// ========================================

#[test]
fn test_handle_machine_timer_interrupt() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Interrupt(InterruptCause::MachineTimer);
    let new_pc = handler.handle_trap(trap, 0x1000, 0, &mut context);

    assert_eq!(new_pc, 0);

    // Check that MCAUSE has interrupt bit set
    let mcause = context.csr.read(machine::MCAUSE).unwrap();
    assert!(mcause & 0x8000_0000 != 0); // Interrupt bit
    assert_eq!(mcause & 0x7FFF_FFFF, 7); // Machine Timer cause
}

#[test]
fn test_handle_machine_external_interrupt() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Interrupt(InterruptCause::MachineExternal);
    let new_pc = handler.handle_trap(trap, 0x3000, 0, &mut context);

    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x3000);

    let mcause = context.csr.read(machine::MCAUSE).unwrap();
    assert_eq!(mcause & 0x7FFF_FFFF, 11); // Machine External cause
}

// ========================================
// Delegated Trap Handling Tests
// ========================================

#[test]
fn test_delegated_exception_to_supervisor() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    // Delegate illegal instruction to supervisor
    handler
        .delegation_mut()
        .delegate_exception(ExceptionCause::IllegalInstruction);

    // Set up supervisor CSRs
    context.csr.write(supervisor::STVEC, 0x4000_0000).unwrap();

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    let new_pc = handler.handle_trap(trap, 0x1000, 0xBAD, &mut context);

    // Should vector to stvec
    assert_eq!(new_pc, 0x4000_0000);

    // Check that supervisor CSRs were updated
    assert_eq!(context.csr.read(supervisor::SEPC).unwrap(), 0x1000);
    assert_eq!(context.csr.read(supervisor::SCAUSE).unwrap(), 2);

    // Check privilege mode
    assert_eq!(context.privilege, PrivilegeMode::Supervisor);
}

#[test]
fn test_delegated_interrupt_to_supervisor() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    // Delegate supervisor timer interrupt
    handler
        .delegation_mut()
        .delegate_interrupt(InterruptCause::SupervisorTimer);

    // Set up supervisor CSRs
    context.csr.write(supervisor::STVEC, 0x4000_0000).unwrap();

    let trap = Trap::Interrupt(InterruptCause::SupervisorTimer);
    let new_pc = handler.handle_trap(trap, 0x2000, 0, &mut context);

    assert_eq!(new_pc, 0x4000_0000);
    assert_eq!(context.csr.read(supervisor::SEPC).unwrap(), 0x2000);
    assert_eq!(context.privilege, PrivilegeMode::Supervisor);
}

#[test]
fn test_undelegated_exception_stays_in_machine() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    // Don't delegate store access fault
    let trap = Trap::Exception(ExceptionCause::StoreAccessFault);
    let new_pc = handler.handle_trap(trap, 0x5000, 0xDEAD, &mut context);

    assert_eq!(new_pc, 0); // mtvec base
    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x5000);
    assert_eq!(context.csr.read(machine::MTVAL).unwrap(), 0xDEAD);
}

// ========================================
// Trap Context Tests
// ========================================

#[test]
fn test_trap_context_creation() {
    let ctx = ruscv_sim::core::TrapContext::new(
        0x1000,
        ExceptionCause::IllegalInstruction.code(),
        0xBAD0,
        PrivilegeMode::Machine,
    );

    assert_eq!(ctx.epc, 0x1000);
    assert_eq!(ctx.cause, ExceptionCause::IllegalInstruction.code());
    assert_eq!(ctx.tval, 0xBAD0);
    assert_eq!(ctx.privilege, PrivilegeMode::Machine);
}

#[test]
fn test_trap_context_interrupt() {
    let ctx = ruscv_sim::core::TrapContext::new(
        0x2000,
        InterruptCause::MachineExternal.code(),
        0,
        PrivilegeMode::Supervisor,
    );

    assert_eq!(ctx.epc, 0x2000);
    assert!(ctx.cause & 0x8000_0000_0000_0000 != 0); // Interrupt bit set
    assert_eq!(ctx.privilege, PrivilegeMode::Supervisor);
}

// ========================================
// Privilege Mode Transition Tests
// ========================================

#[test]
fn test_privilege_modes() {
    assert_eq!(PrivilegeMode::User as u8, 0);
    assert_eq!(PrivilegeMode::Supervisor as u8, 1);
    assert_eq!(PrivilegeMode::Machine as u8, 3);
}

#[test]
fn test_trap_preserves_privilege_in_mpp() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();
    context.privilege = PrivilegeMode::Supervisor;

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    handler.handle_trap(trap, 0x1000, 0, &mut context);

    // Check that MPP was set to Supervisor (1)
    let mstatus = context.csr.read(machine::MSTATUS).unwrap();
    let mpp = (mstatus >> 11) & 0b11;
    assert_eq!(mpp, 1); // MPP = 01 (Supervisor)
}

// ========================================
// Vectored Mode Tests
// ========================================

#[test]
fn test_vectored_mode_exception_offsets() {
    let handler = TrapHandler::new();
    let tvec = 0x1000 | 0b01; // Vectored mode at 0x1001

    // Test exception offsets
    assert_eq!(handler.vector_trap(tvec, 0), 0x1000); // Exception 0 -> offset 0
    assert_eq!(handler.vector_trap(tvec, 1), 0x1004); // Exception 1 -> offset 4
    assert_eq!(handler.vector_trap(tvec, 11), 0x102C); // Exception 11 -> offset 44
}

#[test]
fn test_vectored_mode_interrupt_offsets() {
    let handler = TrapHandler::new();
    let tvec = 0x2000 | 0b01; // Vectored mode at 0x2001

    // Test interrupt offsets (lower 7 bits of cause)
    assert_eq!(handler.vector_trap(tvec, 0x8000_0000_0000_0003), 0x200C); // MSI (3) -> offset 12
    assert_eq!(handler.vector_trap(tvec, 0x8000_0000_0000_0007), 0x201C); // MTI (7) -> offset 28
    assert_eq!(handler.vector_trap(tvec, 0x8000_0000_0000_000B), 0x202C); // MEI (11) -> offset 44
}

// ========================================
// CSR Trap Register Integration Tests
// ========================================

#[test]
fn test_mtvec_direct_mode() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    context.csr.write(machine::MTVEC, 0x8000_0000).unwrap();

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    let new_pc = handler.handle_trap(trap, 0x1000, 0, &mut context);

    assert_eq!(new_pc, 0x8000_0000);
}

#[test]
fn test_mtvec_vectored_mode() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    context
        .csr
        .write(machine::MTVEC, 0x8000_0000 | 0b01)
        .unwrap();

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    let new_pc = handler.handle_trap(trap, 0x1000, 0, &mut context);

    // Should be base + 2*4 = 0x8000_0008
    assert_eq!(new_pc, 0x8000_0008);
}

#[test]
fn test_mepc_stores_trap_pc() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Exception(ExceptionCause::Breakpoint);
    handler.handle_trap(trap, 0x1234_5678, 0, &mut context);

    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x1234_5678);
}

#[test]
fn test_mcause_stores_trap_info() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Exception(ExceptionCause::LoadPageFault);
    handler.handle_trap(trap, 0x1000, 0xABCD, &mut context);

    let mcause = context.csr.read(machine::MCAUSE).unwrap();
    assert_eq!(mcause, 13); // Load page fault code
}

#[test]
fn test_mtval_stores_fault_value() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    let trap = Trap::Exception(ExceptionCause::LoadAddressMisaligned);
    handler.handle_trap(trap, 0x1000, 0xFFFF_FFFC, &mut context);

    assert_eq!(context.csr.read(machine::MTVAL).unwrap(), 0xFFFF_FFFC);
}

// ========================================
// Multiple Trap Scenarios
// ========================================

#[test]
fn test_multiple_traps_sequential() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();

    // First trap
    let trap1 = Trap::Exception(ExceptionCause::IllegalInstruction);
    handler.handle_trap(trap1, 0x1000, 0, &mut context);
    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x1000);

    // Second trap (overwrites first)
    let trap2 = Trap::Exception(ExceptionCause::EcallM);
    handler.handle_trap(trap2, 0x2000, 0, &mut context);
    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x2000);
    assert_eq!(context.csr.read(machine::MCAUSE).unwrap(), 11);
}

#[test]
fn test_interrupt_trap_with_privilege_change() {
    let mut handler = TrapHandler::new();
    let mut context = TestTrapContext::new();
    context.privilege = PrivilegeMode::Supervisor;

    let trap = Trap::Interrupt(InterruptCause::MachineTimer);
    handler.handle_trap(trap, 0x3000, 0, &mut context);

    // MPP should be Supervisor (1)
    let mstatus = context.csr.read(machine::MSTATUS).unwrap();
    let mpp = (mstatus >> 11) & 0b11;
    assert_eq!(mpp, 1);
}
