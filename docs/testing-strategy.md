# RISC-V 虚拟原型平台 - TDD 测试策略

**文档版本**: v2.0 (更新)  
**测试框架**: Rust built-in test + proptest  \n**目标覆盖率**: 单元测试 > 80%, 集成测试 > 70%  
**核心原则**: 纯 Rust 实现，测试策略对应调整

---

## 1. 测试金字塔

```
                    ┌─────────────────┐
                    │   系统测试       │  10%
                    │  (System Tests) │
              ┌─────┴─────┬───────────┴─────┐
              │   集成测试  │   E2E 测试      │  20%
              │  (Integration Tests)       │
        ┌─────┴─────┬─────┴───────┬─────────┴─────┐
        │  组件测试  │   API 测试  │  性能测试      │  30%
        │ (Component)                       │
  ┌─────┴─────┬─────┴─────┬─────┴───────┬─────────┴─────┐
  │  单元测试  │ Fuzz 测试 │  静态分析   │  回归测试      │  40%
  │  (Unit Tests)                                    │
  └────────────────────────────────────────────────────┘
```

### 1.1 各层测试定义

| 层级 | 比例 | 工具 | 目的 |
|------|------|------|------|
| 单元测试 | 40% | Rust `#[test]` | 验证单个函数/类 |
| 组件测试 | 20% | Rust `#[test]` | 验证模块间交互 |
| 集成测试 | 20% | Rust test + assert | 验证子系统集成 |
| 系统测试 | 10% | Rust test + QEMU | 端到端验证 |
| 性能测试 | 10% | criterion.rs | 性能基准 |

---

## 2. 测试策略

### 2.1 单元测试 (Unit Tests)

**目标**：每个核心函数和类都有对应测试

#### 2.1.1 指令测试

```rust
// tests/unit/test_instruction_add.rs

use ruscv_sim::core::{Processor, RegisterFile};\n\n// 测试 ADD 指令基本功能\n#[test]\nfn test_add_basic() {\n    let mut proc = Processor::new(0);\n    proc.reset();\n    \n    proc.set_reg(1, 10);  // x1 = 10\n    proc.set_reg(2, 20);  // x2 = 20\n    proc.set_pc(0x1000);\n    \n    // 执行 ADD x3, x1, x2\n    let raw_insn: u32 = 0x00b28333;  // ADD x3, x1, x2\n    let decoded = ruscv_sim::decode::Decoder::decode(raw_insn).unwrap();\n    \n    ruscv_sim::execute::Executor::execute(&decoded, proc.state_mut()).unwrap();\n    \n    assert_eq!(proc.state().regs()[3], 30);  // x3 = 30\n    assert_eq!(proc.pc(), 0x1004);            // PC 递增\n}\n\n// 测试 ADD 溢出（RV64 不检查溢出）\n#[test]\nfn test_add_overflow() {\n    let mut proc = Processor::new(0);\n    proc.set_reg(1, u64::MAX);\n    proc.set_reg(2, 1);\n    \n    let raw_insn: u32 = 0x00128333;\n    let decoded = ruscv_sim::decode::Decoder::decode(raw_insn).unwrap();\n    \n    ruscv_sim::execute::Executor::execute(&decoded, proc.state_mut()).unwrap();\n    \n    assert_eq!(proc.state().regs()[3], 0);  // 回绕\n}\n\n// 测试 ADD 与 x0\n#[test]\nfn test_add_with_zero() {\n    let mut proc = Processor::new(0);\n    proc.set_reg(1, 42);\n    proc.set_reg(0, 0);  // x0 恒为 0\n    \n    let raw_insn: u32 = 0x00128333;  // ADD x3, x1, x0\n    let decoded = ruscv_sim::decode::Decoder::decode(raw_insn).unwrap();\n    \n    ruscv_sim::execute::Executor::execute(&decoded, proc.state_mut()).unwrap();\n    \n    assert_eq!(proc.state().regs()[3], 42);  // 结果 = x1\n}\n```

}  // namespace test
}  // namespace rv64
```

#### 2.1.2 CSR 测试

```cpp
// tests/unit/csr/mstatus_test.cc
#include <gtest/gtest.h>
#include "core/csr/mstatus.h"

namespace rv64 {
namespace csr {
namespace test {

class MStatusTest : public ::testing::Test {
protected:
    void SetUp() override {
        csr = std::make_unique<MStatusCSR>();
    }
    
