# RISC-V ISS Simulator (ruscv-sim)

基于 Rust 实现的 RISC-V 指令集模拟器 (ISS)，支持 RV32I/RV64I 指令集，遵循 RVA23 Profile 标准。

## 项目概述

本项目旨在实现一个高性能、可扩展的 RISC-V 虚拟原型平台，采用敏捷开发和 TDD 思想。

### 核心特性

- **RVA23 Profile 支持** - RV64IMAFDC + Zicsr + Zifencei
- **模块化架构** - 取指、译码、执行分离
- **TLM2.0 接口** - 支持事务级建模
- **高性能** - 原生 Rust 实现

## 项目状态

| 组件 | 状态 | 说明 |
|------|------|------|
| 核心框架 | ✅ 完成 | 项目初始化和基础结构 |
| CI/CD | ✅ 完成 | GitHub Actions 完整配置 |
| Pre-commit Hook | ✅ 完成 | fmt + check + clippy |
| RV32I 基础指令 | 🔄 开发中 | Sprint 1 进行中 |
| 单元测试 | ✅ 12 tests | 全部通过 |

## 项目结构

```
ruscv-sim/
├── src/
│   ├── lib.rs              # 主模块导出
│   ├── main.rs             # CLI 工具
│   ├── core/               # 核心执行引擎
│   ├── decode/             # 指令译码器
│   ├── execute/            # 指令执行器
│   ├── memory/             # 存储器子系统
│   └── tlm/                # TLM2.0 接口抽象
├── tests/                  # 集成测试
├── docs/                   # 文档
│   ├── architecture.md     # 架构设计
│   ├── sprint-plan.md      # 14 Sprint 开发计划
│   ├── testing-strategy.md # 测试策略
│   └── versions.md         # 依赖版本
├── scripts/                # 工具脚本
│   └── hook/               # Git hooks
├── .github/workflows/      # CI/CD 配置
├── CLAUDE.md               # Agent 指南
└── Cargo.toml
```

## 快速开始

### 环境要求

- Rust 1.93+
- Cargo

### 构建和运行

```bash
# 构建项目
cargo build

# 运行测试
cargo test --all-features

# 运行 CLI
cargo run -- --help
```

## 开发工作流

### 代码检查

项目包含本地 Git hooks 保证代码质量：

```bash
# 安装 hooks
bash scripts/hook/install.sh

# commit 前自动运行: cargo fmt + cargo check
# push 前自动运行: cargo clippy (严格模式)
```

### CI/CD 流程

GitHub Actions 自动运行：
- **Check**: fmt + clippy + doc
- **Test**: 单元测试 + 文档测试
- **Bench**: 性能基准
- **Build**: Release 构建
- **Smoke Test**: 二进制验证

## Sprint 进度

### Sprint 1: 基础架构 ✅
- [x] 项目初始化和 Rust 项目结构
- [x] GitHub Actions CI/CD 配置
- [x] Pre-commit hooks (fmt + check)
- [x] Pre-push hooks (clippy)
- [x] CLAUDE.md 文档
- [x] 12 单元测试全部通过

### Sprint 2-14: 指令实现
- 详细计划见 [docs/sprint-plan.md](docs/sprint-plan.md)

## 测试覆盖

```bash
# 运行所有测试
cargo test --all-features

# 运行特定测试
cargo test test_lui_execution

# 查看测试覆盖
cargo tarpaulin --out lcov
```

## 依赖

### 构建依赖

- `anyhow` - 错误处理
- `thiserror` - 错误类型定义
- `tokio` - 异步支持
- `serde` - 序列化

### 开发依赖

- `proptest` - _property-based testing_
- `criterion` - 性能基准

完整依赖列表见 [docs/versions.md](docs/versions.md)

## 参考资料

- [RISC-V ISA 手册](https://github.com/riscv/riscv-isa-manual)
- [RVA23 Profile](https://github.com/riscv/riscv-profiles)
- [RISC-V Spike](https://github.com/riscv-software-src/riscv-isa-sim)

## 许可证

MIT License

---

最后更新: 2026-01-30
