//! Fault-injection data port for Abstract Access Memory.
//!
//! Backend, protocol, and unknown completions, and a poisoned data-port
//! lock, are simulator faults. Debug Spec 1.0 §3.14 reports them as
//! `cmderr` 7. Misaligned and access-fault accesses stay `cmderr` 3
//! (Debug Spec 1.0 §3.7.1.3).

use super::{
    access_memory, arg0, arg1, clear_cmderr, cmderr, enter_halt, issue, set_arg0, set_arg1,
    typed_handle, DMACTIVE, DMCONTROL, RAM_BASE, RAM_SIZE, RESUMEREQ,
};
use ruscv_sim::core::{RiscvCore, StepOutcome};
use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_UART_BASE};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AtomicBackend, AtomicBackendResult, AtomicRequest, NativeSystemBusBackend, PhysicalBackend,
    PhysicalBackendError, PhysicalBackendResult, PhysicalRequest, PhysicalTargetRejectionReason,
    ValidatedPhysicalAccess,
};
use ruscv_sim::{DebugModule, SharedDataAccess, SharedPhysicalAccess, SimulatorFailureKind};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug)]
enum InjectedFault {
    Host,
    Protocol,
    Unknown,
}

/// Data port that reports a simulator fault instead of a target rejection.
struct InjectedBackend {
    fault: InjectedFault,
    ram: Arc<Mutex<SimpleMemory>>,
    calls: Arc<Mutex<u32>>,
}

impl PhysicalBackend for InjectedBackend {
    fn transact(&mut self, _request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        *self.calls.lock().unwrap() += 1;
        match self.fault {
            InjectedFault::Host => Err(PhysicalBackendError::host("injected backend failure")),
            InjectedFault::Protocol => {
                Err(PhysicalBackendError::protocol("injected protocol failure"))
            }
            InjectedFault::Unknown => {
                // The completion is unknown, so a target byte may already
                // have changed. cmderr 3 would say the access did not happen.
                self.ram.lock().unwrap().write_byte(0x20, 0xa5).unwrap();
                Err(PhysicalBackendError::unknown(
                    "injected completion may have stored",
                ))
            }
        }
    }
}

impl AtomicBackend for InjectedBackend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        Err(PhysicalBackendError::target(
            PhysicalTargetRejectionReason::UnsupportedCategory,
            format!("debug memory test does not issue atomics: {request:?}"),
        ))
    }
}

fn halted_with_instruction_port() -> (
    DebugModule,
    RiscvCore,
    SharedPhysicalAccess,
    Arc<Mutex<SimpleMemory>>,
) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart,
        RAM_BASE,
        RAM_SIZE,
    )));
    let instruction_port: SharedPhysicalAccess = Arc::new(Mutex::new(
        ValidatedPhysicalAccess::new(NativeSystemBusBackend::new(bus.clone())),
    ));
    let data_port: SharedDataAccess = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let mut hart = RiscvCore::new_with_physical_access(
        typed_handle(bus.clone()),
        typed_handle(bus),
        instruction_port.clone(),
        data_port,
    );
    let mut dm = DebugModule::new();
    ram.lock().unwrap().write_word(0, 0x0000_0013).unwrap();
    hart.state_mut().pc = RAM_BASE;
    enter_halt(&mut dm, &mut hart);
    (dm, hart, instruction_port, ram)
}

fn install_injected(
    hart: &mut RiscvCore,
    instruction: SharedPhysicalAccess,
    fault: InjectedFault,
    ram: Arc<Mutex<SimpleMemory>>,
) {
    let data: SharedDataAccess =
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(InjectedBackend {
            fault,
            ram,
            calls: Arc::new(Mutex::new(0)),
        })));
    hart.set_physical_access(instruction, data);
}

