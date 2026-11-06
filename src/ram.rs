//! The Platform's single selectable guest-RAM slot (dev-plan D2).
//!
//! One [`RamSlot`] is the only guest-RAM object in a Platform.  It is either
//! the existing native [`SimpleMemory`] (default, unchanged) or a shared
//! blocking target reached through [`TlmRamAdapter`].  The typed route, the
//! raw physical and atomic backends, the Debug Module's inbound port, image
//! placement and the host views all reach RAM through this slot; none of them
//! keeps a private copy.
//!
//! Host views (tohost polling, signature extraction, `read_mem`/`write_mem`
//! and `Machine::memory()`) use the target's debug transport, never
//! `b_transport` (dev-plan D6).  A blocking target's debug transport is
//! synchronous, has no delay and is not counted as a transaction.
//!
//! [`RamSlot::begin_window`] is the reset entry point.  For a TLM slot it first
//! obtains the target's transport-reset acknowledgement, then zero-fills the
//! whole RAM window through debug transport, so no prior run's contents (or a
//! write the target held after an unknown completion) survive installation.

use crate::blocking::adapter::{PendingDelay, TlmRamAdapter};
use crate::blocking::{
    BlockingInitiator, BlockingTarget, DelayConversionError, Femtoseconds, TickPeriod,
};
use crate::memory::{contains_range, MemoryError, MemoryInterface, SimpleMemory};
use crate::physical::{
    native_ram_atomic_transact, AtomicBackend, AtomicBackendResult, AtomicRequest, PhysicalBackend,
    PhysicalBackendError, PhysicalBackendResult, PhysicalRequest, PhysicalResponse,
    PhysicalResponseBytes, PhysicalResponseCompletion, PhysicalTargetRejectionReason,
};
use std::fmt;
use std::sync::{Arc, Mutex};

/// Debug-transport bytes moved by one zero-fill request.
const ZERO_FILL_CHUNK: usize = 4096;

/// One selectable guest-RAM slot.
#[derive(Clone)]
pub(crate) struct RamSlot {
    inner: Arc<RamSlotInner>,
}

struct RamSlotInner {
    base: u64,
    size: usize,
    kind: RamKind,
}

enum RamKind {
    Native(Arc<Mutex<SimpleMemory>>),
    Tlm(TlmRamAdapter),
}

impl fmt::Debug for RamSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RamSlot")
            .field("base", &format_args!("{:#018x}", self.inner.base))
            .field("size", &self.inner.size)
            .field(
                "kind",
                &match self.inner.kind {
                    RamKind::Native(_) => "native",
                    RamKind::Tlm(_) => "tlm",
                },
            )
            .finish()
    }
}

impl RamSlot {
    /// The native `SimpleMemory` slot.
    pub(crate) fn native(memory: Arc<Mutex<SimpleMemory>>, base: u64, size: usize) -> Self {
        Self {
            inner: Arc::new(RamSlotInner {
                base,
                size,
                kind: RamKind::Native(memory),
            }),
        }
    }

    /// A TLM slot over one shared blocking target.
    ///
    /// The target's declared maximum delay must be representable under the
    /// Machine's declared tick period; otherwise the configuration fails
    /// before any access is issued.  `direct_mem` selects the direct-memory
    /// path and registers this slot's adapter with the target
    /// (direct-memory D2/D8).
    pub(crate) fn tlm(
        target: Arc<dyn BlockingTarget>,
        base: u64,
        size: usize,
        period: TickPeriod,
        direct_mem: bool,
    ) -> Result<Self, DelayConversionError> {
        Ok(Self {
            inner: Arc::new(RamSlotInner {
                base,
                size,
                kind: RamKind::Tlm(TlmRamAdapter::with_tick_period(target, period, direct_mem)?),
            }),
        })
    }

    /// The exact shared native `SimpleMemory`, when this is a native slot.
    pub(crate) fn native_memory(&self) -> Option<Arc<Mutex<SimpleMemory>>> {
        match &self.inner.kind {
            RamKind::Native(memory) => Some(memory.clone()),
            RamKind::Tlm(_) => None,
        }
    }

    /// Number of the TLM adapter's cached direct-memory grants, or zero on
    /// native and with direct memory disabled.
    #[cfg(test)]
    pub(crate) fn cached_direct_mem_regions(&self) -> usize {
        match &self.inner.kind {
            RamKind::Native(_) => 0,
            RamKind::Tlm(adapter) => adapter.cached_regions(),
        }
    }

    /// Installs the test-only pause point inside a direct copy.
    #[cfg(test)]
    pub(crate) fn set_direct_copy_hook(&self, hook: Option<Arc<dyn Fn() + Send + Sync>>) {
        if let RamKind::Tlm(adapter) = &self.inner.kind {
            adapter.set_direct_copy_hook(hook);
        }
    }

