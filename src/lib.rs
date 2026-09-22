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
pub mod hart_amods;
pub mod isa;
pub mod memory;
pub mod mmu;
pub mod peripherals;
pub mod physical;
pub mod tlm;

pub use core::{
    CoreState, CoreStepOutcome, HartStepOutcome, InstructionRetired, LegacyTypedMemoryAdapter,
    MemoryAdapter, PhysicalMemoryAdapter, PrivilegeMode, RiscvCore, SharedPhysicalAccess,
    SimulatorFailure, SimulatorFailureKind, StepOutcome, TrapContinuationPolicy, TrapEntered,
    TrapEntryError,
};
pub use csr::{CsrAccess, CsrError, CsrFile};
pub use debug::{
    Breakpoint, BreakpointManager, BreakpointType, DebugCli, DebugError, DebugTarget, GdbPacket,
    GdbServer, GdbServerConfig, GdbServerState, RspProtocol, StopReason, Watchpoint,
    WatchpointAccess, WatchpointManager, WatchpointType,
};
pub use decode::{DecodeError, DecodedInstruction, InstructionDecoder, InstructionFormat};
pub use execute::{ExecuteError, Executor};
pub use executor::{
    load_and_run, load_and_run_file, ExecutionResult, ExecutorError, RiscVSimulator,
    SYSTEM_BUS_HTIF_BASE, SYSTEM_BUS_HTIF_SIZE, SYSTEM_BUS_UART_BASE, SYSTEM_BUS_UART_WINDOW,
};
pub use fpu::{Fcsr, Fpr, FpuRegisterFile};
pub use hart_amods::{AmoArithmeticError, AmoOperation};
pub use memory::{MemoryError, MemoryInterface, SimpleMemory};
pub use mmu::{
    AccessType, Mmu, MmuConfig, MmuError, Satp, Tlb, TlbEntry, TlbStats, TranslationMode,
};
pub use physical::{
    AccessCategory, AccessWidth, AmoTransform, AmoWidth, AtomicAccess, AtomicAccessError,
    AtomicAccessKind, AtomicAccessResult, AtomicBackend, AtomicBackendFailure, AtomicBackendResult,
    AtomicOrdering, AtomicProtocolError, AtomicRequest, AtomicRequestDescriptor,
    AtomicReservationContext, AtomicResponse, AtomicResponseCompletion, AtomicTargetRejection,
    AtomicUnknownCompletion, CommittedWriteSnapshot, ConditionalStatus, NativePhysicalTarget,
    NativeRamBackend, NativeSystemBusBackend, PhysicalAccess, PhysicalAccessError,
    PhysicalAccessKind, PhysicalAccessPort, PhysicalAccessResult, PhysicalBackend,
    PhysicalBackendError, PhysicalBackendFailure, PhysicalBackendResult, PhysicalCompletion,
    PhysicalCompletionKind, PhysicalProtocolError, PhysicalRequest, PhysicalRequestDescriptor,
    PhysicalRequestKind, PhysicalResponse, PhysicalResponseBytes, PhysicalResponseCompletion,
    PhysicalSpan, PhysicalTargetRejection, PhysicalTargetRejectionReason,
    PhysicalUnknownCompletion, PhysicalWidth, PureAmoTransform, RawPhysicalBytes,
    SharedNativeBackend, SystemBusPhysicalBackend, ValidatedAtomicAccess, ValidatedPhysicalAccess,
    MAX_COMMITTED_WRITE_SNAPSHOT_BYTES, MAX_PHYSICAL_ACCESS_BYTES,
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
