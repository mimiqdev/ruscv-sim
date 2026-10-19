//! Hart-owned Sdext state.
//!
//! Debug Specification 1.0, tag `5695c0a`, chapter Sdext and
//! `xml/core_registers.xml`. These registers are accessible only from Debug
//! Mode. This hart never executes there, so a guest CSR instruction cannot
//! name them. The fields live here, not in [`crate::csr::CsrFile`].

use super::PrivilegeMode;

/// `dcsr.cause` values this profile produces.
///
/// Debug 1.0 `dcsr.cause`. Trigger (2), resethaltreq (5), group (6), and
/// other (7) are not causes this profile enters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DebugCause {
    /// An `ebreak` entered Debug Mode.
    Ebreak = 1,
    /// `haltreq` was set at an instruction boundary.
    HaltReq = 3,
    /// `dcsr.step` retired one instruction or trap, then re-entered.
    Step = 4,
}

impl DebugCause {
    /// Architectural cause encoding.
    pub const fn code(self) -> u64 {
        self as u64
    }
}

/// Writable and read-only `dcsr` bits for one hart.
///
/// Contract D5. `debugver` is 4. `ebreakm`, `ebreaks`, and `ebreaku` reset
/// to 0. `stepie`, `ebreakvs`, `ebreakvu`, `v`, and `nmip` are fixed 0.
/// `stopcount`, `stoptime`, and `mprven` are fixed 1. `cause` is read-only.
/// `prv` stores 0, 1, or 3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HartDebug {
    pub(crate) halted: bool,
    pub(crate) haltreq: bool,
    pub(crate) ebreakm: bool,
    pub(crate) ebreaks: bool,
    pub(crate) ebreaku: bool,
    pub(crate) step: bool,
    pub(crate) cause: Option<DebugCause>,
    pub(crate) prv: PrivilegeMode,
    pub(crate) dpc: u64,
    pub(crate) dscratch0: u64,
    pub(crate) dscratch1: u64,
}

const DEBUGVER: u64 = 4 << 28;
const EBREAKVS: u64 = 1 << 17;
const EBREAKVU: u64 = 1 << 16;
const EBREAKM: u64 = 1 << 15;
const EBREAKS: u64 = 1 << 13;
const EBREAKU: u64 = 1 << 12;
const STEPIE: u64 = 1 << 11;
const STOPCOUNT: u64 = 1 << 10;
const STOPTIME: u64 = 1 << 9;
const V: u64 = 1 << 5;
const MPRVEN: u64 = 1 << 4;
const NMIP: u64 = 1 << 3;
const STEP: u64 = 1 << 2;
const CAUSE_SHIFT: u32 = 6;
const FIXED_ZERO: u64 = EBREAKVS | EBREAKVU | STEPIE | V | NMIP;

/// Privileged §3.1.14 `mepc` IALIGN=32. `dpc` drops bits 1:0.
pub(crate) fn legal_dpc(value: u64) -> u64 {
    value & !0b11
}

/// Debug 1.0 `dcsr.prv`. Encodings 0, 1, and 3 are stored. Encoding 2 keeps
/// the previous legal mode, the same rule as `mstatus.MPP`.
fn legal_prv(current: PrivilegeMode, written: u64) -> PrivilegeMode {
    match written & 0b11 {
        0 => PrivilegeMode::User,
        1 => PrivilegeMode::Supervisor,
        3 => PrivilegeMode::Machine,
        _ => current,
    }
}

impl Default for HartDebug {
    fn default() -> Self {
        Self {
            halted: false,
            haltreq: false,
            ebreakm: false,
            ebreaks: false,
            ebreaku: false,
            step: false,
            cause: None,
            prv: PrivilegeMode::Machine,
            dpc: 0,
            dscratch0: 0,
            dscratch1: 0,
        }
    }
}

impl HartDebug {
    /// True when the `ebreak` bit for `privilege` redirects into Debug Mode.
    ///
    /// Debug 1.0 `dcsr.ebreakm`, `ebreaks`, and `ebreaku`. Contract D8.
    pub(crate) fn ebreak_redirects(&self, privilege: PrivilegeMode) -> bool {
        match privilege {
            PrivilegeMode::Machine => self.ebreakm,
            PrivilegeMode::Supervisor => self.ebreaks,
            PrivilegeMode::User => self.ebreaku,
        }
    }

    /// `stepie` is fixed 0, so a stepped turn does not take an interrupt.
    /// Debug 1.0 `dcsr.stepie`.
    pub(crate) fn take_interrupt_this_turn(&self) -> bool {
        !self.step
    }

    /// Assemble `dcsr`. Fixed bits are forced on every read.
    pub(crate) fn read_dcsr(&self) -> u64 {
        let mut value = DEBUGVER | STOPCOUNT | STOPTIME | MPRVEN;
        if self.ebreakm {
            value |= EBREAKM;
        }
        if self.ebreaks {
            value |= EBREAKS;
        }
        if self.ebreaku {
            value |= EBREAKU;
        }
        value |= self.cause.map(DebugCause::code).unwrap_or(0) << CAUSE_SHIFT;
        if self.step {
            value |= STEP;
        }
        (value | (self.prv as u64)) & !FIXED_ZERO
    }

    /// Apply a debugger write. `cause` and every fixed bit stay put.
    ///
    /// A write of `prv` 2 stores the previous legal value. Debug 1.0 says an
    /// unsupported encoding may become any supported mode. This hart keeps
    /// the previous mode, the same rule `mstatus.MPP` uses.
    pub(crate) fn write_dcsr(&mut self, value: u64) {
        self.ebreakm = value & EBREAKM != 0;
        self.ebreaks = value & EBREAKS != 0;
        self.ebreaku = value & EBREAKU != 0;
        self.step = value & STEP != 0;
        self.prv = legal_prv(self.prv, value);
    }

    /// `dpc` uses the `mepc` IALIGN=32 rule. Privileged §3.1.14.
    pub(crate) fn write_dpc(&mut self, value: u64) -> u64 {
        self.dpc = legal_dpc(value);
        self.dpc
    }
}