    /// True while a direct copy holds the direct-memory lock, for tests.
    #[cfg(test)]
    pub(crate) fn direct_memory_lock_held(&self) -> bool {
        match &self.inner.kind {
            RamKind::Native(_) => false,
            RamKind::Tlm(adapter) => adapter.direct_memory_lock_held(),
        }
    }

    /// Why the TLM transport refuses further transactions, if it does.
    #[cfg(test)]
    pub(crate) fn poison_reason(&self) -> Option<String> {
        match &self.inner.kind {
            RamKind::Native(_) => None,
            RamKind::Tlm(adapter) => adapter.poison_reason(),
        }
    }

    /// Exact total charged guest-turn transport delay so far (zero on native
    /// and for a zero-delay target).
    pub(crate) fn consumed_delay(&self) -> Femtoseconds {
        match &self.inner.kind {
            RamKind::Native(_) => Femtoseconds::ZERO,
            RamKind::Tlm(adapter) => adapter.consumed_delay(),
        }
    }

    /// Opens one guest turn's delay accounting.
    pub(crate) fn begin_transport_turn(&self) {
        if let RamKind::Tlm(adapter) = &self.inner.kind {
            adapter.begin_turn();
        }
    }

    /// Consumes one guest turn's transport delay once.
    pub(crate) fn take_transport_turn_delay(&self) -> PendingDelay {
        match &self.inner.kind {
            RamKind::Native(_) => PendingDelay::default(),
            RamKind::Tlm(adapter) => adapter.take_turn_delay(),
        }
    }

    /// Drops debug-work delay after validation without charging Platform time.
    pub(crate) fn discard_transport_turn_delay(&self) {
        if let RamKind::Tlm(adapter) = &self.inner.kind {
            adapter.discard_turn_delay();
        }
    }

    /// One guest turn's conservative completion bound in Platform ticks.
    pub(crate) fn turn_completion_bound_ticks(&self) -> Option<u64> {
        match &self.inner.kind {
            RamKind::Native(_) => None,
            RamKind::Tlm(adapter) => Some(adapter.turn_completion_bound_ticks()),
        }
    }

    /// Completed-write version for a TLM slot, or `None` on native.
    #[cfg(test)]
    pub(crate) fn committed_write_version(&self) -> Option<u64> {
        match &self.inner.kind {
            RamKind::Native(_) => None,
            RamKind::Tlm(adapter) => Some(adapter.committed_write_version()),
        }
    }

    /// Obtains the transport reset (TLM only), then zero-fills the whole RAM
    /// window through debug transport.
    ///
    /// A native Platform constructs a fresh zeroed `SimpleMemory`, so the
    /// native slot has nothing to reset.
    pub(crate) fn begin_window(&self) -> Result<(), MemoryError> {
        let RamKind::Tlm(adapter) = &self.inner.kind else {
            return Ok(());
        };
        adapter.transport_reset().map_err(memory_error)?;
        let zeros = [0u8; ZERO_FILL_CHUNK];
        let mut offset = 0;
        while offset < self.inner.size {
            let count = ZERO_FILL_CHUNK.min(self.inner.size - offset);
            adapter
                .debug_write(self.inner.base + offset as u64, &zeros[..count])
                .map_err(memory_error)?;
            offset += count;
        }
        Ok(())
    }

    /// One raw non-atomic RAM transaction at `offset` within the window.
    ///
    /// `local_base_ticks` is the turn's `L - E` (quantum D11), which only the
    /// TLM adapter publishes; the native slot has no transaction to carry it.
    pub(crate) fn raw(
        &self,
        request: &PhysicalRequest<'_>,
        offset: u64,
        initiator: BlockingInitiator,
        local_base_ticks: u64,
    ) -> PhysicalBackendResult {
        let width = request.width().bytes();
        self.check_span(offset, width)
            .map_err(|error| physical_target_error(error, request))?;
        match &self.inner.kind {
            RamKind::Native(memory) => {
                let mut memory = memory
                    .lock()
                    .map_err(|_| PhysicalBackendError::host("RAM lock poisoned"))?;
                match request.category() {
                    crate::physical::PhysicalAccessKind::Fetch
                    | crate::physical::PhysicalAccessKind::DataRead => {
                        let mut bytes = [0u8; crate::physical::MAX_PHYSICAL_ACCESS_BYTES];
                        memory
                            .read_bytes_into(offset, &mut bytes[..width])
                            .map_err(crate::physical::map_native_memory_error)?;
                        Ok(PhysicalResponse::new(
                            request.descriptor(),
                            PhysicalResponseCompletion::Read(PhysicalResponseBytes::from_slice(
                                &bytes[..width],
                            )),
                        ))
                    }
                    crate::physical::PhysicalAccessKind::DataWrite => {
                        let payload = request.write_payload().ok_or_else(|| {
                            PhysicalBackendError::protocol(
                                "validated RAM write was missing its payload",
                            )
                        })?;
                        memory
                            .write_bytes(offset, payload)
                            .map_err(crate::physical::map_native_memory_error)?;
                        Ok(PhysicalResponse::new(
                            request.descriptor(),
                            PhysicalResponseCompletion::WriteAcknowledgement,
                        ))
                    }
                }
            }
            RamKind::Tlm(adapter) => adapter.transact(request, initiator, local_base_ticks),
        }
    }

