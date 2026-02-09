//! Commit log module - Output Spike-compatible commit logs
//!
//! This module provides functionality to output commit logs that are
//! compatible with Spike's --log-commits format.

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Memory access information for logging
#[derive(Debug, Clone, Copy)]
pub struct MemoryAccess {
    /// Memory address
    pub addr: u64,
    /// Value read/written (None for load in Spike format)
    pub value: Option<u64>,
    /// True if this is a store, false if it's a load
    pub is_store: bool,
}

impl MemoryAccess {
    /// Create a new load access
    pub fn load(addr: u64) -> Self {
        Self {
            addr,
            value: None,
            is_store: false,
        }
    }

    /// Create a new store access
    pub fn store(addr: u64, value: u64) -> Self {
        Self {
            addr,
            value: Some(value),
            is_store: true,
        }
    }
}

/// Commit logger that writes to a file or stdout
pub struct CommitLogger {
    output: Box<dyn Write>,
}

impl CommitLogger {
    /// Create a new commit logger that writes to stdout
    pub fn new_stdout() -> Self {
        Self {
            output: Box::new(std::io::stdout()),
        }
    }

    /// Create a new commit logger that writes to a file
    pub fn new_file(path: &Path) -> Result<Self, io::Error> {
        let file = File::create(path)?;
        Ok(Self {
            output: Box::new(file),
        })
    }

    /// Log a commit in Spike-compatible format
    ///
    /// Format: core   <hartid>: <privilege> <pc> (<opcode>) [x<reg> <value>] [mem <addr> [value]]
    ///
    /// # Arguments
    /// * `hartid` - Hart ID (typically 0)
    /// * `privilege` - Privilege mode (0=U, 1=S, 3=M)
    /// * `pc` - Program counter
    /// * `opcode` - Instruction opcode (machine code)
    /// * `regs` - Register values [x0..x31]
    /// * `mem_access` - Optional memory access info
    pub fn log_commit(
        &mut self,
        hartid: usize,
        privilege: u8,
        pc: u64,
        opcode: u32,
        regs_before: &[u64; 32],
        regs_after: &[u64; 32],
        mem_access: Option<&MemoryAccess>,
    ) -> io::Result<()> {
        // Format: core   0: 3 0x0000000080000000 (0x00000093)
        write!(
            self.output,
            "core   {}: {} {:#018x} ({:#010x})",
            hartid, privilege, pc, opcode
        )?;

        // Output register changes (x1-x31, skipping x0 which is always 0)
        // Spike outputs all registers that changed value, including non-zero to zero
        let mut first_reg = true;
        for i in 1..32 {
            if regs_before[i] != regs_after[i] {
                if first_reg {
                    write!(self.output, " x{}  {:#018x}", i, regs_after[i])?;
                    first_reg = false;
                } else {
                    write!(self.output, " x{}  {:#018x}", i, regs_after[i])?;
                }
            }
        }

        // Output memory access if present
        if let Some(mem) = mem_access {
            if mem.is_store {
                // Store: mem <addr> <value>
                if let Some(value) = mem.value {
                    write!(self.output, " mem {:#x} {:#x}", mem.addr, value)?;
                } else {
                    write!(self.output, " mem {:#x}", mem.addr)?;
                }
            } else {
                // Load: mem <addr>
                write!(self.output, " mem {:#x}", mem.addr)?;
            }
        }

        writeln!(self.output)
    }

    /// Log a comment/tag line (Spike-compatible)
    ///
    /// Format: >>>>>  <text>
    pub fn log_comment(&mut self, text: &str) -> io::Result<()> {
        writeln!(self.output, ">>>>  {}", text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_commit_format() {
        // Test using a temp file instead of in-memory buffer
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_commit_format.log");
        let test_path = test_file.as_path();

        {
            let mut logger = CommitLogger::new_file(test_path).unwrap();

            let regs = [0u64; 32];
            let mut regs_with_values = regs;
            regs_with_values[1] = 0x12345678;
            regs_with_values[10] = 0xDEADBEEF;

            logger
                .log_commit(0, 3, 0x80000000, 0x00000093, &regs, &regs_with_values, None)
                .unwrap();
        }

        // Read and verify
        let content = std::fs::read_to_string(test_path).unwrap();
        assert!(content.starts_with("core   0: 3 0x0000000080000000 (0x00000093)"));
        assert!(content.contains(" x1  0x0000000012345678"));
        assert!(content.contains(" x10  0x00000000deadbeef")); // lowercase

        // Cleanup
        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_log_commit_with_memory_load() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_commit_load.log");
        let test_path = test_file.as_path();

        {
            let mut logger = CommitLogger::new_file(test_path).unwrap();

            let regs = [0u64; 32];
            let mut regs_with_values = regs;
            regs_with_values[5] = 0xABCD_EF00_1234_5678;

            let mem_access = MemoryAccess {
                addr: 0x8000001000,
                value: None,
                is_store: false,
            };

            logger
                .log_commit(
                    0,
                    3,
                    0x80000008,
                    0x00052083,
                    &regs,
                    &regs_with_values,
                    Some(&mem_access),
                )
                .unwrap();
        }

        let content = std::fs::read_to_string(test_path).unwrap();
        assert!(content.contains("mem 0x8000001000"));

        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_log_commit_with_memory_store() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_commit_store.log");
        let test_path = test_file.as_path();

        {
            let mut logger = CommitLogger::new_file(test_path).unwrap();

            let regs = [0u64; 32];
            let mem_access = MemoryAccess {
                addr: 0x80000180,
                value: Some(0x12345678),
                is_store: true,
            };

            logger
                .log_commit(
                    0,
                    3,
                    0x80000010,
                    0x00152023,
                    &regs,
                    &regs,
                    Some(&mem_access),
                )
                .unwrap();
        }

        let content = std::fs::read_to_string(test_path).unwrap();
        assert!(content.contains("mem 0x80000180 0x12345678"));

        let _ = std::fs::remove_file(test_path);
    }

    #[test]
    fn test_log_comment() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_commit_comment.log");
        let test_path = test_file.as_path();

        {
            let mut logger = CommitLogger::new_file(test_path).unwrap();
            logger.log_comment("loop_start").unwrap();
        }

        let content = std::fs::read_to_string(test_path).unwrap();
        assert_eq!(content, ">>>>  loop_start\n");

        let _ = std::fs::remove_file(test_path);
    }
}
