# RISC-V ISS 技术调研报告

**文档版本**: v1.0  
**调研日期**: 2024年1月30日  
**调研范围**: riscv-isa-sim (Spike)、riscv-vp (TLM2.0)、RVA23 Profile

---

## 1. 执行摘要

本报告对 RISC-V ISS（指令集模拟器）技术进行了深入调研，分析了两个主流开源实现（Spike 和 riscv-vp）的架构设计，并结合 RVA23 Profile 规范，为 RISC-V 虚拟原型平台制定了完整的技术方案。

**核心发现**：
- Spike 采用解释型执行架构，高度模块化，支持 RV32I/RV64I 及 60+ 扩展
- riscv-vp 基于 SystemC TLM2.0，提供时序建模和多核支持
- RVA23 Profile 定义了嵌入式应用的必需指令集组合

---

## 2. Spike (riscv-isa-sim) 架构分析

### 2.1 项目概述

Spike 是 RISC-V 官方维护的功能性 ISA 模拟器，实现了一个或多个 RISC-V hart 的功能模型。

**支持的 ISA 特性**：
- RV32I/RV64I/RV32E/RV64E 基础指令集
- M/A/F/D/Q/C 扩展（完整支持）
- 向量扩展 V (v1.0)
- 加密扩展 Zk*/Zv*
- 特权模式：Machine/Supervisor/User (v1.11)
- Hypervisor 扩展 (v1.0)

### 2.2 核心模块划分

```
spike/
├── riscv/                    # 核心模拟器模块
│   ├── processor.h/cc       # 处理器核心实现
│   ├── sim.h/cc             # 系统级模拟器
│   ├── decode.h             # 指令解码逻辑
│   ├── csrs.h/cc            # CSR 寄存器框架
│   ├── mmu.h/cc             # 内存管理单元
│   ├── insns/               # 指令实现 (927+ 个文件)
│   │   ├── add.h
│   │   ├── mul.h
│   │   ├── ld.h
│   │   └── ...
│   ├── opcodes.h            # 指令 opcode 表
│   ├── trap.h/cc            # 异常/中断处理
│   └── extension.h          # 扩展机制
├── spike_main/              # 命令行入口
│   ├── spike.cc
│   └── xspike.cc
├── softfloat/               # 浮点运算库
└── fesvr/                   # 前端服务器接口
```

### 2.3 指令译码和执行流程

**执行流程**：

```
1. 取指 (Fetch)
   └── mmu_t::load_insn(addr) → 32-bit 指令字

2. 解码 (Decode)
   └── decode.h: insn_t::insn(bits)
   └── 根据 opcode/funct3/funct7 匹配指令
   └── 查表获取对应的执行函数

3. 执行 (Execute)
   └── processor_t::step() 循环
   └── 调用指令专用执行函数
   └── 修改寄存器/内存/CSR

4. 提交 (Commit)
   └── 更新 architectural state
   └── 记录 commit log（可选）
```

**关键数据结构**：

```cpp
// processor.h 中的指令描述符
struct insn_desc_t {
  insn_bits_t match;      // 指令匹配掩码
  insn_bits_t mask;       // 掩码
  insn_func_t fast_rv32i; // RV32I 快速执行函数
  insn_func_t fast_rv64i; // RV64I 快速执行函数
  // ...
};

// 指令函数类型
typedef reg_t (*insn_func_t)(processor_t*, insn_t, reg_t);
```

**指令函数签名**：
```cpp
// 示例：ADD 指令
reg_t add(processor_t* p, insn_t insn, reg_t pc) {
  reg_t rd = insn.rd();
  reg_t rs1 = insn.rs1();
  reg_t rs2 = insn.rs2();
  reg_t result = p->get_reg(rs1) + p->get_reg(rs2);
  p->set_reg(rd, result);
  return pc + 4;
}
```

### 2.4 CSR 处理机制

**CSR 架构**：

