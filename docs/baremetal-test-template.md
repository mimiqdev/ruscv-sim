# RISC-V 指令测试模板

本文档定义了 ruscv-sim 的 ELF 回归测试模板，用于验证指令实现的正确性。

---

## 1. 测试目的

ELF 回归测试用于验证模拟器对 RISC-V 指令的实现是否符合 ISA 规范。每个测试文件：

- 针对特定指令或指令组合
- 使用自检测证机制（Pass/Fail）
- 通过 `tohost` 机制自动报告结果
- 可被 CI 自动化运行

---

## 2. 测试模板代码

### 2.1 最小测试模板

以下是最小化的测试模板结构：

```asm
# Test: <指令名称> - <测试描述>
# Tests: <被测指令列表>

.section .text
.globl _start

_start:
    # === 测试准备 ===
    li      x1, <初始值1>
    li      x2, <初始值2>
    
    # === 执行被测指令 ===
    <被测指令>  x3, x1, x2    # x3 = x1 <op> x2
    
    # === 验证结果 ===
    li      x7, <期望值>      # x7 = 预期结果
    beq     x3, x7, pass      # 如果结果正确，跳转 pass

fail:
    li      x5, 1             # exit_code = 1 (失败)
    j       write_exit

pass:
    li      x5, 0             # exit_code = 0 (通过)

write_exit:
    la      x4, tohost
    li      x6, 1
    slli    x6, x6, 63        # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6        # x5 = WRITE_MARKER | exit_code
    sd      x5, 0(x4)         # 写入 tohost

inf:
    j       inf               # 无限循环

.section .tohost
.align 3
tohost:
    .dword  0
```

### 2.2 完整示例：add.S

```asm
# Simple RISC-V test: calculate 1 + 2 + ... + 10 = 55
# Tests: ADD, ADDI, SUB, BLT, BNE, JALR

.section .text
.globl _start

_start:
    # Initialize
    li      x1, 0           # sum = 0
    li      x2, 1           # i = 1
    li      x3, 10          # n = 10

loop:
    # if i > n, exit loop
    bgt     x2, x3, done

    # sum += i
    add     x1, x1, x2

    # i++
    addi    x2, x2, 1

    j       loop

done:
    # Expected result: x1 = 55 (0x37)
    
    # Verify calculation result
    li      x7, 55          # x7 = expected value (55)
    beq     x1, x7, pass    # if x1 == 55, test passed

fail:
    # Test failed, exit code = 1
    li      x5, 1
    j       write_exit

pass:
    # Test passed, exit code = 0
    li      x5, 0           # return 0 to indicate pass

write_exit:
    # Use la pseudo-instruction to auto-generate auipc/addi/ld sequence
    la      x4, tohost

    li      x6, 1
    slli    x6, x6, 63      # x6 = WRITE_MARKER (1 << 63)
    or      x5, x5, x6      # x5 = WRITE_MARKER | exit_code
    
    # x4 is guaranteed 8-byte aligned (due to .align 3), no error will occur
    sd      x5, 0(x4)       # Write to tohost

    # Loop forever
inf:
    j       inf

# --- Key change: use .tohost section ---
# This aligns with linker.ld to automatically place at 0x80001000
.section .tohost
.align 3
tohost:
    .dword  0
```

---

## 3. 约定说明

### 3.1 Pass/Fail 机制

| 组件 | 约定 | 说明 |
|------|------|------|
| `exit_code` | x5 寄存器 | `0` = Pass, `1` = Fail |
| `WRITE_MARKER` | `1 << 63` | 标记写入操作，避免与测试数据混淆 |
| `tohost` 地址 | `0x80001000` | 模拟器监听此地址的写入 |
| 检测方式 | `sd x5, 0(tohost)` | 写入 64 位值触发模拟器停止 |

### 3.2 验证方法

**寄存器验证**：
```asm
li      x7, <期望值>
beq     <结果寄存器>, x7, pass
j       fail
```

**内存验证**（Load/Store 测试）：
```asm
# 写入测试值
li      x1, 0x12345678
sw      x1, 0(x2)         # 写入内存

# 读回验证
lw      x3, 0(x2)
li      x7, 0x12345678
beq     x3, x7, pass
j       fail
```

**分支验证**：
```asm
li      x1, 5
li      x2, 10
blt     x1, x2, branch_taken    # 应该跳转
j       fail                    # 没跳转则失败

branch_taken:
    li      x5, 0               # 通过
    j       write_exit
```

---

## 4. 目录结构

```
tests/
├── bare-metal-riscv-test/    # ELF 回归测试（汇编源文件）
│   ├── linker.ld             # 链接器脚本（内存布局）
│   ├── Makefile              # 构建脚本
│   ├── rv64i/                # RV64I 基础整数指令测试
│   │   ├── add.S             # ADD 指令测试
│   │   ├── fib.S             # 斐波那契测试
│   │   └── hello.S           # UART 输出测试
│   ├── rv64m/                # RV64M 整数乘除法测试
│   ├── rv64a/                # RV64A 原子指令测试
│   ├── rv64f/                # RV64F 单精度浮点测试
│   ├── rv64d/                # RV64D 双精度浮点测试
│   └── rv64c/                # RV64C 压缩指令测试
│
├── test_elf_loader.rs        # ELF 加载器集成测试
├── test_add_direct.rs        # 指令直接测试（Rust 单元测试）
├── mul_test.rs               # M 扩展测试
├── div_test.rs               # 除法测试
├── f_arith_test.rs           # F 扩展测试
├── d_arith_test.rs           # D 扩展测试
├── rv64c_test.rs             # C 扩展测试
├── csr_access_test.rs        # CSR 访问测试
├── peripheral_tests.rs       # 外设测试
└── ...                       # 其他 Rust 集成测试
```

### 4.1 构建测试

```bash
cd tests/bare-metal-riscv-test
make all          # 编译所有测试
make disasm       # 反汇编查看
make clean        # 清理生成文件
```

### 4.2 运行测试

```bash
# 运行所有测试
cargo test --test test_elf_loader

# 运行单个测试
cargo test test_add -- --nocapture

# 运行模拟器（手动）
cargo run -- run tests/bare-metal-riscv-test/rv64i/add.elf --verbose
```

---

## 5. 新增测试 checklist

创建新测试时请检查：

- [ ] 文件命名遵循 `<instruction>[_<variant>].S` 格式
- [ ] 包含清晰的注释说明测试目的和覆盖的指令
- [ ] 使用 `x7` 作为期望值的临时寄存器
- [ ] 使用 `x5` 作为 exit_code 寄存器
- [ ] 包含 `.tohost` 段定义
- [ ] 在 Makefile 的 `SOURCES` 中添加新文件
- [ ] 执行 `make all` 验证编译通过
- [ ] 执行 `cargo test` 验证测试通过
