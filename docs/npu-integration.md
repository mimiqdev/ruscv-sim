# NPU 架构集成到 ruscv-sim 的技术方案

> 文档版本: v1.0  
> 创建日期: 2026-02-05  
> 项目状态: M7: RISCOF + arch-test 集成阶段

**相关文档：**
- [架构总结](heterogeneous-architecture-summary.md) - 核心决策概览
- [SystemBus 扩展计划](systembus-extension-plan.md) - 设备 trait 设计
- [异构计算研究报告](heterogeneous-computing-research.md) - 深入分析

---

## 1. 背景与目标

### 1.1 ruscv-sim 项目现状

ruscv-sim 是一个用 Rust 实现的 RISC-V ISS（指令集模拟器），当前处于 **M7: RISCOF + arch-test 集成阶段**。

**已完成里程碑：**
- M1: RV64IMAFDC 核心指令集完整实现 (~10,000 行)
- M2: Sv39 MMU/TLB + TLM2.0 外设框架
- M3: 边界测试和原子性改进 (704 个测试全部通过)
- M4: GDB RSP 调试接口
- M5: ELF 执行闭环
- M6: 测试质量强化

**技术栈特征：**
- Rust 语言实现
- TLM2.0 外设抽象
- 动态内存接口 (dyn MemoryInterface)
- SystemBus 路由架构

### 1.2 集成 NPU 的战略意义

**核心价值：**
1. **差异化竞争力** - 区别于通用 RISC-V ISS
2. **异构仿真能力** - 支持 AI/ML 场景仿真
3. **生态扩展** - 吸引 NPU 开发者社区

**挑战：**
- NPU 微架构复杂度高
- 软件栈依赖重
- 验证难度大

---

## 2. 集成架构设计

### 2.1 系统架构总览

```
┌─────────────────────────────────────────────────────────────────┐
│                      ruscv-sim 系统架构                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐        │
│  │  RISC-V     │    │   NPU       │    │   外设      │        │
│  │   Core      │    │   Core      │    │   模块      │        │
│  │             │    │             │    │             │        │
│  │  - Decode   │◄──►│  - PE Array │◄──►│  - CLINT    │        │
│  │  - Execute  │    │  - Scheduler│    │  - PLIC     │        │
│  │  - CSR     │    │  - Local Mem│    │  - UART     │        │
│  └─────────────┘    └─────────────┘    └─────────────┘        │
│         │                  │                  │               │
│         └──────────────────┼──────────────────┘               │
│                            │                                  │
│                   ┌────────┴────────┐                         │
│                   │   SystemBus    │                         │
│                   │                │                         │
│                   │  - 地址路由    │                         │
│                   │  - 内存映射    │                         │
│                   │  - 事务转发    │                         │
│                   └────────┬────────┘                         │
│                            │                                  │
│                   ┌────────┴────────┐                         │
│                   │   ELF Loader    │                         │
│                   │   + Simulator   │                         │
│                   └─────────────────┘                         │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 NPU 模块在 TLM 架构中的位置

```
ruscv-sim 内存映射（添加 NPU 后）:

    0x0000_0000 - 0x0FFF_FFFF   │  CLINT          (0x1000_0000)  │
    0x1000_0000 - 0x1000_FFFF  │  UART 16550     (0x1000_0000)  │
    0x1001_0000 - 0x1001_FFFF  │  NPU Control    (0x1001_0000)  │
    0x1002_0000 - 0x1002_FFFF  │  NPU Data       (0x1002_0000)  │
    ...                         │                          ...   │
    0x8000_0000 -              │  DRAM                       │
