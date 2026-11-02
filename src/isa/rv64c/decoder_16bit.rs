//! RV64 Zca 16-bit compressed-instruction decoder.
//!
//! Classifies every 16-bit parcel and expands each legal Zca instruction to its
//! equivalent 32-bit encoding, which re-decodes through the existing
//! [`crate::decode::InstructionDecoder`] so the expansion carries identical
//! architectural semantics (contract D6, one execution engine).
//!
//! Classification follows the Selectable Zca Compressed-Instruction Profile
//! contract (`docs/dev-plan.md` D2) against the RISC-V ISA Unprivileged
//! Specification version 20250508, Chapter "C":
//!
//! - Legal Zca instructions and ordinary HINT-free forms expand to their
//!   32-bit equivalent.
//! - Exactly the contract's D2 HINT classes classify as [`ZcaClass::Hint`].
//! - Reserved encodings (including the all-zero parcel) classify as
//!   [`ZcaClass::Reserved`].
//! - C.FLD, C.FSD, C.FLDSP and C.FSDSP classify as [`ZcaClass::FpIllegal`]:
//!   without F/D their 32-bit FP counterparts are illegal, so the compressed
//!   forms are illegal too.
//!
//! This component is decode only; wiring compressed parcels into Hart fetch is
//! a later unit. The `exec_c_*` helpers in this module remain standalone and
//! are not an execution path.

use crate::decode::{DecodeError, DecodedInstruction, Funct3, Opcode};

/// Classification of a 16-bit parcel under Zca.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZcaClass {
    /// Legal Zca instruction; `instruction` holds the equivalent 32-bit
    /// encoding.
    Expansion { instruction: u32 },
    /// Contract D2 HINT encoding; retires with no architectural effect.
    /// `instruction` holds the equivalent 32-bit HINT expansion.
    Hint { instruction: u32 },
    /// Reserved encoding; raises illegal-instruction with the zero-extended
    /// 16-bit parcel in `mtval`/`stval`.
    Reserved,
    /// FP-compressed encoding (C.FLD/C.FSD/C.FLDSP/C.FSDSP); illegal under
    /// Zca because F/D is absent.
    FpIllegal,
}

/// Compressed instruction quadrant (C0, C1, C2)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CQuadrant {
    C0 = 0b00,
    C1 = 0b01,
    C2 = 0b10,
}

/// Compressed instruction operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum COpcode {
    // C0 Quadrant (00)
    CAddi4Spn, // Add immediate to sp (scaled)
    CLw,       // Load word
    CLd,       // Load double (RV64)
    CSw,       // Store word
    CSd,       // Store double (RV64)

    // C1 Quadrant (01)
    CAddi,     // Add immediate
    CAddiw,    // Add immediate word (RV64)
    CLi,       // Load immediate
    CLui,      // Load upper immediate
    CSrli,     // Shift right logical immediate
    CSrai,     // Shift right arithmetic immediate
    CAndi,     // AND immediate
    CSub,      // Subtract
    CXor,      // XOR
    COr,       // OR
    CAnd,      // AND
    CSubw,     // Subtract word (RV64)
    CAddw,     // Add word (RV64)
    CJ,        // Jump
    CJr,       // Jump register
    CJalr,     // Jump and link register
    CBeqz,     // Branch if equal zero
    CBnez,     // Branch not equal zero
    CAddi16Sp, // Add immediate to sp (16-bit scaled)

    // C2 Quadrant (10)
    CSlli,   // Shift left logical immediate
    CLwsp,   // Load word from stack pointer
    CLdsp,   // Load double from stack pointer (RV64)
    CSwsp,   // Store word to stack pointer
    CSdsp,   // Store double to stack pointer (RV64)
    CMv,     // Move
    CAdd,    // Add
    CEBreak, // Environment break
}

/// Compressed instruction decoder
#[derive(Debug, Clone)]
pub struct CompressedDecoder;

impl CompressedDecoder {
    /// Create a new compressed instruction decoder
    pub fn new() -> Self {
        Self
    }

