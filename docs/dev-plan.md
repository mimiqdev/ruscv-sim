# Project: RISC-V ISS Simulator (ruscv-sim)

**Current Phase:** M3: 边界测试和原子性改进
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

### M3: 边界测试和原子性改进 [ACTIVE]
**Status:** Active
**Goal:** 提升外设代码质量和测试覆盖率

#### Features
- [ ] **CLINT 原子性改进**:
  - [ ] mtime: u64 → AtomicU64
  - [ ] 添加多线程安全注释
  - [ ] 更新读写方法使用 load/store

- [ ] **边界测试覆盖** (90+ 新测试):
  - [ ] CLINT: 无效 Hart ID、地址越界、最大 mtime 值
  - [ ] PLIC: 无效 hart ID、最大中断源、地址越界
  - [ ] UART: FIFO 溢出、波特率边界、连续溢出
  - [ ] TLM: 超大地址、超大长度传输、无效命令

---

### M4: 调试支持 [PLANNING]
**Status:** Planning
**Goal:** GDB RSP 调试接口可用

#### Features
- [ ] **GDB RSP 协议**: 实现断点、观察点、寄存器/内存访问
- [ ] **CLI 交互**: 提供命令行调试界面
- [ ] **集成测试**: 与 GDB 15+ 版本兼容

---

### M5: 集成测试 [PLANNING]
**Status:** Planning
**Goal:** 完整功能测试、回归测试

#### Features
- [ ] **RISC-V testsuite**: RV64IMAFDC 全指令集测试
- [ ] **冒烟测试**: 端到端功能验证
- [ ] **稳定性测试**: 24小时长时间运行
- [ ] **覆盖率**: 代码覆盖率 > 80%

---

### M6: 优化和 v1.0 发布 [PLANNING]
**Status:** Planning
**Goal:** 性能优化 + v1.0.0 正式发布

#### Features
- [ ] **性能基准**: 建立 CPI、译码/执行延迟基线
- [ ] **性能优化**: 根据基准数据进行针对性优化
- [ ] **文档完善**: 用户指南、API 参考
- [ ] **v1.0.0 发布**: 正式版本发布

---

## Change Log

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
