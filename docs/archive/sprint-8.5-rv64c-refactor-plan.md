# Sprint 8.5: RV64C 指令模块结构重构规划

## 1. 当前结构分析

### 1.1 文件组织结构

```
src/isa/rv64c/
├── mod.rs              # 136行 - 模块入口，导出所有子模块和执行函数
├── decoder_16bit.rs    # 803行 - 16位压缩指令解码器
├── c0_quadw.rs         # 306行 - C0象限指令 (Load/Store)
├── c1_addiw.rs         # 245行 - C1象限Word指令 (ADDIW, SUBW, ADDW)
├── c1_arith.rs         # 525行 - C1象限算术指令
├── c1_shift.rs         # 322行 - C1象限移位指令 (SRLI, SRAI, SLLI)
├── c2_move.rs          # 201行 - C2象限跳转/移动指令 (JR, JALR, EBREAK)
└── c2_stack.rs         # 422行 - C2象限栈指令 (SLLI, LWSP, LDSP, SWSP, SDSP)
```

### 1.2 当前指令分布

| 文件 | 指令 | 象限 | 功能类别 |
|------|------|------|----------|
| c0_quadw.rs | C.ADDI4SPN, C.LW, C.LD, C.SW, C.SD | C0 | Load/Store + SP操作 |
| c1_addiw.rs | C.ADDIW, C.SUBW, C.ADDW | C1 | RV64特有Word算术 |
| c1_arith.rs | C.ADDI, C.LI, C.LUI, C.ADDI16SP, C.ANDI, C.MV, C.ADD, C.SUB, C.XOR, C.OR, C.AND | C1 | 立即数/寄存器算术逻辑 |
| c1_shift.rs | C.SRLI, C.SRAI, C.SLLI | C1/C2 | 移位操作 |
| c2_move.rs | C.JR, C.JALR, C.EBREAK | C2 | 跳转/系统 |
| c2_stack.rs | C.SLLI, C.LWSP, C.LDSP, C.SWSP, C.SDSP | C2 | 栈访问 + 移位 |

### 1.3 存在的问题

#### 问题1：代码重复
- `exec_c_slli` 在 `c1_shift.rs` (第95-110行) 和 `c2_stack.rs` (第30-45行) 中重复定义
- 两处实现几乎完全相同，增加了维护成本

#### 问题2：组织方式不一致
- 部分文件按象限分组（c0_quadw, c1_addiw, c2_stack）
- 部分文件按功能分组（c1_arith, c1_shift, c2_move）
- 缺乏统一的组织原则

#### 问题3：命名不清晰
- `c0_quadw` 暗示"象限0"，但实际包含的是 Load/Store 指令
- `c1_addiw` 只包含3个Word指令，命名过于具体
- `c2_stack` 包含移位指令 C.SLLI，不完全符合"栈"命名

#### 问题4：模块边界模糊
- C.SLLI 属于 C2 象限但被放在 `c1_shift.rs`
- C.ADDI4SPN 是 SP 相关操作但和 Load/Store 混在一起

#### 问题5：与 Codegen 的兼容性
当前结构对代码生成不够友好：
- 分散的文件需要多个模板
- 命名空间不统一（c0_, c1_, c2_ 前缀）
- 指令查找需要跨多个文件

## 2. 参考模式分析

### 2.1 execute 模块组织模式

```
src/execute/
├── mod.rs          # 统一管理导出
├── r_type.rs       # 按指令格式分组
├── i_type.rs       # 按指令格式分组
├── b_type.rs       # 分支指令
├── s_type.rs       # 存储指令
├── mul.rs          # RV64M - 按扩展分组
├── div.rs          # RV64M - 按扩展分组
├── amo.rs          # RV64A - 按功能分组
├── lr_sc.rs        # RV64A - 特殊功能
├── f_arith.rs      # RV64F - 按功能分组
├── f_load_store.rs # RV64F - 按功能分组
...
```

**设计原则**：
1. **按功能分组**：算术、逻辑、访存、分支各自独立
2. **按扩展分组**：M扩展、A扩展、F扩展各自有明确边界
3. **集中导出**：mod.rs 统一管理所有公开接口
4. **一致的命名**：无冗余前缀，直接描述功能

### 2.2 理想的 RV64C 组织模式

参考 execute 模块，RV64C 应该按**功能类别**组织，而非象限：

