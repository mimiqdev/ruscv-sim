# Sprint 8.5: 推广 C 指令模块化模式到所有指令集

## 任务概述

将 RV64C 压缩指令的模块化组织模式推广到所有其他指令集（RV64I, RV64M, RV64A, RV64F, RV64D），实现统一的代码组织结构，提高代码可维护性和可读性。

---

## 1. 当前结构分析

### 1.1 RV64C 指令集组织（目标模式）

```
src/isa/rv64c/
├── mod.rs              # 模块入口，公共接口，re-exports
├── decoder_16bit.rs    # 16位指令译码器
├── c0_quadw.rs         # C0 象限：Load/Store 操作
├── c1_addiw.rs         # C1 象限：ADDIW/ADDW/SUBW
├── c1_arith.rs         # C1 象限：算术和逻辑操作
├── c1_shift.rs         # C1 象限：移位操作
├── c2_move.rs          # C2 象限：移动和跳转
└── c2_stack.rs         # C2 象限：栈操作
```

**优点：**
- 每个文件职责单一，功能清晰
- 文件大小适中（200-400行）
- 测试内联在模块中
- 模块文档完整
- 公共接口统一从 `mod.rs` re-export

### 1.2 其他指令集现状

当前所有其他指令实现都在 `src/execute/` 中，按指令格式组织：

```
src/execute/
├── mod.rs              # 执行器入口（305行）
├── r_type.rs           # R-type 指令（651行）- RV64I 算术/逻辑
├── i_type.rs           # I-type 指令（637行）- Load/立即数操作
├── s_type.rs           # S-type 指令（118行）- Store
├── b_type.rs           # B-type 指令（366行）- 分支
├── u_type.rs           # U-type 指令（137行）- LUI/AUIPC
├── j_type.rs           # J-type 指令（203行）- 跳转
├── mul.rs              # RV64M 乘法指令（420行）
├── div.rs              # RV64M 除法指令（511行）
├── amo.rs              # RV64A AMO 指令（798行）
├── lr_sc.rs            # RV64A LR/SC 指令（515行）
├── system.rs           # 系统指令（698行）
├── f_arith.rs          # RV64F 算术（275行）
├── f_load_store.rs     # RV64F Load/Store（198行）
├── f_compare.rs        # RV64F 比较（314行）
├── f_convert.rs        # RV64F 转换（395行）
├── f_classify.rs       # RV64F 分类（241行）
├── f_div_sqrt.rs       # RV64F 除法/开方（305行）
├── f_madd.rs           # RV64F 乘加（358行）
├── d_arith.rs          # RV64D 算术（404行）
├── d_load_store.rs     # RV64D Load/Store（254行）
├── d_compare.rs        # RV64D 比较（332行）
├── d_convert.rs        # RV64D 转换（885行）
├── d_classify.rs       # RV64D 分类（264行）
├── d_div_sqrt.rs       # RV64D 除法/开方（344行）
└── d_madd.rs           # RV64D 乘加（365行）
```

**问题：**
- 文件过大（651行、885行、798行）
- 混合了不同指令集的实现（如 R-type 同时包含 RV64I 和 RV64M 的指令）
- 职责不单一，功能分类不清晰
- 缺少统一的 ISA 模块结构

---

## 2. 目标结构设计

### 2.1 目标目录结构

