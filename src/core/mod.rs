//! RISC-V RV64I core module
//!
//! Implements RISC-V processor core fetch-decode-execute cycle

pub mod commits;
mod debug_mode;
pub mod observation;
mod observed_memory;
mod translation;
pub mod trap;
use crate::csr::{machine, CsrFile};
use crate::decode::{DecodeError, DecodedInstruction, InstructionDecoder, Opcode};
use crate::execute::{ExecuteError, Executor};
use crate::fpu::{Fcsr, FpuRegisterFile};
use crate::isa::rv64a::ReservationSet;
use crate::memory::{MemoryError, MemoryInterface, SimpleMemory};
use crate::physical::{
    AtomicAccessError, AtomicRequest, AtomicResponse, PhysicalAccess, PhysicalAccessError,
    PhysicalAccessKind, PhysicalDataAccess, PhysicalRequest, PhysicalWidth,
};
use crate::tlm::TlmInterface;
use anyhow::Result;
pub use debug_mode::DebugCause;
use observation::{AtomicEffect, ControlFacts, HartTransition, MemoryEffect, MemoryJournal};
use observed_memory::ObservedMemory;
use std::cell::RefCell;
use std::sync::{Arc, Mutex};
pub use trap::{
    ExceptionCause, InterruptCause, Trap, TrapContext, TrapDelegation, TrapEntryError, TrapHandler,
};

/// RISC-V privilege mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivilegeMode {
    User = 0,
    Supervisor = 1,
    Machine = 3,
}

/// Continuation selected for a completed synchronous trap boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapContinuationPolicy {
    /// Consume the trap fact at the Runner boundary and execute the guest
    /// handler from the selected trap vector on the next turn.
    ContinueToGuestHandler,
}

/// Facts for one retired guest instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstructionRetired {
    /// Architectural PC before the instruction.
    pub pc: u64,
    /// Architectural PC after retirement.
    pub next_pc: u64,
    /// Instruction word fetched by the Hart.
    pub instruction: u32,
    /// Privilege in which the instruction started.
    pub privilege: PrivilegeMode,
    /// Privilege after retirement (different for a legal `MRET`).
    pub next_privilege: PrivilegeMode,
    /// Authoritative retirement counter after this boundary.
    pub minstret: u64,
    /// Whether this instruction explicitly wrote `minstret`.
    pub explicit_minstret_write: bool,
}

/// Facts for one completed trap entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapEntered {
    /// Architectural exception or interrupt cause.
    pub cause: Trap,
    /// PC of the faulting or interrupted instruction, or the faulting fetch.
    pub faulting_pc: u64,
    /// Diagnostic value committed to `mtval`.
    pub mtval: u64,
    /// Trap-vector target after entry.
    pub vector_pc: u64,
    /// Privilege before trap entry.
    pub source_privilege: PrivilegeMode,
    /// Privilege selected to handle the trap.
    pub handler_privilege: PrivilegeMode,
    /// Explicit continuation policy consumed by the Runner.
    pub continuation: TrapContinuationPolicy,
    /// Instruction word when fetch/decode made it available.
    pub instruction: Option<u32>,
}

/// Typed simulator-side failure.  This is never converted into guest trap
/// state; the Runner reports it as an execution error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimulatorFailureKind {
    /// A host lock, allocation, or backend resource prevented completion.
    HostBackend,
    /// The encoding is legal for the profile but this implementation does not
    /// support that operation yet.
    UnsupportedLegalInstruction,
    /// The Hart or adapter violated an internal invariant.
    Invariant,
}

/// Facts for a step that could not complete architecturally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulatorFailure {
    /// PC at the start of the failed turn.
    pub pc: u64,
    /// Typed failure class.
    pub kind: SimulatorFailureKind,
    /// Diagnostic context for the Runner/frontend.
    pub message: String,
}

/// One and only one semantic Hart-step outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// One guest instruction retired exactly once.
    InstructionRetired(InstructionRetired),
    /// Architectural trap entry completed; no instruction retired.
    TrapEntered(TrapEntered),
    /// The Hart could not complete an architectural operation.
    SimulatorFailure(SimulatorFailure),
    /// A retired WFI is waiting. No instruction retired on this call.
    Waiting,
    /// The Hart is in Debug Mode.
    ///
    /// `completed_turn` is true only when this call retired an instruction or
    /// took a trap and then entered (single-step). A `haltreq` entry, an
    /// `ebreak` entry, and a call that finds the Hart already halted are not
    /// completed turns: `mtime`, `mcycle`, `minstret`, and
    /// `ExecutionResult.cycles` stay put (Debug 1.0 `dcsr.stopcount` /
    /// `stoptime`, privileged §3.1.10, contract D9).
    DebugHalted {
        cause: DebugCause,
        /// This call completed one Hart turn before the entry.
        completed_turn: bool,
    },
}

/// A debugger physical access failed without trapping.
///
/// Debug Spec 1.0 §3.7.1.3 maps an M-mode exception to `abstractcs.cmderr` 3.
/// `Misaligned` is the failure an M-mode load or store would report as an
/// address-misaligned exception (contract D15). `AccessFault` is every other
/// rejection of that M-mode access, including an unmapped address, a missing
/// data port, and a width the target refuses. `Other` is a simulator fault
/// (Debug Spec 1.0 §3.14 `cmderr` 7): a backend, protocol, or unknown
/// completion, or a poisoned data-port lock. Those are not M-mode exceptions.
/// An unknown completion may already have changed target state, which
/// `cmderr` 3 would report as not having happened.
/// Result of one program-buffer instruction. Not a guest retirement or trap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugExecOutcome {
    /// The instruction completed.
    Completed,
    /// EBREAK ended the buffer successfully.
    Ebreak,
    /// An exception stopped the buffer (`cmderr=3`). Guest trap state is unchanged.
    Exception,
    /// Host or backend failure, including unknown physical completion.
    Failure(SimulatorFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugMemFault {
    /// `addr` is not a multiple of `size`. Checked before the data port.
    Misaligned,
    /// The physical data port is absent or the target rejected the access.
    AccessFault,
    /// The host could not complete the access, or cannot say whether it did.
    Other,
}

/// Descriptive aliases for callers that name the boundary after the Hart/core.
pub type HartStepOutcome = StepOutcome;
pub type CoreStepOutcome = StepOutcome;

/// RISC-V core state (RV64I)
#[derive(Debug, Clone)]
pub struct CoreState {
    /// 程序计数器 (64-bit for RV64)
    pub pc: u64,
    /// General purpose registers x0-x31 (64-bit for RV64)
    pub regs: [u64; 32],
    /// privilege mode
    pub privilege: PrivilegeMode,
    /// CSR file
    pub csr: CsrFile,
    /// FPU register file (f0-f31)
    pub fpr: FpuRegisterFile,
    /// FCSR (Floating-Point Control and Status Register)
    pub fcsr: Fcsr,
    /// 机器状态寄存器 (简化版) - deprecated, use csr field
    pub mstatus: u64,
    /// 异常程序计数器 - deprecated, use csr field
    pub mepc: u64,
    /// 异常原因 - deprecated, use csr field
    pub mcause: u64,
    /// 异常值 - deprecated, use csr field
    pub mtval: u64,
    /// Track if last instruction was a taken branch (used to skip pc += 4)
    pub branch_taken: bool,
    /// The Hart's single atomic reservation record (dev-plan §5.4, C16/C23).
    ///
    /// A successful LR replaces it, an executed SC consumes it, and reset or
    /// image reload clears it because both install a fresh `CoreState`.  The
    /// record keys the exact reserved byte span by the address issued at the
    /// Hart's access route plus the LR-time committed-write snapshot.
    pub reservation: Option<ReservationSet>,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            pc: 0x0000_0000_0000_0000,
            regs: [0; 32],
            privilege: PrivilegeMode::Machine,
            csr: CsrFile::default(),
            fpr: FpuRegisterFile::new(),
            fcsr: Fcsr::new(),
            mstatus: 0,
            mepc: 0,
            mcause: 0,
            mtval: 0,
            branch_taken: false,
            reservation: None,
        }
    }
}

/// RISC-V core
pub struct RiscvCore {
    /// core state
    state: CoreState,
    /// 指令存储器
    instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    /// 数据存储器
    data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    /// Instruction decoder
    decoder: InstructionDecoder,
    /// Executor
    executor: Executor,
    /// Optional raw physical fetch port.  It is separate from the legacy
    /// instruction-memory handle so old callers retain their supplied object.
    instruction_access: Option<SharedPhysicalAccess>,
    /// Optional validated data port for ordinary integer/FP accesses and for
    /// AMO/LR/SC atomic envelopes.  One port object serves both categories
    /// (dev-plan §7.2); cores without it keep the labeled non-conforming
    /// typed adapter route.
    data_access: Option<SharedDataAccess>,
    /// Optional native RAM range whose storage-offset alignment must retain
    /// the legacy typed-memory rejection.  This is set by the native facade;
    /// generic raw targets remain free to accept unaligned physical spans.
    physical_storage_alignment: Option<(u64, usize)>,
    /// A completion whose effect cannot be established is terminal for this
    /// Hart until the host explicitly resolves/reconstructs the physical
    /// domain.  Keeping the fact here prevents an accidental retry.
    unresolved_physical_access: Option<SimulatorFailure>,
    /// Internal per-call demand/journals, never an external observer callback.
    observation_demand: bool,
    memory_journal: Option<MemoryJournal>,
    gpr_destination: Option<u8>,
    /// Machine-mode synchronous trap handler.
    trap_handler: TrapHandler,
    /// TLM interface（可选）
    tlm_interface: Option<Arc<Mutex<dyn TlmInterface>>>,
    /// Base address for virtual address translation (loaded ELF base address)
    base_addr: u64,
    /// Verbose output flag
    verbose: bool,
    /// A legal WFI has retired and `mip & mie` was zero at that sample.
    /// Privileged specification 20250508 §3.3.3. The next pre-fetch sample
    /// is the wake check.
    wfi_wait: bool,
    debug: debug_mode::HartDebug,
    /// Program-buffer opcode being executed in Debug Mode. While set, the
    /// instruction source is this word, guests traps and counters are bypassed.
    debug_word: Option<u32>,
    /// After a successful fetch, set `haltreq` and keep going. The bit is
    /// sampled only at the next `execute_turn` boundary. A check between
    /// the fetch and retirement would abandon the instruction.
    #[cfg(test)]
    inject_haltreq_after_fetch: bool,
}

