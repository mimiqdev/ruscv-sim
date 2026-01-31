//! Floating-point Conversion Instructions (RV64F)
//!
//! Implements FCVT instructions for converting between 32-bit floating-point
//! and 32/64-bit integer types.

use crate::core::CoreState;
use crate::decode::InstructionFormat;
use crate::decode::{DecodedInstruction, Opcode};
use crate::execute::ExecuteError;
use crate::fpu::fcsr::FpFlags;
use crate::fpu::Fpr;

/// Convert i32 to f32
fn fcvt_i32_to_f32(val: i32) -> f32 {
    val as f32
}

/// Convert u32 to f32
fn fcvt_u32_to_f32(val: u32) -> f32 {
    val as f32
}

/// Execute FCVT.W.S (Convert Single to Signed Word)
/// Converts 32-bit float to 32-bit signed integer
pub fn exec_fcvt_w_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.W.S requires rs1");
    let rd = instr.rd.expect("FCVT.W.S requires rd");
    let _rm = state.fcsr.rounding_mode();

    let val = state.fpr.read(rs1 as usize).get();

    // Check for special cases
    if val.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x7FFFFFFF; // Max i32
        return Ok(());
    }

    if val.is_infinite() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = if val < 0.0 { 0x80000000 } else { 0x7FFFFFFF };
        return Ok(());
    }

    // Check if value is out of range for i32
    if val > (i32::MAX as f32) {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x7FFFFFFF;
        return Ok(());
    }
    if val < (i32::MIN as f32) {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x80000000;
        return Ok(());
    }

    let result = val as i32;
    state.regs[rd as usize] = result as u32;

    Ok(())
}

/// Execute FCVT.L.S (Convert Single to Signed Long)
/// Converts 32-bit float to 64-bit signed integer
pub fn exec_fcvt_l_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.L.S requires rs1");
    let rd = instr.rd.expect("FCVT.L.S requires rd");

    let val = state.fpr.read(rs1 as usize).get();

    // Check for special cases
    if val.is_nan() || val.is_infinite() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x7FFFFFFF;
        return Ok(());
    }

    let result = val as i64;
    state.regs[rd as usize] = result as u32;
    state.regs[(rd + 1) as usize] = (result >> 32) as u32;

    Ok(())
}

/// Execute FCVT.WU.S (Convert Single to Unsigned Word)
/// Converts 32-bit float to 32-bit unsigned integer
pub fn exec_fcvt_wu_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.WU.S requires rs1");
    let rd = instr.rd.expect("FCVT.WU.S requires rd");

    let val = state.fpr.read(rs1 as usize).get();

    // Check for special cases
    if val.is_nan() || val.is_infinite() || val < 0.0 {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = u32::MAX;
        return Ok(());
    }

    // Check if value is out of range for u32
    if val > (u32::MAX as f32) {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = u32::MAX;
        return Ok(());
    }

    let result = val as u32;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute FCVT.LU.S (Convert Single to Unsigned Long)
/// Converts 32-bit float to 64-bit unsigned integer
pub fn exec_fcvt_lu_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.LU.S requires rs1");
    let rd = instr.rd.expect("FCVT.LU.S requires rd");

    let val = state.fpr.read(rs1 as usize).get();

    if val.is_nan() || val.is_infinite() || val < 0.0 {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = u32::MAX;
        return Ok(());
    }

    let result = (val as u64) as u32;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute FCVT.S.W (Convert Signed Word to Single)
/// Converts 32-bit signed integer to 32-bit float
pub fn exec_fcvt_s_w(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.S.W requires rs1");
    let rd = instr.rd.expect("FCVT.S.W requires rd");

    let val = state.regs[rs1 as usize] as i32;
    let result = fcvt_i32_to_f32(val);

    state.fpr.write(rd as usize, Fpr::new(result));
    Ok(())
}

/// Execute FCVT.S.L (Convert Signed Long to Single)
/// Converts 64-bit signed integer to 32-bit float
pub fn exec_fcvt_s_l(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.S.L requires rs1");
    let rd = instr.rd.expect("FCVT.S.L requires rd");

    // Combine rs1 and rs1+1 for 64-bit value
    let low = state.regs[rs1 as usize] as u32;
    let high = state.regs[(rs1 + 1) as usize] as u32;
    let val = ((high as i64) << 32) | (low as i64);
    let result = val as f32;

    state.fpr.write(rd as usize, Fpr::new(result));
    Ok(())
}

/// Execute FCVT.S.WU (Convert Unsigned Word to Single)
/// Converts 32-bit unsigned integer to 32-bit float
pub fn exec_fcvt_s_wu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.S.WU requires rs1");
    let rd = instr.rd.expect("FCVT.S.WU requires rd");

    let val = state.regs[rs1 as usize] as u32;
    let result = fcvt_u32_to_f32(val);

    state.fpr.write(rd as usize, Fpr::new(result));
    Ok(())
}

/// Execute FCVT.S.LU (Convert Unsigned Long to Single)
/// Converts 64-bit unsigned integer to 32-bit float
pub fn exec_fcvt_s_lu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.S.LU requires rs1");
    let rd = instr.rd.expect("FCVT.S.LU requires rd");

    let low = state.regs[rs1 as usize] as u32;
    let high = state.regs[(rs1 + 1) as usize] as u32;
    let val = ((high as u64) << 32) | (low as u64);
    let result = val as f32;

    state.fpr.write(rd as usize, Fpr::new(result));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimpleMemory;

    fn create_test_state() -> CoreState {
        CoreState::default()
    }

    #[test]
    fn test_fcvt_s_w_positive() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 42;

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_s_w(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert!((result - 42.0).abs() < 1e-5);
    }

    #[test]
    fn test_fcvt_s_w_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 0xFFFFFFFEu32; // -2 as i32

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_s_w(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert!((result - (-2.0)).abs() < 1e-5);
    }

    #[test]
    fn test_fcvt_w_s_positive() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(5.5));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 5);
    }

    #[test]
    fn test_fcvt_w_s_nan() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(f32::NAN));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x7FFFFFFF);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_wu_s_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(-1.0));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_wu_s(&decoded, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], u32::MAX);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_s_wu() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 100;

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_s_wu(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert!((result - 100.0).abs() < 1e-5);
    }
}
