# M7: RISCOF + Spike 集成实施计划

**项目:** RISC-V ISS Simulator (ruscv-sim)  
**里程碑:** M7 - RISCOF + arch-test 集成  
**文档版本:** 1.0  
**创建日期:** 2026-02-05  
**状态:** 实施计划（执行中）

---

## 第一部分：执行任务清单

---

## 阶段 1: 环境准备

### T.1.1 安装 RISCOF

**目标**：通过 Docker 运行 RISCOF 测试框架

**命令**：
```bash
# 使用 Docker 运行 RISCOF
docker run --rm ghcr.io/riscv-software-src/riscof:latest riscof --version

# 创建别名方便使用（可选）
alias riscof='docker run --rm -v $(pwd):/workdir -w /workdir ghcr.io/riscv-software-src/riscof:latest'
```

**验证**：执行返回版本号且无报错

**产出**：`riscof` 命令可通过 Docker 使用

---

### T.1.2 安装 Spike

**目标**：通过 Docker 运行 Spike 参考模拟器

**命令**：
```bash
# 使用 Docker 运行 Spike
docker run --rm ghcr.io/riscv-software-src/riscv-isa-sim:latest spike --version

# 创建别名方便使用（可选）
alias spike='docker run --rm -v $(pwd):/workdir -w /workdir ghcr.io/riscv-software-src/riscv-isa-sim:latest spike'

# 对于需要 pk 的场景
alias spike-pk='docker run --rm -v $(pwd):/workdir -w /workdir ghcr.io/riscv-software-src/riscv-isa-sim:latest spike'
```

**验证**：执行返回版本号且无报错

**产出**：`spike` 命令可通过 Docker 使用

---

### T.1.3 获取 riscv-arch-test 套件

**目标**：下载官方架构测试套件

**命令**：
```bash
# 克隆仓库
git clone https://github.com/riscv-software-src/riscv-arch-test.git
cd riscv-arch-test

# 查看测试套件结构
ls -la
```

**验证**：目录存在且包含 `riscv-test-suite` 等子目录

**产出**：`riscv-arch-test/` 目录，包含测试用例

---

## 阶段 2: RISCOF 集成

### T.2.1 创建 ruscv-sim DUT 配置文件

**目标**：编写 RISCOF 所需的 DUT YAML 配置

**命令**：
```bash
# 创建配置目录和文件
mkdir -p scripts/riscof

# 创建 dut.yaml
cat > scripts/riscof/ruscv-sim-dut.yaml << 'EOF'
name: ruscv-sim
workdir: ./work/ruscv-sim
isa: RV64IMAFDC
device: rv64gc_ssaia

runcmd: |
  ./target/release/ruscv-sim \
    --elf {testfile} \
    --signature {signature} \
    --exit-code {exitcode} \
    --priv-mode {priv}

signature: |
  --signature {signature}

exit_code: |
  --exit-code {exitcode}
EOF
```

**验证**：文件存在且 YAML 语法正确

**产出**：`scripts/riscof/ruscv-sim-dut.yaml`

---

### T.2.2 适配签名导出接口

**目标**：实现 `--signature` 命令行参数

**命令**：
```bash
# 在 ruscv-sim 项目中添加参数
# 编辑 src/cli.rs 或 src/main.rs

# 验证参数是否存在
./target/release/ruscv-sim --help | grep signature
```

**验证**：`--signature` 参数可用

**产出**：`--signature <path>` 参数支持

---

### T.2.3 适配退出码机制

**目标**：实现 `--exit-code` 命令行参数和 tohost 机制

**命令**：
```bash
# 在 ruscv-sim 项目中实现退出码导出
# 添加 --exit-code <path> 参数，将退出码写入文件

# 测试退出码导出
echo "0" > exit_code.txt
./target/release/ruscv-sim --elf test.elf --exit-code exit_code.txt
cat exit_code.txt
```

**验证**：退出码正确写入指定文件

**产出**：`--exit-code <path>` 参数支持

---

### T.2.4 运行首批 RV64I 基础测试

**目标**：执行 RISCOF 测试，验证集成

