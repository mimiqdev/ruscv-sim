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
    RType,  // Register-Register
    R4Type, // Register-Register with a third source (FP fused multiply-add)
    IType,  // Immediate operation
    SType,  // Store
    BType,  // Conditional branch
    UType,  // Long immediate (LUI, AUIPC)
    JType,  // Unconditional jump (JAL)
}

/// RV32I Opcode (primary)
#[derive(Debug, Clone, Copy, TryFromPrimitive, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    Load = 0b000_0011,
    LoadFp = 0b000_0111,
    Store = 0b010_0011,
    StoreFp = 0b010_0111,
    Madd = 0b100_0011,
    Msub = 0b100_0111,
    Nmsub = 0b100_1011,
    Nmadd = 0b100_1111,
    MiscMem = 0b000_1111,
    OpImm = 0b001_0011,
    OpImm32 = 0b001_1011,
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

/// Floating-point operand format from the explicit `fmt` field.
///
/// The F and D profile implements `fmt = 00` (single) and `fmt = 01`
/// (double). `fmt = 10` and `fmt = 11` belong to Zfh and Q, which this
/// profile does not implement, so their encodings are reserved and raise
/// illegal-instruction (contract D2, D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FpFormat {
    /// `fmt = 00`, the F extension's single precision.
    Single,
    /// `fmt = 01`, the D extension's double precision.
    Double,
}

impl FpFormat {
    /// Decode the two-bit `fmt` field (OP-FP/R4 bits 26:25).
    ///
    /// Returns `None` for the out-of-profile Zfh and Q formats.
    pub fn from_encoding(bits: u8) -> Option<Self> {
        match bits & 0x3 {
            0 => Some(Self::Single),
            1 => Some(Self::Double),
            _ => None,
        }
    }

    /// The two-bit `fmt` encoding of this format.
    pub fn encoding(self) -> u8 {
        match self {
            Self::Single => 0,
            Self::Double => 1,
        }
    }

    /// True for the double-precision format (`fmt = 01`).
    pub fn is_double(self) -> bool {
        self == Self::Double
    }
}

/// Integer width selected by an `FCVT` instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntWidth {
    /// `W`: the low 32 bits of the integer register.
    Word,
    /// `L`: the full 64-bit integer register.
    Long,
}

/// Explicit F/D operation selected by an OP-FP or R4 encoding.
///
/// The operation is resolved at decode time so execution never re-reads
/// `funct7`, `rs2` or `funct3` to guess the instruction (contract D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FpOp {
    Add,
    Sub,
    Mul,
    Div,
    Sqrt,
    Madd,
    Msub,
    Nmsub,
    Nmadd,
    Sgnj,
    Sgnjn,
    Sgnjx,
    Min,
    Max,
    Eq,
    Lt,
    Le,
    Classify,
    /// `FCVT.W[U].S/D` and `FCVT.L[U].S/D`: floating-point to integer.
    CvtToInt {
        width: IntWidth,
        signed: bool,
    },
    /// `FCVT.S/D.W[U]` and `FCVT.S/D.L[U]`: integer to floating-point.
    CvtFromInt {
        width: IntWidth,
        signed: bool,
    },
    /// `FCVT.S.D` (narrowing).
    CvtToSingle,
    /// `FCVT.D.S` (widening, exact).
    CvtToDouble,
    /// `FMV.X.W` / `FMV.X.D`: an integer-register transfer out of an FPR.
    MoveToGpr,
    /// `FMV.W.X` / `FMV.D.X`: an integer-register transfer into an FPR.
    MoveFromGpr,
    /// `FLW` / `FLD`.
    Load,
    /// `FSW` / `FSD`.
    Store,
}

impl FpOp {
    /// True when the encoding carries an `rm` field that must resolve to an
    /// implemented rounding mode. A reserved static `rm` (5 or 6), or a
    /// dynamic `rm` while `frm` holds 5-7, is illegal-instruction
    /// (contract D5).
    pub fn requires_rounding(self) -> bool {
        matches!(
            self,
            Self::Add
                | Self::Sub
                | Self::Mul
                | Self::Div
                | Self::Sqrt
                | Self::Madd
                | Self::Msub
                | Self::Nmsub
                | Self::Nmadd
                | Self::CvtToInt { .. }
                | Self::CvtFromInt { .. }
                | Self::CvtToSingle
                | Self::CvtToDouble
        )
    }

