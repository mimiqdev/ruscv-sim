//! A9 T1: subscribed completed Hart facts, independent of Runner policy.
use ruscv_sim::core::observation::{HartTransition, Observation, ObservationSink};
use ruscv_sim::core::{ExceptionCause, RiscvCore, StepOutcome};
use ruscv_sim::csr::machine;
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::physical::{
    AccessCategory, AtomicAccessKind, AtomicBackend, AtomicBackendResult, AtomicRequest,
    ConditionalStatus, NativeRamBackend, PhysicalBackend, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalTargetRejectionReason, PhysicalWidth,
    ValidatedPhysicalAccess,
};
use ruscv_sim::PrivilegeMode;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::{Arc, Mutex};

// Thread-local accounting excludes parallel tests/host setup. It measures
// record allocations versus the existing state-staging allocation, not speed.
struct CountAllocator;
thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
unsafe impl GlobalAlloc for CountAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAllocator = CountAllocator;

const TARGET: u64 = 0x200;
fn addi(rd: u32, rs: u32, immediate: u32) -> u32 {
    (immediate << 20) | (rs << 15) | (rd << 7) | 0x13
}
fn atomic(kind: u32, width: PhysicalWidth, rd: u32, rs2: u32) -> u32 {
    (kind << 27)
        | (3 << 25)
        | (rs2 << 20)
        | (1 << 15)
        | (if width == PhysicalWidth::Word { 2 } else { 3 } << 12)
        | (rd << 7)
        | 0x2f
}
fn csr(funct3: u32, rd: u32, addr: u16, rs1: u32) -> u32 {
    (u32::from(addr) << 20) | (rs1 << 15) | (funct3 << 12) | (rd << 7) | 0x73
}

#[derive(Clone, Copy)]
enum Failure {
    Target,
    Host,
    Protocol,
    UnknownAfterWrite,
}
#[derive(Default)]
struct Spy {
    fetches: usize,
    reads: usize,
    writes: usize,
    atomics: usize,
    failure: Option<Failure>,
}
struct Backend {
    ram: NativeRamBackend,
    spy: Arc<Mutex<Spy>>,
}
fn failure(mode: Failure) -> PhysicalBackendError {
    match mode {
        Failure::Target => PhysicalBackendError::target(
            PhysicalTargetRejectionReason::UnsupportedWidth,
            "injected valid target rejection",
        ),
        Failure::Host => PhysicalBackendError::host("injected unavailable resource"),
        Failure::Protocol => PhysicalBackendError::protocol("injected malformed adapter"),
        Failure::UnknownAfterWrite => PhysicalBackendError::unknown("effect may have completed"),
    }
}
impl PhysicalBackend for Backend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        let mode = {
            let mut spy = self.spy.lock().unwrap();
            match request.category() {
                AccessCategory::Fetch => spy.fetches += 1,
                AccessCategory::DataRead => spy.reads += 1,
                AccessCategory::DataWrite => spy.writes += 1,
            }
            if request.category() == AccessCategory::DataWrite {
                spy.failure
            } else {
                None
            }
        };
        if let Some(mode) = mode {
            if matches!(mode, Failure::UnknownAfterWrite) {
                self.ram.transact(request).unwrap();
            }
            return Err(failure(mode));
        }
        self.ram.transact(request)
    }
}
impl AtomicBackend for Backend {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        let mode = {
            let mut spy = self.spy.lock().unwrap();
            spy.atomics += 1;
            spy.failure
        };
        if let Some(mode) = mode {
            return Err(failure(mode));
        }
        self.ram.transact_atomic(request)
    }
}
struct Harness {
    core: RiscvCore,
    memory: Arc<Mutex<SimpleMemory>>,
    spy: Arc<Mutex<Spy>>,
}
fn harness(code: &[u32], typed: bool) -> Harness {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
    {
        let mut ram = memory.lock().unwrap();
        for (index, instruction) in code.iter().enumerate() {
            ram.write_word(index as u64 * 4, *instruction).unwrap();
        }
        ram.write_dword(TARGET, 0x8877_6655_ffff_fffe).unwrap();
    }
    let spy = Arc::new(Mutex::new(Spy::default()));
    let mut core = if typed {
        RiscvCore::new(memory.clone(), memory.clone())
    } else {
        let backend = || {
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(Backend {
                ram: NativeRamBackend::new(memory.clone(), 0, 0x1000),
                spy: spy.clone(),
            })))
        };
        RiscvCore::new_with_physical_ports(memory.clone(), memory.clone(), backend(), backend())
    };
    core.reset(0, 0);
    core.state_mut().regs[1] = TARGET;
    core.state_mut().regs[2] = 5;
    Harness { core, memory, spy }
}
fn commit(transition: &HartTransition) -> &ruscv_sim::core::observation::CommitRecord {
    match transition.observation.as_ref().unwrap() {
        Observation::Commit(record) => record,
        other => panic!("expected commit, got {other:?}"),
    }
}
fn trap(transition: &HartTransition) -> &ruscv_sim::core::observation::TrapRecord {
    match transition.observation.as_ref().unwrap() {
        Observation::Trap(record) => record,
        other => panic!("expected trap, got {other:?}"),
    }
}

