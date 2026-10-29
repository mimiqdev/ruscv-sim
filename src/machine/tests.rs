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
    while owned.debug_command_busy().unwrap() {
        owned.debug_boundary(false).unwrap();
    }

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
