//! JTAG Debug Transport Module and the OpenOCD `remote_bitbang` codec.
//!
//! Debug Specification 1.0, tag `5695c0a`, the JTAG DTM chapter. The TAP moves
//! bits. Register values come from the Debug Module through [`DmiBus`], the
//! same DMI registers U2 implements and U3 admits at a Hart boundary.
//!
//! The character encoding is OpenOCD `remote_bitbang.c` (0.12.0):
//! `'0'`–`'7'` set TDI, TMS, and TCK, `'R'` reads TDO, `'Q'` quits, `'B'`/`'b'`
//! blink, and `'r'`–`'u'` carry TRST and SRST. Pin timing matches Spike's
//! `jtag_dtm_t::set_pins`: TMS and TDI are sampled on the rising edge from the
//! values stored by the previous call, and TDO updates on the other edge.
//! OpenOCD's bitbang driver writes TCK low, samples, then writes TCK high, so
//! the sample observes the bit that edge just presented.

mod session;

pub(crate) use session::BitbangSession;

/// TCP listen port and the `--halted` request for one run.
///
/// `bound_port` is stored with [`std::sync::atomic::Ordering::Release`] once
/// the socket is listening and before the hart fetches.
#[derive(Clone)]
pub struct RemoteBitbangConfig {
    pub port: u16,
    pub halted: bool,
    pub bound_port: Option<std::sync::Arc<std::sync::atomic::AtomicU16>>,
}

impl RemoteBitbangConfig {
    /// Listen on `port`. Port 0 asks the kernel for an ephemeral port.
    pub fn listen(port: u16) -> Self {
        Self {
            port,
            halted: false,
            bound_port: None,
        }
    }

    /// Stop the hart before the first instruction after reset.
    pub fn halted(mut self, halted: bool) -> Self {
        self.halted = halted;
        self
    }
}

/// Why a DMI operation did not return data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DmiFault {
    /// The subordinate rejected the access. `dtmcs.dmistat` becomes 2.
    Failed,
    /// The hart owner cannot apply the access. The run stops.
    Fatal(String),
}

/// DMI subordinate behind the TAP. Addresses are the low 7 bits (`abits` is 7).
pub trait DmiBus {
    fn dmi_read(&mut self, addr: u8) -> Result<u32, DmiFault>;
    fn dmi_write(&mut self, addr: u8, value: u32) -> Result<(), DmiFault>;
}

/// One `remote_bitbang` command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BitbangReply {
    /// Response byte for `'R'`.
    pub tdo: Option<u8>,
    /// The client sent `'Q'`.
    pub quit: bool,
}

/// TAP controller state. The discriminant order is the IEEE 1149.1 encoding
/// Spike indexes in `jtag_dtm_t::set_pins`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TapState {
    TestLogicReset = 0,
    RunTestIdle = 1,
    SelectDrScan = 2,
    CaptureDr = 3,
    ShiftDr = 4,
    Exit1Dr = 5,
    PauseDr = 6,
    Exit2Dr = 7,
    UpdateDr = 8,
    SelectIrScan = 9,
    CaptureIr = 10,
    ShiftIr = 11,
    Exit1Ir = 12,
    PauseIr = 13,
    Exit2Ir = 14,
    UpdateIr = 15,
}

const NEXT: [[TapState; 2]; 16] = {
    use TapState::*;
    [
        [RunTestIdle, TestLogicReset],
        [RunTestIdle, SelectDrScan],
        [CaptureDr, SelectIrScan],
        [ShiftDr, Exit1Dr],
        [ShiftDr, Exit1Dr],
        [PauseDr, UpdateDr],
        [PauseDr, Exit2Dr],
        [ShiftDr, UpdateDr],
        [RunTestIdle, SelectDrScan],
        [CaptureIr, TestLogicReset],
        [ShiftIr, Exit1Ir],
        [ShiftIr, Exit1Ir],
        [PauseIr, UpdateIr],
        [PauseIr, Exit2Ir],
        [ShiftIr, UpdateIr],
        [RunTestIdle, SelectDrScan],
    ]
};

