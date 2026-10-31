//! DMI register addresses and `dmcontrol` fields.
//!
//! Debug Specification 1.0 §3.14. The address is 7 bits, matching the
//! contract's `dtmcs.abits`. `dtmcs` itself is U4.

pub(super) const DATA0: u8 = 0x04;
pub(super) const DMCONTROL: u8 = 0x10;
pub(super) const DMSTATUS: u8 = 0x11;
pub(super) const HARTINFO: u8 = 0x12;
pub(super) const ABSTRACTCS: u8 = 0x16;
pub(super) const COMMAND: u8 = 0x17;
pub(super) const ABSTRACTAUTO: u8 = 0x18;
pub(super) const PROGBUF0: u8 = 0x20;
pub(super) const PROGBUF_LAST: u8 = 0x21;
pub(super) const SBCS: u8 = 0x38;
pub(super) const SBADDRESS0: u8 = 0x39;
pub(super) const SBADDRESS1: u8 = 0x3a;
pub(super) const SBDATA0: u8 = 0x3c;
pub(super) const SBDATA1: u8 = 0x3d;
pub(super) const HALTSUM0: u8 = 0x40;

pub(super) const HALTREQ: u32 = 1 << 31;
pub(super) const RESUMEREQ: u32 = 1 << 30;
pub(super) const ACKHAVERESET: u32 = 1 << 28;
pub(super) const NDMRESET: u32 = 1 << 1;
pub(super) const DMACTIVE: u32 = 1;

/// Two writable program-buffer words (`progbufsize`).
pub(super) const PROGBUF_WORDS: usize = 2;
/// `abstractauto` implemented bits: `autoexecdata[3:0]` and
/// `autoexecprogbuf[17:16]` (xml/dm_registers.xml `abstractauto`).
pub(super) const ABSTRACTAUTO_MASK: u32 = 0x0003_000f;
/// `dmstatus.impebreak`.
pub(super) const IMPEBREAK: u32 = 1 << 22;
/// Implicit EBREAK after the last program-buffer word.
pub(super) const EBREAK: u32 = 0x0010_0073;

pub(super) fn hartsel_of(value: u32) -> u32 {
    let lo = (value >> 16) & 0x3ff;
    let hi = (value >> 6) & 0x3ff;
    lo | (hi << 10)
}

pub(super) fn hartsel_field(hartsel: u32) -> u32 {
    let lo = (hartsel & 0x3ff) << 16;
    let hi = ((hartsel >> 10) & 0x3ff) << 6;
    lo | hi
}
