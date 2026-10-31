//! System bus access: the DM-owned `sbcs`/`sbaddress`/`sbdata` block.
//!
//! Debug Spec 1.0 §3.14, system-bus-access registers, with the selected
//! profile of contract §2 D2/D5/D6: `sbversion` 1, `sbasize` 64,
//! `sbaddress0`/`sbaddress1`, `sbdata0`/`sbdata1`, `sbaccess` widths 8-64
//! and no 128-bit width. `sbaddress2`/`sbaddress3` and `sbdata2`/`sbdata3`
//! are absent and read 0.
//!
//! An access is a *request*, not a call. This module sets `sbbusy`, exposes
//! the request through [`DebugModule::next_sba_request`], and keeps `sbbusy`
//! set until [`DebugModule::complete_sba_access`] reports the target result.
//! Two reasons decide that split. A debugger can only observe `sbbusy` and
//! provoke `sbbusyerror` if the busy window outlives the DMI access that
//! started it. And contract §3 forbids holding a DM lock across a physical
//! target access, so the caller must reach the target between the two calls.
//!
//! The Platform owns that target (U2) and the Machine owns admission at a
//! coherent Hart boundary (U3). Until U3 lands, `Machine` admits DMI but
//! drives no SBA request, so a system bus access started through
//! `Machine::dmi_write` stays busy until a DM reset. Direct DM callers such as
//! the U1 tests complete every request they start.

use super::regs::{SBADDRESS0, SBADDRESS1, SBDATA0, SBDATA1};
use super::DebugModule;

/// `sbcs.sbversion` 1: the System Bus interface conforms to Debug Spec 1.0.
const SBVERSION: u32 = 1 << 29;
/// `sbcs.sbbusyerror`, R/W1C.
const SBBUSYERROR: u32 = 1 << 22;
/// `sbcs.sbbusy`, read-only.
const SBBUSY: u32 = 1 << 21;
/// `sbcs.sbreadonaddr`.
const SBREADONADDR: u32 = 1 << 20;
/// `sbcs.sbaccess`, three bits.
const SBACCESS_SHIFT: u32 = 17;
/// `sbcs.sbautoincrement`.
const SBAUTOINCREMENT: u32 = 1 << 16;
/// `sbcs.sbreadondata`.
const SBREADONDATA: u32 = 1 << 15;
/// `sbcs.sberror`, three bits.
const SBERROR_SHIFT: u32 = 12;
/// `sbcs.sbasize`: the width of the system bus address in bits (contract D2).
const SBASIZE: u32 = 64;
const SBASIZE_SHIFT: u32 = 5;
/// `sbcs.sbaccess8`, `sbaccess16`, `sbaccess32`, `sbaccess64`. Bit 3 is
/// `sbaccess128`, which this profile does not implement.
const SBACCESS_WIDTHS: u32 = 0b1111;

/// `sberror` 2: a bad address was accessed.
const SBERROR_BAD_ADDRESS: u8 = 2;
/// `sberror` 3: there was an alignment error.
const SBERROR_ALIGNMENT: u8 = 3;
/// `sberror` 4: an access of unsupported size was requested.
const SBERROR_SIZE: u8 = 4;

/// A supported system bus access width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbaAccessSize {
    /// `sbaccess` 0.
    Byte,
    /// `sbaccess` 1.
    Half,
    /// `sbaccess` 2. This is the `sbcs` reset value.
    Word,
    /// `sbaccess` 3.
    DoubleWord,
}

impl SbaAccessSize {
    /// The encoded `sbaccess` value, which is also log2 of the byte count.
    fn field(self) -> u8 {
        match self {
            Self::Byte => 0,
            Self::Half => 1,
            Self::Word => 2,
            Self::DoubleWord => 3,
        }
    }

    /// `None` for `sbaccess` 4 and above: 128-bit access does not exist here.
    fn from_field(field: u8) -> Option<Self> {
        match field {
            0 => Some(Self::Byte),
            1 => Some(Self::Half),
            2 => Some(Self::Word),
            3 => Some(Self::DoubleWord),
            _ => None,
        }
    }

    /// The number of bytes this width moves, which the target needs to carry
    /// out the request.
    pub fn bytes(self) -> u32 {
        1 << self.field()
    }

    /// The low bits a value of this width occupies.
    fn mask(self) -> u64 {
        u64::MAX >> (64 - self.bytes() * 8)
    }
}

/// One physical transaction the DM issues as the Platform's inbound master.
///
/// The address is physical: no Hart translation, privilege, PMP, or
/// architectural check applies and no ISA or trap path runs. `write` holds
/// only the bytes this `size` covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SbaRequest {
    /// The physical address, which is `sbaddress`.
    pub address: u64,
    /// `sbaccess`, already validated against the advertised widths.
    pub size: SbaAccessSize,
    /// `Some` for a bus write, `None` for a read.
    pub write: Option<u64>,
}

