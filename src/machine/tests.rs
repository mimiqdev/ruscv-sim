//! Controlled native backend/host-admission seams, not public recovery APIs.
use super::*;
use crate::blocking::test_support::{DelayingRam, DirectMemReply, DirectMemTarget, ScriptTarget};
use crate::blocking::{BlockingDirectMemAccess, BlockingInitiator, Transport, ZeroDelayRam};
use crate::csr::machine;
use crate::memory::MemoryError;
use crate::physical::{
    AccessCategory, AtomicBackend, AtomicBackendResult, AtomicRequest, PhysicalAccessError,
    PhysicalAccessKind, PhysicalAccessResult, PhysicalBackend, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalWidth, ValidatedPhysicalAccess,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::thread;

use crate::executor::observation_fixture as elf;

fn image(word: u32) -> Arc<LoadImage> {
    Arc::new(LoadImage::parse(&elf::elf_with_code(&[word], 0, true, false, 0x4000)).unwrap())
}
fn owner(kind: PlatformKind) -> Machine {
    let owner = Machine::new(MachineConfig::new(kind));
    owner.install(image(elf::nop())).unwrap();
    owner.resume().unwrap();
    owner
}

#[test]
fn admitted_cross_thread_typed_prefix_and_clear_writes_fence_reset_and_replacement() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        for path in 0..3 {
            for reload in [false, true] {
                let owner = owner(kind);
                owner
                    .write_mem(elf::TOHOST_SEGMENT_OFFSET, &7u64.to_le_bytes())
                    .unwrap();
                let view = owner.host_view().unwrap();
                let stale = view.memory.clone();
                let (admitted, admission_rx) = mpsc::channel();
                let (release, release_rx) = mpsc::channel();
                let release_rx = Mutex::new(release_rx);
                *view.writer.before_lock.lock().unwrap() = Some(Arc::new(move || {
                    admitted.send(()).unwrap();
                    release_rx.lock().unwrap().recv().unwrap();
                }));
                let writer = owner.clone();
                let memory = stale.clone();
                let worker = thread::spawn(move || match path {
                    0 => memory
                        .lock()
                        .unwrap()
                        .write_dword(0x200, 55)
                        .map_err(MachineError::from),
                    1 => writer.write_mem(0x200, &55u64.to_le_bytes()),
                    _ => writer.clear_tohost(),
                });
                admission_rx.recv().unwrap();
                assert_eq!(owner.status().unwrap().work.host, 1);
                owner.request_quiesce().unwrap();
                assert!(owner.write_mem(0x300, &[1]).is_err());
                assert!(matches!(owner.try_drain(), Err(MachineError::Busy)));
                assert!(matches!(owner.fresh_reset(), Err(MachineError::Busy)));
                assert!(matches!(
                    owner.install(image(elf::addi(7, 0, 9))),
                    Err(MachineError::Busy)
                ));
                assert!(matches!(
                    owner.set_hart_state(CoreState::default()),
                    Err(MachineError::Busy)
                ));
                assert!(matches!(owner.teardown(), Err(MachineError::Busy)));
                *view.writer.before_lock.lock().unwrap() = None;
                release.send(()).unwrap();
                worker.join().unwrap().unwrap(); // admitted work finishes after quiesce
                let report = owner.try_drain().unwrap();
                assert_eq!(
                    *report.tohost.as_ref().unwrap().value.as_ref().unwrap(),
                    if path == 2 { 0 } else { 7 }
                );
                if path != 2 {
                    assert_eq!(owner.read_mem(0x200, 8).unwrap(), 55u64.to_le_bytes());
                }
                if reload {
                    owner.install(image(elf::addi(7, 0, 9))).unwrap();
                } else {
                    owner.fresh_reset().unwrap();
                }
                stale.lock().unwrap().write_dword(0x200, 0xaa).unwrap();
                stale
                    .lock()
                    .unwrap()
                    .write_dword(elf::TOHOST_SEGMENT_OFFSET, 9)
                    .unwrap();
                assert_eq!(stale.lock().unwrap().read_dword(0x200).unwrap(), 0xaa);
                assert_eq!(owner.read_mem(0x200, 8).unwrap(), [0; 8]);
                assert_eq!(
                    owner.read_mem(elf::TOHOST_SEGMENT_OFFSET, 8).unwrap(),
                    [0; 8]
                );
                owner.resume().unwrap();
                owner
                    .memory()
                    .unwrap()
                    .lock()
                    .unwrap()
                    .write_byte(0x300, 0xab)
                    .unwrap();
                assert_eq!(owner.read_mem(0x300, 1).unwrap(), [0xab]);
            }
        }
    }
}

type LateOperation = Box<dyn FnOnce() -> PhysicalAccessResult + Send>;
struct UnknownWrite {
    actual: crate::core::SharedDataAccess,
    pending: Arc<Mutex<Option<LateOperation>>>,
    calls: Arc<AtomicUsize>,
    before: Option<Box<dyn FnOnce() + Send>>,
}
impl PhysicalBackend for UnknownWrite {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        assert_eq!(request.category(), AccessCategory::DataWrite);
        if let Some(before) = self.before.take() {
            before();
        }
        self.calls.fetch_add(1, Ordering::SeqCst);
        let actual = self.actual.clone();
        let address = request.paddr();
        let width = request.width();
        let payload = request.write_payload().unwrap().to_vec();
        *self.pending.lock().unwrap() = Some(Box::new(move || {
            actual
                .lock()
                .unwrap()
                .access(PhysicalRequest::write(address, width, &payload).unwrap())
        }));
        Err(PhysicalBackendError::unknown(
            "accepted request can complete later",
        ))
    }
}
impl AtomicBackend for UnknownWrite {
    fn transact_atomic(&mut self, _: &AtomicRequest<'_>) -> AtomicBackendResult {
        panic!("ordinary-write unknown seam must not issue an atomic envelope")
    }
}

#[test]
fn unknown_delayed_ram_write_or_native_callback_never_permits_new_generation_or_retry() {
    for (kind, htif) in [
        (PlatformKind::Flat, false),
        (PlatformKind::Native, false),
        (PlatformKind::Native, true),
    ] {
        let callbacks = Arc::new(AtomicUsize::new(0));
        let calls = callbacks.clone();
        let mut config = MachineConfig::new(kind);
        config.htif_write = Some(Arc::new(move |value| {
            assert_eq!(value, 7);
            calls.fetch_add(1, Ordering::SeqCst);
        }));
        let owner = Machine::new(config);
        let original = image(elf::sd(2, 1, 0));
        owner.install(original.clone()).unwrap();
        let pending = Arc::new(Mutex::new(None));
        let requests = Arc::new(AtomicUsize::new(0));
        let ram;
        {
            // Bind one controlled adapter to the existing port/domain. This does
            // not create a second RAM or another architectural execution path.
            let mut domain = owner.inner.installed.lock().unwrap();
            let installed = domain.as_mut().unwrap();
            ram = installed.platform.ram.native_memory().unwrap();
            let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownWrite {
                actual: installed.platform.data.clone(),
                pending: pending.clone(),
                calls: requests.clone(),
                before: None,
            })));
            installed
                .core
                .set_physical_access(installed.platform.fetch.clone(), data);
            installed.core.state_mut().regs[1] = if htif { 0x4000_8000 } else { elf::BASE + 0x200 };
            installed.core.state_mut().regs[2] = if htif { 7 } else { 77 };
        }
        owner.resume().unwrap();
        let generation = owner.status().unwrap().generation;
        let turn = owner.step(true).unwrap();
        assert!(matches!(
            turn.hart().outcome,
            StepOutcome::SimulatorFailure(_)
        ));
        assert!(turn.hart().observation.is_none());
        let StepOutcome::SimulatorFailure(original_failure) = &turn.hart().outcome else {
            unreachable!()
        };
        let original_failure = original_failure.clone();
        assert!(!turn.hart().control.retired && !turn.hart().control.trap_entered);
        assert_eq!(turn.hart().control.minstret_after, 0);
        assert!(turn.events().is_empty() && turn.tohost().is_none());
        drop(turn);
        let (release, wait) = mpsc::channel();
        let (finished, completion) = mpsc::channel();
        let late = thread::spawn(move || {
            wait.recv().unwrap();
            pending.lock().unwrap().take().unwrap()().unwrap();
            finished.send(()).unwrap();
        });
        for after_effect in [false, true] {
            if after_effect {
                release.send(()).unwrap();
                completion.recv().unwrap();
            }
            // No callback/error-flag clear, exclusive borrow, mutex acquisition,
            // idle count or reconstruction can bypass the missing adapter proof.
            owner.request_quiesce().unwrap();
            assert!(matches!(
                owner.try_drain(),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.fresh_reset(),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.install(original.clone()),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.set_hart_state(CoreState::default()),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.teardown(),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.inspect(),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.resume(),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(matches!(
                owner.step(true),
                Err(MachineError::UnknownCompletion)
            ));
            assert!(owner
                .memory()
                .unwrap()
                .lock()
                .unwrap()
                .write_byte(0x300, 1)
                .is_err());
            assert_eq!(owner.status().unwrap().generation, generation);
            assert_eq!(
                owner.status().unwrap().last_failure.as_ref(),
                Some(&original_failure)
            );
            assert_eq!(requests.load(Ordering::SeqCst), 1);
        }
        late.join().unwrap();
        if htif {
            assert_eq!(callbacks.load(Ordering::SeqCst), 1);
            let domain = owner.inner.installed.lock().unwrap();
            assert_eq!(
                domain.as_ref().unwrap().platform.queued_events().unwrap(),
                [PlatformEvent::HtifWrite(7)]
            );
        } else {
            assert_eq!(ram.lock().unwrap().read_dword(0x200).unwrap(), 77);
        }
        assert!(
            matches!(owner.try_drain(), Err(MachineError::UnknownCompletion)),
            "joining a test thread is not an adapter proof API"
        );
        let domain = owner.inner.installed.lock().unwrap();
        let core = &domain.as_ref().unwrap().core;
        assert_eq!(core.state().pc, elf::BASE);
        assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
        assert!(core.unresolved_physical_access().is_some());
        drop(domain);
        let retained_ram = Arc::downgrade(&ram);
        drop(ram);
        drop(owner);
        assert!(
            retained_ram.upgrade().is_some(),
            "dropping a failed owner must quarantine, not tear down, an unresolved domain"
        );
    }
}

#[test]
fn boundary_receipts_and_admitted_host_work_keep_the_owner_alive() {
    let owner = owner(PlatformKind::Flat);
    let weak = Arc::downgrade(&owner.inner);
    let memory = owner.memory().unwrap();
    let receipt = owner.step(true).unwrap();
    drop(owner);
    assert!(weak.upgrade().is_some());
    assert!(receipt.hart().control.retired);
    drop(receipt);
    assert!(weak.upgrade().is_none());
    memory.lock().unwrap().write_byte(0x200, 9).unwrap(); // detached RAM only

    let owner = self::owner(PlatformKind::Flat);
    let weak = Arc::downgrade(&owner.inner);
    let view = owner.host_view().unwrap();
    let memory = view.memory.clone();
    let (admitted, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    *view.writer.before_lock.lock().unwrap() = Some(Arc::new(move || {
        admitted.send(()).unwrap();
        wait.lock().unwrap().recv().unwrap();
    }));
    let worker = thread::spawn(move || memory.lock().unwrap().write_byte(0x200, 77).unwrap());
    started.recv().unwrap();
    drop(owner);
    assert!(weak.upgrade().is_some());
    *view.writer.before_lock.lock().unwrap() = None;
    release.send(()).unwrap();
    worker.join().unwrap();
    assert!(weak.upgrade().is_none());
    assert_eq!(view.memory.lock().unwrap().read_byte(0x200).unwrap(), 77);
    view.memory.lock().unwrap().write_byte(0x200, 88).unwrap();
}

#[test]
fn unwound_native_callback_cannot_be_claimed_drained_or_restorable() {
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.htif_write = Some(Arc::new(|_| panic!("injected incomplete host callback")));
    let owner = Machine::new(config);
    owner.install(image(elf::sd(2, 1, 0))).unwrap();
    {
        let mut domain = owner.inner.installed.lock().unwrap();
        let core = &mut domain.as_mut().unwrap().core;
        core.state_mut().regs[1] = 0x4000_8000;
        core.state_mut().regs[2] = 7;
    }
    owner.resume().unwrap();
    let operation = owner.clone();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || operation.step(true)))
            .is_err()
    );
    assert!(owner.status().unwrap().uncertain);
    assert!(owner.status().unwrap().work.is_empty());
    owner.request_quiesce().unwrap();
    assert!(matches!(
        owner.try_drain(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        owner.fresh_reset(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        owner.teardown(),
        Err(MachineError::UnknownCompletion)
    ));
}

#[test]
fn abstract_unknown_completion_blocks_ndmreset_and_lifecycle_reuse() {
    for kind in [PlatformKind::Flat, PlatformKind::Native] {
        let mut owned = OwnedMachine::new(MachineConfig::new(kind), image(elf::nop())).unwrap();
        let control = owned.test_control();
        let generation = control.status().unwrap().generation;
        let pending = Arc::new(Mutex::new(None));
        let requests = Arc::new(AtomicUsize::new(0));
        let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownWrite {
            actual: owned.test_data(),
            pending: pending.clone(),
            calls: requests.clone(),
            before: None,
        })));
        owned.test_replace_data(data);
        owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
        drop(owned.step(false).unwrap());
        owned.dmi_write(0x04, 77).unwrap();
        owned.dmi_write(0x05, 0).unwrap();
        owned.dmi_write(0x06, (elf::BASE + 0x200) as u32).unwrap();
        owned.dmi_write(0x07, 0).unwrap();
        owned
            .dmi_write(0x17, (2 << 24) | (3 << 20) | (1 << 16))
            .unwrap();
        assert!(control.status().unwrap().uncertain);
        assert!(control.status().unwrap().last_failure.is_some());
        assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 7);
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(
            matches!(
                owned.dmi_write(0x10, (1 << 31) | 3),
                Err(MachineError::UnknownCompletion)
            ),
            "ndmreset bypassed an unresolved abstract memory completion"
        );
        assert_eq!(owned.dmi_read(0x10).unwrap() & 2, 0);
        let status = owned.dmi_read(0x11).unwrap();
        assert_ne!(status & (1 << 8), 0);
        assert_eq!(status & ((1 << 12) | (1 << 24)), 0);
        owned.dmi_write(0x16, 7 << 8).unwrap();
        assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 0);
        owned.dmi_write(0x04, 0x55).unwrap();
        assert!(matches!(
            owned.dmi_write(0x17, (3 << 20) | (1 << 17) | (1 << 16) | 0x1001),
            Err(MachineError::UnknownCompletion)
        ));
        assert_eq!(owned.state().regs[1], 0);
        assert!(matches!(
            owned.dmi_write(0x10, (1 << 30) | 1),
            Err(MachineError::UnknownCompletion)
        ));
        owned.dmi_write(0x10, 0).unwrap();
        owned.dmi_write(0x10, 1).unwrap();
        assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 0);
        assert!(control.status().unwrap().uncertain);
        assert_eq!(control.status().unwrap().generation, generation);
        assert!(matches!(
            control.resume(),
            Err(MachineError::UnknownCompletion)
        ));
        assert!(matches!(
            owned.quiesce(),
            Err(MachineError::UnknownCompletion)
        ));
        assert!(matches!(
            control.fresh_reset(),
            Err(MachineError::UnknownCompletion)
        ));
        assert!(matches!(
            control.teardown(),
            Err(MachineError::UnknownCompletion)
        ));
        pending.lock().unwrap().take().unwrap()().unwrap();
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(matches!(
            owned.quiesce(),
            Err(MachineError::UnknownCompletion)
        ));
        assert_eq!(control.status().unwrap().generation, generation);
    }
}

#[test]
fn abstract_memory_in_flight_blocks_lifecycle_before_unknown_is_reported() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
    drop(owned.step(false).unwrap());
    for (addr, value) in [
        (0x04, 77),
        (0x05, 0),
        (0x06, (elf::BASE + 0x200) as u32),
        (0x07, 0),
    ] {
        owned.dmi_write(addr, value).unwrap();
    }
    let control = owned.test_control();
    let generation = control.status().unwrap().generation;
    let pending = Arc::new(Mutex::new(None));
    let (entered, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownWrite {
        actual: owned.test_data(),
        pending: pending.clone(),
        calls: Arc::new(AtomicUsize::new(0)),
        before: Some(Box::new(move || {
            entered.send(()).unwrap();
            wait.recv().unwrap();
        })),
    })));
    owned.test_replace_data(data);
    let worker = thread::spawn(move || {
        let result = owned.dmi_write(0x17, (2 << 24) | (3 << 20) | (1 << 16));
        (owned, result)
    });
    started
        .recv_timeout(std::time::Duration::from_secs(2))
        .unwrap();
    assert_eq!(control.status().unwrap().work.hart, 1);
    control.request_quiesce().unwrap();
    assert!(matches!(control.try_drain(), Err(MachineError::Busy)));
    assert!(matches!(control.fresh_reset(), Err(MachineError::Busy)));
    assert!(matches!(
        control.install(image(elf::nop())),
        Err(MachineError::Busy)
    ));
    assert!(matches!(control.teardown(), Err(MachineError::Busy)));
    assert_eq!(control.status().unwrap().generation, generation);
    release.send(()).unwrap();
    let (mut owned, result) = worker.join().unwrap();
    result.unwrap();
    assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 7);
    assert!(control.status().unwrap().uncertain);
    assert!(matches!(
        control.try_drain(),
        Err(MachineError::UnknownCompletion)
    ));
    pending.lock().unwrap().take().unwrap()().unwrap();
    assert!(matches!(
        control.try_drain(),
        Err(MachineError::UnknownCompletion)
    ));
    assert_eq!(control.status().unwrap().generation, generation);
}

#[test]
fn dm_halt_request_survives_rebuild_and_explicit_clears_stay_cleared() {
    for replace in [false, true] {
        for clear in [None, Some(1), Some(0)] {
            let mut owned = OwnedMachine::new(
                MachineConfig::new(PlatformKind::Flat),
                image(elf::addi(1, 0, 7)),
            )
            .unwrap();
            owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
            owned.dmi_write(0x10, (1 << 16) | 1).unwrap();
            if let Some(value) = clear {
                owned.dmi_write(0x10, value).unwrap();
            }
            if replace {
                owned.install(image(elf::addi(1, 0, 7))).unwrap();
            } else {
                owned.fresh_reset().unwrap();
            }
            let turn = owned.step(false).unwrap();
            if clear.is_none() {
                assert!(matches!(
                    turn.hart().outcome,
                    StepOutcome::DebugHalted { .. }
                ));
                assert!(!turn.hart().control.retired);
                assert_eq!(owned.dpc(), elf::BASE);
                assert_eq!(owned.state().regs[1], 0);
            } else {
                assert!(turn.hart().control.retired);
                assert_eq!(owned.state().regs[1], 7);
            }
        }
    }
}

#[test]
fn shared_halt_setter_and_dm_control_use_the_same_request_across_reset() {
    for clear in [false, true] {
        let owner = owner(PlatformKind::Native);
        {
            let mut domain = owner.inner.installed.lock().unwrap();
            let core = &mut domain.as_mut().unwrap().core;
            owner
                .inner
                .debug
                .dm
                .lock()
                .unwrap()
                .dmi_write(core, 0x10, (1 << 31) | 1);
        }
        owner.set_haltreq(!clear).unwrap();
        owner.request_quiesce().unwrap();
        owner.try_drain().unwrap();
        owner.fresh_reset().unwrap();
        owner.resume().unwrap();
        let turn = owner.step(false).unwrap();
        assert_eq!(turn.hart().control.retired, clear);
        assert_eq!(
            matches!(turn.hart().outcome, StepOutcome::DebugHalted { .. }),
            !clear
        );
    }
}

#[test]
fn poisoned_debug_module_cannot_publish_a_new_generation() {
    let owner = owner(PlatformKind::Flat);
    owner.write_mem(0x200, &[0x55]).unwrap();
    owner.request_quiesce().unwrap();
    owner.try_drain().unwrap();
    let generation = owner.status().unwrap().generation;
    let debug = owner.clone();
    assert!(thread::spawn(move || {
        let _guard = debug.inner.debug.dm.lock().unwrap();
        panic!("poison the Debug Module");
    })
    .join()
    .is_err());
    assert!(matches!(owner.fresh_reset(), Err(MachineError::Poisoned)));
    assert_eq!(owner.status().unwrap().generation, generation);
    assert_eq!(owner.read_mem(0x200, 1).unwrap(), [0x55]);
}

/// §7 item 3. A `haltreq` posted while an instruction is in progress stays
/// queued. Applying it there is the negative mutation.
#[test]
fn dmi_haltreq_waits_until_the_instruction_boundary() {
    use std::time::{Duration, Instant};

    let mut owned = OwnedMachine::new(
        MachineConfig::new(PlatformKind::Flat),
        image(elf::addi(1, 0, 1)),
    )
    .unwrap();
    let port = owned.attach_debugger();
    owned.set_in_instruction_for_test(true);
    let worker = thread::spawn(move || port.dmi_write(0x10, (1 << 31) | 1));
    let start = Instant::now();
    while owned.debug_queue_len() == 0 {
        assert!(
            !owned.hart_haltreq(),
            "haltreq was accepted mid-instruction"
        );
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "haltreq was accepted mid-instruction: the transaction never waited"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        !owned.hart_haltreq(),
        "haltreq was accepted mid-instruction"
    );
    let refused = owned.dmi_write(0x10, (1 << 31) | 1);
    assert!(
        refused.is_err(),
        "haltreq was accepted mid-instruction: direct DMI ran during the instruction"
    );
    owned.debug_boundary(false).unwrap();
    assert!(
        !owned.hart_haltreq(),
        "haltreq was accepted mid-instruction"
    );
    assert_eq!(owned.debug_queue_len(), 1);
    owned.set_in_instruction_for_test(false);
    owned.debug_boundary(false).unwrap();
    worker.join().unwrap().unwrap();
    assert!(owned.hart_haltreq());
    assert!(!owned.hart_in_debug_mode());
}

/// U2 §4 U3: a started command advances one Hart grant per boundary, and
/// DMI is serviced between grants. No socket input is needed for progress.
#[test]
fn program_buffer_grants_are_bounded_and_interleave_with_dmi() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let mut owned = OwnedMachine::new(MachineConfig::new(kind), image(elf::nop())).unwrap();
        owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
        drop(owned.step(false).unwrap());
        assert!(owned.hart_in_debug_mode());
        // addi x1, x1, 1 in both words. The implicit EBREAK ends the command.
        owned.dmi_write(0x20, 0x0010_8093).unwrap();
        owned.dmi_write(0x21, 0x0010_8093).unwrap();
        owned.dmi_write(0x17, (3 << 20) | (1 << 18)).unwrap();
        assert!(owned.debug_command_busy().unwrap());
        assert_eq!(owned.state().regs[1], 0, "the command write runs no word");

        let busy = owned.dmi_read(0x16).unwrap();
        assert_ne!(busy & (1 << 12), 0, "abstractcs.busy");
        assert_eq!(busy & 0xf, 4, "datacount");

        owned.debug_boundary(false).unwrap();
        assert_eq!(owned.state().regs[1], 1, "one grant per boundary");
        assert_ne!(owned.dmi_read(0x16).unwrap() & (1 << 12), 0);

        owned.debug_boundary(false).unwrap();
        assert_eq!(owned.state().regs[1], 2);
        owned.debug_boundary(false).unwrap();
        assert!(!owned.debug_command_busy().unwrap());
        assert_eq!(owned.state().regs[1], 2, "the implicit EBREAK ran nothing");
        assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 0);
        assert!(owned.hart_in_debug_mode());
    }
}

/// U2 §3: a resume request cannot preempt an unfinished command. Completion
/// becomes visible first, and the resume then proceeds.
#[test]
fn resume_cannot_preempt_an_unfinished_program_buffer_command() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
    drop(owned.step(false).unwrap());
    owned.dmi_write(0x20, 0x0010_8093).unwrap();
    owned.dmi_write(0x21, 0x0010_8093).unwrap();
    owned.dmi_write(0x17, (3 << 20) | (1 << 18)).unwrap();
    assert!(owned.debug_command_busy().unwrap());

    owned.dmi_write(0x10, (1 << 30) | 1).unwrap();
    assert!(
        owned.hart_in_debug_mode(),
        "a resume request preempted the unfinished command"
    );
    assert!(owned.debug_command_busy().unwrap());
    assert_ne!(
        owned.dmi_read(0x11).unwrap() & (1 << 8),
        0,
        "the hart stays halted while the command runs"
    );

    while owned.debug_command_busy().unwrap() {
        owned.debug_boundary(false).unwrap();
    }
    assert_eq!(owned.state().regs[1], 2);
    assert!((owned.dmi_read(0x16).unwrap() >> 8) & 7 == 0);

    owned.dmi_write(0x10, (1 << 30) | 1).unwrap();
    assert!(
        !owned.hart_in_debug_mode(),
        "the resume runs after completion"
    );
}

