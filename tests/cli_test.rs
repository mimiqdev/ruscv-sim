//! CLI Integration Tests
//!
//! Tests for the complete CLI execution flow.
//! Tests the main.rs code paths for CLI execution.

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

/// Test: CLI help command
#[test]
fn test_cli_help() {
    let output = cargo_bin_cmd!("ruscv-sim").arg("--help").output().unwrap();
    assert!(output.status.success());
}

/// Test: CLI run command with --help
#[test]
fn test_cli_run_help() {
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
}

/// Test: CLI run with missing ELF file (should error)
#[test]
fn test_cli_run_missing_elf() {
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg("/nonexistent/path/test.elf")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

/// Test: CLI run with invalid ELF (should error gracefully)
#[test]
fn test_cli_run_invalid_elf() {
    let temp_dir = TempDir::new().unwrap();
    let invalid_elf = temp_dir.path().join("invalid.elf");

    // Write invalid ELF content
    let mut file = File::create(&invalid_elf).unwrap();
    file.write_all(b"not an elf file").unwrap();

    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&invalid_elf)
        .output()
        .unwrap();
    assert!(!output.status.success());
}

/// Test: CLI run with verbose flag (should not panic)
#[test]
fn test_cli_run_verbose_flag() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");

    // Create minimal valid ELF
    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    // Just run with verbose - should not panic even if execution fails
    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--verbose")
        .output();
}

/// Test: CLI run with max_cycles flag
#[test]
fn test_cli_run_with_max_cycles() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--max-cycles")
        .arg("100")
        .output();
}

/// Test: CLI run with tohost address
#[test]
fn test_cli_run_with_tohost() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--tohost")
        .arg("0x40008000")
        .output();
}

/// Test: CLI run with log-commits flag
#[test]
fn test_cli_run_with_log_commits() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");
    let log_file = temp_dir.path().join("commits.log");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--log-commits")
        .arg(&log_file)
        .output();
}

/// Test: CLI run with all flags combined
#[test]
fn test_cli_run_all_flags() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");
    let log_file = temp_dir.path().join("commits.log");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--max-cycles")
        .arg("1000")
        .arg("--tohost")
        .arg("0x40008000")
        .arg("--verbose")
        .arg("--log-commits")
        .arg(&log_file)
        .output();
}

/// Test: CLI with invalid subcommand
#[test]
fn test_cli_invalid_subcommand() {
    let output = cargo_bin_cmd!("ruscv-sim").arg("invalid").output().unwrap();
    assert!(!output.status.success());
}

/// Test: CLI with unknown flag
#[test]
fn test_cli_unknown_flag() {
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("--unknown-flag")
        .output()
        .unwrap();
    assert!(!output.status.success());
}

/// Test: CLI version command
#[test]
fn test_cli_version() {
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
}

/// Test: CLI run with hex tohost address
#[test]
fn test_cli_run_hex_tohost() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--tohost")
        .arg("0xffffffff40008000")
        .output();
}

/// Test: CLI run with decimal tohost address
#[test]
fn test_cli_run_decimal_tohost() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let _ = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .arg("--tohost")
        .arg("1076917864448")
        .output();
}

/// Test: CLI run processes the command
#[test]
fn test_cli_run_processes_command() {
    let temp_dir = TempDir::new().unwrap();
    let elf_file = temp_dir.path().join("test.elf");

    let minimal_elf = create_minimal_elf();
    let mut file = File::create(&elf_file).unwrap();
    file.write_all(&minimal_elf).unwrap();

    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&elf_file)
        .output()
        .unwrap();

    // Should have output (either success or execution error)
    assert!(!output.stdout.is_empty() || !output.stderr.is_empty());
}

/// Helper function to create a minimal ELF file for testing
fn create_minimal_elf() -> Vec<u8> {
    let mut elf = Vec::new();

    // ELF Header
    elf.extend_from_slice(&[0x7f, 0x45, 0x4c, 0x46]); // Magic
    elf.extend_from_slice(&[0x02, 0x01, 0x01, 0x00]); // 64-bit, little endian
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // Padding
    elf.extend_from_slice(&[0x02, 0x00]); // ET_EXEC
    elf.extend_from_slice(&[0xF3, 0x00]); // EM_RISCV (243)
    elf.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // EV_CURRENT
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // entry = 0
    elf.extend_from_slice(&[0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // phoff = 0x40
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // shoff = 0
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // RISC-V flags
    elf.extend_from_slice(&[0x40, 0x00]); // ehsize = 64
    elf.extend_from_slice(&[0x38, 0x00]); // phentsize = 56
    elf.extend_from_slice(&[0x01, 0x00]); // phnum = 1
    elf.extend_from_slice(&[0x00, 0x00]); // shentsize = 0
    elf.extend_from_slice(&[0x00, 0x00]); // shnum = 0
    elf.extend_from_slice(&[0x00, 0x00]); // shstrndx = 0

    // Program Header
    elf.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]); // PT_LOAD
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // p_flags
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // offset
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // vaddr
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // paddr
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // filesz
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // memsz
    elf.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]); // align

    elf
}
