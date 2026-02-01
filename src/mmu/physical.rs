//! Physical memory management
//!
//! Provides physical memory emulation for the RISC-V MMU subsystem.
//! Supports RAM allocation with read/write operations and alignment checking.

use crate::memory::MemoryError;

/// Memory region type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// Main memory (DRAM)
    Ram,
    /// Read-only memory (ROM/Boot ROM)
    Rom,
    /// Memory-mapped I/O
    Mmio,
    /// Reserved/unmapped
    Reserved,
}

/// Memory attributes
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryAttributes {
    /// Cacheable region
    pub cacheable: bool,
    /// Bufferable region
    pub bufferable: bool,
    /// Device memory
    pub device: bool,
}

/// Memory region descriptor
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Start physical address
    pub start: u64,
    /// Size in bytes
    pub size: usize,
    /// Region type
    pub region_type: MemoryRegionType,
    /// Memory attributes
    pub attributes: MemoryAttributes,
}

/// Physical memory interface
///
/// This trait defines the interface for accessing physical memory.
/// Used by the page table walker to read/write page table entries.
pub trait PhysicalMemoryInterface: Send + Sync {
    /// Read a byte from physical memory
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError>;
    /// Write a byte to physical memory
    fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError>;
    /// Read a doubleword (8 bytes) from physical memory
    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError>;
    /// Write a doubleword (8 bytes) to physical memory
    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError>;
}

/// Physical memory manager
///
/// Manages physical RAM and provides access methods with alignment checking.
pub struct PhysicalMemory {
    regions: Vec<MemoryRegion>,
    ram: Vec<u8>,
    ram_base: u64,
}

impl PhysicalMemory {
    /// Create a new physical memory manager with the specified RAM region
    ///
    /// # Arguments
    /// * `ram_base` - Base physical address of RAM
    /// * `ram_size` - Size of RAM in bytes
    ///
    /// # Example
    /// ```
    /// use ruscv_sim::mmu::PhysicalMemory;
    /// let mem = PhysicalMemory::new(0x8000_0000, 0x1000);
    /// ```
    pub fn new(ram_base: u64, ram_size: usize) -> Self {
        let regions = vec![MemoryRegion {
            start: ram_base,
            size: ram_size,
            region_type: MemoryRegionType::Ram,
            attributes: MemoryAttributes {
                cacheable: true,
                bufferable: true,
                device: false,
            },
        }];

        Self {
            regions,
            ram: vec![0u8; ram_size],
            ram_base,
        }
    }

    /// Add a memory region descriptor
    pub fn add_region(&mut self, region: MemoryRegion) {
        self.regions.push(region);
    }

    /// Get the memory region containing the given address
    pub fn get_region(&self, paddr: u64) -> Option<&MemoryRegion> {
        self.regions
            .iter()
            .find(|r| paddr >= r.start && paddr < r.start + r.size as u64)
    }

    /// Check if address is within RAM
    pub fn is_ram(&self, paddr: u64) -> bool {
        paddr >= self.ram_base && paddr < self.ram_base + self.ram.len() as u64
    }

    /// Calculate offset into RAM buffer
    fn ram_offset(&self, paddr: u64) -> Result<usize, MemoryError> {
        if !self.is_ram(paddr) {
            return Err(MemoryError::InvalidAddress(paddr));
        }
        Ok((paddr - self.ram_base) as usize)
    }

