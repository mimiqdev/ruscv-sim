//! DMI-level Debug Module.
//!
//! Debug Specification 1.0, tag `5695c0a`, §3.1 and §3.14. Register reads and
//! writes finish inside the call, except a command with `postexec`. Its
//! program-buffer words run one Hart grant per [`DebugModule::advance`] call,
//! and `abstractcs.busy` is 1 until the last one ends. A system bus access is
//! finished by the caller too: [`DebugModule::next_sba_request`] hands the
//! physical request to a target and [`DebugModule::complete_sba_access`]
//! records its result. There is no DMI `op` result. That belongs to the
//! transport (U4).
//!
//! The Machine owns one of these and admits each transaction at a Hart
//! boundary. `ndmreset` is only a bit here. The Machine sees the rising edge
//! and calls [`crate::machine::Machine::fresh_reset`]; this object is not
//! rebuilt. Tests pass a bare [`RiscvCore`] into the DMI methods and drive the
//! harness turn themselves. The module does not own the hart, so a caller
//! cannot resume, reset, or set `haltreq` through a handle this module hands
//! out. `step_outcome` stays outside these methods: a
//! `haltreq` write must not run a hart turn inside the DM (Debug Spec 1.0 §3.5).

mod abstract_cmd;
mod regs;
mod sba;

use crate::core::RiscvCore;
use regs::{
    hartsel_field, hartsel_of, ABSTRACTAUTO, ABSTRACTAUTO_MASK, ABSTRACTCS, ACKHAVERESET, COMMAND,
    DATA0, DMACTIVE, DMCONTROL, DMSTATUS, HALTREQ, HALTSUM0, HARTINFO, IMPEBREAK, NDMRESET,
    PROGBUF0, PROGBUF_LAST, PROGBUF_WORDS, RESUMEREQ, SBADDRESS0, SBADDRESS1, SBCS, SBDATA0,
    SBDATA1,
};
use sba::SbaState;
pub use sba::{SbaAccessSize, SbaFault, SbaRequest};

const CMDERR_BUSY: u8 = 1;

/// One hart's Debug Module, addressed by a 7-bit DMI register number.
///
/// DMI state lives here. The hart is an argument of [`Self::dmi_read`] and
/// [`Self::dmi_write`], which are the only paths that sample or change its
/// debug latch.
pub struct DebugModule {
    dmactive: bool,
    /// `hartsel`, 20 bits. `HARTSELLEN` is 20 (§3.14).
    hartsel: u32,
    ndmreset: bool,
    /// Hart 0's request survives non-DM reset; the Hart latch is its input.
    haltreq: bool,
    /// `abstractcs.cmderr`, three bits.
    cmderr: u8,
    data: [u32; 4],
    /// `progbuf0`, `progbuf1`. Not addressable by the Hart; only the DM reads
    /// them as opcodes (contract D3).
    progbuf: [u32; PROGBUF_WORDS],
    /// `abstractauto`, masked to `ABSTRACTAUTO_MASK`.
    abstractauto: u32,
    /// Last `command` accepted while idle with `cmderr` 0. Autoexec repeats it.
    last_command: Option<u32>,
    /// `sbcs`, `sbaddress`, `sbdata`, `sbbusy` and the sticky SBA errors.
    sba: SbaState,
    /// Next program-buffer slot to run while `busy`. Slot `PROGBUF_WORDS` is
    /// the implicit EBREAK. `None` is idle.
    exec: Option<u8>,
    /// §3.5 allows the reset value 0 or 1. The hart is already running, so
    /// this module resets resume ack to 1.
    resumeack: bool,
    /// Set at construction for the hart's power-on reset (§3.2). Cleared
    /// only by `ackhavereset` for hart 0. A standalone DMI write does not
    /// change it on `dmactive` or `ndmreset` edges. The Machine sets it when
    /// `ndmreset` releases the hart from [`crate::machine::Machine::fresh_reset`].
    havereset: bool,
    /// `dmstatus.version`. Conforming runs report 3. The U5 mutation reports 4.
    version: u8,
    /// Implemented `data0`–`dataN` registers. Conforming runs report 4.
    /// The U5 mutation reports 2 and ignores `data2` and `data3`.
    datacount: u8,
}

