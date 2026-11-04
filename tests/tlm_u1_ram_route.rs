//! Blocking TLM U1: public CLI and entry-point parity for the TLM RAM route.

#[allow(dead_code)]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::public_elf as elf;
use ruscv_sim::machine::{MachineConfig, PlatformKind};
use ruscv_sim::{load_and_run_file_with_config, Transport, ZeroDelayRam};
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

/// A three-instruction guest: stage the exit payload, load the HTIF base and
/// store the payload. Returns zero through the native HTIF endpoint.
fn exit_elf() -> Vec<u8> {
    let code = vec![
        elf::standard_exit(0),
        elf::lui(4, 0x40008),
        elf::sd(5, 4, 0),
    ];
    elf::elf_with_code(&code, 0, true, false, 0)
}

fn write_elf(directory: &Path) -> std::path::PathBuf {
    let path = directory.join("tlm-exit.elf");
    std::fs::write(&path, exit_elf()).unwrap();
    path
}

fn run_cli(path: &Path, transport: Option<&str>) -> std::process::Output {
    let mut command = cargo_bin_cmd!("ruscv-sim");
    command.arg("run").arg(path).args(["--max-cycles", "8"]);
    if let Some(transport) = transport {
        command.args(["--transport", transport]);
    }
    command.output().unwrap()
}

#[test]
fn cli_native_and_tlm_transports_match_on_a_small_guest() {
    let directory = TempDir::new().unwrap();
    let path = write_elf(directory.path());
    let native = run_cli(&path, None);
    let explicit_native = run_cli(&path, Some("native"));
    let tlm = run_cli(&path, Some("tlm"));

    for (label, output) in [
        ("default", &native),
        ("native", &explicit_native),
        ("tlm", &tlm),
    ] {
        assert!(
            output.status.success(),
            "{label} transport failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        native.stdout, explicit_native.stdout,
        "--transport native changed the run facts"
    );
    assert_eq!(
        native.stdout, tlm.stdout,
        "the TLM route changed the run facts"
    );
    let stdout = String::from_utf8_lossy(&tlm.stdout);
    assert!(stdout.contains("Status:     SUCCESS"), "{stdout}");
    assert!(stdout.contains("Exit Code:  0"), "{stdout}");
}

#[test]
fn cli_rejects_an_unknown_transport() {
    let directory = TempDir::new().unwrap();
    let path = write_elf(directory.path());
    let output = cargo_bin_cmd!("ruscv-sim")
        .arg("run")
        .arg(&path)
        .args(["--transport", "systemc"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid transport"), "{stderr}");
}

#[test]
fn the_config_entry_point_runs_a_file_under_a_tlm_config() {
    let directory = TempDir::new().unwrap();
    let path = write_elf(directory.path());
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(Arc::new(ZeroDelayRam::new()));
    let result = load_and_run_file_with_config(
        path.to_str().unwrap(),
        config,
        Some(8),
        None,
        None,
        false,
        None,
    )
    .unwrap();
    assert_eq!(result.exit_code, 0);
    assert!(!result.timed_out);
    assert!(result.error.is_none(), "{:?}", result.error);
    assert_eq!(result.cycles, 3);
}

#[test]
fn the_config_entry_point_rejects_flat_with_tlm() {
    let directory = TempDir::new().unwrap();
    let path = write_elf(directory.path());
    let mut config = MachineConfig::new(PlatformKind::Flat);
    config.transport = Transport::tlm(Arc::new(ZeroDelayRam::new()));
    let error = load_and_run_file_with_config(
        path.to_str().unwrap(),
        config,
        Some(8),
        None,
        None,
        false,
        None,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("TLM transport requires"),
        "{error}"
    );
}
