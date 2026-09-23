//! A7 T3 Hart/PhysicalAccess connection tests.
//!
//! These tests exercise the real fetch/decode/execute path with separate raw
//! fetch and data ports.  The port sees only raw bytes; the Hart-side typed
//! adapter owns endian conversion, load extension, and FP interpretation.

use ruscv_sim::core::{
    ExceptionCause, PrivilegeMode, RiscvCore, SimulatorFailureKind, StepOutcome,
};
use ruscv_sim::csr::machine;
use ruscv_sim::executor::{SystemBus, SYSTEM_BUS_UART_BASE};
use ruscv_sim::memory::{MemoryInterface, SimpleMemory};
use ruscv_sim::peripherals::Uart16550;
use ruscv_sim::physical::{
    AccessCategory, AccessWidth, AtomicBackend, AtomicBackendResult, AtomicRequest,
    NativeRamBackend, NativeSystemBusBackend, PhysicalBackend, PhysicalBackendError,
    PhysicalBackendResult, PhysicalRequest, PhysicalResponse, PhysicalTargetRejectionReason,
    ValidatedPhysicalAccess,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    descriptor: ruscv_sim::PhysicalRequestDescriptor,
    payload: Option<Vec<u8>>,
}

#[derive(Debug)]
struct TraceBackend {
    inner: NativeRamBackend,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl TraceBackend {
    fn new(memory: Arc<Mutex<SimpleMemory>>, size: usize, calls: Arc<Mutex<Vec<Call>>>) -> Self {
        Self {
            inner: NativeRamBackend::new(memory, 0, size),
            calls,
        }
    }
}

impl PhysicalBackend for TraceBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls.lock().unwrap().push(Call {
            descriptor: request.descriptor(),
            payload: request.payload().map(ToOwned::to_owned),
        });
        self.inner.transact(request)
    }
}

impl AtomicBackend for TraceBackend {
    /// Forward atomics to the wrapped RAM target; the call is recorded so
    /// the atomic envelope never travels a second domain.
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        self.inner.transact_atomic(request)
    }
}

#[derive(Debug, Clone)]
enum Planned {
    Read(Vec<u8>),
    Error(PhysicalBackendError),
}

#[derive(Debug)]
struct PlannedBackend {
    plans: VecDeque<Planned>,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl PlannedBackend {
    fn new(plans: impl IntoIterator<Item = Planned>, calls: Arc<Mutex<Vec<Call>>>) -> Self {
        Self {
            plans: plans.into_iter().collect(),
            calls,
        }
    }
}

impl PhysicalBackend for PlannedBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        self.calls.lock().unwrap().push(Call {
            descriptor: request.descriptor(),
            payload: request.payload().map(ToOwned::to_owned),
        });
        match self.plans.pop_front().expect("test planned every access") {
            Planned::Read(bytes) => Ok(PhysicalResponse::read_for(request, &bytes)),
            Planned::Error(error) => Err(error),
        }
    }
}

impl AtomicBackend for PlannedBackend {
    /// This fixture only serves planned ordinary accesses; an atomic
    /// envelope is a target rejection, not a planned effect.
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        Err(PhysicalBackendError::target(
            PhysicalTargetRejectionReason::UnsupportedCategory,
            format!("planned backend received unexpected atomic {request:?}"),
        ))
    }
}

#[derive(Debug)]
struct RepeatingFetchBackend {
    instruction: u32,
    calls: Arc<Mutex<usize>>,
}

impl PhysicalBackend for RepeatingFetchBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        assert_eq!(request.category(), AccessCategory::Fetch);
        *self.calls.lock().unwrap() += 1;
        Ok(PhysicalResponse::read_for(
            request,
            &self.instruction.to_le_bytes(),
        ))
    }
}

#[derive(Debug)]
struct UnknownAfterEffectBackend {
    calls: Arc<Mutex<usize>>,
    effects: Arc<Mutex<usize>>,
}

impl PhysicalBackend for UnknownAfterEffectBackend {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        assert_eq!(request.category(), AccessCategory::DataWrite);
        *self.calls.lock().unwrap() += 1;
        *self.effects.lock().unwrap() += 1;
        Err(PhysicalBackendError::unknown(
            "injected side effect may have committed",
        ))
    }
}

