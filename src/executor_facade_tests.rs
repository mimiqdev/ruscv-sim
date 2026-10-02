//! T3 tests of the actual facade Runners with bounded admission/backend/sink seams.
//! No test seam is a public adapter or an unknown-resolution API.
use super::*;
use crate::core::observation::{Observation as HartObservation, ObservationSink};
use crate::csr::machine;
use crate::machine::{Lifecycle, MachineError};
use crate::physical::*;
use observation_fixture as elf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;

type Observe = Box<dyn FnMut(&HartObservation) -> std::io::Result<()> + Send>;
struct Sink(Observe);
impl ObservationSink for Sink {
    type Error = std::io::Error;
    fn observe(&mut self, fact: &HartObservation) -> Result<(), Self::Error> {
        (self.0)(fact)
    }
}
fn fail() -> std::io::Error {
    std::io::Error::other("T3 sink failure")
}
fn loaded(code: &[u32]) -> RiscVSimulator {
    let mut sim = RiscVSimulator::new(1);
    sim.load_elf(&elf::elf_with_code(code, 0, true, false, 0x4000))
        .unwrap();
    sim
}

#[test]
fn native_completed_store_exit_budget_and_reporting_facts_are_retained_together() {
    let code = [elf::auipc(1, 1), elf::addi(2, 0, 1), elf::sd(2, 1, 0)];
    let bytes = elf::elf_with_code(&code, 0, true, false, 0x4000);
    let seen = Arc::new(AtomicUsize::new(0));
    let count = seen.clone();
    let observer = Sink(Box::new(move |fact| {
        let HartObservation::Commit(record) = fact else {
            panic!("store fixture retired");
        };
        if record.retired.pc == elf::BASE + 8 {
            assert_eq!(record.retired.instruction, code[2]);
            assert_eq!(record.effects.memory.len(), 1);
            count.fetch_add(1, Ordering::SeqCst);
            Err(fail())
        } else {
            Ok(())
        }
    }));
    let boundaries = Arc::new(AtomicUsize::new(0));
    let count = boundaries.clone();
    let hooks = NativeHooks {
        observer: Some(Box::new(observer)),
        boundary: Some(Box::new(move |turn| {
            if turn.hart().control.after_pc == elf::BASE + 12 {
                assert!(turn.hart().control.retired);
                assert!(turn.boundary().budget_exhausted);
                assert!(!turn.boundary().single_step && !turn.boundary().uncertain);
                assert!(turn
                    .boundary()
                    .reporting_error
                    .as_ref()
                    .unwrap()
                    .contains("T3 sink"));
                assert_eq!(turn.tohost().unwrap().value.as_ref().unwrap(), &1);
                count.fetch_add(1, Ordering::SeqCst);
            }
        })),
        ..NativeHooks::default()
    };
    let result = run_native_inner(&bytes, Some(3), None, None, false, None, hooks).unwrap();
    assert_eq!(
        (result.exit_code, result.cycles, result.final_pc),
        (0, 3, elf::BASE + 12)
    );
    assert!(!result.timed_out);
    assert!(result.error.unwrap().contains("guest exit code 0 retained"));
    assert_eq!(seen.load(Ordering::SeqCst), 1);
    assert_eq!(boundaries.load(Ordering::SeqCst), 1);
}

#[test]
fn native_sink_failure_preserves_completed_trap_and_final_budget_without_commit() {
    let bytes = elf::elf_with_code(&[0xffff_ffff], 0, true, false, 0x4000);
    let hooks = NativeHooks {
        observer: Some(Box::new(Sink(Box::new(|fact| {
            let HartObservation::Trap(record) = fact else {
                panic!("not a retirement");
            };
            assert_eq!(record.trap.faulting_pc, elf::BASE);
            Err(fail())
        })))),
        boundary: Some(Box::new(|turn| {
            assert!(turn.hart().control.trap_entered && !turn.hart().control.retired);
            assert_eq!(turn.hart().control.minstret_after, 0);
            assert!(turn.boundary().budget_exhausted);
            assert!(turn.boundary().reporting_error.is_some());
            assert!(turn.events().is_empty());
        })),
        ..NativeHooks::default()
    };
    let result = run_native_inner(&bytes, Some(1), None, None, false, None, hooks).unwrap();
    assert_eq!(result.cycles, 1);
    assert!(!result.timed_out);
    assert!(result.error.unwrap().contains("execution budget exhausted"));
}

