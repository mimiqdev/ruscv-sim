# RISC-V Bare Metal Tests

本目录包含用于测试 RISC-V 模拟器 ELF 加载器和执行器的裸机测试程序。

## 目录结构

```
tests/bare-metal-riscv-test/
├── Makefile            # 编译脚本
├── linker.ld           # 链接器脚本（默认）
├── linker-s4.ld        # Stage 4 guest（rv64i/s4_*.S）专用链接器脚本
├── README.md           # 本文件
├── rv64a/              # RV64A 原子指令回归 guest
├── rv64i/              # RV64I 基础整数指令测试
├── rv64ifd/            # F/D 与 Zcd 浮点 guest
└── rv64m/              # RV64M 整数乘除法测试
```

## 测试程序

| 程序 | 文件 | 功能 | 预期结果 |
| ------ | ------ | ------ | ---------- |
| 加法测试 | `rv64i/add.elf` | 计算 1+2+...+10 | 退出码 55 (计算结果) |
| 斐波那契 | `rv64i/fib.elf` | 计算 F10 | 退出码 0 (pass/fail) |
| Hello World | `rv64i/hello.elf` | UART 输出 "Hello!" | 退出码 0 |
| BLT Issue | `rv64i/blt_issue.elf` | BLT 分支测试 | 退出码 55 |
| BLT Simple | `rv64i/blt_simple.elf` | 简化 BLT 测试 | 退出码 21 |
| BLT Test | `rv64i/blt_test.elf` | BLT 综合测试 | 退出码 21 |
| Zca 控制流 | `rv64i/zca_ctrl_flow.elf` | 压缩/32 位混合分支与跳转 | 退出码 0 (`--isa rv64imac`) |
| Zca 链接值 | `rv64i/zca_link_values.elf` | `c.jalr` 链接 `pc+2`、`jal` 链接 `pc+4` | 退出码 0 (`--isa rv64imac`) |
| Zca 断点陷阱 | `rv64i/zca_ebreak_trap.elf` | `C.EBREAK` 触发 `mcause=3` 并恢复 | 退出码 0 (`--isa rv64imac`) |
| Zca HINT | `rv64i/zca_hints.elf` | D2 HINT 编码不影响架构状态 | 退出码 0 (`--isa rv64imac`) |
| Zca 跨页取指 | `rv64i/zca_straddle_fetch.elf` | 跨页 32 位指令第二 parcel 取指访问故障 (`mtval=0x2000`) | 退出码 0 (`--isa rv64imac`) |
| F/D 舍入模式 | `rv64ifd/fd_rounding.elf` | 五种舍入模式的静态与动态选择、RMM 与 RNE 的差异、保留 `rm` 陷阱 | 退出码 0 (`--isa rv64imafd`) |
| F/D 异常标志 | `rv64ifd/fd_flags.elf` | NX/UF/OF/DZ/NV 累积、舍入后检测非规格化、`fflags`/`frm`/`fcsr` 别名 | 退出码 0 (`--isa rv64imafd`) |
| F/D NaN 装箱 | `rv64ifd/fd_nanbox.elf` | NaN 装箱、规范 NaN、FSGNJ 载荷保留、窄传输低 32 位 | 退出码 0 (`--isa rv64imafd`) |
| F/D FS 状态 | `rv64ifd/fd_fs_state.elf` | FS=Off 陷阱、Dirty 标记规则、SD 位与 sstatus 视图 | 退出码 0 (`--isa rv64imafd`) |
| F/D 融合乘加 | `rv64ifd/fd_fma.elf` | FMADD/FMSUB/FNMSUB/FNMADD 单次舍入 | 退出码 0 (`--isa rv64imafd`) |
| F/D 转换与分类 | `rv64ifd/fd_convert.elf` | 饱和表、符号扩展、FCLASS 十类、NaN 比较规则 | 退出码 0 (`--isa rv64imafd`) |
| Zcd 缺失对照 | `rv64ifd/fd_zcd_absent.elf` | `rv64imafd` 下四个 Zcd 编码仍触发 `mcause=2` | 退出码 0 (`--isa rv64imafd`) |
| Zcd 传输 | `rv64ifd/zcd_transfer.elf` | 四个 Zcd parcel 的缩放偏移传输与 NaN 载荷保留 | 退出码 0 (`--isa rv64imafdc`) |
| F/D 调试会话 | `rv64ifd/fdc_u6_attach.elf` | 未连接调试器时校验复位 FP 状态；连接时校验调试器写入的 `f0`–`f31`/`fcsr`，并配合 FLD/FADD.D/FSW 单步与 FS 检查 | 退出码 0 (`--isa rv64imafdc`) |

## 快速开始

### 1. 安装 RISC-V 工具链

**Ubuntu/Debian:**

```bash
sudo apt install gcc-riscv64-unknown-elf
```

**macOS:**

```bash
brew install riscv-gnu-toolchain
```

**从源码编译:**

```bash
git clone https://github.com/riscv-collab/riscv-gnu-toolchain.git
cd riscv-gnu-toolchain
./configure --prefix=/opt/riscv
make -j$(nproc)
```

### 2. 编译测试程序

```bash
cd tests/bare-metal-riscv-test
make
```

或者手动编译：

```bash
cd tests/bare-metal-riscv-test/rv64i
riscv64-unknown-elf-as -march=rv64ima -mabi=lp64 add.S -o add.o
riscv64-unknown-elf-ld -T ../linker.ld add.o -o add.elf
```

`s4_*` guest 需改用 `-T ../linker-s4.ld`：其镜像内含页表，tohost 必须位于镜像之后的独立页。`rv64ifd/` 下的 F/D 与 Zcd guest 同样使用 `linker-s4.ld`，镜像内包含数据表、暂存区和陷阱处理器。

### 3. 运行测试

**方式 1: 使用测试脚本**

```bash
cd tests
./test_elf_loader.sh
```

**方式 2: 使用 Rust 测试**

```bash
cargo test --test test_elf_loader
```

**方式 3: 手动运行**

```bash
cargo run -- run tests/bare-metal-riscv-test/rv64i/add.elf --max-cycles 100000
```

## 测试流程

1. **加载 ELF**: 解析 ELF 头，提取入口点和段信息
2. **加载到内存**: 将 PT_LOAD 段加载到模拟器内存
3. **执行**: 从入口点开始执行指令
4. **退出检测**: 检测 tohost 写入信号，返回退出码

## 退出码约定

模拟器支持两种退出信号格式：

1. **标准格式**: `(1 << 63) | exit_code` (上层位标记写入)
2. **直接格式**: 小于 0x100 的值直接作为退出码

```rust
// 示例: 退出码 55
let tohost_value = (1 << 63) | 55;  // 0x8000000000000037
```

## 故障排除

### "RISC-V toolchain not found"

确保工具链已安装并添加到 PATH：

```bash
export PATH=$PATH:/opt/riscv/bin
```

### "Invalid ELF magic number"

确保生成的是 64 位小端 ELF 文件：

```bash
file rv64i/add.elf
# 应输出: add.elf: ELF 64-bit LSB executable, UCB RISC-V
```

### "Unsupported machine"

确保目标架构是 RISC-V (e_machine = 243)：

```bash
riscv64-unknown-elf-readelf -h rv64i/add.elf | grep Machine
# 应输出: Machine:                           RISC-V
```

## 添加新测试

1. 在对应扩展子目录 (`rv64i/`, `rv64m/`, `rv64ifd/` 等) 创建新的 `.S` 文件
2. 遵循现有测试的汇编模板
3. 运行 `make` 自动编译所有子目录中的测试
4. 更新此 README
