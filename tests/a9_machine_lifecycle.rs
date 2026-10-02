//! Direct N=1 owner evidence, not migrated public-facade or fresh guest evidence.
#[allow(dead_code)]
#[path = "common/public_elf.rs"]
mod elf;
use ruscv_sim::core::observation::{Observation, ObservationSink};
use ruscv_sim::core::{CoreState, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::elf::{load_elf_file, LoadImage};
use ruscv_sim::machine::{
    Lifecycle, Machine, MachineConfig, MachineError, PlatformEvent, PlatformKind,
};
use ruscv_sim::{Fpr, PrivilegeMode};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

const TARGET: u64 = 0x200;
fn image(code: &[u32]) -> Arc<LoadImage> {
    Arc::new(
        LoadImage::parse(&elf::elf_with_code(
            code,
            0,
            true,
            true,
            elf::BSS_MEMORY_SIZE,
        ))
        .unwrap(),
    )
}
fn owner(kind: PlatformKind, image: Arc<LoadImage>) -> Machine {
    let machine = Machine::new(MachineConfig::new(kind));
    machine.install(image).unwrap();
    machine
}
fn edit(owner: &Machine, edit: impl FnOnce(&mut CoreState)) {
    let mut state = owner.inspect().unwrap().hart;
    edit(&mut state);
    owner.set_hart_state(state).unwrap();
}
fn atomic(kind: u32, rd: u32, rs2: u32) -> u32 {
    (kind << 27) | (rs2 << 20) | (1 << 15) | (3 << 12) | (rd << 7) | 0x2f
}
fn drained(owner: &Machine) {
    owner.request_quiesce().unwrap();
    assert_eq!(
        owner.try_drain().unwrap().lifecycle,
        Lifecycle::DrainComplete
    );
    assert!(owner.status().unwrap().work.is_empty());
}

#[test]
fn metadata_placement_fresh_reset_zero_fill_and_rerun_match_native_and_flat() {
    let code = [
        elf::auipc(1, 0),
        elf::addi(1, 1, TARGET as i32),
        elf::addi(2, 0, 99),
        elf::sd(2, 1, 0),
        atomic(2, 3, 0),
    ];
    let bytes = elf::elf_with_code(&code, 0, true, true, elf::BSS_MEMORY_SIZE);
    let legacy = load_elf_file(&bytes).unwrap();
    let description = Arc::new(LoadImage::parse(&bytes).unwrap());
    assert_eq!(description.entry_point(), legacy.entry_point);
    assert_eq!(description.base_addr(), legacy.base_addr);
    assert_eq!(description.memory_size(), legacy.memory.len());
    assert_eq!(
        description.segments()[0].zero_fill,
        elf::BSS_MEMORY_SIZE - elf::FILE_DATA_SIZE
    );
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let owner = owner(kind, description.clone());
        assert_eq!(
            owner.read_mem(0, legacy.memory.len()).unwrap(),
            legacy.memory
        );
        let initial = owner.inspect().unwrap();
        assert!(Arc::ptr_eq(&initial.image, &description));
        assert_eq!(
            initial.signature.as_ref().unwrap().as_ref().unwrap(),
            &elf::SIGNATURE_BYTES
        );
        owner.fresh_reset().unwrap();
        let stale = owner.memory().unwrap();
        owner.resume().unwrap();
        owner
            .write_mem(elf::BSS_PROBE_OFFSET as u64, &[0xaa])
            .unwrap();
        owner
            .write_mem(elf::TOHOST_SEGMENT_OFFSET, &7u64.to_le_bytes())
            .unwrap();
        for _ in 0..code.len() {
            let turn = owner.step(true).unwrap();
            assert!(turn.hart().control.retired);
            assert!(matches!(
                turn.hart().observation,
                Some(Observation::Commit(_))
            ));
        }
        let mutated = owner.inspect().unwrap();
        assert_eq!(
            mutated.hart.csr.read(machine::MINSTRET).unwrap(),
            code.len() as u64
        );
        assert!(mutated.hart.reservation.is_some());
        assert_eq!(owner.read_mem(TARGET, 8).unwrap(), 99u64.to_le_bytes());
        assert!(matches!(owner.fresh_reset(), Err(MachineError::NotDrained)));
        drained(&owner);
        edit(&owner, |state| {
            state.privilege = PrivilegeMode::User;
            state.csr.write(machine::MSTATUS, 0x80).unwrap();
            state.csr.write(machine::MEPC, elf::BASE + TARGET).unwrap();
            state.fpr.write(3, Fpr::new(5.0));
            state.fcsr.write(0x1f);
        });
        owner.fresh_reset().unwrap();
        let fresh = owner.inspect().unwrap();
        assert!(Arc::ptr_eq(&fresh.image, &description));
        assert!(fresh.generation > mutated.generation);
        assert_eq!(fresh.hart.pc, legacy.entry_point);
        assert_eq!(fresh.hart.privilege, PrivilegeMode::Machine);
        assert_eq!(fresh.hart.regs, [0; 32]);
        assert_eq!(fresh.hart.csr.read(machine::MINSTRET).unwrap(), 0);
        assert_eq!(
            fresh.hart.csr.read(machine::MSTATUS).unwrap(),
            CoreState::default().csr.read(machine::MSTATUS).unwrap()
        );
        assert_eq!(fresh.hart.csr.read(machine::MEPC).unwrap(), 0);
        assert_eq!(fresh.hart.fcsr.read(), 0);
        assert_eq!(
            fresh.hart.fpr.read(3).bits(),
            CoreState::default().fpr.read(3).bits()
        );
        assert!(fresh.hart.reservation.is_none());
        assert!(fresh.events.is_empty());
        assert_eq!(*fresh.tohost.value.as_ref().unwrap(), 0);
        assert_eq!(
            owner.read_mem(0, legacy.memory.len()).unwrap(),
            legacy.memory
        );
        // These old typed writers still work, but are detached from fresh RAM.
        stale.lock().unwrap().write_dword(TARGET, 0xdd).unwrap();
        stale
            .lock()
            .unwrap()
            .write_dword(elf::TOHOST_SEGMENT_OFFSET, 9)
            .unwrap();
        assert_eq!(owner.read_mem(TARGET, 8).unwrap(), [0; 8]);
        owner.resume().unwrap();
        for _ in 0..code.len() {
            assert!(owner.step(false).unwrap().hart().control.retired);
        }
        assert_eq!(owner.read_mem(TARGET, 8).unwrap(), 99u64.to_le_bytes());
        assert_eq!(
            owner
                .inspect()
                .unwrap()
                .hart
                .csr
                .read(machine::MINSTRET)
                .unwrap(),
            code.len() as u64
        );
        drained(&owner);
        owner.teardown().unwrap();
        assert_eq!(owner.status().unwrap().lifecycle, Lifecycle::TornDown);
        assert!(owner.resume().is_err());
        assert!(owner.install(description.clone()).is_err());
    }
}

