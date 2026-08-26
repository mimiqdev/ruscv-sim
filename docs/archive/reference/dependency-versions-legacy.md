# 已归档的依赖版本清单

> **状态：** 历史快照。当前 Rust 依赖以 `Cargo.toml` 和 `Cargo.lock` 为准，外部工具版本必须由具体里程碑固定。

**文档版本**: v1.0  
**更新日期**: 2026-01-30  
**查询方法**: 官方文档查询 + 本地环境查询

---

## 1. Rust 语言

| 组件 | 文档旧版本 | 当前最新 | 查询来源 |
|------|------------|----------|----------|
| Rust (stable) | 1.80+ | **1.93.0** | `rustc --version` (2026-01-30) |
| Cargo | 1.80+ | **1.93.0** | `cargo --version` (2026-01-30) |
| Edition | 2024 | 2024 | Rust 官方 |

**Rust 1.93.0 发布日期**: 2026-01-19  
**查询命令**: `rustc --version`

---

## 2. Rust Crates

| Crate | 文档旧版本 | 当前最新 | 查询来源 |
|-------|------------|----------|----------|
| anyhow | 1.0 | **1.0.100** | crates.io |
| thiserror | 2.0 | **2.0.18** | crates.io |
| log | 0.4 | **0.4.29** | crates.io |
| env_logger | 0.11 | **0.11.8** | crates.io |
| serde | 1.0 | **1.0.228** | crates.io |
| serde_yaml | 0.9 | **0.9.34** | crates.io |
| tokio | 1.0 | **1.49.0** | crates.io |
| proptest | 1.0 | **1.9.0** | crates.io |
| criterion | 0.5 | **0.8.1** | crates.io |
| rand | 0.8 | **0.9.0** | crates.io |
| hex | 0.4 | **0.5.0** | crates.io |
| num | 0.4 | **0.5.0** | crates.io |
| num-traits | 0.2 | **0.2.19** | crates.io |
| pyo3 | 0.20 | **0.22.0** | crates.io (Python 绑定) |
| cxx | 1.0 | **1.0.130** | crates.io (C++ FFI) |

---

## 3. Python 依赖

| 组件 | 建议版本 | 说明 |
|------|----------|------|
| pytest | 8.0+ | 测试框架 |

---

## 4. 开发工具

| 工具 | 建议版本 | 说明 |
|------|----------|------|
| GitHub Actions | latest | checkout@v4, setup-rust@v5 |
| mdBook | 0.4 | 文档生成 |

---

## 5. 版本更新策略

### 5.1 Rust 版本策略

**推荐策略**: 
- 使用 **stable** 分支
- 每月第一个工作日检查更新
- 重大版本升级（如 1.93 → 2.0）需要完整回归测试

**禁止**: 
- 使用 **nightly** 特性（可能导致编译失败）

### 5.2 Crates 版本策略

**更新检查**:
```bash
cargo outdated --depth 1
```

**更新原则**:
- Patch 版本（x.y.Z）: 可直接更新
- Minor 版本（x.Y.z）: 评估兼容性后更新
- Major 版本（X.y.z）: 需要完整测试

---

## 6. 版本兼容性矩阵

| Rust 版本 | 最低兼容 Crates 版本 |
|-----------|---------------------|
| 1.80 | 全部兼容 |
| 1.90 | 全部兼容 |
| 1.93 | 全部兼容 |

---

## 7. 更新日志

| 日期 | 更新内容 | 操作者 |
|------|----------|--------|
| 2026-01-30 | 初始版本，记录 Rust 1.93.0 及核心 Crates 版本 | 代欧奇希斯 |

---

## 8. 相关文档

- `docs/sprint-plan.md` - 开发计划中的版本要求
- `docs/architecture.md` - 架构设计中的依赖说明
- `Cargo.toml` - 项目实际依赖配置

---

## 9. 查询命令

```bash
# Rust 版本
rustc --version

# Cargo 版本
cargo --version

# 检查依赖更新
cargo outdated --depth 1

# 检查特定 crate 版本
cargo search anyhow --limit 1
```
