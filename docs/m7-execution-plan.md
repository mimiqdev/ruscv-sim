# M7: Log 输出增强实施计划

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

### T.2.3 集成到执行流程

**命令**:
```rust
// 在执行循环中

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
