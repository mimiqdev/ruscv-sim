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
整数/浮点 load/store 当时已接入验证过的 raw physical boundary。
Stage 4 的指令配置变更后，公开 Hart 上的所有浮点指令均触发非法指令异常，
不产生数据访问或 FPR/FCSR 效果。浮点 helper 和测试仍是独立组件，不代表 IEEE 754 支持。
A8 之后 AMO/LR/SC
在标准路径上通过同一验证数据端口发送原子 envelope（每指令恰好一个），旧
`RiscvCore::new` 构造保留类型化兼容性适配路径；原子行为的认证边界仍受 A7/A8
评估约束。

当前合同见 [`docs/dev-plan.md`](docs/dev-plan.md)：Stage 5（平台中断、WFI、建模时间与计数器），于实际轮换合并时成为唯一 Current 合同；合并前 main 仍保留 Stage 4。Stage 4 的[完整合同归档](docs/archive/milestones/stage4-virtual-memory-and-protection.md)保留全部正文，正式完成于本次轮换合并。Stage 5 决策 D0–D9 于 2026-10-16 13:08 CST 由所有者按推荐选项批准；[提案快照](docs/proposals/stage5-contract-draft.md)不是第二份当前合同。A10 由 PR #71 于 2026-10-08 激活，PR #72 于 2026-10-10 交付，其[完整合同归档](docs/archive/milestones/a10-independent-public-path-performance-test-infrastructure.md)与[实现收尾记录](docs/archive/milestones/a10-closeout-record.md)已在 A10 到 Stage 4 的轮换合并时归档。Stage 4 决策由 PR #75 记录，激活为 2026-10-11 UTC 的所有者授权；[提案快照](docs/proposals/stage4-contract-draft.md)不是第二份当前合同。A8 的[有界收尾评估](docs/verification/a8-closeout-assessment.md)及[完整合同归档记录](docs/archive/milestones/a8-single-hart-atomic-physical-convergence.md)不代表 RV64A 认证或完整 VP 集成。后续技术顺序参考[post-A7 路线图提案](docs/proposals/post-a7-roadmap.md)；该 Draft 不是另一份当前合同。

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
进程状态为 0 当且仅当结果为 `Status:     SUCCESS`。宿主只保留进程状态的低 8 位，
因此失败的客户退出码若低字节非零则以该低字节退出，低字节为零（如 `0x10000400`）时以 1 退出；
完整客户退出码仍显示在 `Exit Code` 行。`scripts/run_elf_tests.sh` 仅在进程状态为 0
且输出中唯一的状态行为 `Status:     SUCCESS` 时判定通过。

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
flat Runner 的 tohost 取样与逐字节清除均与公共 `memory()` mutex 的 guard 串行，
不会把仍在同一 guard 内分段更新的信号提前解码为退出码。取样与清除仍是分开的操作，
保留先解码、后清除的客户码；已接受的边界 receipt 允许清除在 quiesce 后完成，
不把它当作新的 host 写入准入。native callback／HTIF 优先级不变。
非空 flat 签名提取同样等待公共 clone guard 结束，再进行协调检查与读取；
等待公共 mutex 时不持有准入 gate，guard 持有者仍能完成其剩余 host 写入。
不存在／零长度签名不访问目标或等待该 mutex，native 签名检查行为不变。

Stage 4 指令配置把 FENCE.I 改为非法指令异常。WFI 在 M 模式及 TW=0 的 S 模式退休为空操作，
在 TW=1 的 S 模式及 U 模式则触发非法指令异常。SRET 和四种 SFENCE.VMA 形式遵守权限检查，
五条 RV64M word 指令执行并将 32 位结果符号扩展至 64 位。
此时没有客户指令产生 `UnsupportedLegalInstruction`。host backend 失败不退休、不退出；
即使 host 修复锁并修改内存，后续 run／step 仍须等待 fresh reset。未知完成的恢复限制不变。

