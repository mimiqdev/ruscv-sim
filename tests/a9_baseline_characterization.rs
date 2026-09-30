//! A9 T0: executable observations of the frozen b36b4d0 behavior, not the
//! future Hart-fact or Machine lifecycle contract. No production hooks added.

#[allow(dead_code)]
mod common;

use common::public_elf as elf;
use ruscv_sim::core::{ExceptionCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{load_and_run, reset_core, step_once, RiscVSimulator, SystemBus};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AtomicBackend, AtomicBackendResult, AtomicRequest, NativeRamBackend, PhysicalBackend,
    PhysicalBackendError, PhysicalBackendResult, PhysicalRequest, ValidatedPhysicalAccess,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use tempfile::TempDir;

const TARGET: u64 = 0x600;
const SEED: u64 = 0x1122_3344_5566_7788;

fn atomic(funct5: u32, rd: u32, rs2: u32) -> u32 {
    (funct5 << 27) | (rs2 << 20) | (1 << 15) | (3 << 12) | (rd << 7) | 0x2f
}

fn native_core(code: &[u32]) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    {
        let mut memory = ram.lock().unwrap();
        for (index, word) in code.iter().enumerate() {
            memory.write_word(index as u64 * 4, *word).unwrap();
        }
        memory.write_dword(TARGET, SEED).unwrap();
    }
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, 0x1000),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, 0x1000),
    )));
    let mut core = RiscvCore::new_with_physical_ports(ram.clone(), ram.clone(), fetch, data);
    core.reset(0, 0);
    core.state_mut().regs[1] = TARGET;
    core.state_mut().regs[2] = 0x55;
    (core, ram)
}

fn retire(core: &mut RiscvCore) {
    assert!(matches!(
        core.step_outcome(),
        StepOutcome::InstructionRetired(_)
    ));
}

#[test]
fn snapshot_log_omits_real_memory_atomic_csr_fpr_and_same_value_writes() {
    let csr_write = (u32::from(machine::MSCRATCH) << 20) | (2 << 15) | (1 << 12) | 0x73;
    let fld = (1 << 15) | (3 << 12) | (1 << 7) | 0x07;
    let code = [
        elf::auipc(1, 0),
        elf::addi(1, 1, TARGET as i32),
        elf::addi(2, 0, 5),
        elf::sd(2, 1, 0),
        atomic(0, 3, 2), // AMOADD.D: returns 5 and writes 10
        atomic(2, 4, 0), // LR.D: returns 10
        atomic(3, 5, 2), // successful SC.D: writes 5, rd remains zero
        atomic(3, 6, 2), // failed SC.D: rd = 1, no memory write
        atomic(3, 0, 2), // failed SC.D x0: retires, no register/memory write
        csr_write,
        fld,
        elf::addi(2, 0, 5), // architectural destination write with equal value
    ];
    let image = elf::elf_with_code(&code, 0, false, false, 0x4000);
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("baseline.log");
    let logged = load_and_run(&image, Some(code.len() as u64), None, Some(&path), false).unwrap();
    let unlogged = load_and_run(&image, Some(code.len() as u64), None, None, false).unwrap();
    assert_eq!(logged.cycles, unlogged.cycles);
    assert_eq!(logged.final_pc, unlogged.final_pc);
    assert_eq!(logged.error, unlogged.error);
    assert!(logged.timed_out && unlogged.timed_out);

    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&image).unwrap();
    let result = flat.run(Some(code.len() as u64)).unwrap();
    assert_eq!(result.cycles, code.len() as u64);
    assert_eq!(
        flat.state().csr.read(machine::MINSTRET).unwrap(),
        code.len() as u64
    );
    assert_eq!(flat.state().csr.read(machine::MSCRATCH).unwrap(), 5);
    assert_eq!(flat.state().fpr.read(1).bits(), 5);
    assert_eq!(flat.state().regs[3], 5);
    assert_eq!(flat.state().regs[4], 10);
    assert_eq!(flat.state().regs[5], 0);
    assert_eq!(flat.state().regs[6], 1);
    assert_eq!(flat.state().regs[0], 0);
    assert_eq!(flat.read_mem(TARGET, 8).unwrap(), 5u64.to_le_bytes());

    let text = std::fs::read_to_string(path).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), code.len());
    for (index, line) in lines.iter().enumerate() {
        assert!(line.contains(&format!("({:#010x})", code[index])));
        assert!(
            !line.contains(" mem "),
            "baseline has no memory facts: {line}"
        );
    }
    assert!(lines[4].contains(" x3  0x0000000000000005"));
    assert!(lines[7].contains(" x6  0x0000000000000001"));
    // These lines contain only identity, despite real effects/retirement.
    for index in [3, 6, 8, 9, 10, 11] {
        assert_eq!(
            lines[index],
            format!(
                "core   0: 3 {:#018x} ({:#010x})",
                elf::BASE + index as u64 * 4,
                code[index]
            )
        );
    }
}