    std::unique_ptr<MStatusCSR> csr;
};

TEST_F(MStatusTest, InitialState) {
    EXPECT_EQ(csr->read(), 0x00000000);
    EXPECT_FALSE(csr->get_mie());
    EXPECT_FALSE(csr->get_mpie());
}

TEST_F(MStatusTest, SetMIE) {
    csr->set_mie(true);
    EXPECT_TRUE(csr->get_mie());
    EXPECT_TRUE(csr->read() & MSTATUS_MIE);
}

TEST_F(MStatusTest, MPPField) {
    // 测试 MPP 字段 WARL 特性
    csr->write(0x00001800);  // MPP = 3 (Machine mode)
    EXPECT_EQ(csr->get_mpp(), PRV_M);
    
    csr->write(0x00001000);  // MPP = 1 (Supervisor mode)
    EXPECT_EQ(csr->get_mpp(), PRV_S);
}

}  // namespace test
}  // namespace csr
}  // namespace rv64
```

#### 2.1.3 寄存器文件测试

```rust
// tests/unit/test_regfile.rs

use ruscv_sim::core::RegisterFile;\n\n// 测试读写功能\n#[test]\nfn test_regfile_write_read() {\n    let mut regs = RegisterFile::new();\n    regs.write(1, 0x12345678);\n    assert_eq!(regs.read(1), 0x12345678);\n}\n\n// 测试 x0 恒为 0\n#[test]\nfn test_regfile_x0_always_zero() {\n    let mut regs = RegisterFile::new();\n    regs.write(0, 0xDEADBEEF);\n    assert_eq!(regs.read(0), 0);  // x0 读回 0\n}\n\n// 测试重置\n#[test]\nfn test_regfile_reset() {\n    let mut regs = RegisterFile::new();\n    regs.write(1, 0x1234);\n    regs.reset();\n    assert_eq!(regs.read(1), 0);\n    assert_eq!(regs.read(0), 0);\n}\n```

### 2.2 组件测试 (Component Tests)

**目标**：验证模块间的交互

```rust
// tests/unit/test_decoder_executor.rs\n\nuse ruscv_sim::core::Processor;\nuse ruscv_sim::decode::Decoder;\n\n// 测试译码和执行流程\n#[test]\nfn test_decode_and_execute_add() {\n    let raw_insn: u32 = 0x00b28333;  // ADD x3, x1, x2\n    \n    // 解码\n    let decoded = Decoder::decode(raw_insn).unwrap();\n    assert_eq!(decoded.opcode, ruscv_sim::decode::Opcode::ADD);\n    assert_eq!(decoded.rd, Some(3));\n    assert_eq!(decoded.rs1, Some(1));\n    assert_eq!(decoded.rs2, Some(2));\n    \n    // 设置寄存器值\n    let mut proc = Processor::new(0);\n    proc.set_reg(1, 10);\n    proc.set_reg(2, 20);\n    \n    // 执行\n    ruscv_sim::execute::Executor::execute(&decoded, proc.state_mut()).unwrap();\n    \n    assert_eq!(proc.state().regs()[3], 30);\n}\n```
#include "core/executor.h"

namespace rv64 {
namespace test {

class DecoderExecutorTest : public ::testing::Test {
protected:
    void SetUp() override {
        decoder = std::make_unique<Decoder>();
        executor = std::make_unique<Executor>();
    }
    
    std::unique_ptr<Decoder> decoder;
    std::unique_ptr<Executor> executor;
};

TEST_F(DecoderExecutorTest, DecodeAndExecuteADD) {
    uint32_t raw_insn = 0x00b28333;  // ADD x3, x1, x2
    
    // 解码
    auto decoded = decoder->decode(raw_insn);
    ASSERT_NE(decoded, nullptr);
    EXPECT_EQ(decoded->opcode, Opcode::ADD);
    EXPECT_EQ(decoded->rd, 3);
    EXPECT_EQ(decoded->rs1, 1);
    EXPECT_EQ(decoded->rs2, 2);
    
    // 设置寄存器值
    proc->set_reg(1, 10);
    proc->set_reg(2, 20);
    
    // 执行
    executor->execute(proc.get(), *decoded);
    
    EXPECT_EQ(proc->get_reg(3), 30);
}

TEST_F(DecoderExecutorTest, DecodeAndExecuteLD) {
    uint32_t raw_insn = 0x0103c283;  // LD x1, 0(x2)
    
    auto decoded = decoder->decode(raw_insn);
    ASSERT_NE(decoded, nullptr);
    EXPECT_EQ(decoded->opcode, Opcode::LD);
    EXPECT_EQ(decoded->rd, 1);
    EXPECT_EQ(decoded->rs1, 2);
    EXPECT_EQ(decoded->imm, 0);
}

}  // namespace test
}  // namespace rv64
```

### 2.3 集成测试 (Integration Tests)

**目标**：验证完整子系统功能

```python
# tests/integration/test_elf_loading.py
import pytest
import riscv_vp