    /// Read a byte from physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to read from
    ///
    /// # Returns
    /// The byte value at the specified address
    pub fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        let offset = self.ram_offset(paddr)?;
        Ok(self.ram[offset])
    }

    /// Write a byte to physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to write to
    /// * `value` - Byte value to write
    pub fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError> {
        let offset = self.ram_offset(paddr)?;
        self.ram[offset] = value;
        Ok(())
    }

    /// Read a halfword (2 bytes) from physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to read from (must be 2-byte aligned)
    ///
    /// # Returns
    /// The halfword value at the specified address
    ///
    /// # Errors
    /// Returns `MemoryError::Misaligned` if address is not 2-byte aligned
    pub fn read_half(&self, paddr: u64) -> Result<u16, MemoryError> {
        if paddr & 0x1 != 0 {
            return Err(MemoryError::Misaligned(paddr, 2));
        }
        let offset = self.ram_offset(paddr)?;
        Ok(u16::from_le_bytes([self.ram[offset], self.ram[offset + 1]]))
    }

    /// Write a halfword (2 bytes) to physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to write to (must be 2-byte aligned)
    /// * `value` - Halfword value to write
    ///
    /// # Errors
    /// Returns `MemoryError::Misaligned` if address is not 2-byte aligned
    pub fn write_half(&mut self, paddr: u64, value: u16) -> Result<(), MemoryError> {
        if paddr & 0x1 != 0 {
            return Err(MemoryError::Misaligned(paddr, 2));
        }
        let offset = self.ram_offset(paddr)?;
        let bytes = value.to_le_bytes();
        self.ram[offset] = bytes[0];
        self.ram[offset + 1] = bytes[1];
        Ok(())
    }

    /// Read a word (4 bytes) from physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to read from (must be 4-byte aligned)
    ///
    /// # Returns
    /// The word value at the specified address
    ///
    /// # Errors
    /// Returns `MemoryError::Misaligned` if address is not 4-byte aligned
    pub fn read_word(&self, paddr: u64) -> Result<u32, MemoryError> {
        if paddr & 0x3 != 0 {
            return Err(MemoryError::Misaligned(paddr, 4));
        }
        let offset = self.ram_offset(paddr)?;
        Ok(u32::from_le_bytes([
            self.ram[offset],
            self.ram[offset + 1],
            self.ram[offset + 2],
            self.ram[offset + 3],
        ]))
    }

    /// Write a word (4 bytes) to physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to write to (must be 4-byte aligned)
    /// * `value` - Word value to write
    ///
    /// # Errors
    /// Returns `MemoryError::Misaligned` if address is not 4-byte aligned
    pub fn write_word(&mut self, paddr: u64, value: u32) -> Result<(), MemoryError> {
        if paddr & 0x3 != 0 {
            return Err(MemoryError::Misaligned(paddr, 4));
        }
        let offset = self.ram_offset(paddr)?;
        let bytes = value.to_le_bytes();
        self.ram[offset] = bytes[0];
        self.ram[offset + 1] = bytes[1];
        self.ram[offset + 2] = bytes[2];
        self.ram[offset + 3] = bytes[3];
        Ok(())
    }

    /// Read a doubleword (8 bytes) from physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to read from (must be 8-byte aligned)
    ///
    /// # Returns
    /// The doubleword value at the specified address
    ///
    /// # Errors
    /// Returns `MemoryError::Misaligned` if address is not 8-byte aligned
    pub fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        if paddr & 0x7 != 0 {
            return Err(MemoryError::Misaligned(paddr, 8));
        }
        let offset = self.ram_offset(paddr)?;
        Ok(u64::from_le_bytes([
            self.ram[offset],
            self.ram[offset + 1],
            self.ram[offset + 2],
            self.ram[offset + 3],
            self.ram[offset + 4],
            self.ram[offset + 5],
            self.ram[offset + 6],
            self.ram[offset + 7],
        ]))
    }

    /// Write a doubleword (8 bytes) to physical memory
    ///
    /// # Arguments
    /// * `paddr` - Physical address to write to (must be 8-byte aligned)
    /// * `value` - Doubleword value to write
    ///
    /// # Errors
    /// Returns `MemoryError::Misaligned` if address is not 8-byte aligned
    pub fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        if paddr & 0x7 != 0 {
            return Err(MemoryError::Misaligned(paddr, 8));
        }
        let offset = self.ram_offset(paddr)?;
        let bytes = value.to_le_bytes();
        self.ram[offset] = bytes[0];
        self.ram[offset + 1] = bytes[1];
        self.ram[offset + 2] = bytes[2];
        self.ram[offset + 3] = bytes[3];
        self.ram[offset + 4] = bytes[4];
        self.ram[offset + 5] = bytes[5];
        self.ram[offset + 6] = bytes[6];
        self.ram[offset + 7] = bytes[7];
        Ok(())
    }

    /// Get the size of RAM
    pub fn ram_size(&self) -> usize {
        self.ram.len()
    }

    /// Get the base address of RAM
    pub fn ram_base(&self) -> u64 {
        self.ram_base
    }

    /// Load data into RAM at the specified offset
    ///
    /// # Arguments
    /// * `offset` - Offset into RAM (relative to ram_base)
    /// * `data` - Data to load
    ///
    /// # Errors
    /// Returns `MemoryError::OutOfBounds` if data would exceed RAM bounds
    pub fn load_data(&mut self, offset: usize, data: &[u8]) -> Result<(), MemoryError> {
        if offset + data.len() > self.ram.len() {
            return Err(MemoryError::OutOfBounds);
        }
        self.ram[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }

    /// Read a slice from RAM
    ///
    /// # Arguments
    /// * `paddr` - Physical address to read from
    /// * `len` - Number of bytes to read
    ///
    /// # Returns
    /// A copy of the bytes at the specified address
    pub fn read_bytes(&self, paddr: u64, len: usize) -> Result<Vec<u8>, MemoryError> {
        let offset = self.ram_offset(paddr)?;
        if offset + len > self.ram.len() {
            return Err(MemoryError::OutOfBounds);
        }
        Ok(self.ram[offset..offset + len].to_vec())
    }

    /// Write a slice to RAM
    ///
    /// # Arguments
    /// * `paddr` - Physical address to write to
    /// * `data` - Data to write
    pub fn write_bytes(&mut self, paddr: u64, data: &[u8]) -> Result<(), MemoryError> {
        let offset = self.ram_offset(paddr)?;
        if offset + data.len() > self.ram.len() {
            return Err(MemoryError::OutOfBounds);
        }
        self.ram[offset..offset + data.len()].copy_from_slice(data);
        Ok(())
    }
}