const IR_IDCODE: u32 = 0x01;
const IR_DTMCS: u32 = 0x10;
const IR_DMI: u32 = 0x11;
const IR_LENGTH: u32 = 5;
const ABITS: u32 = 7;
const DMI_LENGTH: u32 = ABITS + 34;
/// Spike's placeholder. Bit 0 is 1, as JTAG requires. Not a JEDEC code.
const IDCODE: u64 = 0xdead_beef;
const DMIRESET: u64 = 1 << 16;
const DMIHARDRESET: u64 = 1 << 17;
const DMI_OP_SUCCESS: u8 = 0;
const DMI_OP_FAILED: u8 = 2;
const DMI_OP_READ: u8 = 1;
const DMI_OP_WRITE: u8 = 2;

/// 5-bit JTAG TAP plus the DTM registers selected by that instruction.
pub struct JtagDtm {
    state: TapState,
    /// Latched instruction. Reset selects IDCODE (`0x01`).
    ir: u32,
    ir_shift: u32,
    dr: u64,
    dr_length: u32,
    /// Previous DMI scan result: address, data, and `op`.
    dmi: u64,
    /// Sticky `dtmcs.dmistat`. `0` none, `2` failed. Cleared by `dmireset`.
    dmistat: u8,
    tck: bool,
    tms: bool,
    tdi: bool,
    tdo: bool,
}

impl Default for JtagDtm {
    fn default() -> Self {
        Self::new()
    }
}

impl JtagDtm {
    /// Test-Logic-Reset. The instruction register is IDCODE.
    pub fn new() -> Self {
        Self {
            state: TapState::TestLogicReset,
            ir: IR_IDCODE,
            ir_shift: 0x01,
            dr: IDCODE,
            dr_length: 32,
            dmi: 0,
            dmistat: 0,
            tck: false,
            tms: false,
            tdi: false,
            tdo: false,
        }
    }

    /// Latched instruction, low 5 bits.
    pub fn latched_instruction(&self) -> u8 {
        self.ir as u8
    }

    /// Controller state after the last rising edge.
    pub fn tap_state(&self) -> TapState {
        self.state
    }

    /// TRST or the Spike `'r'` reset. IR returns to IDCODE and the sticky DMI
    /// error clears. SRST does not come through here: Debug Spec 1.0 says
    /// system reset must not reset the TAP.
    pub fn reset(&mut self) {
        self.state = TapState::TestLogicReset;
        self.ir = IR_IDCODE;
        self.ir_shift = 0x01;
        self.dr = IDCODE;
        self.dr_length = 32;
        self.dmi = 0;
        self.dmistat = 0;
        self.tck = false;
        self.tdo = false;
    }

    /// Apply one OpenOCD `remote_bitbang` character.
    pub fn command(&mut self, byte: u8, bus: &mut dyn DmiBus) -> Result<BitbangReply, DmiFault> {
        match byte {
            b'Q' => Ok(BitbangReply {
                tdo: None,
                quit: true,
            }),
            b'R' => Ok(BitbangReply {
                tdo: Some(if self.tdo { b'1' } else { b'0' }),
                quit: false,
            }),
            b'0'..=b'7' => {
                let value = byte - b'0';
                self.set_pins(value & 4 != 0, value & 2 != 0, value & 1 != 0, bus)?;
                Ok(BitbangReply {
                    tdo: None,
                    quit: false,
                })
            }
            // OpenOCD packs `(trst << 1) | srst` onto `'r'`. `'t'` and `'u'`
            // assert TRST. `'r'` is Spike's TAP reset (it treats that byte as
            // reset even though the packed TRST bit is clear). `'s'` is SRST
            // only and does not move the TAP.
            b'r' | b't' | b'u' => {
                self.reset();
                Ok(BitbangReply {
                    tdo: None,
                    quit: false,
                })
            }
            _ => Ok(BitbangReply {
                tdo: None,
                quit: false,
            }),
        }
    }