#[test]
fn gpr_csr_counter_write_intent_and_branch_identity_are_hart_owned() {
    let code = [
        addi(2, 0, 5),
        addi(0, 0, 9),
        csr(1, 0, machine::MSCRATCH, 2),
        csr(2, 3, machine::MSCRATCH, 0),
        csr(2, 0, machine::MSCRATCH, 4),
        csr(1, 0, machine::MINSTRET, 5),
        0x0000_0463,
    ]; // BEQ x0,x0,+8
    let mut h = harness(&code, false);
    h.core.state_mut().regs[4] = 0; // CSRRS non-x0 zero still writes
    h.core.state_mut().regs[5] = 5; // explicit MINSTRET equal-value write
    for (index, instruction) in code.iter().enumerate() {
        let turn = h.core.step_transition(true);
        let record = commit(&turn);
        assert_eq!(record.retired.pc, index as u64 * 4);
        assert_eq!(record.retired.instruction, *instruction);
        assert_eq!(record.instruction_length, 4);
        assert_eq!(record.retired.privilege, PrivilegeMode::Machine);
        assert!(
            turn.control.instruction_attempted
                && turn.control.retired
                && !turn.control.trap_entered
        );
        assert!(!record.effects.gpr.iter().any(|write| write.index == 0));
        match index {
            0 => assert_eq!(
                record.effects.gpr[0],
                ruscv_sim::core::observation::RegisterWrite {
                    index: 2,
                    before: 5,
                    after: 5
                }
            ),
            1 | 2 | 4 | 5 | 6 => assert!(record.effects.gpr.is_empty()),
            3 => assert_eq!(record.effects.gpr[0].after, 5),
            _ => unreachable!(),
        }
        if index == 3 {
            assert_eq!(
                record.effects.csr.len(),
                1,
                "CSR read has only retirement counter effect"
            );
        }
        if index == 4 {
            assert!(record
                .effects
                .csr
                .iter()
                .any(|write| write.addr == machine::MSCRATCH
                    && write.old_value == 5
                    && write.new_value == 5));
        }
        if index == 5 {
            assert!(record.retired.explicit_minstret_write);
            assert_eq!(record.effects.csr.len(), 1);
            assert_eq!(record.effects.csr[0].old_value, 5);
            assert_eq!(record.effects.csr[0].new_value, 5);
            assert_eq!(turn.control.minstret_before, turn.control.minstret_after);
        }
        if index == 6 {
            assert_eq!(record.retired.next_pc, 32);
        }
    }
    assert_eq!(
        h.spy.lock().unwrap().fetches,
        code.len(),
        "no observation fetch"
    );
}

