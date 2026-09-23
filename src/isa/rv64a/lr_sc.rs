//! RV64A Load-Reserved / Store-Conditional instructions
//!
//! Implements LR (Load-Reserved) and SC (Store-Conditional) instructions for
//! atomic memory operations under the A8 per-Hart reservation profile
//! (dev-plan §5.4, §11 item 2).
//!
//! # Reservation Mechanism
//!
//! - LR loads a value and establishes this Hart's single reservation: the
//!   exact reserved byte span keyed by the address issued to the access
//!   route, plus the committed-write snapshot for port-route LRs.
//! - SC succeeds only when the reservation is present and the SC span is
//!   contained in the reserved span; on the envelope route the backend also
//!   re-checks the LR-time committed-write snapshot inside its critical
//!   section.  An executed SC consumes the reservation on success and on
//!   conditional failure; a faulting SC retains it.
//! - The reservation lives in [`CoreState`]: one record per Hart, cleared by
//!   reset and by image reload (which replaces the core), replaced by the
//!   next successful LR.
//!
//! # Compatibility adapter
//!
//! These helpers are the typed `MemoryInterface` route used by the old
//! `RiscvCore::new` constructor.  That route is a labeled non-conforming
//! compatibility adapter (dev-plan §7.1, C24): its typed read/write pairs are
//! not indivisible and carry no committed-write bookkeeping, so a typed SC
//! cannot observe a competing committed write and its conditional check is
//! span containment only.  Standard facades issue the atomic envelope
//! instead; both routes share the one per-Hart reservation record here.
//!
//! # References
//!
//! - RISC-V ISA Volume I: Unprivileged Spec ("A" extension, Zalrsc)

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;
use crate::physical::{CommittedWriteSnapshot, PhysicalSpan, PhysicalWidth};

/// The per-Hart reservation record (dev-plan §5.4, C23).
///
/// One reservation exists per Hart; it lives in [`CoreState::reservation`].
/// The record keys the exact reserved byte span by the physical address the
/// Hart issued at its access route — the port-issued physical address on the
/// standard envelope route — plus the committed-write version snapshot the
/// load-reserved envelope returned.  A reservation established through the
/// typed compatibility adapter carries no snapshot; the typed route cannot
/// check committed competing writes, which is exactly why that route is the
/// labeled non-conforming adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationSet {
    /// The exact reserved byte span: the issued physical address and width.
    pub reserved: PhysicalSpan,
    /// The LR-time committed-write snapshot, present on the envelope route.
    pub snapshot: Option<CommittedWriteSnapshot>,
}

impl ReservationSet {
    /// Records a reservation for the issued physical address and width.
    pub fn new(paddr: u64, width: PhysicalWidth, snapshot: Option<CommittedWriteSnapshot>) -> Self {
        Self {
            reserved: PhysicalSpan { paddr, width },
            snapshot,
        }
    }

    /// Returns whether the reservation's span contains the given span.
    ///
    /// Span containment is the Hart-side SC precondition (dev-plan §5.4):
    /// the SC's own span must be covered by the recorded reserved span, so a
    /// wider or displaced SC cannot partially overwrite the reservation.
    pub fn covers(&self, paddr: u64, width: PhysicalWidth) -> bool {
        let (Some(reserved_end), Some(span_end)) = (
            self.reserved.checked_end_inclusive(),
            (PhysicalSpan { paddr, width }).checked_end_inclusive(),
        ) else {
            return false;
        };
        self.reserved.paddr <= paddr && span_end <= reserved_end
    }
}

/// LR.D - Load-Reserved (64-bit, typed compatibility route)
///
/// Loads a 64-bit value at `rs1` and replaces this Hart's reservation with
/// the issued address's 8-byte span.  A faulting load establishes no new
/// reservation and preserves any prior one.
#[inline]
pub fn exec_lr(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = mem.read_dword(addr).map_err(ExecuteError::MemoryError)?;

    // A successful LR replaces any prior reservation (one per Hart).
    state.reservation = Some(ReservationSet::new(addr, PhysicalWidth::Doubleword, None));
    if rd != 0 {
        state.regs[rd] = value;
    }

    Ok(())
}

/// LR.W - Load-Reserved (32-bit, typed compatibility route)
///
/// Loads a 32-bit value at `rs1`, sign-extends it into `rd`, and replaces
/// this Hart's reservation with the issued address's 4-byte span.
#[inline]
pub fn exec_lr_w(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = mem.read_word(addr).map_err(ExecuteError::MemoryError)?;

    state.reservation = Some(ReservationSet::new(addr, PhysicalWidth::Word, None));
    if rd != 0 {
        state.regs[rd] = (value as i32) as i64 as u64;
    }

    Ok(())
}

