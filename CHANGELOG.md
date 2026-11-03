# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **F/D U4: 调试器访问 FP 寄存器与 CSR（issue #148）**
  - FP profile 下 DM 抽象 Access Register 支持 `f0`–`f31`（regno `0x1020`–`0x103f`，`aarsize` 2 与 3）：32 位写入按 NaN-box 存储，32 位读取返回原始低 32 位（D5/D10）
  - `fflags`/`frm`/`fcsr`（CSR `0x001`–`0x003`）经抽象命令读写，不受 `mstatus.FS` 限制；按 pinned Spike，抽象访问本身不改变 FS（相对地，guest 或程序缓冲区里的 FP-CSR 指令仍按 D6 置 Dirty）
  - 程序缓冲区中的 FP 指令遵循常规 D6 FS 规则：FS On 正常退休，FS Off 以 `cmderr=3` 拒绝
  - DM `misa` 读取反映四个 profile；`rv64ima`/`rv64imac` 下 FPR 与 FP-CSR 抽象访问仍为 `cmderr=3`，既有 abstract、program-buffer 与 SBA 行为不变

- **F/D U3a: FP 译码与公共路径执行（issue #147）**
  - 32-bit 译码器新增 R4 opcode（MADD/MSUB/NMSUB/NMADD）与显式 `fmt`/`rm` 译码，覆盖全部 OP-FP、LOAD-FP、STORE-FP 形式；未分配或超出 profile 的 FP 编码按非法指令陷入，不再产生 `SimulatorFailure`
  - FP profile 下全部 F/D 指令经 `src/isa/fd` 在公共 Hart 路径上执行，算术、舍入、异常标志与 NaN 结果来自 U2 的 `src/fpu/arith`；静态与动态舍入均生效，保留的静态 `rm`（5/6）或 `frm` 为 5–7 的动态舍入按非法指令陷入
  - `mstatus.FS=Off` 时全部 F/D 指令（含 FLW/FLD/FSW/FSD、FMV.X.*、FEQ/FLT/FLE、FCLASS、FCVT 到整数）按非法指令陷入；FS-Dirty 遵循 D6：FPR 写入、FP-CSR 写入形式与非零标志累积置 Dirty，FSW/FSD 与不产生异常的 GPR-only FP 指令不改变 FS
  - FLW NaN-box；FSW 与 FMV.X.W 使用原始低 32 位；算术 NaN 结果为规范 NaN，FSGNJ*、FMV 与加载/存储保留 payload（D4/D5）
  - FLW/FLD/FSW/FSD 继续经既有 raw `DataRead`/`DataWrite` 物理路径，保留对齐、Sv39 与访问故障行为
  - 指令退休事实携带 FPR 与 `fcsr` 效果；`rv64ima`/`rv64imac` 的 commit log 文本不变
  - 默认 `rv64ima`、`rv64imac` 行为逐位不变，包括 `s4_fp_absent`

- **F/D U1: FP profile、FPR/`fcsr` 状态与 `mstatus.FS`（issue #145）**
  - 新增 `rv64imafd`（IALIGN=32、`misa` `0x8000000000141129`）与 `rv64imafdc`（IALIGN=16、`misa` `0x800000000014112D`）profile；`run --isa`、native（`load_and_run_with_isa`）和 flat（`RiscVSimulator::new_with_isa`）facade 接受四个 profile，能力检查显式化（`has_f`/`has_d`/`has_fp`）
  - FP profile 下 32 个 FPR 都是普通 64-bit 寄存器（`f0` 不再硬连线）；32-bit 写入 NaN-box；计算型单精度读取对未 box 的值返回规范 NaN `0x7fc00000`，FSW/FMV.X.W/32-bit 调试读取保留低 32 位（D5）
  - `fflags`/`frm`/`fcsr`（0x001–0x003）别名同一个 `fcsr`；保留位读 0、写入忽略；`mstatus.FS=Off` 时三者均按非法指令陷入
  - `mstatus.FS`（含 `sstatus` 视图）在 FP profile 下可写，`SD` 读为 `FS==Dirty`，复位为 Off；FP CSR 写入成功即置 FS=Dirty（CSRRW/CSRRWI 总是写，CSRRS/CSRRC 的 `rs1=x0` 与 CSRRSI/CSRRCI 的零立即数不写）
  - 复位恢复 FPR、`fcsr` 与 FS；`rv64ima`/`rv64imac` 的 FS/SD 仍硬连 0，0x001–0x003 仍非法
  - 本条 U1 只交付 profile 与 FP 状态；F/D/Zcd 指令执行、FP commit-log 格式与 DM 抽象 FPR/FP-CSR 访问由后续 U3a/U3b/U4 单元交付
  - 默认 `rv64ima` 行为逐位不变

- **Zca U3: Debug Mode、单步与程序缓冲区集成（issue #134）**
  - `dpc` 合法性按 profile 掩码：`rv64imac` 入口与写入仅清 bit 0，`rv64ima` 保持 bits 1:0 掩码（D8/D5）
  - C.EBREAK 按 EBREAK 处理：`ebreakm/s/u` 重定向进入 Debug Mode（`dcsr.cause=1`，`dpc` 为自身地址），否则产生 Breakpoint 异常
  - 单步跨过一条 2 或 4 字节指令，包括 `pc % 4 == 2`
  - `rv64imac` 下两个程序缓冲区字构成连续的 8 字节 parcel 流，随后是隐含 EBREAK：混合长度、C.EBREAK 正常终止；C.J/C.JR/C.JALR/C.BEQZ/C.BNEZ 与越过 `progbuf1` 的 32 位指令按 `cmderr=3` 拒绝
  - DM 抽象寄存器读取的 `misa` 反映所选 profile；`rv64ima` 下缓冲区中的压缩 parcel 仍按非法指令报 `cmderr=3`，`progbufsize`、`datacount`、`impebreak` 不变
  - 默认 `rv64ima` 的 abstract、program-buffer 与 SBA 行为逐位不变

- **Zca U2: Hart ISA profile 与 parcel 取指（issue #133）**
  - `run --isa <rv64ima|rv64imac>` 选择 Machine ISA profile；native（`load_and_run_with_isa`）与 flat（`RiscVSimulator::new_with_isa`）facade 支持同一选择，不从 ELF `e_flags` 推断
  - `rv64imac` profile：16-bit parcel 取指（第二 parcel 仅在首 parcel 低两位为 `0b11` 时取）、跨页 straddle 的逐 parcel 翻译与 D4 fault 优先级/tval 规则、IALIGN=16（链接/落点为 `pc + length`，`xepc`/`dpc` 写仅清 bit 0，MRET/SRET 返回 2 字节对齐目标）、只读 `misa` `0x8000000000141105`
  - commit log 对 16-bit 指令按 pinned Spike 格式输出 4 位十六进制
  - 默认 `rv64ima` 行为逐位不变

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

### Fixed

- Debug abstract-memory operations now participate in Machine admission. An unresolved physical completion blocks reset, resume, and further abstract commands while keeping DMI error inspection available.
- Hart 0 halt requests survive Hart reconstruction and selection changes. Explicit request clear and Debug Module reset cancel the persistent request.

### Removed

- **Stage 6 U6:** the unused GDB Remote Serial Protocol debugger under `src/debug/` and its crate re-exports (`DebugTarget`, `GdbServer`, breakpoint and watchpoint managers, and the debug CLI). OpenOCD `remote_bitbang` on `run --rbb-port` is unchanged.

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
