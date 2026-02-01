# RISC-V 虚拟原型平台 - 敏捷开发计划

**文档版本**: v3.0 (重大更新)  
**核心原则**: 纯 Rust 自研，不依赖 C++ 项目 (不使用 Spike)  
**目标 Profile**: RVA23 (RV64IMAFDC + Zicsr + Zifencei)  
**指令数量**: 927+ 条需要纯 Rust 实现  

---

## 1. 工作量重新评估

### 1.1 纯 Rust 自研 vs 复用 Spike 的工作量对比

| 模块 | 复用 Spike | 纯 Rust 自研 | 工作量增幅 |
|------|------------|--------------|------------|
| 指令译码器 | 参考 C++ 逻辑 | 完全重写 Rust 版 | **3-4x** |
| 指令执行器 | 参考实现 (927+ 条) | 完全重写 + 测试 | **4-5x** |
| CSR 框架 | 移植 C++ 类 | Rust trait 重新设计 | **2-3x** |
| TLM2.0 抽象层 | 不需要 | 从零设计实现 | **新增** |
| 内存子系统 | 参考 MMU 设计 | 纯 Rust 重实现 | **2x** |
| 外设模型 | 参考 riscv-vp | 纯 Rust 重实现 | **1.5x** |
| 测试框架 | 复用 testsuite | 编写 Rust 测试 | **2x** |

### 1.2 总体工作量评估

| 类别 | 指令/模块数 | 每条/个工时 | 总工时 | Sprint 估算 |
|------|-------------|-------------|--------|-------------|
| **RV64I 基础指令** | 47 条 | 4-6h | 188-282h | 2.5 Sprint |
| **RV64M 扩展** | 8 条 | 4-6h | 32-48h | 0.5 Sprint |
| **RV64A 扩展** | 22 条 | 4-6h | 88-132h | 1.5 Sprint |
| **RV64F 扩展** | 26 条 | 5-7h | 130-182h | 2 Sprint |
| **RV64D 扩展** | 17 条 | 5-7h | 85-119h | 1.5 Sprint |
| **RV64C 扩展** | 71 条 | 3-5h | 213-355h | 3 Sprint |
| **CSR 框架** | 50+ 个 | 4-6h | 200-300h | 3 Sprint |
| **内存子系统** | MMU/TLB | - | 240h | 2 Sprint |
| **TLM2.0 抽象层** | 新设计 | - | 160h | 1.5 Sprint |
| **外设模型** | 4 个 | - | 200h | 2 Sprint |
| **调试支持** | GDB RSP | - | 160h | 1.5 Sprint |
| **集成测试** | 全覆盖 | - | 200h | 2 Sprint |
| **性能优化** | 优化 | - | 120h | 1 Sprint |

**总计**: 约 2000-2600 工时

### 1.3 Sprint 数量建议

| 方案 | Sprint 数 | 周数 | 说明 |
|------|-----------|------|------|
| **原计划** | 8 | 16 | 严重低估，已知风险 |
| **保守估计** | 16 | 32 | 充足缓冲 |
| **推荐方案** | 14 | 28 | 合理压缩 |
| **乐观估计** | 12 | 24 | 高风险 |

**推荐**: **14 Sprint (28 周)**，包含：
- 基础架构: 1 Sprint
- 核心指令集: 9 Sprint (RV64I:2 + M:0.5 + A:1 + F:1.5 + D:1 + C:2 + CSR:1)
- 内存子系统: 1.5 Sprint
- TLM2.0 抽象层: 0.5 Sprint
- 外设模型: 1 Sprint
- 调试支持: 1 Sprint
- 集成测试: 1 Sprint
- 性能优化: 0.5 Sprint + 风险缓冲: 1 Sprint

---

## 2. Sprint 总览 (v3.0)

| Sprint | 周期 | 主题 | 目标 | 工时估算 |
|--------|------|------|------|----------|
| 1 | Week 1-2 | **基础架构搭建** | 项目骨架、构建系统、基础模块 | 120h |
| 2 | Week 3-4 | **RV64I 基础指令 (上)** | 译码器框架、加减法、位运算 | 140h |
| 3 | Week 5-6 | **RV64I 基础指令 (下)** | 加载存储、分支、跳转 | 140h |
| 4 | Week 7-8 | **CSR 框架** | CSR trait、系统寄存器、特权模式 | 160h |
| 5 | Week 9-10 | **RV64M + RV64A** | 乘除指令、原子操作 | 140h |
| 6 | Week 11-12 | **RV64F 浮点** | 单精度浮点指令、FPU | 140h |
| 7 | Week 13-14 | **RV64D 双精度** | 双精度浮点指令 | 120h |
| 8 | Week 15-16 | **RV64C 压缩 (上)** | 16位压缩指令译码 | 140h |
| 9 | Week 17-18 | **RV64C 压缩 (下)** | 压缩指令执行、测试验证 | 120h |
| 10 | Week 19-20 | **内存子系统** | MMU、TLB、页表遍历 | 160h |
| 11 | Week 21-22 | **TLM2.0 + 外设** | TLM 抽象层、CLINT、PLIC | 160h |
| 12 | Week 23-24 | **调试支持** | GDB RSP、断点、CLI | 140h |
| 13 | Week 25-26 | **集成测试** | 完整功能测试、回归测试 | 160h |
| 14 | Week 27-28 | **优化 & 发布** | 性能优化、风险缓冲、v1.0 | 140h |

---

## 3. Sprint 详细计划

### 3.1 Sprint 1: 基础架构搭建

**目标**: 搭建项目骨架、构建系统、基础模块、CI/CD

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | Cargo.toml |  | workspace 编译成功 |
| 代码 | main.rs | src/ | 程序入口可用 |
| 代码 | logger.rs | src/core/ | 支持 5 级日志 |
| 代码 | config.rs | src/core/ | YAML 解析成功 |
| 代码 | error.rs | src/core/ | 错误链支持 |
| 代码 | lib.rs | src/lib/ | 模块导出正确 |
| 文档 | README.md |  | 项目说明完整 |
| 文档 | CONTRIBUTING.md |  | 开发指南完整 |
| CI | ci.yml | /.github/workflows/ | 构建+测试通过 |
| 测试 | lib_test.rs | src/tests/ | 20+ tests pass |
| 可运行 | ruscv-sim | target/release/ | CLI 可执行 |

### 验收标准
- [ ] 功能测试：`cargo test` 20+ 测试通过
- [ ] 集成测试：`cargo build --release` 成功
- [ ] 性能测试：编译时间 < 2min，冷启动 < 500ms
- [ ] 代码质量：clippy 无 error，rustfmt 格式通过
- [ ] CI/CD：GitHub Actions 全部绿色

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 验收标准 |
|------|------|----------|
| 创建 Cargo workspace | 4h | workspace 结构 |
| 配置基础依赖 | 8h | cargo build 成功 |
| 实现日志系统 | 8h | 支持多级别日志 |
| 实现配置系统 | 16h | YAML 解析成功 |
| 实现错误处理 | 8h | 错误链支持 |
| 搭建 CI/CD | 16h | 自动构建测试 |
| 编写 README | 4h | 项目说明 |
| 搭建基础 benchmark | 8h | perf 基线 |

**技术决策点**:
- [ ] 错误处理: anyhow + thiserror
- [ ] 配置格式: YAML (serde_yaml)
- [ ] 日志: log + env_logger
- [ ] 测试: built-in test + proptest

---

### 3.2 Sprint 2: RV64I 基础指令 (上)

**目标**: 实现指令译码器框架、加减法、位运算指令