**命令**：
```bash
# 进入 riscv-arch-test 目录
cd riscv-arch-test

# 使用 Docker 运行 RISCOF
docker run --rm \
    -v $(pwd):/workdir \
    -v $(pwd)/../ruscv-sim:/dut \
    ghcr.io/riscv-software-src/riscof:latest \
    riscof --suite riscv-tests --workdir ./work \
    --dut-yaml /dut/scripts/riscof/ruscv-sim-dut.yaml \
    --no-ref-model

# 查看测试报告
cat ./work/report.html
```

**验证**：测试执行完成，生成 HTML 报告

**产出**：`work/report.html` 测试报告

---

### T.2.5 调试失败的测试用例

**目标**：分析失败原因并修复

**命令**：
```bash
# 查看详细日志
cat ./work/logs/*.log

# 手动执行单个测试
./target/release/ruscv-sim --elf test.elf --signature sig.bin

# 使用 Docker 运行 Spike 进行比对
docker run --rm -v $(pwd):/workdir ghcr.io/riscv-software-src/riscv-isa-sim:latest spike pk test.elf

# 比对结果
diff sig.bin spike_sig.bin
```

**验证**：失败用例数量减少或归零

**产出**：问题修复记录

---

## 阶段 3: Spike 集成

### T.3.1 研究 Spike 内存映射配置

**目标**：了解 Spike 默认内存布局

**命令**：
```bash
# 查看 Spike 帮助
docker run --rm ghcr.io/riscv-software-src/riscv-isa-sim:latest spike --help

# 运行测试并查看内存映射
docker run --rm -v $(pwd):/workdir ghcr.io/riscv-software-src/riscv-isa-sim:latest spike --debug pk hello 2>&1 | head -100

# 文档参考
cat riscv-isa-sim/docs/spike-dpi.md 2>/dev/null || true
```

**验证**：记录 Spike 默认内存映射

**产出**：`docs/spike-memory-map.md`

---

### T.3.2 实现 Spike 兼容运行模式

**目标**：创建 ruscv-sim 的 Spike 兼容模式

**命令**：
```bash
# 创建兼容模块
cat > src/spike_compat.rs << 'EOF'
/// Spike 兼容运行配置
pub struct SpikeCompatConfig {
    pub memory_map: MemoryMap,
    pub strict_peripherals: bool,
    pub verbose: bool,
}

/// 执行结果导出
pub struct ExecutionSnapshot {
    pub pc: u64,
    pub registers: [u64; 32],
    pub csrs: HashMap<CsrAddr, u64>,
    pub memory: MemorySnapshot,
    pub exit_code: i32,
    pub cycles: u64,
}

impl Simulator {
    pub fn run_spike_compat(
        &self,
        elf: &Path,
        config: SpikeCompatConfig,
    ) -> Result<ExecutionSnapshot, SimError> {
        todo!()
    }
}
EOF

# 编译测试
cargo build
```

**验证**：代码编译通过

**产出**：`src/spike_compat.rs`

---

### T.3.3 实现状态导出接口

**目标**：添加 `--dump-state` 参数

**命令**：
```bash
# 在 CLI 中添加参数
# 编辑 src/cli.rs，添加：

#.arg(Arg::new("dump_state")
#    .long("dump-state")
#    .value_name("PATH")
#    .help("Export execution state to JSON file"))

# 验证参数
./target/release/ruscv-sim --help | grep dump-state
```

**验证**：`--dump-state <path>` 参数可用

**产出**：`--dump-state <path>` 参数支持

---

### T.3.4 开发 Spike 比对脚本

**目标**：编写 Python 比对脚本

