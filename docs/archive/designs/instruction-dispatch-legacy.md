# Archived Instruction Dispatch Design

> **Status:** Superseded proposal tied to an obsolete Sprint. It is not the current dispatch architecture.

**Sprint**: 2.5  
**Author**: Claude Code (updated per user feedback)  
**Date**: 2026-01-31

## 1. Introduction

### 1.1 Problem Statement

Previous design used hierarchical dispatch (Layer 1/2/3) with opcode-only fast path.

**Issue**: This creates ambiguity:
- Layer 1 uses opcode-only (imprecise)
- Layer 2 needs (opcode, funct3, funct7)
- Inconsistent dispatch granularity

### 1.2 New Design Direction

Per user feedback (2026-01-31):
1. **Remove Layer 1**: No opcode-only dispatch
2. **Use complete matching**: (opcode + funct3 + funct7) for ALL instructions
3. **Add LRU Cache**: Hot path optimization for frequently-used instructions
4. **Single unified table**: ~100 entries, easily HashMap-able

### 1.3 Design Goals

1. **Precision**: Complete (opcode, funct3, funct7) matching for every instruction
2. **Simplicity**: Single dispatch layer, no hierarchy
3. **Performance**: LRU cache for instruction locality
4. **Extensibility**: Support RISC-V extensions and custom instructions

---

## 2. RISC-V Instruction Encoding Analysis

### 2.1 32-bit Instruction Format

```
┌─────────────────────────────────────────────────────────────────────┐
│ 31        25 │ 24   20 │ 19   15 │ 14 12 │ 11 7 │ 6    0           │
│   funct7     │   rs2   │   rs1   │ funct3 │  rd  │    opcode       │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Why Complete Matching is Necessary

| Instruction | opcode | funct3 | funct7 | Why All Three |
|-------------|--------|--------|--------|---------------|
| ADD | 0x33 | 000 | 0000000 | Sub/Shift use same funct3 |
| SUB | 0x33 | 000 | 0100000 | Distinguished by funct7 |
| SRLI | 0x13 | 101 | 0000000 | SRAI has funct7=0100000 |
| SRAI | 0x13 | 101 | 0100000 | Distinguished by funct7 |
| BEQ | 0x63 | 000 | - | BNE uses funct3=001 |

**Conclusion**: All three fields required for unambiguous dispatch.

### 2.3 Instruction Count

| Category | Count | Notes |
|----------|-------|-------|
| RV32I Base | ~37 | Already implemented |
| RV32M | +8 | Multiply/divide |
| RV32A | +11 | Atomic |
| RV32F | +26 | Floating-point |
| RV32D | +17 | Double precision |
| RV64I | +12 | 64-bit extensions |
| RV32C | +71 | Compressed (16-bit) |
| **Total** | **~200** | HashMap easily handles |

---

## 3. Dispatch Architecture

### 3.1 Simplified Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Instruction                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    InstructionKey (Triplet)                      │
│   ┌─────────────────────────────────────────────────────────┐   │
│   │ opcode (7 bits) │ funct3 (3 bits) │ funct7 (7 bits)     │   │
│   └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Dispatch Table                              │
│              HashMap<InstructionKey, ExecutorFn>                 │
│                         (~200 entries)                           │
│                           O(1) avg                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      LRU Cache Layer                             │
│                   (16-32 most recent)                            │
│                        O(1) lookup                               │
└─────────────────────────────────────────────────────────────────┘
```

### 3.2 Dispatch Flow

```
Instruction arrives
        │
        ▼
┌─────────────────┐
│ Check LRU Cache │ ──hit──→ Execute cached executor
└─────────────────┘
        │
       miss
        ▼
┌─────────────────────────┐
│ Lookup Dispatch Table   │ ──found──→ Execute + Update Cache
│ (opcode, funct3, funct7)│
└─────────────────────────┘
        │
     not found
        ▼
┌─────────────────────────┐
│ InvalidInstruction Error│
└─────────────────────────┘
```

### 3.3 Why This Works

1. **RISC-V has ~200 instructions** - Small enough for single HashMap
2. **HashMap O(1) average** - Fast enough for simulation
3. **LRU cache captures locality** - Programs show strong instruction locality
4. **Simple code path** - No hierarchy, easier to maintain

---

## 4. Key Data Structures

### 4.1 InstructionKey

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InstructionKey {
    /// 7-bit opcode (0-127)
    pub opcode: u8,
    /// 3-bit funct3 (0-7), ALWAYS used
    pub funct3: u8,
    /// 7-bit funct7 (0-127), ALWAYS used
    pub funct7: u8,
}

