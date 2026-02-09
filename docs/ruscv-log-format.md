# ruscv-sim 日志格式规范

**项目:** ruscv-sim  
**文档版本:** 1.0  
**创建日期:** 2026-02-09  
**状态:** 完成

---

## 概述

本文档定义 ruscv-sim 的日志输出格式，用于与 Spike 日志进行比对。格式设计目标是 **Spike 兼容** + **可解析**。

---

## 当前日志格式

### verbose 模式输出（src/core/mod.rs）

```rust
if self.verbose {
    eprintln!(
        "[STEP] PC: {:#010x} -> {:#010x}, branch_taken={}, instr={:#010x}",
        pc_before, self.state.pc, self.state.branch_taken, instruction
    );
    eprintln!(
        "[REGS] a0(x10)={:#x}, t0(x5)={:#x}, t1(x6)={:#x}, t2(x7)={:#x}",
        self.state.regs[10], self.state.regs[5], self.state.regs[6], self.state.regs[7]
    );
}
```

**当前输出示例：**
```
[STEP] PC: 0x0000000080000000 -> 0x0000000080000004, branch_taken=false, instr=0x00000093
[REGS] a0(x10)=0x0, t0(x5)=0x0, t1(x6)=0x0, t2(x7)=0x0
```

**问题：**
1. 非标准格式，与 Spike 不兼容
2. PC 只显示 10 位十六进制（省略前导零）
3. 缺少寄存器完整状态
4. 缺少内存访问日志
5. 缺少特权模式标识

---

## 目标格式定义

### 格式规范（Spike 兼容）

```
core   <hartid>: <privilege> <pc> (<opcode>) [<changes>]
```

#### 字段定义

| 字段 | 宽度 | 格式 | 说明 |
|------|------|------|------|
| `core   ` | 5+3 | 固定 | 核心标识 |
| `<hartid>` | 1 | 数字 | Hart ID |
| `:` | 1 | 固定 | 分隔符 |
| `<privilege>` | 1 | 数字 | 特权模式 (0/1/3) |
| 空格 | 1 | 固定 | 分隔 |
| `<pc>` | 18 | `0x` + 16位十六进制 | 程序计数器 |
| 空格 | 1 | 固定 | 分隔 |
| `<opcode>` | 10 | `(0x` + 8位十六进制 + `)` | 机器码 |
| 空格 | 1 | 固定 | 分隔 |
| `<changes>` | 可变 | 见下文 | 变化信息 |

#### 寄存器变化格式

**单寄存器：**
```
x<num>  <value>
```
- 寄存器号：1-2 字符（x0-x31）
- 2 空格分隔
- 值：18 字符，零填充

**示例：**
```
x1  0x0000000000000000
x10 0x0000000000001020
x5  0x0000000080000000
```

#### 内存访问格式

**加载：**
```
mem <addr>
```
或带寄存器值：
```
x1  0x0000000012345678 mem 0x00000000800000c0
```

**存储：**
```
mem <addr> <value>
```
- 地址：18 字符
- 值：8 字符（word）或 16 字符（dword）

**示例：**
```
mem 0x0000000080000180 0x12345678     # sw
mem 0x0000000010000000 0x48           # sb
mem 0x0000000000001018                # ld (只显示地址)
```

#### 特殊标记

| 标记 | 含义 |
|------|------|
| `>>>>  <label>` | 标签/注释 |
| `H` 前缀 | 特殊状态 |
| `e` 前缀 | 异常 |

---

## 目标格式示例

### 算术指令
```
core   0: 3 0x0000000080000000 (0x00000093) x1  0x0000000000000000
core   0: 3 0x0000000080000004 (0x00100113) x2  0x0000000000000001
core   0: 3 0x0000000080000010 (0x002080b3) x1  0x0000000000000001
```

### 分支指令（无变化）
```
core   0: 3 0x000000008000000c (0x0021c863)
core   0: 3 0x0000000080000018 (0xff5ff06f)
```

### 加载指令
```
core   0: 3 0x0000000080000008 (0x00052083) x1  0x0000000012345678 mem 0x00000000800000c0
```

### 存储指令
```
core   0: 3 0x0000000080000010 (0x00152023) mem 0x0000000080000180 0x12345678
```

### 标签行
```
>>>>  loop
>>>>  wait_tx_ready
```

---

## 与 Spike 的差异

