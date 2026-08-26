# Archived RV64M Implementation Note

> **Status:** Historical component documentation. It is not an end-to-end support or compliance claim.

This document describes the RV64M (RISC-V 64-bit Multiply/Divide) extension implementation for the ruscv-sim RISC-V Instruction Set Simulator.

## Overview

RV64M is the standard RISC-V 64-bit integer multiplication and division extension. It adds hardware support for integer multiply, divide, and remainder operations to the base RV64I architecture.

### Key Features

- Hardware multiplication (lower and upper 64-bit results)
- Signed and unsigned division
- Signed and unsigned remainder
- Overflow and division-by-zero handling per RISC-V specification

## Instruction Set

### Multiplication Instructions

| Mnemonic | funct7 | Description |
|----------|--------|-------------|
| MUL | 0000001 | Multiply (lower 64 bits) |
| MULH | 0000001 | Multiply signed × signed, upper 64 bits |
| MULHU | 0000001 | Multiply unsigned × unsigned, upper 64 bits |
| MULHSU | 0000001 | Multiply signed × unsigned, upper 64 bits |

**Format**: R-type
```
| funct7  | rs2 | rs1 | funct3 | rd | opcode  |
|---------|-----|-----|--------|----|---------|
| 7 bits  | 5   | 5   | 3      | 5  | 7       |
| 0000001 | rs2 | rs1 | 000    | rd | 0110011 |
```

**funct3 encoding**:
- MUL: `000` (same as ADD/SUB, distinguished by funct7)
- MULH: `001`
- MULHSU: `010`
- MULHU: `011`

### Division and Remainder Instructions

| Mnemonic | funct7 | Description |
|----------|--------|-------------|
| DIV | 0000001 | Divide signed |
| DIVU | 0000001 | Divide unsigned |
| REM | 0000001 | Remainder signed |
| REMU | 0000001 | Remainder unsigned |

**Format**: R-type
```
| funct7  | rs2 | rs1 | funct3 | rd | opcode  |
|---------|-----|-----|--------|----|---------|
| 7 bits  | 5   | 5   | 3      | 5  | 7       |
| 0000001 | rs2 | rs1 | xxx    | rd | 0110011 |
```

**funct3 encoding**:
- DIV: `100`
- DIVU: `101`
- REM: `110`
- REMU: `111`

## Implementation Details

### Multiplication Operations

All multiplication operations produce a 128-bit intermediate result, then select either the lower or upper 64 bits.

#### MUL - Multiply Lower
```
rd = (rs1 × rs2)[63:0]
```
- Performs signed 64-bit multiplication
- Returns lower 64 bits of the 128-bit result
- Uses `wrapping_mul` for defined overflow behavior

#### MULH - Multiply High (Signed × Signed)
```
rd = (rs1 × rs2)[127:64]
```
- Both operands treated as signed (i64)
- 128-bit intermediate result (i128)
- Returns upper 64 bits

#### MULHU - Multiply High Unsigned
```
rd = (rs1 × rs2)[127:64]
```
- Both operands treated as unsigned (u64)
- 128-bit intermediate result (u128)
- Returns upper 64 bits

#### MULHSU - Multiply High Signed × Unsigned
```
rd = (rs1 × rs2)[127:64]
```
- rs1 treated as signed (i64)
- rs2 treated as unsigned (u64)
- 128-bit intermediate result
- Returns upper 64 bits

### Division and Remainder Operations

#### DIV - Signed Division
```
rd = rs1 / rs2 (signed)
```

**Special Cases**:
| Condition | Result |
|-----------|--------|
| divisor = 0 | -1 (0xFFFFFFFFFFFFFFFF) |
| dividend = MIN, divisor = -1 | MIN (0x8000000000000000) |

The overflow case (MIN / -1) returns MIN to match x86 behavior and avoid undefined behavior.

#### DIVU - Unsigned Division
```
rd = rs1 / rs2 (unsigned)
```

**Special Case**:
| Condition | Result |
|-----------|--------|
| divisor = 0 | MAX (0xFFFFFFFFFFFFFFFF) |

