//! RISC-V RV64I core module
//!
//! Implements RISC-V processor core fetch-decode-execute cycle

pub mod commits;
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

/// Facts for one completed synchronous trap entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrapEntered {
    /// Architectural synchronous exception cause.
    pub cause: ExceptionCause,
    /// PC of the faulting instruction or fetch.
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
    /// Machine-mode synchronous trap handler.
    trap_handler: TrapHandler,
    /// TLM interface（可选）
    tlm_interface: Option<Arc<Mutex<dyn TlmInterface>>>,
    /// Base address for virtual address translation (loaded ELF base address)
    base_addr: u64,
    /// Verbose output flag
    verbose: bool,
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
}

impl<'a> PhysicalMemoryAdapter<'a> {
    /// Creates a Hart typed view over one already-selected physical port.
    pub fn new(port: &'a mut dyn PhysicalAccess, base_addr: u64) -> Self {
        Self {
            port: RefCell::new(port),
            base_addr,
            storage_alignment: None,
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
        }
    }

    /// Preserve the flat backend's historical typed-offset alignment rule
    /// after the one guest-to-storage conversion.  This is a target/storage
    /// rejection, not a new Hart alignment check; the Hart has already checked
    /// the guest address before this adapter is entered.  The conversion is
    /// shared with the atomic view through [`hart_issued_paddr`].
    fn checked_paddr(&self, va: u64, width: PhysicalWidth) -> Result<u64, MemoryError> {
        hart_issued_paddr(va, self.base_addr, self.storage_alignment, width)
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
    base_addr: u64,
    /// The same optional storage-offset alignment range the ordinary raw
    /// view preserves, so an atomic envelope hits the identical target
    /// rejection an ordinary wide access would at a misaligned flat offset.
    storage_alignment: Option<(u64, usize)>,
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
            base_addr,
            storage_alignment,
        }
    }

    /// Converts one guest effective address into the port-issued physical
    /// address, applying the configured storage-offset rejection.
    ///
    /// A failure is a [`MemoryError::InvalidAddress`] target rejection; the
    /// Hart maps it to the access-class fault with the original guest
    /// address — including for store-conditional, which faults instead of
    /// retiring `rd = 1` and retains its reservation (the conditional
    /// failure path applies only once an issued address exists).
    pub fn issued_addr(&self, ea: u64, width: PhysicalWidth) -> Result<u64, MemoryError> {
        hart_issued_paddr(ea, self.base_addr, self.storage_alignment, width)
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
        self.port.access_atomic(request).map_err(Self::map_error)
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
            trap_handler: TrapHandler::new(),
            tlm_interface: None,
            base_addr: 0,
            verbose: false,
        }
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

    /// Get core state (read-only)
    pub fn state(&self) -> &CoreState {
        &self.state
    }

    /// Get mutable core state
    pub fn state_mut(&mut self) -> &mut CoreState {
        &mut self.state
    }

    /// Execute one Hart turn and return its typed semantic outcome.
    ///
    /// Architectural exceptions are entered here and returned as
    /// [`StepOutcome::TrapEntered`].  Only host/backend or invariant failures
    /// use [`StepOutcome::SimulatorFailure`]; no error-string inspection is
    /// involved in that classification.
    pub fn step_outcome(&mut self) -> StepOutcome {
        if let Some(failure) = &self.unresolved_physical_access {
            return StepOutcome::SimulatorFailure(failure.clone());
        }

        // CoreState is the architectural mode authority at this boundary.
        // Keep CSR access checks synchronized even when a debugger/test edits
        // the public state through `state_mut`.
        if self.state.csr.get_privilege() != self.state.privilege {
            self.state.csr.set_privilege(self.state.privilege);
        }
        let pc_before = self.state.pc;
        self.state.branch_taken = false;

        // Fixed IALIGN=32 is checked before the instruction-memory transaction.
        if pc_before & 0b11 != 0 {
            return self.enter_trap(
                ExceptionCause::InstructionAddressMisaligned,
                pc_before,
                pc_before,
                None,
            );
        }

        // Fetch keeps the guest PC in the architectural address space.  A
        // migrated core submits one raw Fetch request; the typed view performs
        // the single configured base conversion.  Compatibility callers keep
        // the historical typed instruction-memory route.
        let fetch_result = if let Some(access) = self.instruction_access.as_ref() {
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
            let adapter = self.physical_adapter(&mut *port);
            adapter.fetch_word(pc_before)
        } else {
            let instruction_addr = match pc_before.checked_sub(self.base_addr) {
                Some(address) => address,
                None => {
                    return self.enter_trap(
                        ExceptionCause::InstructionAccessFault,
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
            Ok(instruction) => instruction,
            Err(error) => {
                return match error {
                    MemoryError::InvalidAddress(_)
                    | MemoryError::Misaligned(_, _)
                    | MemoryError::OutOfBounds => self.enter_trap(
                        ExceptionCause::InstructionAccessFault,
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
                            ExceptionCause::IllegalInstruction,
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

        if Self::is_illegal_encoding(&decoded) {
            return self.enter_trap(
                ExceptionCause::IllegalInstruction,
                pc_before,
                instruction as u64,
                Some(instruction),
            );
        }
        if Self::is_unsupported_encoding(&decoded) {
            return self.simulator_failure(
                pc_before,
                SimulatorFailureKind::UnsupportedLegalInstruction,
                "decoded legal instruction is not implemented",
            );
        }

        // Architectural alignment is a Hart check.  It runs before the memory
        // adapter is called, so a misaligned load/store/AMO produces no physical
        // transaction and cannot leak a destination or store side effect.
        if let Some((is_store, address, width)) = Self::data_access(&decoded, &self.state) {
            if address % width != 0 {
                return self.enter_trap(
                    if is_store {
                        ExceptionCause::StoreAddressMisaligned
                    } else {
                        ExceptionCause::LoadAddressMisaligned
                    },
                    pc_before,
                    address,
                    Some(instruction),
                );
            }
        }

        // Taken control-flow targets are checked before the executor can write a
        // link register or mutate PC.  JALR clears only bit 0; fixed IALIGN=32
        // still rejects a resulting target with bit 1 set.
        if let Some(target) = Self::control_target(&decoded, &self.state) {
            if target & 0b11 != 0 {
                return self.enter_trap(
                    ExceptionCause::InstructionAddressMisaligned,
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
            let mut atomic_view = self.physical_atomic_adapter(&mut *port);
            crate::isa::rv64a::execute_amo_port(&decoded, &mut staged, &mut atomic_view)
                .map(|()| None)
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
                    let mut physical_view = self.physical_adapter(&mut *port);
                    self.executor
                        .execute_with_csr_access(&decoded, &mut staged, &mut physical_view)
                }
                _ => {
                    // Legacy typed view for cores created without physical
                    // ports: hold the outer data-memory lock for the complete
                    // Executor/helper call. This is the labeled non-conforming
                    // compatibility adapter, not a standard-facade route.
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
                    self.executor
                        .execute_with_csr_access(&decoded, &mut staged, &mut legacy_view)
                }
            }
        };
        let csr_access = match execution_result {
            Ok(access) => access,
            Err(error) => {
                return self.classify_execute_error(error, &decoded, pc_before, instruction)
            }
        };

        // Executor-owned taken branches/jumps/returns leave PC in place;
        // ordinary instructions receive their fall-through PC here.
        if !staged.branch_taken {
            staged.pc = staged.pc.wrapping_add(4);
        }
        staged.regs[0] = 0;

        let explicit_minstret_write = csr_access
            .as_ref()
            .is_some_and(|access| access.addr == machine::MINSTRET && access.wrote);
        let minstret = if explicit_minstret_write {
            staged.csr.minstret_value()
        } else {
            staged.csr.increment_minstret()
        };
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
            StepOutcome::InstructionRetired(_) | StepOutcome::TrapEntered(_) => Ok(()),
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
        cause: ExceptionCause,
        faulting_pc: u64,
        tval: u64,
        instruction: Option<u32>,
    ) -> StepOutcome {
        let source_privilege = self.state.privilege;
        let mut staged = self.state.clone();
        staged.branch_taken = false;
        let vector_pc = match self.trap_handler.handle_trap_checked(
            Trap::Exception(cause),
            faulting_pc,
            tval,
            &mut staged,
        ) {
            Ok(vector_pc) => vector_pc,
            Err(error) => {
                return self.simulator_failure(
                    faulting_pc,
                    SimulatorFailureKind::Invariant,
                    error.to_string(),
                )
            }
        };
        let handler_privilege = staged.privilege;
        self.state = staged;
        StepOutcome::TrapEntered(TrapEntered {
            cause,
            faulting_pc,
            mtval: cause.mtval(tval),
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
                match self.state.privilege {
                    PrivilegeMode::User => ExceptionCause::EcallU,
                    PrivilegeMode::Supervisor => ExceptionCause::EcallS,
                    PrivilegeMode::Machine => ExceptionCause::EcallM,
                },
                pc,
                0,
                Some(raw),
            ),
            ExecuteError::Ebreak => self.enter_trap(ExceptionCause::Breakpoint, pc, 0, Some(raw)),
            ExecuteError::IllegalInstruction | ExecuteError::CsrError(_) => self.enter_trap(
                ExceptionCause::IllegalInstruction,
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
                self.enter_trap(cause, pc, address, Some(raw))
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
                            if is_store {
                                ExceptionCause::StoreAccessFault
                            } else {
                                ExceptionCause::LoadAccessFault
                            },
                            pc,
                            effective_address,
                            Some(raw),
                        )
                    }
                }
            }
            ExecuteError::InvalidOperation => {
                if matches!(
                    instruction.opcode,
                    Opcode::LoadFp | Opcode::StoreFp | Opcode::OpFp | Opcode::Amo | Opcode::Op32
                ) {
                    self.simulator_failure(
                        pc,
                        SimulatorFailureKind::UnsupportedLegalInstruction,
                        "legal extension instruction is not implemented",
                    )
                } else {
                    self.simulator_failure(
                        pc,
                        SimulatorFailureKind::Invariant,
                        "executor rejected an encoding validated by the Hart",
                    )
                }
            }
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

    fn physical_adapter<'a>(&self, port: &'a mut dyn PhysicalAccess) -> PhysicalMemoryAdapter<'a> {
        match self.physical_storage_alignment {
            Some((base, size)) => {
                PhysicalMemoryAdapter::new_with_storage_alignment(port, self.base_addr, base, size)
            }
            None => PhysicalMemoryAdapter::new(port, self.base_addr),
        }
    }

    /// Builds the Hart atomic view over the already-locked data port.
    ///
    /// The view shares the same base conversion and storage-offset rejection
    /// as [`Self::physical_adapter`]; it exists only inside the data port's
    /// mutex lifetime, so the complete AMO/LR/SC operation runs inside the
    /// backend's single critical section.
    fn physical_atomic_adapter<'a>(
        &self,
        port: &'a mut (dyn PhysicalDataAccess + Send),
    ) -> PhysicalAtomicAdapter<'a> {
        PhysicalAtomicAdapter::new(port, self.base_addr, self.physical_storage_alignment)
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
                    // RV64M encodings are architecturally legal even where
                    // the active executor does not implement every operation.
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
                    // RV64M MULW/DIVW/DIVUW/REMW/REMUW.  These are legal
                    // encodings even though the active Op32 dispatcher does
                    // not implement them yet.
                    (0 | 4 | 5 | 6 | 7, 1) => false,
                    _ => true,
                }
            }
            Opcode::System => match funct3 {
                Some(0) => !matches!(
                    instruction.raw,
                    0x0000_0073 // ECALL
                        | 0x0010_0073 // EBREAK
                        | 0x1020_0073 // SRET (recognized, but out of scope)
                        | 0x3020_0073 // MRET
                        | 0x1050_0073 // WFI (recognized, but unsupported)
                ),
                Some(1 | 2 | 3 | 5 | 6 | 7) => false,
                _ => true,
            },
            Opcode::LoadFp | Opcode::StoreFp => !matches!(funct3, Some(2 | 3)),
            // AMO/LR/SC legality is the approved §2 table: `funct5` selects
            // the operation (AMOSWAP included), `funct3` selects W or D, and
            // every reserved `funct5` value plus LR with `rs2 != 0` is an
            // illegal-instruction trap — checked here, before any alignment
            // or access precheck issues a request (dev-plan §5.3, C6).
            Opcode::Amo => !crate::isa::rv64a::amo_encoding_is_legal(instruction),
            _ => false,
        }
    }

    fn is_unsupported_encoding(instruction: &DecodedInstruction) -> bool {
        matches!(
            instruction.raw,
            0x1020_0073 // SRET is outside this Task 3 integration.
                | 0x1050_0073 // WFI is legal but not implemented.
        )
    }

    /// Reset core
    /// entry_point: virtual address of entry point
    /// base_addr: base address of loaded ELF (used for VA -> PA translation)
    pub fn reset(&mut self, entry_point: u64, base_addr: u64) {
        self.state = CoreState::default();
        self.state.pc = entry_point;
        self.base_addr = base_addr;
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
}
