 > **注意**：本文档中的代码示例仅用于说明设计概念和 API 用途，是伪代码性质的示例，不保证可编译通过。示例代码可能包含语法糖简写、类型占位符等，仅供理解之用。 
# SystemBus 异构扩展计划

> 文档版本: v1.0  
> 创建日期: 2026-02-05  
> 目标: 让 ruscv-sim 支持异构计算仿真 (L2 级别)

**相关文档：**
- [架构总结](heterogeneous-architecture-summary.md) - 核心决策概览
- [NPU 集成方案](npu-integration.md) - NPU 设备实现
- [异构计算研究报告](heterogeneous-computing-research.md) - 深入分析

---

## 1. 背景与目标

### 1.1 当前 SystemBus 能力

**现有实现** (`src/executor.rs`):

```rust
pub struct SystemBus {
    ram: Arc<Mutex<SimpleMemory>>,
    uart: Arc<Mutex<Uart16550>>,
    ram_base: u64,
    ram_size: usize,
    uart_base: u64,
    uart_size: usize,
}

impl SystemBus {
    pub fn new(...) { ... }
    fn is_ram(&self, addr: u64) -> bool { ... }
    fn is_uart(&self, addr: u64) -> bool { ... }
}
```

**能力分析**:

| 特性 | 当前状态 |
|------|----------|
| RAM 支持 | ✅ 已实现 |
| UART 支持 | ✅ 已实现 |
| 动态设备注册 | ❌ 缺失 |
| 多主设备 | ❌ 缺失 |
| DMA 路由 | ❌ 缺失 |
| 中断连接 | ⚠️ 需扩展 |

### 1.2 扩展目标

**L2 级别目标**:

```
┌─────────────────────────────────────────────────────────────────┐
│                    扩展目标                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ✅ 动态设备注册 - NPU/GPU 可动态添加到总线                       │
│  ✅ 多主设备支持 - CPU 和 NPU 都能发起内存访问                    │
│  ✅ DMA 事务 - NPU 直接读写内存，不经过 CPU                        │
│  ✅ 中断路由 - NPU 完成中断能通知 CPU                             │
│  ✅ 共享内存 - CPU 和 NPU 访问同一块 DRAM                         │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 2. 架构设计

### 2.1 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    扩展后 SystemBus 架构                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    SystemBus                           │   │
│  │                                                      │   │
│  │  ┌──────────────────────────────────────────────┐   │   │
│  │  │  Device Manager                             │   │   │
│  │  │  - devices: HashMap<BaseAddr, Box<Device>> │   │   │
│  │  │  - register_device()                      │   │   │
│  │  │  - unregister_device()                    │   │   │
│  │  └──────────────────────────────────────────────┘   │   │
│  │                                                      │   │
│  │  ┌──────────────────────────────────────────────┐   │   │
│  │  │  DMA Controller                            │   │   │
│  │  │  - dma_read()                              │   │   │
│  │  │  - dma_write()                            │   │   │
│  │  │  - 仲裁策略                                │   │   │
│  │  └──────────────────────────────────────────────┘   │   │
│  │                                                      │   │
│  │  ┌──────────────────────────────────────────────┐   │   │
│  │  │  Memory Router                             │   │   │
│  │  │  - RAM 路由                                │   │   │
│  │  │  - MMIO 路由                               │   │   │
│  │  │  - 地址冲突检测                            │   │   │
│  │  └──────────────────────────────────────────────┘   │   │
│  │                                                      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                            │                                    │
│                            ▼                                    │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Memory Subsystem                      │   │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐      │   │
│  │  │    RAM     │  │   MMIO     │  │   PLIC    │      │   │
│  │  └────────────┘  └────────────┘  └────────────┘      │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 设备接口设计

```rust
// src/bus/device.rs

/// 设备 trait - 所有设备必须实现
pub trait Device: Send + Sync {
    /// 设备名称
    fn name(&self) -> &'static str;
    
    /// 基地址
    fn base_addr(&self) -> u64;
    
    /// 内存大小
    fn size(&self) -> usize;
    