/// U2 §3: an unresolved completion refuses a `data` access that would start
/// an automatic command, and the refusal changes no Hart state.
#[test]
fn unknown_completion_refuses_a_triggering_autoexec_access() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
    drop(owned.step(false).unwrap());
    owned
        .dmi_write(0x17, (3 << 20) | (1 << 17) | 0x1001)
        .unwrap();
    assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 0);
    owned.dmi_write(0x18, 1).unwrap();

    let pending = Arc::new(Mutex::new(None));
    let requests = Arc::new(AtomicUsize::new(0));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownWrite {
        actual: owned.test_data(),
        pending,
        calls: requests.clone(),
        before: None,
    })));
    owned.test_replace_data(data);
    owned.dmi_write(0x04, 77).unwrap();
    owned.dmi_write(0x05, 0).unwrap();
    owned.dmi_write(0x06, (elf::BASE + 0x200) as u32).unwrap();
    owned.dmi_write(0x07, 0).unwrap();
    owned
        .dmi_write(0x17, (2 << 24) | (3 << 20) | (1 << 16))
        .unwrap();
    assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 7);
    owned.dmi_write(0x16, 7 << 8).unwrap();
    assert_eq!((owned.dmi_read(0x16).unwrap() >> 8) & 7, 0);

    assert!(matches!(
        owned.dmi_write(0x04, 0x55),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        owned.dmi_read(0x04),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(requests.load(Ordering::SeqCst), 1, "no second data request");
}

/// U2 §3: quiesce stops further debug grants. The active command ends with
/// `cmderr` 4 and no additional word runs.
#[test]
fn quiesce_stops_further_debug_grants() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
    drop(owned.step(false).unwrap());
    // addi x1, x1, 1 in both words; the implicit EBREAK ends the command.
    owned.dmi_write(0x20, 0x0010_8093).unwrap();
    owned.dmi_write(0x21, 0x0010_8093).unwrap();
    owned.dmi_write(0x17, (3 << 20) | (1 << 18)).unwrap();
    owned.debug_boundary(false).unwrap();
    assert_eq!(owned.state().regs[1], 1, "the first grant ran");

    owned.test_control().request_quiesce().unwrap();
    owned.debug_boundary(false).unwrap();
    assert_eq!(
        owned.state().regs[1],
        1,
        "a quiesced Machine ran another debug word"
    );
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(
        (owned.dmi_read(0x16).unwrap() >> 8) & 7,
        4,
        "the command was aborted instead of completed"
    );
    assert!(
        owned.test_control().try_drain().is_ok(),
        "the aborted command held work open"
    );
}

/// Records whether the DM lock is available from inside a data transaction.
struct DmLockProbe {
    inner: std::sync::Arc<Inner>,
    unlocked: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AtomicBackend for DmLockProbe {
    fn transact_atomic(&mut self, _: &AtomicRequest<'_>) -> AtomicBackendResult {
        panic!("the DM-lock probe must not issue an atomic envelope")
    }
}

impl PhysicalBackend for DmLockProbe {
    fn transact(&mut self, _request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.unlocked
            .store(self.inner.debug.dm.try_lock().is_ok(), Ordering::SeqCst);
        Err(PhysicalBackendError::host(
            "probe observes the DM lock and refuses the access",
        ))
    }
}

/// U2 §3: no DM lock is held while the Hart executes or reaches a physical
/// target, and a known simulator failure is reported as `cmderr` 7 while the
/// Machine records it as a failure rather than as unknown completion.
#[test]
fn program_buffer_grant_holds_no_dm_lock_and_records_a_known_failure() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    owned.dmi_write(0x10, (1 << 31) | 1).unwrap();
    drop(owned.step(false).unwrap());

    let unlocked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = std::sync::Arc::new(std::sync::Mutex::new(ValidatedPhysicalAccess::new(
        DmLockProbe {
            inner: std::sync::Arc::clone(&owned.test_control().inner),
            unlocked: unlocked.clone(),
        },
    )));
    owned.test_replace_data(probe);
    // sw x1, 0(x3), then a nop. The store reaches the probed data port.
    owned.dmi_write(0x20, 0x0011_a023).unwrap();
    owned.dmi_write(0x21, 0x0000_0013).unwrap();
    owned.dmi_write(0x17, (3 << 20) | (1 << 18)).unwrap();
    let mut reported = false;
    while owned.debug_command_busy().unwrap() {
        reported |= owned.debug_boundary(false).unwrap().failure.is_some();
    }

    assert!(
        reported,
        "the known simulator failure was not reported at a debug boundary"
    );

    assert!(
        unlocked.load(Ordering::SeqCst),
        "the DM lock was held while the Hart executed"
    );
    assert_eq!(
        (owned.dmi_read(0x16).unwrap() >> 8) & 7,
        7,
        "a known host failure is cmderr 7"
    );
    assert!(
        owned.failure().is_some(),
        "the known simulator failure is not recorded"
    );
    assert!(
        !owned.test_control().status().unwrap().uncertain,
        "a known failure is not unknown completion"
    );

    // A failed Machine admits no new system bus access: the re-staged
    // transfer stays busy and commits nothing until a fresh reset (criterion
    // 4, ADR-0004 §12.2).
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 0x1144_2277).unwrap();
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "a failed Machine granted a new system bus access"
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(storage_read(&owned, SBA_OFFSET), 0);
}

// ---- Program-buffer U3: Machine scheduling, lifecycle and physical access ----

const DM_DMCONTROL: u8 = 0x10;
const DM_ABSTRACTCS: u8 = 0x16;
const DM_COMMAND: u8 = 0x17;
const DM_ABSTRACTAUTO: u8 = 0x18;
const DM_DATA0: u8 = 0x04;
const DM_PROGBUF0: u8 = 0x20;
const DM_DMACTIVE: u32 = 1;
const DM_NDMRESET: u32 = 1 << 1;
const DM_HALTREQ: u32 = 1 << 31;
const DM_RESUMEREQ: u32 = 1 << 30;

/// `sw rs2, imm(rs1)`.
fn sw_word(rs2: u8, rs1: u8, imm: i32) -> u32 {
    let imm = imm as u32;
    ((imm & 0xfe0) << 20)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (2 << 12)
        | ((imm & 0x1f) << 7)
        | 0x23
}

/// `lw rd, imm(rs1)`.
fn lw_word(rd: u8, rs1: u8, imm: i32) -> u32 {
    let imm = imm as u32;
    ((imm & 0xfff) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x03
}

/// `amoadd.d rd, rs2, (rs1)`.
fn amoadd_d(rd: u8, rs2: u8, rs1: u8) -> u32 {
    (u32::from(rs2) << 20) | (u32::from(rs1) << 15) | (3 << 12) | (u32::from(rd) << 7) | 0x2f
}

/// `lr.d rd, (rs1)`.
fn lr_d(rd: u8, rs1: u8) -> u32 {
    (0b00010 << 27) | (u32::from(rs1) << 15) | (3 << 12) | (u32::from(rd) << 7) | 0x2f
}

/// `sc.d rd, rs2, (rs1)`.
fn sc_d(rd: u8, rs2: u8, rs1: u8) -> u32 {
    (0b00011 << 27)
        | (u32::from(rs2) << 20)
        | (u32::from(rs1) << 15)
        | (3 << 12)
        | (u32::from(rd) << 7)
        | 0x2f
}

fn storage_write(owned: &OwnedMachine, offset: u64, value: u64) {
    owned
        .memory()
        .lock()
        .unwrap()
        .write_dword(offset, value)
        .unwrap();
}

fn storage_read(owned: &OwnedMachine, offset: u64) -> u64 {
    owned.memory().lock().unwrap().read_dword(offset).unwrap()
}

fn halted_by_dmi(owned: &mut OwnedMachine) {
    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_HALTREQ)
        .unwrap();
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::DebugHalted { .. }
    ));
    assert!(owned.hart_in_debug_mode());
}

fn write_gpr(owned: &mut OwnedMachine, index: u32, value: u64) {
    owned.dmi_write(DM_DATA0, value as u32).unwrap();
    owned.dmi_write(DM_DATA0 + 1, (value >> 32) as u32).unwrap();
    owned
        .dmi_write(
            DM_COMMAND,
            (3 << 20) | (1 << 17) | (1 << 16) | 0x1000 | index,
        )
        .unwrap();
    assert_eq!(cmderr_of(owned), 0, "Access Register write to x{index}");
}

/// Access Memory, 4 bytes, write. Uses the Hart's data port like the buffer.
fn abstract_write(owned: &mut OwnedMachine, address: u64, value: u32) {
    owned.dmi_write(DM_DATA0, value).unwrap();
    owned.dmi_write(DM_DATA0 + 2, address as u32).unwrap();
    owned
        .dmi_write(DM_DATA0 + 3, (address >> 32) as u32)
        .unwrap();
    owned
        .dmi_write(DM_COMMAND, (2 << 24) | (2 << 20) | (1 << 16))
        .unwrap();
    assert_eq!(cmderr_of(owned), 0, "Access Memory write to {address:#x}");
}

/// Access Memory, 8 bytes, write.
fn abstract_write_dword(owned: &mut OwnedMachine, address: u64, value: u64) {
    owned.dmi_write(DM_DATA0, value as u32).unwrap();
    owned.dmi_write(DM_DATA0 + 1, (value >> 32) as u32).unwrap();
    owned.dmi_write(DM_DATA0 + 2, address as u32).unwrap();
    owned
        .dmi_write(DM_DATA0 + 3, (address >> 32) as u32)
        .unwrap();
    owned
        .dmi_write(DM_COMMAND, (2 << 24) | (3 << 20) | (1 << 16))
        .unwrap();
    assert_eq!(cmderr_of(owned), 0, "Access Memory write to {address:#x}");
}

/// Queue one DMI write for the next debug boundary, as the transport does.
fn enqueue_dmi(owned: &OwnedMachine, addr: u8, value: u32) {
    let (reply, _wait) = mpsc::channel();
    owned
        .test_control()
        .inner
        .debug
        .queue()
        .lock()
        .unwrap()
        .push_back(super::debug_hub::DmiCall {
            addr,
            write: Some(value),
            reply,
        });
}

fn cmderr_of(owned: &mut OwnedMachine) -> u32 {
    (owned.dmi_read(DM_ABSTRACTCS).unwrap() >> 8) & 7
}

/// Write both words and start them through Access Register `postexec`.
fn postexec_buffer(owned: &mut OwnedMachine, words: [u32; 2]) {
    owned.dmi_write(DM_PROGBUF0, words[0]).unwrap();
    owned.dmi_write(DM_PROGBUF0 + 1, words[1]).unwrap();
    owned.dmi_write(DM_COMMAND, (3 << 20) | (1 << 18)).unwrap();
    assert!(owned.debug_command_busy().unwrap());
}

/// Run the active command to its implicit EBREAK, returning every boundary
/// fact. Nothing beyond the two words and the implicit EBREAK may run.
fn run_grants(owned: &mut OwnedMachine) -> DebugBoundary {
    let mut facts = DebugBoundary::default();
    for _ in 0..3 {
        facts.merge(owned.debug_boundary(false).unwrap());
        if !owned.debug_command_busy().unwrap() {
            return facts;
        }
    }
    panic!("the command ran more grants than two words plus the implicit EBREAK");
}

/// U3 §3 accounting: a debug instruction's Platform effect is returned at the
/// grant that issued it, not attached to a later guest turn.
#[test]
fn program_buffer_platform_effect_is_returned_at_its_grant_boundary() {
    let transmitted = Arc::new(Mutex::new(Vec::new()));
    let sink = transmitted.clone();
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.uart_output = Some(Arc::new(move |byte| sink.lock().unwrap().push(byte)));
    let mut owned = OwnedMachine::new(config, image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    write_gpr(&mut owned, 5, crate::executor::SYSTEM_BUS_UART_BASE);
    write_gpr(&mut owned, 6, u64::from(b'A'));
    postexec_buffer(&mut owned, [elf::sb(6, 5, 0), elf::nop()]);

    let first = owned.debug_boundary(false).unwrap();
    assert_eq!(first.events, [PlatformEvent::UartTransmit(b'A')]);
    assert_eq!(
        *transmitted.lock().unwrap(),
        [b'A'],
        "the byte left at the issuing boundary"
    );
    assert_eq!(
        owned.state().csr.read(machine::MINSTRET).unwrap(),
        0,
        "debug work is not guest retirement"
    );
    assert_eq!(owned.state().csr.read(machine::MCYCLE).unwrap(), 0);

    let rest = run_grants(&mut owned);
    assert!(rest.events.is_empty());
    assert_eq!(cmderr_of(&mut owned), 0);

    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    let turn = owned.step(false).unwrap();
    assert!(
        turn.events().is_empty(),
        "a debug-originated Platform effect leaked into a guest turn"
    );
}

/// U3 §3: a native PLIC claim runs through the Hart's data port. The claim is
/// a Platform side effect, not an alternate debugger memory path.
#[test]
fn program_buffer_native_plic_claim_uses_the_hart_data_path() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let plic = crate::executor::SYSTEM_BUS_PLIC_BASE;
    let claim = plic + 0x20_0004;
    abstract_write(&mut owned, plic + 4, 1);
    abstract_write(&mut owned, plic + 0x2000, 1 << 1);
    write_gpr(&mut owned, 5, crate::executor::SYSTEM_BUS_PLIC_GATEWAY);
    write_gpr(&mut owned, 6, 1);
    write_gpr(&mut owned, 7, claim);
    postexec_buffer(&mut owned, [sw_word(6, 5, 0), lw_word(8, 7, 0)]);

    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    assert_eq!(
        owned.state().regs[8],
        1,
        "the buffer load claimed PLIC source 1"
    );
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(owned.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(owned.state().csr.read(machine::MCYCLE).unwrap(), 0);

    // Claiming cleared pending: the normal Abstract Access Memory path, which
    // uses the same data port, sees no second source.
    owned.dmi_write(DM_DATA0 + 2, claim as u32).unwrap();
    owned.dmi_write(DM_DATA0 + 3, (claim >> 32) as u32).unwrap();
    owned.dmi_write(DM_COMMAND, (2 << 24) | (2 << 20)).unwrap();
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(
        owned.dmi_read(DM_DATA0).unwrap(),
        0,
        "source 1 was already claimed"
    );
}

/// U3 §4: AMO and LR/SC from the buffer use the Hart's reservation and atomic
/// envelope, not a DM-side read/modify/write.
#[test]
fn program_buffer_atomics_and_reservations_use_the_hart_data_path() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Flat), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    storage_write(&owned, 0x200, 5);
    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 3);
    postexec_buffer(&mut owned, [amoadd_d(7, 6, 5), elf::nop()]);
    run_grants(&mut owned);
    assert_eq!(owned.state().regs[7], 5, "amo returns the old value");
    assert_eq!(storage_read(&owned, 0x200), 8);

    write_gpr(&mut owned, 6, 9);
    postexec_buffer(&mut owned, [lr_d(7, 5), sc_d(8, 6, 5)]);
    run_grants(&mut owned);
    assert_eq!(
        owned.state().regs[8],
        0,
        "SC succeeds on the held reservation"
    );
    assert_eq!(storage_read(&owned, 0x200), 9);

    // A reservation for another address makes the store-conditional fail.
    write_gpr(&mut owned, 7, elf::BASE + 0x210);
    write_gpr(&mut owned, 6, 0x55);
    postexec_buffer(&mut owned, [lr_d(9, 5), sc_d(8, 6, 7)]);
    run_grants(&mut owned);
    assert_eq!(
        owned.state().regs[8],
        1,
        "SC must fail for an address the reservation does not cover"
    );
    assert_eq!(storage_read(&owned, 0x210), 0);
}

/// U3 §3: a buffer exception is a command error. It does not enter a guest
/// handler, does not quarantine unknown completion, and leaves resume legal.
#[test]
fn program_buffer_fault_is_a_command_error_and_keeps_the_machine_resumable() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let dpc = owned.dpc();
    write_gpr(&mut owned, 6, 0xffff_ffff_ffff_0000);
    postexec_buffer(&mut owned, [elf::ld(5, 6, 0), elf::nop()]);

    let facts = owned.debug_boundary(false).unwrap();
    assert!(
        facts.failure.is_none(),
        "a target fault is not unknown completion"
    );
    assert_eq!(cmderr_of(&mut owned), 3);
    assert!(!owned.debug_command_busy().unwrap());
    let status = owned.test_control().status().unwrap();
    assert!(!status.failed && !status.uncertain);
    assert_eq!(owned.dpc(), dpc);
    assert!(owned.hart_in_debug_mode());
    for csr in [machine::MEPC, machine::MCAUSE, machine::MTVAL] {
        assert_eq!(
            owned.state().csr.read(csr).unwrap(),
            0,
            "a buffer exception wrote guest trap CSRs"
        );
    }

    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
}

/// U3 §3 accounting: debug work adds no automatic guest counter, clock or
/// application cycle charge. Explicit legal writes keep their effects.
#[test]
fn program_buffer_work_adds_no_guest_counter_cycle_or_clock_charge() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let mut owned = OwnedMachine::new(MachineConfig::new(kind), image(elf::nop())).unwrap();
        owned.step(false).unwrap();
        let frozen = (
            owned.state().csr.read(machine::MINSTRET).unwrap(),
            owned.state().csr.read(machine::MCYCLE).unwrap(),
            owned
                .state()
                .csr
                .read(crate::csr::unprivileged::TIME)
                .unwrap(),
        );
        assert_eq!(frozen, (1, 1, 1));

        halted_by_dmi(&mut owned);
        let dpc = owned.dpc();
        postexec_buffer(&mut owned, [elf::addi(9, 9, 1), elf::addi(9, 9, 2)]);
        run_grants(&mut owned);
        assert_eq!(owned.state().regs[9], 3);
        assert_eq!(cmderr_of(&mut owned), 0);
        assert_eq!(
            (
                owned.state().csr.read(machine::MINSTRET).unwrap(),
                owned.state().csr.read(machine::MCYCLE).unwrap(),
                owned
                    .state()
                    .csr
                    .read(crate::csr::unprivileged::TIME)
                    .unwrap(),
            ),
            frozen,
            "{kind:?}: debug work charged the guest profile"
        );
        assert_eq!(owned.dpc(), dpc, "debug grants changed the saved resume PC");

        // An explicit counter write keeps its architectural effect.
        write_gpr(&mut owned, 6, 42);
        let csrrw = (u32::from(machine::MCYCLE) << 20) | (6 << 15) | (1 << 12) | (0x73);
        postexec_buffer(&mut owned, [csrrw, elf::nop()]);
        run_grants(&mut owned);
        assert_eq!(
            owned.state().csr.read(machine::MCYCLE).unwrap(),
            42,
            "{kind:?}: an explicit counter write was dropped"
        );
        assert_eq!(owned.state().csr.read(machine::MINSTRET).unwrap(), frozen.0);
        assert_eq!(owned.dpc(), dpc);
    }
}

/// U3 §3: an explicit CLINT time write keeps its effect at the issuing
/// boundary. The write is a Platform fact, not a guest retirement.
#[test]
fn program_buffer_explicit_time_write_is_observable_at_its_boundary() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let mtime = crate::executor::SYSTEM_BUS_CLINT_BASE + 0xBFF8;
    write_gpr(&mut owned, 5, mtime);
    write_gpr(&mut owned, 6, 0x1234);
    // Store mtime, then read `time` in the next grant. The Hart boundary
    // samples the Platform input without advancing the clock.
    let read_time = (u32::from(crate::csr::unprivileged::TIME) << 20) | (2 << 12) | (8 << 7) | 0x73;
    postexec_buffer(&mut owned, [elf::sd(6, 5, 0), read_time]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    assert!(
        facts.events.is_empty(),
        "a CLINT write is not an event queue fact"
    );
    assert_eq!(
        owned.state().regs[8],
        0x1234,
        "the next debug instruction did not observe the explicit time write"
    );
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(owned.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(owned.state().csr.read(machine::MCYCLE).unwrap(), 0);

    // The same sample makes a debugger Access Register read current.
    abstract_write_dword(&mut owned, mtime, 0x5678);
    owned
        .dmi_write(
            DM_COMMAND,
            (3 << 20) | (1 << 17) | u32::from(crate::csr::unprivileged::TIME),
        )
        .unwrap();
    assert_eq!(cmderr_of(&mut owned), 0);
    let low = u64::from(owned.dmi_read(DM_DATA0).unwrap());
    let high = u64::from(owned.dmi_read(DM_DATA0 + 1).unwrap());
    assert_eq!(
        low | (high << 32),
        0x5678,
        "a debugger read of time saw the stale sample"
    );
}

/// U3 §3: an unresolved physical completion from a buffer store quarantines
/// the Machine. W1C, DM reset and `ndmreset` cannot clear it or make the
/// Machine resumable without adapter proof.
#[test]
fn program_buffer_unknown_completion_resists_w1c_dm_reset_and_ndmreset() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let pending = Arc::new(Mutex::new(None));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownWrite {
        actual: owned.test_data(),
        pending: pending.clone(),
        calls: Arc::new(AtomicUsize::new(0)),
        before: None,
    })));
    owned.test_replace_data(data);
    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 77);
    postexec_buffer(&mut owned, [elf::sd(6, 5, 0), elf::nop()]);

    let facts = owned.debug_boundary(false).unwrap();
    assert!(
        facts.failure.is_some(),
        "the grant failure was not reported at its boundary"
    );
    assert_eq!(cmderr_of(&mut owned), 7);
    let status = owned.test_control().status().unwrap();
    assert!(status.uncertain && status.failed);

    owned.dmi_write(DM_ABSTRACTCS, 7 << 8).unwrap();
    assert_eq!(
        cmderr_of(&mut owned),
        0,
        "W1C clears the sticky command error"
    );
    assert!(
        matches!(
            owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_NDMRESET | DM_HALTREQ),
            Err(MachineError::UnknownCompletion)
        ),
        "ndmreset cleared an unresolved physical completion"
    );

    // DM reset clears its own command state and leaves the Hart halted.
    owned.dmi_write(DM_DMCONTROL, 0).unwrap();
    owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE).unwrap();
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(owned.dmi_read(DM_PROGBUF0).unwrap(), 0);
    assert!(owned.hart_in_debug_mode(), "DM reset resumed the Hart");

    let control = owned.test_control();
    assert!(control.status().unwrap().uncertain);
    assert!(matches!(
        control.resume(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        control.fresh_reset(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        control.teardown(),
        Err(MachineError::UnknownCompletion)
    ));
    let after = owned.debug_boundary(false).unwrap();
    assert!(after.events.is_empty() && after.failure.is_none());
    assert!(!owned.debug_command_busy().unwrap());

    pending.lock().unwrap().take().unwrap()().unwrap();
    assert!(
        matches!(control.try_drain(), Err(MachineError::UnknownCompletion)),
        "a late completion is not adapter proof"
    );
}

/// U3 §3: reset and drain stop new grants and wait for started physical work
/// before lifecycle mutation. A started grant finishes, then the command is
/// aborted instead of running another word.
#[test]
fn started_debug_grant_drains_before_lifecycle_mutation() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.uart_output = Some(Arc::new(move |byte| {
        assert_eq!(byte, b'B');
        entered_tx.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
    }));
    let mut owned = OwnedMachine::new(config, image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    write_gpr(&mut owned, 5, crate::executor::SYSTEM_BUS_UART_BASE);
    write_gpr(&mut owned, 6, u64::from(b'B'));
    postexec_buffer(&mut owned, [elf::sb(6, 5, 0), elf::addi(9, 9, 1)]);

    let control = owned.test_control();
    let generation = control.status().unwrap().generation;
    let worker = thread::spawn(move || {
        let facts = owned.debug_boundary(false).unwrap();
        (owned, facts)
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("the debug grant did not reach its physical callback");
    assert_eq!(control.status().unwrap().work.hart, 1);
    control.request_quiesce().unwrap();
    assert!(
        matches!(control.try_drain(), Err(MachineError::Busy)),
        "started debug work was acknowledged as drained"
    );
    assert!(matches!(control.fresh_reset(), Err(MachineError::Busy)));
    assert!(matches!(control.teardown(), Err(MachineError::Busy)));
    assert_eq!(control.status().unwrap().generation, generation);

    release_tx.send(()).unwrap();
    let (mut owned, facts) = worker.join().unwrap();
    assert_eq!(facts.events, [PlatformEvent::UartTransmit(b'B')]);
    assert!(!control.status().unwrap().uncertain);

    // Quiesce stopped further grants: the remaining word does not run.
    let after = owned.debug_boundary(false).unwrap();
    assert!(after.events.is_empty() && after.failure.is_none());
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(cmderr_of(&mut owned), 4);
    assert_eq!(owned.state().regs[9], 0);
    owned.quiesce().unwrap();
}

/// U3 §3: a non-DM reset aborts the active command with `cmderr` 4 and keeps
/// the DM-owned buffer and `autoexec` state. It does not automatically resume.
#[test]
fn ndmreset_stops_grants_aborts_the_command_and_preserves_dm_state() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let words = [elf::addi(9, 9, 1), elf::addi(9, 9, 2)];
    owned.dmi_write(DM_PROGBUF0, words[0]).unwrap();
    owned.dmi_write(DM_PROGBUF0 + 1, words[1]).unwrap();
    owned.dmi_write(DM_ABSTRACTAUTO, 0x0003_000f).unwrap();
    owned.dmi_write(DM_COMMAND, (3 << 20) | (1 << 18)).unwrap();
    assert!(owned.debug_command_busy().unwrap());
    owned.debug_boundary(false).unwrap();
    assert_eq!(owned.state().regs[9], 1, "the first grant ran");

    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_NDMRESET | DM_HALTREQ)
        .unwrap();
    assert_eq!(
        cmderr_of(&mut owned),
        4,
        "a non-DM reset aborts the command"
    );
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(
        owned.dmi_read(DM_PROGBUF0).unwrap(),
        words[0],
        "the buffer survived the reset"
    );
    assert_eq!(owned.dmi_read(DM_PROGBUF0 + 1).unwrap(), words[1]);
    assert_eq!(
        owned.dmi_read(DM_ABSTRACTAUTO).unwrap(),
        0x0003_000f,
        "autoexec survived the reset"
    );
    assert_eq!(owned.state().regs[9], 0, "a further word ran during reset");
    assert_eq!(owned.state().pc, elf::BASE);
    assert!(
        !owned.hart_in_debug_mode(),
        "the reset itself halted the Hart"
    );
    assert_eq!(owned.state().csr.read(machine::MINSTRET).unwrap(), 0);

    // Release with the persistent halt request. The Hart halts at the reset
    // entry; the reset itself does not resume it.
    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_HALTREQ)
        .unwrap();
    assert!(owned.hart_in_debug_mode());
    assert_eq!(owned.dpc(), elf::BASE);
    assert_eq!(owned.state().regs[9], 0);
    assert_eq!(cmderr_of(&mut owned), 4);
}