#[test]
fn signature_inspection_preserves_empty_absent_and_unreadable_regions_after_fresh_reset() {
    let cases = [
        None,
        Some((elf::BASE + 0x30_000, 0)),
        Some((elf::BASE - 8, 0)),
        Some((u64::MAX, 0)),
        Some((elf::SIGNATURE, 8)),
        Some((elf::BASE + 0x30_000, 8)),
        Some((elf::BASE - 8, 8)),
    ];
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        for region in cases {
            let description = Arc::new(
                LoadImage::parse(&elf::elf_with_signature(
                    &[elf::nop()],
                    0,
                    elf::BASE,
                    None,
                    region,
                    0x4000,
                ))
                .unwrap(),
            );
            let owner = owner(kind, description.clone());
            let initial_generation = owner.status().unwrap().generation;
            for fresh in [false, true] {
                if fresh {
                    owner.resume().unwrap();
                    assert!(owner.step(true).unwrap().hart().control.retired);
                    drained(&owner);
                    owner.fresh_reset().unwrap();
                }
                let snapshot = owner.inspect().unwrap();
                assert!(Arc::ptr_eq(&snapshot.image, &description));
                match region {
                    None => {
                        assert!(snapshot.image.signature().is_none());
                        assert!(snapshot.signature.is_none());
                    }
                    Some((address, size)) => {
                        let metadata = snapshot.image.signature().unwrap();
                        assert_eq!(metadata.vaddr, address);
                        assert_eq!(metadata.size, size);
                        assert_eq!(
                            metadata.file_offset,
                            elf::LOAD_OFFSET as u64 + elf::SIGNATURE_SEGMENT_OFFSET
                        );
                        let artifact = snapshot.signature.as_ref().unwrap();
                        if size == 0 {
                            assert_eq!(
                                artifact.as_ref().unwrap(),
                                &Vec::<u8>::new(),
                                "{kind:?} {address:#x}, fresh={fresh}"
                            );
                        } else if address == elf::SIGNATURE {
                            assert_eq!(artifact.as_ref().unwrap(), &elf::SIGNATURE_BYTES);
                        } else {
                            assert!(artifact.is_err(), "nonempty unmapped signature must fail");
                        }
                    }
                }
                assert_eq!(snapshot.hart.pc, description.entry_point());
                assert_eq!(snapshot.hart.csr.read(machine::MINSTRET).unwrap(), 0);
                assert!(snapshot.events.is_empty());
                assert!(owner.status().unwrap().work.is_empty());
                assert_eq!(snapshot.generation > initial_generation, fresh);
            }
            owner.teardown().unwrap();
        }
    }
}