    /// MMIO 读
    fn read(&self, offset: u64, size: u8) -> Result<u32, DeviceError>;
    
    /// MMIO 写
    fn write(&mut self, offset: u64, value: u32, size: u8) -> Result<(), DeviceError>;
    
    /// 中断号（可选）
    fn interrupt(&self) -> Option<u32> {
        None
    }
}

/// 主设备 trait - 能发起 DMA 的设备
pub trait MasterDevice: Device {
    /// DMA 读
    fn dma_read(&self, addr: u64, size: usize) -> Result<Vec<u8>, DeviceError>;
    
    /// DMA 写
    fn dma_write(&self, addr: u64, data: &[u8]) -> Result<(), DeviceError>;
}

/// 设备错误类型
#[derive(Debug, Error)]
pub enum DeviceError {
    #[error("Invalid address: 0x{0:016x}")]
    InvalidAddress(u64),
    
    #[error("Invalid offset: 0x{0:016x}")]
    InvalidOffset(u64),
    
    #[error("Invalid size: {0}")]
    InvalidSize(u8),
    
    #[error("Device busy")]
    Busy,
    
    #[error("Timeout")]
    Timeout,
}
```

### 2.3 SystemBus 扩展

```rust
// src/bus/mod.rs

/// 扩展后的 SystemBus
pub struct SystemBus {
    /// RAM
    ram: Arc<Mutex<SimpleMemory>>,
    ram_base: u64,
    ram_size: usize,
    
    /// 设备表 (基地址 → 设备)
    devices: HashMap<u64, Box<dyn Device>>,
    
    /// DMA 控制器
    dma_controller: DmaController,
    
    /// PLIC 引用
    plic: Option<Arc<Mutex<Plic>>>,
    
    /// 统计信息
    stats: BusStats,
}

impl SystemBus {
    /// 创建 SystemBus
    pub fn new(ram: Arc<Mutex<SimpleMemory>>, ram_base: u64, ram_size: usize) -> Self {
        Self {
            ram,
            ram_base,
            ram_size,
            devices: HashMap::new(),
            dma_controller: DmaController::new(),
            plic: None,
            stats: BusStats::new(),
        }
    }
    
    /// 连接 PLIC
    pub fn connect_plic(&mut self, plic: Arc<Mutex<Plic>>) {
        self.plic = Some(plic);
    }
    
    /// 动态注册设备
    pub fn register_device(&mut self, device: Box<dyn Device>) -> Result<(), BusError> {
        let base = device.base_addr();
        let size = device.size();
        let end = base + size as u64;
        
        // 1. 检查地址冲突
        for (addr, dev) in &self.devices {
            let dev_end = dev.base_addr() + dev.size() as u64;
            if (base >= *addr && base < dev_end) || 
               (end > *addr && end <= dev_end) {
                return Err(BusError::AddressConflict {
                    new_device: device.name(),
                    existing_device: dev.name(),
                });
            }
        }
        
        // 2. 如果设备有中断，注册到 PLIC
        if let Some(irq) = device.interrupt() {
            if let Some(ref plic) = self.plic {
                let mut plic_guard = plic.lock().unwrap();
                plic_guard.write_priority(irq, 5);  // 默认优先级
                plic_guard.write_enable(0, 0, 1 << irq);
            }
        }
        
        // 3. 注册设备
        self.devices.insert(base, device);
        
        info!("Registered device at 0x{:016x}", base);
        Ok(())
    }
    
    /// 注销设备
    pub fn unregister_device(&mut self, base_addr: u64) -> Result<(), BusError> {
        if let Some(device) = self.devices.remove(&base_addr) {
            info!("Unregistered device: {}", device.name());
            Ok(())
        } else {
            Err(BusError::DeviceNotFound)
        }
    }
    
    /// MMIO 读
    pub fn read(&self, addr: u64, size: u8) -> Result<u32, BusError> {
        self.stats.reads += 1;
        
        // RAM 访问
        if self.is_ram(addr) {
            return self.ram.lock().unwrap().read(addr - self.ram_base, size);
        }
        
        // MMIO 访问
        for (base, device) in &self.devices {
            if addr >= *base && addr < *base + device.size() as u64 {
                let offset = addr - *base;
                return device.read(offset, size).map_err(Into::into);
            }
        }
        
        Err(BusError::InvalidAddress(addr))
    }
    