    /// True when the retired instruction writes `rd` in the integer register
    /// file. These operations form the GPR destination fact of a commit.
    pub fn writes_gpr(self) -> bool {
        matches!(
            self,
            Self::Eq
                | Self::Lt
                | Self::Le
                | Self::Classify
                | Self::CvtToInt { .. }
                | Self::MoveToGpr
        )
    }
}

/// Explicit decode of one F/D instruction (contract U3a, D7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FpInstruction {
    /// Operand format from the explicit `fmt` field (OP-FP/R4 bits 26:25)
    /// or from the LOAD-FP/STORE-FP width in `funct3`.
    pub fmt: FpFormat,
    /// The raw `rm` field (bits 14:12) before dynamic resolution. It is
    /// present only for instructions that round; for FSGNJ*, FMIN/FMAX,
    /// compares, FCLASS and FMV those bits select the operation instead,
    /// and LOAD-FP/STORE-FP have no such field.
    pub rm: Option<u8>,
    /// The resolved operation.
    pub op: FpOp,
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
    /// Explicit F/D decode: operand format, raw `rm` and the resolved
    /// operation (contract U3a). `None` for every non-FP instruction.
    pub fp: Option<FpInstruction>,
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
            fp: None,
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
                decoded.imm = Some((instruction >> 20) as i32 as u32);
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
                // Load uses I-type immediate format [31:20], Store uses S-type format [31:25|11:7]
                decoded.imm = if matches!(opcode, Opcode::Load) {
                    Some(((instruction >> 20) as i32) as u32 & 0xFFF)
                } else {
                    // S-type immediate: imm[11:5] | imm[4:0]
                    let imm11_5 = ((instruction >> 25) & 0x7F) << 5;
                    let imm4_0 = (instruction >> 7) & 0x1F;
                    Some(imm11_5 | imm4_0)
                };
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
            Opcode::OpImm32 => {
                decoded.format = InstructionFormat::IType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8); // Needed for SLLIW/SRLIW/SRAIW
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
            Opcode::Op32 => {
                // Op32 uses R-type format: ADDW, SUBW, SLLW, SRLW, SRAW
                decoded.format = InstructionFormat::RType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.funct7 = Some(((instruction >> 25) & 0x7F) as u8);
            }
            Opcode::MiscMem => {
                // Base-I FENCE is funct3 = 0b000 and Zifencei FENCE.I is 0b001.
                // Other MISC-MEM funct3 values are reserved.
                let funct3_val = ((instruction >> 12) & 0x7) as u8;
                if funct3_val > 1 {
                    return Err(DecodeError::ReservedInstruction);
                }

                decoded.format = InstructionFormat::IType;
                // rs1/rd are reserved for future finer-grain fences and are
                // decoded here only for trace/disassembly completeness; base
                // FENCE execution ignores both.
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.funct3 = Some(Funct3::try_from(funct3_val).ok()).flatten();
                // Raw fm[31:28]|pred[27:24]|succ[23:20] field. Unlike other
                // I-type immediates this is a set of bit flags, not a signed
                // offset, so it must not be sign-extended.
                decoded.imm = Some((instruction >> 20) & 0xFFF);
            }
            Opcode::System => {
                decoded.format = InstructionFormat::IType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                // ECALL: imm[20:0] = 0, EBREAK: imm[20:0] = 1
                // For SYSTEM: bits [31:20] is the imm field (CSR address or function code)
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
            // F extension opcodes. The FP decode is explicit: LOAD-FP and
            // STORE-FP carry their width in `funct3`, and only FLW/FLD and
            // FSW/FSD exist in this profile. Any other width is a reserved
            // encoding, so it raises illegal-instruction rather than an
            // execution failure (contract D7).
            Opcode::LoadFp | Opcode::StoreFp => {
                let is_load = opcode == Opcode::LoadFp;
                decoded.format = if is_load {
                    InstructionFormat::IType
                } else {
                    InstructionFormat::SType
                };
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.funct3 =
                    Some(Funct3::try_from(((instruction >> 12) & 0x7) as u8).ok()).flatten();
                decoded.imm = if is_load {
                    Some(((instruction >> 20) as i32) as u32 & 0xFFF)
                } else {
                    // S-type immediate: imm[11:5] | imm[4:0]
                    let imm11_5 = ((instruction >> 25) & 0x7F) << 5;
                    let imm4_0 = (instruction >> 7) & 0x1F;
                    Some(imm11_5 | imm4_0)
                };
                let funct3 = ((instruction >> 12) & 0x7) as u8;
                let fmt = match funct3 {
                    0b010 => FpFormat::Single,
                    0b011 => FpFormat::Double,
                    _ => return Err(DecodeError::ReservedInstruction),
                };
                if is_load {
                    decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                } else {
                    decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                }
                decoded.fp = Some(FpInstruction {
                    fmt,
                    rm: None,
                    op: if is_load { FpOp::Load } else { FpOp::Store },
                });
            }
            Opcode::Madd | Opcode::Msub | Opcode::Nmsub | Opcode::Nmadd => {
                // R4 type: rd | rm | rs1 | rs2 | fmt | rs3 | opcode.
                decoded.format = InstructionFormat::R4Type;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                decoded.rs3 = Some(((instruction >> 27) & 0x1F) as u8);
                let fmt = FpFormat::from_encoding(((instruction >> 25) & 0x3) as u8)
                    .ok_or(DecodeError::ReservedInstruction)?;
                let op = match opcode {
                    Opcode::Madd => FpOp::Madd,
                    Opcode::Msub => FpOp::Msub,
                    Opcode::Nmsub => FpOp::Nmsub,
                    _ => FpOp::Nmadd,
                };
                decoded.fp = Some(FpInstruction {
                    fmt,
                    rm: Some(((instruction >> 12) & 0x7) as u8),
                    op,
                });
            }
            Opcode::OpFp => {
                decoded.format = InstructionFormat::RType;
                decoded.rd = Some(((instruction >> 7) & 0x1F) as u8);
                decoded.rs1 = Some(((instruction >> 15) & 0x1F) as u8);
                decoded.rs2 = Some(((instruction >> 20) & 0x1F) as u8);
                let funct3 = ((instruction >> 12) & 0x7) as u8;
                decoded.funct3 = Some(Funct3::try_from(funct3).ok()).flatten();
                let funct7 = ((instruction >> 25) & 0x7F) as u8;
                decoded.funct7 = Some(funct7);
                decoded.fp = Some(decode_op_fp(funct7, decoded.rs2.unwrap(), funct3)?);
            }
        }

        Ok(decoded)
    }
}

