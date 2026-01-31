# Code Generation Tools

This document describes the code generation infrastructure for RISC-V instruction implementations.

## Overview

The code generation tools use Rust procedural macros to automatically generate boilerplate code for RISC-V instructions, reducing manual coding and potential errors.

## Architecture

### Components

1. **ruscv-macros** crate: Procedural macro implementations
2. **src/codegen/**: Code generation templates and examples
3. **Template system**: Reusable instruction patterns

### Proc-Macro Crate (`ruscv-macros/`)

Location: `ruscv-macros/src/lib.rs`

Provides derive macros for automatic code generation:

#### `#[derive(RTypeExecutor)]`

Generates execution logic for R-type instructions (register-register).

**R-type format**: `funct7[31:25] rs2[24:20] rs1[19:15] funct3[14:12] rd[11:7] opcode[6:0]`

**Usage**:
```rust
#[derive(RTypeExecutor)]
struct AddInstruction {
    rd: u8,
    rs1: u8,
    rs2: u8,
}

impl AddInstruction {
    fn compute(&self, rs1_val: u64, rs2_val: u64) -> u64 {
        rs1_val.wrapping_add(rs2_val)
    }
}
```

**Generated code**:
- `execute()`: Execute instruction on core
- `decode()`: Decode from raw 32-bit instruction

#### `#[derive(ITypeExecutor)]`

Generates execution logic for I-type instructions (immediate operations).

**I-type format**: `imm[31:20] rs1[19:15] funct3[14:12] rd[11:7] opcode[6:0]`

**Usage**:
```rust
#[derive(ITypeExecutor)]
struct AddiInstruction {
    rd: u8,
    rs1: u8,
    imm: i16,
}

impl AddiInstruction {
    fn compute(&self, rs1_val: u64, imm_val: i64) -> u64 {
        (rs1_val as i64).wrapping_add(imm_val) as u64
    }
}
```

**Generated code**:
- `execute()`: Execute instruction with immediate
- `decode()`: Decode and sign-extend immediate

#### `instruction_batch!` macro (planned)

Generate multiple related instructions at once:

```rust
instruction_batch! {
    R_TYPE_ARITH {
        Add => add,
        Sub => sub,
        Sll => sll,
        Slt => slt,
    }
}
```

#### `instruction_set!` macro (planned)

Generate complete instruction set for an opcode group:

```rust
instruction_set! {
    opcode = 0b0110011,
    format = RType,
    instructions = {
        ADD: { funct3: 0b000, funct7: 0b0000000 },
        SUB: { funct3: 0b000, funct7: 0b0100000 },
    }
}
```

## Template System

### RTypeParams

Template for R-type instruction generation.

**Fields**:
- `name`: Instruction name
- `opcode`: Opcode bits
- `funct3`: Function code (3 bits)
- `funct7`: Function code (7 bits)
- `operation`: Human-readable operation

**Methods**:
- `encode(rd, rs1, rs2)`: Generate instruction encoding

### ITypeParams

Template for I-type instruction generation.

**Fields**:
- `name`: Instruction name
- `opcode`: Opcode bits
- `funct3`: Function code (3 bits)
- `operation`: Human-readable operation

**Methods**:
- `encode(rd, rs1, imm)`: Generate instruction encoding

### Standard Templates

Pre-defined templates for RV32I base instructions:

#### `rv32i_rtype_templates()`

Returns templates for all R-type RV32I instructions:
- ADD, SUB
- SLL, SRL, SRA
- SLT, SLTU
- XOR, OR, AND

#### `rv32i_itype_templates()`

Returns templates for all I-type RV32I instructions:
- ADDI, SLTI, SLTIU
- XORI, ORI, ANDI
- SLLI, SRLI, SRAI

## Usage Examples

### Using Templates

```rust
use ruscv_sim::codegen::template::{rv32i_rtype_templates, RTypeParams};

// Get all R-type templates
let templates = rv32i_rtype_templates();

// Generate ADD instruction
let add = &templates[0];
assert_eq!(add.name, "ADD");

// Encode: ADD x5, x10, x15
let inst = add.encode(5, 10, 15);
```

### Custom Instruction Generation

```rust
use ruscv_sim::codegen::template::RTypeParams;

// Define custom instruction
let custom = RTypeParams::new(
    "CUSTOM_ADD",
    0b0110011,
    0b000,
    0b0000001,
    "custom rs1 + rs2"
);

// Generate encoding
let inst = custom.encode(1, 2, 3);
```

## Benefits

1. **Reduced Boilerplate**: No manual field extraction
2. **Type Safety**: Compile-time checks
3. **Consistency**: Uniform structure across instructions
4. **Maintainability**: Single source of truth
5. **Documentation**: Self-documenting code

## Future Enhancements

### Phase 1 (Current)
- ✅ Basic derive macros (RType, IType)
- ✅ Template system
- ✅ Examples and tests

### Phase 2 (Planned)
- [ ] Full instruction_batch! macro
- [ ] Full instruction_set! macro
- [ ] S-type, B-type, U-type, J-type derives
- [ ] Automatic test generation

### Phase 3 (Future)
- [ ] Instruction fusion detection
- [ ] Optimization hints
- [ ] Vectorization support
- [ ] Custom instruction extensions

## Integration with Existing Code

The code generation tools are designed to complement (not replace) the existing decode/execute infrastructure:

1. **Decode module**: Continues to handle opcode dispatch
2. **Execute module**: Uses generated implementations
3. **Dispatch module**: Routes to generated executors

## Testing

All generated code includes tests:

```bash
# Test proc-macros
cargo test -p ruscv-macros

# Test templates
cargo test --lib codegen

# Test examples
cargo test --lib codegen::examples
```

## Performance

Code generation happens at compile time:
- Zero runtime overhead
- Fully optimized by compiler
- Inline-friendly structures

## Debugging

To see generated code:

```bash
# Expand macros
cargo expand --lib codegen

# Show proc-macro output
cargo rustc -- -Z macro-backtrace
```

## Dependencies

The code generation system requires:

```toml
[dependencies]
ruscv-macros = { path = "ruscv-macros" }

# In ruscv-macros/Cargo.toml
[dependencies]
syn = { version = "2.0", features = ["full"] }
quote = "1.0"
proc-macro2 = "1.0"
```

## Resources

- [Rust Proc-Macro Book](https://doc.rust-lang.org/reference/procedural-macros.html)
- [syn documentation](https://docs.rs/syn/)
- [quote documentation](https://docs.rs/quote/)
- [RISC-V ISA Manual](https://github.com/riscv/riscv-isa-manual)

## Contributing

When adding new instruction types:

1. Add derive macro to `ruscv-macros/src/lib.rs`
2. Create template in `src/codegen/template.rs`
3. Add example to `src/codegen/examples.rs`
4. Write tests for all components
5. Update this documentation

---

Last updated: 2026-01-31
Sprint: 4.5