    /// Classify a 16-bit parcel under the Zca profile (contract D2).
    pub fn classify_16bit(&self, parcel: u16) -> ZcaClass {
        match parcel & 0b11 {
            0b00 => self.classify_c0(parcel),
            0b01 => self.classify_c1(parcel),
            0b10 => self.classify_c2(parcel),
            _ => ZcaClass::Reserved,
        }
    }

    /// Decode a 16-bit compressed parcel into its expanded 32-bit form.
    ///
    /// HINT parcels decode to their 32-bit HINT expansion; reserved and
    /// FP-compressed parcels are decode errors.
    pub fn decode_16bit(&self, parcel: u16) -> Result<DecodedInstruction, DecodeError> {
        match self.classify_16bit(parcel) {
            ZcaClass::Expansion { instruction } | ZcaClass::Hint { instruction } => {
                crate::decode::InstructionDecoder::new().decode(instruction)
            }
            ZcaClass::Reserved | ZcaClass::FpIllegal => Err(DecodeError::ReservedInstruction),
        }
    }

    /// Check if instruction is compressed (16-bit)
    pub fn is_compressed(instruction: u32) -> bool {
        (instruction & 0b11) != 0b11
    }

    /// Get instruction length (16 or 32 bits)
    pub fn instruction_length(instruction: u32) -> u8 {
        if Self::is_compressed(instruction) {
            16
        } else {
            32
        }
    }

    // ============== Quadrant 0 (00): loads and stores ==============

