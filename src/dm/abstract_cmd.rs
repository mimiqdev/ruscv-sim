//! Abstract commands.
//!
//! Debug Spec 1.0 §3.7. Only Access Register (`cmdtype` 0) and Access Memory
//! (`cmdtype` 2) run. The first failing check wins, and an unsupported
//! option is reported before a not-halted hart (§3.7.1.1, §3.7.1.3).

use super::DebugModule;
use crate::core::{DebugMemFault, RiscvCore};

const CMDERR_NOT_SUPPORTED: u8 = 2;
const CMDERR_EXCEPTION: u8 = 3;
const CMDERR_HALT_RESUME: u8 = 4;
/// Debug Spec 1.0 §3.14: a simulator fault, not an M-mode exception.
const CMDERR_OTHER: u8 = 7;

/// §3.7.1.3 cmderr 3 is an M-mode exception. §3.14 cmderr 7 is other.
fn memory_cmderr(fault: DebugMemFault) -> u8 {
    match fault {
        DebugMemFault::Misaligned | DebugMemFault::AccessFault => CMDERR_EXCEPTION,
        DebugMemFault::Other => CMDERR_OTHER,
    }
}

impl DebugModule {
    pub(super) fn write_command(&mut self, hart: &mut RiscvCore, command: u32) {
        // §3.14 `command`: ignored while `cmderr` is not zero. `busy` is
        // never 1, so that half of the rule does not arise.
        if self.cmderr != 0 {
            return;
        }
        if unsupported(command) {
            self.cmderr = CMDERR_NOT_SUPPORTED;
            return;
        }
        // §3.14 cmderr 4: running, unavailable under ndmreset, or not hart 0.
        if self.ndmreset || self.hartsel != 0 || !hart.in_debug_mode() {
            self.cmderr = CMDERR_HALT_RESUME;
            return;
        }
        if command >> 24 == 0 {
            self.access_register(hart, command);
        } else {
            self.access_memory(hart, command);
        }
    }

    fn access_register(&mut self, hart: &mut RiscvCore, command: u32) {
        let size64 = ((command >> 20) & 0b111) == 3;
        let transfer = command & (1 << 17) != 0;
        let write = command & (1 << 16) != 0;
        let regno = command & 0xffff;
        // §3.7.1.1: transfer 0 skips the read or write and does not check regno.
        if !transfer {
            return;
        }
        if let Err(code) = self.transfer_reg(hart, regno, write, size64) {
            self.cmderr = code;
        }
    }

    fn transfer_reg(
        &mut self,
        hart: &mut RiscvCore,
        regno: u32,
        write: bool,
        size64: bool,
    ) -> Result<(), u8> {
        match regno {
            0x1000 => {
                if !write {
                    self.set_arg0(0, size64);
                }
                Ok(())
            }
            0x1001..=0x101f => {
                let index = (regno - 0x1000) as u8;
                if write {
                    let previous = read_gpr(hart, index);
                    let value = self.written_low(size64, previous);
                    write_gpr(hart, index, value);
                } else {
                    self.set_arg0(read_gpr(hart, index), size64);
                }
                Ok(())
            }
            // §3.7.1.1: a register that does not exist returns cmderr 3.
            // misa has F and D clear, so f0–f31 do not exist.
            0x1020..=0x103f => Err(CMDERR_EXCEPTION),
            0x7b0 => self.transfer_dcsr(hart, write, size64),
            0x7b1 => self.transfer_dpc(hart, write, size64),
            0x7b2 => self.transfer_dscratch(hart, 0, write, size64),
            0x7b3 => self.transfer_dscratch(hart, 1, write, size64),
            0x0000..=0x0fff => self.transfer_csr(hart, regno as u16, write, size64),
            _ => Err(CMDERR_EXCEPTION),
        }
    }

    fn transfer_dcsr(&mut self, hart: &mut RiscvCore, write: bool, size64: bool) -> Result<(), u8> {
        if write {
            let value = self.written_low(size64, hart.dcsr());
            hart.write_dcsr(value);
        } else {
            self.set_arg0(hart.dcsr(), size64);
        }
        Ok(())
    }

    fn transfer_dpc(&mut self, hart: &mut RiscvCore, write: bool, size64: bool) -> Result<(), u8> {
        if write {
            let value = self.written_low(size64, hart.dpc());
            hart.write_dpc(value);
        } else {
            self.set_arg0(hart.dpc(), size64);
        }
        Ok(())
    }