    /// MMIO 写
    pub fn write(&mut self, addr: u64, value: u32, size: u8) -> Result<(), BusError> {
        self.stats.writes += 1;
        
        // RAM 访问
        if self.is_ram(addr) {
            return self.ram.lock().unwrap().write(addr - self.ram_base, value, size);
        }
        
        // MMIO 访问
        for (base, device) in &self.devices {
            if addr >= *base && addr < *base + device.size() as u64 {
                let offset = addr - *base;
                // 需要可变引用，使用 get_mut
                let device_ref = self.devices.get_mut(&base).unwrap();
                return device_ref.write(offset, value, size).map_err(Into::into);
            }
        }
        
        Err(BusError::InvalidAddress(addr))
    }
    
    /// DMA 读（主设备发起）
    pub fn dma_read(&self, initiator: u64, addr: u64, size: usize) -> Result<Vec<u8>, BusError> {
        self.stats.dma_reads += 1;
        
        // 记录 DMA 事务
        self.dma_controller.record_transaction(initiator, addr, size, DmaDirection::Read);
        
        if self.is_ram(addr) {
            // RAM DMA
            self.ram.lock().unwrap().dma_read(addr - self.ram_base, size)
        } else {
            // MMIO DMA
            let mut data = vec![0u8; size];
            for (base, device) in &self.devices {
                if addr >= *base && addr < *base + device.size() as u64 {
                    let offset = addr - *base;
                    for i in 0..size {
                        data[i] = device.read(offset + i as u64, 1)? as u8;
                    }
                    return Ok(data);
                }
            }
            Err(BusError::InvalidAddress(addr))
        }
    }
    
    /// DMA 写
    pub fn dma_write(&self, initiator: u64, addr: u64, data: &[u8]) -> Result<(), BusError> {
        self.stats.dma_writes += 1;
        
        self.dma_controller.record_transaction(initiator, addr, data.len(), DmaDirection::Write);
        
        if self.is_ram(addr) {
            self.ram.lock().unwrap().dma_write(addr - self.ram_base, data)
        } else {
            for (base, device) in &self.devices {
                if addr >= *base && addr < *base + device.size() as u64 {
                    let offset = addr - *base;
                    for (i, &byte) in data.iter().enumerate() {
                        device.write(offset + i as u64, byte as u32, 1)?;
                    }
                    return Ok(());
                }
            }
            Err(BusError::InvalidAddress(addr))
        }
    }
    
    /// 触发设备中断
    pub fn trigger_interrupt(&self, device_base: u64) -> Result<(), BusError> {
        let device = self.devices.get(&device_base)
            .ok_or(BusError::DeviceNotFound)?;
        
        if let Some(irq) = device.interrupt() {
            if let Some(ref plic) = self.plic {
                plic.lock().unwrap().trigger_interrupt(irq);
            }
        }
        
        Ok(())
    }
    
    /// 获取总线统计
    pub fn stats(&self) -> &BusStats {
        &self.stats
    }
    
    fn is_ram(&self, addr: u64) -> bool {
        addr >= self.ram_base && addr < self.ram_base + self.ram_size as u64
    }
}

/// 总线错误类型
#[derive(Debug, Error)]
pub enum BusError {
    #[error("Invalid address: 0x{0:016x}")]
    InvalidAddress(u64),
    
    #[error("Address conflict: {new_device} vs {existing_device}")]
    AddressConflict {
        new_device: &'static str,
        existing_device: &'static str,
    },
    
    #[error("Device not found")]
    DeviceNotFound,
    
