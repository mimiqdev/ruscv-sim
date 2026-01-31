//! Example usage of instruction generation macros
//!
//! This module demonstrates how to use the proc-macros to generate
//! instruction implementations with minimal boilerplate.

// Note: These are example templates showing how the macros would be used
// The actual generation is done at compile time by the proc-macro crate

/// Example R-type instruction template
///
/// Shows the structure that would be used with #[derive(RTypeExecutor)]
pub struct RTypeTemplate {
    pub rd: u8,
    pub rs1: u8,
    pub rs2: u8,
}

impl RTypeTemplate {
    /// Compute operation - this would be implemented by each specific instruction
    pub fn compute(&self, rs1_val: u64, rs2_val: u64) -> u64 {
        // Example: ADD operation
        rs1_val.wrapping_add(rs2_val)
    }
}

/// Example I-type instruction template
///
/// Shows the structure that would be used with #[derive(ITypeExecutor)]
pub struct ITypeTemplate {
    pub rd: u8,
    pub rs1: u8,
    pub imm: i16,
}

impl ITypeTemplate {
    /// Compute operation - this would be implemented by each specific instruction
    pub fn compute(&self, rs1_val: u64, imm_val: i64) -> u64 {
        // Example: ADDI operation
        (rs1_val as i64).wrapping_add(imm_val) as u64
    }
}

/// Macro usage examples
///
/// The following code shows how the macros would be used to generate
/// instruction implementations:
///
/// ```ignore
/// use ruscv_macros::{RTypeExecutor, ITypeExecutor};
///
/// #[derive(RTypeExecutor)]
/// struct AddInstruction {
///     rd: u8,
///     rs1: u8,
///     rs2: u8,
/// }
///
/// impl AddInstruction {
///     fn compute(&self, rs1_val: u64, rs2_val: u64) -> u64 {
///         rs1_val.wrapping_add(rs2_val)
///     }
/// }
///
/// #[derive(RTypeExecutor)]
/// struct SubInstruction {
///     rd: u8,
///     rs1: u8,
///     rs2: u8,
/// }
///
/// impl SubInstruction {
///     fn compute(&self, rs1_val: u64, rs2_val: u64) -> u64 {
///         rs1_val.wrapping_sub(rs2_val)
///     }
/// }
///
/// #[derive(ITypeExecutor)]
/// struct AddiInstruction {
///     rd: u8,
///     rs1: u8,
///     imm: i16,
/// }
///
/// impl AddiInstruction {
///     fn compute(&self, rs1_val: u64, imm_val: i64) -> u64 {
///         (rs1_val as i64).wrapping_add(imm_val) as u64
///     }
/// }
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtype_template_compute() {
        let template = RTypeTemplate {
            rd: 1,
            rs1: 2,
            rs2: 3,
        };
        let result = template.compute(100, 200);
        assert_eq!(result, 300);
    }

    #[test]
    fn test_itype_template_compute() {
        let template = ITypeTemplate {
            rd: 1,
            rs1: 2,
            imm: 42,
        };
        let result = template.compute(100, 42);
        assert_eq!(result, 142);
    }

    #[test]
    fn test_itype_template_negative_imm() {
        let template = ITypeTemplate {
            rd: 1,
            rs1: 2,
            imm: -10,
        };
        let result = template.compute(100, -10);
        assert_eq!(result, 90);
    }
}