/// U3 §3: DM reset clears command, buffer, data and autoexec state. It does
/// not resume the Hart and forgets the last accepted command.
#[test]
fn dm_reset_clears_command_state_and_does_not_resume_the_hart() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    write_gpr(&mut owned, 9, 0);
    owned.dmi_write(DM_DATA0, 0x1234).unwrap();
    // Last accepted command: write x9 from data0.
    owned
        .dmi_write(DM_COMMAND, (3 << 20) | (1 << 17) | (1 << 16) | 0x1009)
        .unwrap();
    assert_eq!(owned.state().regs[9], 0x1234);
    owned.dmi_write(DM_ABSTRACTAUTO, 0x0003_000f).unwrap();
    postexec_buffer(&mut owned, [elf::addi(9, 0, 5), elf::nop()]);
    assert!(owned.debug_command_busy().unwrap());

    owned.dmi_write(DM_DMCONTROL, 0).unwrap();
    assert!(
        owned.hart_in_debug_mode(),
        "DM reset must not resume the Hart"
    );
    owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE).unwrap();
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(owned.dmi_read(DM_PROGBUF0).unwrap(), 0);
    assert_eq!(owned.dmi_read(DM_DATA0).unwrap(), 0);
    assert_eq!(owned.dmi_read(DM_ABSTRACTAUTO).unwrap(), 0);

    // No remembered command: a data write cannot replay the buffer.
    owned.dmi_write(DM_ABSTRACTAUTO, 1).unwrap();
    assert_eq!(owned.state().regs[9], 0x1234);
    owned.dmi_write(DM_DATA0, 1).unwrap();
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(
        owned.state().regs[9],
        0x1234,
        "the forgotten command replayed"
    );

    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    assert!(!owned.hart_in_debug_mode());
}

/// U3 §4: `mprven=1` and MPP=S make the buffer use the Hart's Sv39 walk. The
/// fixture maps a virtual address through the Machine's own RAM.
#[test]
fn program_buffer_mprv_sv39_load_uses_the_hart_page_table_path() {
    const ROOT: u64 = elf::BASE + 0x30000;
    const LVL1: u64 = elf::BASE + 0x31000;
    const LVL0: u64 = elf::BASE + 0x32000;
    const DATA: u64 = elf::BASE + 0x33000;
    const VA: u64 = 0x2000;

    let image = Arc::new(
        LoadImage::parse(&elf::elf_with_code(&[elf::nop()], 0, true, false, 0x4_0000)).unwrap(),
    );
    let mut owned = OwnedMachine::new(MachineConfig::new(PlatformKind::Flat), image).unwrap();
    halted_by_dmi(&mut owned);
    let vpn = [(VA >> 12) & 0x1ff, (VA >> 21) & 0x1ff, (VA >> 30) & 0x1ff];
    let pte = |next: u64| ((next >> 12) << 10) | 1;
    let storage = owned.memory().clone();
    let mut storage = storage.lock().unwrap();
    storage
        .write_dword(ROOT - elf::BASE + vpn[2] * 8, pte(LVL1))
        .unwrap();
    storage
        .write_dword(LVL1 - elf::BASE + vpn[1] * 8, pte(LVL0))
        .unwrap();
    storage
        .write_dword(
            LVL0 - elf::BASE + vpn[0] * 8,
            ((DATA >> 12) << 10) | 0x46 | 1,
        )
        .unwrap();
    storage.write_dword(DATA - elf::BASE, 0x1122_3344).unwrap();
    drop(storage);

    let dpc = owned.dpc();
    {
        let state = owned.state_mut();
        state
            .csr
            .write(crate::csr::supervisor::SATP, (8 << 60) | (ROOT >> 12))
            .unwrap();
        state
            .csr
            .write(
                machine::MSTATUS,
                crate::csr::mstatus::MPRV | (1 << crate::csr::mstatus::MPP_SHIFT),
            )
            .unwrap();
        state.regs[1] = VA;
    }
    owned.admit().unwrap();

    postexec_buffer(&mut owned, [elf::ld(5, 1, 0), elf::nop()]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    assert_eq!(
        owned.state().regs[5],
        0x1122_3344,
        "the buffer read the Sv39 leaf, not the virtual address"
    );
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(owned.dpc(), dpc);
    assert_eq!(owned.state().privilege, crate::core::PrivilegeMode::Machine);
}

/// U3 §3: a committed debugger Platform effect survives a reset that arrives
/// in the same DMI boundary. Each call's facts are captured before a later
/// reset can replace the Platform, and the reset still applies.
#[test]
fn ndmreset_after_a_committed_debug_event_keeps_the_event_fact() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let htif = crate::executor::SYSTEM_BUS_HTIF_BASE;
    owned.dmi_write(DM_DATA0, 3).unwrap();
    owned.dmi_write(DM_DATA0 + 1, 0).unwrap();
    owned.dmi_write(DM_DATA0 + 2, htif as u32).unwrap();
    owned.dmi_write(DM_DATA0 + 3, (htif >> 32) as u32).unwrap();
    enqueue_dmi(&owned, DM_COMMAND, (2 << 24) | (3 << 20) | (1 << 16));
    enqueue_dmi(&owned, DM_DMCONTROL, DM_DMACTIVE | DM_NDMRESET | DM_HALTREQ);

    let facts = owned.debug_boundary(false).unwrap();
    assert_eq!(
        facts.events,
        [PlatformEvent::HtifWrite(3)],
        "the committed debugger effect was discarded by a later reset"
    );
    assert!(facts.failure.is_none());
    assert_eq!(owned.state().pc, elf::BASE);
    assert!(!owned.hart_in_debug_mode());
    assert_eq!(
        owned.dmi_read(DM_DMCONTROL).unwrap() & DM_NDMRESET,
        DM_NDMRESET
    );
}

/// U3 §3: held debug effects belong to their generation. An explicit fresh
/// image must not replay them; retention across a reset applies only inside a
/// debug boundary that commits them first.
#[test]
fn explicit_fresh_generation_does_not_replay_held_debug_events() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    abstract_write_dword(&mut owned, crate::executor::SYSTEM_BUS_HTIF_BASE, 3);
    owned.fresh_reset().unwrap();
    let facts = owned.debug_boundary(false).unwrap();
    assert!(
        facts.events.is_empty(),
        "a held effect survived a fresh generation"
    );
    assert!(facts.failure.is_none());
}

// ---- System Bus Access U3: Machine admission, lifecycle and scheduling ----

const DM_SBCS: u8 = 0x38;
const DM_SBADDRESS0: u8 = 0x39;
const DM_SBADDRESS1: u8 = 0x3a;
const DM_SBDATA0: u8 = 0x3c;
const DM_SBDATA1: u8 = 0x3d;

const SBBUSY: u32 = 1 << 21;
const SBBUSYERROR: u32 = 1 << 22;
const SBREADONADDR: u32 = 1 << 20;
const SBACCESS_SHIFT: u32 = 17;
const SBERROR_SHIFT: u32 = 12;
/// `sbaccess` 2: a four-byte access, which is also the `sbcs` reset value.
const SBACCESS_WORD: u32 = 2 << SBACCESS_SHIFT;

/// A RAM offset outside the placed image sections.
const SBA_OFFSET: u64 = 0x800;

/// Power up the DM as a debugger does before its first register access.
fn dm_active(owned: &mut OwnedMachine) {
    owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE).unwrap();
}

/// `sberror` as `sbcs` reports it.
fn sberror_of(owned: &mut OwnedMachine) -> u32 {
    (owned.dmi_read(DM_SBCS).unwrap() >> SBERROR_SHIFT) & 7
}

/// The physical address of one RAM offset in this Platform's address form.
fn sba_address(kind: PlatformKind, offset: u64) -> u64 {
    match kind {
        PlatformKind::Native => elf::BASE + offset,
        PlatformKind::Flat => offset,
    }
}

fn sba_set_address(owned: &mut OwnedMachine, address: u64) {
    owned.dmi_write(DM_SBADDRESS0, address as u32).unwrap();
    owned
        .dmi_write(DM_SBADDRESS1, (address >> 32) as u32)
        .unwrap();
}

/// One four-byte system bus write through `sbdata0`. Returns the resulting
/// `sbcs`.
fn sba_write_word(owned: &mut OwnedMachine, address: u64, value: u32) -> u32 {
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(owned, address);
    owned.dmi_write(DM_SBDATA0, value).unwrap();
    owned.dmi_read(DM_SBCS).unwrap()
}

/// One four-byte system bus read. `sbreadonaddr` triggers the access on the
/// `sbaddress0` write, so the following `sbdata0` read reports its value.
fn sba_read_word(owned: &mut OwnedMachine, address: u64) -> u32 {
    owned
        .dmi_write(DM_SBCS, SBACCESS_WORD | SBREADONADDR)
        .unwrap();
    owned
        .dmi_write(DM_SBADDRESS1, (address >> 32) as u32)
        .unwrap();
    owned.dmi_write(DM_SBADDRESS0, address as u32).unwrap();
    owned.dmi_read(DM_SBDATA0).unwrap()
}

/// `minstret`, `mcycle` and `time`, which SBA must never charge.
fn guest_counters(owned: &OwnedMachine) -> (u64, u64, u64) {
    (
        owned.state().csr.read(machine::MINSTRET).unwrap(),
        owned.state().csr.read(machine::MCYCLE).unwrap(),
        owned
            .state()
            .csr
            .read(crate::csr::unprivileged::TIME)
            .unwrap(),
    )
}

/// One inbound target that cannot prove whether the access committed. It
/// counts every target call, so a test can prove that no further access
/// reached it.
struct UnknownSba {
    calls: Arc<AtomicUsize>,
}
impl PhysicalBackend for UnknownSba {
    fn transact(&mut self, _: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(PhysicalBackendError::unknown(
            "SBA target may have committed the access",
        ))
    }
}

/// U3 §4 and D4: an access completes at a coherent Hart boundary whether the
/// Hart is running or halted, and the DM never holds `sbbusy` afterwards.
#[test]
fn sba_completes_while_the_hart_runs_and_while_halted() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let mut owned = OwnedMachine::new(MachineConfig::new(kind), image(elf::nop())).unwrap();
        dm_active(&mut owned);
        let address = sba_address(kind, SBA_OFFSET);
        let turn = owned.step(false).unwrap();
        assert!(
            matches!(turn.hart().outcome, StepOutcome::InstructionRetired(_)),
            "{kind:?}: the fixture Hart must be running here"
        );
        drop(turn);

        let sbcs = sba_write_word(&mut owned, address, 0xdead_beef);
        assert_eq!(
            sbcs & SBBUSY,
            0,
            "{kind:?}: a running-Hart access is still busy"
        );
        assert_eq!(sberror_of(&mut owned), 0, "{kind:?}");
        assert_eq!(
            storage_read(&owned, SBA_OFFSET) as u32,
            0xdead_beef,
            "{kind:?}: the committed write is not visible to the Hart's RAM"
        );
        assert_eq!(
            sba_read_word(&mut owned, address),
            0xdead_beef,
            "{kind:?}: the read did not return the committed value"
        );

        halted_by_dmi(&mut owned);
        let frozen = guest_counters(&owned);
        let sbcs = sba_write_word(&mut owned, address, 0x0bad_f00d);
        assert_eq!(sbcs & SBBUSY, 0, "{kind:?}: a halted access is still busy");
        assert_eq!(sberror_of(&mut owned), 0, "{kind:?}");
        assert_eq!(storage_read(&owned, SBA_OFFSET) as u32, 0x0bad_f00d);
        assert_eq!(sba_read_word(&mut owned, address), 0x0bad_f00d, "{kind:?}");
        assert_eq!(
            guest_counters(&owned),
            frozen,
            "{kind:?}: SBA charged the guest profile"
        );
    }
}

/// U3 §4 negative control: an access is admitted only at a Hart boundary.
/// A DMI transaction that arrives mid-instruction is refused, so no target
/// access happens outside a coherent boundary.
#[test]
fn sba_is_admitted_only_at_a_hart_boundary() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    dm_active(&mut owned);
    let address = sba_address(PlatformKind::Native, SBA_OFFSET);
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, address);
    owned.set_in_instruction_for_test(true);
    let refused = owned.dmi_write(DM_SBDATA0, 0x1111_2222);
    owned.set_in_instruction_for_test(false);
    assert!(
        matches!(refused, Err(MachineError::Debug(_))),
        "an SBA trigger was accepted mid-instruction: {refused:?}"
    );
    let sbcs = owned.dmi_read(DM_SBCS).unwrap();
    assert_eq!(sbcs & SBBUSY, 0, "the refused trigger started an access");
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(
        storage_read(&owned, SBA_OFFSET),
        0,
        "a refused trigger wrote"
    );

    // The same write at the next boundary completes normally.
    owned.dmi_write(DM_SBDATA0, 0x1111_2222).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);
    assert_eq!(storage_read(&owned, SBA_OFFSET) as u32, 0x1111_2222);
}

/// U3 §3 and the 2026-10-30 ADR-0002 §9 amendment: no DM lock is held while
/// the inbound target carries out the access.
#[test]
fn sba_target_access_holds_no_dm_lock() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let inner = Arc::clone(&owned.test_control().inner);
    let unlocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = unlocked.clone();
    owned.test_before_sba(Arc::new(move || {
        probe.store(inner.debug.dm.try_lock().is_ok(), Ordering::SeqCst);
    }));

    let sbcs = sba_write_word(
        &mut owned,
        sba_address(PlatformKind::Native, SBA_OFFSET),
        0x5a5a_1234,
    );
    assert!(
        unlocked.load(Ordering::SeqCst),
        "the DM lock was held across the target access"
    );
    assert_eq!(sbcs & SBBUSY, 0);
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(storage_read(&owned, SBA_OFFSET) as u32, 0x5a5a_1234);
}

/// U3 §3: an active program-buffer execution is not disturbed, and the staged
/// system bus access waits for it instead of failing or interleaving.
#[test]
fn sba_waits_for_an_active_program_buffer_command() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    postexec_buffer(&mut owned, [elf::addi(9, 9, 1), elf::addi(9, 9, 1)]);

    let sbcs = sba_write_word(
        &mut owned,
        sba_address(PlatformKind::Native, SBA_OFFSET),
        0x1234_5678,
    );
    assert_ne!(
        sbcs & SBBUSY,
        0,
        "the system bus access did not wait for the active command"
    );
    assert_eq!(
        sberror_of(&mut owned),
        0,
        "the access reported a false error"
    );

    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none(), "{facts:?}");
    assert_eq!(
        owned.state().regs[9],
        2,
        "the pending SBA disturbed the command"
    );
    assert_eq!(cmderr_of(&mut owned), 0, "the command reported an error");
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        0,
        "the deferred access never completed"
    );
    assert_eq!(storage_read(&owned, SBA_OFFSET) as u32, 0x1234_5678);
}

/// U3 §3: a sticky `sberror` is DM state only. It does not disturb an abstract
/// command, and it survives until W1C regardless of that command.
#[test]
fn sba_sticky_error_does_not_disturb_an_abstract_command() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let mut owned = OwnedMachine::new(MachineConfig::new(kind), image(elf::nop())).unwrap();
        halted_by_dmi(&mut owned);
        let ram_end = u64::try_from(owned.image().memory_size()).unwrap();
        let unmapped = sba_address(kind, ram_end);
        let sbcs = sba_write_word(&mut owned, unmapped, 1);
        assert_eq!(sberror_of(&mut owned), 2, "{kind:?}: no bad-address error");
        assert_eq!(sbcs & SBBUSY, 0, "{kind:?}: a failed access stayed busy");

        write_gpr(&mut owned, 3, 0x55);
        assert_eq!(cmderr_of(&mut owned), 0, "{kind:?}: the command failed");
        assert_eq!(owned.state().regs[3], 0x55, "{kind:?}");
        assert_eq!(
            sberror_of(&mut owned),
            2,
            "{kind:?}: the abstract command cleared a sticky SBA error"
        );

        // A staging write under the sticky error is stored without starting an
        // access; W1C then allows the next trigger to use it (§3.14).
        owned.dmi_write(DM_SBDATA0, 0x0f0f_0f0f).unwrap();
        assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);
        owned
            .dmi_write(DM_SBCS, SBACCESS_WORD | (2 << SBERROR_SHIFT))
            .unwrap();
        assert_eq!(sberror_of(&mut owned), 0, "{kind:?}: W1C did not clear");
        let good = sba_address(kind, SBA_OFFSET);
        sba_set_address(&mut owned, good);
        owned.dmi_write(DM_SBDATA0, 0x0f0f_0f0f).unwrap();
        assert_eq!(sberror_of(&mut owned), 0, "{kind:?}");
        assert_eq!(
            storage_read(&owned, SBA_OFFSET) as u32,
            0x0f0f_0f0f,
            "{kind:?}: the recovered access did not commit"
        );
    }
}

/// U3 §3 and D9: a reset request stops new grants and lets the started access
/// finish before lifecycle mutation.
#[test]
fn reset_drains_in_flight_sba_before_lifecycle_mutation() {
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    let control = owned.test_control();
    let generation = control.status().unwrap().generation;
    halted_by_dmi(&mut owned);
    let address = sba_address(PlatformKind::Native, SBA_OFFSET);
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, address);
    owned.test_before_sba(Arc::new(move || {
        entered_tx.send(()).unwrap();
        release_rx.lock().unwrap().recv().unwrap();
    }));

    let worker = thread::spawn(move || {
        let result = owned.dmi_write(DM_SBDATA0, 0xabcd_ef01);
        (owned, result)
    });
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("the SBA never reached its target");
    assert_eq!(
        control.status().unwrap().work.hart,
        1,
        "the in-flight access is not admitted work"
    );
    control.request_quiesce().unwrap();
    assert!(
        matches!(control.try_drain(), Err(MachineError::Busy)),
        "in-flight SBA work was acknowledged as drained"
    );
    assert!(matches!(control.fresh_reset(), Err(MachineError::Busy)));
    assert!(matches!(control.teardown(), Err(MachineError::Busy)));
    assert_eq!(control.status().unwrap().generation, generation);

    release_tx.send(()).unwrap();
    let (mut owned, result) = worker.join().unwrap();
    result.unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(
        storage_read(&owned, SBA_OFFSET) as u32,
        0xabcd_ef01,
        "the drained access did not commit"
    );

    // A quiesced Machine grants no new system bus access.
    sba_set_address(&mut owned, address);
    owned.dmi_write(DM_SBDATA0, 0x5).unwrap();
    assert_ne!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        0,
        "a reset request allowed a new SBA grant"
    );
    owned.quiesce().unwrap();
}

/// U3 §3 and D9: a sticky `sberror` survives a non-DM reset like other DM
/// error state, while DM reset clears the SBA command and busy state.
#[test]
fn ndmreset_keeps_sticky_sba_errors_and_dm_reset_clears_sba_state() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let ram_end = u64::try_from(owned.image().memory_size()).unwrap();
    sba_write_word(&mut owned, sba_address(PlatformKind::Native, ram_end), 1);
    assert_eq!(sberror_of(&mut owned), 2);
    owned.dmi_write(DM_SBDATA0, 0x1122_3344).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);

    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_NDMRESET | DM_HALTREQ)
        .unwrap();
    assert_eq!(
        sberror_of(&mut owned),
        2,
        "a non-DM reset cleared a sticky SBA error"
    );
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);

    owned.dmi_write(DM_DMCONTROL, 0).unwrap();
    owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE).unwrap();
    let sbcs = owned.dmi_read(DM_SBCS).unwrap();
    assert_eq!(sberror_of(&mut owned), 0, "DM reset kept the SBA error");
    assert_eq!(
        sbcs & (SBBUSY | SBBUSYERROR),
        0,
        "DM reset kept the SBA command state"
    );
    assert_eq!(owned.dmi_read(DM_SBADDRESS0).unwrap(), 0);
    assert_eq!(owned.dmi_read(DM_SBDATA0).unwrap(), 0);

    // A staged-but-ungranted request is DM command state too. Staged behind
    // an active command here, it is pending with `sbbusy` 1; a DM reset
    // clears it instead of granting it later. The DM reset above cleared the
    // halt request with the rest of the module, so halt the reloaded hart
    // again for the command.
    halted_by_dmi(&mut owned);
    postexec_buffer(&mut owned, [elf::addi(9, 9, 1), elf::addi(9, 9, 1)]);
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 0x2233_4455).unwrap();
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "the request was not staged behind the active command"
    );
    owned.dmi_write(DM_DMCONTROL, 0).unwrap();
    owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE).unwrap();
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & (SBBUSY | SBBUSYERROR),
        0,
        "DM reset kept a pending SBA request"
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(
        storage_read(&owned, SBA_OFFSET),
        0,
        "a pending request was granted across a DM reset"
    );
}