    #[error("{0}")]
    Device(#[from] DeviceError),
}

/// 总线统计
#[derive(Debug, Default)]
pub struct BusStats {
    pub reads: u64,
    pub writes: u64,
    pub dma_reads: u64,
    pub dma_writes: u64,
}
```

### 2.4 DMA 控制器

```rust
// src/bus/dma.rs

/// DMA 控制器
pub struct DmaController {
    /// 事务队列
    transactions: Vec<DmaTransaction>,
    /// 仲裁策略
    arbitration: ArbitrationPolicy,
    /// 带宽限制 (bytes/cycle)
    bandwidth_limit: u64,
}

/// DMA 事务
#[derive(Debug)]
pub struct DmaTransaction {
    /// 发起者 ID
    initiator: u64,
    /// 地址
    addr: u64,
    /// 大小
    size: usize,
    /// 方向
    direction: DmaDirection,
    /// 时间戳
    timestamp: u64,
}

/// DMA 方向
#[derive(Debug, Clone, Copy)]
pub enum DmaDirection {
    Read,
    Write,
}

/// 仲裁策略
#[derive(Debug, Clone, Copy)]
pub enum ArbitrationPolicy {
    /// 先来先服务
    Fifo,
    /// 轮询
    RoundRobin,
    /// 固定优先级
    FixedPriority,
}

impl DmaController {
    pub fn new() -> Self {
        Self {
            transactions: Vec::new(),
            arbitration: ArbitrationPolicy::Fifo,
            bandwidth_limit: u64::MAX,  // 暂无限制
        }
    }
    
    pub fn record_transaction(
        &mut self, 
        initiator: u64, 
        addr: u64, 
        size: usize, 
        direction: DmaDirection
    ) {
        self.transactions.push(DmaTransaction {
            initiator,
            addr,
            size,
            direction,
            timestamp: 0,  // TODO: 添加时间戳
        });
    }
    
    pub fn set_arbitration_policy(&mut self, policy: ArbitrationPolicy) {
        self.arbitration = policy;
    }
    
    pub fn set_bandwidth_limit(&mut self, limit: u64) {
        self.bandwidth_limit = limit;
    }
}
```

---

## 3. NPU 设备实现

### 3.1 NPU 设备结构

```rust
// src/peripherals/npu.rs

/// NPU 设备（主设备）
pub struct NpuDevice {
    /// 基地址
    base_addr: u64,
    
    /// 控制寄存器
    registers: NpuRegisters,
    
    /// PE 阵列
    pe_array: PeArray,
    
    /// 本地 SRAM
    local_sram: Vec<u8>,
    
    /// 系统总线引用（用于 DMA）
    bus: Option<Arc<SystemBus>>,
    
    /// PLIC 引用（用于中断）
    plic: Option<Arc<Mutex<Plic>>>,
}

/// NPU 寄存器
#[derive(Debug, Clone, Copy)]
struct NpuRegisters {
    /// 命令寄存器 (0x00)
    cmd: u32,
    /// 状态寄存器 (0x04)
    status: NpuStatus,
    /// 配置寄存器 (0x08)
    config: u32,
    /// 输入地址 (0x0C)
    input_addr: u64,
    /// 权重地址 (0x10)
    weight_addr: u64,
    /// 输出地址 (0x14)
    output_addr: u64,
}

/// NPU 状态
#[derive(Debug, Clone, Copy, PartialEq)]
enum NpuStatus {
    Idle = 0,
    Running = 1,
    Done = 2,
    Error = 3,
}

impl NpuRegisters {
    fn status(&self) -> u32 {
        match self.status {
            NpuStatus::Idle => 0,
            NpuStatus::Running => 1,
            NpuStatus::Done => 2,
            NpuStatus::Error => 3,
        }
    }
}
```

### 3.2 设备实现

```rust
// Device trait 实现
impl Device for NpuDevice {
    fn name(&self) -> &'static str { "NPU" }
    
    fn base_addr(&self) -> u64 { self.base_addr }
    
    fn size(&self) -> usize { 0x10000 }
    
    fn read(&self, offset: u64, _size: u8) -> Result<u32, DeviceError> {
        match offset {
            0x00 => Ok(self.registers.cmd),
            0x04 => Ok(self.registers.status() as u32),
            0x08 => Ok(self.registers.config),
            _ => Err(DeviceError::InvalidOffset(offset)),
        }
    }
    
