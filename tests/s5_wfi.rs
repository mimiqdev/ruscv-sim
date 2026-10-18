//! Stage 5 U4 WFI guests on the native CLI and the native library.
//!
//! Flat is not a route (contract D9). Privileged §3.3.3.

use ruscv_sim::executor::load_and_run;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const LINKER_SCRIPT: &str = "linker-s4.ld";
const MAX_CYCLES: u64 = 100_000;

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn required_toolchain() -> Option<(String, String)> {
    let prefix = env::var("RISCV_PREFIX").unwrap_or_else(|_| "riscv64-unknown-elf-".to_string());
    let assembler = format!("{prefix}as");
    let linker = format!("{prefix}ld");
    if command_available(&assembler) && command_available(&linker) {
        Some((assembler, linker))
    } else {
        None
    }
}

fn skip_without_toolchain(guest: &str) -> bool {
    if required_toolchain().is_some() {
        return false;
    }
    let message = format!("riscv64-unknown-elf-as/ld unavailable; {guest} skipped");
    if env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
        panic!("{message}; CI requested a real guest run");
    }
    eprintln!("[SKIP] {message}");
    true
}

fn build_elf(guest: &str, source: &Path) -> (TempDir, PathBuf) {
    let (assembler, linker) = required_toolchain().expect("toolchain checked by the caller");
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let linker_script = repository
        .join("tests/bare-metal-riscv-test")
        .join(LINKER_SCRIPT);
    let temp = TempDir::new().expect("temporary s5 guest build directory");
    let object = temp.path().join(format!("{guest}.o"));
    let elf = temp.path().join(format!("{guest}.elf"));
    let assembled = Command::new(&assembler)
        .args(["-march=rv64ima_zicsr", "-mabi=lp64"])
        .arg(source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke {assembler}: {error}"));
    assert!(
        assembled.status.success(),
        "assembling {} failed:\n{}",
        source.display(),
        String::from_utf8_lossy(&assembled.stderr)
    );
    let linked = Command::new(&linker)
        .arg(format!("-T{}", linker_script.display()))
        .arg(&object)
        .arg("-o")
        .arg(&elf)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke {linker}: {error}"));
    assert!(
        linked.status.success(),
        "linking {} failed:\n{}",
        Path::new(LINKER_SCRIPT).display(),
        String::from_utf8_lossy(&linked.stderr)
    );
    (temp, elf)
}

fn source_path(guest: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/bare-metal-riscv-test/rv64i")
        .join(format!("{guest}.S"))
}

fn run_elf(elf: &Path) -> (std::process::Output, ruscv_sim::executor::ExecutionResult) {
    let cli = PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"));
    let cli_result = Command::new(&cli)
        .args(["run", elf.to_str().expect("UTF-8 guest path")])
        .args(["--max-cycles", &MAX_CYCLES.to_string()])
        .output()
        .unwrap_or_else(|error| panic!("failed to run CLI: {error}"));
    let elf_data = std::fs::read(elf).expect("fresh guest ELF");
    let library = load_and_run(&elf_data, Some(MAX_CYCLES), None, None, false)
        .unwrap_or_else(|error| panic!("load_and_run failed: {error}"));
    (cli_result, library)
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn s5_wfi_event_passes_on_the_native_cli_and_library() {
    const GUEST: &str = "s5_wfi_event";
    if skip_without_toolchain(GUEST) {
        return;
    }
    let (_temp, elf) = build_elf(GUEST, &source_path(GUEST));
    let (cli_result, library) = run_elf(&elf);
    assert!(
        cli_result.status.success(),
        "CLI {GUEST} failed with {}:\nstdout:\n{}\nstderr:\n{}",
        cli_result.status,
        stdout(&cli_result),
        String::from_utf8_lossy(&cli_result.stderr)
    );
    assert_eq!(library.exit_code, 0);
    assert!(!library.timed_out);
    assert!(library.error.is_none(), "{:?}", library.error);
    assert!(
        library.cycles < 0x800,
        "the idle jump counted as cycles: {}",
        library.cycles
    );
}

#[test]
fn s5_wfi_event_negative_mutation_fails_its_check() {
    const GUEST: &str = "s5_wfi_event";
    if skip_without_toolchain(GUEST) {
        return;
    }
    let original = std::fs::read_to_string(source_path(GUEST)).expect("guest source");
    const NEEDLE: &str = "    addi    t1, t1, 7";
    const MUTATED: &str = "    addi    t1, t1, 3";
    assert_eq!(original.matches(NEEDLE).count(), 1);
    let temp = TempDir::new().expect("mutated s5 source directory");
    let source = temp.path().join(format!("{GUEST}.S"));
    std::fs::write(&source, original.replacen(NEEDLE, MUTATED, 1)).expect("write mutated source");
    let (_build, elf) = build_elf(GUEST, &source);
    let (cli_result, library) = run_elf(&elf);
    assert!(
        !cli_result.status.success(),
        "mutated {GUEST} passed the CLI:\n{}",
        stdout(&cli_result)
    );
    assert_ne!(library.exit_code, 0);
    assert!(!library.timed_out);
    assert!(library.error.is_none(), "{:?}", library.error);
}

#[test]
fn s5_wfi_masked_passes_on_the_native_cli_and_library() {
    const GUEST: &str = "s5_wfi_masked";
    if skip_without_toolchain(GUEST) {
        return;
    }
    let (_temp, elf) = build_elf(GUEST, &source_path(GUEST));
    let (cli_result, library) = run_elf(&elf);
    assert!(
        cli_result.status.success(),
        "CLI {GUEST} failed with {}:\nstdout:\n{}\nstderr:\n{}",
        cli_result.status,
        stdout(&cli_result),
        String::from_utf8_lossy(&cli_result.stderr)
    );
    assert_eq!(library.exit_code, 0);
    assert!(!library.timed_out);
    assert!(library.error.is_none(), "{:?}", library.error);
    assert!(
        library.cycles < 0x800,
        "the idle jump counted as cycles: {}",
        library.cycles
    );
}

#[test]
fn s5_wfi_masked_negative_mutation_fails_its_check() {
    const GUEST: &str = "s5_wfi_masked";
    if skip_without_toolchain(GUEST) {
        return;
    }
    let original = std::fs::read_to_string(source_path(GUEST)).expect("guest source");
    const NEEDLE: &str = "    andi    t0, t0, 0x80";
    const MUTATED: &str = "    andi    t0, t0, 0x8";
    assert_eq!(original.matches(NEEDLE).count(), 1);
    let temp = TempDir::new().expect("mutated s5 source directory");
    let source = temp.path().join(format!("{GUEST}.S"));
    std::fs::write(&source, original.replacen(NEEDLE, MUTATED, 1)).expect("write mutated source");
    let (_build, elf) = build_elf(GUEST, &source);
    let (cli_result, library) = run_elf(&elf);
    assert!(
        !cli_result.status.success(),
        "mutated {GUEST} passed the CLI:\n{}",
        stdout(&cli_result)
    );
    assert_ne!(library.exit_code, 0);
    assert!(!library.timed_out);
    assert!(library.error.is_none(), "{:?}", library.error);
}

#[test]
fn s5_wfi_none_stops_with_no_progress() {
    const GUEST: &str = "s5_wfi_none";
    if skip_without_toolchain(GUEST) {
        return;
    }
    let (_temp, elf) = build_elf(GUEST, &source_path(GUEST));
    let (cli_result, library) = run_elf(&elf);
    let text = stdout(&cli_result);
    assert!(
        !cli_result.status.success(),
        "a no-event WFI was reported as success:\n{text}"
    );
    assert!(
        text.contains("Error:      NoProgress"),
        "CLI did not report NoProgress:\n{text}"
    );
    assert!(
        !text.contains("TIMEOUT"),
        "NoProgress was reported as a timeout:\n{text}"
    );
    assert_eq!(library.exit_code, 1);
    assert!(!library.timed_out);
    assert_eq!(library.error.as_deref(), Some("NoProgress"));
    // auipc, addi, csrw mtvec, addi, csrw mie, addi, and the WFI retirement.
    // The waiting turn starts a slot and does not complete it.
    assert_eq!(
        library.cycles, 7,
        "NoProgress cycles are the turns completed"
    );
    assert!(
        text.contains("Cycles:     7"),
        "CLI cycle count is not the turns completed:\n{text}"
    );
}

#[test]
fn s5_wfi_none_negative_mutation_is_not_no_progress() {
    // MTIE with the reset compare (all-ones) is a future event. MIE stays
    // clear, so the wake continues at the instruction after WFI and the
    // guest writes a failing tohost. That is not NoProgress.
    const GUEST: &str = "s5_wfi_none";
    if skip_without_toolchain(GUEST) {
        return;
    }
    let original = std::fs::read_to_string(source_path(GUEST)).expect("guest source");
    const NEEDLE: &str = "    addi    t0, zero, 0";
    const MUTATED: &str = "    addi    t0, zero, 0x80";
    assert_eq!(original.matches(NEEDLE).count(), 1);
    let temp = TempDir::new().expect("mutated s5 source directory");
    let source = temp.path().join(format!("{GUEST}.S"));
    std::fs::write(&source, original.replacen(NEEDLE, MUTATED, 1)).expect("write mutated source");
    let (_build, elf) = build_elf(GUEST, &source);
    let (cli_result, library) = run_elf(&elf);
    let text = stdout(&cli_result);
    assert!(
        !cli_result.status.success(),
        "mutated {GUEST} passed:\n{text}"
    );
    assert!(
        !text.contains("NoProgress"),
        "the future compare was still NoProgress:\n{text}"
    );
    assert_ne!(library.exit_code, 0);
    assert!(!library.timed_out);
    assert!(library.error.is_none(), "{:?}", library.error);
    assert!(
        library.cycles < 0x800,
        "the jump to the reset compare counted as cycles: {}",
        library.cycles
    );
}