/// U3 §3: an unresolved completion keeps the quarantine of ADR-0002 §4. No
/// completion or `sberror` is invented, and W1C, DM reset and `ndmreset`
/// cannot clear it.
#[test]
fn sba_unknown_completion_resists_w1c_dm_reset_and_ndmreset() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let calls = Arc::new(AtomicUsize::new(0));
    owned.test_replace_inbound(Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        UnknownSba {
            calls: calls.clone(),
        },
    ))));
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 7).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1, "exactly one target access");

    let control = owned.test_control();
    let status = control.status().unwrap();
    assert!(
        status.uncertain && status.failed,
        "unknown completion did not quarantine the Machine: {status:?}"
    );
    let sbcs = owned.dmi_read(DM_SBCS).unwrap();
    assert_eq!(
        sbcs & SBBUSY,
        SBBUSY,
        "a completion was recorded without proof"
    );
    assert_eq!(
        sberror_of(&mut owned),
        0,
        "an sberror was fabricated for a simulator failure"
    );

    owned
        .dmi_write(DM_SBCS, SBBUSYERROR | (7 << SBERROR_SHIFT))
        .unwrap();
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "W1C cleared an unproven completion"
    );
    assert!(matches!(
        owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_NDMRESET | DM_HALTREQ),
        Err(MachineError::UnknownCompletion)
    ));
    // DM reset clears the DM's own SBA registers; it cannot clear the
    // quarantine, which is Machine state.
    owned.dmi_write(DM_DMCONTROL, 0).unwrap();
    owned.dmi_write(DM_DMCONTROL, DM_DMACTIVE).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);

    // A quarantined Machine grants no new system bus access either: the
    // re-staged transfer stays busy, commits nothing, and never reaches the
    // target again (ADR-0004 §12.2).
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 0x5566_7788).unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a quarantined Machine reached the target with a new access"
    );
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "a quarantined Machine granted a new system bus access"
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(storage_read(&owned, SBA_OFFSET), 0);
    assert!(matches!(
        control.resume(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        control.fresh_reset(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(matches!(
        control.teardown(),
        Err(MachineError::UnknownCompletion)
    ));
    assert!(control.status().unwrap().uncertain);
}

/// U3 §3 and ADR-0004: pending SBA is admissible work at a WFI wait. It is not
/// reported as WFI or NoProgress, and it adds no guest counter, clock or
/// application cycle charge.
#[test]
fn pending_sba_is_not_wfi_no_progress_and_adds_no_charge() {
    const WFI: u32 = 0x1050_0073;
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let mut owned = OwnedMachine::new(MachineConfig::new(kind), image(WFI)).unwrap();
        dm_active(&mut owned);
        // WFI retires on its own turn; the next turn reports the wait.
        let retired = owned.step(false).unwrap();
        assert!(matches!(
            retired.hart().outcome,
            StepOutcome::InstructionRetired(_)
        ));
        drop(retired);
        let turn = owned.step(false).unwrap();
        assert!(
            matches!(turn.hart().outcome, StepOutcome::Waiting),
            "{kind:?}: the fixture Hart must be waiting"
        );
        drop(turn);
        assert_eq!(
            owned.test_settle_wait(),
            WaitSettlement::NoProgress,
            "{kind:?}: the fixture must have no future platform event"
        );

        let frozen = guest_counters(&owned);
        let sbcs = sba_write_word(&mut owned, sba_address(kind, SBA_OFFSET), 0xfeed_face);
        assert_eq!(sbcs & SBBUSY, 0, "{kind:?}: the access did not complete");
        assert_eq!(sberror_of(&mut owned), 0, "{kind:?}");
        assert_eq!(storage_read(&owned, SBA_OFFSET) as u32, 0xfeed_face);
        assert_eq!(
            guest_counters(&owned),
            frozen,
            "{kind:?}: the SBA charged the guest profile or jumped time"
        );

        // The wait is unchanged: it is still NoProgress, not SBA progress.
        assert_eq!(
            owned.test_settle_wait(),
            WaitSettlement::NoProgress,
            "{kind:?}: pending SBA was mistaken for progress"
        );
        let again = owned.step(false).unwrap();
        assert!(matches!(again.hart().outcome, StepOutcome::Waiting));
        assert_eq!(guest_counters(&owned), frozen, "{kind:?}");
    }
}

/// U3 §3 and ADR-0004 §12.2: a grant that fails or cannot be proven complete
/// does not grant the system bus access deferred behind it. The request waits
/// out the recorded failure instead of running on a poisoned boundary.
#[test]
fn a_failing_grant_does_not_grant_the_sba_deferred_behind_it() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    // The buffer store fails on the probed data port; the deferred system bus
    // write uses the separate inbound port and would otherwise commit.
    let unlocked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let probe = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(DmLockProbe {
        inner: Arc::clone(&owned.test_control().inner),
        unlocked,
    })));
    owned.test_replace_data(probe);
    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 77);
    postexec_buffer(&mut owned, [elf::sd(6, 5, 0), elf::nop()]);

    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 0x3344_5566).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, SBBUSY);

    let facts = run_grants(&mut owned);
    assert!(
        facts.failure.is_some(),
        "the failing grant was not reported"
    );
    assert_eq!(cmderr_of(&mut owned), 7);
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "the failing boundary granted the deferred system bus access"
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(storage_read(&owned, SBA_OFFSET), 0);
}

/// U3 §3 and D9: a rising `ndmreset` rejects the system bus request staged
/// behind an active command instead of granting it during the reset. A
/// request already reaching its target finishes inside its own boundary.
#[test]
fn an_ndmreset_request_rejects_the_sba_deferred_behind_a_command() {
    let (accessed_tx, accessed_rx) = mpsc::channel();
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    owned.test_before_sba(Arc::new(move || {
        accessed_tx.send(()).unwrap();
    }));
    postexec_buffer(&mut owned, [elf::addi(9, 9, 1), elf::addi(9, 9, 1)]);
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 0x6655_4433).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, SBBUSY);

    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_NDMRESET | DM_HALTREQ)
        .unwrap();
    assert_eq!(
        cmderr_of(&mut owned),
        4,
        "the reset request did not abort the command"
    );
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        0,
        "the staged request was neither rejected nor completed"
    );
    assert!(
        accessed_rx.try_recv().is_err(),
        "the reset request granted the deferred system bus access"
    );
    assert_eq!(owned.state().pc, elf::BASE, "the reset did not reload");
    owned.quiesce().unwrap();
}

/// U3 §3 and D8: a committed SBA MMIO write is a Platform effect returned at
/// its debug boundary, with no guest counter or clock charge.
#[test]
fn sba_mmio_effect_is_a_platform_event_at_its_boundary() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let frozen = guest_counters(&owned);

    // The HTIF endpoint takes one complete eight-byte transfer.
    owned.dmi_write(DM_SBCS, 3 << SBACCESS_SHIFT).unwrap();
    sba_set_address(&mut owned, crate::executor::SYSTEM_BUS_HTIF_BASE);
    owned.dmi_write(DM_SBDATA1, 0).unwrap();
    owned.dmi_write(DM_SBDATA0, 3).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, 0);
    assert_eq!(sberror_of(&mut owned), 0);

    let facts = owned.debug_boundary(false).unwrap();
    assert_eq!(
        facts.events,
        [PlatformEvent::HtifWrite(3)],
        "the committed MMIO write was not a Platform effect at its boundary"
    );
    assert!(facts.failure.is_none());
    assert_eq!(
        guest_counters(&owned),
        frozen,
        "the MMIO write charged the guest profile"
    );
}

/// A system bus target that fails on the simulator side.
struct HostErrorSba;
impl PhysicalBackend for HostErrorSba {
    fn transact(&mut self, _: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        Err(PhysicalBackendError::host("injected SBA backend failure"))
    }
}

/// U3 §3 and D5: a system bus access that fails on the simulator side keeps
/// the existing non-guest classification. No `sberror` is fabricated, no
/// completion is recorded, and the failure is reported at a debug boundary.
#[test]
fn sba_simulator_failure_keeps_its_classification_and_is_reported() {
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    owned.test_replace_inbound(Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        HostErrorSba,
    ))));
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 9).unwrap();

    assert_eq!(
        sberror_of(&mut owned),
        0,
        "a host failure was fabricated as sberror"
    );
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "a completion was recorded for a failed access"
    );

    let facts = owned.debug_boundary(false).unwrap();
    assert!(
        facts.failure.is_some(),
        "the SBA failure was not reported at a boundary"
    );
    let status = owned.test_control().status().unwrap();
    assert!(
        status.failed && !status.uncertain,
        "a known host failure is not unknown completion: {status:?}"
    );
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        SBBUSY,
        "the failed access lost its busy state"
    );
}

/// U3 §3 and D9: a reset request that lands while a command holds a deferred
/// system bus request aborts the command and rejects the ungranted request.
/// Neither runs during or after the quiesce.
#[test]
fn quiesce_during_a_command_rejects_the_sba_deferred_behind_it() {
    let (accessed_tx, accessed_rx) = mpsc::channel();
    let mut owned =
        OwnedMachine::new(MachineConfig::new(PlatformKind::Native), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    owned.test_before_sba(Arc::new(move || {
        accessed_tx.send(()).unwrap();
    }));
    postexec_buffer(&mut owned, [elf::addi(9, 9, 1), elf::addi(9, 9, 1)]);
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    sba_set_address(&mut owned, sba_address(PlatformKind::Native, SBA_OFFSET));
    owned.dmi_write(DM_SBDATA0, 0x7788_9900).unwrap();
    assert_eq!(owned.dmi_read(DM_SBCS).unwrap() & SBBUSY, SBBUSY);

    owned.test_control().request_quiesce().unwrap();
    owned.debug_boundary(false).unwrap();
    assert_eq!(
        cmderr_of(&mut owned),
        4,
        "the quiesce did not abort the command"
    );
    assert!(!owned.debug_command_busy().unwrap());
    assert_eq!(
        owned.dmi_read(DM_SBCS).unwrap() & SBBUSY,
        0,
        "the deferred request was neither rejected nor completed"
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert!(
        accessed_rx.try_recv().is_err(),
        "the quiesced Machine granted the deferred system bus access"
    );
    assert_eq!(
        storage_read(&owned, SBA_OFFSET),
        0,
        "the deferred request committed"
    );
    assert!(
        owned.test_control().try_drain().is_ok(),
        "the aborted command held work open"
    );
}

// ---- Blocking TLM U1: selectable guest-RAM route ----

#[test]
fn the_default_transport_is_native() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let config = MachineConfig::new(kind);
        assert!(
            config.transport.is_native(),
            "{kind:?}: MachineConfig::new selected a non-native transport"
        );
        assert!(config.transport.target().is_none());
    }
    assert!(Transport::default().is_native());
}

#[test]
fn flat_with_tlm_is_a_configuration_error() {
    let mut config = MachineConfig::new(PlatformKind::Flat);
    config.transport = Transport::tlm(Arc::new(ZeroDelayRam::new()));
    let owner = Machine::new(config);
    assert!(matches!(
        owner.install(image(elf::nop())),
        Err(MachineError::UnsupportedTransport)
    ));
    assert!(matches!(
        owner.status(),
        Ok(status) if status.lifecycle == Lifecycle::DrainComplete
    ));
}

#[test]
fn program_buffer_and_sba_ram_access_use_the_tlm_route() {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::nop())).unwrap();

    halted_by_dmi(&mut owned);
    // Two program-buffer words store then load the same RAM word through the
    // Hart data port.
    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 0x1234_5678);
    postexec_buffer(&mut owned, [sw_word(6, 5, 0), lw_word(7, 5, 0)]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    assert_eq!(cmderr_of(&mut owned), 0);
    assert_eq!(owned.state().regs[7], 0x1234_5678);
    assert_eq!(
        target.bytes_at(elf::BASE + 0x200, 4),
        0x1234_5678u32.to_le_bytes(),
        "the program-buffer store did not reach the TLM RAM target"
    );
    let after_buffer = target.transactions.load(Ordering::SeqCst);
    assert!(
        after_buffer >= 2,
        "program-buffer RAM accesses bypassed the TLM route"
    );
    let calls = target.log();
    assert!(calls.iter().any(|call| {
        call.access_kind == PhysicalAccessKind::DataWrite
            && call.initiator == BlockingInitiator::Hart
    }));
    assert!(calls.iter().any(|call| {
        call.access_kind == PhysicalAccessKind::DataRead
            && call.initiator == BlockingInitiator::Hart
    }));

    // One Debug Module system bus write and read reach the same slot through
    // the inbound port.
    sba_write_word(
        &mut owned,
        sba_address(PlatformKind::Native, 0x200),
        0xdead_beef,
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(
        target.bytes_at(elf::BASE + 0x200, 4),
        0xdead_beefu32.to_le_bytes(),
        "the SBA write did not reach the TLM RAM target"
    );
    let after_write = target.transactions.load(Ordering::SeqCst);
    assert!(after_write > after_buffer);
    assert_eq!(
        sba_read_word(&mut owned, sba_address(PlatformKind::Native, 0x200)),
        0xdead_beef
    );
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(target.transactions.load(Ordering::SeqCst), after_write + 1);
    let calls = target.log();
    assert!(
        calls.iter().any(|call| {
            call.access_kind == PhysicalAccessKind::DataWrite
                && call.initiator == BlockingInitiator::DebugInbound
        }),
        "the SBA write lost its debug-inbound initiator"
    );
    assert!(
        calls.iter().any(|call| {
            call.access_kind == PhysicalAccessKind::DataRead
                && call.initiator == BlockingInitiator::DebugInbound
        }),
        "the SBA read lost its debug-inbound initiator"
    );
}

#[test]
fn tlm_route_zeroes_the_window_on_fresh_reset() {
    let target = Arc::new(ZeroDelayRam::new());
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let owner = Machine::new(config);
    let image = image(elf::nop());
    let size = image.memory_size();
    owner.install(image.clone()).unwrap();
    owner.resume().unwrap();
    owner.write_mem(0x300, &[0xaa]).unwrap();
    assert_eq!(target.bytes_at(elf::BASE + 0x300, 1), vec![0xaa]);
    owner.request_quiesce().unwrap();
    owner.try_drain().unwrap();
    owner.fresh_reset().unwrap();
    assert_eq!(
        target.bytes_at(elf::BASE + 0x300, 1),
        vec![0],
        "a reset did not zero-fill the RAM window"
    );

    // The whole reinstalled window matches a freshly built native Platform,
    // including the image, zero tails and padding.
    let native = Platform::build(&MachineConfig::new(PlatformKind::Native), &image).unwrap();
    assert_eq!(
        target.bytes_at(elf::BASE, size),
        native.ram.host_read(0, size).unwrap(),
        "the TLM reinstallation differs from a native fresh image"
    );
}

#[test]
fn debug_transport_failure_is_a_host_failure() {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    target.set_debug_short(true);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let error = Machine::new(config).install(image(elf::nop())).unwrap_err();
    assert!(
        matches!(&error, MachineError::Memory(MemoryError::Backend(_))),
        "an installation debug failure was not a host failure: {error:?}"
    );

    // A successful installation followed by a failing debug transport makes
    // polling and host views host failures, never guest-visible faults.
    target.set_debug_short(false);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let owner = Machine::new(config);
    owner.install(image(elf::nop())).unwrap();
    target.set_debug_short(true);
    let error = owner.read_mem(0, 4).unwrap_err();
    assert!(
        matches!(&error, MachineError::Memory(MemoryError::Backend(_))),
        "a host-view debug failure was not a host failure: {error:?}"
    );
    let inspection = owner.inspect().unwrap();
    assert!(
        matches!(&inspection.tohost.value, Err(MemoryError::Backend(_))),
        "a tohost poll debug failure was not a host failure: {:?}",
        inspection.tohost.value
    );
}

#[test]
fn installation_uses_debug_transport() {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let owner = Machine::new(config);
    owner.install(image(elf::nop())).unwrap();
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        0,
        "image installation used b_transport"
    );
    assert!(target.debug_requests.load(Ordering::SeqCst) > 0);
}

#[test]
fn tohost_polling_and_host_views_use_debug_transport() {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let owner = Machine::new(config);
    owner.install(image(elf::nop())).unwrap();
    let inspection = owner.inspect().unwrap();
    assert!(inspection.tohost.value.is_ok());
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        0,
        "tohost polling used b_transport"
    );
    let _ = owner.read_mem(0, 8).unwrap();
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        0,
        "a host view used b_transport"
    );
}

#[test]
fn unknown_completion_then_reset_reinstalls_an_intact_image() {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let image = image(elf::nop());
    let platform = Platform::build(&config, &image).unwrap();

    target.set_unknown(true);
    let request =
        PhysicalRequest::data_write(elf::BASE + 0x200, PhysicalWidth::Word, &[1, 2, 3, 4]).unwrap();
    let error = platform.data.lock().unwrap().access(request).unwrap_err();
    assert!(matches!(error, PhysicalAccessError::UnknownCompletion(_)));
    let transactions = target.transactions.load(Ordering::SeqCst);
    let request = PhysicalRequest::data_read(elf::BASE + 0x200, PhysicalWidth::Word).unwrap();
    assert!(matches!(
        platform.data.lock().unwrap().access(request),
        Err(PhysicalAccessError::UnknownCompletion(_))
    ));
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        transactions,
        "a poisoned transport issued another access"
    );

    // The target holds a write that it applies during the next reset. The
    // post-reset zero fill must erase it before installation.
    target.set_unknown(false);
    target.hold_write(elf::BASE + 0x300, vec![0xde, 0xad, 0xbe, 0xef]);
    let rebuilt = Platform::build(&config, &image).unwrap();
    assert!(rebuilt.ram.poison_reason().is_none());
    assert_eq!(
        target.bytes_at(elf::BASE + 0x300, 4),
        vec![0; 4],
        "a held write survived the reset zero fill"
    );
    let native = Platform::build(&MachineConfig::new(PlatformKind::Native), &image).unwrap();
    assert_eq!(
        rebuilt.ram.host_read(0, image.memory_size()).unwrap(),
        native.ram.host_read(0, image.memory_size()).unwrap(),
        "the reset did not reinstall the intact image"
    );
}

#[derive(Default)]
struct CountingTlmInterface {
    reads: AtomicUsize,
    writes: AtomicUsize,
}

impl crate::tlm::TlmInterface for CountingTlmInterface {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>, crate::tlm::TlmError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Err(crate::tlm::TlmError::NotImplemented)
    }

    fn write(&mut self, _: u64, _: &[u8]) -> Result<(), crate::tlm::TlmError> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        Err(crate::tlm::TlmError::NotImplemented)
    }

    fn get_delay(&self) -> crate::tlm::ScTime {
        crate::tlm::ScTime::zero()
    }

    fn set_delay(&mut self, _: crate::tlm::ScTime) {}
}

#[test]
fn the_legacy_tlm_interface_routes_no_access() {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let owner = Machine::new(config);
    owner.install(image(elf::sd(2, 1, 0))).unwrap();
    let interface = Arc::new(Mutex::new(CountingTlmInterface::default()));
    {
        let mut domain = owner.inner.installed.lock().unwrap();
        let installed = domain.as_mut().unwrap();
        installed.core.set_tlm_interface(interface.clone());
        installed.core.state_mut().regs[1] = elf::BASE + 0x200;
        installed.core.state_mut().regs[2] = 0x55;
    }
    owner.resume().unwrap();
    let turn = owner.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        interface.lock().unwrap().reads.load(Ordering::SeqCst),
        0,
        "the legacy tlm_interface routed a read"
    );
    assert_eq!(
        interface.lock().unwrap().writes.load(Ordering::SeqCst),
        0,
        "the legacy tlm_interface routed a write"
    );
    assert!(
        target.transactions.load(Ordering::SeqCst) > 0,
        "the store bypassed the selected TLM RAM route"
    );
    assert_eq!(target.bytes_at(elf::BASE + 0x200, 8)[0], 0x55);
}

// ---- Blocking TLM U3: transport delay and time ----

fn delay_target(declared_max: u64, delay: u64) -> Arc<ScriptTarget> {
    let target = Arc::new(ScriptTarget::new(elf::BASE, 0x1_0000));
    target.set_declared_max_delay(declared_max);
    target.set_delay(delay);
    target
}

fn guest_time(owned: &OwnedMachine) -> u64 {
    owned
        .state()
        .csr
        .read(crate::csr::unprivileged::TIME)
        .unwrap()
}

fn guest_counters_now(owned: &OwnedMachine) -> (u64, u64) {
    (
        owned.state().csr.read(machine::MINSTRET).unwrap(),
        owned.state().csr.read(machine::MCYCLE).unwrap(),
    )
}

/// Set the store's source register and address while the Machine is drained.
fn prepare_store(owned: &mut OwnedMachine, value: u64) {
    let state = owned.state_mut();
    state.regs[1] = elf::BASE + 0x200;
    state.regs[2] = value;
}

/// D7/ADR-0004 §5.1/§5.3: one completed turn advances `mtime` by its turn tick
/// plus the converted delay ticks, exactly once, and never into the ISA
/// counters.  The responses sit at the declared maximum in a multi-transaction
/// turn, and the Machine's bound uses the declared maximum per worst-case
/// transaction; `blocking::tests::a_guest_turn_may_use_the_declared_worst_case_transactions`
/// pins that declared count and its enforcement.
#[test]
fn delay_advances_mtime_by_turns_plus_ticks_without_touching_isa_counters() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    prepare_store(&mut owned, 0x1234_5678);
    let before = (guest_time(&owned), guest_counters_now(&owned));

    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    let transactions = target.transactions.load(Ordering::SeqCst) as u64;
    assert_eq!(
        transactions, 3,
        "two fetch parcels and one store transaction"
    );
    let (turns_before, counters_before) = before;
    assert_eq!(
        guest_time(&owned),
        turns_before + 1 + transactions,
        "mtime must be turns plus converted delay ticks"
    );
    assert_eq!(
        turn.boundary().transport_delay_femtoseconds,
        transactions * 1_000_000,
        "the boundary fact must report the exact charged delay"
    );
    let (minstret_before, mcycle_before) = counters_before;
    assert_eq!(
        guest_counters_now(&owned),
        (minstret_before + 1, mcycle_before + 1),
        "transport delay leaked into mcycle or minstret"
    );

    let bound = owned
        .turn_completion_bound_ticks()
        .expect("a TLM route has a completion bound");
    assert_eq!(
        bound,
        crate::blocking::TurnCompletionBound::new(
            crate::blocking::Femtoseconds::new(1_000_000),
            crate::blocking::TickPeriod::DEFAULT,
        )
        .unwrap()
        .ticks(),
        "the Machine's bound must add the declared maximum for every worst-case transaction"
    );
    assert!(
        transactions <= crate::blocking::MAX_GUEST_TRANSACTIONS_PER_TURN,
        "a multi-transaction turn exceeded the declared worst-case transaction count"
    );
    assert!(
        transactions <= bound,
        "a turn at the declared maximum did not fit its conservative completion bound"
    );
}

/// D7: a positive sub-tick delay is rounded up to one tick, never down to zero.
#[test]
fn a_positive_sub_tick_delay_rounds_up_to_one_tick() {
    let target = delay_target(1, 1);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    prepare_store(&mut owned, 0x1234_5678);

    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    let transactions = target.transactions.load(Ordering::SeqCst) as u64;
    assert_eq!(transactions, 3);
    assert_eq!(
        guest_time(&owned),
        1 + transactions,
        "a sub-tick delay disappeared instead of rounding up"
    );
    assert_eq!(
        turn.boundary().transport_delay_femtoseconds,
        transactions,
        "the exact sub-tick delay must survive in the boundary fact"
    );
}

/// D7 and the 2026-10-28/2026-10-30 ADR-0004 amendments: SBA and program-buffer
/// delay is validated and converted but never charged to Platform time.
#[test]
fn debug_initiated_transport_delay_is_not_charged_to_platform_time() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    halted_by_dmi(&mut owned);
    let (time_before, counters_before) = (guest_time(&owned), guest_counters_now(&owned));

    // The guest store's operands, written while halted.
    write_gpr(&mut owned, 1, elf::BASE + 0x200);
    write_gpr(&mut owned, 2, 0x1234_5678);
    // Two program-buffer words store and load the same RAM word.
    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 0x1234_5678);
    postexec_buffer(&mut owned, [sw_word(6, 5, 0), lw_word(7, 5, 0)]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    let after_buffer = target.transactions.load(Ordering::SeqCst);
    assert!(
        after_buffer >= 2,
        "program-buffer RAM accesses bypassed the TLM route"
    );
    assert_eq!(
        guest_time(&owned),
        time_before,
        "program-buffer transport delay advanced Platform time"
    );

    // One debug system bus write reaches the same TLM RAM slot.
    sba_write_word(
        &mut owned,
        sba_address(PlatformKind::Native, 0x200),
        0xdead_beef,
    );
    assert_eq!(sberror_of(&mut owned), 0);
    let after_sba = target.transactions.load(Ordering::SeqCst);
    assert!(after_sba > after_buffer);
    assert_eq!(
        guest_time(&owned),
        time_before,
        "SBA transport delay advanced Platform time"
    );
    assert_eq!(
        guest_counters_now(&owned),
        counters_before,
        "debug-initiated delay changed an ISA counter"
    );

    // The next guest turn charges only its own delay and advance.
    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    let mut guest = None;
    for _ in 0..3 {
        let turn = owned.step(false).unwrap();
        if matches!(turn.hart().outcome, StepOutcome::InstructionRetired(_)) {
            guest = Some(turn);
            break;
        }
    }
    let guest = guest.expect("the hart did not resume to a guest turn");
    let guest_transactions = target.transactions.load(Ordering::SeqCst) - after_sba;
    assert_eq!(
        guest_transactions, 3,
        "the guest turn issued its own two fetch parcels and store"
    );
    assert_eq!(
        guest_time(&owned),
        time_before + 1 + guest_transactions as u64,
        "debug delay leaked into the guest turn's mtime"
    );
    assert_eq!(
        guest.boundary().transport_delay_femtoseconds,
        guest_transactions as u64 * 1_000_000,
        "the boundary fact must exclude debug-initiated delay"
    );
}

