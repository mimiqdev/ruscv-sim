# RISC-V Virtual Platform (ruscv-sim)

基于 Rust 的 RISC-V 虚拟原型平台，支持 RV32I 指令集和 SystemC TLM2.0 接口。

## 项目概述

本项目旨在实现一个高性能、可扩展的 RISC-V 指令集模拟器 (ISS)，遵循敏捷开发和 TDD 思想。

### 核心特性

- **RV32I 基础指令集支持** - 实现所有 47 条 RV32I 指令
- **SystemC TLM2.0 接口** - 支持事务级建模
- **模块化架构** - 取指、译码、执行分离
- **TDD 开发模式** - 高测试覆盖率

## 项目结构

```
ruscv-sim/
├── Cargo.toml
├── README.md
├── TODO.md
└── src/
    ├── lib.rs          # 主模块导出
    ├── main.rs         # CLI 工具
    ├── core/           # 核心执行引擎
    ├── decode/         # 指令译码
    ├── execute/        # 指令执行
    ├── memory/         # 存储器接口
    └── tlm/            # TLM2.0 接口
```

## 构建和运行

```bash
# 构建项目
cargo build

# 运行测试
cargo test

# 运行示例
cargo run
```

## Sprint 进度

### Sprint 1 (进行中)
- [x] 项目初始化
- [x] Rust 项目结构
- [x] 基础框架代码
- [ ] 核心 ISS 实现
- [ ] 单元测试
- [ ] 原型演示

## 依赖

- Rust 1.70+
- Cargo

## 参考资料

- [RISC-V 指令集手册](https://github.com/riscv/riscv-isa-manual)
- [RISC-V ISA Sim (spike)](https://github.com/riscv-software-src/riscv-isa-sim)
- [RISC-V VP (TLM 参考)](https://github.com/agra-uni-bremen/riscv-vp)

## 许可证

MIT License