    /// One atomic RAM envelope at the request's physical address.
    pub(crate) fn atomic(
        &self,
        request: &AtomicRequest<'_>,
        local_base_ticks: u64,
    ) -> AtomicBackendResult {
        match &self.inner.kind {
            RamKind::Native(memory) => {
                native_ram_atomic_transact(memory, self.inner.base, self.inner.size, request)
            }
            RamKind::Tlm(adapter) => adapter.transact_atomic(request, local_base_ticks),
        }
    }

    /// Writes one span through debug transport, without counting it as a
    /// transaction.  Image placement uses this.
    pub(crate) fn program(&self, offset: u64, bytes: &[u8]) -> Result<(), MemoryError> {
        self.check_span(offset, bytes.len())?;
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .write_bytes(offset, bytes),
            RamKind::Tlm(adapter) => adapter
                .debug_write(self.inner.base + offset, bytes)
                .map_err(memory_error),
        }
    }

    /// One host-view read of raw bytes through debug transport.
    pub(crate) fn host_read(&self, offset: u64, len: usize) -> Result<Vec<u8>, MemoryError> {
        self.check_span(offset, len)?;
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .read_bytes(offset, len),
            RamKind::Tlm(adapter) => adapter
                .debug_read(self.inner.base + offset, len)
                .map_err(memory_error),
        }
    }

    fn typed_read(&self, offset: u64, width: usize) -> Result<Vec<u8>, MemoryError> {
        self.check_typed(offset, width)?;
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .read_bytes(offset, width),
            RamKind::Tlm(adapter) => adapter
                .debug_read(self.inner.base + offset, width)
                .map_err(memory_error),
        }
    }

    fn typed_write(&self, offset: u64, width: usize, bytes: &[u8]) -> Result<(), MemoryError> {
        self.check_typed(offset, width)?;
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .write_bytes(offset, bytes),
            RamKind::Tlm(adapter) => adapter
                .debug_write(self.inner.base + offset, bytes)
                .map_err(memory_error),
        }
    }

    fn check_span(&self, offset: u64, width: usize) -> Result<(), MemoryError> {
        if !contains_range(0, self.inner.size, offset, width) {
            return Err(MemoryError::InvalidAddress(offset));
        }
        Ok(())
    }

    fn check_typed(&self, offset: u64, width: usize) -> Result<(), MemoryError> {
        self.check_span(offset, width)?;
        if !offset.is_multiple_of(width as u64) {
            return Err(MemoryError::Misaligned(offset, width as u32));
        }
        Ok(())
    }

    /// Typed doubleword read over the selected slot.
    pub(crate) fn read_dword(&self, offset: u64) -> Result<u64, MemoryError> {
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .read_dword(offset),
            RamKind::Tlm(_) => {
                let bytes = self.typed_read(offset, 8)?;
                Ok(u64::from_le_bytes(bytes.try_into().expect("eight bytes")))
            }
        }
    }

    /// Typed word read over the selected slot.
    pub(crate) fn read_word(&self, offset: u64) -> Result<u32, MemoryError> {
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .read_word(offset),
            RamKind::Tlm(_) => {
                let bytes = self.typed_read(offset, 4)?;
                Ok(u32::from_le_bytes(bytes.try_into().expect("four bytes")))
            }
        }
    }

    /// Typed halfword read over the selected slot.
    pub(crate) fn read_half(&self, offset: u64) -> Result<u16, MemoryError> {
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .read_half(offset),
            RamKind::Tlm(_) => {
                let bytes = self.typed_read(offset, 2)?;
                Ok(u16::from_le_bytes(bytes.try_into().expect("two bytes")))
            }
        }
    }

    /// Typed byte read over the selected slot.
    pub(crate) fn read_byte(&self, offset: u64) -> Result<u8, MemoryError> {
        match &self.inner.kind {
            RamKind::Native(memory) => memory
                .lock()
                .map_err(|_| MemoryError::Backend("RAM lock poisoned".into()))?
                .read_byte(offset),
            RamKind::Tlm(_) => Ok(self.typed_read(offset, 1)?[0]),
        }
    }

    /// Typed doubleword write over the selected slot.
    pub(crate) fn write_dword(&self, offset: u64, value: u64) -> Result<(), MemoryError> {
        self.typed_write(offset, 8, &value.to_le_bytes())
    }

    /// Typed word write over the selected slot.
    pub(crate) fn write_word(&self, offset: u64, value: u32) -> Result<(), MemoryError> {
        self.typed_write(offset, 4, &value.to_le_bytes())
    }

    /// Typed halfword write over the selected slot.
    pub(crate) fn write_half(&self, offset: u64, value: u16) -> Result<(), MemoryError> {
        self.typed_write(offset, 2, &value.to_le_bytes())
    }

    /// Typed byte write over the selected slot.
    pub(crate) fn write_byte(&self, offset: u64, value: u8) -> Result<(), MemoryError> {
        self.typed_write(offset, 1, &[value])
    }
}

