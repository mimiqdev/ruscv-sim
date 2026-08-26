# M5 测试集成与改进建议报告（riscv-arch-test）

> Historical document. This RISCOF-era report was later superseded by an ACT4 proposal; that proposal was itself archived without completion during the 2026-08-26 architecture reset. Neither plan is active.

> **riscv-arch-test**（可简称 **arch-test**）是 RISC-V 官方架构测试套件，用于验证 ISA 实现正确性。

**日期**: 2026-02-02

## 与 dev-plan.md M5 规划对应关系

| dev-plan.md 章节 | 本报告对应内容 |
|---|---|
| M5 ELF 执行闭环 | 第 1.3 节「必需输入/输出接口」、第 1.4 节「集成步骤」 |
| M6 测试质量强化 | 第 2 节「目前测试体系的改进空间」 |
| M7 RISCOF + arch-test 集成 | 第 1.1 节「推荐工具链」、第 1.2 节「集成结构建议」 |

## 1. riscv-arch-test 集成方式建议

### 1.1 推荐工具链

| 工具 | 用途 | 安装方式 |
|------|------|----------|
| **RISCOF** | 统一的架构测试框架 | `pip3 install riscof` |
| **riscv-arch-test** | 官方测试集 | RISCOF 自动下载或手动 clone |
| **Spike** | 参考模型 | `sudo apt install spike` 或源码编译 |
| **ruscv-sim** | DUT (待测模拟器) | 本仓库 cargo build |

### 1.2 Spike 推荐安装方式

#### 方式一：APT 安装（推荐 Ubuntu/Debian）
```bash
sudo apt update
sudo apt install spike
spike --version
```

#### 方式二：源码编译（最新版本）
```bash
# 安装依赖
sudo apt install build-essential device-tree-compiler libftdi1-dev libssl-dev

# 编译 riscv-tools（包含 Spike）
git clone https://github.com/riscv/riscv-tools.git
cd riscv-tools
./build.sh

# 或单独编译 Spike
git clone https://github.com/riscv/riscv-isa-sim.git
cd riscv-isa-sim
mkdir build && cd build
../configure --prefix=$HOME/riscv
make -j$(nproc)
make install

# 添加到 PATH
export PATH=$HOME/riscv/bin:$PATH
```

#### 验证安装
```bash
spike --help
pk -h  # 测试 proxy kernel
```

### 1.3 集成结构建议

建议采用「RISCOF 驱动 + 双模型对比」方式：

```
RISCOF
 ├── reference (Spike)
 └── DUT (ruscv-sim)
         ├── ELF loader
         ├── 运行入口 (reset → run)
         └── signature 导出
```

### 1.4 必需输入/输出接口

**DUT 需提供**：

- 可执行 ELF 的 loader（或外部 loader 接口）
- 固定入口点支持（如 `_start`）
- signature 区域的内存导出（如 signature begin/end）
- tohost / exit 退出机制

**RISCOF 配置需提供**：

- DUT YAML（ISA、XLEN、支持扩展、特权模式）
- DUT runner 脚本（调用 ruscv-sim 执行 ELF + signature dump）

### 1.5 集成步骤（最小化路径）

1. 安装 RISCOF 与 riscv-arch-test
2. 安装 Spike 作为参考模型
3. 编写 ruscv-sim 的 DUT YAML 配置
4. 增加 ELF loader 与 signature dump
5. 编写 runner 脚本供 RISCOF 调用
6. 分模块运行 arch-test（I/M/A/F/D/C/Priv）并修复失败

## 2. 目前测试体系的改进空间

### 2.1 单元测试（Unit）

**现状**：覆盖 ISA 与核心模块，测试数量充足（700+）。

**改进建议**：
- 引入 **属性测试**（proptest）对外设边界与 MMU 参数做随机探索
- 强化 CSR WARL/WIRI 行为测试（覆盖 "非法值写入后回读" 逻辑）
- 增加指令随机组合/执行序列测试

### 2.2 集成测试（Integration）

**现状**：外设与 TLM 子系统已有覆盖，但缺少完整程序执行流程。

**改进建议**：
- 增加 **ELF 加载 + 程序执行** 的集成测试
- 增加 "最小裸机程序" 级别的回归测试（如 hello world / mret / trap 路径）
- 针对 MMU/Sv39 的「系统级」用例（多页表、多特权模式切换）

### 2.3 系统测试（System / E2E）

**现状**：尚无与外部工具链对接的系统测试。

**改进建议**：
- 将 riscv-arch-test 作为系统测试主干
- 在 CI 中加入小规模 smoke 子集（如 RV64I + CSR）

### 2.4 覆盖率与质量度量

**现状**：无覆盖率报告。

**改进建议**：
- 集成 `cargo-llvm-cov` 输出覆盖率
- 建立门槛（如 core/execute 模块 80%+）

## 3. arch-test 集成后的增量开发建议

1. 先实现 **ELF loader + signature 导出 + tohost 退出**（实现可运行 arch-test 的最小链路）
2. 优先跑 RV64I + Priv 测试，定位特权/异常缺陷
3. 逐步扩展到 M/A/F/D/C 子集
4. 建立失败用例分类表，持续修复

## 4. 结论

M5 的核心任务不是增加指令数量，而是 **建立 arch-test 的"执行闭环"**。完成 RISCOF 集成与 ELF/Signature 支持后，ruscv-sim 才能正式进入架构测试验证阶段。现有单元测试覆盖充分，但在「执行完整程序」与「系统级验证」层面仍存在明显空白，需要通过 arch-test 和 ELF 集成测试补齐。
