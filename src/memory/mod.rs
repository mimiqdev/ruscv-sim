//! Memory Interface Module
//!
//! Defines a generic interface for memory access and a simple implementation.

use std::sync::RwLock;
use thiserror::Error;

/// Memory errors
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Invalid memory address: 0x{0:016x}")]
    InvalidAddress(u64),
    #[error("Misaligned access: addr 0x{0:016x}, requires {1}-byte alignment")]
    Misaligned(u64, u32),
    #[error("Memory access out of bounds")]
    OutOfBounds,
}

/// Memory interface trait (supports RV64I with 64-bit addresses)
pub trait MemoryInterface {
    /// Read double word (8 bytes) - RV64I
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read word (4 bytes)
    fn read_word(&self, addr: u64) -> Result<u32, MemoryError>;
    /// Read half word (2 bytes)
    fn read_half(&self, addr: u64) -> Result<u16, MemoryError>;
    /// Read byte (1 byte)
    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError>;
    /// Read word (zero-extended to 64 bits)
    fn read_word_zext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read half word (zero-extended to 64 bits)
    fn read_half_zext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read byte (zero-extended to 64 bits)
    fn read_byte_zext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read word (sign-extended to 64 bits)
    fn read_word_sext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read half word (sign-extended to 64 bits)
    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError>;
    /// Read byte (sign-extended to 64 bits)
    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError>;

    /// Write double word (8 bytes) - RV64I
    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError>;
    /// Write word (4 bytes)
    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError>;
    /// Write half word (2 bytes)
    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError>;
    /// Write byte (1 byte)
    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError>;

    /// Get memory size
    fn size(&self) -> usize;
}

/// Simple memory implementation
pub struct SimpleMemory {
    /// Memory data (using RwLock for thread-safe reads/writes)
    data: RwLock<Vec<u8>>,
    /// Memory size
    size: usize,
}

impl SimpleMemory {
    /// Creates a new simple memory block.
    pub fn new(size: usize) -> Self {
        Self {
            data: RwLock::new(vec![0; size]),
            size,
        }
    }

    /// Initializes memory from existing data.
    pub fn from_data(data: Vec<u8>) -> Self {
        let size = data.len();
        Self {
            data: RwLock::new(data),
            size,
        }
    }

    /// Loads program data (little-endian).
    ///
    /// The data is loaded starting at a relative offset of 0.
    /// The `_base_addr` parameter is ignored and kept only for API compatibility.
    ///
    /// # BREAKING CHANGE
    ///
    /// This function previously used `base_addr` to determine the write offset.
    /// It now writes to the start of the memory block, ignoring the base address.
    /// This change was made to simplify memory loading, as the `SystemBus` now
    /// handles address mapping.
    pub fn load_program(&self, data: &[u8], _base_addr: u64) {
        let mut mem = self.data.write().unwrap();
        for (i, &byte) in data.iter().enumerate() {
            if i < self.size {
                mem[i] = byte;
            }
        }
    }
}

impl MemoryInterface for SimpleMemory {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        let addr = addr as usize;
        if addr + 8 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        if !addr.is_multiple_of(8) {
            return Err(MemoryError::Misaligned(addr as u64, 8));
        }

        let data = self.data.read().unwrap();
        Ok(u64::from_le_bytes([
            data[addr],
            data[addr + 1],
            data[addr + 2],
            data[addr + 3],
            data[addr + 4],
            data[addr + 5],
            data[addr + 6],
            data[addr + 7],
        ]))
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        let addr = addr as usize;
        if addr + 4 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        if !addr.is_multiple_of(4) {
            return Err(MemoryError::Misaligned(addr as u64, 4));
        }

        let data = self.data.read().unwrap();
        Ok(u32::from_le_bytes([
            data[addr],
            data[addr + 1],
            data[addr + 2],
            data[addr + 3],
        ]))
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        let addr = addr as usize;
        if addr + 2 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        if !addr.is_multiple_of(2) {
            return Err(MemoryError::Misaligned(addr as u64, 2));
        }

        let data = self.data.read().unwrap();
        Ok(u16::from_le_bytes([data[addr], data[addr + 1]]))
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        let addr = addr as usize;
        if addr >= self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        let data = self.data.read().unwrap();
        Ok(data[addr])
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
        let val = self.read_word(addr)?;
        Ok((val as i32) as i64 as u64)
    }

    fn read_half_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        let val = self.read_half(addr)?;
        Ok((val as i16) as i64 as u64)
    }

    fn read_byte_sext(&self, addr: u64) -> Result<u64, MemoryError> {
        let val = self.read_byte(addr)?;
        Ok((val as i8) as i64 as u64)
    }

    fn write_dword(&mut self, addr: u64, value: u64) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr + 8 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        if !addr.is_multiple_of(8) {
            return Err(MemoryError::Misaligned(addr as u64, 8));
        }

        let mut data = self.data.write().unwrap();
        let bytes = value.to_le_bytes();
        data[addr] = bytes[0];
        data[addr + 1] = bytes[1];
        data[addr + 2] = bytes[2];
        data[addr + 3] = bytes[3];
        data[addr + 4] = bytes[4];
        data[addr + 5] = bytes[5];
        data[addr + 6] = bytes[6];
        data[addr + 7] = bytes[7];
        Ok(())
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr + 4 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        if !addr.is_multiple_of(4) {
            return Err(MemoryError::Misaligned(addr as u64, 4));
        }

        let mut data = self.data.write().unwrap();
        let bytes = value.to_le_bytes();
        data[addr] = bytes[0];
        data[addr + 1] = bytes[1];
        data[addr + 2] = bytes[2];
        data[addr + 3] = bytes[3];
        Ok(())
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr + 2 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        if !addr.is_multiple_of(2) {
            return Err(MemoryError::Misaligned(addr as u64, 2));
        }

        let mut data = self.data.write().unwrap();
        let bytes = value.to_le_bytes();
        data[addr] = bytes[0];
        data[addr + 1] = bytes[1];
        Ok(())
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr >= self.size {
            return Err(MemoryError::InvalidAddress(addr as u64));
        }
        let mut data = self.data.write().unwrap();
        data[addr] = value;
        Ok(())
    }

    fn size(&self) -> usize {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_read_write() {
        let mut mem = SimpleMemory::new(1024);

        // Write and read word
        mem.write_word(0x100, 0x12345678).unwrap();
        assert_eq!(mem.read_word(0x100).unwrap(), 0x12345678);

        // Write and read half word
        mem.write_half(0x200, 0xABCD).unwrap();
        assert_eq!(mem.read_half(0x200).unwrap(), 0xABCD);

        // Write and read byte
        mem.write_byte(0x300, 0x42).unwrap();
        assert_eq!(mem.read_byte(0x300).unwrap(), 0x42);
    }

    #[test]
    fn test_memory_misaligned() {
        let mut mem = SimpleMemory::new(1024);

        // Misaligned word access
        assert!(mem.read_word(0x101).is_err());
        assert!(mem.write_word(0x101, 0).is_err());

        // Misaligned half word access
        assert!(mem.read_half(0x101).is_err());
        assert!(mem.write_half(0x101, 0).is_err());
    }

    #[test]
    fn test_load_program() {
        let mem = SimpleMemory::new(0x2000); // 8KB, enough for 0x1000 address
        let program = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

        mem.load_program(&program, 0x1000); // base_addr is ignored, uses relative offset

        assert_eq!(mem.read_byte(0).unwrap(), 0x01);
        assert_eq!(mem.read_byte(7).unwrap(), 0x08);
        assert_eq!(mem.read_word(0).unwrap(), 0x04030201); // little-endian
    }
}
