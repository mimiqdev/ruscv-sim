//! RV64C 16-bit Compressed Instruction Decoder
//!
//! Implements decoding of RISC-V compressed (C) extension instructions.
//! Compressed instructions are 16-bit versions of common 32-bit RV64I operations.
//!
//! Reference: RISC-V ISA Manual Volume I, Chapter 16

use crate::decode::{DecodeError, DecodedInstruction, Funct3, Opcode};

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
    CLw,       // Load word
    CLd,       // Load double (RV64)
    CSw,       // Store word
    CSd,       // Store double (RV64)
    CFld,      // Load float double (RV64D)
    CFlw,      // Load float word (RV64F)
    CFsd,      // Store float double (RV64D)
    CFsw,      // Store float word (RV64F)
    CAddi4Spn, // Add immediate to sp (scaled)

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
    CNop,    // No operation
}

/// Compressed instruction decoder
#[derive(Debug, Clone)]
pub struct CompressedDecoder;

impl CompressedDecoder {
    /// Create a new compressed instruction decoder
    pub fn new() -> Self {
        Self
    }

    /// Decode a 16-bit compressed instruction
    /// Returns the expanded 32-bit equivalent instruction
    pub fn decode_16bit(&self, compressed: u16) -> Result<DecodedInstruction, DecodeError> {
        let quadrant = (compressed & 0b11) as u8;

        match quadrant {
            0b00 => self.decode_c0_quadrant(compressed),
            0b01 => self.decode_c1_quadrant(compressed),
            0b10 => self.decode_c2_quadrant(compressed),
            _ => Err(DecodeError::InvalidInstruction(compressed as u32)),
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

    /// Decode C0 quadrant (00) - Load/Store
    fn decode_c0_quadrant(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let funct3 = (inst >> 13) & 0b111;

        match funct3 {
            0b000 => self.decode_c_addi4spn(inst),
            0b001 => self.decode_c_fld(inst),  // C.FLD (RV64D)
            0b010 => self.decode_c_lw(inst),   // C.LW
            0b011 => self.decode_c_flw(inst),  // C.FLW (RV64F)
            0b100 => Err(DecodeError::ReservedInstruction),
            0b101 => self.decode_c_fsd(inst),  // C.FSD (RV64D)
            0b110 => self.decode_c_sw(inst),   // C.SW
            0b111 => self.decode_c_fsw(inst),  // C.FSW (RV64F)
            _ => Err(DecodeError::ReservedInstruction),
        }
    }

    /// Decode C1 quadrant (01) - Arithmetic, Branches, Jump
    fn decode_c1_quadrant(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let funct3 = (inst >> 13) & 0b111;

        match funct3 {
            0b000 => self.decode_c_addi(inst),
            0b001 => self.decode_c_addiw(inst),
            0b010 => self.decode_c_li(inst),
            0b011 => self.decode_c_lui_addi16sp(inst),
            0b100 => self.decode_c_alu(inst),
            0b101 => self.decode_c_j(inst),
            0b110 => self.decode_c_beqz(inst),
            0b111 => self.decode_c_bnez(inst),
            _ => Err(DecodeError::ReservedInstruction),
        }
    }

    /// Decode C2 quadrant (10) - Register-based ops, Stack access
    fn decode_c2_quadrant(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let funct3 = (inst >> 13) & 0b111;

        match funct3 {
            0b000 => self.decode_c_slli(inst),
            0b001 => self.decode_c_ldsp(inst),
            0b010 => self.decode_c_lwsp(inst),
            0b011 => self.decode_c_ldsp(inst), // c.ldsp for RV64
            0b100 => self.decode_c_mv_add(inst),
            0b101 => self.decode_c_sdsp(inst),
            0b110 => self.decode_c_swsp(inst),
            0b111 => self.decode_c_sdsp(inst), // c.sdsp for RV64
            _ => Err(DecodeError::ReservedInstruction),
        }
    }

    // ============== C0 Quadrant Decoders ==============

    /// C.ADDI4SPN - Add immediate to stack pointer (x2), scaled by 4
    /// Expands to: addi rd', x2, imm
    fn decode_c_addi4spn(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // inst[12:5] contains non-zero immediate bits
        let nzimm = ((inst >> 5) & 0xFF) as u32;
        if nzimm == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        // imm = { 2'b00, inst[10:7], inst[12:11], inst[5], inst[6], 2'b00 }
        let imm = (((inst >> 4) & 0x100) | // bit 8 -> becomes bit 8
                   ((inst >> 7) & 0x030) | // bits 4:3 -> bits 5:4
                   ((inst << 1) & 0x040) | // bit 5 -> bit 6
                   ((inst >> 2) & 0x00C) | // bits 10:9 -> bits 3:2
                   ((inst << 4) & 0x300)) as u32; // bits 7:6 -> bits 9:8

        let rd_prime = ((inst >> 2) & 0x7) as u8 + 8; // rd' = 8-15

        // Build 32-bit ADDI: addi rd', x2, imm
        let expanded = self.build_i_type(Opcode::OpImm, rd_prime, 2, Funct3::AddSub, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.LW - Load word (compressed)
    /// Expands to: lw rd', imm(rs1')
    fn decode_c_lw(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 5'b0, inst[5], inst[12:10], inst[6], 2'b00 }
        let imm = (((inst >> 4) & 0x40) |  // bit 6 -> becomes bit 6
                   ((inst >> 7) & 0x038) | // bits 5:3 -> bits 5:3
                   ((inst << 1) & 0x080)) as u32; // bit 5 -> bit 7

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8; // rs1' = 8-15
        let rd_prime = ((inst >> 2) & 0x7) as u8 + 8; // rd' = 8-15

        // Build 32-bit LW: lw rd', imm(rs1')
        let expanded = self.build_i_type(Opcode::Load, rd_prime, rs1_prime, Funct3::Sll, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.LD - Load doubleword (RV64)
    /// Expands to: ld rd', imm(rs1')
    fn decode_c_ld(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 4'b0, inst[6:5], inst[12:10], 3'b000 }
        let imm = (((inst >> 7) & 0x038) | // inst[12:10] -> imm[5:3]
                   ((inst << 1) & 0x0C0)) as u32; // inst[6:5] -> imm[7:6]

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8;
        let rd_prime = ((inst >> 2) & 0x7) as u8 + 8;

        // Build 32-bit LD: ld rd', imm(rs1')
        let expanded = self.build_i_type(Opcode::Load, rd_prime, rs1_prime, Funct3::Slt, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.SW - Store word (compressed)
    /// Expands to: sw rs2', imm(rs1')
    fn decode_c_sw(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 5'b0, inst[5], inst[12:10], inst[6], 2'b00 }
        let imm = (((inst >> 4) & 0x40) | ((inst >> 7) & 0x038) | ((inst << 1) & 0x080)) as u32;

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8;
        let rs2_prime = ((inst >> 2) & 0x7) as u8 + 8;

        // Build 32-bit SW: sw rs2', imm(rs1')
        let expanded = self.build_s_type(Opcode::Store, rs1_prime, rs2_prime, Funct3::Sll, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.SD - Store doubleword (RV64)
    /// Expands to: sd rs2', imm(rs1')
    fn decode_c_sd(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 4'b0, inst[6:5], inst[12:10], 3'b000 }
        let imm = (((inst >> 7) & 0x038) | ((inst << 1) & 0x0C0)) as u32;

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8;
        let rs2_prime = ((inst >> 2) & 0x7) as u8 + 8;

        // Build 32-bit SD: sd rs2', imm(rs1')
        let expanded = self.build_s_type(Opcode::Store, rs1_prime, rs2_prime, Funct3::Slt, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.FLD - Load double-precision floating-point (compressed)
    /// Expands to: fld rd', imm(rs1')
    fn decode_c_fld(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 4'b0, inst[6:5], inst[12:10], 3'b000 }
        let imm = (((inst >> 7) & 0x038) | // inst[12:10] -> imm[5:3]
                   ((inst << 1) & 0x0C0)) as u32; // inst[6:5] -> imm[7:6]

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8; // rs1' = 8-15
        let rd_prime = ((inst >> 2) & 0x7) as u8 + 8;  // rd' = 8-15 (FP register)

        // Build 32-bit FLD: fld rd', imm(rs1')
        // FLD is I-type: opcode=000_0111 (LoadFp), funct3=011 (D)
        let expanded = self.build_i_type(Opcode::LoadFp, rd_prime, rs1_prime, Funct3::Sltu, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.FLW - Load single-precision floating-point (compressed)
    /// Expands to: flw rd', imm(rs1')
    fn decode_c_flw(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 5'b0, inst[5], inst[12:10], inst[6], 2'b00 }
        let imm = (((inst >> 4) & 0x40) |  // bit 6 -> becomes bit 6
                   ((inst >> 7) & 0x038) | // bits 5:3 -> bits 5:3
                   ((inst << 1) & 0x080)) as u32; // bit 5 -> bit 7

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8; // rs1' = 8-15
        let rd_prime = ((inst >> 2) & 0x7) as u8 + 8;  // rd' = 8-15 (FP register)

        // Build 32-bit FLW: flw rd', imm(rs1')
        // FLW is I-type: opcode=000_0111 (LoadFp), funct3=010 (W)
        let expanded = self.build_i_type(Opcode::LoadFp, rd_prime, rs1_prime, Funct3::Slt, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.FSD - Store double-precision floating-point (compressed)
    /// Expands to: fsd rs2', imm(rs1')
    fn decode_c_fsd(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 4'b0, inst[6:5], inst[12:10], 3'b000 }
        let imm = (((inst >> 7) & 0x038) | // inst[12:10] -> imm[5:3]
                   ((inst << 1) & 0x0C0)) as u32; // inst[6:5] -> imm[7:6]

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8; // rs1' = 8-15
        let rs2_prime = ((inst >> 2) & 0x7) as u8 + 8; // rs2' = 8-15 (FP register)

        // Build 32-bit FSD: fsd rs2', imm(rs1')
        // FSD is S-type: opcode=010_0111 (StoreFp), funct3=011 (D)
        let expanded = self.build_s_type(Opcode::StoreFp, rs1_prime, rs2_prime, Funct3::Sltu, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.FSW - Store single-precision floating-point (compressed)
    /// Expands to: fsw rs2', imm(rs1')
    fn decode_c_fsw(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        // imm = { 5'b0, inst[5], inst[12:10], inst[6], 2'b00 }
        let imm = (((inst >> 4) & 0x40) |  // bit 6 -> becomes bit 6
                   ((inst >> 7) & 0x038) | // bits 5:3 -> bits 5:3
                   ((inst << 1) & 0x080)) as u32; // bit 5 -> bit 7

        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8; // rs1' = 8-15
        let rs2_prime = ((inst >> 2) & 0x7) as u8 + 8; // rs2' = 8-15 (FP register)

        // Build 32-bit FSW: fsw rs2', imm(rs1')
        // FSW is S-type: opcode=010_0111 (StoreFp), funct3=010 (W)
        let expanded = self.build_s_type(Opcode::StoreFp, rs1_prime, rs2_prime, Funct3::Slt, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    // ============== C1 Quadrant Decoders ==============

    /// C.ADDI - Add immediate (compressed)
    /// Expands to: addi rd, rd, nzimm[5:0]
    fn decode_c_addi(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let nzimm = self.extract_c_imm(inst);
        let rd = ((inst >> 7) & 0x1F) as u8;

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        let expanded = self.build_i_type(Opcode::OpImm, rd, rd, Funct3::AddSub, nzimm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.ADDIW - Add immediate word (RV64)
    /// Expands to: addiw rd, rd, imm[5:0]
    fn decode_c_addiw(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let imm = self.extract_c_imm(inst);
        let rd = ((inst >> 7) & 0x1F) as u8;

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        let expanded = self.build_i_type(Opcode::Op32, rd, rd, Funct3::AddSub, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.LI - Load immediate
    /// Expands to: addi rd, x0, imm[5:0]
    fn decode_c_li(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let imm = self.extract_c_imm(inst);
        let rd = ((inst >> 7) & 0x1F) as u8;

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        let expanded = self.build_i_type(Opcode::OpImm, rd, 0, Funct3::AddSub, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.LUI / C.ADDI16SP
    fn decode_c_lui_addi16sp(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rd = ((inst >> 7) & 0x1F) as u8;
        let nzimm = self.extract_c_lui_imm(inst);

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        if rd == 2 {
            // C.ADDI16SP - Add immediate to stack pointer
            if nzimm == 0 {
                return Err(DecodeError::ReservedInstruction);
            }
            // addi x2, x2, nzimm
            let expanded = self.build_i_type(Opcode::OpImm, 2, 2, Funct3::AddSub, nzimm);
            let decoder = crate::decode::InstructionDecoder::new();
            decoder.decode(expanded)
        } else {
            // C.LUI - Load upper immediate
            if nzimm == 0 {
                return Err(DecodeError::ReservedInstruction);
            }
            let expanded = self.build_u_type(Opcode::Lui, rd, nzimm);
            let decoder = crate::decode::InstructionDecoder::new();
            decoder.decode(expanded)
        }
    }

    /// C.ALU - ALU operations in C1 quadrant
    fn decode_c_alu(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let op_type = (inst >> 10) & 0b11;
        let rs2_prime = ((inst >> 2) & 0x7) as u8 + 8;
        let rd_prime = ((inst >> 7) & 0x7) as u8 + 8;

        match op_type {
            0b00 => {
                // C.SRLI - Shift right logical immediate
                let imm = self.extract_c_shamt(inst, false);
                let expanded =
                    self.build_i_type(Opcode::OpImm, rd_prime, rd_prime, Funct3::SrlSra, imm);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            }
            0b01 => {
                // C.SRAI - Shift right arithmetic immediate
                let imm = self.extract_c_shamt(inst, true);
                let expanded =
                    self.build_i_type(Opcode::OpImm, rd_prime, rd_prime, Funct3::SrlSra, imm);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            }
            0b10 => {
                // C.ANDI - AND immediate
                let imm = self.extract_c_imm(inst);
                let expanded =
                    self.build_i_type(Opcode::OpImm, rd_prime, rd_prime, Funct3::And, imm);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            }
            0b11 => {
                // More ALU ops: C.SUB, C.XOR, C.OR, C.AND, C.SUBW, C.ADDW
                let funct2 = (inst >> 5) & 0b11;
                let funct1 = (inst >> 12) & 0b1;

                match (funct1, funct2) {
                    (0b0, 0b00) => {
                        // C.SUB
                        let expanded = self.build_r_type(
                            Opcode::Op,
                            rd_prime,
                            rd_prime,
                            rs2_prime,
                            Funct3::AddSub,
                            0b0100000,
                        );
                        let decoder = crate::decode::InstructionDecoder::new();
                        decoder.decode(expanded)
                    }
                    (0b0, 0b01) => {
                        // C.XOR
                        let expanded = self.build_r_type(
                            Opcode::Op,
                            rd_prime,
                            rd_prime,
                            rs2_prime,
                            Funct3::Xor,
                            0,
                        );
                        let decoder = crate::decode::InstructionDecoder::new();
                        decoder.decode(expanded)
                    }
                    (0b0, 0b10) => {
                        // C.OR
                        let expanded = self.build_r_type(
                            Opcode::Op,
                            rd_prime,
                            rd_prime,
                            rs2_prime,
                            Funct3::Or,
                            0,
                        );
                        let decoder = crate::decode::InstructionDecoder::new();
                        decoder.decode(expanded)
                    }
                    (0b0, 0b11) => {
                        // C.AND
                        let expanded = self.build_r_type(
                            Opcode::Op,
                            rd_prime,
                            rd_prime,
                            rs2_prime,
                            Funct3::And,
                            0,
                        );
                        let decoder = crate::decode::InstructionDecoder::new();
                        decoder.decode(expanded)
                    }
                    (0b1, 0b00) => {
                        // C.SUBW (RV64)
                        let expanded = self.build_r_type(
                            Opcode::Op32,
                            rd_prime,
                            rd_prime,
                            rs2_prime,
                            Funct3::AddSub,
                            0b0100000,
                        );
                        let decoder = crate::decode::InstructionDecoder::new();
                        decoder.decode(expanded)
                    }
                    (0b1, 0b01) => {
                        // C.ADDW (RV64)
                        let expanded = self.build_r_type(
                            Opcode::Op32,
                            rd_prime,
                            rd_prime,
                            rs2_prime,
                            Funct3::AddSub,
                            0,
                        );
                        let decoder = crate::decode::InstructionDecoder::new();
                        decoder.decode(expanded)
                    }
                    _ => Err(DecodeError::ReservedInstruction),
                }
            }
            _ => Err(DecodeError::ReservedInstruction),
        }
    }

    /// C.J - Unconditional jump
    fn decode_c_j(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let imm = self.extract_c_j_imm(inst);
        // jal x0, offset (unconditional jump)
        let expanded = self.build_j_type(Opcode::Jal, 0, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.BEQZ - Branch if equal to zero
    fn decode_c_beqz(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let imm = self.extract_c_b_imm(inst);
        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8;
        // beq rs1', x0, offset
        let expanded = self.build_b_type(Opcode::Branch, rs1_prime, 0, Funct3::AddSub, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.BNEZ - Branch if not equal to zero
    fn decode_c_bnez(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let imm = self.extract_c_b_imm(inst);
        let rs1_prime = ((inst >> 7) & 0x7) as u8 + 8;
        // bne rs1', x0, offset
        let expanded = self.build_b_type(Opcode::Branch, rs1_prime, 0, Funct3::Sll, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    // ============== C2 Quadrant Decoders ==============

    /// C.SLLI - Shift left logical immediate
    fn decode_c_slli(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rd = ((inst >> 7) & 0x1F) as u8;

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        let imm = self.extract_c_shamt(inst, false);
        let expanded = self.build_i_type(Opcode::OpImm, rd, rd, Funct3::Sll, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.LWSP - Load word from stack pointer
    fn decode_c_lwsp(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rd = ((inst >> 7) & 0x1F) as u8;

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        // imm = { 4'b0, inst[3:2], inst[12], inst[6:4], 2'b00 }
        let imm = (((inst >> 7) & 0x020) | ((inst >> 2) & 0x01C) | ((inst << 4) & 0x0C0)) as u32;

        let expanded = self.build_i_type(Opcode::Load, rd, 2, Funct3::Sll, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.LDSP - Load doubleword from stack pointer (RV64)
    fn decode_c_ldsp(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rd = ((inst >> 7) & 0x1F) as u8;

        if rd == 0 {
            return Err(DecodeError::ReservedInstruction);
        }

        // imm = { 3'b0, inst[4:2], inst[12], inst[6:5], 3'b000 }
        let imm = (((inst >> 7) & 0x018) | ((inst >> 2) & 0x007) | ((inst << 4) & 0x0C0)) as u32;

        let expanded = self.build_i_type(Opcode::Load, rd, 2, Funct3::Slt, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.MV / C.ADD / C.JR / C.JALR / C.EBREAK
    fn decode_c_mv_add(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rd = ((inst >> 7) & 0x1F) as u8;
        let rs2 = ((inst >> 2) & 0x1F) as u8;

        if (inst >> 12) & 1 == 0 {
            // C.MV or C.JR
            if rs2 == 0 {
                // C.JR - Jump register
                if rd == 0 {
                    return Err(DecodeError::ReservedInstruction);
                }
                // jalr x0, 0(rs1)
                let expanded = self.build_i_type(Opcode::Jalr, 0, rd, Funct3::AddSub, 0);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            } else {
                // C.MV - Move
                // add rd, x0, rs2
                let expanded = self.build_r_type(Opcode::Op, rd, 0, rs2, Funct3::AddSub, 0);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            }
        } else {
            // C.ADD or C.JALR or C.EBREAK
            if rs2 == 0 && rd == 0 {
                // C.EBREAK
                let expanded = 0x00100073u32; // ebreak
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            } else if rs2 == 0 {
                // C.JALR - Jump and link register
                // jalr x1, 0(rs1)
                let expanded = self.build_i_type(Opcode::Jalr, 1, rd, Funct3::AddSub, 0);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            } else {
                // C.ADD
                // add rd, rd, rs2
                let expanded = self.build_r_type(Opcode::Op, rd, rd, rs2, Funct3::AddSub, 0);
                let decoder = crate::decode::InstructionDecoder::new();
                decoder.decode(expanded)
            }
        }
    }

    /// C.SWSP - Store word to stack pointer
    fn decode_c_swsp(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rs2 = ((inst >> 2) & 0x1F) as u8;

        // imm = { 4'b0, inst[8:7], inst[12:9], 2'b00 }
        let imm = (((inst >> 9) & 0x00C) | ((inst >> 1) & 0x060) | ((inst >> 4) & 0x180)) as u32;

        let expanded = self.build_s_type(Opcode::Store, 2, rs2, Funct3::Sll, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    /// C.SDSP - Store doubleword to stack pointer (RV64)
    fn decode_c_sdsp(&self, inst: u16) -> Result<DecodedInstruction, DecodeError> {
        let rs2 = ((inst >> 2) & 0x1F) as u8;

        // imm = { 3'b0, inst[9:7], inst[12:10], 3'b000 }
        let imm = (((inst >> 10) & 0x006) | ((inst >> 1) & 0x038) | ((inst >> 4) & 0x180)) as u32;

        let expanded = self.build_s_type(Opcode::Store, 2, rs2, Funct3::Slt, imm);
        let decoder = crate::decode::InstructionDecoder::new();
        decoder.decode(expanded)
    }

    // ============== Instruction Builders ==============

    /// Build I-type instruction
    fn build_i_type(&self, opcode: Opcode, rd: u8, rs1: u8, funct3: Funct3, imm: u32) -> u32 {
        ((imm & 0xFFF) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | (opcode as u32)
    }

    /// Build R-type instruction
    fn build_r_type(
        &self,
        opcode: Opcode,
        rd: u8,
        rs1: u8,
        rs2: u8,
        funct3: Funct3,
        funct7: u8,
    ) -> u32 {
        ((funct7 as u32) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | (opcode as u32)
    }

    /// Build S-type instruction
    fn build_s_type(&self, opcode: Opcode, rs1: u8, rs2: u8, funct3: Funct3, imm: u32) -> u32 {
        let imm_11_5 = (imm >> 5) & 0x7F;
        let imm_4_0 = imm & 0x1F;
        (imm_11_5 << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | (imm_4_0 << 7)
            | (opcode as u32)
    }

    /// Build B-type instruction
    fn build_b_type(&self, opcode: Opcode, rs1: u8, rs2: u8, funct3: Funct3, imm: u32) -> u32 {
        let imm_12 = (imm >> 12) & 1;
        let imm_10_5 = (imm >> 5) & 0x3F;
        let imm_4_1 = (imm >> 1) & 0xF;
        let imm_11 = (imm >> 11) & 1;

        (imm_12 << 31)
            | (imm_10_5 << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((funct3 as u32) << 12)
            | (imm_4_1 << 8)
            | (imm_11 << 7)
            | (opcode as u32)
    }

    /// Build U-type instruction
    fn build_u_type(&self, opcode: Opcode, rd: u8, imm: u32) -> u32 {
        (imm & 0xFFFFF000) | ((rd as u32) << 7) | (opcode as u32)
    }

    /// Build J-type instruction
    fn build_j_type(&self, opcode: Opcode, rd: u8, imm: u32) -> u32 {
        let imm_20 = (imm >> 20) & 1;
        let imm_10_1 = (imm >> 1) & 0x3FF;
        let imm_11 = (imm >> 11) & 1;
        let imm_19_12 = (imm >> 12) & 0xFF;

        (imm_20 << 31)
            | (imm_19_12 << 12)
            | (imm_11 << 20)
            | (imm_10_1 << 21)
            | ((rd as u32) << 7)
            | (opcode as u32)
    }

    // ============== Immediate Extractors ==============

    /// Extract 6-bit signed immediate for C.ADDI, C.LI, etc.
    fn extract_c_imm(&self, inst: u16) -> u32 {
        let imm5 = ((inst >> 12) & 1) as u32;
        let imm4_0 = ((inst >> 2) & 0x1F) as u32;
        let imm = (imm5 << 5) | imm4_0;

        // Sign extend
        if imm5 != 0 {
            imm | 0xFFFFFFC0 // Sign extend to 32 bits
        } else {
            imm
        }
    }

    /// Extract 6-bit unsigned immediate for shifts (C.SRLI, C.SRAI, C.SLLI)
    fn extract_c_shamt(&self, inst: u16, is_arith: bool) -> u32 {
        let shamt5 = ((inst >> 12) & 1) as u32;
        let shamt4_0 = ((inst >> 2) & 0x1F) as u32;
        let shamt = (shamt5 << 5) | shamt4_0;

        // For arithmetic shifts, set funct7 bit
        if is_arith {
            shamt | (0x20 << 5) // Set funct7[5] for SRAI
        } else {
            shamt
        }
    }

    /// Extract 18-bit signed immediate for C.LUI
    fn extract_c_lui_imm(&self, inst: u16) -> u32 {
        let nzimm17 = ((inst >> 12) & 1) as u32;
        let nzimm16_12 = ((inst >> 2) & 0x1F) as u32;
        let imm = (nzimm17 << 17) | (nzimm16_12 << 12);

        // Sign extend
        if nzimm17 != 0 {
            imm | 0xFFFC0000
        } else {
            imm
        }
    }

    /// Extract 12-bit signed immediate for C.J
    fn extract_c_j_imm(&self, inst: u16) -> u32 {
        let imm5 = ((inst >> 12) & 1) as u32;
        let imm3_1 = ((inst >> 3) & 0x7) as u32;
        let imm7 = ((inst >> 2) & 1) as u32;
        let imm6 = ((inst >> 7) & 1) as u32;
        let imm10 = ((inst >> 8) & 1) as u32;
        let imm9_8 = ((inst >> 9) & 0x3) as u32;
        let imm4 = ((inst >> 11) & 1) as u32;
        let imm11 = ((inst >> 12) & 1) as u32;

        let imm = (imm11 << 11)
            | (imm10 << 10)
            | (imm9_8 << 8)
            | (imm7 << 7)
            | (imm6 << 6)
            | (imm5 << 5)
            | (imm4 << 4)
            | (imm3_1 << 1);

        // Sign extend
        if imm11 != 0 {
            imm | 0xFFFFF000
        } else {
            imm
        }
    }

    /// Extract 9-bit signed immediate for C.BEQZ/C.BNEZ
    fn extract_c_b_imm(&self, inst: u16) -> u32 {
        let imm5 = ((inst >> 12) & 1) as u32;
        let imm2_1 = ((inst >> 3) & 0x3) as u32;
        let imm7_6 = ((inst >> 5) & 0x3) as u32;
        let imm4_3 = ((inst >> 10) & 0x3) as u32;
        let imm8 = ((inst >> 12) & 1) as u32;

        let imm = (imm8 << 8) | (imm7_6 << 6) | (imm5 << 5) | (imm4_3 << 3) | (imm2_1 << 1);

        // Sign extend
        if imm8 != 0 {
            imm | 0xFFFFFE00
        } else {
            imm
        }
    }
}

impl Default for CompressedDecoder {
    fn default() -> Self {
        Self::new()
    }
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
    fn test_decode_c_nop() {
        let decoder = CompressedDecoder::new();
        // C.NOP is encoded as c.addi x0, 0 which is reserved (hints)
        // Let's test a simple instruction instead
        // C.ADDI x1, 1: funct3=000, rd=1, nzimm=1 -> 0x0001
        let result = decoder.decode_16bit(0x0021); // c.addi x1, 0 is reserved
                                                   // This should fail since nzimm=0 is reserved for C.ADDI
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_c_addi_valid() {
        let decoder = CompressedDecoder::new();
        // C.ADDI x1, 1: rd=1, nzimm[5:0]=1
        // C.ADDI is in C1 quadrant (01)
        // Encoded: funct3=000, inst[12]=nzimm[5], inst[11:7]=rd, inst[6:2]=nzimm[4:0], inst[1:0]=01
        // Binary: 000_0_00001_00001_01 = 0x0085
        let inst: u16 = 0b0000_0000_1000_0101;
        let result = decoder.decode_16bit(inst);
        assert!(result.is_ok());
        let decoded = result.unwrap();
        assert_eq!(decoded.opcode, Opcode::OpImm);
        assert_eq!(decoded.rd, Some(1));
        assert_eq!(decoded.rs1, Some(1));
    }
}
