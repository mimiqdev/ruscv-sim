# RISC-V 虚拟原型平台 - 技术架构设计

**文档版本**: v2.0 (更新)  
**基于调研**: 纯 Rust 自研 + Rust TLM2.0 抽象  
**目标 Profile**: RVA23 (RV64IMAFDC)  
**核心原则**: 完全自主 Rust 实现，不依赖 C++ 项目

---

## 1. 整体架构图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         RISC-V 虚拟原型平台架构                               │
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
│  │  │  │  Fetch   │→│  Decode  │→│ Execute  │→│  Commit  │      │  │    │
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

---

## 2. 核心模块设计

### 2.1 目录结构

```
riscv-vp/
├── include/                     # 公共头文件
│   ├── core/                    # 核心模拟器接口
│   │   ├── isimulator.h
│   │   ├── processor.h
│   │   └── timing_model.h
│   ├── peripherals/             # 外设接口
│   │   ├── bus.h
│   │   ├── clint.h
│   │   └── plic.h
│   └── utils/                   # 工具类
│       ├── elf_loader.h
│       └── register.h
│
├── src/
│   ├── core/                    # 核心实现
│   │   ├── processor.cc
│   │   ├── decoder.cc
│   │   ├── executor.cc
│   │   ├── csr_bank.cc
│   │   ├── timing_none.cc       # 零时序模型
│   │   ├── timing_simple.cc     # 简单时序模型
│   │   └── timing_accurate.cc   # 精确时序模型
│   │
│   ├── peripherals/             # 外设实现
│   │   ├── bus.cc
│   │   ├── clint.cc
│   │   ├── plic.cc
│   │   └── uart.cc
│   │
│   └── platform/                # 平台配置
│       ├── hifive1.cc
│       └── generic.cc
│
├── tests/                       # 单元测试
│   ├── unit/
│   ├── integration/
│   └── system/
│
├── examples/                    # 示例程序
│   ├── hello_world/
│   └── bare_metal/
│
├── docs/                        # 文档
│   ├── architecture.md
│   ├── api_reference.md
│   └── user_guide.md
│
└── Cargo.toml                   # 构建配置
```

### 2.2 核心类设计

#### 2.2.1 ISimulator 接口

```rust
// src/core/isimulator.rs\nuse std::sync::atomic::{AtomicU64, Ordering};\n\n/// ISS 模拟器 trait\npub trait ISimulator: Send + Sync {\n    // 生命周期\n    fn reset(&mut self);\n    fn run(&mut self);\n    fn step(&mut self, count: u64);\n    fn stop(&mut self);\n    \n    // 状态查询\n    fn is_halted(&self) -> bool;\n    fn cycle_count(&self) -> u64;\n    fn instruction_count(&self) -> u64;\n    \n    // 寄存器访问\n    fn reg(&self, idx: u8) -> u64;\n    fn set_reg(&mut self, idx: u8, value: u64);\n    fn pc(&self) -> u64;\n    fn set_pc(&mut self, value: u64);\n    \n    // 内存访问\n    fn read_mem(&self, addr: u64, size: usize) -> Vec<u8>;\n    fn write_mem(&mut self, addr: u64, data: &[u8]);\n    \n    // 断点\n    fn set_breakpoint(&mut self, addr: u64);\n    fn remove_breakpoint(&mut self, addr: u64);\n    \n    // 回调\n    fn set_instruction_callback<F>(&mut self, callback: F)\n    where F: Fn(u64, u32) + Send + 'static;\n}\n```

#### 2.2.2 Processor 核心类

