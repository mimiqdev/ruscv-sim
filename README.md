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
| RV32I 基础指令 | ✅ 完成 | Sprint 2 |
| RV64M 乘除法 | ✅ 完成 | Sprint 5 |
| RV64A 原子操作 | ✅ 完成 | Sprint 5 |
| CSR 框架 | ✅ 完成 | Sprint 4 |
| RV64F 浮点单元 | ✅ 完成 | Sprint 6 |
| 陷阱处理 | ✅ 完成 | Sprint 5 |
| **测试覆盖** | ✅ **200+ tests** | 全部通过 |

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
│   ├── tlm/                # TLM2.0 接口抽象
│   ├── fpu/                # 浮点运算单元 (RV64F)
│   ├── csr/                # CSR 寄存器框架
│   └── trap/               # 陷阱处理
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

### ✅ Sprint 1: 基础架构
- [x] 项目初始化和 Rust 项目结构
- [x] GitHub Actions CI/CD 配置
- [x] Pre-commit hooks (fmt + check)
- [x] Pre-push hooks (clippy)
- [x] CLAUDE.md 文档
- [x] 12 单元测试全部通过

### ✅ Sprint 2: RV32I 基础指令
- [x] LUI, AUIPC, JAL, JALR
- [x] B-type: BEQ, BNE, BLT, BGE, BLTU, BGEU
- [x] I-type: LB, LH, LW, LBU, LHU
- [x] S-type: SB, SH, SW
- [x] I-type: ADDI, SLTI, SLTIU, XORI, ORI, ANDI
- [x] R-type: ADD, SUB, SLL, SRL, SRA, SLT, SLTU, XOR, OR, AND
- [x] SLLI, SRLI, SRAI (移位指令)
- [x] FENCE, FENCE.I (内存同步)
- [x] ECALL, EBREAK (系统调用/断点)
- [x] 约 40 条基础指令实现

### ✅ Sprint 4: CSR 框架
- [x] CSR 寄存器映射和访问
- [x] CSRRW, CSRRS, CSRRC
- [x] CSRRWI, CSRRSI, CSRRCI
- [x] 读写控制位操作
- [x] 异常相关 CSR (mstatus, mie, mip, mepc, mcause, mtval)
- [x] 基础中断处理框架

### ✅ Sprint 5: RV64M + RV64A + 陷阱处理
- [x] MUL, MULH, MULHSU, MULHU (乘法)
- [x] DIV, DIVU, REM, REMU (除法)
- [x] LR.W, SC.W (原子加载/条件存储)
- [x] AMOADD, AMOXOR, AMOOR, AMOAND, AMOMIN, AMOMAX, AMOMINU, AMOMAXU
- [x] 陷阱向量处理
- [x] 异常分发和精确异常
- [x] 特权模式基础

### ✅ Sprint 6: RV64F 浮点单元
- [x] FLW, FSW (加载/存储)
- [x] FADD.S, FSUB.S, FMUL.S, FDIV.S, FSQRT.S
- [x] FMADD.S, FMSUB.S, FNMSUB.S, FNMADD.S
- [x] FMIN.S, FMAX.S
- [x] FCVT.W.S, FCVT.S.W, FCVT.WU.S, FCVT.S.WU
- [x] FMV.X.W, FMV.W.X, FCLASS.S
- [x] FEQ.S, FLT.S, FLE.S
- [x] 浮点比较和符号注入

### 📋 剩余 Sprint (待开发)
- **Sprint 3**: 未开始
- **Sprint 7-14**: 完整 RVA23 支持 (压缩指令、向量扩展等)

详细计划见 [docs/sprint-plan.md](docs/sprint-plan.md)

## 测试覆盖

项目已实现 **200+ 单元测试**，覆盖：

- ✅ RV32I 基础指令 (全部 40+ 指令)
- ✅ CSR 读写和控制
- ✅ 乘除法运算 (MUL/DIV/REM 系列)
- ✅ 原子操作 (LR/SC/AMO 系列)
- ✅ 浮点运算 (RV64F 全部指令)
- ✅ 陷阱处理和异常

```bash
# 运行所有测试
cargo test --all-features

# 运行特定测试
cargo test test_rv64i_add

# 查看测试覆盖
cargo tarpaulin --out lcov
```

### FPU 浮点单元

Sprint 6 完成的浮点单元特性：

- **单精度浮点**: 符合 IEEE 754-2008 标准
- **运算指令**: ADD, SUB, MUL, DIV, SQRT, FMA
- **转换指令**: 整数 ↔ 浮点
- **移动指令**: 寄存器 ↔ 浮点寄存器
- **比较指令**: EQ, LT, LE, CLASSIFY
- **NaN boxing 支持**

### 陷阱处理

Sprint 5 实现的陷阱处理机制：

- **精确异常**: 支持所有 RISC-V 异常类型
- **异常原因**: mcause 寄存器记录
- **异常地址**: mtval 寄存器提供附加信息
- **PC 保存**: mepc 寄存器保存返回地址
- **中断支持**: 基础定时器/软件中断框架
- **特权模式**: Machine 模式支持

### 原子操作

Sprint 5 实现的 RV64A 原子指令：

- **Load-Reserved**: LR.W (原子读取)
- **Store-Conditional**: SC.W (条件存储，失败时恢复)
- **AMO (Atomic Memory Operations)**:
  - ADD, XOR, OR, AND
  - MIN, MAX, MINU, MAXU
- **释放一致性**: 支持 Acquire/Release 语义

## 依赖

### 构建依赖

- `anyhow` - 错误处理
- `thiserror` - 错误类型定义
- `tokio` - 异步支持
- `serde` - 序列化

### 开发依赖

- `proptest` - property-based testing
- `criterion` - 性能基准

完整依赖列表见 [docs/versions.md](docs/versions.md)

## 参考资料

- [RISC-V ISA 手册](https://github.com/riscv/riscv-isa-manual)
- [RVA23 Profile](https://github.com/riscv/riscv-profiles)
- [RISC-V Spike](https://github.com/riscv-software-src/riscv-isa-sim)

## 许可证

MIT License

---

最后更新: 2025-01-15