    fn write(&mut self, offset: u64, value: u32, _size: u8) -> Result<(), DeviceError> {
        match offset {
            0x00 => {
                self.registers.cmd = value;
                self.process_command();  // 启动 NPU
                Ok(())
            }
            0x08 => {
                self.registers.config = value;
                Ok(())
            }
            0x0C => {
                self.registers.input_addr = value as u64;
                Ok(())
            }
            0x10 => {
                self.registers.weight_addr = value as u64;
                Ok(())
            }
            0x14 => {
                self.registers.output_addr = value as u64;
                Ok(())
            }
            _ => Err(DeviceError::InvalidOffset(offset)),
        }
    }
    
    fn interrupt(&self) -> Option<u32> {
        Some(10)  // NPU 中断号
    }
}

// MasterDevice trait 实现
impl MasterDevice for NpuDevice {
    fn dma_read(&self, addr: u64, size: usize) -> Result<Vec<u8>, DeviceError> {
        match &self.bus {
            Some(bus) => {
                bus.dma_read(self.base_addr, addr, size)
            }
            None => Err(DeviceError::Busy),  // 总线未连接
        }
    }
    
    fn dma_write(&self, addr: u64, data: &[u8]) -> Result<(), DeviceError> {
        match &self.bus {
            Some(bus) => {
                bus.dma_write(self.base_addr, addr, data);
                Ok(())
            }
            None => Err(DeviceError::Busy),
        }
    }
}

impl NpuDevice {
    /// 创建 NPU
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            registers: NpuRegisters::default(),
            pe_array: PeArray::new(8, 8),  // 8x8 PE 阵列
            local_sram: vec![0u8; 64 * 1024],  // 64KB
            bus: None,
            plic: None,
        }
    }
    
    /// 连接总线
    pub fn connect_bus(&mut self, bus: &Arc<SystemBus>) {
        self.bus = Some(bus.clone());
    }
    
    /// 连接 PLIC
    pub fn connect_plic(&mut self, plic: &Arc<Mutex<Plic>>) {
        self.plic = Some(plic.clone());
    }
    
    /// 处理命令
    fn process_command(&mut self) {
        let cmd = self.registers.cmd;
        let cmd_type = cmd & 0xF;  // 低 4 位是命令类型
        
        match cmd_type {
            0 => self.execute_conv2d(),  // 卷积
            1 => self.execute_gemm(),   // GEMM
            2 => self.execute_pool(),   // 池化
            _ => {
                self.registers.status = NpuStatus::Error;
            }
        }
    }
    
    /// 执行卷积
    fn execute_conv2d(&mut self) {
        self.registers.status = NpuStatus::Running;
        
        // 1. DMA 读输入
        let input = match self.dma_read(self.registers.input_addr, 1024) {
            Ok(data) => data,
            Err(e) => {
                error!("NPU DMA read failed: {:?}", e);
                self.registers.status = NpuStatus::Error;
                return;
            }
        };
        
        // 2. DMA 读权重
        let weights = match self.dma_read(self.registers.weight_addr, 512) {
            Ok(data) => data,
            Err(e) => {
                error!("NPU DMA read failed: {:?}", e);
                self.registers.status = NpuStatus::Error;
                return;
            }
        };
        
        // 3. 执行卷积（功能验证版）
        let mut output = vec![0i32; 256];
        self.pe_array.conv2d(&input, &weights, &mut output);
        
        // 4. DMA 写回结果
        let output_bytes: Vec<u8> = output.iter()
            .flat_map(|&v| v.to_le_bytes())
            .collect();
        
        if let Err(e) = self.dma_write(self.registers.output_addr, &output_bytes) {
            error!("NPU DMA write failed: {:?}", e);
            self.registers.status = NpuStatus::Error;
            return;
        }
        
        // 5. 标记完成并触发中断
        self.registers.status = NpuStatus::Done;
        self.trigger_interrupt();
        
        info!("NPU conv2d completed");
    }
    
    /// 触发中断
    fn trigger_interrupt(&self) {
        if let Some(ref plic) = self.plic {
            plic.lock().unwrap().trigger_interrupt(10);  // IRQ 10
        }
    }
}
```

---

## 4. PE 阵列实现

### 4.1 功能验证版

```rust
// src/npu/pe_array.rs