```cpp
// csr_t 基类（虚函数框架）
class csr_t {
  virtual reg_t read() const = 0;
  virtual void write(reg_t val) = 0;
  virtual void set(reg_t val) { write(read() | val); }
};

// 特化 CSR 实现示例
class mstatus_csr_t : public csr_t {
  reg_t read() const override { /* ... */ }
  void write(reg_t val) override { /* ... */ }
};

// CSR 注册表
struct state_t {
  std::unordered_map<reg_t, csr_t_p> csrmap;
  // 预定义 CSR
  mstatus_csr_t_p mstatus;
  mie_csr_t_p mie;
  mip_csr_t_p mip;
  // ...
};
```

**关键特性**：
- 使用 `std::unordered_map` 实现 CSR 地址到对象的映射
- 支持 WARL/WIRI 等字段属性
- 支持虚拟化 CSR（如 satp）
- 支持 CSR 索引扩展（sscsrind）

### 2.5 内存模型

**MMU 实现**：

```cpp
class mmu_t {
  // 三级页表查找
  reg_t translate(reg_t addr, access_type type);
  
  // TLB 缓存
  tlb_entry_t* tlb;
  
  // 物理内存访问
  char* addr_to_mem(reg_t paddr);
  
  // MMIO 设备访问
  bool mmio_load(reg_t paddr, size_t len, uint8_t* bytes);
  bool mmio_store(reg_t paddr, size_t len, const uint8_t* bytes);
};
```

**内存访问流程**：
```
虚拟地址 → 页表查找 → 物理地址 → (TLB 命中?) → 直接访问 / 页表遍历
                                                         ↓
                                            MMIO 设备 / 主存
```

### 2.6 扩展机制

**Extension 框架**：

```cpp
class extension_t {
public:
  virtual ~extension_t() = default;
  virtual const char* name() const = 0;
  virtual void reset(processor_t& proc) = 0;
  virtual void step(processor_t& proc) = 0;
  
  // 指令扩展
  virtual std::vector<insn_desc_t> get_instructions() = 0;
  
  // CSR 扩展
  virtual std::vector<csr_t_p> get_csrs(processor_t& proc) = 0;
};
```

---

## 3. riscv-vp TLM2.0 实现分析

### 3.1 项目概述

riscv-vp 是由德国不来梅大学开发的 RISC-V 虚拟原型，基于 SystemC TLM2.0 标准实现。

**核心特性**：
- RV32GC/RV64GC 支持（RV32IMAFDC/RV64IMAFDC）
- SystemC TLM-2.0 建模
- 时序建模（指令级 + 事务级延迟）
- 多核支持
- 虚拟外设（CLINT、PLIC、显示、Flash 等）
- GDB 调试接口
- FreeRTOS/RIOT/Zephyr/Linux 支持

### 3.2 TLM 接口设计

**核心接口定义**：

```cpp
// 指令内存接口
struct instr_memory_if {
  virtual uint32_t load_instr(uint64_t addr) = 0;
};

// 数据内存接口
struct data_memory_if {
  virtual void load(uint64_t addr, uint8_t* data, uint32_t len) = 0;
  virtual void store(uint64_t addr, const uint8_t* data, uint32_t len) = 0;
};

// 中断接口
class external_interrupt_target {
public:
  virtual void trigger_external_interrupt(PrivilegeLevel level) = 0;
  virtual void clear_external_interrupt(PrivilegeLevel level) = 0;
};
```

**TLM 事务传输**：

```cpp
// 使用 TLM 发起器 socket 进行内存访问
tlm_utils::simple_initiator_socket<ISS> initiator_socket;

// 发起读事务
tlm::tlm_generic_payload trans;
trans.set_address(addr);
trans.set_data_ptr(data);
trans.set_data_length(len);
trans.set_command(tlm::TLM_READ_COMMAND);

sc_time delay = quantum_keeper.get_local_time();
initiator_socket->b_transport(trans, delay);
quantum_keeper.inc(delay);
```

### 3.3 时序建模方法

**时序模型架构**：

```cpp
// 时序接口
struct timing_if {
  virtual void update_timing(Instruction instr, Opcode::Mapping op, ISS& iss) = 0;
};

// 简单时序模型
struct SimpleTimingDecorator : public timing_if {
  std::array<sc_core::sc_time, Opcode::NUMBER_OF_INSTRUCTIONS> instr_cycles;
  
  void update_timing(Instruction instr, Opcode::Mapping op, ISS& iss) override {
    iss.quantum_keeper.inc(instr_cycles[op]);
  }
};
```

