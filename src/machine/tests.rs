//! Controlled native backend/host-admission seams, not public recovery APIs.
use super::*;
use crate::csr::machine;
use crate::physical::{
    AccessCategory, AtomicBackend, AtomicBackendResult, AtomicRequest, PhysicalAccessResult,
    PhysicalBackend, PhysicalBackendError, PhysicalBackendResult, PhysicalRequest,
    ValidatedPhysicalAccess,
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
            ram = installed.platform.ram.clone();
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