#[test]
fn self_modifying_store_log_uses_fetched_instruction_not_replaced_bytes() {
    let store = elf::sd(2, 1, 0);
    let code = [
        elf::auipc(1, 0),
        elf::addi(1, 1, 16),
        elf::addi(2, 0, 0x13),
        elf::nop(),
        store,
    ];
    let image = elf::elf_with_code(&code, 0, false, false, 0x4000);
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("self-modifying.log");
    let result = load_and_run(&image, Some(5), None, Some(&path), false).unwrap();
    assert!(result.timed_out);
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&image).unwrap();
    flat.run(Some(5)).unwrap();
    assert_eq!(flat.state().csr.read(machine::MINSTRET).unwrap(), 5);
    assert_eq!(flat.read_mem(16, 8).unwrap(), 0x13u64.to_le_bytes());
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text
        .lines()
        .last()
        .unwrap()
        .contains(&format!("({store:#010x})")));
}

#[test]
fn failed_sc_retires_but_faulting_sc_does_not_write_rd_or_retire() {
    for rd in [0, 3] {
        for invalid_target in [false, true] {
            let (mut core, ram) = native_core(&[atomic(3, rd, 2)]);
            core.state_mut().regs[3] = 0xbeef;
            if invalid_target {
                core.state_mut().regs[1] = 0x2000;
            }
            let outcome = core.step_outcome();
            if invalid_target {
                assert!(matches!(outcome, StepOutcome::TrapEntered(trap)
                    if trap.cause == ExceptionCause::StoreAccessFault && trap.mtval == 0x2000));
                assert_eq!(core.state().regs[3], 0xbeef);
                assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
            } else {
                assert!(matches!(outcome, StepOutcome::InstructionRetired(_)));
                assert_eq!(core.state().regs[3], if rd == 3 { 1 } else { 0xbeef });
                assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 1);
                assert_eq!(core.state().pc, 4);
            }
            assert_eq!(core.state().regs[0], 0);
            assert_eq!(ram.lock().unwrap().read_dword(TARGET).unwrap(), SEED);
        }
    }
}

#[test]
fn completed_exit_wins_final_budget_and_step_does_not_poll_or_clear() {
    let code = [
        elf::auipc(1, 1),
        elf::addi(2, 0, 7),
        elf::sd(2, 1, 0),
        elf::nop(),
    ];
    let image = elf::elf_with_code(&code, 0, true, false, 0x4000);
    let native = load_and_run(&image, Some(3), None, None, false).unwrap();
    assert_eq!(native.exit_code, 3);
    assert_eq!(native.cycles, 3);
    assert_eq!(native.final_pc, elf::BASE + 12);
    assert!(!native.timed_out);
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&image).unwrap();
    for _ in 0..3 {
        flat.step().unwrap();
    }
    assert_eq!(flat.read_mem(0x1000, 8).unwrap(), 7u64.to_le_bytes());
    let zero = flat.run(Some(0)).unwrap();
    assert!(zero.timed_out);
    assert_eq!(zero.cycles, 0);
    assert_eq!(flat.read_mem(0x1000, 8).unwrap(), 7u64.to_le_bytes());
    let resumed = flat.run(Some(1)).unwrap();
    assert_eq!(resumed.exit_code, 3);
    assert_eq!(resumed.cycles, 1);
    assert!(!resumed.timed_out);
    assert_eq!(flat.state().csr.read(machine::MINSTRET).unwrap(), 4);
    assert_eq!(flat.read_mem(0x1000, 8).unwrap(), [0; 8]);
}

#[test]
fn failed_last_slot_does_not_present_pending_exit_or_timeout_and_facade_can_reuse() {
    let unsupported = 0x0000_100f; // legal but unimplemented FENCE.I
    let mut image = elf::elf_with_code(&[unsupported, elf::nop()], 0, true, false, 0x4000);
    image[elf::LOAD_OFFSET + 0x1000..elf::LOAD_OFFSET + 0x1008]
        .copy_from_slice(&7u64.to_le_bytes());
    let native = load_and_run(&image, Some(1), None, None, false).unwrap();
    assert_eq!(native.cycles, 0);
    assert_eq!(native.exit_code, 1);
    assert!(!native.timed_out);
    assert!(native
        .error
        .as_deref()
        .unwrap()
        .contains("UnsupportedLegalInstruction"));
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&image).unwrap();
    let failed = flat.run(Some(1)).unwrap();
    assert_eq!(failed.cycles, 0);
    assert!(!failed.timed_out);
    assert!(failed
        .error
        .as_deref()
        .unwrap()
        .contains("UnsupportedLegalInstruction"));
    assert_eq!(flat.state().pc, elf::BASE);
    assert_eq!(flat.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(flat.read_mem(0x1000, 8).unwrap(), 7u64.to_le_bytes());
    // Ordinary failure is not an unresolved physical completion. Existing
    // state mutation and resumability remain available (not a drain proof).
    flat.state_mut().pc += 4;
    let reused = flat.run(Some(1)).unwrap();
    assert_eq!(reused.exit_code, 3);
    assert_eq!(reused.cycles, 1);
    assert!(!reused.timed_out);
    assert_eq!(flat.read_mem(0x1000, 8).unwrap(), [0; 8]);
}

