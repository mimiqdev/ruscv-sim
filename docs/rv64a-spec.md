# RV64A Specification Documentation

This document describes the RV64A (RISC-V 64-bit Atomic Operations) extension implementation for the ruscv-sim RISC-V Instruction Set Simulator.

## Overview

RV64A is the standard RISC-V 64-bit atomic instructions extension. It provides primitives for synchronization between multiple harts (hardware threads) through atomic memory operations.

### Key Features

- **Load-Reserved/Store-Conditional (LR/SC)**: Atomic read-modify-write primitives
- **Atomic Memory Operations (AMO)**: Read-modify-write operations for synchronization
- **Memory ordering**: Acquire (aq) and Release (rl) bits for memory consistency

## Instruction Set

### Load-Reserved / Store-Conditional

| Mnemonic | funct5 | Description |
|----------|--------|-------------|
| LR.W | 00010 | Load-Reserved 32-bit |
| LR.D | 00010 | Load-Reserved 64-bit |
| SC.W | 00011 | Store-Conditional 32-bit |
| SC.D | 00011 | Store-Conditional 64-bit |

**Format**: R-type
```
| funct5 | aq | rl | rs2 | rs1 | funct3 | rd | opcode  |
|--------|----|----|-----|-----|--------|----|---------|
| 5 bits | 1  | 1  | 5   | 5   | 3      | 5  | 7       |
```

**Encoding Details**:
- LR: `rs2 = 00000` (not used)
- SC: `rs2` contains value to store
- `funct3 = 010` (32-bit) or `011` (64-bit)
- `opcode = 0101111` (AMO)

### Atomic Memory Operations

| Mnemonic | funct5 | Description |
|----------|--------|-------------|
| AMOADD.W | 00000 | Atomic Add 32-bit |
| AMOSWAP.W | 00001 | Atomic Swap 32-bit |
| AMOXOR.W | 00100 | Atomic XOR 32-bit |
| AMOOR.W | 00110 | Atomic OR 32-bit |
| AMOAND.W | 00011 | Atomic AND 32-bit |
| AMOMIN.W | 01000 | Atomic Min Signed 32-bit |
| AMOMAX.W | 01010 | Atomic Max Signed 32-bit |
| AMOMINU.W | 01001 | Atomic Min Unsigned 32-bit |
| AMOMAXU.W | 01011 | Atomic Max Unsigned 32-bit |

**Format**: R-type
```
| funct5 | aq | rl | rs2 | rs1 | funct3 | rd | opcode  |
|--------|----|----|-----|-----|--------|----|---------|
| 5 bits | 1  | 1  | 5   | 5   | 3      | 5  | 7       |
```

## Implementation Details

### Load-Reserved (LR)

The LR instruction:
1. Loads a value from memory at address `rs1`
2. Creates a reservation on that address
3. Returns the loaded value to `rd`

**32-bit variant (LR.W)**:
```
value = MEM[rs1][31:0]
rd = sign_extend(value)
create_reservation(rs1)
```

**64-bit variant (LR.D)**:
```
rd = MEM[rs1][63:0]
create_reservation(rs1)
```

### Store-Conditional (SC)

The SC instruction:
1. Checks if a valid reservation exists for address `rs1`
2. If valid: stores `rs2` to memory, returns 0 to `rd`
3. If invalid: does not store, returns non-zero to `rd`
4. Clears the reservation regardless of success

**Operation**:
```
if has_reservation(rs1):
    MEM[rs1] = rs2
    rd = 0          // Success
else:
    rd = 1          // Failure
clear_reservation()
```

### Reservation Set

The `ReservationSet` struct tracks the reservation state:

```rust
pub struct ReservationSet {
    reserved_addr: Option<u64>,
}
```

**Methods**:
- `has_reservation(addr)`: Check if address has active reservation
- `reserve(addr)`: Create reservation for address
- `clear()`: Clear all reservations
- `clear_if_matching(addr)`: Clear only if matching address

**Current Limitation**: This implementation uses a global reservation singleton. In a production multi-core system, reservations must be per-hart.

### Atomic Memory Operations

All AMO instructions follow the same pattern:
1. Read current value from memory at `rs1`
2. Compute new value based on operation
3. Store new value to memory
4. Return original value to `rd` (sign-extended for 32-bit)

#### AMOADD - Atomic Add
```
temp = MEM[rs1]
MEM[rs1] = temp + rs2
rd = sign_extend(temp)
```

#### AMOAND - Atomic AND
```
temp = MEM[rs1]
MEM[rs1] = temp & rs2
rd = sign_extend(temp)
```

#### AMOOR - Atomic OR
```
temp = MEM[rs1]
MEM[rs1] = temp | rs2
rd = sign_extend(temp)
```

#### AMOXOR - Atomic XOR
```
temp = MEM[rs1]
MEM[rs1] = temp ^ rs2
rd = sign_extend(temp)
```

#### AMOMAX/AMOMIN - Atomic Max/Min (Signed)
```
temp = MEM[rs1]
MEM[rs1] = max/min(temp as i32, rs2 as i32) as u32
rd = sign_extend(temp)
```

#### AMOMAXU/AMOMINU - Atomic Max/Min (Unsigned)
```
temp = MEM[rs1]
MEM[rs1] = max/min(temp, rs2 as u32)
rd = sign_extend(temp)
```

## Code Structure

### Source Files

- `src/execute/lr_sc.rs`: LR/SC instructions and reservation set
- `src/execute/amo.rs`: Atomic memory operations

### Key Types and Functions