impl AtomicBackend for UnknownAfterEffectBackend {
    /// This fixture only serves ordinary writes; an atomic envelope is a
    /// target rejection, not an unknown completion.
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        Err(PhysicalBackendError::target(
            PhysicalTargetRejectionReason::UnsupportedCategory,
            format!("unknown-after-effect backend received unexpected atomic {request:?}"),
        ))
    }
}

fn addi(rd: u8, rs1: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20) | ((rs1 as u32) << 15) | ((rd as u32) << 7) | 0x13
}

fn load(rd: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x03
}

fn load_fp(rd: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    ((immediate as u32 & 0xfff) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((rd as u32) << 7)
        | 0x07
}

fn store(rs2: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    assert!((-2048..=2047).contains(&immediate));
    let immediate = immediate as u32 & 0xfff;
    ((immediate >> 5) << 25)
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((immediate & 0x1f) << 7)
        | 0x23
}

fn store_fp(rs2: u8, rs1: u8, funct3: u8, immediate: i32) -> u32 {
    ((immediate as u32 & 0xfff) >> 5) << 25
        | ((rs2 as u32) << 20)
        | ((rs1 as u32) << 15)
        | ((funct3 as u32) << 12)
        | ((immediate as u32 & 0x1f) << 7)
        | 0x27
}

fn typed_handle<M: MemoryInterface + Send + Sync + 'static>(
    memory: Arc<Mutex<M>>,
) -> Arc<Mutex<dyn MemoryInterface + Send + Sync>> {
    memory
}

fn traced_core(
    memory: Arc<Mutex<SimpleMemory>>,
    size: usize,
    calls: Arc<Mutex<Vec<Call>>>,
) -> RiscvCore {
    let instruction = typed_handle(memory.clone());
    let data = typed_handle(memory.clone());
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(TraceBackend::new(
        memory.clone(),
        size,
        calls.clone(),
    ))));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(TraceBackend::new(
        memory, size, calls,
    ))));
    RiscvCore::new_with_physical_ports(instruction, data, instruction_port, data_port)
}

fn mtvec(core: &mut RiscvCore) {
    core.state_mut().csr.write(machine::MTVEC, 0x40).unwrap();
}

fn native_bus_core(ram_base: u64, ram_size: usize) -> (RiscvCore, Arc<Mutex<SimpleMemory>>) {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(ram_size)));
    let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart,
        ram_base,
        ram_size,
    )));
    let instruction_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let data_port = Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
        NativeSystemBusBackend::new(bus.clone()),
    )));
    let core = RiscvCore::new_with_physical_ports(
        typed_handle(bus.clone()),
        typed_handle(bus),
        instruction_port,
        data_port,
    );
    (core, ram)
}

fn assert_access_trap(outcome: StepOutcome, cause: ExceptionCause, mtval: u64, core: &RiscvCore) {
    let StepOutcome::TrapEntered(trap) = outcome else {
        panic!("expected architectural access trap")
    };
    assert_eq!(trap.cause, cause);
    assert_eq!(trap.mtval, mtval);
    assert_eq!(core.state().pc, 0x40);
    assert_eq!(core.state().privilege, PrivilegeMode::Machine);
}

