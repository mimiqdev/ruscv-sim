//! DMI-level Debug Module.
//!
//! Debug Specification 1.0, tag `5695c0a`, §3.1 and §3.14. Register reads and
//! writes finish inside the call, so `abstractcs.busy` is 0. There is no
//! DMI `op` result. That belongs to the transport (U4).
//!
//! The flat `RiscVSimulator::run` loop and the native `run_native_inner` loop
//! both step a `Machine`. Hosting this module on either loop is U3, which
//! admits a transaction at a hart boundary and applies `ndmreset`. U2 tests
//! drive a bare hart and `RiscvCore::step_outcome`.

mod abstract_cmd;
mod hart;
mod regs;

pub use hart::DebugHart;

use regs::{
    hartsel_field, hartsel_of, ABSTRACTCS, ACKHAVERESET, COMMAND, DATA0, DMACTIVE, DMCONTROL,
    DMSTATUS, HALTREQ, HALTSUM0, HARTINFO, NDMRESET, RESUMEREQ,
};

/// One hart's Debug Module, addressed by a 7-bit DMI register number.
pub struct DebugModule<H> {
    hart: H,
    dmactive: bool,
    /// `hartsel`, 20 bits. `HARTSELLEN` is 20 (§3.14).
    hartsel: u32,
    ndmreset: bool,
    /// `abstractcs.cmderr`, three bits.
    cmderr: u8,
    data: [u32; 4],
    /// §3.5 allows the reset value 0 or 1. The hart is already running, so
    /// this module resets resume ack to 1.
    resumeack: bool,
    /// Set at construction for the hart's power-on reset (§3.2). Cleared
    /// only by `ackhavereset` for hart 0. Not updated on `dmactive` or
    /// `ndmreset` edges. The `ndmreset` half of §3.2 is U3.
    havereset: bool,
}

impl<H: DebugHart> DebugModule<H> {
    /// `dmactive` powers up at 0 (§3.14). `havereset` is already 1.
    /// Version discovery (§3.13) is a debugger sequence, not part of `new`.
    pub fn new(hart: H) -> Self {
        Self {
            hart,
            dmactive: false,
            hartsel: 0,
            ndmreset: false,
            cmderr: 0,
            data: [0; 4],
            resumeack: true,
            havereset: true,
        }
    }

    /// Read one DMI register. The address is the low 7 bits (§3.1, §3.14).
    ///
    /// While `dmactive` is 0 every read returns 0. Unimplemented addresses
    /// also return 0, including `abstractauto`, `progbuf0`, `sbcs`, and
    /// `data4`–`data11`.
    pub fn dmi_read(&mut self, addr: u8) -> u32 {
        if !self.dmactive {
            return 0;
        }
        match addr & 0x7f {
            addr @ DATA0..=0x07 => self.data[usize::from(addr - DATA0)],
            DMCONTROL => self.read_dmcontrol(),
            DMSTATUS => self.read_dmstatus(),
            HARTINFO => self.read_hartinfo(),
            ABSTRACTCS => self.read_abstractcs(),
            COMMAND => 0,
            HALTSUM0 => self.read_haltsum0(),
            _ => 0,
        }
    }

    /// Write one DMI register. The address is the low 7 bits (§3.14).
    ///
    /// A write that leaves `dmactive` at 0 is dropped. A 1-to-0 transition
    /// resets this module and leaves the hart halted. §3.5 leaves resume on
    /// DM reset UNSPECIFIED. A later `resumereq` is what calls `debug_resume`.
    pub fn dmi_write(&mut self, addr: u8, value: u32) {
        let addr = addr & 0x7f;
        if addr == DMCONTROL {
            self.write_dmcontrol(value);
            return;
        }
        if !self.dmactive {
            return;
        }
        match addr {
            addr @ DATA0..=0x07 => self.data[usize::from(addr - DATA0)] = value,
            ABSTRACTCS => {
                // §3.14 `cmderr` is R/W1C. §1.3.3 leaves a partial write
                // undefined. This module clears each bit that is written 1,
                // which is Spike's behavior. `relaxedpriv` is hardwired 0.
                let clear = ((value >> 8) & 0b111) as u8;
                self.cmderr &= !clear;
            }
            COMMAND => self.write_command(value),
            _ => {}
        }
    }

