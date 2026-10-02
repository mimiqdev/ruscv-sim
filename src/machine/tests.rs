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
}
impl PhysicalBackend for UnknownWrite {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        assert_eq!(request.category(), AccessCategory::DataWrite);
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