| 功能类别 | 包含指令 | 原象限分布 |
|----------|----------|------------|
| Load/Store | C.LW, C.LD, C.SW, C.SD, C.LWSP, C.LDSP, C.SWSP, C.SDSP | C0, C2 |
| Arithmetic | C.ADDI, C.ADDIW, C.ADD, C.ADDW, C.SUB, C.SUBW, C.LI, C.LUI, C.ADDI16SP, C.ADDI4SPN | C0, C1, C2 |
| Logical | C.AND, C.OR, C.XOR, C.ANDI | C1 |
| Shift | C.SLLI, C.SRLI, C.SRAI | C1, C2 |
| Branch | C.J, C.JR, C.JALR, C.BEQZ, C.BNEZ | C1 |
| System | C.EBREAK, C.NOP | C1, C2 |

## 3. 建议的新结构

### 3.1 目标文件组织

```
src/isa/rv64c/
├── mod.rs              # 模块入口，统一导出
├── decoder.rs          # 重命名：decoder_16bit.rs → decoder.rs
├── memory.rs           # 合并：C0 Load/Store + C2 Stack Load/Store
├── arithmetic.rs       # 合并：所有算术指令
├── logic.rs            # 新增：逻辑运算指令
├── shift.rs            # 合并：所有移位指令（去重）
├── branch.rs           # 新增：所有分支跳转指令
├── system.rs           # 新增：系统指令
└── immediate.rs        # 新增：立即数操作指令（可选）
```

### 3.2 指令映射表

| 新文件 | 包含指令 | 来源文件 |
|--------|----------|----------|
| **memory.rs** | C.LW, C.LD, C.SW, C.SD, C.LWSP, C.LDSP, C.SWSP, C.SDSP, C.ADDI4SPN | c0_quadw.rs + c2_stack.rs |
| **arithmetic.rs** | C.ADD, C.ADDW, C.SUB, C.SUBW, C.ADDI, C.ADDIW, C.MV | c1_arith.rs + c1_addiw.rs |
| **immediate.rs** | C.LI, C.LUI, C.ADDI16SP, C.ANDI | c1_arith.rs |
| **logic.rs** | C.AND, C.OR, C.XOR | c1_arith.rs |
| **shift.rs** | C.SLLI, C.SRLI, C.SRAI | c1_shift.rs (去重) |
| **branch.rs** | C.J, C.JR, C.JALR, C.BEQZ, C.BNEZ | decoder.rs 直接解码为32位分支指令 |
| **system.rs** | C.EBREAK, C.NOP | c2_move.rs |

### 3.3 模块结构设计

#### mod.rs 结构

```rust
//! RV64C Compressed Instruction Extension
//!
//! This module implements the RISC-V Compressed (C) extension for RV64.

// 子模块
declare_mod! {
    pub mod decoder;      // 解码器
    pub mod memory;       // Load/Store 指令
    pub mod arithmetic;   // 算术指令
    pub mod immediate;    // 立即数指令
    pub mod logic;        // 逻辑指令
    pub mod shift;        // 移位指令
    pub mod branch;       // 分支指令
    pub mod system;       // 系统指令
}

// 公开导出
pub use decoder::{COpcode, CQuadrant, CompressedDecoder};
pub use memory::*;
pub use arithmetic::*;
pub use immediate::*;
pub use logic::*;
pub use shift::*;
pub use branch::*;
pub use system::*;

// 工具函数
pub use decoder::{is_compressed, instruction_length};
```

#### 各子模块结构示例

**memory.rs**:
```rust
//! RV64C Memory Access Instructions
//!
//! Includes both compressed register-relative and stack-pointer-relative
//! load/store operations.

use crate::core::CoreState;
use crate::execute::ExecuteError;
use crate::memory::MemoryInterface;

// C0 Quadrant - Register-relative
pub fn exec_c_lw(...)
pub fn exec_c_ld(...)
pub fn exec_c_sw(...)
pub fn exec_c_sd(...)
pub fn exec_c_addi4spn(...)

// C2 Quadrant - Stack-pointer relative
pub fn exec_c_lwsp(...)
pub fn exec_c_ldsp(...)
pub fn exec_c_swsp(...)
pub fn exec_c_sdsp(...)

#[cfg(test)]
mod tests { ... }
```

**shift.rs** (去重后):
```rust
//! RV64C Shift Instructions
//!
//! All shift operations: SLLI, SRLI, SRAI

use crate::core::CoreState;
use crate::execute::ExecuteError;

/// C.SLLI - Shift left logical immediate (C2 quadrant)
pub fn exec_c_slli(...)

/// C.SRLI - Shift right logical immediate (C1 quadrant)
pub fn exec_c_srli(...)

/// C.SRAI - Shift right arithmetic immediate (C1 quadrant)
pub fn exec_c_srai(...)

#[cfg(test)]
mod tests { ... }
```

## 4. 需要修改的文件列表

### 4.1 新增文件

