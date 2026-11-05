//! Direct-memory selection on the public CLI (direct-memory D2/D14).
#![cfg(unix)]

#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod public_elf;

use std::path::PathBuf;
use std::process::{Command, Output};

/// A guest that runs four instructions and then exits through HTIF, so the TLM
/// route installs the image, fetches parcels and completes a store before the
/// run ends.
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

#[test]
fn direct_memory_requires_the_tlm_transport() {
    let temp = tempfile::tempdir().unwrap();
    let elf = write_guest(temp.path());
    for transport in [None, Some("native")] {
        let mut args = vec!["--dmi"];
        if let Some(name) = transport {
            args.extend(["--transport", name]);
        }
        let output = run(&elf, &args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "--dmi without the TLM route was accepted: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--dmi requires --transport tlm"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn the_guest_result_is_identical_with_and_without_direct_memory() {
    let temp = tempfile::tempdir().unwrap();
    let elf = write_guest(temp.path());
    let native = run(&elf, &[]);
    let tlm = run(&elf, &["--transport", "tlm"]);
    let direct = run(&elf, &["--transport", "tlm", "--dmi"]);
    assert_eq!(
        native.status.code(),
        direct.status.code(),
        "the host status differs: {}",
        String::from_utf8_lossy(&direct.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&tlm.stdout),
        "the TLM route changed the reported result"
    );
    assert_eq!(
        String::from_utf8_lossy(&tlm.stdout),
        String::from_utf8_lossy(&direct.stdout),
        "enabling direct memory changed the reported result"
    );
    assert!(
        String::from_utf8_lossy(&direct.stdout).contains("Cycles:"),
        "the run produced no result: {}",
        String::from_utf8_lossy(&direct.stdout)
    );
}