**背景**: 不使用 Spike 的译码器，需要纯 Rust 实现表驱动译码

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | decoder.rs | src/isa/ | 47 条指令译码 |
| 代码 | executor.rs | src/isa/ | 执行逻辑框架 |
| 代码 | register.rs | src/cpu/ | x0-x31 寄存器文件 |
| 代码 | add_sub.rs | src/isa/rv64i/ | ADD/SUB/SLT/SLTU |
| 代码 | bitwise.rs | src/isa/rv64i/ | AND/OR/XOR/SLL/SRL/SRA |
| 代码 | lui.rs | src/isa/rv64i/ | LUI/AUIPC |
| 代码 | instruction.rs | src/isa/ | 指令数据结构 |
| 文档 | decoder-design.md | docs/ | 译码器设计文档 |
| 测试 | rv64i_add_sub_test.rs | src/tests/ | 50 tests pass |
| 测试 | rv64i_bitwise_test.rs | src/tests/ | 40 tests pass |
| 测试 | rv64i_lui_test.rs | src/tests/ | 30 tests pass |

### 验收标准
- [ ] 功能测试：ADD/SUB/SLT/SLTU/AND/OR/XOR/SLL/SRL/SRA/LUI/AUIPC 测试全部通过
- [ ] 集成测试：指令译码 → 执行流程正确
- [ ] 性能测试：单指令译码 < 100ns
- [ ] 代码质量：指令覆盖率 > 85%，clippy 无 error
- [ ] 集成验收：寄存器文件与译码器正确对接

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 设计指令编码数据结构 | 16h | - |
| 实现 Opcode 表驱动解码 | 24h | - |
| 实现寄存器文件 | 8h | - |
| 实现 ADD/SUB/SLT/SLTU | 16h | 解码器 |
| 实现 AND/OR/XOR | 8h | 解码器 |
| 实现 SLL/SRL/SRA | 8h | 解码器 |
| 实现 LUI/AUIPC | 8h | 解码器 |
| 编写指令测试 | 32h | 指令实现 |
| 性能基准测试 | 8h | 测试 |

**指令译码设计**:

```rust
// 指令描述符
pub struct Instruction {
    pub name: &'static str,
    pub opcode: u7,
    pub funct3: Option<u3>,
    pub funct7: Option<u7>,
    pub executor: fn(decoder: &mut Decoder, insn: u32) -> ExecutionResult,
}

// 译码流程
pub fn decode(insn: u32) -> Option<Instruction> {
    let opcode = insn.bits(0..7);
    let funct3 = insn.bits(12..15);
    let funct7 = insn.bits(25..32);
    
    // 查表匹配
    INSTRUCTION_TABLE
        .iter()
        .find(|i| i.opcode == opcode 
            && i.funct3.map_or(true, |f| f == funct3)
            && i.funct7.map_or(true, |f| f == funct7))
        .cloned()
}
```

---

### 3.2.5 Sprint 2.5: 指令查找优化

**目标**: 设计高效的 decode → execute 映射

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | dispatch_table.rs | src/decode/ | O(1) 查找表 |
| 代码 | bit_ops.rs | src/decode/ | 位操作优化 |
| 文档 | decode-optimization.md | docs/ | 优化方案文档 |
| 测试 | dispatch_test.rs | src/tests/ | 20 tests pass |

### 验收标准
- [ ] 功能测试：O(1) 指令查找表实现
- [ ] 性能测试：指令分发延迟 < 50ns
- [ ] 代码质量：覆盖率 > 80%
- [ ] 集成验收：与译码器正确对接

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 设计 O(1) 查找表 | 8h | - |
| 实现指令分发机制 | 8h | 查找表 |
| **指令解码优化** (PR Review) | 16h | - |
| 评估位操作优化 | 4h | - |
| 查找表预计算 | 4h | - |
| 实现指令缓存机制 | 8h | - |
| 测试验证 | 8h | 全部实现 |

---

### 3.3 Sprint 3: RV64I 基础指令 (下)

**目标**: 实现加载存储、分支、跳转指令

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | load.rs | src/isa/rv64i/ | LB/LH/LW/LD/LBU/LHU/LWU |
| 代码 | store.rs | src/isa/rv64i/ | SB/SH/SW/SD |
| 代码 | branch.rs | src/isa/rv64i/ | BEQ/BNE/BLT/BLTU/BGE/BGEU |
| 代码 | jump.rs | src/isa/rv64i/ | JAL/JALR |
| 代码 | pseudo.rs | src/isa/rv64i/ | 伪指令 (NOP/MV/LA 等) |
| 测试 | load_test.rs | src/tests/ | 80 tests pass |
| 测试 | branch_test.rs | src/tests/ | 60 tests pass |
| 测试 | jump_test.rs | src/tests/ | 40 tests pass |
| 测试 | pseudo_test.rs | src/tests/ | 20 tests pass |
| 可运行 | rv64i-bench | target/release/ | 200+ 指令/周期 |

### 验收标准
- [ ] 功能测试：全部 47 条 RV64I 指令测试通过 (load 80 + branch 60 + jump 40 + pseudo 20 = 200 tests)
- [ ] 集成测试：加载存储 → 内存子系统集成正确
- [ ] 性能测试：CPI < 1.5 (无内存访问时)
- [ ] 代码质量：指令覆盖率 > 90%
- [ ] 集成验收：与 Sprint 1 架构正确对接

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 实现 LB/LH/LW/LD | 24h | 寄存器文件 |
| 实现 LBU/LHU/LWU | 16h | LB/LH/LW |
| 实现 SB/SH/SW/SD | 16h | 加载指令 |
| 实现 BEQ/BNE/BLT | 24h | - |
| 实现 BGE/BLTU/BGEU | 16h | BEQ/BNE |
| 实现 JAL 链接 | 16h | - |
| 实现 JALR 间接跳转 | 16h | JAL |
| 实现伪指令 | 8h | 基础指令 |
| 加载存储测试 | 24h | 全部实现 |

---

### 3.3.5 Sprint 3.5: 模块化重构 + RV64I 规划 ✅ **DONE**

**目标**: 按指令类型拆分 execute 模块，规划 RV64I 扩展

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | r_type.rs | src/execute/ | R-type 指令 |
| 代码 | i_type.rs | src/execute/ | I-type 指令 |
| 代码 | s_type.rs | src/execute/ | S-type 指令 |
| 代码 | b_type.rs | src/execute/ | B-type 指令 |
| 代码 | u_type.rs | src/execute/ | U-type 指令 |
| 代码 | j_type.rs | src/execute/ | J-type 指令 |
| 文档 | rv64i-plan.md | docs/ | RV64I 扩展规划 |
| 测试 | execute_mod_test.rs | src/tests/ | 每个模块独立测试 |

### 验收标准
- [x] 功能测试：按指令类型拆分 execute 模块 ✅
- [x] 集成测试：每个文件独立测试通过 ✅
- [x] RV64I 规划：完成 RV32I 与 RV64I 差异分析 ✅
- [x] 代码质量：覆盖率 > 80% ✅
- [x] 文档增强：所有公共 API 添加 examples ✅

### Sprint 完成检查清单
- [x] 所有验收标准 ✅
- [x] 代码审查通过 ✅
- [x] CI/CD 绿色 ✅
- [x] 文档完整 ✅
- [x] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 | 状态 |
|------|------|------|------|
| 拆分 r_type.rs | 4h | - | ✅ 完成 |
| 拆分 i_type.rs | 4h | - | ✅ 完成 |
| 拆分 s_type.rs | 4h | - | ✅ 完成 |
| 拆分 b_type.rs | 4h | - | ✅ 完成 |
| 拆分 u_type.rs | 4h | - | ✅ 完成 |
| 拆分 j_type.rs | 4h | - | ✅ 完成 |
| **RV64I 扩展规划** (PR Review) | 24h | - | ✅ 完成 |
| 分析 RV32I 与 RV64I 差异 | 8h | - | ✅ 完成 |
| 识别需要扩展的指令 | 8h | - | ✅ 完成 |
| 添加 64 位专用指令测试规划 | 8h | - | ✅ 完成 |
| **文档增强** (PR Review - 持续任务) | 8h | - | ✅ 完成 |
| 为所有公共 API 添加 examples | 4h | - | ✅ 完成 |
| 添加模块级文档说明设计决策 | 4h | - | ✅ 完成 |