#[test]
fn actual_native_runner_callback_then_observer_receipt_block_lifecycle_mutation() {
    for htif in [false, true] {
        let (in_callback, callback_rx) = mpsc::channel();
        let (release_callback, callback_wait) = mpsc::channel();
        let callback_wait = Mutex::new(callback_wait);
        let callback = Arc::new(move |value: u64| {
            assert_eq!(value, if htif { 7 } else { u64::from(b'A') });
            in_callback.send(()).unwrap();
            callback_wait.lock().unwrap().recv().unwrap();
        });
        let mut config = MachineConfig::new(PlatformKind::Native);
        if htif {
            config.htif_write = Some(callback);
        } else {
            config.uart_output = Some(Arc::new(move |byte| callback(u64::from(byte))));
        }
        let (control_tx, control_rx) = mpsc::channel();
        let (in_sink, sink_rx) = mpsc::channel();
        let (release_sink, sink_wait) = mpsc::channel();
        let bytes = elf::elf_with_code(
            &[if htif {
                elf::sd(2, 1, 0)
            } else {
                elf::sb(2, 1, 0)
            }],
            0,
            false,
            false,
            0x4000,
        );
        let hooks = NativeHooks {
            config: Some(config),
            prepare: Some(Box::new(move |owner| {
                let state = owner.state_mut();
                state.regs[1] = if htif {
                    SYSTEM_BUS_HTIF_BASE
                } else {
                    SYSTEM_BUS_UART_BASE
                };
                state.regs[2] = if htif { 7 } else { u64::from(b'A') };
                control_tx.send(owner.test_control()).unwrap();
            })),
            observer: Some(Box::new(Sink(Box::new(move |fact| {
                assert!(matches!(fact, HartObservation::Commit(_)));
                in_sink.send(()).unwrap();
                sink_wait.recv().unwrap();
                Err(fail())
            })))),
            boundary: Some(Box::new(move |turn| {
                assert!(turn.hart().control.retired);
                assert_eq!(turn.boundary().lifecycle, Lifecycle::QuiesceRequested);
                assert_eq!(
                    turn.events(),
                    if htif {
                        vec![PlatformEvent::HtifWrite(7)]
                    } else {
                        vec![PlatformEvent::UartTransmit(b'A')]
                    }
                );
                assert!(turn.boundary().reporting_error.is_some());
            })),
        };
        let worker = thread::spawn(move || {
            run_native_inner(&bytes, Some(1), None, None, false, None, hooks).unwrap()
        });
        let control = control_rx.recv().unwrap();
        callback_rx.recv().unwrap();
        assert_eq!(control.status().unwrap().work.hart, 1);
        control.request_quiesce().unwrap();
        assert!(matches!(control.try_drain(), Err(MachineError::Busy)));
        assert!(matches!(control.fresh_reset(), Err(MachineError::Busy)));
        release_callback.send(()).unwrap();
        sink_rx.recv().unwrap();
        assert_eq!(control.status().unwrap().work.boundary, 1);
        assert_eq!(control.status().unwrap().work.hart, 0);
        assert!(matches!(control.try_drain(), Err(MachineError::Busy)));
        assert!(matches!(control.teardown(), Err(MachineError::Busy)));
        release_sink.send(()).unwrap();
        let result = worker.join().unwrap();
        assert_eq!(result.cycles, 1);
        assert_eq!(result.final_pc, elf::BASE + 4);
        assert_eq!(result.exit_code, if htif { 3 } else { 1 });
        assert!(!result.timed_out);
        assert!(result.error.unwrap().contains("T3 sink failure"));
    }
}

