 > **注意**：本文档中的代码示例仅用于说明设计概念和 API 用途，是伪代码性质的示例，不保证可编译通过。示例代码可能包含语法糖简写、类型占位符等，仅供理解之用。 
# 已归档的异构计算仿真研究报告

> **状态：** 历史研究。保留 DMA、共享内存、同步和加速器建模分析；推荐路线与工期已失效。

> **文档版本**: v1.0  
> **创建日期**: 2026-02-05  
> **作者**: Claude Code (硬件仿真工程师)

**相关文档：**
- [架构总结](heterogeneous-architecture-summary.md) - 历史决策概览
- [已归档的 SystemBus 扩展计划](../plans/systembus-heterogeneous-extension.md) - 历史设备 trait 设计
- [NPU 集成方案](npu-integration-study.md) - 历史设备设计

---

## 目录

1. [异构计算仿真概述](#1-异构计算仿真概述)
2. [ruscv-sim 现有架构分析](#2-ruscv-sim-现有架构分析)
3. [异构仿真方案设计](#3-异构仿真方案设计)
4. [关键技术挑战与解决方案](#4-关键技术挑战与解决方案)
5. [推荐实现路线](#5-推荐实现路线)
6. [参考案例分析](#6-参考案例分析)

---

## 1. 异构计算仿真概述

### 1.1 什么是异构计算

异构计算（Heterogeneous Computing）是指在同一个计算系统中集成多种不同类型的处理器或计算单元，协同完成计算任务。常见的异构组合包括：

| 组合类型 | 典型应用场景 | 代表产品 |
|---------|-------------|---------|
| CPU + GPU | 图形渲染、深度学习训练 | NVIDIA CUDA、AMD ROCm |
| CPU + NPU | 边缘 AI 推理、图像处理 | 苹果 Neural Engine、华为达芬奇架构 |
| CPU + FPGA | 硬件加速、网络处理 | Intel HLS、Xilinx Vitis |
| CPU + DSP | 信号处理、多媒体编解码 | Qualcomm Hexagon |

### 1.2 异构计算仿真需求

在硬件系统开发过程中，异构仿真面临以下核心需求：

1. **协同仿真**：CPU 与协处理器需要在同一时间域内同步执行
2. **共享内存一致性**：多个处理器核心访问同一内存空间时需要保证一致性
3. **任务调度与同步**：异构任务需要在运行时正确调度
4. **性能建模**：需要评估异构系统的整体性能瓶颈
5. **软件栈验证**：验证异构系统的固件、驱动和上层软件

### 1.3 仿真方法论

异构系统仿真通常采用以下几种方法：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        异构计算仿真方法对比                                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐        │
│  │   顺序仿真       │    │   交叉仿真       │    │   协同仿真       │        │
│  │                 │    │                 │    │                 │        │
│  │  CPU 执行 ->    │    │  CPU 在 Host    │    │  CPU 和协处理    │        │
│  │  协处理器模拟    │    │  协处理器在 FPGA │    │  器并行仿真      │        │
│  │                 │    │                 │    │                 │        │
│  │  + 简单实现      │    │ + 硬件在环      │    │ + 精确时序      │        │
│  │  - 速度慢       │    │ - 需要硬件      │    │ - 复杂同步      │        │
│  └─────────────────┘    └─────────────────┘    └─────────────────┘        │
│                                                                              │
│  ┌─────────────────┐    ┌─────────────────┐                                │
│  │   TLM 事务级     │    │   周期级精确     │                                │
│  │   仿真           │    │   仿真           │                                │
│  │  抽象事务建模    │    │  每个周期精确    │                                │
│  │                 │    │  模拟            │                                │
│  │  + 速度与精度    │    │                 │                                │
│  │    平衡          │    │  + 最高精度     │                                │
│  │                 │    │  - 速度最慢     │                                │
│  └─────────────────┘    └─────────────────┘                                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. ruscv-sim 现有架构分析

### 2.1 系统架构总览

ruscv-sim 是一个纯 Rust 实现的 RISC-V 指令集模拟器（ISS），采用现代化的分层架构设计：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           ruscv-sim 系统架构                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                        应用层 (Application Layer)                    │    │
│  │   GDB 调试器 │ CLI 界面 │ Python API │ 性能分析器 │ 测试框架          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                    │                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                      SDK / 编程接口层 (SDK Layer)                    │    │
│  │   ┌──────────────┐  ┌──────────────┐  ┌──────────────┐              │    │
│  │   │ C++ Core API │  │ Python Bindings │ │ 配置文件 API │              │    │
│  │   └──────────────┘  └──────────────┘  └──────────────┘              │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                    │                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                      模拟器核心层 (Core Simulator)                   │    │
│  │                                                                      │    │
│  │  ┌───────────────────────────────────────────────────────────────┐  │    │
│  │  │                    Instruction Set Simulator                  │  │    │
│  │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐      │  │    │
│  │  │  │  Fetch   │→│  Decode  │→│  Execute  │→│  Commit  │      │  │    │
│  │  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘      │  │    │
│  │  └───────────────────────────────────────────────────────────────┘  │    │
│  │                                                                      │    │
│  │  ┌───────────────────────────────────────────────────────────────┐  │    │
│  │  │                    Timing Model (可插拔)                      │  │    │
│  │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐   │  │    │
│  │  │  │Zero Timing  │  │Instr Timing │  │ Micro-architecture │   │  │    │
│  │  │  │(功能验证)    │  │(性能分析)   │  │(精确时序)           │   │  │    │
│  │  │  └─────────────┘  └─────────────┘  └─────────────────────┘   │  │    │
│  │  └───────────────────────────────────────────────────────────────┘  │    │
│  │                                                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                    │                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    硬件抽象层 (Hardware Abstraction)                 │    │
│  │                                                                      │    │
│  │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐   │    │
│  │  │   Register File  │  │      CSR Bank    │  │    MMU/TLB       │   │    │
│  │  └──────────────────┘  └──────────────────┘  └──────────────────┘   │    │
│  │                                                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                    │                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    外设模型层 (Peripheral Models)                   │    │
│  │   基于 TLM 2.0 标准                                                   │    │
│  │                                                                      │    │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐    │    │
│  │  │   Bus/AXI  │  │   CLINT    │  │    PLIC    │  │    UART    │    │    │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘    │    │
│  │                                                                      │    │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌────────────┐    │    │
│  │  │  Timer     │  │  GPIO      │  │  Memory    │  │ Custom IF  │    │    │
│  │  └────────────┘  └────────────┘  └────────────┘  └────────────┘    │    │
│  │                                                                      │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 现有 TLM2.0 外设框架分析

ruscv-sim 已实现完整的 TLM2.0 外设抽象层，这为异构计算仿真提供了坚实基础：

```rust
// 核心 TLM 接口定义

/// 外设设备 trait（遵循 TLM2.0 模式）
pub trait PeripheralDevice {
    /// 处理 MMIO 读操作
    fn read(&self, offset: u64, size: u8) -> Result<u32, BusError>;
    
    /// 处理 MMIO 写操作
    fn write(&self, offset: u64, data: u32, size: u8) -> Result<(), BusError>;
    
    /// 获取设备名称
    fn name(&self) -> &'static str;
    
    /// 获取基地址
    fn base_address(&self) -> u64;
    
    /// 获取设备大小
    fn size(&self) -> u64;
}

/// 系统总线 trait
pub trait SystemBus: Send + Sync {
    /// 读取内存
    fn read(&self, addr: u64, data: &mut [u8]) -> Result<(), BusError>;
    
    /// 写入内存
    fn write(&self, addr: u64, data: &[u8]) -> Result<(), BusError>;
    
    /// 注册内存区域
    fn register_memory(
        &mut self,
        name: &str,
        base: u64,
        size: u64,
        memory: Arc<dyn MemoryRegion>,
    ) -> Result<(), BusError>;
    
    /// 注册 MMIO 设备
    fn register_mmio(
        &mut self,
        name: &str,
        base: u64,
        size: u64,
        device: Arc<dyn MmioDevice>,
    ) -> Result<(), BusError>;
}
```

**现有外设组件**：

| 外设 | 状态 | 功能 |
|-----|------|------|
| CLINT | ✅ 完成 | RISC-V 核心本地中断器 |
| PLIC | ✅ 完成 | Platform Level Interrupt Controller |
| UART 16550 | ✅ 完成 | 串口通信 |
| GPIO | ✅ 完成 | 通用输入输出 |
| SystemBus | ✅ 完成 | 内存映射路由 |

### 2.3 SystemBus 扩展能力

SystemBus 是 ruscv-sim 的核心路由组件，已支持动态内存区域注册：

```rust
// SystemBus 实现

pub struct SystemBus {
    /// 内存区域列表
    memory_regions: Vec<MemoryRegion>,
    /// MMIO 设备列表
    mmio_devices: Vec<MmioDevice>,
    /// 物理内存实现
    physical_memory: Arc<dyn PhysicalMemory>,
}

impl SystemBus {
    /// 添加 NPU 设备
    pub fn register_npu(
        &mut self,
        name: &str,
        base: u64,
        size: u64,
        npu: Arc<dyn NpuDevice>,
    ) -> Result<(), BusError> {
        // 检查地址冲突
        self.check_address_conflict(base, size)?;
        
        // 注册 MMIO 设备
        self.mmio_devices.push(MmioDevice {
            name: name.to_string(),
            base,
            size,
            device: npu,
        });
        
        info!("Registered NPU at 0x{:016x} - 0x{:016x}", base, base + size);
        Ok(())
    }
    
    /// 检查地址冲突
    fn check_address_conflict(&self, base: u64, size: u64) -> Result<(), BusError> {
        let end = base + size;
        
        for region in &self.memory_regions {
            if base < region.base + region.size && end > region.base {
                return Err(BusError::AddressConflict);
            }
        }
        
        for device in &self.mmio_devices {
            if base < device.base + device.size && end > device.base {
                return Err(BusError::AddressConflict);
            }
        }
        
        Ok(())
    }
}
```

### 2.4 指令集扩展可能性

ruscv-sim 采用单一统一分发表 + LRU 缓存设计，扩展自定义指令非常方便：

```rust
// 指令分发架构

/// 指令键（opcode + funct3 + funct7 完整匹配）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct InstructionKey {
    pub opcode: u8,
    pub funct3: u8,
    pub funct7: u8,
}

/// 指令分发器
pub struct Dispatcher {
    /// 主分发表
    dispatch_table: HashMap<InstructionKey, ExecutorFn>,
    /// LRU 缓存
    cache: LruCache,
}

/// 添加自定义指令的示例
impl Dispatcher {
    /// 注册 NPU 相关指令
    pub fn register_npu_instructions(&mut self) {
        // NPU 卷积指令 (CUSTOM0)
        self.register(
            0x0B,  // CUSTOM0 opcode
            0b000, // funct3 = Conv2D
            0x00,  // funct7
            Executor::exec_npu_conv2d,
        );
        
        // NPU 矩阵乘法指令
        self.register(
            0x0B,
            0b001, // funct3 = Gemm
            0x00,
            Executor::exec_npu_gemm,
        );
        
        // NPU 池化指令
        self.register(
            0x0B,
            0b010, // funct3 = Pool
            0x00,
            Executor::exec_npu_pool,
        );
    }
}
```

### 2.5 内存架构支持

ruscv-sim 已实现完整的 Sv39 MMU，支持虚拟内存管理：

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Sv39 地址空间布局                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  0x0000_0000_0000_0000 ─┐                                                   │
│                         │  User Memory (256GB)                              │
│  0x0000_003F_FFFF_FFFF ─┘                                                   │
│                                                                              │
│  0x0000_0040_0000_0000 ─┐                                                   │
│                         │  Unmapped (Hole)                                  │
│  0x003F_FFBF_FFFF_FFFF ─┘                                                   │
│                                                                              │
│  0x003F_FFC0_0000_0000 ─┐                                                   │
│                         │  Supervisor Memory (256GB)                        │
│  0x003F_FFFF_FFFF_FFFF ─┘                                                   │
│                                                                              │
│  0x0040_0000_0000_0000 ─┐                                                   │
│                         │  Unmapped (Hole)                                  │
│  0xFFFF_FBFF_FFFF_FFFF ─┘                                                   │
│                                                                              │
│  0xFFFF_FC00_0000_0000 ─┐                                                   │
│                         │  Machine Memory (256GB)                           │
│  0xFFFF_FFFF_FFFF_FFFF ─┘                                                   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**关键特性**：
- 4-way LRU TLB（64 entries）
- 页表遍历器（PTW）
- PMP（物理内存保护）
- MMIO 支持

### 2.6 NPU 集成现有工作

ruscv-sim 项目已存在一份详细的 NPU 集成方案文档（`docs/npu-integration.md`），为异构计算仿真提供了蓝图。

---

## 3. 异构仿真方案设计

### 3.1 CPU + NPU 协同仿真方案

#### 3.1.1 系统架构设计

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       CPU + NPU 协同仿真架构                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                         RISC-V Core                                 │   │
│   │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │   │
│   │  │  Fetch   │→│  Decode  │→│  Execute │→│  Commit  │           │   │
│   │  └──────────┘  └──────────┘  └──────────┘  └──────────┘           │   │
│   │                                                                      │   │
│   │  ┌─────────────────────────────────────────────────────────────┐   │   │
│   │  │  指令扩展: CUSTOM0 (NPU 命令)                                 │   │   │
│   │  │  - npu_conv2d: 卷积运算                                       │   │   │
│   │  │  - npu_gemm: 矩阵乘法                                         │   │   │
│   │  │  - npu_pool: 池化操作                                         │   │   │
│   │  └─────────────────────────────────────────────────────────────┘   │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                      SystemBus                                      │   │
│   │                                                                      │   │
│   │   0x0000_0000 - 0x0FFF_FFFF  │  CLINT       @ 0x0200_0000          │   │
│   │   0x1000_0000 - 0x1000_FFFF  │  UART        @ 0x1001_3000          │   │
│   │   0x1001_0000 - 0x1001_FFFF  │  NPU Control @ 0x1001_0000          │   │
│   │   0x1002_0000 - 0x1002_FFFF  │  NPU Data    @ 0x1002_0000          │   │
│   │   0x8000_0000 -              │  DRAM        @ 0x8000_0000          │   │
│   │                                                                      │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                    ┌───────────────┼───────────────┐                       │
│                    │               │               │                       │
│                    ▼               ▼               ▼                       │
│            ┌─────────────┐ ┌─────────────┐ ┌─────────────┐                │
│            │  NPU Core   │ │  CLINT      │ │    DRAM     │                │
│            │             │ │             │ │             │                │
│            │ - PE Array  │ │ - Timer     │ │ - 主内存    │                │
│            │ - Scheduler │ │ - SWI       │ │ - 共享数据  │                │
│            │ - Local Mem │ │ - MTIP      │ │             │                │
│            └─────────────┘ └─────────────┘ └─────────────┘                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.1.2 NPU 设备实现

```rust
// src/npu/mod.rs

use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use crate::tlm::{SystemBus, BusError, MmioDevice};

/// NPU 设备
pub struct NpuDevice {
    /// 基地址
    base_addr: u64,
    /// 控制寄存器
    regs: NpuRegisters,
    /// PE 阵列
    pe_array: PeArray,
    /// 本地 SRAM
    local_sram: Vec<u8>,
    /// 任务调度器
    scheduler: NpuScheduler,
    /// 状态标志
    running: AtomicBool,
    /// 中断挂起
    interrupt_pending: AtomicBool,
    /// 系统总线引用
    bus: RwLock<Option<Arc<SystemBus>>>,
}

/// NPU 寄存器定义
#[derive(Debug, Clone, Copy)]
struct NpuRegisters {
    /// 命令寄存器 (0x00)
    cmd: u32,
    /// 状态寄存器 (0x04)
    status: u32,
    /// 配置寄存器 (0x08)
    config: u32,
    /// 中断使能 (0x0C)
    irq_enable: u32,
    /// 输入地址低 32 位 (0x10)
    input_addr_lo: u32,
    /// 输入地址高 32 位 (0x14)
    input_addr_hi: u32,
    /// 权重地址低 32 位 (0x18)
    weight_addr_lo: u32,
    /// 权重地址高 32 位 (0x1C)
    weight_addr_hi: u32,
    /// 输出地址低 32 位 (0x20)
    output_addr_lo: u32,
    /// 输出地址高 32 位 (0x24)
    output_addr_hi: u32,
    /// 维度参数 (0x28)
    dims: u32,
}

impl NpuRegisters {
    // 状态位定义
    const STATUS_BUSY: u32 = 1 << 0;
    const STATUS_DONE: u32 = 1 << 1;
    const STATUS_ERROR: u32 = 1 << 2;
    
    // 命令位定义
    const CMD_START: u32 = 1 << 0;
    const CMD_TYPE_MASK: u32 = 0xF << 4;
    const CMD_CONV2D: u32 = 0x0 << 4;
    const CMD_GEMM: u32 = 0x1 << 4;
    const CMD_POOL: u32 = 0x2 << 4;
}

impl NpuDevice {
    /// 创建 NPU 设备
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            regs: NpuRegisters::default(),
            pe_array: PeArray::new(8, 8),  // 8x8 PE 阵列
            local_sram: vec![0u8; 64 * 1024],  // 64KB SRAM
            scheduler: NpuScheduler::new(),
            running: AtomicBool::new(false),
            interrupt_pending: AtomicBool::new(false),
            bus: RwLock::new(None),
        }
    }
    
    /// 连接到系统总线
    pub fn connect_bus(&self, bus: &Arc<SystemBus>) {
        *self.bus.write().unwrap() = Some(Arc::clone(bus));
    }
}

impl MmioDevice for NpuDevice {
    fn name(&self) -> &'static str {
        "NPU"
    }
    
    fn base_address(&self) -> u64 {
        self.base_addr
    }
    
    fn size(&self) -> u64 {
        0x10000  // 64KB MMIO 空间
    }
    
    fn read(&self, offset: u64, size: u8) -> Result<u32, BusError> {
        match offset {
            0x00 => Ok(self.regs.cmd),
            0x04 => Ok(self.regs.status),
            0x08 => Ok(self.regs.config),
            0x0C => Ok(self.regs.irq_enable),
            0x10 => Ok(self.regs.input_addr_lo),
            0x14 => Ok(self.regs.input_addr_hi),
            0x18 => Ok(self.regs.weight_addr_lo),
            0x1C => Ok(self.regs.weight_addr_hi),
            0x20 => Ok(self.regs.output_addr_lo),
            0x24 => Ok(self.regs.output_addr_hi),
            0x28 => Ok(self.regs.dims),
            _ => Err(BusError::LoadAccessFault(self.base_addr + offset)),
        }
    }
    
    fn write(&self, offset: u64, value: u32, size: u8) -> Result<(), BusError> {
        match offset {
            0x00 => {
                // 启动命令
                if value & NpuRegisters::CMD_START != 0 {
                    self.start_operation()?;
                }
                Ok(())
            }
            0x04 => {
                // 状态寄存器（只读，忽略写入）
                Ok(())
            }
            0x08 => {
                self.regs.config = value;
                Ok(())
            }
            0x0C => {
                self.regs.irq_enable = value;
                Ok(())
            }
            0x10 => { self.regs.input_addr_lo = value; Ok(()) }
            0x14 => { self.regs.input_addr_hi = value; Ok(()) }
            0x18 => { self.regs.weight_addr_lo = value; Ok(()) }
            0x1C => { self.regs.weight_addr_hi = value; Ok(()) }
            0x20 => { self.regs.output_addr_lo = value; Ok(()) }
            0x24 => { self.regs.output_addr_hi = value; Ok(()) }
            0x28 => { self.regs.dims = value; Ok(()) }
            _ => Err(BusError::StoreAccessFault(self.base_addr + offset)),
        }
    }
}

impl NpuDevice {
    /// 启动 NPU 操作
    fn start_operation(&self) -> Result<(), BusError> {
        if self.running.load(Ordering::SeqCst) {
            return Err(BusError::DeviceBusy);
        }
        
        self.running.store(true, Ordering::SeqCst);
        self.regs.status |= NpuRegisters::STATUS_BUSY;
        
        // 在后台线程执行运算
        let bus = self.bus.read().unwrap();
        let bus_ref = bus.as_ref().cloned();
        
        std::thread::spawn(move || {
            if let Some(bus) = bus_ref {
                Self::execute_operation(&bus);
            }
        });
        
        Ok(())
    }
    
    /// 执行 NPU 运算
    fn execute_operation(bus: &SystemBus) {
        // 简化的执行逻辑
        // 实际实现需要从内存读取数据，执行运算，写回结果
        // 设置完成状态
    }
    
    /// 检查是否完成
    pub fn is_done(&self) -> bool {
        !self.running.load(Ordering::SeqCst) && 
        (self.regs.status & NpuRegisters::STATUS_DONE) != 0
    }
}
```

#### 3.1.3 PE 阵列仿真

```rust
// src/npu/pe_array.rs

/// 处理元素（Processing Element）
pub struct PeUnit {
    /// 累加器值
    accumulator: i32,
    /// 乘法器输入缓存
    input_cache: i8,
    /// 权重缓存
    weight_cache: i8,
    /// PE 是否繁忙
    busy: bool,
}

impl PeUnit {
    /// 执行 MAC 操作
    pub fn mac(&mut self) {
        self.accumulator += (self.input_cache as i32) * (self.weight_cache as i32);
    }
    
    /// 加载输入数据
    pub fn load_input(&mut self, value: i8) {
        self.input_cache = value;
    }
    
    /// 加载权重
    pub fn load_weight(&mut self, value: i8) {
        self.weight_cache = value;
    }
    
    /// 读取累加器
    pub fn read_accumulator(&self) -> i32 {
        self.accumulator
    }
    
    /// 复位
    pub fn reset(&mut self) {
        self.accumulator = 0;
        self.input_cache = 0;
        self.weight_cache = 0;
        self.busy = false;
    }
}

/// PE 阵列（多个 PE 组成）
pub struct PeArray {
    /// PE 阵列（行 × 列）
    pes: Vec<PeUnit>,
    /// 阵列行数
    rows: usize,
    /// 阵列列数
    cols: usize,
    /// 局部累加器（输出行累加）
    row_accumulators: Vec<i32>,
}

impl PeArray {
    /// 创建新的 PE 阵列
    pub fn new(rows: usize, cols: usize) -> Self {
        let pes = vec![PeUnit::default(); rows * cols];
        let row_accumulators = vec![0; rows];
        
        Self {
            pes,
            rows,
            cols,
            row_accumulators,
        }
    }
    
    /// 执行卷积运算（简化版）
    pub fn conv2d(
        &mut self,
        input: &[i8],
        weights: &[i8],
        output: &mut [i32],
        in_channels: usize,
        out_channels: usize,
        height: usize,
        width: usize,
        kernel: usize,
    ) {
        // 简化的卷积实现
        for oc in 0..out_channels {
            for h in 0..(height - kernel + 1) {
                for w in 0..(width - kernel + 1) {
                    let mut sum = 0;
                    
                    for ic in 0..in_channels {
                        for kh in 0..kernel {
                            for kw in 0..kernel {
                                let in_idx = (ic * height + h + kh) * width + w + kw;
                                let w_idx = (oc * in_channels + ic) * kernel * kernel + kh * kernel + kw;
                                
                                sum += (input[in_idx] as i32) * (weights[w_idx] as i32);
                            }
                        }
                    }
                    
                    output[(oc * (height - kernel + 1) + h) * (width - kernel + 1) + w] = sum;
                }
            }
        }
    }
    
    /// 重置阵列
    pub fn reset(&mut self) {
        for pe in &mut self.pes {
            pe.reset();
        }
        for acc in &mut self.row_accumulators {
            *acc = 0;
        }
    }
}
```

### 3.2 CPU + GPU 协同仿真方案

#### 3.2.1 GPU 仿真架构设计

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       CPU + GPU 协同仿真架构                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                         RISC-V Core                                 │   │
│   │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │   │
│   │  │  Fetch   │→│  Decode  │→│  Execute │→│  Commit  │           │   │
│   │  └──────────┘  └──────────┘  └──────────┘  └──────────┘           │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                                    ▼                                        │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                      SystemBus                                      │   │
│   │                                                                      │   │
│   │   0x1003_0000 - 0x1003_FFFF  │  GPU Control @ 0x1003_0000          │   │
│   │   0x1004_0000 - 0x1007_FFFF  │  GPU Channel @ 0x1004_0000          │   │
│   │   0xC000_0000 - 0xCFFF_FFFF  │  GPU VRAM    @ 0xC000_0000          │   │
│   │                                                                      │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│                    ┌───────────────┼───────────────┐                       │
│                    │               │               │                       │
│                    ▼               ▼               ▼                       │
│            ┌─────────────┐ ┌─────────────┐ ┌─────────────┐                │
│            │  GPU Core   │ │  GPU        │ │   VRAM      │                │
│            │             │ │  Command    │ │   (显存)    │                │
│            │ - Compute   │ │  Processor  │ │             │                │
│            │   Units     │ │  - DMA      │ │ - Framebuf │                │
│            │ - Registers │ │  - Sync     │ │ - Texture  │                │
│            │ - Schedular │ │             │ │ - Buffer   │                │
│            └─────────────┘ └─────────────┘ └─────────────┘                │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.2.2 GPU 设备实现

```rust
// src/gpu/mod.rs

/// GPU 设备结构体
pub struct GpuDevice {
    /// 基地址
    base_addr: u64,
    /// 控制寄存器
    regs: GpuRegisters,
    /// 命令队列
    cmd_queue: CommandQueue,
    /// 计算单元阵列
    compute_units: Vec<ComputeUnit>,
    /// 显存
    vram: Vec<u8>,
    /// 系统总线引用
    bus: RwLock<Option<Arc<SystemBus>>>,
}

/// GPU 寄存器
#[derive(Debug, Clone)]
struct GpuRegisters {
    /// 控制寄存器 (0x00)
    control: u32,
    /// 状态寄存器 (0x04)
    status: u32,
    /// 命令队列头指针 (0x08)
    queue_head: u64,
    /// 命令队列尾指针 (0x10)
    queue_tail: u64,
    /// 忙标志 (0x18)
    busy: u32,
    /// 中断标志 (0x1C)
    interrupt: u32,
}

/// 命令队列项
#[derive(Debug, Clone)]
struct GpuCommand {
    /// 命令类型
    cmd_type: GpuCmdType,
    /// 参数地址
    args_addr: u64,
    /// 同步屏障
    barrier: bool,
}

/// GPU 命令类型
#[derive(Debug, Clone, Copy)]
enum GpuCmdType {
    /// 启动内核
    LaunchKernel,
    /// 内存复制
    Memcpy,
    /// 内存填充
    Memset,
    /// 同步
    Synchronize,
    /// 绘制三角形
    DrawTriangle,
    /// 绘制矩形
    DrawRect,
}

impl GpuDevice {
    /// 创建 GPU 设备
    pub fn new(base_addr: u64, vram_size: u64) -> Self {
        let cu_count = 8;  // 8 个计算单元
        
        Self {
            base_addr,
            regs: GpuRegisters::default(),
            cmd_queue: CommandQueue::new(256),
            compute_units: (0..cu_count)
                .map(|id| ComputeUnit::new(id))
                .collect(),
            vram: vec![0u8; vram_size as usize],
            bus: RwLock::new(None),
        }
    }
    
    /// 处理命令队列
    pub fn process_command_queue(&mut self) {
        while self.cmd_queue.head != self.cmd_queue.tail {
            let cmd = self.cmd_queue.pop();
            self.execute_command(&cmd);
            
            if cmd.barrier {
                // 同步屏障：等待所有计算单元完成
                self.synchronize();
            }
        }
    }
    
    /// 执行命令
    fn execute_command(&mut self, cmd: &GpuCommand) {
        match cmd.cmd_type {
            GpuCmdType::LaunchKernel => self.launch_kernel(cmd.args_addr),
            GpuCmdType::Memcpy => self.memcpy(cmd.args_addr),
            GpuCmdType::Memset => self.memset(cmd.args_addr),
            GpuCmdType::Synchronize => self.synchronize(),
            GpuCmdType::DrawTriangle => self.draw_triangle(cmd.args_addr),
            GpuCmdType::DrawRect => self.draw_rect(cmd.args_addr),
        }
    }
    
    /// 启动计算内核
    fn launch_kernel(&mut self, args_addr: u64) {
        // 从内存读取内核参数
        let bus = self.bus.read().unwrap();
        let args = bus.as_ref().unwrap().read_kernel_args(args_addr);
        
        // 调度到计算单元
        for (i, cu) in self.compute_units.iter_mut().enumerate() {
            if i < args.workgroup_count {
                cu.execute(&args.kernel_data[i]);
            }
        }
    }
    
    /// 同步等待
    fn synchronize(&mut self) {
        for cu in &mut self.compute_units {
            while cu.is_busy() {
                // 忙等待
            }
        }
    }
}

/// 计算单元
pub struct ComputeUnit {
    /// CU ID
    id: u8,
    /// 是否繁忙
    busy: bool,
    /// 本地寄存器文件
    registers: [u32; 32],
    /// 状态
    status: ComputeUnitStatus,
}

impl ComputeUnit {
    pub fn new(id: u8) -> Self {
        Self {
            id,
            busy: false,
            registers: [0; 32],
            status: ComputeUnitStatus::Idle,
        }
    }
    
    pub fn is_busy(&self) -> bool {
        self.busy
    }
    
    pub fn execute(&mut self, kernel: &Kernel) {
        self.busy = true;
        self.status = ComputeUnitStatus::Executing;
        // 执行内核逻辑
        self.busy = false;
        self.status = ComputeUnitStatus::Idle;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComputeUnitStatus {
    Idle,
    Executing,
    WaitMemory,
    Error,
}

#[derive(Debug, Clone)]
pub struct Kernel {
    pub code_addr: u64,
    pub workgroup_size: u32,
    pub args: Vec<u32>,
}
```

### 3.3 共享内存一致性设计

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       共享内存一致性架构                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                      SystemBus                                      │   │
│  │                                                                      │   │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │   │
│  │  │  Lock    │  │  Signal  │  │  MemCopy │  │  Cache   │           │   │
│  │  │  Manager │  │  Manager │  │  Engine  │  │  Coherency│           │   │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘           │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    │                                        │
│         ┌──────────────────────────┼──────────────────────────┐           │
│         │                          │                          │           │
│         ▼                          ▼                          ▼           │
│  ┌─────────────┐          ┌─────────────┐          ┌─────────────┐        │
│  │  NPU        │          │  GPU        │          │  CPU        │        │
│  │  Local Mem  │          │  VRAM       │          │  Cache      │        │
│  └─────────────┘          └─────────────┘          └─────────────┘        │
│         │                          │                          │           │
│         └──────────────────────────┼──────────────────────────┘           │
│                                    ▼                                       │
│                          ┌─────────────────┐                               │
│                          │   主内存 (DRAM)  │                               │
│                          │                 │                               │
│                          │  - 共享缓冲区   │                               │
│                          │  - 权重存储     │                               │
│                          │  - 输入/输出    │                               │
│                          └─────────────────┘                               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 3.3.1 内存屏障实现

```rust
// src/memory/memory_fence.rs

/// 内存屏障类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MemoryFence {
    /// 读屏障：确保所有读操作在屏障前的指令看到屏障前的内存状态
    Read,
    /// 写屏障：确保所有写操作在屏障前的指令看到屏障前的内存状态
    Write,
    /// 全屏障：确保所有读/写在屏障前的指令看到屏障前的内存状态
    Full,
}

/// 内存屏障管理器
pub struct MemoryFenceManager {
    /// 挂起的读屏障计数
    pending_read_fences: AtomicU32,
    /// 挂起的写屏障计数
    pending_write_fences: AtomicU32,
    /// 挂起的全屏障计数
    pending_full_fences: AtomicU32,
    /// 等待队列
    wait_queue: Vec<Arc<Thread>>,
}

impl MemoryFenceManager {
    /// 等待读屏障
    pub fn wait_read_fence(&self) {
        while self.pending_read_fences.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(std::time::Duration::from_nanos(100));
        }
    }
    
    /// 等待写屏障
    pub fn wait_write_fence(&self) {
        while self.pending_write_fences.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(std::time::Duration::from_nanos(100));
        }
    }
    
    /// 等待全屏障
    pub fn wait_full_fence(&self) {
        while self.pending_full_fences.load(Ordering::SeqCst) > 0 {
            std::thread::sleep(std::time::Duration::from_nanos(100));
        }
    }
}
```

### 3.4 任务调度与同步机制

```rust
// src/scheduler/mod.rs

/// 异构任务调度器
pub struct HeteroScheduler {
    /// CPU 任务队列
    cpu_queue: Vec<Task>,
    /// NPU 任务队列
    npu_queue: Vec<Task>,
    /// GPU 任务队列
    gpu_queue: Vec<Task>,
    /// 任务依赖图
    dependency_graph: DependencyGraph,
    /// 全局时钟
    clock: u64,
}

impl HeteroScheduler {
    /// 提交任务
    pub fn submit_task(&mut self, task: Task) -> TaskId {
        let id = TaskId::new();
        task.id = Some(id);
        
        // 根据任务类型添加到对应队列
        match task.target {
            TaskTarget::Cpu => self.cpu_queue.push(task),
            TaskTarget::Npu => self.npu_queue.push(task),
            TaskTarget::Gpu => self.gpu_queue.push(task),
        }
        
        id
    }
    
    /// 调度下一个任务
    pub fn schedule_next(&mut self) -> Option<Task> {
        // 检查依赖
        let ready_tasks: Vec<_> = self.cpu_queue
            .iter()
            .filter(|t| self.dependency_graph.are_dependencies_met(t.id))
            .collect();
        
        if let Some(task) = ready_tasks.first() {
            self.cpu_queue.remove(0);
            return Some(task.clone());
        }
        
        None
    }
    
    /// 等待任务完成
    pub fn wait_task(&self, task_id: TaskId) {
        while !self.dependency_graph.is_completed(task_id) {
            std::thread::sleep(std::time::Duration::from_micros(10));
        }
    }
}

/// 任务定义
#[derive(Debug, Clone)]
pub struct Task {
    pub id: Option<TaskId>,
    pub target: TaskTarget,
    pub priority: u8,
    pub params: TaskParams,
    pub callback: Option<Box<dyn FnOnce() + Send>>,
}

/// 任务目标
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskTarget {
    Cpu,
    Npu,
    Gpu,
}
```

---

## 4. 关键技术挑战与解决方案

### 4.1 挑战列表

| 挑战 | 描述 | 影响 | 优先级 |
|-----|------|------|--------|
| **性能差距** | NPU/GPU 运算速度远快于 CPU 仿真 | 仿真时间不可接受 | 高 |
| **内存带宽** | 共享内存访问成为瓶颈 | 性能不均衡 | 高 |
| **同步开销** | 频繁的 CPU-NPU/GPU 同步 | 仿真速度慢 | 中 |
| **缓存一致性** | 多处理器缓存同步复杂 | 正确性问题 | 中 |
| **工具链支持** | 自定义指令需要工具链配合 | 软件开发困难 | 中 |

### 4.2 性能差距解决方案

```rust
// src/timing/npu_timing.rs

/// NPU 仿真模式
pub enum NpuSimulationMode {
    /// 直接计算（最快）
    Direct,
    
    /// 周期近似（推荐）
    CycleApprox {
        /// 估计每个 MAC 的周期数
        cycles_per_mac: u32,
        /// PE 利用率 (0.0-1.0)
        pe_utilization: f32,
    },
    
    /// 精确时钟（最慢）
    ClockAccurate,
}

impl NpuDevice {
    /// 执行运算（根据仿真模式选择策略）
    pub fn execute(&mut self, mode: NpuSimulationMode) {
        match mode {
            NpuSimulationMode::Direct => {
                // 直接计算，无延迟模拟
                self.direct_compute();
            }
            NpuSimulationMode::CycleApprox { cycles_per_mac, pe_utilization } => {
                // 估计延迟
                let total_macs = self.calculate_total_macs();
                let estimated_cycles = (total_macs as f32 * pe_utilization.recip()) as u32
                    * cycles_per_mac;
                
                self.simulate_cycles(estimated_cycles);
                self.direct_compute();
            }
            NpuSimulationMode::ClockAccurate => {
                // 逐周期模拟
                self.cycle_accurate_execute();
            }
        }
    }
    
    /// 计算总 MAC 运算次数
    fn calculate_total_macs(&self) -> u64 {
        // 根据当前配置计算 MAC 次数
        let in_channels = self.regs.in_channels as u64;
        let out_channels = self.regs.out_channels as u64;
        let height = self.regs.height as u64;
        let width = self.regs.width as u64;
        let kernel = self.regs.kernel as u64;
        
        in_channels * kernel * kernel * out_channels * (height - kernel + 1) * (width - kernel + 1)
    }
    
    /// 模拟周期延迟
    fn simulate_cycles(&self, cycles: u32) {
        // 推进模拟器时钟
        // 注意：这里需要与主模拟器同步
        // crate::simulator().advance_cycles(cycles);
    }
}
```

### 4.3 共享内存带宽优化

```rust
// src/memory/bandwidth_manager.rs

/// 内存带宽管理器
pub struct BandwidthManager {
    /// 带宽限制 (bytes/cycle)
    limits: HashMap<DeviceId, u64>,
    /// 当前带宽使用
    usage: HashMap<DeviceId, u64>,
    /// 访问队列
    queues: HashMap<DeviceId, Vec<MemoryRequest>>,
}

impl BandwidthManager {
    /// 提交内存访问请求
    pub fn submit_request(&mut self, request: MemoryRequest) {
        let device_id = request.device_id;
        
        // 检查带宽限制
        if self.is_bandwidth_exceeded(device_id) {
            // 带宽已耗尽，加入队列
            self.queues.entry(device_id)
                .or_insert_with(Vec::new)
                .push(request);
        } else {
            // 直接处理
            self.process_request(request);
        }
    }
    
    /// 检查是否超出带宽限制
    fn is_bandwidth_exceeded(&self, device_id: DeviceId) -> bool {
        let used = self.usage.get(&device_id).cloned().unwrap_or(0);
        let limit = self.limits.get(&device_id).cloned().unwrap_or(u64::MAX);
        used >= limit
    }
    
    /// 分配带宽
    pub fn allocate_bandwidth(&mut self, device_id: DeviceId, bytes: u64) {
        let current = self.usage.entry(device_id).or_insert(0);
        *current += bytes;
    }
    
    /// 释放带宽（每个周期调用）
    pub fn release_bandwidth(&mut self) {
        for usage in self.usage.values_mut() {
            *usage = 0;
        }
        
        // 处理等待队列
        for (device_id, queue) in &mut self.queues {
            let limit = self.limits.get(device_id).cloned().unwrap_or(u64::MAX);
            let mut remaining = self.usage.get(device_id).cloned().unwrap_or(0);
            
            while remaining < limit && !queue.is_empty() {
                if let Some(request) = queue.first() {
                    if remaining + request.size <= limit {
                        let req = queue.remove(0);
                        self.process_request(req);
                        remaining += req.size;
                    } else {
                        break;
                    }
                }
            }
        }
    }
}
```

### 4.4 同步机制设计

```rust
// src/sync/mod.rs

/// 事件标志（用于 CPU-NPU/GPU 同步）
pub struct EventFlags {
    flags: AtomicU32,
    waiters: Mutex<Vec<Arc<Thread>>>,
}

impl EventFlags {
    /// 创建事件标志
    pub fn new() -> Self {
        Self {
            flags: AtomicU32::new(0),
            waiters: Mutex::new(Vec::new()),
        }
    }
    
    /// 设置标志位
    pub fn set(&self, mask: u32) {
        self.flags.fetch_or(mask, Ordering::SeqCst);
        self.notify_waiters();
    }
    
    /// 清除标志位
    pub fn clear(&self, mask: u32) {
        self.flags.fetch_and(!mask, Ordering::SeqCst);
    }
    
    /// 等待标志位
    pub fn wait(&self, mask: u32) {
        loop {
            let current = self.flags.load(Ordering::SeqCst);
            if current & mask != 0 {
                return;
            }
            
            // 等待（实际实现应使用条件变量）
            std::thread::sleep(std::time::Duration::from_nanos(100));
        }
    }
    
    /// 通知等待者
    fn notify_waiters(&self) {
        let mut waiters = self.waiters.lock().unwrap();
        for waiter in waiters.drain(..) {
            waiter.wake();
        }
    }
}

/// 信号量（用于资源计数）
pub struct Semaphore {
    count: AtomicUsize,
    max_count: usize,
    waiters: Mutex<Vec<Arc<Thread>>>,
}

impl Semaphore {
    pub fn new(initial: usize, max: usize) -> Self {
        Self {
            count: AtomicUsize::new(initial),
            max_count: max,
            waiters: Mutex::new(Vec::new()),
        }
    }
    
    /// P 操作（等待）
    pub fn wait(&self) {
        loop {
            let current = self.count.load(Ordering::SeqCst);
            if current > 0 {
                if self.count.compare_exchange(
                    current, 
                    current - 1,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ).is_ok() {
                    return;
                }
            }
            
            std::thread::sleep(std::time::Duration::from_nanos(100));
        }
    }
    
    /// V 操作（信号）
    pub fn signal(&self) {
        let current = self.count.load(Ordering::SeqCst);
        if current < self.max_count {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }
}
```

---

## 5. 推荐实现路线

### 5.1 阶段性计划

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       异构计算仿真实现路线图                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Phase 1: 基础架构 (2-3 周)                                                   │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  • NPU 设备框架 (MMIO 接口)                                          │   │
│  │  • SystemBus 路由扩展                                                │   │
│  │  • 基础测试用例                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    ▼                                         │
│  Phase 2: NPU 核心 (4-6 周)                                                  │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  • PE 阵列仿真 (8x8 MAC 单元)                                        │   │
│  │  • 本地 SRAM 仿真                                                    │   │
│  │  • 卷积/矩阵乘法运算实现                                             │   │
│  │  • 性能建模                                                          │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    ▼                                         │
│  Phase 3: 指令集成 (2-3 周)                                                  │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  • CUSTOM0 指令实现                                                  │   │
│  │  • GCC/Clang 工具链支持                                              │   │
│  │  • 软件栈验证                                                        │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    ▼                                         │
│  Phase 4: GPU 扩展 (可选，4-8 周)                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  • GPU 设备框架                                                      │   │
│  │  • 命令处理器                                                        │   │
│  │  • 渲染管线模拟                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                    ▼                                         │
│  Phase 5: 优化与验证 (持续)                                                  │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  • 性能优化                                                          │   │
│  │  • 更多测试用例                                                      │   │
│  │  • 文档完善                                                          │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Phase 1 详细计划

**Week 1: 基础设施**

| 任务 | 描述 | 预估时间 |
|------|------|---------|
| NPU 模块结构 | 创建 `src/npu/` 目录和基础文件 | 1 天 |
| 寄存器定义 | 实现控制/状态/配置寄存器 | 2 天 |
| SystemBus 集成 | 添加 NPU 内存映射路由 | 1 天 |
| 基础测试 | 验证 MMIO 读写 | 1 天 |

**Week 2: 核心功能**

| 任务 | 描述 | 预估时间 |
|------|------|---------|
| PE 阵列 | 实现 8x8 MAC 单元仿真 | 3 天 |
| 本地 SRAM | 64KB 仿真存储 | 1 天 |
| 命令解析 | 实现 CMD 寄存器处理 | 2 天 |
| 集成测试 | 端到端命令执行 | 1 天 |

### 5.3 资源估算

| 资源 | 估算 |
|-----|------|
| 开发时间 | 12-20 人周 |
| 代码量 | 5,000-10,000 行 Rust |
| 测试用例 | 50+ 单元测试 + 10+ 集成测试 |

---

## 6. 参考案例分析

### 6.1 QEMU 异构仿真

**QEMU** 是最著名的开源处理器仿真器，其异构仿真方案值得参考：

| 特性 | QEMU 方案 | ruscv-sim 可借鉴点 |
|-----|----------|-------------------|
| 协处理器仿真 | 通过 TCG 插件支持 | 动态指令翻译 |
| 设备建模 | 设备模型框架 (DeviceClass) | TLM2.0 外设框架 |
| 多核支持 | SMP 模拟 | 多 hart 仿真经验 |

```c
// QEMU 协处理器注册示例
static void npu_reset(DeviceState *dev) {
    NpuState *s = NPU(dev);
    memset(s->regs, 0, sizeof(s->regs));
}

static void npu_write(void *opaque, hwaddr offset, uint64_t value,
                      unsigned size) {
    NpuState *s = opaque;
    
    switch (offset) {
        case NPU_CMD:
            if (value & NPU_CMD_START) {
                npu_start_operation(s);
            }
            break;
        case NPU_STATUS:
            // 只读，忽略写入
            break;
    }
}
```

### 6.2 Spike 仿真器

**Spike** 是 RISC-V 官方参考实现，其异构支持方案：

| 特性 | Spike 方案 | ruscv-sim 可借鉴点 |
|-----|----------|-------------------|
| CSR 扩展 | 定制 CSR 寄存器 | CSR 框架已实现 |
| PMP | 物理内存保护 | 已实现 |
| 调试接口 | DTM (Debug Transport Module) | GDB RSP 已实现 |

### 6.3 Renode 异构仿真

**Renode** 是 Antmicro 开发的开源异构仿真框架：

| 特性 | Renode 方案 | ruscv-sim 可借鉴点 |
|-----|----------|-------------------|
| 多架构支持 | ARM/RISC-V/x86 | 框架设计 |
| 外设仿真 | 设备行为建模 | TLM2.0 |
| 脚本控制 | Python API | 已支持 Python 绑定 |

### 6.4 经验总结

从参考案例中可以总结以下关键经验：

1. **分层设计**：上层应用与底层硬件解耦
2. **标准化接口**：使用成熟的接口标准（TLM2.0、GDB RSP）
3. **可扩展性**：预留扩展点（自定义指令、外设）
4. **工具链集成**：与 GCC/Clang 紧密配合
5. **性能优化**：提供多种仿真精度级别

---

## 附录

### A. 内存映射参考

| 地址范围 | 设备 | 大小 |
|---------|------|------|
| 0x0000_0000 - 0x0FFF_FFFF | 保留/未映射 | 256MB |
| 0x0200_0000 - 0x0200_FFFF | CLINT | 64KB |
| 0x0C00_0000 - 0x0FFF_FFFF | PLIC | 64MB |
| 0x1001_0000 - 0x1001_FFFF | NPU Control | 64KB |
| 0x1002_0000 - 0x1002_FFFF | NPU Data | 64KB |
| 0x1003_0000 - 0x1003_FFFF | GPU Control | 64KB |
| 0x1004_0000 - 0x1007_FFFF | GPU Channel | 256KB |
| 0xC000_0000 - 0xCFFF_FFFF | GPU VRAM | 256MB |
| 0x8000_0000 - 0xBFFF_FFFF | DRAM | 1GB |

### B. NPU 寄存器映射

| Offset | 名称 | 访问 | 描述 |
|--------|------|------|------|
| 0x00 | CMD | RW | 命令寄存器 |
| 0x04 | STATUS | RO | 状态寄存器 |
| 0x08 | CONFIG | RW | 配置参数 |
| 0x0C | IRQ_ENABLE | RW | 中断使能 |
| 0x10 | INPUT_ADDR_LO | RW | 输入地址低 32 位 |
| 0x14 | INPUT_ADDR_HI | RW | 输入地址高 32 位 |
| 0x18 | WEIGHT_ADDR_LO | RW | 权重地址低 32 位 |
| 0x1C | WEIGHT_ADDR_HI | RW | 权重地址高 32 位 |
| 0x20 | OUTPUT_ADDR_LO | RW | 输出地址低 32 位 |
| 0x24 | OUTPUT_ADDR_HI | RW | 输出地址高 32 位 |
| 0x28 | DIMS | RW | 维度参数 |

### C. 相关文档链接

- [当前架构入口](../../architecture/README.md) - 当前系统架构
- [已归档的内存架构](../designs/memory-architecture-legacy.md) - 历史内存设计
- [当前开发计划](../../dev-plan.md) - 当前开发计划
- [已归档的 NPU 集成方案](npu-integration-study.md) - 历史 NPU 方案
- [已归档的指令分发设计](../designs/instruction-dispatch-legacy.md) - 历史分发设计

---

*文档结束*
