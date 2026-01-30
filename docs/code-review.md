# Code Review Comments

**Date**: 2026-01-30  
**Reviewer**: Code Review Session  
**Status**: Pending Implementation

---

## 1. 语言统一 (Language Consistency)

### Comment #1
- **Severity**: 🔵 Info
- **Category**: Style
- **Location**: Global

**Description**:
代码和注释应使用英文，确保国际化一致性和可维护性。

**Current Issue**:
- 部分中文注释和字符串
- 混合语言影响代码可读性

**Requirement**:
- 所有代码字符串使用英文
- 所有注释使用英文
- 错误消息使用英文

**Example**:
```rust
// Before (mixed)
fn 执行指令(&self) -> Result<()> {
    // 这里是中文注释
}

// After (English)
fn execute_instruction(&self) -> Result<()> {
    // Execute single instruction and update state
}
```

**Priority**: 🟢 Low  
**Estimated Effort**: 4h (whole codebase)  
**Suggested Sprint**: Sprint 15 (Cleanup)

---

## 2. 指令执行函数映射设计 (Instruction Execution Mapping)

### Comment #2
- **Severity**: 🟡 Medium
- **Category**: Architecture
- **Location**: `src/execute/mod.rs`

**Description**:
需要设计高效的 decode 结果到执行函数的映射关系。

**Current Issue**:
- 缺乏统一的映射机制
- 手动 match opcode 效率可能不是最优

**Requirement**:
1. 设计 O(1) 时间复杂度的查找机制
2. 支持 opcode + funct3 + funct7 组合查找
3. 考虑使用 HashMap 或数组索引
4. 保持代码可读性和可维护性

**Design Options**:
```rust
// Option 1: HashMap lookup (flexible)
static INSTRUCTION_TABLE: Lazy<HashMap<Opcode, fn()>> = Lazy::new(|| {
    HashMap::from([
        (Opcode::Add, execute_add),
        (Opcode::Sub, execute_sub),
        // ...
    ])
});

// Option 2: Array index (fastest)
static INSTRUCTION_TABLE: Lazy<Vec<fn()>> = Lazy::new(|| {
    vec![
        /* opcode value -> function */
    ]
});

// Option 3: Hierarchical lookup
match opcode {
    Opcode::Op => TABLE_OP[funct7 as usize][funct3 as usize](...),
    Opcode::Op32 => TABLE_OP32[funct7 as usize][funct3 as usize](...),
    // ...
}
```

**Priority**: 🟠 Medium  
**Estimated Effort**: 16h  
**Suggested Sprint**: Sprint 2 (after RV32I foundation)

---

## 3. 模块化指令执行 (Modular Execution Functions)

### Comment #3
- **Severity**: 🟡 Medium
- **Category**: Code Quality
- **Location**: `src/execute/`

**Description**:
当前所有执行函数在单个文件中，应按指令类型拆分。

**Current Issue**:
- `src/execute/mod.rs` 过大 (>300 行)
- 缺乏模块化组织
- 难以维护和导航

**Requirement**:
按 RISC-V 指令格式拆分文件：

```
src/execute/
├── mod.rs           # 主模块，公共接口
├── r_type.rs        # R-type: ADD, SUB, AND, OR, XOR, etc.
├── i_type.rs        # I-type: ADDI, LW, JALR, etc.
├── s_type.rs        # S-type: SW, SH, SB
├── b_type.rs        # B-type: BEQ, BNE, BLT, BGE, etc.
├── u_type.rs        # U-type: LUI, AUIPC
├── j_type.rs        # J-type: JAL
├── system.rs        # System: ECALL, EBREAK, MRET, etc.
└── generated/       # Auto-generated files
    └── mod.rs
```

**Priority**: 🟠 Medium  
**Estimated Effort**: 24h  
**Suggested Sprint**: Sprint 3 (Refactoring)

---

## 4. 代码生成 (Code Generation)

### Comment #4
- **Severity**: 🟢 Low
- **Category**: Developer Experience
- **Location**: `src/execute/`

**Description**:
评估使用代码生成工具批量生成指令执行函数。

**Current Issue**:
- 手动编写 47+ 条指令执行函数
- 大量重复代码
- 容易出错

**Requirement**:
1. 评估 proc-macro 或 codegen 工具
2. 生成重复代码（函数签名、match 分支等）
3. 生成文件放在 `src/execute/generated/`

**Tools to Evaluate**:
- `cargo-instrument` - Instrumentation codegen
- `quote` + `proc-macro2` - Procedural macros
- `syn` + `quote` - Rust AST manipulation
- Custom `build.rs` script

**Generated Code Pattern**:
```rust
// templates/execute_fn.rs
macro_rules! generate_execution_fn {
    ($name:ident, $opcode:expr) => {
        fn $name(instr: &DecodedInstruction, state: &mut CoreState) -> Result<()> {
            // Standard execution pattern
            let rs1 = state.read_reg(instr.rs1.unwrap());
            let rs2 = state.read_reg(instr.rs2.unwrap());
            let rd = instr.rd.unwrap();
            let result = // calculation
            state.write_reg(rd, result);
            Ok(())
        }
    };
}

generate_execution_fn!(execute_add, Opcode::Op);
generate_execution_fn!(execute_sub, Opcode::Op);
```

**Priority**: 🟢 Low (Quality of Life)  
**Estimated Effort**: 16h (setup + implementation)  
**Suggested Sprint**: Sprint 4 (Optimization)

---

## Summary

| # | Category | Priority | Effort | Suggested Sprint |
|---|----------|----------|--------|------------------|
| 1 | Language | Low | 4h | Sprint 15 |
| 2 | Architecture | Medium | 16h | Sprint 2 |
| 3 | Modularization | Medium | 24h | Sprint 3 |
| 4 | Code Gen | Low | 16h | Sprint 4 |

## Recommended Action Items

- [ ] **Sprint 2**: Design instruction lookup table (Comment #2)
- [ ] **Sprint 3**: Modularize execute module (Comment #3)
- [ ] **Sprint 4**: Evaluate code generation (Comment #4)
- [ ] **Sprint 15**: English-only refactor (Comment #1)

## References

- RVA23 Profile Specification
- RISC-V ISA Manual
- Rust proc-macro book