**RV64I 扩展指令清单**:
- ADDIW, SLLIW, SRLIW, SRAIW (I-type 64-bit)
- ADDW, SUBW, SLLW, SRLW, SRAW (R-type 64-bit)

---

### 3.4 Sprint 4: CSR 框架 ✅ **COMPLETED**

**目标**: 实现 CSR trait 框架、系统寄存器、特权模式切换

**背景**: 需要从零设计 Rust 版本的 CSR 框架（不能直接移植 C++）

**完成时间**: 2026-01-31  
**PR**: #4 - Sprint 4: CSR Framework Implementation

### 实际产出 (Actual Deliverables)
| 类型 | 产出 | 文件路径 | 状态 |
|------|------|----------|------|
| 代码 | mod.rs | src/csr/mod.rs | ✅ 395 行，35 CSR 寄存器 |
| 代码 | system.rs | src/execute/system.rs | ✅ 210 行，6 条 CSR 指令 |
| 代码 | CoreState 集成 | src/core/mod.rs | ✅ CsrFile 集成 |
| 代码 | ExecuteError | src/execute/mod.rs | ✅ CsrError 变体 |
| 测试 | csr_basic_test.rs | tests/csr_basic_test.rs | ✅ 35 tests |
| 测试 | csr_access_test.rs | tests/csr_access_test.rs | ✅ 20 tests |
| 测试 | privilege_transition_test.rs | tests/privilege_transition_test.rs | ✅ 19 tests |

**实际实现统计**:
- **CSR 寄存器**: 35 个 (13 M-mode, 10 S-mode, 9 V-mode, 3 计数器)
- **CSR 指令**: 6 条 (CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI)
- **特权模式**: 3 种 (Machine, Supervisor, User)
- **测试总数**: 191 个 (117 lib + 74 集成，新增 94 个)
- **代码变更**: +1463 行 / -9 行，涉及 8 个文件

### 验收标准
- [x] 功能测试：CSR 读/写/Set/Clear 全部通过 ✅
- [x] 集成测试：特权模式切换正常 (M→S→U) ✅
- [x] 代码质量：覆盖率 > 85%，所有测试通过 (191/191) ✅
- [x] 集成验收：与 RV64I 指令正确对接 (CSRRW/CSRRC 等) ✅
- [ ] 性能测试：CSR 访问 < 50ns (移至 Sprint 4.5)

