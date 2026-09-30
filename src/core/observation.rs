//! Optional Hart-owned facts. No sink is stored in or called by the Hart.
//! Records own their data and are delivered by shared borrow after a transition.

use super::{CoreState, InstructionRetired, PrivilegeMode, StepOutcome, TrapEntered};
use crate::csr::{machine, CsrAccess};
use crate::physical::{AtomicAccessKind, AtomicOrdering, ConditionalStatus, PhysicalWidth};
use std::cell::RefCell;

/// One architectural register write, including equal-value writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterWrite {
    pub index: u8,
    pub before: u64,
    pub after: u64,
}

/// A completed memory operation, captured from the response accepted by Hart.
/// Bytes are little endian; only the first `width.bytes()` bytes are significant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryEffect {
    pub guest_address: u64,
    /// None when a typed conditional failure issued no memory operation.
    pub issued_address: Option<u64>,
    pub width: PhysicalWidth,
    pub read: Option<[u8; 8]>,
    pub write: Option<[u8; 8]>,
    /// None for ordinary accesses. Typed compatibility effects remain separate
    /// read/write operations, not falsely advertised indivisible envelopes.
    pub atomic: Option<AtomicEffect>,
}

/// Atomic operation identity and conditional completion, not ISA arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtomicEffect {
    pub kind: AtomicAccessKind,
    pub ordering: AtomicOrdering,
    pub conditional: Option<ConditionalStatus>,
    /// False for the explicitly non-conforming typed compatibility route.
    pub indivisible: bool,
}

/// Progress/PC/counter facts are present even without observation demand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlFacts {
    pub before_pc: u64,
    pub after_pc: u64,
    pub before_privilege: PrivilegeMode,
    pub after_privilege: PrivilegeMode,
    pub instruction_attempted: bool,
    pub retired: bool,
    pub trap_entered: bool,
    pub minstret_before: u64,
    pub minstret_after: u64,
}

/// Hart-owned reservation transition; snapshots are opaque physical context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationChange {
    pub before: Option<crate::isa::rv64a::ReservationSet>,
    pub after: Option<crate::isa::rv64a::ReservationSet>,
}

/// Effects of the accepted architectural transition, never a Runner snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchitecturalEffects {
    pub gpr: Vec<RegisterWrite>,
    pub fpr: Vec<RegisterWrite>,
    /// Explicit and implicit accepted CSR writes in execution order. MINSTRET
    /// is included here rather than invented by outer budget accounting.
    pub csr: Vec<CsrAccess>,
    pub fcsr: Option<(u32, u32)>,
    pub memory: Vec<MemoryEffect>,
    pub reservation: Option<ReservationChange>,
}

/// Exactly one retired instruction; this Hart currently has fixed IALIGN=32.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRecord {
    pub hart_id: u64,
    pub retired: InstructionRetired,
    pub instruction_length: u8,
    pub effects: ArchitecturalEffects,
}

/// Exactly one completed synchronous trap entry, with no instruction retirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrapRecord {
    pub hart_id: u64,
    pub trap: TrapEntered,
    pub saved_pc: u64,
    pub instruction_length: Option<u8>,
    pub effects: ArchitecturalEffects,
}

/// Materialized only on demand at an accepted completed boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    Commit(CommitRecord),
    Trap(TrapRecord),
}

/// One Hart call's authoritative result and optional observation plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HartTransition {
    pub outcome: StepOutcome,
    pub control: ControlFacts,
    pub observation: Option<Observation>,
}

/// A sink has no Hart reference and cannot run inside a Hart transition.
pub trait ObservationSink {
    type Error;
    fn observe(&mut self, observation: &Observation) -> Result<(), Self::Error>;
}

impl HartTransition {
    /// Delivery failure is an outer reporting error. The transition, control
    /// facts and immutable observation remain owned by the caller unchanged.
    pub fn deliver<S: ObservationSink + ?Sized>(&self, sink: &mut S) -> Result<(), S::Error> {
        if let Some(observation) = &self.observation {
            sink.observe(observation)?;
        }
        Ok(())
    }
}

