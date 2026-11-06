//! Existing Platform configurations and placement, without run-stop policy.
use super::inbound::InboundPort;
use super::{MachineConfig, MachineError, PlatformKind};
use crate::blocking::Transport;
use crate::core::{SharedDataAccess, SharedPhysicalAccess};
use crate::elf::LoadImage;
use crate::executor::{
    InboundSystemBusBackend, PresentedInterruptLines, SystemBus, SYSTEM_BUS_HTIF_BASE,
    SYSTEM_BUS_UART_BASE,
};
use crate::image::{AddressForm, ImagePlacement};
use crate::memory::{MemoryInterface, SimpleMemory};
use crate::peripherals::uart16550::{Uart16550, UartInspection};
use crate::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use crate::ram::RamSlot;
use std::sync::{Arc, Mutex};

pub(super) type SharedMemory = Arc<Mutex<dyn MemoryInterface + Send + Sync>>;

/// Raw Platform facts; only Runner decodes/classifies a terminal exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformEvent {
    HtifWrite(u64),
    UartTransmit(u8),
}

pub(super) struct Platform {
    /// The single selectable guest-RAM slot.  Every RAM user in this Platform
    /// reaches memory through this object.
    pub ram: RamSlot,
    pub typed: SharedMemory,
    pub fetch: SharedPhysicalAccess,
    pub data: SharedDataAccess,
    /// The DM's inbound physical port (ADR-0002 §9). A separate validated
    /// view over the same RAM/device backend as `data`, never the Hart's own
    /// port: no translation, PMP, trap or atomic envelope.
    pub inbound: InboundPort,
    pub form: AddressForm,
    pub placement: ImagePlacement,
    pub selected_tohost: u64,
    pub ram_tohost_offset: Option<u64>,
    uart: Option<Arc<Mutex<Uart16550>>>,
    events: Arc<Mutex<Vec<PlatformEvent>>>,
    /// Native CLINT. Flat has no device window.
    bus: Option<Arc<Mutex<SystemBus>>>,
    /// Flat `mtime`. Native time lives in the CLINT.
    flat_mtime: u64,
}
impl Platform {
    pub fn build(config: &MachineConfig, image: &LoadImage) -> Result<Self, MachineError> {
        let placement = ImagePlacement::new(
            image.base_addr(),
            image.memory_size(),
            image.tohost(),
            image.signature().cloned(),
        );
        let form = match config.platform {
            PlatformKind::Native => AddressForm::Bus,
            PlatformKind::Flat => placement.address_form(),
        };
        let declared = placement
            .tohost(form)
            .map_err(|error| MachineError::Placement(error.to_string()))?;
        if config.platform == PlatformKind::Flat
            && declared.is_some_and(|offset| !offset.is_multiple_of(8))
        {
            return Err(MachineError::Placement(
                "ELF tohost is not eight-byte aligned at its flat offset".into(),
            ));
        }
        let selected_tohost = match config.platform {
            PlatformKind::Native => config.tohost_override.or(declared),
            PlatformKind::Flat => declared.or(config.tohost_override),
        }
        .unwrap_or(SYSTEM_BUS_HTIF_BASE);
        // A TLM route needs the native Platform's device map; the flat
        // configuration has no device transport and no TLM route.
        if config.platform == PlatformKind::Flat && !config.transport.is_native() {
            return Err(MachineError::UnsupportedTransport);
        }
        // Direct memory is a property of the blocking route, so a native
        // selection that enables it has no transport to grant it
        // (direct-memory D2).
        if config.dmi && config.transport.is_native() {
            return Err(MachineError::UnsupportedTransport);
        }
        // quantum D2: a non-zero quantum is a TLM-route selection. The native
        // route has no local offset to bound, and a flat Platform has no TLM
        // route at all.
        if config.quantum != 0 && config.transport.is_native() {
            return Err(MachineError::UnsupportedTransport);
        }
        let ram = match &config.transport {
            Transport::Native => RamSlot::native(
                Arc::new(Mutex::new(SimpleMemory::new(image.memory_size()))),
                image.base_addr(),
                image.memory_size(),
            ),
            Transport::Tlm(target) => RamSlot::tlm(
                target.clone(),
                image.base_addr(),
                image.memory_size(),
                config.tick_period,
                config.dmi,
            )
            .map_err(|error| {
                MachineError::Placement(format!(
                    "blocking target declares an invalid maximum delay: {error}"
                ))
            })?,
        };
        // After every transport reset and before installation, zero-fill the
        // whole RAM window so no prior run's contents survive (dev-plan D6).
        ram.begin_window()?;
        // quantum D2: the selected quantum's declared maximum turn bound
        // `(Q + t + 1) x tick_period` must fit as u64 femtoseconds, where `t`
        // is the route's conservative turn completion bound.  The check does
        // not run for `Q` = 0, which accepts everything accepted today.
        if config.quantum != 0 {
            let turn_bound = ram.turn_completion_bound_ticks().unwrap_or(0);
            let period = config.tick_period.femtoseconds().as_u64();
            let represented = config
                .quantum
                .checked_add(turn_bound)
                .and_then(|sum| sum.checked_add(1))
                .and_then(|sum| sum.checked_mul(period));
            if represented.is_none() {
                return Err(MachineError::Placement(format!(
                    "quantum {} ticks plus the route's turn completion bound of {} ticks cannot \
                     be represented under the declared tick period of {} fs",
                    config.quantum, turn_bound, period
                )));
            }
        }
        Self::place(&ram, image)?;
        let events = Arc::new(Mutex::new(Vec::new()));
        let (typed, fetch, data, uart, bus): (
            SharedMemory,
            SharedPhysicalAccess,
            SharedDataAccess,
            _,
            _,
        ) = match config.platform {
            PlatformKind::Flat => {
                let memory = ram
                    .native_memory()
                    .ok_or(MachineError::UnsupportedTransport)?;
                let port = || {
                    Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                        NativeRamBackend::new(memory.clone(), 0, image.memory_size()),
                    )))
                };
                let fetch = port();
                let data = port();
                (memory, fetch, data, None, None)
            }
            PlatformKind::Native => {
                let uart = Arc::new(Mutex::new(Uart16550::new(SYSTEM_BUS_UART_BASE)));
                let queue = events.clone();
                let output = config.uart_output.clone();
                uart.lock()
                    .map_err(|_| MachineError::Poisoned)?
                    .set_output_callback(move |byte| {
                        queue
                            .lock()
                            .expect("Platform event lock")
                            .push(PlatformEvent::UartTransmit(byte));
                        if let Some(callback) = &output {
                            callback(byte);
                        }
                    });
                let bus = Arc::new(Mutex::new(SystemBus::with_ram_slot(
                    ram.clone(),
                    uart.clone(),
                    image.base_addr(),
                    image.memory_size(),
                )));
                let queue = events.clone();
                let callback = config.htif_write.clone();
                bus.lock()
                    .map_err(|_| MachineError::Poisoned)?
                    .set_htif_write_callback(move |value| {
                        queue
                            .lock()
                            .expect("Platform event lock")
                            .push(PlatformEvent::HtifWrite(value));
                        if let Some(callback) = &callback {
                            callback(value);
                        }
                    });
                let port = || {
                    Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                        SystemBus::physical_backend(bus.clone()),
                    )))
                };
                (bus.clone(), port(), port(), Some(uart), Some(bus))
            }
        };
        // The inbound master is the DM, not the Hart. It shares the routing,
        // raw-byte and side-effect semantics through the same backend object
        // and lock domain, but it gets its own port so no Hart fetch/data
        // capability (translation delta, host admission) can serve it.
        let inbound_port: SharedPhysicalAccess = match &bus {
            Some(bus) => Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                InboundSystemBusBackend::new(bus.clone()),
            ))),
            None => {
                let memory = ram
                    .native_memory()
                    .ok_or(MachineError::UnsupportedTransport)?;
                Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                    NativeRamBackend::new(memory, 0, image.memory_size()),
                )))
            }
        };
        let offset = match config.platform {
            PlatformKind::Native => selected_tohost.checked_sub(image.base_addr()),
            PlatformKind::Flat => Some(selected_tohost),
        };
        let ram_tohost_offset = offset
            .filter(|offset| crate::memory::contains_range(0, image.memory_size(), *offset, 8));
        Ok(Self {
            ram,
            typed,
            fetch,
            data,
            inbound: InboundPort::new(inbound_port),
            form,
            placement,
            selected_tohost,
            ram_tohost_offset,
            uart,
            events,
            bus,
            flat_mtime: 0,
        })
    }

    /// One DM-originated system bus access through the inbound port.
    ///
    /// [`crate::machine::Machine`]'s hart owner calls this at a coherent Hart
    /// boundary. The DM lock is not held here; this method only reaches the
    /// Platform's target. A target refusal is `sberror` material, while a
    /// simulator-side failure must keep its own classification.
    pub(super) fn sba_access(
        &self,
        request: crate::dm::SbaRequest,
    ) -> Result<u64, super::inbound::SbaAccessError> {
        self.inbound.access(request)
    }

    pub(super) fn read_mtime(&self) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.mtime());
        }
        Ok(self.flat_mtime)
    }

    /// Exact total charged blocking-transport delay in this Platform's RAM
    /// slot.  Zero on native and for a zero-delay target, and unchanged by
    /// debug work whose delay is validated but not charged.
    pub(super) fn transport_delay_femtoseconds(&self) -> u64 {
        self.ram.consumed_delay().as_u64()
    }

    /// Opens one guest turn's transport-delay accounting.
    pub(super) fn begin_transport_turn(&self) {
        self.ram.begin_transport_turn();
    }

    /// Consumes one guest turn's transport delay into Platform time once.
    pub(super) fn take_transport_turn_delay(&self) -> crate::blocking::adapter::PendingDelay {
        self.ram.take_transport_turn_delay()
    }

    /// Drops debug-work delay after validation, without charging Platform
    /// time (ADR-0004 2026-10-28 and 2026-10-30 amendments).
    pub(super) fn discard_transport_turn_delay(&self) {
        self.ram.discard_transport_turn_delay();
    }

    /// One guest turn's conservative completion bound in Platform ticks.
    pub(super) fn turn_completion_bound_ticks(&self) -> Option<u64> {
        self.ram.turn_completion_bound_ticks()
    }

    /// Hart 0 lines. Flat has no CLINT or PLIC, so every line stays clear.
    pub(super) fn machine_interrupt_lines(&self) -> Result<PresentedInterruptLines, MachineError> {
        if let Some(bus) = &self.bus {
            let bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.machine_interrupt_lines());
        }
        Ok(PresentedInterruptLines::clear())
    }

    /// Hart 0 `mtimecmp`, if this platform has a CLINT.
    pub(super) fn read_mtimecmp(&self) -> Result<Option<u64>, MachineError> {
        if let Some(bus) = &self.bus {
            let bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.mtimecmp());
        }
        Ok(None)
    }

    /// Replace `mtime`. Does not tick `mcycle` or `minstret`.
    pub(super) fn set_mtime(&mut self, value: u64) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let mut bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            bus.set_mtime(value);
            return Ok(bus.mtime());
        }
        self.flat_mtime = value;
        Ok(self.flat_mtime)
    }

    /// One completed Hart turn. A `SimulatorFailure` does not call this.
    /// Returns the `mtime` visible to the next instruction.
    pub(super) fn advance_mtime(&mut self) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let mut bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            bus.advance_mtime();
            return Ok(bus.mtime());
        }
        self.flat_mtime = self.flat_mtime.wrapping_add(1);
        Ok(self.flat_mtime)
    }

    /// Add `ticks` Platform ticks without touching `mcycle` or `minstret`.
    ///
    /// Transport-derived time uses this checked addition, so modeled time
    /// never wraps.  A completed turn with no transport delay keeps the
    /// existing native one-tick path.
    pub(super) fn advance_mtime_by(&mut self, ticks: u64) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let mut bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            let next = bus.mtime().checked_add(ticks).ok_or_else(|| {
                MachineError::Placement(format!(
                    "Platform mtime overflow advancing by {ticks} ticks"
                ))
            })?;
            bus.set_mtime(next);
            return Ok(bus.mtime());
        }
        self.flat_mtime = self.flat_mtime.checked_add(ticks).ok_or_else(|| {
            MachineError::Placement(format!(
                "Platform mtime overflow advancing by {ticks} ticks"
            ))
        })?;
        Ok(self.flat_mtime)
    }

    /// quantum D3: the Hart's local time `L`, committed time plus the local
    /// offset with wrapping addition.  The offset is zero when the quantum is
    /// off, so this is committed time on the pre-quantum path.
    pub(super) fn read_local_time(&self) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.mtime().wrapping_add(bus.local_offset()));
        }
        Ok(self.flat_mtime)
    }

    /// quantum D3: the Platform's local offset `O` in Platform ticks.
    pub(super) fn local_offset(&self) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.local_offset());
        }
        Ok(0)
    }

    /// quantum D3/D10: the cumulative boundary-sync count of this Platform.
    pub(super) fn quantum_syncs(&self) -> Result<u64, MachineError> {
        if let Some(bus) = &self.bus {
            let bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.quantum_syncs());
        }
        Ok(0)
    }

    /// quantum D4: one turn's `advance` enters the local offset `O` in place
    /// of committed time.  A turn with delay ticks is checked and a refused
    /// advance enters neither committed time nor the offset; a turn with no
    /// delay ticks wraps `L` like the native one-tick path (quantum F10).
    pub(super) fn accumulate_local_offset(
        &self,
        advance: u64,
        checked: bool,
    ) -> Result<(), MachineError> {
        if let Some(bus) = &self.bus {
            let mut bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return bus
                .accumulate_local_offset(advance, checked)
                .map_err(MachineError::Placement);
        }
        // A non-zero quantum is refused without the native bus (quantum D2),
        // so a flat Platform never accumulates an offset.
        Ok(())
    }

    /// quantum D3: one boundary sync.  Committed time takes the local offset
    /// and the offset returns to zero under the bus lock; the sync is counted
    /// once when it moved committed time.  It makes no target call, charges no
    /// delay and changes no architectural or delay counter.  Returns whether
    /// it moved committed time.
    pub(super) fn commit_local_time(&self) -> Result<bool, MachineError> {
        if let Some(bus) = &self.bus {
            let mut bus = bus.lock().map_err(|_| MachineError::Poisoned)?;
            return Ok(bus.commit_local_time());
        }
        Ok(false)
    }

    /// One active RAM allocation; image descriptions are immutable restoration
    /// inputs. Preserve segment order, explicit zero tails, gaps and padding.
    ///
    /// Placement uses the selected slot's debug transport, never
    /// `b_transport` (dev-plan D6).
    fn place(ram: &RamSlot, image: &LoadImage) -> Result<(), MachineError> {
        for segment in image.segments() {
            let offset = segment
                .guest_address
                .checked_sub(image.base_addr())
                .ok_or_else(|| MachineError::Placement("segment below base".into()))?;
            if !segment.file_bytes.is_empty() {
                ram.program(offset, &segment.file_bytes)?;
            }
            let mut zero_offset = offset
                .checked_add(segment.file_bytes.len() as u64)
                .ok_or_else(|| MachineError::Placement("segment overflow".into()))?;
            let mut remaining = segment.zero_fill;
            let zeros = [0; 4096];
            while remaining != 0 {
                let count = remaining.min(zeros.len());
                ram.program(zero_offset, &zeros[..count])?;
                zero_offset += count as u64;
                remaining -= count;
            }
        }
        Ok(())
    }
    pub fn take_events(&self) -> Result<Vec<PlatformEvent>, MachineError> {
        Ok(std::mem::take(
            &mut *self.events.lock().map_err(|_| MachineError::Poisoned)?,
        ))
    }
    pub fn queued_events(&self) -> Result<Vec<PlatformEvent>, MachineError> {
        Ok(self
            .events
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .clone())
    }
    pub fn tohost_value(&self) -> Result<u64, crate::memory::MemoryError> {
        self.typed
            .lock()
            .map_err(|_| crate::memory::MemoryError::Backend("poisoned Platform".into()))?
            .read_dword(self.selected_tohost)
    }
    pub fn uart_inspection(&self) -> Result<Option<UartInspection>, MachineError> {
        self.uart
            .as_ref()
            .map(|uart| {
                uart.lock()
                    .map(|uart| uart.inspect())
                    .map_err(|_| MachineError::Poisoned)
            })
            .transpose()
    }
    pub fn receive_uart(&self, byte: u8) -> Result<(), MachineError> {
        self.uart
            .as_ref()
            .ok_or(MachineError::NoUart)?
            .lock()
            .map_err(|_| MachineError::Poisoned)?
            .receive_byte(byte);
        Ok(())
    }
    pub fn signature(&self, image: &LoadImage) -> Option<Result<Vec<u8>, String>> {
        image.signature().map(|info| {
            // Empty artifacts preserve metadata without mapping or target reads
            // (ADR-0003 §7), even when the declared address is outside RAM.
            if info.size == 0 {
                return Ok(Vec::new());
            }
            let addr = self
                .placement
                .signature_address(info, self.form)
                .map_err(|error| error.to_string())?;
            let len = usize::try_from(info.size)
                .map_err(|_| "signature size exceeds host capacity".to_owned())?;
            let memory = self
                .typed
                .lock()
                .map_err(|_| "poisoned Platform".to_owned())?;
            (0..len)
                .map(|offset| {
                    addr.checked_add(offset as u64)
                        .ok_or_else(|| "signature address overflow".to_owned())
                        .and_then(|address| {
                            memory.read_byte(address).map_err(|error| error.to_string())
                        })
                })
                .collect()
        })
    }
}