```cpp
// include/core/processor.h
#pragma once

#include "csr_bank.h"
#include "register_file.h"
#include "mmu.h"
#include "timing_model.h"
#include "instruction.h"

#include <array>
#include <vector>
#include <memory>

namespace rv64 {

class Processor {
public:
    Processor(uint32_t hart_id, ITimingModel* timing_model = nullptr);
    ~Processor();
    
    // 初始化
    void initialize(IMMU* mmu, IBus* bus);
    void reset();
    
    // 执行控制
    void step();
    void run_until(uint64_t pc, uint64_t max_cycles = UINT64_MAX);
    
    // 特权模式
    enum class PrivilegeMode : uint8_t {
        User = 0,
        Supervisor = 1,
        Machine = 3
    };
    
    // 寄存器文件
    RegisterFile& regs() { return regs_; }
    const RegisterFile& regs() const { return regs_; }
    
    // CSR 访问
    CSRBank& csrs() { return csrs_; }
    const CSRBank& csrs() const { return csrs_; }
    
    // PC
    uint64_t pc() const { return pc_; }
    void set_pc(uint64_t value) { pc_ = value; }
    
    // 状态
    bool is_halted() const { return halted_; }
    uint64_t cycle_count() const { return cycle_count_; }
    
    // MMU
    IMMU* mmu() { return mmu_.get(); }
    
    // 中断处理
    void raise_interrupt(PrivilegeMode mode, uint32_t interrupt_id);
    void clear_interrupt(PrivilegeMode mode, uint32_t interrupt_id);
    
private:
    void fetch();
    void decode();
    void execute();
    void commit();
    void handle_trap(Trap& trap);
    
    uint32_t hart_id_;
    uint64_t pc_ = 0;
    bool halted_ = false;
    uint64_t cycle_count_ = 0;
    uint64_t instruction_count_ = 0;
    
    RegisterFile regs_;
    CSRBank csrs_;
    std::unique_ptr<IMMU> mmu_;
    IBus* bus_ = nullptr;
    
    Instruction current_instr_;
    DecodedInstruction decoded_;
    
    ITimingModel* timing_model_ = nullptr;
    std::unique_ptr<TimingRecord> timing_record_;
};

} // namespace rv64
```

#### 2.2.3 时序模型接口

```cpp
// include/core/timing_model.h
#pragma once

#include <cstdint>
#include <chrono>

class ITimingModel {
public:
    virtual ~ITimingModel() = default;
    
    // 周期消耗
    virtual uint64_t get_cycles_for(uint32_t opcode) const = 0;
    
    // 内存访问延迟
    virtual uint64_t get_memory_latency(bool is_load, uint64_t size) const = 0;
    
    // 流水线阶段延迟
    virtual uint64_t get_fetch_latency() const = 0;
    virtual uint64_t get_decode_latency() const = 0;
    virtual uint64_t get_execute_latency() const = 0;
    virtual uint64_t get_memory_latency() const = 0;
    virtual uint64_t get_writeback_latency() const = 0;
};

// 零时序模型（功能验证用）
class ZeroTimingModel : public ITimingModel {
public:
    uint64_t get_cycles_for(uint32_t opcode) const override { return 1; }
    uint64_t get_memory_latency(bool, uint64_t) const override { return 1; }
    uint64_t get_fetch_latency() const override { return 1; }
    // ...
};

// 简单时序模型（性能分析用）
class SimpleTimingModel : public ITimingModel {
public:
    SimpleTimingModel(uint64_t core_frequency_hz);
    
    uint64_t get_cycles_for(uint32_t opcode) const override;
    uint64_t get_memory_latency(bool is_load, uint64_t size) const override;
    
    void set_memory_latency_cycles(uint64_t cycles) { memory_latency_ = cycles; }
    void set_mul_div_latency_cycles(uint64_t cycles) { mul_div_latency_ = cycles; }
    
private:
    uint64_t core_frequency_hz_;
    uint64_t memory_latency_ = 4;
    uint64_t mul_div_latency_ = 8;
};
```

---

## 3. TLM 接口规范

### 3.1 外设接口定义

```cpp
// include/peripherals/bus_interface.h
#pragma once

#include <tlm>
#include <systemc>

class IBusSlave : public sc_core::sc_module {
public:
    // TLM-2.0 标准接口
    tlm_utils::simple_target_socket<IBusSlave> socket;
    
    IBusSlave(sc_module_name name) : socket("socket") {
        socket.register_b_transport(this, &IBusSlave::b_transport);
    }
    
    virtual void b_transport(
        tlm::tlm_generic_payload& trans,
        sc_core::sc_time& delay) = 0;
    
    // 地址范围查询
    virtual uint64_t get_address() const = 0;
    virtual uint64_t get_size() const = 0;
};

// 总线主设备接口
class IBusMaster {
public:
    virtual ~IBusMaster() = default;
    
    virtual void read(
        uint64_t addr, void* data, size_t len,
        sc_core::sc_time& delay) = 0;
        
    virtual void write(
        uint64_t addr, const void* data, size_t len,
        sc_core::sc_time& delay) = 0;
};
```

### 3.2 中断接口