#[test]
fn pure_zero_fill_segment_preserves_loader_order_and_restores_overlapping_file_bytes() {
    let mut bytes = elf::elf_with_code(&[elf::nop()], 0, true, false, 0x4000);
    bytes[56..58].copy_from_slice(&2u16.to_le_bytes()); // two program headers
    let header = 64 + 56;
    bytes[header..header + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    bytes[header + 4..header + 8].copy_from_slice(&6u32.to_le_bytes());
    bytes[header + 8..header + 16].copy_from_slice(&(elf::LOAD_OFFSET as u64).to_le_bytes());
    let address = elf::BASE + elf::FILE_BYTE_OFFSET as u64;
    bytes[header + 16..header + 24].copy_from_slice(&address.to_le_bytes());
    bytes[header + 24..header + 32].copy_from_slice(&address.to_le_bytes());
    bytes[header + 40..header + 48].copy_from_slice(&8u64.to_le_bytes()); // no file bytes
    bytes[header + 48..header + 56].copy_from_slice(&1u64.to_le_bytes());
    let legacy = load_elf_file(&bytes).unwrap();
    let description = Arc::new(LoadImage::parse(&bytes).unwrap());
    assert!(description.segments()[1].file_bytes.is_empty());
    assert_eq!(description.segments()[1].zero_fill, 8);
    assert_eq!(legacy.memory[elf::FILE_BYTE_OFFSET], 0);
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let owner = owner(kind, description.clone());
        assert_eq!(
            owner.read_mem(0, legacy.memory.len()).unwrap(),
            legacy.memory
        );
        owner.resume().unwrap();
        owner
            .write_mem(elf::FILE_BYTE_OFFSET as u64, &[0xaa; 8])
            .unwrap();
        drained(&owner);
        owner.fresh_reset().unwrap();
        assert_eq!(
            owner.read_mem(elf::FILE_BYTE_OFFSET as u64, 8).unwrap(),
            [0; 8]
        );
    }
}

#[test]
fn replacement_isolates_cloned_writers_and_preserves_entry_and_artifact_identity() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let first = image(&[atomic(2, 3, 0)]);
        let owner = owner(kind, first);
        edit(&owner, |state| state.regs[1] = elf::BASE + TARGET);
        owner.resume().unwrap();
        owner.write_mem(TARGET, &55u64.to_le_bytes()).unwrap();
        assert!(owner.step(false).unwrap().hart().control.retired);
        let stale = owner.memory().unwrap();
        let replacement = Arc::new(
            LoadImage::parse(&elf::elf_with_placement(
                &[elf::addi(7, 0, 9)],
                16,
                0x9000_0000,
                None,
                0x4000,
            ))
            .unwrap(),
        );
        assert!(matches!(
            owner.install(replacement.clone()),
            Err(MachineError::NotDrained)
        ));
        drained(&owner);
        owner.install(replacement.clone()).unwrap();
        let (start, release) = mpsc::channel();
        let writer = thread::spawn(move || {
            release.recv().unwrap();
            let mut stale = stale.lock().unwrap();
            stale.write_dword(TARGET, 0xaa).unwrap();
            stale.write_dword(elf::TOHOST_SEGMENT_OFFSET, 1).unwrap();
            assert_eq!(stale.read_dword(TARGET).unwrap(), 0xaa);
        });
        start.send(()).unwrap();
        writer.join().unwrap();
        let inspection = owner.inspect().unwrap();
        assert!(Arc::ptr_eq(&inspection.image, &replacement));
        assert_eq!(inspection.hart.pc, 0x9000_0010);
        assert!(inspection.hart.reservation.is_none());
        assert!(inspection.signature.is_none());
        assert!(inspection.events.is_empty());
        assert_eq!(owner.read_mem(TARGET, 8).unwrap(), [0; 8]);
        assert_eq!(
            owner.read_mem(elf::TOHOST_SEGMENT_OFFSET, 8).unwrap(),
            [0; 8]
        );
        if kind == PlatformKind::Native {
            assert_eq!(*inspection.tohost.value.as_ref().unwrap(), 0);
        } else {
            assert!(inspection.tohost.value.is_err());
        }
        owner.resume().unwrap();
        assert!(owner.step(false).unwrap().hart().control.retired);
        assert_eq!(owner.inspect().unwrap().hart.regs[7], 9);
    }
}