/// ADR-0004 §5.3: a started failed turn consumes its known delay once and gets
/// no completed-turn tick or ISA counter delta.
#[test]
fn a_failed_turn_consumes_its_known_delay_without_a_turn_tick() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    prepare_store(&mut owned, 0x1234_5678);
    let before = (guest_time(&owned), guest_counters_now(&owned));
    target.set_status(crate::blocking::BlockingResponseStatus::GenericError);

    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::SimulatorFailure(_)
    ));
    assert_eq!(
        guest_time(&owned),
        before.0 + 1,
        "a failed turn must consume its known delay without a turn tick"
    );
    assert_eq!(
        guest_counters_now(&owned),
        before.1,
        "a failed turn changed an ISA counter"
    );
    assert_eq!(
        turn.boundary().transport_delay_femtoseconds,
        1_000_000,
        "the failed turn's known delay must reach the boundary fact"
    );
}

/// Native responses carry zero delay and keep the existing one-tick-per-turn
/// Platform time.
#[test]
fn the_native_route_reports_zero_transport_delay() {
    let mut owned = OwnedMachine::new(
        MachineConfig::new(PlatformKind::Native),
        image(elf::sd(2, 1, 0)),
    )
    .unwrap();
    prepare_store(&mut owned, 0x1234_5678);
    let before = guest_time(&owned);
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(guest_time(&owned), before + 1);
    assert_eq!(turn.boundary().transport_delay_femtoseconds, 0);
    assert!(owned.turn_completion_bound_ticks().is_none());
}

/// D7: transport delay is consumed into the same `mtime` as completed turns,
/// so a timer compare sees it at the next boundary.
#[test]
fn transport_delay_moves_the_timer_compare() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    {
        // Hart 0 mtimecmp is the dword at CLINT + 0x4000.
        let state = owned.state_mut();
        state.regs[1] = crate::executor::SYSTEM_BUS_CLINT_BASE + 0x4000;
        state.regs[2] = 3;
    }
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    // Two fetch parcels each carry one tick of delay: mtime = 1 + 2.
    assert_eq!(guest_time(&owned), 3);
    assert_ne!(
        owned.state().csr.mip() & (1 << 7),
        0,
        "the timer compare did not see the consumed transport delay"
    );
}

/// Regression for the U3 review: a known overshooting response must keep its
/// accounted time and stay a known simulator failure, not become an
/// unknown-completion quarantine that drops the delay.
#[test]
fn an_overshooting_delay_keeps_known_time_and_reports_a_failure() {
    let target = delay_target(0, 1);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    prepare_store(&mut owned, 0x1234_5678);

    let turn = owned.step(false).unwrap();
    let StepOutcome::SimulatorFailure(failure) = &turn.hart().outcome else {
        panic!(
            "an overshoot did not fail the turn: {:?}",
            turn.hart().outcome
        );
    };
    assert!(
        failure.message.contains("overshoot"),
        "the failure lost its overshoot detail: {failure:?}"
    );
    assert_eq!(
        target.transactions.load(Ordering::SeqCst),
        1,
        "the first fetch was the overshooting transaction"
    );
    assert_eq!(
        guest_time(&owned),
        1,
        "the known one-tick delay must be consumed once by the failed turn"
    );
    assert_eq!(
        turn.boundary().transport_delay_femtoseconds,
        1,
        "the failed turn's known delay must reach the boundary fact"
    );
    let status = owned.test_control().status().unwrap();
    assert!(status.failed);
    assert!(
        !status.uncertain,
        "a known overshoot became unknown completion"
    );
    assert!(
        status
            .last_failure
            .as_ref()
            .is_some_and(|failure| failure.message.contains("overshoot")),
        "the Machine lost the known failure diagnostic: {status:?}"
    );
}

/// Regression for the U3 review: a transport delay must never wrap `mtime`,
/// even when the converted amount is one tick.
#[test]
fn transport_delay_never_wraps_mtime() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    prepare_store(&mut owned, 0x1234_5678);
    owned.set_platform_mtime(u64::MAX).unwrap();

    let turn = owned.step(false).unwrap();
    let StepOutcome::SimulatorFailure(failure) = &turn.hart().outcome else {
        panic!(
            "an overflow did not fail the turn: {:?}",
            turn.hart().outcome
        );
    };
    assert!(
        failure.message.contains("overflow"),
        "the failure lost its overflow detail: {failure:?}"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        u64::MAX,
        "a transport delay wrapped Platform time"
    );
    assert!(owned.test_control().status().unwrap().failed);
}

/// Regression for the U3 re-review: a one-tick transport advance at the last
/// representable `mtime` must not wrap, and must not replace the transport
/// failure that produced the tick with a bare clock diagnostic.
#[test]
fn a_one_tick_transport_advance_preserves_the_original_failure() {
    let target = delay_target(0, 1);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    let mut owned = OwnedMachine::new(config, image(elf::sd(2, 1, 0))).unwrap();
    prepare_store(&mut owned, 0x1234_5678);
    owned.set_platform_mtime(u64::MAX).unwrap();

    let turn = owned.step(false).unwrap();
    let StepOutcome::SimulatorFailure(failure) = &turn.hart().outcome else {
        panic!(
            "the one-tick overflow did not fail the turn: {:?}",
            turn.hart().outcome
        );
    };
    assert!(
        failure.message.contains("overshoot"),
        "the original overshoot diagnostic was lost: {failure:?}"
    );
    assert!(
        failure.message.contains("overflow"),
        "the clock-overflow context was lost: {failure:?}"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        u64::MAX,
        "a one-tick transport advance wrapped Platform time"
    );
    let status = owned.test_control().status().unwrap();
    assert!(status.failed);
    assert!(!status.uncertain);
    let last = status
        .last_failure
        .as_ref()
        .expect("the Machine lost the known failure");
    assert!(
        last.message.contains("overshoot") && last.message.contains("overflow"),
        "the retained failure lost context: {last:?}"
    );
}

/// An atomic guest turn follows the same delay rules as a data access: its one
/// atomic transaction is charged to `mtime`, never to an ISA counter.
#[test]
fn atomic_delay_advances_mtime_like_a_data_access() {
    let target = DelayingRam::new(1_000_000, 1_000_000);
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    // amoadd.w x7, x2, (x1)
    let mut owned = OwnedMachine::new(config, image(0x0020_a3af)).unwrap();
    {
        let state = owned.state_mut();
        state.regs[1] = elf::BASE + 0x200;
        state.regs[2] = 5;
    }
    let before = (guest_time(&owned), guest_counters_now(&owned));

    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    let transactions = target.transactions.load(Ordering::SeqCst) as u64;
    assert_eq!(
        transactions, 3,
        "two fetch parcels and one atomic transaction"
    );
    assert_eq!(
        guest_time(&owned),
        before.0 + 1 + transactions,
        "the atomic delay did not advance mtime with the turn"
    );
    assert_eq!(
        turn.boundary().transport_delay_femtoseconds,
        transactions * 1_000_000,
        "the atomic delay was not reported in the boundary fact"
    );
    assert_eq!(
        owned.state().regs[7],
        0,
        "the AMO did not return the old value"
    );
    assert_eq!(
        target.bytes_at(elf::BASE + 0x200, 4),
        5u32.to_le_bytes(),
        "the atomic operation did not reach the delaying target"
    );
    let (minstret_before, mcycle_before) = before.1;
    assert_eq!(
        guest_counters_now(&owned),
        (minstret_before + 1, mcycle_before + 1),
        "the atomic delay leaked into mcycle or minstret"
    );
}

// ---- Direct memory U1: grants and read path ----

fn direct_mem_image(code: &[u32]) -> Arc<LoadImage> {
    Arc::new(LoadImage::parse(&elf::elf_with_code(code, 0, true, false, 0x4000)).unwrap())
}

fn direct_mem_config(target: &Arc<DirectMemTarget>, direct_mem: bool) -> MachineConfig {
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    config.dmi = direct_mem;
    config
}

#[test]
fn direct_memory_defaults_off_and_requires_the_tlm_route() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        let config = MachineConfig::new(kind);
        assert!(
            !config.dmi,
            "{kind:?}: MachineConfig::new selected direct memory"
        );
    }
    // Direct memory with the native route has no transport to grant it.
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.dmi = true;
    let owner = Machine::new(config);
    assert!(
        matches!(
            owner.install(image(elf::nop())),
            Err(MachineError::UnsupportedTransport)
        ),
        "direct memory with the native route was not a configuration error"
    );
}

#[test]
fn covered_fetches_and_loads_leave_b_transport_only_with_direct_memory() {
    let code = [elf::nop(), elf::ld(3, 1, 0)];
    let value = 0x1234_5678_9abc_def0u64;

    let run = |direct_mem: bool| {
        let target = DirectMemTarget::new(elf::BASE, 0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, value)
            .unwrap();
        owned.state_mut().regs[1] = elf::BASE + 0x200;
        let start = target.transactions();
        let turn = owned.step(false).unwrap();
        assert!(matches!(
            turn.hart().outcome,
            StepOutcome::InstructionRetired(_)
        ));
        // The returned boundary holds its admission until it is dropped.
        drop(turn);
        let after_first = target.transactions();
        let turn = owned.step(false).unwrap();
        assert!(matches!(
            turn.hart().outcome,
            StepOutcome::InstructionRetired(_)
        ));
        drop(turn);
        assert_eq!(
            owned.state().regs[3],
            value,
            "the load returned wrong bytes"
        );
        (
            after_first - start,
            target.transactions() - after_first,
            target.queries(),
        )
    };

    // Direct memory off: two fetch parcels per instruction and one load, all
    // blocking.
    assert_eq!(
        run(false),
        (2, 3, 0),
        "direct memory off changed the transactions"
    );
    // Direct memory on: the first parcel obtains the grant, everything after it
    // is direct.
    assert_eq!(
        run(true),
        (1, 0, 1),
        "a covered fetch or load took b_transport"
    );
}

/// direct-memory U3 (direct-memory D11): a system-bus read and a system-bus
/// write each stay one counted transaction while direct memory is active, and
/// both directions observe the other route's bytes: the guest's direct load
/// returns what the system bus wrote, and a host view returns what a direct
/// store wrote.
#[test]
fn sba_access_stays_counted_and_each_route_sees_the_other() {
    // nop, then load 0(x1) and store it to 8(x1).
    let code = [elf::nop(), elf::ld(3, 1, 0), elf::sd(3, 1, 8)];
    let target = DirectMemTarget::new(elf::BASE, 0x4000);
    // The store below needs the write half of the grant (direct-memory D5).
    target.set_reply(DirectMemReply::Chunk(BlockingDirectMemAccess::ReadWrite));
    let mut owned =
        OwnedMachine::new(direct_mem_config(&target, true), direct_mem_image(&code)).unwrap();
    let address = elf::BASE + 0x200;
    owned.state_mut().regs[1] = address;
    halted_by_dmi(&mut owned);

    // Each direction stays exactly one counted transaction.  A `DebugInbound`
    // request never takes the direct path, and it never queries either
    // (direct-memory D3/D4).
    let before = target.transactions();
    let sbcs = sba_write_word(&mut owned, address, 0x1122_3344);
    assert_eq!(sbcs & SBBUSY, 0, "the system bus write did not complete");
    assert_eq!(sberror_of(&mut owned), 0);
    assert_eq!(
        target.transactions(),
        before + 1,
        "an SBA write was not exactly one counted transaction"
    );
    let seen = sba_read_word(&mut owned, address);
    assert_eq!(
        seen, 0x1122_3344,
        "the system bus read did not see its write"
    );
    assert_eq!(
        target.transactions(),
        before + 2,
        "an SBA read was not exactly one counted transaction"
    );
    assert_eq!(
        target.queries(),
        0,
        "a debug-initiated access queried for a grant"
    );

    // The guest's load sees the host-written bytes through the direct path.
    // Only the run's first parcel (the cold fetch that installs the grant) is
    // allowed to transact, and it is the third transaction since the halt.
    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    let mut retired = 0;
    while retired < 2 {
        let turn = owned.step(false).unwrap();
        if matches!(turn.hart().outcome, StepOutcome::InstructionRetired(_)) {
            retired += 1;
        }
    }
    assert_eq!(
        owned.state().regs[3],
        0x1122_3344,
        "the direct load did not see the SBA write"
    );
    assert_eq!(
        target.transactions(),
        before + 3,
        "a covered load took b_transport"
    );

    // The covered store is direct too.
    let turn = owned.step(false).unwrap();
    assert!(
        matches!(turn.hart().outcome, StepOutcome::InstructionRetired(_)),
        "the store did not retire: {:?}",
        turn.hart().outcome
    );
    // The returned boundary holds its admission until it is dropped.
    drop(turn);
    assert_eq!(
        target.transactions(),
        before + 3,
        "a covered store took b_transport"
    );
    let stored = owned.state().regs[3];
    assert_eq!(
        target.bytes_at(address + 8, 8),
        stored.to_le_bytes().to_vec(),
        "the direct store did not reach the target"
    );

    // A host view reads the direct store back, and it is debug transport, so
    // it is not a transaction of its own (direct-memory D11).
    assert_eq!(
        storage_read(&owned, 0x208),
        stored,
        "the host view missed the direct store"
    );
    assert_eq!(
        target.transactions(),
        before + 3,
        "a host view used b_transport"
    );
}
#[test]
fn a_failed_recomposition_after_reset_leaves_the_old_composition_able_to_regrant() {
    let target = DirectMemTarget::new(elf::BASE, 0x4000);
    let code = [elf::nop(), elf::nop(), elf::nop()];
    let mut owned =
        OwnedMachine::new(direct_mem_config(&target, true), direct_mem_image(&code)).unwrap();
    let start = target.transactions();
    owned.step(false).unwrap();
    assert_eq!(target.transactions(), start + 1);
    assert_eq!(target.queries(), 1);
    let granted = target.transactions();
    owned.step(false).unwrap();
    assert_eq!(
        target.transactions(),
        granted,
        "the cached grant was not used"
    );

    // A recomposition builds a new composition, which resets the shared target
    // before this one is replaced, and then fails.
    target.set_debug_short(true);
    let error = owned.install(direct_mem_image(&code)).unwrap_err();
    assert!(
        matches!(&error, MachineError::Memory(MemoryError::Backend(_))),
        "the failed build reported {error:?}"
    );
    target.set_debug_short(false);
    // The failed build's registration is inert: only the still-current
    // composition's handle is live.
    assert_eq!(
        target.registrations(),
        1,
        "the failed build left a live or a replacing registration"
    );

    // The old composition is still current, and its grant is gone: restore a
    // decodable instruction through debug transport and step.
    for index in 0..8 {
        owned
            .memory()
            .lock()
            .unwrap()
            .write_word(index * 4, elf::nop())
            .unwrap();
    }
    let stale = target.transactions();
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    assert_eq!(
        target.transactions(),
        stale + 1,
        "the old composition used a grant the reset removed"
    );
    drop(turn);
    // That response carried the hint again, so the next turn is direct.
    let regranted = target.transactions();
    owned.step(false).unwrap();
    assert_eq!(
        target.transactions(),
        regranted,
        "the old composition could not obtain a new grant"
    );
}

#[test]
fn a_second_compositions_reset_on_a_shared_target_removes_the_first_regions() {
    let target = DirectMemTarget::new(elf::BASE, 0x4000);
    let config = direct_mem_config(&target, true);
    let image = image(elf::nop());
    let first = Platform::build(&config, &image).unwrap();
    assert_eq!(target.registrations(), 1);

    let read = || {
        let request = PhysicalRequest::data_read(elf::BASE, PhysicalWidth::Word).unwrap();
        first.ram.raw(&request, 0, BlockingInitiator::Hart, 0)
    };
    read().unwrap();
    assert_eq!(first.ram.cached_direct_mem_regions(), 1);
    let covered = target.transactions();
    read().unwrap();
    assert_eq!(
        target.transactions(),
        covered,
        "the first grant was not in use"
    );

    // The second composition's build resets the shared target while the first
    // composition is still current.
    let second = Platform::build(&config, &image).unwrap();
    assert_eq!(
        target.registrations(),
        2,
        "a registration replaced an earlier one"
    );
    assert_eq!(
        first.ram.cached_direct_mem_regions(),
        0,
        "the second composition's reset left the first composition's grant live"
    );
    let before = target.transactions();
    read().unwrap();
    assert_eq!(
        target.transactions(),
        before + 1,
        "the first composition served an access from a removed grant"
    );
    drop(second);
}

#[test]
fn a_reservation_retained_across_a_failed_build_matches_with_direct_memory_on_and_off() {
    let code = [lr_d(3, 1), sc_d(4, 0, 1)];
    let reserved = 0x1111_2222_3333_4444u64;
    let run = |direct_mem: bool| {
        let target = DirectMemTarget::new(elf::BASE, 0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, reserved)
            .unwrap();
        owned.state_mut().regs[1] = elf::BASE + 0x200;

        let turn = owned.step(false).unwrap();
        assert!(matches!(
            turn.hart().outcome,
            StepOutcome::InstructionRetired(_)
        ));
        drop(turn);
        assert_eq!(
            owned.state().regs[3],
            reserved,
            "the reservation loaded wrong bytes"
        );

        // The recomposition fails after the shared target was reset.
        target.set_debug_short(true);
        assert!(owned.install(direct_mem_image(&code)).is_err());
        target.set_debug_short(false);

        // Restore the code and the reserved word through debug transport.
        for (index, word) in code.iter().enumerate() {
            owned
                .memory()
                .lock()
                .unwrap()
                .write_word(index as u64 * 4, *word)
                .unwrap();
        }
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, reserved)
            .unwrap();
        owned.state_mut().regs[2] = 0xaaaa_bbbb_cccc_ddddu64;

        let turn = owned.step(false).unwrap();
        assert!(matches!(
            turn.hart().outcome,
            StepOutcome::InstructionRetired(_)
        ));
        owned.state().regs[4]
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with, without,
        "the store-conditional outcome diverged with direct memory on"
    );
    assert_eq!(
        with, 1,
        "a reservation retained across the reset should fail its store-conditional"
    );
}

#[test]
fn direct_read_latency_never_reaches_the_isa_counters() {
    let target = DirectMemTarget::new(elf::BASE, 0x4000);
    target.set_declared_max_delay(1_000_000);
    target.set_delay(1_000_000);
    target.set_grant_latency(1_000_000, 1_000_000);
    let code = [elf::nop(), elf::nop(), elf::nop()];
    let mut owned =
        OwnedMachine::new(direct_mem_config(&target, true), direct_mem_image(&code)).unwrap();

    // The first turn's second fetch parcel is direct.
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    drop(turn);
    let time_before = guest_time(&owned);
    let (minstret_before, mcycle_before) = guest_counters_now(&owned);

    // The second turn is two direct fetch parcels: two ticks of latency, one
    // turn tick, and nothing in the ISA counters.
    let transactions = target.transactions();
    let turn = owned.step(false).unwrap();
    assert!(matches!(
        turn.hart().outcome,
        StepOutcome::InstructionRetired(_)
    ));
    drop(turn);
    assert_eq!(
        target.transactions(),
        transactions,
        "the second turn took b_transport"
    );
    assert_eq!(
        guest_time(&owned),
        time_before + 1 + 2,
        "direct read latency did not advance mtime by its ticks"
    );
    assert_eq!(
        guest_counters_now(&owned),
        (minstret_before + 1, mcycle_before + 1),
        "direct read latency leaked into mcycle or minstret"
    );
}

// ---- Direct memory U2: write path and atomics ----

/// A direct-memory target that grants read-write chunks with zero latency.
fn direct_store_target(size: usize) -> Arc<DirectMemTarget> {
    let target = DirectMemTarget::new(elf::BASE, size);
    target.set_reply(DirectMemReply::Chunk(BlockingDirectMemAccess::ReadWrite));
    target
}

/// A direct-memory image with an explicit RAM window size.
fn direct_mem_image_of(code: &[u32], size: usize) -> Arc<LoadImage> {
    Arc::new(LoadImage::parse(&elf::elf_with_code(code, 0, true, false, size)).unwrap())
}

/// One guest turn that must retire an instruction.
fn retire(owned: &mut OwnedMachine) {
    let turn = owned.step(false).unwrap();
    assert!(
        matches!(turn.hart().outcome, StepOutcome::InstructionRetired(_)),
        "the turn did not retire an instruction: {:?}",
        turn.hart().outcome
    );
}

/// True when the target serviced a store at `address` after log index `from`.
///
/// This is the scheduler-independent way to observe that a store reached the
/// target instead of being copied through a direct-memory grant.
fn store_reached(target: &Arc<DirectMemTarget>, from: usize, address: u64) -> bool {
    target.log()[from..].iter().any(|call| {
        call.address == address && call.command == crate::blocking::BlockingCommand::Write
    })
}

fn storage_dword(owned: &OwnedMachine, offset: u64) -> u64 {
    owned.memory().lock().unwrap().read_dword(offset).unwrap()
}

fn storage_word(owned: &OwnedMachine, offset: u64) -> u32 {
    owned.memory().lock().unwrap().read_word(offset).unwrap()
}

/// U1's deferred acceptance item, in its full form: the guest takes its
/// reservation *after* a store that the direct path served, and keeps that
/// reservation across a recomposition that resets the shared target and then
/// fails.  The store-conditional outcome must match the same run with direct
/// memory off.
#[test]
fn a_reservation_taken_after_a_direct_store_and_retained_across_a_failed_build_matches_with_direct_memory_on_and_off(
) {
    // ld primes the read-write grant, sd is direct, lr.d takes the
    // reservation, and sc.d runs after the failed build.
    let code = [
        elf::ld(5, 1, 0),
        elf::sd(2, 1, 0),
        lr_d(3, 1),
        sc_d(4, 6, 1),
    ];
    let stored = 0x1111_2222_3333_4444u64;
    let run = |direct_mem: bool| -> (u64, u64) {
        let target = direct_store_target(0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, 0)
            .unwrap();
        {
            let state = owned.state_mut();
            state.regs[1] = elf::BASE + 0x200;
            state.regs[2] = stored;
            state.regs[6] = 0xaaaa_bbbb_cccc_ddddu64;
        }

        // The load is the first access, so it takes b_transport and its hint
        // obtains the read-write grant.
        retire(&mut owned);
        // The store is direct with direct memory on: no guest reservation
        // exists yet, so the write-suspension gate is closed.
        let log = target.log().len();
        retire(&mut owned);
        assert_eq!(
            store_reached(&target, log, elf::BASE + 0x200),
            !direct_mem,
            "the store before the reservation took the wrong path"
        );
        assert_eq!(storage_dword(&owned, 0x200), stored);

        // The reservation is taken after that store, so the LR-time snapshot
        // covers whatever version the store left.
        retire(&mut owned);
        assert_eq!(
            owned.state().regs[3],
            stored,
            "the reservation loaded wrong bytes"
        );

        // The recomposition fails after the shared target was reset.
        target.set_debug_short(true);
        assert!(owned.install(direct_mem_image(&code)).is_err());
        target.set_debug_short(false);

        // Restore the code and the data through debug transport.
        for (index, word) in code.iter().enumerate() {
            owned
                .memory()
                .lock()
                .unwrap()
                .write_word(index as u64 * 4, *word)
                .unwrap();
        }
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, stored)
            .unwrap();

        retire(&mut owned);
        (owned.state().regs[4], storage_dword(&owned, 0x200))
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with, without,
        "the store-conditional outcome after a direct store diverged with direct memory on"
    );
    assert_eq!(
        with.0, 1,
        "a reservation retained across the reset should fail its store-conditional"
    );
}

/// D7: a guest load-reserved suspends direct stores, so an overlapping store
/// still reaches the target and the store-conditional outcome matches native.
#[test]
fn a_store_overlapping_a_reservation_matches_with_direct_memory_on_and_off() {
    let code = [
        elf::ld(5, 1, 0),
        lr_d(3, 1),
        elf::sd(2, 1, 0),
        sc_d(4, 6, 1),
    ];
    let reserved = 0x1111_2222_3333_4444u64;
    let stored = 0xaaaa_bbbb_cccc_ddddu64;
    let run = |direct_mem: bool| -> (u64, u64, bool) {
        let target = direct_store_target(0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, reserved)
            .unwrap();
        {
            let state = owned.state_mut();
            state.regs[1] = elf::BASE + 0x200;
            state.regs[2] = stored;
            state.regs[6] = 0x5555_6666_7777_8888u64;
        }

        retire(&mut owned);
        retire(&mut owned);
        assert_eq!(
            owned.state().regs[3],
            reserved,
            "the reservation loaded wrong bytes"
        );

        // The store is direct-eligible but the gate is open, so it must reach
        // the target and commit a version to the reserved span.
        let log = target.log().len();
        retire(&mut owned);
        assert!(
            store_reached(&target, log, elf::BASE + 0x200),
            "a store after a guest load-reserved was served by the pointer"
        );
        assert_eq!(storage_dword(&owned, 0x200), stored);

        retire(&mut owned);
        (owned.state().regs[4], storage_dword(&owned, 0x200), true)
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with.0, 1,
        "a store the gate suspended must fail the store-conditional"
    );
    assert_eq!(
        with, without,
        "an overlapping store changed the store-conditional outcome with direct memory on"
    );
}