/// Memory adapter for virtual to physical address translation
///
/// This adapter wraps a MemoryInterface and translates virtual addresses
/// to physical addresses before performing memory operations.
pub struct MemoryAdapter<'a> {
    /// Inner memory interface (mutable reference)
    mem: &'a mut dyn MemoryInterface,
    /// Base address for VA -> PA translation
    base_addr: u64,
}

impl<'a> MemoryAdapter<'a> {
    /// Create new memory adapter
    pub fn new(mem: &'a mut dyn MemoryInterface, base_addr: u64) -> Self {
        Self { mem, base_addr }
    }

    /// Convert virtual address to physical address
    #[inline]
    fn va_to_pa(&self, va: u64) -> Result<u64, MemoryError> {
        va.checked_sub(self.base_addr)
            .ok_or(MemoryError::InvalidAddress(va))
    }
}

impl MemoryInterface for MemoryAdapter<'_> {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_dword(self.va_to_pa(addr)?)
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        self.mem.read_word(self.va_to_pa(addr)?)
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        self.mem.read_half(self.va_to_pa(addr)?)
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        self.mem.read_byte(self.va_to_pa(addr)?)
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_word_zext(self.va_to_pa(addr)?)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_half_zext(self.va_to_pa(addr)?)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_byte_zext(self.va_to_pa(addr)?)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_word_sext(self.va_to_pa(addr)?)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_half_sext(self.va_to_pa(addr)?)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        self.mem.read_byte_sext(self.va_to_pa(addr)?)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        self.mem.write_dword(self.va_to_pa(addr)?, value)
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        self.mem.write_word(self.va_to_pa(addr)?, value)
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        self.mem.write_half(self.va_to_pa(addr)?, value)
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        self.mem.write_byte(self.va_to_pa(addr)?, value)
    }

    fn size(&self) -> usize {
        self.mem.size()
    }
}

/// The compatibility typed view used by the unchanged legacy executor path.
///
/// In particular, AMO/LR/SC helpers receive this view while the core holds the
/// complete outer `data_mem` mutex guard.  It performs the historical checked
/// base subtraction and typed calls; it must not be replaced with a raw-port
/// call in the middle of a legacy helper.
pub type LegacyTypedMemoryAdapter<'a> = MemoryAdapter<'a>;

/// Shared ownership form used to connect a Hart to a validated physical port.
/// The port mutex is the outer lock for raw transactions; legacy typed helpers
/// continue to use their independent `MemoryInterface` mutex.
pub type SharedPhysicalAccess = Arc<Mutex<dyn PhysicalAccess + Send>>;

/// Shared ownership form for the Hart's one validated data port.
///
/// The data port serves ordinary loads/stores and AMO/LR/SC atomic envelopes
/// through a single port object, so the raw/atomic path locks exactly one
/// port handle for the complete operation (dev-plan §7.2): there is no
/// second port, RAM, or lock domain for atomics.
pub type SharedDataAccess = Arc<Mutex<dyn PhysicalDataAccess + Send>>;

/// Hart-side typed interpretation of the raw non-atomic physical port.
///
/// This view owns only address conversion, little-endian integer decoding,
/// load extension, and FP bit transport.  The target receives raw bytes and
/// never performs ISA interpretation.  The caller must hold the port's mutex
/// for the complete view lifetime; no target lock is reacquired by this type.
pub struct PhysicalMemoryAdapter<'a> {
    // `MemoryInterface` intentionally exposes reads as `&self`, while a
    // physical port uses `&mut self` to serialize a transaction.  RefCell is
    // only an adapter-local reborrow: the caller already owns the port mutex,
    // so no second mutex is acquired here.
    port: RefCell<&'a mut dyn PhysicalAccess>,
    base_addr: u64,
    /// Optional target range whose physical address is a legacy storage
    /// offset.  Outside this range, the generic physical address alignment
    /// rule remains in force without imposing RAM semantics on devices.
    storage_alignment: Option<(u64, usize)>,
    /// Page delta (translated - virtual) applied to the guest base before
    /// the guest-to-issued conversion while Sv39 translation is active.
    /// The register file stays architectural; only the address path moves.
    translation_delta: Option<u64>,
}

impl<'a> PhysicalMemoryAdapter<'a> {
    /// Creates a Hart typed view over one already-selected physical port.
    pub fn new(port: &'a mut dyn PhysicalAccess, base_addr: u64) -> Self {
        Self {
            port: RefCell::new(port),
            base_addr,
            storage_alignment: None,
            translation_delta: None,
        }
    }

    /// Creates a translated view: every guest address is shifted by
    /// `translation_delta` before the usual base conversion, so the helper's
    /// architectural effective address issues at the translated
    /// guest-physical address.
    pub fn new_translated(
        port: &'a mut dyn PhysicalAccess,
        base_addr: u64,
        storage_alignment: Option<(u64, usize)>,
        translation_delta: u64,
    ) -> Self {
        Self {
            port: RefCell::new(port),
            base_addr,
            storage_alignment,
            translation_delta: Some(translation_delta),
        }
    }

    /// Creates a Hart view that also preserves typed alignment for one
    /// target/storage range.  The range is an opt-in compatibility policy;
    /// direct raw backends remain allowed to transfer unaligned spans.
    pub fn new_with_storage_alignment(
        port: &'a mut dyn PhysicalAccess,
        base_addr: u64,
        storage_base: u64,
        storage_size: usize,
    ) -> Self {
        Self {
            port: RefCell::new(port),
            base_addr,
            storage_alignment: Some((storage_base, storage_size)),
            translation_delta: None,
        }
    }

    /// Preserve the flat backend's historical typed-offset alignment rule
    /// after the one guest-to-storage conversion.  This is a target/storage
    /// rejection, not a new Hart alignment check; the Hart has already checked
    /// the guest address before this adapter is entered.  The conversion is
    /// shared with the atomic view through [`hart_issued_paddr`]; a staged
    /// Sv39 translation delta shifts the guest base first.
    fn checked_paddr(&self, va: u64, width: PhysicalWidth) -> Result<u64, MemoryError> {
        let guest = self
            .translation_delta
            .map_or(va, |delta| va.wrapping_add(delta));
        hart_issued_paddr(guest, self.base_addr, self.storage_alignment, width)
    }

    fn map_error(error: PhysicalAccessError) -> MemoryError {
        match error {
            PhysicalAccessError::TargetRejected(rejection) => {
                MemoryError::InvalidAddress(rejection.request.paddr)
            }
            PhysicalAccessError::BackendFailure(failure) => MemoryError::Backend(failure.context),
            PhysicalAccessError::Protocol(error) => MemoryError::Protocol(error.to_string()),
            PhysicalAccessError::UnknownCompletion(unknown) => {
                MemoryError::Unknown(unknown.context)
            }
        }
    }

    fn read_raw(
        &self,
        addr: u64,
        width: PhysicalWidth,
        category: PhysicalAccessKind,
    ) -> Result<[u8; 8], MemoryError> {
        let paddr = self.checked_paddr(addr, width)?;
        let request = PhysicalRequest::new(category, paddr, width, None)
            .map_err(|error| MemoryError::Protocol(error.to_string()))?;
        let response = self
            .port
            .borrow_mut()
            .access(request)
            .map_err(Self::map_error)?;
        let bytes = response.read_bytes().ok_or_else(|| {
            MemoryError::Protocol("physical read returned no raw-byte completion".into())
        })?;
        if bytes.len() != width.bytes() {
            return Err(MemoryError::Protocol(format!(
                "physical read returned {} bytes, expected {}",
                bytes.len(),
                width.bytes()
            )));
        }
        let mut raw = [0; 8];
        raw[..width.bytes()].copy_from_slice(bytes);
        Ok(raw)
    }

    fn write_raw(
        &mut self,
        addr: u64,
        width: PhysicalWidth,
        bytes: &[u8],
    ) -> Result<(), MemoryError> {
        let paddr = self.checked_paddr(addr, width)?;
        let request = PhysicalRequest::data_write(paddr, width, bytes)
            .map_err(|error| MemoryError::Protocol(error.to_string()))?;
        let response = self
            .port
            .get_mut()
            .access(request)
            .map_err(Self::map_error)?;
        if !response.is_write_acknowledgement() {
            return Err(MemoryError::Protocol(
                "physical write returned a read completion".into(),
            ));
        }
        Ok(())
    }

    /// Fetches one instruction word with the distinct Fetch category.
    pub fn fetch_word(&self, addr: u64) -> Result<u32, MemoryError> {
        let raw = self.read_raw(addr, PhysicalWidth::Word, PhysicalAccessKind::Fetch)?;
        Ok(u32::from_le_bytes(
            raw[..4].try_into().expect("word width is four bytes"),
        ))
    }
}

/// Converts one guest effective address to the physical address issued at
/// the port, preserving the configured storage-offset alignment rule.
///
/// This is the single conversion both Hart views share: the typed
/// [`PhysicalMemoryAdapter`] subtracts the configured base once, and the
/// optional storage range keeps the legacy typed-offset alignment rejection
/// for requests wholly inside it.  The error carries the rejected
/// (storage-aligned) address for diagnostics; callers map it to the guest
/// address when raising `mtval`.
fn hart_issued_paddr(
    ea: u64,
    base_addr: u64,
    storage_alignment: Option<(u64, usize)>,
    width: PhysicalWidth,
) -> Result<u64, MemoryError> {
    let paddr = ea
        .checked_sub(base_addr)
        .ok_or(MemoryError::InvalidAddress(ea))?;
    let alignment_address = storage_alignment
        .filter(|(base, size)| crate::memory::contains_range(*base, *size, paddr, width.bytes()))
        .and_then(|(base, _)| paddr.checked_sub(base))
        .unwrap_or(paddr);
    if !alignment_address.is_multiple_of(width.bytes() as u64) {
        return Err(MemoryError::InvalidAddress(alignment_address));
    }
    Ok(paddr)
}

/// Hart-side typed interpretation of the atomic envelope data port.
///
/// This view owns only the guest-to-issued address conversion and error
/// mapping; the envelope vocabulary ([`AtomicRequest`]/[`AtomicResponse`])
/// and the Hart reservation record carry the atomic semantics.  The caller
/// holds the data port's mutex for the complete adapter lifetime, so the
/// conditional store-conditional check and its write remain inside the
/// backend's single critical section.  Like [`PhysicalMemoryAdapter`], this
/// view performs no ISA interpretation of its own.
pub struct PhysicalAtomicAdapter<'a> {
    port: &'a mut (dyn PhysicalDataAccess + Send),
    journal: Option<&'a MemoryJournal>,
    base_addr: u64,
    /// The same optional storage-offset alignment range the ordinary raw
    /// view preserves, so an atomic envelope hits the identical target
    /// rejection an ordinary wide access would at a misaligned flat offset.
    storage_alignment: Option<(u64, usize)>,
    /// Page delta (translated - virtual) applied to the guest base before
    /// the guest-to-issued conversion while Sv39 translation is active.
    translation_delta: Option<u64>,
}

