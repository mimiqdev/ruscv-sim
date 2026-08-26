# 已归档的内存子系统对比研究

> **状态：** 历史研究。参考项目分析可以重新验证后复用；旧 Sprint 与 ruscv-sim 设计结论不是当前决策。

**文档版本**: v1.0  
**Sprint**: Sprint 10  
**最后更新**: 2026-02-01

---

## 目录

1. [概述](#1-概述)
2. [Spike 实现分析](#2-spike-实现分析)
3. [OBI 总线协议分析](#3-obi-总线协议分析)
4. [Rust riscv crate 分析](#4-rust-riscv-crate-分析)
5. [对比总结与设计决策](#5-对比总结与设计决策)
6. [ruscv-sim 设计选择理由](#6-ruscv-sim-设计选择理由)

---

## 1. 概述

### 1.1 对比目标

本文档对比分析主流 RISC-V 模拟器和实现项目的内存子系统设计，为 ruscv-sim Sprint 10 的设计决策提供参考。

### 1.2 参考项目

| 项目 | 类型 | 语言 | 特点 |
|-----|------|-----|------|
| **Spike** | ISA Simulator | C++ | RISC-V 官方参考模拟器 |
| **OBI** | Bus Protocol | - | OpenHW Group 总线标准 |
| **riscv crate** | Rust Library | Rust | Rust 嵌入式 RISC-V 支持 |
| **CVA6** | RTL (开源) | SystemVerilog | 应用级 RISC-V 处理器 |

---

## 2. Spike 实现分析

### 2.1 架构概览

Spike 是 RISC-V 官方 ISA 模拟器，作为黄金参考模型广泛使用。

**核心文件**:
- `riscv/mmu.h` / `riscv/mmu.cc` - MMU 核心实现
- `riscv/encoding.h` - PTE 位定义
- `riscv/csrs.cc` - PMP 实现

**关键数据结构**:
```cpp
// 分离式 TLB: 3 个独立 TLB
dtlb_entry_t tlb_load[256];    // 加载 TLB
dtlb_entry_t tlb_store[256];   // 存储 TLB  
dtlb_entry_t tlb_insn[256];    // 指令 TLB

// PTE 缓存 (页表遍历优化)
pte_cache_entry_t pte_cache[251];

// 布隆过滤器 (TLB shootdown 优化)
bloom_filter_t<..., 256*16, 3> reverse_tags;
```

### 2.2 TLB 设计

#### 2.2.1 结构设计

| 参数 | Spike 设计 |
|-----|-----------|
| TLB 条目数 | 256 entries x 3 = 768 |
| 关联度 | 直接映射 (1-way) |
| 替换策略 | 无 (直接覆盖) |
| 分离设计 | ITLB, DTLB(读), DTLB(写) |
| PTE 缓存 | 251 entries |

#### 2.2.2 与 ruscv-sim 对比

| 参数 | Spike | ruscv-sim (设计) |
|-----|-------|-----------------|
| TLB 条目数 | 768 | 128 |
| 关联度 | 直接映射 | 4-way |
| 替换策略 | 直接覆盖 | LRU |
| 分离设计 | 3 TLBs | 2 TLBs |
| PTE 缓存 | 有 | 可选 |

**设计理由**:
- Spike 使用大 TLB (768 entries) 是因为其目标是大规模软件模拟
- ruscv-sim 使用更紧凑的设计 (128 entries)，对于 ISS 模拟器足够
- 4-way LRU 相比直接映射显著降低冲突率，软件实现容易

### 2.3 页表遍历实现

**Spike 支持的翻译模式**:

| 模式 | 层级 | VPN 位数 | 虚拟地址空间 |
|-----|------|---------|-------------|
| Sv32 | 2 | 10 | 4GB |
| Sv39 | 3 | 9 | 512GB |
| Sv48 | 4 | 9 | 256TB |
| Sv57 | 5 | 9 | 128PB |

**ruscv-sim 选择**:
- 仅支持 Sv39/Sv48 (RV64 专用)
- 不实现 Sv32 (32 位模式不需要)

### 2.4 性能优化技术

| 优化技术 | 描述 | ruscv-sim 采用 |
|---------|------|---------------|
| Host Address Mapping | 缓存主机虚拟地址 | 否 (简化设计) |
| PTE Cache | 251 entries 缓存 | 可选 |
| Bloom Filter | TLB shootdown 优化 | 否 |
| Separate Slow Path | 异常处理分离 | 是 |

---

## 3. OBI 总线协议分析

### 3.1 协议概述

OBI (Open Bus Interface) 是 OpenHW Group 制定的开源总线协议，主要用于 CORE-V 系列 RISC-V 处理器。

### 3.2 与 AXI/AHB 对比

| 特性 | OBI | AXI4 | AHB5 |
|-----|-----|------|------|
| 通道数 | 2 (A, R) | 5 | 1 |
| 乱序传输 | 否 | 是 | 否 |
| 突发传输 | 否 | 是 | 是 |
| 信号数量 | ~10 (最小) | 50+ | 中等 |
| 面积 | 最小 | 大 | 中等 |

### 3.3 ruscv-sim 设计选择

当前使用简化的内存接口，未来可能采用 OBI 风格:

- 当前: 简单的 trait-based 内存接口
- 未来: 考虑 OBI 风格的 req/gnt/rvalid 握手

---

## 4. Rust riscv crate 分析

### 4.1 架构概览

`riscv` crate 是 Rust 嵌入式工作组维护的 RISC-V 底层支持库。

### 4.2 CSR 访问设计

**SATP 寄存器实现**:
```rust
#[derive(Clone, Copy)]
pub struct Satp { bits: u64 }

pub enum Mode {
    Bare = 0, Sv39 = 8, Sv48 = 9, Sv57 = 10,
}

impl Satp {
    pub fn mode(&self) -> Mode {
        match (self.bits >> 60) & 0xF {
            0 => Mode::Bare, 8 => Mode::Sv39,
            9 => Mode::Sv48, _ => Mode::Bare,
        }
    }
    pub fn asid(&self) -> usize { ((self.bits >> 44) & 0xFFFF) as usize }
    pub fn ppn(&self) -> usize { (self.bits & ((1 << 44) - 1)) as usize }
}
```

**借鉴点**:
- 类型安全的 CSR 封装
- 位域访问方法
- 枚举类型用于模式选择

### 4.3 页表项定义

```rust
pub mod pte_flags {
    pub const V: u64 = 1 << 0;   // Valid
    pub const R: u64 = 1 << 1;   // Read
    pub const W: u64 = 1 << 2;   // Write
    pub const X: u64 = 1 << 3;   // Execute
    pub const U: u64 = 1 << 4;   // User
    pub const G: u64 = 1 << 5;   // Global
    pub const A: u64 = 1 << 6;   // Accessed
    pub const D: u64 = 1 << 7;   // Dirty
}
```

---

## 5. 对比总结与设计决策

### 5.1 TLB 设计对比表

| 项目 | Spike | CVA6 | riscv crate | ruscv-sim |
|-----|-------|------|-------------|-----------|
| 大小 | 256x3 | 8 | N/A | 64x2 |
| 关联度 | 直接映射 | 全相联 | N/A | 4-way |
| 替换策略 | 直接覆盖 | LRU | N/A | LRU |
| 分离设计 | 3 TLBs | 1 TLB | N/A | 2 TLBs |

### 5.2 页表遍历对比表

| 项目 | Spike | CVA6 | ruscv-sim |
|-----|-------|------|-----------|
| 实现方式 | 软件 | 硬件 | 软件 |
| 支持模式 | Sv32/39/48/57 | Sv39/48 | Sv39/Sv48 |
| 大页支持 | 是 | 是 | 是 |
| PTE 缓存 | 有 | 无 | 可选 |

### 5.3 内存保护对比表

| 项目 | Spike | CVA6 | ruscv-sim |
|-----|-------|------|-----------|
| PMP 条目 | 16/64 | 8 | 16 |
| 地址模式 | TOR/NA4/NAPOT | TOR/NA4/NAPOT | TOR/NA4/NAPOT |
| 页表权限 | 完整 | 完整 | 完整 |

---

## 6. ruscv-sim 设计选择理由

### 6.1 TLB 设计理由

**选择: 64 entries, 4-way set associative, LRU**

| 方案 | 优点 | 缺点 | 选择 |
|-----|------|------|------|
| Spike 768 entries | 高命中率 | 内存占用大 | 否 |
| CVA6 8 entries | 硬件友好 | 软件模拟冲突高 | 否 |
| ruscv-sim 128 entries | 平衡 | - | 是 |

**理由**:
1. 64 entries: 对于模拟器足够，内存占用合理
2. 4-way: 相比直接映射显著降低冲突率
3. LRU: 软件实现容易，符合程序局部性
4. 分离 ITLB/DTLB: 避免指令/数据访问竞争

### 6.2 页表遍历设计理由

**选择: 软件遍历，支持 Sv39/Sv48**

**理由**:
1. 软件遍历: ISS 模拟器天然适合
2. Sv39 优先: 64 位 Linux 主要使用 Sv39
3. Sv48 可选: 为未来大内存系统预留
4. 不实现 Sv32: RV64 专用模拟器

### 6.3 PMP 设计理由

**选择: 16 entries, 完整 TOR/NA4/NAPOT 支持**

**理由**:
1. 16 entries: 参考 Linux 内核默认配置
2. 完整地址模式: 兼容 RISC-V 规范
3. 软件检查: ISS 模拟器中性能足够

### 6.4 设计权衡总结

| 方面 | 选择 | 权衡 |
|-----|------|------|
| TLB 大小 | 128 entries | 平衡命中率与内存占用 |
| 关联度 | 4-way | 平衡冲突率与查找速度 |
| 页表模式 | Sv39/Sv48 | 覆盖主流场景，简化实现 |
| PTE 缓存 | 可选 | 复杂性与性能的平衡 |
| PMP 条目 | 16 | 足够保护关键区域 |

---

## 附录: 参考链接

- Spike: https://github.com/riscv-software-src/riscv-isa-sim
- riscv crate: https://github.com/rust-embedded/riscv
- OBI Spec: https://github.com/openhwgroup/programs
- RISC-V Spec: https://riscv.org/technical/specifications/

---

## 7. QEMU/Fast Models/riscv-vp 对比分析

### 7.1 QEMU 内存子系统

QEMU 使用 TCG (Tiny Code Generator) 进行动态翻译，其内存子系统设计：

**地址空间模型**：
- AddressSpace：代表整个地址空间
- MemoryRegion：表示内存区域 (RAM, ROM, MMIO)
- 支持分层嵌套的内存区域

**TLB 实现**：
- 软 TLB：在 CPU 状态中维护
- 不使用硬件 TLB（因为是二进制翻译）
- 通过 AddressSpace::read/write 进行地址转换

**页表管理**：
- 使用两阶段页表（软件模拟）
- 支持巨页 (2MB, 1GB)

### 7.2 Fast Models 内存架构

Fast Models (ARM) 使用 LISA+ 语言描述系统：

**TLM2.0 事务**：
- initiator/transport/target 模式
- 支持延迟和带宽建模

**内存类型**：
- SRAM, DRAM, ROM, MMIO
- Cache 建模支持

### 7.3 riscv-vp 分析

基于 SystemC TLM2.0 的 RISC-V 虚拟原型：

**特点**：
- 时序精确模式可选
- 可配置的 TLB
- 页表遍历由软件实现

### 7.4 综合对比

| 特性 | QEMU | Fast Models | riscv-vp | Spike | ruscv-sim |
|------|------|-------------|----------|-------|-----------|
| TLB 方式 | 软 TLB | TLM2.0 | 可配置 | 硬件 TLB | 硬件 TLB |
| 页表遍历 | 软件 | TLM | 软件 | 软件 | 软件 |
| 多核支持 | 完整 | 完整 | 基础 | 基础 | 预留 |
| 时序精度 | 可配置 | 高 | 可配置 | 低 | 低 |
| 扩展性 | 高 | 高 | 中 | 中 | 高 |

### 7.5 ruscv-sim 设计定位

- 采用 Spike 风格硬件 TLB（与 QEMU 不同）
- 使用软件页表遍历（与所有参考一致）
- 预留多核接口（参考 QEMU/Fast Models）
- 可配置的时序精度（参考 riscv-vp）
