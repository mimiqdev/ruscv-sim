use super::admission::{Gate, Lease};
use crate::memory::{MemoryError, MemoryInterface, SimpleMemory};
use std::sync::{Arc, Mutex};

#[cfg(test)]
type BeforeWrite = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

#[derive(Clone)]
pub(super) struct HostMemory {
    pub ram: Arc<Mutex<SimpleMemory>>,
    pub gate: Gate,
    pub generation: u64,
    pub size: usize,
    #[cfg(test)]
    pub before_lock: BeforeWrite,
}
impl HostMemory {
    fn with_write<T>(
        &self,
        write: impl FnOnce(&mut SimpleMemory) -> Result<T, MemoryError>,
    ) -> Result<T, MemoryError> {
        let _lease = Lease::host(&self.gate, self.generation)
            .map_err(|error| MemoryError::Backend(error.to_string()))?;
        #[cfg(test)]
        {
            let hook = self.before_lock.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook();
            }
        }
        let mut ram = self
            .ram
            .lock()
            .map_err(|_| MemoryError::Backend("poisoned RAM".into()))?;
        write(&mut ram)
    }
    pub fn write_bytes(&self, addr: u64, bytes: &[u8]) -> Result<(), MemoryError> {
        // Compatibility write_mem commits accepted byte prefixes, not a new bulk
        // all-or-nothing host transaction. One admission covers the entire call.
        self.with_write(|ram| {
            for (index, byte) in bytes.iter().enumerate() {
                let address = addr
                    .checked_add(index as u64)
                    .ok_or(MemoryError::InvalidAddress(addr))?;
                ram.write_byte(address, *byte)?;
            }
            Ok(())
        })
    }
}
macro_rules! reads {
    ($($name:ident -> $ty:ty),* $(,)?) => {$ (
        fn $name(&self, addr: u64) -> Result<$ty, MemoryError> {
            self.ram.lock().map_err(|_| MemoryError::Backend("poisoned RAM".into()))?.$name(addr)
        }
    )*};
}
macro_rules! writes {
    ($($name:ident: $ty:ty),* $(,)?) => {$ (
        fn $name(&mut self, addr: u64, value: $ty) -> Result<(), MemoryError> {
            self.with_write(|ram| ram.$name(addr, value))
        }
    )*};
}
impl MemoryInterface for HostMemory {
    reads! { read_dword -> u64, read_word -> u32, read_half -> u16, read_byte -> u8,
    read_word_zext -> u64, read_half_zext -> u64, read_byte_zext -> u64,
    read_word_sext -> u64, read_half_sext -> u64, read_byte_sext -> u64 }
    writes! { write_dword: u64, write_word: u32, write_half: u16, write_byte: u8 }
    fn size(&self) -> usize {
        self.size
    }
}