pub(crate) type MemoryJournal = RefCell<Vec<MemoryEffect>>;

/// The only completed-transition record builder. Called after staged state has
/// been accepted. Before-state capture and journals exist only on demand.
pub(crate) fn build(
    before: &CoreState,
    after: &mut CoreState,
    outcome: &StepOutcome,
    destination: Option<u8>,
    memory: Vec<MemoryEffect>,
) -> Option<Observation> {
    if matches!(outcome, StepOutcome::SimulatorFailure(_)) {
        return None;
    }
    #[cfg(test)]
    BUILDS.with(|count| count.set(count.get() + 1));
    let gpr = destination
        .filter(|index| *index != 0)
        .map(|index| RegisterWrite {
            index,
            before: before.regs[index as usize],
            after: after.regs[index as usize],
        })
        .into_iter()
        .collect();
    let fpr = (0..32)
        .filter(|index| after.fpr.observed_writes() & (1 << index) != 0)
        .map(|index| RegisterWrite {
            index: index as u8,
            before: before.fpr.read(index).bits(),
            after: after.fpr.read(index).bits(),
        })
        .collect();
    let effects = ArchitecturalEffects {
        gpr,
        fpr,
        csr: after.csr.take_observed_writes(),
        fcsr: (before.fcsr.read() != after.fcsr.read())
            .then(|| (before.fcsr.read(), after.fcsr.read())),
        memory,
        reservation: (before.reservation != after.reservation).then(|| ReservationChange {
            before: before.reservation.clone(),
            after: after.reservation.clone(),
        }),
    };
    let hart_id = after.csr.hart_id();
    match outcome {
        StepOutcome::InstructionRetired(retired) => Some(Observation::Commit(CommitRecord {
            hart_id,
            retired: *retired,
            instruction_length: 4,
            effects,
        })),
        StepOutcome::TrapEntered(trap) => Some(Observation::Trap(TrapRecord {
            hart_id,
            trap: *trap,
            saved_pc: after.csr.architectural_value(
                if trap.handler_privilege == PrivilegeMode::Machine {
                    machine::MEPC
                } else {
                    crate::csr::supervisor::SEPC
                },
            ),
            instruction_length: trap.instruction.map(|_| 4),
            effects,
        })),
        StepOutcome::SimulatorFailure(_) => unreachable!(),
    }
}

/// Describe completed typed atomic helper effects without claiming a physical
/// envelope. Old/new bytes are the actual forwarded helper results, not a
/// speculative load. No write means conditional SC failure on this route.
pub(crate) fn typed_atomic(
    instruction: &crate::decode::DecodedInstruction,
    before: &CoreState,
    journal: &MemoryJournal,
) {
    use crate::isa::rv64a::dispatch::{decode_amo, AmoKind};
    let decoded =
        decode_amo(instruction).expect("retired typed atomic was decoded by the same dispatcher");
    let kind = match decoded.kind {
        AmoKind::Rmw(_) => AtomicAccessKind::Rmw,
        AmoKind::LoadReserved => AtomicAccessKind::LoadReserved,
        AmoKind::StoreConditional => AtomicAccessKind::StoreConditional,
    };
    let mut effects = journal.borrow_mut();
    let issued_address = effects.first().and_then(|effect| effect.issued_address);
    let read = effects.iter().find_map(|effect| effect.read);
    let write = effects.iter().find_map(|effect| effect.write);
    let address = before.regs[decoded.rs1];
    effects.clear();
    effects.push(MemoryEffect {
        guest_address: address,
        issued_address,
        width: decoded.width.physical_width(),
        read,
        write,
        atomic: Some(AtomicEffect {
            kind,
            ordering: decoded.ordering,
            conditional: (kind == AtomicAccessKind::StoreConditional).then_some(
                if write.is_some() {
                    ConditionalStatus::Success
                } else {
                    ConditionalStatus::Failure
                },
            ),
            indivisible: false,
        }),
    });
}