impl<'a> PhysicalAtomicAdapter<'a> {
    /// Creates a Hart atomic view over the already-selected data port.
    pub fn new(
        port: &'a mut (dyn PhysicalDataAccess + Send),
        base_addr: u64,
        storage_alignment: Option<(u64, usize)>,
    ) -> Self {
        Self {
            port,
            journal: None,
            base_addr,
            storage_alignment,
            translation_delta: None,
        }
    }

    /// Creates a translated atomic view: every guest effective address is
    /// shifted by `translation_delta` before the base conversion, so the
    /// envelope is issued at the translated guest-physical address while
    /// the register file stays architectural.
    pub fn new_translated(
        port: &'a mut (dyn PhysicalDataAccess + Send),
        base_addr: u64,
        storage_alignment: Option<(u64, usize)>,
        translation_delta: u64,
    ) -> Self {
        Self {
            port,
            journal: None,
            base_addr,
            storage_alignment,
            translation_delta: Some(translation_delta),
        }
    }

    /// Converts one guest effective address into the port-issued physical
    /// address, applying the configured storage-offset rejection.
    ///
    /// A failure is a [`MemoryError::InvalidAddress`] target rejection; the
    /// Hart maps it to the access-class fault with the original guest
    /// address. SC performs this conversion regardless of reservation state,
    /// before its one envelope lets the target validate store access and
    /// atomic capability. A rejected address faults and any live reservation
    /// is retained by staged-state discard (C15). A staged Sv39 translation
    /// delta shifts the guest base first.
    pub fn issued_addr(&self, ea: u64, width: PhysicalWidth) -> Result<u64, MemoryError> {
        let guest = self
            .translation_delta
            .map_or(ea, |delta| ea.wrapping_add(delta));
        hart_issued_paddr(guest, self.base_addr, self.storage_alignment, width)
    }

    /// Submits one atomic envelope and maps the typed error taxonomy onto
    /// the memory-error surface the Hart already classifies.
    ///
    /// Target rejections become [`MemoryError::InvalidAddress`] (guest-visible
    /// access faults); host/backend and protocol failures and unknown
    /// completions keep their simulator-side categories.
    pub fn access_atomic(
        &mut self,
        request: AtomicRequest<'_>,
    ) -> Result<AtomicResponse, MemoryError> {
        let response = self.port.access_atomic(request).map_err(Self::map_error)?;
        // Keep malformed completion handling independent of subscription.
        // Normally the validated port has already enforced this width.
        if response
            .old_bytes()
            .is_some_and(|bytes| bytes.len() != request.width().bytes())
        {
            return Err(MemoryError::Protocol(
                "atomic completion has incorrect old-byte width".into(),
            ));
        }
        if let Some(journal) = self.journal {
            let mut old = [0; 8];
            let read = response.old_bytes().map(|bytes| {
                old[..bytes.len()].copy_from_slice(bytes);
                old
            });
            let write = match request.kind() {
                crate::physical::AtomicAccessKind::Rmw => request
                    .transform()
                    .zip(response.old_bytes())
                    .zip(request.operand_bytes())
                    .map(|((transform, old), operand)| transform.apply(old, operand)),
                crate::physical::AtomicAccessKind::StoreConditional
                    if response.conditional_status()
                        == Some(crate::physical::ConditionalStatus::Success) =>
                {
                    let mut bytes = [0; 8];
                    bytes[..request.width().bytes()]
                        .copy_from_slice(request.store_payload().expect("validated SC payload"));
                    Some(bytes)
                }
                _ => None,
            };
            journal.borrow_mut().push(MemoryEffect {
                guest_address: request
                    .paddr()
                    .wrapping_add(self.base_addr)
                    .wrapping_sub(self.translation_delta.unwrap_or(0)),
                issued_address: Some(request.paddr()),
                width: request.width(),
                read,
                write,
                atomic: Some(AtomicEffect {
                    kind: request.kind(),
                    ordering: request.ordering(),
                    conditional: response.conditional_status(),
                    indivisible: true,
                }),
                implicit: false,
            });
        }
        Ok(response)
    }

    fn map_error(error: AtomicAccessError) -> MemoryError {
        match error {
            AtomicAccessError::TargetRejected(rejection) => {
                MemoryError::InvalidAddress(rejection.request.paddr)
            }
            AtomicAccessError::BackendFailure(failure) => MemoryError::Backend(failure.context),
            AtomicAccessError::Protocol(error) => MemoryError::Protocol(error.to_string()),
            AtomicAccessError::UnknownCompletion(unknown) => MemoryError::Unknown(unknown.context),
        }
    }
}

impl MemoryInterface for PhysicalMemoryAdapter<'_> {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(u64::from_le_bytes(self.read_raw(
            addr,
            PhysicalWidth::Doubleword,
            PhysicalAccessKind::DataRead,
        )?))
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        let raw = self.read_raw(addr, PhysicalWidth::Word, PhysicalAccessKind::DataRead)?;
        Ok(u32::from_le_bytes(
            raw[..4].try_into().expect("word width is four bytes"),
        ))
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        let raw = self.read_raw(addr, PhysicalWidth::Halfword, PhysicalAccessKind::DataRead)?;
        Ok(u16::from_le_bytes(
            raw[..2].try_into().expect("halfword width is two bytes"),
        ))
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        Ok(self.read_raw(addr, PhysicalWidth::Byte, PhysicalAccessKind::DataRead)?[0])
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(self.read_word(addr)? as u64)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(self.read_half(addr)? as u64)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(self.read_byte(addr)? as u64)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok((self.read_word(addr)? as i32) as i64 as u64)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok((self.read_half(addr)? as i16) as i64 as u64)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok((self.read_byte(addr)? as i8) as i64 as u64)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        self.write_raw(addr, PhysicalWidth::Doubleword, &value.to_le_bytes())
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        self.write_raw(addr, PhysicalWidth::Word, &value.to_le_bytes())
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        self.write_raw(addr, PhysicalWidth::Halfword, &value.to_le_bytes())
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        self.write_raw(addr, PhysicalWidth::Byte, &[value])
    }

    fn size(&self) -> usize {
        0
    }
}

impl RiscvCore {
    /// Create a core over legacy typed `MemoryInterface` handles.
    ///
    /// AMO/LR/SC use the typed read/write compatibility route here, which is a
    /// labeled non-conforming adapter: it cannot provide indivisible physical
    /// envelopes or committed-write visibility. Standard facades install
    /// validated physical ports instead.
    pub fn new(
        instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
        data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    ) -> Self {
        Self {
            state: CoreState::default(),
            instruction_mem,
            data_mem,
            decoder: InstructionDecoder::new(),
            executor: Executor::new(),
            instruction_access: None,
            data_access: None,
            physical_storage_alignment: None,
            unresolved_physical_access: None,
            observation_demand: false,
            memory_journal: None,
            gpr_destination: None,
            trap_handler: TrapHandler::new(),
            tlm_interface: None,
            base_addr: 0,
            verbose: false,
            wfi_wait: false,
            debug: debug_mode::HartDebug::default(),
            debug_word: None,
            #[cfg(test)]
            inject_haltreq_after_fetch: false,
        }
    }

    pub(crate) fn verbose(&self) -> bool {
        self.verbose
    }