type LateWrite = Box<dyn FnOnce() + Send>;
struct UnknownWrite {
    actual: crate::core::SharedDataAccess,
    pending: Arc<Mutex<Option<LateWrite>>>,
    calls: Arc<AtomicUsize>,
}
impl PhysicalBackend for UnknownWrite {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        assert_eq!(request.category(), AccessCategory::DataWrite);
        self.calls.fetch_add(1, Ordering::SeqCst);
        let actual = self.actual.clone();
        let address = request.paddr();
        let width = request.width();
        let bytes = request.write_payload().unwrap().to_vec();
        *self.pending.lock().unwrap() = Some(Box::new(move || {
            actual
                .lock()
                .unwrap()
                .access(PhysicalRequest::write(address, width, &bytes).unwrap())
                .unwrap();
        }));
        Err(PhysicalBackendError::unknown("T3 possible late effect"))
    }
}
impl AtomicBackend for UnknownWrite {
    fn transact_atomic(&mut self, _: &AtomicRequest<'_>) -> AtomicBackendResult {
        panic!("ordinary write fixture");
    }
}
fn inject_unknown(
    owner: &mut OwnedMachine,
    pending: Arc<Mutex<Option<LateWrite>>>,
    calls: Arc<AtomicUsize>,
) {
    let port = ValidatedPhysicalAccess::new(UnknownWrite {
        actual: owner.test_data(),
        pending,
        calls,
    });
    owner.test_replace_data(Arc::new(Mutex::new(port)));
}

#[test]
fn flat_public_unknown_write_refuses_retry_edit_reload_and_fresh_reset_even_after_late_effect() {
    let mut sim = loaded(&[elf::sd(2, 1, 0)]);
    sim.state_mut().regs[1] = elf::TOHOST;
    sim.state_mut().regs[2] = 7;
    let pending = Arc::new(Mutex::new(None));
    let calls = Arc::new(AtomicUsize::new(0));
    inject_unknown(&mut sim.machine, pending.clone(), calls.clone());
    let memory = sim.memory().clone();
    let first = sim.run(Some(1)).unwrap();
    assert_eq!(
        (first.cycles, first.final_pc, first.exit_code),
        (0, elf::BASE, 1)
    );
    assert!(!first.timed_out);
    assert!(first
        .error
        .as_ref()
        .unwrap()
        .contains("T3 possible late effect"));
    for late in [false, true] {
        if late {
            pending.lock().unwrap().take().unwrap()();
        }
        assert!(sim.step().is_err());
        let again = sim.run(Some(1)).unwrap();
        assert_eq!(again.error, first.error);
        assert_eq!(again.cycles, 0);
        assert!(!again.timed_out);
        assert!(sim.fresh_reset().is_err());
        assert!(sim
            .load_elf(&elf::elf_with_code(&[elf::nop()], 0, true, false, 0x4000))
            .is_err());
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sim.state_mut().pc += 4;
        }))
        .is_err());
        assert!(memory.lock().unwrap().write_byte(0x300, 1).is_err());
        assert!(Arc::ptr_eq(&memory, sim.memory()));
        assert_eq!(sim.state().pc, elf::BASE);
        assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    assert_eq!(
        memory
            .lock()
            .unwrap()
            .read_dword(elf::TOHOST_SEGMENT_OFFSET)
            .unwrap(),
        7
    );
}

