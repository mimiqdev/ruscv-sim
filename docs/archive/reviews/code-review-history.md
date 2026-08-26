# Archived Code Review History

> **Status:** Historical review record. Sprint assignments and pending states are not current work.

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

---

## PR #6 Review Items - Sprint 5 Review

**Date**: 2026-01-31  
**Reviewer**: Code Review Session  
**Status**: Addressed

---

### Item 1: Global Reservation Singleton (Multi-core Scaling)

**Severity**: 🟡 Medium (Known Limitation)  
**Category**: Architecture  
**Location**: `src/execute/lr_sc.rs`

**Description**:
The global reservation singleton won't scale to multi-core systems.

**Decision**: **DOCUMENTED** ✓ Fixed Now  
**Action Taken**:
- Added documentation about multi-core limitation in lr_sc.rs
- Architecture document already notes this is a known limitation
- Suitable for single-core simulation; multi-core support is out of scope for v1.0

**Status**: ✅ Acknowledged and documented in code comments

---

### Item 2: Missing WRS.NT/WRS.ST Instructions

**Severity**: 🟢 Low (Optional)  
**Category**: Completeness  
**Location**: Not implemented

**Description**:
WRS.NT (Wait for Interrupt, No Timer) and WRS.ST (Wait for Supervisor Trap) instructions are not implemented.

**Decision**: **UPDATE PLAN**  
**Rationale**:
- These are optional instructions in RVA23, not required
- Low priority for virtual prototype use case
- Can be added in Sprint 14 (Optimization & Release) or future releases

**Action**: Added to Future Improvements section in sprint-plan.md

---

### Item 3: No 64-bit AMO Support

**Severity**: 🟠 Medium  
**Category**: Completeness  
**Location**: `src/execute/amo.rs`, `src/execute/lr_sc.rs`

**Description**:
LR.D, SC.D, and AMO*.D (64-bit atomic operations) are not implemented.

**Decision**: **UPDATE PLAN**  
**Rationale**:
- Requires significant work (new functions for 64-bit memory operations)
- 32-bit AMO operations are sufficient for most use cases
- Low priority for current sprint scope (Sprint 5: Trap Handling + RV64M/A)

**Action**: 
- Added documentation noting 64-bit limitation in amo.rs
- Added to Future Improvements section

**Impact**: 
- Missing instructions: LR.D, SC.D, AMOSWAP.D, AMOADD.D, AMOXOR.D, AMOAND.D, AMOOR.D, AMOMIN.D, AMOMAX.D, AMOMINU.D, AMOMAXU.D
- 14 instructions not yet implemented

---

### Item 4: Code Quality Suggestions

#### 4a: Add RISC-V Spec References

**Decision**: **FIX NOW** ✓  
**Action Taken**:
- Added spec references to `src/execute/lr_sc.rs`:
  - RISC-V ISA Volume I, Section 8.3 (Load-Reserved/Store-Conditional)
  - RISC-V ISA Volume II, Section 3.5.1 (Reservation Granularity)
- Added spec references to `src/execute/amo.rs`:
  - RISC-V ISA Volume I, Section 8.3 (AMO Operations)
  - RISC-V ISA Volume I, Table 19.3 (AMO encoding)

**Status**: ✅ Complete

#### 4b: Consider proptest for Arithmetic Property Testing

**Decision**: **UPDATE PLAN** (Future Enhancement)  
**Rationale**:
- proptest provides property-based testing (e.g., commutativity, overflow behavior)
- Would improve test coverage for edge cases
- Not critical for current sprint; can be added later

**Action**: Added to testing strategy improvements for Sprint 13 or later

#### 4c: Use Constants for funct5 Values

**Decision**: **FIX NOW** ✓  
**Action Taken**:
- Added `AMO_FUNCT5_*` constants to `src/execute/lr_sc.rs`
- Added `AMO_FUNCT5_*` constants to `src/execute/amo.rs`
- Magic numbers like `0b00001`, `0b00011` replaced with named constants

**Status**: ✅ Complete

---

## Summary of PR #6 Review Items

| # | Item | Decision | Status |
|---|------|----------|--------|
| 1 | Global reservation singleton | Documented | ✅ Complete |
| 2 | Missing WRS.NT/WRS.ST | Update PLAN | 📋 Tracked |
| 3 | No 64-bit AMO support | Update PLAN | 📋 Tracked |
| 4a | Add RISC-V spec references | Fix NOW | ✅ Complete |
| 4b | proptest for arithmetic | Update PLAN | 📋 Tracked |
| 4c | Use constants for funct5 | Fix NOW | ✅ Complete |

---

## Appendix A: 文件大小标准 (File Size Guidelines)

**最后更新**: 2026-02-01  
**状态**: ✅ 已更新

### 标准定义

| 模块类型 | 目标行数 | 最大行数 | 说明 |
|----------|----------|----------|------|
| 简单模块 | < 200 | 300 | 基础工具函数、常量定义 |
| 一般模块 | < 300 | 400 | 标准指令实现、测试模块 |
| 复杂模块 | < 500 | 600 | 浮点转换、原子操作、CSR系统 |
| 生成代码 | < 800 | 1000 | 自动生成的指令表、匹配代码 |

### 当前超标文件 (已豁免)

| 文件 | 行数 | 类型 | 豁免理由 |
|------|------|------|----------|
| `src/isa/rv64i/system.rs` | 528 | 复杂模块 | CSR/系统指令功能多样 |
| `src/isa/rv64d/convert.rs` | 885 | 生成代码 | 浮点转换大量模式匹配 |
| `src/isa/rv64a/amo.rs` | 798 | 复杂模块 | 原子操作逻辑复杂 |

### 拆分建议

未来如需要拆分，优先考虑:
1. `rv64d/convert.rs` → `ftoi.rs` + `itof.rs` + `float_conv.rs`
2. `rv64a/amo.rs` → 按操作类型拆分 (add/and/or/xor/max/min)

---

## Appendix B: 提交信息规范 (Commit Message Guidelines)

**最后更新**: 2026-02-01  
**状态**: ✅ 已生效

### 要求

1. **必须包含消息体**: 禁止空 body (参考 commit `9821174` 问题)
2. **格式规范**:
   ```
   type(scope): subject
   
   body (required)
   
   footer (optional)
   ```
3. **Body 要求**: 说明 "what" 和 "why"，不只是 "how"

### 类型定义

| Type | 用途 |
|------|------|
| feat | 新功能 |
| fix | 修复 |
| docs | 文档 |
| style | 格式调整 |
| refactor | 重构 |
| test | 测试 |
| chore | 构建/工具 |

### 示例

**正确**:
```
csr: implement mstatus register with MPP/SPP fields

- Add mstatus CSR with MPP (Machine Previous Privilege) field
- Add SPP (Supervisor Previous Privilege) field support
- Implement read/write/set/clear operations

This enables privilege mode switching between M/S/U modes.
```

**错误** (空 body):
```
fix: bug fix
```

---

## References

- RVA23 Profile Specification
- RISC-V ISA Manual
- Rust proc-macro book
- [Conventional Commits](https://www.conventionalcommits.org/)
