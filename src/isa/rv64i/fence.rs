//! RV64I FENCE (MISC-MEM) Operation
//!
//! This module implements the base-I `FENCE` instruction: MISC-MEM
//! (`opcode = 0b000_1111`) with `funct3 = 0b000`. `FENCE.I` (Zifencei,
//! `funct3 = 0b001`) and every other MISC-MEM `funct3` (e.g. `0b010` for
//! Zicbo cache-block operations) are separate extensions; the decoder
//! rejects them with `DecodeError::UnimplementedInstruction` before
//! execution ever reaches this module.
//!
//! ## Memory model and why FENCE is a no-op here
//!
//! `ruscv-sim` runs one Hart against a synchronous, in-order
//! [`MemoryInterface`]: there is no cache,
//! no store buffer, no speculative execution, and no second observer that
//! could see a memory effect out of program order. Every prior
//! instruction's memory effect is therefore already globally visible before
//! the next instruction issues -- which is exactly the ordering guarantee
//! `FENCE` exists to provide on machines that can reorder. On this machine
//! there is nothing left to order, so `FENCE` is correctly implemented as a
//! no-op that only lets the PC advance. This is a consequence of the
//! existing execution model, not a new concurrency or ordering model: no
//! barrier flags, TLM hooks or pretend multi-Hart machinery are introduced.
//!
//! Per the RISC-V unprivileged spec (base-I FENCE semantics):
//! - `rs1` and `rd` are reserved for future finer-grain fences and **shall
//!   be ignored** by base implementations: [`exec_fence`] never reads `rs1`
//!   and never writes `rd`, even when either field is nonzero.
//! - `fm = 0b1000` with `pred = succ = RW` is `FENCE.TSO`; every other `fm`
//!   value, including other `fm = 0b1000` predecessor/successor
//!   combinations, is reserved and must be treated as an ordinary
//!   `fm = 0b0000` fence.
//! - Encodings with `pred == 0` or `succ == 0` are HINTs and execute as
//!   no-ops.
//!
//! All of the above collapse to the same action on this synchronous
//! single-Hart machine: do nothing and let the normal `pc += 4` fall
//! through in [`RiscvCore::step`](crate::core::RiscvCore::step).

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

