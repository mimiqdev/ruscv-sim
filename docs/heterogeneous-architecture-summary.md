 > **注意**：本文档中的代码示例仅用于说明设计概念和 API 用途，是伪代码性质的示例，不保证可编译通过。示例代码可能包含语法糖简写、类型占位符等，仅供理解之用。 
# ruscv-sim 异构计算仿真架构总结

> 文档版本: v1.0  
> 日期: 2026-02-05  
> 讨论日期: 2026-02-05

**相关文档：**
- [详细研究报告](heterogeneous-computing-research.md) - 深入分析 CPU+NPU/GPU 协同仿真
- [SystemBus 扩展计划](systembus-extension-plan.md) - 核心代码设计
- [NPU 集成方案](npu-integration.md) - NPU 设备实现

---

## 1. 核心决策

| 问题 | 决策 |
|------|------|
| 总线协议抽象 | 简化事务接口，不暴露协议细节 |
| TLM2.0 | 不使用，用 Rust 实现简化版 TLM |
| Time Annotation | 预留 `Option<TimeAnnotation>` 扩展 |
| SystemC 接入 | 预留 `ExternalDevice trait` + IPC 通道 |
| 时钟门控 | 先不做，Phase 3 考虑 |

---

## 2. 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                    Rust 仿真器                              │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐                  │
│  │  CPU    │  │SystemBus │  │  NPU    │                  │
│  └─────────┘  └─────────┘  └─────────┘                  │
│         │                                                │
│         │ IPC (Unix Socket / FFI)                        │
│         ▼                                                │
│  ┌─────────────────────────────────────────────────────┐│
│  │               SystemC 模型                            ││
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐          ││
│  │  │  GPU    │  │  DDR    │  │  ISP    │          ││
│  │  └─────────┘  └─────────┘  └─────────┘          ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

---

## 3. 文档索引

### 3.1 SystemBus 扩展
**文件**: [systembus-extension-plan.md](systembus-extension-plan.md)

包含：
- Device trait 设计
- SystemBus 扩展实现
- DMA Controller
- 错误类型

### 3.2 NPU 集成
**文件**: [npu-integration.md](npu-integration.md)

包含：
- NPU 设备实现
- PE 阵列仿真
- RISC-V 交互方案
- 执行流程

### 3.3 深入研究
**文件**: [heterogeneous-computing-research.md](heterogeneous-computing-research.md)

包含：
- CPU + NPU/GPU 协同仿真设计
- 共享内存一致性
- 任务调度与同步
- 性能建模

---

## 4. 组件状态

| 组件 | 状态 | 文档 |
|------|------|------|
| RISC-V Core | ✅ 完整 | - |
| CSR | ✅ 完整 | - |
| MMU/TLB | ✅ 完整 | - |
| **SystemBus** | ⚠️ 需扩展 | systembus-extension-plan.md |
| PLIC | ✅ 完整 | - |
| CLINT | ✅ 完整 | - |
| UART | ✅ 完整 | - |
| **NPU** | ⚠️ 计划中 | npu-integration.md |
| GPU | ❌ 缺失 | - |
| 多核支持 | ❌ 缺失 | - |

---

## 5. 开发路线

```
Phase 1 (2 天): Device trait + SystemBus 扩展
Phase 2 (1 天): DMA Controller
Phase 3 (3 天): NPU 设备（功能验证版）
Phase 4 (2 天): 测试 + 文档
```

---

## 6. 核心 trait

```rust
// 设备 trait
pub trait Device: Send + Sync {
    fn name(&self) -> &'static str;
    fn base_addr(&self) -> u64;
    fn size(&self) -> usize;
    fn read(&self, offset: u64, size: u8) -> Result<u32, DeviceError>;
    fn write(&mut self, offset: u64, value: u32, size: u8) -> Result<(), DeviceError>;
    fn interrupt(&self) -> Option<u32> { None }
}

// 主设备 trait
pub trait MasterDevice: Device {
    fn dma_read(&self, addr: u64, size: usize) -> Result<Vec<u8>, DeviceError>;
    fn dma_write(&self, addr: u64, data: &[u8]) -> Result<(), DeviceError>;
}
```

---

## 7. 预留扩展

### 7.1 Time Annotation
```rust
struct Transaction {
    addr: u64,
    data: Vec<u8>,
    annotation: Option<TimeAnnotation>,  // 预留
}
```

### 7.2 SystemC 接入
```rust
trait ExternalDevice: Send + Sync {
    fn init(&mut self, config: &ExternalConfig) -> Result<(), ExternalError>;
    fn step(&mut self, cycles: u64) -> Result<(), ExternalError>;
    fn transport(&mut self, trans: &mut Transaction) -> Result<(), ExternalError>;
}
```

---

## 8. 下一步

1. ✅ 阅读架构总结
2. 📖 阅读 [systembus-extension-plan.md](systembus-extension-plan.md)
3. 📖 阅读 [npu-integration.md](npu-integration.md)
4. 💻 实现 Device trait + SystemBus 扩展
5. 💻 添加 DMA Controller
6. 💻 实现 NPU 设备

---

## 9. 参考资料

- [异构计算研究报告](heterogeneous-computing-research.md)
- RISC-V Privileged Architecture Specification
- AMBA Protocol Specification (ARM)
- TLM-2.0 Standard (Accellera)
- FastModels AMBA-PV (ARM)

---

*文档结束*
