use super::PrivilegeMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DebugCause {
    Ebreak = 1,
    HaltReq = 3,
    Step = 4,
}

impl DebugCause {
    pub const fn code(self) -> u64 {
        self as u64
    }
}

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

pub(crate) fn legal_dpc(value: u64) -> u64 {
    value & !0b11
}

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
    pub(crate) fn ebreak_redirects(&self, privilege: PrivilegeMode) -> bool {
        match privilege {
            PrivilegeMode::Machine => self.ebreakm,
            PrivilegeMode::Supervisor => self.ebreaks,
            PrivilegeMode::User => self.ebreaku,
        }
    }

    pub(crate) fn take_interrupt_this_turn(&self) -> bool {
        !self.step
    }

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

    pub(crate) fn write_dcsr(&mut self, value: u64) {
        self.ebreakm = value & EBREAKM != 0;
        self.ebreaks = value & EBREAKS != 0;
        self.ebreaku = value & EBREAKU != 0;
        self.step = value & STEP != 0;
        self.prv = legal_prv(self.prv, value);
    }

    pub(crate) fn write_dpc(&mut self, value: u64) -> u64 {
        self.dpc = legal_dpc(value);
        self.dpc
    }
}