/// Execute `FENCE` (RV64I MISC-MEM, `funct3 = 0b000`).
///
/// Writes no register (including `rd`, per the reserved-field rule above),
/// performs no memory access, and never sets `state.branch_taken`, so the
/// caller's normal PC fall-through advances the PC by exactly 4. See the
/// module documentation for why this is the exact semantics required by the
/// spec on this simulator's synchronous single-Hart memory model, not an
/// approximation of it.
#[inline]
pub fn exec_fence(
    _instr: &DecodedInstruction,
    _state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::RiscvCore;
    use crate::decode::{InstructionDecoder, InstructionFormat, Opcode};
    use crate::memory::SimpleMemory;
    use std::sync::{Arc, Mutex};

    /// Distinct nonzero sentinel per register (x0 stays hardwired to zero)
    /// so a stray write to any single register, including `rd`, is caught.
    fn seeded_regs() -> [u64; 32] {
        let mut regs = [0u64; 32];
        for (i, r) in regs.iter_mut().enumerate().skip(1) {
            *r = 0x1111_1111_0000_0000 + i as u64;
        }
        regs
    }

    /// Decode `word` and run it directly through `exec_fence`, asserting
    /// every FENCE invariant: successful decode as MISC-MEM/I-type,
    /// successful execute, no GPR write (including a nonzero `rd`), no
    /// memory write, and `branch_taken` left false.
    fn assert_fence_decode_and_execute_is_noop(word: u32) {
        let decoded = InstructionDecoder::new()
            .decode(word)
            .unwrap_or_else(|e| panic!("expected FENCE 0x{word:08x} to decode, got {e:?}"));
        assert_eq!(decoded.opcode, Opcode::MiscMem);
        assert_eq!(decoded.format, InstructionFormat::IType);

        let mut state = CoreState {
            regs: seeded_regs(),
            ..Default::default()
        };
        let before_regs = state.regs;

        let mut mem = SimpleMemory::new(0x100);
        mem.write_word(0x40, 0xDEAD_BEEF).unwrap();

        exec_fence(&decoded, &mut state, &mut mem)
            .unwrap_or_else(|e| panic!("expected FENCE 0x{word:08x} to execute, got {e:?}"));

        assert_eq!(state.regs, before_regs, "FENCE must not write any GPR");
        assert!(!state.branch_taken);
        assert_eq!(mem.read_word(0x40).unwrap(), 0xDEAD_BEEF);
    }

    /// Run `word` through the full fetch-decode-execute `RiscvCore::step`
    /// pipeline and assert the PC advances by exactly 4, matching the
    /// no-op contract observed by every other public-path instruction that
    /// does not branch.
    fn assert_fence_steps_pc_by_4(word: u32) {
        let mem = Arc::new(Mutex::new(SimpleMemory::new(0x100)));
        mem.lock().unwrap().write_word(0, word).unwrap();

        let mut core = RiscvCore::new(mem.clone(), mem.clone());
        *core.state_mut() = CoreState {
            regs: seeded_regs(),
            ..Default::default()
        };
        let before_regs = core.state().regs;

        core.step()
            .unwrap_or_else(|e| panic!("expected FENCE 0x{word:08x} to step, got {e:?}"));

        assert_eq!(core.state().pc, 4);
        assert!(!core.state().branch_taken);
        assert_eq!(core.state().regs, before_regs);
        assert_eq!(mem.lock().unwrap().read_word(0).unwrap(), word);
    }

    // Encodings extracted from the pinned ACT4 `I-fence-00.S` source
    // (riscv-arch-test @ a7c99303516f4e668f7488f172043392e23b9dfd), covering
    // every FENCE shape that test emits: the plain/iorw fence, an explicit
    // rw,rw fence, FENCE.TSO, reserved-but-must-still-execute rs1/rd/fm
    // combinations, and pred=0/succ=0 hints.
    const FENCE: u32 = 0x0ff0000f; // fence (iorw, iorw)
    const FENCE_RW_RW: u32 = 0x0330000f; // fence rw, rw
    const FENCE_TSO: u32 = 0x8330000f; // fence.tso
    const FENCE_NONZERO_RS1: u32 = 0x0331000f; // rs1 must be ignored, not faulted on
    const FENCE_NONZERO_RD: u32 = 0x0330008f; // rd = x1: must be ignored, never written
    const FENCE_RESERVED_FM: u32 = 0x1330000f; // reserved fm -> treated as fm=0000
    const FENCE_TSO_FM_OTHER_PRED_SUCC: u32 = 0x8110000f; // fm=1000, pred/succ != RW -> normal fence
    const FENCE_HINT_1: u32 = 0x0031000f;
    const FENCE_HINT_2: u32 = 0x0301000f;
    const FENCE_HINT_3: u32 = 0x0030008f; // hint, rd = x1
    const FENCE_HINT_4: u32 = 0x0300008f; // hint, rd = x1
    const FENCE_HINT_5: u32 = 0x0020000f;
    const FENCE_HINT_6: u32 = 0x0200000f;

    const ALL_I_FENCE_00_ENCODINGS: [u32; 13] = [
        FENCE,
        FENCE_RW_RW,
        FENCE_TSO,
        FENCE_NONZERO_RS1,
        FENCE_NONZERO_RD,
        FENCE_RESERVED_FM,
        FENCE_TSO_FM_OTHER_PRED_SUCC,
        FENCE_HINT_1,
        FENCE_HINT_2,
        FENCE_HINT_3,
        FENCE_HINT_4,
        FENCE_HINT_5,
        FENCE_HINT_6,
    ];

    #[test]
    fn all_i_fence_00_encodings_decode_and_execute_as_noop() {
        for word in ALL_I_FENCE_00_ENCODINGS {
            assert_fence_decode_and_execute_is_noop(word);
        }
    }

    #[test]
    fn all_i_fence_00_encodings_advance_pc_by_4_through_step() {
        for word in ALL_I_FENCE_00_ENCODINGS {
            assert_fence_steps_pc_by_4(word);
        }
    }

    #[test]
    fn fence_i_is_not_base_i_and_stays_rejected() {
        // FENCE.I (Zifencei, funct3 = 0b001) must remain unimplemented: base-I
        // FENCE support must not silently widen to cover it.
        const FENCE_I: u32 = 0x0000_100f;
        let err = InstructionDecoder::new().decode(FENCE_I).unwrap_err();
        assert!(matches!(
            err,
            crate::decode::DecodeError::UnimplementedInstruction
        ));
    }
}