impl PhysicalMemoryInterface for PhysicalMemory {
    fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        // Direct implementation to avoid recursion
        let offset = self.ram_offset(paddr)?;
        Ok(self.ram[offset])
    }

    fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError> {
        // Direct implementation to avoid recursion
        let offset = self.ram_offset(paddr)?;
        self.ram[offset] = value;
        Ok(())
    }

    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        // Direct implementation to avoid recursion
        if paddr & 0x7 != 0 {
            return Err(MemoryError::Misaligned(paddr, 8));
        }
        let offset = self.ram_offset(paddr)?;
        Ok(u64::from_le_bytes([
            self.ram[offset],
            self.ram[offset + 1],
            self.ram[offset + 2],
            self.ram[offset + 3],
            self.ram[offset + 4],
            self.ram[offset + 5],
            self.ram[offset + 6],
            self.ram[offset + 7],
        ]))
    }

    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        // Direct implementation to avoid recursion
        if paddr & 0x7 != 0 {
            return Err(MemoryError::Misaligned(paddr, 8));
        }
        let offset = self.ram_offset(paddr)?;
        let bytes = value.to_le_bytes();
        self.ram[offset] = bytes[0];
        self.ram[offset + 1] = bytes[1];
        self.ram[offset + 2] = bytes[2];
        self.ram[offset + 3] = bytes[3];
        self.ram[offset + 4] = bytes[4];
        self.ram[offset + 5] = bytes[5];
        self.ram[offset + 6] = bytes[6];
        self.ram[offset + 7] = bytes[7];
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physical_memory_basic() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);

        mem.write_byte(0x8000_0000, 0x42).unwrap();
        assert_eq!(mem.read_byte(0x8000_0000).unwrap(), 0x42);
    }

    #[test]
    fn test_physical_memory_half() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);

        mem.write_half(0x8000_0000, 0x1234).unwrap();
        assert_eq!(mem.read_half(0x8000_0000).unwrap(), 0x1234);

        // Little endian check
        assert_eq!(mem.read_byte(0x8000_0000).unwrap(), 0x34);
        assert_eq!(mem.read_byte(0x8000_0001).unwrap(), 0x12);
    }

    #[test]
    fn test_physical_memory_word() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);

        mem.write_word(0x8000_0000, 0x1234_5678).unwrap();
        assert_eq!(mem.read_word(0x8000_0000).unwrap(), 0x1234_5678);

        // Little endian check
        assert_eq!(mem.read_half(0x8000_0000).unwrap(), 0x5678);
        assert_eq!(mem.read_half(0x8000_0002).unwrap(), 0x1234);
    }

    #[test]
    fn test_physical_memory_dword() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);

        mem.write_dword(0x8000_0000, 0x1234_5678_9ABC_DEF0).unwrap();
        assert_eq!(mem.read_dword(0x8000_0000).unwrap(), 0x1234_5678_9ABC_DEF0);

        // Little endian check
        assert_eq!(mem.read_word(0x8000_0000).unwrap(), 0x9ABC_DEF0);
        assert_eq!(mem.read_word(0x8000_0004).unwrap(), 0x1234_5678);
    }

    #[test]
    fn test_physical_memory_alignment() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);

        // Halfword alignment
        assert!(mem.read_half(0x8000_0001).is_err());
        assert!(mem.write_half(0x8000_0001, 0).is_err());

        // Word alignment
        assert!(mem.read_word(0x8000_0001).is_err());
        assert!(mem.write_word(0x8000_0001, 0).is_err());
        assert!(mem.read_word(0x8000_0002).is_err());
        assert!(mem.write_word(0x8000_0002, 0).is_err());

        // Doubleword alignment
        assert!(mem.read_dword(0x8000_0001).is_err());
        assert!(mem.write_dword(0x8000_0001, 0).is_err());
        assert!(mem.read_dword(0x8000_0004).is_err()); // 4-byte aligned but not 8-byte aligned
        assert!(mem.read_dword(0x8000_0008).is_ok()); // 8-byte aligned
    }

    #[test]
    fn test_invalid_address() {
        let mem = PhysicalMemory::new(0x8000_0000, 0x1000);

        assert!(mem.read_byte(0x0000_0000).is_err());
        assert!(mem.read_byte(0x9000_0000).is_err());
    }

    #[test]
    fn test_memory_region() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);
        mem.add_region(MemoryRegion {
            start: 0x1000_0000,
            size: 0x100,
            region_type: MemoryRegionType::Mmio,
            attributes: MemoryAttributes {
                cacheable: false,
                bufferable: false,
                device: true,
            },
        });

        let region = mem.get_region(0x1000_0000).unwrap();
        assert_eq!(region.region_type, MemoryRegionType::Mmio);
        assert!(region.attributes.device);
    }

    #[test]
    fn test_load_data() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);
        let data = vec![0x01, 0x02, 0x03, 0x04];

        mem.load_data(0, &data).unwrap();
        assert_eq!(mem.read_byte(0x8000_0000).unwrap(), 0x01);
        assert_eq!(mem.read_byte(0x8000_0003).unwrap(), 0x04);

        // Out of bounds
        assert!(mem.load_data(0x0FFF, &data).is_err());
    }

    #[test]
    fn test_read_write_bytes() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);
        let data = vec![0x01, 0x02, 0x03, 0x04, 0x05];

        mem.write_bytes(0x8000_0100, &data).unwrap();
        let read = mem.read_bytes(0x8000_0100, 5).unwrap();
        assert_eq!(read, data);
    }
}