#[test]
fn rejected_flat_placement_is_non_destructive_and_static_manual_signal_is_restored() {
    let mut config = MachineConfig::new(PlatformKind::Flat);
    config.tohost_override = Some(0x1008);
    let owner = Machine::new(config);
    let no_metadata = Arc::new(
        LoadImage::parse(&elf::elf_with_placement(
            &[elf::nop()],
            0,
            elf::BASE,
            None,
            0x4000,
        ))
        .unwrap(),
    );
    owner.install(no_metadata.clone()).unwrap();
    let before = owner.inspect().unwrap();
    let memory = owner.memory().unwrap();
    let bad = Arc::new(
        LoadImage::parse(&elf::elf_with_placement(
            &[elf::nop()],
            0,
            elf::BASE,
            Some(elf::BASE - 8),
            0x4000,
        ))
        .unwrap(),
    );
    assert!(matches!(
        owner.install(bad),
        Err(MachineError::Placement(_))
    ));
    assert!(Arc::ptr_eq(&memory, &owner.memory().unwrap()));
    assert_eq!(owner.status().unwrap().generation, before.generation);
    assert_eq!(owner.inspect().unwrap().tohost.address, 0x1008);
    owner.fresh_reset().unwrap();
    assert_eq!(owner.inspect().unwrap().tohost.address, 0x1008);
}

#[test]
fn active_typed_and_clear_writers_share_atomic_versions_without_disabling_ordinary_execution() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        for overlap in [false, true] {
            let owner = owner(kind, image(&[atomic(2, 3, 0), atomic(3, 4, 2)]));
            edit(&owner, |state| {
                state.regs[1] = elf::BASE + TARGET;
                state.regs[2] = 55;
            });
            owner.resume().unwrap();
            owner.write_mem(TARGET, &99u64.to_le_bytes()).unwrap();
            owner.step(true).unwrap();
            let handle = owner.memory().unwrap();
            let (go, wait) = mpsc::channel();
            let writer = thread::spawn(move || {
                wait.recv().unwrap();
                handle
                    .lock()
                    .unwrap()
                    .write_byte(if overlap { TARGET } else { TARGET + 8 }, 0xaa)
                    .unwrap();
            });
            go.send(()).unwrap();
            writer.join().unwrap();
            owner.clear_tohost().unwrap(); // disjoint committed writer
            let turn = owner.step(true).unwrap();
            let Some(Observation::Commit(record)) = &turn.hart().observation else {
                panic!("SC must retire");
            };
            assert_eq!(record.effects.gpr[0].after, u64::from(overlap));
            assert_eq!(record.effects.memory[0].write.is_some(), !overlap);
            drop(turn);
            assert_eq!(owner.inspect().unwrap().hart.regs[4], u64::from(overlap));
            // Prefix writes preserve existing accepted-side-effect semantics.
            let last = owner.inspect().unwrap().image.memory_size() as u64 - 1;
            assert!(owner.write_mem(last, &[0xab, 0xcd]).is_err());
            assert_eq!(owner.read_mem(last, 1).unwrap(), [0xab]);
        }
    }
}

