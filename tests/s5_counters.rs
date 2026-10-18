//! Stage 5 U1 guest on the native CLI and the native library.
//!
//! Flat has no CLINT window (contract D9), so it is not a route for this guest.

use ruscv_sim::executor::load_and_run;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const GUEST: &str = "s5_counters";
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

#[test]
fn s5_counters_passes_on_the_native_cli_and_library() {
    let Some((assembler, linker)) = required_toolchain() else {
        let message = "riscv64-unknown-elf-as/ld unavailable; s5_counters skipped";
        if env::var_os("RISCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
            panic!("{message}; CI requested a real guest run");
        }
        eprintln!("[SKIP] {message}");
        return;
    };

    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = repository
        .join("tests/bare-metal-riscv-test/rv64i")
        .join(format!("{GUEST}.S"));
    let linker_script = repository
        .join("tests/bare-metal-riscv-test")
        .join(LINKER_SCRIPT);
    let temp = TempDir::new().expect("temporary s5 guest build directory");
    let object = temp.path().join(format!("{GUEST}.o"));
    let elf = temp.path().join(format!("{GUEST}.elf"));

    let assembled = Command::new(&assembler)
        .args(["-march=rv64ima_zicsr", "-mabi=lp64"])
        .arg(&source)
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

    let cli = PathBuf::from(env!("CARGO_BIN_EXE_ruscv-sim"));
    let cli_result = Command::new(&cli)
        .args(["run", elf.to_str().expect("UTF-8 guest path")])
        .args(["--max-cycles", &MAX_CYCLES.to_string()])
        .output()
        .unwrap_or_else(|error| panic!("failed to run CLI: {error}"));
    assert!(
        cli_result.status.success(),
        "CLI s5_counters failed with {}:\nstdout:\n{}\nstderr:\n{}",
        cli_result.status,
        String::from_utf8_lossy(&cli_result.stdout),
        String::from_utf8_lossy(&cli_result.stderr)
    );

    let elf_data = std::fs::read(&elf).expect("fresh guest ELF");
    let library = load_and_run(&elf_data, Some(MAX_CYCLES), None, None, false)
        .unwrap_or_else(|error| panic!("load_and_run failed: {error}"));
    assert_eq!(library.exit_code, 0);
    assert!(!library.timed_out);
    assert!(library.error.is_none(), "{:?}", library.error);
}
