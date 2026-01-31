//! 执行模块
//!
//! RV32I instruction execution

use crate::core::CoreState;
use crate::decode::{DecodedInstruction, Funct3, Opcode};
use crate::memory::{MemoryError, MemoryInterface};
use thiserror::Error;

/// Instruction executor function type
type ExecutorFn = fn(
    &Executor,
    &DecodedInstruction,
    &mut CoreState,
    &mut dyn MemoryInterface,
) -> Result<(), ExecuteError>;

/// 执行错误
#[derive(Error, Debug)]
pub enum ExecuteError {
    #[error("Misaligned memory access: addr 0x{0:08x}, alignment {1}")]
    MisalignedAccess(u32, u32),
    #[error("Invalid register access: x{0}")]
    InvalidRegister(u8),
    #[error("Invalid operation")]
    InvalidOperation,
    #[error("ECALL exception")]
    Ecall,
    #[error("EBREAK exception")]
    Ebreak,
    #[error("Memory access error: {0}")]
    MemoryError(#[from] MemoryError),
}

/*
 * OPTIMIZATION STRATEGY: Instruction Dispatch Lookup Table
 * -------------------------------------------------------
 * This implementation uses an array-based dispatch table for O(1) instruction lookup.
 *
 * Why Array over HashMap?
 * - Opcode enum values are stable u8 representations (0x03, 0x07, 0x13, etc.)
 * - Array lookup is a single bounds check + index (no hashing)
 * - No heap allocation (stack-allocated fixed-size array)
 * - Better cache locality (small, contiguous memory)
 * - Compiler can optimize array access with bounds-check elimination
 *
 * Dispatch Table Layout:
 * - Index: opcode as u8 (0-255)
 * - Value: Option<ExecutorFn> (Some(fn) if opcode is supported, None otherwise)
 * - Size: 256 entries (one per possible u8 opcode value)
 *
 * Performance Benefits:
 * - Single bounds check + array access vs. hash computation + lookup
 * - #[inline] hints on executor functions enable aggressive inlining
 * - Monomorphization eliminates dynamic dispatch overhead
 *
 * Future Optimization:
 * - Could use match statement with exhaustive opcode coverage
 * - Or const fn array for compile-time initialization
 */
/// 执行器
pub struct Executor {
    /// Opcode to executor function lookup table (array-based for O(1) access)
    dispatch_table: [Option<ExecutorFn>; 256],
}

impl Executor {
    /// 创建新的执行器
    pub fn new() -> Self {
        // Initialize dispatch table with None (256 entries for all possible u8 opcodes)
        let mut dispatch_table: [Option<ExecutorFn>; 256] = [None; 256];

        // Populate dispatch table with supported opcodes
        // This uses direct array indexing for O(1) lookup without hashing
        dispatch_table[Opcode::Load as u8 as usize] = Some(Executor::exec_load);
        dispatch_table[Opcode::LoadFp as u8 as usize] = None; // Not implemented
        dispatch_table[Opcode::Store as u8 as usize] = Some(Executor::exec_store);
        dispatch_table[Opcode::StoreFp as u8 as usize] = None; // Not implemented
        dispatch_table[Opcode::MiscMem as u8 as usize] = None; // Not implemented
        dispatch_table[Opcode::OpImm as u8 as usize] = Some(Executor::exec_op_imm);
        dispatch_table[Opcode::Op as u8 as usize] = Some(Executor::exec_op);
        dispatch_table[Opcode::Op32 as u8 as usize] = None; // Not implemented (RV64M)
        dispatch_table[Opcode::Lui as u8 as usize] = Some(Executor::exec_lui);
        dispatch_table[Opcode::Auipc as u8 as usize] = Some(Executor::exec_auipc);
        dispatch_table[Opcode::Branch as u8 as usize] = Some(Executor::exec_branch);
        dispatch_table[Opcode::Jalr as u8 as usize] = Some(Executor::exec_jalr);
        dispatch_table[Opcode::Jal as u8 as usize] = Some(Executor::exec_jal);
        dispatch_table[Opcode::System as u8 as usize] = Some(Executor::exec_system);

        Self { dispatch_table }
    }