**时序参数配置**：

```cpp
// 示例配置
const sc_core::sc_time cycle_time(10, sc_core::SC_NS);  // 100 MHz
const sc_core::sc_time memory_access_cycles = 4 * cycle_time;
const sc_core::sc_time mul_div_cycles = 8 * cycle_time;

// 指令周期分配
instr_cycles[Opcode::LB] = memory_access_cycles;
instr_cycles[Opcode::MUL] = mul_div_cycles;
```

### 3.4 与 SystemC 的集成方式

**ISS 与 SystemC 的集成**：

```cpp
// ISS 作为 SystemC 模块
struct ISS : public sc_core::sc_module {
  sc_core::sc_event wfi_event;
  tlm_utils::tlm_quantumkeeper quantum_keeper;
  
  SC_HAS_PROCESS(ISS);
  
  ISS(sc_module_name name, uint32_t hart_id) : 
    quantum_keeper(1000, sc_core::SC_MS) {  // 量子时间：1000ms
    SC_THREAD(exec_loop);
  }
  
  void exec_loop() {
    while (!shall_exit) {
      exec_step();
      quantum_keeper.inc(cycle_time);
      
      if (quantum_keeper.need_sync()) {
        quantum_keeper.sync();  // 同步到全局时间
      }
    }
  }
};
```

**总线互联**：

```cpp
// 通用可配置总线
class Bus : public sc_core::sc_module {
  tlm_utils::simple_target_socket<Bus> target;
  std::map<uint32_t, memory_region_t> regions;
  
  void transport(tlm::tlm_generic_payload& trans, sc_time& delay) {
    // 路由到对应外设
    for (auto& region : regions) {
      if (addr_in_range(trans.get_address(), region)) {
        region.device->b_transport(trans, delay);
        return;
      }
    }
  }
};
```

### 3.5 外设模型

**CLINT（核心本地中断器）**：

```cpp
struct clint_if {
  virtual void increment(uint32_t hart_id) = 0;
  virtual uint64_t get_time() = 0;
};

class Clint : public sc_core::sc_module, public clint_if {
  std::array<uint64_t, N_HARTS> mtimecmp;
  uint64_t mtime;
  
  void update_mtime();
};
```

**PLIC（平台级中断控制器）**：

```cpp
class Plic : public sc_core::sc_module {
  std::vector<external_interrupt_target*> targets;
  uint32_t pending;
  
  void handle_interrupt(uint32_t source);
};
```

---

## 4. RVA23 Profile 分析

### 4.1 Profile 概述

RVA23 Profile 定义了 RISC-V 嵌入式应用的标准配置，类似于 ARM 的 Cortex-A55 profile。

**基础要求**：
- **基础 ISA**: RV64I（可选 RV32I）
- **宽度**: 64 位（LP64 数据模型）

### 4.2 必需指令扩展

| 扩展 | 名称 | 优先级 | 依赖关系 |
|------|------|--------|----------|
| **I** | 基础整数指令 | 必需 | 无 |
| **M** | 整数乘除 | 必需 | 无 |
| **A** | 原子操作 | 必需 | 无 |
| **F** | 单精度浮点 | 必需 | 无 |
| **D** | 双精度浮点 | 必需 | F |
| **C** | 压缩指令 | 必需 | 无 |
| **Zicsr** | CSR 读写 | 必需 | 无 |
| **Zifencei** | 指令获取屏障 | 必需 | 无 |

### 4.3 扩展依赖关系图

```
RV64I (基础)
    ├── M (独立)
    ├── A (独立)
    ├── F
    │   └── D (依赖 F)
    └── C (独立)
    
Zicsr ──┬── 独立
Zifencei ┘
```

### 4.4 可选扩展建议

| 扩展 | 功能 | 建议 |
|------|------|------|
| **Zba** | 地址生成 | 推荐 |
| **Zbb** | 基本位操作 | 推荐 |
| **Zbc** | 进位/借位 | 可选 |
| **Zbs** | 位设置 | 可选 |
| **Zicbom** | 缓存块操作 | 可选 |
| **Sv39/Sv48** | 虚拟内存 | 操作系统支持时必需 |

