//! D Extension Conversion Instructions (RV64D)
//!
//! Implements FCVT instructions for converting between double-precision
//! floating-point and single-precision/integer types.

use crate::core::CoreState;
use crate::decode::DecodedInstruction;
use crate::execute::ExecuteError;
use crate::fpu::fcsr::FpFlags;
use crate::fpu::Fpr;

/// Execute FCVT.D.S (Convert Single to Double)
/// Converts 32-bit float to 64-bit double
pub fn exec_fcvt_d_s(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.D.S requires rs1");
    let rd = instr.rd.expect("FCVT.D.S requires rd");

    // Read as 32-bit float (lower 32 bits)
    let val_f32 = state.fpr.read(rs1 as usize).get();
    // Convert to 64-bit double
    let val_f64 = val_f32 as f64;

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(val_f64.to_bits()));
    Ok(())
}

/// Execute FCVT.S.D (Convert Double to Single)
/// Converts 64-bit double to 32-bit float (may lose precision)
pub fn exec_fcvt_s_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.S.D requires rs1");
    let rd = instr.rd.expect("FCVT.S.D requires rd");
    let rm = state.fcsr.rounding_mode();

    // Read as 64-bit double
    let val_f64 = f64::from_bits(state.fpr.read(rs1 as usize).bits());
    // Convert to 32-bit float with rounding
    let val_f32 = match rm {
        1 => (val_f64 as f32).trunc(),
        2 => (val_f64 as f32).floor(),
        3 => (val_f64 as f32).ceil(),
        _ => val_f64 as f32,
    };

    // NaN-box the result
    state.fpr.write(rd as usize, Fpr::new(val_f32));
    Ok(())
}

/// Execute FCVT.W.D (Convert Double to Signed Word)
/// Converts 64-bit double to 32-bit signed integer
pub fn exec_fcvt_w_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.W.D requires rs1");
    let rd = instr.rd.expect("FCVT.W.D requires rd");

    let val = f64::from_bits(state.fpr.read(rs1 as usize).bits());

    // Check for special cases
    if val.is_nan() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x7FFFFFFF_u64;
        return Ok(());
    }

    if val.is_infinite() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = if val < 0.0 { 0x80000000_u64 } else { 0x7FFFFFFF_u64 };
        return Ok(());
    }

    // Check if value is out of range for i32
    if val > (i32::MAX as f64) {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x7FFFFFFF_u64;
        return Ok(());
    }
    if val < (i32::MIN as f64) {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x80000000_u64;
        return Ok(());
    }

    // Result is sign-extended to 64 bits
    let result = val as i32;
    state.regs[rd as usize] = result as i64 as u64;

    Ok(())
}

/// Execute FCVT.L.D (Convert Double to Signed Long)
/// Converts 64-bit double to 64-bit signed integer
pub fn exec_fcvt_l_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.L.D requires rs1");
    let rd = instr.rd.expect("FCVT.L.D requires rd");

    let val = f64::from_bits(state.fpr.read(rs1 as usize).bits());

    // Check for special cases
    if val.is_nan() || val.is_infinite() {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = 0x7FFFFFFF_FFFFFFFF_u64;
        return Ok(());
    }

    let result = val as i64;
    state.regs[rd as usize] = result as u64;

    Ok(())
}

/// Execute FCVT.WU.D (Convert Double to Unsigned Word)
/// Converts 64-bit double to 32-bit unsigned integer
pub fn exec_fcvt_wu_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.WU.D requires rs1");
    let rd = instr.rd.expect("FCVT.WU.D requires rd");

    let val = f64::from_bits(state.fpr.read(rs1 as usize).bits());

    // Check for special cases
    if val.is_nan() || val.is_infinite() || val < 0.0 {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = u32::MAX as u64;
        return Ok(());
    }

    // Check if value is out of range for u32
    if val > (u32::MAX as f64) {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = u32::MAX as u64;
        return Ok(());
    }

    let result = val as u32;
    state.regs[rd as usize] = result as u64;

    Ok(())
}

/// Execute FCVT.LU.D (Convert Double to Unsigned Long)
/// Converts 64-bit double to 64-bit unsigned integer
pub fn exec_fcvt_lu_d(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.LU.D requires rs1");
    let rd = instr.rd.expect("FCVT.LU.D requires rd");

    let val = f64::from_bits(state.fpr.read(rs1 as usize).bits());

    if val.is_nan() || val.is_infinite() || val < 0.0 {
        state.fcsr.set_flag(FpFlags::NV);
        state.regs[rd as usize] = u64::MAX;
        return Ok(());
    }

    let result = val as u64;
    state.regs[rd as usize] = result;

    Ok(())
}

/// Execute FCVT.D.W (Convert Signed Word to Double)
/// Converts 32-bit signed integer to 64-bit double
pub fn exec_fcvt_d_w(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.D.W requires rs1");
    let rd = instr.rd.expect("FCVT.D.W requires rd");

    let val = state.regs[rs1 as usize] as i32;
    let result = val as f64;

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(result.to_bits()));
    Ok(())
}

/// Execute FCVT.D.L (Convert Signed Long to Double)
/// Converts 64-bit signed integer to 64-bit double
pub fn exec_fcvt_d_l(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.D.L requires rs1");
    let rd = instr.rd.expect("FCVT.D.L requires rd");

    // In RV64, register is already 64-bit
    let val = state.regs[rs1 as usize] as i64;
    let result = val as f64;

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(result.to_bits()));
    Ok(())
}

/// Execute FCVT.D.WU (Convert Unsigned Word to Double)
/// Converts 32-bit unsigned integer to 64-bit double
pub fn exec_fcvt_d_wu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.D.WU requires rs1");
    let rd = instr.rd.expect("FCVT.D.WU requires rd");

    // Take lower 32 bits as unsigned
    let val = state.regs[rs1 as usize] as u32;
    let result = val as f64;

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(result.to_bits()));
    Ok(())
}

