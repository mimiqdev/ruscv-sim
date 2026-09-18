//! Trap handling tests
//!
//! Tests for trap handling framework including:
//! - Exception handling
//! - Interrupt handling
//! - MRET/SRET instructions
//! - CSR trap register interactions

use ruscv_sim::core::PrivilegeMode;
use ruscv_sim::core::{
    CoreState, ExceptionCause, InterruptCause, RiscvCore, StepOutcome, Trap, TrapDelegation,
    TrapHandler,
};
use ruscv_sim::csr::machine;
use ruscv_sim::csr::supervisor;
use ruscv_sim::memory::SimpleMemory;
use ruscv_sim::{MemoryError, MemoryInterface};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

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

// Helper to create a test context
fn create_test_context() -> CoreState {
    CoreState {
        privilege: PrivilegeMode::Machine,
        ..Default::default()
    }
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

    // Synchronous exceptions always use BASE, even in vectored mode.
    assert_eq!(handler.vector_trap(tvec, 0), 0x8000_0000);
    assert_eq!(handler.vector_trap(tvec, 1), 0x8000_0000);
    assert_eq!(handler.vector_trap(tvec, 7), 0x8000_0000);

    // Interrupts retain BASE + 4*cause semantics.
    assert_eq!(
        handler.vector_trap(tvec, 0x8000_0000_0000_0003),
        0x8000_000C
    );
}

#[test]
fn test_vector_trap_vectored_mode_wrapping() {
    let handler = TrapHandler::new();
    // Use a vectored mode address near end of 64-bit address space.
    // 0xFFFF_FFFD has bits[1:0] = 01, which is vectored mode.
    let tvec = 0xFFFF_FFFF_FFFF_FFFD;

    // Test wrapping behavior for an interrupt cause.  Synchronous cause 100
    // would correctly use BASE instead.
    let result = handler.vector_trap(tvec, (1u64 << 63) | 100);
    // base = 0xFFFF_FFFF_FFFF_FFFC
    // offset = 100 * 4 = 0x190
    // result wraps to 0x18C.
    assert_eq!(result, 0x18C);
}

// ========================================
// Trap Exception Handling Tests
// ========================================

#[test]
fn test_handle_illegal_instruction_exception() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

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
    let mut context = create_test_context();

    let trap = Trap::Exception(ExceptionCause::EcallM);
    let new_pc = handler.handle_trap(trap, 0x2000, 0, &mut context);

    assert_eq!(new_pc, 0);
    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x2000);
    assert_eq!(context.csr.read(machine::MCAUSE).unwrap(), 11);
}

#[test]
fn test_handle_exception_updates_mstatus() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

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

#[test]
fn test_machine_trap_entry_all_a6_synchronous_causes() {
    let cases = [
        (
            ExceptionCause::InstructionAddressMisaligned,
            0x1111_2222_3333_4444,
        ),
        (
            ExceptionCause::InstructionAccessFault,
            0x2222_3333_4444_5555,
        ),
        (ExceptionCause::IllegalInstruction, 0x3333_4444_5555_6666),
        (ExceptionCause::Breakpoint, 0x4444_5555_6666_7777),
        (ExceptionCause::LoadAddressMisaligned, 0x5555_6666_7777_8888),
        (ExceptionCause::LoadAccessFault, 0x6666_7777_8888_9999),
        (
            ExceptionCause::StoreAddressMisaligned,
            0x7777_8888_9999_AAAA,
        ),
        (ExceptionCause::StoreAccessFault, 0x8888_9999_AAAA_BBBB),
        (ExceptionCause::EcallU, 0x9999_AAAA_BBBB_CCCC),
        (ExceptionCause::EcallS, 0xAAAA_BBBB_CCCC_DDDD),
        (ExceptionCause::EcallM, 0xBBBB_CCCC_DDDD_EEEE),
    ];
    let fault_pc = 0x1234_5678_9ABC_D000;
    let vector_base = 0x8000_1000;

    for (cause, supplied_tval) in cases {
        let mut handler = TrapHandler::new();
        let mut context = create_test_context();
        context.csr.write(machine::MTVEC, vector_base | 1).unwrap();
        context.csr.write(machine::MSTATUS, 1 << 3).unwrap();

        let target = handler.handle_trap(
            Trap::Exception(cause),
            fault_pc,
            supplied_tval,
            &mut context,
        );

        assert_eq!(target, vector_base, "cause {} vector", cause.code());
        assert_eq!(context.pc, vector_base, "cause {} pc", cause.code());
        assert_eq!(context.privilege, PrivilegeMode::Machine);
        assert_eq!(
            context.csr.read(machine::MEPC).unwrap(),
            fault_pc,
            "cause {} mepc",
            cause.code()
        );
        assert_eq!(
            context.csr.read(machine::MCAUSE).unwrap(),
            cause.code(),
            "cause {} mcause",
            cause.code()
        );
        let expected_tval = match cause {
            ExceptionCause::Breakpoint
            | ExceptionCause::EcallU
            | ExceptionCause::EcallS
            | ExceptionCause::EcallM => 0,
            _ => supplied_tval,
        };
        assert_eq!(
            context.csr.read(machine::MTVAL).unwrap(),
            expected_tval,
            "cause {} mtval",
            cause.code()
        );

        let mstatus = context.csr.read(machine::MSTATUS).unwrap();
        assert_eq!((mstatus >> 7) & 1, 1, "cause {} MPIE", cause.code());
        assert_eq!((mstatus >> 3) & 1, 0, "cause {} MIE", cause.code());
        assert_eq!((mstatus >> 11) & 0b11, 0b11, "cause {} MPP", cause.code());
    }
}

