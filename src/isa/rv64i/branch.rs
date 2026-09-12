//! RV64I Branch Operations
//!
//! This module implements the branch instructions for RV64I:
//! - BEQ: Branch if Equal
//! - BNE: Branch if Not Equal
//! - BLT: Branch if Less Than (signed)
//! - BGE: Branch if Greater or Equal (signed)
//! - BLTU: Branch if Less Than Unsigned
//! - BGEU: Branch if Greater or Equal Unsigned

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;

/// Execute branch instructions (RV64I)
///
/// # Operations
/// - BEQ: Branch if Equal
/// - BNE: Branch if Not Equal
/// - BLT: Branch if Less Than (signed)
/// - BGE: Branch if Greater or Equal (signed)
/// - BLTU: Branch if Less Than Unsigned
/// - BGEU: Branch if Greater or Equal Unsigned
///
/// # Arguments
/// * `instr` - Decoded instruction
/// * `state` - Core state (PC, registers)
/// * `_mem` - Memory interface (unused for branches)
#[inline]
pub fn exec_branch(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
        (instr.rs1, instr.rs2, instr.imm, instr.funct3)
    else {
        return Err(ExecuteError::InvalidOperation);
    };

    let rs1_val = state.regs[rs1 as usize];
    let rs2_val = state.regs[rs2 as usize];

    // Extract raw funct3 value (3 bits) for branch instruction decoding
    let funct3_val = funct3 as u8;
    let take_branch = match funct3_val {
        0b000 => rs1_val == rs2_val,                   // BEQ
        0b001 => rs1_val != rs2_val,                   // BNE
        0b100 => (rs1_val as i64) < (rs2_val as i64),  // BLT (signed)
        0b101 => (rs1_val as i64) >= (rs2_val as i64), // BGE (signed)
        0b110 => rs1_val < rs2_val,                    // BLTU (unsigned)
        0b111 => rs1_val >= rs2_val,                   // BGEU (unsigned)
        _ => false,
    };

    if take_branch {
        // Sign-extend the branch offset and add to PC
        // B-type immediate: imm[12|10:5|4:1|11] is a 13-bit signed offset (bit 0 is always 0)
        // Decode ensures imm only contains 13 valid bits (bits 0-12), so we sign-extend from bit 12
        // Using i32 << 19 >> 19 to sign-extend the 13-bit value to 32 bits
        let imm_sext = ((imm as i32) << 19 >> 19) as i64 as u64;
        state.pc = state.pc.wrapping_add(imm_sext);
        // Mark that a branch was taken (used by step() to skip pc += 4)
        state.branch_taken = true;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{
        DecodedInstruction, Funct3, InstructionDecoder, InstructionFormat, Opcode,
    };
    use crate::memory::SimpleMemory;

    /// Encode a raw 32-bit B-type instruction word for a given branch offset.
    ///
    /// The B-type immediate is a 13-bit signed byte offset (bit 0 always 0)
    /// scattered as imm[12|10:5|4:1|11]. This lets the tests drive the real
    /// decode+execute path with a raw 13-bit immediate rather than an
    /// already-sign-extended value.
    fn encode_btype(funct3: u8, rs1: u8, rs2: u8, offset: i32) -> u32 {
        let imm = offset as u32;
        let imm12 = (imm >> 12) & 1;
        let imm11 = (imm >> 11) & 1;
        let imm10_5 = (imm >> 5) & 0x3F;
        let imm4_1 = (imm >> 1) & 0xF;
        (imm12 << 31)
            | (imm10_5 << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | (imm4_1 << 8)
            | (imm11 << 7)
            | 0b110_0011
    }

    /// Decode an encoded B-type word and execute it, returning the resulting
    /// PC and whether the branch was taken. Exercises the raw decoded
    /// B-immediate through `exec_branch`, which is where sign extension occurs.
    fn decode_and_run(funct3: u8, rs1_val: u64, rs2_val: u64, offset: i32, pc: u64) -> (u64, bool) {
        let word = encode_btype(funct3, 1, 2, offset);
        let instr = InstructionDecoder::new().decode(word).unwrap();
        assert_eq!(instr.opcode, Opcode::Branch);
        assert_eq!(instr.format, InstructionFormat::BType);
        let mut state = CoreState {
            pc,
            ..Default::default()
        };
        state.regs[1] = rs1_val;
        state.regs[2] = rs2_val;
        let mut mem = SimpleMemory::new(0x1000);
        exec_branch(&instr, &mut state, &mut mem).unwrap();
        (state.pc, state.branch_taken)
    }

    fn create_test_instr(funct3: Funct3, rs1: u8, rs2: u8, imm: u32) -> DecodedInstruction {
        DecodedInstruction {
            raw: 0,
            format: InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(funct3),
            funct7: None,
            rs1: Some(rs1),
            rs2: Some(rs2),
            rs3: None,
            rd: None,
            imm: Some(imm),
            branch_taken: false,
        }
    }

    #[test]
    fn test_beq_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::AddSub, 1, 2, 0x20); // BEQ uses funct3=0
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_beq_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr(Funct3::AddSub, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bne_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = create_test_instr(Funct3::Sll, 1, 2, 0x20); // BNE uses funct3=1
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_blt_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = (-5i64) as u64;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::Xor, 1, 2, 0x20); // BLT uses funct3=4
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bge_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = create_test_instr(Funct3::SrlSra, 1, 2, 0x20); // BGE uses funct3=5
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bge_equal() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::SrlSra, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bltu_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = create_test_instr(Funct3::Or, 1, 2, 0x20); // BLTU uses funct3=6
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bgeu_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = create_test_instr(Funct3::And, 1, 2, 0x20); // BGEU uses funct3=7
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bltu_not_taken_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE;
        state.regs[2] = 5;

        let instr = create_test_instr(Funct3::Or, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bgeu_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE;
        state.regs[2] = 5;

        let instr = create_test_instr(Funct3::And, 1, 2, 0x20);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    // Branch-displacement regressions: the B-type immediate is 13-bit signed
    // and must sign-extend from bit 12. Each offset below has bit 12 != bit 11,
    // so a 12-bit (bit-11) sign extension produces a distinctly wrong PC. These
    // failing cases pin the fix.
    const BEQ: u8 = 0b000;
    const BNE: u8 = 0b001;
    const BLT: u8 = 0b100;
    const BGE: u8 = 0b101;
    const BLTU: u8 = 0b110;
    const BGEU: u8 = 0b111;
    const PC: u64 = 0x8000_2000;

    #[test]
    fn test_bimm_positive_2048() {
        // raw 0x0800 (bit 11 set, bit 12 clear) => +2048, not -2048.
        let (pc, taken) = decode_and_run(BEQ, 7, 7, 2048, PC);
        assert!(taken);
        assert_eq!(pc, PC + 2048);
    }

    #[test]
    fn test_bimm_positive_4092() {
        // raw 0x0ffc => +4092, not -4.
        let (pc, taken) = decode_and_run(BNE, 1, 2, 4092, PC);
        assert!(taken);
        assert_eq!(pc, PC + 4092);
    }

    #[test]
    fn test_bimm_positive_max_4094() {
        // Spec maximum reachable positive B offset: raw 0x0ffe => +4094, not -2.
        let (pc, taken) = decode_and_run(BLT, (-1i64) as u64, 5, 4094, PC);
        assert!(taken);
        assert_eq!(pc, PC + 4094);
    }

    #[test]
    fn test_bimm_negative_min_4096() {
        // Spec minimum B offset: raw 0x1000 (bit 12 set, bit 11 clear) => -4096,
        // not 0.
        let (pc, taken) = decode_and_run(BGE, 5, 5, -4096, PC);
        assert!(taken);
        assert_eq!(pc, PC - 4096);
    }

    #[test]
    fn test_bimm_unsigned_predicates_displacement() {
        // Unsigned comparators must use the same corrected displacement.
        let (pc, taken) = decode_and_run(BLTU, 3, 9, 2048, PC);
        assert!(taken);
        assert_eq!(pc, PC + 2048);

        let (pc, taken) = decode_and_run(BGEU, 9, 3, -4096, PC);
        assert!(taken);
        assert_eq!(pc, PC - 4096);
    }

    #[test]
    fn test_bimm_not_taken_no_displacement() {
        // Not-taken branches never apply the (previously wrong) displacement.
        let (pc, taken) = decode_and_run(BEQ, 1, 2, 2048, PC);
        assert!(!taken);
        assert_eq!(pc, PC);

        let (pc, taken) = decode_and_run(BLT, 10, 5, -4096, PC);
        assert!(!taken);
        assert_eq!(pc, PC);
    }

    #[test]
    fn test_negative_offset() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        // Feed the raw 13-bit decoded B-immediate for -32 (0x1FE0), matching
        // what the decoder produces, rather than a pre-sign-extended u32.
        let instr = create_test_instr(Funct3::AddSub, 1, 2, 0x1FE0);
        let mut mem = SimpleMemory::new(0x1000);

        exec_branch(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x0FE0);
    }
}
