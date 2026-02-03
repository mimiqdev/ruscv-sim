# RISC-V ELF Loader 测试

本目录包含用于测试 RISC-V 模拟器 ELF 加载器和执行器的测试程序。

## 测试程序

| 程序 | 文件 | 功能 | 预期结果 |
|------|------|------|----------|
| 加法测试 | `add.elf` | 计算 1+2+...+10 | 退出码 55 |
| 斐波那契 | `fib.elf` | 计算 F10 | 退出码 34 |
| Hello World | `hello.elf` | UART 输出 "Hello!" | 退出码 0 |

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
cd tests/riscv-tests
make
```

或者手动编译：
```bash
riscv64-unknown-elf-as -march=rv64ima -mabi=lp64 add.S -o add.o
riscv64-unknown-elf-ld -Ttext=0x80000000 add.o -o add.elf
```

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
cargo run -- run tests/riscv-tests/add.elf --max-cycles 100000
```

## 目录结构

```
tests/
├── test_elf_loader.sh      # Shell 测试脚本
├── test_elf_loader.rs      # Rust 集成测试
└── riscv-tests/
    ├── Makefile            # 编译脚本
    ├── add.S               # 加法测试 (1+2+...+10)
    ├── fib.S               # 斐波那契测试 (F10)
    └── hello.S             # UART 输出测试
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
file add.elf
# 应输出: add.elf: ELF 64-bit LSB executable, UCB RISC-V
```

### "Unsupported machine"

确保目标架构是 RISC-V (e_machine = 243)：
```bash
riscv64-unknown-elf-readelf -h add.elf | grep Machine
# 应输出: Machine:                           RISC-V
```

## 添加新测试

1. 在 `riscv-tests/` 目录创建新的 `.S` 文件
2. 遵循现有测试的汇编模板
3. 更新 `Makefile` 添加新目标
4. 更新此 README