    pub fn hart(&self) -> &H {
        &self.hart
    }

    pub fn hart_mut(&mut self) -> &mut H {
        &mut self.hart
    }

    fn write_dmcontrol(&mut self, value: u32) {
        let active = value & DMACTIVE != 0;
        if !active {
            if self.dmactive {
                self.reset_module();
            }
            return;
        }
        self.dmactive = true;
        // Writes of haltreq, resumereq, and ackhavereset apply to the new
        // hartsel (§3.14).
        self.hartsel = hartsel_of(value) & 0x000f_ffff;
        let haltreq = value & HALTREQ != 0;
        if self.hartsel == 0 {
            self.hart.set_haltreq(haltreq);
        }
        if value & ACKHAVERESET != 0 && self.hartsel == 0 {
            self.havereset = false;
        }
        // resumereq is ignored when haltreq is set after this same write.
        if value & RESUMEREQ != 0 && !haltreq {
            self.resume_selected();
        }
        self.ndmreset = value & NDMRESET != 0;
    }

    fn reset_module(&mut self) {
        self.dmactive = false;
        self.hartsel = 0;
        self.ndmreset = false;
        self.cmderr = 0;
        self.data = [0; 4];
        self.resumeack = true;
        self.hart.set_haltreq(false);
    }

    fn resume_selected(&mut self) {
        // A hart under ndmreset is unavailable, not halted (§3.2, §3.4).
        // The bit checked here is the value from before this write stores
        // ndmreset. hartsel other than 0 does not touch hart 0.
        if self.hartsel != 0 || self.ndmreset {
            return;
        }
        if self.hart.in_debug_mode() {
            self.hart.debug_resume();
            // The resume completes before dmi_write returns, so ack is 1
            // on the next dmstatus read (§3.5).
            self.resumeack = true;
        }
    }

    fn read_dmcontrol(&self) -> u32 {
        let mut value = hartsel_field(self.hartsel) | DMACTIVE;
        if self.ndmreset {
            value |= NDMRESET;
        }
        value
    }

    fn read_dmstatus(&self) -> u32 {
        // version = 3, authenticated = 1. impebreak, authbusy, hasresethaltreq,
        // confstrptrvalid, and stickyunavail stay 0.
        let mut value = 3 | (1 << 7);
        if self.ndmreset {
            value |= 1 << 24;
        }
        if self.hartsel == 0 {
            if self.havereset {
                value |= (1 << 19) | (1 << 18);
            }
            if self.resumeack {
                value |= (1 << 17) | (1 << 16);
            }
            // Exactly one of nonexistent, unavailable, running, halted (§3.4).
            // An unavailable hart is not halted (§3.14).
            if self.ndmreset {
                value |= (1 << 13) | (1 << 12);
            } else if self.hart_halted() {
                value |= (1 << 9) | (1 << 8);
            } else {
                value |= (1 << 11) | (1 << 10);
            }
        } else {
            value |= (1 << 15) | (1 << 14);
        }
        value
    }

    fn read_hartinfo(&self) -> u32 {
        // nscratch = 2 at bits 23:20. dataaccess, datasize, and dataaddr are 0.
        // No CSR shadows data0–data3 (§3.14).
        if self.hartsel == 0 {
            2 << 20
        } else {
            0
        }
    }

    fn read_abstractcs(&self) -> u32 {
        // progbufsize 0, busy 0, relaxedpriv 0, datacount 4.
        (u32::from(self.cmderr) << 8) | 4
    }

    fn read_haltsum0(&self) -> u32 {
        // The LSB is the halt status of hart {hartsel[19:5], 5'b0} (§3.14).
        // Only hart 0 exists, and an unavailable hart is not halted.
        if self.hartsel < 32 && self.hart_halted() {
            1
        } else {
            0
        }
    }

    fn hart_halted(&self) -> bool {
        self.hart.in_debug_mode() && !self.ndmreset
    }
}