/// Why a target refused an [`SbaRequest`].
///
/// These are the target-side members of `sberror`. An unsupported
/// `sbaccess` width is decided here, not by a target, because a target never
/// sees the width selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SbaFault {
    /// `sberror` 2: the physical target does not route or admit this address.
    BadAddress,
}

/// A started access whose target result has not been recorded yet.
#[derive(Clone, Copy, Debug)]
struct Pending {
    request: SbaRequest,
    /// Set once [`DebugModule::next_sba_request`] returned it, so one
    /// request cannot be handed to a target twice.
    handed_out: bool,
}

/// `sbcs`, `sbaddress`, `sbdata`, `sbbusy` and the sticky error state.
pub(super) struct SbaState {
    /// `sbcs.sbreadonaddr`.
    readonaddr: bool,
    /// `sbcs.sbaccess`, always three bits.
    access: u8,
    /// `sbcs.sbautoincrement`.
    autoincrement: bool,
    /// `sbcs.sbreadondata`.
    readondata: bool,
    /// `sbcs.sberror`. Holds the encoded value, sticky until W1C.
    error: u8,
    /// `sbcs.sbbusyerror`. Sticky until W1C.
    busy_error: bool,
    /// `sbaddress`, 64-bit.
    address: u64,
    /// `sbdata`, 64-bit.
    data: u64,
    /// The access `sbbusy` reports. `None` is idle.
    pending: Option<Pending>,
}

impl Default for SbaState {
    /// The `sbcs` reset values: `sbaccess` 2, every other writable field 0,
    /// address and data 0, no busy access and no error.
    fn default() -> Self {
        Self {
            readonaddr: false,
            access: SbaAccessSize::Word.field(),
            autoincrement: false,
            readondata: false,
            error: 0,
            busy_error: false,
            address: 0,
            data: 0,
            pending: None,
        }
    }
}

impl SbaState {
    /// `sbcs`. The profile fields are constant: version 1, `sbasize` 64, and
    /// widths 8-64 with `sbaccess128` reading 0.
    pub(super) fn read_cs(&self) -> u32 {
        let mut value = SBVERSION | (SBASIZE << SBASIZE_SHIFT) | SBACCESS_WIDTHS;
        if self.busy_error {
            value |= SBBUSYERROR;
        }
        if self.pending.is_some() {
            value |= SBBUSY;
        }
        if self.readonaddr {
            value |= SBREADONADDR;
        }
        value |= u32::from(self.access) << SBACCESS_SHIFT;
        if self.autoincrement {
            value |= SBAUTOINCREMENT;
        }
        if self.readondata {
            value |= SBREADONDATA;
        }
        value |= u32::from(self.error) << SBERROR_SHIFT;
        value
    }

    /// Write `sbcs`.
    ///
    /// §3.14 leaves a write while `sbbusy` is high undefined and requires a
    /// debugger to wait for `sbbusy` 0, so this module drops the write
    /// instead of retargeting the access that is already in flight. That
    /// also keeps a running access from being invalidated by a later
    /// `sberror` W1C.
    pub(super) fn write_cs(&mut self, value: u32) {
        if self.pending.is_some() {
            return;
        }
        self.readonaddr = value & SBREADONADDR != 0;
        self.access = ((value >> SBACCESS_SHIFT) & 0b111) as u8;
        self.autoincrement = value & SBAUTOINCREMENT != 0;
        self.readondata = value & SBREADONDATA != 0;
        // `sberror` and `sbbusyerror` are R/W1C. Clearing is per bit.
        self.error &= !(((value >> SBERROR_SHIFT) & 0b111) as u8);
        if value & SBBUSYERROR != 0 {
            self.busy_error = false;
        }
    }

    /// `sbaddress0` and `sbaddress1`. A read has no side effect, including
    /// while `sbbusy` is high.
    pub(super) fn read_address(&self, addr: u8) -> u32 {
        match addr {
            SBADDRESS1 => (self.address >> 32) as u32,
            _ => self.address as u32,
        }
    }

    /// Write `sbaddress0` or `sbaddress1`. A busy manager sets
    /// `sbbusyerror` and drops the write.
    pub(super) fn write_address(&mut self, addr: u8, value: u32) {
        if self.busy_sets_error() {
            return;
        }
        let shift = if addr == SBADDRESS1 { 32 } else { 0 };
        let mask = u64::from(u32::MAX) << shift;
        self.address = (self.address & !mask) | (u64::from(value) << shift);
        // Only `sbaddress0` triggers (§3.14 sbaddress0 and sbaddress1), and
        // only while no error is pending.
        if addr == SBADDRESS0 && self.readonaddr && !self.access_blocked() {
            self.start(None);
        }
    }

