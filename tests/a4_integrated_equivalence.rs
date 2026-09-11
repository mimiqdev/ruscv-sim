//! Integrated A4 evidence for the two public ELF entry loops, not RiscvCore::run.

pub mod common;

use common::public_elf as fixture;
use ruscv_sim::executor::{load_and_run, RiscVSimulator};

const ENTRY_OFFSET: usize = 0x180;
const EXIT_CYCLES: u64 = 11;

/// Loaded data, rather than an immediate exit payload, determines the result.
/// The zero-filled bytes at the segment base are deliberately not executable:
/// confusing the nonzero entry offset with the base must fail this workflow.
fn data_workflow_elf(entry_offset: usize, increment: i32, fail_at_exit: bool) -> Vec<u8> {
    let entry = i32::try_from(entry_offset).unwrap();
    let code = [
        fixture::auipc(6, 2),
        fixture::addi(6, 6, -entry), // x6 = guest signature address
        fixture::lbu(5, 6, 1),       // file-backed seed = 17
        fixture::addi(5, 5, increment),
        fixture::sd(5, 6, 0), // overwrite all eight artifact bytes
        fixture::ld(7, 6, 0), // reload the stored value to form the exit
        fixture::slli(7, 7, 1),
        fixture::ori(7, 7, 1),
        fixture::auipc(4, 1),
        fixture::addi(4, 4, -entry - 32), // x4 = guest tohost address
        if fail_at_exit {
            0 // fail after the data effects, without retiring this instruction
        } else {
            fixture::sd(7, 4, 0)
        },
        0, // an exit must stop before this invalid instruction
    ];
    fixture::elf_with_code(&code, entry_offset, true, true, fixture::BSS_MEMORY_SIZE)
}

#[test]
fn nonzero_entry_data_workflow_agrees_at_run_control_boundaries() {
    // Each row passes exactly the same ELF bytes and budget to both public APIs.
    // Assert known outcomes, not just equality (both paths could be wrong).
    for (budget, fail_at_exit, cycles, exit_code, timed_out) in [
        (0, false, 0, 1, true),
        (10, false, 10, 1, true),
        (11, false, EXIT_CYCLES, 42, false),
        (12, false, EXIT_CYCLES, 42, false),
        (11, true, 10, 1, false),
    ] {
        let elf = data_workflow_elf(ENTRY_OFFSET, 25, fail_at_exit);
        let cli = load_and_run(&elf, Some(budget), None, None, false).unwrap();
        let mut simulator = RiscVSimulator::new(0x1_0000);
        let entry = simulator.load_elf(&elf).unwrap();
        assert_eq!(entry, fixture::BASE + ENTRY_OFFSET as u64);
        assert_eq!(simulator.state().pc, entry);
        let flat = simulator.run(Some(budget)).unwrap();
        let expected_bytes = if budget == 0 {
            fixture::SIGNATURE_BYTES
        } else {
            42u64.to_le_bytes()
        };

        for result in [&cli, &flat] {
            assert_eq!(result.cycles, cycles, "{result:?}");
            assert_eq!(result.exit_code, exit_code, "{result:?}");
            assert_eq!(result.final_pc, entry + cycles * 4, "{result:?}");
            assert_eq!(result.timed_out, timed_out, "{result:?}");
            assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
            assert_eq!(result.signature_data.as_deref(), Some(&expected_bytes[..]));
            if timed_out {
                assert_eq!(
                    result.error.as_deref(),
                    Some(format!("Timeout after {budget} cycles").as_str())
                );
            } else if fail_at_exit {
                assert!(result
                    .error
                    .as_ref()
                    .unwrap()
                    .starts_with("Execution error"));
            } else {
                assert!(result.error.is_none(), "{result:?}");
            }
        }
        if fail_at_exit {
            // Retain the documented diagnostic difference; final_pc is shared.
            assert!(cli.error.as_ref().unwrap().contains(" at PC "));
            assert!(!flat.error.as_ref().unwrap().contains(" at PC "));
        }
        assert_eq!(simulator.state().regs[0], 0);
        assert_eq!(simulator.state().regs[7], if budget == 0 { 0 } else { 85 });
        assert_eq!(
            simulator
                .read_mem(fixture::SIGNATURE_SEGMENT_OFFSET, 8)
                .unwrap(),
            expected_bytes
        );
        // In exit rows the nonzero code survived consuming/clearing the signal.
        assert_eq!(
            simulator
                .read_mem(fixture::TOHOST_SEGMENT_OFFSET, 8)
                .unwrap(),
            [0; 8]
        );
    }
}

#[test]
fn replacement_load_restores_data_and_entry_before_the_same_public_workflow() {
    let mut simulator = RiscVSimulator::new(0x1_0000);
    let first = data_workflow_elf(ENTRY_OFFSET, 25, false);
    simulator.load_elf(&first).unwrap();
    assert_eq!(simulator.run(Some(EXIT_CYCLES)).unwrap().exit_code, 42);
    // Make prior RAM and pending-exit state observably unlike a fresh image.
    simulator
        .write_mem(fixture::BSS_PROBE_OFFSET as u64, &[0xa5])
        .unwrap();
    simulator
        .write_mem(fixture::TOHOST_SEGMENT_OFFSET, &199u64.to_le_bytes())
        .unwrap();
    simulator.set_tohost(fixture::TOHOST_SEGMENT_OFFSET + 8);

    let replacement_offset = 0x240;
    let replacement = data_workflow_elf(replacement_offset, 26, false);
    let entry = simulator.load_elf(&replacement).unwrap();
    assert_eq!(entry, fixture::BASE + replacement_offset as u64);
    assert_eq!(simulator.state().pc, entry);
    assert_eq!(simulator.state().regs, [0; 32]);
    assert_eq!(
        simulator
            .read_mem(fixture::SIGNATURE_SEGMENT_OFFSET, 8)
            .unwrap(),
        fixture::SIGNATURE_BYTES
    );
    assert_eq!(
        simulator
            .read_mem(fixture::TOHOST_SEGMENT_OFFSET, 8)
            .unwrap(),
        [0; 8]
    );
    assert_eq!(
        simulator
            .read_mem(fixture::BSS_PROBE_OFFSET as u64, 1)
            .unwrap(),
        [0]
    );
    assert_eq!(
        simulator.read_mem(ENTRY_OFFSET as u64, 4).unwrap(),
        [0; 4],
        "the old entry's code must not survive replacement"
    );

    let cli = load_and_run(&replacement, Some(EXIT_CYCLES), None, None, false).unwrap();
    let flat = simulator.run(Some(EXIT_CYCLES)).unwrap();
    for result in [&cli, &flat] {
        assert_eq!(result.exit_code, 43, "{result:?}");
        assert_eq!(result.cycles, EXIT_CYCLES);
        assert_eq!(result.final_pc, entry + EXIT_CYCLES * 4);
        assert!(!result.timed_out);
        assert!(result.error.is_none(), "{result:?}");
        assert_eq!(result.signature_addr, Some(fixture::SIGNATURE));
        assert_eq!(result.signature_data, Some(43u64.to_le_bytes().to_vec()));
    }
    assert_eq!(simulator.state().regs[7], 87);
    assert_eq!(
        simulator
            .read_mem(fixture::TOHOST_SEGMENT_OFFSET, 8)
            .unwrap(),
        [0; 8]
    );
}