| 维度 | Spike | ruscv-sim (当前) | ruscv-sim (目标) |
|------|-------|------------------|------------------|
| PC 格式 | `0x` + 16位零填充 | `0x` + 10位 | ✅ 统一 |
| 机器码 | 8位十六进制 | 10位十六进制 | ✅ 统一 |
| 寄存器格式 | `x<num>  <value>` | `x<num>=<value>` | ⚠️ 需要适配 |
| 内存格式 | `mem <addr> [value]` | 无 | ⚠️ 需要实现 |
| 特权模式 | 数字标识 (3/1/0) | 无 | ⚠️ 需要实现 |
| 字段对齐 | 固定宽度 | 可变 | ⚠️ 需要适配 |
| 解析友好 | ✅ | ❌ | ✅ |
| Spike 兼容 | - | ❌ | ✅ |

---

## 实现优先级

### Phase 1: 基础对齐
- [ ] PC 格式改为 18 字符零填充
- [ ] 机器码格式保持
- [ ] 寄存器输出格式对齐

### Phase 2: 完整实现
- [ ] 添加 `--log-commits` 参数
- [ ] 实现内存访问日志
- [ ] 添加特权模式标识
- [ ] 实现字段对齐

### Phase 3: 增强功能
- [ ] 添加 `--log-file <path>` 参数
- [ ] 实现日志级别控制
- [ ] 添加 JSON 输出格式选项

---

## 代码实现建议

### 新增日志模块 (src/log.rs)

```rust
/// 日志输出器
pub struct CommitLogger {
    /// 输出目标
    output: Box<dyn Write>,
    /// 详细模式
    verbose: bool,
}

impl CommitLogger {
    /// 创建新日志器
    pub fn new(output: Box<dyn Write>, verbose: bool) -> Self {
        Self { output, verbose }
    }

    /// 输出提交行
    pub fn log_commit(
        &mut self,
        hartid: usize,
        privilege: u8,
        pc: u64,
        opcode: u32,
        reg_changes: &[(&str, u64)],
        mem_changes: &[MemChange],
    ) -> std::io::Result<()> {
        // 格式化特权模式
        let priv_str = match privilege {
            3 => "3",
            1 => "1",
            0 => "0",
            _ => "?",
        };

        // 格式化 PC（18字符）
        let pc_str = format!("0x{:016x}", pc);

        // 格式化机器码（10字符）
        let opcode_str = format!("(0x{:08x})", opcode);

        // 格式化寄存器变化
        let reg_strs: Vec<String> = reg_changes
            .iter()
            .map(|(name, value)| format!("{}  0x{:016x}", name, value))
            .collect();

        // 格式化内存变化
        let mem_strs: Vec<String> = mem_changes
            .iter()
            .map(|mc| {
                if let Some(value) = mc.value {
                    format!("mem 0x{:016x} 0x{:08x}", mc.addr, value)
                } else {
                    format!("mem 0x{:016x}", mc.addr)
                }
            })
            .collect();

        // 组合所有变化
        let changes: Vec<String> = reg_strs.into_iter().chain(mem_strs).collect();
        let changes_str = changes.join(" ");

        // 写入日志行
        writeln!(self.output, "core   {}: {} {} {} {}", hartid, priv_str, pc_str, opcode_str, changes_str)
    }
}
```

### 在 Core 中集成

```rust
// src/core/mod.rs

impl RiscvCore {
    pub fn step(&mut self) -> Result<()> {
        // ... 执行逻辑 ...

        // 提交日志
        if self.verbose {
            let priv_mode = self.state.privilege as u8;
            let changes = self.collect_changes(); // 收集变化
            self.logger.log_commit(0, priv_mode, pc_before, instruction, &changes);
        }
    }
}
```

---

## 测试验证

### 比对测试命令

```bash
# 编译测试程序
riscv64-unknown-elf-gcc -march=rv64imafdc -mabi=lp64 -nostdlib \
    -T tests/bare-metal-riscv-test/linker.ld \
    tests/bare-metal-riscv-test/rv64i/add.S -o add.elf

# Spike 参考日志
spike --log-commits -l add.elf > spike_add.log

# ruscv-sim 日志
./target/release/ruscv-sim --elf add.elf --log-commits ruscv_add.log

# 比对
diff spike_add.log ruscv_add.log
```

---

## 成功标准

- [ ] ruscv-sim 输出与 Spike 格式 95%+ 相似
- [ ] 字段对齐不影响可读性
- [ ] 日志可被脚本解析
- [ ] 与 RISCOF 框架兼容

---

## 参考文档

- [Spike 日志格式](spike-log-format.md)
- [Spike 官方文档](https://github.com/riscv-software-src/riscv-isa-sim)