这些是 Stage 4 的基础变更，尚不代表完整里程碑验收。`satp` 的 CSR 字段策略接受 Bare 和 Sv39，
`mcycle` 计完成的 Hart turn，包括陷入但不退休的指令。`minstret` 仍只计退休的指令。
显式写入其中一个计数器会替代该指令对同一计数器的隐式增量，包括写入与旧值相同的值。
`cycle` 和 `instret` 是只读影子。`mcounteren` 保持 0，所以 S 模式和 U 模式读任一影子都触发非法指令异常。
失败的 turn 不增加任一计数器。

Stage 5 U1 把 `time`（`0xC01`）做成 `mtime` 的只读影子（特权规范 20250508 §3.2.1），因此这个配置声称 Zicntr。`mcounteren` 仍为 0，所以 S 模式和 U 模式读 `cycle`、`time` 或 `instret` 都触发非法指令异常。对 `time` 的写在 M 模式也是非法指令。原生总线把 SiFive CLINT 放在 `0x0200_0000`（窗口 `0xC000`）。`mtime` 每完成一个 Hart turn 加一，写它不改变 `mcycle` 或 `minstret`，反过来也一样。失败的 turn 不增加 `mtime`。

Stage 5 U2 让 MSIP 跟随 `msip` 的 bit 0，让 MTIP 跟随无符号比较 `mtime >= mtimecmp`。依据是特权规范 20250508 §3.1.9 和 §3.2.1。这两个比特在 `mip` 里只读。MSIE 和 MTIE 可写，复位为 0。中断在取指前进入 M 模式。条件是对应使能已置位，并且 hart 低于 M 或 MIE 为 1。MSI 优先于 MTI。被打断的指令不退休。

Stage 5 U3 把 PLIC 1.0.0 放在 `0x0C00_0000`（实现窗口 `0x400000`；规范全图是 `0x0C00_0000` 到 `0x0FFF_FFFF`）。源 1 的电平是模拟器专用测试设备，在 `0x4000_0000` 的 bit 0，不是 PLIC 寄存器。机器上下文的输出驱动 MEIP。MEIP 在 `mip` 里只读。MEIE 可写，复位为 0。4 字节 claim 读返回源 ID 并清除 pending。4 字节 complete 写结束服务。电平仍为 1 时 pending 回来。陷入本身不 claim。监督上下文的线与软件可写的 SEIP 位相或。CSRRS 和 CSRRC 只改软件位。优先级是 MEI、MSI、MTI、SEI、SSI、STI。

Stage 5 U4 让合法的 WFI 先退休一次，然后等待，直到下一次取指前采样到 `mip & mie` 非零（特权规范 20250508 §3.3.3）。全局使能 MIE 不参与唤醒。特权规范 §3.1.9 在下一条指令之前判断中断，所以已经可陷入的中断在 WFI 之前进入，`mepc` 就是 WFI 的地址。`mip & mie` 非零但不可陷入时，例如 M 模式且 MIE 为 0，WFI 退休后从下一条指令继续，中断保持 pending。等待被唤醒后，hart 低于 M，或处于 M 且 MIE 为 1 时，下一边界陷入，`mepc` 是 WFI 地址加 4。continue／run 在 MTIE 已置位且 `mtimecmp` 仍在未来时把 `mtime` 跳到比较值。这次跳跃不加 `mcycle`、`minstret` 或 `cycles`。没有未来平台事件时运行以 `NoProgress` 停止，`timed_out` 为假。虚拟时间截止点的停止诊断是 `DeadlineReached`。单步一个等待中的 hart 不移动 `mtime`。截止点早于下一次 `mtimecmp` 时，跳跃停在截止点且不退休指令。公开 CLI 没有截止点参数。U 模式的 WFI，以及 TW=1 的 S 模式 WFI，仍然立即非法指令，不等待。

