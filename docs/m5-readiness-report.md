# M5 里程碑就绪性评估报告（riscv-arch-test）

**日期**: 2026-02-02

## 1. 目标与判定标准

根据 `docs/dev-plan.md`，M5 的目标是「集成 riscv-arch-test，准备 v1.0 发布」。
可执行的判定标准（M5 完成条件）建议如下：

- 已完成 RISCOF 框架安装与可执行配置
- 已完成 DUT（ruscv-sim）配置并可被 RISCOF 驱动
- 已完成 Spike 参考模型集成
- 能运行 RV64IMAFDC 架构测试并输出一致的 signature
- 已形成失败用例的分类与修复流程

## 2. 当前能力盘点（与 M5 的关系）

**已具备的基础能力（M1-M4）**：

- RV64IMAFDC + Zicsr + Zifencei 指令实现（`docs/dev-plan.md`、`README.md`）
- MMU/TLB 与 Sv39 页表（`src/mmu` + 360+ 测试）
- CLINT/PLIC/UART 外设模型（`src/peripherals`）
- CSR 与异常/陷阱框架（`src/csr`、`src/core/trap`）
- Debug 能力（GDB RSP + CLI）
- 测试覆盖：目前 700+ 单元/集成测试全通过

**与 arch-test 直接相关的关键能力**：

- RV64IMAFDC 指令与 CSR 可执行
- 基础异常/特权模型（mret/sret/satp 等）
- 内存读写、页表翻译

## 3. 是否已达到“可以运行 riscv-arch-test”的状态？

**结论：尚未达到。**

理由如下（存在阻塞项）：

1. **缺少 RISCOF 集成**
   - 目前仓库没有 RISCOF 相关配置文件、DUT YAML、运行脚本或示例。

2. **缺少 ELF 级测试运行器**
   - arch-test 输出为 ELF 测试程序；当前代码没有 ELF loader 或标准的 “加载 + 运行 + 终止” 流程。
   - `src/main.rs` 仅演示单指令解码，不能执行完整测试。

3. **缺少 signature 导出机制**
   - arch-test 依赖 signature 区域（内存范围）进行结果比对。
   - 目前没有机制从模拟器导出 signature 区域，也未对接 tohost 退出信号。

4. **特权模式/异常行为可能不完整**
   - 虽已具备 mret/sret/satp 等基础行为，但 arch-test 中的部分特权/异常用例可能涉及更严格的 CSR 细节（如 WARL/WIRI、mstatus 行为、非法指令/地址例外）。需要通过实际 arch-test 验证。

## 4. 缺失项（M5 阶段需补齐的关键功能）

| 类别 | 缺失项 | 影响 | 优先级 |
|---|---|---|---|
| 测试框架 | RISCOF 安装与配置 | 无法执行 arch-test | P0 |
| DUT 配置 | ruscv-sim DUT YAML 与 runner | 无法被 RISCOF 驱动 | P0 |
| 参考模型 | Spike 集成与配置 | 无对照结果 | P0 |
| 执行器 | ELF loader + 执行入口 | 无法运行 ELF 测试 | P0 |
| 结果采集 | signature 区域导出/对比 | 无法判断 pass/fail | P0 |
| 退出机制 | tohost/exit 约定 | 无法自动结束测试 | P1 |
| 特权行为 | CSR/WFI/异常细节补齐 | 可能大量失败 | P1 |

## 5. 风险与验证策略

- **最大风险**：即使 ISA 指令齐全，也可能在 CSR 行为、异常边界上被 arch-test 暴露问题。
- **验证策略**：在 M5 期间应按模块逐类运行 arch-test（I/M/A/F/D/C/Priv）并做缺陷归因表。

## 6. 结论

目前 ruscv-sim 已具备 **执行 RV64IMAFDC 指令集的大部分能力**，但 **缺少 arch-test 的运行工具链、ELF 执行闭环与 signature 对比机制**，因此 **尚未达到可运行 riscv-arch-test 的状态**。M5 应优先补齐执行与对比链路，其次针对失败用例补齐特权与异常细节。
