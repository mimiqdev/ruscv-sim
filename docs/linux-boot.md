# Linux Kernel 启动所需支持（计划外补充）

本项目的开发计划已经覆盖 **S 模式、MMU/Sv39、特权 CSR、陷阱/中断框架** 等基础能力。若希望进一步启动 Linux kernel，除计划内事项之外，还需要以下支持：

## 1. SBI 固件（必需）
- Linux 通过 **SBI** 与 M 模式交互，必须提供 SBI 实现。
- 推荐使用 **OpenSBI** 或 **RustSBI**（不需要 “libgross”）。
- 需要支持的 SBI 功能（最小集合）：
  - Timer（定时器中断）
  - IPI（核间中断，若多核）
  - Console（串口输出）
  - RFENCE（TLB 刷新）
  - System Reset / Shutdown

## 2. 启动链路与镜像加载（必需）
- 需要 **bootloader/加载器** 将 Linux `Image`、`initrd` 与 `dtb` 放入内存。
- 可选方案：
  - OpenSBI + U-Boot（标准链路）
  - 在模拟器中实现 **直接加载**（kernel + dtb + initrd）

## 3. Device Tree（必需）
- Linux 依赖 **DTB** 获取平台信息。
- 需要生成与模拟平台一致的设备树，包括：
  - CPU/hart 配置与 ISA
  - 内存布局（DRAM 基址/大小）
  - CLINT/PLIC/UART 地址与中断号
  - 额外设备（如 virtio-blk）

## 4. Linux 必需外设（计划外）
除已在计划中的 CLINT/PLIC/UART 外，通常还需要：
- **块设备**：virtio-mmio 或简单 RAM 盘（用于 rootfs）
- （可选）virtio-net（网络）

## 5. 兼容的内存布局（计划外）
- 需与 OpenSBI/Linux 兼容的内存映射（例如 DRAM 从 `0x8000_0000` 开始）。
- 需预留固件/DTB/Initrd 区域，避免与内核冲突。

---

### 小结
Linux kernel 启动需要 SBI 固件（OpenSBI/RustSBI）、启动链路（bootloader 或直接加载）、正确的设备树以及 Linux 期望的外设与内存布局。这些属于 **开发计划之外** 的系统集成工作。**