公开路径的页表翻译和 A/D 效果仍待完成。已启用的 SSIP、STIP 和 SEIP 在取指前进入 M 模式陷阱，不取指也不退休。`mcause` 保留中断位。S 模式下 `sip` 和 `sie` 仍读 0。
完整交付边界以当前合同和对应实现测试为准。

独立 MMU 组件的 Sv39 翻译入口在 TLB 查询和页表读取之前检查地址是否为规范地址。
不符合符号扩展规则的地址返回 `MmuError::PageFault`，其中
`PageFaultReason::NonCanonicalAddress` 保留原始访问类型和虚拟地址。
直接调用 `VirtualAddress::new` 仍返回原有的 `InvalidVirtualAddress` 解析错误。
新增错误枚举分支会使下游穷尽匹配需要更新；现有方法签名不变。
组件页表读取错误按类型保留 target、host、protocol 和 unknown 四种类别。
只有 target 拒绝返回 `AccessFault`；其余分别返回 `WalkHostFault`、
`WalkProtocolFault` 和 `WalkUnknownFault`，保留原始虚拟地址。原始访问类型仍由调用方持有。
`WalkResult` 与 `MmuError` 的新增分支也要求下游更新穷尽匹配；现有方法签名、
`WalkResult` 的 `Copy`／`Clone` 和 `MmuError` 的 `PartialEq` 保持不变。
组件错误不保留 backend 文本。
组件 walker 按特权规范 §12.3.2 第 9 步设置 A/D：只有 A（或写访问的 D）为 0 时，
才对整个叶 PTE 发出 walk 私有的 load-reserved/store-conditional，并与第 2 步检查过的值比较。
该 reservation 不是 Hart 的架构 reservation。比较或 SC 失败时不写入，并以同一页表和层级回到第 2 步。
LR/SC 失败按页表读取的同一四类分类；不支持原子能力的内存返回 `AccessFault`。
重试达到诊断上限时返回新增的 `WalkResult::AdUpdateRetryLimit`／`MmuError::WalkAdRetryLimit`，
它是模拟器诊断，不是页错误。`PhysicalMemoryInterface` 新增带默认实现的 `access_atomic`，
`PhysicalMemory` 改用 `SimpleMemory` 存储以共享已提交写记账。写访问命中 D=0 的 TLB 项时丢弃该项并重新遍历内存。
这些组件修复不表示公开 ELF 路径已启用地址翻译或已接入 walk 的 `SimulatorFailure` 分类。

返回的 `MachineTurn` 保持已接受的边界/观察工作存活；保留 receipt 或正在执行 callback/sink
时不能完成 drain 或重置。未知完成始终拒绝重用、reset 和 teardown；目前没有 adapter
终止证明/恢复接口。若调用者丢弃未知或无法证明安全的 owner，其失败 domain 会被保留到
进程退出，而不是假装已安全清理。这是资源保留的失败策略，不是强制恢复或完整 VP 声明。

[A9 有界验收评估](docs/verification/a9-closeout-assessment.md)保留原始 checkpoint／工具／artifact
身份，另列最终 PR-head 的完整 gate、独立审查，以及 merge-head CI 的 58/58 新编译
项目 CLI guest 结果；Coverage 为 skipped，不是通过。[阶段记录](docs/verification/a9-stage2-progress.md)
保留原始缺陷复现；[实现收尾记录](docs/archive/milestones/a9-closeout-record.md)明确技术验收与
正式滚动轮换的边界。[A10 基线页](docs/verification/a10-baseline.md)记录三工作负载基线（原 Darwin 观测、merged-main
重跑，以及 per-platform pin 之后的 Linux/x86_64 会话，跨主机不可比）；[A10 实现收尾记录](docs/archive/milestones/a10-closeout-record.md)记录 P0–P3′ 交付证据与待完成的
独立 PR-head 审查；[Stage 3 已批准提案快照](docs/proposals/a10-performance-test-infrastructure.md)
保留原始决策请求措辞作为历史，不是第二份 Current 合同或已实现的性能设施；无新 ACT4／性能结果声明。

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