/// D7: the gate suspends every store, not only an overlapping one, and a
/// non-overlapping store still leaves the reservation able to commit.
#[test]
fn a_store_outside_a_reservation_matches_with_direct_memory_on_and_off() {
    let code = [
        elf::ld(5, 1, 0),
        lr_d(3, 1),
        elf::sd(2, 7, 0),
        sc_d(4, 6, 1),
    ];
    let reserved = 0x1111_2222_3333_4444u64;
    let stored = 0xaaaa_bbbb_cccc_ddddu64;
    let conditional = 0x5555_6666_7777_8888u64;
    let run = |direct_mem: bool| -> (u64, u64, u64) {
        let target = direct_store_target(0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, reserved)
            .unwrap();
        {
            let state = owned.state_mut();
            state.regs[1] = elf::BASE + 0x200;
            state.regs[2] = stored;
            state.regs[6] = conditional;
            state.regs[7] = elf::BASE + 0x300;
        }

        retire(&mut owned);
        retire(&mut owned);
        let log = target.log().len();
        retire(&mut owned);
        assert!(
            store_reached(&target, log, elf::BASE + 0x300),
            "a non-overlapping store was served by the pointer while the gate was open"
        );
        assert_eq!(storage_dword(&owned, 0x300), stored);
        retire(&mut owned);
        (
            owned.state().regs[4],
            storage_dword(&owned, 0x200),
            storage_dword(&owned, 0x300),
        )
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with.0, 0,
        "a store outside the reservation must not fail the store-conditional"
    );
    assert_eq!(
        with, without,
        "a non-overlapping store changed the store-conditional outcome with direct memory on"
    );
    assert_eq!(
        with.1, conditional,
        "the successful store-conditional did not write its payload to the reserved span"
    );
    assert_eq!(with.2, stored, "the non-overlapping store did not land");
}

/// D7: a gate left open by a load-reserved with no store-conditional is
/// correct and only slower.
#[test]
fn a_load_reserved_without_a_store_conditional_matches_with_direct_memory_on_and_off() {
    let code = [elf::ld(5, 1, 0), lr_d(3, 1), elf::sd(2, 7, 0)];
    let reserved = 0x1111_2222_3333_4444u64;
    let stored = 0xaaaa_bbbb_cccc_ddddu64;
    let run = |direct_mem: bool| -> (u64, u64) {
        let target = direct_store_target(0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, reserved)
            .unwrap();
        {
            let state = owned.state_mut();
            state.regs[1] = elf::BASE + 0x200;
            state.regs[2] = stored;
            state.regs[7] = elf::BASE + 0x300;
        }
        retire(&mut owned);
        retire(&mut owned);
        let log = target.log().len();
        retire(&mut owned);
        assert!(
            store_reached(&target, log, elf::BASE + 0x300),
            "a store after a load-reserved with no store-conditional was served by the pointer"
        );
        (owned.state().regs[3], storage_dword(&owned, 0x300))
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with, without,
        "a load-reserved with no store-conditional changed a later store with direct memory on"
    );
    assert_eq!(
        with.1, stored,
        "the store after the load-reserved was wrong"
    );
}

/// U2/U3 §4 step-9: the walk's A/D pair runs through the adapter as exactly
/// one load-reserved and one store-conditional, and (direct-memory D7) it
/// neither opens nor closes the write-suspension gate.
const PTE_V: u64 = 0x1;
const PTE_RW: u64 = 0x6;
const PTE_A: u64 = 0x40;
const PTE_D: u64 = 0x80;
// Storage offsets within the RAM window; the page table stores physical
// addresses, so the fixture adds `elf::BASE` where one is needed.
const SV39_ROOT: u64 = 0x3000;
const SV39_LEVEL1: u64 = 0x4000;
const SV39_LEVEL0: u64 = 0x5000;
const SV39_DATA: u64 = 0x6000;
const SV39_VA: u64 = 0x2000;
const SV39_LEAF_OFFSET: u64 = 0x5010;

/// A direct-memory image with room for the Sv39 fixtures.
fn sv39_image(code: &[u32]) -> Arc<LoadImage> {
    direct_mem_image_of(code, 0x8000)
}

/// Programs a three-level Sv39 mapping of `SV39_VA` to `SV39_DATA` with the
/// given leaf flags, through the Machine's own RAM.
fn install_sv39_leaf(owned: &OwnedMachine, flags: u64) {
    let vpns = [
        (SV39_VA >> 12) & 0x1ff,
        (SV39_VA >> 21) & 0x1ff,
        (SV39_VA >> 30) & 0x1ff,
    ];
    let pointer = |offset: u64| (((elf::BASE + offset) >> 12) << 10) | PTE_V;
    let mut memory = owned.memory().lock().unwrap();
    memory
        .write_dword(SV39_ROOT + vpns[2] * 8, pointer(SV39_LEVEL1))
        .unwrap();
    memory
        .write_dword(SV39_LEVEL1 + vpns[1] * 8, pointer(SV39_LEVEL0))
        .unwrap();
    memory
        .write_dword(
            SV39_LEVEL0 + vpns[0] * 8,
            (((elf::BASE + SV39_DATA) >> 12) << 10) | flags,
        )
        .unwrap();
    // The fixture must land exactly where the walk reads it.
    assert_eq!(
        memory.read_dword(SV39_ROOT + vpns[2] * 8).unwrap(),
        pointer(SV39_LEVEL1),
        "the root PTE did not land where the walk reads it"
    );
    assert_eq!(
        memory.read_dword(SV39_LEVEL1 + vpns[1] * 8).unwrap(),
        pointer(SV39_LEVEL0)
    );
    assert_eq!(
        memory.read_dword(SV39_LEVEL0 + vpns[0] * 8).unwrap(),
        (((elf::BASE + SV39_DATA) >> 12) << 10) | flags
    );
}

/// Switches the Hart to M-mode with `MPRV=1` and `MPP=S`, so a data access
/// walks the Sv39 table while instruction fetch stays untranslated.
fn enable_mprv_sv39(owned: &mut OwnedMachine) {
    let state = owned.state_mut();
    state
        .csr
        .write(
            crate::csr::supervisor::SATP,
            (8 << 60) | ((elf::BASE + SV39_ROOT) >> 12),
        )
        .unwrap();
    state
        .csr
        .write(
            machine::MSTATUS,
            crate::csr::mstatus::MPRV | (1 << crate::csr::mstatus::MPP_SHIFT),
        )
        .unwrap();
}

/// A load's walk sets A through one LR and one SC, and that pair must not open
/// the direct-store gate: the following untranslated store stays direct.
#[test]
fn a_translation_ad_update_does_not_open_the_write_suspension_gate() {
    let code = [elf::ld(5, 1, 0), elf::sd(2, 7, 0)];
    let run = |direct_mem: bool| -> (u64, bool) {
        let target = direct_store_target(0x8000);
        let mut owned =
            OwnedMachine::new(direct_mem_config(&target, direct_mem), sv39_image(&code)).unwrap();
        install_sv39_leaf(&owned, PTE_V | PTE_RW);
        {
            enable_mprv_sv39(&mut owned);
            owned.state_mut().regs[1] = SV39_VA;
        }

        // The walk sets A, then the load itself reaches the target and its
        // hint obtains the read-write grant over the data page.
        retire(&mut owned);
        assert_eq!(
            target.atomics(),
            2,
            "the A update was not one load-reserved and one store-conditional"
        );
        assert_eq!(owned.state().regs[5], 0);
        let leaf = storage_dword(&owned, SV39_LEAF_OFFSET);
        assert_eq!(leaf & PTE_A, PTE_A, "the walk did not set A");
        assert_eq!(leaf & PTE_D, 0, "a load set D");

        // Drop MPRV so the store is untranslated: only the gate can stop it.
        owned.state_mut().csr.write(machine::MSTATUS, 0).unwrap();
        {
            let state = owned.state_mut();
            state.regs[7] = elf::BASE + SV39_DATA + 8;
            state.regs[2] = 0xdead_beef;
        }
        let log = target.log().len();
        retire(&mut owned);
        assert_eq!(storage_dword(&owned, 0x6008), 0xdead_beef);
        (
            storage_dword(&owned, 0x6008),
            store_reached(&target, log, elf::BASE + SV39_DATA + 8),
        )
    };

    let with = run(true);
    assert!(
        !with.1,
        "an A/D load-reserved opened the write-suspension gate"
    );
    let without = run(false);
    assert!(without.1, "the store did not reach the target with dmi off");
    assert_eq!(with.0, without.0);
}

/// D7: a guest load-reserved followed by an A/D update, a store and a
/// store-conditional matches the same run with direct memory off.
#[test]
fn a_guest_lr_then_a_translation_ad_update_then_a_store_and_sc_matches() {
    let code = [
        elf::ld(5, 1, 0),
        lr_d(3, 1),
        elf::sd(2, 1, 0),
        sc_d(4, 6, 1),
    ];
    let reserved = 0x1111_2222_3333_4444u64;
    let stored = 0xaaaa_bbbb_cccc_ddddu64;
    let run = |direct_mem: bool| -> (u64, u64) {
        let target = direct_store_target(0x8000);
        let mut owned =
            OwnedMachine::new(direct_mem_config(&target, direct_mem), sv39_image(&code)).unwrap();
        // A is set and D is clear, so only the store's walk updates the leaf.
        install_sv39_leaf(&owned, PTE_V | PTE_RW | PTE_A);
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(SV39_DATA, reserved)
            .unwrap();
        {
            enable_mprv_sv39(&mut owned);
            let state = owned.state_mut();
            state.regs[1] = SV39_VA;
            state.regs[2] = stored;
            state.regs[6] = 0x5555_6666_7777_8888u64;
        }

        // The load primes the grant; the load-reserved then opens the gate.
        retire(&mut owned);
        let atomics = target.atomics();
        retire(&mut owned);
        assert_eq!(owned.state().regs[3], reserved);
        assert_eq!(
            target.atomics(),
            atomics + 1,
            "the load-reserved's own walk updated A/D"
        );

        // The store's walk sets D through one LR and one SC, and the store
        // itself must reach the target because the gate is open.
        let atomics = target.atomics();
        let log = target.log().len();
        retire(&mut owned);
        assert_eq!(
            target.atomics(),
            atomics + 2,
            "the D update was not one load-reserved and one store-conditional"
        );
        assert_eq!(
            storage_dword(&owned, SV39_LEAF_OFFSET) & PTE_D,
            PTE_D,
            "the walk did not set D"
        );
        assert!(
            store_reached(&target, log, elf::BASE + SV39_DATA),
            "the store after an A/D update was served by the pointer"
        );

        // The store committed a version to the reserved span, so the
        // reservation that outlived the A/D update fails.
        retire(&mut owned);
        (owned.state().regs[4], storage_dword(&owned, 0x6000))
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with.0, 1,
        "a store the gate suspended must fail the store-conditional"
    );
    assert_eq!(
        with, without,
        "a guest load-reserved followed by an A/D update diverged with direct memory on"
    );
}

/// `mret`.
fn mret() -> u32 {
    0x3020_0073
}

/// D7: a store-conditional the target faults leaves the gate open, because the
/// Hart keeps the reservation that authored it; a later store and
/// store-conditional match the same run with direct memory off.
#[test]
fn a_target_that_faults_a_store_conditional_leaves_the_gate_open() {
    // 0: ld primes the read-write grant over the reserved word.
    // 4: lr.d takes the reservation and opens the gate.
    // 8: sc.d is rejected by the target.
    // 12: a later ordinary store, after the reservation is consumed.
    // 20: the handler stores over the reserved word, then mret re-executes the
    //     faulting store-conditional.
    let code = [
        elf::ld(9, 1, 0),
        lr_d(3, 1),
        sc_d(4, 6, 1),
        elf::sd(5, 7, 0),
        elf::nop(),
        elf::sd(2, 1, 0),
        mret(),
    ];
    let handler = elf::BASE + 20;
    let reserved = 0x1111_2222_3333_4444u64;
    let handled = 0xaaaa_bbbb_cccc_ddddu64;
    let later = 0x9999_8888_7777_6666u64;
    let run = |direct_mem: bool| -> (u64, u64, u64) {
        let target = direct_store_target(0x4000);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        owned
            .memory()
            .lock()
            .unwrap()
            .write_dword(0x200, reserved)
            .unwrap();
        {
            let state = owned.state_mut();
            state.csr.write(machine::MTVEC, handler).unwrap();
            state.regs[1] = elf::BASE + 0x200;
            state.regs[2] = handled;
            state.regs[5] = later;
            state.regs[6] = 0x5555_6666_7777_8888u64;
            state.regs[7] = elf::BASE + 0x300;
        }

        retire(&mut owned);
        retire(&mut owned);
        assert_eq!(owned.state().regs[3], reserved);

        // The target rejects this store-conditional: a guest-visible fault, not
        // a poisoning completion.
        target.fault_next_store_conditional();
        let turn = owned.step(false).unwrap();
        assert!(
            matches!(turn.hart().outcome, StepOutcome::TrapEntered(_)),
            "the rejected store-conditional did not trap: {:?}",
            turn.hart().outcome
        );
        drop(turn);
        assert_eq!(owned.state().pc, handler);

        // The handler's store overlaps the retained reservation, and the gate
        // the faulted store-conditional did not close sends it to the target.
        let log = target.log().len();
        retire(&mut owned);
        assert!(
            store_reached(&target, log, elf::BASE + 0x200),
            "a store after a faulted store-conditional was served by the pointer"
        );
        assert_eq!(storage_dword(&owned, 0x200), handled);

        // The handler returns to the faulting instruction, which now completes
        // conclusively: it fails, consumes the reservation and closes the gate.
        retire(&mut owned);
        assert_eq!(owned.state().pc, elf::BASE + 8);
        retire(&mut owned);
        // A later ordinary store is direct-eligible again with direct memory on.
        retire(&mut owned);
        (
            owned.state().regs[4],
            storage_dword(&owned, 0x200),
            storage_dword(&owned, 0x300),
        )
    };

    let with = run(true);
    let without = run(false);
    assert_eq!(
        with.0, 1,
        "the re-executed store-conditional should fail after the handler's store"
    );
    assert_eq!(
        with, without,
        "a faulted store-conditional diverged with direct memory on"
    );
    assert_eq!(with.1, handled);
    assert_eq!(with.2, later);
}

/// D6/D7: a non-zero-latency target charges a direct store's grant latency
/// exactly as its annotated `b_transport` delay, so Platform time, `mtime` and
/// the boundary delay total match with direct memory on and off.
///
/// Only the data chunk is granted, so the instruction fetch always takes
/// `b_transport` and the store is the run's one direct access.  The granted read
/// latency is deliberately different from the write latency, so a store that
/// charged the wrong direction would leave the totals one tick short.
#[test]
fn direct_store_latency_matches_with_direct_memory_on_and_off() {
    const ANNOTATED: u64 = 2_000_000; // two ticks per transaction
    const READ_LATENCY: u64 = 1_000_000; // one tick
    let code = [elf::sd(2, 1, 0), elf::sd(2, 1, 8)];
    let run = |direct_mem: bool| -> (Vec<(u64, u64, u64, u64)>, bool) {
        let target = direct_store_target(0x4000);
        target.set_reply(DirectMemReply::ChunkWithin {
            first: elf::BASE + 0x1000,
            last: elf::BASE + 0x1fff,
            access: BlockingDirectMemAccess::ReadWrite,
        });
        target.set_declared_max_delay(ANNOTATED);
        target.set_delay(ANNOTATED);
        target.set_grant_latency(READ_LATENCY, ANNOTATED);
        let mut owned = OwnedMachine::new(
            direct_mem_config(&target, direct_mem),
            direct_mem_image(&code),
        )
        .unwrap();
        {
            let state = owned.state_mut();
            state.regs[1] = elf::BASE + 0x1100;
            state.regs[2] = 0x1234_5678_9abc_def0;
        }
        let mut facts = Vec::new();
        let mut store_direct = false;
        for turn_index in 0..2 {
            let log = target.log().len();
            let turn = owned.step(false).unwrap();
            assert!(matches!(
                turn.hart().outcome,
                StepOutcome::InstructionRetired(_)
            ));
            if turn_index == 1 {
                store_direct = !store_reached(&target, log, elf::BASE + 0x1108);
            }
            let (minstret, mcycle) = guest_counters_now(&owned);
            facts.push((
                guest_time(&owned),
                turn.boundary().transport_delay_femtoseconds,
                minstret,
                mcycle,
            ));
            drop(turn);
        }
        (facts, store_direct)
    };

    let (with, store_direct) = run(true);
    let (without, _) = run(false);
    assert!(
        store_direct,
        "the second store did not take the direct path"
    );
    assert_eq!(
        with, without,
        "direct store latency changed Platform time, the boundary total or the ISA counters"
    );
    // Each turn issues two fetch parcels and one store, and each access is
    // charged the annotated two ticks.  A store charged the granted read
    // latency would leave the totals one tick short.
    assert_eq!(with[0].1, 3 * ANNOTATED);
    assert_eq!(with[1].1, 6 * ANNOTATED);
}

/// D6: a direct store issued by debug work (program buffer) is validated but
/// never charged to Platform time or the ISA counters.
#[test]
fn debug_mode_store_latency_is_not_charged_with_direct_memory() {
    const DELAY: u64 = 1_000_000;
    let target = direct_store_target(0x4000);
    target.set_declared_max_delay(DELAY);
    target.set_delay(DELAY);
    target.set_grant_latency(DELAY, DELAY);
    let mut owned = OwnedMachine::new(direct_mem_config(&target, true), image(elf::nop())).unwrap();
    halted_by_dmi(&mut owned);
    let (time_before, counters_before) = (guest_time(&owned), guest_counters_now(&owned));

    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 0x1234_5678);
    postexec_buffer(&mut owned, [sw_word(6, 5, 0), elf::nop()]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    let primed = target.transactions();
    assert!(
        primed > 0,
        "the program-buffer store bypassed the TLM route"
    );
    assert!(
        target.queries() > 0,
        "the program-buffer store is not a Hart-initiated access"
    );

    // The second program-buffer store is direct: the grant covers it, and the
    // write-suspension gate is shut because no guest load-reserved is live.
    write_gpr(&mut owned, 5, elf::BASE + 0x204);
    postexec_buffer(&mut owned, [sw_word(6, 5, 0), elf::nop()]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    assert_eq!(
        target.transactions(),
        primed,
        "the second program-buffer store was not direct"
    );
    assert_eq!(storage_word(&owned, 0x204), 0x1234_5678);
    assert_eq!(
        guest_time(&owned),
        time_before,
        "debug store latency advanced Platform time"
    );
    assert_eq!(
        guest_counters_now(&owned),
        counters_before,
        "debug store latency changed an ISA counter"
    );
}

// ---- Quantum U1: local time and threshold (issue 201) ----
//
// The contract's U1 row (docs/dev-plan.md §4) drives every test here: D2–D5
// and D10, with the F3, F5, F6 and F10 forced syncs that this unit owns.

/// A hand-encoded WFI opcode for the F3 wait tests.
const WFI: u32 = 0x1050_0073;
/// The `mie` CSR number used by the F3 wait tests.
const MIE: u16 = 0x304;

/// `csrrs rd, csr, rs1` (RISC-V privileged CSR instruction encoding).
fn csrrs(rd: u8, csr: u16, rs1: u8) -> u32 {
    (u32::from(csr) << 20) | (u32::from(rs1) << 15) | (2 << 12) | (u32::from(rd) << 7) | 0x73
}

fn quantum_image(code: &[u32]) -> Arc<LoadImage> {
    Arc::new(LoadImage::parse(&elf::elf_with_code(code, 0, true, false, 0x4000)).unwrap())
}

fn quantum_config(target: &Arc<ScriptTarget>, quantum: u64) -> MachineConfig {
    let mut config = MachineConfig::new(PlatformKind::Native);
    config.transport = Transport::tlm(target.clone());
    config.quantum = quantum;
    config
}

/// `C` read through the shared owner's installed domain, as the contract's
/// U1 controls name their observables.
fn domain_mtime(owner: &Machine) -> u64 {
    owner
        .inner
        .installed
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .platform
        .read_mtime()
        .unwrap()
}

/// `L` read through the shared owner's installed domain.
fn domain_local_time(owner: &Machine) -> u64 {
    owner
        .inner
        .installed
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .platform
        .read_local_time()
        .unwrap()
}

/// `O` read through the shared owner's installed domain.
fn domain_local_offset(owner: &Machine) -> u64 {
    owner
        .inner
        .installed
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .platform
        .local_offset()
        .unwrap()
}

fn domain_syncs(owner: &Machine) -> u64 {
    owner
        .inner
        .installed
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .platform
        .quantum_syncs()
        .unwrap()
}

/// quantum D2: the selection defaults off, and a non-zero quantum requires the
/// TLM route and a representable `(Q + t + 1) x tick_period` bound.
#[test]
fn quantum_defaults_off_and_refuses_native_and_oversized_selections() {
    for kind in [PlatformKind::Native, PlatformKind::Flat] {
        assert_eq!(
            MachineConfig::new(kind).quantum,
            0,
            "{kind:?}: a fresh MachineConfig selected a quantum"
        );
    }
    // A non-zero quantum on the native transport is the widened transport error.
    let mut native = MachineConfig::new(PlatformKind::Native);
    native.quantum = 4;
    let owner = Machine::new(native);
    let error = owner
        .install(image(elf::nop()))
        .expect_err("a native quantum selection was accepted");
    assert!(
        matches!(error, MachineError::UnsupportedTransport),
        "the native quantum selection was not a transport error: {error:?}"
    );
    assert!(
        error.to_string().contains("quantum"),
        "the widened transport message does not name the quantum case: {error}"
    );
    // An oversized quantum cannot represent its declared maximum turn bound.
    let target = delay_target(1_000_000, 0);
    let owner = Machine::new(quantum_config(&target, u64::MAX - 100));
    let error = owner
        .install(image(elf::nop()))
        .expect_err("an unrepresentable quantum bound was accepted");
    assert!(
        matches!(error, MachineError::Placement(_)),
        "an unrepresentable quantum bound was not a placement error: {error:?}"
    );
    // The check does not run for Q = 0: the same oversized target is accepted.
    let oversized = delay_target(2_000_000_000_000_000_000, 0);
    let owner = Machine::new(quantum_config(&oversized, 0));
    owner
        .install(image(elf::nop()))
        .expect("Q = 0 ran the quantum bound check");
}

/// quantum D5/D7: a run with `Q` = 1 has the same committed time, presented
/// time, ISA counters and delay total as the base `Q` = 0 route, turn by turn;
/// every boundary syncs through the sync path and leaves the counters alone.
#[test]
fn quantum_zero_and_one_match_the_base_route_turn_by_turn() {
    // (committed time C, presented time L, minstret, mcycle, delay total fs)
    type Record = (u64, u64, u64, u64, u64);

    fn route(quantum: u64, target: &Arc<ScriptTarget>) -> Vec<Record> {
        let mut owned = OwnedMachine::new(
            quantum_config(target, quantum),
            quantum_image(&[elf::nop(), elf::sd(2, 1, 0), elf::nop()]),
        )
        .unwrap();
        prepare_store(&mut owned, 0x1234_5678);
        (0..3)
            .map(|_| {
                let turn = owned.step_request(false, false, false).unwrap();
                assert!(
                    !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
                    "the parity guest failed: {:?}",
                    turn.hart().outcome
                );
                let (minstret, mcycle) = guest_counters_now(&owned);
                (
                    owned.platform_mtime().unwrap(),
                    owned.platform_local_time().unwrap(),
                    minstret,
                    mcycle,
                    turn.boundary().transport_delay_femtoseconds,
                )
            })
            .collect()
    }

    let parity_target = delay_target(1_000_000, 1_000_000);
    let parity = route(0, &parity_target);
    let one_target = delay_target(1_000_000, 1_000_000);
    let one = route(1, &one_target);
    assert_eq!(
        parity, one,
        "Q = 1 diverged from the base route in committed time, time, counters or delay"
    );
    assert_eq!(
        parity[0].0, 3,
        "the delay target did not charge one tick per transaction"
    );

    // The Q = 1 route syncs at every boundary and never touches a counter.
    let mut owned = OwnedMachine::new(
        quantum_config(&delay_target(1_000_000, 1_000_000), 1),
        quantum_image(&[elf::nop(), elf::sd(2, 1, 0), elf::nop()]),
    )
    .unwrap();
    prepare_store(&mut owned, 0x1234_5678);
    for boundary in 1..=3u64 {
        let turn = owned.step_request(false, false, false).unwrap();
        let facts = turn
            .boundary()
            .quantum
            .expect("Q = 1 must report quantum facts");
        assert_eq!(facts.ticks, 1);
        assert_eq!(facts.interrupt_latency_bound_ticks, 0);
        assert!(!facts.reduced_accuracy);
        assert!(facts.reached, "Q = 1 must reach the threshold every turn");
        assert_eq!(
            facts.syncs, boundary,
            "the boundary sync was not counted once"
        );
        assert_eq!(owned.platform_local_offset().unwrap(), 0);
        assert_eq!(
            guest_counters_now(&owned).0,
            boundary,
            "a sync charged a counter"
        );
    }
}

/// quantum D4/D5/D10: the advance enters `O`, `O < Q` at every unsynced
/// boundary, and the threshold test syncs once `O >= Q`, reporting `reached`
/// from the test on `O` after the advance.
#[test]
fn quantum_offset_stays_below_the_threshold_until_a_boundary_syncs() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 4),
        quantum_image(&[elf::nop(), elf::sd(2, 1, 0), elf::nop()]),
    )
    .unwrap();
    prepare_store(&mut owned, 0x55);

    let mut local = 0u64;
    let mut committed = 0u64;
    let mut syncs = 0u64;
    for _ in 0..3 {
        let before = target.transactions.load(Ordering::SeqCst) as u64;
        let turn = owned.step_request(false, false, false).unwrap();
        let charged = target.transactions.load(Ordering::SeqCst) as u64 - before;
        local += 1 + charged;
        // `O` after the advance, before this boundary's own sync.
        let pending = local - committed;
        let facts = turn
            .boundary()
            .quantum
            .expect("Q = 4 must report quantum facts");
        assert_eq!(
            facts.reached,
            pending >= 4,
            "reached must be D5's test on O after the advance"
        );
        if facts.reached {
            committed += pending;
            syncs += 1;
        }
        assert_eq!(
            facts.syncs, syncs,
            "the D5 boundary sync was not counted once"
        );
        assert_eq!(owned.platform_mtime().unwrap(), committed);
        assert_eq!(
            owned.platform_local_offset().unwrap(),
            local - committed,
            "an unsynced boundary must keep O and a synced one must clear it"
        );
        assert!(
            owned.platform_local_offset().unwrap() < 4,
            "a boundary kept O >= Q"
        );
        assert_eq!(
            owned.platform_local_time().unwrap(),
            local,
            "local time must accumulate exactly one advance per turn"
        );
        assert_eq!(
            guest_time(&owned),
            local,
            "the turn-end publish must present L"
        );
    }
    assert_eq!(syncs, 1, "the threshold did not sync exactly once");
    assert_eq!(
        committed, 7,
        "the threshold sync committed the wrong amount"
    );
    assert_eq!(local, 10, "the parity local total is wrong");
}