impl DebugModule {
    /// `dmactive` powers up at 0 (§3.14). `havereset` is already 1.
    /// Version discovery (§3.13) is a debugger sequence, not part of `new`.
    ///
    /// `RUSCV_S6_DMSTATUS_VERSION=4` and `RUSCV_S6_DATACOUNT=2` are the §7
    /// item 5 negative mutations. They are not CLI flags. Unset, or any other
    /// value, keeps version 3 and `datacount` 4.
    pub fn new() -> Self {
        let (version, datacount) = u5_discovery_from_env();
        Self::with_discovery(version, datacount)
    }

    fn with_discovery(version: u8, datacount: u8) -> Self {
        Self {
            dmactive: false,
            hartsel: 0,
            ndmreset: false,
            haltreq: false,
            cmderr: 0,
            data: [0; 4],
            progbuf: [0; PROGBUF_WORDS],
            abstractauto: 0,
            last_command: None,
            sba: SbaState::default(),
            exec: None,
            resumeack: true,
            havereset: true,
            version,
            datacount,
        }
    }

    /// Read one DMI register. The address is the low 7 bits (§3.1, §3.14).
    ///
    /// While `dmactive` is 0 every read returns 0. Unimplemented addresses
    /// also return 0, including `data4`–`data11`, `sbaddress2`/`sbaddress3`
    /// and `sbdata2`/`sbdata3`.
    ///
    /// A `data` or `progbuf` read can start the last command (`abstractauto`).
    /// The returned value is captured before that command runs. The hart is
    /// mutable for that reason (contract D9). An SBA read can start a bus
    /// access the same way: the value is sampled first, and `sbcs.sbbusy`
    /// then stays set until [`Self::complete_sba_access`].
    pub fn dmi_read(&mut self, hart: &mut RiscvCore, addr: u8) -> u32 {
        if !self.dmactive {
            return 0;
        }
        match addr & 0x7f {
            addr @ DATA0..=0x07 => match self.data_slot(addr) {
                Some(index) => {
                    let value = self.data[index];
                    self.buffer_access(hart, index as u32);
                    value
                }
                None => 0,
            },
            DMCONTROL => self.read_dmcontrol(),
            DMSTATUS => self.read_dmstatus(hart),
            HARTINFO => self.read_hartinfo(),
            ABSTRACTCS => self.read_abstractcs(),
            COMMAND => 0,
            ABSTRACTAUTO => self.abstractauto,
            addr @ PROGBUF0..=PROGBUF_LAST => {
                let index = usize::from(addr - PROGBUF0);
                let value = self.progbuf[index];
                self.buffer_access(hart, 16 + index as u32);
                value
            }
            SBCS => self.sba.read_cs(),
            addr @ (SBADDRESS0 | SBADDRESS1) => self.sba.read_address(addr),
            addr @ (SBDATA0 | SBDATA1) => self.sba.read_data(addr),
            HALTSUM0 => self.read_haltsum0(hart),
            _ => 0,
        }
    }

    /// Write one DMI register. The address is the low 7 bits (§3.14).
    ///
    /// A write that leaves `dmactive` at 0 is dropped. A 1-to-0 transition
    /// resets this module and leaves the hart halted. §3.5 leaves resume on
    /// DM reset UNSPECIFIED. A later `resumereq` is what calls `debug_resume`.
    ///
    /// A `data` or `progbuf` write stages its value, then can start the last
    /// command (`abstractauto`). An SBA write updates `sbaddress` or `sbdata`
    /// and can start a bus access.
    pub fn dmi_write(&mut self, hart: &mut RiscvCore, addr: u8, value: u32) {
        let addr = addr & 0x7f;
        if addr == DMCONTROL {
            self.write_dmcontrol(hart, value);
            return;
        }
        if !self.dmactive {
            return;
        }
        match addr {
            addr @ DATA0..=0x07 => self.write_data(hart, addr, value),
            ABSTRACTCS => {
                if self.busy_error() {
                    return;
                }
                // §3.14 `cmderr` is R/W1C. §1.3.3 leaves a partial write
                // undefined. This module clears each bit that is written 1,
                // which is Spike's behavior. `relaxedpriv` is hardwired 0.
                let clear = ((value >> 8) & 0b111) as u8;
                self.cmderr &= !clear;
            }
            COMMAND => self.write_command(hart, value),
            ABSTRACTAUTO => self.write_abstractauto(value),
            addr @ PROGBUF0..=PROGBUF_LAST => self.write_progbuf(hart, addr, value),
            SBCS => self.sba.write_cs(value),
            addr @ (SBADDRESS0 | SBADDRESS1) => self.sba.write_address(addr, value),
            addr @ (SBDATA0 | SBDATA1) => self.sba.write_data(addr, value),
            _ => {}
        }
    }