/// A backend, protocol, or unknown completion, and a poisoned data-port
/// lock, are simulator faults. Debug Spec 1.0 §3.14 reports them as
/// `cmderr` 7 (other). They are not the M-mode exception §3.7.1.3 reports
/// as `cmderr` 3. Misaligned and access-fault accesses stay at 3.
#[test]
fn simulator_memory_fault_is_cmderr_7() {
    for fault in [
        InjectedFault::Host,
        InjectedFault::Protocol,
        InjectedFault::Unknown,
    ] {
        let (mut dm, mut hart, instruction, ram) = halted_with_instruction_port();
        install_injected(&mut hart, instruction, fault, ram.clone());
        set_arg0(&mut dm, &mut hart, 0x11);
        set_arg1(&mut dm, &mut hart, RAM_BASE + 8);
        issue(&mut dm, &mut hart, access_memory(false, 2, true, true));
        assert_eq!(cmderr(&mut dm, &mut hart), 7, "{fault:?}");
        assert_eq!(arg0(&mut dm, &mut hart), 0x11, "the argument is unchanged");
        assert_eq!(
            arg1(&mut dm, &mut hart),
            RAM_BASE + 8,
            "no postincrement on a simulator fault"
        );
        assert!(hart.in_debug_mode());
        assert_eq!(hart.state().pc, RAM_BASE);
        let marker = ram.lock().unwrap().read_byte(0x20).unwrap();
        if matches!(fault, InjectedFault::Unknown) {
            assert_eq!(marker, 0xa5, "unknown completion may already have stored");
        } else {
            assert_eq!(marker, 0);
            assert!(
                hart.unresolved_physical_access().is_none(),
                "{fault:?} does not latch a terminal completion"
            );
        }
    }

    let (mut dm, mut hart, instruction, ram) = halted_with_instruction_port();
    let data: SharedDataAccess =
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(InjectedBackend {
            fault: InjectedFault::Host,
            ram: ram.clone(),
            calls: Arc::new(Mutex::new(0)),
        })));
    let doomed = data.clone();
    let joined = std::thread::spawn(move || {
        let _guard = doomed.lock().unwrap();
        panic!("poison the data port");
    })
    .join();
    assert!(joined.is_err(), "the lock holder panicked");
    hart.set_physical_access(instruction, data);
    set_arg1(&mut dm, &mut hart, RAM_BASE);
    issue(&mut dm, &mut hart, access_memory(false, 2, true, false));
    assert_eq!(cmderr(&mut dm, &mut hart), 7, "poisoned data-port lock");
    assert_eq!(arg1(&mut dm, &mut hart), RAM_BASE, "no postincrement");
    assert!(hart.in_debug_mode());
    assert!(hart.unresolved_physical_access().is_none());
}

/// `MemoryError::Unknown` on Abstract Access Memory sets the same sticky
/// latch the instruction path sets. The next `step_outcome` fails before
/// retirement, and a later abstract access returns `cmderr` 7 without
/// touching the data port. Debug Spec 1.0 §3.14 `cmderr` 7.
#[test]
fn unknown_completion_latches_and_blocks_the_next_access() {
    let (mut dm, mut hart, instruction, ram) = halted_with_instruction_port();
    let calls = Arc::new(Mutex::new(0u32));
    let data: SharedDataAccess =
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(InjectedBackend {
            fault: InjectedFault::Unknown,
            ram: ram.clone(),
            calls: calls.clone(),
        })));
    hart.set_physical_access(instruction, data);
    set_arg0(&mut dm, &mut hart, 0x11);
    set_arg1(&mut dm, &mut hart, RAM_BASE + 8);
    issue(&mut dm, &mut hart, access_memory(false, 2, true, true));
    assert_eq!(cmderr(&mut dm, &mut hart), 7);
    assert_eq!(*calls.lock().unwrap(), 1);
    let failure = hart
        .unresolved_physical_access()
        .expect("unknown completion latches");
    assert_eq!(failure.kind, SimulatorFailureKind::HostBackend);
    assert!(
        failure.message.contains("unresolved physical completion"),
        "{}",
        failure.message
    );
    assert_eq!(ram.lock().unwrap().read_byte(0x20).unwrap(), 0xa5);

    clear_cmderr(&mut dm, &mut hart);
    set_arg0(&mut dm, &mut hart, 0x22);
    set_arg1(&mut dm, &mut hart, RAM_BASE + 16);
    issue(&mut dm, &mut hart, access_memory(false, 2, true, true));
    assert_eq!(
        cmderr(&mut dm, &mut hart),
        7,
        "the latched access is still other"
    );
    assert_eq!(
        *calls.lock().unwrap(),
        1,
        "a second abstract access does not touch the port"
    );
    assert_eq!(arg1(&mut dm, &mut hart), RAM_BASE + 16, "no postincrement");
    assert_eq!(ram.lock().unwrap().read_byte(0x20).unwrap(), 0xa5);

    let pc = hart.state().pc;
    let outcome = hart.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::SimulatorFailure(_)),
        "step_outcome fails before retirement: {outcome:?}"
    );
    assert_eq!(hart.state().pc, pc);
    assert!(hart.in_debug_mode());

    dm.dmi_write(&mut hart, DMCONTROL, DMACTIVE | RESUMEREQ);
    assert!(!hart.in_debug_mode());
    let pc = hart.state().pc;
    let outcome = hart.step_outcome();
    assert!(
        matches!(outcome, StepOutcome::SimulatorFailure(_)),
        "a resumed hart still does not retire: {outcome:?}"
    );
    assert_eq!(hart.state().pc, pc);
}
