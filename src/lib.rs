//! RISC-V simulator main module
//!
//! This crate provides a RISC-V instruction set simulator with SystemC TLM2.0 interface support.

pub mod codegen;
pub mod core;
pub mod csr;
pub mod debug;
pub mod decode;
pub mod dispatch;
pub mod elf;
pub mod execute;
pub mod executor;
pub mod fpu;
pub mod isa;
pub mod memory;
pub mod mmu;
pub mod peripherals;
pub mod tlm;

pub use core::{CoreState, PrivilegeMode, RiscvCore};
pub use csr::{CsrError, CsrFile};
pub use debug::{
    Breakpoint, BreakpointManager, BreakpointType, DebugCli, DebugError, DebugTarget, GdbPacket,
    GdbServer, GdbServerConfig, GdbServerState, RspProtocol, StopReason, Watchpoint,
    WatchpointAccess, WatchpointManager, WatchpointType,
};
pub use decode::{DecodeError, DecodedInstruction, InstructionDecoder, InstructionFormat};
pub use execute::{ExecuteError, Executor};
pub use executor::{
    load_and_run, load_and_run_file, ExecutionResult, ExecutorError, RiscVSimulator,
};
pub use fpu::{Fcsr, Fpr, FpuRegisterFile};
pub use memory::{MemoryError, MemoryInterface, SimpleMemory};
pub use mmu::{
    AccessType, Mmu, MmuConfig, MmuError, Satp, Tlb, TlbEntry, TlbStats, TranslationMode,
};
pub use ruscv_macros::*;

// TLM2.0 导出
pub use tlm::{
    // 地址和 DMI
    AddressRange,
    ArbitrationPolicy,
    BusRoute,
    DataExtensionMode,
    DmiAccessRights,
    DmiData,
    // 时间管理
    ScTime,
    ScTimeUnit,
    // 总线和路由
    TlmBus,
    TlmBusBridge,
    // 基础类型
    TlmCommand,
    // 错误和同步
    TlmError,
    // 核心结构
    TlmGenericPayload,
    // 接口 trait
    TlmInitiator,
    TlmInterface,
    TlmPayloadBuilder,
    TlmPhase,
    TlmResponseStatus,
    // 简单内存
    TlmSimpleMemory,
    TlmSyncEnum,
    TlmTarget,
    TlmTime,
};

// 外设导出
pub use peripherals::{
    // CLINT
    Clint,
    // 错误类型
    PeripheralError,
    // 配置
    PlatformConfig,
    // PLIC
    Plic,
    // UART
    Uart16550,
    CLINT_SIZE,
    FIFO_DEPTH,
    MAX_INTERRUPT_SOURCES,
    MAX_PRIORITY,
    PLIC_SIZE,
    UART_SIZE,
};

// 子模块特定导出
/// CLINT 寄存器偏移
pub use peripherals::clint::reg_offset as clint_reg;
/// PLIC 寄存器偏移
pub use peripherals::plic::reg_offset as plic_reg;
/// UART 寄存器偏移和位定义
pub use peripherals::uart16550::{
    fcr_bits as uart_fcr, ier_bits as uart_ier, iir_bits as uart_iir, lcr_bits as uart_lcr,
    lsr_bits as uart_lsr, mcr_bits as uart_mcr, reg_offset as uart_reg,
};