| 文件路径 | 行数预估 | 说明 |
|----------|----------|------|
| `src/isa/rv64c/memory.rs` | ~350行 | 合并 C0 Load/Store + C2 Stack |
| `src/isa/rv64c/arithmetic.rs` | ~250行 | 合并算术指令 |
| `src/isa/rv64c/immediate.rs` | ~200行 | 立即数操作指令 |
| `src/isa/rv64c/logic.rs` | ~150行 | 逻辑运算指令 |
| `src/isa/rv64c/shift.rs` | ~180行 | 移位指令（去重版）|
| `src/isa/rv64c/branch.rs` | ~50行 | 分支指令（或保持解码器直接展开）|
| `src/isa/rv64c/system.rs` | ~80行 | 系统指令 |

### 4.2 修改文件

| 文件路径 | 修改类型 | 修改内容 |
|----------|----------|----------|
| `src/isa/rv64c/mod.rs` | 重写 | 更新模块声明和导出 |
| `src/isa/rv64c/decoder.rs` | 重命名+修改 | decoder_16bit.rs → decoder.rs，简化 |

### 4.3 删除文件

| 文件路径 | 说明 |
|----------|------|
| `src/isa/rv64c/c0_quadw.rs` | 功能合并到 memory.rs |
| `src/isa/rv64c/c1_addiw.rs` | 功能合并到 arithmetic.rs |
| `src/isa/rv64c/c1_arith.rs` | 功能拆分到 arithmetic.rs, immediate.rs, logic.rs |
| `src/isa/rv64c/c1_shift.rs` | 功能合并到 shift.rs |
| `src/isa/rv64c/c2_move.rs` | 功能拆分到 branch.rs, system.rs |
| `src/isa/rv64c/c2_stack.rs` | 功能合并到 memory.rs, shift.rs |

## 5. 代码改动预估

### 5.1 改动统计

| 类别 | 数量 | 说明 |
|------|------|------|
| 新增文件 | 7个 | 按功能组织的子模块 |
| 修改文件 | 2个 | mod.rs, decoder.rs |
| 删除文件 | 6个 | 旧象限分组文件 |
| 代码行数变化 | -200行 | 删除重复代码后净减少 |
| 测试用例迁移 | ~50个 | 保持测试覆盖率 |

### 5.2 影响范围分析

#### 高影响区域
1. **mod.rs 导出接口**：所有 `pub use` 语句需要更新
2. **解码器**：保持向后兼容，但内部可简化
3. **测试用例**：需要迁移到新模块

#### 中影响区域
1. **Executor 集成**：检查是否有直接引用 RV64C 执行函数
2. **文档**：README 和代码文档需要更新

#### 低影响区域
1. **外部 API**：公开接口保持不变
2. **Codegen**：改进兼容性，无破坏性变更

### 5.3 风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| 功能回退 | 低 | 高 | 完整的测试迁移，功能验证 |
| 编译错误 | 中 | 低 | 渐进式重构，频繁编译检查 |
| 性能下降 | 低 | 低 | 仅文件重组，无逻辑变更 |
| 文档不一致 | 中 | 低 | 同步更新所有文档 |

## 6. Codegen 兼容性设计

### 6.1 代码生成模板设计

为新结构设计的 codegen 模板：

```rust
// codegen/template_rv64c.rs

/// RV64C 指令模板
pub struct CInstructionTemplate {
    pub name: &'static str,
    pub quadrant: CQuadrant,
    pub funct3: u8,
    pub module: &'static str,  // 新增：目标模块
    pub exec_fn: &'static str,
}

/// 按模块分组的指令
pub const C_MEMORY_INSTRUCTIONS: &[CInstructionTemplate] = &[
    CInstructionTemplate { 
        name: "C.LW", 
        module: "memory",
        exec_fn: "exec_c_lw",
        ... 
    },
    // ...
];

pub const C_ARITHMETIC_INSTRUCTIONS: &[CInstructionTemplate] = &[
    CInstructionTemplate { 
        name: "C.ADD", 
        module: "arithmetic",
        exec_fn: "exec_c_add",
        ... 
    },
    // ...
];
```

### 6.2 代码生成工作流

```
1. 解析指令定义 (YAML/JSON)
2. 按 module 字段分组
3. 为每个模块生成代码：
   - memory.rs: Load/Store 函数
   - arithmetic.rs: 算术函数
   - logic.rs: 逻辑函数
   - shift.rs: 移位函数
4. 更新 mod.rs 导出
5. 生成对应测试
```

## 7. 重构实施计划

### Phase 1: 准备 (1天)
- [ ] 创建功能分支 `refactor/rv64c-module-structure`
- [ ] 备份当前文件
- [ ] 创建新文件框架（空模块）
- [ ] 更新 `mod.rs` 引入新模块

