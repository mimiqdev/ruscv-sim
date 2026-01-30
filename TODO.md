# Sprint 1 TODO 列表

## 目标
完成项目初始化和基础 ISS 框架，支持 RV32I 核心指令。

## 任务清单

### 1. 项目初始化 ✅
- [x] 创建项目目录
- [x] 初始化 Rust 项目
- [x] 配置 Cargo.toml 依赖
- [x] 创建模块结构

### 2. 核心框架
- [x] 实现 `CoreState` 核心状态结构
- [x] 实现 `RiscvCore` 核心执行引擎
- [x] 实现 `step()` 单步执行
- [ ] 实现 `run()` 循环执行
- [ ] 实现异常处理

### 3. 指令译码
- [x] 实现 `InstructionDecoder`
- [x] 支持 R/I/S/B/U/J 格式
- [x] 实现所有 RV32I 操作码
- [ ] 添加指令验证
- [ ] 优化译码性能

### 4. 指令执行
- [x] 实现 `Executor`
- [x] 实现算术/逻辑指令 (ADD, SUB, AND, OR, XOR, SLL, SRL, SRA)
- [x] 实现立即数指令
- [x] 实现加载/存储指令
- [x] 实现分支指令
- [x] 实现跳转指令
- [ ] 实现 CSR 指令
- [ ] 实现系统指令 (ECALL, EBREAK)

### 5. 存储器接口
- [x] 实现 `MemoryInterface` Trait
- [x] 实现 `SimpleMemory`
- [x] 支持字节/半字/字访问
- [ ] 实现对齐检查
- [ ] 实现内存映射 I/O

### 6. TLM2.0 接口
- [x] 实现 `TlmInterface`
- [x] 实现 `TlmGenericPayload`
- [x] 实现 `TlmSimpleMemory`
- [ ] 实现阻塞传输 (b_transport)
- [ ] 实现非阻塞传输 (nb_transport)
- [ ] 实现时间建模

### 7. 测试覆盖
- [x] 单元测试框架
- [x] 译码测试
- [x] 执行测试
- [x] 存储器测试
- [ ] TLM 测试
- [ ] 集成测试

### 8. 文档和演示
- [x] README.md
- [x] 代码注释
- [ ] API 文档
- [ ] 原型演示程序

## 验收标准

1. 项目可成功构建 (`cargo build`)
2. 所有单元测试通过 (`cargo test`)
3. 可执行简单测试程序 (LUI, ADD 等)
4. 代码覆盖率 > 80%

## 下一阶段规划

- Sprint 2: 完整 RV32I 实现 + 性能优化
- Sprint 3: 中断/异常处理
- Sprint 4: 外设模型集成