```cpp
// include/peripherals/interrupt_if.h
#pragma once

enum class InterruptType : uint8_t {
    External = 0,
    Software = 1,
    Timer = 2
};

class IInterruptReceiver {
public:
    virtual ~IInterruptReceiver() = default;
    virtual void interrupt(InterruptType type, uint32_t id) = 0;
    virtual void clear_interrupt(InterruptType type, uint32_t id) = 0;
    virtual bool has_interrupt() const = 0;
    virtual uint32_t get_interrupt_id() const = 0;
};

class IInterruptSender {
public:
    virtual ~IInterruptSender() = default;
    virtual void register_receiver(
        uint32_t irq_id,
        IInterruptReceiver* receiver) = 0;
};
```

### 3.3 内存接口

```cpp
// include/peripherals/memory_if.h
#pragma once

class IMemoryDevice {
public:
    virtual ~IMemoryDevice() = default;
    
    virtual uint64_t base_address() const = 0;
    virtual uint64_t size() const = 0;
    virtual bool contains(uint64_t addr) const = 0;
    
    virtual void read(uint64_t addr, void* data, size_t len) = 0;
    virtual void write(uint64_t addr, const void* data, size_t len) = 0;
    
    // 访问权限
    virtual bool readable() const { return true; }
    virtual bool writable() const { return true; }
    virtual bool executable() const { return false; }
};
```

---

## 4. 扩展点设计

### 4.1 指令扩展

```cpp
// include/core/extension.h
#pragma once

#include "instruction.h"
#include "processor.h"

class IExtension {
public:
    virtual ~IExtension() = default;
    
    virtual const char* name() const = 0;
    virtual uint32_t id() const = 0;
    
    // 初始化
    virtual void on_install(Processor* proc) = 0;
    virtual void on_uninstall() = 0;
    virtual void reset() = 0;
    
    // 指令处理
    virtual bool decode(DecodedInstruction& instr) = 0;
    virtual ExecutionResult execute(
        Processor* proc, 
        const DecodedInstruction& instr) = 0;
    
    // CSR 扩展
    virtual std::vector<CSR*> get_csrs() { return {}; }
    
    // 调试支持
    virtual void enable_debug(bool enable) = 0;
};

// 扩展注册表
class ExtensionRegistry {
public:
    static ExtensionRegistry& instance();
    
    void register_extension(std::unique_ptr<IExtension> ext);
    IExtension* get_extension(const char* name);
    std::vector<IExtension*> get_all_extensions() const;
    
private:
    std::map<std::string, std::unique_ptr<IExtension>> extensions_;
};
```

### 4.2 外设扩展

```cpp
// include/peripherals/peripheral.h
#pragma once

#include "bus_interface.h"
#include "interrupt_if.h"

class IPeripheral : public IBusSlave {
public:
    using IBusSlave::IBusSlave;
    
    virtual const char* name() const = 0;
    virtual uint32_t version() const { return 1; }
    
    // 生命周期
    virtual void init() = 0;
    virtual void reset() = 0;
    
    // 配置
    virtual void configure(const std::string& key, const void* value) = 0;
    
    // 状态
    virtual std::map<std::string, std::string> get_status() const = 0;
};

// 外设工厂
class PeripheralFactory {
public:
    using CreateFunc = std::function<std::unique_ptr<IPeripheral>()>;
    
    static PeripheralFactory& instance();
    
    void register_creator(const char* name, CreateFunc creator);
    std::unique_ptr<IPeripheral> create(const char* name);
    
    std::vector<const char*> get_available_peripherals() const;
    
private:
    std::map<std::string, CreateFunc> creators_;
};
```

### 4.3 时序模型扩展

```cpp
// include/core/timing_architecture.h
#pragma once

#include "timing_model.h"

// 微架构时序参数
struct MicroArchitectureTiming : public ITimingModel {
    // 流水线参数
    uint64_t fetch_width = 1;
    uint64_t decode_width = 1;
    uint64_t issue_width = 1;
    uint64_t commit_width = 1;
    
    // 功能单元延迟（周期数）
    uint64_t alu_latency = 1;
    uint64_t mul_latency = 4;
    uint64_t div_latency = 32;
    uint64_t load_latency = 4;
    uint64_t store_latency = 2;
    uint64_t branch_latency = 1;
    uint64_t fp_alu_latency = 4;
    uint64_t fp_mul_latency = 6;
    uint64_t fp_div_latency = 20;
    
    // 缓存参数
    struct CacheParams {
        uint64_t size_bytes;
        uint64_t line_size;
        uint64_t latency;
        uint64_t hit_rate;
    };
    CacheParams icache{32 * 1024, 64, 1, 0.95};
    CacheParams dcache{32 * 1024, 64, 1, 0.90};
    CacheParams l2{256 * 1024, 64, 10, 0.95};
    
    // ITimingModel 实现
    uint64_t get_cycles_for(uint32_t opcode) const override;
    uint64_t get_memory_latency(bool is_load, uint64_t size) const override;
    // ...
};
```