impl InstructionKey {
    /// Create from decoded instruction
    pub fn from_instr(instr: &DecodedInstruction) -> Self {
        Self {
            opcode: instr.opcode as u8,
            funct3: instr.funct3.unwrap_or(0),
            funct7: instr.funct7.unwrap_or(0),
        }
    }
    
    /// Create from raw instruction word
    pub fn from_raw(raw: u32) -> Self {
        Self {
            opcode: (raw & 0x7F) as u8,
            funct3: ((raw >> 12) & 0x7) as u8,
            funct7: ((raw >> 25) & 0x7F) as u8,
        }
    }
    
    /// Validate all fields are within range
    pub fn is_valid(&self) -> bool {
        self.opcode < 128 && self.funct3 < 8 && self.funct7 < 128
    }
}

/// Executor function type
type ExecutorFn = fn(
    &mut Executor,
    &DecodedInstruction,
    &mut CoreState,
    &mut dyn MemoryInterface,
) -> Result<(), ExecuteError>;
```

### 4.2 LRU Cache (using std::collections::LinkedHashMap or lru_cache crate)

```rust
use std::collections::LinkedHashMap;

/// LRU Cache for hot instruction dispatch
pub struct LruCache {
    /// Maximum cache entries
    capacity: usize,
    /// Ordered map (most recent at end)
    entries: LinkedHashMap<InstructionKey, ExecutorFn>,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: LinkedHashMap::new(),
        }
    }
    
    /// Get executor from cache, updating LRU order
    pub fn get(&mut self, key: &InstructionKey) -> Option<ExecutorFn> {
        // Move to end (most recent) if exists
        if let Some(executor) = self.entries.remove(key) {
            self.entries.insert(*key, executor);
            Some(executor)
        } else {
            None
        }
    }
    
    /// Insert new entry, evicting LRU if at capacity
    pub fn insert(&mut self, key: InstructionKey, executor: ExecutorFn) {
        self.entries.insert(key, executor);
        if self.entries.len() > self.capacity {
            // Remove first entry (least recently used)
            self.entries.pop_front();
        }
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
```

### 4.3 Dispatcher

```rust
pub struct Dispatcher {
    /// Main dispatch table: complete (opcode, funct3, funct7) → executor
    /// All instructions registered here
    dispatch_table: HashMap<InstructionKey, ExecutorFn>,
    
    /// LRU cache for hot instructions (default 32 entries)
    cache: LruCache,
}

impl Dispatcher {
    pub fn new(cache_capacity: usize) -> Self {
        let mut dispatch_table = HashMap::new();
        let cache = LruCache::new(cache_capacity);
        
        // Register all base RISC-V instructions
        Self::register_rv32i_base(&mut dispatch_table);
        
        Self {
            dispatch_table,
            cache,
        }
    }
    
    /// Main dispatch function
    pub fn dispatch(
        &mut self,
        instr: &DecodedInstruction,
        state: &mut CoreState,
        mem: &mut dyn MemoryInterface,
    ) -> Result<(), ExecuteError> {
        let key = InstructionKey::from_instr(instr);
        
        // Step 1: Check LRU cache
        if let Some(executor) = self.cache.get(&key) {
            return executor(self, instr, state, mem);
        }
        
        // Step 2: Lookup main table
        if let Some(executor) = self.dispatch_table.get(&key) {
            // Step 3: Update cache
            self.cache.insert(key, *executor);
            return executor(self, instr, state, mem);
        }
        
        Err(ExecuteError::InvalidOperation)
    }
    
    /// Register a new instruction
    pub fn register(
        &mut self,
        opcode: u8,
        funct3: u8,
        funct7: u8,
        executor: ExecutorFn,
    ) {
        let key = InstructionKey {
            opcode,
            funct3,
            funct7,
        };
        self.dispatch_table.insert(key, executor);
    }
    
    /// Register RISC-V base instructions (RV32I)
    fn register_rv32i_base(table: &mut HashMap<InstructionKey, ExecutorFn>) {
        // LUI
        table.insert(
            InstructionKey { opcode: 0x37, funct3: 0, funct7: 0 },
            Executor::exec_lui,
        );
        
        // AUIPC
        table.insert(
            InstructionKey { opcode: 0x17, funct3: 0, funct7: 0 },
            Executor::exec_auipc,
        );
        
        // JAL
        table.insert(
            InstructionKey { opcode: 0x6F, funct3: 0, funct7: 0 },
            Executor::exec_jal,
        );
        
        // JALR
        table.insert(
            InstructionKey { opcode: 0x67, funct3: 0, funct7: 0 },
            Executor::exec_jalr,
        );
        
        // BRANCH: BEQ, BNE, BLT, BGE, BLTU, BGEU
        table.insert(InstructionKey { opcode: 0x63, funct3: 0b000, funct7: 0 }, Executor::exec_beq);
        table.insert(InstructionKey { opcode: 0x63, funct3: 0b001, funct7: 0 }, Executor::exec_bne);
        table.insert(InstructionKey { opcode: 0x63, funct3: 0b100, funct7: 0 }, Executor::exec_blt);
        table.insert(InstructionKey { opcode: 0x63, funct3: 0b101, funct7: 0 }, Executor::exec_bge);
        table.insert(InstructionKey { opcode: 0x63, funct3: 0b110, funct7: 0 }, Executor::exec_bltu);
        table.insert(InstructionKey { opcode: 0x63, funct3: 0b111, funct7: 0 }, Executor::exec_bgeu);
        
        // LOAD: LB, LH, LW, LBU, LHU
        // (handled by exec_load with funct3 discrimination)
        table.insert(InstructionKey { opcode: 0x03, funct3: 0b000, funct7: 0 }, Executor::exec_load);
        table.insert(InstructionKey { opcode: 0x03, funct3: 0b001, funct7: 0 }, Executor::exec_load);
        table.insert(InstructionKey { opcode: 0x03, funct3: 0b010, funct7: 0 }, Executor::exec_load);
        table.insert(InstructionKey { opcode: 0x03, funct3: 0b100, funct7: 0 }, Executor::exec_load);
        table.insert(InstructionKey { opcode: 0x03, funct3: 0b101, funct7: 0 }, Executor::exec_load);
        
        // STORE: SB, SH, SW
        table.insert(InstructionKey { opcode: 0x23, funct3: 0b000, funct7: 0 }, Executor::exec_store);
        table.insert(InstructionKey { opcode: 0x23, funct3: 0b001, funct7: 0 }, Executor::exec_store);
        table.insert(InstructionKey { opcode: 0x23, funct3: 0b010, funct7: 0 }, Executor::exec_store);
        
        // OP-IMM: ADDI, SLTI, SLTIU, XORI, ORI, ANDI, SLLI, SRLI, SRAI
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b000, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b010, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b011, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b100, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b110, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b111, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b001, funct7: 0 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b101, funct7: 0b0000000 }, Executor::exec_op_imm);
        table.insert(InstructionKey { opcode: 0x13, funct3: 0b101, funct7: 0b0100000 }, Executor::exec_op_imm);
        
        // OP: ADD, SUB, SLL, SLT, SLTU, XOR, SRL, SRA, OR, AND
        // Note: Some use funct7 for discrimination
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b000, funct7: 0b0000000 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b000, funct7: 0b0100000 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b001, funct7: 0 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b010, funct7: 0 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b011, funct7: 0 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b100, funct7: 0 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b101, funct7: 0b0000000 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b101, funct7: 0b0100000 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b110, funct7: 0 }, Executor::exec_op);
        table.insert(InstructionKey { opcode: 0x33, funct3: 0b111, funct7: 0 }, Executor::exec_op);
        
        // SYSTEM: ECALL, EBREAK, MRET, etc.
        table.insert(InstructionKey { opcode: 0x73, funct3: 0, funct7: 0 }, Executor::exec_system); // ECALL
        table.insert(InstructionKey { opcode: 0x73, funct3: 0, funct7: 0b0010000 }, Executor::exec_system); // EBREAK
        table.insert(InstructionKey { opcode: 0x73, funct3: 0, funct7: 0b0011000 }, Executor::exec_system); // MRET
    }
}
```

---

## 5. Performance Analysis

### 5.1 Time Complexity

| Operation | Best Case | Average | Worst Case |
|-----------|-----------|---------|------------|
| Cache Lookup | O(1) | O(1) | O(1) |
| HashMap Lookup | O(1) | O(1) | O(n) |
| **Overall Dispatch** | **O(1)** | **O(1)** | **O(n)** |

### 5.2 Cache Hit Rate

Typical program behavior:
- Loop-heavy code: 95%+ cache hit rate
- Mixed code: 80-90% cache hit rate
- Branch-heavy code: 70-80% cache hit rate

### 5.3 Cache Size Selection

| Size | Hit Rate | Memory | Recommendation |
|------|----------|--------|----------------|
| 16 | Good | 128 bytes | Minimum |
| 32 | Better | 256 bytes | Recommended |
| 64 | Best | 512 bytes | High locality code |

**Default**: 32 entries provides good balance.

---

## 6. Extensibility

### 6.1 Adding RISC-V Extensions

```rust
impl Dispatcher {
    /// Register M extension (multiply/divide)
    pub fn register_rv32m(&mut self) {
        self.register(0x33, 0b000, 0b0000001, Executor::exec_mul);
        self.register(0x33, 0b001, 0b0000001, Executor::exec_mulh);
        self.register(0x33, 0b010, 0b0000001, Executor::exec_mulhsu);
        self.register(0x33, 0b011, 0b0000001, Executor::exec_mulhu);
        self.register(0x33, 0b100, 0b0000001, Executor::exec_div);
        self.register(0x33, 0b101, 0b0000001, Executor::exec_divu);
        self.register(0x33, 0b110, 0b0000001, Executor::exec_rem);
        self.register(0x33, 0b111, 0b0000001, Executor::exec_remu);
    }
    