    fn classify_c0(&self, parcel: u16) -> ZcaClass {
        let p = parcel as u32;
        match (p >> 13) & 0b111 {
            0b000 => {
                // C.ADDI4SPN: addi rd', x2, nzuimm. nzuimm=0 is reserved; the
                // rd'=x0 form with nonzero immediate is not one of D2's HINT
                // classes and expands to addi x0, x2, nzuimm, so it stays a
                // legal expansion.
                let nzuimm = c_addi4spn_imm(p);
                if nzuimm == 0 {
                    return ZcaClass::Reserved;
                }
                let rd = 8 + ((p >> 2) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm, rd, 2, Funct3::AddSub, nzuimm),
                }
            }
            0b001 | 0b101 => ZcaClass::FpIllegal, // C.FLD / C.FSD
            0b010 => {
                // C.LW: lw rd', imm(rs1')
                let imm = c_lw_imm(p);
                let rs1 = 8 + ((p >> 7) & 0b111);
                let rd = 8 + ((p >> 2) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::Load, rd, rs1, Funct3::Slt, imm),
                }
            }
            0b011 => {
                // C.LD (RV64): ld rd', imm(rs1')
                let imm = c_ld_imm(p);
                let rs1 = 8 + ((p >> 7) & 0b111);
                let rd = 8 + ((p >> 2) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::Load, rd, rs1, Funct3::Sltu, imm),
                }
            }
            0b100 => ZcaClass::Reserved,
            0b110 => {
                // C.SW: sw rs2', imm(rs1')
                let imm = c_lw_imm(p);
                let rs1 = 8 + ((p >> 7) & 0b111);
                let rs2 = 8 + ((p >> 2) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_s_type(Opcode::Store, rs1, rs2, Funct3::Slt, imm),
                }
            }
            _ => {
                // C.SD (RV64): sd rs2', imm(rs1')
                let imm = c_ld_imm(p);
                let rs1 = 8 + ((p >> 7) & 0b111);
                let rs2 = 8 + ((p >> 2) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_s_type(Opcode::Store, rs1, rs2, Funct3::Sltu, imm),
                }
            }
        }
    }

    // ============== Quadrant 1 (01): ALU, branches, jumps ==============

    fn classify_c1(&self, parcel: u16) -> ZcaClass {
        let p = parcel as u32;
        match (p >> 13) & 0b111 {
            0b000 => {
                // C.ADDI / C.NOP: addi rd, rd, imm. C.NOP (rd=0, imm=0) is an
                // ordinary NOP; C.NOP with nonzero immediate and C.ADDI with a
                // zero immediate are D2 HINTs.
                let rd = (p >> 7) & 0b11111;
                let imm = ci_imm(p);
                let instruction = build_i_type(Opcode::OpImm, rd, rd, Funct3::AddSub, imm);
                if (rd == 0) != (imm == 0) {
                    // C.NOP with a nonzero immediate, or C.ADDI with a zero
                    // immediate, is a D2 HINT; C.NOP itself is an ordinary NOP.
                    return ZcaClass::Hint { instruction };
                }
                ZcaClass::Expansion { instruction }
            }
            0b001 => {
                // C.ADDIW (RV64): addiw rd, rd, imm. rd=0 is reserved.
                let rd = (p >> 7) & 0b11111;
                if rd == 0 {
                    return ZcaClass::Reserved;
                }
                let imm = ci_imm(p);
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm32, rd, rd, Funct3::AddSub, imm),
                }
            }
            0b010 => {
                // C.LI: addi rd, x0, imm. rd=0 is a D2 HINT.
                let rd = (p >> 7) & 0b11111;
                let imm = ci_imm(p);
                if rd == 0 {
                    return ZcaClass::Hint {
                        instruction: build_i_type(Opcode::OpImm, rd, 0, Funct3::AddSub, imm),
                    };
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm, rd, 0, Funct3::AddSub, imm),
                }
            }
            0b011 => {
                // C.ADDI16SP (rd=2) / C.LUI: a zero immediate is reserved in
                // both (including rd=0), and C.LUI with rd=0 and a nonzero
                // immediate is a D2 HINT.
                let rd = (p >> 7) & 0b11111;
                if rd == 2 {
                    let imm = c_addi16sp_imm(p);
                    if imm == 0 {
                        return ZcaClass::Reserved;
                    }
                    ZcaClass::Expansion {
                        instruction: build_i_type(Opcode::OpImm, 2, 2, Funct3::AddSub, imm),
                    }
                } else {
                    let imm = c_lui_imm(p);
                    if imm == 0 {
                        return ZcaClass::Reserved;
                    }
                    let instruction = build_u_type(Opcode::Lui, rd, imm);
                    if rd == 0 {
                        return ZcaClass::Hint { instruction };
                    }
                    ZcaClass::Expansion { instruction }
                }
            }
            0b100 => self.classify_c1_alu(p),
            0b101 => {
                // C.J: jal x0, offset
                ZcaClass::Expansion {
                    instruction: build_j_type(Opcode::Jal, 0, c_j_imm(p)),
                }
            }
            0b110 => {
                // C.BEQZ: beq rs1', x0, offset
                let rs1 = 8 + ((p >> 7) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_b_type(Opcode::Branch, rs1, 0, Funct3::AddSub, c_b_imm(p)),
                }
            }
            _ => {
                // C.BNEZ: bne rs1', x0, offset
                let rs1 = 8 + ((p >> 7) & 0b111);
                ZcaClass::Expansion {
                    instruction: build_b_type(Opcode::Branch, rs1, 0, Funct3::Sll, c_b_imm(p)),
                }
            }
        }
    }

    /// C1 funct3=100 group: C.SRLI/C.SRAI/C.ANDI and the register ALU block.
    fn classify_c1_alu(&self, p: u32) -> ZcaClass {
        let rd = 8 + ((p >> 7) & 0b111);
        let rs2 = 8 + ((p >> 2) & 0b111);
        match (p >> 10) & 0b11 {
            0b00 => {
                // C.SRLI: srl rd', rd', shamt. Zero shift is a D2 HINT.
                let shamt = c_shamt(p);
                if shamt == 0 {
                    return ZcaClass::Hint {
                        instruction: build_i_type(Opcode::OpImm, rd, rd, Funct3::SrlSra, shamt),
                    };
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm, rd, rd, Funct3::SrlSra, shamt),
                }
            }
            0b01 => {
                // C.SRAI: srai rd', rd', shamt. Zero shift is a D2 HINT.
                let shamt = c_shamt(p);
                if shamt == 0 {
                    return ZcaClass::Hint {
                        instruction: build_i_type(
                            Opcode::OpImm,
                            rd,
                            rd,
                            Funct3::SrlSra,
                            shamt | 0x400,
                        ),
                    };
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm, rd, rd, Funct3::SrlSra, shamt | 0x400),
                }
            }
            0b10 => {
                // C.ANDI: andi rd', rd', imm
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm, rd, rd, Funct3::And, ci_imm(p)),
                }
            }
            _ => {
                // Register ALU block: bits[6:5] select the operation, bit 12
                // selects the W-suffixed row (RV64).
                let funct2 = (p >> 5) & 0b11;
                let op32 = (p >> 12) & 0b1;
                match (op32, funct2) {
                    (0b0, 0b00) => ZcaClass::Expansion {
                        instruction: build_r_type(
                            Opcode::Op,
                            rd,
                            rd,
                            rs2,
                            Funct3::AddSub,
                            0b0100000,
                        ),
                    },
                    (0b0, 0b01) => ZcaClass::Expansion {
                        instruction: build_r_type(Opcode::Op, rd, rd, rs2, Funct3::Xor, 0),
                    },
                    (0b0, 0b10) => ZcaClass::Expansion {
                        instruction: build_r_type(Opcode::Op, rd, rd, rs2, Funct3::Or, 0),
                    },
                    (0b0, 0b11) => ZcaClass::Expansion {
                        instruction: build_r_type(Opcode::Op, rd, rd, rs2, Funct3::And, 0),
                    },
                    (0b1, 0b00) => ZcaClass::Expansion {
                        instruction: build_r_type(
                            Opcode::Op32,
                            rd,
                            rd,
                            rs2,
                            Funct3::AddSub,
                            0b0100000,
                        ),
                    },
                    (0b1, 0b01) => ZcaClass::Expansion {
                        instruction: build_r_type(Opcode::Op32, rd, rd, rs2, Funct3::AddSub, 0),
                    },
                    _ => ZcaClass::Reserved,
                }
            }
        }
    }

    // ============== Quadrant 2 (10): register and stack ops ==============

    fn classify_c2(&self, parcel: u16) -> ZcaClass {
        let p = parcel as u32;
        match (p >> 13) & 0b111 {
            0b000 => {
                // C.SLLI: slli rd, rd, shamt. rd=0 and zero shift are D2 HINTs.
                let rd = (p >> 7) & 0b11111;
                let shamt = c_shamt(p);
                if rd == 0 || shamt == 0 {
                    return ZcaClass::Hint {
                        instruction: build_i_type(Opcode::OpImm, rd, rd, Funct3::Sll, shamt),
                    };
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::OpImm, rd, rd, Funct3::Sll, shamt),
                }
            }
            0b001 => ZcaClass::FpIllegal, // C.FLDSP
            0b011 => {
                // C.LDSP (RV64): ld rd, imm(x2). rd=0 is reserved.
                let rd = (p >> 7) & 0b11111;
                if rd == 0 {
                    return ZcaClass::Reserved;
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::Load, rd, 2, Funct3::Sltu, c_ldsp_imm(p)),
                }
            }
            0b010 => {
                // C.LWSP: lw rd, imm(x2). rd=0 is reserved.
                let rd = (p >> 7) & 0b11111;
                if rd == 0 {
                    return ZcaClass::Reserved;
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::Load, rd, 2, Funct3::Slt, c_lwsp_imm(p)),
                }
            }
            0b100 => self.classify_c2_mv_add(p),
            0b101 => ZcaClass::FpIllegal, // C.FSDSP
            0b110 => {
                // C.SWSP: sw rs2, imm(x2)
                ZcaClass::Expansion {
                    instruction: build_s_type(
                        Opcode::Store,
                        2,
                        (p >> 2) & 0b11111,
                        Funct3::Slt,
                        c_swsp_imm(p),
                    ),
                }
            }
            _ => {
                // C.SDSP (RV64): sd rs2, imm(x2)
                ZcaClass::Expansion {
                    instruction: build_s_type(
                        Opcode::Store,
                        2,
                        (p >> 2) & 0b11111,
                        Funct3::Sltu,
                        c_sdsp_imm(p),
                    ),
                }
            }
        }
    }

    /// C2 funct3=100 group: C.JR/C.JALR/C.MV/C.ADD/C.EBREAK.
    fn classify_c2_mv_add(&self, p: u32) -> ZcaClass {
        let rd = (p >> 7) & 0b11111;
        let rs2 = (p >> 2) & 0b11111;
        if (p >> 12) & 0b1 == 0 {
            if rs2 == 0 {
                // C.JR: jalr x0, 0(rs1). rs1=0 (rd=0) is reserved.
                if rd == 0 {
                    return ZcaClass::Reserved;
                }
                ZcaClass::Expansion {
                    instruction: build_i_type(Opcode::Jalr, 0, rd, Funct3::AddSub, 0),
                }
            } else {
                // C.MV: add rd, x0, rs2. rd=0 with rs2!=0 is a D2 HINT.
                if rd == 0 {
                    return ZcaClass::Hint {
                        instruction: build_r_type(Opcode::Op, rd, 0, rs2, Funct3::AddSub, 0),
                    };
                }
                ZcaClass::Expansion {
                    instruction: build_r_type(Opcode::Op, rd, 0, rs2, Funct3::AddSub, 0),
                }
            }
        } else if rs2 == 0 && rd == 0 {
            // C.EBREAK: ebreak
            ZcaClass::Expansion {
                instruction: build_i_type(Opcode::System, 0, 0, Funct3::AddSub, 1),
            }
        } else if rs2 == 0 {
            // C.JALR: jalr x1, 0(rs1)
            ZcaClass::Expansion {
                instruction: build_i_type(Opcode::Jalr, 1, rd, Funct3::AddSub, 0),
            }
        } else {
            // C.ADD: add rd, rd, rs2. rd=0 with rs2!=0 is a D2 HINT.
            if rd == 0 {
                return ZcaClass::Hint {
                    instruction: build_r_type(Opcode::Op, rd, rd, rs2, Funct3::AddSub, 0),
                };
            }
            ZcaClass::Expansion {
                instruction: build_r_type(Opcode::Op, rd, rd, rs2, Funct3::AddSub, 0),
            }
        }
    }
}

