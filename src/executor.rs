//! M5 ELF Executor
//!
//! Implements ELF loading and execution for the RISC-V simulator.
//! Provides `load_and_run` function for loading and executing ELF files
//! with tohost exit signal support.

use crate::core::{commits::CommitLogger, CoreState, RiscvCore};
use crate::elf::{load_elf_file, ElfError, SignatureInfo};
use crate::memory::{MemoryError, MemoryInterface, SimpleMemory};
use crate::peripherals::Uart16550;
use std::path::PathBuf;
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

/// System Bus connecting CPU, RAM, and Peripherals
///
/// # Memory Map
/// - RAM: Configurable base address and size
/// - UART: Fixed at 0x10000000, size 0x100
/// - HTIF: Fixed at 0x40008000, size 8 (tohost register)
///
/// # UART Access Restrictions
/// The UART 16550 only supports byte-wide (8-bit) register access.
/// Multi-byte accesses (halfword, word, dword) to UART addresses will fail
/// with `MemoryError::InvalidAddress`. This is intentional as real UART
/// hardware typically does not support multi-byte accesses.
///
/// # HTIF (Host-Target Interface)
/// The HTIF device at 0x40008000 provides exit signal support:
/// - Write to tohost triggers exit check
/// - Read returns 0 (no signal)
/// - After processing exit, value is cleared
pub struct SystemBus {
    ram: Arc<Mutex<SimpleMemory>>,
    uart: Arc<Mutex<Uart16550>>,
    ram_base: u64,
    ram_size: usize,
    uart_base: u64,
    uart_size: usize,
    htif_base: u64,
    htif_size: usize,
    /// Callback for HTIF write events (exit signal detection)
    htif_write_callback: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}

impl SystemBus {
    pub fn new(
        ram: Arc<Mutex<SimpleMemory>>,
        uart: Arc<Mutex<Uart16550>>,
        ram_base: u64,
        ram_size: usize,
    ) -> Self {
        Self {
            ram,
            uart,
            ram_base,
            ram_size,
            uart_base: 0x10000000,
            uart_size: 0x100,
            htif_base: HTIF_BASE,
            htif_size: HTIF_SIZE,
            htif_write_callback: None,
        }
    }

    /// Set callback for HTIF write events
    pub fn set_htif_write_callback<F>(&mut self, callback: F)
    where
        F: Fn(u64) + Send + Sync + 'static,
    {
        self.htif_write_callback = Some(Arc::new(callback));
    }

    /// Check if address is HTIF MMIO
    fn is_htif(&self, addr: u64) -> bool {
        addr >= self.htif_base && addr < self.htif_base + self.htif_size as u64
    }

    fn is_ram(&self, addr: u64) -> bool {
        addr >= self.ram_base && addr < self.ram_base + self.ram_size as u64
    }

    fn is_uart(&self, addr: u64) -> bool {
        addr >= self.uart_base && addr < self.uart_base + self.uart_size as u64
    }
}

