//! 指令译码模块
//!
//! RV32I instruction decoder

use num_enum::TryFromPrimitive;
use std::hash::Hash;
use thiserror::Error;

/// 译码错误
#[derive(Error, Debug)]
pub enum DecodeError {
    #[error("Invalid instruction encoding: 0x{0:08x}")]
    InvalidInstruction(u32),
    #[error("Reserved instruction")]
    ReservedInstruction,
    #[error("Unimplemented instruction")]
    UnimplementedInstruction,
}

/// RV32I Instruction format
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstructionFormat {
    RType, // Register-Register
    IType, // Immediate operation
    SType, // Store
    BType, // Conditional branch
    UType, // Long immediate (LUI, AUIPC)
    JType, // Unconditional jump (JAL)
}

/// RV32I Opcode (primary)
#[derive(Debug, Clone, Copy, TryFromPrimitive, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    Load = 0b000_0011,
    LoadFp = 0b000_0111,
    Store = 0b010_0011,
    StoreFp = 0b010_0111,
    MiscMem = 0b000_1111,
    OpImm = 0b001_0011,
    Op = 0b011_0011,
    Op32 = 0b011_1011,
    Lui = 0b011_0111,
    Auipc = 0b001_0111,
    Branch = 0b110_0011,
    Jalr = 0b110_0111,
    Jal = 0b110_1111,
    System = 0b111_0011,
    Amo = 0b010_1111,
    OpFp = 0b101_0011,
}

/// RV32I Function code (funct3)
#[derive(Debug, Clone, Copy, TryFromPrimitive, PartialEq)]
#[repr(u8)]
pub enum Funct3 {
    AddSub = 0b000,
    Sll = 0b001,
    Slt = 0b010,
    Sltu = 0b011,
    Xor = 0b100,
    SrlSra = 0b101,
    Or = 0b110,
    And = 0b111,
}

/// F extension rounding mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FPRoundingMode {
    RNE = 0, // Round to Nearest, ties to Even
    RTZ = 1, // Round Towards Zero
    RDN = 2, // Round Down (towards -∞)
    RUP = 3, // Round Up (towards +∞)
    RMM = 4, // Round to Nearest, ties to Max Magnitude
    DYN = 7, // Dynamic rounding mode
}

/// 译码后的指令
#[derive(Debug, Clone)]
pub struct DecodedInstruction {
    /// 原始指令编码
    pub raw: u32,
    /// Instruction format
    pub format: InstructionFormat,
    /// 操作码
    pub opcode: Opcode,
    /// funct3 (if applicable)
    pub funct3: Option<Funct3>,
    /// funct7 (if applicable)
    pub funct7: Option<u8>,
    /// Source register 1 (rs1)
    pub rs1: Option<u8>,
    /// Source register 2 (rs2)
    pub rs2: Option<u8>,
    /// Source register 3 (rs3) - for R4-type instructions (FMADD/FMSUB/etc)
    pub rs3: Option<u8>,
    /// 目标寄存器 (rd)
    pub rd: Option<u8>,
    /// 立即数
    pub imm: Option<u32>,
    /// 是否发生分支跳转
    pub branch_taken: bool,
}

impl DecodedInstruction {
    /// 创建新的译码指令
    pub fn new(raw: u32) -> Self {
        Self {
            raw,
            format: InstructionFormat::RType,
            opcode: Opcode::Lui, // 默认值
            funct3: None,
            funct7: None,
            rs1: None,
            rs2: None,
            rs3: None,
            rd: None,
            imm: None,
            branch_taken: false,
        }
    }
}

/// 指令译码器
pub struct InstructionDecoder {}

impl InstructionDecoder {
    /// 创建新的译码器
    pub fn new() -> Self {
        Self {}
    }

