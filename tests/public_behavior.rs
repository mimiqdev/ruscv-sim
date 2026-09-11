//! Persistent public behavior tests.
//!
//! These tests exercise the public CLI/ELF and flat-library paths with small,
//! hand-built RV64I ELF fixtures. The fixtures use only ADDI, AUIPC, LUI, ORI,
//! LBU, LD, SB, SD, SLLI, and the public RAM/UART/HTIF configurations under
//! test. A1 reproductions remain explicit for unrepaired gaps. A2 T1 replaced the
//! G-02 hang reproduction with a bounded error-return regression, and A2 T2
//! replaced the G-01 contrast with a correct flat-library exit regression plus
//! placement, precedence and limit assertions.

mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::public_elf as fixture;
use ruscv_sim::elf::{load_elf_file, ElfLoader};
use ruscv_sim::executor::{load_and_run, ExecutionResult, ExecutorError, RiscVSimulator};
use ruscv_sim::PrivilegeMode;
use std::io::{Cursor, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
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
    let elf = fixture::elf_with_code(&code, 0, true, true, fixture::BSS_MEMORY_SIZE);
    let loaded = load_elf_file(&elf).unwrap();

    assert_eq!(loaded.entry_point, fixture::BASE);
    assert_eq!(loaded.base_addr, fixture::BASE);
    assert_eq!(&loaded.memory[0..4], &code[0].to_le_bytes());
    assert_eq!(loaded.memory[fixture::FILE_BYTE_OFFSET], fixture::FILE_BYTE);
    assert_eq!(loaded.memory[0x2000..0x2008], fixture::SIGNATURE_BYTES);
    assert_eq!(loaded.memory[fixture::BSS_PROBE_OFFSET], 0);
    assert_eq!(loaded.memory.len(), fixture::EXPECTED_BSS_MEMORY_LEN);
    assert_eq!(loaded.tohost, Some(fixture::TOHOST));
    assert_eq!(
        loaded.signature.as_ref().map(|signature| signature.vaddr),
        Some(fixture::SIGNATURE)
    );
}

#[test]
fn elf_loader_clears_bss_in_prefilled_memory() {
    let code = [fixture::nop()];
    let elf = fixture::elf_with_code(&code, 0, true, false, fixture::BSS_MEMORY_SIZE);
    let mut cursor = Cursor::new(&elf);
    let loader = ElfLoader::load(&mut cursor).unwrap();
    let mut memory = vec![0x5a; fixture::EXPECTED_BSS_MEMORY_LEN];

    assert_eq!(memory[fixture::BSS_PROBE_OFFSET], 0x5a);
    loader.load_into_memory(&mut cursor, &mut memory).unwrap();

    assert_eq!(&memory[0..4], &code[0].to_le_bytes());
    assert_eq!(memory[fixture::FILE_BYTE_OFFSET], fixture::FILE_BYTE);
    assert_eq!(memory[fixture::BSS_PROBE_OFFSET], 0);
}

#[test]
fn public_zero_fill_is_observed_by_guest_execution() {
    let code = [
        fixture::auipc(4, 0x18),
        fixture::addi(4, 4, -1),
        fixture::lbu(5, 4, 0),
        fixture::ori(5, 5, 1),
        fixture::lui(4, 0x40008),
        fixture::sd(5, 4, 0),
    ];
    let elf = fixture::elf_with_code(&code, 0, true, false, fixture::BSS_MEMORY_SIZE);

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

/// Store the exit payload at the image's declared tohost from `entry_offset`.
///
/// The address is built from the AUIPC at `entry_offset + 4`, so the delta is
/// split to stay inside the ADDI immediate range and the sequence can sit
/// anywhere in the image.
fn tohost_writer_at(entry_offset: usize, exit_instruction: u32) -> Vec<u32> {
    let pc = entry_offset as i64 + 4;
    let delta = fixture::TOHOST_SEGMENT_OFFSET as i64 - pc;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    vec![
        exit_instruction,
        fixture::auipc(4, upper as u32),
        fixture::addi(4, 4, lower as i32),
        fixture::sd(5, 4, 0),
    ]
}

/// Guest that writes an exit payload to its image-declared `.tohost`.
fn declared_tohost_writer(exit_instruction: u32) -> Vec<u32> {
    padded(tohost_writer_at(0, exit_instruction))
}

/// Guest that stores `value` at `guest_offset` from the image base.
///
/// The address is built from the AUIPC at `entry_offset + 4`.
fn store_byte_at(entry_offset: usize, value: u8, guest_offset: i64) -> Vec<u32> {
    let pc = entry_offset as i64 + 4;
    let delta = guest_offset - pc;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    vec![
        fixture::addi(5, 0, i32::from(value)),
        fixture::auipc(4, upper as u32),
        fixture::addi(4, 4, lower as i32),
        fixture::sb(5, 4, 0),
    ]
}

/// Guest that stores `value` at `guest_offset` and then exits through the
/// image's declared tohost.
fn signature_writer(value: u8, guest_offset: i64, exit_code: u32) -> Vec<u32> {
    let mut code = store_byte_at(0, value, guest_offset);
    let exit_offset = code.len() * 4;
    code.extend(tohost_writer_at(
        exit_offset,
        fixture::standard_exit(exit_code),
    ));
    padded(code)
}

/// Guest that writes an exit payload to `guest_offset` from the image base.
///
/// The address is built from the AUIPC at `entry + 4`, so the delta is split
/// into an upper and a lower part to stay inside the ADDI immediate range.
fn fixed_offset_writer(exit_instruction: u32, guest_offset: i64) -> Vec<u32> {
    let delta = guest_offset - 4;
    let upper = (delta + 0x800) >> 12;
    let lower = delta - (upper << 12);
    padded(vec![
        exit_instruction,
        fixture::auipc(4, upper as u32),
        fixture::addi(4, 4, lower as i32),
        fixture::sd(5, 4, 0),
    ])
}

fn padded(mut code: Vec<u32>) -> Vec<u32> {
    code.extend(std::iter::repeat_n(fixture::nop(), 24));
    code
}

fn load_and_run_library(elf: &[u8], max_cycles: u64) -> (RiscVSimulator, ExecutionResult) {
    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(elf).unwrap();
    let result = simulator.run(Some(max_cycles)).unwrap();
    (simulator, result)
}

#[test]
fn flat_library_elf_tohost_metadata_selects_the_ram_exit_signal() {
    let elf = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(0)),
        0,
        true,
        false,
        0x3000,
    );

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
    assert_eq!(library_result.exit_code, 0);
    assert_eq!(library_result.cycles, 4);
    assert_eq!(library_result.final_pc, fixture::BASE + 0x10);
    assert!(!library_result.timed_out);
    assert!(library_result.error.is_none());
}