**Reservation Set (`lr_sc.rs`)**:
```rust
pub struct ReservationSet {
    reserved_addr: Option<u64>,
}

pub fn exec_lr(instr, state, mem) -> Result<()>
pub fn exec_lr_w(instr, state, mem) -> Result<()>
pub fn exec_sc(instr, state, mem) -> Result<()>
pub fn exec_sc_w(instr, state, mem) -> Result<()>
```

**AMO Operations (`amo.rs`)**:
```rust
pub fn exec_amoadd(instr, state, mem) -> Result<()>
pub fn exec_amoand(instr, state, mem) -> Result<()>
pub fn exec_amoor(instr, state, mem) -> Result<()>
pub fn exec_amoxor(instr, state, mem) -> Result<()>
pub fn exec_amomax(instr, state, mem) -> Result<()>
pub fn exec_amomin(instr, state, mem) -> Result<()>
pub fn exec_amomaxu(instr, state, mem) -> Result<()>
pub fn exec_amominu(instr, state, mem) -> Result<()>
```

### AMO funct5 Constants

```rust
const AMO_FUNCT5_AMOSWAP: u8 = 0b00001;
const AMO_FUNCT5_AMOADD:  u8 = 0b00001;  // Note: Same as swap in some contexts
const AMO_FUNCT5_AMOXOR:  u8 = 0b00100;
const AMO_FUNCT5_AMOAND:  u8 = 0b00011;
const AMO_FUNCT5_AMOOR:   u8 = 0b00110;
const AMO_FUNCT5_AMOMIN:  u8 = 0b01000;
const AMO_FUNCT5_AMOMAX:  u8 = 0b01010;
const AMO_FUNCT5_AMOMINU: u8 = 0b01001;
const AMO_FUNCT5_AMOMAXU: u8 = 0b01011;
```

## Testing

### LR/SC Tests (`lr_sc.rs`)

| Test | Description |
|------|-------------|
| `test_lr_basic` | Basic load-reserved operation |
| `test_lr_creates_reservation` | Verify reservation is created |
| `test_sc_success` | Successful store-conditional |
| `test_sc_fail_no_reservation` | SC fails without prior LR |
| `test_sc_fail_after_conflict` | SC fails after reservation cleared |
| `test_lr_sc_atomic_sequence` | Complete atomic read-modify-write |
| `test_sc_clears_reservation` | Verify reservation cleared after SC |
| `test_reservation_set_operations` | Unit tests for ReservationSet |

### AMO Tests (`amo.rs`)

| Test | Description |
|------|-------------|
| `test_amoadd_basic` | Basic atomic add |
| `test_amoadd_wrapping` | 32-bit wraparound behavior |
| `test_amoand_basic` | Atomic AND operation |
| `test_amoor_basic` | Atomic OR operation |
| `test_amoxor_basic` | Atomic XOR operation |
| `test_amoxor_toggle` | XOR as toggle mechanism |
| `test_amomax_basic` | Signed maximum |
| `test_amomax_negative` | Signed max with negative values |
| `test_amomin_basic` | Signed minimum |
| `test_amomaxu_basic` | Unsigned maximum |
| `test_amomaxu_unsigned_comparison` | Unsigned vs signed comparison |
| `test_amominu_basic` | Unsigned minimum |
| `test_amo_sequence` | Sequential AMO operations |

### Running Tests

```bash
# Run all RV64A tests
cargo test lr_sc amo

# Run specific LR/SC tests
cargo test test_lr_sc_atomic_sequence

# Run specific AMO tests
cargo test test_amoadd_basic

# Clear reservations before tests (automatic in test setup)
clear_reservation();
```

### Test Helper Functions

```rust
fn create_lr_instr(rs1: u8, rd: u8, funct5: u8, aq: u8, rl: u8) -> DecodedInstruction
fn create_sc_instr(rs1: u8, rs2: u8, rd: u8, funct5: u8, aq: u8, rl: u8) -> DecodedInstruction
fn create_amo_instr(rs1: u8, rs2: u8, rd: u8, funct5: u8, aq: u8, rl: u8) -> DecodedInstruction
```

## Usage Patterns

### Atomic Increment

Using LR/SC for atomic increment:
```
retry:
    lr.w  t0, (a0)       // Load and reserve
    addi  t0, t0, 1      // Increment
    sc.w  t1, t0, (a0)   // Store conditional
    bnez  t1, retry      // Retry if failed
```

### Compare-and-Swap

Using LR/SC for CAS:
```
retry:
    lr.d  t0, (a0)       // Load current value
    bne   t0, a1, fail   // Compare with expected
    sc.d  t1, a2, (a0)   // Store new value
    bnez  t1, retry      // Retry if SC failed
    li    a0, 0          // Success
    ret
fail:
    li    a0, 1          // Failure
    ret
```

### Lock Implementation

Using AMO for spinlock:
```
lock:
    li    t0, 1
    amoswap.w.aq t0, t0, (a0)  // Try to acquire lock
    bnez  t0, lock             // Retry if locked
    ret

unlock:
    amoswap.w.rl x0, x0, (a0)  // Release lock
    ret
```

## Limitations

1. **64-bit AMO Support**: Current implementation supports 32-bit AMO operations. 64-bit variants (AMO*D) need full implementation.

2. **Multi-core Scaling**: Global reservation singleton limits scaling. Production systems need per-hart reservations.

3. **Memory Ordering**: Acquire/Release bits (aq/rl) are parsed but not fully implemented for memory consistency.

## Compliance

This implementation targets:
- **RISC-V ISA Manual Volume I**: Unprivileged ISA, Chapter 8 (A Extension)
- **RVA23 Profile**: Atomic operations for application processors

## References

- [RISC-V ISA Manual](https://github.com/riscv/riscv-isa-manual)
- `src/execute/lr_sc.rs` - LR/SC implementation
- `src/execute/amo.rs` - AMO implementation