/// PE 阵列（功能验证版）
pub struct PeArray {
    rows: usize,
    cols: usize,
}

impl PeArray {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self { rows, cols }
    }
    
    /// 卷积运算 - 功能验证版
    pub fn conv2d(&self, input: &[u8], weights: &[u8], output: &mut [i32]) {
        // 简化实现：正确性优先
        let in_channels = 1;
        let out_channels = 1;
        let height = 32;
        let width = 32;
        let kernel = 3;
        
        for oc in 0..out_channels {
            for h in 0..(height - kernel + 1) {
                for w in 0..(width - kernel + 1) {
                    let mut sum = 0i32;
                    
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
    
    /// GEMM - 功能验证版
    pub fn gemm(&self, a: &[i32], b: &[i32], c: &mut [i32], m: usize, n: usize, k: usize) {
        for i in 0..m {
            for j in 0..n {
                let mut sum = 0i32;
                for p in 0..k {
                    sum += a[i * k + p] * b[p * n + j];
                }
                c[i * n + j] = sum;
            }
        }
    }
}
```

---

## 5. 使用示例

```rust
// examples/heterogeneous.rs

use ruscv_sim::{
    peripherals::{Plic, NpuDevice, UART_SIZE, PLIC_SIZE},
    bus::{SystemBus, Device},
    memory::SimpleMemory,
};
use std::sync::{Arc, Mutex};

const RAM_BASE: u64 = 0x8000_0000;
const RAM_SIZE: usize = 256 * 1024 * 1024;  // 256MB

const NPU_BASE: u64 = 0x1000_0000;
const NPU_SIZE: usize = 64 * 1024;  // 64KB

const UART_BASE: u64 = 0x1000_1000;

const PLIC_BASE: u64 = 0x0C00_0000;

fn main() {
    // 1. 创建内存
    let ram = Arc::new(Mutex::new(SimpleMemory::new(RAM_SIZE)));
    
    // 2. 创建 PLIC
    let plic = Arc::new(Mutex::new(Plic::new(PLIC_BASE, 32, 2)));
    
    // 3. 创建 SystemBus
    let mut bus = SystemBus::new(ram.clone(), RAM_BASE, RAM_SIZE);
    bus.connect_plic(plic.clone());
    
    // 4. 创建并注册 NPU
    let mut npu = Box::new(NpuDevice::new(NPU_BASE));
    npu.connect_bus(&Arc::new(Mutex::new(bus.clone())));
    npu.connect_plic(&plic);
    bus.register_device(npu).unwrap();
    
    // 5. 创建并注册 UART
    let uart = Box::new(Uart16550::new(UART_BASE));
    bus.register_device(uart).unwrap();
    
    // 6. 使用 NPU（通过 CPU 指令）
    // CPU 执行:
    //   li t0, NPU_BASE + 0x0C   # 输入地址寄存器
    //   li t1, 0x8000_1000       # 输入数据地址
    //   sw t1, 0x0C(t0)          # 写入输入地址
    //   li t0, NPU_BASE          # 命令寄存器
    //   li t1, 0                 # 卷积命令
    //   sw t1, 0(t0)             # 启动 NPU
    
    println!("Heterogeneous system ready!");
    println!("RAM: 0x{:016x} - 0x{:016x}", RAM_BASE, RAM_BASE + RAM_SIZE as u64);
    println!("NPU: 0x{:016x} - 0x{:016x}", NPU_BASE, NPU_BASE + NPU_SIZE as u64);
    println!("UART: 0x{:016x}", UART_BASE);
    println!("PLIC: 0x{:016x}", PLIC_BASE);
}
```

---

## 6. 实现计划

### 6.1 文件结构

```
src/
├── bus/
│   ├── mod.rs              # SystemBus 主文件
│   ├── device.rs           # Device trait
│   ├── dma.rs              # DMA 控制器
│   └── error.rs            # 错误类型
├── peripherals/
│   ├── mod.rs              # 已存在
│   ├── npu.rs              # NPU 设备（新增）
│   ├── clint.rs            # 已存在
│   ├── plic.rs             # 已存在
│   └── uart16550.rs        # 已存在
└── npu/
    ├── mod.rs              # NPU 模块入口
    └── pe_array.rs         # PE 阵列（新增）
```

### 6.2 开发阶段

| 阶段 | 任务 | 预估时间 |
|------|------|----------|
| **Phase 1** | Device trait + SystemBus 扩展 | 2 天 |
| **Phase 2** | DMA Controller | 1 天 |
| **Phase 3** | NPU 设备实现（功能验证版）| 3 天 |
| **Phase 4** | PE 阵列实现 | 2 天 |
| **Phase 5** | 测试 + 文档 | 2 天 |

### 6.3 测试计划

```rust
// tests/heterogeneous_tests.rs

#[test]
fn test_device_registration() {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(1024)));
    let mut bus = SystemBus::new(ram, 0x8000_0000, 1024);
    
    // 注册 NPU
    let npu = Box::new(NpuDevice::new(0x1000_0000));
    assert!(bus.register_device(npu).is_ok());
    
    // 重复注册应该失败
    let npu2 = Box::new(NpuDevice::new(0x1000_0000));
    assert!(bus.register_device(npu2).is_err());
}

#[test]
fn test_npu_dma() {
    let ram = Arc::new(Mutex::new(SimpleMemory::new(1024)));
    let mut bus = SystemBus::new(ram.clone(), 0x8000_0000, 1024);
    
    // 写入测试数据
    let test_data = [1u8, 2, 3, 4, 5, 6, 7, 8];
    ram.lock().unwrap().write(0x8000_0100, &test_data);
    
    // 模拟 NPU DMA 读
    let result = bus.dma_read(0x1000_0000, 0x8000_0100, 8).unwrap();
    assert_eq!(&result, &test_data);
}

#[test]
fn test_npu_interrupt() {
    let plic = Arc::new(Mutex::new(Plic::new(0x0C00_0000, 32, 2)));
    
    // 配置 NPU 中断
    let mut npu = NpuDevice::new(0x1000_0000);
    npu.connect_plic(&plic);
    
    // 触发中断
    npu.trigger_interrupt();
    
    // 验证中断挂起
    assert!(plic.lock().unwrap().is_pending(10));
}
```

---

## 7. 风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| 地址冲突 | 中 | 中 | 严格的冲突检测 |
| 并发访问冲突 | 中 | 高 | 使用 Mutex |
| 性能瓶颈 | 低 | 中 | 后续优化 |
| PLIC 连接问题 | 低 | 低 | 完善的测试 |

---

## 8. 后续扩展

### L3 级别扩展

```rust
// 缓存一致性（未来）
pub trait CacheCoherent {
    fn invalidate(&self, addr: u64, size: usize);
    fn flush(&self, addr: u64, size: usize);
}

// 多主设备仲裁（未来）
pub enum Arbitration {
    Fifo,
    RoundRobin { weights: HashMap<u64, u32> },
    Priority { priority: HashMap<u64, u8> },
}
```

---

## 附录

### A. 寄存器映射表

| Offset | 名称 | 访问 | 描述 |
|--------|------|------|------|
| 0x00 | CMD | RW | 命令寄存器 |
| 0x04 | STATUS | RO | 状态寄存器 |
| 0x08 | CONFIG | RW | 配置寄存器 |
| 0x0C | INPUT_ADDR | RW | 输入地址 |
| 0x10 | WEIGHT_ADDR | RW | 权重地址 |
| 0x14 | OUTPUT_ADDR | RW | 输出地址 |

### B. 命令编码

| 命令 | 编码 | 描述 |
|------|------|------|
| CONV2D | 0x0 | 二维卷积 |
| GEMM | 0x1 | 通用矩阵乘法 |
| POOL | 0x2 | 池化操作 |

---

*文档结束*