#[test]
fn native_public_unknown_callback_never_fabricates_commit_exit_budget_or_retry() {
    let bytes = elf::elf_with_code(&[elf::sd(2, 1, 0)], 0, false, false, 0x4000);
    let pending = Arc::new(Mutex::new(None));
    let pending_prepare = pending.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_prepare = calls.clone();
    let callbacks = Arc::new(AtomicUsize::new(0));
    let count = callbacks.clone();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.htif_write = Some(Arc::new(move |value| {
        assert_eq!(value, 7);
        count.fetch_add(1, Ordering::SeqCst);
    }));
    let hooks = NativeHooks {
        config: Some(config),
        prepare: Some(Box::new(move |owner| {
            owner.state_mut().regs[1] = SYSTEM_BUS_HTIF_BASE;
            owner.state_mut().regs[2] = 7;
            inject_unknown(owner, pending_prepare, calls_prepare);
        })),
        observer: Some(Box::new(Sink(Box::new(|_| {
            panic!("no completed observation for unknown")
        })))),
        boundary: Some(Box::new(move |turn| {
            assert!(matches!(
                turn.hart().outcome,
                StepOutcome::SimulatorFailure(_)
            ));
            assert!(turn.hart().observation.is_none());
            assert!(turn.boundary().uncertain && !turn.boundary().budget_exhausted);
            assert!(turn.events().is_empty() && turn.tohost().is_none());
            pending.lock().unwrap().take().unwrap()(); // causal late callback, not safe recovery
        })),
    };
    let result = run_native_inner(&bytes, Some(1), None, None, false, None, hooks).unwrap();
    assert_eq!(
        (result.exit_code, result.cycles, result.final_pc),
        (1, 0, elf::BASE)
    );
    assert!(!result.timed_out);
    assert!(result.error.unwrap().contains("T3 possible late effect"));
    assert_eq!(callbacks.load(Ordering::SeqCst), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn flat_clone_writes_during_admitted_turn_preserve_real_reservation_visibility() {
    for overlap in [false, true] {
        let lr = (2 << 27) | (1 << 15) | (3 << 12) | (3 << 7) | 0x2f;
        let sc = (3 << 27) | (2 << 20) | (1 << 15) | (3 << 12) | (4 << 7) | 0x2f;
        let mut sim = loaded(&[lr, sc]);
        sim.state_mut().regs[1] = elf::BASE + 0x200;
        sim.state_mut().regs[2] = 55;
        sim.step().unwrap();
        let clone = sim.memory().clone();
        let (started, start_rx) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let wait = Mutex::new(wait);
        sim.machine.test_before_turn(Arc::new(move || {
            started.send(()).unwrap();
            wait.lock().unwrap().recv().unwrap();
        }));
        let worker = thread::spawn(move || {
            let result = sim.run(Some(1)).unwrap();
            (sim, result)
        });
        start_rx.recv().unwrap(); // Hart slot admitted; not a between-run write
        clone
            .lock()
            .unwrap()
            .write_dword(if overlap { 0x200 } else { 0x300 }, 99)
            .unwrap();
        release.send(()).unwrap();
        let (sim, result) = worker.join().unwrap();
        assert_eq!(result.cycles, 1);
        assert!(result.timed_out);
        assert_eq!(sim.state().regs[4], u64::from(overlap));
        assert_eq!(
            sim.read_mem(0x200, 8).unwrap(),
            if overlap { 99u64 } else { 55u64 }.to_le_bytes()
        );
        assert!(sim.state().reservation.is_none());
    }
}

#[test]
fn public_reset_reload_and_borrowed_edit_wait_for_admitted_clone_writer_then_isolate() {
    for operation in 0..3 {
        let mut sim = loaded(&[elf::nop()]);
        let stale = sim.memory().clone();
        let control = sim.machine.test_control();
        let (admitted, admitted_rx) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let wait = Mutex::new(wait);
        let first = std::sync::atomic::AtomicBool::new(true);
        sim.machine.test_before_host(Arc::new(move || {
            if first.swap(false, Ordering::SeqCst) {
                admitted.send(()).unwrap();
                wait.lock().unwrap().recv().unwrap();
            }
        }));
        let writer_memory = stale.clone();
        let writer = thread::spawn(move || {
            writer_memory
                .lock()
                .unwrap()
                .write_dword(0x200, 77)
                .unwrap()
        });
        admitted_rx.recv().unwrap();
        let (draining, drain_rx) = mpsc::channel();
        sim.machine.test_before_drain(Arc::new(move || {
            draining.send(()).unwrap();
        }));
        let (finished, finished_rx) = mpsc::channel();
        let mutator = thread::spawn(move || {
            match operation {
                0 => sim.fresh_reset().unwrap(),
                1 => {
                    sim.load_elf(&elf::elf_with_code(
                        &[elf::addi(9, 0, 11)],
                        0x100,
                        true,
                        false,
                        0x4000,
                    ))
                    .unwrap();
                }
                _ => {
                    sim.state_mut().regs[9] = 11;
                }
            }
            finished.send(()).unwrap();
            sim
        });
        drain_rx.recv().unwrap();
        assert_eq!(control.status().unwrap().work.host, 1);
        assert_eq!(
            control.status().unwrap().lifecycle,
            Lifecycle::QuiesceRequested
        );
        assert!(matches!(
            finished_rx.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        assert!(control.write_mem(0x300, &[1]).is_err());
        release.send(()).unwrap();
        writer.join().unwrap();
        let mut sim = mutator.join().unwrap();
        finished_rx.recv().unwrap();
        if operation == 2 {
            assert_eq!(
                control.status().unwrap().lifecycle,
                Lifecycle::DrainComplete
            );
            assert!(control.write_mem(0x300, &[1]).is_err());
            assert_eq!(sim.state().regs[9], 11); // ends edit, reopens ordinary writers
            sim.machine.test_before_host(Arc::new(|| {}));
            stale.lock().unwrap().write_byte(0x300, 8).unwrap();
            assert_eq!(sim.read_mem(0x200, 8).unwrap(), 77u64.to_le_bytes());
        } else {
            assert!(!Arc::ptr_eq(&stale, sim.memory()));
            stale.lock().unwrap().write_dword(0x200, 99).unwrap();
            stale
                .lock()
                .unwrap()
                .write_dword(elf::TOHOST_SEGMENT_OFFSET, 7)
                .unwrap();
            assert_eq!(sim.read_mem(0x200, 8).unwrap(), [0; 8]);
            assert_eq!(sim.read_mem(elf::TOHOST_SEGMENT_OFFSET, 8).unwrap(), [0; 8]);
            assert_eq!(
                sim.state().pc,
                elf::BASE + if operation == 1 { 0x100 } else { 0 }
            );
            sim.step().unwrap();
        }
    }
}

#[test]
fn lifecycle_request_during_native_observation_is_in_final_boundary_facts() {
    let controller = Arc::new(Mutex::new(None::<crate::machine::Machine>));
    let setup = controller.clone();
    let sink = controller.clone();
    let hooks = NativeHooks {
        prepare: Some(Box::new(move |owner| {
            *setup.lock().unwrap() = Some(owner.test_control())
        })),
        observer: Some(Box::new(Sink(Box::new(move |_| {
            let control = sink.lock().unwrap();
            let control = control.as_ref().unwrap();
            control.request_quiesce().unwrap();
            assert!(matches!(control.try_drain(), Err(MachineError::Busy)));
            Err(fail())
        })))),
        boundary: Some(Box::new(|turn| {
            assert_eq!(turn.boundary().lifecycle, Lifecycle::QuiesceRequested);
            assert!(turn.boundary().budget_exhausted);
            assert!(turn.boundary().reporting_error.is_some());
            assert!(turn.hart().control.retired);
        })),
        ..NativeHooks::default()
    };
    let bytes = elf::elf_with_code(&[elf::nop()], 0, true, false, 0x4000);
    let result = run_native_inner(&bytes, Some(1), None, None, false, None, hooks).unwrap();
    assert_eq!(result.cycles, 1);
    assert!(result.error.unwrap().contains("T3 sink failure"));
}

#[test]
fn flat_signal_poll_waits_for_guarded_multipart_write_and_decodes_final_value() {
    use crate::machine::SignalOperation;
    let mut sim = loaded(&[elf::nop()]);
    let clone = sim.memory().clone();
    let control = sim.machine.test_control();
    let (poll_ready, poll_rx) = mpsc::channel();
    let (allow_poll, poll_wait) = mpsc::channel();
    let poll_wait = Mutex::new(poll_wait);
    let (clear_ready, clear_rx) = mpsc::channel();
    let (allow_clear, clear_wait) = mpsc::channel();
    let clear_wait = Mutex::new(clear_wait);
    let (probe, probe_rx) = mpsc::channel();
    sim.machine.test_signal_hooks(
        Arc::new(move |operation| match operation {
            SignalOperation::Poll => {
                poll_ready.send(()).unwrap();
                poll_wait.lock().unwrap().recv().unwrap();
            }
            SignalOperation::Clear => {
                clear_ready.send(()).unwrap();
                clear_wait.lock().unwrap().recv().unwrap();
            }
            SignalOperation::Signature => panic!("fixture has no signature"),
        }),
        Arc::new(move |operation, contended| {
            if operation == SignalOperation::Poll {
                probe.send(contended).unwrap();
            }
        }),
        Arc::new(|| {}),
    );
    let runner = thread::spawn(move || {
        let result = sim.run(Some(1)).unwrap();
        (sim, result)
    });
    poll_rx.recv().unwrap(); // actual Runner: after NOP retirement, before poll
    assert_eq!(control.status().unwrap().work.boundary, 1);
    let mut guard = clone.lock().unwrap();
    guard.write_byte(elf::TOHOST_SEGMENT_OFFSET, 1).unwrap();
    allow_poll.send(()).unwrap();
    let contended = probe_rx.recv().unwrap();
    if !contended {
        // Deterministic pre-fix reproduction: raw polling already decoded the
        // partial 1 before this same guarded writer supplied the remaining bytes.
        clear_rx.recv().unwrap();
    }
    guard.write_byte(elf::TOHOST_SEGMENT_OFFSET, 7).unwrap();
    guard.write_byte(elf::TOHOST_SEGMENT_OFFSET + 1, 1).unwrap();
    assert_eq!(guard.read_dword(elf::TOHOST_SEGMENT_OFFSET).unwrap(), 0x107);
    drop(guard);
    if contended {
        clear_rx.recv().unwrap();
    }
    allow_clear.send(()).unwrap();
    let (sim, result) = runner.join().unwrap();
    assert_eq!(
        result.exit_code, 0x83,
        "decode the final guarded 0x107, not partial 1"
    );
    assert!(contended, "poll must wait on the public clone guard");
    assert_eq!((result.cycles, result.final_pc), (1, elf::BASE + 4));
    assert!(!result.timed_out && result.error.is_none());
    assert_eq!(sim.read_mem(elf::TOHOST_SEGMENT_OFFSET, 8).unwrap(), [0; 8]);
}

#[test]
fn flat_signal_clear_waits_for_clone_guard_and_remains_receipt_covered_after_quiesce() {
    use crate::machine::SignalOperation;
    for quiesce in [false, true] {
        let mut sim = loaded(&[elf::nop()]);
        sim.write_mem(elf::TOHOST_SEGMENT_OFFSET, &7u64.to_le_bytes())
            .unwrap();
        let clone = sim.memory().clone();
        let control = sim.machine.test_control();
        let (clear_ready, clear_rx) = mpsc::channel();
        let (allow_clear, clear_wait) = mpsc::channel();
        let clear_wait = Mutex::new(clear_wait);
        let (probe, probe_rx) = mpsc::channel();
        let (cleared, cleared_rx) = mpsc::channel();
        let (allow_return, return_wait) = mpsc::channel();
        let return_wait = Mutex::new(return_wait);
        sim.machine.test_signal_hooks(
            Arc::new(move |operation| {
                if operation == SignalOperation::Clear {
                    clear_ready.send(()).unwrap();
                    clear_wait.lock().unwrap().recv().unwrap();
                }
            }),
            Arc::new(move |operation, contended| {
                if operation == SignalOperation::Clear {
                    probe.send(contended).unwrap();
                }
            }),
            Arc::new(move || {
                cleared.send(()).unwrap();
                return_wait.lock().unwrap().recv().unwrap();
            }),
        );
        let runner = thread::spawn(move || {
            let result = sim.run(Some(1)).unwrap();
            (sim, result)
        });
        clear_rx.recv().unwrap(); // actual Runner has decoded and retained code 3
        let mut guard = clone.lock().unwrap();
        guard.write_byte(elf::TOHOST_SEGMENT_OFFSET, 1).unwrap();
        if quiesce {
            // Finish this host update before quiesce; only receipt-covered
            // clearing, not newly admitted host writes, may follow the request.
            guard.write_byte(elf::TOHOST_SEGMENT_OFFSET + 1, 2).unwrap();
            control.request_quiesce().unwrap();
            assert!(control.write_mem(0x300, &[1]).is_err());
        }
        allow_clear.send(()).unwrap();
        let contended = probe_rx.recv().unwrap();
        if !contended {
            cleared_rx.recv().unwrap(); // pre-fix clear ran despite held guard
        }
        let while_guarded = guard.read_dword(elf::TOHOST_SEGMENT_OFFSET).unwrap();
        if !quiesce {
            guard.write_byte(elf::TOHOST_SEGMENT_OFFSET + 1, 2).unwrap();
        }
        assert_eq!(control.status().unwrap().work.boundary, 1);
        assert!(matches!(control.try_drain(), Err(MachineError::Busy)));
        drop(guard);
        if contended {
            cleared_rx.recv().unwrap();
        }
        allow_return.send(()).unwrap();
        let (sim, result) = runner.join().unwrap();
        assert!(contended, "clear must wait on the public clone guard");
        assert_eq!(while_guarded, if quiesce { 0x201 } else { 1 });
        assert_eq!(
            result.exit_code, 3,
            "retain decoded code, never read after clear"
        );
        assert_eq!((result.cycles, result.final_pc), (1, elf::BASE + 4));
        assert!(!result.timed_out && result.error.is_none());
        assert_eq!(sim.read_mem(elf::TOHOST_SEGMENT_OFFSET, 8).unwrap(), [0; 8]);
    }
}

#[test]
fn flat_zero_budget_signature_waits_for_guarded_multipart_update_and_returns_final_bytes() {
    use crate::machine::SignalOperation;
    let mut sim = RiscVSimulator::new(1);
    sim.load_elf(&elf::elf_with_code(&[elf::nop()], 0, true, true, 0x4000))
        .unwrap();
    let clone = sim.memory().clone();
    let control = sim.machine.test_control();
    let (signature_ready, signature_rx) = mpsc::channel();
    let (allow_signature, signature_wait) = mpsc::channel();
    let signature_wait = Mutex::new(signature_wait);
    let (probe, probe_rx) = mpsc::channel();
    sim.machine.test_signal_hooks(
        Arc::new(move |operation| {
            assert!(operation == SignalOperation::Signature);
            signature_ready.send(()).unwrap();
            signature_wait.lock().unwrap().recv().unwrap();
        }),
        Arc::new(move |operation, contended| {
            assert!(operation == SignalOperation::Signature);
            probe.send(contended).unwrap();
        }),
        Arc::new(|| panic!("zero budget cannot poll or clear a signal")),
    );
    let (finished, result_rx) = mpsc::channel();
    let runner = thread::spawn(move || {
        let result = sim.run(Some(0)).unwrap();
        finished.send(result).unwrap();
        sim
    });
    signature_rx.recv().unwrap();
    let mut guard = clone.lock().unwrap();
    let expected = [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7];
    for (index, byte) in expected[..2].iter().enumerate() {
        guard
            .write_byte(elf::SIGNATURE_SEGMENT_OFFSET + index as u64, *byte)
            .unwrap();
    }
    assert!(
        control.status().unwrap().work.is_empty(),
        "no admitted call does not imply this guard ended"
    );
    allow_signature.send(()).unwrap();
    let contended = probe_rx.recv().unwrap();
    let premature = if contended {
        None
    } else {
        // Reproduction cleanup: the reviewed raw path reads while this guard is
        // still held. Capture its actual partial result before finishing writes.
        Some(result_rx.recv().unwrap())
    };
    // These real guarded host writes need the admission gate. Completing them
    // while extraction waits proves the artifact wait does not hold that gate.
    for (index, byte) in expected[2..].iter().enumerate() {
        guard
            .write_byte(elf::SIGNATURE_SEGMENT_OFFSET + index as u64 + 2, *byte)
            .unwrap();
    }
    drop(guard);
    let result = premature.unwrap_or_else(|| result_rx.recv().unwrap());
    let sim = runner.join().unwrap();
    assert_eq!(
        result.signature_data,
        Some(expected.to_vec()),
        "artifact must contain the final guarded update, not its prefix"
    );
    assert!(
        contended,
        "nonempty flat extraction must wait for the public clone guard"
    );
    assert_eq!(
        (result.cycles, result.final_pc, result.exit_code),
        (0, elf::BASE, 1)
    );
    assert!(result.timed_out);
    assert_eq!(result.error.as_deref(), Some("Timeout after 0 cycles"));
    assert_eq!(result.signature_addr, Some(elf::SIGNATURE));
    assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn flat_zero_budget_absent_and_unmapped_empty_artifacts_do_not_wait_for_clone_guard() {
    for signature in [None, Some((elf::BASE + 0x30_000, 0))] {
        let mut sim = RiscVSimulator::new(1);
        sim.load_elf(&elf::elf_with_signature(
            &[elf::nop()],
            0,
            elf::BASE,
            Some(elf::TOHOST),
            signature,
            0x4000,
        ))
        .unwrap();
        sim.machine.test_signal_hooks(
            Arc::new(|_| panic!("absent/empty artifact must not touch a target")),
            Arc::new(|_, _| panic!("absent/empty artifact must not acquire a mutex")),
            Arc::new(|| panic!("zero budget cannot clear")),
        );
        let clone = sim.memory().clone();
        let guard = clone.lock().unwrap();
        let (finished, completion) = mpsc::channel();
        let runner = thread::spawn(move || {
            let result = sim.run(Some(0)).unwrap();
            finished.send(result).unwrap();
            sim
        });
        let result = completion.recv().unwrap(); // completes WHILE clone guard is held
        assert_eq!(result.signature_addr, signature.map(|(address, _)| address));
        assert_eq!(result.signature_data, signature.map(|_| Vec::new()));
        assert_eq!((result.cycles, result.final_pc), (0, elf::BASE));
        assert!(result.timed_out);
        assert_eq!(result.error.as_deref(), Some("Timeout after 0 cycles"));
        drop(guard);
        let sim = runner.join().unwrap();
        assert_eq!(sim.state().csr.read(machine::MINSTRET).unwrap(), 0);
    }
}

#[test]
fn native_zero_budget_signature_keeps_raw_platform_inspection_not_flat_clone_serialization() {
    use crate::machine::SignalOperation;
    let bytes = elf::elf_with_code(&[elf::nop()], 0, true, true, 0x4000);
    let (memory_tx, memory_rx) = mpsc::channel();
    let (ready, ready_rx) = mpsc::channel();
    let (allow, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    let (probe, probe_rx) = mpsc::channel();
    let hooks = NativeHooks {
        prepare: Some(Box::new(move |owner| {
            memory_tx.send(owner.memory().clone()).unwrap();
            owner.test_signal_hooks(
                Arc::new(move |operation| {
                    assert!(operation == SignalOperation::Signature);
                    ready.send(()).unwrap();
                    wait.lock().unwrap().recv().unwrap();
                }),
                Arc::new(move |operation, contended| {
                    assert!(operation == SignalOperation::Signature);
                    probe.send(contended).unwrap();
                }),
                Arc::new(|| panic!("zero budget cannot clear")),
            );
        })),
        ..NativeHooks::default()
    };
    let runner = thread::spawn(move || {
        run_native_inner(&bytes, Some(0), None, None, false, None, hooks).unwrap()
    });
    let clone = memory_rx.recv().unwrap(); // internal native RAM capability, not a new public facade API
    ready_rx.recv().unwrap();
    let guard = clone.lock().unwrap();
    allow.send(()).unwrap();
    let contended = probe_rx.recv().unwrap();
    // Native inspects its raw Platform and therefore finishes before this clone
    // guard ends. Drop early only to clean up an incorrect mutex selection.
    let result = if contended {
        drop(guard);
        runner.join().unwrap()
    } else {
        let result = runner.join().unwrap();
        drop(guard);
        result
    };
    assert!(
        !contended,
        "native must not adopt flat clone-mutex semantics"
    );
    assert_eq!(result.signature_data, Some(elf::SIGNATURE_BYTES.to_vec()));
    assert_eq!((result.cycles, result.final_pc), (0, elf::BASE));
    assert!(result.timed_out);
    assert_eq!(result.error.as_deref(), Some("Timeout after 0 cycles"));
}
