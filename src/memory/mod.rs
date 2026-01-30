//! 存储器接口模块
//!
//! 定义内存访问的通用接口和简单实现

use std::sync::RwLock;
use thiserror::Error;

/// 存储器错误
#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("无效的内存地址: 0x{0:08x}")]
    InvalidAddress(u32),
    #[error("未对齐访问: 地址 0x{0:08x}, 需要 {1}-字节对齐")]
    Misaligned(u32, u32),
    #[error("内存访问越界")]
    OutOfBounds,
}

/// 存储器接口 Trait
pub trait MemoryInterface {
    /// 读取字 (4字节)
    fn read_word(&self, addr: u32) -> Result<u32, MemoryError>;
    /// 读取半字 (2字节)
    fn read_half(&self, addr: u32) -> Result<u16, MemoryError>;
    /// 读取字节 (1字节)
    fn read_byte(&self, addr: u32) -> Result<u8, MemoryError>;
    /// 读取半字 (零扩展)
    fn read_half_zext(&self, addr: u32) -> Result<u32, MemoryError>;
    /// 读取字节 (零扩展)
    fn read_byte_zext(&self, addr: u32) -> Result<u32, MemoryError>;
    /// 读取半字 (符号扩展)
    fn read_half_sext(&self, addr: u32) -> Result<u32, MemoryError>;
    /// 读取字节 (符号扩展)
    fn read_byte_sext(&self, addr: u32) -> Result<u32, MemoryError>;

    /// 写入字 (4字节)
    fn write_word(&mut self, addr: u32, value: u32) -> Result<(), MemoryError>;
    /// 写入半字 (2字节)
    fn write_half(&mut self, addr: u32, value: u16) -> Result<(), MemoryError>;
    /// 写入字节 (1字节)
    fn write_byte(&mut self, addr: u32, value: u8) -> Result<(), MemoryError>;

    /// 获取存储器大小
    fn size(&self) -> usize;
}

/// 简单存储器实现
pub struct SimpleMemory {
    /// 存储器数据 (使用RwLock支持线程安全读写)
    data: RwLock<Vec<u8>>,
    /// 存储器大小
    size: usize,
}

impl SimpleMemory {
    /// 创建新的简单存储器
    pub fn new(size: usize) -> Self {
        Self {
            data: RwLock::new(vec![0; size]),
            size,
        }
    }

    /// 从数据初始化存储器
    pub fn from_data(data: Vec<u8>) -> Self {
        let size = data.len();
        Self {
            data: RwLock::new(data),
            size,
        }
    }

    /// 加载程序数据 (小端序)
    pub fn load_program(&self, data: &[u8], base_addr: u32) {
        let mut mem = self.data.write().unwrap();
        for (i, &byte) in data.iter().enumerate() {
            let addr = (base_addr as usize) + i;
            if addr < self.size {
                mem[addr] = byte;
            }
        }
    }
}

impl MemoryInterface for SimpleMemory {
    fn read_word(&self, addr: u32) -> Result<u32, MemoryError> {
        let addr = addr as usize;
        if addr + 4 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u32));
        }
        if !addr.is_multiple_of(4) {
            return Err(MemoryError::Misaligned(addr as u32, 4));
        }

        let data = self.data.read().unwrap();
        Ok(u32::from_le_bytes([
            data[addr],
            data[addr + 1],
            data[addr + 2],
            data[addr + 3],
        ]))
    }

    fn read_half(&self, addr: u32) -> Result<u16, MemoryError> {
        let addr = addr as usize;
        if addr + 2 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u32));
        }
        if !addr.is_multiple_of(2) {
            return Err(MemoryError::Misaligned(addr as u32, 2));
        }

        let data = self.data.read().unwrap();
        Ok(u16::from_le_bytes([data[addr], data[addr + 1]]))
    }

    fn read_byte(&self, addr: u32) -> Result<u8, MemoryError> {
        let addr = addr as usize;
        if addr >= self.size {
            return Err(MemoryError::InvalidAddress(addr as u32));
        }
        let data = self.data.read().unwrap();
        Ok(data[addr])
    }

    fn read_half_zext(&self, addr: u32) -> Result<u32, MemoryError> {
        Ok(self.read_half(addr)?.into())
    }

    fn read_byte_zext(&self, addr: u32) -> Result<u32, MemoryError> {
        Ok(self.read_byte(addr)?.into())
    }

    fn read_half_sext(&self, addr: u32) -> Result<u32, MemoryError> {
        let val = self.read_half(addr)?;
        Ok((val as i16) as i32 as u32)
    }

    fn read_byte_sext(&self, addr: u32) -> Result<u32, MemoryError> {
        let val = self.read_byte(addr)?;
        Ok((val as i8) as i32 as u32)
    }

    fn write_word(&mut self, addr: u32, value: u32) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr + 4 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u32));
        }
        if !addr.is_multiple_of(4) {
            return Err(MemoryError::Misaligned(addr as u32, 4));
        }

        let mut data = self.data.write().unwrap();
        let bytes = value.to_le_bytes();
        data[addr] = bytes[0];
        data[addr + 1] = bytes[1];
        data[addr + 2] = bytes[2];
        data[addr + 3] = bytes[3];
        Ok(())
    }

    fn write_half(&mut self, addr: u32, value: u16) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr + 2 > self.size {
            return Err(MemoryError::InvalidAddress(addr as u32));
        }
        if !addr.is_multiple_of(2) {
            return Err(MemoryError::Misaligned(addr as u32, 2));
        }

        let mut data = self.data.write().unwrap();
        let bytes = value.to_le_bytes();
        data[addr] = bytes[0];
        data[addr + 1] = bytes[1];
        Ok(())
    }

    fn write_byte(&mut self, addr: u32, value: u8) -> Result<(), MemoryError> {
        let addr = addr as usize;
        if addr >= self.size {
            return Err(MemoryError::InvalidAddress(addr as u32));
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

        // 写入并读取字
        mem.write_word(0x100, 0x12345678).unwrap();
        assert_eq!(mem.read_word(0x100).unwrap(), 0x12345678);

        // 写入并读取半字
        mem.write_half(0x200, 0xABCD).unwrap();
        assert_eq!(mem.read_half(0x200).unwrap(), 0xABCD);

        // 写入并读取字节
        mem.write_byte(0x300, 0x42).unwrap();
        assert_eq!(mem.read_byte(0x300).unwrap(), 0x42);
    }

    #[test]
    fn test_memory_misaligned() {
        let mut mem = SimpleMemory::new(1024);

        // 未对齐的字访问
        assert!(mem.read_word(0x101).is_err());
        assert!(mem.write_word(0x101, 0).is_err());

        // 未对齐的半字访问
        assert!(mem.read_half(0x101).is_err());
        assert!(mem.write_half(0x101, 0).is_err());
    }

    #[test]
    fn test_load_program() {
        let mem = SimpleMemory::new(0x2000); // 8KB，足以容纳 0x1000 地址
        let program = vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];

        mem.load_program(&program, 0x1000);

        assert_eq!(mem.read_byte(0x1000).unwrap(), 0x01);
        assert_eq!(mem.read_byte(0x1007).unwrap(), 0x08);
        assert_eq!(mem.read_word(0x1000).unwrap(), 0x04030201); // 小端: 0x04+0x03*256+0x02*65536+0x01*16777216
    }
}