```
src/
├── isa/
│   ├── mod.rs              # ISA 模块入口
│   ├── rv64c/              # 压缩指令（已存在，参考模式）
│   │   └── ...
│   ├── rv64i/              # 基础整数指令（新建）
│   │   ├── mod.rs
│   │   ├── alu.rs          # 算术逻辑运算
│   │   ├── shift.rs        # 移位操作
│   │   ├── load.rs         # 加载指令
│   │   ├── store.rs        # 存储指令
│   │   ├── branch.rs       # 分支指令
│   │   ├── jump.rs         # 跳转指令
│   │   ├── lui_auipc.rs    # 高位立即数
│   │   └── system.rs       # 系统指令
│   ├── rv64m/              # 乘除指令（新建）
│   │   ├── mod.rs
│   │   ├── mul.rs          # 乘法指令
│   │   └── div.rs          # 除法/取余指令
│   ├── rv64a/              # 原子指令（新建）
│   │   ├── mod.rs
│   │   ├── lr_sc.rs        # 保留/条件存储
│   │   └── amo.rs          # 原子内存操作
│   ├── rv64f/              # 单精度浮点（新建）
│   │   ├── mod.rs
│   │   ├── arith.rs        # 算术运算
│   │   ├── load_store.rs   # Load/Store
│   │   ├── compare.rs      # 比较
│   │   ├── convert.rs      # 转换
│   │   ├── classify.rs     # 分类
│   │   ├── div_sqrt.rs     # 除法/开方
│   │   └── madd.rs         # 乘加
│   └── rv64d/              # 双精度浮点（新建）
│       ├── mod.rs
│       ├── arith.rs
│       ├── load_store.rs
│       ├── compare.rs
│       ├── convert.rs
│       ├── classify.rs
│       ├── div_sqrt.rs
│       └── madd.rs
└── execute/
    └── mod.rs              # 保留执行器，但简化
```

### 2.2 文件拆分详情

| 原文件 | 新位置 | 说明 |
|--------|--------|------|
| `execute/r_type.rs` (651行) | `isa/rv64i/alu.rs` + `isa/rv64i/shift.rs` | 拆分算术和移位 |
| `execute/i_type.rs` (637行) | `isa/rv64i/load.rs` + `isa/rv64i/alu.rs` (立即数部分) | 拆分加载和立即数运算 |
| `execute/s_type.rs` (118行) | `isa/rv64i/store.rs` | 直接迁移 |
| `execute/b_type.rs` (366行) | `isa/rv64i/branch.rs` | 直接迁移 |
| `execute/u_type.rs` (137行) | `isa/rv64i/lui_auipc.rs` | 直接迁移 |
| `execute/j_type.rs` (203行) | `isa/rv64i/jump.rs` | 直接迁移 |
| `execute/mul.rs` (420行) | `isa/rv64m/mul.rs` | 直接迁移 |
| `execute/div.rs` (511行) | `isa/rv64m/div.rs` | 直接迁移 |
| `execute/amo.rs` (798行) | `isa/rv64a/amo.rs` | 直接迁移 |
| `execute/lr_sc.rs` (515行) | `isa/rv64a/lr_sc.rs` | 直接迁移 |
| `execute/system.rs` (698行) | `isa/rv64i/system.rs` | 迁移到 RV64I |
| `execute/f_*.rs` (共7个文件) | `isa/rv64f/*.rs` | 整体迁移 |
| `execute/d_*.rs` (共7个文件) | `isa/rv64d/*.rs` | 整体迁移 |

---

## 3. API 兼容性策略

### 3.1 保持现有接口

为了保持向后兼容，`src/execute/mod.rs` 将继续 re-export 所有执行函数：

```rust
// src/execute/mod.rs
pub use crate::isa::rv64i::{
    exec_add, exec_sub, exec_and, exec_or, exec_xor,  // ALU
    exec_sll, exec_srl, exec_sra,                      // Shift
    exec_lb, exec_lh, exec_lw, exec_ld,                // Load
    exec_sb, exec_sh, exec_sw, exec_sd,                // Store
    exec_beq, exec_bne, exec_blt, exec_bge,            // Branch
    exec_jal, exec_jalr,                               // Jump
    exec_lui, exec_auipc,                              // LUI/AUIPC
    exec_system,                                       // System
};

pub use crate::isa::rv64m::{
    exec_mul, exec_mulh, exec_mulhu, exec_mulhsu,      // Mul
    exec_div, exec_divu, exec_rem, exec_remu,          // Div
};

pub use crate::isa::rv64a::{
    exec_lr, exec_lr_w, exec_sc, exec_sc_w,            // LR/SC
    exec_amoadd, exec_amoand, /* ... */                // AMO
};

pub use crate::isa::rv64f::{
    exec_fadd_s, exec_fsub_s, /* ... */                // F extension
};

pub use crate::isa::rv64d::{
    exec_fadd_d, exec_fsub_d, /* ... */                // D extension
};
```

### 3.2 执行器核心逻辑简化