    fn transfer_dscratch(
        &mut self,
        hart: &mut RiscvCore,
        which: u8,
        write: bool,
        size64: bool,
    ) -> Result<(), u8> {
        let current = if which == 0 {
            hart.dscratch0()
        } else {
            hart.dscratch1()
        };
        if write {
            let value = self.written_low(size64, current);
            if which == 0 {
                hart.write_dscratch0(value);
            } else {
                hart.write_dscratch1(value);
            }
        } else {
            self.set_arg0(current, size64);
        }
        Ok(())
    }

    fn transfer_csr(
        &mut self,
        hart: &mut RiscvCore,
        addr: u16,
        write: bool,
        size64: bool,
    ) -> Result<(), u8> {
        if write {
            let previous = hart
                .state()
                .csr
                .read_as_machine(addr)
                .map_err(|_| CMDERR_EXCEPTION)?;
            let value = self.written_low(size64, previous);
            hart.state_mut()
                .csr
                .write_as_machine(addr, value)
                .map_err(|_| CMDERR_EXCEPTION)
        } else {
            let value = hart
                .state()
                .csr
                .read_as_machine(addr)
                .map_err(|_| CMDERR_EXCEPTION)?;
            self.set_arg0(value, size64);
            Ok(())
        }
    }

    fn access_memory(&mut self, hart: &mut RiscvCore, command: u32) {
        let aamsize = (command >> 20) & 0b111;
        let size = 1u32 << aamsize;
        let write = command & (1 << 16) != 0;
        let postincrement = command & (1 << 19) != 0;
        // §3.7 introduction, 1.0 bugfix: aamsize does not change argument
        // width. The address is always data2 and data3.
        let addr = self.arg1();
        if write {
            let value = self.arg0();
            if let Err(fault) = hart.debug_physical_store(addr, size, value) {
                // Misaligned and access fault are cmderr 3 (contract D15,
                // §3.7.1.3). A simulator fault is cmderr 7 (§3.14).
                self.cmderr = memory_cmderr(fault);
                return;
            }
        } else {
            match hart.debug_physical_load(addr, size) {
                Ok(value) => {
                    // Unused high bits of a short read are UNSPECIFIED.
                    // This module writes one zero-extended result.
                    self.set_arg0(value, true);
                }
                Err(fault) => {
                    self.cmderr = memory_cmderr(fault);
                    return;
                }
            }
        }
        if postincrement {
            self.set_arg1(addr.wrapping_add(u64::from(size)));
        }
    }

    fn arg0(&self) -> u64 {
        u64::from(self.data[0]) | (u64::from(self.data[1]) << 32)
    }

    fn arg1(&self) -> u64 {
        u64::from(self.data[2]) | (u64::from(self.data[3]) << 32)
    }

    /// `aarsize` 2 keeps bits 63:32. That half is UNSPECIFIED in §3.7.1.1.
    fn written_low(&self, size64: bool, previous: u64) -> u64 {
        if size64 {
            self.arg0()
        } else {
            (previous & 0xffff_ffff_0000_0000) | u64::from(self.data[0])
        }
    }

    fn set_arg0(&mut self, value: u64, size64: bool) {
        self.data[0] = value as u32;
        if size64 {
            self.data[1] = (value >> 32) as u32;
        }
    }

    fn set_arg1(&mut self, value: u64) {
        self.data[2] = value as u32;
        self.data[3] = (value >> 32) as u32;
    }
}

fn read_gpr(hart: &RiscvCore, index: u8) -> u64 {
    if index == 0 {
        0
    } else {
        hart.state().regs[usize::from(index)]
    }
}

fn write_gpr(hart: &mut RiscvCore, index: u8, value: u64) {
    if index != 0 {
        hart.state_mut().regs[usize::from(index)] = value;
    }
}

/// Unsupported `cmdtype`, a bit defined as 0, or an option this module does
/// not implement. This does not look at halt state.
fn unsupported(command: u32) -> bool {
    match command >> 24 {
        0 => {
            let aarsize = (command >> 20) & 0b111;
            command & (1 << 23) != 0
                || !matches!(aarsize, 2 | 3)
                || command & (1 << 19) != 0
                || command & (1 << 18) != 0
        }
        2 => {
            let aamsize = (command >> 20) & 0b111;
            // `aamvirtual`, `aamsize` above 3, and bits defined as 0
            // (18:17 and 13:0). Bits 15:14 are reserved for target-specific
            // uses and this target defines none, so a 1 is an unsupported
            // option. Debug Spec 1.0 §3.7: unsupported options and defined-0
            // bits set `cmderr` to 2. Bit 16 is `write`. Bit 19 is
            // `aampostincrement`, which is implemented.
            command & (1 << 23) != 0
                || aamsize > 3
                || command & (0b11 << 17) != 0
                || command & 0xffff != 0
        }
        // Quick Access is cmdtype 1. Every other cmdtype is also unsupported.
        _ => true,
    }
}