/// Decode one OP-FP word into its explicit format, raw `rm` and operation.
///
/// `funct7[6:2]` selects the operation and `funct7[1:0]` is the explicit
/// `fmt`. Bits 14:12 are the `rm` field for instructions that round and an
/// operation selector for the others. Every unassigned combination is a
/// reserved encoding (contract D7).
fn decode_op_fp(funct7: u8, rs2: u8, selector: u8) -> Result<FpInstruction, DecodeError> {
    let fmt = FpFormat::from_encoding(funct7 & 0x3).ok_or(DecodeError::ReservedInstruction)?;
    let opcode = funct7 >> 2;
    let (op, rounds) = match opcode {
        0x00 => (FpOp::Add, true),
        0x01 => (FpOp::Sub, true),
        0x02 => (FpOp::Mul, true),
        0x03 => (FpOp::Div, true),
        0x0b if rs2 == 0 => (FpOp::Sqrt, true),
        0x04 => match selector {
            0 => (FpOp::Sgnj, false),
            1 => (FpOp::Sgnjn, false),
            2 => (FpOp::Sgnjx, false),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        0x05 => match selector {
            0 => (FpOp::Min, false),
            1 => (FpOp::Max, false),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        0x14 => match selector {
            0 => (FpOp::Le, false),
            1 => (FpOp::Lt, false),
            2 => (FpOp::Eq, false),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        // FCVT.W[U].S/D and FCVT.L[U].S/D: rs2 selects width and signedness.
        0x18 => match rs2 {
            0 => (
                FpOp::CvtToInt {
                    width: IntWidth::Word,
                    signed: true,
                },
                true,
            ),
            1 => (
                FpOp::CvtToInt {
                    width: IntWidth::Word,
                    signed: false,
                },
                true,
            ),
            2 => (
                FpOp::CvtToInt {
                    width: IntWidth::Long,
                    signed: true,
                },
                true,
            ),
            3 => (
                FpOp::CvtToInt {
                    width: IntWidth::Long,
                    signed: false,
                },
                true,
            ),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        // FCVT.S/D.W[U] and FCVT.S/D.L[U].
        0x1a => match rs2 {
            0 => (
                FpOp::CvtFromInt {
                    width: IntWidth::Word,
                    signed: true,
                },
                true,
            ),
            1 => (
                FpOp::CvtFromInt {
                    width: IntWidth::Word,
                    signed: false,
                },
                true,
            ),
            2 => (
                FpOp::CvtFromInt {
                    width: IntWidth::Long,
                    signed: true,
                },
                true,
            ),
            3 => (
                FpOp::CvtFromInt {
                    width: IntWidth::Long,
                    signed: false,
                },
                true,
            ),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        // FCVT.S.D has rs2 = 1; FCVT.D.S has rs2 = 0. The `rs2` field names
        // the source format, so it must agree with `fmt`.
        0x08 => match (fmt, rs2) {
            (FpFormat::Single, 1) => (FpOp::CvtToSingle, true),
            (FpFormat::Double, 0) => (FpOp::CvtToDouble, true),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        // FMV.X.W/D and FCLASS.S/D: funct3 selects the operation.
        0x1c => match selector {
            0 if rs2 == 0 => (FpOp::MoveToGpr, false),
            1 if rs2 == 0 => (FpOp::Classify, false),
            _ => return Err(DecodeError::ReservedInstruction),
        },
        // FMV.W.X and FMV.D.X.
        0x1e if rs2 == 0 && selector == 0 => (FpOp::MoveFromGpr, false),
        _ => return Err(DecodeError::ReservedInstruction),
    };
    Ok(FpInstruction {
        fmt,
        rm: rounds.then_some(selector),
        op,
    })
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

    #[test]
    fn test_store_decode() {
        // SD x6, 5(x5) - S-type instruction
        // Format: imm[11:5] | rs2 | rs1 | funct3 | imm[4:0] | opcode
        // imm = 5, rs2 = 6, rs1 = 5, funct3 = 011 (SD), opcode = 0100011
        let instruction = (6 << 20) | (5 << 15) | (3 << 12) | (5 << 7) | 0b010_0011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::Store);
        assert_eq!(decoded.rs1, Some(5), "rs1 should be x5");
        assert_eq!(decoded.rs2, Some(6), "rs2 should be x6");
        assert_eq!(decoded.imm, Some(5), "immediate should be 5");
        assert_eq!(decoded.funct3, Some(Funct3::Sltu)); // funct3 = 0b011
        assert!(decoded.rd.is_none(), "Store should not have rd");
    }

    #[test]
    fn test_op_imm_32_decode() {
        // ADDIW x3, x4, 5 - I-type instruction
        // Format: imm[11:0] | rs1 | funct3 | rd | opcode
        // imm = 5, rs1 = 4, funct3 = 000 (ADDIW), rd = 3, opcode = 0011011
        let instruction = (5 << 20) | (4 << 15) | (3 << 7) | 0b001_1011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::OpImm32);
        assert_eq!(decoded.rd, Some(3), "rd should be x3");
        assert_eq!(decoded.rs1, Some(4), "rs1 should be x4");
        assert_eq!(decoded.imm, Some(5), "immediate should be 5");
        assert_eq!(decoded.funct3, Some(Funct3::AddSub));
    }

    #[test]
    fn test_op_32_decode() {
        // ADDW x1, x2, x3 - R-type instruction
        // Format: funct7 | rs2 | rs1 | funct3 | rd | opcode
        // funct7 = 0, rs2 = 3, rs1 = 2, funct3 = 000 (ADDW), rd = 1, opcode = 0111011
        let instruction = (3 << 20) | (2 << 15) | (1 << 7) | 0b011_1011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::Op32);
        assert_eq!(decoded.rd, Some(1), "rd should be x1");
        assert_eq!(decoded.rs1, Some(2), "rs1 should be x2");
        assert_eq!(decoded.rs2, Some(3), "rs2 should be x3");
        assert_eq!(decoded.funct3, Some(Funct3::AddSub));
        assert_eq!(decoded.funct7, Some(0));
    }

    #[test]
    fn test_subw_decode() {
        // SUBW x1, x2, x3 - R-type instruction
        // Format: funct7 | rs2 | rs1 | funct3 | rd | opcode
        // funct7 = 0x20, rs2 = 3, rs1 = 2, funct3 = 000 (SUBW), rd = 1, opcode = 0111011
        let instruction = (0x20 << 25) | (3 << 20) | (2 << 15) | (1 << 7) | 0b011_1011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::Op32);
        assert_eq!(decoded.rd, Some(1));
        assert_eq!(decoded.rs1, Some(2));
        assert_eq!(decoded.rs2, Some(3));
        assert_eq!(decoded.funct3, Some(Funct3::AddSub));
        assert_eq!(decoded.funct7, Some(0x20));
    }

    #[test]
    fn test_slliw_decode() {
        // SLLIW x2, x1, 4 - I-type instruction
        // Format: funct7 | shamt | rs1 | funct3 | rd | opcode
        // funct7 = 0, shamt = 4, rs1 = 1, funct3 = 001 (SLLIW), rd = 2, opcode = 0011011
        let instruction = (4 << 20) | (1 << 15) | (1 << 12) | (2 << 7) | 0b001_1011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::OpImm32);
        assert_eq!(decoded.rd, Some(2));
        assert_eq!(decoded.rs1, Some(1));
        assert_eq!(decoded.imm, Some(4));
        assert_eq!(decoded.funct3, Some(Funct3::Sll));
        assert_eq!(decoded.funct7, Some(0));
    }

    #[test]
    fn test_sraiw_decode() {
        // SRAIW x2, x1, 4 - I-type instruction
        // Format: funct7 | shamt | rs1 | funct3 | rd | opcode
        // funct7 = 0x20, shamt = 4, rs1 = 1, funct3 = 101 (SRAIW), rd = 2, opcode = 0011011
        let instruction = (0x20 << 25) | (4 << 20) | (1 << 15) | (5 << 12) | (2 << 7) | 0b001_1011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::OpImm32);
        assert_eq!(decoded.rd, Some(2));
        assert_eq!(decoded.rs1, Some(1));
        // imm field contains full 12-bit immediate: funct7 << 5 | shamt = 0x20 << 5 | 4 = 0x404 = 1028
        // The actual shamt (4) is extracted by exec_op_imm_32 using imm & 0x1F
        assert_eq!(decoded.imm, Some(0x404)); // 1028 = 0x20 << 5 | 4
        assert_eq!(decoded.funct3, Some(Funct3::SrlSra));
        assert_eq!(decoded.funct7, Some(0x20));
    }

    #[test]
    fn test_load_decode() {
        // LD x3, 8(x4) - I-type instruction
        // Format: imm[11:0] | rs1 | funct3 | rd | opcode
        // imm = 8, rs1 = 4, funct3 = 011 (LD), rd = 3, opcode = 0000011
        let instruction = (8 << 20) | (4 << 15) | (3 << 12) | (3 << 7) | 0b000_0011;
        let decoder = InstructionDecoder::new();
        let decoded = decoder.decode(instruction).unwrap();

        assert_eq!(decoded.opcode, Opcode::Load);
        assert_eq!(decoded.rs1, Some(4), "rs1 should be x4");
        assert_eq!(decoded.rd, Some(3), "rd should be x3");
        assert_eq!(decoded.imm, Some(8), "immediate should be 8");
        assert_eq!(decoded.funct3, Some(Funct3::Sltu)); // funct3 = 0b011
        assert!(decoded.rs2.is_none(), "Load should not have rs2");
    }

    // ============== F/D explicit decode (contract U3a, D7) ==============

    fn op_fp(funct7: u32, rs2: u32, rs1: u32, selector: u32, rd: u32) -> u32 {
        (funct7 << 25) | (rs2 << 20) | (rs1 << 15) | (selector << 12) | (rd << 7) | 0b101_0011
    }

    fn r4(opcode: u32, rs3: u32, fmt: u32, rs2: u32, rs1: u32, rm: u32, rd: u32) -> u32 {
        (rs3 << 27) | (fmt << 25) | (rs2 << 20) | (rs1 << 15) | (rm << 12) | (rd << 7) | opcode
    }

    fn fp_decode(raw: u32) -> FpInstruction {
        InstructionDecoder::new()
            .decode(raw)
            .unwrap()
            .fp
            .expect("an FP instruction carries an explicit decode")
    }

    #[test]
    fn fp_arithmetic_decode_is_explicit_about_fmt_and_rm() {
        // FADD.S fa0, fa1, fa2, RMM; FADD.D with the same fields.
        let s = fp_decode(op_fp(0x00, 12, 11, 4, 10));
        assert_eq!(s.fmt, FpFormat::Single);
        assert_eq!(s.rm, Some(4));
        assert_eq!(s.op, FpOp::Add);

        let d = fp_decode(op_fp(0x01, 12, 11, 4, 10));
        assert_eq!(d.fmt, FpFormat::Double);
        assert_eq!(d.rm, Some(4));
        assert_eq!(d.op, FpOp::Add);

        let sqrt = fp_decode(op_fp(0x2D, 0, 11, 2, 10));
        assert_eq!(
            (sqrt.fmt, sqrt.op, sqrt.rm),
            (FpFormat::Double, FpOp::Sqrt, Some(2))
        );
    }

    #[test]
    fn fp_non_rounding_selectors_are_not_reported_as_rm() {
        for (selector, op) in [(0, FpOp::Sgnj), (1, FpOp::Sgnjn), (2, FpOp::Sgnjx)] {
            let fp = fp_decode(op_fp(0x10, 12, 11, selector, 10));
            assert_eq!(fp.op, op);
            assert_eq!(fp.rm, None);
        }
        assert_eq!(fp_decode(op_fp(0x15, 12, 11, 1, 10)).op, FpOp::Max);
        assert_eq!(fp_decode(op_fp(0x51, 12, 11, 0, 10)).op, FpOp::Le);
        assert_eq!(fp_decode(op_fp(0x50, 12, 11, 1, 10)).op, FpOp::Lt);
        assert_eq!(fp_decode(op_fp(0x51, 12, 11, 2, 10)).op, FpOp::Eq);
        assert_eq!(fp_decode(op_fp(0x70, 0, 11, 1, 10)).op, FpOp::Classify);
        assert_eq!(fp_decode(op_fp(0x71, 0, 11, 0, 10)).op, FpOp::MoveToGpr);
        assert_eq!(fp_decode(op_fp(0x79, 0, 11, 0, 10)).op, FpOp::MoveFromGpr);
    }

    #[test]
    fn fp_conversions_decode_their_direction_and_width() {
        let to_int = fp_decode(op_fp(0x60, 1, 11, 3, 10));
        assert_eq!(
            to_int.op,
            FpOp::CvtToInt {
                width: IntWidth::Word,
                signed: false
            }
        );
        assert_eq!((to_int.fmt, to_int.rm), (FpFormat::Single, Some(3)));

        let from_int = fp_decode(op_fp(0x69, 2, 11, 0, 10));
        assert_eq!(
            from_int.op,
            FpOp::CvtFromInt {
                width: IntWidth::Long,
                signed: true
            }
        );
        assert_eq!((from_int.fmt, from_int.rm), (FpFormat::Double, Some(0)));

        let narrow = fp_decode(op_fp(0x20, 1, 11, 1, 10));
        assert_eq!(
            (narrow.op, narrow.fmt),
            (FpOp::CvtToSingle, FpFormat::Single)
        );
        let widen = fp_decode(op_fp(0x21, 0, 11, 2, 10));
        assert_eq!((widen.op, widen.fmt), (FpOp::CvtToDouble, FpFormat::Double));
    }

    #[test]
    fn fp_load_store_decode_uses_funct3_width() {
        let flw = InstructionDecoder::new()
            .decode((4 << 20) | (11 << 15) | (2 << 12) | (10 << 7) | 0b000_0111)
            .unwrap();
        assert_eq!(flw.opcode, Opcode::LoadFp);
        assert_eq!(flw.rd, Some(10));
        assert_eq!(flw.rs1, Some(11));
        assert_eq!(
            flw.fp,
            Some(FpInstruction {
                fmt: FpFormat::Single,
                rm: None,
                op: FpOp::Load
            })
        );
        let fsd = InstructionDecoder::new()
            .decode((4 << 25) | (12 << 20) | (11 << 15) | (3 << 12) | (8 << 7) | 0b010_0111)
            .unwrap();
        assert_eq!(fsd.rs2, Some(12));
        assert_eq!(fsd.imm, Some(4 << 5 | 8));
        assert_eq!(
            fsd.fp,
            Some(FpInstruction {
                fmt: FpFormat::Double,
                rm: None,
                op: FpOp::Store
            })
        );
    }

    #[test]
    fn r4_decode_carries_fmt_rm_and_rs3() {
        let fmadd = InstructionDecoder::new()
            .decode(r4(0b100_0011, 13, 0, 12, 11, 2, 10))
            .unwrap();
        assert_eq!(fmadd.opcode, Opcode::Madd);
        assert_eq!(fmadd.format, InstructionFormat::R4Type);
        assert_eq!(fmadd.rs3, Some(13));
        assert_eq!(
            fmadd.fp,
            Some(FpInstruction {
                fmt: FpFormat::Single,
                rm: Some(2),
                op: FpOp::Madd
            })
        );

        let nmsub = InstructionDecoder::new()
            .decode(r4(0b100_1011, 13, 1, 12, 11, 4, 10))
            .unwrap();
        assert_eq!(nmsub.opcode, Opcode::Nmsub);
        assert_eq!(
            nmsub.fp,
            Some(FpInstruction {
                fmt: FpFormat::Double,
                rm: Some(4),
                op: FpOp::Nmsub
            })
        );
    }

    #[test]
    fn reserved_fp_encodings_are_decode_errors() {
        let decoder = InstructionDecoder::new();
        for raw in [
            // fmt = 10 (Zfh) is out of profile.
            op_fp(0x02, 12, 11, 0, 10),
            // FSQRT with a nonzero rs2.
            op_fp(0x2C, 1, 11, 0, 10),
            // FSGNJ/FMIN/FCMP with a reserved selector.
            op_fp(0x10, 12, 11, 3, 10),
            op_fp(0x14, 12, 11, 2, 10),
            op_fp(0x50, 12, 11, 3, 10),
            // FCVT with an unassigned rs2, and a source/destination format
            // mismatch (FCVT.S.S is not an instruction).
            op_fp(0x60, 4, 11, 0, 10),
            op_fp(0x20, 0, 11, 0, 10),
            op_fp(0x21, 1, 11, 0, 10),
            // FCLASS/FMV with an unassigned selector or rs2.
            op_fp(0x70, 0, 11, 2, 10),
            op_fp(0x70, 1, 11, 0, 10),
            op_fp(0x78, 0, 11, 1, 10),
            // An unassigned OP-FP major operation.
            op_fp(0x06, 0, 11, 0, 10),
            // LOAD-FP/STORE-FP widths other than 4 and 8 bytes.
            0b000_0111 | (11 << 15) | (10 << 7),
            0b010_0111 | (12 << 20) | (11 << 15) | (1 << 12),
        ] {
            assert!(
                matches!(decoder.decode(raw), Err(DecodeError::ReservedInstruction)),
                "{raw:#010x} must be reserved"
            );
        }
        for raw in [
            r4(0b100_0011, 13, 2, 12, 11, 0, 10),
            r4(0b100_0111, 13, 3, 12, 11, 0, 10),
        ] {
            assert!(
                matches!(decoder.decode(raw), Err(DecodeError::ReservedInstruction)),
                "{raw:#010x} must be reserved"
            );
        }
    }

    #[test]
    fn reserved_rm_still_decodes_for_the_hart_to_trap() {
        // rm = 5 is a reserved static rounding mode, not a decode failure:
        // the Hart owns the trap so the encoding reaches the FP legality
        // check with the encoding intact (contract D5/D7).
        let fp = fp_decode(op_fp(0x00, 12, 11, 5, 10));
        assert_eq!(fp.rm, Some(5));
    }
}