impl MemoryInterface for RamSlot {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        RamSlot::read_dword(self, addr)
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        RamSlot::read_word(self, addr)
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        RamSlot::read_half(self, addr)
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        RamSlot::read_byte(self, addr)
    }

    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(RamSlot::read_word(self, addr)? as u64)
    }

    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(RamSlot::read_half(self, addr)? as u64)
    }

    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok(RamSlot::read_byte(self, addr)? as u64)
    }

    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok((RamSlot::read_word(self, addr)? as i32) as i64 as u64)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok((RamSlot::read_half(self, addr)? as i16) as i64 as u64)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        Ok((RamSlot::read_byte(self, addr)? as i8) as i64 as u64)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        RamSlot::write_dword(self, addr, value)
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        RamSlot::write_word(self, addr, value)
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        RamSlot::write_half(self, addr, value)
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        RamSlot::write_byte(self, addr, value)
    }

    fn size(&self) -> usize {
        self.inner.size
    }
}

impl PhysicalBackend for RamSlot {
    fn transact(&mut self, request: &PhysicalRequest<'_>) -> PhysicalBackendResult {
        let width = request.width().bytes();
        if !contains_range(self.inner.base, self.inner.size, request.paddr(), width) {
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                format!(
                    "RAM window does not contain the complete {:?} span at {:#018x} \
                     ({} bytes)",
                    request.category(),
                    request.paddr(),
                    width
                ),
            ));
        }
        let offset = request.paddr() - self.inner.base;
        // This direct slot path carries no turn clock, because the Hart's own
        // ports are the bus, which publishes `L - E` itself (quantum D11).
        self.raw(request, offset, BlockingInitiator::Hart, 0)
    }
}

impl AtomicBackend for RamSlot {
    fn transact_atomic(&mut self, request: &AtomicRequest<'_>) -> AtomicBackendResult {
        let width = request.width().bytes();
        if !contains_range(self.inner.base, self.inner.size, request.paddr(), width) {
            return Err(PhysicalBackendError::target(
                PhysicalTargetRejectionReason::Unmapped,
                format!(
                    "RAM window does not contain the complete {} span at {:#018x} \
                     ({} bytes)",
                    match request.kind() {
                        crate::physical::AtomicAccessKind::Rmw => "atomic",
                        crate::physical::AtomicAccessKind::LoadReserved => "load-reserved",
                        crate::physical::AtomicAccessKind::StoreConditional => {
                            "store-conditional"
                        }
                    },
                    request.paddr(),
                    width
                ),
            ));
        }
        self.atomic(request, 0)
    }
}

fn memory_error(error: PhysicalBackendError) -> MemoryError {
    match error {
        PhysicalBackendError::Host { context } => MemoryError::Backend(context),
        PhysicalBackendError::Protocol { context } => MemoryError::Protocol(context),
        PhysicalBackendError::Unknown { context } => MemoryError::Unknown(context),
        PhysicalBackendError::Target { context, .. } => MemoryError::Backend(context),
    }
}

fn physical_target_error(
    error: MemoryError,
    request: &PhysicalRequest<'_>,
) -> PhysicalBackendError {
    match error {
        MemoryError::InvalidAddress(address) => PhysicalBackendError::target(
            crate::physical::PhysicalTargetRejectionReason::Unmapped,
            format!(
                "RAM window does not contain the complete {:?} span at {address:#018x} \
                 ({} bytes, request at {:#018x})",
                request.category(),
                request.width().bytes(),
                request.paddr()
            ),
        ),
        other => crate::physical::map_native_memory_error(other),
    }
}