#[test]
fn retained_boundary_no_progress_or_run_return_never_acknowledges_lifecycle_drain() {
    let owner = owner(PlatformKind::Flat, image(&[elf::nop(), elf::nop()]));
    owner.resume().unwrap();
    let turn = owner.step(false).unwrap();
    assert!(turn.hart().observation.is_none());
    assert_eq!(owner.status().unwrap().work.boundary, 1);
    assert!(matches!(owner.step(false), Err(MachineError::Busy)));
    owner.request_quiesce().unwrap();
    assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
    assert!(matches!(owner.fresh_reset(), Err(MachineError::Busy)));
    assert!(matches!(
        owner.set_hart_state(CoreState::default()),
        Err(MachineError::Busy)
    ));
    drop(turn);
    assert_eq!(
        owner.status().unwrap().lifecycle,
        Lifecycle::QuiesceRequested
    );
    assert!(matches!(owner.fresh_reset(), Err(MachineError::NotDrained)));
    owner.try_drain().unwrap();
    owner.resume().unwrap();
    owner.step(false).unwrap();
    // Zero outstanding work (including an outer timeout/NoProgress claim) is
    // insufficient: only the explicit requested, proven drain grants mutation.
    assert!(owner.status().unwrap().work.is_empty());
    assert!(matches!(owner.try_drain(), Err(MachineError::NotDrained)));
    assert!(matches!(owner.fresh_reset(), Err(MachineError::NotDrained)));
    assert!(matches!(owner.teardown(), Err(MachineError::NotDrained)));
    drained(&owner);
    owner.fresh_reset().unwrap();
}