**命令**：
```bash
# 创建比对脚本
cat > scripts/spike/compare.py << 'EOF'
#!/usr/bin/env python3
"""Spike vs ruscv-sim 执行结果比对工具"""

import subprocess
import json
import sys
import os

DOCKER_SPIKE_CMD = [
    "docker", "run", "--rm",
    "-v", f"{os.getcwd()}:/workdir",
    "-w", "/workdir",
    "ghcr.io/riscv-software-src/riscv-isa-sim:latest",
    "spike"
]

def run_spike(elf, output):
    """运行 Spike 并导出状态"""
    cmd = DOCKER_SPIKE_CMD + [
        "--log", output + "_spike.log",
        "pk", elf
    ]
    subprocess.run(cmd, check=True)

def run_ruscv(elf, output):
    """运行 ruscv-sim 并导出状态"""
    cmd = [
        "./target/release/ruscv-sim",
        "--elf", elf,
        "--dump-state", output + "_ruscv.json"
    ]
    subprocess.run(cmd, check=True)

def compare_states(spike_state, ruscv_state):
    """比对执行状态"""
    # 比对寄存器
    for i in range(32):
        if spike_state['regs'][i] != ruscv_state['regs'][i]:
            print(f"Register x{i} mismatch: {spike_state['regs'][i]} vs {ruscv_state['regs'][i]}")
            return False
    return True

if __name__ == "__main__":
    elf = sys.argv[1]
    run_spike(elf, "/tmp/spike")
    run_ruscv(elf, "/tmp/ruscv")
    # 执行比对...
EOF

chmod +x scripts/spike/compare.py
```

**验证**：脚本可执行，比对功能正常

**产出**：`scripts/spike/compare.py`

---

### T.3.5 验证 Spike 比对结果

**目标**：运行比对测试，验证正确性

**命令**：
```bash
# 运行比对测试
python3 scripts/spike/compare.py tests/simple/test.elf

# 查看比对报告
cat /tmp/compare_report.txt
```

**验证**：比对完成，无状态差异

**产出**：`/tmp/compare_report.txt` 比对报告

---

## 阶段 4: 验证与优化

### T.4.1 运行完整测试套件

**目标**：执行所有 riscv-arch-test 测试用例

**命令**：
```bash
# 使用 Docker 运行完整测试套件
docker run --rm \
    -v $(pwd):/workdir \
    ghcr.io/riscv-software-src/riscof:latest \
    riscof --suite riscv-arch-test \
    --workdir ./work/full \
    --dut-yaml scripts/riscof/ruscv-sim-dut.yaml

# 生成覆盖率报告
docker run --rm \
    -v $(pwd):/workdir \
    ghcr.io/riscv-software-src/riscof:latest \
    riscof report --workdir ./work/full \
    --output ./coverage_report.html
```

**验证**：测试套件执行完成，生成覆盖率报告

**产出**：`work/full/report.html` 和覆盖率报告

---

### T.4.2 记录测试结果

**目标**：整理测试结果文档

**命令**：
```bash
# 提取测试统计
cat work/full/report.json | jq '.summary'

# 创建结果摘要
cat > m7-test-results.md << 'EOF'
# M7 测试结果摘要

## 测试统计
- 总测试数：XXX
- 通过：XXX
- 失败：XXX
- 通过率：XX%

## 失败用例分析
| 用例 | 原因 | 状态 |
|------|------|------|
| xxx | xxx | 待修复 |
EOF
```

**验证**：结果文档生成

**产出**：`m7-test-results.md`

---

### T.4.3 更新项目文档

**目标**：完善 M7 集成文档

**命令**：
```bash
# 更新目录结构和文档
cat > docs/m7-riscof-integration.md << 'EOF'
# RISCOF + Spike 集成指南

## 快速开始

### 1. 环境准备
\`\`\`bash
# RISCOF（通过 Docker）
docker run --rm ghcr.io/riscv-software-src/riscof:latest riscof --version

# Spike（通过 Docker）
docker run --rm ghcr.io/riscv-software-src/riscv-isa-sim:latest spike --version
\`\`\`

### 2. 运行测试
\`\`\`bash
docker run --rm \
    -v $(pwd):/workdir \
    ghcr.io/riscv-software-src/riscof:latest \
    riscof --suite riscv-tests --workdir ./work \
    --dut-yaml scripts/riscof/ruscv-sim-dut.yaml
\`\`\`

### 3. Spike 比对
\`\`\`bash
python3 scripts/spike/compare.py <test-elf>
\`\`\`
EOF
```

**验证**：文档更新完成

**产出**：`docs/m7-riscof-integration.md`

---

## 第二部分：参考资料

