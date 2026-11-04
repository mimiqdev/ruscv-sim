//! Blocking TLM U1: public CLI and entry-point parity for the TLM RAM route.

#[allow(dead_code)]
mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::public_elf as elf;
use ruscv_sim::machine::{MachineConfig, PlatformKind};
use ruscv_sim::{
    load_and_run_file_with_config, BlockingCommand, BlockingDebug, BlockingTarget,
    BlockingTransaction, Femtoseconds, Transport, ZeroDelayRam,
};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// A zero-delay RAM target that starts failing debug transport once the guest
/// issues its first blocking transaction (installation is over by then).
struct FailingHostViews {
    ram: ZeroDelayRam,
    started: AtomicBool,
    fail_writes_only: bool,
}

impl FailingHostViews {
    fn new(fail_writes_only: bool) -> Self {
        Self {
            ram: ZeroDelayRam::new(),
            started: AtomicBool::new(false),
            fail_writes_only,
        }
    }
}

impl BlockingTarget for FailingHostViews {
    fn declared_max_delay(&self) -> Femtoseconds {
        Femtoseconds::ZERO
    }

    fn b_transport(&self, transaction: &mut BlockingTransaction<'_>) {
        self.started.store(true, Ordering::SeqCst);
        self.ram.b_transport(transaction);
    }

    fn transport_dbg(&self, transaction: &mut BlockingDebug<'_>) -> usize {
        if self.started.load(Ordering::SeqCst)
            && !(self.fail_writes_only && transaction.command() == BlockingCommand::Read)
        {
            return 0;
        }
        self.ram.transport_dbg(transaction)
    }

    fn transport_reset(&self) -> bool {
        self.ram.transport_reset()
    }
}

/// A guest that stores the exit payload to a RAM-resident `.tohost`, so the
/// run loop polls and clears it instead of using the HTIF endpoint.
fn ram_exit_elf() -> Vec<u8> {
    let code = vec![
        elf::standard_exit(0),
        elf::auipc(4, 0),
        elf::addi(4, 4, -4),
        elf::addi(4, 4, 0x7ff),
        elf::addi(4, 4, 0x7ff),
        elf::addi(4, 4, 0x2),
        elf::sd(5, 4, 0),
    ];
    elf::elf_with_code(&code, 0, true, false, 0)
}

#[test]
fn a_failed_poll_transport_is_a_host_failure_not_a_guest_result() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("ram-exit.elf");
    std::fs::write(&path, ram_exit_elf()).unwrap();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(Arc::new(FailingHostViews::new(false)));
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
    assert!(!result.timed_out);
    let error = result.error.expect("a failed poll must be reported");
    assert!(error.contains("tohost read failed"), "{error}");
    assert_ne!(
        result.exit_code, 0,
        "a host failure must not look successful"
    );
}

#[test]
fn a_failed_clear_transport_retains_the_guest_code_and_reports_failure() {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("ram-exit.elf");
    std::fs::write(&path, ram_exit_elf()).unwrap();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(Arc::new(FailingHostViews::new(true)));
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
    assert!(!result.timed_out, "{result:?}");
    assert_eq!(result.exit_code, 0, "the guest exit code must be retained");
    let error = result.error.expect("a failed clear must be reported");
    assert!(error.contains("tohost clear failed"), "{error}");
    assert!(error.contains("guest exit code 0 retained"), "{error}");
}