    /// Register A extension (atomic)
    pub fn register_rv32a(&mut self) {
        // LR/SC, AMO operations...
    }
}
```

### 6.2 Adding Custom Instructions

```rust
impl Dispatcher {
    /// Register custom instruction
    pub fn register_custom(
        &mut self,
        opcode: u8,
        funct3: u8,
        funct7: u8,
        name: &str,
        executor: ExecutorFn,
    ) {
        self.register(opcode, funct3, funct7, executor);
        info!("Registered custom instruction: {}", name);
    }
}
```

### 6.3 Compressed Instructions (RV32C)

Compressed instructions are 16-bit, not 32-bit.

**Approach**: Decode-to-expand
1. Detect 16-bit (bits [1:0] != 0b11)
2. Expand to 32-bit equivalent
3. Dispatch using same mechanism

```rust
pub fn decode_compressed(raw: u16) -> u32 {
    // Expand 16-bit C instruction to 32-bit RISC-V encoding
    // Then dispatch normally
}
```

---

## 7. Migration Plan

### Phase 1: Data Structures (4h)
- [ ] Create `InstructionKey` struct
- [ ] Create `LruCache` struct
- [ ] Create `Dispatcher` struct
- [ ] Add unit tests

### Phase 2: Dispatch Logic (8h)
- [ ] Implement `dispatch()` function
- [ ] Implement `register()` API
- [ ] Register all RV32I instructions
- [ ] Run existing tests (verify correctness)

### Phase 3: Integration (4h)
- [ ] Replace current `Executor::execute()` with `Dispatcher::dispatch()`
- [ ] Update all executor function signatures
- [ ] Add integration tests

### Phase 4: Extensions (8h)
- [ ] Implement M extension registration
- [ ] Implement A extension registration
- [ ] Implement F/D registration
- [ ] Document extension API

### Phase 5: Testing & Benchmarking (6h)
- [ ] Add dispatch performance benchmark
- [ ] Test cache hit rates
- [ ] Test all instruction registrations
- [ ] Verify no regressions

---

## 8. Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `src/dispatch/mod.rs` | Create | Main dispatcher module |
| `src/dispatch/key.rs` | Create | InstructionKey type |
| `src/dispatch/cache.rs` | Create | LRU cache implementation |
| `src/execute/mod.rs` | Modify | Use Dispatcher instead of current dispatch |
| `tests/dispatch_test.rs` | Create | Unit tests |
| `benches/dispatch_bench.rs` | Create | Performance benchmarks |

---

## 9. Comparison: Old vs New Design

### Old Design (Hierarchical)

```rust
// Three layers, inconsistent dispatch
fast_path: [Option<ExecutorFn>; 128],      // opcode-only
extended_table: HashMap<InstructionKey, fn>, // (opcode, funct3, funct7)
custom_table: HashMap<u32, fn>,             // custom
```

### New Design (Simplified)

```rust
// Single unified table, complete matching
dispatch_table: HashMap<InstructionKey, ExecutorFn>,  // All instructions
cache: LruCache,                                      // Hot path optimization
```

### Benefits of New Design

| Aspect | Old | New |
|--------|-----|-----|
| Simplicity | Complex (3 layers) | Simple (1 layer) |
| Dispatch | Inconsistent | Consistent |
| Extensibility | Complex | Simple |
| Performance | Array + HashMap | HashMap + Cache |
| Code Size | Larger | Smaller |
| Maintainability | Harder | Easier |

---

## 10. References

- [RISC-V Specification](https://riscv.org/technical/specifications/)
- [当前架构入口](../../architecture/README.md)
- [已归档的 Sprint 计划](../sprint-plan-archived.md)

---

## 11. Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| v1.0 | 2026-01-31 | Claude Code | Initial design |
| v2.0 | 2026-01-31 | Claude Code | Simplified per user feedback: removed Layer 1, added LRU cache, complete matching |
