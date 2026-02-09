# M7: Log 输出增强实施计划

**项目:** RISC-V ISS Simulator (ruscv-sim)
**里程碑:** M7 - Log 输出增强
**文档版本:** 2.0
**创建日期:** 2026-02-08
**更新日期:** 2026-02-08
**状态:** 实施中

---

## 背景

M7 原计划集成 RISCOF + arch-test，现调整为**先增强日志输出**，实现与 Spike 直接对比。

**核心理念**: 先研究清楚需要什么格式，再实现。

---

## 阶段 1: 研究阶段 ✅ (完成)

### T.1.1 分析 Spike 日志格式 ✅

**产出**: `docs/spike-log-format.md`

**Spike 标准格式：**
```
core   <hartid>: <privilege> <pc> (<opcode>) [<changes>]
```

**关键字段：**
| 字段 | 格式 | 示例 |
|------|------|------|
| PC | `0x` + 16位零填充 | `0x0000000080000000` |
| 机器码 | `(0x` + 8位十六进制 + `)` | `(0x00000297)` |
| 寄存器 | `x<num>  <value>`（2空格） | `x5  0x0000000000001000` |
| 内存 | `mem <addr> [value]` | `mem 0x00000000800000c0 0x12345678` |
| 特权模式 | 数字 (3=M, 1=S, 0=U) | `3` |

---

### T.1.2 分析 ruscv-sim 当前格式 ✅

**当前问题：**
1. PC 只显示 10 位十六进制（应 16 位）
2. 寄存器格式不兼容：`x<num>=<value>` → 应为 `x<num>  <value>`
3. 缺少内存访问日志
4. 缺少特权模式标识
5. 字段宽度不对齐

---

### T.1.3 定义目标格式规范 ✅

**产出**: `docs/ruscv-log-format.md`

**目标格式（与 Spike 兼容）：**
```
core   0: 3 0x0000000080000000 (0x00000297) x5  0x0000000000001000
core   0: 3 0x0000000080000004 (0x00028023) mem 0x00000000800000c0
```

---

## 阶段 2: 格式修复

### T.2.1 修复 PC 输出宽度

**目标**: PC 显示为 16 位十六进制

**修改位置**: `src/core/` 相关模块

**示例**:
```rust
// 当前
println!("PC: {:#010x}", pc);

// 目标
println!("{:#018x}", pc);
```

**验证**: `0x80000000` → `0x0000000080000000`

---

### T.2.2 修复寄存器格式

**目标**: 寄存器格式对齐 Spike

**修改位置**: `src/cli.rs`, `src/core/executor.rs`

**示例**:
```rust
// 当前
println!("a0(x10)={:#x}", reg);

// 目标
println!("x10  {:#018x}", reg);
```

---

### T.2.3 添加特权模式标识

**目标**: 输出当前特权级别 (3=M-mode)

**修改位置**: `src/core/mod.rs`

**示例**:
```rust
println!("core 0: 3 {:#018x} ({:#010x})", pc, instr);
```

---

### T.2.4 字段宽度对齐

**目标**: 所有字段宽度一致

**Spike 对齐示例:**
```
core   0: 3 0x0000000080000000 (0x00000093) x1  0x0000000000000000
core   0: 3 0x0000000080000004 (0x00100113) x2  0x0000000000000001
```

---

## 阶段 3: 实现 --log-commits

### T.3.1 添加 CLI 参数

**命令**:
```rust
// src/cli.rs

.arg(Arg::new("log_commits")
    .long("log-commits")
    .value_name("FILE")
    .help("Output commit log compatible with Spike --log-commits"))
```

**验证**: `--log-commits <path>` 参数可用

---

### T.3.2 创建日志输出模块

**命令**:
```rust
// src/core/commits.rs

/// 输出与 Spike --log-commits 兼容的日志
pub fn log_commit(pc: u64, instr: u32, priv_mode: u8, regs: &[u64; 32]) {
    println!("core 0: {} {:#018x} ({:#010x})", priv_mode, pc, instr);

    // 输出变化的寄存器
    for i in 0..32 {
        if i == 0 { continue; } // x0 总是 0
        println!("core 0: {}          x{}  {:#018x}", priv_mode, i, regs[i]);
**项目:** RISC-V ISS Simulator (ruscv-sim)  
**里程碑:** M7 - Log 输出增强  
**文档版本:** 1.2  
**创建日期:** 2026-02-08  
**更新日期:** 2026-02-09  
**状态:** 实施计划（执行中）

---

## 核心理念

**先研究，再实现** - 在动手编码之前，先深入理解 Spike 日志格式规范，确保实现方向正确。

## 目标

增强 ruscv-sim 的日志输出能力，支持与 Spike 直接对比，不再依赖 RISCOF 框架。

### 研究优先原则
1. 先分析 Spike `--log-commits` 的完整输出格式
2. 再定义 ruscv-sim 的目标日志规范
3. 最后实现兼容输出模块

---

## 执行任务清单（按优先级排序）

---

## 阶段 1: 研究阶段（先做）

### T.1.1 分析 Spike `--log-commits` 输出

**命令**:
```bash
# 运行 Spike 并生成日志
docker run --rm -v $(pwd):/workdir ghcr.io/mimiqdev/riscv-dev:latest \
  spike --log-commits tests/bare-metal-riscv-test/rv64i/p/fib.elf > spike.log 2>&1