#[test]
fn ordinary_load_store_all_widths_are_response_bytes_not_register_guesses() {
    for (funct3, width) in [
        (0, PhysicalWidth::Byte),
        (1, PhysicalWidth::Halfword),
        (2, PhysicalWidth::Word),
        (3, PhysicalWidth::Doubleword),
    ] {
        // Load into x0 still records read; store records payload despite no rd.
        let load = (1 << 15) | (funct3 << 12) | 0x03;
        let store = (2 << 20) | (1 << 15) | (funct3 << 12) | 0x23;
        let mut h = harness(&[load, store], false);
        let read = h.core.step_transition(true);
        let effect = &commit(&read).effects.memory[0];
        assert_eq!(effect.guest_address, TARGET);
        assert_eq!(effect.issued_address, Some(TARGET));
        assert_eq!(effect.width, width);
        assert!(effect.write.is_none() && effect.atomic.is_none());
        assert_eq!(
            &effect.read.unwrap()[..width.bytes()],
            &0x8877_6655_ffff_fffeu64.to_le_bytes()[..width.bytes()]
        );
        assert!(commit(&read).effects.gpr.is_empty());
        let write = h.core.step_transition(true);
        let effect = &commit(&write).effects.memory[0];
        assert_eq!(effect.width, width);
        assert!(effect.read.is_none());
        assert_eq!(
            &effect.write.unwrap()[..width.bytes()],
            &5u64.to_le_bytes()[..width.bytes()]
        );
        let spy = h.spy.lock().unwrap();
        assert_eq!((spy.fetches, spy.reads, spy.writes), (2, 1, 1));
    }
}

#[test]
fn fp_load_store_and_equal_value_fpr_writes_are_observed_without_gpr_aliases() {
    for width in [PhysicalWidth::Word, PhysicalWidth::Doubleword] {
        let funct3 = if width == PhysicalWidth::Word { 2 } else { 3 };
        let load = (1 << 15) | (funct3 << 12) | (3 << 7) | 0x07;
        let store = (3 << 20) | (1 << 15) | (funct3 << 12) | 0x27;
        let mut h = harness(&[load, load, store], false);
        let first = h.core.step_transition(true);
        assert_eq!(commit(&first).effects.fpr[0].index, 3);
        assert!(commit(&first).effects.gpr.is_empty());
        let equal = h.core.step_transition(true);
        let fpr = &commit(&equal).effects.fpr[0];
        assert_eq!(fpr.before, fpr.after);
        assert_eq!(commit(&equal).effects.memory[0].width, width);
        let stored = h.core.step_transition(true);
        assert!(commit(&stored).effects.fpr.is_empty());
        assert_eq!(
            commit(&stored).effects.memory[0].write.unwrap(),
            commit(&first).effects.memory[0].read.unwrap()
        );
        let spy = h.spy.lock().unwrap();
        assert_eq!((spy.fetches, spy.reads, spy.writes), (3, 2, 1));
    }
}

#[test]
fn fp_flags_are_changes_of_the_completed_transition_and_off_state_is_identical() {
    let fadd = (2 << 20) | (1 << 15) | (3 << 7) | 0x53;
    let mut on = harness(&[fadd], false);
    let mut off = harness(&[fadd], false);
    for h in [&mut on, &mut off] {
        h.core
            .state_mut()
            .fpr
            .write(1, ruscv_sim::fpu::Fpr::new(f32::MAX));
        h.core
            .state_mut()
            .fpr
            .write(2, ruscv_sim::fpu::Fpr::new(f32::MAX));
    }
    let recorded = on.core.step_transition(true);
    let plain = off.core.step_transition(false);
    assert_eq!(recorded.control, plain.control);
    let effects = &commit(&recorded).effects;
    assert_eq!(effects.fpr[0].index, 3);
    assert!(effects.gpr.is_empty() && effects.memory.is_empty());
    assert_eq!(effects.fcsr, Some((0, on.core.state().fcsr.read())));
    assert_ne!(on.core.state().fcsr.read(), 0);
    assert_eq!(on.core.state().fcsr.read(), off.core.state().fcsr.read());
    assert_eq!(
        on.core.state().fpr.read(3).bits(),
        off.core.state().fpr.read(3).bits()
    );
}