---

## 5. 核心设计模式总结

### 5.1 Spike 设计模式

| 模式 | 应用 | 优点 |
|------|------|------|
| **解释执行** | 逐条指令解释 | 简单、可调试、灵活 |
| **表驱动解码** | opcode 查表 | 高效、易扩展 |
| **CSR 框架** | 虚函数 + 注册表 | 灵活、可扩展 |
| **插件扩展** | extension_t 接口 | 支持自定义扩展 |
| **分层 MMU** | TLB + 页表遍历 | 性能与准确性的平衡 |

### 5.2 riscv-vp 设计模式

| 模式 | 应用 | 优点 |
|------|------|------|
| **TLM2.0 建模** | 事务级建模 | 可组合、可重用 |
| **时序解耦** | 量子时间同步 | 多核协同仿真 |
| **接口抽象** | 虚拟接口类 | 模块化、松耦合 |
| **装饰器模式** | timing_if | 时序模型可替换 |

---

## 6. 技术选型建议

### 6.1 架构选型决策

| 维度 | Spike 方案 | riscv-vp 方案 | 纯 Rust 自研 |
|------|------------|---------------|--------------|
| **控制权** | 依赖外部项目 | 依赖外部项目 | 完全自主 |
| **扩展性** | 受限 C++ API | 中等 | **优秀** (Rust trait) |
| **维护性** | 依赖上游更新 | 依赖上游更新 | **完全可控** |
| **学习成本** | 需要理解 Spike | 需要理解 C++/TLM | **最小** (自设计) |
| **性能** | 已优化 | 中等 | **可接近 C++** |
| **时序集成** | 需要适配层 | 原生 TLM | **原生 Rust TLM** |

### 6.2 推荐架构

**纯 Rust 自研架构**：

```
┌─────────────────────────────────────────────────────────────────┐
│                     RISC-V 虚拟原型平台 (Rust)                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                   ISS Core (纯 Rust 实现)                  │  │
│  │                                                              │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │  │
│  │  │   Decoder   │→│  Executor   │→│   Register File     │ │  │
│  │  │  (表驱动)    │  │  (指令实现)  │  │   (x0-x31)         │ │  │
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘ │  │
│  │                                                              │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │  │
│  │  │  CSR Bank   │  │    MMU      │  │   Instruction Cache │ │  │
│  │  │ (虚函数框架) │  │  (Sv39/48)  │  │   (可选)           │ │  │
│  │  └─────────────┘  └─────────────┘  └─────────────────────┘ │  │
│  └───────────────────────────────────────────────────────────┘  │
│                            │                                    │
│  ┌─────────────────────────┴─────────────────────────────────┐  │
│  │                   时序模型层 (可配置)                        │  │
│  │                                                              │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │  │
│  │  │  ZeroTiming     │  │  SimpleTiming   │  │  Accurate   │ │  │
│  │  │  (功能验证)      │  │  (性能分析)      │  │  (微架构)   │ │  │
│  │  │  - 1 cycle/insn │  │  - 指令周期表    │  │  - 流水线   │ │  │
│  │  │                 │  │  - 可配置延迟    │  │  - 缓存模拟  │ │  │
│  │  └─────────────────┘  └─────────────────┘  └─────────────┘ │  │
│  └────────────────────────────────────────────────────────────┘  │
│                            │                                    │
│  ┌─────────────────────────┴─────────────────────────────────┐  │
│  │                   Rust TLM2.0 抽象层                        │  │
│  │                                                              │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │  │
│  │  │  TlmInitiator   │  │   TlmTarget     │  │  TlmBus     │ │  │
│  │  │  (发起器 socket) │  │  (目标 socket)   │  │  (互联)     │ │  │
│  │  └─────────────────┘  └─────────────────┘  └─────────────┘ │  │
│  │                                                              │  │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────┐ │  │
│  │  │  GenericPayload │  │   TimeQuantum   │  │  Phase      │ │  │
│  │  │  (事务载荷)      │  │  (时间量子)      │  │  (时序阶段)  │ │  │
│  │  └─────────────────┘  └─────────────────┘  └─────────────┘ │  │
│  └────────────────────────────────────────────────────────────┘  │
│                            │                                    │
│  ┌─────────────────────────┴─────────────────────────────────┐  │
│  │                   外设模型层 (可选 SystemC 桥接)             │  │
│  │                                                              │  │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────┐            │  │
│  │  │   CLINT    │  │    PLIC    │  │   UART     │            │  │
│  │  │  (纯 Rust) │  │  (纯 Rust) │  │  (纯 Rust) │            │  │
│  │  └────────────┘  └────────────┘  └────────────┘            │  │
│  │                                                              │  │
│  │  ┌─────────────────────────────────────────────────────┐   │  │
│  │  │          SystemC TLM2.0 外设 (可选桥接)              │   │  │
│  │  │  rust-bindings → SystemC module → TLM transaction    │   │  │
│  │  └─────────────────────────────────────────────────────┘   │  │
│  └────────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 6.3 纯 Rust 自研理由

| 优势 | 说明 |
|------|------|
| **完全自主** | 不依赖外部 C++ 项目，架构设计自由 |
| **类型安全** | Rust 所有权系统消除内存错误 |
| **模块化** | trait 对象模式支持灵活组合 |
| **高性能** | zero-cost abstraction，接近 C++ |
| **跨平台** | 天然支持 Linux/macOS/Windows |
| **可测试性** | 内置测试框架，单元测试覆盖容易 |
| **长期维护** | 无上游依赖断裂风险 |

### 6.4 Rust TLM2.0 抽象设计

```rust
// Rust 实现的 TLM2.0 核心 trait