# 查看日志格式
head -50 spike.log
```

**输出示例**:
```
core   0: 3 0x0000000080000000 (0x00000093) x1  0x0000000000000000
core   0: 3 0x0000000080000004 (0x00100113) x2  0x0000000000000001
core   0: 3 0x0000000080000008 (0x00500193) x3  0x0000000000000002
core   0: 3 0x000000008000000c (0x00218463)          x4  0x0000000000000003
core   0: 3 0x0000000080000010 (0x00618633) x5  0x0000000000000004 x6  0x0000000000000005
```

**格式解析**:
- `core <hart>: <prv> <pc> (<instr_hex>) [x<reg> <value>] [mem <addr>]`
- `<prv>`: 特权级别 (3=M-mode, 1=S-mode, 0=U-mode)
- 寄存器变化在指令行后，每行一个 `xN VALUE`
- 内存访问显示 `mem <addr>`

**验证**: 理解完整格式规范

**产出**: `docs/spike-log-format.md`

---

### T.1.2 分析 ruscv-sim 当前日志格式

**当前格式**:
```
[STEP] PC: 0x80000000 -> 0x80000004, branch_taken=false, instr=0x00000093
[REGS] a0(x10)=0x0, t0(x5)=0x0, t1(x6)=0x0, t2(x7)=0x0
```

**问题**:
- 每条指令输出两条日志
- 只显示 4 个寄存器
- 格式与 Spike 不兼容

**产出**: 格式差异分析文档

---

### T.1.3 定义目标日志格式规范

**决策**: 采用与 Spike `--log-commits` 完全兼容的格式

```
# 指令行
core   0: 3 <pc> (<instr_hex>) [x<reg> <value>] [mem <addr>]

# 寄存器变化行（每行一个）
core   0: 3          x<reg> <value>
```

**产出**: `docs/ruscv-log-format.md`

---

## 阶段 2: 实现阶段（再做）

### T.2.1 添加 CLI 参数

**命令**:
```rust
// src/cli.rs

.arg(Arg::new("log_commits")
    .long("log-commits")
    .value_name("FILE")
    .help("Output commit log compatible with Spike --log-commits"))
```

**验证**: 参数编译通过

**产出**: `--log-commits <path>` 参数

---

### T.2.2 实现日志输出模块

**命令**:
```rust
// src/core/commits.rs

/// 输出与 Spike --log-commits 兼容的日志
pub fn log_commit(pc: u64, instr: u32, regs_before: &[u64; 32], regs_after: &[u64; 32]) {
    // 指令行
    println!("core 0: 3 {:#018x} ({:#010x})", pc, instr);
    
    // 输出变化的寄存器
    for i in 0..32 {
        if regs_before[i] != regs_after[i] {
            println!("core 0: 3          x{} {:#018x}", i, regs_after[i]);
        }
    }
}
```

**验证**: 编译通过，输出格式正确

**产出**: `src/core/commits.rs`

---

### T.3.3 集成到执行流程
### T.2.3 集成到执行流程

**命令**:
```rust
// 在执行循环中

for _ in 0..max_cycles {
    let result = core.step()?;

    if config.log_commits {
        core::log_commit(
            core.state.pc,
            result.instruction,
            core.state.priv_mode,
            &core.state.regs
        );
    }

    if result.halted {
        break;
    }
}
```

---

## 阶段 4: 添加内存访问日志

### T.4.1 实现内存访问追踪

**目标**: 支持 `mem <addr> [value]` 日志

**修改位置**: `src/memory/`, `src/core/executor.rs`

**示例**:
```rust
if let Some((addr, value, is_store)) = result.memory_access {
    if is_store {
        println!("core 0: {} mem {:#x} {:#x}", priv_mode, addr, value);
    } else {
        println!("core 0: {} mem {:#x}", priv_mode, addr);
    }
}
```

---

## 阶段 5: 创建对比工具

### T.5.1 Python 对比脚本

let result = core.step()?;
let regs_before = core.state.regs;

if config.log_commits {
    core::log_commit(
        core.state.pc,
        result.instruction,
        &regs_before,
        &core.state.regs
    );
}
```

**验证**: 运行时正确输出日志