// ========================================
// Trap Interrupt Handling Tests
// ========================================

#[test]
fn test_handle_machine_timer_interrupt() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

    let trap = Trap::Interrupt(InterruptCause::MachineTimer);
    let new_pc = handler.handle_trap(trap, 0x1000, 0, &mut context);

    assert_eq!(new_pc, 0);

    // Check that MCAUSE has interrupt bit set (RV64 uses bit 63)
    let mcause = context.csr.read(machine::MCAUSE).unwrap();
    assert!(mcause & 0x8000_0000_0000_0000 != 0); // Interrupt bit
    assert_eq!(mcause & 0x7FFF_FFFF_FFFF_FFFF, 7); // Machine Timer cause
}

#[test]
fn test_handle_machine_external_interrupt() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

    let trap = Trap::Interrupt(InterruptCause::MachineExternal);
    handler.handle_trap(trap, 0x3000, 0, &mut context);

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
    let mut context = create_test_context();

    // Set privilege to Supervisor mode so delegation can work
    // (delegation only works when not in Machine mode)
    context.privilege = PrivilegeMode::Supervisor;

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
    let mut context = create_test_context();

    // Set privilege to Supervisor mode so delegation can work
    // (delegation only works when not in Machine mode)
    context.privilege = PrivilegeMode::Supervisor;

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
    let mut context = create_test_context();

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
        0x1234_5678_9ABC_D000,
        ExceptionCause::IllegalInstruction.code(),
        0xFEDC_BA98_7654_3210,
        PrivilegeMode::Machine,
    );

    assert_eq!(ctx.epc, 0x1234_5678_9ABC_D000);
    assert_eq!(ctx.cause, ExceptionCause::IllegalInstruction.code());
    assert_eq!(ctx.tval, 0xFEDC_BA98_7654_3210);
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
    let mut context = create_test_context();
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
fn test_vectored_mode_synchronous_exceptions_use_base() {
    let handler = TrapHandler::new();
    let tvec = 0x1000 | 0b01; // Vectored mode at 0x1001

    // Every synchronous exception uses BASE, not BASE + 4*cause.
    assert_eq!(handler.vector_trap(tvec, 0), 0x1000);
    assert_eq!(handler.vector_trap(tvec, 1), 0x1000);
    assert_eq!(handler.vector_trap(tvec, 11), 0x1000);
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
    let mut context = create_test_context();

    context.csr.write(machine::MTVEC, 0x8000_0000).unwrap();

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    let new_pc = handler.handle_trap(trap, 0x1000, 0, &mut context);

    assert_eq!(new_pc, 0x8000_0000);
}

#[test]
fn test_mtvec_vectored_mode() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

    context
        .csr
        .write(machine::MTVEC, 0x8000_0000 | 0b01)
        .unwrap();

    let trap = Trap::Exception(ExceptionCause::IllegalInstruction);
    let new_pc = handler.handle_trap(trap, 0x1000, 0, &mut context);

    // Synchronous exceptions use BASE even when MODE=Vectored.
    assert_eq!(new_pc, 0x8000_0000);
}