/// TLM 事务阶段
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TlmPhase {
    BeginReq,
    EndReq,
    BeginResp,
    EndResp,
}

/// TLM 响应状态
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TlmResponseStatus {
    Ok,
    AddressError,
    CommandError,
    // ...
}

/// TLM 通用事务载荷
pub struct TlmGenericPayload {
    command: TlmCommand,
    address: u64,
    data: Vec<u8>,
    byte_enable: Option<Vec<u8>>,
    response_status: TlmResponseStatus,
    streaming: bool,
}

/// TLM 时间类型
#[derive(Debug, Clone, Copy)]
pub enum TlmTime {
    Ps(u64),
    Ns(u64),
    Us(u64),
    Ms(u64),
    S(u64),
}

/// TLM 发起器接口 (对应 TLM initiator socket)
pub trait TlmInitiator {
    fn b_transport(&self, trans: &mut TlmGenericPayload, delay: &mut TlmTime);
    fn nb_transport_fw(&self, trans: &mut TlmGenericPayload, phase: TlmPhase, delay: &mut TlmTime) -> TlmSyncEnum;
}

/// TLM 目标接口 (对应 TLM target socket)
pub trait TlmTarget {
    fn b_transport(&self, trans: &mut TlmGenericPayload, delay: &mut TlmTime);
    fn nb_transport_bw(&self, trans: &mut TlmGenericPayload, phase: TlmPhase, delay: &mut TlmTime) -> TlmSyncEnum;
    fn transport_dbg(&self, trans: &TlmGenericPayload) -> usize;
}