class TestELPLoading:
    """ELF 文件加载测试"""
    
    def test_load_simple_elf(self, tmp_path):
        """测试加载简单 ELF 文件"""
        sim = riscv_vp.Simulator("generic")
        
        # 编译测试程序
        test_elf = tmp_path / "test.elf"
        compile_simple_test(test_elf)
        
        # 加载 ELF
        sim.load_elf(str(test_elf))
        
        // 验证入口点
        assert sim.pc() == 0x80000000
        
        // 验证寄存器初始状态
        assert sim.regs.sp == 0x80010000
    }
    
    #[test]
    fn test_load_elf_with_data(&self, tmp_path: &TempDir) {
        // 测试加载包含数据的 ELF
        let sim = Simulator::new("generic");
        
        let test_elf = tmp_path.join("data_test.elf");
        compile_with_data(&test_elf);
        
        sim.load_elf(&test_elf).unwrap();
        
        // 验证数据段加载
        let data = sim.memory.read(0x80001000, 4);
        assert_eq!(data, 0x12345678);
    }
}
```

### 2.4 系统测试 (System Tests)

**目标**：端到端功能验证

```rust
// tests/system/test_riscv_tests.rs\n\nuse std::path::PathBuf;\nuse ruscv_sim::Simulator;\n\n// 运行 RISC-V ISA 测试\n#[test]\nfn test_rv64ui_add() {\n    let sim = Simulator::new(\"generic\");\n    sim.load_elf(\"tests/firmware/rv64ui-p-add.elf\").unwrap();\n    sim.run();\n    \n    // 测试程序在通过时 a0 = 0\n    assert_eq!(sim.regs().a0(), 0);\n}\n\n#[test]\nfn test_rv64ui_sub() {\n    let sim = Simulator::new(\"generic\");\n    sim.load_elf(\"tests/firmware/rv64ui-p-sub.elf\").unwrap();\n    sim.run();\n    \n    assert_eq!(sim.regs().a0(), 0);\n}\n\n// 参数化测试示例\nconst ISA_TESTS: &[&str] = &[\n    \"add\", \"sub\", \"and\", \"or\", \"xor\",\n    \"sll\", \"srl\", \"sra\", \"slt\", \"sltu\",\n];\n\n#[test]\nfn test_isa_tests(test_name: &str) {\n    let sim = Simulator::new(\"generic\");\n    let test_elf = format!(\"tests/firmware/rv64ui-p-{}.elf\", test_name);\n    \n    if !PathBuf::from(&test_elf).exists() {\n        return;  // 跳过不存在的测试\n    }\n    \n    sim.load_elf(&test_elf).unwrap();\n    sim.run();\n    \n    assert_eq!(sim.regs().a0(), 0);\n}\n```        
        # 编译测试
        test_elf = f"riscv-tests/isa/rv64ui-p-{test_name}.elf"
        if not os.path.exists(test_elf):
            pytest.skip(f"Test {test_name} not found")
        
        sim.load_elf(test_elf)
        sim.run()
        
        # 检查测试通过标志
        # 测试程序在通过时写入特定值到 a0
        assert sim.regs.a0 == 0, f"Test {test_name} failed"
    
    def test_privileged_mode(self):
        """测试特权模式切换"""
        sim = riscv_vp.Simulator("generic")
        
        sim.load_elf("riscv-tests/rv64mi-p-csr.elf")
        
        # 运行到结束
        sim.run(timeout=10000)
        
        assert sim.regs.a0 == 0, "CSR test failed"
```

### 2.5 性能测试 (Performance Tests)

```cpp
// tests/performance/benchmark.cc
#include <benchmark/benchmark.h>
#include "core/simulator.h"

static void BM_SimulateEmptyLoop(benchmark::State& state) {
    Simulator sim("generic");
    sim.load_elf("benchmarks/empty_loop.elf");
    
    for (auto _ : state) {
        sim.run();
    }
    
    state.SetItemsProcessed(state.iterations() * 1000000);
}
BENCHMARK(BM_SimulateEmptyLoop);

