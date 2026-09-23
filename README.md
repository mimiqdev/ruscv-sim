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
项目自建 ELF 和 ACT4 选择集各自保持 51 个用例边界；ACT4 仍只是 RV64I
非陷阱证据，不代表完整 ISA、原子、特权/MMU、中断或 OS 支持。A7 的普通取指和
整数/浮点 load/store 已接入验证过的 raw physical boundary；A8 之后 AMO/LR/SC
在标准路径上通过同一验证数据端口发送原子 envelope（每指令恰好一个），旧
`RiscvCore::new` 构造保留类型化兼容性适配路径；原子行为的认证边界仍受 A7/A8
评估约束。

当前合同是已批准的 A8 单 Hart 原子收敛里程碑；T4 正在验证标准 facade 的原子桥接退出，不能据此声称整个里程碑已完成。后续方向见
[post-A7 short-term technical development roadmap proposal](docs/proposals/post-a7-roadmap.md)，该文档只是 Draft
阶段性技术实施规划，不是整体产品路线图，也不替换 `docs/dev-plan.md`。

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

退出码来自客户程序的 `tohost` 值。超时、加载失败或模拟器内部错误时进程以非零状态退出。

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
