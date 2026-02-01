# RISC-V 内存子系统架构设计文档

**文档版本**: v1.0  
**Sprint**: Sprint 10  
**目标**: 实现完整的 MMU、TLB、页表遍历 (Sv39/Sv48)  
**最后更新**: 2026-02-01

---

## 目录

1. [架构概览](#1-架构概览)
2. [组件设计](#2-组件设计)
3. [数据结构设计](#3-数据结构设计)
4. [地址转换流程](#4-地址转换流程)
5. [页表格式](#5-页表格式)
6. [MMIO 设计](#6-mmio-设计)
7. [内存保护](#7-内存保护)
8. [接口定义](#8-接口定义)
9. [性能优化](#9-性能优化)
10. [测试策略](#10-测试策略)

---

## 1. 架构概览

### 1.1 系统架构图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           RISC-V Core (RV64)                                │
│                                                                             │
│   ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                     │
│   │   取指单元   │    │  加载/存储   │    │   调试单元   │                     │
│   │   (IFU)     │    │   (LSU)     │    │    (DM)     │                     │
│   └──────┬──────┘    └──────┬──────┘    └──────┬──────┘                     │
│          │                  │                  │                           │
│          └──────────────────┼──────────────────┘                           │
│                             ▼                                              │
│   ┌──────────────────────────────────────────────────────────────┐        │
│   │                     MMU (Memory Management Unit)              │        │
│   │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │        │
│   │  │  ITLB       │    │  DTLB       │    │  PTW        │       │        │
│   │  │ (64 entries)│    │ (64 entries)│    │ (页表遍历器) │       │        │
│   │  │ 4-way LRU   │    │ 4-way LRU   │    │             │       │        │
│   │  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘       │        │
│   │         │                  │                  │              │        │
│   │         └──────────────────┼──────────────────┘              │        │
│   │                            ▼                                 │        │
│   │  ┌─────────────────────────────────────────────────────────┐ │        │
│   │  │              Address Translation Engine                  │ │        │
│   │  │  - Virtual → Physical address translation               │ │        │
│   │  │  - Permission checking (R/W/X/U)                        │ │        │
│   │  │  - PMP checking                                         │ │        │
│   │  └─────────────────────────────────────────────────────────┘ │        │
│   └──────────────────────────────────────────────────────────────┘        │
│                                    │                                       │
│                                    ▼                                       │
│   ┌──────────────────────────────────────────────────────────────┐        │
│   │              Physical Memory & MMIO Subsystem                 │        │
│   │  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │        │
│   │  │   RAM       │    │   MMIO      │    │   PMP       │       │        │
│   │  │  (Main)     │    │  (Device)   │    │ (Protection)│       │        │
│   │  │ 0x8000_0000 │    │ 0x0000_0000 │    │             │       │        │
│   │  └─────────────┘    └─────────────┘    └─────────────┘       │        │
│   └──────────────────────────────────────────────────────────────┘        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 地址空间布局 (Sv39)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Sv39 Virtual Address Space                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  0x0000_0000_0000_0000 ─┐                                                   │
│                         │  User Memory (256GB)                              │
│  0x0000_003F_FFFF_FFFF ─┘                                                   │
│                                                                             │
│  0x0000_0040_0000_0000 ─┐                                                   │
│                         │  Unmapped (Hole)                                  │
│  0x003F_FFBF_FFFF_FFFF ─┘                                                   │
│                                                                             │
│  0x003F_FFC0_0000_0000 ─┐                                                   │
│                         │  Supervisor Memory (256GB)                        │
│  0x003F_FFFF_FFFF_FFFF ─┘                                                   │
│                                                                             │
│  0x0040_0000_0000_0000 ─┐                                                   │
│                         │  Unmapped (Hole)                                  │
│  0xFFFF_FBFF_FFFF_FFFF ─┘                                                   │
│                                                                             │
│  0xFFFF_FC00_0000_0000 ─┐                                                   │
│                         │  Machine Memory (256GB)                           │
│  0xFFFF_FFFF_FFFF_FFFF ─┘                                                   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 1.3 模块依赖关系

```
                    ┌─────────────────┐
                    │   Core State    │
                    │  (satp, mstatus)│
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│     ITLB      │    │     DTLB      │    │      PTW      │
│  (指令 TLB)    │    │  (数据 TLB)    │    │  (页表遍历器)  │
└───────┬───────┘    └───────┬───────┘    └───────┬───────┘
        │                    │                    │
        └────────────────────┼────────────────────┘
                             ▼
                    ┌─────────────────┐
                    │  Address Trans  │
                    │  (地址转换引擎)  │
                    └────────┬────────┘
                             │
        ┌────────────────────┼────────────────────┐
        │                    │                    │
        ▼                    ▼                    ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  Physical Mem │    │     MMIO      │    │  PMP Checker  │
│  (物理内存)    │    │  (内存映射IO)  │    │ (PMP检查器)   │
└───────────────┘    └───────────────┘    └───────────────┘
```

---

## 2. 组件设计

### 2.1 Physical Memory (物理内存)

**设计目标**:
- 支持 64 位物理地址空间
- 支持多种内存区域 (RAM, ROM, MMIO)
- 支持内存属性 (Cacheable, Bufferable, etc.)

**文件位置**: `src/mmu/physical.rs`

```rust
/// 物理内存区域类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegionType {
    /// 主内存 (DRAM)
    Ram,
    /// 只读内存 (ROM/Boot ROM)
    Rom,
    /// 内存映射 I/O
    Mmio,
    /// 保留/未映射
    Reserved,
}

/// 物理内存区域描述
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// 起始物理地址
    pub start: u64,
    /// 大小 (字节)
    pub size: usize,
    /// 区域类型
    pub region_type: MemoryRegionType,
    /// 内存属性
    pub attributes: MemoryAttributes,
}

/// 内存属性
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryAttributes {
    /// 可缓存
    pub cacheable: bool,
    /// 可缓冲
    pub bufferable: bool,
    /// 可共享
    pub shareable: bool,
    /// 设备内存 (Device-nGnRnE, Device-nGnRE, etc.)
    pub device: bool,
}
```

### 2.2 TLB (Translation Lookaside Buffer)

**设计决策**:
- **大小**: 64 entries (参考 Spike 的 256，但针对模拟器优化)
- **关联度**: 4-way set associative
- **替换策略**: LRU (Least Recently Used)
- **分离式设计**: ITLB (指令) + DTLB (数据)

**文件位置**: `src/mmu/tlb.rs`

**关键设计理由**:
1. **4-way associative**: 平衡查找速度和冲突率
2. **LRU 替换**: 相比随机替换，更符合程序访问模式
3. **分离式设计**: 避免指令/数据访问竞争，提高命中率

```rust
/// TLB 配置常量
pub const TLB_SIZE: usize = 64;
pub const TLB_WAYS: usize = 4;
pub const TLB_SETS: usize = TLB_SIZE / TLB_WAYS; // 16 sets

/// TLB 条目
#[derive(Debug, Clone, Copy)]
pub struct TlbEntry {
    /// 虚拟页号 (VPN)
    pub vpn: u64,
    /// 物理页号 (PPN)
    pub ppn: u64,
    /// 地址空间标识符
    pub asid: u16,
    /// 全局映射标志
    pub global: bool,
    /// 权限标志
    pub permissions: PagePermissions,
    /// 访问标志 (用于 LRU)
    pub accessed: bool,
    /// 脏标志
    pub dirty: bool,
    /// LRU 计数器
    pub lru_count: u8,
}

/// TLB 结构
pub struct Tlb {
    /// TLB 条目数组 [set][way]
    entries: [[Option<TlbEntry>; TLB_WAYS]; TLB_SETS],
    /// 访问计数器 (用于 LRU)
    access_counter: u64,
}
```

### 2.3 页表遍历器 (Page Table Walker)

**设计决策**:
- **软件遍历**: 适用于 ISS 模拟器，灵活性高
- **支持 Sv39/Sv48**: 可配置的页表层级
- **PTE 缓存**: 缓存最近访问的页表项

**文件位置**: `src/mmu/ptw.rs`

```rust
/// 页表遍历配置
#[derive(Debug, Clone)]
pub struct PageTableConfig {
    /// 地址转换模式 (Sv39/Sv48)
    pub mode: TranslationMode,
    /// 页表基址物理页号
    pub root_ppn: u64,
    /// 地址空间标识符
    pub asid: u16,
}

/// 页表层级
#[derive(Debug, Clone, Copy)]
pub enum TranslationMode {
    /// 无地址转换 (裸机模式)
    Bare = 0,
    /// Sv39: 39 位虚拟地址
    Sv39 = 8,
    /// Sv48: 48 位虚拟地址
    Sv48 = 9,
}

/// 页表遍历结果
#[derive(Debug, Clone)]
pub enum WalkResult {
    /// 成功找到页表项
    Success(PageTableEntry),
    /// 页表项无效
    InvalidEntry { level: usize },
    /// 权限错误
    PermissionFault { level: usize },
    /// 访问标志未设置
    AccessFault { level: usize },
}

/// 页表遍历器
pub struct PageTableWalker {
    /// 物理内存接口
    memory: Arc<dyn PhysicalMemoryInterface>,
    /// PTE 缓存
    pte_cache: PteCache,
}
```

### 2.4 地址转换引擎

**文件位置**: `src/mmu/translation.rs`

```rust
/// 地址转换请求
#[derive(Debug, Clone)]
pub struct TranslationRequest {
    /// 虚拟地址
    pub vaddr: u64,
    /// 访问类型
    pub access_type: AccessType,
    /// 当前特权模式
    pub privilege: PrivilegeMode,
    /// satp 寄存器值
    pub satp: u64,
    /// mstatus 寄存器值
    pub mstatus: u64,
}

/// 地址转换结果
#[derive(Debug, Clone)]
pub struct TranslationResult {
    /// 物理地址
    pub paddr: u64,
    /// 页表项 (用于更新 A/D 位)
    pub pte: Option<PageTableEntry>,
    /// PTE 地址 (用于更新)
    pub pte_addr: Option<u64>,
}

/// 地址转换引擎
pub struct AddressTranslator {
    /// ITLB
    itlb: Tlb,
    /// DTLB
    dtlb: Tlb,
    /// 页表遍历器
    ptw: PageTableWalker,
}
```

---

## 3. 数据结构设计

### 3.1 页表项 (Page Table Entry)

**Sv39 页表项格式**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Sv39 Page Table Entry (64-bit)                    │
├─────────────────────────────────────────────────────────────────────────────┤
│ 63 │ 62-54 │ 53-28 │ 27-19 │ 18-10 │ 9 │ 8 │ 7 │ 6 │ 5 │ 4 │ 3 │ 2 │ 1 │ 0 │
├────┼───────┼───────┼───────┼───────┼───┼───┼───┼───┼───┼───┼───┼───┼───┼───┤
│ N  │  RSW  │  PPN2 │  PPN1 │  PPN0 │RSW│ D │ A │ G │ U │ X │ W │ R │ V │
│    │[8:6]  │[53:28]│[27:19]│[18:10]│[9]│   │   │   │   │   │   │   │   │
└────┴───────┴───────┴───────┴───────┴───┴───┴───┴───┴───┴───┴───┴───┴───┴───┘

位定义:
- V (0): Valid - 页表项有效
- R (1): Read - 可读
- W (2): Write - 可写
- X (3): Execute - 可执行
- U (4): User - 用户态可访问
- G (5): Global - 全局映射
- A (6): Accessed - 已访问
- D (7): Dirty - 已修改
- RSW (8-9): Reserved for Software - 软件保留
- PPN0 (10-18): Physical Page Number 0
- PPN1 (19-27): Physical Page Number 1
- PPN2 (28-53): Physical Page Number 2
- RSW (54-62): Reserved for Software (extended)
- N (63): Svnapot extension (optional)
```

**实现代码**:

```rust
/// 页表项 (64-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageTableEntry(u64);

impl PageTableEntry {
    // 标志位常量
    pub const V: u64 = 1 << 0;   // Valid
    pub const R: u64 = 1 << 1;   // Read
    pub const W: u64 = 1 << 2;   // Write
    pub const X: u64 = 1 << 3;   // Execute
    pub const U: u64 = 1 << 4;   // User
    pub const G: u64 = 1 << 5;   // Global
    pub const A: u64 = 1 << 6;   // Accessed
    pub const D: u64 = 1 << 7;   // Dirty
    pub const RSW_MASK: u64 = 0b11 << 8;  // Reserved for Software
    
    // 位域位置
    const PPN0_SHIFT: u32 = 10;
    const PPN1_SHIFT: u32 = 19;
    const PPN2_SHIFT: u32 = 28;
    const PPN_MASK: u64 = 0x1FF;  // 9 bits
    
    /// 从原始值创建 PTE
    pub const fn from_raw(bits: u64) -> Self {
        Self(bits)
    }
    
    /// 获取原始值
    pub const fn bits(&self) -> u64 {
        self.0
    }
    
    /// 检查有效位
    pub const fn is_valid(&self) -> bool {
        self.0 & Self::V != 0
    }
    
    /// 检查是否是叶节点 (R|W|X 任一位置位)
    pub const fn is_leaf(&self) -> bool {
        self.0 & (Self::R | Self::W | Self::X) != 0
    }
    
    /// 检查是否是下一级页表指针
    pub const fn is_pointer(&self) -> bool {
        self.is_valid() && !self.is_leaf()
    }
    
    /// 获取 PPN0
    pub fn ppn0(&self) -> u64 {
        (self.0 >> Self::PPN0_SHIFT) & Self::PPN_MASK
    }
    
    /// 获取 PPN1
    pub fn ppn1(&self) -> u64 {
        (self.0 >> Self::PPN1_SHIFT) & Self::PPN_MASK
    }
    
    /// 获取 PPN2
    pub fn ppn2(&self) -> u64 {
        (self.0 >> Self::PPN2_SHIFT) & Self::PPN_MASK
    }
    
    /// 获取完整物理页号 (Sv39)
    pub fn ppn(&self) -> u64 {
        self.0 >> 10
    }
    
    /// 获取物理地址 (页对齐)
    pub fn physical_address(&self) -> u64 {
        self.ppn() << 12
    }
    
    /// 获取权限
    pub fn permissions(&self) -> PagePermissions {
        PagePermissions {
            read: self.0 & Self::R != 0,
            write: self.0 & Self::W != 0,
            execute: self.0 & Self::X != 0,
            user: self.0 & Self::U != 0,
        }
    }
    
    /// 设置访问位
    pub fn set_accessed(&mut self) {
        self.0 |= Self::A;
    }
    
    /// 设置脏位
    pub fn set_dirty(&mut self) {
        self.0 |= Self::D;
    }
}
```

### 3.2 虚拟地址结构

```rust
/// Sv39 虚拟地址结构
#[derive(Debug, Clone, Copy)]
pub struct VirtualAddress(u64);

impl VirtualAddress {
    // Sv39 常量
    pub const VPN_WIDTH: u32 = 9;
    pub const PAGE_OFFSET_WIDTH: u32 = 12;
    pub const LEVELS: usize = 3;
    pub const VA_BITS: u32 = 39;
    
    /// 创建虚拟地址
    pub fn new(addr: u64) -> Result<Self, TranslationError> {
        // 检查高位扩展 (sign extension)
        let sign_bits = addr >> Self::VA_BITS;
        if sign_bits != 0 && sign_bits != 0x1F_FFFF {
            return Err(TranslationError::InvalidVirtualAddress(addr));
        }
        Ok(Self(addr))
    }
    
    /// 获取页内偏移
    pub fn page_offset(&self) -> u64 {
        self.0 & ((1 << Self::PAGE_OFFSET_WIDTH) - 1)
    }
    
    /// 获取 VPN[i] (i = 0, 1, 2)
    pub fn vpn(&self, level: usize) -> u64 {
        assert!(level < Self::LEVELS);
        let shift = Self::PAGE_OFFSET_WIDTH + (level as u32) * Self::VPN_WIDTH;
        (self.0 >> shift) & ((1 << Self::VPN_WIDTH) - 1)
    }
    
    /// 获取所有 VPN 作为数组 [VPN2, VPN1, VPN0]
    pub fn vpns(&self) -> [u64; 3] {
        [self.vpn(2), self.vpn(1), self.vpn(0)]
    }
    
    /// 获取页对齐的虚拟地址
    pub fn page_aligned(&self) -> u64 {
        self.0 & !((1 << Self::PAGE_OFFSET_WIDTH) - 1)
    }
}
```

### 3.3 SATP 寄存器结构

```rust
/// SATP (Supervisor Address Translation and Protection) Register
/// 
/// RV64 格式:
/// ┌─────────┬────────────────┬────────────────────────────────┐
/// │  Mode   │     ASID       │            PPN                 │
/// │ [60:63] │   [44:59]      │          [0:43]                │
/// └─────────┴────────────────┴────────────────────────────────┘
#[derive(Debug, Clone, Copy)]
pub struct Satp(u64);

impl Satp {
    /// 获取地址转换模式
    pub fn mode(&self) -> TranslationMode {
        match (self.0 >> 60) & 0xF {
            0 => TranslationMode::Bare,
            8 => TranslationMode::Sv39,
            9 => TranslationMode::Sv48,
            _ => TranslationMode::Bare, // 保留值视为 Bare
        }
    }
    
    /// 获取 ASID
    pub fn asid(&self) -> u16 {
        ((self.0 >> 44) & 0xFFFF) as u16
    }
    
    /// 获取根页表 PPN
    pub fn ppn(&self) -> u64 {
        self.0 & ((1 << 44) - 1)
    }
    
    /// 获取根页表物理地址
    pub fn root_page_table_addr(&self) -> u64 {
        self.ppn() << 12
    }
    
    /// 检查是否启用分页
    pub fn paging_enabled(&self) -> bool {
        self.mode() != TranslationMode::Bare
    }
}
```

---

## 4. 地址转换流程

### 4.1 完整转换流程图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Virtual Address Translation Flow                     │
└─────────────────────────────────────────────────────────────────────────────┘

                        ┌─────────────┐
    Virtual Address ───►│  VA Check   │────┬──────────────────────────────────►
                        │ (Sign Ext)  │    │ (Invalid VA)
                        └──────┬──────┘    │
                               │ Valid     │ Page Fault
                               ▼           │
                        ┌─────────────┐    │
                        │ Check SATP  │────┼──────────────────────────────────►
                        │ Mode == Bare│    │ (Bare Mode)
                        └──────┬──────┘    │ Pass-through
                               │ Paging    │
                               ▼           │
                    ┌─────────────────────┐│
                    │  Check MPRV/MXR/SUM ││
                    │  (mstatus fields)   ││
                    └──────────┬──────────┘│
                               │           │
              ┌────────────────┴────────┐  │
              │                         │  │
              ▼                         ▼  │
    ┌─────────────────┐       ┌─────────────────┐
    │   TLB Lookup    │       │   TLB Lookup    │
    │   (ITLB/DTLB)   │       │   (ITLB/DTLB)   │
    └────────┬────────┘       └────────┬────────┘
             │ Hit                     │ Miss
             ▼                         ▼
    ┌─────────────────┐       ┌─────────────────┐
    │  Check ASID/    │       │  Page Table     │
    │  Global Match   │       │  Walk (PTW)     │
    └────────┬────────┘       └────────┬────────┘
             │ Match                   │
             ▼                         ▼
    ┌─────────────────┐       ┌─────────────────┐
    │  Permission     │       │  Update TLB     │
    │  Check (R/W/X)  │       │  (if success)   │
    └────────┬────────┘       └────────┬────────┘
             │ Pass                    │
             ▼                         ▼
    ┌─────────────────┐       ┌─────────────────┐
    │  PMP Check      │       │  Check Result   │
    │  (Physical Mem) │       │  (Success/      │
    └────────┬────────┘       │   Fault)        │
             │ Pass          └────────┬────────┘
             ▼                        │
    ┌─────────────────┐               │
    │  Update A/D     │               │
    │  Bits (if need) │               │
    └────────┬────────┘               │
             │                        │
             ▼                        ▼
    ┌─────────────────┐       ┌─────────────────┐
    │ Physical Address│       │  Page Fault     │
    │    Output       │       │  (Exception)    │
    └─────────────────┘       └─────────────────┘
```

### 4.2 页表遍历算法

```rust
/// 执行页表遍历 (Sv39)
pub fn walk(&self, vaddr: VirtualAddress, satp: Satp) -> Result<WalkResult, TranslationError> {
    let vpn = vaddr.vpns();
    let mut ppn = satp.ppn();
    
    // 从最高级 (level 2) 开始遍历
    for level in (0..Self::LEVELS).rev() {
        // 计算页表项地址
        let pte_addr = (ppn << 12) + (vpn[level] * 8);
        
        // 读取页表项
        let pte_val = self.memory.read_dword(pte_addr)?;
        let pte = PageTableEntry::from_raw(pte_val);
        
        // 检查有效性
        if !pte.is_valid() {
            return Ok(WalkResult::InvalidEntry { level });
        }
        
        // 检查是否是叶节点
        if pte.is_leaf() {
            // 检查权限一致性
            if pte.0 & Self::W != 0 && pte.0 & Self::R == 0 {
                // W=1, R=0 是保留组合
                return Ok(WalkResult::InvalidEntry { level });
            }
            
            // 构建物理地址
            let paddr = self.build_physical_address(&pte, &vaddr, level)?;
            return Ok(WalkResult::Success(pte));
        }
        
        // 非叶节点，继续遍历
        ppn = pte.ppn();
    }
    
    // 遍历完所有层级仍未找到叶节点
    Ok(WalkResult::InvalidEntry { level: 0 })
}

/// 构建物理地址
fn build_physical_address(
    &self,
    pte: &PageTableEntry,
    vaddr: &VirtualAddress,
    level: usize,
) -> Result<u64, TranslationError> {
    let ppn = pte.ppn();
    let offset = vaddr.page_offset();
    
    match level {
        0 => {
            // 4KB 页: 使用全部 PPN
            Ok((ppn << 12) | offset)
        }
        1 => {
            // 2MB mega-page: VPN[0] 是页内偏移的一部分
            let ppn_mask = (1 << 9) - 1;  // VPN[0] 的掩码
            let vpn0 = vaddr.vpn(0);
            Ok(((ppn << 12) | (vpn0 << 12)) | offset)
        }
        2 => {
            // 1GB giga-page: VPN[1:0] 是页内偏移的一部分
            let vpn0 = vaddr.vpn(0);
            let vpn1 = vaddr.vpn(1);
            Ok(((ppn << 12) | (vpn1 << 21) | (vpn0 << 12)) | offset)
        }
        _ => Err(TranslationError::InvalidLevel(level)),
    }
}
```

---

## 5. 页表格式

### 5.1 Sv39 页表结构

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Sv39 Page Table Layout                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Level 2 (Root)          Level 1              Level 0                      │
│   ┌─────────────┐         ┌─────────────┐      ┌─────────────┐             │
│   │  PTE[511]   │         │  PTE[511]   │      │  PTE[511]   │             │
│   │  ...        │────────►│  ...        │─────►│  ...        │             │
│   │  PTE[VPN2]  │         │  PTE[VPN1]  │      │  PTE[VPN0]  │──┐          │
│   │  ...        │         │  ...        │      │  ...        │  │          │
│   │  PTE[0]     │         │  PTE[0]     │      │  PTE[0]     │  │          │
│   └─────────────┘         └─────────────┘      └─────────────┘  │          │
│          │                       │                    │         │          │
│          │  (PPN << 12)         │  (PPN << 12)        │         │          │
│          ▼                       ▼                    ▼         │          │
│   ┌─────────────┐         ┌─────────────┐      ┌─────────────┐  │          │
│   │  4KB Pages  │         │  4KB Pages  │      │  4KB Page   │  │          │
│   │  (or 1GB    │         │  (or 2MB    │      │  (Physical  │◄─┘          │
│   │   mega-pg)  │         │   mega-pg)  │      │   Memory)   │             │
│   └─────────────┘         └─────────────┘      └─────────────┘             │
│                                                                             │
│   Page Table Entry = 8 bytes                                                │
│   Entries per table = 512 (4KB / 8 bytes)                                   │
│   VPN width = 9 bits (indexes 0-511)                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 大页支持

| 页大小 | 层级 | VPN 位 | 偏移位 | 说明 |
|--------|------|--------|--------|------|
| 4KB    | 0    | VPN[0] | 12     | 标准页 |
| 2MB    | 1    | VPN[1] | 21     | Mega-page |
| 1GB    | 2    | VPN[2] | 30     | Giga-page |

### 5.3 Sv48 扩展

Sv48 在 Sv39 基础上增加第 4 级页表，支持 48 位虚拟地址:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Sv48 Page Table Layout                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   VPN3 (9 bits) -> VPN2 (9 bits) -> VPN1 (9 bits) -> VPN0 (9 bits)         │
│                                                                             │
│   Level 3 (Root)     Level 2        Level 1        Level 0                 │
│        │                │              │              │                    │
│        └────────────────┴──────────────┴──────────────┘                    │
│                        Sv39 兼容结构                                       │
│                                                                             │
│   新增 512GB 大页支持 (Level 3 叶节点)                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 6. MMIO 设计

### 6.1 MMIO 区域映射

```rust
/// 默认 RISC-V 内存映射
pub const DEFAULT_MEMORY_MAP: &[MemoryRegion] = &[
    // Boot ROM
    MemoryRegion {
        start: 0x0000_1000,
        size: 0x0000_F000,  // 60KB
        region_type: MemoryRegionType::Rom,
        attributes: MemoryAttributes {
            cacheable: false,
            bufferable: false,
            ..Default::default()
        },
    },
    // Main RAM
    MemoryRegion {
        start: 0x8000_0000,
        size: 0x8000_0000,  // 2GB
        region_type: MemoryRegionType::Ram,
        attributes: MemoryAttributes {
            cacheable: true,
            bufferable: true,
            ..Default::default()
        },
    },
    // UART0
    MemoryRegion {
        start: 0x1000_0000,
        size: 0x0000_0100,
        region_type: MemoryRegionType::Mmio,
        attributes: MemoryAttributes {
            device: true,
            ..Default::default()
        },
    },
    // CLINT
    MemoryRegion {
        start: 0x0200_0000,
        size: 0x0001_0000,
        region_type: MemoryRegionType::Mmio,
        attributes: MemoryAttributes {
            device: true,
            ..Default::default()
        },
    },
    // PLIC
    MemoryRegion {
        start: 0x0C00_0000,
        size: 0x0400_0000,
        region_type: MemoryRegionType::Mmio,
        attributes: MemoryAttributes {
            device: true,
            ..Default::default()
        },
    },
];
```

### 6.2 MMIO 访问处理

```rust
/// MMIO 设备接口
trait MmioDevice: Send + Sync {
    /// 读取 MMIO 寄存器
    fn read(&self, offset: u64, size: AccessSize) -> Result<u64, MemoryError>;
    
    /// 写入 MMIO 寄存器
    fn write(&mut self, offset: u64, value: u64, size: AccessSize) -> Result<(), MemoryError>;
    
    /// 获取设备大小
    fn size(&self) -> u64;
}

/// MMIO 管理器
pub struct MmioManager {
    /// 设备映射表
    devices: HashMap<u64, Box<dyn MmioDevice>>,
}

impl MmioManager {
    /// 注册 MMIO 设备
    pub fn register_device(
        &mut self,
        base_addr: u64,
        device: Box<dyn MmioDevice>,
    ) -> Result<(), MemoryError> {
        // 检查地址冲突
        for (addr, dev) in &self.devices {
            let end = *addr + dev.size();
            if base_addr < end && base_addr + device.size() > *addr {
                return Err(MemoryError::AddressConflict);
            }
        }
        
        self.devices.insert(base_addr, device);
        Ok(())
    }
    
    /// 访问 MMIO
    pub fn access(
        &self,
        addr: u64,
        access_type: MmioAccessType,
    ) -> Result<u64, MemoryError> {
        // 找到对应的设备
        for (base, device) in &self.devices {
            let end = *base + device.size();
            if addr >= *base && addr < end {
                let offset = addr - *base;
                return match access_type {
                    MmioAccessType::Read(size) => device.read(offset, size),
                    MmioAccessType::Write(value, size) => {
                        device.write(offset, value, size)?;
                        Ok(0)
                    }
                };
            }
        }
        
        Err(MemoryError::InvalidAddress(addr))
    }
}
```

---

## 7. 内存保护

### 7.1 PMP (Physical Memory Protection)

```rust
/// PMP 配置寄存器 (8-bit per entry)
/// 
/// ┌─────┬─────┬─────┬─────┬────────┬─────┬─────┬─────┐
/// │  L  │     │     │     │ A[1:0] │  X  │  W  │  R  │
/// │ [7] │ [6] │ [5] │ [4] │ [3:2]  │ [1] │ [0] │
/// └─────┴─────┴─────┴─────┴────────┴─────┴─────┴─────┘
#[derive(Debug, Clone, Copy)]
pub struct PmpConfig(u8);

impl PmpConfig {
    pub const L_BIT: u8 = 1 << 7;
    pub const A_MASK: u8 = 0b11 << 3;
    pub const X_BIT: u8 = 1 << 2;
    pub const W_BIT: u8 = 1 << 1;
    pub const R_BIT: u8 = 1 << 0;
    
    /// 获取地址匹配模式
    pub fn addr_mode(&self) -> PmpAddrMode {
        match (self.0 & Self::A_MASK) >> 3 {
            0 => PmpAddrMode::Off,
            1 => PmpAddrMode::Tor,
            2 => PmpAddrMode::Na4,
            3 => PmpAddrMode::Napot,
            _ => unreachable!(),
        }
    }
    
    /// 获取权限
    pub fn permissions(&self) -> PmpPermissions {
        PmpPermissions {
            read: self.0 & Self::R_BIT != 0,
            write: self.0 & Self::W_BIT != 0,
            execute: self.0 & Self::X_BIT != 0,
        }
    }
    
    /// 检查是否锁定
    pub fn locked(&self) -> bool {
        self.0 & Self::L_BIT != 0
    }
}

/// PMP 地址匹配模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmpAddrMode {
    Off = 0,    // 禁用
    Tor = 1,    // Top of Range
    Na4 = 2,    // 4-byte 自然对齐
    Napot = 3,  // 2^n 自然对齐
}

/// PMP 检查器
pub struct PmpChecker {
    /// PMP 配置 (最多 64 个条目)
    configs: [Option<PmpConfig>; 64],
    /// PMP 地址
    addresses: [u64; 64],
    /// 是否启用 PMP (M-mode 无锁定条目时可为 false)
    enabled: bool,
}

impl PmpChecker {
    /// 检查内存访问权限
    pub fn check_access(
        &self,
        addr: u64,
        size: usize,
        access_type: AccessType,
        priv_mode: PrivilegeMode,
    ) -> bool {
        // M-mode 在无 PMP 锁定条目时总是允许
        if priv_mode == PrivilegeMode::Machine && !self.has_locked_entries() {
            return true;
        }
        
        // 遍历 PMP 条目 (0 = 最高优先级)
        for i in 0..64 {
            if let Some(config) = self.configs[i] {
                if self.matches(addr, size, i, config.addr_mode()) {
                    return config.permissions().allows(access_type);
                }
            }
        }
        
        // 无匹配条目时，S/U mode 拒绝，M-mode 允许
        priv_mode == PrivilegeMode::Machine
    }
    
    /// 检查地址是否匹配 PMP 条目
    fn matches(&self, addr: u64, size: usize, index: usize, mode: PmpAddrMode) -> bool {
        match mode {
            PmpAddrMode::Off => false,
            PmpAddrMode::Tor => {
                // 需要前一个条目的地址作为起始
                if index == 0 {
                    addr < (self.addresses[0] << 2)
                } else {
                    let start = self.addresses[index - 1] << 2;
                    let end = self.addresses[index] << 2;
                    addr >= start && addr + size as u64 <= end
                }
            }
            PmpAddrMode::Na4 => {
                let pmp_addr = self.addresses[index] << 2;
                addr == pmp_addr && size <= 4
            }
            PmpAddrMode::Napot => {
                // NAPOT 编码: 地址低位为 1 表示范围大小
                let pmp_addr = self.addresses[index];
                self.napot_match(addr, size, pmp_addr)
            }
        }
    }
}
```

### 7.2 页表权限检查

```rust
/// 检查页表权限
fn check_page_permissions(
    pte: &PageTableEntry,
    access_type: AccessType,
    priv_mode: PrivilegeMode,
    mstatus: u64,
) -> Result<(), PageFault> {
    let perms = pte.permissions();
    
    // U 位检查
    if priv_mode == PrivilegeMode::User && !perms.user {
        return Err(PageFault::UserAccessViolation);
    }
    
    if priv_mode == PrivilegeMode::Supervisor && perms.user {
        // SUM 位检查 (Supervisor User Memory access)
        let sum = (mstatus >> 18) & 1 != 0;
        if !sum {
            return Err(PageFault::SupervisorUserPage);
        }
    }
    
    // 访问类型检查
    match access_type {
        AccessType::InstructionFetch => {
            if !perms.execute {
                return Err(PageFault::InstructionPageFault);
            }
        }
        AccessType::Read => {
            // MXR 位检查 (Make eXecutable Readable)
            let mxr = (mstatus >> 19) & 1 != 0;
            if !perms.read && !(mxr && perms.execute) {
                return Err(PageFault::LoadPageFault);
            }
        }
        AccessType::Write => {
            if !perms.write {
                return Err(PageFault::StorePageFault);
            }
            // 检查脏位
            if !pte.is_dirty() {
                return Err(PageFault::StorePageFault);
            }
        }
    }
    
    Ok(())
}
```

---

## 8. 接口定义

### 8.1 MMU 核心接口

```rust
/// MMU 核心 trait
pub trait Mmu: Send + Sync {
    /// 初始化 MMU
    fn init(&mut self, config: MmuConfig);
    
    /// 刷新 TLB (SFENCE.VMA 实现)
    fn flush_tlb(&mut self, vaddr: Option<u64>, asid: Option<u16>);
    
    /// 转换虚拟地址到物理地址
    fn translate(
        &self,
        vaddr: u64,
        access_type: AccessType,
        satp: u64,
        mstatus: u64,
        privilege: PrivilegeMode,
    ) -> Result<u64, TranslationError>;
    
    /// 获取 TLB 统计信息
    fn tlb_stats(&self) -> TlbStats;
}

/// MMU 配置
#[derive(Debug, Clone)]
pub struct MmuConfig {
    /// TLB 大小
    pub tlb_size: usize,
    /// TLB 关联度
    pub tlb_ways: usize,
    /// 启用 Sv48
    pub enable_sv48: bool,
    /// PMP 条目数
    pub pmp_entries: usize,
}

/// TLB 统计
#[derive(Debug, Clone, Default)]
pub struct TlbStats {
    /// 访问次数
    pub accesses: u64,
    /// 命中次数
    pub hits: u64,
    /// 未命中次数
    pub misses: u64,
    /// 刷新次数
    pub flushes: u64,
}

impl TlbStats {
    pub fn hit_rate(&self) -> f64 {
        if self.accesses == 0 {
            0.0
        } else {
            self.hits as f64 / self.accesses as f64
        }
    }
}
```

### 8.2 物理内存接口

```rust
/// 物理内存接口
trait PhysicalMemoryInterface: Send + Sync {
    /// 读取数据
    fn read(&self, paddr: u64, size: AccessSize) -> Result<u64, MemoryError>;
    
    /// 写入数据
    fn write(&mut self, paddr: u64, value: u64, size: AccessSize) -> Result<(), MemoryError>;
    
    /// 读取双字 (用于页表遍历)
    fn read_dword(&self, paddr: u64) -> Result<u64, MemoryError>;
    
    /// 写入双字 (用于更新 A/D 位)
    fn write_dword(&mut self, paddr: u64, value: u64) -> Result<(), MemoryError>;
    
    /// 获取内存区域信息
    fn get_region(&self, paddr: u64) -> Option<MemoryRegion>;
}
```

---

## 9. 性能优化

### 9.1 TLB 优化策略

| 优化技术 | 描述 | 预期收益 |
|---------|------|---------|
| **分离 ITLB/DTLB** | 指令和数据使用独立 TLB | 减少冲突，提高并行性 |
| **4-way 组相联** | 相比直接映射，降低冲突率 | 命中率 +5-10% |
| **LRU 替换** | 替换最近最少使用条目 | 更符合访问模式 |
| **全局映射** | G=1 的映射在 ASID 切换时保留 | 减少内核态 TLB miss |
| **投机查找** | 同时进行 TLB 查找和权限检查 | 降低延迟 |

### 9.2 页表遍历优化

```rust
/// PTE 缓存 (快速路径)
pub struct PteCache {
    /// 缓存条目 [vpn_hash % SIZE]
    entries: [Option<PteCacheEntry>; 256],
}

struct PteCacheEntry {
    vpn: u64,
    pte: PageTableEntry,
    asid: u16,
    timestamp: u64,
}

impl PteCache {
    /// 查找缓存的 PTE
    pub fn lookup(&self, vpn: u64, asid: u16) -> Option<PageTableEntry> {
        let index = (vpn as usize) % self.entries.len();
        self.entries[index].and_then(|entry| {
            if entry.vpn == vpn && entry.asid == asid {
                Some(entry.pte)
            } else {
                None
            }
        })
    }
}
```

### 9.3 快速路径优化

```rust
/// 地址转换快速路径
#[inline(always)]
pub fn translate_fast(
    &self,
    vaddr: u64,
    access_type: AccessType,
    satp: Satp,
) -> Option<u64> {
    // 仅处理最常见的情况
    if satp.mode() != TranslationMode::Sv39 {
        return None;
    }
    
    // TLB 查找 (内联展开)
    let vpn = vaddr >> 12;
    if let Some(entry) = self.tlb.lookup(vpn, satp.asid()) {
        // 快速权限检查
        if self.quick_permission_check(&entry, access_type) {
            let paddr = (entry.ppn << 12) | (vaddr & 0xFFF);
            return Some(paddr);
        }
    }
    
    None  // 走慢速路径
}
```

---

## 10. 测试策略

### 10.1 单元测试

| 测试模块 | 测试内容 | 预期测试数 |
|---------|---------|-----------|
| `tlb_test` | TLB 命中/未命中/替换/刷新 | 20 |
| `pte_test` | 页表项解析/权限检查 | 15 |
| `ptw_test` | 页表遍历/大页/错误处理 | 25 |
| `translation_test` | 地址转换流程 | 20 |
| `pmp_test` | PMP 匹配/权限检查 | 15 |
| `mmio_test` | MMIO 访问/设备注册 | 10 |

### 10.2 集成测试

```rust
/// Sv39 基本地址转换测试
#[test]
fn test_sv39_basic_translation() {
    // 设置页表
    let mut mem = setup_memory();
    let root_ppn = create_page_table(&mut mem, &[
        (0x1000, 0x8000_0000, PagePermissions::rwx()),
    ]);
    
    // 设置 satp
    let satp = Satp::new(TranslationMode::Sv39, 0, root_ppn);
    
    // 创建 MMU
    let mmu = Mmu::new(mem);
    
    // 测试转换
    let vaddr = 0x1000;
    let paddr = mmu.translate(
        vaddr,
        AccessType::Read,
        satp.bits(),
        0,  // mstatus
        PrivilegeMode::Supervisor,
    ).unwrap();
    
    assert_eq!(paddr, 0x8000_0000);
}

/// TLB 命中测试
#[test]
fn test_tlb_hit() {
    let mut mmu = create_test_mmu();
    
    // 第一次访问 (TLB miss)
    let _ = mmu.translate(/* ... */);
    
    // 第二次访问 (TLB hit)
    let _ = mmu.translate(/* ... */);
    
    let stats = mmu.tlb_stats();
    assert_eq!(stats.accesses, 2);
    assert_eq!(stats.hits, 1);
    assert_eq!(stats.misses, 1);
    assert!((stats.hit_rate() - 0.5).abs() < 0.01);
}
```

### 10.3 性能测试

| 测试项 | 目标 | 测试方法 |
|-------|------|---------|
| TLB 查找 | < 5ns | Criterion benchmark |
| 页表遍历 | < 100ns | 模拟内存延迟 |
| TLB 命中率 | > 90% | 运行测试程序 |
| MMIO 访问 | < 50ns | 设备模拟 |

---

## 附录

### A. 参考规范

- RISC-V Privileged Architecture Specification v1.12
- RISC-V Base ISA Specification v2.1
- Spike RISC-V ISA Simulator (参考实现)

### B. 术语表

| 术语 | 说明 |
|-----|------|
| VPN | Virtual Page Number |
| PPN | Physical Page Number |
| PTE | Page Table Entry |
| TLB | Translation Lookaside Buffer |
| PTW | Page Table Walker |
| PMP | Physical Memory Protection |
| MMIO | Memory-Mapped I/O |
| ASID | Address Space Identifier |
| SFENCE.VMA | 刷新虚拟内存地址的指令 |

### C. 相关文件

- `src/mmu/mod.rs` - MMU 模块入口
- `src/mmu/tlb.rs` - TLB 实现
- `src/mmu/ptw.rs` - 页表遍历器
- `src/mmu/translation.rs` - 地址转换引擎
- `src/mmu/physical.rs` - 物理内存管理
- `src/mmu/pmp.rs` - PMP 实现
- `src/mmu/mmio.rs` - MMIO 支持
