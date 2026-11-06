//! Quantum selection on the public CLI (quantum D2, D7, D10).
#![cfg(unix)]

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod public_elf;

use std::path::PathBuf;
use std::process::{Command, Output};

/// A guest that runs four instructions and exits through HTIF, so the TLM
/// route installs the image, fetches parcels and completes a store before the
/// run ends. The run has several completed turns and no device access, so a
/// quantum can only sync at the report's forced points.
fn write_guest(dir: &std::path::Path) -> PathBuf {
    let code = [
        public_elf::auipc(5, 1),
        public_elf::addi(6, 0, 1),
        public_elf::slli(6, 6, 63),
        public_elf::sd(6, 5, 0),
    ];
    let elf_path = dir.join("guest.elf");
    let elf = public_elf::elf_with_code(&code, 0, true, false, 0x4000);
    std::fs::write(&elf_path, elf).unwrap();
    elf_path
}

fn run(elf: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ruscv-sim"))
        .arg("run")
        .arg(elf)
        .args(["--max-cycles", "64"])
        .args(args)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The single `Quantum:` line, or `None` when the report has none.
fn quantum_line(output: &str) -> Option<&str> {
    output.lines().find(|line| line.starts_with("Quantum:"))
}

/// One `Name:     value` integer from the result block.
fn reported(output: &str, name: &str) -> u64 {
    let prefix = format!("{name}:");
    let line = output
        .lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no {name} line in:\n{output}"));
    line[prefix.len()..]
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|e| panic!("{name} is not an integer: {e}\n{output}"))
}

/// The stdout with the report's `Quantum:` line removed, so a selected quantum
/// can be compared against the parity run's otherwise identical report.
fn without_quantum_line(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with("Quantum:"))
        .map(|line| format!("{line}\n"))
        .collect()
}

// quantum D2: the native route has no local offset, so the option is refused
// with status 2 whenever the TLM route is not selected, `--quantum 0` included.
#[test]
fn the_quantum_option_requires_the_tlm_transport() {
    let temp = tempfile::tempdir().unwrap();
    let elf = write_guest(temp.path());
    for (transport, ticks) in [
        (None, "1"),
        (Some("native"), "1"),
        (None, "0"),
        (Some("native"), "1024"),
    ] {
        let mut args = vec!["--quantum", ticks];
        if let Some(name) = transport {
            args.extend(["--transport", name]);
        }
        let output = run(&elf, &args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "--quantum {ticks} without the TLM route was accepted: {}",
            stdout(&output)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--quantum requires --transport tlm"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// quantum D7/D10: `Q` = 0 prints no line and keeps the TLM report byte
// identical to the native one; `Q` = 1 adds the one line and nothing else.
#[test]
fn the_report_adds_one_quantum_line() {
    let temp = tempfile::tempdir().unwrap();
    let elf = write_guest(temp.path());
    let native = run(&elf, &[]);
    let tlm_zero = run(&elf, &["--transport", "tlm", "--quantum", "0"]);
    let tlm_one = run(&elf, &["--transport", "tlm", "--quantum", "1"]);
    assert_eq!(tlm_zero.status.code(), native.status.code());
    assert_eq!(tlm_one.status.code(), native.status.code());
    assert_eq!(
        stdout(&native),
        stdout(&tlm_zero),
        "Q = 0 changed the TLM report"
    );
    assert_eq!(quantum_line(&stdout(&native)), None);
    assert_eq!(quantum_line(&stdout(&tlm_zero)), None);
    let one = stdout(&tlm_one);
    assert_eq!(
        without_quantum_line(&one),
        stdout(&native),
        "Q = 1 changed the report beyond its Quantum line"
    );
    let line = quantum_line(&one).expect("Q = 1 must report its facts");
    assert!(line.contains("1 ticks"), "{line}");
    assert!(
        line.contains("latency bound 0 ticks"),
        "Q = 1 must be precise: {line}"
    );
    assert!(!line.contains("reduced accuracy"), "{line}");
}

// quantum D8/D10: a selected `Q` >= 2 reports the declared bound and the
// reduced-accuracy request, and an all-RAM run syncs less often than it
// completes turns.
#[test]
fn a_reduced_accuracy_run_reports_the_bound_and_syncs_below_turns() {
    let temp = tempfile::tempdir().unwrap();
    let elf = write_guest(temp.path());
    let output = run(&elf, &["--transport", "tlm", "--dmi", "--quantum", "1024"]);
    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
    let report = stdout(&output);
    let line = quantum_line(&report).expect("a selected quantum must report its facts");
    assert!(line.contains("1024 ticks"), "{line}");
    assert!(line.contains("latency bound 1023 ticks"), "{line}");
    assert!(line.contains("reduced accuracy"), "{line}");
    let syncs = line
        .split_whitespace()
        .nth(3)
        .expect("the sync count")
        .parse::<u64>()
        .unwrap();
    let turns = reported(&report, "Cycles");
    assert!(
        syncs < turns,
        "the RAM-only run synced {syncs} times in {turns} completed turns"
    );
}
