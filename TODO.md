# Sprint 8.5 TODO 列表

## 目标
推广 C 指令的模块化模式到所有指令集，建立统一的 ISA 模块结构。

## 任务清单

### 1. 规划阶段 ✅
- [x] 分析当前 C 指令组织结构
- [x] 分析现有指令集模块结构
- [x] 设计重构方案
- [x] 编写详细规划文档

### 2. RV64I 重构
- [ ] 创建 `src/isa/rv64i/` 目录结构
- [ ] 实现 `mod.rs` 模块入口
- [ ] 迁移 `r_type.rs` → `alu.rs` + `shift.rs`
- [ ] 迁移 `i_type.rs` → `load.rs` + `alu.rs` (立即数部分)
- [ ] 迁移 `s_type.rs` → `store.rs`
- [ ] 迁移 `b_type.rs` → `branch.rs`
- [ ] 迁移 `j_type.rs` → `jump.rs`
- [ ] 迁移 `u_type.rs` → `lui_auipc.rs`
- [ ] 迁移 `system.rs` → `system.rs`
- [ ] 更新 `src/isa/mod.rs` 添加 rv64i 模块

### 3. RV64M 重构
- [ ] 创建 `src/isa/rv64m/` 目录结构
- [ ] 实现 `mod.rs` 模块入口
- [ ] 迁移 `mul.rs` → `mul.rs`
- [ ] 迁移 `div.rs` → `div.rs`
- [ ] 更新 `src/isa/mod.rs` 添加 rv64m 模块

### 4. RV64A 重构
- [ ] 创建 `src/isa/rv64a/` 目录结构
- [ ] 实现 `mod.rs` 模块入口
- [ ] 迁移 `lr_sc.rs` → `lr_sc.rs`
- [ ] 迁移 `amo.rs` → `amo.rs`
- [ ] 更新 `src/isa/mod.rs` 添加 rv64a 模块

### 5. RV64F 重构
- [ ] 创建 `src/isa/rv64f/` 目录结构
- [ ] 实现 `mod.rs` 模块入口
- [ ] 迁移 `f_arith.rs` → `arith.rs`
- [ ] 迁移 `f_load_store.rs` → `load_store.rs`
- [ ] 迁移 `f_compare.rs` → `compare.rs`
- [ ] 迁移 `f_convert.rs` → `convert.rs`
- [ ] 迁移 `f_classify.rs` → `classify.rs`
- [ ] 迁移 `f_div_sqrt.rs` → `div_sqrt.rs`
- [ ] 迁移 `f_madd.rs` → `madd.rs`
- [ ] 更新 `src/isa/mod.rs` 添加 rv64f 模块

### 6. RV64D 重构
- [ ] 创建 `src/isa/rv64d/` 目录结构
- [ ] 实现 `mod.rs` 模块入口
- [ ] 迁移 `d_arith.rs` → `arith.rs`
- [ ] 迁移 `d_load_store.rs` → `load_store.rs`
- [ ] 迁移 `d_compare.rs` → `compare.rs`
- [ ] 迁移 `d_convert.rs` → `convert.rs`
- [ ] 迁移 `d_classify.rs` → `classify.rs`
- [ ] 迁移 `d_div_sqrt.rs` → `div_sqrt.rs`
- [ ] 迁移 `d_madd.rs` → `madd.rs`
- [ ] 更新 `src/isa/mod.rs` 添加 rv64d 模块

### 7. 兼容层更新
- [ ] 更新 `src/execute/mod.rs` 添加 re-exports
- [ ] 验证所有现有 API 可用
- [ ] 验证测试无需修改即可通过

### 8. 验证与清理
- [ ] 运行 `cargo fmt` 格式化
- [ ] 运行 `cargo clippy` 检查
- [ ] 运行 `cargo test` 验证所有测试
- [ ] 更新相关文档
- [ ] 删除旧文件

## 验收标准

1. [ ] 项目可成功构建 (`cargo build`)
2. [ ] 所有单元测试通过 (`cargo test --lib`)
3. [ ] 所有集成测试通过 (`cargo test --test '*'`)
4. [ ] `cargo fmt` 检查通过
5. [ ] `cargo clippy` 检查通过
6. [ ] API 保持向后兼容

## 参考文档

- 详细规划文档: `docs/sprint-8.5-plan.md`
- C 指令模块模式: `src/isa/rv64c/`