static void BM_DecodeInstructions(benchmark::State& state) {
    Decoder decoder;
    std::vector<uint32_t> instructions = {
        0x00b28333,  // ADD
        0x0013c283,  // LD
        0x0062c233,  // ADDW
        // ...
    };
    
    for (auto _ : state) {
        for (auto insn : instructions) {
            benchmark::DoNotOptimize(decoder.decode(insn));
        }
    }
}
BENCHMARK(BM_DecodeInstructions);
```

---

## 3. CI/CD 流程

### 3.1 GitHub Actions 工作流

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v4
    
    - name: Install Rust
      uses: dtolnay/rust-toolchain@stable
      with:
        toolchain: stable
        components: rustfmt, clippy
    
    - name: Cache dependencies
      uses: actions/cache@v4
      with:
        path: ~/.cargo
        key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
    
    - name: Build
      run: cargo build --release
    
    - name: Run unit tests
      run: cargo test --lib -- --test-threads=1
    
    - name: Run doc tests
      run: cargo test --doc
    
    - name: Run integration tests
      run: cargo test --test integration
    
    - name: Run system tests
      run: cargo test --test system -- --test-threads=1
    
    - name: Performance benchmarks
      if: github.ref == 'refs/heads/main'
      run: cargo bench -- --measurement-time=1
    
    - name: Upload coverage
      if: always()
      uses: codecov/codecov-action@v4
      with:
        files: ./coverage/lcov.info
```
```

### 3.2 测试执行策略

| 触发条件 | 执行测试 |
|----------|----------|
| 每次提交 | 单元测试 + 组件测试 |
| PR 合并 | 完整测试套件 |
| 每日构建 | 性能基准测试 |
| 标签发布 | 完整测试 + 覆盖率报告 |

### 3.3 质量门禁

```yaml
quality_gates:
  coverage:
    unit: 80%
    component: 70%
    integration: 50%
  
  tests:
    pass_rate: 100%
    flaky_max: 0
  
  performance:
    regression_max: 10%
```

---

## 4. 测试数据管理

### 4.1 测试固件

| 测试类型 | 固件来源 | 位置 |
|----------|----------|------|
| ISA 测试 | riscv-tests | tests/firmware/riscv-tests/ |
| 平台测试 | 自定义 | tests/firmware/platform/ |
| 性能测试 | 自定义 | tests/firmware/benchmarks/ |

### 4.2 测试配置

```yaml
# tests/config/test_config.yaml
test_suites:
  isa:
    name: "RISC-V ISA Tests"
    pattern: "rv64*-p-*.elf"
    timeout: 300
    pass_marker: "TEST_PASSED"
  
  privileged:
    name: "Privileged Tests"
    pattern: "rv64mi-p-*.elf"
    timeout: 300
  
  performance:
    name: "Performance Benchmarks"
    metrics:
      - ips (instructions per second)
      - cpi (cycles per instruction)
```

---

## 5. 测试覆盖策略

### 5.1 指令覆盖矩阵

| 指令类别 | 数量 | 测试优先级 |
|----------|------|------------|
| RV64I 基础 | 47 | P0 - 必须覆盖 |
| RV64M 乘除 | 8 | P0 - 必须覆盖 |
| RV64A 原子 | 11 | P1 - 高优先级 |
| RV64F/D 浮点 | 22 | P1 - 高优先级 |
| RV64C 压缩 | 22 | P2 - 中优先级 |

### 5.2 CSR 覆盖

| CSR 类别 | 数量 | 测试策略 |
|----------|------|----------|
| M-mode CSR | 20 | 全量测试 |
| S-mode CSR | 15 | 完整测试 |
| U-mode CSR | 5 | 基础测试 |
| 虚拟 CSR | 10 | 完整测试 |

---

## 6. 持续测试最佳实践

### 6.1 测试命名规范

```
<module>_<function>_<scenario>_<expected_result>

示例:
- decoder_decode_add_valid_success
- csr_mstatus_write_mpp_illegal_value_ignored
- regfile_x0_write_always_zero
```

### 6.2 测试数据生成

```python
# tests/utils/test_data_generator.py
class InstructionGenerator:
    """指令测试数据生成器"""
    
    @staticmethod
    def generate_add_cases():
        """生成 ADD 指令测试用例"""
        return [
            # (rs1, rs2, expected_result, description)
            (0, 0, 0, "zero + zero"),
            (1, 1, 2, "one + one"),
            (UINT64_MAX, 1, 0, "overflow"),
            (0x80000000, 0x80000000, 0, "signed overflow"),
        ]
    
    @staticmethod
    def generate_csr_cases():
        """生成 CSR 测试用例"""
        return [
            # (initial_value, write_value, expected_read, field_mask)
            (0, 0x00000001, 0x00000001, "MIE"),
            (0x00000001, 0x00000008, 0x00000009, "MIE|MPIE"),
        ]
```

---

## 7. 测试报告

### 7.1 自动化报告

- **单元测试报告**: `build/test_results/unit/`
- **集成测试报告**: `build/test_results/integration/`
- **覆盖率报告**: `build/coverage/`
- **性能报告**: `build/benchmark/`

### 7.2 关键指标

| 指标 | 目标 | 当前 |
|------|------|------|
| 单元测试覆盖率 | > 80% | - |
| 分支覆盖率 | > 70% | - |
| 测试通过率 | 100% | - |
| 测试执行时间 | < 30min | - |
