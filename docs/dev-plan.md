# Project: RISC-V ISS Simulator (ruscv-sim)

**Current Phase:** M5: ELF 执行闭环
**Last Updated:** 2026-02-02

---

## Milestones

### M1: Sprint 1-9 (基础 + 指令集) [COMPLETED]
**Status:** Completed
**Goal:** RV64IMAFDC 核心指令集完整实现
**Sprint Alias:** 1, 2, 3, 4, 5, 6, 7, 8, 8.5, 9

#### Features (Completed)
- [x] **Sprint 1**: 项目骨架、构建系统、基础模块
- [x] **Sprint 2-3**: RV64I 整数指令 (47条)
- [x] **Sprint 4**: CSR 框架 (35个寄存器, 6条指令)
- [x] **Sprint 5**: RV64M/A + 陷阱处理 (乘除 + 原子操作)
- [x] **Sprint 6-7**: RV64F/D 浮点单元 (单/双精度)
- [x] **Sprint 8-9**: RV64C 压缩指令 (71条)
- [x] **Sprint 8.5**: ISA 模块化重构 (31个文件, ~10000行)

#### Buffer Zone
- [ ] [BUFFER] 清理 `src/execute/` 旧模块文件 (见 archive/sprint-8.5-completed.md)

---

### M2: Sprint 10-11 (内存子系统 + 外设) [COMPLETED]
**Status:** Completed
**Goal:** Sv39 MMU/TLB + TLM2.0 外设框架
**Sprint Alias:** 10, 11

#### Features (Completed)
- [x] **Sprint 10**: MMU/TLB 内存管理 (Sv39页表, 4-way LRU TLB)
- [x] **Sprint 11**: TLM2.0 抽象层 + CLINT + PLIC + UART 16550

#### Buffer Zone
- [ ] [BUFFER] Sv48/Sv57 页表支持 (规划文档: archive/sprint-plan-archived.md 8.4节)
- [ ] [BUFFER] RV32I 纯32位仿真支持 (规划文档: archive/sprint-plan-archived.md 8.5节)

---

### M3: 边界测试和原子性改进 [COMPLETED]
**Status:** Completed
**Goal:** 提升外设代码质量和测试覆盖率

#### Features
- [x] **CLINT 原子性改进**:
  - [x] mtime: u64 → AtomicU64
  - [x] 添加多线程安全注释
  - [x] 更新读写方法使用 load/store

- [x] **边界测试覆盖** (90+ 新测试):
  - [x] CLINT: 22 个测试（新增 10 个边界测试：无效 Hart ID、地址越界、最大 mtime 值等）
  - [x] PLIC: 25 个测试（新增 14 个边界测试：无效 hart ID、最大中断源、地址越界等）
  - [x] UART: 36 个测试（新增 19 个边界测试：FIFO 溢出、波特率边界、连续溢出等）
  - [x] TLM: 19 个测试（新增 10 个边界测试：超大地址、超大长度传输、无效命令等）
  
**完成总结**: 总测试数 704 个全部通过，外设模块测试覆盖率达 95%+

---

### M4: 调试支持 [COMPLETED]
**Status:** Completed
**Goal:** GDB RSP 调试接口可用

#### Features (Completed)
- [x] **GDB RSP 服务器实现**: 完整的 Remote Serial Protocol 支持
- [x] **CLI 调试界面**: 交互式命令行调试工具
- [x] **断点管理器**: 软件/硬件断点支持
- [x] **观察点管理器**: 内存访问观察点
- [x] **版本控制的 git hooks**: 自动格式化、代码检查

---

### M5: ELF 执行闭环 [COMPLETED]
**Status:** Completed
**Goal:** 建立 ELF 加载与执行能力，支持 arch-test 运行

#### Features (Completed)
- [x] **ELF Loader**: 加载 arch-test ELF（入口点/段映射）
- [x] **运行入口**: reset → run 的最小执行流程
- [x] **Signature 导出**: 支持 signature 区域 dump
- [x] **退出机制**: tohost/exit 约定，用于自动停止测试
- [x] **System Bus & Memory Map Fix**:
  - 实现 SystemBus 路由 (RAM @ 0x80000000, UART @ 0x10000000)
  - 修复 `riscv-tests` 运行时的无效内存访问问题
  - 重构 Core 使用 `dyn MemoryInterface` 支持灵活总线

---

### M6: 测试质量强化 [PLANNING]
**Status:** Planning
**Goal:** 提升本地回归测试能力

#### Features
- [ ] **ELF 集成测试**: 最小裸机程序执行回归
- [ ] **属性测试 (proptest)**: 关键外设边界自动化探索
- [ ] **覆盖率报告**: 集成 `cargo-llvm-cov` 输出覆盖率

---

### M7: RISCOF + arch-test 集成 [PLANNING]
**Status:** Planning
**Goal:** 对接官方架构测试框架

#### Features
- [ ] **RISCOF 框架**: 安装和配置 RISCOF 测试框架
- [ ] **DUT 配置**: 创建 ruscv-sim 的 YAML 配置文件
- [ ] **Spike 集成**: 安装 Spike 作为参考模型
- [ ] **riscv-arch-test 运行**: 执行 RV64IMAFDC 架构测试
- [ ] **问题修复**: 解决测试失败问题

---