    /// 译码单条指令
    pub fn decode(&self, instruction: u32) -> Result<DecodedInstruction, DecodeError> {
        let opcode_val = (instruction & 0x7F) as u8;
        let opcode = Opcode::try_from(opcode_val)
            .map_err(|_| DecodeError::InvalidInstruction(instruction))?;

        let mut decoded = DecodedInstruction::new(instruction);
        decoded.opcode = opcode;

        // 根据操作码解析不同格式
        match opcode {
            Opcode::Lui | Opcode::Auipc => {
                decoded.format = InstructionFormat::UType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.imm = Some(instruction & 0xFFFFF000); // Upper 20-bit immediate
            }
            Opcode::Jal => {
                decoded.format = InstructionFormat::JType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                // J-type immediate: imm[20|10:1|11|19:12]
                let imm20 = ((instruction >> 31) & 1) << 20;
                let imm101 = ((instruction >> 21) & 0x3FF) << 1;
                let imm11 = ((instruction >> 20) & 1) << 11;
                let imm1912 = ((instruction >> 12) & 0xFF) << 12;
                decoded.imm = Some(imm20 | imm1912 | imm11 | imm101);
            }
            Opcode::Jalr => {
                decoded.format = InstructionFormat::IType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.imm = Some(((instruction >> 20) as i32) as u32 & 0xFFF);
                // 符号扩展
            }
            Opcode::Branch => {
                decoded.format = InstructionFormat::BType;
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                // B-type immediate: imm[12|10:5|4:1|11]
                let imm12 = ((instruction >> 31) & 1) << 12;
                let imm105 = ((instruction >> 25) & 0x3F) << 5;
                let imm41 = ((instruction >> 8) & 0xF) << 1;
                let imm11 = ((instruction >> 7) & 1) << 11;
                decoded.imm = Some(imm12 | imm105 | imm41 | imm11);
            }
            Opcode::Load | Opcode::Store => {
                decoded.format = if matches!(opcode, Opcode::Load) {
                    InstructionFormat::IType
                } else {
                    InstructionFormat::SType
                };
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rd = if matches!(opcode, Opcode::Load) {
                    Some(((instruction >> 7) & 0x1F) as u8)
                } else {
                    None
                };
                decoded.rs2 = if matches!(opcode, Opcode::Store) {
                    Some(((instruction >> 20) & 0x1F) as u8)
                } else {
                    None
                };
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.imm = Some(((instruction >> 20) as i32) as u32 & 0xFFF);
            }
            Opcode::OpImm => {
                decoded.format = InstructionFormat::IType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8); // Needed for SLLI/SRLI/SRAI
                decoded.imm = Some(((instruction >> 20) as i32) as u32 & 0xFFF);
            }
            Opcode::Op => {
                decoded.format = InstructionFormat::RType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8);
            }
            Opcode::MiscMem => {
                return Err(DecodeError::UnimplementedInstruction);
            }
            Opcode::System => {
                decoded.format = InstructionFormat::IType;
                // ECALL: imm[20:0] = 0, EBREAK: imm[20:0] = 1
                // Extract imm[20:0] from bits [31:20] (funct12) shifted
                // ECALL: funct7=0, rs2=0, funct3=0, rd=0 -> imm[20:0] = funct7 << 14 | rs2 << 9 | funct3 << 6 | rd << 1
                // Actually simpler: the imm[20:0] field is in bits [31:20] (12-bit funct7 in upper, etc)
                // For SYSTEM: bits [31:20] is the imm field
                let imm_20_0 = (instruction >> 20) & 0xFFF;
                decoded.imm = Some(imm_20_0);
            }
            Opcode::Amo => {
                decoded.format = InstructionFormat::RType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8);
            }
            // F extension opcodes
            Opcode::LoadFp => {
                decoded.format = InstructionFormat::IType;
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.imm = Some(((instruction >> 20) as i32) as u32 & 0xFFF);
            }
            Opcode::StoreFp => {
                decoded.format = InstructionFormat::SType;
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                // S-type immediate: imm[11:5] | imm[4:0]
                let imm11_5 = ((instruction >> 25) & 0x7F) << 5;
                let imm4_0 = ((instruction >> 7) & 0x1F) as u32;
                decoded.imm = Some(imm11_5 | imm4_0);
            }
            Opcode::OpFp => {
                // Check if this is an R4-type instruction (FMADD/FMSUB/FNMSUB/FNMADD)
                // R4-type has rs3 field in bits [31:27]
                let rs3_bits = (instruction >> 27) & 0x1F;
                if rs3_bits <= 4 {
                    // R4-type for FMA instructions
                    decoded.format = InstructionFormat::RType;
                    decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                    decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                    decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                    decoded.rs3 = Some(((instruction >> 27) & 0x1F) as u8);
                    decoded.funct3 =
                        Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                    decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8);
                } else {
                    // Standard R-type for other FPU operations
                    decoded.format = InstructionFormat::RType;
                    decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                    decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                    decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                    decoded.funct3 =
                        Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                    decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8);
                }
            }
            _ => {
                return Err(DecodeError::ReservedInstruction);
            }
        }

        Ok(decoded)
    }
}

impl Default for InstructionDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lui_decode() {
        // LUI x1, 0x12345
        // 格式: imm[31:12] | rd | opcode
        let instruction = (0x12345 << 12) | (1 << 7) | 0b011_0111;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::Lui);
        assert_eq!(decoded.rd, Some(1));
        assert_eq!(decoded.imm, Some(0x12345000));
    }

    #[test]
    fn test_add_decode() {
        // ADD x1, x2, x3
        // 格式: funct7 | rs2 | rs1 | funct3 | rd | opcode
        let instruction = (3 << 20) | (2 << 15) | (1 << 7) | 0b011_0011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::Op);
        assert_eq!(decoded.rd, Some(1));
        assert_eq!(decoded.rs1, Some(2));
        assert_eq!(decoded.rs2, Some(3));
        assert_eq!(decoded.funct3, Some(Funct3::AddSub));
    }

    #[test]
    fn test_ecall_decode() {
        // ECALL: 0b0000000_00000_000_00000_1110011
        // funct7=0, rs2=0, funct3=0, rd=0, opcode=SYSTEM
        let instruction = 0x00000073;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::System);
        assert_eq!(decoded.imm, Some(0));
    }

    #[test]
    fn test_ebreak_decode() {
        // EBREAK: 0b0000000_00001_000_00000_1110011
        // funct7=0, rs2=1, funct3=0, rd=0, opcode=SYSTEM
        let instruction = 0x00100073;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::System);
        assert_eq!(decoded.imm, Some(1));
    }
}