#[test]
fn flat_library_reports_zero_and_nonzero_guest_exits() {
    for exit_code in [0u32, 1, 42] {
        let elf = fixture::elf_with_code(
            &declared_tohost_writer(fixture::standard_exit(exit_code)),
            0,
            true,
            false,
            0,
        );
        let (_, result) = load_and_run_library(&elf, 12);

        assert_eq!(result.exit_code, exit_code, "exit code {exit_code}");
        assert_eq!(result.cycles, 4, "exit code {exit_code}");
        assert_eq!(result.final_pc, fixture::BASE + 0x10, "exit {exit_code}");
        assert!(!result.timed_out, "exit code {exit_code}");
        assert!(result.error.is_none(), "exit code {exit_code}");
    }
}

#[test]
fn flat_library_reports_base_zero_placement() {
    let base = 0u64;
    let elf = fixture::elf_with_placement(
        &declared_tohost_writer(fixture::standard_exit(7)),
        0,
        base,
        Some(base + fixture::TOHOST_SEGMENT_OFFSET),
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    assert_eq!(simulator.load_elf(&elf).unwrap(), base);
    let result = simulator.run(Some(12)).unwrap();

    assert_eq!(result.exit_code, 7);
    assert_eq!(result.cycles, 4);
    assert_eq!(result.final_pc, base + 0x10);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn flat_library_retains_the_nonzero_exit_before_clearing_the_signal() {
    let elf = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(1)),
        0,
        true,
        false,
        0,
    );
    let (simulator, result) = load_and_run_library(&elf, 12);

    assert_eq!(result.exit_code, 1);
    assert!(result.error.is_none());
    assert_eq!(
        simulator
            .read_mem(fixture::TOHOST_SEGMENT_OFFSET, 8)
            .unwrap(),
        vec![0u8; 8],
        "the guest RAM signal is cleared after the exit is reported"
    );
}

#[test]
fn flat_library_manual_flat_tohost_overrides_image_metadata() {
    let elf = fixture::elf_with_code(
        &fixed_offset_writer(fixture::standard_exit(1), 0x100),
        0,
        true,
        false,
        0,
    );

    // The image declares its tohost at flat 0x1000, but this guest writes to
    // flat 0x100, so the declared signal is never written.
    let (_, declared) = load_and_run_library(&elf, 12);
    assert!(declared.timed_out, "{declared:?}");
    assert_eq!(declared.cycles, 12);

    // An explicit flat offset set after loading selects the guest's signal.
    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&elf).unwrap();
    simulator.set_tohost(0x100);
    let result = simulator.run(Some(12)).unwrap();

    assert_eq!(result.exit_code, 1);
    assert_eq!(result.cycles, 4);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
}

#[test]
fn flat_library_manual_tohost_before_load_is_superseded_by_image_metadata() {
    let elf = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(1)),
        0,
        true,
        false,
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.set_tohost(0x100);
    simulator.load_elf(&elf).unwrap();
    let result = simulator.run(Some(12)).unwrap();

    assert_eq!(result.exit_code, 1);
    assert_eq!(result.cycles, 4);
    assert!(!result.timed_out);
}

#[test]
fn flat_library_manual_tohost_survives_a_load_without_metadata() {
    let elf = fixture::elf_with_code(
        &fixed_offset_writer(fixture::standard_exit(1), 0x100),
        0,
        false,
        false,
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.set_tohost(0x100);
    simulator.load_elf(&elf).unwrap();
    let result = simulator.run(Some(12)).unwrap();

    assert_eq!(result.exit_code, 1);
    assert_eq!(result.cycles, 4);
    assert!(!result.timed_out);
}

#[test]
fn flat_library_image_without_metadata_does_not_reuse_the_previous_tohost() {
    let first = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(4)),
        0,
        true,
        false,
        0,
    );
    let second = fixture::elf_with_code(
        &fixed_offset_writer(fixture::standard_exit(4), 0x1000),
        0,
        false,
        false,
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&first).unwrap();
    let first_result = simulator.run(Some(12)).unwrap();
    assert_eq!(first_result.exit_code, 4);

    simulator.load_elf(&second).unwrap();
    let second_result = simulator.run(Some(12)).unwrap();

    assert!(
        second_result.timed_out,
        "the second image must not inherit the first image's declared tohost: {second_result:?}"
    );
    assert_eq!(second_result.cycles, 12);
    assert_eq!(
        second_result.error.as_deref(),
        Some("Timeout after 12 cycles")
    );
}

