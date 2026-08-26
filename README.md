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

## 快速开始

需要 Stable Rust，以及 `rustfmt` / `clippy`。跑项目自带的裸机 ELF 测试还需要 `riscv64-unknown-elf` 工具链。

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

更细的模块说明见 [docs/architecture.md](docs/architecture.md)。

## 测试

```bash
cargo test --all-features
cargo test --test test_add_direct
```

集成测试里既有纯 Rust 用例，也有 `tests/bare-metal-riscv-test/` 下的汇编程序（当前主要是 RV64I 和 RV64M）。缺少交叉编译器时，依赖现场汇编的测试会失败，这不代表模拟器本身编译失败。

## 参考

- [RISC-V ISA 手册](https://github.com/riscv/riscv-isa-manual)
- [RVA23 Profile](https://github.com/riscv/riscv-profiles)
- [Spike](https://github.com/riscv-software-src/riscv-isa-sim)

## 许可证

[MIT License](LICENSE)