#[test]
fn real_fetch_integer_and_fp_accesses_use_raw_ports_and_one_ram() {
    let size = 0x100;
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    let calls = Arc::new(Mutex::new(Vec::new()));
    {
        let mut ram = memory.lock().unwrap();
        let program = [
            addi(1, 0, 0x80),
            addi(2, 0, -128),
            store(2, 1, 0, 0), // SB
            load(3, 1, 0, 0),  // LB
            load(4, 1, 4, 0),  // LBU
            addi(1, 1, 4),
            load_fp(1, 1, 2, 0),  // FLW
            store_fp(1, 1, 2, 4), // FSW
            addi(1, 1, 4),
            load_fp(2, 1, 3, 8),  // FLD
            store_fp(2, 1, 3, 0), // FSD
        ];
        for (index, instruction) in program.into_iter().enumerate() {
            ram.write_word(index as u64 * 4, instruction).unwrap();
        }
        ram.write_word(0x84, 0x3fc0_0000).unwrap();
        ram.write_dword(0x90, 0x4009_21fb_5444_2d18).unwrap();
    }

    let mut core = traced_core(memory.clone(), size, calls.clone());
    core.reset(0, 0);
    for _ in 0..11 {
        assert!(matches!(
            core.step_outcome(),
            StepOutcome::InstructionRetired(_)
        ));
    }

    assert_eq!(core.state().regs[3], u64::MAX - 127, "LB sign extension");
    assert_eq!(core.state().regs[4], 0x80, "LBU zero extension");
    assert_eq!(core.state().fpr.read(1).get().to_bits(), 0x3fc0_0000);
    assert_eq!(
        core.state().fpr.read(2).bits(),
        0x4009_21fb_5444_2d18,
        "FLD preserves raw FP bits"
    );

    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        18,
        "eleven fetches plus seven ordinary transfers"
    );
    let fetches = calls
        .iter()
        .filter(|call| call.descriptor.category == AccessCategory::Fetch)
        .collect::<Vec<_>>();
    assert_eq!(fetches.len(), 11);
    assert!(fetches.iter().enumerate().all(|(index, call)| {
        call.descriptor.width == AccessWidth::Word && call.descriptor.paddr == index as u64 * 4
    }));
    let data = calls
        .iter()
        .filter(|call| call.descriptor.category != AccessCategory::Fetch)
        .map(|call| {
            (
                call.descriptor.category,
                call.descriptor.width,
                call.descriptor.paddr,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        data,
        vec![
            (AccessCategory::DataWrite, AccessWidth::Byte, 0x80),
            (AccessCategory::DataRead, AccessWidth::Byte, 0x80),
            (AccessCategory::DataRead, AccessWidth::Byte, 0x80),
            (AccessCategory::DataRead, AccessWidth::Word, 0x84),
            (AccessCategory::DataWrite, AccessWidth::Word, 0x88),
            (AccessCategory::DataRead, AccessWidth::Doubleword, 0x90),
            (AccessCategory::DataWrite, AccessWidth::Doubleword, 0x88),
        ],
    );
    assert_eq!(
        calls
            .iter()
            .find(|call| {
                call.descriptor.category == AccessCategory::DataWrite
                    && call.descriptor.width == AccessWidth::Doubleword
            })
            .unwrap()
            .payload,
        Some(0x4009_21fb_5444_2d18u64.to_le_bytes().to_vec())
    );
}

#[test]
fn hart_alignment_and_misaligned_storage_offset_issue_no_data_request() {
    let size = 0x40;
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    let calls = Arc::new(Mutex::new(Vec::new()));
    memory
        .lock()
        .unwrap()
        .write_word(0, load(5, 1, 3, 0))
        .unwrap();
    let mut core = traced_core(memory.clone(), size, calls.clone());
    core.reset(0, 0);
    mtvec(&mut core);
    core.state_mut().regs[1] = 3;
    let outcome = core.step_outcome();
    assert_access_trap(outcome, ExceptionCause::LoadAddressMisaligned, 3, &core);
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        calls.lock().unwrap()[0].descriptor.category,
        AccessCategory::Fetch
    );

    calls.lock().unwrap().clear();
    core.state_mut().pc = 2;
    let outcome = core.step_outcome();
    assert_access_trap(
        outcome,
        ExceptionCause::InstructionAddressMisaligned,
        2,
        &core,
    );
    assert!(
        calls.lock().unwrap().is_empty(),
        "misaligned fetch is pre-port"
    );

    // Guest address base+4 is aligned for LD, but one checked base
    // subtraction produces an offset that the flat typed storage historically
    // rejected.  The raw target is not allowed to grant that old access.
    let base = 0x8000_0004;
    let memory = Arc::new(Mutex::new(SimpleMemory::new(size)));
    let calls = Arc::new(Mutex::new(Vec::new()));
    memory
        .lock()
        .unwrap()
        .write_word(4, load(5, 1, 3, 0))
        .unwrap();
    let mut core = traced_core(memory, size, calls.clone());
    core.reset(base + 4, base);
    mtvec(&mut core);
    core.state_mut().regs[1] = base + 4;
    let outcome = core.step_outcome();
    assert_access_trap(outcome, ExceptionCause::LoadAccessFault, base + 4, &core);
    assert_eq!(calls.lock().unwrap().len(), 1, "fetch only");
}