    /// `sbdata0` and `sbdata1` reads. Only `sbdata0` has the
    /// `sbreadondata` side effect, and the returned value is the `sbdata`
    /// the read sampled before that side effect.
    pub(super) fn read_data(&mut self, addr: u8) -> u32 {
        let value = match addr {
            SBDATA1 => (self.data >> 32) as u32,
            _ => self.data as u32,
        };
        if !self.busy_sets_error() && addr == SBDATA0 && self.readondata && !self.access_blocked() {
            self.start(None);
        }
        value
    }

    /// Write `sbdata0` or `sbdata1`. A `sbdata0` write starts a bus write of
    /// the new `sbdata`; `sbdata1` only stages the high half, so a debugger
    /// writes `sbdata0` last as §3.14 requires.
    pub(super) fn write_data(&mut self, addr: u8, value: u32) {
        if self.busy_sets_error() {
            return;
        }
        let shift = if addr == SBDATA1 { 32 } else { 0 };
        let mask = u64::from(u32::MAX) << shift;
        self.data = (self.data & !mask) | (u64::from(value) << shift);
        if addr == SBDATA0 && !self.access_blocked() {
            let data = self.data;
            self.start(Some(data));
        }
    }

    /// A register access while the manager is busy sets `sbbusyerror` and
    /// does nothing else (§3.14 `sbaddress0`, `sbdata0`, `sbdata1`). Returns
    /// whether the access must be dropped.
    fn busy_sets_error(&mut self) -> bool {
        if self.pending.is_some() {
            self.busy_error = true;
            return true;
        }
        false
    }

    /// Whether a new bus access may start.
    ///
    /// §3.14 suppresses the *trigger* while `sberror` or `sbbusyerror` is
    /// set, not the register update: `sbaddress0` initializes only when both
    /// are 0, `sbdata0` starts a write only when none is set. The staged
    /// value still lands, as it does in the pinned Spike, so the address and
    /// data registers keep reading back what the debugger wrote.
    fn access_blocked(&self) -> bool {
        self.error != 0 || self.busy_error
    }

    /// Start one access at the current `sbaddress`.
    ///
    /// An unsupported `sbaccess` width and a misaligned address are decided
    /// before any target is reached: §3.14 says the access is not performed
    /// and `sberror` is set. The Hart rejects a misaligned data access before
    /// its physical port for the same reason, so the DM does it here.
    fn start(&mut self, write: Option<u64>) {
        let Some(size) = SbaAccessSize::from_field(self.access) else {
            self.error = SBERROR_SIZE;
            return;
        };
        if !self.address.is_multiple_of(u64::from(size.bytes())) {
            self.error = SBERROR_ALIGNMENT;
            return;
        }
        self.pending = Some(Pending {
            request: SbaRequest {
                address: self.address,
                size,
                write: write.map(|value| value & size.mask()),
            },
            handed_out: false,
        });
    }

    /// The request a target has not been given yet.
    pub(super) fn next_request(&mut self) -> Option<SbaRequest> {
        let pending = self.pending.as_mut()?;
        if pending.handed_out {
            return None;
        }
        pending.handed_out = true;
        Some(pending.request)
    }

    /// Record the target result and clear `sbbusy`.
    ///
    /// Only a request that [`Self::next_request`] handed out can complete: a
    /// caller that never reached a target cannot report a result it does not
    /// have. A successful read updates `sbdata`; `sbautoincrement` advances
    /// `sbaddress` by the access size, which also carries into
    /// `sbaddress1`. A failed access changes neither.
    pub(super) fn complete(&mut self, result: Result<u64, SbaFault>) {
        let Some(pending) = self.pending.filter(|pending| pending.handed_out) else {
            return;
        };
        self.pending = None;
        let request = pending.request;
        match result {
            Ok(value) => {
                if request.write.is_none() {
                    self.data = value & request.size.mask();
                }
                if self.autoincrement {
                    self.address = self.address.wrapping_add(u64::from(request.size.bytes()));
                }
            }
            Err(SbaFault::BadAddress) => self.error = SBERROR_BAD_ADDRESS,
        }
    }
}

impl DebugModule {
    /// The system bus access a target has not been given yet.
    ///
    /// The caller reaches the physical target and reports the result with
    /// [`Self::complete_sba_access`]. Until that call `sbcs.sbbusy` reads 1
    /// and any further `sbaddress`/`sbdata` access sets `sbbusyerror`. Each
    /// request is handed out once.
    pub fn next_sba_request(&mut self) -> Option<SbaRequest> {
        self.sba.next_request()
    }

    /// Record the target result for the request taken by
    /// [`Self::next_sba_request`] and clear `sbbusy`. A call with no request
    /// in flight, or with a request that was never taken, does nothing.
    pub fn complete_sba_access(&mut self, result: Result<u64, SbaFault>) {
        self.sba.complete(result);
    }
}