#[test]
fn core_only_reset_clears_hart_but_preserves_mutated_ram_and_ports() {
    let (mut core, ram) = native_core(&[atomic(2, 3, 0), elf::sd(2, 1, 0)]);
    retire(&mut core);
    assert!(core.state().reservation.is_some());
    retire(&mut core);
    assert_eq!(ram.lock().unwrap().read_dword(TARGET).unwrap(), 0x55);
    reset_core(&mut core, 0, 0);
    assert_eq!(core.state().regs, [0; 32]);
    assert!(core.state().reservation.is_none());
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(ram.lock().unwrap().read_dword(TARGET).unwrap(), 0x55);
    core.state_mut().regs[1] = TARGET;
    step_once(&mut core).unwrap();
    assert_eq!(core.state().regs[3], 0x55, "same port sees stale run bytes");
}

#[test]
fn cloneable_cross_thread_writers_affect_sc_only_for_committed_overlap() {
    for (offset, rejected, expected_sc) in [
        (TARGET, false, 1),
        (TARGET + 16, false, 0),
        (0x10000, true, 0),
    ] {
        let image =
            elf::elf_with_code(&[atomic(2, 3, 0), atomic(3, 4, 2)], 0, false, false, 0x4000);
        let mut flat = RiscVSimulator::new(1);
        flat.load_elf(&image).unwrap();
        flat.state_mut().regs[1] = elf::BASE + TARGET;
        flat.state_mut().regs[2] = 0x55;
        flat.write_mem(TARGET, &SEED.to_le_bytes()).unwrap();
        assert!(flat.run(Some(1)).unwrap().timed_out);
        let handle = flat.memory().clone();
        let (start_tx, start_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let writer = thread::spawn(move || {
            start_rx.recv().unwrap();
            let result = handle.lock().unwrap().write_byte(offset, 0xaa);
            done_tx.send(result.is_err()).unwrap();
        });
        start_tx.send(()).unwrap();
        assert_eq!(done_rx.recv().unwrap(), rejected);
        writer.join().unwrap();
        assert!(flat.run(Some(1)).unwrap().timed_out);
        assert_eq!(flat.state().regs[4], expected_sc);
        assert_eq!(flat.state().csr.read(machine::MINSTRET).unwrap(), 2);
        assert!(flat.state().reservation.is_none());
        let expected = if expected_sc == 0 {
            0x55
        } else {
            (SEED & !0xff) | 0xaa
        };
        assert_eq!(flat.read_mem(TARGET, 8).unwrap(), expected.to_le_bytes());
    }
}

#[test]
fn reload_isolates_stale_cross_thread_handle_and_failed_load_preserves_current_image() {
    let first = elf::elf_with_code(&[atomic(2, 3, 0)], 0, true, false, 0x4000);
    let second = elf::elf_with_code(&[elf::addi(7, 0, 9)], 0, true, false, 0x4000);
    let mut flat = RiscVSimulator::new(1);
    flat.load_elf(&first).unwrap();
    flat.state_mut().regs[1] = elf::BASE + TARGET;
    flat.write_mem(TARGET, &SEED.to_le_bytes()).unwrap();
    flat.step().unwrap();
    assert!(flat.state().reservation.is_some());
    let stale = flat.memory().clone();
    let (tx, rx) = mpsc::channel();
    let writer = thread::spawn(move || {
        rx.recv().unwrap();
        let mut old = stale.lock().unwrap();
        old.write_dword(TARGET, 0x99).unwrap();
        old.write_dword(0x1000, 7).unwrap();
        assert_eq!(old.read_dword(TARGET).unwrap(), 0x99);
    });
    flat.load_elf(&second).unwrap();
    tx.send(()).unwrap();
    writer.join().unwrap();
    assert_eq!(flat.read_mem(TARGET, 8).unwrap(), [0; 8]);
    assert_eq!(flat.read_mem(0x1000, 8).unwrap(), [0; 8]);
    assert_eq!(flat.state().regs, [0; 32]);
    assert!(flat.state().reservation.is_none());
    assert!(flat.run(Some(1)).unwrap().timed_out);
    assert_eq!(flat.state().regs[7], 9);
    let current = flat.memory().clone();
    let rejected = elf::elf_with_placement(&[elf::nop()], 0, elf::BASE, Some(elf::BASE - 8), 0);
    assert!(flat.load_elf(&rejected).is_err());
    assert!(Arc::ptr_eq(&current, flat.memory()));
    assert_eq!(flat.state().pc, elf::BASE + 4);
    assert_eq!(flat.state().regs[7], 9);
}

#[test]
fn native_callbacks_run_inside_started_turn_before_retirement_is_returned() {
    for uart_output in [false, true] {
        let word = if uart_output {
            elf::sb(2, 1, 0)
        } else {
            elf::sd(2, 1, 0)
        };
        let (mut core, ram) = native_core(&[word]);
        let uart = Arc::new(Mutex::new(Uart16550::new(0x1000_0000)));
        let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart.clone(), 0, 0x1000)));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let callback = move |value: u64| {
            started_tx.send(value).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
        };
        if uart_output {
            uart.lock()
                .unwrap()
                .set_output_callback(move |byte| callback(u64::from(byte)));
        } else {
            bus.lock().unwrap().set_htif_write_callback(callback);
        }
        let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            SystemBus::physical_backend(bus.clone()),
        )));
        let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            SystemBus::physical_backend(bus),
        )));
        core.set_physical_access(fetch, data);
        core.state_mut().regs[1] = if uart_output {
            0x1000_0000
        } else {
            0x4000_8000
        };
        core.state_mut().regs[2] = if uart_output { u64::from(b'A') } else { 7 };
        let finished = Arc::new(AtomicBool::new(false));
        let observed_finished = finished.clone();
        let observer = thread::spawn(move || {
            assert_eq!(
                started_rx.recv().unwrap(),
                if uart_output { u64::from(b'A') } else { 7 }
            );
            assert!(
                !observed_finished.load(Ordering::SeqCst),
                "callback is not completed-turn delivery"
            );
            release_tx.send(()).unwrap();
        });
        retire(&mut core);
        finished.store(true, Ordering::SeqCst);
        observer.join().unwrap();
        assert_eq!(core.state().pc, 4);
        assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 1);
    }
}