#### REM - Signed Remainder
```
rd = rs1 % rs2 (signed)
```

**Special Cases**:
| Condition | Result |
|-----------|--------|
| divisor = 0 | dividend (unchanged) |
| dividend = MIN, divisor = -1 | 0 |

#### REMU - Unsigned Remainder
```
rd = rs1 % rs2 (unsigned)
```

**Special Case**:
| Condition | Result |
|-----------|--------|
| divisor = 0 | dividend (unchanged) |

### x0 Register Handling

As with all RISC-V instructions, when `rd = x0` (register 0), the destination register is not modified (always reads as 0). The operation is still performed, which may have side effects on memory or reservation state.

## Code Structure

### Source Files

- `src/execute/mul.rs`: Multiplication instructions (MUL, MULH, MULHU, MULHSU)
- `src/execute/div.rs`: Division and remainder instructions (DIV, DIVU, REM, REMU)

### Function Signatures

```rust
pub fn exec_mul(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError>

pub fn exec_div(
    instr: &DecodedInstruction,
    state: &mut CoreState,
    _mem: &mut dyn MemoryInterface,
) -> Result<(), ExecuteError>
```

## Testing

### Unit Tests

Tests are embedded in the source files using `#[cfg(test)]` modules.

#### Multiplication Tests (`mul.rs`)

| Test | Description |
|------|-------------|
| `test_mul_basic` | Basic multiplication (6 × 7 = 42) |
| `test_mul_negative` | Signed multiplication with negative operand |
| `test_mul_overflow` | Overflow handling (-1 × -1 = 1) |
| `test_mul_zero` | Multiplication by zero |
| `test_mul_x0_dest` | Destination register x0 handling |
| `test_mulh_basic` | Upper bits multiplication |
| `test_mulh_negative_result` | MULH with negative result |
| `test_mulhu_basic` | Unsigned high multiply |
| `test_mulhsu_basic` | Signed × unsigned high multiply |

#### Division Tests (`div.rs`)

| Test | Description |
|------|-------------|
| `test_div_basic` | Basic division (42 / 6 = 7) |
| `test_div_negative` | Signed division with negative operand |
| `test_div_by_zero` | Division by zero returns -1 |
| `test_div_overflow` | MIN / -1 overflow handling |
| `test_divu_basic` | Unsigned division |
| `test_divu_by_zero` | Unsigned division by zero returns MAX |
| `test_rem_basic` | Basic remainder |
| `test_rem_negative` | Signed remainder with negative operand |
| `test_rem_by_zero` | Remainder by zero returns dividend |
| `test_remu_basic` | Unsigned remainder |
| `test_div_min_dividend` | Minimum value division |

### Running Tests

```bash
# Run all RV64M tests
cargo test mul div

# Run specific test
cargo test test_mul_basic

# Run with output
cargo test test_div_overflow -- --nocapture
```

### Test Helper Functions

```rust
fn create_mul_instr(rs1: u8, rs2: u8, rd: u8, funct7: u8) -> DecodedInstruction
fn create_div_instr(rs1: u8, rs2: u8, rd: u8, funct7: u8) -> DecodedInstruction
```

## Edge Cases

### Multiplication Edge Cases

1. **Large numbers**: Values near 2^48 produce non-zero upper 64 bits
2. **Signed/unsigned mixing**: MULHSU handles signed × unsigned correctly
3. **Maximum values**: MIN_i64 × MIN_i64 produces 0 in lower 64 bits

### Division Edge Cases

1. **Division by zero**: Returns all 1s (signed: -1, unsigned: MAX)
2. **Overflow**: MIN / -1 returns MIN (not positive overflow)
3. **Negative divisors**: Remainder sign follows dividend (truncated division)

## Compliance

This implementation follows:
- **RISC-V ISA Manual Volume I**: Unprivileged ISA, Chapter 7 (M Extension)
- **Overflow behavior**: Matches x86 semantics for MIN / -1

## References

- [RISC-V ISA Manual](https://github.com/riscv/riscv-isa-manual)
- `src/execute/mul.rs` - Multiplication implementation
- `src/execute/div.rs` - Division implementation
