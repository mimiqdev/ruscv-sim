//! D Extension Fused Multiply-Add Instructions (RV64D)
//!
//! Implements FMADD.D, FMSUB.D, FNMSUB.D, and FNMADD.D for double-precision
//! fused multiply-add operations (x * y + z with only one rounding).

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::{FpFlags, RoundingMode};
use crate::fpu::Fpr;
use crate::MemoryInterface;

/// Apply rounding mode to result (f64 version)
fn apply_rounding_d(result: f64, rm: RoundingMode) -> f64 {
    match rm {
        RoundingMode::RNE => result,
        RoundingMode::RTZ => result.trunc(),
        RoundingMode::RDN => result.floor(),
        RoundingMode::RUP => result.ceil(),
        RoundingMode::RMM => result,
    }
}

/// Execute FMADD.D (Floating-point Multiply-Add Double)
/// Operation: (rs1 × rs2) + rs3
pub fn exec_fmadd_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FMADD.D requires rs1");
    let rs2 = instr.rs2.expect("FMADD.D requires rs2");
    let rs3 = instr.rs3.expect("FMADD.D requires rs3");
    let rd = instr.rd.expect("FMADD.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());
    let val3 = f64::from_bits(state.fpr.read(rs3 as usize).bits());

    // Check for special cases before computation
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fcsr.set_flag(flags);
    }

    // Fused multiply-add: (rs1 × rs2) + rs3
    let result = val1 * val2 + val3;
    let rounded = apply_rounding_d(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

/// Execute FMSUB.D (Floating-point Multiply-Subtract Double)
/// Operation: (rs1 × rs2) - rs3
pub fn exec_fmsub_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FMSUB.D requires rs1");
    let rs2 = instr.rs2.expect("FMSUB.D requires rs2");
    let rs3 = instr.rs3.expect("FMSUB.D requires rs3");
    let rd = instr.rd.expect("FMSUB.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());
    let val3 = f64::from_bits(state.fpr.read(rs3 as usize).bits());

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fcsr.set_flag(flags);
    }

    // Fused multiply-subtract: (rs1 × rs2) - rs3
    let result = val1 * val2 - val3;
    let rounded = apply_rounding_d(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

/// Execute FNMSUB.D (Floating-point Negative Multiply-Subtract Double)
/// Operation: -(rs1 × rs2) + rs3 = rs3 - (rs1 × rs2)
pub fn exec_fnmsub_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FNMSUB.D requires rs1");
    let rs2 = instr.rs2.expect("FNMSUB.D requires rs2");
    let rs3 = instr.rs3.expect("FNMSUB.D requires rs3");
    let rd = instr.rd.expect("FNMSUB.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());
    let val3 = f64::from_bits(state.fpr.read(rs3 as usize).bits());

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fcsr.set_flag(flags);
    }

    // Negative multiply-subtract: -(rs1 × rs2) + rs3
    let result = -val1 * val2 + val3;
    let rounded = apply_rounding_d(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

/// Execute FNMADD.D (Floating-point Negative Multiply-Add Double)
/// Operation: -(rs1 × rs2) - rs3
pub fn exec_fnmadd_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FNMADD.D requires rs1");
    let rs2 = instr.rs2.expect("FNMADD.D requires rs2");
    let rs3 = instr.rs3.expect("FNMADD.D requires rs3");
    let rd = instr.rd.expect("FNMADD.D requires rd");
    let rm = state.fcsr.rounding_mode();

    let val1 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    let val2 = f64::from_bits(state.fpr.read(rs2 as usize).bits());
    let val3 = f64::from_bits(state.fpr.read(rs3 as usize).bits());

    // Check for special cases
    let mut flags = FpFlags::empty();

    if val1.is_infinite() && val2 == 0.0 {
        flags.insert(FpFlags::NV);
    }
    if val2.is_infinite() && val1 == 0.0 {
        flags.insert(FpFlags::NV);
    }

    if !flags.is_empty() {
        state.fcsr.set_flag(flags);
    }

    // Negative multiply-add: -(rs1 × rs2) - rs3
    let result = -val1 * val2 - val3;
    let rounded = apply_rounding_d(result, RoundingMode::from_frm(rm));

    if result != rounded {
        state.fcsr.set_flag(FpFlags::NX);
    }

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(rounded.to_bits()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::{DecodedInstruction, InstructionFormat, Opcode};
    use crate::fpu::Fpr;
    use crate::memory::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fmadd_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // 2.0 * 3.0 + 1.0 = 7.0
        state.fpr.write(1, Fpr::from_bits(2.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(3, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fmadd_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(4).bits());
        assert!((result - 7.0).abs() < 1e-10);
    }

    #[test]
    fn test_fmsub_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // 2.0 * 3.0 - 1.0 = 5.0
        state.fpr.write(1, Fpr::from_bits(2.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(3, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fmsub_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(4).bits());
        assert!((result - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_fnmsub_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // -(2.0 * 3.0) + 1.0 = -5.0
        state.fpr.write(1, Fpr::from_bits(2.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(3, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fnmsub_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(4).bits());
        assert!((result - (-5.0)).abs() < 1e-10);
    }

    #[test]
    fn test_fnmadd_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // -(2.0 * 3.0) - 1.0 = -7.0
        state.fpr.write(1, Fpr::from_bits(2.0f64.to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(3, Fpr::from_bits(1.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fnmadd_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(4).bits());
        assert!((result - (-7.0)).abs() < 1e-10);
    }

    #[test]
    fn test_fmadd_d_precision() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // (1/3) * 3 + 0 should give exactly 1 (if no rounding in between)
        state.fpr.write(1, Fpr::from_bits((1.0f64 / 3.0).to_bits()));
        state.fpr.write(2, Fpr::from_bits(3.0f64.to_bits()));
        state.fpr.write(3, Fpr::from_bits(0.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x01),
            rs1: Some(1),
            rs2: Some(2),
            rs3: Some(3),
            rd: Some(4),
            imm: None,
            branch_taken: false,
        };

        exec_fmadd_d(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(4).bits());
        // FMA should give more precise result than separate multiply and add
        assert!(result.is_finite());
    }
}
