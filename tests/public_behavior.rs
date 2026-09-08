//! Persistent A1 public behavior tests.
//!
//! These tests exercise the public CLI/ELF and flat-library paths with small,
//! hand-built RV64I ELF fixtures. The fixtures use only ADDI, AUIPC, LUI, ORI,
//! LBU, LD, SB, SD, SLLI, and the public RAM/UART/HTIF configurations under
//! test. They intentionally keep known defects as explicit reproductions instead
//! of turning them into compatibility claims.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::public_elf as fixture;
use ruscv_sim::elf::load_elf_file;
use ruscv_sim::executor::{load_and_run, RiscVSimulator};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn write_fixture(temp_dir: &TempDir, name: &str, elf: &[u8]) -> std::path::PathBuf {
    let path = temp_dir.path().join(name);
    std::fs::write(&path, elf).unwrap();
    path
}

fn run_fixture(
    elf: &[u8],
    max_cycles: Option<u64>,
    tohost: Option<u64>,
) -> ruscv_sim::ExecutionResult {
    load_and_run(elf, max_cycles, tohost, None, false).unwrap()
}

#[test]
fn elf_segments_preserve_file_bytes_and_zero_fill() {
    let code = [fixture::nop()];
    let elf = fixture::elf_with_code(&code, 0, true, true, 0x3000);
    let loaded = load_elf_file(&elf).unwrap();

    assert_eq!(loaded.entry_point, fixture::BASE);
    assert_eq!(loaded.base_addr, fixture::BASE);
    assert_eq!(&loaded.memory[0..4], &code[0].to_le_bytes());
    assert_eq!(loaded.memory[fixture::FILE_BYTE_OFFSET], fixture::FILE_BYTE);
    assert_eq!(loaded.memory[0x2000..0x2008], fixture::SIGNATURE_BYTES);
    assert_eq!(loaded.memory[0x2fff], 0);
    assert_eq!(loaded.memory.len(), 0x10000);
    assert_eq!(loaded.tohost, Some(fixture::TOHOST));
    assert_eq!(
        loaded.signature.as_ref().map(|signature| signature.vaddr),
        Some(fixture::SIGNATURE)
    );
}

