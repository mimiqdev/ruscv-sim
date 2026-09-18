//! A6 Task 4 fresh guest-ELF integration.
//!
//! This test deliberately assembles and links the five trap guests into a
//! temporary directory for every invocation.  It exercises both public Rust
//! entry points and the public `ruscv-sim run` binary; a pre-existing ELF is
//! never accepted as a substitute for the current assembly sources.

use ruscv_sim::executor::{load_and_run, RiscVSimulator};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const GUESTS: [&str; 5] = [
    "trap_ecall",
    "trap_illegal",
    "trap_ebreak",
    "trap_vectored",
    "trap_mret_priv",
];
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

fn assemble_and_link(
    assembler: &str,
    linker: &str,
    source: &Path,
    linker_script: &Path,
    output_dir: &Path,
    name: &str,
) -> PathBuf {
    let object = output_dir.join(format!("{name}.o"));
    let elf = output_dir.join(format!("{name}.elf"));

    let assembled = Command::new(assembler)
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

    let linked = Command::new(linker)
        .arg(format!("-T{}", linker_script.display()))
        .arg(&object)
        .arg("-o")
        .arg(&elf)
        .output()
        .unwrap_or_else(|error| panic!("failed to invoke {linker}: {error}"));
    assert!(
        linked.status.success(),
        "linking {} failed:\n{}",
        source.display(),
        String::from_utf8_lossy(&linked.stderr)
    );
    assert!(elf.is_file(), "linker did not produce {}", elf.display());
    elf
}

fn cli_binary() -> PathBuf {
    if let Some(binary) = env::var_os("CARGO_BIN_EXE_ruscv-sim") {
        return PathBuf::from(binary);
    }

    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/ruscv-sim");
    assert!(
        fallback.is_file(),
        "Cargo did not provide CARGO_BIN_EXE_ruscv-sim and fallback binary is absent at {}",
        fallback.display()
    );
    fallback
}

#[test]
fn a6_trap_guests_pass_through_library_and_cli() {
    let Some((assembler, linker)) = required_toolchain() else {
        let message = "riscv64-unknown-elf-as/ld unavailable; A6 trap ELF integration skipped";
        if env::var_os("RUSCV_REQUIRE_RISCV_TOOLCHAIN").is_some() {
            panic!("{message}; CI requested a real guest run");
        }
        eprintln!("[SKIP] {message}");
        return;
    };

    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_dir = repository.join("tests/bare-metal-riscv-test/rv64i");
    let linker_script = repository.join("tests/bare-metal-riscv-test/linker.ld");
    let temp = TempDir::new().expect("temporary A6 guest build directory");
    let cli = cli_binary();

    for name in GUESTS {
        let source = source_dir.join(format!("{name}.S"));
        let elf = assemble_and_link(
            &assembler,
            &linker,
            &source,
            &linker_script,
            temp.path(),
            name,
        );
        let elf_data = std::fs::read(&elf).expect("fresh guest ELF");

        let cli_result = Command::new(&cli)
            .args(["run", elf.to_str().expect("UTF-8 guest path")])
            .args(["--max-cycles", &MAX_CYCLES.to_string()])
            .output()
            .unwrap_or_else(|error| panic!("failed to run CLI for {name}: {error}"));
        assert!(
            cli_result.status.success(),
            "CLI guest {name} failed with {}:\nstdout:\n{}\nstderr:\n{}",
            cli_result.status,
            String::from_utf8_lossy(&cli_result.stdout),
            String::from_utf8_lossy(&cli_result.stderr)
        );

        let cli_api_result = load_and_run(&elf_data, Some(MAX_CYCLES), None, None, false)
            .unwrap_or_else(|error| panic!("load_and_run failed for {name}: {error}"));
        assert_eq!(cli_api_result.exit_code, 0, "library CLI path: {name}");
        assert!(
            !cli_api_result.timed_out,
            "library CLI path timed out: {name}"
        );
        assert!(
            cli_api_result.error.is_none(),
            "library CLI path error for {name}: {:?}",
            cli_api_result.error
        );

        let mut flat = RiscVSimulator::new(0x20_000);
        flat.load_elf(&elf_data)
            .unwrap_or_else(|error| panic!("flat facade load failed for {name}: {error}"));
        let flat_result = flat
            .run(Some(MAX_CYCLES))
            .unwrap_or_else(|error| panic!("flat facade run failed for {name}: {error}"));
        assert_eq!(flat_result.exit_code, 0, "flat facade: {name}");
        assert!(!flat_result.timed_out, "flat facade timed out: {name}");
        assert!(
            flat_result.error.is_none(),
            "flat facade error for {name}: {:?}",
            flat_result.error
        );

        eprintln!("[PASS] {name}: CLI process, load_and_run, and RiscVSimulator exit 0");
    }
}