#[test]
fn flat_base_conversion_is_recorded_once_without_translation_or_refetch() {
    let mut h = harness(&[0x0000_b183], false); // LD x3,(x1)
    const BASE: u64 = 0x8000_0000;
    h.core.reset(BASE, BASE);
    h.core.state_mut().regs[1] = BASE + TARGET;
    let turn = h.core.step_transition(true);
    let record = commit(&turn);
    assert_eq!(record.retired.pc, BASE);
    assert_eq!(record.retired.next_pc, BASE + 4);
    assert_eq!(record.effects.memory[0].guest_address, BASE + TARGET);
    assert_eq!(record.effects.memory[0].issued_address, Some(TARGET));
    assert_eq!(h.spy.lock().unwrap().fetches, 1);
    assert_eq!(h.spy.lock().unwrap().reads, 1);
}

#[test]
fn lr_sc_amo_wd_facts_include_old_new_reservation_and_suppressed_destinations() {
    for width in [PhysicalWidth::Word, PhysicalWidth::Doubleword] {
        for rd in [0, 3] {
            for typed in [false, true] {
                let code = [
                    atomic(0, width, rd, 2),
                    atomic(2, width, rd, 0),
                    atomic(3, width, rd, 2),
                    atomic(3, width, rd, 2),
                ];
                let mut h = harness(&code, typed);
                for index in 0..4 {
                    let turn = h.core.step_transition(true);
                    let record = commit(&turn);
                    let effect = &record.effects.memory[0];
                    let atomic = effect.atomic.unwrap();
                    assert_eq!(effect.width, width);
                    assert_eq!(atomic.indivisible, !typed);
                    assert!(atomic.ordering.aq && atomic.ordering.rl);
                    assert_eq!(record.effects.gpr.len(), usize::from(rd != 0));
                    match index {
                        0 => {
                            assert_eq!(atomic.kind, AtomicAccessKind::Rmw);
                            assert!(effect.read.is_some() && effect.write.is_some());
                            let expected: u64 = if width == PhysicalWidth::Word {
                                3
                            } else {
                                0x8877_6656_0000_0003
                            };
                            assert_eq!(
                                &effect.write.unwrap()[..width.bytes()],
                                &expected.to_le_bytes()[..width.bytes()]
                            );
                            if rd != 0 {
                                assert_eq!(
                                    record.effects.gpr[0].after,
                                    if width == PhysicalWidth::Word {
                                        (-2i64) as u64
                                    } else {
                                        0x8877_6655_ffff_fffe
                                    }
                                );
                            }
                        }
                        1 => {
                            assert_eq!(atomic.kind, AtomicAccessKind::LoadReserved);
                            assert!(effect.read.is_some() && effect.write.is_none());
                            assert!(record.effects.reservation.as_ref().unwrap().after.is_some());
                        }
                        2 => {
                            assert_eq!(atomic.conditional, Some(ConditionalStatus::Success));
                            assert!(effect.read.is_none() && effect.write.is_some());
                            assert!(record.effects.reservation.as_ref().unwrap().after.is_none());
                            if rd != 0 {
                                assert_eq!(record.effects.gpr[0].after, 0);
                            }
                        }
                        3 => {
                            assert_eq!(atomic.conditional, Some(ConditionalStatus::Failure));
                            assert!(effect.read.is_none() && effect.write.is_none());
                            if rd != 0 {
                                assert_eq!(record.effects.gpr[0].after, 1);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                if !typed {
                    assert_eq!(h.spy.lock().unwrap().atomics, 4);
                }
            }
        }
    }
}

#[test]
fn completed_trap_and_mret_include_precise_entry_return_effects() {
    let mut h = harness(&[0x0000_0073, 0x3020_0073], false);
    h.core.state_mut().privilege = PrivilegeMode::User;
    h.core.state_mut().csr.write(machine::MTVEC, 4).unwrap();
    let entry = h.core.step_transition(true);
    let record = trap(&entry);
    assert_eq!(record.trap.cause, ExceptionCause::EcallU);
    assert_eq!(record.trap.source_privilege, PrivilegeMode::User);
    assert_eq!(record.trap.handler_privilege, PrivilegeMode::Machine);
    assert_eq!(record.saved_pc, 0);
    assert_eq!(record.trap.vector_pc, 4);
    assert_eq!(record.trap.mtval, 0);
    assert_eq!(record.instruction_length, Some(4));
    assert_eq!(
        record
            .effects
            .csr
            .iter()
            .map(|write| write.addr)
            .collect::<Vec<_>>(),
        [
            machine::MEPC,
            machine::MCAUSE,
            machine::MTVAL,
            machine::MSTATUS
        ]
    );
    assert!(record.effects.gpr.is_empty() && record.effects.memory.is_empty());
    assert!(!entry.control.retired && entry.control.trap_entered);
    assert_eq!(entry.control.minstret_after, 0);
    let returned = h.core.step_transition(true);
    let record = commit(&returned);
    assert_eq!(record.retired.pc, 4);
    assert_eq!(record.retired.next_pc, 0);
    assert_eq!(record.retired.next_privilege, PrivilegeMode::User);
    assert_eq!(record.effects.csr[0].addr, machine::MSTATUS);
    assert_eq!(record.effects.csr[1].addr, machine::MINSTRET);
    assert!(record.effects.gpr.is_empty());
    // The first owned record did not borrow mutable core/CSR storage.
    assert_eq!(trap(&entry).trap.vector_pc, 4);
    assert_eq!(h.spy.lock().unwrap().fetches, 2);
}

#[test]
fn fetch_alignment_and_data_fault_traps_have_no_fabricated_destinations() {
    let mut misaligned = harness(&[addi(3, 0, 7)], false);
    misaligned.core.state_mut().pc = 2;
    let turn = misaligned.core.step_transition(true);
    assert_eq!(trap(&turn).instruction_length, None);
    assert_eq!(trap(&turn).trap.instruction, None);
    assert_eq!(misaligned.spy.lock().unwrap().fetches, 0);
    for width in [PhysicalWidth::Word, PhysicalWidth::Doubleword] {
        let mut h = harness(&[atomic(2, width, 3, 0), atomic(3, width, 3, 2)], false);
        h.core.step_transition(true);
        let reserved = h.core.state().reservation.clone();
        h.core.state_mut().regs[3] = 0xbeef;
        h.spy.lock().unwrap().failure = Some(Failure::Target);
        let turn = h.core.step_transition(true);
        let record = trap(&turn);
        assert_eq!(record.trap.cause, ExceptionCause::StoreAccessFault);
        assert_eq!(record.trap.mtval, TARGET);
        assert_eq!(record.saved_pc, 4);
        assert!(record.effects.gpr.is_empty() && record.effects.memory.is_empty());
        assert!(record.effects.reservation.is_none());
        assert_eq!(h.core.state().reservation, reserved);
        assert_eq!(h.core.state().regs[3], 0xbeef);
        assert_eq!(turn.control.minstret_after, 1);
        assert!(!turn.control.retired);
    }
}

#[test]
fn malformed_atomic_completion_cannot_make_subscription_panic_or_retire() {
    use ruscv_sim::physical::{
        AtomicAccess, AtomicAccessResult, AtomicResponse, PhysicalAccess, PhysicalAccessError,
        PhysicalAccessResult, PhysicalProtocolError,
    };
    struct Broken {
        short: bool,
    }
    impl PhysicalAccess for Broken {
        fn access(&mut self, _: PhysicalRequest<'_>) -> PhysicalAccessResult {
            Err(PhysicalAccessError::Protocol(
                PhysicalProtocolError::Invariant {
                    context: "unused ordinary data port".into(),
                },
            ))
        }
    }
    impl AtomicAccess for Broken {
        fn access_atomic(&mut self, request: AtomicRequest<'_>) -> AtomicAccessResult {
            Ok(if self.short {
                AtomicResponse::rmw_for(&request, &[0])
            } else {
                AtomicResponse::store_conditional_for(&request, ConditionalStatus::Success)
            })
        }
    }
    for short in [false, true] {
        for observe in [false, true] {
            let h = harness(&[atomic(0, PhysicalWidth::Doubleword, 3, 2)], false);
            let fetch = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                NativeRamBackend::new(h.memory.clone(), 0, 0x1000),
            )));
            let mut core = RiscvCore::new_with_physical_ports(
                h.memory.clone(),
                h.memory.clone(),
                fetch,
                Arc::new(Mutex::new(Broken { short })),
            );
            core.state_mut().regs[1] = TARGET;
            core.state_mut().regs[2] = 5;
            let turn = core.step_transition(observe);
            assert!(matches!(turn.outcome, StepOutcome::SimulatorFailure(_)));
            assert!(turn.observation.is_none());
            assert_eq!(core.state().pc, 0);
            assert_eq!(core.state().regs[3], 0);
            assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
        }
    }
}

#[test]
fn backend_protocol_and_unknown_failures_have_no_observation_or_guest_trap() {
    for mode in [Failure::Host, Failure::Protocol, Failure::UnknownAfterWrite] {
        let mut h = harness(&[0x0020_b023], false); // SD x2,(x1)
        h.spy.lock().unwrap().failure = Some(mode);
        let turn = h.core.step_transition(true);
        assert!(matches!(turn.outcome, StepOutcome::SimulatorFailure(_)));
        assert!(turn.observation.is_none());
        assert!(!turn.control.retired && !turn.control.trap_entered);
        assert_eq!(turn.control.minstret_after, 0);
        assert_eq!(h.core.state().pc, 0);
        assert_eq!(h.core.state().csr.read(machine::MCAUSE).unwrap(), 0);
        if matches!(mode, Failure::UnknownAfterWrite) {
            assert_eq!(h.memory.lock().unwrap().read_dword(TARGET).unwrap(), 5);
            let repeated = h.core.step_transition(true);
            assert_eq!(repeated.outcome, turn.outcome);
            assert!(repeated.observation.is_none());
            assert!(!repeated.control.instruction_attempted);
            assert_eq!(
                h.spy.lock().unwrap().writes,
                1,
                "terminal unknown is not retried"
            );
        }
    }
}

struct Sink {
    calls: usize,
    fail: bool,
    records: Vec<Observation>,
}
impl ObservationSink for Sink {
    type Error = &'static str;
    fn observe(&mut self, observation: &Observation) -> Result<(), Self::Error> {
        self.calls += 1;
        self.records.push(observation.clone());
        if self.fail {
            Err("reporting failure")
        } else {
            Ok(())
        }
    }
}

#[test]
fn delivery_failures_are_outer_errors_and_cannot_unretire_or_replace_trap() {
    let mut h = harness(&[addi(3, 0, 7), 0x0000_0073], false);
    let mut sink = Sink {
        calls: 0,
        fail: true,
        records: Vec::new(),
    };
    let retired = h.core.step_transition(true);
    assert_eq!(h.core.state().pc, 4, "state boundary precedes delivery");
    assert_eq!(h.core.state().csr.read(machine::MINSTRET).unwrap(), 1);
    assert_eq!(retired.deliver(&mut sink), Err("reporting failure"));
    assert!(retired.control.retired);
    assert_eq!(h.core.state().regs[3], 7);
    assert_eq!(h.spy.lock().unwrap().fetches, 1);
    let trapped = h.core.step_transition(true);
    assert_eq!(trapped.deliver(&mut sink), Err("reporting failure"));
    assert!(trapped.control.trap_entered && !trapped.control.retired);
    assert_eq!(h.core.state().csr.read(machine::MEPC).unwrap(), 4);
    assert_eq!(h.core.state().csr.read(machine::MINSTRET).unwrap(), 1);
    assert_eq!(sink.calls, 2);
    assert_eq!(
        sink.records,
        [retired.observation.unwrap(), trapped.observation.unwrap()]
    );
}

#[test]
fn observation_off_has_state_control_physical_parity_and_no_sink_calls() {
    let code = [
        addi(3, 0, 5),
        0x0020_b023,
        atomic(0, PhysicalWidth::Word, 3, 2),
        atomic(2, PhysicalWidth::Doubleword, 4, 0),
        atomic(3, PhysicalWidth::Doubleword, 0, 2),
        csr(1, 0, machine::MSCRATCH, 2),
        0x0000_0073,
    ];
    let mut on = harness(&code, false);
    let mut off = harness(&code, false);
    let mut sink = Sink {
        calls: 0,
        fail: false,
        records: Vec::new(),
    };
    for _ in 0..code.len() {
        let recorded = on.core.step_transition(true);
        let plain = off.core.step_transition(false);
        assert_eq!(recorded.outcome, plain.outcome);
        assert_eq!(recorded.control, plain.control);
        assert!(plain.observation.is_none());
        plain.deliver(&mut sink).unwrap();
        assert_eq!(on.core.state().regs, off.core.state().regs);
        assert_eq!(on.core.state().pc, off.core.state().pc);
        assert_eq!(on.core.state().privilege, off.core.state().privilege);
        assert_eq!(on.core.state().reservation, off.core.state().reservation);
        for addr in [
            machine::MINSTRET,
            machine::MSCRATCH,
            machine::MSTATUS,
            machine::MEPC,
            machine::MCAUSE,
            machine::MTVAL,
        ] {
            assert_eq!(
                on.core.state().csr.read(addr).unwrap(),
                off.core.state().csr.read(addr).unwrap()
            );
        }
        assert_eq!(
            on.memory.lock().unwrap().read_dword(TARGET).unwrap(),
            off.memory.lock().unwrap().read_dword(TARGET).unwrap()
        );
    }
    assert_eq!(sink.calls, 0);
    let a = on.spy.lock().unwrap();
    let b = off.spy.lock().unwrap();
    assert_eq!(
        (a.fetches, a.reads, a.writes, a.atomics),
        (b.fetches, b.reads, b.writes, b.atomics)
    );
}

#[test]
fn allocator_spy_disabled_turn_allocates_only_existing_state_staging() {
    fn measured(observe: bool) -> (usize, HartTransition) {
        let mut h = harness(&[addi(3, 0, 7)], false);
        // Warm up host/TLS initialization, then count only the next turn.
        h.core.step_transition(false);
        h.core.reset(0, 0);
        ALLOCATIONS.with(|count| count.set(0));
        COUNTING.with(|flag| flag.set(true));
        let turn = h.core.step_transition(observe);
        COUNTING.with(|flag| flag.set(false));
        (ALLOCATIONS.with(Cell::get), turn)
    }
    let (off_allocations, plain) = measured(false);
    assert!(plain.observation.is_none());
    // Existing CoreState staging clones the CSR HashMap once. There is no
    // before-state observation clone, journal allocation or materialized record.
    assert_eq!(off_allocations, 1);
    let (on_allocations, recorded) = measured(true);
    assert!(recorded.observation.is_some());
    assert_eq!(recorded.control, plain.control);
    assert!(on_allocations > off_allocations);
}