`Executor::execute()` 方法保持逻辑，但改为调用 ISA 模块的函数：

```rust
// src/execute/mod.rs
impl Executor {
    pub fn execute(&mut self, instr: &DecodedInstruction, ...) -> Result<(), ExecuteError> {
        match instr.opcode {
            Opcode::Lui => exec_lui(instr, state, mem),
            Opcode::Op => exec_op(instr, state, mem),      // 内部再分发到 RV64I/RV64M
            // ...
        }
    }
}
```

---

## 4. 实现计划

### 阶段 1: RV64I 重构（优先级高）

1. **创建目录结构**
   ```
   src/isa/rv64i/
   ├── mod.rs
   ├── alu.rs
   ├── shift.rs
   ├── load.rs
   ├── store.rs
   ├── branch.rs
   ├── jump.rs
   ├── lui_auipc.rs
   └── system.rs
   ```

2. **迁移内容**
   - `r_type.rs` → `alu.rs` + `shift.rs`
   - `i_type.rs` → `load.rs` + `alu.rs` (立即数部分)
   - `s_type.rs` → `store.rs`
   - `b_type.rs` → `branch.rs`
   - `j_type.rs` → `jump.rs`
   - `u_type.rs` → `lui_auipc.rs`
   - `system.rs` → `system.rs`

3. **更新 execute/mod.rs**
   - 添加 `pub mod rv64i;`
   - 更新 re-exports

### 阶段 2: RV64M 重构（优先级高）

1. **创建目录结构**
   ```
   src/isa/rv64m/
   ├── mod.rs
   ├── mul.rs
   └── div.rs
   ```

2. **迁移内容**
   - `mul.rs` → `mul.rs`
   - `div.rs` → `div.rs`

### 阶段 3: RV64A 重构（优先级中）

1. **创建目录结构**
   ```
   src/isa/rv64a/
   ├── mod.rs
   ├── lr_sc.rs
   └── amo.rs
   ```

2. **迁移内容**
   - `lr_sc.rs` → `lr_sc.rs`
   - `amo.rs` → `amo.rs`

### 阶段 4: RV64F/RV64D 重构（优先级中）

1. **创建目录结构**
   ```
   src/isa/rv64f/
   ├── mod.rs
   ├── arith.rs
   ├── load_store.rs
   ├── compare.rs
   ├── convert.rs
   ├── classify.rs
   ├── div_sqrt.rs
   └── madd.rs

   src/isa/rv64d/
   ├── mod.rs
   ├── arith.rs
   ├── load_store.rs
   ├── compare.rs
   ├── convert.rs
   ├── classify.rs
   ├── div_sqrt.rs
   └── madd.rs
   ```

2. **迁移内容**
   - `f_*.rs` → `rv64f/*.rs`
   - `d_*.rs` → `rv64d/*.rs`

### 阶段 5: 清理和验证（优先级高）

1. 更新 `src/execute/mod.rs`
2. 更新 `src/isa/mod.rs`
3. 确保所有测试通过
4. 更新文档

---

## 5. 文件改动预估

### 新建文件（23个）