### M8: v1.0 发布 [PLANNING]
**Status:** Planning
**Goal:** 通过 riscv-arch-test，发布 v1.0.0

#### Features
- [ ] **riscv-arch-test 集成**: 使用 RISCOF 框架验证模拟器正确性
- [ ] **v1.0.0 发布**: 通过架构测试后正式发布

#### Future Goals (Post v1.0)
- [ ] [FUTURE] **性能优化**: CPI、译码/执行延迟优化
- [ ] [FUTURE] **文档完善**: 用户指南、API 参考
- [ ] [FUTURE] **模糊测试 (fuzzing)**: 外设边界模糊测试框架
- [ ] [FUTURE] **冒烟测试**: 端到端功能验证
- [ ] [FUTURE] **稳定性测试**: 长时间运行测试
- [ ] [FUTURE] **覆盖率报告**: cargo-llvm-cov 集成

#### Version Plan
| Version | Milestone | Meaning |
|---------|-----------|---------|
| v0.1.0 | Current | 基础功能就绪 |
| v0.5.0 | M4 Complete | 调试支持就绪 |
| v0.9.0-RC1 | M7 Complete | riscv-arch-test 候选 |
| **v1.0.0** | **M8 Complete** | **官方架构测试通过** |

---
## Change Log

**2026-02-03**: M5 Fix - System Bus Implementation
- 实现 `SystemBus` 以支持 `riscv-tests` 内存映射
- 修复 `hello.elf` 运行时的无效内存访问 (UART @ 0x10000000)
- 增加 `verbose` 模式用于调试输出控制
- 重构 `load_elf_file` 消除 clippy 警告

**2026-02-03**: M5 完成 - ELF 执行闭环
- M5 状态从 [ACTIVE] 改为 [COMPLETED]
- 功能列表: ELF Loader、Executor、Signature 导出、tohost/exit 机制
- PR #28 已通过 CI，待合并

**2026-02-02**: 里程碑重编号
- M5.1 → M5 (ELF 执行闭环)
- M5.2 → M6 (测试质量强化)
- M5.3 → M7 (RISCOF + arch-test 集成)
- M6 → M8 (v1.0 发布)
- M5 状态改为 [ACTIVE]
- M6/M7/M8 状态改为 [PLANNING]

**2026-02-02**: M4 完成 - 调试支持
- M4 状态从 [PLANNING] 改为 [COMPLETED]
- 添加功能列表: GDB RSP 服务器、CLI 调试界面、断点管理器、观察点管理器、git hooks

**2026-02-02**: 版本规划调整
- M6 目标简化为：仅通过 riscv-arch-test 后发布 v1.0.0
- M5 调整为：架构测试集成阶段
- 其他目标（性能优化、文档、模糊测试等）移至 Future Goals
- 添加版本规划表：v0.1.0 → v0.5.0 → v0.9.0-RC1 → v1.0.0

**2026-02-02**: Code Review 改进建议评估（M3 完成后）
评估了三项改进建议，决策如下：

| 建议 | 内容 | 决策 | 理由 |
|------|------|------|------|
| #1 | 属性测试 (proptest) | 采纳 → M6 Buffer | 已添加 `proptest = "1.0"` 依赖，M6 阶段探索性引入关键外设测试 |
| #2 | 模糊测试 (fuzzing) | 采纳 → M8 Buffer | 长期规划，需评估投入产出比后再决定 |
| #3 | 测试覆盖报告 (cargo-llvm-cov) | 采纳 → M6 Buffer | 低成本（仅安装工具）高收益（可视化覆盖率），M6 必做 |

**2026-02-02**: M3 完成 - 边界测试和原子性改进
- CLINT: `mtime` 改为 `AtomicU64`，添加多线程安全注释和原子操作方法
- CLINT 新增边界测试: 无效 Hart ID、地址越界、最大 mtime 值、原子操作等 (22 个测试)
- PLIC 新增边界测试: 无效 hart ID、最大中断源、地址越界、优先级边界等 (25 个测试)
- UART 新增边界测试: FIFO 溢出、波特率边界、连续溢出、触发级别等 (36 个测试)
- TLM 新增边界测试: 超大地址、超大长度传输、无效命令、DMI 操作等 (19 个测试)
- 总测试数: 704 个全部通过

**2026-02-02**: 创建 dev-plan.md，替换 sprint-plan.md
- 归档原 sprint-plan.md → archive/sprint-plan-archived.md
- 保留 Sprint 别名用于历史提交查找
- 未完成项使用功能描述而非 Sprint 命名

---

## Usage Notes

**Status tags:**
- `[ ]` - Pending/Not started
- [x] - Completed
- [ACTIVE] - Currently working on
- [BLOCKED] - Waiting on dependency
- [POSTPONED] - Explicitly delayed due to other priorities
- [BUFFER] - In cleanup/refactor queue

**Hierarchy:**
- **Milestone** = Major product capability (answers "where am I going?")
- **Feature** = Independently demonstrable functionality (answers "what am I building?")
- **Task** = Concrete deliverable in focused session (answers "what am I doing now?")
- Sub-tasks (implementation details) stay in code comments, not tracked here

**Energy conservation:**
- Adding a Task → Must postpone or remove another Task
- Track all scope changes in Change Log
- Buffer Zone is for discovered improvements, not commitments