#[test]
fn flat_library_rejects_a_declared_tohost_the_flat_image_cannot_represent() {
    let code = declared_tohost_writer(fixture::standard_exit(0));

    let mut below_base = RiscVSimulator::new(0x1_0000);
    let below = fixture::elf_with_placement(&code, 0, fixture::BASE, Some(fixture::BASE - 8), 0);
    let error = below_base.load_elf(&below).unwrap_err();
    assert!(
        format!("{error}").contains("tohost"),
        "unexpected error: {error}"
    );

    let mut beyond_memory = RiscVSimulator::new(0x1_0000);
    let beyond =
        fixture::elf_with_placement(&code, 0, fixture::BASE, Some(fixture::BASE + 0x20_0000), 0);
    assert!(beyond_memory.load_elf(&beyond).is_err());

    let mut overflow = RiscVSimulator::new(0x1_0000);
    let wrapping = fixture::elf_with_placement(&code, 0, 0, Some(u64::MAX - 7), 0);
    assert!(overflow.load_elf(&wrapping).is_err());

    // An in-memory but misaligned signal can never be read by the dword poll.
    let mut misaligned = RiscVSimulator::new(0x1_0000);
    let skewed = fixture::elf_with_placement(
        &code,
        0,
        fixture::BASE,
        Some(fixture::BASE + fixture::TOHOST_SEGMENT_OFFSET + 4),
        0,
    );
    let error = misaligned.load_elf(&skewed).unwrap_err();
    assert!(
        format!("{error}").contains("eight-byte aligned"),
        "unexpected error: {error}"
    );

    let mut representable = RiscVSimulator::new(0x1_0000);
    let in_range = fixture::elf_with_placement(&code, 0, fixture::BASE, Some(fixture::TOHOST), 0);
    assert!(representable.load_elf(&in_range).is_ok());
}

#[test]
fn flat_library_rejected_placement_leaves_the_previous_image_runnable() {
    let good = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(1)),
        0,
        true,
        true,
        0,
    );
    let rejected = fixture::elf_with_placement(
        &declared_tohost_writer(fixture::standard_exit(0)),
        0,
        fixture::BASE,
        Some(fixture::BASE - 8),
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&good).unwrap();
    assert!(simulator.load_elf(&rejected).is_err());

    let result = simulator.run(Some(12)).unwrap();
    assert_eq!(result.exit_code, 1);
    assert_eq!(result.cycles, 4);
    assert!(!result.timed_out);
    assert_eq!(
        result.signature_addr,
        Some(fixture::SIGNATURE),
        "the rejected image must not replace the loaded image's signature metadata"
    );
}

#[test]
fn flat_library_bounds_zero_budget_and_final_slot_exits() {
    let elf = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(1)),
        0,
        true,
        false,
        0,
    );

    let (_, zero_budget) = load_and_run_library(&elf, 0);
    assert_eq!(zero_budget.exit_code, 1);
    assert_eq!(zero_budget.cycles, 0);
    assert_eq!(zero_budget.final_pc, fixture::BASE);
    assert!(zero_budget.timed_out);
    assert_eq!(zero_budget.error.as_deref(), Some("Timeout after 0 cycles"));

    let (_, one_short) = load_and_run_library(&elf, 3);
    assert_eq!(one_short.cycles, 3);
    assert!(one_short.timed_out);
    assert_eq!(one_short.error.as_deref(), Some("Timeout after 3 cycles"));

    let (_, final_slot) = load_and_run_library(&elf, 4);
    assert_eq!(final_slot.exit_code, 1);
    assert_eq!(final_slot.cycles, 4);
    assert!(!final_slot.timed_out);
    assert!(final_slot.error.is_none());
}