#[test]
fn observer_delivery_is_drained_after_hart_acceptance_and_sink_error_cannot_erase_it() {
    let owner = owner(PlatformKind::Flat, image(&[elf::addi(3, 0, 7)]));
    owner.resume().unwrap();
    let (started, started_rx) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    struct Sink {
        owner: Machine,
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl ObservationSink for Sink {
        type Error = &'static str;
        fn observe(&mut self, observation: &Observation) -> Result<(), Self::Error> {
            assert!(matches!(observation, Observation::Commit(_)));
            assert!(matches!(self.owner.step(false), Err(MachineError::Busy)));
            self.started.send(()).unwrap();
            self.release.recv().unwrap();
            Err("injected observer failure")
        }
    }
    let machine = owner.clone();
    let worker = thread::spawn(move || {
        let turn = machine.step(true).unwrap();
        let mut sink = Sink {
            owner: machine,
            started,
            release: release_rx,
        };
        assert_eq!(turn.deliver(&mut sink), Err("injected observer failure"));
        assert!(turn.hart().control.retired);
        assert_eq!(turn.hart().control.minstret_after, 1);
    });
    started_rx.recv().unwrap();
    owner.request_quiesce().unwrap();
    assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
    assert!(matches!(
        owner.install(image(&[elf::nop()])),
        Err(MachineError::Busy)
    ));
    assert!(matches!(owner.teardown(), Err(MachineError::Busy)));
    release.send(()).unwrap();
    worker.join().unwrap();
    owner.try_drain().unwrap();
    assert_eq!(owner.inspect().unwrap().hart.regs[3], 7);
    owner.fresh_reset().unwrap();
    assert_eq!(owner.inspect().unwrap().hart.regs[3], 0);
}

#[test]
fn native_callback_work_and_its_returned_causal_receipt_both_prevent_drain() {
    for uart in [false, true] {
        let (started, started_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let callback = move |value: u64| {
            started.send(value).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
        };
        let mut config = MachineConfig::new(PlatformKind::Native);
        if uart {
            config.uart_output = Some(Arc::new(move |byte| callback(u64::from(byte))));
        } else {
            config.htif_write = Some(Arc::new(callback));
        }
        let owner = Machine::new(config);
        owner
            .install(image(&[if uart {
                elf::sb(2, 1, 0)
            } else {
                elf::sd(2, 1, 0)
            }]))
            .unwrap();
        edit(&owner, |state| {
            state.regs[1] = if uart { 0x1000_0000 } else { 0x4000_8000 };
            state.regs[2] = if uart { u64::from(b'A') } else { 7 };
        });
        owner.resume().unwrap();
        let clone = owner.clone();
        let worker = thread::spawn(move || clone.step(true).unwrap());
        assert_eq!(
            started_rx.recv().unwrap(),
            if uart { u64::from(b'A') } else { 7 }
        );
        assert_eq!(owner.status().unwrap().work.hart, 1);
        owner.request_quiesce().unwrap();
        assert!(matches!(owner.inspect(), Err(MachineError::Busy)));
        assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
        assert!(matches!(owner.fresh_reset(), Err(MachineError::Busy)));
        assert!(matches!(owner.teardown(), Err(MachineError::Busy)));
        assert!(owner.step(false).is_err());
        release.send(()).unwrap();
        let turn = worker.join().unwrap();
        assert!(turn.hart().control.retired);
        assert_eq!(
            turn.events(),
            &[if uart {
                PlatformEvent::UartTransmit(b'A')
            } else {
                PlatformEvent::HtifWrite(7)
            }]
        );
        assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
        drop(turn);
        owner.try_drain().unwrap();
        assert_eq!(
            owner
                .inspect()
                .unwrap()
                .hart
                .csr
                .read(machine::MINSTRET)
                .unwrap(),
            1
        );
        owner.fresh_reset().unwrap();
        assert!(owner.inspect().unwrap().events.is_empty());
    }
}

#[test]
fn native_dynamic_uart_state_and_callback_connections_are_restored() {
    let writes = Arc::new(Mutex::new(Vec::new()));
    let captured = writes.clone();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.uart_output = Some(Arc::new(move |byte| captured.lock().unwrap().push(byte)));
    let owner = Machine::new(config);
    let code = [
        elf::lui(1, 0x10000),
        elf::addi(2, 0, 65),
        elf::sb(2, 1, 0),
        elf::addi(2, 0, 0x83),
        elf::sb(2, 1, 3),
        elf::addi(2, 0, 5),
        elf::sb(2, 1, 0),
        elf::sb(2, 1, 1),
    ];
    owner.install(image(&code)).unwrap();
    let initial = owner.inspect().unwrap().uart.unwrap();
    for iteration in 1..=2 {
        owner.resume().unwrap();
        owner.receive_uart(b'R').unwrap();
        for _ in &code {
            owner.step(false).unwrap();
        }
        let changed = owner.inspect().unwrap().uart.unwrap();
        assert_eq!(changed.rx_fifo, [b'R']);
        assert_eq!(changed.tx_fifo, [b'A']);
        assert_eq!(changed.registers[2], 0x83);
        assert_eq!(changed.registers[7..9], [5, 5]);
        assert_eq!(writes.lock().unwrap().len(), iteration);
        drained(&owner);
        owner.fresh_reset().unwrap();
        assert_eq!(owner.inspect().unwrap().uart.unwrap(), initial);
    }
}

#[test]
fn known_failure_can_drain_but_requires_restoration_not_flag_clearing_or_retry() {
    let owner = owner(PlatformKind::Flat, image(&[0x0000_100f])); // unsupported legal FENCE.I
    owner.resume().unwrap();
    let turn = owner.step(true).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::SimulatorFailure(_)
    ));
    assert!(turn.hart().observation.is_none());
    assert!(turn.tohost().is_none());
    drop(turn);
    assert!(!owner.status().unwrap().uncertain);
    assert!(matches!(
        owner.step(false),
        Err(MachineError::NeedsFreshReset)
    ));
    drained(&owner);
    assert!(matches!(owner.resume(), Err(MachineError::NeedsFreshReset)));
    owner.install(image(&[elf::nop()])).unwrap();
    owner.resume().unwrap();
    assert!(owner.step(false).unwrap().hart().control.retired);
}