```

### 2.3 NPU 模块内部架构

```
┌─────────────────────────────────────────────────────────────────┐
│                      NPU Core 模块                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    控制寄存器接口                         │   │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐   │   │
│  │  │  CMD     │ │  STATUS  │ │  CONFIG  │ │  IRQ     │   │   │
│  │  │  (0x00)  │ │  (0x04)  │ │  (0x08)  │ │  (0x0C)  │   │   │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                            │                                    │
│                   ┌────────┴────────┐                           │
│                   │  Control FSM    │                           │
│                   │                 │                           │
│                   │  - 任务解析     │                           │
│                   │  - 调度决策     │                           │
│                   │  - 状态机管理   │                           │
│                   └────────┬────────┘                           │
│                            │                                    │
│         ┌──────────────────┼──────────────────┐                 │
│         │                  │                  │                 │
│  ┌──────┴──────┐    ┌──────┴──────┐    ┌──────┴──────┐        │
│  │   PE Array  │    │  Data Mover │    │  Local SRAM │        │
│  │             │    │             │    │             │        │
│  │  4x4 / 8x8  │    │  - DMA 引擎 │    │  32KB-256KB │        │
│  │  MAC 单元   │    │  - 预取     │    │  - 权重缓存  │        │
│  │  - INT8/16  │    │  - 压缩支持 │    │  - 特征缓存 │        │
│  └─────────────┘    └─────────────┘    └─────────────┘        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## 3. Rust 实现方案

### 3.1 模块结构设计

```
src/
├── npu/
│   ├── mod.rs                    # 模块入口
│   ├── types.rs                  # 类型定义
│   ├── registers.rs              # 控制寄存器
│   ├── pe_array.rs               # PE 阵列仿真
│   ├── scheduler.rs              # 任务调度器
│   ├── data_mover.rs             # 数据搬运引擎
│   ├── local_memory.rs           # 本地 SRAM
│   └── transaction.rs             # 事务处理
├── bus/
│   └── system_bus.rs             # 已存在，添加 NPU 路由
└── ...
```

### 3.2 核心 trait 设计

```rust
// NPU 设备 trait（遵循 TLM2.0 模式）
pub trait NpuDevice {
    /// 处理 MMIO 读操作
    fn read(&self, offset: u32, size: u8) -> Result<u32, BusError>;
    
    /// 处理 MMIO 写操作
    fn write(&self, offset: u32, data: u32, size: u8) -> Result<(), BusError>;
    
    /// 获取 NPU 状态
    fn status(&self) -> NpuStatus;
}

/// NPU 命令接口
#[derive(Debug, Clone, Copy)]
pub enum NpuCommand {
    /// 启动卷积运算
    Conv2D {
        input_addr: u64,
        weight_addr: u64,
        output_addr: u64,
        channels: u16,
        height: u16,
        width: u16,
        kernel: u8,
        stride: u8,
    },
    /// 启动矩阵乘法
    Gemm {
        a_addr: u64,
        b_addr: u64,
        c_addr: u64,
        m: u16,
        n: u16,
        k: u16,
    },
    /// 启动池化操作
    Pool {
        input_addr: u64,
        output_addr: u64,
        height: u16,
        width: u16,
        kernel: u8,
        stride: u8,
        pool_type: PoolType,
    },
    /// 同步等待
    Sync,
}

/// NPU 状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NpuStatus {
    Idle,
    Running,
    Done,
    Error(NpuError),
}

/// NPU 错误类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NpuError {
    InvalidCommand,
    InvalidParameter,
    MemoryAccessError,
    Overflow,
    Timeout,
}
```

### 3.3 PE 阵列仿真实现

```rust
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

### 3.4 NPU 设备实现

```rust
/// NPU 设备结构体
pub struct Npu {
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
    /// 仿真器上下文引用
    sim: Weak<dyn Simulator>,
    /// 系统总线引用
    bus: RwLock<Option<SystemBus>>,
}

impl Npu {
    /// 创建 NPU 设备
    pub fn new(base_addr: u64) -> Self {
        Self {
            base_addr,
            regs: NpuRegisters::default(),
            pe_array: PeArray::new(8, 8),  // 8x8 PE 阵列
            local_sram: vec![0; 64 * 1024], // 64KB 本地 SRAM
            scheduler: NpuScheduler::new(),
            sim: Weak::new(),
            bus: RwLock::new(None),
        }
    }
    
    /// 初始化（设置仿真器引用）
    pub fn initialize(&mut self, sim: &Arc<dyn Simulator>) {
        self.sim = Arc::downgrade(sim);
    }
    
    /// 连接到系统总线
    pub fn connect_bus(&self, bus: &Arc<RwLock<SystemBus>>) {
        *self.bus.write().unwrap() = Some(Arc::clone(bus));
    }
    