    /// Stage a `data` write, then let `abstractauto` decide whether to start
    /// the last command. A busy access stages nothing.
    fn write_data(&mut self, hart: &mut RiscvCore, addr: u8, value: u32) {
        if self.busy_error() {
            return;
        }
        if let Some(index) = self.data_slot(addr) {
            self.data[index] = value;
            self.buffer_access(hart, index as u32);
        }
    }

    /// Stage a `progbuf` write, then let `abstractauto` decide whether to
    /// start the last command. A busy access stages nothing.
    fn write_progbuf(&mut self, hart: &mut RiscvCore, addr: u8, value: u32) {
        if self.busy_error() {
            return;
        }
        let index = usize::from(addr - PROGBUF0);
        self.progbuf[index] = value;
        self.buffer_access(hart, 16 + index as u32);
    }

    /// `abstractauto`: only the implemented bits are writable, and a busy
    /// access does not change them.
    fn write_abstractauto(&mut self, value: u32) {
        if !self.busy_error() {
            self.abstractauto = value & ABSTRACTAUTO_MASK;
        }
    }

    /// §3.14 `cmderr` 1: a register was accessed while a command ran. Only
    /// written when `cmderr` is 0. Returns whether the access is ignored.
    fn busy_error(&mut self) -> bool {
        if self.exec.is_none() {
            return false;
        }
        if self.cmderr == 0 {
            self.cmderr = CMDERR_BUSY;
        }
        true
    }

    /// Tail of a `data` or `progbuf` access. `bit` is the `abstractauto` bit
    /// that selects this register. A busy access is an error and never starts
    /// a command. A nonzero `cmderr` also suppresses it.
    fn buffer_access(&mut self, hart: &mut RiscvCore, bit: u32) {
        if self.busy_error() || self.abstractauto & (1 << bit) == 0 || self.cmderr != 0 {
            return;
        }
        if let Some(command) = self.last_command {
            self.run_command(hart, command);
        }
    }

    /// Whether a `data` or `progbuf` DMI access would start a command. The
    /// Machine uses it to refuse such an access after unknown completion.
    pub(crate) fn access_triggers_command(&self, addr: u8) -> bool {
        let bit = match addr & 0x7f {
            addr @ DATA0..=0x07 if self.data_slot(addr).is_some() => u32::from(addr - DATA0),
            addr @ PROGBUF0..=PROGBUF_LAST => 16 + u32::from(addr - PROGBUF0),
            _ => return false,
        };
        self.dmactive
            && !self.busy()
            && self.cmderr == 0
            && self.abstractauto & (1 << bit) != 0
            && self.last_command.is_some()
    }

    /// `abstractcs.busy`: a command is between its first and last grant.
    pub fn busy(&self) -> bool {
        self.exec.is_some()
    }

    fn write_dmcontrol(&mut self, hart: &mut RiscvCore, value: u32) {
        let active = value & DMACTIVE != 0;
        if !active {
            if self.dmactive {
                self.reset_module(hart);
            }
            return;
        }
        self.dmactive = true;
        // Writes of haltreq, resumereq, and ackhavereset apply to the new
        // hartsel (§3.14).
        self.hartsel = hartsel_of(value) & 0x000f_ffff;
        let haltreq = value & HALTREQ != 0;
        if self.hartsel == 0 {
            self.set_haltreq(hart, haltreq);
        }
        if value & ACKHAVERESET != 0 && self.hartsel == 0 {
            self.havereset = false;
        }
        // resumereq is ignored when haltreq is set after this same write.
        // The call below sees the previous ndmreset. The new ndmreset bit
        // is stored after that judgment (Debug Spec 1.0 §3.14). A resume
        // cannot preempt an unfinished command: the command's completion or
        // error must be visible first.
        if value & RESUMEREQ != 0 && !haltreq && !self.busy() {
            self.resume_selected(hart);
        }
        let ndmreset = value & NDMRESET != 0;
        if ndmreset && !self.ndmreset && self.exec.take().is_some() && self.cmderr == 0 {
            // A non-DM reset aborts the active command. The buffer, data and
            // autoexec bits survive.
            self.cmderr = abstract_cmd::CMDERR_HALT_RESUME;
        }
        self.ndmreset = ndmreset;
    }

