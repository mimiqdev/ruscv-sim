# ruscv-sim

[![CI](https://github.com/mimiqdev/ruscv-sim/actions/workflows/ci.yml/badge.svg)](https://github.com/mimiqdev/ruscv-sim/actions)
[![Codecov](https://codecov.io/gh/mimiqdev/ruscv-sim/branch/main/graph/badge.svg)](https://codecov.io/gh/mimiqdev/ruscv-sim)

Rust 实现的 RISC-V 指令集模拟器。公开入口是一个 ELF 加载/执行 CLI；仓库里还有 MMU、TLM 风格总线、外设和 GDB RSP 等可独立测试的组件，它们不一定已经接到这条执行路径上。

长期方向是可扩展的 RISC-V 虚拟原型平台，而不是只做一个解释器内核。

## 现在能做什么

`ruscv-sim run` 可以加载 RISC-V ELF64，在平坦内存上执行，并通过 HTIF `tohost` 或 UART 观察程序结束和输出。

- 取指/译码/执行分离
- ELF64 加载、`tohost` 退出、可选 Spike 风格 commit log
- UART 16550（固定映射 `0x10000000`）和 HTIF（`0x40008000`）
- 指令实现覆盖 RV64I / M / A / F / D / C、CSR 和陷阱相关逻辑
- 另有 Sv39 MMU/TLB、TLM 总线、CLINT/PLIC，以及 GDB RSP / 断点 / 观察点组件

公开 CLI 目前只有 `run`。GDB 服务器和交互式调试器作为 library API 存在，没有单独的调试 binary。ELF 核心循环按 32 位指令取指；压缩指令、页表翻译和 TLM 外设总线还没有全部接到这条路径。

外部验证已有 [A7 有界能力证据](docs/verification/a7-capability-assessment.md)：
A7 时项目自建 ELF 和 ACT4 选择集各自有 51 个用例；A8 新增七个项目原子 guest，
项目自建用例现为 58 个，ACT4 冻结选择仍是独立的 51 个 RV64I 非陷阱用例。
它们不代表完整 ISA、原子扩展认证、特权/MMU、中断或 OS 支持。A7 的普通取指和
整数/浮点 load/store 已接入验证过的 raw physical boundary；A8 之后 AMO/LR/SC
在标准路径上通过同一验证数据端口发送原子 envelope（每指令恰好一个），旧
`RiscvCore::new` 构造保留类型化兼容性适配路径；原子行为的认证边界仍受 A7/A8
评估约束。

已批准的后继计划见 [`docs/dev-plan.md`](docs/dev-plan.md)：A9（Hart 事实与安全的 N=1 Machine 生命周期），限定在现有单 Hart native／flat 配置。该详细合同和文档轮换于 2026-09-29 获明确授权在所需检查和独立审查后合并，并于合并时成为唯一当前合同；合并前 main 上仍由 A8 保持 Current。轮换本身不实现 A9 Rust 代码。A8 的[有界收尾评估](docs/verification/a8-closeout-assessment.md)及随轮换生效的[完整合同归档记录](docs/archive/milestones/a8-single-hart-atomic-physical-convergence.md)不代表 RV64A 认证或完整 VP 集成。后续技术顺序参考[post-A7 路线图提案](docs/proposals/post-a7-roadmap.md)；该 Draft 不是另一份当前合同。

## 快速开始

需要 Stable Rust，以及 `rustfmt` / `clippy`。跑项目自带的裸机 ELF 测试还需要 `riscv64-unknown-elf` 工具链。

推荐使用仓库内的完整开发镜像，它还包含本机 C/C++、裸机 RISC-V
交叉工具链和 Spike：

```bash
docker build -t ruscv-sim-dev .
docker run --rm -it --init -v "$PWD:/workspace" ruscv-sim-dev
```

也可以直接拉取 `main` 上自动验证并发布的镜像：

```bash
docker pull ghcr.io/mimiqdev/ruscv-sim-dev:main
```

发布镜像同时支持 `linux/amd64` 和 `linux/arm64`，Apple Silicon 会直接使用
ARM64 版本。裸机交叉工具链面向 freestanding guest，不预装目标端 libc 或
libstdc++。

镜像内容、版本策略和非默认 UID/GID 用法见 [开发环境说明](docs/development-environment.md)。

```bash
cargo build --release
cargo test --all-features
cargo run -- --help
```

执行一个 ELF：

```bash
cargo run -- run path/to/program.elf --max-cycles 100000
```

常用选项：

| 选项 | 作用 |
| ------ | ------ |
| `-m, --max-cycles <N>` | 周期上限 |
| `-t, --tohost <ADDR>` | 退出探测地址，例如 `0x40008000` |
| `-v, --verbose` | 详细日志 |
| `--log-commits <FILE>` | 写出 Spike 兼容的 commit log |

Commit log 在 Hart 完成指令后消费其事实记录，不重新取指或在 Runner 中比较寄存器快照。
文本格式保留原有的 GPR 展示与省略规则，并非完整的 FPR/CSR/内存效果导出。
未启用日志时不构造每条指令的观察记录。日志写入失败会在运行结果的 `error`
中报告，但不会撤销已退休指令、存储或已识别的客户退出码。
即使客户退出码为 0，CLI 也将日志报告错误显示为 `FAILED` 并以非零状态退出；
结果中的 `Exit Code` 仍保留客户退出码。

退出码来自客户程序的 `tohost` 值。超时、加载失败或模拟器内部错误时进程以非零状态退出。

## N=1 Machine 与公共 facade（A9 T2/T3）

`machine::Machine` 直接拥有一个 Hart 与现有 native 或 flat Platform 的组合。
`elf::LoadImage::parse` 只提供不可变的段、零填充和地址元数据；Machine 协调安装，
Platform 放置字节。CLI／`load_and_run` 的 native 配置与 `RiscVSimulator` 的 flat 配置
均通过同一 Machine 组合和 Hart 调用路径执行；Runner 仍选择预算、日志、退出优先级和
artifact 策略。flat 地址仍为存储偏移，不获得 native UART／HTIF 映射；native 不可读签名
仍省略，flat 则报告 artifact 错误。原有构造器及 core-only `reset` helper 保留，
不能被当作安全 fresh Machine reset。

```rust,ignore
use ruscv_sim::elf::LoadImage;
use ruscv_sim::machine::{Machine, MachineConfig, PlatformKind};
use std::sync::Arc;

let image = Arc::new(LoadImage::parse(&elf_bytes)?);
let machine = Machine::new(MachineConfig::new(PlatformKind::Flat));
machine.install(image)?;               // 初始配置已静止；安装后保持静止
machine.resume()?;                     // 开放普通执行与 host RAM 写入
{
    let turn = machine.step(false)?;    // 同一 Hart 语义；返回未经退出策略分类的事实
    assert!(turn.hart().observation.is_none());
}                                      // 消费边界 receipt 后才能推进/完成 drain
machine.request_quiesce()?;
machine.try_drain()?;                  // 在途工作存在时返回拒绝，不阻塞等待
machine.fresh_reset()?;                // 保留 image/config，恢复 RAM/Hart/devices
machine.teardown()?;
```

`memory()`、`write_mem` 和 `clear_tohost` 在普通执行期间保持真实的 RAM/version 写入。
克隆 handle 使用存储偏移（native 也仅访问 RAM，不访问设备）；fresh reset/reload 后旧
handle 仍可写旧 RAM，但不能影响新一代存储或退出事件。当前代写入在 quiesce 期间明确拒绝，
已准入的写入继续完成。`inspect()` 提供协调快照；handle/`read_mem` 仅是 volatile RAM 视图。
签名元数据不存在时 `inspect().signature` 为 `None`；零长度签名保留元数据并返回
`Some(Ok(Vec::new()))`，不要求地址可映射；非空不可读区域返回 `Some(Err(...))`，
由 Runner 决定最终 artifact 策略。控制状态编辑必须已 drain，且使 reservation 失效，
不能导入旧一代的 LR snapshot。

`RiscVSimulator::run`／`step` 返回后保持可续跑状态，不自动 fresh reset 或确认 drain。
新增 `fresh_reset()` 会停止新写入、等待已准入的 host 写入、确认 drain，然后恢复已安装
image 的 RAM／零填充与 Hart，保留预算、verbosity 和手工 tohost 选择，并开放新一代写入。
`load_elf` 同样协调替换；无效 image 保持原状态。旧 `memory()` 克隆不能影响新 image。

`state()`／`state_mut()`／`memory()` 仍返回实际借用引用，不是快照或 guard 替代 API。
`state_mut()` 先排空已准入写入；编辑阶段的新 host 写入明确拒绝，下一次 facade
状态／内存访问或执行请求结束编辑阶段并重新开放普通写入。编辑可保留当前 reservation，
但不能导入不同的 opaque LR token。未知完成无法通过编辑、重载或 fresh reset 清除；
无法返回 `Result` 的旧 `state_mut()` 在不能证明 drain 时 panic，其他生命周期操作返回错误。
未实现的合法指令仍是失败的 started slot，不退休、不退出；该兼容 facade 允许 host
修补和下一次独立 run／step 请求（不重试同一 run 的失败 slot），不适用于 host／未知完成。

返回的 `MachineTurn` 保持已接受的边界/观察工作存活；保留 receipt 或正在执行 callback/sink
时不能完成 drain 或重置。未知完成始终拒绝重用、reset 和 teardown；目前没有 adapter
终止证明/恢复接口。若调用者丢弃未知或无法证明安全的 owner，其失败 domain 会被保留到
进程退出，而不是假装已安全清理。这是资源保留的失败策略，不是强制恢复或完整 VP 声明。

## 仓库结构

```text
ruscv-sim/
├── src/
│   ├── main.rs          # CLI
│   ├── executor.rs      # ELF 加载、系统总线、UART、HTIF
│   ├── elf.rs           # ELF64 解析
│   ├── core/            # 架构状态与取指循环
│   ├── decode/          # 32 位译码
│   ├── execute/         # 执行分发
│   ├── isa/             # RV64I/M/A/F/D/C 实现
│   ├── csr/             # CSR
│   ├── fpu/             # 浮点寄存器与 NaN boxing
│   ├── memory/          # 平坦内存
│   ├── mmu/             # Sv39 / TLB
│   ├── tlm/             # TLM 风格总线抽象
│   ├── peripherals/     # CLINT、PLIC、UART 16550
│   └── debug/           # GDB RSP、断点、观察点
├── tests/               # Rust 集成测试与自建裸机程序
├── benches/             # 基准
├── docs/                # 设计与开发文档
└── scripts/             # 编译和对比脚本
```

文档入口见 [docs/README.md](docs/README.md)，目标产品架构见 [docs/architecture/README.md](docs/architecture/README.md)。

## 测试

```bash
cargo test --all-features
cargo test --test test_add_direct
```

集成测试里既有纯 Rust 用例，也有 `tests/bare-metal-riscv-test/` 下的汇编程序（包括 RV64I、RV64M 和新增的 RV64A 原子 guest）。A6 的五个陷阱 guest 会在临时目录中 fresh assemble/link，并分别验证 `ruscv-sim run`、`load_and_run` 和 `RiscVSimulator`；缺少交叉编译器时本地用例明确打印 `[SKIP]`，CI 通过 `RISCV_REQUIRE_RISCV_TOOLCHAIN=1` 将缺失视为失败。完整 ELF 脚本会在隔离的 `target/` 输出目录中重建，并检查真实 `_start` 入口；`cargo test` 成功不能冒充完整 ELF suite。详见 [裸机验证指南](docs/verification/bare-metal-tests.md) 和 [A6 证据矩阵](docs/verification/a6-capability-assessment.md)。

## 参考

- [RISC-V ISA 手册](https://github.com/riscv/riscv-isa-manual)
- [RVA23 Profile](https://github.com/riscv/riscv-profiles)
- [Spike](https://github.com/riscv-software-src/riscv-isa-sim)

## 许可证

[MIT License](LICENSE)