/// 同步枚举
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TlmSyncEnum {
    Accept,
    Wait,
    Release,
}
```

### 6.5 技术栈建议

| 层级 | 技术选择 | 理由 |
|------|----------|------|
| **核心语言** | Rust 2024 | 内存安全、高性能、现代语言特性 |
| **构建工具** | Cargo | 官方包管理器，依赖管理完善 |
| **测试框架** | Rust built-in test + proptest | 单元测试 + 模糊测试 |
| **序列化** | serde | 配置文件和状态导出 |
| **文档** | mdBook | 静态文档生成 |
| **CI/CD** | GitHub Actions | 集成度高 |
| **性能分析** | perf/criterion.rs | Rust 性能基准 |

### 6.6 ISS 核心设计模式

参考 Spike 但用 Rust 重实现：

| Spike 模式 | Rust 等价实现 |
|------------|---------------|
| 表驱动解码 | `HashMap<Opcode, Vec<InstructionPattern>>` |
| CSR 虚函数 | `trait Csr: Read + Write + Debug` |
| 插件扩展 | `trait Extension: Send + Sync` |
| TLB 缓存 | `LruCache<VAddr, PAddr>` |
| MMU 页表遍历 | 递归/迭代页表查找 |

---

## 7. 风险评估与缓解

| 风险 | 可能性 | 影响 | 风险等级 | 缓解措施 |
|------|--------|------|----------|----------|
| 指令实现遗漏 | 中 | 高 | 🔴 高 | TDD 开发 + RISC-V tests 自动化验证 |
| 性能不达标 | 中 | 中 | 🟡 中 | 预留 Sprint 8 优化时间 |
| Rust TLM 集成 | 低 | 中 | 🟡 中 | 参考 riscv-vp 设计，避免重复造轮子 |
| RVA23 规范变更 | 低 | 低 | 🟢 低 | 模块化指令实现，易于更新 |
| 多核同步复杂 | 中 | 高 | 🔴 高 | Sprint 5 重点攻关，完整时序验证 |

### 7.1 缓解策略

1. **TDD 开发模式**：每条指令先写测试，再实现
2. **RISC-V testsuite**：使用官方测试套件自动化验证
3. **渐进式实现**：先完成 RV64I，再逐步添加扩展
4. **性能基准**：建立性能基准线，定期检测

---

## 8. 结论

基于本次调研，**推荐采用纯 Rust 自主实现**的架构方案：

### 核心决策

**不使用 Spike 作为核心**，而是基于以下理由自主研发：

1. **完全自主可控** - 无外部 C++ 依赖，架构设计自由
2. **Rust 优势** - 内存安全、高性能、现代语言特性
3. **更好的扩展性** - trait 对象模式支持灵活组合
4. **长期维护** - 无上游项目断裂风险

### 推荐架构

```
┌────────────────────────────────────────────────────────────┐
│              RISC-V 虚拟原型平台 (纯 Rust)                  │
│                                                            │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐ │
│  │  ISS Core    │  │  Timing      │  │  Rust TLM2.0     │ │
│  │  (自研)       │  │  Model       │  │  Interface       │ │
│  └──────────────┘  └──────────────┘  └──────────────────┘ │
│                                                            │
│  目标：RVA23 Profile (RV64IMAFDC)                          │
└────────────────────────────────────────────────────────────┘
```

### 技术要点

| 方面 | 方案 |
|------|------|
| **指令实现** | Rust 重构 Spike 设计（表驱动解码、CSR 框架） |
| **TLM2.0** | Rust trait 模拟 TLM2.0 socket 概念 |
| **时序模型** | 可配置装饰器（Zero/Simple/Accurate） |
| **开发方式** | TDD + RISC-V tests 自动化验证 |
| **迭代计划** | 8 个 Sprint（每 2 周） |

### 最终目标

构建一个**高性能、完全自主可控**的 RISC-V 虚拟原型平台，支持：

- ✅ RVA23 Profile (RV64IMAFDC + Zicsr + Zifencei)
- ✅ 可配置的时序建模
- ✅ Rust TLM2.0 风格外设接口
- ✅ 多核支持
- ✅ GDB 调试
- ✅ Python/C++ SDK

---

## 参考资料

### RISC-V 规范
1. RISC-V 官方仓库: https://github.com/riscv-software-src/riscv-isa-sim
2. RISC-V 指令集手册: https://github.com/riscv/riscv-isa-manual
3. RVA23 Profile: https://github.com/riscv/riscv-profiles

### 参考实现
4. riscv-vp (TLM2.0 参考): https://github.com/agra-uni-bremen/riscv-vp
5. rust-rv32ima (纯 Rust 参考): https://github.com/cerivitos/rust-rv32ima

### Rust 相关
6. Rust 官方文档: https://doc.rust-lang.org/
7. Rust 异步运行时 Tokio: https://tokio.rs/
8. TLM2.0 标准: https://www.accellera.org/standards/tlm

### 测试资源
9. RISC-V testsuite: https://github.com/riscv/riscv-tests
10. RISC-V compliance tests: https://github.com/riscv/riscv-compliance