#[test]
fn flat_library_distinguishes_guest_exit_timeout_and_execution_error() {
    let exit_elf = fixture::elf_with_code(
        &declared_tohost_writer(fixture::standard_exit(1)),
        0,
        true,
        false,
        0,
    );
    let (_, guest_exit) = load_and_run_library(&exit_elf, 12);
    assert!(!guest_exit.timed_out);
    assert!(guest_exit.error.is_none());

    let silent_elf = fixture::elf_with_code(&padded(vec![fixture::nop()]), 0, false, false, 0);
    let (_, timeout) = load_and_run_library(&silent_elf, 8);
    assert!(timeout.timed_out);
    assert_eq!(timeout.error.as_deref(), Some("Timeout after 8 cycles"));

    let broken_elf = fixture::elf_with_placement(&[0x0000_0000], 0, fixture::BASE, None, 0);
    let (_, broken) = load_and_run_library(&broken_elf, 8);
    assert!(!broken.timed_out);
    assert_eq!(broken.cycles, 0);
    assert_eq!(broken.final_pc, fixture::BASE);
    assert!(
        broken
            .error
            .as_deref()
            .is_some_and(|message| message.contains("Execution error")),
        "{broken:?}"
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

const READ_MEM_CHILD_ENV: &str = "RUSCV_SIM_READ_MEM_CHILD";
const READ_MEM_READY_ENV: &str = "RUSCV_SIM_READ_MEM_READY";
const READ_MEM_CHILD_MODE_ENV: &str = "RUSCV_SIM_READ_MEM_CHILD_MODE";
const READ_MEM_TEST_NAME: &str = "out_of_range_flat_read_mem_returns_error_without_hanging";

#[derive(Debug)]
struct ChildRun {
    ready: bool,
    survived_hang_window: bool,
    status: Option<ExitStatus>,
    cleanup_error: Option<String>,
    diagnostics: String,
}

struct KillOnDrop {
    child: Child,
    reaped: bool,
}

impl KillOnDrop {
    fn new(child: Child) -> Self {
        Self {
            child,
            reaped: false,
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.reaped = true;
        }
        Ok(status)
    }

    fn kill_and_wait(&mut self) -> std::io::Result<ExitStatus> {
        if self.reaped {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "child was already reaped",
            ));
        }

        match self.child.kill() {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput | std::io::ErrorKind::NotFound
                ) => {}
            Err(error) => return Err(error),
        }
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }

    fn diagnostics(&mut self) -> String {
        fn read_pipe<T: Read>(pipe: &mut Option<T>) -> String {
            let mut bytes = Vec::new();
            if let Some(pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }

        let stdout = read_pipe(&mut self.child.stdout);
        let stderr = read_pipe(&mut self.child.stderr);
        format!("stdout:\n{stdout}\nstderr:\n{stderr}")
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.reaped = true;
        }
    }
}

fn finish_child(
    child: &mut KillOnDrop,
    ready: bool,
    survived_hang_window: bool,
    reason: impl Into<String>,
    status: Option<ExitStatus>,
) -> ChildRun {
    let (status, cleanup_error) = match status {
        Some(status) => (Some(status), None),
        None => match child.kill_and_wait() {
            Ok(status) => (Some(status), None),
            Err(error) => (None, Some(error.to_string())),
        },
    };
    let reason = reason.into();
    let child_output = if cleanup_error.is_none() {
        child.diagnostics()
    } else {
        "child output unavailable before cleanup completed".to_string()
    };
    let diagnostics = format!("reason: {reason}\n{child_output}");

    ChildRun {
        ready,
        survived_hang_window,
        status,
        cleanup_error,
        diagnostics,
    }
}

fn run_read_mem_child(
    skip_ready: bool,
    exit_before_ready: bool,
    ready_timeout: Duration,
    hang_timeout: Duration,
) -> ChildRun {
    let temp_dir = TempDir::new().unwrap();
    let ready_path = temp_dir.path().join("read-mem-ready");
    let executable = std::env::current_exe().unwrap();
    let mode = if exit_before_ready {
        "exit-before-ready"
    } else if skip_ready {
        "skip-ready"
    } else {
        "normal"
    };
    let spawned = Command::new(executable)
        .arg("--exact")
        .arg(READ_MEM_TEST_NAME)
        .arg("--nocapture")
        .env(READ_MEM_CHILD_ENV, "1")
        .env(READ_MEM_READY_ENV, &ready_path)
        .env(READ_MEM_CHILD_MODE_ENV, mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut child = KillOnDrop::new(spawned);

    let ready_deadline = Instant::now() + ready_timeout;
    loop {
        match std::fs::read(&ready_path) {
            Ok(bytes) if bytes == b"ready" => break,
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return finish_child(
                    &mut child,
                    false,
                    false,
                    format!("ready handshake read failed: {error}"),
                    None,
                );
            }
        }

        match child.try_wait() {
            Ok(Some(status)) => {
                return finish_child(
                    &mut child,
                    false,
                    false,
                    format!("read_mem child exited before ready: {status}"),
                    Some(status),
                );
            }
            Ok(None) => {}
            Err(error) => {
                return finish_child(
                    &mut child,
                    false,
                    false,
                    format!("checking read_mem child before ready failed: {error}"),
                    None,
                );
            }
        }

        if Instant::now() >= ready_deadline {
            return finish_child(
                &mut child,
                false,
                false,
                format!("ready handshake timed out after {ready_timeout:?}"),
                None,
            );
        }
        thread::sleep(Duration::from_millis(5));
    }

    let hang_deadline = Instant::now() + hang_timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return finish_child(
                    &mut child,
                    true,
                    false,
                    format!("read_mem child exited during hang window: {status}"),
                    Some(status),
                );
            }
            Ok(None) => {}
            Err(error) => {
                return finish_child(
                    &mut child,
                    true,
                    false,
                    format!("checking read_mem child during hang window failed: {error}"),
                    None,
                );
            }
        }

        if Instant::now() >= hang_deadline {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    finish_child(
        &mut child,
        true,
        true,
        format!("read_mem child survived {hang_timeout:?} hang window"),
        None,
    )
}

