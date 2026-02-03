//! M5 ELF Executor
//!
//! Implements ELF loading and execution for the RISC-V simulator.
//! Provides `load_and_run` function for loading and executing ELF files
//! with tohost exit signal support.

use crate::core::{CoreState, RiscvCore};
use crate::elf::{load_elf_file, ElfError, SignatureInfo};
use crate::memory::MemoryInterface;
use crate::memory::SimpleMemory;
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Execution result
#[derive(Debug, Clone, Default)]
pub struct ExecutionResult {
    /// Exit code (0 for success, non-zero for failure)
    pub exit_code: u32,
    /// Number of executed cycles
    pub cycles: u64,
    /// Final program counter
    pub final_pc: u64,
    /// Whether execution timed out
    pub timed_out: bool,
    /// Whether an error occurred
    pub error: Option<String>,
    /// Signature section address (if available)
    pub signature_addr: Option<u64>,
    /// Signature data (if available)
    pub signature_data: Option<Vec<u8>>,
}

/// Executor errors
#[derive(Error, Debug)]
pub enum ExecutorError {
    #[error("ELF loading failed: {0}")]
    ElfLoadError(#[from] ElfError),
    #[error("Memory allocation failed")]
    MemoryAllocationFailed,
    #[error("Execution timeout after {0} cycles")]
    Timeout(u64),
    #[error("Execution error: {0}")]
    ExecutionError(String),
    #[error("Invalid tohost address")]
    InvalidTohostAddress,
    #[error("Core execution error: {0}")]
    CoreError(#[from] anyhow::Error),
}

/// Default maximum cycles before timeout
const DEFAULT_MAX_CYCLES: u64 = 10_000_000;

/// Default tohost address (commonly used in RISC-V tests)
const DEFAULT_TOHOST: u64 = 0x8000_1000;

/// Write marker to distinguish tohost/fromhost access
const WRITE_MARKER: u64 = 1 << 63;

/// Dump signature data from memory
///
/// Reads signature region from memory and returns as bytes.
/// Returns None if signature section info is not available.
pub fn dump_signature(
    mem: &Arc<Mutex<SimpleMemory>>,
    sig_info: Option<&crate::elf::SignatureInfo>,
) -> Result<Option<Vec<u8>>, ExecutorError> {
    let sig_info = match sig_info {
        Some(info) => info,
        None => return Ok(None),
    };

    let addr = sig_info.vaddr;
    let size = sig_info.size;

    if size == 0 {
        return Ok(Some(Vec::new()));
    }

    let mut data = Vec::with_capacity(size as usize);
    let mut current_addr = addr;

    // Read memory in larger chunks to reduce lock overhead
    while data.len() < size as usize {
        let guard = mem.lock().unwrap();
        let bytes_to_read = ((size as usize - data.len()).min(8)).min(8);

        // Try to read 8 bytes at a time if aligned
        if bytes_to_read == 8 && current_addr % 8 == 0 {
            match guard.read_dword(current_addr) {
                Ok(value) => {
                    // Store bytes in little-endian order
                    for i in 0..8 {
                        data.push((value >> (i * 8)) as u8);
                    }
                    current_addr += 8;
                }
                Err(_) => {
                    // Fall back to byte-by-byte read
                    drop(guard);
                    break;
                }
            }
        } else {
            // Read byte by byte for remaining bytes
            match guard.read_byte(current_addr) {
                Ok(byte) => {
                    data.push(byte);
                    current_addr += 1;
                }
                Err(e) => {
                    return Err(ExecutorError::ExecutionError(format!(
                        "Failed to read signature byte at 0x{:016x}: {}",
                        current_addr, e
                    )));
                }
            }
        }
    }

    // Fall back to byte-by-byte read for remaining bytes
    while data.len() < size as usize {
        let guard = mem.lock().unwrap();
        match guard.read_byte(current_addr) {
            Ok(byte) => {
                data.push(byte);
                current_addr += 1;
            }
            Err(e) => {
                return Err(ExecutorError::ExecutionError(format!(
                    "Failed to read signature byte at 0x{:016x}: {}",
                    current_addr, e
                )));
            }
        }
    }

    Ok(Some(data))
}

/// Extract exit code from tohost value
///
/// Supports two exit code formats:
/// 1. Standard format (WRITE_MARKER | exit_code): Upper bit set, lower 32 bits contain exit code
/// 2. Direct format: Small positive number (< 0x100, non-zero) representing exit code directly
///
/// Returns Some(exit_code) if a valid exit code is detected, None otherwise.
fn extract_exit_code(tohost_value: u64) -> Option<u32> {
    // Standard tohost format: upper bit indicates write, lower bits contain exit code
    if tohost_value & WRITE_MARKER != 0 {
        Some((tohost_value & 0xFFFFFFFF) as u32)
    } else {
        None
    }
}

/// Unified exit code extraction from tohost value
///
/// This function handles both exit code formats consistently:
/// - Format 1 (standard): `WRITE_MARKER | exit_code` - upper bit set
/// - Format 2 (direct): Small exit code value (< 0x100, non-zero)
///
/// Returns the exit code if detected, None if not an exit signal.
fn try_extract_exit_code(tohost_value: u64) -> Option<u32> {
    // Try standard format first (WRITE_MARKER | exit_code)
    if let Some(code) = extract_exit_code(tohost_value) {
        return Some(code);
    }

    // Fall back to direct format: small positive number is likely an exit code
    // Exit codes are typically in range 0-255 (0xFF)
    if tohost_value != 0 && tohost_value < 0x100 {
        return Some(tohost_value as u32);
    }

    None
}

/// Load and execute an ELF file
///
/// # Arguments
/// * `elf_data` - Raw ELF file bytes
/// * `max_cycles` - Maximum cycles before timeout (default: 10 million)
/// * `tohost_addr` - Optional tohost address (auto-detected from ELF if not provided)
///
/// # Returns
/// ExecutionResult containing exit code, cycle count, and final state
pub fn load_and_run(
    elf_data: &[u8],
    max_cycles: Option<u64>,
    tohost_addr: Option<u64>,
) -> Result<ExecutionResult, ExecutorError> {
    let max_cycles = max_cycles.unwrap_or(DEFAULT_MAX_CYCLES);

    // Step 1: Load ELF file
    let (entry_point, memory, signature, elf_tohost) = load_elf_file(elf_data)?;

    // Use provided tohost address or try to find it in ELF
    let tohost = tohost_addr.or(elf_tohost).unwrap_or(DEFAULT_TOHOST);

    // Step 2: Allocate and initialize memory
    let mem_size = memory.len();
    if mem_size == 0 {
        return Err(ExecutorError::MemoryAllocationFailed);
    }

    let mem = Arc::new(Mutex::new(SimpleMemory::new(mem_size)));

    // Copy loaded ELF data into memory
    {
        let mut mem_guard = mem.lock().unwrap();
        for (i, &byte) in memory.iter().enumerate() {
            mem_guard.write_byte(i as u64, byte).ok();
        }
    }

    // Step 3: Create and configure core
    let mut core = RiscvCore::new(mem.clone(), mem.clone());

    // Reset core with entry point
    core.reset(entry_point);

    // Step 4: Execution loop
    let mut cycles = 0u64;

    while cycles < max_cycles {
        // Read current PC for result
        let current_pc = core.state().pc;

        // Execute one instruction
        match core.step() {
            Ok(()) => {
                cycles += 1;

                // Check for tohost write (exit signal)
                // We need to check if tohost was written to
                if let Ok(mem_guard) = mem.lock() {
                    // Read from tohost address (64-bit read)
                    match mem_guard.read_dword(tohost) {
                        Ok(tohost_value) => {
                            // Unified exit signal detection
                            // Supports both WRITE_MARKER format and direct exit code format
                            if let Some(code) = try_extract_exit_code(tohost_value) {
                                let sig_data =
                                    dump_signature(&mem, signature.as_ref()).ok().flatten();
                                return Ok(ExecutionResult {
                                    exit_code: code,
                                    cycles,
                                    final_pc: core.state().pc,
                                    timed_out: false,
                                    error: None,
                                    signature_addr: signature.map(|s| s.vaddr),
                                    signature_data: sig_data,
                                });
                            }
                        }
                        Err(_) => {
                            // tohost read failed, continue execution
                        }
                    }
                }
            }
            Err(e) => {
                let sig_data = dump_signature(&mem, signature.as_ref()).ok().flatten();
                return Ok(ExecutionResult {
                    exit_code: 1,
                    cycles,
                    final_pc: current_pc,
                    timed_out: false,
                    error: Some(format!(
                        "Execution error at PC 0x{:016x}: {}",
                        current_pc, e
                    )),
                    signature_addr: signature.map(|s| s.vaddr),
                    signature_data: sig_data,
                });
            }
        }
    }

    // Timeout reached
    let sig_data = dump_signature(&mem, signature.as_ref()).ok().flatten();
    Ok(ExecutionResult {
        exit_code: 1, // Non-zero indicates abnormal termination
        cycles,
        final_pc: core.state().pc,
        timed_out: true,
        error: Some(format!("Timeout after {} cycles", max_cycles)),
        signature_addr: signature.map(|s| s.vaddr),
        signature_data: sig_data,
    })
}

/// Load and execute an ELF file from a file path
///
/// # Arguments
/// * `elf_path` - Path to ELF file
/// * `max_cycles` - Maximum cycles before timeout
///
/// # Returns
/// ExecutionResult
pub fn load_and_run_file(
    elf_path: &str,
    max_cycles: Option<u64>,
) -> Result<ExecutionResult, ExecutorError> {
    // Read ELF file
    let elf_data = std::fs::read(elf_path)
        .map_err(|e| ExecutorError::ElfLoadError(ElfError::IoError(e.to_string())))?;

    load_and_run(&elf_data, max_cycles, None)
}

/// Reset the simulator state
///
/// Creates a fresh core state ready for execution
pub fn reset_core(core: &mut RiscvCore, entry_point: u64) {
    core.reset(entry_point);
}

/// Run a single step and return the result
pub fn step_once(core: &mut RiscvCore) -> Result<(), ExecutorError> {
    core.step()
        .map_err(|e| ExecutorError::ExecutionError(e.to_string()))
}

/// Get current core state
pub fn get_core_state(core: &RiscvCore) -> CoreState {
    core.state().clone()
}

/// Execute multiple steps and check for exit signal
pub fn run_until_exit(
    simulator: &mut RiscVSimulator,
    max_cycles: u64,
) -> Result<ExecutionResult, ExecutorError> {
    simulator.run(Some(max_cycles))
}

/// Simplified RISC-V Simulator wrapper
pub struct RiscVSimulator {
    /// The RISC-V core
    core: RiscvCore,
    /// Shared memory
    memory: Arc<Mutex<SimpleMemory>>,
    /// tohost address for exit detection
    tohost: u64,
    /// Maximum cycles
    max_cycles: u64,
    /// Signature section info
    signature: Option<SignatureInfo>,
}

impl RiscVSimulator {
    /// Create new simulator with memory
    pub fn new(mem_size: usize) -> Self {
        let memory = Arc::new(Mutex::new(SimpleMemory::new(mem_size)));
        let core = RiscvCore::new(memory.clone(), memory.clone());
        Self {
            core,
            memory,
            tohost: DEFAULT_TOHOST,
            max_cycles: DEFAULT_MAX_CYCLES,
            signature: None,
        }
    }

    /// Load ELF data into memory
    pub fn load_elf(&mut self, elf_data: &[u8]) -> Result<u64, ExecutorError> {
        let (entry_point, memory, sig, tohost) = load_elf_file(elf_data)?;
        self.signature = sig;

        // Resize memory if needed
        let required_size = memory.len();
        let current_size = {
            let mem = self.memory.lock().unwrap();
            mem.size()
        };

        if required_size > current_size {
            // Create new larger memory and copy
            let new_mem = Arc::new(Mutex::new(SimpleMemory::new(required_size)));
            {
                let mut guard = new_mem.lock().unwrap();
                for (i, &byte) in memory.iter().enumerate() {
                    guard.write_byte(i as u64, byte).ok();
                }
            }
            self.memory = new_mem;
            self.core = RiscvCore::new(self.memory.clone(), self.memory.clone());
        } else {
            // Copy into existing memory
            let mut guard = self.memory.lock().unwrap();
            for (i, &byte) in memory.iter().enumerate() {
                guard.write_byte(i as u64, byte).ok();
            }
        }

        // Update tohost address
        if let Some(addr) = tohost {
            self.tohost = addr;
        }

        // Reset core to entry point
        self.core.reset(entry_point);

        Ok(entry_point)
    }

    /// Load ELF from file
    pub fn load_elf_file(&mut self, path: &str) -> Result<u64, ExecutorError> {
        let data = std::fs::read(path)
            .map_err(|e| ExecutorError::ElfLoadError(ElfError::IoError(e.to_string())))?;
        self.load_elf(&data)
    }

    /// Set maximum cycles
    pub fn set_max_cycles(&mut self, cycles: u64) {
        self.max_cycles = cycles;
    }

    /// Set tohost address
    pub fn set_tohost(&mut self, addr: u64) {
        self.tohost = addr;
    }

    /// Step one instruction
    pub fn step(&mut self) -> Result<(), ExecutorError> {
        self.core
            .step()
            .map_err(|e| ExecutorError::ExecutionError(e.to_string()))
    }

    /// Run until exit or timeout
    pub fn run(&mut self, max_cycles: Option<u64>) -> Result<ExecutionResult, ExecutorError> {
        let max_cycles = max_cycles.unwrap_or(self.max_cycles);
        let mut cycles = 0u64;

        while cycles < max_cycles {
            // Check for tohost write before stepping
            if self.check_exit()? {
                return Ok(self.get_result(cycles));
            }

            match self.step() {
                Ok(()) => {
                    cycles += 1;
                }
                Err(e) => {
                    let sig_data = dump_signature(&self.memory, self.signature.as_ref())
                        .ok()
                        .flatten();
                    return Ok(ExecutionResult {
                        exit_code: 1,
                        cycles,
                        final_pc: self.core.state().pc,
                        timed_out: false,
                        error: Some(format!("Execution error: {}", e)),
                        signature_addr: self.signature.as_ref().map(|s| s.vaddr),
                        signature_data: sig_data,
                    });
                }
            }
        }

        // Timeout
        let sig_data = dump_signature(&self.memory, self.signature.as_ref())
            .ok()
            .flatten();
        Ok(ExecutionResult {
            exit_code: 1,
            cycles,
            final_pc: self.core.state().pc,
            timed_out: true,
            error: Some(format!("Timeout after {} cycles", max_cycles)),
            signature_addr: self.signature.as_ref().map(|s| s.vaddr),
            signature_data: sig_data,
        })
    }

    /// Check if exit signal received
    fn check_exit(&self) -> Result<bool, ExecutorError> {
        let guard = self.memory.lock().unwrap();
        match guard.read_dword(self.tohost) {
            Ok(value) => {
                if let Some(code) = try_extract_exit_code(value) {
                    println!("[EXIT] Received exit signal with code: {}", code);
                    return Ok(true);
                }
            }
            Err(_) => {
                // Memory read failed, continue
            }
        }
        Ok(false)
    }

    /// Get execution result
    fn get_result(&self, cycles: u64) -> ExecutionResult {
        let exit_code = {
            let guard = self.memory.lock().unwrap();
            match guard.read_dword(self.tohost) {
                Ok(value) => try_extract_exit_code(value).unwrap_or_default(),
                Err(_) => 0,
            }
        };

        let sig_data = dump_signature(&self.memory, self.signature.as_ref())
            .ok()
            .flatten();

        ExecutionResult {
            exit_code,
            cycles,
            final_pc: self.core.state().pc,
            timed_out: false,
            error: None,
            signature_addr: self.signature.as_ref().map(|s| s.vaddr),
            signature_data: sig_data,
        }
    }

    /// Get current core state
    pub fn state(&self) -> &CoreState {
        self.core.state()
    }

    /// Get mutable core state
    pub fn state_mut(&mut self) -> &mut CoreState {
        self.core.state_mut()
    }

    /// Get memory reference
    pub fn memory(&self) -> &Arc<Mutex<SimpleMemory>> {
        &self.memory
    }

    /// Read from memory
    pub fn read_mem(&self, addr: u64, size: usize) -> Result<Vec<u8>, ExecutorError> {
        let mut data = vec![0u8; size];
        let mut current_addr = addr;
        let mut offset = 0;

        while offset < size {
            let remaining = size - offset;
            let guard = self.memory.lock().unwrap();

            // Try to read in larger chunks when possible
            if remaining >= 8 && current_addr.is_multiple_of(8) {
                // Read 8 bytes at a time when aligned
                match guard.read_dword(current_addr) {
                    Ok(val) => {
                        for i in 0..8 {
                            data[offset + i] = (val >> (i * 8)) as u8;
                        }
                        drop(guard);
                        current_addr += 8;
                        offset += 8;
                    }
                    Err(_) => {
                        // Fall back to word read
                        if remaining >= 4 && current_addr.is_multiple_of(4) {
                            drop(guard);
                            continue;
                        }
                        drop(guard);
                        // Fall through to byte-by-byte read
                    }
                }
            } else if remaining >= 4 && current_addr.is_multiple_of(4) {
                // Read 4 bytes at a time
                match guard.read_word(current_addr) {
                    Ok(val) => {
                        data[offset] = val as u8;
                        data[offset + 1] = (val >> 8) as u8;
                        data[offset + 2] = (val >> 16) as u8;
                        data[offset + 3] = (val >> 24) as u8;
                        drop(guard);
                        current_addr += 4;
                        offset += 4;
                    }
                    Err(_) => {
                        // Fall through to byte-by-byte read
                    }
                }
            } else if remaining >= 2 && current_addr.is_multiple_of(2) {
                // Read 2 bytes at a time
                match guard.read_half(current_addr) {
                    Ok(val) => {
                        data[offset] = val as u8;
                        data[offset + 1] = (val >> 8) as u8;
                        drop(guard);
                        current_addr += 2;
                        offset += 2;
                    }
                    Err(_) => {
                        // Fall through to byte-by-byte read
                    }
                }
            } else {
                // Read one byte
                match guard.read_byte(current_addr) {
                    Ok(byte) => {
                        data[offset] = byte;
                        current_addr += 1;
                        offset += 1;
                    }
                    Err(e) => {
                        return Err(ExecutorError::ExecutionError(format!(
                            "Memory read error at 0x{:016x}: {}",
                            current_addr, e
                        )));
                    }
                }
            }
        }

        Ok(data)
    }

    /// Write to memory
    pub fn write_mem(&self, addr: u64, data: &[u8]) -> Result<(), ExecutorError> {
        let mut guard = self.memory.lock().unwrap();
        for (i, &byte) in data.iter().enumerate() {
            guard
                .write_byte(addr + i as u64, byte)
                .map_err(|e| ExecutorError::ExecutionError(format!("Memory write error: {}", e)))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_extraction() {
        // Normal exit code (0x1 with write marker)
        let val = WRITE_MARKER | 0x1;
        assert_eq!(extract_exit_code(val), Some(1));

        // Exit code 0 (success)
        let val = WRITE_MARKER;
        assert_eq!(extract_exit_code(val), Some(0));

        // Non-exit value
        let val = 0x1234_5678;
        assert_eq!(extract_exit_code(val), None);
    }

    #[test]
    fn test_unified_exit_code_extraction() {
        // Standard format with WRITE_MARKER
        let val = WRITE_MARKER | 0x5;
        assert_eq!(try_extract_exit_code(val), Some(5));

        // Standard format with exit code 0
        let val = WRITE_MARKER;
        assert_eq!(try_extract_exit_code(val), Some(0));

        // Direct format (small exit code)
        let val = 0x2;
        assert_eq!(try_extract_exit_code(val), Some(2));

        // Direct format (exit code 0)
        let val = 0x0;
        assert_eq!(try_extract_exit_code(val), None);

        // Non-exit value (too large)
        let val = 0x1234_5678;
        assert_eq!(try_extract_exit_code(val), None);

        // Edge case: max direct exit code
        let val = 0xFF;
        assert_eq!(try_extract_exit_code(val), Some(0xFF));

        // Just over direct format range
        let val = 0x100;
        assert_eq!(try_extract_exit_code(val), None);
    }

    #[test]
    fn test_simulator_creation() {
        let sim = RiscVSimulator::new(0x10000);
        assert_eq!(sim.state().pc, 0);
    }

    #[test]
    fn test_load_and_run_simple() {
        // Create a minimal ELF-like program that exits immediately
        // For now, just verify the function signature works
        let result = load_and_run(&[], Some(100), None);
        // Should fail due to invalid ELF
        assert!(result.is_err());
    }
}