impl Default for CompressedDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============== Instruction builders ==============

fn build_i_type(opcode: Opcode, rd: u32, rs1: u32, funct3: Funct3, imm: u32) -> u32 {
    ((imm & 0xFFF) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((funct3 as u32) << 12)
        | ((rd & 0x1F) << 7)
        | (opcode as u32)
}

fn build_r_type(opcode: Opcode, rd: u32, rs1: u32, rs2: u32, funct3: Funct3, funct7: u32) -> u32 {
    ((funct7 & 0x7F) << 25)
        | ((rs2 & 0x1F) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((funct3 as u32) << 12)
        | ((rd & 0x1F) << 7)
        | (opcode as u32)
}

fn build_s_type(opcode: Opcode, rs1: u32, rs2: u32, funct3: Funct3, imm: u32) -> u32 {
    (((imm >> 5) & 0x7F) << 25)
        | ((rs2 & 0x1F) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((funct3 as u32) << 12)
        | ((imm & 0x1F) << 7)
        | (opcode as u32)
}

fn build_b_type(opcode: Opcode, rs1: u32, rs2: u32, funct3: Funct3, imm: u32) -> u32 {
    (((imm >> 12) & 0x1) << 31)
        | (((imm >> 5) & 0x3F) << 25)
        | ((rs2 & 0x1F) << 20)
        | ((rs1 & 0x1F) << 15)
        | ((funct3 as u32) << 12)
        | (((imm >> 1) & 0xF) << 8)
        | (((imm >> 11) & 0x1) << 7)
        | (opcode as u32)
}

fn build_u_type(opcode: Opcode, rd: u32, imm: u32) -> u32 {
    (imm & 0xFFFFF000) | ((rd & 0x1F) << 7) | (opcode as u32)
}

fn build_j_type(opcode: Opcode, rd: u32, imm: u32) -> u32 {
    (((imm >> 20) & 0x1) << 31)
        | (((imm >> 12) & 0xFF) << 12)
        | (((imm >> 11) & 0x1) << 20)
        | (((imm >> 1) & 0x3FF) << 21)
        | ((rd & 0x1F) << 7)
        | (opcode as u32)
}

// ============== Immediate extractors ==============

fn sext(value: u32, bits: u32) -> u32 {
    let sign_bit = 1 << (bits - 1);
    if value & sign_bit != 0 {
        value | !((sign_bit << 1) - 1)
    } else {
        value
    }
}

/// C.ADDI/C.LI/C.ANDI/C.ADDIW 6-bit signed immediate.
/// imm[5] <- parcel[12], imm[4:0] <- parcel[6:2].
fn ci_imm(p: u32) -> u32 {
    sext((((p >> 12) & 0x1) << 5) | ((p >> 2) & 0x1F), 6)
}

/// C.LUI 18-bit signed immediate (already shifted to bit 17:12).
/// imm[17] <- parcel[12], imm[16:12] <- parcel[6:2].
fn c_lui_imm(p: u32) -> u32 {
    sext((((p >> 12) & 0x1) << 17) | (((p >> 2) & 0x1F) << 12), 18)
}

/// C.ADDI16SP 10-bit signed immediate scaled by 16.
/// imm[9]<-p[12], imm[4]<-p[6], imm[6]<-p[5], imm[8:7]<-p[4:3], imm[5]<-p[2].
fn c_addi16sp_imm(p: u32) -> u32 {
    sext(
        (((p >> 12) & 0x1) << 9)
            | (((p >> 6) & 0x1) << 4)
            | (((p >> 5) & 0x1) << 6)
            | (((p >> 3) & 0x3) << 7)
            | (((p >> 2) & 0x1) << 5),
        10,
    )
}

/// C.ADDI4SPN unsigned immediate scaled by 4.
/// imm[5:4]<-p[12:11], imm[9:6]<-p[10:7], imm[3]<-p[5], imm[2]<-p[6].
fn c_addi4spn_imm(p: u32) -> u32 {
    (((p >> 11) & 0x3) << 4)
        | (((p >> 7) & 0xF) << 6)
        | (((p >> 5) & 0x1) << 3)
        | (((p >> 6) & 0x1) << 2)
}

/// C.LW/C.SW unsigned offset: imm[5:3]<-p[12:10], imm[2]<-p[6], imm[6]<-p[5].
fn c_lw_imm(p: u32) -> u32 {
    (((p >> 10) & 0x7) << 3) | (((p >> 6) & 0x1) << 2) | (((p >> 5) & 0x1) << 6)
}

/// C.LD/C.SD unsigned offset: imm[5:3]<-p[12:10], imm[7:6]<-p[6:5].
fn c_ld_imm(p: u32) -> u32 {
    (((p >> 10) & 0x7) << 3) | (((p >> 5) & 0x3) << 6)
}

/// C.LWSP unsigned offset: imm[5]<-p[12], imm[4:2]<-p[6:4], imm[7:6]<-p[3:2].
fn c_lwsp_imm(p: u32) -> u32 {
    (((p >> 12) & 0x1) << 5) | (((p >> 4) & 0x7) << 2) | (((p >> 2) & 0x3) << 6)
}

/// C.LDSP unsigned offset: imm[5]<-p[12], imm[4:3]<-p[6:5], imm[8:6]<-p[4:2].
fn c_ldsp_imm(p: u32) -> u32 {
    (((p >> 12) & 0x1) << 5) | (((p >> 5) & 0x3) << 3) | (((p >> 2) & 0x7) << 6)
}

/// C.SWSP unsigned offset: imm[5:2]<-p[12:9], imm[7:6]<-p[8:7].
fn c_swsp_imm(p: u32) -> u32 {
    (((p >> 9) & 0xF) << 2) | (((p >> 7) & 0x3) << 6)
}

/// C.SDSP unsigned offset: imm[5:3]<-p[12:10], imm[8:6]<-p[9:7].
fn c_sdsp_imm(p: u32) -> u32 {
    (((p >> 10) & 0x7) << 3) | (((p >> 7) & 0x7) << 6)
}

/// C.J 12-bit signed offset.
/// imm[11]<-p[12], imm[4]<-p[11], imm[9:8]<-p[10:9], imm[10]<-p[8],
/// imm[6]<-p[7], imm[7]<-p[6], imm[3:1]<-p[5:3], imm[5]<-p[2].
fn c_j_imm(p: u32) -> u32 {
    sext(
        (((p >> 12) & 0x1) << 11)
            | (((p >> 11) & 0x1) << 4)
            | (((p >> 9) & 0x3) << 8)
            | (((p >> 8) & 0x1) << 10)
            | (((p >> 7) & 0x1) << 6)
            | (((p >> 6) & 0x1) << 7)
            | (((p >> 3) & 0x7) << 1)
            | (((p >> 2) & 0x1) << 5),
        12,
    )
}

/// C.BEQZ/C.BNEZ 9-bit signed offset.
/// imm[8]<-p[12], imm[4:3]<-p[11:10], imm[7:6]<-p[6:5], imm[2:1]<-p[4:3],
/// imm[5]<-p[2].
fn c_b_imm(p: u32) -> u32 {
    sext(
        (((p >> 12) & 0x1) << 8)
            | (((p >> 10) & 0x3) << 3)
            | (((p >> 5) & 0x3) << 6)
            | (((p >> 3) & 0x3) << 1)
            | (((p >> 2) & 0x1) << 5),
        9,
    )
}

/// 6-bit shift amount for C.SLLI/C.SRLI/C.SRAI (RV64).
/// shamt[5] <- parcel[12], shamt[4:0] <- parcel[6:2].
fn c_shamt(p: u32) -> u32 {
    (((p >> 12) & 0x1) << 5) | ((p >> 2) & 0x1F)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_compressed() {
        assert!(CompressedDecoder::is_compressed(0x0000));
        assert!(CompressedDecoder::is_compressed(0x0001));
        assert!(CompressedDecoder::is_compressed(0x0002));
        assert!(!CompressedDecoder::is_compressed(0x0003));
        assert!(!CompressedDecoder::is_compressed(0xFFFFFFFF));
    }

    #[test]
    fn test_instruction_length() {
        assert_eq!(CompressedDecoder::instruction_length(0x0000), 16);
        assert_eq!(CompressedDecoder::instruction_length(0x0001), 16);
        assert_eq!(CompressedDecoder::instruction_length(0x0003), 32);
    }

    #[test]
    fn test_c_nop_is_legal_and_hints_decode() {
        let decoder = CompressedDecoder::new();
        // C.NOP (rd=0, imm=0) is an ordinary legal NOP.
        assert_eq!(
            decoder.classify_16bit(0x0001),
            ZcaClass::Expansion {
                instruction: 0x00000013
            }
        );
        // C.NOP with nonzero immediate is a D2 HINT.
        assert!(matches!(
            decoder.classify_16bit(0x0021), // c.nop 8
            ZcaClass::Hint { .. }
        ));
        // Both decode through the expansion.
        assert!(decoder.decode_16bit(0x0001).is_ok());
        assert!(decoder.decode_16bit(0x0021).is_ok());
    }

    #[test]
    fn test_decode_c_addi_valid() {
        let decoder = CompressedDecoder::new();
        // C.ADDI x1, 1: funct3=000, rd=1, imm=1
        let inst: u16 = 0b0000_0000_1000_0101;
        let result = decoder.decode_16bit(inst);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded.opcode, Opcode::OpImm);
        assert_eq!(decoded.rd, Some(1));
        assert_eq!(decoded.rs1, Some(1));
        assert_eq!(decoded.imm, Some(1));
    }

    #[test]
    fn test_sext() {
        assert_eq!(sext(0b0_0101, 5), 5);
        assert_eq!(sext(0b1_1111, 5), u32::MAX); // -1
        assert_eq!(sext(0b1_0000, 5), 0xFFFFFFF0); // -16
        assert_eq!(sext(0b0_11111, 6), 31);
        assert_eq!(sext(0b111_1111, 7), u32::MAX); // -1
        assert_eq!(sext(0b1000_0000, 8), 0xFFFFFF80); // -128
    }
}