#[test]
fn public_zero_fill_is_observed_by_guest_execution() {
    let code = [
        fixture::auipc(4, 3),
        fixture::addi(4, 4, -1),
        fixture::lbu(5, 4, 0),
        fixture::ori(5, 5, 1),
        fixture::lui(4, 0x40008),
        fixture::sd(5, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let result = run_fixture(&elf, Some(20), Some(0x4000_8000));

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 6);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn public_entry_and_nonzero_base_execute_from_the_declared_entry() {
    let code = [
        fixture::addi(5, 0, 1),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -0x104),
        fixture::sd(5, 4, 0),
    ];
    let entry_offset = 0x100;
    let elf = fixture::elf_with_code(&code, entry_offset, true, false, 0x3000);

    let result = run_fixture(&elf, Some(20), None);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 4);
    assert_eq!(result.final_pc, fixture::BASE + entry_offset as u64 + 16);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn public_ram_store_and_load_affect_the_same_image() {
    let code = [
        fixture::addi(5, 0, 85),
        fixture::auipc(4, 0),
        fixture::addi(4, 4, 0x1fc),
        fixture::sd(5, 4, 0),
        fixture::ld(6, 4, 0),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -0x14),
        fixture::sd(6, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let result = run_fixture(&elf, Some(20), None);

    assert_eq!(result.exit_code, 42);
    assert_eq!(result.cycles, 8);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn default_limit_path_returns_an_early_guest_exit() {
    let code = [
        fixture::standard_exit(0),
        fixture::lui(4, 0x40008),
        fixture::sd(5, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let result = run_fixture(&elf, None, Some(0x4000_8000));

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 3);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn exact_limits_include_zero_and_the_final_exit_slot() {
    let code = [
        fixture::addi(5, 0, 1),
        fixture::lui(4, 0x40008),
        fixture::nop(),
        fixture::nop(),
        fixture::nop(),
        fixture::sd(5, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let zero = run_fixture(&elf, Some(0), Some(0x4000_8000));
    assert_eq!(zero.exit_code, 1);
    assert_eq!(zero.cycles, 0);
    assert_eq!(zero.final_pc, fixture::BASE);
    assert!(zero.timed_out);
    assert_eq!(zero.error.as_deref(), Some("Timeout after 0 cycles"));

    let before_exit = run_fixture(&elf, Some(5), Some(0x4000_8000));
    assert_eq!(before_exit.exit_code, 1);
    assert_eq!(before_exit.cycles, 5);
    assert_eq!(before_exit.final_pc, fixture::BASE + 20);
    assert!(before_exit.timed_out);
    assert_eq!(before_exit.error.as_deref(), Some("Timeout after 5 cycles"));

    let final_slot = run_fixture(&elf, Some(6), Some(0x4000_8000));
    assert_eq!(final_slot.exit_code, 0);
    assert_eq!(final_slot.cycles, 6);
    assert_eq!(final_slot.final_pc, fixture::BASE + 24);
    assert!(!final_slot.timed_out);
    assert!(final_slot.error.is_none());
}

#[test]
fn cli_propagates_zero_and_nonzero_guest_exit_codes() {
    let temp_dir = TempDir::new().unwrap();
    let zero_code = [
        fixture::standard_exit(0),
        fixture::lui(4, 0x40008),
        fixture::sd(5, 4, 0),
    ];
    let nonzero_code = [
        fixture::standard_exit(42),
        fixture::lui(4, 0x40008),
        fixture::sd(5, 4, 0),
    ];
    let zero_path = write_fixture(
        &temp_dir,
        "exit-zero.elf",
        &fixture::elf_with_code(&zero_code, 0, true, false, 0x3000),
    );
    let nonzero_path = write_fixture(
        &temp_dir,
        "exit-42.elf",
        &fixture::elf_with_code(&nonzero_code, 0, true, false, 0x3000),
    );

    let zero = cargo_bin_cmd!("ruscv-sim")
        .args([
            "run",
            zero_path.to_str().unwrap(),
            "--tohost",
            "0x40008000",
            "--max-cycles",
            "10",
        ])
        .output()
        .unwrap();
    assert_eq!(zero.status.code(), Some(0));
    let zero_stdout = String::from_utf8(zero.stdout).unwrap();
    assert!(zero_stdout.contains("Exit Code:  0"));
    assert!(zero_stdout.contains("Cycles:     3"));
    assert!(zero_stdout.contains("Status:     SUCCESS"));

    let nonzero = cargo_bin_cmd!("ruscv-sim")
        .args([
            "run",
            nonzero_path.to_str().unwrap(),
            "--tohost",
            "0x40008000",
            "--max-cycles",
            "10",
        ])
        .output()
        .unwrap();
    assert_eq!(nonzero.status.code(), Some(42));
    let nonzero_stdout = String::from_utf8(nonzero.stdout).unwrap();
    assert!(nonzero_stdout.contains("Exit Code:  42"));
    assert!(nonzero_stdout.contains("Status:     FAILED"));
}

#[test]
fn alternative_exit_encoding_is_observed_on_the_public_path() {
    let code = fixture::alternative_exit(42);
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let result = run_fixture(&elf, Some(20), Some(0x4000_8000));

    assert_eq!(result.exit_code, 42);
    assert_eq!(result.cycles, 5);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn tohost_precedence_selects_elf_then_cli_override_then_fixed_endpoint() {
    let mut code = vec![
        fixture::addi(5, 0, 1),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, 4),
        fixture::sd(5, 4, 0),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -16),
        fixture::sd(5, 4, 0),
    ];
    code.extend(std::iter::repeat_n(fixture::nop(), 8));
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let elf_selected = run_fixture(&elf, Some(20), None);
    assert_eq!(elf_selected.exit_code, 0);
    assert_eq!(elf_selected.cycles, 7);
    assert!(!elf_selected.timed_out);

    let override_selected = run_fixture(&elf, Some(20), Some(fixture::ALTERNATE_TOHOST));
    assert_eq!(override_selected.exit_code, 0);
    assert_eq!(override_selected.cycles, 4);
    assert!(!override_selected.timed_out);

    let fixed_selected = run_fixture(&elf, Some(10), Some(0x4000_8000));
    assert_eq!(fixed_selected.exit_code, 1);
    assert_eq!(fixed_selected.cycles, 10);
    assert!(fixed_selected.timed_out);
    assert_eq!(
        fixed_selected.error.as_deref(),
        Some("Timeout after 10 cycles")
    );
}

#[test]
fn uart_bytes_are_emitted_by_the_cli_elf_path() {
    let code = [
        fixture::lui(4, 0x10000),
        fixture::addi(5, 0, i32::from(b'A')),
        fixture::sb(5, 4, 0),
        fixture::addi(5, 0, i32::from(b'1')),
        fixture::sb(5, 4, 0),
        fixture::addi(5, 0, i32::from(b'!')),
        fixture::sb(5, 4, 0),
        fixture::addi(5, 0, i32::from(b'\n')),
        fixture::sb(5, 4, 0),
        fixture::addi(6, 0, 1),
        fixture::lui(4, 0x40008),
        fixture::sd(6, 4, 0),
    ];
    let temp_dir = TempDir::new().unwrap();
    let path = write_fixture(
        &temp_dir,
        "uart.elf",
        &fixture::elf_with_code(&code, 0, true, false, 0x3000),
    );

    let output = cargo_bin_cmd!("ruscv-sim")
        .args([
            "run",
            path.to_str().unwrap(),
            "--tohost",
            "0x40008000",
            "--max-cycles",
            "20",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("A1!\n"));
    assert!(stdout.contains("Exit Code:  0"));
    assert!(stdout.contains("Cycles:     12"));
}

#[test]
fn signature_bytes_are_returned_after_public_execution() {
    let code = [
        fixture::auipc(4, 1),
        fixture::standard_exit(0),
        fixture::sd(5, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, true, 0x3000);

    let result = run_fixture(&elf, Some(20), None);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 3);
    assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(
        result.signature_data,
        Some(fixture::SIGNATURE_BYTES.to_vec())
    );
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn flat_library_helpers_round_trip_bytes_but_elf_tohost_is_not_adapted() {
    let code = vec![
        fixture::addi(5, 0, 1),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -4),
        fixture::sd(5, 4, 0),
    ]
    .into_iter()
    .chain(std::iter::repeat_n(fixture::nop(), 24))
    .collect::<Vec<_>>();
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);

    let cli_result = run_fixture(&elf, Some(20), None);
    assert_eq!(cli_result.exit_code, 0);
    assert_eq!(cli_result.cycles, 4);
    assert!(!cli_result.timed_out);

    let mut simulator = RiscVSimulator::new(0x1000);
    simulator
        .write_mem(0x100, &[0xde, 0xad, 0xbe, 0xef])
        .unwrap();
    assert_eq!(
        simulator.read_mem(0x100, 4).unwrap(),
        vec![0xde, 0xad, 0xbe, 0xef]
    );

    let entry = simulator.load_elf(&elf).unwrap();
    assert_eq!(entry, fixture::BASE);
    let library_result = simulator.run(Some(20)).unwrap();
    assert_eq!(library_result.exit_code, 1);
    assert_eq!(library_result.cycles, 20);
    assert!(library_result.timed_out);
    assert_eq!(
        library_result.error.as_deref(),
        Some("Timeout after 20 cycles")
    );
}

#[test]
fn public_commit_log_reproduces_nonzero_base_opcode_and_memory_suffix_gaps() {
    let code = [
        fixture::addi(5, 0, 1),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -4),
        fixture::sd(5, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, false, 0x3000);
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("commits.log");

    let result = load_and_run(&elf, Some(20), None, Some(&log_path), false).unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 4);

    let log = std::fs::read_to_string(&log_path).unwrap();
    let lines = log.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 4);
    assert!(lines.iter().all(|line| line.contains("(0x00000000)")));
    assert!(!lines.iter().any(|line| line.contains(" mem ")));
    assert!(lines[3].contains("0x000000008000000c"));
}

#[test]
fn out_of_range_flat_read_mem_reproduction_is_bounded_and_reaped() {
    const CHILD_ENV: &str = "RUSCV_SIM_READ_MEM_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        let simulator = RiscVSimulator::new(0x1000);
        let _ = simulator.read_mem(0x2000, 4);
        panic!("out-of-range read_mem unexpectedly returned");
    }

    let executable = std::env::current_exe().unwrap();
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg("out_of_range_flat_read_mem_reproduction_is_bounded_and_reaped")
        .arg("--nocapture")
        .env(CHILD_ENV, "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("read_mem child exited before the bound: {status}");
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    if let Some(status) = child.try_wait().unwrap() {
        panic!("read_mem child returned at the deadline: {status}");
    }
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success());
}
