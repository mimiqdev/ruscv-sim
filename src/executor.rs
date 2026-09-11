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
pub(crate) const HTIF_DEVICE_SYSCALL: u64 = 0;
/// HTIF Command ID for syscall
pub(crate) const HTIF_CMD_SYSCALL: u64 = 0;
/// HTIF device shift
pub(crate) const HTIF_DEVICE_SHIFT: u64 = 56;
/// HTIF command shift
pub(crate) const HTIF_CMD_SHIFT: u64 = 48;
/// HTIF device mask
pub(crate) const HTIF_DEVICE_MASK: u64 = 0xFF << HTIF_DEVICE_SHIFT;
/// HTIF command mask
pub(crate) const HTIF_CMD_MASK: u64 = 0xFF << HTIF_CMD_SHIFT;
/// HTIF payload mask (lower 48 bits)
pub(crate) const HTIF_PAYLOAD_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

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
/// Extract exit code from tohost value using HTIF format or alternative formats
///
/// Returns the exit code if a valid exit signal is detected, None otherwise.
pub(crate) fn try_extract_exit_code(tohost_value: u64) -> Option<u32> {
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
pub(crate) fn clear_tohost(
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

/// Address form a configuration uses for image-declared metadata.
///
/// Both public entry points address the same loaded image differently: the
/// native bus maps RAM at the image base, so a guest address is already the
/// address the bus takes, while the flat library holds the image relative to its
/// base and addresses it by a checked storage offset.
#[derive(Debug, Clone, Copy)]
enum AddressForm {
    /// Native bus configuration: image/guest addresses pass through unchanged.
    Bus,
    /// Flat configuration: image/guest addresses resolve to buffer offsets.
    Flat { base_addr: u64, memory_size: u64 },
}

impl AddressForm {
    /// Resolve an image-declared guest range into this configuration's address.
    ///
    /// The flat form is checked: an address below the image base, a range that
    /// overflows the address space, or a range that leaves the image buffer is
    /// an explicit error rather than a wrapped or truncated offset. Per-use
    /// requirements such as the exit poll's eight-byte alignment are enforced by
    /// the caller, not here.
    fn resolve(self, guest_addr: u64, len: u64, what: &str) -> Result<u64, ExecutorError> {
        match self {
            AddressForm::Bus => Ok(guest_addr),
            AddressForm::Flat {
                base_addr,
                memory_size,
            } => {
                let offset = guest_addr.checked_sub(base_addr).ok_or_else(|| {
                    ExecutorError::ExecutionError(format!(
                        "{what} address 0x{guest_addr:016x} is below image base 0x{base_addr:016x}"
                    ))
                })?;
                let end = offset.checked_add(len).ok_or_else(|| {
                    ExecutorError::ExecutionError(format!(
                        "{what} address 0x{guest_addr:016x} overlaps the end of the address space"
                    ))
                })?;
                if end > memory_size {
                    return Err(ExecutorError::ExecutionError(format!(
                        "{what} address 0x{guest_addr:016x} maps to flat offset 0x{offset:016x}, outside the {memory_size:#x}-byte image memory"
                    )));
                }
                Ok(offset)
            }
        }
    }
}

/// Where a loaded image declares its exit signal and signature artifact.
///
/// One owner holds the image's placement facts so both entry points resolve them
/// through the same checked conversion instead of each implementing its own.
#[derive(Debug, Clone, Default)]
struct ImagePlacement {
    /// Lowest load-segment address of the image.
    base_addr: u64,
    /// Bytes addressable in the configuration's image buffer or RAM window.
    memory_size: u64,
    /// Image-declared exit signal, as a guest address.
    tohost: Option<u64>,
    /// Image-declared signature region, as guest metadata.
    signature: Option<SignatureInfo>,
}

impl ImagePlacement {
    fn new(
        base_addr: u64,
        memory_size: usize,
        tohost: Option<u64>,
        signature: Option<SignatureInfo>,
    ) -> Self {
        Self {
            base_addr,
            memory_size: memory_size as u64,
            tohost,
            signature,
        }
    }

    /// The address form the flat library configuration addresses images in.
    fn address_form(&self) -> AddressForm {
        AddressForm::Flat {
            base_addr: self.base_addr,
            memory_size: self.memory_size,
        }
    }

    /// The image's declared exit signal in the requested address form.
    ///
    /// Absent metadata yields `None`; a placement this configuration cannot
    /// address is an error.
    fn tohost(&self, form: AddressForm) -> Result<Option<u64>, ExecutorError> {
        self.tohost
            .map(|addr| form.resolve(addr, 8, "ELF tohost"))
            .transpose()
    }

    /// The address of a declared signature region in the requested address form.
    fn signature_address(
        &self,
        info: &SignatureInfo,
        form: AddressForm,
    ) -> Result<u64, ExecutorError> {
        form.resolve(info.vaddr, info.size, "ELF signature")
    }

    /// The image's declared signature metadata, including its guest address.
    fn signature_info(&self) -> Option<&SignatureInfo> {
        self.signature.as_ref()
    }

    /// The image-declared exit signal as a guest address, if it declares one.
    fn tohost_guest(&self) -> Option<u64> {
        self.tohost
    }
}

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

    // The bus configuration maps RAM at the image base, so image-declared
    // metadata resolves to its own guest address.
    let placement = ImagePlacement::new(base_addr, memory.len(), elf_tohost, signature);

    // Determine tohost address with priority:
    // 1. Command line provided address (tohost_addr)
    // 2. Address from ELF .tohost section (elf_tohost)
    // 3. Default address (DEFAULT_TOHOST)
    let tohost = tohost_addr
        .or(placement.tohost(AddressForm::Bus)?)
        .unwrap_or(DEFAULT_TOHOST);

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
    // Note: No explicit flush here - stdout will be flushed at program exit
    {
        let mut uart_guard = uart.lock().unwrap();
        uart_guard.set_output_callback(|byte| {
            print!("{}", byte as char);
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
        .map(|path| {
            CommitLogger::new_file(path).map_err(|e| {
                let error_type = match e.kind() {
                    std::io::ErrorKind::PermissionDenied => "Permission denied",
                    std::io::ErrorKind::NotFound => "Path not found",
                    std::io::ErrorKind::AlreadyExists => "File already exists",
                    std::io::ErrorKind::IsADirectory => "Path is a directory",
                    _ => "Unknown error",
                };
                ExecutorError::ExecutionError(format!(
                    "Failed to create commit log file '{}': {} ({})",
                    path.display(),
                    error_type,
                    e
                ))
            })
        })
        .transpose()?;

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
        let regs_before = core.state().regs;

        // Execute one instruction
        match core.step() {
            Ok(()) => {
                cycles += 1;

                // Capture register state after execution
                let regs_after = core.state().regs;

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
                    let sig_data = dump_signature(&bus_interface, placement.signature_info())
                        .ok()
                        .flatten();
                    return Ok(ExecutionResult {
                        exit_code: htif_exit,
                        cycles,
                        final_pc: core.state().pc,
                        timed_out: false,
                        error: None,
                        signature_addr: placement.signature_info().map(|s| s.vaddr),
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
                                let sig_data =
                                    dump_signature(&bus_interface, placement.signature_info())
                                        .ok()
                                        .flatten();
                                return Ok(ExecutionResult {
                                    exit_code: exit_code_val,
                                    cycles,
                                    final_pc: core.state().pc,
                                    timed_out: false,
                                    error: None,
                                    signature_addr: placement.signature_info().map(|s| s.vaddr),
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
                let sig_data = dump_signature(&bus_interface, placement.signature_info())
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
                    signature_addr: placement.signature_info().map(|s| s.vaddr),
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

    let sig_data = dump_signature(&bus_interface, placement.signature_info())
        .ok()
        .flatten();
    Ok(ExecutionResult {
        exit_code: 1, // Non-zero indicates abnormal termination
        cycles,
        final_pc: core.state().pc,
        timed_out: true,
        error: Some(format!("Timeout after {} cycles", max_cycles)),
        signature_addr: placement.signature_info().map(|s| s.vaddr),
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
///
/// The wrapper owns one flat RAM image. Guest virtual addresses are converted to
/// flat storage offsets by subtracting the loaded image's base address; those
/// flat offsets are the same address space used by [`RiscVSimulator::read_mem`],
/// [`RiscVSimulator::write_mem`] and [`RiscVSimulator::set_tohost`].
///
/// # Exit signal configuration
///
/// The exit signal polled by [`RiscVSimulator::run`] is selected in this order:
///
/// 1. An explicit [`RiscVSimulator::set_tohost`] flat offset.
/// 2. The `.tohost`/`tohost` metadata of the loaded image, converted from its
///    guest address to a flat offset at load time.
/// 3. The default tohost address.
///
/// Loading an image that declares its own tohost discards a manual offset set
/// *before* the load, so the image's declared signal wins. Calling `set_tohost`
/// after loading always overrides the image. Loading an image with no declared
/// tohost clears a previous image's derived offset instead of reusing it.
pub struct RiscVSimulator {
    /// The RISC-V core
    core: RiscvCore,
    /// Shared memory
    memory: Arc<Mutex<dyn MemoryInterface + Send + Sync>>,
    /// Explicit flat storage offset set through `set_tohost`
    manual_tohost: Option<u64>,
    /// Loaded image placement; resolved through the shared address forms
    image: ImagePlacement,
    /// Maximum cycles
    max_cycles: u64,
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
            manual_tohost: None,
            image: ImagePlacement::default(),
            max_cycles: DEFAULT_MAX_CYCLES,
            verbose: false,
        }
    }

    /// Flat storage offset polled by [`RiscVSimulator::run`] for the exit signal.
    ///
    /// Re-resolving the image's declared signal cannot fail here: the loaded
    /// image and its memory are replaced together in [`RiscVSimulator::load_elf`],
    /// and a declared signal that this configuration cannot address is rejected
    /// before either is assigned. Keep that pairing if the load path changes.
    fn tohost_offset(&self) -> Result<u64, ExecutorError> {
        match self.manual_tohost {
            Some(addr) => Ok(addr),
            None => Ok(self
                .image
                .tohost(self.image.address_form())?
                .unwrap_or(DEFAULT_TOHOST)),
        }
    }

    /// Set verbosity
    pub fn set_verbose(&mut self, verbose: bool) {
        self.verbose = verbose;
        self.core.set_verbose(verbose);
    }

    /// Load ELF data into memory and reset the core to its entry point.
    ///
    /// # Address handling
    /// The image is loaded into the wrapper's flat RAM relative to the lowest
    /// segment address, and the core is reset with that base so guest addresses
    /// translate to flat offsets by subtraction. The image's declared
    /// `.tohost`/`tohost` metadata is converted the same way, so
    /// [`RiscVSimulator::run`] polls the signal the guest actually writes. This
    /// is storage adaptation for the flat configuration; it is not guest
    /// virtual-to-physical translation and it grants no UART/HTIF device
    /// mapping. Use [`load_and_run`] for the native bus configuration.
    ///
    /// Loading replaces the previous image and its exit configuration.
    ///
    /// # Errors
    /// A declared tohost that the flat image cannot use, because it is below the
    /// image base, outside the image memory, or not eight-byte aligned at its
    /// flat offset, fails the load instead of silently timing out later.
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
        // Resolve image-derived metadata before mutating wrapper state, so a
        // placement this configuration cannot address leaves the wrapper
        // unchanged.
        let image = ImagePlacement::new(base_addr, memory.len(), tohost, sig);
        if let (Some(guest), Some(offset)) =
            (image.tohost_guest(), image.tohost(image.address_form())?)
        {
            if !offset.is_multiple_of(8) {
                return Err(ExecutorError::ExecutionError(format!(
                    "ELF tohost address 0x{guest:016x} maps to flat offset 0x{offset:016x}, which the exit poll cannot read eight-byte aligned"
                )));
            }
        }

        // NOTE: This implementation is simplified and still uses SimpleMemory internally
        // if created via new(). It does not support SystemBus yet.
        // For full support, use load_and_run.

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

        // Replace image-owned metadata. A manual offset set before this load is
        // superseded when the image declares its own tohost; an image without
        // metadata must not inherit the previous image's derived offset.
        if image.tohost_guest().is_some() {
            self.manual_tohost = None;
        }
        self.image = image;

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

    /// Set the flat storage offset polled for the guest exit signal.
    ///
    /// `addr` is an offset into the wrapper's flat RAM, the same address space
    /// used by [`RiscVSimulator::read_mem`] and [`RiscVSimulator::write_mem`]. It
    /// is not an ELF/guest virtual address and is not translated.
    ///
    /// An explicit offset overrides image-declared tohost metadata. When it is
    /// called before [`RiscVSimulator::load_elf`], the loaded image's own
    /// declared tohost still takes precedence; call it after loading to override
    /// the image.
    pub fn set_tohost(&mut self, addr: u64) {
        self.manual_tohost = Some(addr);
    }

    /// Step one instruction
    pub fn step(&mut self) -> Result<(), ExecutorError> {
        self.core
            .step()
            .map_err(|e| ExecutorError::ExecutionError(e.to_string()))
    }

    /// Run until the guest exits, the budget is exhausted, or a step fails.
    ///
    /// The exit signal is polled at the configured flat storage offset after
    /// every retired instruction. A decoded guest exit is retained before the
    /// RAM signal is cleared, so a nonzero exit code is reported exactly once.
    /// The reported cycle count and `final_pc` describe the instruction that
    /// wrote the signal. A zero budget executes no instruction and reports a
    /// timeout, and an exit in the final permitted slot is not a timeout.
    pub fn run(&mut self, max_cycles: Option<u64>) -> Result<ExecutionResult, ExecutorError> {
        let max_cycles = max_cycles.unwrap_or(self.max_cycles);
        let tohost = self.tohost_offset()?;
        let mut cycles = 0u64;

        // Track last tohost value for verbose diagnostics
        let mut last_tohost_value: u64 = 0;

        while cycles < max_cycles {
            // Execute one instruction first
            match self.step() {
                Ok(()) => {
                    cycles += 1;
                }
                Err(e) => {
                    return Ok(self.finish(
                        cycles,
                        1,
                        false,
                        Some(format!("Execution error: {}", e)),
                    ));
                }
            }

            // Check for tohost write AFTER executing instruction
            // This ensures we detect the write immediately
            let observed = self.memory.lock().unwrap().read_dword(tohost);
            match observed {
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
                        // Clear tohost after processing (Spike-compatible behavior),
                        // keeping the decoded guest exit for the result.
                        clear_tohost(&self.memory, tohost, self.verbose);
                        return Ok(self.finish(cycles, exit_code, false, None));
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

        Ok(self.finish(
            cycles,
            1,
            true,
            Some(format!("Timeout after {} cycles", max_cycles)),
        ))
    }

    /// Build an execution result from already-observed state.
    ///
    /// The exit code is supplied by the caller because the guest's RAM signal
    /// may already have been cleared. The signature artifact is read from the
    /// image's declared range in flat memory and reported alongside, without
    /// disturbing the exit, cycle count or final PC.
    fn finish(
        &self,
        cycles: u64,
        exit_code: u32,
        timed_out: bool,
        error: Option<String>,
    ) -> ExecutionResult {
        let (signature_addr, signature_data, artifact_error) = self.signature_artifact();
        let error = match (error, artifact_error) {
            (primary, None) => primary,
            (None, Some(artifact)) => Some(artifact),
            (Some(primary), Some(artifact)) => Some(format!("{primary}; {artifact}")),
        };

        ExecutionResult {
            exit_code,
            cycles,
            final_pc: self.core.state().pc,
            timed_out,
            error,
            signature_addr,
            signature_data,
        }
    }

    /// Read the loaded image's declared signature artifact from flat memory.
    ///
    /// The returned address is the guest metadata address from the image, while
    /// the bytes come from the corresponding flat offset. Absent metadata yields
    /// no artifact, a zero-length region yields an empty artifact, and a region
    /// that cannot be mapped or read yields an explicit diagnostic instead of
    /// silent absence.
    fn signature_artifact(&self) -> (Option<u64>, Option<Vec<u8>>, Option<String>) {
        let Some(info) = self.image.signature_info() else {
            return (None, None, None);
        };
        let addr = info.vaddr;
        let size = info.size;

        if size == 0 {
            return (Some(addr), Some(Vec::new()), None);
        }

        match self
            .image
            .signature_address(info, self.image.address_form())
        {
            Ok(offset) => match self.read_mem(offset, size as usize) {
                Ok(bytes) => (Some(addr), Some(bytes), None),
                Err(error) => (
                    Some(addr),
                    None,
                    Some(format!("Signature artifact unavailable: {error}")),
                ),
            },
            Err(error) => (
                Some(addr),
                None,
                Some(format!("Signature artifact unavailable: {error}")),
            ),
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

    /// Read `size` bytes from a flat storage offset.
    ///
    /// `addr` is a backend storage offset, not an ELF/guest virtual address and
    /// not a translated physical address. An empty request returns an empty
    /// vector without accessing memory. A nonempty in-range request returns
    /// exactly the requested bytes in ascending address order. A nonempty range
    /// that is out of range or overflows the address space returns
    /// [`ExecutorError`] and does not wrap, retry without progress, fabricate
    /// bytes, or return a partial vector as success. Inspection does not execute
    /// guest instructions or modify PC, registers, or RAM.
    pub fn read_mem(&self, addr: u64, size: usize) -> Result<Vec<u8>, ExecutorError> {
        if size == 0 {
            return Ok(Vec::new());
        }

        if addr.checked_add((size - 1) as u64).is_none() {
            return Err(ExecutorError::ExecutionError(format!(
                "Memory read error: address 0x{addr:016x} size {size} overflows the address space"
            )));
        }

        let mut data = Vec::with_capacity(size);
        let guard = self.memory.lock().unwrap();
        for offset in 0..size {
            let current_addr = addr + offset as u64;
            match guard.read_byte(current_addr) {
                Ok(byte) => data.push(byte),
                Err(e) => {
                    return Err(ExecutorError::ExecutionError(format!(
                        "Memory read error at 0x{current_addr:016x}: {e}"
                    )));
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
        let result = load_and_run(&[], Some(100), None, None, false);
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

    const PLACEMENT_BASE: u64 = 0x8000_0000;

    fn signature(vaddr: u64, size: u64) -> SignatureInfo {
        SignatureInfo {
            vaddr,
            size,
            file_offset: 0,
        }
    }

    #[test]
    fn test_placement_resolves_both_address_forms() {
        let placement = ImagePlacement::new(
            PLACEMENT_BASE,
            0x1_0000,
            Some(PLACEMENT_BASE + 0x1000),
            Some(signature(PLACEMENT_BASE + 0x2000, 8)),
        );

        // The bus configuration maps RAM at the image base, so the guest
        // address is the address it polls and reads.
        assert_eq!(
            placement.tohost(AddressForm::Bus).unwrap(),
            Some(PLACEMENT_BASE + 0x1000)
        );
        assert_eq!(
            placement
                .signature_address(placement.signature_info().unwrap(), AddressForm::Bus)
                .unwrap(),
            PLACEMENT_BASE + 0x2000
        );

        // The flat configuration addresses the same metadata as offsets.
        assert_eq!(
            placement.tohost(placement.address_form()).unwrap(),
            Some(0x1000)
        );
        assert_eq!(
            placement
                .signature_address(
                    placement.signature_info().unwrap(),
                    placement.address_form()
                )
                .unwrap(),
            0x2000
        );
        assert_eq!(
            placement.signature_info().map(|info| info.vaddr),
            Some(PLACEMENT_BASE + 0x2000),
            "the reported signature address stays the guest metadata address"
        );
    }

    #[test]
    fn test_placement_base_zero_uses_raw_offsets() {
        let placement = ImagePlacement::new(0, 0x1_0000, Some(0x1000), None);

        assert_eq!(
            placement.tohost(placement.address_form()).unwrap(),
            Some(0x1000)
        );
        assert_eq!(
            placement.tohost(AddressForm::Bus).unwrap(),
            Some(0x1000),
            "both forms coincide at base zero"
        );
    }

    #[test]
    fn test_placement_rejects_unaddressable_flat_ranges() {
        let below_base =
            ImagePlacement::new(PLACEMENT_BASE, 0x1_0000, Some(PLACEMENT_BASE - 8), None);
        assert!(below_base.tohost(below_base.address_form()).is_err());

        let beyond_image = ImagePlacement::new(
            PLACEMENT_BASE,
            0x1_0000,
            Some(PLACEMENT_BASE + 0x1_0000),
            None,
        );
        assert!(
            beyond_image.tohost(beyond_image.address_form()).is_err(),
            "the eight-byte dword must fit inside the image memory"
        );

        let overflowing = ImagePlacement::new(0, 0x1_0000, Some(u64::MAX - 7), None);
        assert!(overflowing.tohost(overflowing.address_form()).is_err());

        // The bus form performs no conversion, so it neither wraps nor invents
        // an address: the configuration's own device map decides what exists.
        assert_eq!(
            overflowing.tohost(AddressForm::Bus).unwrap(),
            Some(u64::MAX - 7)
        );
    }

    #[test]
    fn test_placement_absent_metadata_is_none() {
        let placement = ImagePlacement::new(PLACEMENT_BASE, 0x1_0000, None, None);

        assert_eq!(placement.tohost(AddressForm::Bus).unwrap(), None);
        assert_eq!(placement.tohost(placement.address_form()).unwrap(), None);
        assert!(placement.signature_info().is_none());
    }

    #[test]
    fn test_placement_signature_range_boundaries() {
        // The last byte of the image is addressable; one past it is not.
        let ends_at_limit = ImagePlacement::new(
            PLACEMENT_BASE,
            0x1000,
            None,
            Some(signature(PLACEMENT_BASE + 0x800, 0x800)),
        );
        assert_eq!(
            ends_at_limit
                .signature_address(
                    ends_at_limit.signature_info().unwrap(),
                    ends_at_limit.address_form()
                )
                .unwrap(),
            0x800
        );

        let one_past = ImagePlacement::new(
            PLACEMENT_BASE,
            0x1000,
            None,
            Some(signature(PLACEMENT_BASE + 0x800, 0x801)),
        );
        assert!(one_past
            .signature_address(one_past.signature_info().unwrap(), one_past.address_form())
            .is_err());

        // An empty range needs no bytes; the library's empty-artifact rule
        // short-circuits before this call, and the conversion still refuses an
        // offset that leaves the image memory entirely.
        let empty_at_limit = ImagePlacement::new(
            PLACEMENT_BASE,
            0x1000,
            None,
            Some(signature(PLACEMENT_BASE + 0x1000, 0)),
        );
        assert_eq!(
            empty_at_limit
                .signature_address(
                    empty_at_limit.signature_info().unwrap(),
                    empty_at_limit.address_form()
                )
                .unwrap(),
            0x1000
        );

        let empty_beyond = ImagePlacement::new(
            PLACEMENT_BASE,
            0x1000,
            None,
            Some(signature(PLACEMENT_BASE + 0x1001, 0)),
        );
        assert!(empty_beyond
            .signature_address(
                empty_beyond.signature_info().unwrap(),
                empty_beyond.address_form()
            )
            .is_err());
    }
}