#[test]
fn out_of_range_flat_read_mem_returns_error_without_hanging() {
    if std::env::var_os(READ_MEM_CHILD_ENV).is_some() {
        let simulator = RiscVSimulator::new(0x1000);
        let mode = std::env::var(READ_MEM_CHILD_MODE_ENV).unwrap();
        if mode == "exit-before-ready" {
            eprintln!("read_mem child exiting before ready handshake");
            return;
        }
        if mode == "skip-ready" {
            // Stay alive without the ready marker so the parent can exercise
            // ready-timeout + kill/reap against a live child. This is harness
            // cleanup coverage, not the G-02 hang regression.
            thread::park();
            panic!("skip-ready child was unparked");
        }
        let ready_path = std::path::PathBuf::from(
            std::env::var_os(READ_MEM_READY_ENV).expect("ready path missing"),
        );
        std::fs::write(&ready_path, b"ready")
            .unwrap_or_else(|error| panic!("failed to signal ready handshake: {error}"));
        let result = simulator.read_mem(0x2000, 4);
        assert!(
            result.is_err(),
            "out-of-range aligned read_mem must return an error, got {result:?}"
        );
        return;
    }

    let outcome = run_read_mem_child(false, false, Duration::from_secs(2), Duration::from_secs(2));
    assert!(outcome.ready, "{outcome:?}");
    assert!(!outcome.survived_hang_window, "{outcome:?}");
    assert!(
        matches!(outcome.status.as_ref(), Some(status) if status.success()),
        "{outcome:?}"
    );
    assert!(outcome.cleanup_error.is_none(), "{outcome:?}");
}

#[test]
fn out_of_range_flat_read_mem_handshake_failure_is_reaped() {
    let outcome = run_read_mem_child(
        true,
        false,
        Duration::from_millis(250),
        Duration::from_millis(250),
    );
    assert!(!outcome.ready, "{outcome:?}");
    assert!(!outcome.survived_hang_window, "{outcome:?}");
    assert!(outcome.status.is_some(), "{outcome:?}");
    assert!(
        outcome.diagnostics.contains("ready handshake timed out"),
        "{outcome:?}"
    );
    assert!(outcome.cleanup_error.is_none(), "{outcome:?}");
}

#[test]
fn out_of_range_flat_read_mem_early_exit_is_reaped() {
    let outcome = run_read_mem_child(
        false,
        true,
        Duration::from_secs(1),
        Duration::from_millis(250),
    );
    assert!(!outcome.ready, "{outcome:?}");
    assert!(!outcome.survived_hang_window, "{outcome:?}");
    assert_eq!(
        outcome.status.as_ref().and_then(|status| status.code()),
        Some(0),
        "{outcome:?}"
    );
    assert!(
        outcome
            .diagnostics
            .contains("exiting before ready handshake"),
        "{outcome:?}"
    );
    assert!(outcome.cleanup_error.is_none(), "{outcome:?}");
}

const FLAT_MEM_SIZE: usize = 0x1000;
const NOP: u32 = 0x0000_0013;

fn pattern_byte(offset: usize) -> u8 {
    (offset % 251) as u8
}

fn simulator_with_pattern() -> RiscVSimulator {
    let simulator = RiscVSimulator::new(FLAT_MEM_SIZE);
    let pattern: Vec<u8> = (0..FLAT_MEM_SIZE).map(pattern_byte).collect();
    simulator.write_mem(0, &pattern).unwrap();
    simulator
}

fn expected_bytes(addr: u64, size: usize) -> Vec<u8> {
    (0..size)
        .map(|offset| pattern_byte(addr as usize + offset))
        .collect()
}

fn core_snapshot(simulator: &RiscVSimulator) -> (u64, [u64; 32], PrivilegeMode) {
    let state = simulator.state();
    (state.pc, state.regs, state.privilege)
}

fn assert_read_error(result: Result<Vec<u8>, ExecutorError>) {
    assert!(
        result.is_err(),
        "expected a bounded inspection error, got {result:?}"
    );
}

#[test]
fn flat_read_mem_returns_exact_bytes_for_aligned_unaligned_and_mixed_lengths() {
    let simulator = simulator_with_pattern();
    let before = core_snapshot(&simulator);
    let cases = [
        (0x00u64, 1usize),
        (0x00, 2),
        (0x00, 4),
        (0x00, 8),
        (0x00, 16),
        (0x01, 1),
        (0x01, 2),
        (0x01, 3),
        (0x02, 2),
        (0x03, 5),
        (0x07, 9),
        (0x100, 6),
        (0xffc, 4),
        (0xfff, 1),
    ];

    for (addr, size) in cases {
        assert_eq!(
            simulator.read_mem(addr, size).unwrap(),
            expected_bytes(addr, size),
            "addr=0x{addr:x} size={size}"
        );
    }

    assert_eq!(core_snapshot(&simulator), before);
    assert_eq!(
        simulator.read_mem(0, FLAT_MEM_SIZE).unwrap(),
        expected_bytes(0, FLAT_MEM_SIZE)
    );
}

#[test]
fn flat_read_mem_empty_requests_do_not_access_memory() {
    let simulator = simulator_with_pattern();
    let before = core_snapshot(&simulator);
    let memory_before = simulator.read_mem(0, FLAT_MEM_SIZE).unwrap();

    for addr in [0u64, 0xfff, 0x1000, 0x2000, u64::MAX] {
        assert_eq!(simulator.read_mem(addr, 0).unwrap(), Vec::<u8>::new());
    }

    assert_eq!(core_snapshot(&simulator), before);
    assert_eq!(simulator.read_mem(0, FLAT_MEM_SIZE).unwrap(), memory_before);
}