---

## 技术决策说明

### Golden Model 选择：Spike vs Sail

**决策：使用 Spike**

| 维度 | Spike | Sail |
|------|-------|------|
| 官方支持 | RISC-V 官方 | Cambridge/Arm |
| 执行速度 | 快 (2-5x) | 较慢 |
| 集成难度 | 简单 | 较复杂 |
| 生态成熟度 | 非常成熟 | 一般 |

**理由**：Spike 生态成熟、速度快、集成简单，满足 M7 功能验证需求。

### 安装方式：Docker vs 直接安装

**决策：使用 Docker**

- 避免 host 环境依赖冲突
- 便于在不同机器上复现一致的环境
- 简化 CI/CD 集成流程
- 可选：创建 wrapper 脚本简化日常使用

---

## ruscv-sim 所需接口

### 必要接口

| 接口 | 功能 | 命令行参数 |
|------|------|-----------|
| 启动接口 | 接收测试 ELF，执行并返回 | `--elf <path>` |
| 签名导出 | 导出测试签名区域 | `--signature <path>` |
| 退出码 | 反馈测试执行状态 | `--exit-code <path>` |
| 状态导出 | 导出执行后状态 (JSON) | `--dump-state <path>` |
| Spike 兼容模式 | 以 Spike 兼容方式运行 | `--spike-compat` |
| 比对模式 | 与 Spike 结果比对 | `--compare-spike` |

### DUT YAML 配置示例

```yaml
name: ruscv-sim
workdir: ./work/ruscv-sim
isa: RV64IMAFDC
device: rv64gc_ssaia

runcmd: |
  ./target/release/ruscv-sim \
    --elf {testfile} \
    --signature {signature} \
    --exit-code {exitcode}
```

---

## Spike vs ruscv-sim 比对维度

1. **寄存器状态**
   - General Purpose (x0-x31)
   - CSR 寄存器
   - PC 寄存器

2. **内存状态**
   - 加载/存储地址
   - 数据值

3. **异常/中断行为**
   - 异常类型
   - 异常原因 (mcause)
   - 异常地址 (mepc, mtval)

4. **执行终止状态**
   - 退出码
   - 终止指令

---

## 成功标准

- [ ] RISCOF 框架可正常运行
- [ ] 至少 50 个 riscv-arch-test 测试用例通过
- [ ] Spike 比对脚本可正常工作
- [ ] 所有失败的测试用例有明确的分析和记录
- [ ] 集成文档完整，团队可复现

---

## 参考链接

### 官方文档

- **RISCOF**: https://riscof.readthedocs.io/
- **riscv-arch-test**: https://github.com/riscv-software-src/riscv-arch-test
- **Spike**: https://github.com/riscv-software-src/riscv-isa-sim
- **RISC-V ISA Manual**: https://github.com/riscv/riscv-isa-manual

### 项目内部文档

- **Spike 文档**: `docs/spike-dpi.md`
- **测试策略**: `docs/testing-strategy.md`
- **ELF 执行**: `docs/dev-plan.md` M5 部分

### 社区资源

- **RISC-V Discord**: #verification 频道
- **RISC-V Forum**: https://forum.riscv.org/

---

## 术语表

| 术语 | 定义 |
|------|------|
| DUT | Design Under Test，被测试的硬件/软件设计 |
| ISA | Instruction Set Architecture，指令集架构 |
| Signature | 测试结束时导出的处理器状态快照 |
| tohost | RISCV-Tools 约定的测试完成标记地址 |
| RV64IMAFDC | RISC-V 64位基础+整数乘除+原子+单双精度浮点+压缩指令 |

---

## 目录结构

```
ruscv-sim/
├── scripts/
│   ├── riscof/
│   │   └── ruscv-sim-dut.yaml    # DUT 配置文件
│   └── spike/
│       └── compare.py             # 比对脚本
├── tests/
│   ├── riscv-arch-test/          # arch-test 测试套件
│   └── spike-compare/            # Spike 比对测试
├── src/
│   └── spike_compat.rs           # Spike 兼容模块
└── docs/
    └── m7-riscof-integration.md  # 集成文档
```
