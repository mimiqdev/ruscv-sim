//! Bounded A4 stop-decision regressions through the public configurations.

pub mod common;

use assert_cmd::cargo::cargo_bin_cmd;
use common::public_elf as fixture;
use ruscv_sim::executor::{load_and_run, RiscVSimulator};

#[test]
fn failed_first_instruction_does_not_consume_a_pending_ram_exit() {
    let mut elf = fixture::elf_with_code(&[0], 0, true, false, 0);
    let signal = fixture::LOAD_OFFSET + fixture::TOHOST_SEGMENT_OFFSET as usize;
    elf[signal..signal + 8].copy_from_slice(&85u64.to_le_bytes());

    let cli = load_and_run(&elf, Some(1), None, None, false).unwrap();
    let mut simulator = RiscVSimulator::new(0x1_0000);
    simulator.load_elf(&elf).unwrap();
    let flat = simulator.run(Some(1)).unwrap();
    for result in [&cli, &flat] {
        assert_eq!(result.cycles, 0);
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.final_pc, fixture::BASE);
        assert!(!result.timed_out);
        assert!(result
            .error
            .as_ref()
            .unwrap()
            .starts_with("Execution error"));
    }
    assert_eq!(
        simulator
            .read_mem(fixture::TOHOST_SEGMENT_OFFSET, 8)
            .unwrap(),
        85u64.to_le_bytes()
    );
}

#[test]
fn cli_final_slot_htif_exit_skips_ram_poll_and_timeout_diagnostics() {
    let mut code = vec![fixture::standard_exit(0), fixture::lui(4, 0x40008)];
    code.resize(999, fixture::nop());
    code.push(fixture::sd(5, 4, 0));
    let elf = fixture::elf_with_code(&code, 0, true, false, 0);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("final-slot.elf");
    std::fs::write(&path, elf).unwrap();
    let output = cargo_bin_cmd!()
        .arg("run")
        .arg(path)
        .args(["--max-cycles", "1000", "--tohost", "0", "--verbose"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("[DEBUG] HTIF exit signal detected: code=0"));
    // Address zero is unmapped. Polling it in slot 1000 would emit this
    // periodic diagnostic; HTIF must short-circuit that lower-priority read.
    assert!(!stderr.contains("tohost read failed"), "{stderr}");
    assert!(!stderr.contains("[DEBUG] Cycle 1000:"), "{stderr}");
    assert!(!stderr.contains("[DEBUG] Timeout"), "{stderr}");
}