    fn reset_module(&mut self, hart: &mut RiscvCore) {
        // `version` and `datacount` are implementation identity, not DM
        // state. `dmactive` 1-to-0 does not change them, and it does not
        // clear `havereset` (that stays the pre-reset behavior).
        self.dmactive = false;
        self.hartsel = 0;
        self.ndmreset = false;
        self.cmderr = 0;
        self.data = [0; 4];
        self.progbuf = [0; PROGBUF_WORDS];
        self.abstractauto = 0;
        self.last_command = None;
        self.sba = SbaState::default();
        self.exec = None;
        self.resumeack = true;
        self.set_haltreq(hart, false);
    }

    /// `data0` is DMI `0x04`. A register at or above `datacount` is absent.
    fn data_slot(&self, addr: u8) -> Option<usize> {
        let index = usize::from(addr - DATA0);
        if index < usize::from(self.datacount) && index < self.data.len() {
            Some(index)
        } else {
            None
        }
    }

    fn resume_selected(&mut self, hart: &mut RiscvCore) {
        // Debug Spec 1.0 §3.5 and dm_registers.xml `dmcontrol.resumereq`:
        // writing 1 clears the resume ack of each selected hart. hartsel
        // other than 0 does not touch hart 0. The ack is set again only
        // after an actual resume completes, before dmi_write returns.
        // A hart under the previous ndmreset is unavailable, not halted
        // (§3.2, §3.4), so that bit blocks the resume after the clear.
        if self.hartsel != 0 {
            return;
        }
        self.resumeack = false;
        if self.ndmreset {
            return;
        }
        if hart.in_debug_mode() {
            hart.debug_resume();
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

    fn read_dmstatus(&self, hart: &RiscvCore) -> u32 {
        // version is 3, authenticated is 1, impebreak is 1. authbusy,
        // hasresethaltreq, confstrptrvalid, and stickyunavail stay 0. The U5
        // mutation reports 4.
        let mut value = u32::from(self.version) | (1 << 7) | IMPEBREAK;
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
            } else if self.hart_halted(hart) {
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
        // progbufsize 2, relaxedpriv 0. datacount is 4 unless the U5
        // mutation reports 2.
        ((PROGBUF_WORDS as u32) << 24)
            | (u32::from(self.busy()) << 12)
            | (u32::from(self.cmderr) << 8)
            | u32::from(self.datacount)
    }

    fn read_haltsum0(&self, hart: &RiscvCore) -> u32 {
        // The LSB is the halt status of hart {hartsel[19:5], 5'b0} (§3.14).
        // Only hart 0 exists, and an unavailable hart is not halted.
        if self.hartsel < 32 && self.hart_halted(hart) {
            1
        } else {
            0
        }
    }

    fn hart_halted(&self, hart: &RiscvCore) -> bool {
        hart.in_debug_mode() && !self.ndmreset
    }

    pub(crate) fn set_haltreq(&mut self, hart: &mut RiscvCore, haltreq: bool) {
        self.haltreq = haltreq;
        self.apply_haltreq(hart);
    }

    pub(crate) fn apply_haltreq(&self, hart: &mut RiscvCore) {
        hart.set_haltreq(self.haltreq);
    }

    pub(crate) fn terminal_write_allowed(&self, addr: u8, value: u32) -> bool {
        match addr & 0x7f {
            COMMAND => false,
            DMCONTROL => {
                value & DMACTIVE == 0 || (!self.ndmreset && value & (NDMRESET | RESUMEREQ) == 0)
            }
            _ => true,
        }
    }

    pub(crate) fn ndmreset(&self) -> bool {
        self.ndmreset
    }

    /// Debug Spec 1.0 §3.2. The selected hart has come out of reset.
    /// `havereset` sticks until `ackhavereset`. Called by the Machine when
    /// `ndmreset` falls after the reload, not by [`Self::dmi_write`].
    pub(crate) fn note_reset_release(&mut self) {
        self.havereset = true;
    }
}

impl Default for DebugModule {
    fn default() -> Self {
        Self::new()
    }
}

const U5_VERSION_ENV: &str = "RUSCV_S6_DMSTATUS_VERSION";
const U5_DATACOUNT_ENV: &str = "RUSCV_S6_DATACOUNT";

fn u5_discovery_from_env() -> (u8, u8) {
    let version = std::env::var(U5_VERSION_ENV).ok();
    let datacount = std::env::var(U5_DATACOUNT_ENV).ok();
    let chosen = u5_discovery(version.as_deref(), datacount.as_deref());
    if chosen.0 != 3 {
        eprintln!(
            "ruscv-sim: verification override {U5_VERSION_ENV}={} (dmstatus.version)",
            chosen.0
        );
    }
    if chosen.1 != 4 {
        eprintln!(
            "ruscv-sim: verification override {U5_DATACOUNT_ENV}={} (abstractcs.datacount)",
            chosen.1
        );
    }
    chosen
}

/// §7 item 5. `Some("4")` is the version OpenOCD 0.12.0 rejects. `Some("2")`
/// is the `datacount` that hides the upper half of a 64-bit address. Every
/// other string keeps the conforming discovery values.
pub(crate) fn u5_discovery(version: Option<&str>, datacount: Option<&str>) -> (u8, u8) {
    let version = match version {
        Some("4") => 4,
        Some(other) => {
            eprintln!("ruscv-sim: ignoring {U5_VERSION_ENV}={other}; the U5 mutation is 4");
            3
        }
        None => 3,
    };
    let datacount = match datacount {
        Some("2") => 2,
        Some(other) => {
            eprintln!("ruscv-sim: ignoring {U5_DATACOUNT_ENV}={other}; the U5 mutation is 2");
            4
        }
        None => 4,
    };
    (version, datacount)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RiscvCore;
    use crate::memory::{MemoryInterface, SimpleMemory};
    use std::sync::{Arc, Mutex};

    fn hart() -> RiscvCore {
        let mem: Arc<Mutex<dyn MemoryInterface + Send + Sync>> =
            Arc::new(Mutex::new(SimpleMemory::new(4096)));
        RiscvCore::new(Arc::clone(&mem), mem)
    }

    #[test]
    fn u5_discovery_keeps_version_3_and_datacount_4_unless_mutated() {
        assert_eq!(u5_discovery(None, None), (3, 4));
        assert_eq!(u5_discovery(Some("4"), Some("nope")), (4, 4));
        assert_eq!(u5_discovery(Some("3"), Some("2")), (3, 2));
        assert_eq!(u5_discovery(Some("2"), Some("1")), (3, 4));
    }

    #[test]
    fn version_4_and_datacount_2_change_the_registers_openocd_reads() {
        let mut dm = DebugModule::with_discovery(4, 2);
        let mut hart = hart();
        dm.dmi_write(&mut hart, DMCONTROL, DMACTIVE);
        assert_eq!(dm.dmi_read(&mut hart, DMSTATUS) & 0xf, 4);
        assert_eq!(dm.dmi_read(&mut hart, ABSTRACTCS) & 0xf, 2);
        dm.dmi_write(&mut hart, DATA0, 0x1111_1111);
        dm.dmi_write(&mut hart, DATA0 + 1, 0x2222_2222);
        dm.dmi_write(&mut hart, DATA0 + 2, 0x3333_3333);
        dm.dmi_write(&mut hart, DATA0 + 3, 0x4444_4444);
        assert_eq!(dm.dmi_read(&mut hart, DATA0), 0x1111_1111);
        assert_eq!(dm.dmi_read(&mut hart, DATA0 + 1), 0x2222_2222);
        assert_eq!(
            dm.dmi_read(&mut hart, DATA0 + 2),
            0,
            "data2 is outside datacount 2"
        );
        assert_eq!(
            dm.dmi_read(&mut hart, DATA0 + 3),
            0,
            "data3 is outside datacount 2"
        );
    }
}