#[test]
fn native_non_aligned_storage_offsets_retain_fetch_load_and_store_faults() {
    // The guest PC is aligned at base+2, but the native RAM storage offset is
    // two.  The old typed SystemBus route rejected this fetch before reading
    // the instruction.
    let fetch_base = 0x8000_0002;
    let (mut fetch_core, fetch_ram) = native_bus_core(fetch_base, 0x40);
    fetch_ram
        .lock()
        .unwrap()
        .write_bytes(2, &addi(5, 0, 7).to_le_bytes())
        .unwrap();
    fetch_core.set_physical_storage_alignment(fetch_base, 0x40);
    fetch_core.reset(fetch_base + 2, 0);
    mtvec(&mut fetch_core);
    assert_access_trap(
        fetch_core.step_outcome(),
        ExceptionCause::InstructionAccessFault,
        fetch_base + 2,
        &fetch_core,
    );
    assert_eq!(fetch_core.state().regs[5], 0);

    // With a base four-bytes aligned but not eight-bytes aligned, guest LD/SD
    // addresses remain architecturally aligned while their RAM offsets do not.
    // The adapter rejects those offsets before the native bus can read/write.
    let ram_base = 0x8000_0004;
    let (mut load_core, load_ram) = native_bus_core(ram_base, 0x40);
    load_ram
        .lock()
        .unwrap()
        .write_bytes(0, &load(5, 1, 3, 0).to_le_bytes())
        .unwrap();
    let load_bytes = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    load_ram
        .lock()
        .unwrap()
        .write_bytes(4, &load_bytes)
        .unwrap();
    load_core.set_physical_storage_alignment(ram_base, 0x40);
    load_core.reset(ram_base, 0);
    mtvec(&mut load_core);
    load_core.state_mut().regs[1] = ram_base + 4;
    load_core.state_mut().regs[5] = 0xfeed_face_cafe_babe;
    assert_access_trap(
        load_core.step_outcome(),
        ExceptionCause::LoadAccessFault,
        ram_base + 4,
        &load_core,
    );
    assert_eq!(load_core.state().regs[5], 0xfeed_face_cafe_babe);
    assert_eq!(
        load_ram.lock().unwrap().read_bytes(4, 8).unwrap(),
        load_bytes
    );

    let (mut store_core, store_ram) = native_bus_core(ram_base, 0x40);
    store_ram
        .lock()
        .unwrap()
        .write_bytes(0, &store(2, 1, 3, 0).to_le_bytes())
        .unwrap();
    store_ram
        .lock()
        .unwrap()
        .write_bytes(4, &load_bytes)
        .unwrap();
    let before = store_ram.lock().unwrap().read_bytes(4, 8).unwrap();
    store_core.set_physical_storage_alignment(ram_base, 0x40);
    store_core.reset(ram_base, 0);
    mtvec(&mut store_core);
    store_core.state_mut().regs[1] = ram_base + 4;
    store_core.state_mut().regs[2] = 0x0123_4567_89ab_cdef;
    assert_access_trap(
        store_core.step_outcome(),
        ExceptionCause::StoreAccessFault,
        ram_base + 4,
        &store_core,
    );
    assert_eq!(store_ram.lock().unwrap().read_bytes(4, 8).unwrap(), before);
}

#[test]
fn unknown_completion_is_terminal_and_does_not_retry_the_physical_domain() {
    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    let fetch_calls = Arc::new(Mutex::new(0));
    let data_calls = Arc::new(Mutex::new(0));
    let effects = Arc::new(Mutex::new(0));
    let mut core = RiscvCore::new_with_physical_ports(
        typed_handle(memory.clone()),
        typed_handle(memory),
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            RepeatingFetchBackend {
                instruction: store(2, 1, 0, 0),
                calls: fetch_calls.clone(),
            },
        ))),
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            UnknownAfterEffectBackend {
                calls: data_calls.clone(),
                effects: effects.clone(),
            },
        ))),
    );
    core.reset(0, 0);
    core.state_mut().regs[1] = 0x80;
    core.state_mut().regs[2] = 0xaa;

    let first = core.step_outcome();
    assert!(matches!(
        &first,
        StepOutcome::SimulatorFailure(failure)
            if failure.kind == SimulatorFailureKind::HostBackend
                && failure.message.contains("unresolved physical completion")
    ));
    assert!(core.unresolved_physical_access().is_some());
    assert_eq!(core.state().pc, 0);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    assert_eq!(*fetch_calls.lock().unwrap(), 1);
    assert_eq!(*data_calls.lock().unwrap(), 1);
    assert_eq!(*effects.lock().unwrap(), 1);

    let second = core.step_outcome();
    assert_eq!(
        second, first,
        "terminal unknown state is replayed, not retried"
    );
    assert_eq!(*fetch_calls.lock().unwrap(), 1);
    assert_eq!(*data_calls.lock().unwrap(), 1);
    assert_eq!(*effects.lock().unwrap(), 1);
    assert_eq!(core.state().pc, 0);
    assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
}

