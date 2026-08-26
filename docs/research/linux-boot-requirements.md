# Linux Boot Requirements — Research Note

**Status:** Research

**Authority:** Informational; not an implementation or milestone claim

**Last reviewed:** 2026-08-26

启动 Linux 需要完整且经过端到端验证的 S 模式、MMU/Sv39、特权 CSR、陷阱与中断路径。仓库中存在相关组件并不等于这些能力已经集成。除 Hart 侧能力外，还需要以下系统能力：

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

## 4. Linux 所需平台设备
典型平台至少需要正确集成并验证 CLINT/PLIC/UART；根据启动介质与系统配置，还可能需要：
- **块设备**：virtio-mmio 或简单 RAM 盘（用于 rootfs）
- （可选）virtio-net（网络）

## 5. 兼容的内存布局
- 需与 OpenSBI/Linux 兼容的内存映射（例如 DRAM 从 `0x8000_0000` 开始）。
- 需预留固件/DTB/Initrd 区域，避免与内核冲突。

---

### 小结
Linux kernel 启动需要 SBI 固件（OpenSBI/RustSBI）、启动链路（bootloader 或直接加载）、正确的设备树，以及 Linux 期望的外设与内存布局。是否进入开发计划应由后续架构里程碑单独决定。
