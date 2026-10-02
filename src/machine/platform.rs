//! Existing Platform configurations and placement, without run-stop policy.
use super::{MachineConfig, MachineError, PlatformKind};
use crate::core::{SharedDataAccess, SharedPhysicalAccess};
use crate::elf::LoadImage;
use crate::executor::{SystemBus, SYSTEM_BUS_HTIF_BASE, SYSTEM_BUS_UART_BASE};
use crate::image::{AddressForm, ImagePlacement};
use crate::memory::{MemoryInterface, SimpleMemory};
use crate::peripherals::uart16550::{Uart16550, UartInspection};
use crate::physical::{NativeRamBackend, ValidatedPhysicalAccess};
use std::sync::{Arc, Mutex};

pub(super) type SharedMemory = Arc<Mutex<dyn MemoryInterface + Send + Sync>>;

/// Raw Platform facts; only Runner decodes/classifies a terminal exit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformEvent {
    HtifWrite(u64),
    UartTransmit(u8),
}

pub(super) struct Platform {
    pub ram: Arc<Mutex<SimpleMemory>>,
    pub typed: SharedMemory,
    pub fetch: SharedPhysicalAccess,
    pub data: SharedDataAccess,
    pub form: AddressForm,
    pub placement: ImagePlacement,
    pub selected_tohost: u64,
    pub ram_tohost_offset: Option<u64>,
    uart: Option<Arc<Mutex<Uart16550>>>,
    events: Arc<Mutex<Vec<PlatformEvent>>>,
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
        let ram = Arc::new(Mutex::new(SimpleMemory::new(image.memory_size())));
        Self::place(&ram, image)?;
        let events = Arc::new(Mutex::new(Vec::new()));
        let (typed, fetch, data, uart): (SharedMemory, SharedPhysicalAccess, SharedDataAccess, _) =
            match config.platform {
                PlatformKind::Flat => {
                    let port = || {
                        Arc::new(Mutex::new(ValidatedPhysicalAccess::new(
                            NativeRamBackend::new(ram.clone(), 0, image.memory_size()),
                        )))
                    };
                    (ram.clone(), port(), port(), None)
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
                    let bus = Arc::new(Mutex::new(SystemBus::new(
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
                    (bus.clone(), port(), port(), Some(uart))
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
            form,
            placement,
            selected_tohost,
            ram_tohost_offset,
            uart,
            events,
        })
    }

    /// One active RAM allocation; image descriptions are immutable restoration
    /// inputs. Preserve segment order, explicit zero tails, gaps and padding.
    fn place(ram: &Arc<Mutex<SimpleMemory>>, image: &LoadImage) -> Result<(), MachineError> {
        let mut ram = ram.lock().map_err(|_| MachineError::Poisoned)?;
        for segment in image.segments() {
            let offset = segment
                .guest_address
                .checked_sub(image.base_addr())
                .ok_or_else(|| MachineError::Placement("segment below base".into()))?;
            if !segment.file_bytes.is_empty() {
                ram.write_bytes(offset, &segment.file_bytes)?;
            }
            let mut zero_offset = offset
                .checked_add(segment.file_bytes.len() as u64)
                .ok_or_else(|| MachineError::Placement("segment overflow".into()))?;
            let mut remaining = segment.zero_fill;
            let zeros = [0; 4096];
            while remaining != 0 {
                let count = remaining.min(zeros.len());
                ram.write_bytes(zero_offset, &zeros[..count])?;
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