---

## 5. 平台配置

### 5.1 平台描述文件

```yaml
# config/platform/hifive1.yml
platform:
  name: "SiFive HiFive1"
  description: "RISC-V Freedom E310 based development board"
  
hart:
  count: 1
  isa: "RV64IMAFDC"
  privilege_modes: ["M", "S", "U"]
  
memory:
  - name: "flash"
    base: 0x20000000
    size: 0x4000000
    type: "rom"
  - name: "dtim"
    base: 0x80000000
    size: 0x4000
    type: "ram"
  - name: "itim"
    base: 0x8000000
    size: 0x4000
    type: "ram"
    
peripherals:
  - name: "clint"
    type: "clint"
    base: 0x02000000
    size: 0x10000
  - name: "plic"
    type: "plic"
    base: 0x0C000000
    size: 0x400000
  - name: "uart0"
    type: "uart"
    base: 0x10013000
    size: 0x1000
    params:
      baudrate: 115200
      
timing:
  model: "simple"
  frequency_hz: 32000000
```

### 5.2 平台初始化代码

```cpp
// src/platform/hifive1.cc
#include "platform.h"
#include "peripherals/clint.h"
#include "peripherals/plic.h"
#include "peripherals/uart.h"

namespace platform {

std::unique_ptr<Platform> create_hi_five_1() {
    auto platform = std::make_unique<Platform>();
    
    // 配置内存
    platform->add_memory_region(
        "flash",
        0x20000000,
        0x4000000,
        MemoryType::ROM
    );
    platform->add_memory_region(
        "dtim",
        0x80000000,
        0x4000,
        MemoryType::RAM
    );
    
    // 添加外设
    auto clint = std::make_unique<CLINT>(0x02000000);
    platform->add_peripheral(std::move(clint));
    
    auto plic = std::make_unique<PLIC>(0x0C000000);
    platform->add_peripheral(std::move(plic));
    
    auto uart = std::make_unique<UART>(0x10013000);
    uart->set_baudrate(115200);
    platform->add_peripheral(std::move(uart));
    
    return platform;
}

} // namespace platform
```

---

## 6. API 设计

### 6.1 Rust API 示例

```rust
// 使用示例
use ruscv_sim::{Simulator, Platform};

fn main() -> Result<()> {
    // 创建模拟器
    let mut simulator = Simulator::new();
    
    // 加载平台配置
    simulator.load_platform("hifive1");
    
    // 加载固件
    simulator.load_elf("firmware.elf")?;
    
    // 设置断点
    simulator.set_breakpoint(0x80000000);
    
    // 运行
    simulator.run();
    
    // 检查结果
    let reg_a0 = simulator.regs().a0();
    println!("Return value: {}", reg_a0);
    
    Ok(())
}
```

### 6.2 Python 绑定

```python
# Python API 示例
import riscv_vp

# 创建模拟器
sim = riscv_vp.Simulator("hifive1")

# 加载程序
sim.load_elf("firmware.elf")

# 运行直到断点
sim.run_to_breakpoint()

# 检查寄存器
print(f"a0 = {sim.regs.a0}")

# 步进执行
for i in range(10):
    sim.step()
    print(f"pc = 0x{sim.pc:08x}")

# 内存访问
data = sim.memory.read(0x80000000, 4)
print(f"memory[0x80000000] = 0x{data:08x}")
```

---

## 7. 构建配置

### 7.1 Cargo.toml 概要

```toml
[package]
name = \"ruscv-sim\"
version = \"0.1.0\"
edition = \"2024\"
description = \"RISC-V ISS with Rust TLM2.0 interface\"

[dependencies]
# 核心依赖
anyhow = \"1.0\"
thiserror = \"2.0\"
log = \"0.4\"
env_logger = \"0.11\"

# 序列化
serde = { version = \"1.0\", features = [\"derive\"] }
serde_yaml = \"0.9\"

# 并发
tokio = { version = \"1.0\", features = [\"full\"] }

# 测试
[dev-dependencies]
proptest = \"1.0\"
criterion = \"0.5\"

[profile.release]
lto = true
opt-level = 3
```

---

## 8. 版本兼容性

| 组件 | 版本要求 |
|------|----------|
| Rust | 2024 Edition (1.80+) |
| Cargo | 1.80+ |
| RISC-V ISA | RVA23 Profile |