#[test]
fn test_mepc_stores_trap_pc() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

    let trap = Trap::Exception(ExceptionCause::Breakpoint);
    handler.handle_trap(trap, 0x1234_5678, 0, &mut context);

    assert_eq!(context.csr.read(machine::MEPC).unwrap(), 0x1234_5678);
}

#[test]
fn test_mcause_stores_trap_info() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

    let trap = Trap::Exception(ExceptionCause::LoadPageFault);
    handler.handle_trap(trap, 0x1000, 0xABCD, &mut context);

    let mcause = context.csr.read(machine::MCAUSE).unwrap();
    assert_eq!(mcause, 13); // Load page fault code
}

#[test]
fn test_mtval_stores_fault_value() {
    let mut handler = TrapHandler::new();
    let mut context = create_test_context();

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
    let mut context = create_test_context();

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
    let mut context = create_test_context();
    context.privilege = PrivilegeMode::Supervisor;

    let trap = Trap::Interrupt(InterruptCause::MachineTimer);
    handler.handle_trap(trap, 0x3000, 0, &mut context);

    // MPP should be Supervisor (1)
    let mstatus = context.csr.read(machine::MSTATUS).unwrap();
    let mpp = (mstatus >> 11) & 0b11;
    assert_eq!(mpp, 1);
}

// ========================================
// Task 4 induced-fault boundary coverage
// ========================================

struct CountingMemory {
    inner: SimpleMemory,
    transactions: AtomicUsize,
}

impl CountingMemory {
    fn new(size: usize) -> Self {
        Self {
            inner: SimpleMemory::new(size),
            transactions: AtomicUsize::new(0),
        }
    }

    fn count(&self) {
        self.transactions.fetch_add(1, Ordering::Relaxed);
    }

    fn transactions(&self) -> usize {
        self.transactions.load(Ordering::Relaxed)
    }
}

impl MemoryInterface for CountingMemory {
    fn read_dword(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_dword(address)
    }

    fn read_word(&self, address: u64) -> Result<u32, MemoryError> {
        self.count();
        self.inner.read_word(address)
    }

    fn read_half(&self, address: u64) -> Result<u16, MemoryError> {
        self.count();
        self.inner.read_half(address)
    }

    fn read_byte(&self, address: u64) -> Result<u8, MemoryError> {
        self.count();
        self.inner.read_byte(address)
    }

    fn read_word_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_word_zext(address)
    }

    fn read_half_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_half_zext(address)
    }

    fn read_byte_zext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_byte_zext(address)
    }

    fn read_word_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_word_sext(address)
    }

    fn read_half_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_half_sext(address)
    }

    fn read_byte_sext(&self, address: u64) -> Result<u64, MemoryError> {
        self.count();
        self.inner.read_byte_sext(address)
    }

    fn write_dword(&mut self, address: u64, value: u64) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_dword(address, value)
    }

    fn write_word(&mut self, address: u64, value: u32) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_word(address, value)
    }

    fn write_half(&mut self, address: u64, value: u16) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_half(address, value)
    }

    fn write_byte(&mut self, address: u64, value: u8) -> Result<(), MemoryError> {
        self.count();
        self.inner.write_byte(address, value)
    }

    fn size(&self) -> usize {
        self.inner.size()
    }
}

fn i_type(opcode: u32, funct3: u32, rd: u8, rs1: u8, immediate: i32) -> u32 {
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | ((rd as u32) << 7)
        | opcode
}

