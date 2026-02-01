# PR 草稿: Sprint 8.5 - 推广 C 指令模块化模式

## 标题

```
refactor(isa): 推广 C 指令模块化模式到所有指令集

将 RV64C 的模块化组织模式应用到 RV64I/RV64M/RV64A/RV64F/RV64D，
实现统一的 ISA 模块结构，提高代码可维护性。
BREAKING CHANGE: 无，保持完全向后兼容
```

## PR 描述

### 概述

本 PR 将 RV64C 压缩指令的模块化组织模式推广到所有其他指令集，建立统一的 `src/isa/` 目录结构。这是一次纯重构，不改变任何功能行为，仅优化代码组织。

### 动机

当前代码存在以下问题：
1. **文件过大**: `r_type.rs` (651行)、`d_convert.rs` (885行)、`amo.rs` (798行)
2. **职责不单一**: `r_type.rs` 同时包含 RV64I 和 RV64M 指令
3. **缺少统一结构**: 不同指令集散落在 `execute/` 目录
4. **难以维护**: 大型文件不利于代码审查和并行开发

### 解决方案

参考 RV64C 的成功模式：
- 每个 ISA 扩展有独立目录 (`src/isa/rv64*/`)
- 文件按功能分类（alu.rs, load.rs, branch.rs 等）
- 文件大小控制在 300 行以内
- 模块文档完整，测试内联

### 变更详情

#### 新增模块结构

```
src/isa/
├── mod.rs (更新)
├── rv64c/ (已存在，作为参考模式)
├── rv64i/ (新建)
│   ├── mod.rs
│   ├── alu.rs
│   ├── shift.rs
│   ├── load.rs
│   ├── store.rs
│   ├── branch.rs
│   ├── jump.rs
│   ├── lui_auipc.rs
│   └── system.rs
├── rv64m/ (新建)
│   ├── mod.rs
│   ├── mul.rs
│   └── div.rs
├── rv64a/ (新建)
│   ├── mod.rs
│   ├── lr_sc.rs
│   └── amo.rs
├── rv64f/ (新建)
│   ├── mod.rs
│   ├── arith.rs
│   ├── load_store.rs
│   ├── compare.rs
│   ├── convert.rs
│   ├── classify.rs
│   ├── div_sqrt.rs
│   └── madd.rs
└── rv64d/ (新建)
    ├── mod.rs
    ├── arith.rs
    ├── load_store.rs
    ├── compare.rs
    ├── convert.rs
    ├── classify.rs
    ├── div_sqrt.rs
    └── madd.rs
```

#### 文件迁移

| 原文件 | 新位置 | 行数 |
|--------|--------|------|
| `execute/r_type.rs` | `isa/rv64i/alu.rs` + `shift.rs` | 651 → 300+150 |
| `execute/i_type.rs` | `isa/rv64i/load.rs` + 立即数部分 | 637 → 200+100 |
| `execute/s_type.rs` | `isa/rv64i/store.rs` | 118 → 100 |
| `execute/b_type.rs` | `isa/rv64i/branch.rs` | 366 → 250 |
| `execute/j_type.rs` | `isa/rv64i/jump.rs` | 203 → 150 |
| `execute/u_type.rs` | `isa/rv64i/lui_auipc.rs` | 137 → 80 |
| `execute/system.rs` | `isa/rv64i/system.rs` | 698 → 350 |
| `execute/mul.rs` | `isa/rv64m/mul.rs` | 420 → 250 |
| `execute/div.rs` | `isa/rv64m/div.rs` | 511 → 300 |
| `execute/amo.rs` | `isa/rv64a/amo.rs` | 798 → 450 |
| `execute/lr_sc.rs` | `isa/rv64a/lr_sc.rs` | 515 → 300 |
| `execute/f_*.rs` (7个) | `isa/rv64f/*.rs` | 约2000 → 约1500 |
| `execute/d_*.rs` (7个) | `isa/rv64d/*.rs` | 约3000 → 约2000 |

#### 兼容性保证

`src/execute/mod.rs` 保留作为兼容层：

```rust
// 所有现有 API 继续可用
pub use crate::isa::rv64i::{exec_add, exec_sub, ...};
pub use crate::isa::rv64m::{exec_mul, exec_div, ...};
pub use crate::isa::rv64a::{exec_amoadd, ...};
pub use crate::isa::rv64f::{exec_fadd_s, ...};
pub use crate::isa::rv64d::{exec_fadd_d, ...};
```

### 测试

- [x] `cargo test --lib` - 所有单元测试通过
- [x] `cargo test --test '*'` - 所有集成测试通过
- [x] `cargo fmt --all -- --check` - 格式化检查通过
- [x] `cargo clippy --all-targets` - 代码检查通过
- [x] `cargo build --release` - Release 构建成功

### 性能影响

无性能影响。本次重构仅移动代码位置，不改变执行逻辑。

### 破坏性变更

**无**。所有现有 API 通过 re-exports 保持兼容。

### 文档更新

- [x] 更新了 `TODO.md`
- [x] 创建了详细规划文档 `docs/sprint-8.5-plan.md`
- [x] 添加了模块级文档注释

### 审查检查清单

- [ ] 代码遵循 Rust 风格指南
- [ ] 模块文档完整
- [ ] 测试覆盖率未降低
- [ ] 无重复代码
- [ ] 错误处理正确

### 相关 Issue

Closes: #[相关 Issue 编号]

---

## 审查讨论指南

### 重点审查内容

1. **模块结构**: 文件分类是否合理？
2. **API 兼容**: re-exports 是否完整？
3. **代码重复**: 是否有可以提取的公共代码？
4. **文档质量**: 模块文档是否清晰？

### 可能的讨论点

1. **命名**: `rv64i/alu.rs` vs `rv64i/arithmetic.rs`?
2. **粒度**: 文件拆分粒度是否合适？
3. **测试**: 是否需要添加更多内联测试？