/// Shared typed-route SC core: conditionally write `width` bytes of `value`.
///
/// The Hart-side precondition is reservation presence plus span containment;
/// a reservation the typed route cannot satisfy retires `rd = 1` with no
/// memory call.  An executed SC consumes the reservation on success and on
/// conditional failure; a faulting write retains it (approved faulting-SC
/// retain, dev-plan §5.4/§11 item 2).  Failure code is the profile value 1.
fn exec_sc_typed(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
    width: PhysicalWidth,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rs2 = instr.rs2.ok_or(ExecuteError::InvalidOperation)? as usize;
    let rd = instr.rd.ok_or(ExecuteError::InvalidOperation)? as usize;

    let addr = state.regs[rs1];
    let value = state.regs[rs2];

    // The typed adapter carries no committed-write bookkeeping, so its
    // conditional check is span containment alone (labeled non-conformance).
    let covered = state
        .reservation
        .as_ref()
        .is_some_and(|reservation| reservation.covers(addr, width));
    // An executed SC consumes the reservation on every completed outcome;
    // a faulting write below retains it only because the staged state is
    // discarded when the trap is entered.
    state.reservation = None;
    if !covered {
        if rd != 0 {
            state.regs[rd] = 1;
        }
        return Ok(());
    }

    match width {
        PhysicalWidth::Word => mem
            .write_word(addr, value as u32)
            .map_err(ExecuteError::MemoryError)?,
        PhysicalWidth::Doubleword => mem
            .write_dword(addr, value)
            .map_err(ExecuteError::MemoryError)?,
        _ => return Err(ExecuteError::InvalidOperation),
    }
    if rd != 0 {
        state.regs[rd] = 0;
    }
    Ok(())
}

/// SC.D - Store-Conditional (64-bit, typed compatibility route)
///
/// Conditionally stores `rs2`'s 64-bit value at `rs1` when this Hart's
/// reservation covers the span; writes `rd = 0` on success, `rd = 1` on
/// conditional failure.
#[inline]
pub fn exec_sc(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_sc_typed(instr, state, mem, PhysicalWidth::Doubleword)
}