impl MemoryInterface for SystemBus {
    fn read_dword(&self, addr: u64) -> Result<u64, MemoryError> {
        if addr >= self.ram_base && (addr + 8) <= (self.ram_base + self.ram_size as u64) {
            return self.ram.lock().unwrap().read_dword(addr - self.ram_base);
        }
        if self.is_uart(addr) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        if self.is_htif(addr) {
            // HTIF tohost register reads return 0 (no signal pending)
            // This matches Spike behavior
            return Ok(0);
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn read_word(&self, addr: u64) -> Result<u32, MemoryError> {
        if addr >= self.ram_base && (addr + 4) <= (self.ram_base + self.ram_size as u64) {
            return self.ram.lock().unwrap().read_word(addr - self.ram_base);
        }
        if self.is_uart(addr) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn read_half(&self, addr: u64) -> Result<u16, MemoryError> {
        if addr >= self.ram_base && (addr + 2) <= (self.ram_base + self.ram_size as u64) {
            return self.ram.lock().unwrap().read_half(addr - self.ram_base);
        }
        if self.is_uart(addr) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn read_byte(&self, addr: u64) -> Result<u8, MemoryError> {
        if self.is_ram(addr) {
            return self.ram.lock().unwrap().read_byte(addr - self.ram_base);
        }
        if self.is_uart(addr) {
            let offset = addr - self.uart_base;
            return Ok(self.uart.lock().unwrap().read_reg(offset));
        }
        Err(MemoryError::InvalidAddress(addr))
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
        if addr >= self.ram_base && (addr + 8) <= (self.ram_base + self.ram_size as u64) {
            return self
                .ram
                .lock()
                .unwrap()
                .write_dword(addr - self.ram_base, value);
        }
        if self.is_uart(addr) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        if self.is_htif(addr) {
            // HTIF tohost write - trigger callback if registered
            if let Some(ref callback) = self.htif_write_callback {
                callback(value);
            }
            return Ok(());
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn write_word(&mut self, addr: u64, value: u32) -> Result<(), MemoryError> {
        if addr >= self.ram_base && (addr + 4) <= (self.ram_base + self.ram_size as u64) {
            return self
                .ram
                .lock()
                .unwrap()
                .write_word(addr - self.ram_base, value);
        }
        if self.is_uart(addr) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn write_half(&mut self, addr: u64, value: u16) -> Result<(), MemoryError> {
        if addr >= self.ram_base && (addr + 2) <= (self.ram_base + self.ram_size as u64) {
            return self
                .ram
                .lock()
                .unwrap()
                .write_half(addr - self.ram_base, value);
        }
        if self.is_uart(addr) {
            return Err(MemoryError::InvalidAddress(addr));
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn write_byte(&mut self, addr: u64, value: u8) -> Result<(), MemoryError> {
        if self.is_ram(addr) {
            return self
                .ram
                .lock()
                .unwrap()
                .write_byte(addr - self.ram_base, value);
        }
        if self.is_uart(addr) {
            let offset = addr - self.uart_base;
            self.uart.lock().unwrap().write_reg(offset, value);
            return Ok(());
        }
        Err(MemoryError::InvalidAddress(addr))
    }

    fn size(&self) -> usize {
        self.ram_size + self.uart_size // Approximate
    }
}

/// Default maximum cycles before timeout
const DEFAULT_MAX_CYCLES: u64 = 10_000_000;

/// Default tohost address (matches Spike's HTIF at 0x40008000)
const DEFAULT_TOHOST: u64 = 0x4000_8000;

/// HTIF MMIO base address (matches Spike's HTIF device)
const HTIF_BASE: u64 = 0x4000_8000;
/// HTIF MMIO size (single 8-byte register for tohost)
const HTIF_SIZE: usize = 8;

// HTIF (Host-Target Interface) constants
/// HTIF Device ID for syscall
const HTIF_DEVICE_SYSCALL: u64 = 0;
/// HTIF Command ID for syscall
const HTIF_CMD_SYSCALL: u64 = 0;
/// HTIF device shift
const HTIF_DEVICE_SHIFT: u64 = 56;
/// HTIF command shift
const HTIF_CMD_SHIFT: u64 = 48;
/// HTIF device mask
const HTIF_DEVICE_MASK: u64 = 0xFF << HTIF_DEVICE_SHIFT;
/// HTIF command mask
const HTIF_CMD_MASK: u64 = 0xFF << HTIF_CMD_SHIFT;
/// HTIF payload mask (lower 48 bits)
const HTIF_PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

/// Dump signature data from memory
///
/// Reads signature region from memory and returns as bytes.
/// Returns None if signature section info is not available.
pub fn dump_signature(
    mem: &Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
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

/// Extract exit code from tohost value using HTIF format or alternative formats
///
/// Supports two formats:
/// 1. Standard HTIF format: tohost = (device << 56) | (cmd << 48) | payload
///    - device: 8 bits (device ID), must be 0 for syscall/exit
///    - cmd: 8 bits (command ID), must be 0 for exit
///    - payload: 48 bits, exit signal format: (exit_code << 1) | 1
///
/// 2. Alternative format (used by some test programs): tohost = (1 << 63) | exit_code
///    - bit 63: write marker
///    - bits 0-31: exit code directly
///
/// Returns the exit code if a valid exit signal is detected, None otherwise.
fn try_extract_exit_code(tohost_value: u64) -> Option<u32> {
    // Skip zero value (no signal)
    if tohost_value == 0 {
        return None;
    }

    // Try standard HTIF format first
    // Extract device (bits 56-63)
    let device = (tohost_value & HTIF_DEVICE_MASK) >> HTIF_DEVICE_SHIFT;
    // Extract command (bits 48-55)
    let cmd = (tohost_value & HTIF_CMD_MASK) >> HTIF_CMD_SHIFT;
    // Extract payload (bits 0-47)
    let payload = tohost_value & HTIF_PAYLOAD_MASK;

    // Standard HTIF: device=0, cmd=0, payload lowest bit = 1
    if device == HTIF_DEVICE_SYSCALL && cmd == HTIF_CMD_SYSCALL && payload & 1 != 0 {
        return Some((payload >> 1) as u32);
    }

    // Try alternative format: (1 << 63) | exit_code
    // This is used by some RISC-V test programs that set bit 63 as a write marker
    if tohost_value & (1u64 << 63) != 0 {
        // Extract exit code from lower 32 bits (or 16 bits for small values)
        let exit_code = (tohost_value & 0xFFFFFFFF) as u32;
        return Some(exit_code);
    }

    // No recognized exit signal format
    None
}

/// Clear tohost value in memory (Spike-compatible behavior)
///
/// After processing a tohost write, the tohost location should be cleared to 0.
fn clear_tohost(
    mem: &Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    tohost_addr: u64,
    verbose: bool,
) {
    let tohost_pa = tohost_addr; // Already physical address in callers
    if let Ok(mut guard) = mem.lock() {
        // Write 8 bytes of zeros to clear tohost
        for i in 0..8 {
            if let Err(e) = guard.write_byte(tohost_pa + i, 0) {
                if verbose {
                    eprintln!("[WARN] Failed to clear tohost byte {}: {}", i, e);
                }
            }
        }
    } else if verbose {
        eprintln!("[WARN] Failed to lock memory for clear_tohost");
    }
}

use std::path::Path;

/// Load and execute an ELF file
///
/// # Arguments
/// * `elf_data` - Raw ELF file bytes
/// * `max_cycles` - Maximum cycles before timeout (default: 10 million)
/// * `tohost_addr` - Optional tohost address (auto-detected from ELF if not provided)
/// * `log_commits` - Optional path to write commit log (Spike-compatible format)
/// * `verbose` - Whether to print verbose debug output
///
/// # Returns
/// ExecutionResult containing exit code, cycle count, and final state
pub fn load_and_run(
    elf_data: &[u8],
    max_cycles: Option<u64>,
    tohost_addr: Option<u64>,
    log_commits: Option<&Path>,
    verbose: bool,
) -> Result<ExecutionResult, ExecutorError> {
    let max_cycles = max_cycles.unwrap_or(DEFAULT_MAX_CYCLES);

    // Step 1: Load ELF file
    let loaded = load_elf_file(elf_data)?;
    let (entry_point, memory, signature, elf_tohost, base_addr) = (
        loaded.entry_point,
        loaded.memory,
        loaded.signature,
        loaded.tohost,
        loaded.base_addr,
    );

    if verbose {
        eprintln!("[DEBUG] load_elf_file returned: entry_point=0x{:016x}, base_addr=0x{:016x}, memory.len()={}, elf_tohost={:?}",
                  entry_point, base_addr, memory.len(), elf_tohost);
    }

    // Determine tohost address with priority:
    // 1. Command line provided address (tohost_addr)
    // 2. Address from ELF .tohost section (elf_tohost)
    // 3. Default address (DEFAULT_TOHOST)
    let tohost = tohost_addr.or(elf_tohost).unwrap_or(DEFAULT_TOHOST);

    // Step 2: Allocate and initialize memory
    let mem_size = memory.len();
    if mem_size == 0 {
        return Err(ExecutorError::MemoryAllocationFailed);
    }

    // Create RAM
    let ram = Arc::new(Mutex::new(SimpleMemory::new(mem_size)));
    {
        let mem_guard = ram.lock().unwrap();
        mem_guard.load_program(&memory, base_addr);
    }

    // Create UART
    let uart = Arc::new(Mutex::new(Uart16550::new(0x10000000)));

    // Set UART output callback to print to stdout
    {
        let mut uart_guard = uart.lock().unwrap();
        uart_guard.set_output_callback(|byte| {
            print!("{}", byte as char);
            use std::io::Write;
            std::io::stdout().flush().unwrap();
        });
    }

    // Create System Bus
    let bus = Arc::new(Mutex::new(SystemBus::new(
        ram.clone(),
        uart.clone(),
        base_addr,
        mem_size,
    )));

    // Create exit signal tracker for HTIF callback
    let exit_code = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX)); // u32::MAX = not set
    let exit_code_clone = exit_code.clone();

    // Register HTIF write callback
    {
        let mut bus_guard = bus.lock().unwrap();
        let exit_code_inner = exit_code_clone.clone();
        bus_guard.set_htif_write_callback(move |value| {
            // Check if this is an exit signal
            if let Some(code) = try_extract_exit_code(value) {
                exit_code_inner.store(code, std::sync::atomic::Ordering::SeqCst);
            }
        });
    }

    // Cast to MemoryInterface trait object
    let bus_interface: Arc<Mutex<dyn MemoryInterface + Send + Sync>> = bus;

    // Step 3: Create and configure core
    let mut core = RiscvCore::new(bus_interface.clone(), bus_interface.clone());
    core.set_verbose(verbose);

    // Reset core with entry point and base address 0 for SystemBus mapping
    // SystemBus expects PA = VA (identity mapping) or explicit ranges
    // With base_addr=0 in Core, VA is passed directly to SystemBus.
    core.reset(entry_point, 0);

    // Create commit logger if requested
    let mut commit_logger: Option<CommitLogger> = log_commits
        .map(CommitLogger::new_file)
        .transpose()
        .map_err(|e| {
            ExecutorError::ExecutionError(format!("Failed to create commit log: {}", e))
        })?;

    // Save initial register state for logging
    let mut regs_before = [0u64; 32];
    let mut regs_after = [0u64; 32];

    // Step 4: Execution loop
    let mut cycles = 0u64;
    let mut last_tohost_value: u64 = 0;

    // Convert tohost virtual address to physical address for checking
    // Since we use SystemBus with base_addr=0 in Core, VA = PA.
    let tohost_pa = tohost;

    if verbose {
        eprintln!("[DEBUG] Starting execution: entry_point=0x{:016x}, base_addr=0 (for bus), tohost=0x{:016x}",
                  entry_point, tohost_pa);
    }

    while cycles < max_cycles {
        // Read current PC for result
        let current_pc = core.state().pc;
        let pc_before = current_pc;

        // Get instruction for logging (need to read before step)
        let instruction = {
            let mem = bus_interface.lock().unwrap();
            mem.read_word(pc_before.wrapping_sub(base_addr))
                .unwrap_or(0)
        };

        // Capture register state before execution
        regs_before = core.state().regs;

        // Execute one instruction
        match core.step() {
            Ok(()) => {
                cycles += 1;

                // Capture register state after execution
                regs_after = core.state().regs;

                // Log commit if logger is active
                if let Some(ref mut logger) = commit_logger {
                    // Get privilege mode (3 = machine mode)
                    let privilege = core.state().privilege as u8;

                    // Try to detect memory access
                    let mem_access = None; // Simplified: detect in executor if needed

                    // Log the commit
                    let _ = logger.log_commit(
                        0, // hartid
                        privilege,
                        pc_before,
                        instruction,
                        &regs_before,
                        &regs_after,
                        mem_access,
                    );
                }

                // Check for exit signal from HTIF callback first
                // This handles writes to HTIF MMIO at 0x40008000
                let htif_exit = exit_code.load(std::sync::atomic::Ordering::SeqCst);
                if htif_exit != u32::MAX {
                    if verbose {
                        eprintln!("[DEBUG] HTIF exit signal detected: code={}", htif_exit);
                    }
                    // Reset for potential re-use
                    exit_code.store(u32::MAX, std::sync::atomic::Ordering::SeqCst);
                    let sig_data = dump_signature(&bus_interface, signature.as_ref())
                        .ok()
                        .flatten();
                    return Ok(ExecutionResult {
                        exit_code: htif_exit,
                        cycles,
                        final_pc: core.state().pc,
                        timed_out: false,
                        error: None,
                        signature_addr: signature.map(|s| s.vaddr),
                        signature_data: sig_data,
                    });
                }

                // Check for tohost write (exit signal) after EVERY instruction
                // This ensures we detect the write immediately
                if let Ok(mem_guard) = bus_interface.lock() {
                    match mem_guard.read_dword(tohost_pa) {
                        Ok(tohost_value) => {
                            // Track tohost value changes for debugging
                            if verbose && tohost_value != last_tohost_value {
                                eprintln!(
                                    "[DEBUG] Cycle {}: tohost changed from 0x{:016x} to 0x{:016x}",
                                    cycles, last_tohost_value, tohost_value
                                );
                                last_tohost_value = tohost_value;
                            }

                            // Check if tohost contains a valid exit signal
                            if let Some(exit_code_val) = try_extract_exit_code(tohost_value) {
                                if verbose {
                                    eprintln!(
                                        "[DEBUG] Exit signal detected: code={}",
                                        exit_code_val
                                    );
                                }
                                // Clear tohost after processing (Spike-compatible behavior)
                                drop(mem_guard);
                                clear_tohost(&bus_interface, tohost_pa, verbose);
                                let sig_data = dump_signature(&bus_interface, signature.as_ref())
                                    .ok()
                                    .flatten();
                                return Ok(ExecutionResult {
                                    exit_code: exit_code_val,
                                    cycles,
                                    final_pc: core.state().pc,
                                    timed_out: false,
                                    error: None,
                                    signature_addr: signature.map(|s| s.vaddr),
                                    signature_data: sig_data,
                                });
                            } else if tohost_value != 0 && verbose {
                                // Non-zero but without exit command marker - possible memory corruption or other command
                                eprintln!(
                                    "[WARN] tohost has non-command value: {:#x}",
                                    tohost_value
                                );
                            }
                        }
                        Err(e) => {
                            // Only log errors periodically to avoid spam
                            if verbose && cycles.is_multiple_of(1000) {
                                eprintln!("[DEBUG] Cycle {}: tohost read failed: {}", cycles, e);
                            }
                        }
                    }
                }

                // Debug output every 1000 cycles
                if verbose && cycles.is_multiple_of(1000) {
                    let state = core.state();
                    eprintln!(
                        "[DEBUG] Cycle {}: PC=0x{:010x}, ra={}, sp={}, gp={}",
                        cycles, current_pc, state.regs[1], state.regs[2], state.regs[3]
                    );
                }
            }
            Err(e) => {
                let sig_data = dump_signature(&bus_interface, signature.as_ref())
                    .ok()
                    .flatten();
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
    if verbose {
        eprintln!(
            "[DEBUG] Timeout at cycle {}: PC=0x{:016x}, tohost=0x{:016x}",
            cycles,
            core.state().pc,
            last_tohost_value
        );
    }

    let sig_data = dump_signature(&bus_interface, signature.as_ref())
        .ok()
        .flatten();
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
/// * `tohost_addr` - Optional tohost address for exit detection
/// * `log_commits` - Optional path to write commit log (Spike-compatible format)
/// * `verbose` - Whether to print verbose debug output
///
/// # Returns
/// ExecutionResult
pub fn load_and_run_file(
    elf_path: &str,
    max_cycles: Option<u64>,
    tohost_addr: Option<u64>,
    log_commits: Option<PathBuf>,
    verbose: bool,
) -> Result<ExecutionResult, ExecutorError> {
    // Read ELF file
    let elf_data = std::fs::read(elf_path)
        .map_err(|e| ExecutorError::ElfLoadError(ElfError::IoError(e.to_string())))?;

    load_and_run(
        &elf_data,
        max_cycles,
        tohost_addr,
        log_commits.as_deref(),
        verbose,
    )
}

/// Reset the simulator state
///
/// Creates a fresh core state ready for execution
pub fn reset_core(core: &mut RiscvCore, entry_point: u64, base_addr: u64) {
    core.reset(entry_point, base_addr);
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
    memory: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    /// tohost address for exit detection
    tohost: u64,
    /// Maximum cycles
    max_cycles: u64,
    /// Signature section info
    signature: Option<SignatureInfo>,
    /// Verbose output flag
    verbose: bool,
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
            verbose: false,
        }
    }

    /// Set verbosity
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
        self.core.set_verbose(verbose);
    }

    /// Load ELF data into memory
    ///
    /// # Address Handling
    /// This method loads the ELF program at the addresses specified by the ELF
    /// headers. The `base_addr` from the ELF is used to correctly place segments
    /// in memory, but the core is reset with `base_addr` for VA-to-PA translation.
    ///
    /// Note: This differs from `load_and_run` which uses SystemBus with
    /// identity mapping (VA = PA). This method uses SimpleMemory which expects
    /// the program to be loaded at the correct virtual addresses.
    ///
    /// # Returns
    /// The entry point address from the ELF header
    pub fn load_elf(&mut self, elf_data: &[u8]) -> Result<u64, ExecutorError> {
        let loaded = load_elf_file(elf_data)?;
        let (entry_point, memory, sig, tohost, base_addr) = (
            loaded.entry_point,
            loaded.memory,
            loaded.signature,
            loaded.tohost,
            loaded.base_addr,
        );
        self.signature = sig;

        // NOTE: This implementation is simplified and still uses SimpleMemory internally
        // if created via new(). It does not support SystemBus yet.
        // For full support, use load_and_run.

        // This is a partial fix to allow compilation.
        // Ideally RiscVSimulator should be refactored to use SystemBus as well.
        // Create new memory and load program
        // We create a SimpleMemory here because RiscVSimulator is typically used for
        // unit tests or benchmarks that expect a simple flat memory environment.
        // For full system simulation (UART, etc.), load_and_run should be used.
        let ram = Arc::new(Mutex::new(SimpleMemory::new(memory.len())));
        {
            let guard = ram.lock().unwrap();
            guard.load_program(&memory, base_addr);
        }
        self.memory = ram;
        self.core = RiscvCore::new(self.memory.clone(), self.memory.clone());
        self.core.set_verbose(self.verbose);

        // Update tohost address
        if let Some(addr) = tohost {
            self.tohost = addr;
        }

        // Reset core to entry point with base address for VA translation
        self.core.reset(entry_point, base_addr);

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

        // Track last tohost value to detect changes
        let mut last_tohost_value: u64 = 0;

        while cycles < max_cycles {
            // Execute one instruction first
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

            // Check for tohost write AFTER executing instruction
            // This ensures we detect the write immediately
            let guard = self.memory.lock().unwrap();
            match guard.read_dword(self.tohost) {
                Ok(tohost_value) => {
                    // Track tohost value changes for debugging
                    if self.verbose && tohost_value != last_tohost_value {
                        eprintln!(
                            "[DEBUG] Cycle {}: PC=0x{:010x}, tohost changed from 0x{:016x} to 0x{:016x}",
                            cycles,
                            self.core.state().pc,
                            last_tohost_value,
                            tohost_value
                        );
                        last_tohost_value = tohost_value;
                    }

                    // Check for exit signal using consistent extraction logic
                    if let Some(exit_code) = try_extract_exit_code(tohost_value) {
                        if self.verbose {
                            eprintln!("[DEBUG] Exit signal detected: code={}", exit_code);
                        }
                        // Clear tohost after processing (Spike-compatible behavior)
                        drop(guard);
                        clear_tohost(&self.memory, self.tohost, self.verbose);
                        return Ok(self.get_result(cycles));
                    } else if tohost_value != 0 && self.verbose {
                        // Non-zero but without exit command marker - possible memory corruption or other command
                        eprintln!("[WARN] tohost has non-command value: {:#x}", tohost_value);
                    }
                }
                Err(e) => {
                    // Only log errors periodically to avoid spam
                    if self.verbose && cycles.is_multiple_of(1000) {
                        eprintln!(
                            "[DEBUG] Cycle {}: PC=0x{:010x}, tohost read failed: {}",
                            cycles,
                            self.core.state().pc,
                            e
                        );
                    }
                }
            }
            drop(guard);
        }

        // Timeout - final debug output
        if self.verbose {
            eprintln!(
                "[DEBUG] Timeout at cycle {}: PC=0x{:010x}, tohost=0x{:016x}",
                cycles,
                self.core.state().pc,
                last_tohost_value
            );
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
    pub fn memory(&self) -> &Arc<Mutex<dyn MemoryInterface + Send + Sync>> {
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
    fn test_htif_exit_code_extraction() {
        // HTIF standard format: exit code 0
        let val = (HTIF_DEVICE_SYSCALL << HTIF_DEVICE_SHIFT)
            | (HTIF_CMD_SYSCALL << HTIF_CMD_SHIFT)
            | 1u64;
        assert_eq!(try_extract_exit_code(val), Some(0));

        // HTIF standard format: exit code 1
        let val = (HTIF_DEVICE_SYSCALL << HTIF_DEVICE_SHIFT)
            | (HTIF_CMD_SYSCALL << HTIF_CMD_SHIFT)
            | 3u64;
        assert_eq!(try_extract_exit_code(val), Some(1));

        // Alternative format
        let val = (1u64 << 63) | 42;
        assert_eq!(try_extract_exit_code(val), Some(42));
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
        let result = load_and_run(&[], Some(100), None, false);
        // Should fail due to invalid ELF
        assert!(result.is_err());
    }

    #[test]
    fn test_system_bus_routing() {
        let ram_size = 0x1000;
        let ram_base = 0x80000000;
        let uart_base = 0x10000000;

        let ram = Arc::new(Mutex::new(SimpleMemory::new(ram_size)));
        let uart = Arc::new(Mutex::new(Uart16550::new(uart_base)));

        let mut bus = SystemBus::new(ram.clone(), uart.clone(), ram_base, ram_size);

        // Test RAM access
        let ram_addr = ram_base + 0x100;
        bus.write_word(ram_addr, 0x12345678).unwrap();
        assert_eq!(bus.read_word(ram_addr).unwrap(), 0x12345678);
        assert_eq!(ram.lock().unwrap().read_word(0x100).unwrap(), 0x12345678);

        // Test UART access
        // Uart IER register is at base + 1
        let uart_addr = uart_base + 1;
        bus.write_byte(uart_addr, 0x01).unwrap();
        assert_eq!(bus.read_byte(uart_addr).unwrap(), 0x01);
        assert_eq!(uart.lock().unwrap().read_reg(1), 0x01);

        // Test unmapped access
        let unmapped_addr = 0x20000000;
        assert!(bus.read_word(unmapped_addr).is_err());
    }

    #[test]
    fn test_simulator_load_elf_reinit() {
        let mut sim = RiscVSimulator::new(0x1000);

        let dummy_elf = vec![0; 100]; // Invalid ELF

        // First load attempt
        let _ = sim.load_elf(&dummy_elf);

        // Second load attempt - this should not panic or deadlock
        let _ = sim.load_elf(&dummy_elf);

        // Verify we can still access the simulator
        assert_eq!(sim.state().pc, 0);
    }
}