/// quantum D4: with direct memory on and off, one turn's transport delay enters
/// the offset exactly once, so committed local time is the same as the base
/// route and no delay is converted twice by a sync.
#[test]
fn quantum_charges_one_transport_delay_exactly_once_with_direct_memory_on_and_off() {
    for direct_mem in [false, true] {
        let run = |quantum: u64| -> (u64, u64) {
            let target = DirectMemTarget::new(elf::BASE, 0x4000);
            target.set_declared_max_delay(1_000_000);
            target.set_delay(1_000_000);
            target.set_grant_latency(1_000_000, 1_000_000);
            let mut config = MachineConfig::new(PlatformKind::Native);
            config.transport = Transport::tlm(target.clone());
            config.dmi = direct_mem;
            config.quantum = quantum;
            let mut owned =
                OwnedMachine::new(config, direct_mem_image(&[elf::nop(), elf::sd(2, 1, 0)]))
                    .unwrap();
            prepare_store(&mut owned, 0x77);
            let mut delay_fs = 0;
            for _ in 0..2 {
                let turn = owned.step_request(false, false, false).unwrap();
                assert!(
                    !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
                    "dmi {direct_mem}: the guest failed: {:?}",
                    turn.hart().outcome
                );
                delay_fs = turn.boundary().transport_delay_femtoseconds;
            }
            (owned.platform_local_time().unwrap(), delay_fs)
        };
        let parity = run(0);
        let quantum = run(1024);
        assert_eq!(
            parity, quantum,
            "dmi {direct_mem}: the quantum offset converted a delay differently"
        );
        assert_eq!(
            quantum.0,
            2 + quantum.1 / 1_000_000,
            "dmi {direct_mem}: the delay was not charged exactly once"
        );
    }
}

/// quantum D4: a sync never reaches `mcycle` or `minstret`; the delay enters
/// committed time once and only once.
#[test]
fn quantum_syncs_do_not_touch_the_isa_counters() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1),
        quantum_image(&[elf::sd(2, 1, 0)]),
    )
    .unwrap();
    prepare_store(&mut owned, 0x99);
    let (minstret, mcycle) = guest_counters_now(&owned);
    let turn = owned.step_request(false, false, false).unwrap();
    let facts = turn
        .boundary()
        .quantum
        .expect("Q = 1 must report quantum facts");
    assert_eq!(facts.syncs, 1);
    assert!(facts.reached);
    assert_eq!(
        guest_counters_now(&owned),
        (minstret + 1, mcycle + 1),
        "a sync or a delay charged an ISA counter"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        4,
        "the synced delay did not enter committed time exactly once"
    );
}

/// quantum D4/F10: the overflow split at `u64::MAX` — a zero-delay turn wraps
/// and syncs the wrapped value like the parity run, and a delayed turn is
/// checked as today's simulator failure without moving committed time.
#[test]
fn quantum_overflow_splits_at_u64_max() {
    // Zero-delay: the wrapping rule, then the F10 turn-end sync.
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(quantum_config(&target, 1024), image(elf::nop())).unwrap();
    owned.set_platform_mtime(u64::MAX).unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(
        matches!(turn.hart().outcome, StepOutcome::InstructionRetired(_)),
        "the zero-delay wrap failed: {:?}",
        turn.hart().outcome
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        0,
        "the zero-delay wrap did not commit the wrapped local time"
    );
    assert_eq!(owned.platform_local_time().unwrap(), 0);
    assert_eq!(guest_time(&owned), 0);

    // Delayed: the checked rule, today's simulator failure, and neither C nor
    // O takes the refused advance.
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::sd(2, 1, 0)]),
    )
    .unwrap();
    prepare_store(&mut owned, 0x99);
    owned.set_platform_mtime(u64::MAX).unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    let StepOutcome::SimulatorFailure(failure) = &turn.hart().outcome else {
        panic!(
            "a delayed advance past u64::MAX was accepted: {:?}",
            turn.hart().outcome
        );
    };
    assert!(
        failure.message.contains("overflow"),
        "the delayed overflow lost its diagnostic: {failure:?}"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        u64::MAX,
        "a refused delayed advance moved committed time"
    );
    assert_eq!(
        owned.platform_local_offset().unwrap(),
        0,
        "a refused delayed advance entered the offset"
    );
}

/// quantum F3: with a non-zero-delay target, a WFI wake presents the same
/// time as the parity run, because the waiting turn syncs before the jump.
#[test]
fn quantum_wfi_wake_time_matches_the_parity_run() {
    // mtimecmp = 200 with MTIE set, then WFI; the wake turn retires the next
    // instruction.  mie is set without mstatus.MIE so no trap is taken.
    let code = [
        elf::lui(4, 0x2004),
        elf::addi(5, 0, 200),
        elf::sd(5, 4, 0),
        elf::addi(6, 0, 0x80),
        csrrs(0, MIE, 6),
        WFI,
        elf::nop(),
    ];
    type Record = (u64, u64, u64, u64);
    let route = |quantum: u64, target: &Arc<ScriptTarget>| -> Vec<Record> {
        let mut owned =
            OwnedMachine::new(quantum_config(target, quantum), quantum_image(&code)).unwrap();
        (0..7)
            .map(|_| {
                let turn = owned.step_request(false, false, false).unwrap();
                assert!(
                    !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
                    "the wait guest failed: {:?}",
                    turn.hart().outcome
                );
                let (minstret, mcycle) = guest_counters_now(&owned);
                (
                    owned.platform_local_time().unwrap(),
                    guest_time(&owned),
                    minstret,
                    mcycle,
                )
            })
            .collect()
    };
    let parity = route(0, &delay_target(1_000_000, 1_000_000));
    let quantum = route(1024, &delay_target(1_000_000, 1_000_000));
    assert_eq!(
        parity, quantum,
        "the wake presented a different time than the parity run"
    );
    assert_eq!(
        parity[6].0, 203,
        "the parity run did not land the wake on mtimecmp plus one turn"
    );
}

/// quantum F5: a single-step turn ends in a sync through both entry points, so
/// committed time equals local time after every `Machine::step` and every
/// `OwnedMachine::step`.
#[test]
fn quantum_single_step_syncs_the_offset() {
    let target = delay_target(1_000_000, 1_000_000);
    let owner = Machine::new(quantum_config(&target, 1024));
    owner.install(image(elf::sd(2, 1, 0))).unwrap();
    owner.resume().unwrap();
    for boundary in 1..=2u64 {
        let turn = owner.step(false).unwrap();
        assert!(!matches!(
            turn.hart().outcome,
            StepOutcome::SimulatorFailure(_)
        ));
        assert_eq!(
            domain_mtime(&owner),
            domain_local_time(&owner),
            "Machine::step left the offset unsynced"
        );
        assert_eq!(domain_local_offset(&owner), 0);
        assert_eq!(domain_syncs(&owner), boundary);
    }

    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::sd(2, 1, 0)]),
    )
    .unwrap();
    prepare_store(&mut owned, 0x11);
    for boundary in 1..=2u64 {
        owned.step(false).unwrap();
        assert_eq!(
            owned.platform_mtime().unwrap(),
            owned.platform_local_time().unwrap(),
            "OwnedMachine::step left the offset unsynced"
        );
        assert_eq!(owned.platform_local_offset().unwrap(), 0);
        assert_eq!(owned.platform_quantum_syncs().unwrap(), boundary);
    }
}

/// quantum F6: a failed turn syncs the offset it knows, including known delay,
/// without touching the transport, so committed time equals the D9 sum.
#[test]
fn quantum_failed_turn_syncs_its_known_delay() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        let turn = owned.step_request(false, false, false).unwrap();
        assert!(!turn.boundary().quantum.unwrap().reached);
    }
    let known = owned.platform_local_time().unwrap();
    assert_eq!(owned.platform_mtime().unwrap(), 0);
    let transactions = target.transactions.load(Ordering::SeqCst) as u64;
    assert_eq!(
        known,
        2 + transactions,
        "two delayed turns did not charge their turns plus transactions"
    );

    target.set_unknown(true);
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(
        matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
        "the unknown completion did not fail the turn: {:?}",
        turn.hart().outcome
    );
    assert!(turn.boundary().uncertain);
    assert_eq!(
        owned.platform_mtime().unwrap(),
        owned.platform_local_time().unwrap(),
        "a failed turn left its offset unsynced"
    );
    assert!(
        owned.platform_mtime().unwrap() >= known,
        "F6 lost the run's known offset"
    );
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
}

/// quantum D10: the boundary and control facts report the selected quantum,
/// the declared latency bound, the reduced-accuracy request and the sync count;
/// they are absent when the quantum is off.
#[test]
fn quantum_facts_report_the_selected_quantum() {
    // Off: no fact at all.
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(quantum_config(&target, 0), image(elf::nop())).unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(turn.boundary().quantum.is_none());
    assert!(owned.control_boundary(false).unwrap().quantum.is_none());
    assert!(owned.quantum_facts(false).unwrap().is_none());

    // Q = 1 is precise: zero bound, no reduced accuracy, reached every turn.
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(quantum_config(&target, 1), image(elf::nop())).unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    let facts = turn.boundary().quantum.expect("Q = 1 must report facts");
    assert_eq!(facts.ticks, 1);
    assert_eq!(facts.interrupt_latency_bound_ticks, 0);
    assert!(!facts.reduced_accuracy);
    assert!(facts.reached);
    assert_eq!(facts.syncs, 1);

    // Q >= 2: `Q - 1 + t` bound with `t` the route's turn completion bound.
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(quantum_config(&target, 4), image(elf::nop())).unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    let facts = turn.boundary().quantum.expect("Q = 4 must report facts");
    let bound = crate::blocking::MAX_GUEST_TRANSACTIONS_PER_TURN;
    assert_eq!(facts.ticks, 4);
    assert_eq!(facts.interrupt_latency_bound_ticks, 4 - 1 + bound);
    assert!(facts.reduced_accuracy);
    assert!(!facts.reached);
    assert_eq!(facts.syncs, 0);
    // A control boundary populates the same facts and reports `reached` false.
    let control = owned
        .control_boundary(false)
        .unwrap()
        .quantum
        .expect("Q = 4 must report control facts");
    assert_eq!(control.ticks, 4);
    assert_eq!(control.interrupt_latency_bound_ticks, 4 - 1 + bound);
    assert!(!control.reached);
    assert_eq!(control.syncs, 0);
    // The result report carries the same cumulative facts.
    let report = owned
        .quantum_facts(false)
        .unwrap()
        .expect("Q = 4 must report result facts");
    assert_eq!(report.ticks, 4);
    assert!(report.reduced_accuracy);
}

/// quantum D4/F6: a turn that fails only the clock check still syncs the offset
/// it already knows, so committed time equals local time and the D9 sum holds.
#[test]
fn quantum_d4_clock_failure_syncs_the_pre_existing_offset() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::sd(2, 1, 0)]),
    )
    .unwrap();
    prepare_store(&mut owned, 0x42);
    for _ in 0..2 {
        let turn = owned.step_request(false, false, false).unwrap();
        assert!(!turn.boundary().quantum.unwrap().reached);
    }
    let known = owned.platform_local_time().unwrap();
    let transactions = target.transactions.load(Ordering::SeqCst) as u64;
    assert_eq!(known, 2 + transactions);
    // A committed time near u64::MAX makes the delayed store's checked advance
    // fail, but the two turns' offset is still the run's known sum.
    owned.set_platform_mtime(u64::MAX - 6).unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    let StepOutcome::SimulatorFailure(failure) = &turn.hart().outcome else {
        panic!(
            "the delayed clock check did not fail: {:?}",
            turn.hart().outcome
        );
    };
    assert!(
        failure.message.contains("overflow"),
        "the clock failure lost its diagnostic: {failure:?}"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        u64::MAX,
        "F6 did not commit the known offset after the clock check failed"
    );
    assert_eq!(owned.platform_local_time().unwrap(), u64::MAX);
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    let facts = turn.boundary().quantum.expect("Q = 1024 must report facts");
    assert_eq!(
        facts.syncs, 1,
        "the clock-failure boundary sync was not counted once"
    );
    assert!(!facts.reached);
}

/// quantum D3/D5: a waiting boundary counts one sync even though F3's sync and
/// the wake turn's threshold sync both move committed time, and D5's test keeps
/// `reached` true when that forced sync coincides.
#[test]
fn quantum_wait_boundary_counts_one_sync_and_keeps_reached() {
    // MTIE, mtimecmp = 100, a store that ends the turn before WFI in a sync,
    // and a RAM store after WFI as the wake turn.
    let code = [
        elf::addi(6, 0, 0x80),
        csrrs(0, MIE, 6),
        elf::lui(4, 0x2004),
        elf::addi(5, 0, 100),
        elf::sd(5, 4, 0),
        elf::lui(1, 0x40000),
        elf::slli(1, 1, 1),
        elf::addi(2, 0, 0x55),
        elf::sd(2, 1, 0),
        WFI,
        elf::sd(2, 1, 0),
    ];
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned = OwnedMachine::new(quantum_config(&target, 4), quantum_image(&code)).unwrap();
    let mut local = 0u64;
    let mut committed = 0u64;
    let mut syncs = 0u64;
    for index in 1..=10usize {
        let before = target.transactions.load(Ordering::SeqCst) as u64;
        let turn = owned.step_request(false, false, false).unwrap();
        local += 1 + (target.transactions.load(Ordering::SeqCst) as u64 - before);
        // `O` after the advance, before this boundary's own sync.
        let pending = local - committed;
        let facts = turn.boundary().quantum.expect("Q = 4 must report facts");
        assert_eq!(
            facts.reached,
            pending >= 4,
            "D5's threshold result must survive the boundary's own sync"
        );
        // Turn 5 stores `mtimecmp` through the CLINT, so F1 marks it and that
        // boundary syncs even though D5's threshold was not reached.
        if facts.reached || index == 5 {
            committed = local;
            syncs += 1;
        }
        assert_eq!(facts.syncs, syncs);
        assert_eq!(owned.platform_mtime().unwrap(), committed);
    }
    // The waiting exchange: F3 commits the pre-jump offset, the jump lands on
    // mtimecmp, and the wake store's advance re-reaches the threshold.
    let before = target.transactions.load(Ordering::SeqCst) as u64;
    let turn = owned.step_request(false, false, false).unwrap();
    let wake_advance = 1 + (target.transactions.load(Ordering::SeqCst) as u64 - before);
    let facts = turn.boundary().quantum.expect("Q = 4 must report facts");
    assert!(
        facts.reached,
        "a coincident forced sync cleared the threshold result"
    );
    assert_eq!(
        facts.syncs,
        syncs + 1,
        "one waiting boundary counted more than one sync"
    );
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    assert_eq!(owned.platform_mtime().unwrap(), 100 + wake_advance);
    assert_eq!(
        owned.platform_local_time().unwrap(),
        owned.platform_mtime().unwrap()
    );
}

/// `andi rd, rs1, imm` (RISC-V I-type logical immediate).
fn andi(rd: u8, rs1: u8, imm: i32) -> u32 {
    (((imm as u32) & 0xfff) << 20)
        | (u32::from(rs1) << 15)
        | (7 << 12)
        | (u32::from(rd) << 7)
        | 0x13
}

/// `jal x0, 0`: a self-loop, so a recognized guest can never run past its code.
const SPIN: u32 = 0x0000_006f;

/// The CLINT device window's base, as a physical address.
const CLINT_BASE_ADDRESS: u64 = 0x0200_0000;

/// The `mip` machine-timer bit.
const MTIP: u64 = 1 << 7;

fn mtip_presented(owned: &OwnedMachine) -> bool {
    owned.state().csr.read(machine::MIP).unwrap() & MTIP != 0
}

fn clint_device_offset(offset: u64) -> u64 {
    CLINT_BASE_ADDRESS + offset
}

/// quantum F1/D8: a CLINT `mtime` load in the middle of a quantum returns the
/// parity value, because the Hart device access flushes the offset under the
/// bus lock before the device sees it. The marked turn then ends in a counted
/// sync, so the boundary carries `C` = `L`.
#[test]
fn quantum_f1_clint_mtime_read_mid_quantum_returns_the_parity_value() {
    // x4 = 0x0200_bff8 (the CLINT mtime dword), then ld x5, 0(x4).
    let code = [
        elf::lui(4, 0x200c),
        elf::addi(4, 4, -8),
        elf::nop(),
        elf::ld(5, 4, 0),
    ];
    // (loaded mtime, committed time, local time, counted syncs) after the F1
    // turn.
    fn route(quantum: u64, code: &[u32]) -> (u64, u64, u64, u64) {
        let target = delay_target(0, 0);
        let mut owned =
            OwnedMachine::new(quantum_config(&target, quantum), quantum_image(code)).unwrap();
        let mut record = (0, 0, 0, 0);
        for turn in 1..=4u64 {
            let boundary = owned.step_request(false, false, false).unwrap();
            assert!(
                !matches!(boundary.hart().outcome, StepOutcome::SimulatorFailure(_)),
                "the CLINT load guest failed: {:?}",
                boundary.hart().outcome
            );
            if turn == 4 {
                record = (
                    owned.state().regs[5],
                    owned.platform_mtime().unwrap(),
                    owned.platform_local_time().unwrap(),
                    boundary.boundary().quantum.map_or(0, |facts| facts.syncs),
                );
            }
        }
        record
    }
    let parity = route(0, &code);
    let quantum = route(1024, &code);
    assert_eq!(
        quantum.0, parity.0,
        "the CLINT mtime load did not return the parity value"
    );
    assert_eq!(
        quantum.1, quantum.2,
        "the marked turn did not sync committed time to local time"
    );
    assert_eq!(
        (quantum.1, quantum.2, quantum.3),
        (parity.1, parity.2, 1),
        "the marked boundary did not count exactly one sync"
    );
}

/// quantum F1/D8: a store of `mtimecmp` equal to `C` plus that turn's `advance`
/// presents MTIP at the next turn, because the marked turn ends in a sync that
/// moves `C` to `L`. A compare at or below `C` cannot expose the mark.
#[test]
fn quantum_f1_mtimecmp_store_presents_mtip_at_the_next_turn() {
    // mtimecmp = 0x0200_4000; the preloaded x6 holds the compare.
    let code = [
        elf::nop(),
        elf::nop(),
        elf::nop(),
        elf::lui(4, 0x2004),
        elf::sd(6, 4, 0),
        elf::nop(),
    ];
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(quantum_config(&target, 8), quantum_image(&code)).unwrap();
    for _ in 0..4 {
        owned.step_request(false, false, false).unwrap();
    }
    // This boundary is the store turn's start: `L` = 4, and the DMI write below
    // syncs (F4), so `C` = 4 as well. One completed turn adds exactly one tick.
    let compare = owned.platform_local_time().unwrap() + 1;
    write_gpr(&mut owned, 6, compare);
    assert_eq!(owned.platform_mtime().unwrap(), 4);
    owned.step_request(false, false, false).unwrap();
    assert_eq!(
        owned.platform_mtime().unwrap(),
        compare,
        "the marked turn did not commit the compare's own local time"
    );
    assert_eq!(owned.platform_local_time().unwrap(), compare);
    owned.step_request(false, false, false).unwrap();
    assert!(
        mtip_presented(&owned),
        "MTIP was not presented at the turn after the marked sync"
    );
}

/// quantum F1: an atomic envelope routed to a device window flushes the offset
/// under the bus lock, so `platform_mtime` equals `L` at the boundary, both for
/// an HTIF AMO that commits and for a CLINT AMO the device rejects.
#[test]
fn quantum_f1_atomic_device_envelope_flushes_the_offset() {
    let code = [elf::nop(), elf::nop(), amoadd_d(7, 6, 4)];
    // (committed time, local time, AMO old value, trapped)
    fn route(address: u64, code: &[u32]) -> (u64, u64, u64, bool) {
        let target = delay_target(0, 0);
        let mut owned =
            OwnedMachine::new(quantum_config(&target, 1024), quantum_image(code)).unwrap();
        {
            let state = owned.state_mut();
            state.regs[4] = address;
            state.regs[6] = 1;
        }
        let mut turn = owned.step_request(false, false, false).unwrap();
        for _ in 0..2 {
            assert!(
                !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
                "the AMO guest failed: {:?}",
                turn.hart().outcome
            );
            drop(turn);
            turn = owned.step_request(false, false, false).unwrap();
        }
        assert!(
            !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
            "the AMO guest failed: {:?}",
            turn.hart().outcome
        );
        (
            owned.platform_mtime().unwrap(),
            owned.platform_local_time().unwrap(),
            owned.state().regs[7],
            matches!(turn.hart().outcome, StepOutcome::TrapEntered(_)),
        )
    }
    let htif = route(crate::executor::SYSTEM_BUS_HTIF_BASE, &code);
    assert_eq!(
        (htif.0, htif.1),
        (3, 3),
        "the HTIF AMO did not flush the offset at the boundary"
    );
    assert_eq!(htif.2, 0, "the HTIF endpoint did not return its zero read");
    assert!(!htif.3, "the HTIF AMO did not complete its turn");
    let clint = route(
        clint_device_offset(crate::peripherals::clint::reg_offset::MTIMECMP_BASE),
        &code,
    );
    assert_eq!(
        (clint.0, clint.1),
        (3, 3),
        "the rejected CLINT AMO did not flush the offset at the boundary"
    );
    assert!(clint.3, "the rejected CLINT AMO did not fault");
}