**产出**: 日志输出功能集成

---

## 阶段 3: 验证阶段

### T.3.1 创建 Python 对比脚本

**命令**:
```python
#!/usr/bin/env python3
# scripts/log-compare.py

import re
from typing import Dict

class LogParser:
    PC_PATTERN = re.compile(r'core\s+\d+:\s+\d+\s+([0-9a-f]+)\s+\(([0-9a-f]+)\)')
    REG_PATTERN = re.compile(r'x(\d+)\s+([0-9a-f]+)')

    def parse(self, filename: str) -> Dict[int, Dict[str, int]]:
        """解析日志文件"""
        pc_to_regs = {}
        with open(filename) as f:
            for line in f:
                pc_match = self.PC_PATTERN.search(line)
                if pc_match:
                    pc = int(pc_match.group(1), 16)
                    pc_to_regs[pc] = {}

                reg_match = self.REG_PATTERN.search(line)
                if reg_match:
                    reg = int(reg_match.group(1))
                    val = int(reg_match.group(2), 16)
                    if pc in pc_to_regs:
                        pc_to_regs[pc][f'x{reg}'] = val
        return pc_to_regs

    def compare(self, spike: Dict, ruscv: Dict):
        """对比并返回差异"""
        diffs = []
        all_pcs = set(spike.keys()) | set(ruscv.keys())
        for pc in sorted(all_pcs):
            spike_regs = spike.get(pc, {})
            ruscv_regs = ruscv.get(pc, {})
            for reg in set(spike_regs.keys()) | set(ruscv_regs.keys()):
                s = spike_regs.get(reg)
                r = ruscv_regs.get(reg)
                if s != r:
                    diffs.append((pc, reg, s, r))
        return diffs

if __name__ == "__main__":
    import sys
    parser = LogParser()
    spike = parser.parse(sys.argv[1])
    ruscv = parser.parse(sys.argv[2])
    diffs = parser.compare(spike, ruscv)

    if not diffs:
        print("✅ No differences found!")
    else:
        print("❌ Differences:")
        for pc, reg, s, r in diffs:
            print(f"  PC {pc:#x}: {reg} = spike {s:#x}, ruscv {r:#x}")
```

import sys
import re
from typing import Dict, List, Tuple

class LogParser:
    """解析 Spike 和 ruscv-sim 日志"""

    PC_PATTERN = re.compile(r'core\s+\d+:\s+\d+\s+([0-9a-f]+)\s+\(([0-9a-f]+)\)')
    REG_PATTERN = re.compile(r'x(\d+)\s+([0-9a-f]+)')

    def parse(self, filename: str) -> Dict[int, Dict[str, int]]:
        """解析日志文件，返回 PC -> 寄存器映射"""
        pc_to_regs = {}

        with open(filename) as f:
            for line in f:
                pc_match = self.PC_PATTERN.search(line)
                if pc_match:
                    pc = int(pc_match.group(1), 16)
                    pc_to_regs[pc] = {}

                reg_match = self.REG_PATTERN.search(line)
                if reg_match:
                    reg = int(reg_match.group(1))
                    val = int(reg_match.group(2), 16)
                    if pc in pc_to_regs:
                        pc_to_regs[pc][f'x{reg}'] = val

        return pc_to_regs

    def compare(self, spike_data: Dict, ruscv_data: Dict) -> List[Tuple]:
        """对比两个日志，返回差异"""
        differences = []

        all_pcs = set(spike_data.keys()) | set(ruscv_data.keys())

        for pc in sorted(all_pcs):
            spike_regs = spike_data.get(pc, {})
            ruscv_regs = ruscv_data.get(pc, {})

            all_regs = set(spike_regs.keys()) | set(ruscv_regs.keys())
            for reg in all_regs:
                spike_val = spike_regs.get(reg)
                ruscv_val = ruscv_regs.get(reg)

                if spike_val != ruscv_val:
                    differences.append((pc, reg, spike_val, ruscv_val))

        return differences

def main():
    if len(sys.argv) != 3:
        print("Usage: log-compare.py <spike.log> <ruscv.log>")
        sys.exit(1)

    parser = LogParser()
    spike_data = parser.parse(sys.argv[1])
    ruscv_data = parser.parse(sys.argv[2])
    diffs = parser.compare(spike_data, ruscv_data)

    if not diffs:
        print("✅ No differences found!")
        return

    print("❌ Differences found:")
    for pc, reg, spike_val, ruscv_val in diffs:
        print(f"  PC {pc:#x}: {reg} = spike {spike_val:#x}, ruscv {ruscv_val:#x}")

if __name__ == "__main__":
    main()
```

**验证**: 脚本可执行，对比结果正确

**产出**: `scripts/log-compare.py`

---

### T.5.2 Shell 对比脚本

**命令**:
```bash
#!/bin/bash
# scripts/compare.sh