    fn set_pins(
        &mut self,
        tck: bool,
        tms: bool,
        tdi: bool,
        bus: &mut dyn DmiBus,
    ) -> Result<(), DmiFault> {
        if !self.tck && tck {
            match self.state {
                TapState::ShiftDr => {
                    let length = self.dr_length;
                    self.dr >>= 1;
                    if self.tdi {
                        self.dr |= 1_u64 << (length - 1);
                    }
                }
                TapState::CaptureIr => self.ir_shift = 0x01,
                TapState::ShiftIr => {
                    self.ir_shift >>= 1;
                    if self.tdi {
                        self.ir_shift |= 1 << (IR_LENGTH - 1);
                    }
                }
                _ => {}
            }
            let index = self.state as usize;
            self.state = NEXT[index][usize::from(self.tms)];
        } else {
            match self.state {
                TapState::TestLogicReset => self.ir = IR_IDCODE,
                TapState::CaptureDr => self.capture_dr(),
                TapState::ShiftDr => self.tdo = self.dr & 1 != 0,
                TapState::UpdateDr => self.update_dr(bus)?,
                TapState::ShiftIr => self.tdo = self.ir_shift & 1 != 0,
                TapState::UpdateIr => self.ir = self.ir_shift & 0x1f,
                _ => {}
            }
        }
        self.tck = tck;
        self.tms = tms;
        self.tdi = tdi;
        Ok(())
    }

    fn capture_dr(&mut self) {
        match self.ir {
            IR_IDCODE => {
                self.dr = IDCODE;
                self.dr_length = 32;
            }
            IR_DTMCS => {
                // version 1, abits 7, idle 0. dmistat is the sticky alias of
                // the DMI `op` this transport records. `dmireset` reads as 0.
                self.dr = 1 | (u64::from(ABITS) << 4) | (u64::from(self.dmistat) << 10);
                self.dr_length = 32;
            }
            IR_DMI => {
                self.dr = if self.dmistat == 0 {
                    self.dmi
                } else {
                    u64::from(self.dmistat)
                };
                self.dr_length = DMI_LENGTH;
            }
            _ => {
                // IEEE 1149.1 loads the bypass register with 0.
                self.dr = 0;
                self.dr_length = 1;
            }
        }
    }

    fn update_dr(&mut self, bus: &mut dyn DmiBus) -> Result<(), DmiFault> {
        if self.ir == IR_DTMCS {
            if self.dr & DMIRESET != 0 {
                self.dmistat = 0;
                self.dmi &= !0b11;
            }
            if self.dr & DMIHARDRESET != 0 {
                // Clear the DTM's DMI state only. Moving the TAP here would
                // desynchronize a debugger that is still in Run-Test/Idle.
                self.dmistat = 0;
                self.dmi = 0;
            }
            return Ok(());
        }
        if self.ir != IR_DMI || self.dmistat != 0 {
            return Ok(());
        }
        let op = (self.dr & 0b11) as u8;
        let data = ((self.dr >> 2) & 0xffff_ffff) as u32;
        let addr = ((self.dr >> 34) & ((1 << ABITS) - 1)) as u8;
        match op {
            0 => Ok(()),
            DMI_OP_READ => match bus.dmi_read(addr) {
                Ok(value) => {
                    self.dmi = (u64::from(addr) << 34)
                        | (u64::from(value) << 2)
                        | u64::from(DMI_OP_SUCCESS);
                    Ok(())
                }
                Err(DmiFault::Failed) => {
                    self.fail_dmi();
                    Ok(())
                }
                Err(fatal) => Err(fatal),
            },
            DMI_OP_WRITE => match bus.dmi_write(addr, data) {
                Ok(()) => {
                    self.dmi = (u64::from(addr) << 34)
                        | (u64::from(data) << 2)
                        | u64::from(DMI_OP_SUCCESS);
                    Ok(())
                }
                Err(DmiFault::Failed) => {
                    self.fail_dmi();
                    Ok(())
                }
                Err(fatal) => Err(fatal),
            },
            _ => {
                // Debug Spec 1.0 leaves DMI `op` 3 (reserved) unspecified.
                // This DTM records a sticky failure so `dmireset` has a
                // client-visible error to clear. OpenOCD does not send `op` 3.
                self.fail_dmi();
                Ok(())
            }
        }
    }

    fn fail_dmi(&mut self) {
        self.dmistat = DMI_OP_FAILED;
        self.dmi = u64::from(DMI_OP_FAILED);
    }
}
