| 任务分解 | | |
|------|------|------|------|
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

### 3.10 Sprint 10: 内存子系统 + SDK 设计

**目标**: 实现 MMU、TLB、页表遍历，同时开始 SDK 设计（与外设模型同步）

> **参考文档**: [architecture.md - 内存子系统设计](./architecture.md#54-内存子系统设计)

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | physical_mem.rs | src/mmu/ | 物理内存管理 |
| 代码 | sv39.rs | src/mmu/ | Sv39 页表实现 |
| 代码 | sv48.rs | src/mmu/ | Sv48 页表实现 |
| 代码 | tlb.rs | src/mmu/ | TLB 缓存 |
_trans.rs | src/mmu/ | 地址转换流程 |
| 代码 | mmio.rs | src/mmu/ | MMIO 支持 |
| 代码 | mem_protect.rs | src/mmu/ | 内存保护检查 |
| 代码 | sdk.rs | src/sdk/ | SDK 基础框架 |
| 文档 | memory-arch.md | docs/ | 内存架构文档 |
| 文档 | sdk-design.md | docs/ | SDK 设计文档 |
| 测试 | tlb_test.rs | src/tests/ | 40 tests pass |
| 测试 | page_table_test.rs | src/tests/ | 50 tests pass |
| 测试 | address_trans_test.rs | src/tests/ | 30 tests pass |

### 验收标准
- [ ] 功能测试：Sv39/Sv48 页表遍历正常
- [ ] 集成测试：TLB 命中率 > 90%
- [ ] 性能测试：TLB 查找 < 5ns，页表遍历 < 100ns（在零时序模型下）
- [ ] 代码质量：覆盖率 > 85%
- [ ] 集成验收：与加载存储指令正确对接
- [ ] SDK 验收：SDK 基础框架可用，API 设计文档完成

### Sprint 完成检查清单
- [ ] 所有验收标准 ✅
- [ ] 代码审查通过 ✅
- [ ] CI/CD 绿色 ✅
- [ ] 文档完整 ✅
- [ ] 技术债务清理 ✅

**任务分解**:

| 任务 | 工时 | 依赖 |
|------|------|------|
| 设计内存模型 | 16h | - |
| 实现物理内存 | 24h | - |
| 实现 Sv39 页表 | 32h | - |
| 实现 Sv48 页表 | 24h | Sv39 |
| 实现 TLB | 32h | 页表 |
| 实现地址转换 | 24h | TLB |
| 实现 MMIO | 16h | 内存 |
| SDK 基础框架 | 24h | - |
| 内存测试 | 24h | 全部实现 |

---

### 3.11 Sprint 11: TLM2.0 + 外设

**目标**: 实现 Rust TLM2.0 抽象层、外设模型

> **参考文档**: [architecture.md - TLM2.0 抽象层设计](./architecture.md#55-tlm20-抽象层设计)

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
- [ ] 性能测试：TLB 事务 < 1μs（在零时序模型下）
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

> **参考文档**: [testing-strategy.md - 集成测试](./testing-strategy.md#23-集成测试-integration-tests) | [testing-strategy.md - 系统测试](./testing-strategy.md#24-系统测试-system-tests)

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
| 报告 | test-report.md | docs/ | 完整测试报告 |
| 报告 | coverage_report | target/ | 覆盖率 > 80% |

### 验收标准
- [ ] 功能测试：RISC-V testsuite 完整通过（参考 [testing-strategy.md - 测试数据管理](./testing-strategy.md#4-测试数据管理)）
- [ ] 集成测试：所有模块集成正确
- [ ] 性能测试：基准测试通过（运行 `cargo bench`）
- [ ] 代码质量：代码覆盖率 > 80%，文档覆盖率 > 90%
- [ ] 集成验收：端到端测试通过
- [ ] 冒烟测试：运行 `scripts/smoke_test.sh` 通过

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
| 测试报告 | 16h | 全部完成 |

---

### 3.14 Sprint 14: 优化 & 发布

**目标**: 性能优化、24h 稳定性测试、v1.0 发布

> **参考文档**: [testing-strategy.md - 性能测试](./testing-strategy.md#25-性能测试-performance-tests)

### 产出物清单
| 类型 | 产出 | 文件路径 | 验收标准 |
|------|------|----------|----------|
| 代码 | perf_opt.rs | src/optimize/ | 性能优化模块 |
| 代码 | tlb_opt.rs | src/optimize/ | TLB 优化 |
| 代码 | cache_opt.rs | src/optimize/ | 指令缓存优化 |
| 代码 | sdk.rs | src/sdk/ | SDK 完善 |
| 文档 | user-guide.md | docs/ | 用户使用指南 |
| 文档 | api-reference.md | docs/ | API 文档 |
| 文档 | migration.md | docs/ | 迁移指南 |
| 测试 | stability_test.rs | src/tests/ | 24h 稳定性测试通过 |
| 版本 | v1.0.0 | GitHub Release | 稳定版本发布 |
| 可运行 | ruscv-sim | target/release/ | 最终发布版 |

### 验收标准
- [ ] 功能测试：所有功能回归测试通过
- [ ] 稳定性测试：24h 连续运行无内存泄漏、无崩溃（运行 `scripts/stability_test.sh`）
- [ ] 集成测试：端到端流程验证通过
- [ ] 性能测试：CPI < 1.2 (优化后)，启动时间 < 1s（在零时序模型下）
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
| SDK 完善 | 24h | Sprint 10 |
| 文档完善 | 24h | - |
| 24h 稳定性测试 | 24h | 冒烟通过 |
| 风险缓冲 | 8h | - |
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

---

## 4.1 技术重构阶段 (Tech Debt Sprint)

在 Sprint 2-4 之间安排技术重构，提高代码质量：

### Sprint 2.5: 指令查找优化
- **目标**: 设计高效的 decode → execute 映射
- **任务**:
  - 设计 O(1) 查找表 (HashMap/数组)
  - 实现指令分发机制
  - 工时: 16h

### Sprint 3.5: 模块化重构
- **目标**: 按指令类型拆分 execute 模块
- **任务**:
  - 拆分 r_type.rs, i_type.rs, s_type.rs, b_type.rs, u_type.rs, j_type.rs
  - 每个文件独立测试
  - 工时: 24h

### Sprint 4.5: 代码生成工具
- **目标**: 评估并实现代码生成
- **任务**:
  - 评估 proc-macro 工具
  - 实现模板生成重复代码
  - 工时: 16h

### Sprint 15: 语言统一 (Code Cleanup)
- **目标**: 全局英文统一
- **任务**:
  - 移除所有中文注释
  - 统一错误消息为英文
  - 工时: 4h

### Code Review Comments 参考
详细技术评论见: [docs/code-review.md](./code-review.md)

---

## 5. 风险评估与缓解 (v3.2)

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

### 5.2 外部依赖风险

| 依赖 | 版本 | 风险说明 | 缓解措施 |
|------|------|----------|----------|
| **Rust 工具链** | 1.80 (固定版本) | 新版本可能引入不兼容变更 | 使用 `rust-toolchain` 文件固定版本 |
| **RISC-V testsuite** | riscv-tests (固定 commit) | 测试套件更新可能导致测试失败 | 锁定特定 commit，使用 vendored 副本 |
| **Cargo 依赖** | crates.io 依赖 | 依赖包作者可能停止维护 | 定期审查依赖健康度，关键依赖 fork 备份 |

### 5.3 缓解策略

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

5. **外部依赖管理**
   - 使用 `rust-toolchain` 文件固定 Rust 版本
   - RISC-V testsuite 使用 vendored 副本并锁定 commit
   - 定期更新依赖并审查安全公告

### 5.4 风险监控

| 监控项 | 阈值 | 行动 |
|--------|------|------|
| Sprint 延迟 | >20% | 评估范围裁剪 |
| 测试覆盖率 | <70% | 增加测试时间 |
| 性能基线 | 低于预期 30% | 启动优化 Sprint |
| 缺陷数量 | >10/周 | 暂停开发，修复缺陷 |
| 依赖安全漏洞 | 任意 | 24h 内评估并修复 |

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
| Rust | **1.80 (固定版本)** |
| Cargo | 1.80 |
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
| v3.1 | 2024-02-XX | - | 修复路径错误，更新验收标准 |
| v3.2 | 2024-02-XX | - | 修复 Review Comments 问题： |
| | | | - 修复所有文件路径 ( → ) |
| | | | - Sprint 2/3 职责划分明确 (Sprint 3 包含全部 47 条指令验收) |
| | | | - 添加测试数量基于功能点的说明 |
| | | | - 添加 architecture.md 和 testing-strategy.md 交叉引用 |
| | | | - 明确性能测试条件 (零时序模型，排除内存访问延迟) |
| | | | - 完善验收标准的可验证性 (添加自动化验证脚本引用) |
| | | | - 添加外部依赖风险小节 (Rust 1.80 固定版本) |
| | | | - SDK 设计提前至 Sprint 10 |
| | | | - 24h 稳定性测试移至 Sprint 14 |

---

## 8. Future Improvements

基于 Sprint 3.5 PR Review 建议 (2026-01-31):

### 8.1 指令解码优化
- **目标**: 提高 decode 阶段性能
- **方案**:
  - 评估位操作优化 (bit manipulation)
  - 考虑查找表预计算
  - 实现指令缓存机制
- **工时**: 16h
- **依赖**: Sprint 2-3 完成

### 8.2 性能基准测试
- **目标**: 建立性能基线，监控性能变化
- **方案**:
  - 实现 benchmarks/ 目录
  - 使用 `criterion` 或 custom benchmarks
  - 测量: CPI、译码延迟、执行延迟
  - 集成到 CI (仅 self-hosted runner)
- **工时**: 24h
- **依赖**: Sprint 2-3 完成

### 8.3 文档增强
- **目标**: 提升代码可读性和可维护性
- **方案**:
  - 为所有公共 API 添加 examples
  - 添加模块级文档说明设计决策
  - 创建 architecture diagrams
- **工时**: 16h
- **依赖**: 无

### 8.4 RV64I 扩展规划
- **目标**: 支持 RV64I (64位整数指令)
- **方案**:
  - 分析 RV32I 与 RV64I 差异
  - 识别需要扩展的指令 (ADDIW, SLLIW, etc.)
  - 添加 64位专用指令测试
- **工时**: 24h
- **依赖**: Sprint 2-3 完成

### 8.5 后续技术债务
- **Sprint 4.5**: 代码生成工具 (proc-macro)
- **Sprint 15**: 语言统一 (英文注释)