| 模块 | 文件 | 预估行数 |
|------|------|----------|
| rv64i/mod.rs | 模块入口 | 150行 |
| rv64i/alu.rs | 算术逻辑 | 300行 |
| rv64i/shift.rs | 移位操作 | 150行 |
| rv64i/load.rs | 加载指令 | 200行 |
| rv64i/store.rs | 存储指令 | 100行 |
| rv64i/branch.rs | 分支指令 | 250行 |
| rv64i/jump.rs | 跳转指令 | 150行 |
| rv64i/lui_auipc.rs | 高位立即数 | 80行 |
| rv64i/system.rs | 系统指令 | 350行 |
| rv64m/mod.rs | 模块入口 | 80行 |
| rv64m/mul.rs | 乘法 | 250行 |
| rv64m/div.rs | 除法 | 300行 |
| rv64a/mod.rs | 模块入口 | 80行 |
| rv64a/lr_sc.rs | LR/SC | 300行 |
| rv64a/amo.rs | AMO | 450行 |
| rv64f/mod.rs | 模块入口 | 100行 |
| rv64f/*.rs | 7个文件 | 约1500行 |
| rv64d/mod.rs | 模块入口 | 100行 |
| rv64d/*.rs | 7个文件 | 约2000行 |

**总计：约 6500 行新代码**

### 修改文件（3个）

| 文件 | 改动内容 |
|------|----------|
| `src/execute/mod.rs` | 大幅简化，仅保留 re-exports 和 Executor |
| `src/isa/mod.rs` | 添加新模块声明 |
| `tests/*.rs` | 可能需要更新 import 路径 |

### 删除文件（24个）

所有 `src/execute/*.rs` 中的独立指令文件（除 mod.rs 外）

---

## 6. 测试策略

### 6.1 保持测试不变

- 所有现有测试继续通过
- 测试文件位置不变（仍在 `tests/` 目录）
- 测试导入路径通过 re-exports 保持兼容

### 6.2 新增内联测试

每个新的 ISA 模块文件应包含内联测试（参考 rv64c 模式）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_add() {
        // 测试代码
    }
}
```

---

## 7. 风险与缓解

| 风险 | 可能性 | 影响 | 缓解措施 |
|------|--------|------|----------|
| 破坏现有 API | 低 | 高 | 保持 re-exports，确保向后兼容 |
| 测试失败 | 中 | 中 | 逐步重构，每阶段验证测试 |
| 编译错误 | 中 | 低 | 使用 Rust 编译器检查，逐步修复 |
| 代码审查困难 | 高 | 中 | 分阶段提交 PR，每阶段独立审查 |

---

## 8. 验收标准

1. **功能完整**
   - [ ] 所有现有指令功能保持不变
   - [ ] 所有测试通过 (`cargo test`)

2. **结构清晰**
   - [ ] 每个 ISA 扩展有独立目录
   - [ ] 文件按功能分类
   - [ ] 文件大小控制在 300 行以内

3. **API 兼容**
   - [ ] 现有代码无需修改即可使用新结构
   - [ ] `execute` 模块保持向后兼容

4. **代码质量**
   - [ ] 通过 `cargo fmt`
   - [ ] 通过 `cargo clippy`
   - [ ] 文档完整

---

## 9. PR 草稿

### PR 标题
```
refactor(isa): 推广 C 指令模块化模式到所有指令集

将 RV64C 的模块化组织模式应用到 RV64I/RV64M/RV64A/RV64F/RV64D，
实现统一的 ISA 模块结构，提高代码可维护性。
```

### PR 描述
```markdown
## 概述

本 PR 将 RV64C 压缩指令的模块化组织模式推广到所有其他指令集，
建立统一的 `src/isa/` 目录结构。

## 变更内容

### 新增模块
- `src/isa/rv64i/` - 基础整数指令（9个文件）
- `src/isa/rv64m/` - 乘除指令（3个文件）
- `src/isa/rv64a/` - 原子指令（3个文件）
- `src/isa/rv64f/` - 单精度浮点（8个文件）
- `src/isa/rv64d/` - 双精度浮点（8个文件）

### 文件迁移
- 从 `src/execute/` 迁移所有指令实现到 `src/isa/`
- 保持 `src/execute/mod.rs` 作为兼容层

### 保持兼容性
- 所有现有 API 通过 re-exports 保持可用
- 测试文件无需修改

## 测试

- [x] 所有单元测试通过
- [x] 所有集成测试通过
- [x] `cargo fmt` 通过
- [x] `cargo clippy` 通过

## 文件统计

- 新增：23 个文件
- 修改：3 个文件
- 删除：24 个文件（原 execute/ 下的指令文件）
- 净增：约 6500 行代码

## 破坏性变更

无。所有现有 API 保持兼容。
```

---

## 10. 时间估算

| 阶段 | 预估时间 |
|------|----------|
| 阶段 1: RV64I | 2-3 小时 |
| 阶段 2: RV64M | 1 小时 |
| 阶段 3: RV64A | 1-2 小时 |
| 阶段 4: RV64F/D | 2-3 小时 |
| 阶段 5: 清理验证 | 1-2 小时 |
| **总计** | **7-11 小时** |