    /// 使用相同存储器创建core（指令+数据共用）
    pub fn new_with_memory(mem_size: usize) -> Self {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(mem_size)));
        Self::new(mem.clone(), mem)
    }

    /// Creates a core with separate validated raw fetch and data ports.
    ///
    /// The legacy instruction/data handles remain independently supplied and
    /// serve operations outside the port set (System, MiscMem); AMO/LR/SC
    /// issue one atomic envelope per instruction through the same data port
    /// the ordinary loads and stores use (dev-plan §7.2).  The two raw ports
    /// are not merged; callers may connect them to distinct targets.
    pub fn new_with_physical_access(
        instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
        data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
        instruction_access: SharedPhysicalAccess,
        data_access: SharedDataAccess,
    ) -> Self {
        let mut core = Self::new(instruction_mem, data_mem);
        core.instruction_access = Some(instruction_access);
        core.data_access = Some(data_access);
        core
    }

    /// Generic convenience constructor for concrete validated-port handles.
    ///
    /// The data port must satisfy [`PhysicalDataAccess`] — ordinary accesses
    /// and atomic envelopes through one port object.
    pub fn new_with_physical_ports<I, D>(
        instruction_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
        data_mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
        instruction_access: Arc<Mutex<I>>,
        data_access: Arc<Mutex<D>>,
    ) -> Self
    where
        I: PhysicalAccess + Send + 'static,
        D: PhysicalDataAccess + Send + 'static,
    {
        let instruction_access: SharedPhysicalAccess = instruction_access;
        let data_access: SharedDataAccess = data_access;
        Self::new_with_physical_access(instruction_mem, data_mem, instruction_access, data_access)
    }

    /// Installs separate raw fetch/data ports without changing the supplied
    /// legacy typed handles.  This is the minimal migration seam used by the
    /// standard native and flat facades; the data port must also carry the
    /// atomic envelope surface ([`PhysicalDataAccess`]).
    pub fn set_physical_access(
        &mut self,
        instruction_access: SharedPhysicalAccess,
        data_access: SharedDataAccess,
    ) {
        self.instruction_access = Some(instruction_access);
        self.data_access = Some(data_access);
    }

    /// Preserve the legacy typed alignment rule for one native RAM mapping.
    ///
    /// The guest continues to submit its physical address unchanged; only
    /// requests wholly inside this configured RAM range are checked against
    /// their storage offset.  This is intentionally separate from the raw
    /// backend contract, which remains free to accept unaligned transfers.
    pub fn set_physical_storage_alignment(&mut self, storage_base: u64, storage_size: usize) {
        self.physical_storage_alignment = Some((storage_base, storage_size));
    }

    /// Returns the unresolved physical completion, if this Hart is terminal.
    pub fn unresolved_physical_access(&self) -> Option<&SimulatorFailure> {
        self.unresolved_physical_access.as_ref()
    }

    /// Clears a previously retained unresolved completion after the host has
    /// resolved or reconstructed the affected physical domain.
    pub fn clear_unresolved_physical_access(&mut self) {
        self.unresolved_physical_access = None;
    }

    /// Set verbosity
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
    }

    /// Set TLM interface
    pub fn set_tlm_interface(&mut self, tlm: Arc<Mutex<dyn TlmInterface>>) {
        self.tlm_interface = Some(tlm);
    }

    pub(crate) fn is_wfi_waiting(&self) -> bool {
        self.wfi_wait
    }

    /// Get core state (read-only)
    pub fn state(&self) -> &CoreState {
        &self.state
    }

    /// Get mutable core state
    pub fn state_mut(&mut self) -> &mut CoreState {
        &mut self.state
    }

    /// Level-sensitive halt request sampled at the next instruction boundary.
    ///
    /// Debug 1.0 `dmcontrol.haltreq`. The Hart resumes only through
    /// [`Self::debug_resume`]. A later boundary halts again while the bit
    /// remains set.
    pub fn set_haltreq(&mut self, haltreq: bool) {
        self.debug.haltreq = haltreq;
    }

    pub fn haltreq(&self) -> bool {
        self.debug.haltreq
    }

    pub fn in_debug_mode(&self) -> bool {
        self.debug.halted
    }

    /// Read `dcsr` (`0x7b0`).
    pub fn dcsr(&self) -> u64 {
        self.debug.read_dcsr()
    }

    /// Debugger write of `dcsr`. Returns the value a later read observes.
    pub fn write_dcsr(&mut self, value: u64) -> u64 {
        self.debug.write_dcsr(value);
        self.debug.read_dcsr()
    }

    /// Read `dpc` (`0x7b1`).
    pub fn dpc(&self) -> u64 {
        self.debug.dpc
    }

    /// Debugger write of `dpc`. Bits 1:0 are read-only zero.
    pub fn write_dpc(&mut self, value: u64) -> u64 {
        self.debug.write_dpc(value)
    }

    /// Read `dscratch0` (`0x7b2`).
    pub fn dscratch0(&self) -> u64 {
        self.debug.dscratch0
    }

    pub fn write_dscratch0(&mut self, value: u64) {
        self.debug.dscratch0 = value;
    }

    /// Read `dscratch1` (`0x7b3`).
    pub fn dscratch1(&self) -> u64 {
        self.debug.dscratch1
    }

    pub fn write_dscratch1(&mut self, value: u64) {
        self.debug.dscratch1 = value;
    }

    /// Leave Debug Mode. Returns false when the Hart is not halted.
    ///
    /// PC becomes `dpc`. Privilege becomes `dcsr.prv`. `mstatus.MPRV` is
    /// cleared when the new privilege is below M. `haltreq` is not cleared.
    pub fn debug_resume(&mut self) -> bool {
        if !self.debug.halted {
            return false;
        }
        self.state.pc = self.debug.dpc;
        let privilege = self.debug.prv;
        if privilege != PrivilegeMode::Machine {
            let mstatus = self.state.csr.mstatus() & !crate::csr::mstatus::MPRV;
            self.state.csr.set_mstatus(mstatus);
        }
        self.state.privilege = privilege;
        self.state.csr.set_privilege(privilege);
        self.debug.halted = false;
        true
    }

    /// Physical load for Abstract Access Memory.
    ///
    /// Debug Spec 1.0 §3.7.1.3. The access uses this hart's data port with
    /// no translation delta, so `mstatus.MPRV` and `satp` do not apply.
    /// A misaligned address returns [`DebugMemFault::Misaligned`] before the
    /// port is locked. A hart with no data port uses its legacy data memory,
    /// which is the M-mode load and store path on that hart (Debug Spec 1.0
    /// §3.7.1.3). Nothing here enters a trap or changes PC, privilege,
    /// or the counters. [`MemoryError::Unknown`] sets the same sticky
    /// [`Self::unresolved_physical_access`] latch as the instruction path.
    /// A later access returns [`DebugMemFault::Other`] without locking the
    /// port, and the next [`Self::step_outcome`] fails before retirement.
    pub fn debug_physical_load(&mut self, addr: u64, size: u32) -> Result<u64, DebugMemFault> {
        self.debug_physical(addr, size, None)
    }

    /// Physical store for Abstract Access Memory. See [`Self::debug_physical_load`].
    pub fn debug_physical_store(
        &mut self,
        addr: u64,
        size: u32,
        value: u64,
    ) -> Result<(), DebugMemFault> {
        self.debug_physical(addr, size, Some(value)).map(|_| ())
    }

    fn debug_physical(
        &mut self,
        addr: u64,
        size: u32,
        store: Option<u64>,
    ) -> Result<u64, DebugMemFault> {
        // The instruction path refuses another turn after Unknown. Do the
        // same here, before the data port is locked.
        if self.unresolved_physical_access.is_some() {
            return Err(DebugMemFault::Other);
        }
        if !matches!(size, 1 | 2 | 4 | 8) {
            return Err(DebugMemFault::AccessFault);
        }
        // The hart rejects a misaligned data access before translation and
        // before the port (this file's execute path). Do the same here.
        if !addr.is_multiple_of(u64::from(size)) {
            return Err(DebugMemFault::Misaligned);
        }
        // The same choice `execute_instruction` makes. A port is the M-mode
        // path, including a PLIC claim. A core with no port uses the legacy
        // data memory, which is the path an M-mode load or store takes.
        // Debug Spec 1.0 §3.7.1.3: the access may fail only if that M-mode
        // access might fail.
        let result = if let Some(port) = self.data_access.clone() {
            let mut guard = port.lock().map_err(|_| DebugMemFault::Other)?;
            let mut adapter = self.physical_adapter(&mut *guard, None);
            Self::debug_sized_access(&mut adapter, addr, size, store)
        } else {
            let mut guard = self.data_mem.lock().map_err(|_| DebugMemFault::Other)?;
            let mut adapter = LegacyTypedMemoryAdapter::new(&mut *guard, self.base_addr);
            Self::debug_sized_access(&mut adapter, addr, size, store)
        };
        match result {
            Ok(value) => Ok(value),
            Err(error) => Err(self.debug_mem_fault(error)),
        }
    }

    /// One aligned load or store through the hart's typed data view.
    fn debug_sized_access(
        adapter: &mut dyn MemoryInterface,
        addr: u64,
        size: u32,
        store: Option<u64>,
    ) -> Result<u64, MemoryError> {
        if let Some(value) = store {
            match size {
                1 => adapter.write_byte(addr, value as u8),
                2 => adapter.write_half(addr, value as u16),
                4 => adapter.write_word(addr, value as u32),
                _ => adapter.write_dword(addr, value),
            }
            .map(|()| 0)
        } else {
            match size {
                1 => adapter.read_byte_zext(addr),
                2 => adapter.read_half_zext(addr),
                4 => adapter.read_word_zext(addr),
                _ => adapter.read_dword(addr),
            }
        }
    }

    /// Map a data-port error onto an abstract-command fault.
    ///
    /// Misaligned and target rejection stay the M-mode exception of Debug
    /// Spec 1.0 §3.7.1.3. Backend, protocol, and a poisoned lock are §3.14
    /// `cmderr` 7 and do not latch. `Unknown` may already have side effects,
    /// so it uses the instruction path's sticky terminal latch and is also
    /// `cmderr` 7.
    fn debug_mem_fault(&mut self, error: MemoryError) -> DebugMemFault {
        match error {
            MemoryError::Misaligned(_, _) => DebugMemFault::Misaligned,
            MemoryError::Unknown(message) => {
                let _ = self.unresolved_physical_failure(self.state.pc, message);
                DebugMemFault::Other
            }
            MemoryError::Backend(_) | MemoryError::Protocol(_) => DebugMemFault::Other,
            _ => DebugMemFault::AccessFault,
        }
    }

    /// Execute one program-buffer opcode through the shared instruction engine.
    ///
    /// The Hart must be in Debug Mode. Execution uses M privilege, masked
    /// interrupts and no automatic counter charge. `pc` and privilege are
    /// restored so `dpc` sequencing is untouched. Explicit CSR, GPR and
    /// memory writes keep their effects.
    pub fn debug_execute(&mut self, word: u32) -> DebugExecOutcome {
        if !self.debug.halted {
            return DebugExecOutcome::Failure(SimulatorFailure {
                pc: self.state.pc,
                kind: SimulatorFailureKind::Invariant,
                message: "debug_execute requires Debug Mode".to_string(),
            });
        }
        if let Some(failure) = &self.unresolved_physical_access {
            return DebugExecOutcome::Failure(failure.clone());
        }
        if let Some(done) = self.debug_csr_instruction(word) {
            return done;
        }
        let saved_pc = self.state.pc;
        let saved_privilege = self.state.privilege;
        let saved_wfi = self.wfi_wait;
        let saved_observation = self.observation_demand;
        self.observation_demand = false;
        self.memory_journal = None;
        self.state.privilege = PrivilegeMode::Machine;
        self.state.csr.set_privilege(PrivilegeMode::Machine);
        self.debug_word = Some(word);
        let outcome = self.execute_instruction();
        self.debug_word = None;
        self.state.pc = saved_pc;
        self.state.privilege = saved_privilege;
        self.state.csr.set_privilege(saved_privilege);
        self.wfi_wait = saved_wfi;
        self.observation_demand = saved_observation;
        self.state.regs[0] = 0;
        match outcome {
            StepOutcome::InstructionRetired(_) => DebugExecOutcome::Completed,
            StepOutcome::DebugHalted { .. } => DebugExecOutcome::Ebreak,
            StepOutcome::TrapEntered(_) => DebugExecOutcome::Exception,
            StepOutcome::SimulatorFailure(failure) => DebugExecOutcome::Failure(failure),
            StepOutcome::Waiting => DebugExecOutcome::Completed,
        }
    }

    /// Zicsr access to the Hart-owned Sdext CSRs (`0x7b0`-`0x7b3`). Returns
    /// `None` for any other instruction so the shared engine handles it.
    fn debug_csr_instruction(&mut self, word: u32) -> Option<DebugExecOutcome> {
        if word & 0x7f != 0x73 {
            return None;
        }
        let funct3 = (word >> 12) & 7;
        let csr = (word >> 20) as u16;
        if funct3 == 0 || funct3 == 4 || !(0x7b0..=0x7b3).contains(&csr) {
            return None;
        }
        let rd = ((word >> 7) & 0x1f) as usize;
        let field = ((word >> 15) & 0x1f) as usize;
        let operand = if funct3 >= 5 {
            field as u64
        } else {
            self.state.regs[field]
        };
        let old = match csr {
            0x7b0 => self.debug.read_dcsr(),
            0x7b1 => self.debug.dpc,
            0x7b2 => self.debug.dscratch0,
            _ => self.debug.dscratch1,
        };
        // CSRRW/CSRRWI always write. CSRRS/CSRRC (and immediates) write only
        // when the source field is nonzero.
        let new = match funct3 & 3 {
            1 => Some(operand),
            2 => (field != 0).then_some(old | operand),
            _ => (field != 0).then_some(old & !operand),
        };
        if let Some(value) = new {
            match csr {
                0x7b0 => self.debug.write_dcsr(value),
                0x7b1 => {
                    self.debug.write_dpc(value);
                }
                0x7b2 => self.debug.dscratch0 = value,
                _ => self.debug.dscratch1 = value,
            }
        }
        if rd != 0 {
            self.state.regs[rd] = old;
        }
        Some(DebugExecOutcome::Completed)
    }

    /// D6: control transfers, PC-dependent and privilege-changing opcodes
    /// are rejected in the program buffer.
    fn debug_forbidden(instruction: &DecodedInstruction) -> bool {
        matches!(
            instruction.opcode,
            Opcode::Jal | Opcode::Jalr | Opcode::Branch | Opcode::Auipc
        ) || matches!(
            instruction.raw,
            0x0000_0073 | 0x3020_0073 | 0x1020_0073 | 0x7b20_0073
        )
    }

    /// Execute one Hart turn and return its typed semantic outcome.
    ///
    /// Architectural exceptions are entered here and returned as
    /// [`StepOutcome::TrapEntered`].  Only host/backend or invariant failures
    /// use [`StepOutcome::SimulatorFailure`]; no error-string inspection is
    /// involved in that classification.
    pub fn step_outcome(&mut self) -> StepOutcome {
        self.step_transition(false).outcome
    }

    /// Execute once and optionally materialize authoritative completed Hart
    /// facts. There is no observer callback during this call. Delivery is an
    /// outer operation on the returned immutable observation.
    pub fn step_transition(&mut self, observe: bool) -> HartTransition {
        let before = observe.then(|| self.state.clone());
        let before_pc = self.state.pc;
        let before_privilege = self.state.privilege;
        let minstret_before = self.state.csr.minstret_value();
        self.observation_demand = observe;
        self.memory_journal = observe.then(|| RefCell::new(Vec::new()));
        self.gpr_destination = None;
        let (outcome, instruction_attempted, stepped) = self.execute_turn();
        // A single-step publishes DebugHalted. The retirement or trap that
        // D9 already counted is still the control fact and the observation.
        let fact = stepped.as_ref().unwrap_or(&outcome);
        let control = ControlFacts {
            before_pc,
            after_pc: self.state.pc,
            before_privilege,
            after_privilege: self.state.privilege,
            instruction_attempted,
            retired: matches!(fact, StepOutcome::InstructionRetired(_)),
            trap_entered: matches!(fact, StepOutcome::TrapEntered(_)),
            minstret_before,
            minstret_after: self.state.csr.minstret_value(),
        };
        let memory = self.memory_journal.take().map(RefCell::into_inner);
        let observation = before.and_then(|before| {
            observation::build(
                &before,
                &mut self.state,
                fact,
                self.gpr_destination,
                memory.unwrap_or_default(),
            )
        });
        self.state.csr.begin_observation(false);
        self.state.fpr.begin_observation(false);
        self.observation_demand = false;
        self.gpr_destination = None;
        HartTransition {
            outcome,
            control,
            observation,
        }
    }

    /// Return the outcome, whether an instruction attempt began, and the
    /// instruction or trap a single-step wrapped, when one committed.
    fn execute_turn(&mut self) -> (StepOutcome, bool, Option<StepOutcome>) {
        if let Some(failure) = &self.unresolved_physical_access {
            return (StepOutcome::SimulatorFailure(failure.clone()), false, None);
        }

        // CoreState is the architectural mode authority at this boundary.
        // Keep CSR access checks synchronized even when a debugger/test edits
        // the public state through `state_mut`.
        if self.state.csr.get_privilege() != self.state.privilege {
            self.state.csr.set_privilege(self.state.privilege);
        }

        // Debug Mode does not retire. A repeated call keeps the recorded cause.
        if self.debug.halted {
            let cause = self
                .debug
                .cause
                .expect("Debug Mode records the cause that entered it");
            return (
                StepOutcome::DebugHalted {
                    cause,
                    completed_turn: false,
                },
                false,
                None,
            );
        }

        // haltreq is higher than a takeable interrupt and higher than step.
        // Debug 1.0 cause-priority table. Contract D6. A waiting WFI has
        // already retired, so dpc is that instruction plus 4.
        if self.debug.haltreq {
            return (
                self.enter_debug(DebugCause::HaltReq, self.state.pc, false),
                false,
                None,
            );
        }

        // Setting `step` while already in a WFI wait returns Waiting.
        if self.wfi_wait {
            let wake = self.state.csr.mip() & self.state.csr.mie() != 0;
            if !wake {
                return (StepOutcome::Waiting, false, None);
            }
            self.wfi_wait = false;
        }

        // stepie is fixed 0, so a takeable interrupt waits until after the step.
        if self.debug.take_interrupt_this_turn() {
            if let Some(cause) = self.accepted_interrupt() {
                // Privileged §3.1.9 evaluates the interrupt before the next
                // instruction. A takeable pending interrupt is taken at this
                // PC, including a WFI. mepc is that address.
                return (
                    self.enter_trap(Trap::Interrupt(cause), self.state.pc, 0, None),
                    false,
                    None,
                );
            }
        }

        let outcome = self.execute_instruction();
        if self.debug.step {
            let stepped = match &outcome {
                StepOutcome::InstructionRetired(_) | StepOutcome::TrapEntered(_) => {
                    Some(outcome.clone())
                }
                _ => None,
            };
            return (self.finish_step(outcome), true, stepped);
        }
        (outcome, true, None)
    }

    fn enter_debug(&mut self, cause: DebugCause, dpc: u64, completed_turn: bool) -> StepOutcome {
        self.debug.halted = true;
        self.debug.cause = Some(cause);
        self.debug.prv = self.state.privilege;
        self.debug.dpc = debug_mode::legal_dpc(dpc);
        self.state.reservation = None;
        self.wfi_wait = false;
        StepOutcome::DebugHalted {
            cause,
            completed_turn,
        }
    }

    fn finish_step(&mut self, outcome: StepOutcome) -> StepOutcome {
        match outcome {
            StepOutcome::DebugHalted { .. } | StepOutcome::SimulatorFailure(_) => outcome,
            StepOutcome::InstructionRetired(retired) => {
                self.wfi_wait = false;
                self.enter_debug(DebugCause::Step, retired.next_pc, true)
            }
            StepOutcome::TrapEntered(_) => self.enter_debug(DebugCause::Step, self.state.pc, true),
            StepOutcome::Waiting => outcome,
        }
    }

    fn accepted_interrupt(&self) -> Option<InterruptCause> {
        const PRIORITY: [(u64, InterruptCause); 6] = [
            (
                1u64 << (InterruptCause::MachineExternal as u32),
                InterruptCause::MachineExternal,
            ),
            (
                1u64 << (InterruptCause::MachineSoftware as u32),
                InterruptCause::MachineSoftware,
            ),
            (
                1u64 << (InterruptCause::MachineTimer as u32),
                InterruptCause::MachineTimer,
            ),
            (
                1u64 << (InterruptCause::SupervisorExternal as u32),
                InterruptCause::SupervisorExternal,
            ),
            (
                1u64 << (InterruptCause::SupervisorSoftware as u32),
                InterruptCause::SupervisorSoftware,
            ),
            (
                1u64 << (InterruptCause::SupervisorTimer as u32),
                InterruptCause::SupervisorTimer,
            ),
        ];
        let enabled = self.state.privilege != PrivilegeMode::Machine
            || self.state.csr.mstatus() & crate::csr::mstatus::MIE != 0;
        if !enabled {
            return None;
        }
        let pending = self.state.csr.mip() & self.state.csr.mie();
        PRIORITY
            .into_iter()
            .find(|(bit, _)| pending & bit != 0)
            .map(|(_, cause)| cause)
    }

    fn execute_instruction(&mut self) -> StepOutcome {
        let pc_before = self.state.pc;
        self.state.branch_taken = false;

        // Fixed IALIGN=32 is checked before the instruction-memory transaction.
        if self.debug_word.is_none() && pc_before & 0b11 != 0 {
            return self.enter_trap(
                Trap::Exception(ExceptionCause::InstructionAddressMisaligned),
                pc_before,
                pc_before,
                None,
            );
        }

        // Instruction fetch translation. Sv39 translates the fetch effective
        // address through the data port walk before the instruction port
        // sees the resulting guest-physical address (D2, D3). The walk
        // itself runs on the data port; the instruction-memory transaction
        // keeps its Fetch category.
        let fetch_paddr = if self.debug_word.is_some() {
            0
        } else {
            match self.translate_address(pc_before, translation::AccessKind::Fetch, pc_before, None)
            {
                Ok(paddr) => paddr,
                Err(outcome) => return outcome,
            }
        };

        // Fetch keeps the guest PC in the architectural address space.  A
        // migrated core submits one raw Fetch request; the typed view performs
        // the single configured base conversion.  Compatibility callers keep
        // the historical typed instruction-memory route.
        let fetch_result = if let Some(word) = self.debug_word {
            Ok(word)
        } else if let Some(access) = self.instruction_access.as_ref() {
            let mut port = match access.lock() {
                Ok(port) => port,
                Err(_) => {
                    return self.simulator_failure(
                        pc_before,
                        SimulatorFailureKind::HostBackend,
                        "failed to lock physical instruction access",
                    )
                }
            };
            let adapter = self.physical_adapter(&mut *port, None);
            adapter.fetch_word(fetch_paddr)
        } else {
            let instruction_addr = match pc_before.checked_sub(self.base_addr) {
                Some(address) => address,
                None => {
                    return self.enter_trap(
                        Trap::Exception(ExceptionCause::InstructionAccessFault),
                        pc_before,
                        pc_before,
                        None,
                    )
                }
            };
            let mem = match self.instruction_mem.lock() {
                Ok(mem) => mem,
                Err(_) => {
                    return self.simulator_failure(
                        pc_before,
                        SimulatorFailureKind::HostBackend,
                        "failed to lock instruction memory",
                    )
                }
            };
            mem.read_word(instruction_addr)
        };
        let instruction = match fetch_result {
            Ok(instruction) => {
                // The fetch has completed and the instruction has not retired.
                // `haltreq` is not sampled again until the next Hart boundary.
                #[cfg(test)]
                if self.inject_haltreq_after_fetch {
                    self.debug.haltreq = true;
                }
                instruction
            }
            Err(error) => {
                return match error {
                    MemoryError::InvalidAddress(_)
                    | MemoryError::Misaligned(_, _)
                    | MemoryError::OutOfBounds => self.enter_trap(
                        Trap::Exception(ExceptionCause::InstructionAccessFault),
                        pc_before,
                        pc_before,
                        None,
                    ),
                    MemoryError::Backend(message) | MemoryError::Protocol(message) => self
                        .simulator_failure(pc_before, SimulatorFailureKind::HostBackend, message),
                    MemoryError::Unknown(message) => {
                        self.unresolved_physical_failure(pc_before, message)
                    }
                }
            }
        };

        let decoded = match self.decoder.decode(instruction) {
            Ok(decoded) => decoded,
            Err(error) => {
                return match error {
                    DecodeError::InvalidInstruction(_) | DecodeError::ReservedInstruction => self
                        .enter_trap(
                            Trap::Exception(ExceptionCause::IllegalInstruction),
                            pc_before,
                            instruction as u64,
                            Some(instruction),
                        ),
                    DecodeError::UnimplementedInstruction => self.simulator_failure(
                        pc_before,
                        SimulatorFailureKind::UnsupportedLegalInstruction,
                        "decoded legal instruction is not implemented",
                    ),
                }
            }
        };

        if Self::is_illegal_encoding(&decoded)
            || self
                .debug_word
                .is_some_and(|_| Self::debug_forbidden(&decoded))
        {
            return self.enter_trap(
                Trap::Exception(ExceptionCause::IllegalInstruction),
                pc_before,
                instruction as u64,
                Some(instruction),
            );
        }

        // Architectural alignment is a Hart check on the virtual effective
        // address.  It runs before translation and before the memory adapter
        // is called (D6), so a misaligned load/store/AMO produces no physical
        // transaction, no walk, and no leaked destination or store side
        // effect.
        if let Some((is_store, address, width)) = Self::data_access(&decoded, &self.state) {
            if address % width != 0 {
                return self.enter_trap(
                    Trap::Exception(if is_store {
                        ExceptionCause::StoreAddressMisaligned
                    } else {
                        ExceptionCause::LoadAddressMisaligned
                    }),
                    pc_before,
                    address,
                    Some(instruction),
                );
            }
        }

        // Data translation (D2, D3, D6, D9). The Sv39 walk runs on the data
        // port for every load, store, and AMO effective address before the
        // executor issues its own access. A translated access keeps the
        // same page offset (the alignment check above keeps the access
        // within one page), so shifting the guest-to-issued base conversion
        // by the page delta between virtual and guest-physical base makes
        // every executor helper issue at the translated address while the
        // register file stays purely architectural: rs1 keeps the virtual
        // base, and rs2 keeps the architectural store/AMO operand even when
        // rs2 == rs1.
        let mut translation_delta: Option<u64> = None;
        if let Some((is_store, address, _)) = Self::data_access(&decoded, &self.state) {
            let kind = if is_store {
                translation::AccessKind::Store
            } else {
                translation::AccessKind::Load
            };
            match self.translate_address(address, kind, pc_before, Some(instruction)) {
                Err(outcome) => return outcome,
                Ok(paddr) => {
                    // A data access exists only for a decoded instruction
                    // with an rs1 field; `Self::data_access` already read it.
                    translation_delta = Some(paddr.wrapping_sub(address));
                }
            }
        }

        // Taken control-flow targets are checked before the executor can write a
        // link register or mutate PC.  JALR clears only bit 0; fixed IALIGN=32
        // still rejects a resulting target with bit 1 set.
        if let Some(target) = Self::control_target(&decoded, &self.state) {
            if target & 0b11 != 0 {
                return self.enter_trap(
                    Trap::Exception(ExceptionCause::InstructionAddressMisaligned),
                    pc_before,
                    target,
                    Some(instruction),
                );
            }
        }

        // Execute against a staged architectural state.  The memory backend is
        // deliberately not cloned: successful MMIO is a committed physical
        // transaction, while all current faulting data operations return before
        // any architectural destination/store effect.  This prevents partial
        // GPR/CSR/PC/privilege effects without pretending that external MMIO is
        // rollback-able.
        let mut staged = self.state.clone();
        staged.csr.begin_observation(self.observation_demand);
        staged.fpr.begin_observation(self.observation_demand);
        let physical_access = self.data_access.clone();
        let execution_result = if let (Opcode::Amo, Some(access)) =
            (decoded.opcode, &physical_access)
        {
            // The standard A8 route (dev-plan §7.1/§7.2): one admitted AMO/
            // LR/SC issues exactly one atomic envelope through the same
            // validated data port the ordinary accesses use.  The port mutex
            // is the outer lock for the complete Hart step, so the backend's
            // conditional section is covered by exactly one lock domain and
            // no Hart state is touched inside it.
            let mut port = match access.lock() {
                Ok(port) => port,
                Err(_) => {
                    return self.simulator_failure(
                        pc_before,
                        SimulatorFailureKind::HostBackend,
                        "failed to lock physical data access",
                    )
                }
            };
            let mut atomic_view = self.physical_atomic_adapter(&mut *port, translation_delta);
            crate::isa::rv64a::execute_amo_port(&decoded, &mut staged, &mut atomic_view)
                .map(|()| (None, decoded.rd))
        } else {
            match (Self::uses_non_atomic_access(&decoded), physical_access) {
                (true, Some(access)) => {
                    let mut port = match access.lock() {
                        Ok(port) => port,
                        Err(_) => {
                            return self.simulator_failure(
                                pc_before,
                                SimulatorFailureKind::HostBackend,
                                "failed to lock physical data access",
                            )
                        }
                    };
                    let mut physical_view = self.physical_adapter(&mut *port, translation_delta);
                    let mut memory = ObservedMemory::new(
                        &mut physical_view,
                        self.memory_journal.as_ref(),
                        self.base_addr,
                        translation_delta.unwrap_or(0),
                    );
                    self.executor
                        .execute_transition(&decoded, &mut staged, &mut memory)
                }
                _ => {
                    // Legacy typed view for cores created without physical
                    // ports: hold the outer data-memory lock for the complete
                    // Executor/helper call. This is the labeled non-conforming
                    // compatibility adapter, not a standard-facade route.
                    // Translation never activates here (the Sv39 walk requires
                    // the data port), so no translation delta applies.
                    let mut mem = match self.data_mem.lock() {
                        Ok(mem) => mem,
                        Err(_) => {
                            return self.simulator_failure(
                                pc_before,
                                SimulatorFailureKind::HostBackend,
                                "failed to lock data memory",
                            )
                        }
                    };
                    let mut legacy_view = LegacyTypedMemoryAdapter::new(&mut *mem, self.base_addr);
                    let mut memory = ObservedMemory::new(
                        &mut legacy_view,
                        self.memory_journal.as_ref(),
                        self.base_addr,
                        0,
                    );
                    self.executor
                        .execute_transition(&decoded, &mut staged, &mut memory)
                }
            }
        };
        let (csr_access, destination) = match execution_result {
            Ok(access) => access,
            Err(error) => {
                // The staged state is discarded on a failed execution; the
                // trap is classified from the untranslated architectural
                // state and mtval already holds the virtual address.
                return self.classify_execute_error(error, &decoded, pc_before, instruction);
            }
        };

        if decoded.opcode == Opcode::Amo && self.data_access.is_none() {
            if let Some(journal) = &self.memory_journal {
                observation::typed_atomic(&decoded, &self.state, journal);
            }
        }

        // Executor-owned taken branches/jumps/returns leave PC in place;
        // ordinary instructions receive their fall-through PC here.
        if !staged.branch_taken {
            staged.pc = staged.pc.wrapping_add(4);
        }
        staged.regs[0] = 0;

        let explicit_minstret_write = csr_access
            .as_ref()
            .is_some_and(|access| access.addr == machine::MINSTRET && access.wrote);
        let explicit_mcycle_write = csr_access
            .as_ref()
            .is_some_and(|access| access.addr == machine::MCYCLE && access.wrote);
        let debugging = self.debug_word.is_some();
        let minstret = if explicit_minstret_write || debugging {
            staged.csr.minstret_value()
        } else {
            staged.csr.increment_minstret()
        };
        if !explicit_mcycle_write && !debugging {
            staged.csr.increment_mcycle();
        }
        self.gpr_destination = destination;
        let privilege = self.state.privilege;
        let next_privilege = staged.privilege;
        let next_pc = staged.pc;
        self.state = staged;

        if self.verbose {
            eprintln!(
                "[STEP] PC: {:#010x} -> {:#010x}, branch_taken={}, instr={:#010x}",
                pc_before, next_pc, self.state.branch_taken, instruction
            );
            eprintln!(
                "[REGS] a0(x10)={:#x}, t0(x5)={:#x}, t1(x6)={:#x}, t2(x7)={:#x}",
                self.state.regs[10], self.state.regs[5], self.state.regs[6], self.state.regs[7]
            );
        }

        if instruction == 0x1050_0073 && !debugging {
            let wake = self.state.csr.mip() & self.state.csr.mie() != 0;
            self.wfi_wait = !wake;
        }

        StepOutcome::InstructionRetired(InstructionRetired {
            pc: pc_before,
            next_pc,
            instruction,
            privilege,
            next_privilege,
            minstret,
            explicit_minstret_write,
        })
    }

    /// Compatibility wrapper retained for existing callers.  Trap entry is a
    /// successful Hart transition; only a typed simulator failure is surfaced
    /// through the legacy `anyhow::Result` API.
    pub fn step(&mut self) -> Result<()> {
        match self.step_outcome() {
            StepOutcome::InstructionRetired(_)
            | StepOutcome::TrapEntered(_)
            | StepOutcome::Waiting
            | StepOutcome::DebugHalted { .. } => Ok(()),
            StepOutcome::SimulatorFailure(failure) => {
                Err(anyhow::anyhow!("{:?}: {}", failure.kind, failure.message))
            }
        }
    }

    fn simulator_failure(
        &self,
        pc: u64,
        kind: SimulatorFailureKind,
        message: impl Into<String>,
    ) -> StepOutcome {
        StepOutcome::SimulatorFailure(SimulatorFailure {
            pc,
            kind,
            message: message.into(),
        })
    }

    fn unresolved_physical_failure(&mut self, pc: u64, message: impl Into<String>) -> StepOutcome {
        let failure = SimulatorFailure {
            pc,
            kind: SimulatorFailureKind::HostBackend,
            message: format!("unresolved physical completion: {}", message.into()),
        };
        self.unresolved_physical_access = Some(failure.clone());
        StepOutcome::SimulatorFailure(failure)
    }

    fn enter_trap(
        &mut self,
        trap: Trap,
        faulting_pc: u64,
        tval: u64,
        instruction: Option<u32>,
    ) -> StepOutcome {
        if self.debug_word.is_some() {
            // Program-buffer exception: no guest trap CSRs, PC or counters change.
            return StepOutcome::TrapEntered(TrapEntered {
                cause: trap,
                faulting_pc,
                mtval: 0,
                vector_pc: faulting_pc,
                source_privilege: self.state.privilege,
                handler_privilege: self.state.privilege,
                continuation: TrapContinuationPolicy::ContinueToGuestHandler,
                instruction,
            });
        }
        let source_privilege = self.state.privilege;
        let mut staged = self.state.clone();
        staged.csr.begin_observation(self.observation_demand);
        staged.fpr.begin_observation(self.observation_demand);
        staged.branch_taken = false;
        let mtval = match trap {
            Trap::Exception(cause) => cause.mtval(tval),
            Trap::Interrupt(_) => 0,
        };
        let vector_pc =
            match self
                .trap_handler
                .handle_trap_checked(trap, faulting_pc, mtval, &mut staged)
            {
                Ok(vector_pc) => vector_pc,
                Err(error) => {
                    return self.simulator_failure(
                        faulting_pc,
                        SimulatorFailureKind::Invariant,
                        error.to_string(),
                    )
                }
            };
        staged.csr.increment_mcycle();
        let handler_privilege = staged.privilege;
        self.state = staged;
        StepOutcome::TrapEntered(TrapEntered {
            cause: trap,
            faulting_pc,
            mtval,
            vector_pc,
            source_privilege,
            handler_privilege,
            continuation: TrapContinuationPolicy::ContinueToGuestHandler,
            instruction,
        })
    }

    fn classify_execute_error(
        &mut self,
        error: ExecuteError,
        instruction: &DecodedInstruction,
        pc: u64,
        raw: u32,
    ) -> StepOutcome {
        match error {
            ExecuteError::Ecall => self.enter_trap(
                Trap::Exception(match self.state.privilege {
                    PrivilegeMode::User => ExceptionCause::EcallU,
                    PrivilegeMode::Supervisor => ExceptionCause::EcallS,
                    PrivilegeMode::Machine => ExceptionCause::EcallM,
                }),
                pc,
                0,
                Some(raw),
            ),
            ExecuteError::Ebreak if self.debug_word.is_some() => StepOutcome::DebugHalted {
                cause: DebugCause::Ebreak,
                completed_turn: false,
            },
            ExecuteError::Ebreak => {
                if self.debug.ebreak_redirects(self.state.privilege) {
                    self.enter_debug(DebugCause::Ebreak, pc, false)
                } else {
                    self.enter_trap(
                        Trap::Exception(ExceptionCause::Breakpoint),
                        pc,
                        0,
                        Some(raw),
                    )
                }
            }
            ExecuteError::IllegalInstruction | ExecuteError::CsrError(_) => self.enter_trap(
                Trap::Exception(ExceptionCause::IllegalInstruction),
                pc,
                raw as u64,
                Some(raw),
            ),
            ExecuteError::MisalignedAccess(address, _) => {
                let cause = if matches!(
                    instruction.opcode,
                    Opcode::Jal | Opcode::Jalr | Opcode::Branch
                ) {
                    ExceptionCause::InstructionAddressMisaligned
                } else if Self::data_access(instruction, &self.state)
                    .is_some_and(|(is_store, _, _)| is_store)
                {
                    ExceptionCause::StoreAddressMisaligned
                } else {
                    ExceptionCause::LoadAddressMisaligned
                };
                self.enter_trap(Trap::Exception(cause), pc, address, Some(raw))
            }
            ExecuteError::MemoryError(error) => {
                match error {
                    MemoryError::Backend(message) | MemoryError::Protocol(message) => {
                        self.simulator_failure(pc, SimulatorFailureKind::HostBackend, message)
                    }
                    MemoryError::Unknown(message) => self.unresolved_physical_failure(pc, message),
                    MemoryError::InvalidAddress(_)
                    | MemoryError::OutOfBounds
                    | MemoryError::Misaligned(_, _) => {
                        let Some((is_store, effective_address, _)) =
                            Self::data_access(instruction, &self.state)
                        else {
                            return self.simulator_failure(
                                pc,
                                SimulatorFailureKind::Invariant,
                                "memory error without a data-access instruction",
                            );
                        };
                        // Use the Hart's original address for mtval; the
                        // backend address remains diagnostic-only.
                        self.enter_trap(
                            Trap::Exception(if is_store {
                                ExceptionCause::StoreAccessFault
                            } else {
                                ExceptionCause::LoadAccessFault
                            }),
                            pc,
                            effective_address,
                            Some(raw),
                        )
                    }
                }
            }
            // Every encoding the Hart admits is implemented, so an executor
            // rejection is an internal invariant failure, never a guest
            // result.
            ExecuteError::InvalidOperation => self.simulator_failure(
                pc,
                SimulatorFailureKind::Invariant,
                "executor rejected an encoding validated by the Hart",
            ),
            ExecuteError::InvalidRegister(register) => self.simulator_failure(
                pc,
                SimulatorFailureKind::Invariant,
                format!("invalid register x{register}"),
            ),
        }
    }

    fn sign_extend_12(imm: u32) -> u64 {
        ((imm as i32) << 20 >> 20) as i64 as u64
    }

    fn is_load_reserved(instruction: &DecodedInstruction) -> bool {
        instruction.opcode == Opcode::Amo
            && ((instruction.raw >> 27) & 0x1f) == 0b00010
            && instruction.rs2 == Some(0)
    }

    fn physical_adapter<'a>(
        &self,
        port: &'a mut dyn PhysicalAccess,
        translation_delta: Option<u64>,
    ) -> PhysicalMemoryAdapter<'a> {
        match (self.physical_storage_alignment, translation_delta) {
            (Some((base, size)), Some(delta)) => PhysicalMemoryAdapter::new_translated(
                port,
                self.base_addr,
                Some((base, size)),
                delta,
            ),
            (Some((base, size)), None) => {
                PhysicalMemoryAdapter::new_with_storage_alignment(port, self.base_addr, base, size)
            }
            (None, Some(delta)) => {
                PhysicalMemoryAdapter::new_translated(port, self.base_addr, None, delta)
            }
            (None, None) => PhysicalMemoryAdapter::new(port, self.base_addr),
        }
    }

    /// Builds the Hart atomic view over the already-locked data port.
    ///
    /// The view shares the same base conversion, storage-offset rejection,
    /// and translation delta as [`Self::physical_adapter`]; it exists only
    /// inside the data port's mutex lifetime, so the complete AMO/LR/SC
    /// operation runs inside the backend's single critical section.
    fn physical_atomic_adapter<'a>(
        &'a self,
        port: &'a mut (dyn PhysicalDataAccess + Send),
        translation_delta: Option<u64>,
    ) -> PhysicalAtomicAdapter<'a> {
        let mut adapter = match translation_delta {
            Some(delta) => PhysicalAtomicAdapter::new_translated(
                port,
                self.base_addr,
                self.physical_storage_alignment,
                delta,
            ),
            None => {
                PhysicalAtomicAdapter::new(port, self.base_addr, self.physical_storage_alignment)
            }
        };
        adapter.journal = self.memory_journal.as_ref();
        adapter
    }

    /// Ordinary integer/FP memory operations use the raw physical port.
    /// AMO/LR/SC use the atomic envelope route above whenever a physical data
    /// port is installed; the typed branch below is only for the explicitly
    /// non-conforming `RiscvCore::new` compatibility adapter. Other
    /// non-memory instructions stay on their existing execution path.
    fn uses_non_atomic_access(instruction: &DecodedInstruction) -> bool {
        matches!(
            instruction.opcode,
            Opcode::Load | Opcode::Store | Opcode::LoadFp | Opcode::StoreFp
        )
    }

    fn data_access(
        instruction: &DecodedInstruction,
        state: &CoreState,
    ) -> Option<(bool, u64, u64)> {
        let rs1 = instruction.rs1? as usize;
        let address = if instruction.opcode == Opcode::Amo {
            // AMOs use an R-type encoding: rs1 is the complete effective
            // address and the immediate field is intentionally absent.
            state.regs[rs1]
        } else {
            let imm = Self::sign_extend_12(instruction.imm?);
            state.regs[rs1].wrapping_add(imm)
        };
        let width = match instruction.opcode {
            Opcode::Load => match instruction.funct3? as u8 {
                0 | 4 => 1,
                1 | 5 => 2,
                2 | 6 => 4,
                3 => 8,
                _ => return None,
            },
            Opcode::Store => match instruction.funct3? as u8 {
                0 => 1,
                1 => 2,
                2 => 4,
                3 => 8,
                _ => return None,
            },
            Opcode::LoadFp => match instruction.funct3? as u8 {
                2 => 4,
                3 => 8,
                _ => return None,
            },
            Opcode::StoreFp => match instruction.funct3? as u8 {
                2 => 4,
                3 => 8,
                _ => return None,
            },
            Opcode::Amo => match instruction.funct3? as u8 {
                2 => 4,
                3 => 8,
                _ => return None,
            },
            _ => return None,
        };
        let is_store = match instruction.opcode {
            Opcode::Store | Opcode::StoreFp => true,
            // LR.W/LR.D are load-class accesses even though they share the
            // AMO major opcode. SC and read-modify-write AMOs are store-class
            // accesses for architectural cause selection.
            Opcode::Amo => !Self::is_load_reserved(instruction),
            _ => false,
        };
        Some((is_store, address, width))
    }

    fn control_target(instruction: &DecodedInstruction, state: &CoreState) -> Option<u64> {
        match instruction.opcode {
            Opcode::Jal => {
                let imm = instruction.imm?;
                let offset = ((imm as i32) << 11 >> 11) as i64 as u64;
                Some(state.pc.wrapping_add(offset))
            }
            Opcode::Jalr => {
                let rs1 = instruction.rs1? as usize;
                let imm = Self::sign_extend_12(instruction.imm?);
                Some(state.regs[rs1].wrapping_add(imm) & !1)
            }
            Opcode::Branch => {
                let rs1 = instruction.rs1? as usize;
                let rs2 = instruction.rs2? as usize;
                let taken = match instruction.funct3? as u8 {
                    0 => state.regs[rs1] == state.regs[rs2],
                    1 => state.regs[rs1] != state.regs[rs2],
                    4 => (state.regs[rs1] as i64) < (state.regs[rs2] as i64),
                    5 => (state.regs[rs1] as i64) >= (state.regs[rs2] as i64),
                    6 => state.regs[rs1] < state.regs[rs2],
                    7 => state.regs[rs1] >= state.regs[rs2],
                    _ => return None,
                };
                if taken {
                    let imm = instruction.imm?;
                    let offset = ((imm as i32) << 19 >> 19) as i64 as u64;
                    Some(state.pc.wrapping_add(offset))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// SFENCE.VMA: SYSTEM, funct3 = 0, funct7 = 0b0001001, rd = 0.  Every
    /// rs1 and rs2 value is legal.  SFENCE.VMA with rd != 0 and HFENCE.*
    /// without H are reserved encodings, whose behavior the specification
    /// leaves unspecified; they fall through to the illegal-instruction trap
    /// for consistency with Spike and Sail, not because the spec requires it.
    fn is_sfence_vma(raw: u32) -> bool {
        raw & 0xFE00_7FFF == 0x1200_0073
    }

    fn is_illegal_encoding(instruction: &DecodedInstruction) -> bool {
        let funct3 = instruction.funct3.map(|value| value as u8);
        let funct7 = instruction.funct7;
        match instruction.opcode {
            Opcode::Load => !matches!(funct3, Some(0..=6)),
            Opcode::Store => !matches!(funct3, Some(0..=3)),
            Opcode::Branch => !matches!(funct3, Some(0 | 1 | 4 | 5 | 6 | 7)),
            Opcode::Jalr => funct3 != Some(0),
            Opcode::OpImm => match funct3 {
                // RV64I SLLI/SRLI/SRAI use a six-bit shamt.  Decoder
                // funct7 bit 0 is shamt[5]; only the remaining funct7 bits
                // carry the operation's fixed encoding.
                Some(1) => !funct7.is_some_and(|value| value & 0x7e == 0),
                Some(5) => !funct7.is_some_and(|value| matches!(value & 0x7e, 0 | 0x20)),
                Some(0 | 2 | 3 | 4 | 6 | 7) => false,
                _ => true,
            },
            Opcode::OpImm32 => match funct3 {
                Some(1) => funct7 != Some(0),
                Some(5) => !matches!(funct7, Some(0) | Some(0x20)),
                Some(0) => false,
                _ => true,
            },
            Opcode::Op => {
                let Some(funct3) = funct3 else { return true };
                let Some(funct7) = funct7 else { return true };
                if funct7 == 1 {
                    return false;
                }
                match funct3 {
                    0 => !matches!(funct7, 0 | 0x20),
                    1 | 2 | 3 | 4 | 6 | 7 => funct7 != 0,
                    5 => !matches!(funct7, 0 | 0x20),
                    _ => true,
                }
            }
            Opcode::Op32 => {
                let Some(funct3) = funct3 else { return true };
                let Some(funct7) = funct7 else { return true };
                match (funct3, funct7) {
                    // RV64I ADDW/SUBW: funct7 selects add/sub.
                    (0, 0 | 0x20) => false,
                    // RV64I SLLW only permits funct7 = 0.
                    (1, 0) => false,
                    // RV64I SRLW/SRAW: funct7 selects logical/arithmetic.
                    (5, 0 | 0x20) => false,
                    // RV64M MULW/DIVW/DIVUW/REMW/REMUW.
                    (0 | 4 | 5 | 6 | 7, 1) => false,
                    _ => true,
                }
            }
            Opcode::System => match funct3 {
                Some(0) => {
                    !(matches!(
                        instruction.raw,
                        0x0000_0073 // ECALL
                        | 0x0010_0073 // EBREAK
                        | 0x1020_0073 // SRET
                        | 0x3020_0073 // MRET
                        | 0x1050_0073 // WFI
                    ) || Self::is_sfence_vma(instruction.raw))
                }
                Some(1 | 2 | 3 | 5 | 6 | 7) => false,
                _ => true,
            },
            // F and D are not implemented (misa F = D = 0).  Every LOAD-FP,
            // STORE-FP, and OP-FP encoding is illegal, and this check runs
            // before alignment, translation, and any data request.
            Opcode::LoadFp | Opcode::StoreFp | Opcode::OpFp => true,
            // Base FENCE and Zifencei FENCE.I (funct3 0 and 1). Other MISC-MEM
            // funct3 values are reserved.
            Opcode::MiscMem => !matches!(funct3, Some(0 | 1)),
            // AMO/LR/SC legality is the approved §2 table: `funct5` selects
            // the operation (AMOSWAP included), `funct3` selects W or D, and
            // every reserved `funct5` value plus LR with `rs2 != 0` is an
            // illegal-instruction trap — checked here, before any alignment
            // or access precheck issues a request (dev-plan §5.3, C6).
            Opcode::Amo => !crate::isa::rv64a::amo_encoding_is_legal(instruction),
            _ => false,
        }
    }

    /// Reset core. Clears the `haltreq` latch.
    /// entry_point: virtual address of entry point
    /// base_addr: base address of loaded ELF (used for VA -> PA translation)
    pub fn reset(&mut self, entry_point: u64, base_addr: u64) {
        self.state = CoreState::default();
        self.state.pc = entry_point;
        self.base_addr = base_addr;
        self.wfi_wait = false;
        self.debug = debug_mode::HartDebug::default();
        self.trap_handler = TrapHandler::new();
    }

    /// Run until the core budget is exhausted.  This compatibility method
    /// counts completed Hart turns; the public executor applies platform stop
    /// and host-slot policy around [`Self::step_outcome`].
    pub fn run(&mut self, max_cycles: u64) -> Result<u64> {
        let mut cycles = 0;
        while cycles < max_cycles {
            match self.step_outcome() {
                StepOutcome::InstructionRetired(_) | StepOutcome::TrapEntered(_) => {
                    cycles += 1;
                }
                StepOutcome::Waiting => return Ok(cycles),
                StepOutcome::DebugHalted { completed_turn, .. } => {
                    if completed_turn {
                        cycles += 1;
                    }
                    return Ok(cycles);
                }
                StepOutcome::SimulatorFailure(failure) => {
                    return Err(anyhow::anyhow!("{:?}: {}", failure.kind, failure.message));
                }
            }
        }
        Ok(cycles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_initialization() {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
        let core = RiscvCore::new(mem.clone(), mem);

        assert_eq!(core.state.pc, 0);
        assert_eq!(core.state.regs[0], 0); // x0 always returns 0
        assert_eq!(core.state.privilege, PrivilegeMode::Machine);
    }

    #[test]
    fn test_core_reset() {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
        let mut core = RiscvCore::new(mem.clone(), mem);

        core.state.pc = 0x100;
        core.state.regs[1] = 42;

        core.reset(0x200, 0x80000000);

        assert_eq!(core.state.pc, 0x200);
        assert_eq!(core.state.regs[1], 0); // regs cleared after reset
    }

    #[test]
    fn test_memory_adapter_va_to_pa() {
        let mut mem = SimpleMemory::new(0x10000);
        let base_addr = 0x80000000u64;

        // Write to physical address 0x100 using raw memory
        mem.write_dword(0x100, 0x12345678).unwrap();

        // Create adapter and read from virtual address
        let adapter = MemoryAdapter::new(&mut mem, base_addr);

        // Read from virtual address 0x80000100
        // VA 0x80000100 -> PA 0x100
        let value = adapter.read_dword(0x80000100).unwrap();
        assert_eq!(value, 0x12345678);
    }

    #[test]
    fn test_memory_adapter_different_base() {
        let mut mem = SimpleMemory::new(0x10000);
        let base_addr = 0x40000000u64;

        // Write to physical address 0x200
        mem.write_dword(0x200, 0xDEADBEEF).unwrap();

        let adapter = MemoryAdapter::new(&mut mem, base_addr);

        // Read from virtual address 0x40000200
        let value = adapter.read_dword(0x40000200).unwrap();
        assert_eq!(value, 0xDEADBEEF);
    }

    #[test]
    fn test_memory_adapter_byte_access() {
        let mut mem = SimpleMemory::new(0x1000);
        let base_addr = 0x80000000u64;

        // Write byte to physical address 0x50
        mem.write_byte(0x50, 0xAB).unwrap();

        let adapter = MemoryAdapter::new(&mut mem, base_addr);

        // Read from virtual address 0x80000050
        let value = adapter.read_byte(0x80000050).unwrap();
        assert_eq!(value, 0xAB);
    }

    /// §7 item 3 negative mutation. `haltreq` becomes set after the fetch and
    /// before decode. Accepting it there enters Debug Mode instead of retiring
    /// `addi`, leaves `x1` clear, and stores `dpc` at the interrupted PC.
    #[test]
    fn haltreq_after_fetch_does_not_abandon_the_instruction() {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(0x1000)));
        mem.lock().unwrap().write_word(0, 0x0010_0093).unwrap();
        let mut core = RiscvCore::new(mem.clone(), mem);
        core.reset(0, 0);
        core.inject_haltreq_after_fetch = true;

        match core.step_outcome() {
            StepOutcome::InstructionRetired(_) => {}
            other => panic!("haltreq was accepted mid-instruction: {other:?}"),
        }
        assert_eq!(core.state().regs[1], 1, "addi did not retire");
        assert_eq!(core.state().pc, 4);
        assert!(!core.in_debug_mode());
        assert!(core.haltreq());
        assert_eq!(core.state().csr.minstret_value(), 1);

        match core.step_outcome() {
            StepOutcome::DebugHalted {
                cause: DebugCause::HaltReq,
                completed_turn: false,
            } => {}
            other => panic!("the next boundary did not halt: {other:?}"),
        }
        assert_eq!(
            core.dpc(),
            4,
            "dpc is the instruction after the one that retired"
        );
        assert_ne!(
            core.dpc(),
            0,
            "haltreq must not abandon the interrupted addi"
        );
        assert_eq!(core.state().csr.minstret_value(), 1);
    }
}
