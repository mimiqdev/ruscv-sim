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
| **MMU/TLB** | ✅ **完成** | **Sprint 10 - Sv39 页表, 4-way LRU TLB** |
| **TLM2.0 + 外设** | ✅ **完成** | **Sprint 11 - CLINT, PLIC, UART 16550** |
| **测试覆盖** | ✅ **90+ tests** | Sprint 11 新增，全部通过 |

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
│   ├── mmu/                # MMU/TLB/Sv39 页表 (Sprint 10)
│   ├── tlm/                # TLM2.0 接口抽象 (Sprint 11)
│   │   ├── phase.rs        # 传输相位
│   │   ├── time.rs         # 时间管理
│   │   ├── payload.rs      # 事务载荷
│   │   ├── traits.rs       # Initiator/Target 接口
│   │   └── bus.rs          # 总线实现
│   ├── peripherals/        # 外设模型 (Sprint 11)
│   │   ├── clint.rs        # 本地中断控制器
│   │   ├── plic.rs         # 平台级中断控制器
│   │   └── uart16550.rs    # UART 串口
│   ├── fpu/                # 浮点运算单元 (RV64F)
│   ├── csr/                # CSR 寄存器框架
│   └── trap/               # 陷阱处理 (集成在 core/)
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

### ✅ Sprint 10: 内存子系统 (MMU/TLB)
- [x] MMU 架构实现 (mod.rs, physical.rs, pte.rs)
- [x] Sv39 页表遍历 (3级页表，4KB/2MB/1GB 页)
- [x] TLB 缓存 (64 entries, 4-way 组相联)
- [x] LRU 替换策略 + 老化算法
- [x] A/D 位处理 (Accessed/Dirty bit)
- [x] SATP 模式 (Bare/Sv39)
- [x] 约 360 个 MMU 测试
- [x] 558 个测试全部通过

### ✅ Sprint 11: TLM2.0 + 外设
- [x] TLM2.0 基础类型: TlmPhase, TlmResponseStatus, TlmCommand
- [x] TLM2.0 时间管理: ScTime (SystemC 风格，皮秒精度)
- [x] TLM2.0 核心结构: TlmGenericPayload, TlmPayloadBuilder
- [x] TLM2.0 接口: TlmInitiator, TlmTarget traits
- [x] TLM2.0 总线: TlmBus, TlmBusBridge, ArbitrationPolicy
- [x] TLM2.0 内存: TlmSimpleMemory, DmiData 直接内存接口
- [x] CLINT 外设: mtime/mtimecmp 定时器, MSIP 软件中断
- [x] PLIC 外设: 1024 中断源, 优先级仲裁, Claim/Complete
- [x] UART 16550: FIFO, 中断, 波特率, 流控
- [x] 平台配置: PlatformConfig (HiFive1, QEMU Virt)
- [x] 90+ 测试 (TLM 30 个 + 外设 20+ 个)

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

项目已实现 **558 个测试**，全部通过，覆盖：

- ✅ RV32I 基础指令 (全部 40+ 指令)
- ✅ CSR 读写和控制
- ✅ 乘除法运算 (MUL/DIV/REM 系列)
- ✅ 原子操作 (LR/SC/AMO 系列)
- ✅ 浮点运算 (RV64F 全部指令)
- ✅ 陷阱处理和异常
- ✅ **MMU/TLB/Sv39** (Sprint 10 新增 ~360 测试)
  - Sv39 页表遍历 (4KB/2MB/1GB 页)
  - TLB 命中/未命中/刷新
  - A/D 位处理
  - LRU 老化算法

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

### MMU/TLB 内存管理

Sprint 10 实现的内存管理单元：

- **Sv39 页表**: 3级页表遍历，支持 4KB/2MB/1GB 页
- **TLB 缓存**: 64 entries, 4-way 组相联缓存
- **LRU 算法**: 带老化的 LRU 替换策略，防止计数器溢出
- **A/D 位**: Accessed/Dirty bit 自动处理和回写
- **SATP 模式**: 支持 Bare 和 Sv39 模式
- **物理内存抽象**: PhysicalMemoryInterface trait

### TLM2.0 事务级建模

Sprint 11 实现的 SystemC TLM2.0 风格接口：

- **传输相位**: BEGIN_REQ, END_REQ, BEGIN_RESP, END_RESP 四阶段协议
- **时间管理**: ScTime 皮秒精度，支持加减运算
- **事务载荷**: TlmGenericPayload 支持字节使能、流式传输、DMI
- **接口定义**: TlmInitiator/Target traits 支持阻塞/非阻塞传输
- **总线实现**: TlmBus 多设备互联，支持固定优先级/轮询/LRU 仲裁
- **桥接器**: TlmBusBridge 支持地址转换和延迟调整

### RISC-V 平台外设

Sprint 11 实现的 RISC-V 标准外设：

- **CLINT**: Core Local Interruptor
  - mtime: 64位全局定时器
  - mtimecmp: 定时器比较器（每个 Hart）
  - MSIP: 软件中断挂起（每个 Hart）
- **PLIC**: Platform-Level Interrupt Controller
  - 最多 1024 个外部中断源
  - 7 级可编程优先级
  - 阈值屏蔽和中断仲裁
  - 支持 M-mode 和 S-mode
- **UART 16550**: 标准串口控制器
  - 16 字节收发 FIFO
  - 可编程波特率
  - 中断驱动收发
  - 流控支持 (RTS/CTS)

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

最后更新: 2026-02-02 (Sprint 11 完成 - TLM2.0 + 外设)
