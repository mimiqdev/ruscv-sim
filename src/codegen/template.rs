//! Instruction generation templates
//!
//! This module provides template structures and functions for generating
//! RISC-V instruction implementations.

/// R-type instruction template parameters
#[derive(Debug, Clone)]
pub struct RTypeParams {
    pub name: String,
    pub opcode: u8,
    pub funct3: u8,
    pub funct7: u8,
    pub operation: String,
}

impl RTypeParams {
    /// Create new R-type parameters
    pub fn new(name: &str, opcode: u8, funct3: u8, funct7: u8, operation: &str) -> Self {
        Self {
            name: name.to_string(),
            opcode,
            funct3,
            funct7,
            operation: operation.to_string(),
        }
    }

    /// Generate instruction encoding
    pub fn encode(&self, rd: u8, rs1: u8, rs2: u8) -> u32 {
        ((self.funct7 as u32) << 25)
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | (self.opcode as u32)
    }
}

/// I-type instruction template parameters
#[derive(Debug, Clone)]
pub struct ITypeParams {
    pub name: String,
    pub opcode: u8,
    pub funct3: u8,
    pub operation: String,
}

impl ITypeParams {
    /// Create new I-type parameters
    pub fn new(name: &str, opcode: u8, funct3: u8, operation: &str) -> Self {
        Self {
            name: name.to_string(),
            opcode,
            funct3,
            operation: operation.to_string(),
        }
    }

    /// Generate instruction encoding
    pub fn encode(&self, rd: u8, rs1: u8, imm: i16) -> u32 {
        let imm_bits = (imm as u32) & 0xFFF;
        (imm_bits << 20)
            | ((rs1 as u32) << 15)
            | ((self.funct3 as u32) << 12)
            | ((rd as u32) << 7)
            | (self.opcode as u32)
    }
}

/// Standard RV32I R-type instruction templates
pub fn rv32i_rtype_templates() -> Vec<RTypeParams> {
    vec![
        RTypeParams::new("ADD", 0b0110011, 0b000, 0b0000000, "rs1 + rs2"),
        RTypeParams::new("SUB", 0b0110011, 0b000, 0b0100000, "rs1 - rs2"),
        RTypeParams::new("SLL", 0b0110011, 0b001, 0b0000000, "rs1 << rs2[4:0]"),
        RTypeParams::new("SLT", 0b0110011, 0b010, 0b0000000, "rs1 <s rs2"),
        RTypeParams::new("SLTU", 0b0110011, 0b011, 0b0000000, "rs1 <u rs2"),
        RTypeParams::new("XOR", 0b0110011, 0b100, 0b0000000, "rs1 ^ rs2"),
        RTypeParams::new("SRL", 0b0110011, 0b101, 0b0000000, "rs1 >>u rs2[4:0]"),
        RTypeParams::new("SRA", 0b0110011, 0b101, 0b0100000, "rs1 >>s rs2[4:0]"),
        RTypeParams::new("OR", 0b0110011, 0b110, 0b0000000, "rs1 | rs2"),
        RTypeParams::new("AND", 0b0110011, 0b111, 0b0000000, "rs1 & rs2"),
    ]
}

/// Standard RV32I I-type instruction templates
pub fn rv32i_itype_templates() -> Vec<ITypeParams> {
    vec![
        ITypeParams::new("ADDI", 0b0010011, 0b000, "rs1 + imm"),
        ITypeParams::new("SLTI", 0b0010011, 0b010, "rs1 <s imm"),
        ITypeParams::new("SLTIU", 0b0010011, 0b011, "rs1 <u imm"),
        ITypeParams::new("XORI", 0b0010011, 0b100, "rs1 ^ imm"),
        ITypeParams::new("ORI", 0b0010011, 0b110, "rs1 | imm"),
        ITypeParams::new("ANDI", 0b0010011, 0b111, "rs1 & imm"),
        ITypeParams::new("SLLI", 0b0010011, 0b001, "rs1 << imm[4:0]"),
        ITypeParams::new("SRLI", 0b0010011, 0b101, "rs1 >>u imm[4:0]"),
        ITypeParams::new("SRAI", 0b0010011, 0b101, "rs1 >>s imm[4:0]"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtype_encode_add() {
        let add = RTypeParams::new("ADD", 0b0110011, 0b000, 0b0000000, "rs1 + rs2");
        // ADD x1, x2, x3
        let encoded = add.encode(1, 2, 3);
        assert_eq!(encoded & 0x7F, 0b0110011); // opcode
        assert_eq!((encoded >> 7) & 0x1F, 1); // rd
        assert_eq!((encoded >> 12) & 0x7, 0b000); // funct3
        assert_eq!((encoded >> 15) & 0x1F, 2); // rs1
        assert_eq!((encoded >> 20) & 0x1F, 3); // rs2
        assert_eq!((encoded >> 25) & 0x7F, 0b0000000); // funct7
    }

    #[test]
    fn test_itype_encode_addi() {
        let addi = ITypeParams::new("ADDI", 0b0010011, 0b000, "rs1 + imm");
        // ADDI x1, x2, 100
        let encoded = addi.encode(1, 2, 100);
        assert_eq!(encoded & 0x7F, 0b0010011); // opcode
        assert_eq!((encoded >> 7) & 0x1F, 1); // rd
        assert_eq!((encoded >> 12) & 0x7, 0b000); // funct3
        assert_eq!((encoded >> 15) & 0x1F, 2); // rs1
        assert_eq!(((encoded >> 20) & 0xFFF) as i16, 100); // imm
    }

    #[test]
    fn test_itype_encode_negative_imm() {
        let addi = ITypeParams::new("ADDI", 0b0010011, 0b000, "rs1 + imm");
        // ADDI x1, x2, -100
        let encoded = addi.encode(1, 2, -100);
        let imm = ((encoded >> 20) as i32) << 20 >> 20; // Sign extend
        assert_eq!(imm, -100);
    }

    #[test]
    fn test_rv32i_templates() {
        let rtype = rv32i_rtype_templates();
        assert_eq!(rtype.len(), 10);
        assert_eq!(rtype[0].name, "ADD");
        assert_eq!(rtype[1].name, "SUB");

        let itype = rv32i_itype_templates();
        assert_eq!(itype.len(), 9);
        assert_eq!(itype[0].name, "ADDI");
        assert_eq!(itype[1].name, "SLTI");
    }
}