    /// 执行译码后的指令
    #[inline]
    pub fn execute(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        // O(1) array-based instruction dispatch
        // Direct array indexing is faster than HashMap lookup (no hashing)
        let opcode_idx = instr.opcode as u8 as usize;
        match self.dispatch_table[opcode_idx] {
            Some(executor_fn) => executor_fn(self, instr, state, mem),
            None => Err(ExecuteError::InvalidOperation),
        }
    }

    /// LUI (Load Upper Immediate (LUI) (LUI))
    #[inline]
    fn exec_lui(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
            if rd != 0 {
                state.regs[rd as usize] = imm;
            }
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// AUIPC (Add Upper Immediate to PC (AUIPC) (AUIPC))
    #[inline]
    fn exec_auipc(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
            if rd != 0 {
                state.regs[rd as usize] = state.pc.wrapping_add(imm);
            }
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// JAL (Jump and Link (JAL) (JAL))
    #[inline]
    fn exec_jal(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(imm)) = (instr.rd, instr.imm) {
            let return_addr = state.pc.wrapping_add(4);
            let target = state.pc.wrapping_add(imm);

            if rd != 0 {
                state.regs[rd as usize] = return_addr;
            }

            state.pc = target;
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// JALR (Jump and Link (JAL) (JAL) Register)
    #[inline]
    fn exec_jalr(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        if let (Some(rd), Some(rs1), Some(imm)) = (instr.rd, instr.rs1, instr.imm) {
            let return_addr = state.pc.wrapping_add(4);
            let base = state.regs[rs1 as usize];
            let target = (base.wrapping_add(imm)) & !1u32; // LSB cleared

            if rd != 0 {
                state.regs[rd as usize] = return_addr;
            }

            state.pc = target;
            Ok(())
        } else {
            Err(ExecuteError::InvalidOperation)
        }
    }

    /// 分支指令
    ///
    /// Branch instruction encoding (funct3 field):
    /// - BEQ (000): Branch if Equal
    /// - BNE (001): Branch if Not Equal
    /// - BLT (100): Branch if Less Than (signed)
    /// - BGE (101): Branch if Greater or Equal (signed)
    /// - BLTU (110): Branch if Less Than Unsigned
    /// - BGEU (111): Branch if Greater or Equal Unsigned
    #[inline]
    fn exec_branch(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
            (instr.rs1, instr.rs2, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let rs1_val = state.regs[rs1 as usize];
        let rs2_val = state.regs[rs2 as usize];

        // Extract raw funct3 value (3 bits) for branch instruction decoding
        // Branch instructions use specific funct3 codes: 000=BEQ, 001=BNE, 100=BLT, 101=BGE, 110=BLTU, 111=BGEU
        let funct3_val = funct3 as u8;
        let take_branch = match funct3_val {
            0b000 => rs1_val == rs2_val,                   // BEQ
            0b001 => rs1_val != rs2_val,                   // BNE
            0b100 => (rs1_val as i32) < (rs2_val as i32),  // BLT (signed)
            0b101 => (rs1_val as i32) >= (rs2_val as i32), // BGE (signed)
            0b110 => rs1_val < rs2_val,                    // BLTU (unsigned)
            0b111 => rs1_val >= rs2_val,                   // BGEU (unsigned)
            _ => false,
        };

        if take_branch {
            state.pc = state.pc.wrapping_add(imm);
        }

        Ok(())
    }

    /// 加载指令
    #[inline]
    fn exec_load(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
            (instr.rd, instr.rs1, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let base = state.regs[rs1 as usize];
        let addr = base.wrapping_add(imm);

        let value = match funct3 {
            Funct3::AddSub => mem.read_word(addr).map(|v| v as i32 as u32)?, // LW
            Funct3::Sll => mem.read_half(addr).map(|v| v as i16 as i32 as u32)?, // LH
            Funct3::Slt => mem.read_byte(addr).map(|v| v as i8 as i32 as u32)?, // LB
            Funct3::Sltu => mem.read_half_zext(addr)?,                       // LHU
            Funct3::Xor => mem.read_byte_zext(addr)?,                        // LBU
            _ => return Err(ExecuteError::InvalidOperation),
        };

        if rd != 0 {
            state.regs[rd as usize] = value;
        }

        Ok(())
    }

    /// 存储指令
    #[inline]
    fn exec_store(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let (Some(rs1), Some(rs2), Some(imm), Some(funct3)) =
            (instr.rs1, instr.rs2, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let base = state.regs[rs1 as usize];
        let addr = base.wrapping_add(imm);
        let value = state.regs[rs2 as usize];

        match funct3 {
            Funct3::AddSub => mem.write_word(addr, value)?, // SW
            Funct3::Sll => mem.write_half(addr, value as u16)?, // SH
            Funct3::Slt => mem.write_byte(addr, value as u8)?, // SB
            _ => return Err(ExecuteError::InvalidOperation),
        }

        Ok(())
    }

    /// I-type operation instructions
    #[inline]
    fn exec_op_imm(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let (Some(rd), Some(rs1), Some(imm), Some(funct3)) =
            (instr.rd, instr.rs1, instr.imm, instr.funct3)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        // Extract shamt from imm[25:20] (lower 5 bits) for shift instructions
        // For shift instructions, shamt is in imm[4:0]
        let shamt = imm & 0x1F;

        let result: i32 = match funct3 {
            // ADDI (add immediate)
            Funct3::AddSub => {
                let rs1_val = state.regs[rs1 as usize] as i32;
                let imm_val = imm as i32;
                rs1_val.wrapping_add(imm_val)
            }
            // SLLI (shift left logical immediate)
            Funct3::Sll => {
                let rs1_val = state.regs[rs1 as usize];
                (rs1_val.wrapping_shl(shamt)) as i32
            }
            // SLTI (set less than immediate)
            Funct3::Slt => {
                let rs1_val = state.regs[rs1 as usize] as i32;
                let imm_val = imm as i32;
                if rs1_val < imm_val {
                    1
                } else {
                    0
                }
            }
            // SLTIU (set less than immediate unsigned)
            Funct3::Sltu => {
                let rs1_val = state.regs[rs1 as usize];
                if rs1_val < imm {
                    1
                } else {
                    0
                }
            }
            // XORI (exclusive or immediate)
            Funct3::Xor => {
                let rs1_val = state.regs[rs1 as usize];
                (rs1_val ^ imm) as i32
            }
            // SRLI/SRAI (shift right logical/arithmetic immediate)
            Funct3::SrlSra => {
                let rs1_val = state.regs[rs1 as usize];
                // Distinguish SRLI (funct7=0x00) vs SRAI (funct7=0x20)
                match instr.funct7 {
                    Some(0x00) => (rs1_val.wrapping_shr(shamt)) as i32, // SRLI
                    Some(0x20) => (rs1_val as i32).wrapping_shr(shamt), // SRAI
                    _ => return Err(ExecuteError::InvalidOperation),
                }
            }
            // ORI (or immediate)
            Funct3::Or => {
                let rs1_val = state.regs[rs1 as usize];
                (rs1_val | imm) as i32
            }
            // ANDI (and immediate)
            Funct3::And => {
                let rs1_val = state.regs[rs1 as usize];
                (rs1_val & imm) as i32
            }
        };

        if rd != 0 {
            state.regs[rd as usize] = result as u32;
        }

        Ok(())
    }

    /// R-type operation instructions
    #[inline]
    fn exec_op(
        &self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let (Some(rd), Some(rs1), Some(rs2), Some(funct3), Some(funct7)) =
            (instr.rd, instr.rs1, instr.rs2, instr.funct3, instr.funct7)
        else {
            return Err(ExecuteError::InvalidOperation);
        };

        let rs1_val = state.regs[rs1 as usize] as i32;
        let rs2_val = state.regs[rs2 as usize] as i32;
        let mut result: i32 = 0;

        // ADD/SUB
        if funct3 == Funct3::AddSub {
            if funct7 == 0 {
                result = rs1_val.wrapping_add(rs2_val);
            } else if funct7 == 0x20 {
                result = rs1_val.wrapping_sub(rs2_val);
            }
        }
        // SLL (logical left shift)
        else if funct3 == Funct3::Sll {
            let shamt = (rs2_val & 0x1F) as u32;
            result = (rs1_val as u32).wrapping_shl(shamt) as i32;
        }
        // SRL/SRA (shift right logical/arithmetic)
        else if funct3 == Funct3::SrlSra {
            let shamt = (rs2_val & 0x1F) as u32;
            if funct7 == 0 {
                result = (rs1_val as u32).wrapping_shr(shamt) as i32;
            } else {
                result = rs1_val.wrapping_shr(shamt);
            }
        }
        // SLT (set less than)
        else if funct3 == Funct3::Slt {
            result = if rs1_val < rs2_val { 1 } else { 0 };
        }
        // SLT (set less than)U
        else if funct3 == Funct3::Sltu {
            let rs1_u = state.regs[rs1 as usize];
            let rs2_u = state.regs[rs2 as usize];
            result = if rs1_u < rs2_u { 1 } else { 0 };
        }
        // XOR
        else if funct3 == Funct3::Xor {
            result = rs1_val ^ rs2_val;
        }
        // OR
        else if funct3 == Funct3::Or {
            result = rs1_val | rs2_val;
        }
        // AND
        else if funct3 == Funct3::And {
            result = rs1_val & rs2_val;
        }

        if rd != 0 {
            state.regs[rd as usize] = result as u32;
        }

        Ok(())
    }

    /// System instructions (ECALL, EBREAK)
    #[inline]
    fn exec_system(
        &self,
        instr: &DecodedInstruction,
        _state: &mut CoreState,
        _mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let Some(imm) = instr.imm else {
            return Err(ExecuteError::InvalidOperation);
        };

        match imm {
            0 => Err(ExecuteError::Ecall),
            1 => Err(ExecuteError::Ebreak),
            _ => Err(ExecuteError::InvalidOperation),
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::SimpleMemory;

    #[test]
    fn test_lui_execution() {
        let mut state = CoreState::default();
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::UType,
            opcode: Opcode::Lui,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rd: Some(1),
            imm: Some(0x12345000),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[1], 0x12345000);
    }

    #[test]
    fn test_add_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::RType,
            opcode: Opcode::Op,
            funct3: Some(Funct3::AddSub),
            funct7: Some(0),
            rs1: Some(1),
            rs2: Some(2),
            rd: Some(3),
            imm: None,
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[3], 30);
    }

    #[test]
    fn test_addi_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::AddSub),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 15);
    }

    #[test]
    fn test_addi_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // ADDI x2, x1, -3
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::AddSub),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-3i32) as u32),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2] as i32, 7);
    }

    #[test]
    fn test_slti_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        // SLTI x2, x1, 5
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Slt),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_slti_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // SLTI x2, x1, -5 (should be false since 10 > -5)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Slt),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-5i32) as u32),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_sltiu_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 3;

        // SLTIU x2, x1, 5
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sltu),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_sltiu_negative_rs1() {
        let mut state = CoreState::default();
        state.regs[1] = (-1i32) as u32; // 0xFFFFFFFF (large unsigned)

        // SLTIU x2, x1, 5 (should be false since 0xFFFFFFFF > 5)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sltu),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(5),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_sltiu_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 10;

        // SLTIU x2, x1, -5 (0xFFFFFFFB)
        // In unsigned comparison, 10 < 0xFFFFFFFB is true
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sltu),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-5i32) as u32),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 1);
    }

    #[test]
    fn test_xori_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000; // 192

        // XORI x2, x1, 0b1010_1010 (0xAA)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Xor),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0b1010_1010), // 0xAA
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0b1100_0000 ^ 0b1010_1010 = 0b0110_1010 = 0x6A = 106
        assert_eq!(state.regs[2], 0b0110_1010);
    }

    #[test]
    fn test_xori_with_negative_immediate() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFFF; // -1 as i32

        // XORI x2, x1, -1 (0xFFFFFFFF)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Xor),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-1i32) as u32), // 0xFFFFFFFF
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFF ^ 0xFFFFFFFF = 0
        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_ori_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1100_0000; // 192

        // ORI x2, x1, 0b1010_1010 (0xAA)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Or),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0b1010_1010), // 0xAA
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0b1100_0000 | 0b1010_1010 = 0b1110_1010 = 0xEA = 234
        assert_eq!(state.regs[2], 0b1110_1010);
    }

    #[test]
    fn test_ori_with_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        // ORI x2, x1, 0 (should keep the value)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Or),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x12345678);
    }

    #[test]
    fn testori_with_all_ones() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        // ORI x2, x1, -1 (0xFFFFFFFF) should result in all 1s
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Or),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-1i32) as u32), // 0xFFFFFFFF
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0xFFFFFFFF);
    }

    #[test]
    fn test_andi_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1111_1111_0000_0000; // 0xFF00

        // ANDI x2, x1, 0b1010_1010_1010_1010 (0xAAAA)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::And),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0b1010_1010_1010_1010), // 0xAAAA
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFF00 & 0xAAAA = 0xAA00
        assert_eq!(state.regs[2], 0xAA00);
    }

    #[test]
    fn test_andi_with_zero() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        // ANDI x2, x1, 0 (should clear the value)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::And),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(0),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0);
    }

    #[test]
    fn test_andi_with_all_ones() {
        let mut state = CoreState::default();
        state.regs[1] = 0x12345678;

        // ANDI x2, x1, -1 (0xFFFFFFFF) should keep the value
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::And),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some((-1i32) as u32), // 0xFFFFFFFF
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 0x12345678);
    }

    #[test]
    fn test_slli_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b0000_0001; // 1

        // SLLI x2, x1, 4 (shift left by 4)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sll),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(4), // shamt = 4
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 1 << 4 = 16
        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_slli_large_shift() {
        let mut state = CoreState::default();
        state.regs[1] = 1;

        // SLLI x2, x1, 8 (shift left by 8)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::Sll),
            funct7: None,
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(8), // shamt = 8
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.regs[2], 256);
    }

    #[test]
    fn test_srli_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0b1_0000_0000; // 256

        // SRLI x2, x1, 4 (shift right logical by 4)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::SrlSra),
            funct7: Some(0x00), // SRLI
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(4), // shamt = 4
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 256 >> 4 = 16
        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_srli_with_negative_value() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFF0; // Large unsigned value

        // SRLI x2, x1, 4 (shift right logical by 4)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::SrlSra),
            funct7: Some(0x00), // SRLI
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(4), // shamt = 4
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFF0 >> 4 = 0x0FFFFFFF
        assert_eq!(state.regs[2], 0x0FFFFFFF);
    }

    #[test]
    fn test_srai_execution() {
        let mut state = CoreState::default();
        state.regs[1] = 0xFFFFFFF0; // -16 as i32

        // SRAI x2, x1, 4 (shift right arithmetic by 4)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::SrlSra),
            funct7: Some(0x20), // SRAI
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(4), // shamt = 4
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // -16 >> 4 = -1 (0xFFFFFFFF) - sign extension preserves the sign bit
        assert_eq!(state.regs[2] as i32, -1);
    }

    #[test]
    fn test_srai_with_positive_value() {
        let mut state = CoreState::default();
        state.regs[1] = 256; // Positive value

        // SRAI x2, x1, 4 (shift right arithmetic by 4)
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::OpImm,
            funct3: Some(Funct3::SrlSra),
            funct7: Some(0x20), // SRAI
            rs1: Some(1),
            rs2: None,
            rd: Some(2),
            imm: Some(4), // shamt = 4
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 256 >> 4 = 16
        assert_eq!(state.regs[2], 16);
    }

    #[test]
    fn test_ecall() {
        let mut state = CoreState::default();
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::System,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rd: None,
            imm: Some(0), // ECALL
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        let result = executor.execute(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::Ecall)));
    }

    #[test]
    fn test_ebreak() {
        let mut state = CoreState::default();
        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::IType,
            opcode: Opcode::System,
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rd: None,
            imm: Some(1), // EBREAK
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        let result = executor.execute(&instr, &mut state, &mut mem);

        assert!(matches!(result, Err(ExecuteError::Ebreak)));
    }

    // Branch instruction tests

    #[test]
    fn test_beq_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::AddSub), // BEQ (funct3=000)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20), // Branch offset
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

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

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::AddSub), // BEQ (funct3=000)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000); // PC unchanged
    }

    #[test]
    fn test_bne_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 20;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Sll), // BNE (funct3=001)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bne_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Sll), // BNE (funct3=001)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_blt_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = -5i32 as u32; // 0xFFFFFFFB
        state.regs[2] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Xor), // BLT (funct3=100)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_blt_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Xor), // BLT (funct3=100)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_blt_negative_vs_positive() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = -10i32 as u32; // 0xFFFFFFF6
        state.regs[2] = 5;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Xor), // BLT (funct3=100)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // -10 < 5, so branch should be taken
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

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::SrlSra), // BGE (funct3=101)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

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

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::SrlSra), // BGE (funct3=101)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 10 >= 10, so branch should be taken
        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bge_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::SrlSra), // BGE (funct3=101)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bgeu_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 10;
        state.regs[2] = 5;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::And), // BGEU (funct3=111)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bgeu_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE; // Large unsigned (treated as -2 signed)
        state.regs[2] = 5;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::And), // BGEU (funct3=111)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFE >= 5 as unsigned, branch should be taken
        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bgeu_not_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::And), // BGEU (funct3=111)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bltu_taken() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 10;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Or), // BLTU (funct3=110)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        assert_eq!(state.pc, 0x1020);
    }

    #[test]
    fn test_bltu_not_taken_large_unsigned() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 0xFFFFFFFE; // Large unsigned
        state.regs[2] = 5;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Or), // BLTU (funct3=110)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFE is not < 5 as unsigned
        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bltu_negative_vs_positive() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = -1i32 as u32; // 0xFFFFFFFF
        state.regs[2] = 5;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Or), // BLTU (funct3=110)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 0xFFFFFFFF is not < 5 as unsigned (it's larger)
        assert_eq!(state.pc, 0x1000);
    }

    #[test]
    fn test_bltu_small_vs_large() {
        let mut state = CoreState {
            pc: 0x1000,
            ..Default::default()
        };
        state.regs[1] = 5;
        state.regs[2] = 0xFFFFFFFF;

        let instr = DecodedInstruction {
            raw: 0,
            format: crate::decode::InstructionFormat::BType,
            opcode: Opcode::Branch,
            funct3: Some(Funct3::Or), // BLTU (funct3=110)
            funct7: None,
            rs1: Some(1),
            rs2: Some(2),
            rd: None,
            imm: Some(0x20),
            branch_taken: false,
        };

        let mut executor = Executor::new();
        let mut mem = SimpleMemory::new(0x1000);
        executor.execute(&instr, &mut state, &mut mem).unwrap();

        // 5 < 0xFFFFFFFF as unsigned
        assert_eq!(state.pc, 0x1020);
    }
}