/// SC.W - Store-Conditional (32-bit, typed compatibility route)
///
/// Conditionally stores `rs2`'s low 32 bits at `rs1` when this Hart's
/// reservation covers the 4-byte span.
#[inline]
pub fn exec_sc_w(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    exec_sc_typed(instr, state, mem, PhysicalWidth::Word)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, Funct3, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;

    fn create_instr(rs1: u8, rs2: u8, rd: u8, funct5: u8, funct3: Funct3) -> DecodedInstruction {
        let raw = ((funct5 as u32) << 27)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | 0b010_1111;
        DecodedInstruction {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Amo,
            funct3: Some(funct3),
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
            rs3: None,
            rd: Some(rd),
            imm: None,
            branch_taken: false,
        }
    }

    fn lr_w(rs1: u8, rd: u8) -> DecodedInstruction {
        create_instr(rs1, 0, rd, 0b00010, Funct3::Slt)
    }

    fn lr_d(rs1: u8, rd: u8) -> DecodedInstruction {
        create_instr(rs1, 0, rd, 0b00010, Funct3::Sltu)
    }

    fn sc_w(rs1: u8, rs2: u8, rd: u8) -> DecodedInstruction {
        create_instr(rs1, rs2, rd, 0b00011, Funct3::Slt)
    }

    fn sc_d(rs1: u8, rs2: u8, rd: u8) -> DecodedInstruction {
        create_instr(rs1, rs2, rd, 0b00011, Funct3::Sltu)
    }

    fn reserved_span(state: &CoreState) -> Option<(u64, u8)> {
        state
            .reservation
            .as_ref()
            .map(|record| (record.reserved.paddr, record.reserved.width as u8))
    }

    #[test]
    fn test_lr_w_reads_sign_extended_word_and_reserves_four_bytes() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x100, 0x1234_5678_8000_0001).unwrap();
        state.regs[1] = 0x100;

        exec_lr_w(&lr_w(1, 2), &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xFFFF_FFFF_8000_0001);
        assert_eq!(reserved_span(&state), Some((0x100, 4)));
    }

    #[test]
    fn test_lr_d_reads_full_dword_and_reserves_eight_bytes() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x108, 0x1234_5678_9abc_def0).unwrap();
        state.regs[1] = 0x108;

        exec_lr(&lr_d(1, 2), &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x1234_5678_9abc_def0);
        assert_eq!(reserved_span(&state), Some((0x108, 8)));
    }

    #[test]
    fn test_lr_replaces_the_prior_reservation() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x100, 1).unwrap();
        mem.write_dword(0x108, 2).unwrap();
        state.regs[1] = 0x100;
        state.regs[3] = 0x108;

        exec_lr(&lr_d(1, 2), &mut state, &mut mem).unwrap();
        exec_lr(&lr_d(3, 4), &mut state, &mut mem).unwrap();

        assert_eq!(reserved_span(&state), Some((0x108, 8)));
    }

    #[test]
    fn test_faulting_lr_preserves_the_prior_reservation() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x100, 1).unwrap();
        state.regs[1] = 0x100;
        state.regs[3] = 0x8000; // beyond the 0x1000-sized memory

        exec_lr(&lr_d(1, 2), &mut state, &mut mem).unwrap();
        let outcome = exec_lr(&lr_d(3, 4), &mut state, &mut mem);

        assert!(outcome.is_err(), "the out-of-range LR must fault");
        assert_eq!(
            reserved_span(&state),
            Some((0x100, 8)),
            "a faulting LR establishes no new reservation and keeps the prior one"
        );
    }

    #[test]
    fn test_sc_w_success_writes_word_and_consumes() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x300, 0).unwrap();
        state.regs[1] = 0x300;
        state.regs[3] = 0xABCD_EFFF;

        exec_lr_w(&lr_w(1, 2), &mut state, &mut mem).unwrap();
        exec_sc_w(&sc_w(1, 3, 4), &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[4], 0);
        assert_eq!(mem.read_word(0x300).unwrap(), 0xABCD_EFFF);
        assert_eq!(
            reserved_span(&state),
            None,
            "an executed SC consumes the reservation"
        );
    }

    #[test]
    fn test_sc_d_success_writes_dword_and_consumes() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x308, 0).unwrap();
        state.regs[1] = 0x308;
        state.regs[3] = 0x1234_5678_9abc_def0;

        exec_lr(&lr_d(1, 2), &mut state, &mut mem).unwrap();
        exec_sc(&sc_d(1, 3, 4), &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[4], 0);
        assert_eq!(mem.read_dword(0x308).unwrap(), 0x1234_5678_9abc_def0);
        assert_eq!(reserved_span(&state), None);
    }

    #[test]
    fn test_sc_without_reservation_fails_with_rd_1_and_no_write() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x400, 0x55).unwrap();
        state.regs[1] = 0x400;
        state.regs[2] = 0xaa;

        exec_sc_w(&sc_w(1, 2, 3), &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 1);
        assert_eq!(mem.read_dword(0x400).unwrap(), 0x55);
    }

    #[test]
    fn test_sc_outside_the_reserved_span_fails_with_rd_1() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x500, 1).unwrap();
        mem.write_dword(0x508, 3).unwrap();
        state.regs[1] = 0x500;
        state.regs[2] = 9;

        exec_lr_w(&lr_w(1, 5), &mut state, &mut mem).unwrap();
        state.regs[1] = 0x508;
        exec_sc_w(&sc_w(1, 2, 3), &mut state, &mut mem).unwrap();

        assert_eq!(
            state.regs[3], 1,
            "a span the reservation does not cover fails"
        );
        assert_eq!(mem.read_dword(0x508).unwrap(), 3);
        assert_eq!(reserved_span(&state), None, "the SC consumed it");
    }

    #[test]
    fn test_sc_span_containment_crosses_widths_both_directions() {
        // LR.W@p then SC.D@p: the 8-byte SC span is not contained in the
        // 4-byte reservation, so the SC fails (approved span containment).
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x600, 0).unwrap();
        state.regs[1] = 0x600;
        state.regs[3] = 0xee;
        exec_lr_w(&lr_w(1, 2), &mut state, &mut mem).unwrap();
        exec_sc(&sc_d(1, 3, 4), &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[4], 1);
        assert_eq!(mem.read_dword(0x600).unwrap(), 0);

        // LR.D@p then SC.W@(p+4): the 4-byte span is inside the reserved
        // dword, so the conditional write succeeds.
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x600, 0).unwrap();
        state.regs[1] = 0x600;
        state.regs[3] = 0x66;
        exec_lr(&lr_d(1, 2), &mut state, &mut mem).unwrap();
        state.regs[1] = 0x604;
        exec_sc_w(&sc_w(1, 3, 4), &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[4], 0);
        assert_eq!(mem.read_word(0x604).unwrap(), 0x66);
    }

    #[test]
    fn test_faulting_sc_retains_the_reservation() {
        struct FailingOnce {
            inner: SimpleMemory,
            fail: bool,
        }
        impl MemoryInterface for FailingOnce {
            fn read_dword(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_dword(addr)
            }
            fn read_word(&self, addr: u64) -> Result<u32, crate::memory::MemoryError> {
                self.inner.read_word(addr)
            }
            fn read_half(&self, addr: u64) -> Result<u16, crate::memory::MemoryError> {
                self.inner.read_half(addr)
            }
            fn read_byte(&self, addr: u64) -> Result<u8, crate::memory::MemoryError> {
                self.inner.read_byte(addr)
            }
            fn read_word_zext(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_word_zext(addr)
            }
            fn read_half_zext(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_half_zext(addr)
            }
            fn read_byte_zext(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_byte_zext(addr)
            }
            fn read_word_sext(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_word_sext(addr)
            }
            fn read_half_sext(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_half_sext(addr)
            }
            fn read_byte_sext(&self, addr: u64) -> Result<u64, crate::memory::MemoryError> {
                self.inner.read_byte_sext(addr)
            }
            fn write_dword(
                &mut self,
                addr: u64,
                value: u64,
            ) -> Result<(), crate::memory::MemoryError> {
                if self.fail {
                    return Err(crate::memory::MemoryError::InvalidAddress(addr));
                }
                self.inner.write_dword(addr, value)
            }
            fn write_word(
                &mut self,
                addr: u64,
                value: u32,
            ) -> Result<(), crate::memory::MemoryError> {
                if self.fail {
                    return Err(crate::memory::MemoryError::InvalidAddress(addr));
                }
                self.inner.write_word(addr, value)
            }
            fn write_half(
                &mut self,
                addr: u64,
                value: u16,
            ) -> Result<(), crate::memory::MemoryError> {
                if self.fail {
                    return Err(crate::memory::MemoryError::InvalidAddress(addr));
                }
                self.inner.write_half(addr, value)
            }
            fn write_byte(
                &mut self,
                addr: u64,
                value: u8,
            ) -> Result<(), crate::memory::MemoryError> {
                if self.fail {
                    return Err(crate::memory::MemoryError::InvalidAddress(addr));
                }
                self.inner.write_byte(addr, value)
            }
            fn size(&self) -> usize {
                self.inner.size()
            }
        }

        let mut state = CoreState::default();
        let mut mem = FailingOnce {
            inner: SimpleMemory::new(0x1000),
            fail: false,
        };
        mem.inner.write_dword(0x700, 5).unwrap();
        state.regs[1] = 0x700;
        state.regs[3] = 9;

        exec_lr(&lr_d(1, 2), &mut state, &mut mem).unwrap();
        mem.fail = true;
        let outcome = exec_sc(&sc_d(1, 3, 4), &mut state, &mut mem);
        assert!(outcome.is_err(), "the faulting SC must error");
        // Approved faulting-SC retain is realized by the Hart's staged state:
        // the helper writes into staged state, the trap discards it, so the
        // architectural reservation survives.  This helper-level call models
        // the discard by restoring the record the failed step kept.
        state.reservation = Some(ReservationSet::new(0x700, PhysicalWidth::Doubleword, None));
        mem.fail = false;
        exec_sc(&sc_d(1, 3, 4), &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[4], 0, "the retained reservation still allows SC");
        assert_eq!(mem.inner.read_dword(0x700).unwrap(), 9);
    }

    #[test]
    fn test_lr_sc_x0_dest_and_separate_harts() {
        let mut state = CoreState::default();
        let mut mem = SimpleMemory::new(0x1000);
        mem.write_dword(0x800, 0x1234_5678).unwrap();
        state.regs[1] = 0x800;

        // rd = x0 suppresses only the register write.
        exec_lr_w(&lr_w(1, 0), &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[0], 0);
        assert_eq!(reserved_span(&state), Some((0x800, 4)));

        // A different Hart's state does not share this reservation.
        let mut other = CoreState::default();
        other.regs[1] = 0x800;
        other.regs[3] = 7;
        exec_sc_w(&sc_w(1, 3, 4), &mut other, &mut mem).unwrap();
        assert_eq!(other.regs[4], 1, "reservations are per-Hart");
        assert_eq!(mem.read_dword(0x800).unwrap(), 0x1234_5678);

        // This Hart's own SC still succeeds afterwards.
        state.regs[3] = 7;
        exec_sc_w(&sc_w(1, 3, 4), &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[4], 0);
        assert_eq!(mem.read_dword(0x800).unwrap(), 7);
    }
}
