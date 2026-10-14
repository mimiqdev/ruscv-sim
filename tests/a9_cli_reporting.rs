//! R1: reporting failure outranks successful guest exit in the actual CLI.
#![cfg(unix)]

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod public_elf;

use std::process::{Command, Output};

/// Write a one-NOP ELF whose initial tohost word already holds `tohost`.
fn write_exit_elf(dir: &std::path::Path, tohost: u64) -> std::path::PathBuf {
    let elf_path = dir.join("exit.elf");
    let mut elf = public_elf::elf_with_code(&[public_elf::nop()], 0, true, false, 0x4000);
    let offset = public_elf::LOAD_OFFSET + public_elf::TOHOST_SEGMENT_OFFSET as usize;
    elf[offset..offset + 8].copy_from_slice(&tohost.to_le_bytes());
    std::fs::write(&elf_path, elf).unwrap();
    elf_path
}

fn run_zero_exit(failing_sink: bool) -> Output {
    let temp = tempfile::tempdir().unwrap();
    let elf_path = write_exit_elf(temp.path(), 1);
    let log_path = temp.path().join("commits.log");

    let binary = env!("CARGO_BIN_EXE_ruscv-sim");
    let mut command = if failing_sink {
        // Apply a zero regular-file size limit to this child only. File creation
        // still succeeds, but the first log write fails deterministically. Ignore
        // SIGXFSZ so Rust receives the I/O error instead of a process signal.
        // This works without /dev/full, timing races, or a production test hook.
        let mut shell = Command::new("/bin/sh");
        shell.args([
            "-c",
            "trap '' XFSZ && ulimit -f 0 && exec \"$@\"",
            "a9-log-error",
            binary,
        ]);
        shell
    } else {
        Command::new(binary)
    };
    let output = command
        .arg("run")
        .arg(&elf_path)
        .args(["--max-cycles", "1", "--log-commits"])
        .arg(&log_path)
        .output()
        .unwrap();
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    if failing_sink {
        assert_eq!(std::fs::metadata(log_path).unwrap().len(), 0);
    } else {
        assert_eq!(
            std::fs::read_to_string(log_path).unwrap().lines().count(),
            1
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Exit Code:  0"), "{stdout}");
    assert!(stdout.contains("Cycles:     1"), "{stdout}");
    assert!(
        stdout.contains(&format!("Final PC:   0x{:016x}", public_elf::BASE + 4)),
        "{stdout}"
    );
    assert!(!stdout.contains("TIMEOUT"), "{stdout}");
    output
}

#[test]
fn zero_guest_exit_with_failing_log_is_failed_and_exits_nonzero() {
    let output = run_zero_exit(true);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(stdout.contains("Status:     FAILED"), "{stdout}");
    assert!(!stdout.contains("SUCCESS"), "{stdout}");
    assert!(stdout.contains("Commit log reporting failure"), "{stdout}");
    assert!(stdout.contains("guest exit code 0 retained"), "{stdout}");
}

#[test]
fn zero_guest_exit_with_working_log_remains_successful() {
    let output = run_zero_exit(false);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("Status:     SUCCESS"), "{stdout}");
    assert!(!stdout.contains("Error:"), "{stdout}");
}

/// A failing guest code whose low byte is zero must not exit 0: the host keeps
/// only 8 bits of the process status.
#[test]
fn failing_guest_code_with_zero_low_byte_exits_nonzero() {
    for code in [0x100u32, 0x1000_0400] {
        let temp = tempfile::tempdir().unwrap();
        // HTIF exit: device 0, cmd 0, payload (code << 1) | 1.
        let elf_path = write_exit_elf(temp.path(), (u64::from(code) << 1) | 1);
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ruscv-sim"))
            .arg("run")
            .arg(&elf_path)
            .args(["--max-cycles", "1"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains(&format!("Exit Code:  {code}\n")),
            "{stdout}"
        );
        assert!(stdout.contains("Status:     FAILED"), "{stdout}");
        assert!(!stdout.contains("SUCCESS"), "{stdout}");
        assert_eq!(output.status.code(), Some(1), "{code:#x}: {stdout}");
    }
}