#[test]
fn flat_read_mem_rejects_out_of_range_crossing_and_overflow_without_wrapping() {
    let simulator = simulator_with_pattern();
    let before = core_snapshot(&simulator);
    let memory_before = simulator.read_mem(0, FLAT_MEM_SIZE).unwrap();

    assert_read_error(simulator.read_mem(0x2000, 4));
    assert_read_error(simulator.read_mem(0x2000, 8));
    assert_read_error(simulator.read_mem(0x1000, 1));
    assert_read_error(simulator.read_mem(0xffe, 4));
    assert_read_error(simulator.read_mem(0, FLAT_MEM_SIZE + 1));
    assert_read_error(simulator.read_mem(u64::MAX, 1));
    assert_read_error(simulator.read_mem(u64::MAX - 7, 16));
    assert_read_error(simulator.read_mem(u64::MAX, 2));

    assert_eq!(core_snapshot(&simulator), before);
    assert_eq!(simulator.read_mem(0, FLAT_MEM_SIZE).unwrap(), memory_before);
}

#[test]
fn flat_read_mem_does_not_execute_or_mutate_after_guest_step() {
    let mut simulator = simulator_with_pattern();
    simulator.write_mem(0, &NOP.to_le_bytes()).unwrap();
    simulator.state_mut().pc = 0;
    simulator.state_mut().regs[5] = 0x1111_2222_3333_4444;
    simulator.step().unwrap();
    assert_eq!(simulator.state().pc, 4);

    let before = core_snapshot(&simulator);
    let memory_before = simulator.read_mem(0, FLAT_MEM_SIZE).unwrap();

    assert_eq!(simulator.read_mem(0, 4).unwrap(), NOP.to_le_bytes());
    assert_read_error(simulator.read_mem(0x2000, 4));
    assert_eq!(simulator.read_mem(4, 3).unwrap(), expected_bytes(4, 3));

    assert_eq!(core_snapshot(&simulator), before);
    assert_eq!(simulator.read_mem(0, FLAT_MEM_SIZE).unwrap(), memory_before);
    assert_eq!(simulator.state().regs[5], 0x1111_2222_3333_4444);
}

const SIGNATURE_WRITTEN_BYTE: u8 = 0x5a;

fn expected_signature_bytes() -> Vec<u8> {
    let mut expected = fixture::SIGNATURE_BYTES.to_vec();
    expected[0] = SIGNATURE_WRITTEN_BYTE;
    expected
}

#[test]
fn flat_library_returns_guest_written_signature_bytes_at_nonzero_base() {
    let elf = fixture::elf_with_code(
        &signature_writer(
            SIGNATURE_WRITTEN_BYTE,
            fixture::SIGNATURE_SEGMENT_OFFSET as i64,
            0,
        ),
        0,
        true,
        true,
        0,
    );
    let (simulator, result) = load_and_run_library(&elf, 20);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 8);
    assert_eq!(result.final_pc, fixture::BASE + 0x20);
    assert!(!result.timed_out);
    assert!(result.error.is_none());
    assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(result.signature_data, Some(expected_signature_bytes()));
    assert_eq!(
        simulator
            .read_mem(fixture::SIGNATURE_SEGMENT_OFFSET, 8)
            .unwrap(),
        expected_signature_bytes(),
        "the artifact must be the flat bytes the guest wrote"
    );
}

#[test]
fn flat_library_returns_signature_bytes_at_base_zero() {
    let base = 0u64;
    let elf = fixture::elf_with_signature(
        &signature_writer(SIGNATURE_WRITTEN_BYTE, 0x2000, 0),
        0,
        base,
        Some(base + fixture::TOHOST_SEGMENT_OFFSET),
        Some((base + fixture::SIGNATURE_SEGMENT_OFFSET, 8)),
        0,
    );
    let (_, result) = load_and_run_library(&elf, 20);

    assert_eq!(result.exit_code, 0);
    assert_eq!(result.cycles, 8);
    assert_eq!(
        result.signature_addr,
        Some(base + fixture::SIGNATURE_SEGMENT_OFFSET)
    );
    assert_eq!(result.signature_data, Some(expected_signature_bytes()));
    assert!(result.error.is_none());
}

#[test]
fn flat_library_distinguishes_absent_empty_and_unreadable_signatures() {
    let guest = padded(declared_tohost_writer(fixture::standard_exit(0)));
    let absent =
        fixture::elf_with_signature(&guest, 0, fixture::BASE, Some(fixture::TOHOST), None, 0);
    let empty = fixture::elf_with_signature(
        &guest,
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::SIGNATURE, 0)),
        0,
    );
    let unreadable = fixture::elf_with_signature(
        &guest,
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::BASE + 0x20_0000, 8)),
        0,
    );
    // An empty region needs no mapping, so the same unmappable address stays an
    // empty artifact rather than a failure.
    let empty_and_unmappable = fixture::elf_with_signature(
        &guest,
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::BASE + 0x20_0000, 0)),
        0,
    );

    let (_, absent_result) = load_and_run_library(&absent, 12);
    assert_eq!(absent_result.exit_code, 0);
    assert_eq!(absent_result.cycles, 4);
    assert!(!absent_result.timed_out);
    assert_eq!(absent_result.signature_addr, None);
    assert_eq!(absent_result.signature_data, None);
    assert!(absent_result.error.is_none(), "{absent_result:?}");

    let (_, empty_result) = load_and_run_library(&empty, 12);
    assert_eq!(empty_result.exit_code, 0);
    assert_eq!(empty_result.cycles, 4);
    assert_eq!(empty_result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(empty_result.signature_data, Some(Vec::new()));
    assert!(empty_result.error.is_none(), "{empty_result:?}");

    let (_, unreadable_result) = load_and_run_library(&unreadable, 12);
    assert_eq!(unreadable_result.exit_code, 0);
    assert_eq!(unreadable_result.cycles, 4);
    assert_eq!(
        unreadable_result.signature_addr,
        Some(fixture::BASE + 0x20_0000)
    );
    assert_eq!(unreadable_result.signature_data, None);
    let error = unreadable_result
        .error
        .as_deref()
        .expect("an unusable declared region must not be silently absent");
    assert!(error.contains("Signature artifact unavailable"), "{error}");
    assert!(!unreadable_result.timed_out);

    let (_, empty_unmappable_result) = load_and_run_library(&empty_and_unmappable, 12);
    assert_eq!(
        empty_unmappable_result.signature_addr,
        Some(fixture::BASE + 0x20_0000)
    );
    assert_eq!(empty_unmappable_result.signature_data, Some(Vec::new()));
    assert!(
        empty_unmappable_result.error.is_none(),
        "{empty_unmappable_result:?}"
    );
}