    /// 处理 MMIO 读
    pub fn read(&self, offset: u32) -> Result<u32, BusError> {
        match offset {
            // 状态寄存器
            0x00 => Ok(self.regs.status.bits()),
            // 命令寄存器
            0x04 => Ok(self.regs.command.bits()),
            // 清除中断
            0x08 => Ok(0),
            // ... 其他寄存器
            _ => Err(BusError::LoadAccessFault(self.base_addr + offset as u64)),
        }
    }
    
    /// 处理 MMIO 写
    pub fn write(&mut self, offset: u32, value: u32) -> Result<(), BusError> {
        match offset {
            // 命令寄存器
            0x04 => {
                self.regs.command = NpuCommandReg::from_bits(value);
                self.process_command()?;
                Ok(())
            }
            // 清除中断
            0x08 => {
                self.regs.clear_irq();
                Ok(())
            }
            // ... 其他寄存器
            _ => Err(BusError::StoreAccessFault(self.base_addr + offset as u64)),
        }
    }
    
    /// 处理命令
    fn process_command(&mut self) -> Result<(), NpuError> {
        if !self.regs.command.start() {
            return Ok(());
        }
        
        self.regs.set_busy(true);
        
        match self.regs.command.cmd_type() {
            NpuCmdType::Conv2D => self.execute_conv2d(),
            NpuCmdType::Gemm => self.execute_gemm(),
            NpuCmdType::Pool => self.execute_pool(),
            NpuCmdType::Sync => self.execute_sync(),
        }
    }
    
    /// 执行卷积
    fn execute_conv2d(&mut self) -> Result<(), NpuError> {
        let params = self.regs.conv_params;
        
        // 从内存读取数据
        let input = self.load_from_memory(params.input_addr, params.input_size as usize)?;
        let weights = self.load_from_memory(params.weight_addr, params.weight_size as usize)?;
        let mut output = vec![0i32; params.output_size as usize];
        
        // 执行卷积运算
        self.pe_array.conv2d(
            &input,
            &weights,
            &mut output,
            params.in_channels as usize,
            params.out_channels as usize,
            params.height as usize,
            params.width as usize,
            params.kernel as usize,
        );
        
        // 写回结果
        self.store_to_memory(params.output_addr, &output)?;
        
        self.regs.set_done(true);
        self.regs.set_busy(false);
        
        Ok(())
    }
    
    /// 从系统内存加载数据
    fn load_from_memory(&self, addr: u64, size: usize) -> Result<Vec<u8>, NpuError> {
        let bus = self.bus.read().unwrap();
        let bus = bus.as_ref().ok_or(NpuError::MemoryAccessError)?;
        
        let mut data = vec![0u8; size];
        bus.read(addr, &mut data).map_err(|_| NpuError::MemoryAccessError)?;
        
        Ok(data)
    }
    
    /// 存储数据到系统内存
    fn store_to_memory(&self, addr: u64, data: &[u8]) -> Result<(), NpuError> {
        let bus = self.bus.read().unwrap();
        let bus = bus.as_ref().ok_or(NpuError::MemoryAccessError)?;
        
        bus.write(addr, data).map_err(|_| NpuError::MemoryAccessError)?;
        
        Ok(())
    }
    
    // ... 其他命令实现
}
```

---

## 4. RISC-V 交互方案

### 4.1 集成方式选择

| 方案 | 描述 | 优点 | 缺点 | 推荐场景 |
|------|------|------|------|---------|
| **MMIO** | NPU 作为内存映射外设 | 简单、兼容 | 延迟高 | 快速原型 |
| **Custom Inst** | RISC-V 定制指令 | 低延迟、透明 | 复杂、需工具链 | 性能关键 |
| **协处理器接口** | 标准 RISC-V 协处理器 | 平衡 | 中等复杂度 | 推荐方案 |

### 4.2 推荐方案：协处理器 + MMIO

```rust
// 在 RISC-V Core 中调用 NPU

impl Core {
    /// 处理自定义指令（CUSTOM0）
    fn execute_custom0(&mut self, inst: Inst) -> Result<(), Exception> {
        let funct3 = inst.funct3();
        
        match funct3 {
            0b000 => self.npu_conv2d(inst),
            0b001 => self.npu_gemm(inst),
            0b010 => self.npu_pool(inst),
            _ => Err(Exception::IllegalInstruction),
        }
    }
    