SPIKE_LOG=${1:-spike.log}
RUSCV_LOG=${2:-ruscv.log}
ELF=${3:-tests/bare-metal-riscv-test/rv64i/p/fib.elf}

echo "🆚 对比 Spike 和 ruscv-sim..."

# 生成 Spike 日志
docker run --rm -v $(pwd):/workdir ghcr.io/mimiqdev/riscv-dev:latest \
  spike --log-commits "$ELF" > "$SPIKE_LOG"

# 生成 ruscv-sim 日志
./target/release/ruscv-sim --elf "$ELF" --log-commits "$RUSCV_LOG"

# 对比
python3 scripts/log-compare.py "$SPIKE_LOG" "$RUSCV_LOG"
```

**产出**: `scripts/compare.sh`

---

## 阶段 6: 测试与验证

### T.6.1 格式验证

**命令**:
```bash
# 验证格式对齐
./target/release/ruscv-sim --elf tests/add.elf --log-commits ruscv.log
docker run --rm -v $(pwd):/workdir ghcr.io/mimiqdev/riscv-dev:latest \
  spike --log-commits tests/add.elf > spike.log

# 逐行对比
diff spike.log ruscv.log || true
```

---

### T.6.2 功能测试

**测试用例：**
- `add.elf` - 算术指令
- `hello.elf` - UART 输出
- `lw.elf` / `sw.elf` - 加载/存储
- `fib.elf` - 循环分支
### T.3.2 运行对比测试

**命令**:
```bash
# 对比 fib.elf
ELF=tests/bare-metal-riscv-test/rv64i/p/fib.elf
bash scripts/compare.sh

# 对比 hello.elf
ELF=tests/bare-metal-riscv-test/rv64i/p/hello.elf
bash scripts/compare.sh
```

**验证**: 无差异或差异可解释

**产出**: 对比测试报告

---

### T.3.3 更新文档

**命令**:
```bash
cat > docs/log-compare-guide.md << 'EOF'
# 日志对比指南

## 快速开始

### 1. 生成 Spike 日志
\`\`\`bash
docker run --rm -v $(pwd):/workdir ghcr.io/mimiqdev/riscv-dev:latest \
  spike --log-commits test.elf > spike.log
\`\`\`

### 2. 生成 ruscv-sim 日志
\`\`\`bash
./target/release/ruscv-sim --elf test.elf --log-commits ruscv.log
\`\`\`

### 3. 对比
\`\`\`bash
python3 scripts/log-compare.py spike.log ruscv.log
\`\`\`
EOF
```

**验证**: 文档完整可读

**产出**: `docs/log-compare-guide.md`

---

## 成功标准

- [x] 研究 Spike 日志格式（阶段 1）
- [ ] 修复 PC 和寄存器格式（阶段 2）
- [ ] 实现 `--log-commits` 参数（阶段 3）
- [ ] 添加内存访问日志（阶段 4）
- [ ] log-compare.py 脚本可用（阶段 5）
- [ ] 至少 5 个测试用例对比通过（阶段 6）

---

## 测试程序

已编译可用的 ELF 文件：
```
tests/bare-metal-riscv-test/
├── rv64i/
│   ├── p/add.elf
│   ├── p/hello.elf
│   ├── p/lw.elf
│   ├── p/sw.elf
│   └── p/fib.elf
```

---

## 参考文档

- `docs/spike-log-format.md` - Spike 日志格式分析
- `docs/ruscv-log-format.md` - ruscv-sim 目标格式
- [ ] `--log-commits` 参数可用（阶段 2）
- [ ] 日志格式与 Spike 完全兼容（阶段 2）
- [ ] log-compare.py 脚本正常工作（阶段 3）
- [ ] 至少 10 个测试用例对比通过（阶段 3）
- [ ] 文档完整，团队可复现（阶段 3）

---

## 备选方案：直接签名对比

如果日志对比太复杂，可以简化为**最终签名对比**:

```bash
# 运行 Spike（获取 exit code 或 signature）
docker run --rm -v $(pwd):/workdir ghcr.io/mimiqdev/riscv-dev:latest \
  spike pk test.elf

# 运行 ruscv-sim
./target/release/ruscv-sim --elf test.elf --signature sig.bin

# 直接对比签名文件
diff <(xxd sig.bin) <(xxd spike_sig.bin)
```

**优点**: 简单直接，无需修改日志格式
**缺点**: 无法定位具体指令差异

---

## 参考资料

### Spike 日志格式
- https://github.com/riscv-software-src/riscv-isa-sim#commit-log

### RISC-V ISA Manual
- https://github.com/riscv/riscv-isa-manual

### 项目内部文档
- `docs/dev-plan.md` - 主计划文件