#[test]
fn failed_and_faulting_sc_keep_the_t1_fact_and_a8_target_boundaries() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        for (fault, rd) in [(false, 0), (false, 4), (true, 4)] {
            let owner = owner(kind, image(&[atomic(3, rd, 2)]));
            let target = if !fault {
                elf::BASE + TARGET
            } else if kind == PlatformKind::Native {
                0x4000_8000 // native HTIF D-c rejects SC before any callback
            } else {
                elf::BASE + owner.inspect().unwrap().image.memory_size() as u64
            };
            edit(&owner, |state| {
                state.regs[1] = target;
                state.regs[2] = 7;
                state.regs[4] = 88;
            });
            owner.resume().unwrap();
            let turn = owner.step(true).unwrap();
            assert!(turn.events().is_empty());
            assert_eq!(*turn.tohost().unwrap().value.as_ref().unwrap(), 0);
            if fault {
                let Some(Observation::Trap(record)) = &turn.hart().observation else {
                    panic!("target rejection must trap");
                };
                assert_eq!(
                    record.trap.cause,
                    ruscv_sim::core::ExceptionCause::StoreAccessFault
                );
                assert_eq!(record.trap.mtval, target);
                assert_eq!(turn.hart().control.minstret_after, 0);
            } else {
                let Some(Observation::Commit(record)) = &turn.hart().observation else {
                    panic!("failed SC must retire");
                };
                assert_eq!(record.effects.gpr.len(), usize::from(rd != 0));
                assert!(record.effects.memory[0].write.is_none());
                assert_eq!(turn.hart().control.minstret_after, 1);
            }
            drop(turn);
            assert_eq!(
                owner.inspect().unwrap().hart.regs[4],
                if fault || rd == 0 { 88 } else { 1 }
            );
            assert_eq!(owner.read_mem(TARGET, 8).unwrap(), [0; 8]);
        }
    }
}

#[test]
fn fresh_native_htif_events_do_not_inherit_pending_exit_or_stale_writer_effects() {
    let callbacks = Arc::new(Mutex::new(Vec::new()));
    let received = callbacks.clone();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.htif_write = Some(Arc::new(move |value| received.lock().unwrap().push(value)));
    let owner = Machine::new(config);
    owner
        .install(image(&[
            elf::lui(1, 0x40008),
            elf::addi(2, 0, 7),
            elf::sd(2, 1, 0),
        ]))
        .unwrap();
    let stale = owner.memory().unwrap();
    for iteration in 1..=2 {
        owner.resume().unwrap();
        owner.step(false).unwrap();
        owner.step(false).unwrap();
        let turn = owner.step(true).unwrap();
        assert_eq!(turn.events(), [PlatformEvent::HtifWrite(7)]);
        let Some(Observation::Commit(record)) = &turn.hart().observation else {
            panic!("exit-causing store must retire");
        };
        assert_eq!(record.retired.minstret, 3);
        assert!(record.effects.gpr.is_empty());
        assert_eq!(record.effects.memory[0].write, Some(7u64.to_le_bytes()));
        assert_eq!(callbacks.lock().unwrap().len(), iteration);
        drop(turn);
        drained(&owner);
        owner.fresh_reset().unwrap();
        assert_eq!(
            owner
                .inspect()
                .unwrap()
                .hart
                .csr
                .read(machine::MINSTRET)
                .unwrap(),
            0
        );
        assert!(owner.inspect().unwrap().events.is_empty());
        stale
            .lock()
            .unwrap()
            .write_dword(elf::TOHOST_SEGMENT_OFFSET, 1)
            .unwrap();
        assert!(stale.lock().unwrap().write_dword(0x4000_8000, 1).is_err());
        assert_eq!(*owner.inspect().unwrap().tohost.value.as_ref().unwrap(), 0);
        assert_eq!(callbacks.lock().unwrap().len(), iteration);
    }
}

#[test]
fn drained_control_edits_cannot_import_previous_generation_reservations() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let owner = owner(kind, image(&[atomic(2, 3, 0), atomic(3, 4, 2)]));
        edit(&owner, |state| {
            state.regs[1] = elf::BASE + TARGET;
            state.regs[2] = 55;
        });
        owner.resume().unwrap();
        owner.step(false).unwrap();
        let old = owner.inspect().unwrap().hart;
        assert!(old.reservation.is_some());
        drained(&owner);
        owner.fresh_reset().unwrap();
        owner.set_hart_state(old).unwrap();
        assert!(owner.inspect().unwrap().hart.reservation.is_none());
        owner.resume().unwrap();
        let turn = owner.step(true).unwrap();
        assert!(turn.hart().control.retired);
        drop(turn);
        assert_eq!(owner.inspect().unwrap().hart.regs[4], 1);
        assert_eq!(owner.read_mem(TARGET, 8).unwrap(), [0; 8]);
    }
}
