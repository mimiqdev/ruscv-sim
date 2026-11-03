//! Commit log module - Output Spike-compatible commit logs
//!
//! This module provides functionality to output commit logs that are
//! compatible with Spike's --log-commits format.

use super::observation::FpCsrAlias;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

/// Pinned Spike keys every logged register effect by
/// `(register_or_csr << 4) | kind` in a `std::map` and prints that map in
/// ascending key order (`riscv/execute.cc`, `commit_log_print_insn`). The
/// kinds are 0 for an integer register, 1 for a floating-point register and
/// 4 for a CSR, which is why each register index prints its integer write
/// before its floating-point write.
const SPIKE_CSR_KIND: u64 = 4;
/// CSR numbers of the aliased `fflags`/`frm` CSRs (contract D5). Pinned
/// Spike's `fcsr` is a composite of these two, so its commit log names them:
/// `fflags` prints as `c1_fflags` at map key `0x14` and `frm` as `c2_frm` at
/// `0x24`, in the register buckets of CSR numbers 1 and 2.
const FFLAGS_CSR: u64 = 0x001;
const FRM_CSR: u64 = 0x002;

/// Pinned Spike's `log_reg_write` map key for one effect.
fn spike_effect_key(index: u64, kind: u64) -> u64 {
    (index << 4) | kind
}

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

    #[cfg(test)]
    pub(crate) fn for_test(output: Box<dyn Write>) -> Self {
        Self { output }
    }

    /// Log a commit in Spike-compatible format
    ///
    /// Format: `core   <hartid>: <privilege> <pc> (<opcode>) [x<reg>  <value>]... [mem <addr> [<value>]]`
    ///
    /// Where:
    /// - `<hartid>` is the hart ID (typically 0)
    /// - `<privilege>` is the privilege mode (0=U, 1=S, 3=M)
    /// - `<pc>` is the program counter (hex, 18 digits with 0x prefix)
    /// - `<opcode>` is the instruction opcode (hex, 10 digits with 0x prefix)
    /// - `[x<reg>  <value>]` is repeated for each register that changed (x1-x31)
    /// - `[mem <addr>]` for loads, `[mem <addr> <value>]` for stores
    ///
    /// # Arguments
    /// * `hartid` - Hart ID (typically 0)
    /// * `privilege` - Privilege mode (0=U, 1=S, 3=M)
    /// * `pc` - Program counter
    /// * `opcode` - Instruction opcode (machine code)
    /// * `regs_before` - Register values before execution [x0..x31]
    /// * `regs_after` - Register values after execution [x0..x31]
    /// * `mem_access` - Optional memory access info
    #[allow(clippy::too_many_arguments)]
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

    /// Format Hart-owned effects with the established snapshot-era text
    /// presentation. Rich memory/CSR/FPR facts remain available to other sinks;
    /// this compatibility text deliberately omits them. Equal-value writes
    /// are facts, but remain omitted in this presentation.
    ///
    /// The opcode field follows pinned Spike's `commit_log_print_insn`:
    /// the fetched bits zero-padded to `length / 4` hex digits — `(0x0001)`
    /// for a 16-bit parcel, `(0x00000093)` for a 32-bit instruction
    /// (contract D7).
    ///
    /// Floating-point effects follow the same pinned function (contract D9):
    /// an FPR write prints as ` f<rd>  <FLEN hex>`, and the aliased FP-CSR
    /// writes as ` c1_fflags <XLEN hex>` / ` c2_frm <XLEN hex>` — Spike logs
    /// the CSRs behind its composite `fcsr`, so an `fcsr` write prints both
    /// and an equal-value write still prints. Everything follows the
    /// ascending `(index << 4) | kind` key order of Spike's `log_reg_write`
    /// map. Under `rv64ima`/`rv64imac` no FPR or FP-CSR fact exists, so the
    /// text is unchanged.
    pub fn log_record(&mut self, record: &super::observation::CommitRecord) -> io::Result<()> {
        let retired = record.retired;
        write!(
            self.output,
            "core   {}: {} {:#018x} (",
            record.hart_id, retired.privilege as u8, retired.pc
        )?;
        match record.instruction_length {
            2 => write!(self.output, "0x{:04x})", retired.instruction)?,
            _ => write!(self.output, "0x{:08x})", retired.instruction)?,
        };
        // Reproduce Spike's map order: for each register index the integer
        // write (kind 0) precedes the floating-point write (kind 1), and each
        // FP-CSR write (kind 4) sorts into the bucket of its CSR number —
        // `fflags` (bucket 1) after `f1` and before `x2`, `frm` (bucket 2)
        // after `f2` and before `x3`.
        for index in 0..32u8 {
            let gpr = record
                .effects
                .gpr
                .iter()
                .find(|write| write.index == index && write.before != write.after);
            if let Some(write) = gpr {
                write!(self.output, " x{}  {:#018x}", write.index, write.after)?;
            }
            let fpr = record.effects.fpr.iter().find(|write| write.index == index);
            if let Some(write) = fpr {
                // Spike prints `" %c%-2d "` at FLEN and `0x%016` hex digits.
                write!(self.output, " f{:<2} {:#018x}", write.index, write.after)?;
            }
            for (csr, name, alias) in [
                (FFLAGS_CSR, "fflags", FpCsrAlias::Fflags),
                (FRM_CSR, "frm", FpCsrAlias::Frm),
            ] {
                if (spike_effect_key(csr, SPIKE_CSR_KIND) >> 4) as u8 != index {
                    continue;
                }
                let write = record
                    .effects
                    .fp_csr_writes
                    .iter()
                    .find(|write| write.alias == alias);
                if let Some(write) = write {
                    // Spike prints `" c%d_%s "` at XLEN for a CSR write.
                    write!(self.output, " c{}_{} {:#018x}", csr, name, write.value)?;
                }
            }
        }
        writeln!(self.output)
    }

    /// Log a comment/tag line (Spike-compatible)
    ///
    /// Format: >>>>>  &lt;text&gt;
    pub fn log_comment(&mut self, text: &str) -> io::Result<()> {
        writeln!(self.output, ">>>>  {}", text)
    }
}

impl super::observation::ObservationSink for CommitLogger {
    type Error = io::Error;
    fn observe(&mut self, observation: &super::observation::Observation) -> io::Result<()> {
        match observation {
            super::observation::Observation::Commit(record) => self.log_record(record),
            super::observation::Observation::Trap(_) => Ok(()),
        }
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
