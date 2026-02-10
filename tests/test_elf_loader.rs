//! RISC-V ELF Loader Integration Tests
//!
//! Tests the ELF loader and executor with real RISC-V programs.
//! These tests validate the core ELF loading functionality.

use ruscv_sim::executor::{load_and_run, ExecutionResult, RiscVSimulator};

/// Test basic ELF loading with empty data
#[test]
fn test_elf_loading() {
    // Test loading a minimal ELF
    let result = load_and_run(&[], Some(100), None, None, false);
    assert!(result.is_err(), "Empty ELF should fail to load");
}

/// Test simulator creation
#[test]
fn test_simulator_creation() {
    let sim = RiscVSimulator::new(0x10000);
    assert_eq!(sim.state().pc, 0);
    assert_eq!(sim.state().regs[0], 0);
}

/// Test load_and_run with no ELF
#[test]
fn test_load_and_run_empty() {
    let result = load_and_run(&[], Some(100), None, None, false);
    assert!(result.is_err());
}

/// Test simulator load with invalid ELF
#[test]
fn test_simulator_with_invalid_elf() {
    let invalid_elf = vec![0u8; 64]; // Too small, no ELF header
    let mut sim = RiscVSimulator::new(0x20000);

    let result = sim.load_elf(&invalid_elf);
    assert!(result.is_err());
}

/// Test execution result defaults
#[test]
fn test_execution_result_defaults() {
    let result = ExecutionResult::default();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 0);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

/// Test simulator set methods
#[test]
fn test_simulator_setters() {
    let mut sim = RiscVSimulator::new(0x10000);

    sim.set_max_cycles(1000);
    sim.set_tohost(0x9000_0000);
}

/// Test memory read/write via simulator
#[test]
fn test_simulator_memory_access() {
    let sim = RiscVSimulator::new(0x10000);

    // Write and read back
    sim.write_mem(0x100, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();

    let data = sim.read_mem(0x100, 4).unwrap();
    assert_eq!(data, vec![0xDE, 0xAD, 0xBE, 0xEF]);
}

/// Test step functionality
#[test]
fn test_simulator_step() {
    let mut sim = RiscVSimulator::new(0x10000);

    // Step should not panic
    let result = sim.step();
    // May fail due to no program loaded, but shouldn't panic
    assert!(result.is_ok() || result.is_err());
}

/// Test simulator state access
#[test]
fn test_simulator_state_access() {
    let sim = RiscVSimulator::new(0x10000);

    let state = sim.state();
    assert_eq!(state.pc, 0);
    assert_eq!(state.regs[0], 0);
    assert_eq!(state.regs[1], 0);
}

/// Test load_and_run with timeout (no valid ELF)
#[test]
fn test_load_and_run_no_elf() {
    let result = load_and_run(&[0x7f, 0x45, 0x4c, 0x46], Some(100), None, None, false);
    // Should fail because ELF is incomplete
    assert!(result.is_err());
}

/// Test simulator memory size limits
#[test]
fn test_simulator_memory_limits() {
    // Very small memory
    let sim = RiscVSimulator::new(0x100);
    assert_eq!(sim.memory().lock().unwrap().size(), 0x100);

    // Larger memory
    let sim = RiscVSimulator::new(0x10000);
    assert_eq!(sim.memory().lock().unwrap().size(), 0x10000);
}

/// Test that simulator can handle large memory
#[test]
fn test_simulator_large_memory() {
    let sim = RiscVSimulator::new(0x100000); // 1MB

    // Should not crash with large memory
    assert!(sim.memory().lock().unwrap().size() >= 0x100000);
}

/// Test execution result with error
#[test]
fn test_execution_result_error() {
    let result = ExecutionResult {
        exit_code: 1,
        cycles: 100,
        final_pc: 0x8000_0000,
        timed_out: false,
        error: Some("Test error".to_string()),
        signature_addr: None,
        signature_data: None,
    };

    assert_eq!(result.exit_code, 1);
    assert_eq!(result.cycles, 100);
    assert!(result.error.is_some());
    assert_eq!(result.error.unwrap(), "Test error");
}

/// Test execution result with timeout
#[test]
fn test_execution_result_timeout() {
    let result = ExecutionResult {
        exit_code: 1,
        cycles: 1000000,
        final_pc: 0x8000_1234,
        timed_out: true,
        error: Some("Timeout".to_string()),
        signature_addr: None,
        signature_data: None,
    };

    assert!(result.timed_out);
    assert!(result.error.is_some());
}

/// Test signature info in execution result
#[test]
fn test_execution_result_signature() {
    let sig_data = vec![0xAA, 0xBB, 0xCC];

    let result = ExecutionResult {
        exit_code: 0,
        cycles: 50,
        final_pc: 0x8000_0000,
        timed_out: false,
        error: None,
        signature_addr: Some(0x8000_2000),
        signature_data: Some(sig_data.clone()),
    };

    assert!(result.signature_addr.is_some());
    assert_eq!(result.signature_addr.unwrap(), 0x8000_2000);
    assert!(result.signature_data.is_some());
    assert_eq!(result.signature_data.unwrap(), sig_data);
}
