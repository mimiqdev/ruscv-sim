//! Physical memory management

use crate::memory::MemoryError;
use std::collections::HashMap;

/// Memory region type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    Ram,
    Rom,
    Mmio,
    Reserved,
}

/// Memory attributes
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryAttributes {
    pub cacheable: bool,
    pub bufferable: bool,
    pub device: bool,
}

/// Memory region descriptor
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub start: u64,
    pub size: usize,
    pub region_type: MemoryRegionType,
    pub attributes: MemoryAttributes,
}

/// Physical memory interface
trait PhysicalMemoryInterface: Send + Sync {
    fn read(&self, paddr: u64, size: u8) -> Result<u64, MemoryError>;
    fn write(&mut self, paddr: u64, value: u64, size: u8) -> Result<(), MemoryError>;
    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError>;
    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError>;
}

/// Physical memory manager
pub struct PhysicalMemory {
    regions: Vec<MemoryRegion>,
    ram: Vec<u8>,
    ram_base: u64,
}

impl PhysicalMemory {
    pub fn new(ram_base: u64, ram_size: usize) -> Self {
        let mut regions = Vec::new();
        
        // Default RAM region
        regions.push(MemoryRegion {
            start: ram_base,
            size: ram_size,
            region_type: MemoryRegionType::Ram,
            attributes: MemoryAttributes {
                cacheable: true,
                bufferable: true,
                device: false,
            },
        });

        Self {
            regions,
            ram: vec![0u8; ram_size],
            ram_base,
        }
    }

    pub fn add_region(&mut self, region: MemoryRegion) {
        self.regions.push(region);
    }

    pub fn get_region(&self, paddr: u64) -> Option<&MemoryRegion> {
        self.regions.iter().find(|r| {
            paddr >= r.start && paddr < r.start + r.size as u64
        })
    }

    pub fn is_ram(&self, paddr: u64) -> bool {
        paddr >= self.ram_base && paddr < self.ram_base + self.ram.len() as u64
    }

    pub fn read_byte(&self, paddr: u64) -> Result<u8, MemoryError> {
        if !self.is_ram(paddr) {
            return Err(MemoryError::InvalidAddress(paddr));
        }
        let offset = (paddr - self.ram_base) as usize;
        Ok(self.ram[offset])
    }

    pub fn write_byte(&mut self, paddr: u64, value: u8) -> Result<(), MemoryError> {
        if !self.is_ram(paddr) {
            return Err(MemoryError::InvalidAddress(paddr));
        }
        let offset = (paddr - self.ram_base) as usize;
        self.ram[offset] = value;
        Ok(())
    }

    pub fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError> {
        if paddr & 0x7 != 0 {
            return Err(MemoryError::Misaligned(paddr, 8));
        }
        let mut value = 0u64;
        for i in 0..8 {
            value |= (self.read_byte(paddr + i)? as u64) << (i * 8);
        }
        Ok(value)
    }

    pub fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError> {
        if paddr & 0x7 != 0 {
            return Err(MemoryError::Misaligned(paddr, 8));
        }
        for i in 0..8 {
            self.write_byte(paddr + i, ((value >> (i * 8)) & 0xFF) as u8)?;
        }
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
    fn test_physical_memory_dword() {
        let mut mem = PhysicalMemory::new(0x8000_0000, 0x1000);
        
        mem.write_dword(0x8000_0000, 0x1234_5678_9ABC_DEF0).unwrap();
        assert_eq!(mem.read_dword(0x8000_0000).unwrap(), 0x1234_5678_9ABC_DEF0);
    }

    #[test]
    fn test_invalid_address() {
        let mem = PhysicalMemory::new(0x8000_0000, 0x1000);
        
        assert!(mem.read_byte(0x0000_0000).is_err());
        assert!(mem.read_byte(0x9000_0000).is_err());
    }
}