#[test]
fn target_rejections_map_to_fetch_load_store_causes_with_original_mtval() {
    let target_error = || {
        PhysicalBackendError::target(
            PhysicalTargetRejectionReason::TargetError,
            "injected target refusal",
        )
    };

    let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
    let fetch_calls = Arc::new(Mutex::new(Vec::new()));
    let data_calls = Arc::new(Mutex::new(Vec::new()));
    let mut fetch_fault = RiscvCore::new_with_physical_ports(
        typed_handle(memory.clone()),
        typed_handle(memory.clone()),
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            PlannedBackend::new([Planned::Error(target_error())], fetch_calls),
        ))),
        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
            PlannedBackend::new([], data_calls),
        ))),
    );
    fetch_fault.reset(0x100, 0);
    mtvec(&mut fetch_fault);
    assert_access_trap(
        fetch_fault.step_outcome(),
        ExceptionCause::InstructionAccessFault,
        0x100,
        &fetch_fault,
    );
    assert_eq!(fetch_fault.state().csr.read(machine::MINSTRET).unwrap(), 0);

    let scenarios = [
        (load(5, 1, 3, 0), ExceptionCause::LoadAccessFault, 0x80),
        (store(2, 1, 3, 0), ExceptionCause::StoreAccessFault, 0x80),
    ];
    for (instruction, cause, address) in scenarios {
        let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
        let fetch_calls = Arc::new(Mutex::new(Vec::new()));
        let data_calls = Arc::new(Mutex::new(Vec::new()));
        let mut core = RiscvCore::new_with_physical_ports(
            typed_handle(memory.clone()),
            typed_handle(memory),
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                PlannedBackend::new(
                    [Planned::Read(instruction.to_le_bytes().to_vec())],
                    fetch_calls,
                ),
            ))),
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                PlannedBackend::new([Planned::Error(target_error())], data_calls),
            ))),
        );
        core.reset(0, 0);
        mtvec(&mut core);
        core.state_mut().regs[1] = address;
        core.state_mut().regs[2] = 0xdead_beef_cafe_babe;
        core.state_mut().regs[5] = 0xfeed_face_cafe_babe;
        assert_access_trap(core.step_outcome(), cause, address, &core);
        assert_eq!(core.state().regs[5], 0xfeed_face_cafe_babe);
        assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    }
}

#[test]
fn host_protocol_and_unknown_failures_are_simulator_failures_not_traps() {
    for failure in [
        PhysicalBackendError::host("host unavailable"),
        PhysicalBackendError::protocol("malformed completion"),
        PhysicalBackendError::unknown("effect may have committed"),
    ] {
        let memory = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
        let fetch_calls = Arc::new(Mutex::new(Vec::new()));
        let data_calls = Arc::new(Mutex::new(Vec::new()));
        let mut core = RiscvCore::new_with_physical_ports(
            typed_handle(memory.clone()),
            typed_handle(memory),
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                PlannedBackend::new(
                    [Planned::Read(load(5, 1, 3, 0).to_le_bytes().to_vec())],
                    fetch_calls,
                ),
            ))),
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                PlannedBackend::new([Planned::Error(failure)], data_calls),
            ))),
        );
        core.reset(0, 0);
        mtvec(&mut core);
        core.state_mut().regs[1] = 0x80;
        core.state_mut().regs[5] = 0x1234;
        let outcome = core.step_outcome();
        assert!(matches!(
            outcome,
            StepOutcome::SimulatorFailure(failure)
                if failure.kind == SimulatorFailureKind::HostBackend
        ));
        assert_eq!(core.state().pc, 0);
        assert_eq!(core.state().regs[5], 0x1234);
        assert_eq!(core.state().csr.read(machine::MINSTRET).unwrap(), 0);
    }
}