#[test]
fn flat_library_keeps_the_run_when_the_signature_is_unreadable() {
    let guest = padded(declared_tohost_writer(fixture::standard_exit(1)));
    let readable = fixture::elf_with_signature(
        &guest,
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::SIGNATURE, 8)),
        0,
    );
    let unreadable = fixture::elf_with_signature(
        &guest,
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::BASE + 0x20_0000, 8)),
        0,
    );

    let (good_simulator, good) = load_and_run_library(&readable, 12);
    let (bad_simulator, bad) = load_and_run_library(&unreadable, 12);

    assert_eq!(bad.exit_code, good.exit_code);
    assert_eq!(bad.cycles, good.cycles);
    assert_eq!(bad.final_pc, good.final_pc);
    assert_eq!(bad.timed_out, good.timed_out);
    assert!(good.error.is_none());
    assert_eq!(good.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(bad.signature_addr, Some(fixture::BASE + 0x20_0000));
    assert!(good.signature_data.is_some());
    assert_eq!(bad.signature_data, None);

    assert_eq!(
        bad_simulator.read_mem(0, 0x1000).unwrap(),
        good_simulator.read_mem(0, 0x1000).unwrap(),
        "reading the artifact must not change guest state"
    );

    let timeout_elf = fixture::elf_with_signature(
        &padded(vec![fixture::nop()]),
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::BASE + 0x20_0000, 8)),
        0,
    );
    let (_, timed_out) = load_and_run_library(&timeout_elf, 6);
    assert!(timed_out.timed_out);
    assert_eq!(timed_out.cycles, 6);
    let timeout_error = timed_out.error.as_deref().unwrap();
    assert!(
        timeout_error.contains("Timeout after 6 cycles"),
        "{timeout_error}"
    );
    assert!(
        timeout_error.contains("Signature artifact unavailable"),
        "the primary failure must be preserved alongside the artifact failure: {timeout_error}"
    );

    let broken_elf = fixture::elf_with_signature(
        &[0x0000_0000],
        0,
        fixture::BASE,
        Some(fixture::TOHOST),
        Some((fixture::BASE + 0x20_0000, 8)),
        0,
    );
    let (_, broken) = load_and_run_library(&broken_elf, 6);
    assert_eq!(broken.cycles, 0);
    let broken_error = broken.error.as_deref().unwrap();
    assert!(broken_error.contains("Execution error"), "{broken_error}");
    assert!(
        broken_error.contains("Signature artifact unavailable"),
        "{broken_error}"
    );
}