struct PendingWrite {
    address: u64,
    bytes: Vec<u8>,
}

struct UnknownWrite {
    pending: Arc<Mutex<Option<PendingWrite>>>,
    calls: Arc<Mutex<usize>>,
}

impl AtomicBackend for UnknownWrite {
    fn transact_atomic(&mut self, _request: &AtomicRequest<'_>) -> AtomicBackendResult {
        panic!("ordinary-store characterization must not issue an atomic envelope")
    }
}

impl PhysicalBackend for UnknownWrite {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        *self.calls.lock().unwrap() += 1;
        *self.pending.lock().unwrap() = Some(PendingWrite {
            address: request.paddr(),
            bytes: request.write_payload().unwrap().to_vec(),
        });
        Err(PhysicalBackendError::unknown(
            "accepted write may complete later",
        ))
    }
}

#[test]
fn unresolved_completion_survives_core_reset_while_late_backend_effect_is_not_drained() {
    let (mut core, ram) = native_core(&[elf::sd(2, 1, 0)]);
    let pending = Arc::new(Mutex::new(None));
    let calls = Arc::new(Mutex::new(0));
    let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeRamBackend::new(ram.clone(), 0, 0x1000),
    )));
    let data = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(UnknownWrite {
        pending: pending.clone(),
        calls: calls.clone(),
    })));
    core.set_physical_access(fetch, data);
    let failure = core.step_outcome();
    assert!(matches!(&failure, StepOutcome::SimulatorFailure(_)));
    assert_eq!(core.state().pc, 0);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(ram.lock().unwrap().read_dword(TARGET).unwrap(), SEED);
    core.reset(0, 0);
    assert!(core.unresolved_physical_access().is_some());
    assert_eq!(core.step_outcome(), failure);
    assert_eq!(
        *calls.lock().unwrap(),
        1,
        "reset must not retry unknown work"
    );
    // Controllable adapter seam: release a real possible late effect only
    // after core reset. This is failure evidence, not safe reset/recovery.
    let writer = thread::spawn(move || {
        let PendingWrite { address, bytes } = pending.lock().unwrap().take().unwrap();
        ram.lock()
            .unwrap()
            .write_dword(address, u64::from_le_bytes(bytes.try_into().unwrap()))
            .unwrap();
        ram.lock().unwrap().read_dword(address).unwrap()
    });
    assert_eq!(writer.join().unwrap(), 0x55);
    assert_eq!(core.step_outcome(), failure);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
}
