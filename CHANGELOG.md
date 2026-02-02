# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **M4: 调试支持**
  - GDB RSP 服务器实现
    - 完整的 Remote Serial Protocol 支持
    - 断点/观察点/寄存器/内存访问
  - CLI 调试界面
    - 交互式命令行调试工具
    - 支持源码级调试
  - 断点管理器
    - 软件断点 (breakpoint)
    - 硬件断点 (hardware breakpoint)
  - 观察点管理器
    - 读观察点 (read watchpoint)
    - 写观察点 (write watchpoint)
    - 访问观察点 (access watchpoint)
  - 版本控制 git hooks
    - commit 前自动 fmt + check
    - push 前自动 clippy

### Changed

- **Sprint 11: TLM2.0 + 外设**
  - TLM2.0 抽象层完整实现
    - TlmPhase: BEGIN_REQ, END_REQ, BEGIN_RESP, END_RESP 四阶段协议
    - TlmResponseStatus: 完整的响应状态枚举和错误分类
    - TlmCommand: Read/Write 命令类型
    - ScTime: SystemC 风格的时间管理（皮秒精度）
    - TlmGenericPayload: 通用事务载荷，支持字节使能、流式传输、DMI
    - TlmPayloadBuilder: Builder 模式构造载荷
    - TlmInitiator/TlmTarget: 发起者和目标接口 trait
    - TlmBus: 多设备互联总线，支持固定优先级/轮询/LRU 仲裁
    - TlmBusBridge: 总线桥接器，支持地址转换
    - TlmSimpleMemory: 简单内存实现
    - DmiData: 直接内存接口支持
  - 外设模型实现
    - CLINT (Core Local Interruptor): mtime/mtimecmp 定时器，MSIP 软件中断
    - PLIC (Platform-Level Interrupt Controller): 最多 1024 个中断源，优先级仲裁
    - UART 16550: 标准串口控制器，支持 FIFO、中断、波特率配置
  - 平台配置
    - PlatformConfig: 预定义 SiFive HiFive1 和 QEMU Virt 平台配置
  - 测试套件
    - TLM 基础类型测试: 10 个
    - TLM Payload 测试: 10 个
    - TLM 总线测试: 10 个
    - 外设集成测试: 20+ 个
    - 总计 90+ 个测试

### Planned

- **Sprint 8.5: 推广 C 指令模块化模式到所有指令集**
  - 分析当前 C 指令的组织结构（src/isa/rv64c/）
  - 设计重构方案：将 RV64I/RV64M/RV64A/RV64F/RV64D 按功能拆分成多个小文件
  - 参考 C 指令的组织模式，保持 API 兼容性
  - 规划文档: `docs/sprint-8.5-plan.md`
  - 预计改动：新增 23 个文件，修改 3 个文件，删除 24 个文件
  - 时间估算：7-11 小时

### Added

- RV64I base instruction set support (64-bit integer operations)
  - 64-bit register file (x0-x31 as 64-bit registers)
  - 64-bit arithmetic instructions: ADDW, SUBW, SLLW, SRLW, SRAW
  - 64-bit immediate instructions: ADDIW, SLLIW, SRLIW, SRAIW
  - 64-bit load/store: LD, SD, LWU (zero-extending word load)
- RV64M multiplication and division extension
  - 64-bit multiply: MUL, MULH, MULHU, MULHSU
  - 64-bit divide: DIV, DIVU, REM, REMU
  - Proper overflow handling per RISC-V spec (i64::MIN / -1 returns i64::MIN)
- RV64A atomic operation extension
  - Load-reserved/Store-conditional: LR.D, SC.D
  - Atomic memory operations: AMOADD.D, AMOSWAP.D, AMOAND.D, AMOOR.D, AMOXOR.D,
    AMOMAX.D, AMOMIN.D, AMOMAXU.D, AMOMINU.D
- RV64F single-precision floating-point extension
  - 32-bit floating-point register operations
  - IEEE 754-2008 compliant arithmetic
  - NaN boxing/unboxing for upper 32 bits of 64-bit registers
- RV64D double-precision floating-point extension
  - 64-bit floating-point register operations
  - Full IEEE 754-2008 compliant arithmetic
- CSR (Control and Status Register) framework
  - Machine mode CSRs: mstatus, misa, medeleg, mideleg, mie, mtvec, mcounteren,
    mscratch, mepc, mcause, mtval, mip, mhartid
  - Supervisor mode CSRs: sstatus, sie, stvec, scounteren, sscratch, sepc,
    scause, stval, sip, satp
  - Virtualization mode CSRs: vsstatus, vsie, vstvec, vsscratch, vsepc,
    vscause, vstval, vsip, vsatp
  - CSR instruction support: CSRRW, CSRRS, CSRRC, CSRRWI, CSRRSI, CSRRCI
- Privilege mode support
  - User (U), Supervisor (S), and Machine (M) modes
  - Privilege mode transitions and protection
  - Trap handling framework with MRET/SRET instructions

### Changed

- Migrated from RV32I to RV64I as the base architecture
- All integer registers upgraded from 32-bit to 64-bit
- Memory addressing upgraded to support 64-bit virtual address space
- MSTATUS register mask updated for RV64 (0x8000_0003_000D_FFEA)

### Technical Details

#### Division Overflow Handling
Per RISC-V Spec Volume I, Section 2.4:
- Division by zero returns all ones (-1)
- Overflow case (signed MIN / -1) returns MIN (not undefined behavior)

#### MSTATUS Register Layout (RV64)
Per RISC-V Privileged Spec, Section 3.1.6:
- Bit 63 (SD): State Dirty (read-only)
- Bits 35:34 (SXL): Supervisor XLEN
- Bits 33:32 (UXL): User XLEN
- Bit 22 (TSR): Trap SRET
- Bit 21 (TW): Timeout Wait
- Bit 20 (TVM): Trap Virtual Memory
- Bit 18 (MXR): Make Executable Readable
- Bit 17 (SUM): Supervisor User Memory Access
- Bit 13 (FS): Floating-point State
- Bits 12:11 (MPP): Machine Previous Privilege
- Bit 8 (SPP): Supervisor Previous Privilege
- Bit 7 (MPIE): Machine Previous Interrupt Enable
- Bit 5 (SPIE): Supervisor Previous Interrupt Enable
- Bit 3 (MIE): Machine Interrupt Enable
- Bit 1 (SIE): Supervisor Interrupt Enable

## [0.1.0] - 2025-01-XX

### Added

- Initial project setup with Rust 2024 Edition
- CI/CD pipeline with GitHub Actions
- Pre-commit and pre-push git hooks
- Basic project structure and documentation
- RV32I base instruction set foundation