fn s_type(funct3: u32, rs2: u8, rs1: u8, immediate: i32) -> u32 {
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | (funct3 << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn fault_core(instruction: u32) -> (RiscvCore, Arc<Mutex<CountingMemory>>) {
    let instruction_memory = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
    instruction_memory
        .lock()
        .unwrap()
        .write_word(0, instruction)
        .unwrap();
    let data_memory = Arc::new(Mutex::new(CountingMemory::new(0x40)));
    let mut core = RiscvCore::new(instruction_memory, data_memory.clone());
    core.reset(0, 0);
    core.state_mut().csr.write(machine::MTVEC, 0x20).unwrap();
    (core, data_memory)
}

fn assert_fault(
    core: &mut RiscvCore,
    expected_cause: ExceptionCause,
    expected_pc: u64,
    expected_tval: u64,
) {
    let outcome = core.step_outcome();
    let StepOutcome::TrapEntered(fact) = outcome else {
        panic!("expected {:?} trap, got {outcome:?}", expected_cause);
    };
    assert_eq!(fact.cause, expected_cause);
    assert_eq!(fact.faulting_pc, expected_pc);
    assert_eq!(fact.mtval, expected_tval);
    assert_eq!(core.state().pc, 0x20);
    assert_eq!(core.state().csr.read(machine::MEPC).unwrap(), expected_pc);
    assert_eq!(
        core.state().csr.read(machine::MTVAL).unwrap(),
        expected_tval
    );
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn task4_induced_faults_preserve_boundary_state_and_physical_transactions() {
    // Cause 0: JALR computes an unaligned target before its link register is
    // written, so no data transaction or destination-register side effect is
    // possible.
    let jump_target = i_type(0x67, 0, 5, 1, 0);
    let (mut jump, jump_data) = fault_core(jump_target);
    jump.state_mut().regs[1] = 2;
    jump.state_mut().regs[5] = 0xDEAD_BEEF;
    assert_fault(
        &mut jump,
        ExceptionCause::InstructionAddressMisaligned,
        0,
        2,
    );
    assert_eq!(jump.state().regs[5], 0xDEAD_BEEF);
    assert_eq!(jump_data.lock().unwrap().transactions(), 0);

    // Cause 1: a rejected instruction fetch saves the fetch PC and has no
    // instruction word available for the optional trap fact.
    let (mut fetch, _) = fault_core(0x0000_0013);
    fetch.reset(0x40, 0);
    fetch.state_mut().csr.write(machine::MTVEC, 0x20).unwrap();
    let fetch_outcome = fetch.step_outcome();
    let StepOutcome::TrapEntered(fetch_fact) = fetch_outcome else {
        panic!("unmapped fetch must enter an instruction access fault");
    };
    assert_eq!(fetch_fact.cause, ExceptionCause::InstructionAccessFault);
    assert_eq!(fetch_fact.faulting_pc, 0x40);
    assert_eq!(fetch_fact.mtval, 0x40);
    assert_eq!(fetch.state().csr.read(machine::MEPC).unwrap(), 0x40);
    assert_eq!(fetch.state().csr.read(machine::MINSTRET).unwrap(), 0);

    // Cause 4: alignment is rejected before the data backend is called.
    let (mut misaligned_load, load_data) = fault_core(i_type(0x03, 1, 5, 0, 1));
    misaligned_load.state_mut().regs[5] = 0x1111_2222_3333_4444;
    assert_fault(
        &mut misaligned_load,
        ExceptionCause::LoadAddressMisaligned,
        0,
        1,
    );
    assert_eq!(misaligned_load.state().regs[5], 0x1111_2222_3333_4444);
    assert_eq!(load_data.lock().unwrap().transactions(), 0);

    // Cause 5: an aligned load reaches the physical backend, which rejects
    // the unmapped address; rd remains unchanged.
    let (mut access_load, access_load_data) = fault_core(i_type(0x03, 3, 5, 0, 0x40));
    access_load.state_mut().regs[5] = 0x5555_6666_7777_8888;
    assert_fault(&mut access_load, ExceptionCause::LoadAccessFault, 0, 0x40);
    assert_eq!(access_load.state().regs[5], 0x5555_6666_7777_8888);
    assert_eq!(access_load_data.lock().unwrap().transactions(), 1);

    // Cause 6: a misaligned store is rejected before any write transaction.
    let (mut misaligned_store, store_data) = fault_core(s_type(3, 5, 0, 1));
    misaligned_store.state_mut().regs[5] = 0x9999_AAAA_BBBB_CCCC;
    assert_fault(
        &mut misaligned_store,
        ExceptionCause::StoreAddressMisaligned,
        0,
        1,
    );
    assert_eq!(store_data.lock().unwrap().transactions(), 0);

    // Cause 7: an aligned store reaches the rejecting backend but cannot leave
    // a partial RAM write behind.
    let (mut access_store, access_store_data) = fault_core(s_type(3, 5, 0, 0x40));
    access_store.state_mut().regs[5] = 0xAAAA_BBBB_CCCC_DDDD;
    assert_fault(&mut access_store, ExceptionCause::StoreAccessFault, 0, 0x40);
    let access_store_data = access_store_data.lock().unwrap();
    assert_eq!(access_store_data.transactions(), 1);
    assert_eq!(access_store_data.inner.read_dword(0).unwrap(), 0);
}