### Sprint 完成检查清单
- [x] 所有验收标准 ✅
- [x] 代码审查通过 ✅ (PR #4)
- [x] CI/CD 绿色 ✅
- [x] 文档完整 ✅ (PR 描述详细)
- [x] 技术债务清理 ✅

**任务分解** (实际执行):

| 任务 | 计划工时 | 实际状态 | 备注 |
|------|---------|---------|------|
| 设计 CSR trait | 16h | ✅ 完成 | 简化为单个 mod.rs |
| 实现 CSRMap 注册表 | 24h | ✅ 完成 | 基于 HashMap 实现 |
| 实现 mstatus | 16h | ✅ 完成 | 支持 MPP/SPP 位字段 |
| 实现 mie/mip | 16h | ✅ 完成 | 13 个 M-mode CSR |
| 实现 medeleg/mideleg | 8h | ✅ 完成 | 包含在 35 个 CSR 中 |
| 实现 satp | 24h | ✅ 完成 | 10 个 S-mode CSR |
| 实现 time/timeh | 8h | ✅ 完成 | 3 个计数器 CSR |
| 实现特权模式切换 | 24h | ✅ 完成 | 19 个特权测试通过 |
| CSR 测试 | 24h | ✅ 完成 | 94 个新测试 |

**设计决策**:
- ✅ 采用扁平化设计：所有 CSR 在 `mod.rs` 中实现
- ✅ 基于 HashMap 的注册表，支持动态 CSR 查找
- ✅ 原子读-修改-写操作 (CSRRS/CSRRC)
- ✅ 只读 CSR 强制保护 (hartid, mvendorid 等)

**CSR 框架设计**:

```rust
// CSR Trait 定义
pub trait Csr: Debug {
    fn addr(&self) -> u12;
    fn read(&self, cpu: &Cpu) -> u64;
    fn write(&mut self, cpu: &mut Cpu, val: u64);
    fn set(&mut self, cpu: &mut Cpu, val: u64) { /* 默认实现 */ }
    fn clear(&mut self, cpu: &mut Cpu, val: u64) { /* 默认实现 */ }
}

// CSR 字段属性
pub enum FieldAttr {
    WARL, // Write Any, Read Legal
    WIRI, // Write Ignored, Read Ignored
    WLRL, // Write Legal, Read Legal
    // ...
}
```

---

### 3.4.5 Sprint 4.5: 代码生成工具 + 性能基准

**目标**: 评估并实现代码生成，建立性能基线

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | instruction_macro.rs | src/macros/ | proc-macro 实现 |
| 代码 | template.rs | src/codegen/ | 代码生成模板 |
| 测试 | benchmark.rs | benches/ | 性能基准测试 |
| 文档 | benchmark-report.md | docs/ | 基准测试报告 |
| CI | benchmark.yml | /.github/workflows/ | CI 集成 (self-hosted) |

### 验收标准
- [ ] 功能测试：proc-macro 工具可生成指令代码
- [ ] 性能测试：建立 CPI、译码延迟、执行延迟基线
- [ ] 集成测试：基准测试集成到 CI (self-hosted runner)
- [ ] 代码质量：覆盖率 > 75%
- [ ] 文档增强：创建架构图和模块文档

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 评估 proc-macro 工具 | 8h | - |
| 实现模板生成重复代码 | 8h | proc-macro |
| **性能基准测试** (PR Review) | 24h | - |
| 实现 benchmarks/ 目录 | 8h | - |
| 使用 criterion 或 custom benchmarks | 8h | - |
| 测量: CPI、译码延迟、执行延迟 | 8h | - |
| 集成到 CI (仅 self-hosted runner) | 8h | - |
| **文档增强** (PR Review - 持续任务) | 8h | - |
| 创建 architecture diagrams | 4h | - |
| 完善模块级文档 | 4h | - |

---

### 3.5 Sprint 5: 陷阱处理 + RV64M/A 指令

**目标**: 实现陷阱处理机制 (MRET/SRET + Trap Handling + CSR 副作用)，实现乘除指令、原子操作指令

**背景**: Sprint 4 成功实现 CSR 框架，现需补充陷阱处理机制以完成特权模式完整支持

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| **陷阱处理 (优先)** | | | |
| 代码 | trap.rs | src/core/ | 陷阱处理框架 |
| 代码 | mret_sret.rs | src/execute/system.rs | MRET/SRET 指令 |
| 测试 | trap_test.rs | tests/ | 30 tests pass |
| **RV64M 乘除指令** | | | |
| 代码 | mul.rs | src/execute/ | MUL/MULH/MULHU/MULHSU |
| 代码 | div.rs | src/execute/ | DIV/DIVU/REM/REMU |
| 测试 | mul_test.rs | tests/ | 40 tests pass |
| 测试 | div_test.rs | tests/ | 40 tests pass |
| **RV64A 原子指令** | | | |
| 代码 | lr_sc.rs | src/execute/ | LR/SC |
| 代码 | amo.rs | src/execute/ | AMOADD/AMOAND/AMOOR/AMOXOR/AMOMAX/MIN |
| 测试 | amo_test.rs | tests/ | 50 tests pass |
| 文档 | rv64m-spec.md | docs/ | M 扩展说明 |
| 文档 | rv64a-spec.md | docs/ | A 扩展说明 |
| 可运行 | rv64ma-bench | target/release/ | 原子操作 < 200ns |

### 验收标准
- [ ] **陷阱处理测试**：异常捕获、中断处理、MRET/SRET 正常工作
- [ ] 功能测试：8 条 M 指令 + 22 条 A 指令全部通过
- [ ] 集成测试：原子操作与内存子系统正确对接
- [ ] 性能测试：MUL < 20ns，DIV < 100ns，AMO < 200ns
- [ ] 代码质量：覆盖率 > 80%
- [ ] 集成验收：LR/SC 保留机制正确工作

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 | 优先级 |
|------|------|------|--------|
| **实现陷阱处理框架** | 24h | Sprint 4 CSR | **P0** |
| **实现 MRET/SRET** | 16h | 陷阱处理 | **P0** |
| **实现 CSR 副作用处理** | 16h | 陷阱处理 | **P0** |
| **陷阱处理测试** | 16h | MRET/SRET | **P0** |
| 实现 MUL 乘法 | 16h | RV64I | P1 |
| 实现 MULH/MULHU/MULHSU | 16h | MUL | P1 |
| 实现 DIV/DIVU | 16h | MUL | P1 |
| 实现 REM/REMU | 16h | DIV | P1 |
| 实现 LR/SC | 24h | 原子框架 | P2 |
| 实现 AMOADD/AMOAND | 16h | LR/SC | P2 |
| 实现 AMOOR/AMOXOR | 16h | AMOADD | P2 |
| 实现 AMOMAX/MIN | 16h | AMOOR | P2 |
| M 扩展测试 | 16h | 全部实现 | P1 |
| A 扩展测试 | 16h | 全部实现 | P2 |

---

### 3.6 Sprint 6: RV64F 浮点

**目标**: 实现单精度浮点指令、FPU

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | f_register.rs | src/fpu/ | f0-f31 寄存器文件 |
| 代码 | fcsr.rs | src/fpu/ | fcsr 控制寄存器 |
| 代码 | f_load_store.rs | src/isa/rv64f/ | FLW/FSD |
| 代码 | f_arith.rs | src/isa/rv64f/ | FADD.S/FSUB.S/FMUL.S |
| 代码 | f_div_sqrt.rs | src/isa/rv64f/ | FDIV.S/FSQRT.S |
| 代码 | f_madd.rs | src/isa/rv64f/ | FMADD.S/FMSUB.S/NMADD.S/NMSUB.S |
| 代码 | f_compare.rs | src/isa/rv64f/ | FEQ/FLT/FLE |
| 代码 | f_convert.rs | src/isa/rv64f/ | FCVT.X.F/FCVT.F.X |
| 代码 | f_classify.rs | src/isa/rv64f/ | FCLASS.S |
| 文档 | rv64f-spec.md | docs/ | F 扩展说明 |
| 测试 | f_register_test.rs | src/tests/ | 30 tests pass |
| 测试 | f_arith_test.rs | src/tests/ | 50 tests pass |
| 测试 | f_convert_test.rs | src/tests/ | 30 tests pass |

### 验收标准
- [ ] 功能测试：26 条 F 指令全部通过
- [ ] 集成测试：浮点与 CSR 框架正确对接 (fcsr)
- [ ] 性能测试：FADD < 50ns，FDIV < 200ns
- [ ] 代码质量：覆盖率 > 80%，NaN 处理正确
- [ ] 集成验收：与内存子系统正确对接 (FLW/FSD)
- [ ] **代码审查跟进**:
  - [ ] RISC-V spec 引用已添加到代码注释（参考 Section 11-12）
  - [ ] NaN 传播测试已添加（见测试文件）
  - [ ] proptest 集成（可选，后续优化）

#### 代码审查跟进 - Sprint 6 Review Follow-up

**RISC-V ISA Spec 引用** (RV64F, Volume I):

| 指令 | Spec Section | 说明 |
|------|-------------|------|
| FLW/FSD | 11.3 | 加载存储指令 |
| FADD.S/FSUB.S | 11.4 | 算术指令 |
| FMUL.S/FDIV.S/FSQRT.S | 11.5 | 乘除指令 |
| FMADD.S/FMSUB.S/FNMADD.S/FNMSUB.S | 11.6 | 融合乘加 |
| FSQRT.S | 11.5.1 | 平方根 |
| FCVT.X.F/FCVT.F.X | 11.7 | 浮点转换 |
| FCVT.W.S/FCVT.L.S | 11.7.1 | Float→Int |
| FCVT.S.W/FCVT.S.L | 11.7.2 | Int→Float |
| FEQ.S/FLT.S/FLE.S | 11.8 | 比较指令 |
| FCLASS.S | 11.9 | 分类指令 |
| FCSR (FRM/FFLAGS) | 11.10-11.11 | 控制寄存器 |

**NaN 传播测试场景** (建议添加):

```
测试文件: tests/f_*_test.rs 或 src/execute/f_*_test.rs
- quiet NaN 传播 (0x7FC00000)
- signaling NaN 处理
- min/max NaN 操作
- NaN 比较结果 (always false)
- 非规格化数处理
- 无穷运算 (∞ + ∞, ∞ - ∞, etc.)
```

**proptest 集成策略** (可选，后续 Sprint):

```rust
// 在 f_arith_test.rs 中添加
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_fadd_associativity(a: f32, b: f32, c: f32) {
        // (a + b) + c ≈ a + (b + c)
        let result1 = (a + b) + c;
        let result2 = a + (b + c);
        prop_assert!((result1 - result2).abs() < 1e-6 || result1.is_nan() && result2.is_nan());
    }
}
```

**性能优化备注**:

| 优化项 | 目标 | 策略 |
|--------|------|------|
| FADD.S | 50ns→40ns | 快速路径 (非 NaN/Inf) |
| FMUL.S | 60ns→50ns | 流水线优化 |
| FDIV.S | 200ns→150ns | 近似算法 |

---

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 实现 F 寄存器文件 | 16h | RV64I |
| 实现 fcsr | 8h | CSR 框架 |
| 实现 FLW/FSD | 24h | F 寄存器 |
| 实现 FADD.S/FSUB.S | 16h | FLW |
| 实现 FMUL.S | 16h | FADD |
| 实现 FDIV.S/FSQRT.S | 24h | FMUL |
| 实现 FMADD.S 等 | 24h | FDIV |
| 实现浮点比较 | 8h | FMUL |
| 实现浮点转换 | 16h | fcsr |
| 实现浮点分类 | 8h | fcsr |
| F 扩展测试 | 24h | 全部实现 |

---

### 3.7 Sprint 7: RV64D 双精度

**目标**: 实现双精度浮点指令

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | d_load_store.rs | src/isa/rv64d/ | FLD/FSD |
| 代码 | d_arith.rs | src/isa/rv64d/ | FADD.D/FSUB.D/FMUL.D |
| 代码 | d_div_sqrt.rs | src/isa/rv64d/ | FDIV.D/FSQRT.D |
| 代码 | d_madd.rs | src/isa/rv64d/ | FMADD.D/FMSUB.D/NMADD.D/NMSUB.D |
| 代码 | d_compare.rs | src/isa/rv64d/ | FEQ.D/FLT.D/FLE.D |
| 代码 | d_convert.rs | src/isa/rv64d/ | FCVT.X.D/FCVT.D.X/FCVT.S.D/FCVT.D.S |
| 代码 | d_classify.rs | src/isa/rv64d/ | FCLASS.D |
| 代码 | nan_boxing.rs | src/fpu/ | NaN boxing 处理 |
| 文档 | rv64d-spec.md | docs/ | D 扩展说明 |
| 测试 | d_arith_test.rs | src/tests/ | 50 tests pass |
| 测试 | d_convert_test.rs | src/tests/ | 40 tests pass |
| 测试 | nan_boxing_test.rs | src/tests/ | 20 tests pass |

### 验收标准
- [ ] 功能测试：17 条 D 指令全部通过
- [ ] 集成测试：单双精度转换正确 (FCVT.S.D/FCVT.D.S)
- [ ] 性能测试：FADD.D < 60ns，FDIV.D < 300ns
- [ ] 代码质量：覆盖率 > 80%，NaN boxing 正确
- [ ] 集成验收：与 F 扩展正确复用代码

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 实现 FLD/FSD | 8h | Sprint 6 |
| 实现 FADD.D | 16h | FLW |
| 实现 FMUL.D | 16h | FADD |
| 实现 FDIV.D | 24h | FMUL |
| 实现 FMADD.D | 16h | FDIV |
| 实现浮点转换 | 24h | fcsr |
| 实现 NaN boxing | 8h | 转换指令 |
| D 扩展测试 | 24h | 全部实现 |

---

### 3.8 Sprint 8: RV64C 压缩 (上)

**目标**: 实现 16 位压缩指令译码

**背景**: 71 条压缩指令需要独立 Sprint，压缩率约 40%

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | decoder_16bit.rs | src/isa/rv64c/ | 16位指令译码器 |
| 代码 | c0_quadw.rs | src/isa/rv64c/ | C.LW/C.LD/C.SW/C.SD |
| 代码 | c1_arith.rs | src/isa/rv64c/ | C.ADDI/C.SRLI/C.SRAI/C.ANDI |
| 代码 | c1_addiw.rs | src/isa/rv64c/ | C.ADDIW |
| 代码 | c1_shift.rs | src/isa/rv64c/ | C.SRLI/C.SRAI |
| 代码 | c2_move.rs | src/isa/rv64c/ | C.MV/C.ADD/C.LI/C.LUI/C.ADDI16SP |
| 代码 | c2_stack.rs | src/isa/rv64c/ | C.ADDI4SPN |
| 文档 | rv64c-spec.md | docs/ | C 扩展说明 |
| 测试 | c_decoder_test.rs | src/tests/ | 60 tests pass |
| 测试 | c_arith_test.rs | src/tests/ | 40 tests pass |

### 验收标准
- [ ] 功能测试：C0/C1/C2 约 40 条压缩指令通过
- [ ] 集成测试：32位↔16位指令正确转换
- [ ] 性能测试：压缩指令译码 < 50ns
- [ ] 代码质量：覆盖率 > 80%
- [ ] 集成验收：与 RV64I 译码器无缝集成

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 设计压缩译码 | 16h | Sprint 2-3 |
| 实现 C0 类指令 | 24h | 译码 |
| 实现 C1 类指令 | 32h | C0 |
| 实现 C2 类指令 | 24h | C1 |
| 实现 C3/C4 伪指令 | 16h | C2 |
| 压缩译码测试 | 16h | 全部实现 |

---

### 3.9 Sprint 9: RV64C 压缩 (下)

**目标**: 剩余压缩指令、测试验证

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | c1_branch.rs | src/isa/rv64c/ | C.BEQZ/C.BNEZ |
| 代码 | c2_jalr.rs | src/isa/rv64c/ | C.JALR |
| 代码 | c1_jump.rs | src/isa/rv64c/ | C.J |
| 代码 | c0_break.rs | src/isa/rv64c/ | C.EBREAK/C.ECALL/C.MRET |
| 代码 | c1_zero.rs | src/isa/rv64c/ | C.NOP/C.ADD |
| 文档 | rv64c-test-plan.md | docs/ | 测试计划 |
| 测试 | c_branch_test.rs | src/tests/ | 30 tests pass |
| 测试 | c_riscv_compliance.rs | src/tests/ | RISC-V tests 通过 |

### 验收标准
- [ ] 功能测试：剩余约 31 条压缩指令通过 (共 71 条)
- [ ] 集成测试：RISC-V compliance tests 通过
- [ ] 性能测试：代码体积减少 > 30%
- [ ] 代码质量：覆盖率 > 85%
- [ ] 集成验收：ebreak/ecall/mret 与 CSR 对接

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 实现条件移动 | 16h | Sprint 8 |
| 实现分支压缩 | 24h | 条件移动 |
| 实现 JALR 压缩 | 16h | 分支 |
| 实现 ebreak/ecall | 8h | CSR |
| RISC-V tests 适配 | 32h | 全部实现 |
| 压缩测试验证 | 24h | tests |

---

### 3.10 Sprint 10: 内存子系统 (v1.1 - 更新版)

**目标**: 实现 MMU、TLB、页表遍历 (Sv39/Sv48)

**状态**: 🔄 设计中 (设计文档已完成)

**完成时间**: Week 19-20  
**设计文档**: [docs/memory-arch.md](./memory-arch.md)  
**对比分析**: [docs/memory-comparison.md](./memory-comparison.md)

### 产出物清单

#### 核心代码 (P0 - 必须实现)
| 类型 | 产出 | 文件路径 | 验收标准 | 优先级 |
|------|------|----------|----------|--------|
| 代码 | mod.rs | src/mmu/mod.rs | 模块入口和 trait 定义 | P0 |
| 代码 | physical.rs | src/mmu/physical.rs | 物理内存管理 | P0 |
| 代码 | pte.rs | src/mmu/pte.rs | 页表项定义和操作 | P0 |
| 代码 | sv39.rs | src/mmu/sv39.rs | Sv39 页表实现 | P0 |
| 代码 | tlb.rs | src/mmu/tlb.rs | TLB 缓存 (64 entries, 4-way) | P0 |
| 代码 | translator.rs | src/mmu/translator.rs | 地址转换引擎 | P0 |

#### 扩展代码 (P1 - 重要)
| 类型 | 产出 | 文件路径 | 验收标准 | 优先级 |
|------|------|----------|----------|--------|
| 代码 | sv48.rs | src/mmu/sv48.rs | Sv48 页表实现 | P1 |
| 代码 | pmp.rs | src/mmu/pmp.rs | PMP 内存保护 (16 entries) | P1 |
| 代码 | mmio.rs | src/mmu/mmio.rs | MMIO 支持框架 | P1 |

#### 文档 (P0)
| 类型 | 产出 | 文件路径 | 验收标准 | 优先级 |
|------|------|----------|----------|--------|
| 文档 | memory-arch.md | docs/memory-arch.md | 架构设计文档 | P0 ✅ |
| 文档 | memory-comparison.md | docs/memory-comparison.md | 参考对比分析 | P0 ✅ |

#### 测试 (P0)
| 类型 | 产出 | 文件路径 | 验收标准 | 优先级 |
|------|------|----------|----------|--------|
| 测试 | tlb_test.rs | tests/tlb_test.rs | TLB 命中/未命中/刷新测试 | P0 |
| 测试 | sv39_test.rs | tests/sv39_test.rs | Sv39 页表遍历测试 | P0 |
| 测试 | translation_test.rs | tests/translation_test.rs | 地址转换集成测试 | P0 |
| 测试 | pmp_test.rs | tests/pmp_test.rs | PMP 权限检查测试 | P1 |

### 验收标准
- [ ] 功能测试：Sv39 页表遍历正常，支持 4KB/2MB/1GB 页
- [ ] 集成测试：TLB 命中率 > 90% (测试程序)
- [ ] 性能测试：TLB 查找 < 10ns，页表遍历 < 200ns
- [ ] 代码质量：覆盖率 > 85%，Clippy 无警告
- [ ] 集成验收：与加载存储指令正确对接
- [ ] 文档验收：memory-arch.md 和 memory-comparison.md 完成

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

### 任务分解 (详细)

#### Phase 1: 基础架构 (Milestone 1 - Week 19 前半)
| 任务 | 工时 | 依赖 | 负责人 | 输出 |
|------|------|------|--------|------|
| 创建 mmu 模块目录 | 1h | - | - | src/mmu/ 目录 |
| 定义核心 trait | 4h | - | - | mod.rs |
| 实现物理内存接口 | 8h | trait | - | physical.rs |
| 实现页表项 (PTE) | 6h | trait | - | pte.rs |
| 实现 Sv39 页表 | 16h | PTE | - | sv39.rs |
| 编写基础单元测试 | 8h | Sv39 | - | sv39_test.rs |
| **Milestone 1 合计** | **43h** | | | |

#### Phase 2: TLB 和地址转换 (Milestone 2 - Week 19 后半)
| 任务 | 工时 | 依赖 | 负责人 | 输出 |
|------|------|------|--------|------|
| 实现 TLB 数据结构 | 8h | - | - | tlb.rs (struct) |
| 实现 LRU 替换策略 | 4h | TLB struct | - | tlb.rs (LRU) |
| 实现 TLB 查找/插入 | 6h | LRU | - | tlb.rs (ops) |
| 实现地址转换引擎 | 12h | TLB, Sv39 | - | translator.rs |
| 实现 TLB 刷新 (SFENCE.VMA) | 4h | TLB | - | translator.rs |
| 集成测试 | 8h | 全部 | - | translation_test.rs |
| **Milestone 2 合计** | **42h** | | | |

#### Phase 3: 扩展功能 (Milestone 3 - Week 20 前半)
| 任务 | 工时 | 依赖 | 负责人 | 输出 |
|------|------|------|--------|------|
| 实现 Sv48 页表 | 12h | Sv39 | - | sv48.rs |
| 实现 PMP 检查 | 10h | - | - | pmp.rs |
| 实现 MMIO 框架 | 8h | physical | - | mmio.rs |
| 扩展测试 | 8h | 全部 | - | pmp_test.rs |
| **Milestone 3 合计** | **38h** | | | |

#### Phase 4: 集成与优化 (Milestone 4 - Week 20 后半)
| 任务 | 工时 | 依赖 | 负责人 | 输出 |
|------|------|------|--------|------|
| 与加载存储指令集成 | 8h | translator | - | execute/mod.rs 更新 |
| 性能基准测试 | 4h | 集成 | - | benches/mmu_bench.rs |
| 文档完善 | 4h | 全部 | - | 文档更新 |
| 代码审查修复 | 4h | 审查 | - | 修复 |
| **Milestone 4 合计** | **20h** | | | |

### 总工作量: ~143 工时 (~18 工作日，符合 2 周 Sprint)

### 依赖关系图

```
mod.rs (trait)
    │
    ├──► physical.rs ──────┐
    │                       │
    ├──► pte.rs ────────┐   │
    │                    │   │
    ├──► sv39.rs ◄───────┘   │
    │       │                │
    │       ▼                │
    ├──► tlb.rs              │
    │       │                │
    │       ▼                │
    └──► translator.rs ◄─────┘
                │
                ├──► sv48.rs (P1)
                ├──► pmp.rs (P1)
                └──► mmio.rs (P1)
```

### 风险评估

| 风险 | 可能性 | 影响 | 缓解措施 |
|-----|--------|------|---------|
| Sv48 实现复杂度 | 中 | 中 | 作为 P1，可选实现 |
| TLB 性能不达标 | 低 | 中 | 预留优化时间 |
| 与现有内存接口冲突 | 中 | 高 | 提前设计接口兼容层 |
| 测试覆盖不足 | 中 | 中 | TDD 模式，持续测试 |

### 参考资源

- 设计文档: [docs/memory-arch.md](./memory-arch.md)
- 对比分析: [docs/memory-comparison.md](./memory-comparison.md)
- 参考实现: Spike (riscv-isa-sim), riscv crate
- 规范: RISC-V Privileged Spec v1.12

---

### 3.11 Sprint 11: TLM2.0 + 外设

**目标**: 实现 Rust TLM2.0 抽象层、外设模型

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | tlm_trait.rs | src/tlm/ | TLM Initiator/Target trait |
| 代码 | generic_payload.rs | src/tlm/ | GenericPayload 数据结构 |
| 代码 | time_quantum.rs | src/tlm/ | 时间管理 |
| 代码 | tlm_bus.rs | src/tlm/ | 总线互联 |
| 代码 | clint.rs | src/peripherals/ | CLINT |
| 代码 | plic.rs | src/peripherals/ | PLIC |
| 代码 | uart.rs | src/peripherals/ | UART 16550 |
| 文档 | tlm-design.md | docs/ | TLM 抽象层设计 |
| 测试 | tlm_test.rs | src/tests/ | 40 tests pass |
| 测试 | clint_test.rs | src/tests/ | 30 tests pass |
| 测试 | uart_test.rs | src/tests/ | 20 tests pass |
| 可运行 | ruscv-sim | target/release/ | 可模拟启动 |

### 验收标准
- [ ] 功能测试：TLM 传输、CLINT 中断、PLIC 中断控制器、UART 收发
- [ ] 集成测试：外设与总线正确对接
- [ ] 性能测试：TLB 事务 < 1μs
- [ ] 代码质量：覆盖率 > 75%
- [ ] 集成验收：与内存子系统正确对接

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 设计 TLM trait | 16h | - |
| 实现 Initiator trait | 16h | TLM trait |
| 实现 Target trait | 16h | TLM trait |
| 实现 GenericPayload | 16h | Initiator |
| 实现 TlmBus | 16h | Target |
| 实现 CLINT | 24h | 总线 |
| 实现 PLIC | 32h | 总线 |
| 实现 UART | 24h | 总线 |
| 外设测试 | 24h | 全部实现 |

---

### 3.12 Sprint 12: 调试支持

**目标**: 实现 GDB RSP 调试接口

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | gdb_rsp.rs | src/debug/ | GDB RSP 协议实现 |
| 代码 | breakpoint.rs | src/debug/ | 软件断点支持 |
| 代码 | watchpoint.rs | src/debug/ | 观察点支持 |
| 代码 | reg_access.rs | src/debug/ | 寄存器读写 |
| 代码 | mem_access.rs | src/debug/ | 内存读写 |
| 代码 | trace.rs | src/debug/ | 指令跟踪 |
| 代码 | cli.rs | src/debug/ | CLI 交互界面 |
| 文档 | gdb-integration.md | docs/ | GDB 集成文档 |
| 测试 | gdb_rsp_test.rs | src/tests/ | 30 tests pass |
| 测试 | breakpoint_test.rs | src/tests/ | 20 tests pass |
| 可运行 | ruscv-sim | target/release/ | GDB 连接可用 |

### 验收标准
- [ ] 功能测试：GDB RSP 协议支持，断点/观察点正常工作
- [ ] 集成测试：与 GDB 15+ 版本兼容
- [ ] 性能测试：断点切换 < 1ms
- [ ] 代码质量：覆盖率 > 75%
- [ ] 集成验收：与 CPU 状态同步正确

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 设计 GDB 协议 | 16h | - |
| 实现 GDB RSP | 32h | 协议 |
| 实现断点 | 16h | GDB |
| 实现观察点 | 16h | 断点 |
| 实现寄存器访问 | 8h | CSR |
| 实现内存访问 | 8h | 内存 |
| 实现指令跟踪 | 16h | - |
| 实现 CLI | 24h | GDB |
| 调试测试 | 16h | 全部实现 |

---

### 3.13 Sprint 13: 集成测试

**目标**: 完整功能测试、回归测试、冒烟测试

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 测试 | rv64i_full_test.rs | src/tests/ | 47 条指令全部通过 |
| 测试 | rv64m_full_test.rs | src/tests/ | 8 条 M 指令通过 |
| 测试 | rv64a_full_test.rs | src/tests/ | 22 条 A 指令通过 |
| 测试 | rv64f_full_test.rs | src/tests/ | 26 条 F 指令通过 |
| 测试 | rv64d_full_test.rs | src/tests/ | 17 条 D 指令通过 |
| 测试 | rv64c_full_test.rs | src/tests/ | 71 条 C 指令通过 |
| 测试 | csr_full_test.rs | src/tests/ | CSR 测试通过 |
| 测试 | privilege_test.rs | src/tests/ | 特权模式测试通过 |
| 测试 | smoke_test.rs | src/tests/ | 冒烟测试套件 |
| 测试 | stability_test.rs | src/tests/ | 24h 稳定性测试 |
| 报告 | test-report.md | docs/ | 完整测试报告 |
| 报告 | coverage_report | target/ | 覆盖率 > 80% |

### 验收标准
- [ ] 功能测试：RISC-V testsuite 完整通过
- [ ] 集成测试：所有模块集成正确
- [ ] 性能测试：基准测试通过
- [ ] 代码质量：代码覆盖率 > 80%，文档覆盖率 > 90%
- [ ] 集成验收：端到端测试通过

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 运行 RV64I tests | 24h | Sprint 2-3 |
| 运行 RV64M/A tests | 16h | Sprint 5 |
| 运行 RV64F/D tests | 16h | Sprint 6-7 |
| 运行 RV64C tests | 16h | Sprint 8-9 |
| 运行 CSR tests | 16h | Sprint 4 |
| 运行特权模式测试 | 16h | Sprint 4 |
| 覆盖率分析 | 8h | tests |
| 冒烟测试 | 16h | 全部功能 |
| 稳定性测试 | 16h | 冒烟通过 |
| 测试报告 | 8h | 全部完成 |

---

### 3.14 Sprint 14: 优化 & 发布

**目标**: 性能优化、风险缓冲、v1.0 发布

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | perf_opt.rs | src/optimize/ | 性能优化模块 |
| 代码 | tlb_opt.rs | src/optimize/ | TLB 优化 |
| 代码 | cache_opt.rs | src/optimize/ | 指令缓存优化 |
| 代码 | sdk.rs | src/sdk/ | SDK 基础框架 |
| 文档 | user-guide.md | docs/ | 用户使用指南 |
| 文档 | api-reference.md | docs/ | API 文档 |
| 文档 | migration.md | docs/ | 迁移指南 |
| 版本 | v1.0.0 | GitHub Release | 稳定版本发布 |
| 可运行 | ruscv-sim | target/release/ | 最终发布版 |

### 验收标准
- [ ] 功能测试：所有功能回归测试通过
- [ ] 集成测试：端到端流程验证通过
- [ ] 性能测试：CPI < 1.2 (优化后)，启动时间 < 1s
- [ ] 代码质量：覆盖率 > 80%，clippy/format 无 error
- [ ] 集成验收：SDK 可用，文档完整

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 性能分析 | 16h | - |
| TLB 优化 | 16h | Sprint 10 |
| 指令缓存优化 | 16h | - |
| SDK 基础框架 | 24h | - |
| 文档完善 | 24h | - |
| 风险缓冲 | 24h | - |
| 发布准备 | 8h | 全部完成 |
| v1.0.0 发布 | 8h | 发布准备 |

---

## 4. 里程碑计划

| 里程碑 | 时间 | 验收标准 | 风险等级 |
|--------|------|----------|----------|
| **M1: 骨架完成** | Sprint 1 | 本地编译成功，CI 通过 | 🟢 低 |
| **M2: 基础指令** | Sprint 3 | RV64I 47条通过测试 | 🟡 中 |
| **M3: CSR 完成** | Sprint 4 | 特权模式切换正常 | 🟠 高 |
| **M4: 扩展完成** | Sprint 9 | M/A/F/D/C 全部通过 | 🔴 高 |
| **M5: 内存完成** | Sprint 10 | MMU 功能正常 | 🟠 高 |
| **M6: 外设完成** | Sprint 11 | CLINT/PLIC 正常 | 🟡 中 |
| **M7: 调试完成** | Sprint 12 | GDB 可调试 | 🟢 低 |
| **M8: 测试完成** | Sprint 13 | 覆盖率 > 80% | 🟡 中 |
| **M9: 发布** | Sprint 14 | v1.0.0 发布 | 🟢 低 |

### 4.1 技术重构 Sprint (Tech Debt Sprints)

在 Sprint 2-4 之间安排技术重构，提高代码质量：

| Sprint | 位置 | 目标 | 工时 | 状态 |
|--------|------|------|------|------|
| **Sprint 2.5** | Section 3.2.5 | 指令查找优化 (O(1)查找表、位操作、缓存) | 32h | 待开始 |
| **Sprint 3.5** | Section 3.3.5 | 模块化重构 + RV64I 规划 (✅ **DONE**) | 56h | ✅ 完成 |
| **Sprint 4.5** | Section 3.4.5 | 代码生成工具 + 性能基准 (proc-macro、benchmarks) | 48h | 待开始 |

**参考**: 详细技术评论见 [docs/code-review.md](./code-review.md)

---

## 5. 风险评估与缓解 (v3.0)

### 5.1 风险清单

| 风险 | 可能性 | 影响 | 风险等级 | 当前状态 |
|------|--------|------|----------|----------|
| **CSR 框架设计缺陷** | 中 | 高 | 🔴 高 | Sprint 4 重点攻关 |
| **RV64C 压缩指令遗漏** | 中 | 高 | 🔴 高 | Sprint 8-9 重点攻关 |
| **TLM2.0 设计不合理** | 低 | 中 | 🟡 中 | Sprint 11 验证 |
| **性能不达标** | 中 | 中 | 🟡 中 | Sprint 14 优化 |
| **指令实现遗漏** | 中 | 高 | 🔴 高 | TDD + testsuite |
| **多核支持复杂** | 中 | 高 | 🔴 高 | 移出 v1.0 范围 |
| **Rust TLM 性能** | 低 | 中 | 🟡 中 | 原型验证 |

### 5.2 缓解策略

1. **TDD 开发模式**
   - 每条指令先写测试，再实现
   - 实时运行 RISC-V testsuite 验证

2. **渐进式实现**
   - 先完成 RV64I 基础
   - 再逐步添加各扩展
   - 每个 Sprint 可独立演示

3. **风险 Sprint**
   - Sprint 14 预留风险缓冲时间
   - 用于处理未预见问题

4. **外部验证**
   - RISC-V testsuite 自动化验证
   - 定期代码审查

### 5.3 风险监控

| 监控项 | 阈值 | 行动 |
|--------|------|------|
| Sprint 延迟 | >20% | 评估范围裁剪 |
| 测试覆盖率 | <70% | 增加测试时间 |
| 性能基线 | 低于预期 30% | 启动优化 Sprint |
| 缺陷数量 | >10/周 | 暂停开发，修复缺陷 |

---

## 6. 资源需求

### 6.1 人力配置

| 角色 | 人数 | 职责 | 投入 |
|------|------|------|------|
| 技术负责人 | 1 | 架构设计、技术决策、代码审查 | 100% |
| 高级开发者 | 2 | 核心模块开发 (译码器/CSR/内存) | 100% |
| 开发者 | 2-3 | 指令实现、外设开发 | 100% |
| 测试工程师 | 1 | 测试设计、执行、验证 | 50% |

### 6.2 开发环境

| 环境 | 要求 |
|------|------|
| 操作系统 | Linux (Ubuntu 20.04+) / macOS / Windows WSL2 |
| Rust | 1.80+ (2024 Edition) |
| Cargo | 1.80+ |
| Python | 3.8+ (用于测试工具) |
| GDB | 12.1+ (用于调试测试) |

### 6.3 硬件需求

| 资源 | 要求 |
|------|------|
| CPU | 4+ 核心 (推荐 8 核) |
| 内存 | 16GB+ (推荐 32GB) |
| 存储 | 10GB+ SSD |
| 网络 | 稳定网络 (下载依赖) |

---

## 8. Code Review 决策记录 (Code Review Decisions)

### 8.1 文件大小标准调整

**日期**: 2026-02-01  
**来源**: Code Review - Issue #1  
**决策**: ✅ **接受 - 更新目标为 < 600 行**

| 文件 | 当前行数 | 决策 | 理由 |
|------|----------|------|------|
| `src/isa/rv64i/system.rs` | 528 | 保持 | CSR/系统指令功能多样，当前规模合理 |
| `src/isa/rv64d/convert.rs` | 885 | 保持 | 浮点转换需要大量模式匹配 |
| `src/isa/rv64a/amo.rs` | 798 | 保持 | 原子操作逻辑复杂 |

**新标准**:
- 一般模块: < 300 行 (保持)
- 复杂指令模块 (浮点/原子/CSR): < 600 行 (放宽)
- 代码生成文件: < 1000 行 (特殊情况)

---

### 8.2 文档增强任务

**日期**: 2026-02-01  
**来源**: Code Review - Issue #2  
**决策**: ✅ **接受 - 添加到 Sprint 8.5 任务**

**任务**: 为 `rv64i/mod.rs` 添加模块级示例文档  
**参考**: 参照 `rv64c/mod.rs` 的示例格式 (第 55-66 行)  
**优先级**: 低  
**估计工时**: 2h

**示例目标**:
```rust
//! ## Usage
//!
//! ```rust
//! use ruscv_sim::isa::rv64i::execute;
//!
//! // Example: Execute LUI instruction
//! let instr = DecodedInstruction { ... };
//! execute(&instr, &mut state, &mut mem)?;
//! ```
```

---

### 8.3 提交信息规范

**日期**: 2026-02-01  
**来源**: Code Review - Issue #3 (commit `9821174`)  
**决策**: ✅ **接受 - 添加开发规范提醒**

**规范要求**:
1. 提交消息必须包含描述性正文 (body)，不能为空
2. 格式遵循: `type(scope): subject` + 空行 + body
3. Body 应说明 "what" 和 "why"，不只是 "how"

**正确示例**:
```
csr: implement mstatus register with MPP/SPP fields

- Add mstatus CSR with MPP (Machine Previous Privilege) field
- Add SPP (Supervisor Previous Privilege) field support
- Implement read/write/set/clear operations

This enables privilege mode switching between M/S/U modes.
```

---

## 9. 未来改进 (Future Improvements)

以下功能超出 v1.0 范围，计划在后续版本中实现：

### 8.1 64位原子操作 (64-bit AMO Support)

**状态**: 未实现  
**优先级**: 中  
**影响指令**: 14 条

| 指令 | funct5 | 描述 |
|------|--------|------|
| LR.D | 00010 | Load-Reserved 64-bit |
| SC.D | 00011 | Store-Conditional 64-bit |
| AMOSWAP.D | 00001 | Atomic Swap 64-bit |
| AMOADD.D | 00001 | Atomic Add 64-bit |
| AMOXOR.D | 00100 | Atomic XOR 64-bit |
| AMOAND.D | 00011 | Atomic AND 64-bit |
| AMOOR.D | 00110 | Atomic OR 64-bit |
| AMOMIN.D | 01000 | Atomic Min (signed) 64-bit |
| AMOMAX.D | 01010 | Atomic Max (signed) 64-bit |
| AMOMINU.D | 01001 | Atomic Min (unsigned) 64-bit |
| AMOMAXU.D | 01011 | Atomic Max (unsigned) 64-bit |

**依赖**: 64位内存读写接口 (read_double/write_double)  
**估计工时**: 24h

### 8.2 可选指令 (Optional Instructions)

#### WRS.NT / WRS.ST
- **状态**: 未实现  
- **优先级**: 低  
- **描述**: Wait for Register Supply (optional RVA23)
- **RISC-V Spec**: Section X.X (TBD)
- **估计工时**: 8h

#### 其他可选指令
- SFENCE.VMA (已在 CSR 框架考虑)
- HFENCE.GVMA (虚拟化扩展)

### 8.3 多核支持 (Multi-core Support)

**状态**: 已知限制  
**优先级**: 移出 v1.0

当前限制：
- 全局 reservation singleton 不支持多 hart
- 共享内存模型未实现
- 缓存一致性协议未实现

**参考**: `src/execute/lr_sc.rs` 注释说明

### 8.4 测试增强 (Testing Enhancements)

#### 64-bit 专用集成测试 (64-bit Integration Tests)
- **状态**: PR #4 Review 提出  
- **优先级**: 中  
- **描述**: 针对 RV64I 64位特性的专项集成测试
- **测试场景**:
  - 64位地址空间边界测试 (0x0000_0000_0000_0000 到 0xFFFF_FFFF_FFFF_FFFF)
  - 大立即数处理 (超过 32-bit 范围的立即数)
  - 符号扩展行为验证 (32-bit 结果符号扩展到 64-bit)
  - 64位算术溢出测试 (ADD/SUB 64-bit overflow)
  - 大偏移量分支跳转 (超过 32-bit 寻址范围)
  - 64位 CSR 读写测试 (mstatus 高 32 位，sxl/uxl 字段)
  - 混合 32/64 位指令序列测试
- **依赖**: RV64I 基础指令完成  
- **估计工时**: 16h  
- **参考**: PR #4 Review Feedback Item 4

#### Proptest 集成
- **状态**: 建议实现  
- **优先级**: 低  
- **描述**: 属性测试用于算术指令 (交换性、溢出行为)
- **依赖**: proptest crate
- **估计工时**: 16h

#### 模糊测试 (Fuzz Testing)
- 使用 libFuzzer 进行边界条件测试
- 估计工时: 8h

---

## 7. 变更日志

| 版本 | 日期 | 作者 | 变更说明 |
|------|------|------|----------|
| v1.0 | 2024-01-30 | - | 初始版本 (基于 Spike 方案) |
| v2.0 | 2024-01-30 | - | 更新 Sprint 详细计划 |
| v3.0 | 2024-01-31 | - | 重大更新：纯 Rust 自研工作量重新评估 |
| | | | - Sprint 数: 8 → 14 |
| | | | - 周数: 16 → 28 |
| | | | - 增加 RV64C 专用 Sprint |
| | | | - 增加 CSR 框架 Sprint |
| | | | - 增加 TLM2.0 抽象层 Sprint |
| | | | - 更新风险评估 |
| v3.1 | 2026-01-31 | - | 移除 Sprint 15 (语言统一) |
| | | | - 删除 Section 3.15 |
| | | | - 从 Tech Debt Sprints 表中移除 |
| v3.2 | 2026-01-31 | - | **Sprint 4 完成记录** (PR #4) |
| | | | - 标记 Sprint 4 (CSR 框架) 为 COMPLETED |
| | | | - 实现 35 个 CSR 寄存器 (M/S/V 模式) |
| | | | - 实现 6 条 CSR 指令 (CSRRW 系列) |
| | | | - 新增 94 个测试，总计 191 个测试通过 |
| | | | - 调整 Sprint 5: 陷阱处理优先于 RV64M/A |
| | | | - 陷阱处理从 Sprint 4 延迟至 Sprint 5 |
| v3.3 | 2026-01-31 | - | **清理延迟任务标记** |
| | | | - 移除 Sprint 4 中的 "⏸️ 延迟" 和 "⚠️" 标记 |
| | | | - 规范化 Sprint 5 目标和任务 |
| | | | - 新增 Sprint 5 任务: CSR 副作用处理 (P0) |
| v3.4 | 2026-02-01 | - | **Code Review 决策记录** |
| | | | - 文件大小标准: < 300行 → < 600行 (复杂模块) |
| | | | - 添加 rv64i/mod.rs 文档增强任务 |
| | | | - 添加提交信息规范要求 |
