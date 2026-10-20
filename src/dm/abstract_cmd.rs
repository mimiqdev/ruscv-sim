//! Abstract commands.
//!
//! Debug Spec 1.0 §3.7. Only Access Register (`cmdtype` 0) and Access Memory
//! (`cmdtype` 2) run. The first failing check wins, and an unsupported
//! option is reported before a not-halted hart (§3.7.1.1, §3.7.1.3).

use super::hart::DebugHart;
use super::DebugModule;

const CMDERR_NOT_SUPPORTED: u8 = 2;
const CMDERR_EXCEPTION: u8 = 3;
const CMDERR_HALT_RESUME: u8 = 4;

impl<H: DebugHart> DebugModule<H> {
    pub(super) fn write_command(&mut self, command: u32) {
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
        if self.ndmreset || self.hartsel != 0 || !self.hart.in_debug_mode() {
            self.cmderr = CMDERR_HALT_RESUME;
            return;
        }
        if command >> 24 == 0 {
            self.access_register(command);
        } else {
            self.access_memory(command);
        }
    }

    fn access_register(&mut self, command: u32) {
        let size64 = ((command >> 20) & 0b111) == 3;
        let transfer = command & (1 << 17) != 0;
        let write = command & (1 << 16) != 0;
        let regno = command & 0xffff;
        // §3.7.1.1: transfer 0 skips the read or write and does not check regno.
        if !transfer {
            return;
        }
        if let Err(code) = self.transfer_reg(regno, write, size64) {
            self.cmderr = code;
        }
    }

    fn transfer_reg(&mut self, regno: u32, write: bool, size64: bool) -> Result<(), u8> {
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
                    let previous = self.hart.read_gpr(index);
                    let value = self.written_low(size64, previous);
                    self.hart.write_gpr(index, value);
                } else {
                    self.set_arg0(self.hart.read_gpr(index), size64);
                }
                Ok(())
            }
            // §3.7.1.1: a register that does not exist returns cmderr 3.
            // misa has F and D clear, so f0–f31 do not exist.
            0x1020..=0x103f => Err(CMDERR_EXCEPTION),
            0x7b0 => self.transfer_dcsr(write, size64),
            0x7b1 => self.transfer_dpc(write, size64),
            0x7b2 => self.transfer_dscratch(0, write, size64),
            0x7b3 => self.transfer_dscratch(1, write, size64),
            0x0000..=0x0fff => self.transfer_csr(regno as u16, write, size64),
            _ => Err(CMDERR_EXCEPTION),
        }
    }

    fn transfer_dcsr(&mut self, write: bool, size64: bool) -> Result<(), u8> {
        if write {
            let value = self.written_low(size64, self.hart.dcsr());
            self.hart.write_dcsr(value);
        } else {
            self.set_arg0(self.hart.dcsr(), size64);
        }
        Ok(())
    }

    fn transfer_dpc(&mut self, write: bool, size64: bool) -> Result<(), u8> {
        if write {
            let value = self.written_low(size64, self.hart.dpc());
            self.hart.write_dpc(value);
        } else {
            self.set_arg0(self.hart.dpc(), size64);
        }
        Ok(())
    }

    fn transfer_dscratch(&mut self, which: u8, write: bool, size64: bool) -> Result<(), u8> {
        let current = if which == 0 {
            self.hart.dscratch0()
        } else {
            self.hart.dscratch1()
        };
        if write {
            let value = self.written_low(size64, current);
            if which == 0 {
                self.hart.write_dscratch0(value);
            } else {
                self.hart.write_dscratch1(value);
            }
        } else {
            self.set_arg0(current, size64);
        }
        Ok(())
    }

    fn transfer_csr(&mut self, addr: u16, write: bool, size64: bool) -> Result<(), u8> {
        if write {
            let previous = self
                .hart
                .read_csr_machine(addr)
                .map_err(|_| CMDERR_EXCEPTION)?;
            let value = self.written_low(size64, previous);
            self.hart
                .write_csr_machine(addr, value)
                .map_err(|_| CMDERR_EXCEPTION)
        } else {
            let value = self
                .hart
                .read_csr_machine(addr)
                .map_err(|_| CMDERR_EXCEPTION)?;
            self.set_arg0(value, size64);
            Ok(())
        }
    }

    fn access_memory(&mut self, command: u32) {
        let aamsize = (command >> 20) & 0b111;
        let size = 1u32 << aamsize;
        let write = command & (1 << 16) != 0;
        let postincrement = command & (1 << 19) != 0;
        // §3.7 introduction, 1.0 bugfix: aamsize does not change argument
        // width. The address is always data2 and data3.
        let addr = self.arg1();
        if write {
            let value = self.arg0();
            if self.hart.debug_physical_store(addr, size, value).is_err() {
                // Misaligned and access fault are both the exception code.
                // Contract D15. §3.7.1.3.
                self.cmderr = CMDERR_EXCEPTION;
                return;
            }
        } else {
            match self.hart.debug_physical_load(addr, size) {
                Ok(value) => {
                    // Unused high bits of a short read are UNSPECIFIED.
                    // This module writes one zero-extended result.
                    self.set_arg0(value, true);
                }
                Err(_) => {
                    self.cmderr = CMDERR_EXCEPTION;
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
            // `aamvirtual`, `aamsize` above 3, bits 18:17, and bits 15:0.
            // Bit 16 is `write`. Bit 19 is `aampostincrement`, which is implemented.
            command & (1 << 23) != 0
                || aamsize > 3
                || command & (0b11 << 17) != 0
                || command & 0xffff != 0
        }
        // Quick Access is cmdtype 1. Every other cmdtype is also unsupported.
        _ => true,
    }
}