#[test]
fn flat_library_replaces_image_metadata_and_ram_on_a_second_load() {
    let first = fixture::elf_with_code(
        &signature_writer(
            SIGNATURE_WRITTEN_BYTE,
            fixture::SIGNATURE_SEGMENT_OFFSET as i64,
            3,
        ),
        0,
        true,
        true,
        0,
    );
    let second = fixture::elf_with_code(
        &padded(declared_tohost_writer(fixture::standard_exit(7))),
        0,
        true,
        false,
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&first).unwrap();
    let first_result = simulator.run(Some(20)).unwrap();
    assert_eq!(first_result.exit_code, 3);
    assert_eq!(first_result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(
        first_result.signature_data,
        Some(expected_signature_bytes())
    );

    simulator.load_elf(&second).unwrap();
    let second_result = simulator.run(Some(20)).unwrap();
    assert_eq!(second_result.exit_code, 7);
    assert_eq!(second_result.cycles, 4);
    assert!(!second_result.timed_out);
    assert!(second_result.error.is_none(), "{second_result:?}");
    assert_eq!(second_result.signature_addr, None);
    assert_eq!(second_result.signature_data, None);
    assert_eq!(
        simulator
            .read_mem(fixture::SIGNATURE_SEGMENT_OFFSET, 8)
            .unwrap(),
        vec![0u8; 8],
        "the second image must replace the first image's RAM"
    );

    simulator.load_elf(&first).unwrap();
    let third_result = simulator.run(Some(20)).unwrap();
    assert_eq!(third_result.exit_code, 3);
    assert_eq!(third_result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(
        third_result.signature_data,
        Some(expected_signature_bytes())
    );
}

const INTEGRATED_DATA_OFFSET: u64 = 0x100;
const INTEGRATED_DATA_VALUE: i32 = 42;
const INTEGRATED_RELOADED_VALUE: i32 = 69;
const INTEGRATED_EXIT_CODE: u32 = 42;
const INTEGRATED_CYCLES: u64 = 16;

/// Guest that stores and reloads a value, modifies RAM, writes the declared
/// signature byte, and exits through the image's declared tohost.
fn integrated_workflow_guest(exit_code: u32) -> Vec<u32> {
    let mut code = vec![
        fixture::addi(5, 0, INTEGRATED_DATA_VALUE),
        fixture::auipc(6, 0),
        fixture::addi(6, 6, 0xfc),
        fixture::sd(5, 6, 0),
        fixture::addi(7, 0, 0),
        fixture::ld(7, 6, 0),
        fixture::addi(7, 7, 27),
        fixture::sb(7, 6, 0),
    ];
    let signature_offset = code.len() * 4;
    code.extend(store_byte_at(
        signature_offset,
        SIGNATURE_WRITTEN_BYTE,
        fixture::SIGNATURE_SEGMENT_OFFSET as i64,
    ));
    let exit_offset = code.len() * 4;
    code.extend(tohost_writer_at(
        exit_offset,
        fixture::standard_exit(exit_code),
    ));
    padded(code)
}

#[test]
fn integrated_load_run_result_inspect_workflow() {
    let elf = fixture::elf_with_code(
        &integrated_workflow_guest(INTEGRATED_EXIT_CODE),
        0,
        true,
        true,
        0,
    );

    let mut simulator = RiscVSimulator::new(0x1_0000);
    let entry = simulator.load_elf(&elf).unwrap();
    assert_eq!(entry, fixture::BASE);

    let result = simulator.run(Some(32)).unwrap();

    // Result: the guest's own nonzero exit, at the writing instruction.
    assert_eq!(result.exit_code, INTEGRATED_EXIT_CODE);
    assert_eq!(result.cycles, INTEGRATED_CYCLES);
    assert_eq!(result.final_pc, fixture::BASE + INTEGRATED_CYCLES * 4);
    assert!(!result.timed_out);
    assert!(result.error.is_none(), "{result:?}");

    // Registers left by the workflow.
    let state = simulator.state();
    assert_eq!(
        state.regs[7],
        u64::try_from(INTEGRATED_RELOADED_VALUE).unwrap()
    );
    assert_eq!(state.regs[6], fixture::BASE + INTEGRATED_DATA_OFFSET);
    assert_eq!(state.pc, fixture::BASE + INTEGRATED_CYCLES * 4);

    // Inspection of the state the guest left behind, before and after.
    let before_inspection = core_snapshot(&simulator);
    let ram = simulator.read_mem(INTEGRATED_DATA_OFFSET, 8).unwrap();
    assert_eq!(
        ram,
        vec![
            u8::try_from(INTEGRATED_RELOADED_VALUE).unwrap(),
            0,
            0,
            0,
            0,
            0,
            0,
            0
        ]
    );
    assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
    assert_eq!(result.signature_data, Some(expected_signature_bytes()));
    assert_eq!(
        simulator
            .read_mem(fixture::SIGNATURE_SEGMENT_OFFSET, 8)
            .unwrap(),
        expected_signature_bytes()
    );

    assert_eq!(
        core_snapshot(&simulator),
        before_inspection,
        "inspection must not execute guest code or change PC, registers or privilege"
    );
    assert_eq!(simulator.read_mem(INTEGRATED_DATA_OFFSET, 8).unwrap(), ram);
}

#[test]
fn integrated_workflow_records_the_retained_cli_device_difference() {
    // A declared RAM tohost: the CLI and the flat library agree.
    let parity_elf = fixture::elf_with_code(
        &padded(declared_tohost_writer(fixture::standard_exit(5))),
        0,
        true,
        false,
        0,
    );
    let cli = run_fixture(&parity_elf, Some(12), None);
    let (_, library) = load_and_run_library(&parity_elf, 12);
    assert_eq!(cli.exit_code, 5);
    assert_eq!(library.exit_code, 5);
    assert_eq!(cli.cycles, library.cycles);
    assert_eq!(cli.final_pc, library.final_pc);
    assert!(!library.timed_out);
    assert!(library.error.is_none());

    // UART MMIO: the CLI device map serves it and the guest exits through HTIF;
    // the flat wrapper has no device mapping, so the same store fails instead.
    let uart_guest = padded(vec![
        fixture::lui(4, 0x10000),
        fixture::addi(5, 0, i32::from(b'A')),
        fixture::sb(5, 4, 0),
        fixture::addi(6, 0, 1),
        fixture::lui(4, 0x40008),
        fixture::sd(6, 4, 0),
    ]);
    let uart_elf = fixture::elf_with_code(&uart_guest, 0, false, false, 0);

    let cli_uart = run_fixture(&uart_elf, Some(20), None);
    assert_eq!(cli_uart.exit_code, 0);
    assert!(!cli_uart.timed_out);
    assert!(cli_uart.error.is_none(), "{cli_uart:?}");

    let (_, library_uart) = load_and_run_library(&uart_elf, 20);
    assert_eq!(library_uart.exit_code, 1);
    assert_eq!(library_uart.cycles, 2);
    assert_eq!(library_uart.final_pc, fixture::BASE + 8);
    assert!(!library_uart.timed_out);
    let error = library_uart
        .error
        .as_deref()
        .expect("the flat wrapper has no device mapping for the UART aperture");
    assert!(error.contains("Execution error"), "{error}");
}