/// Execute FCVT.D.LU (Convert Unsigned Long to Double)
/// Converts 64-bit unsigned integer to 64-bit double
pub fn exec_fcvt_d_lu(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn crate::memory::MemoryInterface,
) -> Result<(), ExecuteError> {
    let rs1 = instr.rs1.expect("FCVT.D.LU requires rs1");
    let rd = instr.rd.expect("FCVT.D.LU requires rd");

    // In RV64, register is already 64-bit
    let val = state.regs[rs1 as usize];
    let result = val as f64;

    state
        .fpr
        .write(rd as usize, Fpr::from_bits(result.to_bits()));
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
    fn test_fcvt_d_s() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::new(std::f32::consts::PI));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_d_s(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert!((result - std::f64::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn test_fcvt_s_d() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state
            .fpr
            .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x40),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        assert!((result - std::f32::consts::PI).abs() < 1e-5);
    }

    #[test]
    fn test_fcvt_d_w() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(42.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(1),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 42);
    }

    #[test]
    fn test_fcvt_w_d_nan() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::NAN.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(1),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 0x7FFFFFFF);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_wu_d_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits((-1.0f64).to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], u32::MAX as u64);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_d_w_positive() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 100;

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(3),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_d_w(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert!((result - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_fcvt_d_w_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 0xFFFFFFFEu64; // -2 as i32

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(3),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_d_w(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert!((result - (-2.0)).abs() < 1e-10);
    }

    #[test]
    fn test_fcvt_d_wu() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 100;

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(4),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_d_wu(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(2).bits());
        assert!((result - 100.0).abs() < 1e-10);
    }

    #[test]
    fn test_fcvt_s_d_precision_loss() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // PI to more precision than f32 can hold
        state
            .fpr
            .write(1, Fpr::from_bits(std::f64::consts::PI.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x40),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();

        let result = state.fpr.read(2).get();
        // Converting back to f64 should show precision loss
        let back_to_f64 = result as f64;
        let original = std::f64::consts::PI;
        assert!((back_to_f64 - original).abs() > 1e-15);
    }

    #[test]
    fn test_fcvt_s_d_rounding_modes() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(2.7f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x40),
            rs1: Some(1),
            rs2: Some(0),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        // Test RTZ (Round Towards Zero)
        state.fcsr.set_rounding_mode(1);
        exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();
        let rtz_result = state.fpr.read(2).get();
        assert!((rtz_result - 2.0).abs() < 0.1);

        // Test RDN (Round Down)
        state.fcsr.set_rounding_mode(2);
        exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();
        let rdn_result = state.fpr.read(2).get();
        assert!((rdn_result - 2.0).abs() < 0.1);

        // Test RUP (Round Up)
        state.fcsr.set_rounding_mode(3);
        exec_fcvt_s_d(&decoded, &mut state, &mut mem).unwrap();
        let rup_result = state.fpr.read(2).get();
        assert!((rup_result - 3.0).abs() < 0.1);
    }

    #[test]
    fn test_fcvt_w_d_overflow_positive() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // Value larger than i32::MAX
        state
            .fpr
            .write(1, Fpr::from_bits((i32::MAX as f64 + 1000.0).to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(1),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 0x7FFFFFFF);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_w_d_overflow_negative() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        // Value smaller than i32::MIN
        state
            .fpr
            .write(1, Fpr::from_bits((i32::MIN as f64 - 1000.0).to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(1),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 0x80000000);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_w_d_infinity() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::INFINITY.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(1),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 0x7FFFFFFF);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_w_d_neg_infinity() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state
            .fpr
            .write(1, Fpr::from_bits(f64::NEG_INFINITY.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(1),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_w_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 0x80000000);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_l_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(42.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_l_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 42);
    }

    #[test]
    fn test_fcvt_l_d_special() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(f64::NAN.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_l_d(&decoded, &mut state, &mut mem).unwrap();
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_wu_d_overflow() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state
            .fpr
            .write(1, Fpr::from_bits((u32::MAX as f64 + 1000.0).to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], u32::MAX as u64);
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_wu_d_normal() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(12345.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_wu_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[3], 12345);
    }

    #[test]
    fn test_fcvt_lu_d_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits(12345.0f64.to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(3),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_lu_d(&decoded, &mut state, &mut mem).unwrap();
        assert_eq!(state.regs[2], 12345);
    }

    #[test]
    fn test_fcvt_lu_d_special() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.fpr.write(1, Fpr::from_bits((-1.0f64).to_bits()));

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(3),
            rs3: None,
            rd: Some(2),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_lu_d(&decoded, &mut state, &mut mem).unwrap();
        assert!(state.fcsr.flags().contains(FpFlags::NV));
    }

    #[test]
    fn test_fcvt_d_l_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 12345;
        state.regs[2] = 0;

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(2),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_d_l(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - 12345.0).abs() < 1e-10);
    }

    #[test]
    fn test_fcvt_d_lu_basic() {
        let mut state = create_test_state();
        let mut mem = SimpleMemory::new(0x1000);

        state.regs[1] = 12345;
        state.regs[2] = 0;

        let decoded = DecodedInstruction {
            raw: 0,
            format: InstructionFormat::RType,
            opcode: Opcode::OpFp,
            funct3: None,
            funct7: Some(0x41),
            rs1: Some(1),
            rs2: Some(3),
            rs3: None,
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        exec_fcvt_d_lu(&decoded, &mut state, &mut mem).unwrap();

        let result = f64::from_bits(state.fpr.read(3).bits());
        assert!((result - 12345.0).abs() < 1e-10);
    }
}
