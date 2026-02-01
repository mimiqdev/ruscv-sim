# Sprint 8.5 任务卡片: RV64C 模块结构重构

## 任务信息

| 属性 | 值 |
|------|-----|
| **Sprint** | 8.5 |
| **任务类型** | Refactor |
| **优先级** | Medium |
| **预估工时** | 5 天 |
| **故事点** | 5 |

## 任务描述

重构 RV64C 压缩指令模块的文件组织结构，从当前基于象限的分组改为基于功能的分组，以提高代码可维护性和与代码生成工具的兼容性。

## 验收标准

### 必须完成 (MVP)
- [ ] 新模块结构实现完成（7个功能模块）
- [ ] 消除 `exec_c_slli` 代码重复
- [ ] 所有现有测试通过
- [ ] 公开 API 保持不变（向后兼容）
- [ ] 代码通过 clippy 检查

### 建议完成
- [ ] 更新模块文档
- [ ] 更新架构文档
- [ ] 创建重构总结报告

## 依赖关系

### 前置依赖
- 无（这是独立的重构任务）

### 后置任务
- Sprint 9: RV64C Codegen 集成
- Sprint 10: RV64C 完整测试套件

## 技术细节

### 当前结构问题
```
src/isa/rv64c/
├── c0_quadw.rs      # 命名不清晰
├── c1_addiw.rs      # 过于具体
├── c1_arith.rs      # 包含多种功能
├── c1_shift.rs      # 与 c2_stack.rs 重复
├── c2_move.rs       # 功能混杂
└── c2_stack.rs      # 包含非栈指令
```

### 目标结构
```
src/isa/rv64c/
├── decoder.rs       # 统一解码器
├── memory.rs        # Load/Store
├── arithmetic.rs    # 算术运算
├── immediate.rs     # 立即数
├── logic.rs         # 逻辑运算
├── shift.rs         # 移位（去重）
├── branch.rs        # 分支跳转
└── system.rs        # 系统指令
```

## 子任务分解

### Day 1: 准备与框架
- [ ] 创建功能分支
- [ ] 创建新文件框架
- [ ] 更新 mod.rs 引入新模块
- [ ] 编译验证

### Day 2: 核心功能迁移
- [ ] 迁移 memory.rs (Load/Store)
- [ ] 迁移 shift.rs (去重)
- [ ] 运行测试验证

### Day 3: 算术与逻辑
- [ ] 迁移 arithmetic.rs
- [ ] 迁移 immediate.rs
- [ ] 迁移 logic.rs
- [ ] 运行测试验证

### Day 4: 其他功能
- [ ] 迁移 branch.rs
- [ ] 迁移 system.rs
- [ ] 更新 decoder.rs
- [ ] 完整测试验证

### Day 5: 清理与文档
- [ ] 删除旧文件
- [ ] 最终清理
- [ ] 更新文档
- [ ] PR 准备

## 相关资源

- [详细规划文档](./sprint-8.5-rv64c-refactor-plan.md)
- [重构映射表](./rv64c-refactor-mapping.md)
- [参考实现示例](./rv64c-new-structure-example.md)

## 备注

- 这是一个纯重构任务，不涉及功能变更
- 需要特别注意测试的完整迁移
- 保持向后兼容是关键
