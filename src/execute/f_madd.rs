//! Floating-point Fused Multiply-Add Instructions (RV64F)
//!
//! Implements FMADD.S, FMSUB.S, FNMSUB.S, and FNMADD.S for single-precision
//! fused multiply-add operations (x * y + z with only one rounding).
//!
//! R4-type format: | funct2=rm | rs3 | rs2 | rs1 | funct3 | rd | opcode=OpFp(1000011) |

use crate::core::CoreState;
use crate::decode::InstructionFormat;
use crate::decode::{DecodedInstruction, Opcode};
use crate::execute::ExecuteError;
use crate::fpu::fcsr::{FpFlags, RoundingMode};
use crate::fpu::Fpr;
use crate::memory::{MemoryError, MemoryInterface};

/// Apply rounding mode to result
fn apply_rounding(result: f32, rm: RoundingMode) -> f32 {
    match rm {
        RoundingMode::RNE => result,
        RoundingMode::RTZ => result.trunc(),
        RoundingMode::RDN => result.floor(),
        RoundingMode::RUP => result.ceil(),
        RoundingMode::RMM => result,
    }
}

/// Execute FMADD.S (Floating-point Multiply-Add Single)
/// Operation: (rs1 × rs2) + rs3
pub fn exec_fmadd_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FMADD.S requires rs1");
    let rs2 = instr.rs2.expect("FMADD.S requires rs2");
    let rs3 = instr.rs3.expect("FMADD.S requires rs3");
    let rd = instr.rd.expect("FMADD.S requires rd");
    let rm = state.fpr.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();
    let val3 = state.fpr.read(rs3 as usize).get();

    // Check for special cases before computation
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fpr.fcsr.set_flag(flags);
    }

    // Fused multiply-add: (rs1 × rs2) + rs3
    let result = val1 * val2 + val3;
    let rounded = apply_rounding(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fpr.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

/// Execute FMSUB.S (Floating-point Multiply-Subtract Single)
/// Operation: (rs1 × rs2) - rs3
pub fn exec_fmsub_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FMSUB.S requires rs1");
    let rs2 = instr.rs2.expect("FMSUB.S requires rs2");
    let rs3 = instr.rs3.expect("FMSUB.S requires rs3");
    let rd = instr.rd.expect("FMSUB.S requires rd");
    let rm = state.fpr.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();
    let val3 = state.fpr.read(rs3 as usize).get();

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fpr.fcsr.set_flag(flags);
    }

    // Fused multiply-subtract: (rs1 × rs2) - rs3
    let result = val1 * val2 - val3;
    let rounded = apply_rounding(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fpr.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

/// Execute FNMSUB.S (Floating-point Negative Multiply-Subtract Single)
/// Operation: -(rs1 × rs2) + rs3 = rs3 - (rs1 × rs2)
pub fn exec_fnmsub_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FNMSUB.S requires rs1");
    let rs2 = instr.rs2.expect("FNMSUB.S requires rs2");
    let rs3 = instr.rs3.expect("FNMSUB.S requires rs3");
    let rd = instr.rd.expect("FNMSUB.S requires rd");
    let rm = state.fpr.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();
    let val3 = state.fpr.read(rs3 as usize).get();

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fpr.fcsr.set_flag(flags);
    }

    // Negative multiply-subtract: -(rs1 × rs2) + rs3
    let result = -val1 * val2 + val3;
    let rounded = apply_rounding(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fpr.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

/// Execute FNMADD.S (Floating-point Negative Multiply-Add Single)
/// Operation: -(rs1 × rs2) - rs3
pub fn exec_fnmadd_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FNMADD.S requires rs1");
    let rs2 = instr.rs2.expect("FNMADD.S requires rs2");
    let rs3 = instr.rs3.expect("FNMADD.S requires rs3");
    let rd = instr.rd.expect("FNMADD.S requires rd");
    let rm = state.fpr.fcsr.rounding_mode();

    let val1 = state.fpr.read(rs1 as usize).get();
    let val2 = state.fpr.read(rs2 as usize).get();
    let val3 = state.fpr.read(rs3 as usize).get();

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fpr.fcsr.set_flag(flags);
    }

    // Negative multiply-add: -(rs1 × rs2) - rs3
    let result = -val1 * val2 - val3;
    let rounded = apply_rounding(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fpr.fcsr.set_flag(FpFlags::NX);
    }

    state.fpr.write(rd as usize, Fpr::new(rounded));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fmadd_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // 2.0 * 3.0 + 1.0 = 7.0
        state.fpr.write(1, Fpr::new(2.0));
        state.fpr.write(2, Fpr::new(3.0));
        state.fpr.write(3, Fpr::new(1.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fmadd_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(4).get();
        assert!((result - 7.0).abs() < 1e-5);
    }

    #[test]
    fn test_fmsub_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // 2.0 * 3.0 - 1.0 = 5.0
        state.fpr.write(1, Fpr::new(2.0));
        state.fpr.write(2, Fpr::new(3.0));
        state.fpr.write(3, Fpr::new(1.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fmsub_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(4).get();
        assert!((result - 5.0).abs() < 1e-5);
    }

    #[test]
    fn test_fnmsub_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // -(2.0 * 3.0) + 1.0 = -5.0
        state.fpr.write(1, Fpr::new(2.0));
        state.fpr.write(2, Fpr::new(3.0));
        state.fpr.write(3, Fpr::new(1.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fnmsub_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(4).get();
        assert!((result - (-5.0)).abs() < 1e-5);
    }

    #[test]
    fn test_fnmadd_s_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // -(2.0 * 3.0) - 1.0 = -7.0
        state.fpr.write(1, Fpr::new(2.0));
        state.fpr.write(2, Fpr::new(3.0));
        state.fpr.write(3, Fpr::new(1.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fnmadd_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(4).get();
        assert!((result - (-7.0)).abs() < 1e-5);
    }

    #[test]
    fn test_fmadd_s_zero_multiply() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // 0.0 * 5.0 + 3.0 = 3.0
        state.fpr.write(1, Fpr::new(0.0));
        state.fpr.write(2, Fpr::new(5.0));
        state.fpr.write(3, Fpr::new(3.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fmadd_s(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(4).get();
        assert!((result - 3.0).abs() < 1e-5);
    }
}