    /// NPU 卷积指令
    fn npu_conv2d(&mut self, inst: Inst) -> Result<(), Exception> {
        let rd = inst.rd();
        let rs1 = inst.rs1();
        let rs2 = inst.rs2();
        
        // 构造 NPU 命令
        let cmd = NpuCommand::Conv2D {
            input_addr: self.reg_read(rs1),
            weight_addr: self.reg_read(rs2),
            // ... 其他参数从立即数获取
        };
        
        // 发送到 NPU
        self.npu.send_command(cmd)?;
        
        // 轮询等待完成
        while !self.npu.is_done() {
            // 让出 CPU（模拟真实行为）
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        
        // 写回结果地址
        self.reg_write(rd, self.npu.output_addr());
        
        Ok(())
    }
}
```

### 4.3 软件调用示例

```assembly
# RISC-V 汇编调用示例

# 加载输入张量地址到 a0
la a0, input_tensor

# 加载权重地址到 a1  
la a1, weight_tensor

# 加载输出地址到 a2
la a2, output_tensor

# 配置 NPU 参数
li a3, 3          # in_channels
li a4, 16         # out_channels
li a5, 32         # height
li a6, 32         # width
li a7, 3          # kernel
li t0, 1          # stride

# 构造参数结构体地址（通过寄存器传递）
la t1, conv_params

# 启动 NPU 卷积（使用 CUSTOM0 指令）
.custom0 0b000, t1, t1  # funct3=0 (Conv2D), rs1=t1, rs2=t1

# 检查 NPU 状态
1:
  lw t2, 0x1001_0004, t3  # 读取 NPU_STATUS
  andi t2, t2, 0x2        # 检查 DONE 位
  beqz t2, 1b             # 未完成则继续等待

# NPU 完成，继续执行
```

---

## 5. 仿真性能考量

### 5.1 模拟精度 vs 速度

| 级别 | 描述 | 速度 | 精度 | 适用阶段 |
|------|------|------|------|---------|
| **功能级** | TLM-1.0 (不计时) | 快 | 低 | 早期验证 |
| **周期级** | 周期近似 | 中 | 中 | 开发调试 |
| **时钟级** | 精确时钟 | 慢 | 高 | 性能调优 |

### 5.2 性能优化策略

```rust
/// 加速 NPU 运算的策略
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

impl Npu {
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
}
```

---

## 6. 开发路线图

### 6.1 阶段划分

| 阶段 | 目标 | 时间估算 | 交付物 |
|------|------|---------|--------|
| **Phase 1** | 基础 NPU 外设 | 2-3 周 | MMIO 接口、基本命令 |
| **Phase 2** | PE 阵列仿真 | 4-6 周 | 矩阵乘法、卷积运算 |
| **Phase 3** | 编译器/工具链集成 | 4-8 周 | TVM 后端支持 |
| **Phase 4** | 性能优化与验证 | 4-6 周 | 基准测试、优化 |

### 6.2 Phase 1 详细计划

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

### 6.3 风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| 系统总线不兼容 | 低 | 高 | 提前验证接口 |
| 性能不达预期 | 中 | 中 | 提供多种仿真模式 |
| 验证复杂度高 | 高 | 中 | 复用现有测试框架 |
| 需求变更 | 中 | 中 | 模块化设计 |

---

## 7. 验证策略

### 7.1 测试层次

```
验证金字塔:

           ┌─────────────┐
           │   系统级    │  ← 端到端测试（ELF 程序）
           │   (10%)     │
       ┌───┴─────────────┴───┐
       │     集成级           │  ← 模块间协作
       │     (30%)           │
   ┌───┴─────────────────────┴───┐
   │          单元级             │  ← 核心算法验证
   │          (60%)             │
   └─────────────────────────────┘
```

### 7.2 测试用例示例

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 PE MAC 功能
    #[test]
    fn test_pe_mac() {
        let mut pe = PeUnit::new();
        
        pe.load_input(3);
        pe.load_weight(4);
        pe.mac();
        
        assert_eq!(pe.read_accumulator(), 12); // 3 * 4 = 12
    }
    
    /// 测试 PE 阵列卷积
    #[test]
    fn test_pe_array_conv2d() {
        let mut array = PeArray::new(4, 4);
        
        // 简单 2x2 输入
        let input = [1, 2, 3, 4];
        // 2x2 权重
        let weights = [1, 0, 0, 1];
        // 1x1 输出
        let mut output = [0i32; 1];
        
        array.conv2d(
            &input,
            &weights,
            &mut output,
            1, 1, 2, 2, 2
        );
        
        // 输入与权重逐元素相乘后相加: 1*1 + 2*0 + 3*0 + 4*1 = 5
        assert_eq!(output[0], 5);
    }
    
    /// 测试 NPU 寄存器读写
    #[test]
    fn test_npu_registers() {
        let npu = Npu::new(0x1001_0000);
        
        // 验证基地址
        assert_eq!(npu.base_addr, 0x1001_0000);
        
        // 验证初始状态
        assert_eq!(npu.regs.status(), NpuStatus::Idle);
    }
}
```

### 7.3 端到端测试

```rust
/// NPU 端到端测试
#[test]
fn test_npu_end_to_end() {
    let sim = setup_simulator();
    let npu = sim.get_npu();
    
    // 1. 加载测试数据
    let input_data = load_test_vector("conv_test/input.bin");
    let weight_data = load_test_vector("conv_test/weights.bin");
    
    // 2. 设置 NPU 参数
    npu.set_conv_params(
        input_addr: 0x8000_0000,
        weight_addr: 0x9000_0000,
        output_addr: 0xA000_0000,
        in_channels: 3,
        out_channels: 16,
        height: 32,
        width: 32,
        kernel: 3,
    );
    
    // 3. 写入数据到内存
    sim.memory.write(0x8000_0000, &input_data);
    sim.memory.write(0x9000_0000, &weight_data);
    
    // 4. 启动 NPU
    npu.start();
    
    // 5. 等待完成
    while !npu.is_done() {
        sim.step(1);
    }
    
    // 6. 验证结果
    let output = sim.memory.read_slice(0xA000_0000, 16 * 28 * 28);
    let expected = load_test_vector("conv_test/expected.bin");
    
    assert_eq!(output, expected);
}
```

---

## 8. 参考资料

### 8.1 内部文档

- [dev-plan.md](../docs/dev-plan.md) - ruscv-sim 开发计划
- [architecture.md](../docs/architecture.md) - 系统架构文档
- [memory-arch.md](../docs/memory-arch.md) - 内存架构设计

### 8.2 外部资源

- [RISC-V External Debug Specification](https://riscv.org/wp-content/uploads/2019/03/riscv-debug-release.pdf)
- [TLM 2.0 Standard](https://www.accellera.org/standards/tlm)
- [TVM: Deep Learning Compiler](https://tvm.apache.org/)
- [ONNX Runtime](https://onnxruntime.ai/)

---

## 9. 附录

### 9.1 寄存器映射表

| Offset | 名称 | 访问 | 描述 |
|--------|------|------|------|
| 0x00 | CMD | RW | 命令寄存器 |
| 0x04 | STATUS | RO | 状态寄存器 |
| 0x08 | IRQ | RW | 中断控制 |
| 0x0C | CONFIG | RW | 配置参数 |
| 0x10 | INPUT_L | RW | 输入地址低 32 位 |
| 0x14 | INPUT_H | RW | 输入地址高 32 位 |
| 0x18 | WEIGHT_L | RW | 权重地址低 32 位 |
| 0x1C | WEIGHT_H | RW | 权重地址高 32 位 |
| 0x20 | OUTPUT_L | RW | 输出地址低 32 位 |
| 0x24 | OUTPUT_H | RW | 输出地址高 32 位 |
| ... | ... | ... | ... |

### 9.2 命令格式

```rust
/// NPU 命令格式
#[derive(Debug)]
#[repr(C)]
struct NpuCommand {
    /// 命令类型 (bits 0-3)
    cmd_type: u4,
    /// 保留 (bits 4-7)
    reserved: u4,
    /// 输入通道数 (bits 8-15)
    in_channels: u8,
    /// 输出通道数 (bits 16-23)
    out_channels: u8,
    /// 特征图高度 (bits 24-31)
    height: u8,
    /// 特征图宽度 (bits 0-7)
    width: u8,
    /// 卷积核大小 (bits 8-11)
    kernel: u4,
    /// 步长 (bits 12-15)
    stride: u4,
    /// 填充 (bits 16-19)
    padding: u4,
    /// 保留 (bits 20-31)
    _reserved: u12,
}
```

---

*文档结束*