### Phase 2: 功能迁移 (2天)
- [ ] **Day 1**: 迁移 memory.rs（合并 c0_quadw + c2_stack 访存部分）
- [ ] **Day 1**: 迁移 shift.rs（去重 SLLI）
- [ ] **Day 2**: 迁移 arithmetic.rs（合并 c1_arith + c1_addiw 算术部分）
- [ ] **Day 2**: 迁移 logic.rs, immediate.rs, system.rs, branch.rs

### Phase 3: 解码器更新 (1天)
- [ ] 重命名 decoder_16bit.rs → decoder.rs
- [ ] 更新解码器文档
- [ ] 验证解码功能完整

### Phase 4: 清理 (1天)
- [ ] 删除旧文件（c0_*, c1_*, c2_*）
- [ ] 更新 mod.rs 导出
- [ ] 运行完整测试套件
- [ ] 修复任何编译警告

### Phase 5: 文档 (1天)
- [ ] 更新模块文档
- [ ] 更新 CHANGELOG.md
- [ ] 更新架构文档
- [ ] 创建重构总结

## 8. PR 草稿内容

### PR 标题
```
refactor(rv64c): 重构压缩指令模块结构，按功能分组
```

### PR 描述

```markdown
## 概述
重构 RV64C 压缩指令模块的组织结构，从按象限分组改为按功能分组，
提高代码可维护性和与 codegen 的兼容性。

## 变更内容

### 主要变更
- **文件重组**: 6个象限分组文件 → 7个功能分组文件
- **消除重复**: 移除 `exec_c_slli` 的重复实现
- **命名改进**: 使用描述性文件名替代象限编号
- **解码器简化**: decoder_16bit.rs → decoder.rs

### 文件映射
| 旧文件 | 新文件 | 说明 |
|--------|--------|------|
| c0_quadw.rs | memory.rs | Load/Store 指令 |
| c1_addiw.rs | arithmetic.rs | Word 算术指令 |
| c1_arith.rs | arithmetic.rs + immediate.rs + logic.rs | 拆分按功能 |
| c1_shift.rs | shift.rs | 统一移位指令 |
| c2_move.rs | branch.rs + system.rs | 拆分按功能 |
| c2_stack.rs | memory.rs + shift.rs | 拆分按功能 |

### 接口变更
- **无破坏性变更**: 公开 API 保持不变
- **模块导出**: mod.rs 统一导出所有执行函数
- **向后兼容**: 现有调用代码无需修改

## 测试
- [x] 所有现有测试通过
- [x] 测试覆盖率保持不变
- [x] 无功能回退

## 文档
- [x] 模块文档更新
- [x] 架构文档更新
- [x] CHANGELOG 更新

## 关联 Issue
Closes #<issue_number>
```

### 检查清单

```markdown
## 提交前检查清单

### 代码质量
- [ ] `cargo fmt` 通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo check` 编译通过

### 测试
- [ ] `cargo test --lib` 通过
- [ ] `cargo test --doc` 通过
- [ ] `cargo test --test '*'` 通过

### 文档
- [ ] 模块级文档完整
- [ ] 公开 API 有文档注释
- [ ] 示例代码可运行

### 兼容性
- [ ] 无破坏性 API 变更
- [ ] 下游代码无需修改
```

## 9. 长期收益

### 9.1 可维护性提升
- **清晰的组织结构**: 按功能而非象限分组，更符合直觉
- **减少重复代码**: 消除 SLLI 等重复实现
- **一致的命名**: 统一的命名规范

### 9.2 Codegen 友好
- **模块化生成**: 可为每个功能模块独立生成代码
- **模板简化**: 减少条件分支，直接映射到模块
- **易于扩展**: 新增指令只需修改对应模块

### 9.3 测试改进
- **聚焦测试**: 每个模块的测试职责单一
- **并行测试**: 模块独立，可并行运行测试
- **覆盖率跟踪**: 更容易跟踪各功能类别的覆盖率

## 10. 附录

### A. 完整的文件内容映射

详见附件：`rv64c-refactor-mapping.md`

### B. 新模块依赖图

```
mod.rs
├── decoder.rs (独立)
├── memory.rs (依赖: CoreState, MemoryInterface, ExecuteError)
├── arithmetic.rs (依赖: CoreState, ExecuteError)
├── immediate.rs (依赖: CoreState, ExecuteError)
├── logic.rs (依赖: CoreState, ExecuteError)
├── shift.rs (依赖: CoreState, ExecuteError)
├── branch.rs (依赖: CoreState, ExecuteError)
└── system.rs (依赖: CoreState, ExecuteError)
```

### C. 参考实现示例

详见：`examples/rv64c_new_structure.md`

---

**规划完成日期**: 2026-02-01  
**规划版本**: v1.0  
**作者**: Kimi Code CLI