/// quantum F2/D10: a non-zero RAM `tohost` sample syncs before the exit rule
/// decodes it and refreshes the boundary's count; a zero poll is not a sync.
#[test]
fn quantum_f2_tohost_sync_refreshes_the_boundary() {
    let code = [elf::nop(), elf::nop(), elf::sd(2, 1, 0)];
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(quantum_config(&target, 1024), quantum_image(&code)).unwrap();
    {
        let state = owned.state_mut();
        state.regs[1] = elf::TOHOST;
        // Not an exit command: the shared exit rule decodes nothing.
        state.regs[2] = 0x1234;
    }
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let mut turn = owned.step_request(false, false, false).unwrap();
    assert!(
        !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
        "the tohost guest failed: {:?}",
        turn.hart().outcome
    );
    let local = owned.platform_local_time().unwrap();
    assert_eq!(turn.boundary().quantum.unwrap().syncs, 0);
    owned.observe_tohost(&mut turn, elf::TOHOST);
    owned.sync_after_tohost(&mut turn).unwrap();
    assert_eq!(
        owned.platform_mtime().unwrap(),
        local,
        "F2 did not sync before the exit decode"
    );
    assert_eq!(
        turn.boundary().quantum.unwrap().syncs,
        1,
        "the F2 boundary did not count its sync"
    );
    // A zero poll is not a sync.
    storage_write(&owned, elf::TOHOST_SEGMENT_OFFSET, 0);
    owned.observe_tohost(&mut turn, elf::TOHOST);
    owned.sync_after_tohost(&mut turn).unwrap();
    assert_eq!(
        turn.boundary().quantum.unwrap().syncs,
        1,
        "a zero tohost poll counted a sync"
    );
}

/// quantum F4/D10: an in-crate `step_request` enters a debug halt at a
/// non-zero offset below the threshold, so neither the threshold nor F5 nor F7
/// masks the halt's own sync. `E`, `C` and the presented `time` all equal `L`.
#[test]
fn quantum_f4_halt_syncs_the_offset() {
    // EBREAK with dcsr.ebreakm set halts the Hart.
    let code = [elf::nop(), elf::nop(), 0x0010_0073];
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(quantum_config(&target, 1024), quantum_image(&code)).unwrap();
    owned.write_dcsr(1 << 15);
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(
        matches!(turn.hart().outcome, StepOutcome::DebugHalted { .. }),
        "EBREAK with dcsr.ebreakm did not halt: {:?}",
        turn.hart().outcome
    );
    let local = owned.platform_local_time().unwrap();
    assert_eq!(
        owned.platform_mtime().unwrap(),
        local,
        "the debug halt left its offset unsynced"
    );
    assert_eq!(owned.platform_anchor().unwrap(), local);
    assert_eq!(guest_time(&owned), local);
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    assert_eq!(
        turn.boundary().quantum.unwrap().syncs,
        1,
        "the halted boundary did not count its sync"
    );
}

/// quantum F4: a queued DMI batch is pending debug work, so it synced before
/// `sample_debug_inputs`; the debugger reads `time` and `mtime` at `L`.
#[test]
fn quantum_f4_apply_dmi_syncs_before_the_debug_sample() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let local = owned.platform_local_time().unwrap();
    assert_eq!(owned.platform_mtime().unwrap(), 0);
    dm_active(&mut owned);
    assert_eq!(
        owned.platform_mtime().unwrap(),
        local,
        "a queued DMI batch did not sync before the debug sample"
    );
    assert_eq!(guest_time(&owned), local);
    assert_eq!(owned.platform_anchor().unwrap(), local);
}

/// quantum D4: debug-work delay is validated and discarded, so it never
/// enters the local offset and a later guest turn charges only its own delay.
#[test]
fn quantum_debug_transport_delay_does_not_enter_the_offset() {
    let target = delay_target(1_000_000, 1_000_000);
    let mut owned =
        OwnedMachine::new(quantum_config(&target, 64), image(elf::sd(2, 1, 0))).unwrap();
    halted_by_dmi(&mut owned);
    let committed = owned.platform_mtime().unwrap();
    // Two program-buffer words store and load one RAM word, each with a one-tick
    // annotated delay.
    write_gpr(&mut owned, 5, elf::BASE + 0x200);
    write_gpr(&mut owned, 6, 0x1234_5678);
    postexec_buffer(&mut owned, [sw_word(6, 5, 0), lw_word(7, 5, 0)]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none());
    assert!(
        target.transactions.load(Ordering::SeqCst) >= 2,
        "the program-buffer RAM accesses bypassed the TLM route"
    );
    assert_eq!(
        owned.platform_local_offset().unwrap(),
        0,
        "debug work added its delay to the local offset"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        committed,
        "debug work committed its delay"
    );
    assert_eq!(
        owned.platform_local_time().unwrap(),
        committed,
        "debug work moved the Hart's local time"
    );
}

/// quantum F4: a busy program-buffer command is pending debug work. The test
/// seeds the offset the debug path cannot otherwise reach, and the grant's own
/// sync keeps `time` and `mtime` at `L` across it.
#[test]
fn quantum_f4_busy_command_syncs_the_seeded_offset() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    halted_by_dmi(&mut owned);
    postexec_buffer(&mut owned, [elf::nop(), elf::nop()]);
    owned.debug_boundary(false).unwrap();
    assert!(owned.debug_command_busy().unwrap());
    owned.test_seed_offset(2).unwrap();
    assert_eq!(owned.platform_local_time().unwrap(), 2);
    owned.debug_boundary(false).unwrap();
    assert_eq!(
        owned.platform_mtime().unwrap(),
        2,
        "the busy command's grant did not sync the seeded offset"
    );
    assert_eq!(guest_time(&owned), 2);
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
}

/// The recorded `sbdata0` value of the last staged system bus read.
fn staged_sba_value(owned: &mut OwnedMachine) -> u64 {
    u64::from(owned.dmi_read(DM_SBDATA0).unwrap())
}

/// quantum F4/D8: staged system bus work is pending debug work, so the idle
/// debug path syncs it and the SBA `mtime` read between the turns of a running
/// Hart returns `L`.
#[test]
fn quantum_f4_staged_sba_syncs_and_reads_local_time() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let local = owned.platform_local_time().unwrap();
    assert_eq!(local, 2);
    owned.test_stage_sba_read(clint_device_offset(
        crate::peripherals::clint::reg_offset::MTIME,
    ));
    owned.debug_boundary(false).unwrap();
    assert_eq!(
        owned.platform_mtime().unwrap(),
        local,
        "staged system bus work did not sync the offset"
    );
    assert_eq!(
        staged_sba_value(&mut owned),
        local,
        "the SBA mtime read did not return local time"
    );
}

/// quantum F4/D8: under a halted Hart the offset is zero, so the same SBA read
/// is the positive control: it returns `C` = `L`.
#[test]
fn quantum_f4_sba_mtime_under_a_halted_hart_returns_committed_time() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    dm_active(&mut owned);
    halted_by_dmi(&mut owned);
    let committed = owned.platform_mtime().unwrap();
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    owned.test_stage_sba_read(clint_device_offset(
        crate::peripherals::clint::reg_offset::MTIME,
    ));
    owned.debug_boundary(false).unwrap();
    assert_eq!(
        staged_sba_value(&mut owned),
        committed,
        "an SBA mtime read under a halted Hart did not return committed time"
    );
}

/// quantum F1: a turn start clears the F1 mark, so a marked device access that
/// debug work made with no turn in progress cannot sync the next guest turn.
#[test]
fn quantum_turn_start_clears_a_stale_mark() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    owned.test_seed_turn_mark().unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(
        !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
        "the guest failed: {:?}",
        turn.hart().outcome
    );
    assert_eq!(
        owned.platform_quantum_syncs().unwrap(),
        0,
        "a stale F1 mark synced a guest turn that flushed nothing"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        0,
        "a stale F1 mark committed time before the guest advanced it"
    );
    assert_eq!(owned.platform_local_offset().unwrap(), 1);
}

/// quantum D3/F1/F6: a turn whose Hart device access flushed the offset and
/// whose own commit then moves no further time still republishes `time` and the
/// machine lines from the new committed time, so MTIP is not left stale.
///
/// The real path needs an unknown completion behind a translation walk; the
/// test injects exactly the mid-turn flush state instead.
#[test]
fn quantum_flush_only_failure_publishes_the_boundary_lines() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop()]),
    )
    .unwrap();
    // Hart 0's compare is 1: committed time 0 leaves MTIP clear, and the flush
    // below moves committed time to 2.
    dm_active(&mut owned);
    let address = clint_device_offset(crate::peripherals::clint::reg_offset::MTIMECMP_BASE);
    sba_set_address(&mut owned, address);
    // The compare resets to all ones, so both halves are written.
    assert_eq!(sba_write_word(&mut owned, address + 4, 0) & SBERROR_MASK, 0);
    assert_eq!(sba_write_word(&mut owned, address, 1) & SBERROR_MASK, 0);
    assert_eq!(owned.platform_mtime().unwrap(), 0);
    assert!(!mtip_presented(&owned));
    owned.test_seed_offset(2).unwrap();
    let flushed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let once = std::sync::Arc::clone(&flushed);
    owned.test_after_turn_start(std::sync::Arc::new(move |platform| {
        if once.swap(false, Ordering::SeqCst) {
            platform.test_device_flush().unwrap();
        }
    }));
    // The zero-delay target's next access completes unknown, so the turn fails
    // with no completed tick and no delay ticks.
    target.set_unknown(true);
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(
        matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
        "the turn did not fail: {:?}",
        turn.hart().outcome
    );
    assert!(turn.boundary().uncertain);
    assert_eq!(
        owned.platform_mtime().unwrap(),
        2,
        "the flush did not commit the pending offset"
    );
    assert_eq!(owned.platform_local_time().unwrap(), 2);
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    assert_eq!(
        owned.platform_quantum_syncs().unwrap(),
        1,
        "the flush-only boundary did not count its one sync"
    );
    assert!(
        mtip_presented(&owned),
        "the flush-only failure left the machine lines unpresented"
    );
    assert_eq!(guest_time(&owned), 2);
}

/// `sbcs.sberror`, all three bits.
const SBERROR_MASK: u32 = 7 << SBERROR_SHIFT;

/// quantum D3/D9: a guest store that lowers `mtime` is a rebase. MTIP falls at
/// the next turn as in a parity run, and `E` = `C` = `L` afterwards.
#[test]
fn quantum_guest_mtime_store_rebases_and_lowers_mtip() {
    // x1 = mtimecmp, x2 = the mtime dword: arm the compare at 8, spin, then
    // lower mtime to 1.
    let code = [
        elf::lui(1, 0x2004),
        elf::addi(5, 0, 8),
        elf::sd(5, 1, 0),
        elf::lui(2, 0x200c),
        elf::addi(2, 2, -8),
        elf::nop(),
        elf::nop(),
        elf::nop(),
        elf::nop(),
        elf::nop(),
        elf::addi(6, 0, 1),
        elf::sd(6, 2, 0),
        elf::nop(),
    ];
    fn route(quantum: u64, code: &[u32]) -> Vec<bool> {
        let target = delay_target(0, 0);
        let mut owned =
            OwnedMachine::new(quantum_config(&target, quantum), quantum_image(code)).unwrap();
        (0..13)
            .map(|_| {
                let turn = owned.step_request(false, false, false).unwrap();
                assert!(
                    !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
                    "the mtime-store guest failed: {:?}",
                    turn.hart().outcome
                );
                mtip_presented(&owned)
            })
            .collect()
    }
    let parity = route(0, &code);
    let quantum = route(2, &code);
    assert!(
        parity[..12].iter().any(|&mtip| mtip),
        "MTIP never rose before the lowering store"
    );
    assert!(
        quantum[..12].iter().any(|&mtip| mtip),
        "Q = 2 never presented MTIP before the lowering store"
    );
    assert_eq!(
        parity[11..],
        quantum[11..],
        "the lowering mtime store presented MTIP differently from the parity run"
    );
    assert!(
        !parity[12],
        "the lowering mtime store did not clear MTIP at the next turn"
    );

    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(quantum_config(&target, 2), quantum_image(&code)).unwrap();
    for _ in 0..12 {
        owned.step_request(false, false, false).unwrap();
    }
    let committed = owned.platform_mtime().unwrap();
    assert_eq!(
        committed, 2,
        "the store turn did not commit the store's value plus its advance"
    );
    assert_eq!(owned.platform_local_time().unwrap(), committed);
    assert_eq!(
        owned.platform_anchor().unwrap(),
        committed,
        "E was not re-anchored by the guest mtime store"
    );
}

/// quantum D3: a debug store to `mtime` through the DM inbound port between the
/// turns of a running Hart re-anchors `E` = `C` = `L`, and its F1 mark does not
/// survive into the next guest turn.
#[test]
fn quantum_debug_mtime_store_reanchors_and_leaves_no_mark() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let address = clint_device_offset(crate::peripherals::clint::reg_offset::MTIME);
    dm_active(&mut owned);
    sba_set_address(&mut owned, address);
    owned.dmi_write(DM_SBCS, SBACCESS_WORD).unwrap();
    // The store runs inside this DMI write and no sync follows it, so the
    // anchor it leaves is the one a later boundary would publish.
    owned.dmi_write(DM_SBDATA0, 0x40).unwrap();
    assert_eq!(owned.platform_mtime().unwrap(), 0x40);
    assert_eq!(
        owned.platform_anchor().unwrap(),
        0x40,
        "E was not re-anchored by the debug mtime store"
    );
    assert_eq!(owned.platform_local_time().unwrap(), 0x40);
    let syncs = owned.platform_quantum_syncs().unwrap();
    owned.step_request(false, false, false).unwrap();
    assert_eq!(
        owned.platform_quantum_syncs().unwrap(),
        syncs,
        "a debug mtime store left a turn mark for the next guest turn"
    );
}

/// quantum D3: an abstract-command store to `mtime` on the Hart data port while
/// the Hart is halted re-anchors `E` = `C` = `L`, and its F1 mark is cleared at
/// the next guest turn's start.
#[test]
fn quantum_abstract_mtime_store_reanchors_and_clears_its_mark() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop()]),
    )
    .unwrap();
    halted_by_dmi(&mut owned);
    let address = clint_device_offset(crate::peripherals::clint::reg_offset::MTIME);
    abstract_write_dword(&mut owned, address, 0x40);
    assert_eq!(owned.platform_mtime().unwrap(), 0x40);
    assert_eq!(
        owned.platform_anchor().unwrap(),
        0x40,
        "E was not re-anchored by the abstract mtime store"
    );
    assert_eq!(owned.platform_local_time().unwrap(), 0x40);
    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    let syncs = owned.platform_quantum_syncs().unwrap();
    owned.step_request(false, false, false).unwrap();
    assert_eq!(
        owned.platform_quantum_syncs().unwrap(),
        syncs,
        "the F1 turn mark survived into the next guest turn"
    );
    assert_eq!(
        owned.platform_local_time().unwrap(),
        0x41,
        "the guest turn did not advance local time"
    );
}

/// quantum F1: a program-buffer word is debug work on the Hart port, so its
/// device store flushes and marks with no turn in progress. The mark must not
/// survive into the next guest turn, which would then sync an offset no guest
/// turn flushed.
#[test]
fn quantum_program_buffer_device_store_leaves_no_mark() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    halted_by_dmi(&mut owned);
    write_gpr(&mut owned, 5, crate::executor::SYSTEM_BUS_PLIC_GATEWAY);
    write_gpr(&mut owned, 6, 1);
    // Two words, so the last executed word is the device store.
    postexec_buffer(&mut owned, [elf::nop(), sw_word(6, 5, 0)]);
    let facts = run_grants(&mut owned);
    assert!(facts.failure.is_none(), "{:?}", facts.failure);
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    owned
        .dmi_write(DM_DMCONTROL, DM_DMACTIVE | DM_RESUMEREQ)
        .unwrap();
    let syncs = owned.platform_quantum_syncs().unwrap();
    let turn = owned.step_request(false, false, false).unwrap();
    assert!(
        !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
        "the guest failed: {:?}",
        turn.hart().outcome
    );
    assert_eq!(
        owned.platform_quantum_syncs().unwrap(),
        syncs,
        "a debug device store left a mark that synced the next guest turn"
    );
    assert_eq!(
        owned.platform_mtime().unwrap(),
        0,
        "a debug device store committed time before the guest advanced it"
    );
}

/// quantum F7/D10: an in-crate `last_slot` stop syncs the offset even when the
/// threshold leaves it pending, and refreshes the boundary's sync count.
#[test]
fn quantum_f7_last_slot_syncs_and_refreshes_the_count() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let turn = owned.step_request(false, false, true).unwrap();
    let facts = turn.boundary().quantum.expect("Q = 1024 reports facts");
    assert_eq!(
        owned.platform_mtime().unwrap(),
        owned.platform_local_time().unwrap(),
        "the last slot did not sync the offset"
    );
    assert!(!facts.reached, "the last slot is not the D5 threshold");
    assert_eq!(
        facts.syncs, 1,
        "the last-slot boundary did not refresh its sync count"
    );
    assert!(turn.boundary().budget_exhausted);
}

/// quantum F8: an in-crate stop that leaves `O` non-zero commits it in the
/// drain report's generation. No public path reaches this.
#[test]
fn quantum_f8_drain_commits_the_offset() {
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(
        quantum_config(&target, 1024),
        quantum_image(&[elf::nop(), elf::nop(), elf::nop()]),
    )
    .unwrap();
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    let local = owned.platform_local_time().unwrap();
    let generation = owned.control_boundary(false).unwrap();
    assert_eq!(generation.quantum.unwrap().syncs, 0);
    assert_eq!(owned.platform_mtime().unwrap(), 0);
    let report = owned.quiesce().unwrap();
    assert_eq!(
        owned.platform_mtime().unwrap(),
        local,
        "the drain acknowledgment did not commit the offset"
    );
    assert_eq!(owned.platform_local_offset().unwrap(), 0);
    assert_eq!(owned.platform_anchor().unwrap(), local);
    assert_eq!(owned.platform_quantum_syncs().unwrap(), 1);
    assert_eq!(report.lifecycle, Lifecycle::DrainComplete);
}

/// quantum F9: no offset crosses a generation. A fresh Platform starts with
/// `C` = `O` = `E` = 0, so the first turn of `install` and `fresh_reset`
/// presents `time` = 0.
#[test]
fn quantum_f9_fresh_generations_start_with_no_offset() {
    // The guest samples `time` first, so the value it reads is the turn-start
    // `L` of the fresh generation.
    let code = [csrrs(5, crate::csr::unprivileged::TIME, 0), elf::nop()];
    fn first_time(owned: &mut OwnedMachine) -> u64 {
        let turn = owned.step_request(false, false, false).unwrap();
        assert!(
            !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
            "the guest failed: {:?}",
            turn.hart().outcome
        );
        assert_eq!(
            owned.platform_mtime().unwrap(),
            0,
            "a fresh generation committed time on its first turn"
        );
        assert_eq!(owned.platform_anchor().unwrap(), 0);
        assert_eq!(
            owned.state().regs[5],
            0,
            "the first turn's `time` was not `C`"
        );
        owned.state().regs[5]
    }
    let target = delay_target(0, 0);
    let mut owned = OwnedMachine::new(quantum_config(&target, 1024), quantum_image(&code)).unwrap();
    assert_eq!(first_time(&mut owned), 0, "install started with an offset");
    for _ in 0..2 {
        owned.step_request(false, false, false).unwrap();
    }
    assert!(owned.platform_local_offset().unwrap() > 0);
    owned.fresh_reset().unwrap();
    assert_eq!(
        first_time(&mut owned),
        0,
        "fresh_reset started with an offset"
    );
}

/// quantum D3/D6/D8/D9: a timer guest that spins on `mip` after arming
/// `mtimecmp` through the CLINT. `Q` = 0 and `Q` = 1 reproduce the native route
/// turn by turn in PC, `time` and MTIP; at `Q` = 16 the crossing is recognized
/// with exactly the lateness its sync phase and quantum predict, inside D8; and
/// the run's final committed time is its D9 identity after the F7 last-slot
/// sync.
#[test]
fn quantum_timer_spin_recognizes_mtip_within_the_bound() {
    const MTIMECMP: i32 = 205;
    let code = [
        elf::lui(4, 0x2004),
        elf::addi(5, 0, MTIMECMP),
        elf::sd(5, 4, 0),
        elf::addi(6, 0, 0x80),
        csrrs(0, MIE, 6),
        csrrs(7, machine::MIP, 0),
        andi(7, 7, 0x80),
        elf::beq(7, 0, -8),
        SPIN,
    ];
    const TURNS: usize = 320;
    /// One boundary's record. `presented` is the CSR `time` the guest reads
    /// after that boundary, and `ledger` is the D9 ledger at it.
    struct SpinRecord {
        pc: u64,
        committed: u64,
        local: u64,
        presented: u64,
        mtip: bool,
        ledger: TimeLedger,
    }
    impl SpinRecord {
        /// The turn-by-turn facts the parity comparison names.
        fn key(&self) -> (u64, u64, u64, u64, bool) {
            (
                self.pc,
                self.committed,
                self.local,
                self.presented,
                self.mtip,
            )
        }
    }
    fn step(owned: &mut OwnedMachine, last_slot: bool) -> SpinRecord {
        let turn = owned.step_request(false, false, last_slot).unwrap();
        assert!(
            !matches!(turn.hart().outcome, StepOutcome::SimulatorFailure(_)),
            "the timer guest failed: {:?}",
            turn.hart().outcome
        );
        SpinRecord {
            pc: owned.state().pc,
            committed: owned.platform_mtime().unwrap(),
            local: owned.platform_local_time().unwrap(),
            presented: guest_time(owned),
            mtip: mtip_presented(owned),
            ledger: owned.test_ledger(),
        }
    }
    fn route(quantum: u64, code: &[u32]) -> Vec<SpinRecord> {
        let target = delay_target(0, 0);
        let mut owned =
            OwnedMachine::new(quantum_config(&target, quantum), quantum_image(code)).unwrap();
        (0..TURNS)
            // The final turn is the run's last slot, so F7 syncs it and the run
            // ends at `C` = `L`.
            .map(|index| step(&mut owned, index + 1 == TURNS))
            .collect()
    }
    let parity = route(0, &code);
    let one = route(1, &code);
    // The native route takes no quantum (quantum D2) and is the reference the
    // contract's U2 row names for `Q` = 1.
    let native = {
        let mut owned = OwnedMachine::new(
            MachineConfig::new(PlatformKind::Native),
            quantum_image(&code),
        )
        .unwrap();
        (0..TURNS)
            .map(|index| step(&mut owned, index + 1 == TURNS))
            .collect::<Vec<SpinRecord>>()
    };
    let native_keys = native.iter().map(SpinRecord::key).collect::<Vec<_>>();
    assert_eq!(
        parity.iter().map(SpinRecord::key).collect::<Vec<_>>(),
        native_keys,
        "Q = 0 diverged from the native route in PC, time or MTIP"
    );
    assert_eq!(
        one.iter().map(SpinRecord::key).collect::<Vec<_>>(),
        native_keys,
        "Q = 1 diverged from the native route in PC, time or MTIP"
    );
    let quantum = route(16, &code);
    // The guest's own F1 `mtimecmp` store turn ends in a sync that commits the
    // store's turn, so committed time afterwards moves in steps of `Q` from that
    // phase. The presentation at or past the compare is therefore computable
    // from the guest's sync phase and `Q` alone, and `X` is the compare itself.
    const STORE_PHASE: u64 = 3;
    const QUANTUM: u64 = 16;
    let compare = u64::try_from(MTIMECMP).unwrap();
    let expected_presentation = STORE_PHASE + (compare - STORE_PHASE).div_ceil(QUANTUM) * QUANTUM;
    let expected_lateness = expected_presentation - compare;
    let presented = quantum
        .iter()
        .position(|record| record.mtip)
        .expect("MTIP was never presented");
    assert_eq!(
        quantum[presented].committed, expected_presentation,
        "the recognition's committed time was not the sync phase plus a whole quantum"
    );
    assert_eq!(
        quantum[presented].local - compare,
        expected_lateness,
        "the recognition's turn-start local time was not `X` plus the computed lateness"
    );
    assert_eq!(
        expected_lateness, 6,
        "the computed D8 lateness is not the contract's `R - X` for this run"
    );
    assert!(
        expected_lateness < QUANTUM,
        "the computed recognition exceeded the D8 bound"
    );
    // quantum D9: after every sync, committed time is the run's charged
    // completed turns, delay ticks and idle-jump distance, with the guest's
    // `mtime` stores rebasing it; the run ends on the F7 last-slot sync with no
    // pending offset.
    for record in &quantum {
        if record.committed == record.local {
            assert_eq!(
                record.ledger.expected_mtime(),
                record.committed,
                "the ledger's D9 identity diverged from committed time at a sync"
            );
        }
    }
    let last = quantum.last().unwrap();
    assert_eq!(
        last.committed, last.local,
        "the last-slot sync left a pending offset"
    );
    assert_eq!(
        last.ledger.expected_mtime(),
        last.committed,
        "the timer run's final committed time is not its D9 identity"
    );
    assert!(last.ledger.charged() > 0, "the ledger charged nothing");
    let parity_last = parity.last().unwrap();
    assert_eq!(
        parity_last.ledger.expected_mtime(),
        parity_last.committed,
        "the parity ledger's D9 identity diverged from committed time"
    );
}