#[cfg(test)]
thread_local! {
    pub(crate) static BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RiscvCore;
    use crate::memory::{MemoryInterface, SimpleMemory};
    use std::sync::{Arc, Mutex};

    #[test]
    fn native_exit_store_fact_is_built_after_callback_and_delivered_after_retirement() {
        use crate::executor::SystemBus;
        use crate::peripherals::Uart16550;
        use crate::physical::ValidatedPhysicalAccess;
        use std::sync::atomic::{AtomicUsize, Ordering};
        let ram = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
        ram.lock().unwrap().write_word(0, 0x0020_b023).unwrap();
        let uart = Arc::new(Mutex::new(Uart16550::new(0x1000_0000)));
        let bus = Arc::new(Mutex::new(SystemBus::new(ram, uart, 0, 0x40)));
        let callbacks = Arc::new(AtomicUsize::new(0));
        let calls = callbacks.clone();
        BUILDS.with(|count| count.set(0));
        bus.lock().unwrap().set_htif_write_callback(move |value| {
            assert_eq!(value, 7);
            assert_eq!(
                BUILDS.with(std::cell::Cell::get),
                0,
                "no mid-transition materialized observation"
            );
            calls.fetch_add(1, Ordering::SeqCst);
        });
        let port = || {
            Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                SystemBus::physical_backend(bus.clone()),
            )))
        };
        let mut core = RiscvCore::new_with_physical_ports(bus.clone(), bus.clone(), port(), port());
        core.state_mut().regs[1] = 0x4000_8000;
        core.state_mut().regs[2] = 7;
        let completed = core.step_transition(true);
        assert_eq!(callbacks.load(Ordering::SeqCst), 1);
        assert_eq!(BUILDS.with(std::cell::Cell::get), 1);
        let Some(Observation::Commit(record)) = &completed.observation else {
            panic!("store must retire before presentation");
        };
        assert!(completed.control.retired);
        assert_eq!(record.retired.next_pc, core.state().pc);
        assert_eq!(record.effects.memory[0].write, Some(7u64.to_le_bytes()));
        assert!(record.effects.gpr.is_empty());
        struct CompletedSink;
        impl ObservationSink for CompletedSink {
            type Error = ();
            fn observe(&mut self, observation: &Observation) -> Result<(), ()> {
                assert_eq!(BUILDS.with(std::cell::Cell::get), 1);
                assert!(matches!(observation, Observation::Commit(_)));
                Ok(())
            }
        }
        completed.deliver(&mut CompletedSink).unwrap();
        assert_eq!(
            callbacks.load(Ordering::SeqCst),
            1,
            "delivery causes no extra physical write"
        );
    }

    #[test]
    fn builder_spy_proves_disabled_and_failed_turns_never_materialize_records() {
        let ram = Arc::new(Mutex::new(SimpleMemory::new(0x40)));
        ram.lock().unwrap().write_word(0, 0x0070_0193).unwrap();
        let mut core = RiscvCore::new(ram.clone(), ram.clone());
        BUILDS.with(|count| count.set(0));
        for _ in 0..8 {
            core.state_mut().pc = 0;
            assert!(core.step_transition(false).observation.is_none());
        }
        assert_eq!(BUILDS.with(std::cell::Cell::get), 0);
        core.state_mut().pc = 0;
        assert!(core.step_transition(true).observation.is_some());
        assert_eq!(BUILDS.with(std::cell::Cell::get), 1);
        ram.lock().unwrap().write_word(0, 0x0000_100f).unwrap();
        core.state_mut().pc = 0;
        assert!(core.step_transition(true).observation.is_none());
        assert_eq!(BUILDS.with(std::cell::Cell::get), 1);
        ram.lock().unwrap().write_word(0, 0xffff_ffff).unwrap();
        assert!(matches!(
            core.step_transition(true).observation,
            Some(Observation::Trap(_))
        ));
        assert_eq!(BUILDS.with(std::cell::Cell::get), 2);
    }
}
