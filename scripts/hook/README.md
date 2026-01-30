# Git Hooks for RISC-V ISS Simulator

## 概述

本目录包含用于保证代码质量的 Git hooks。

## 快速开始

### 安装 Hook

```bash
# 在项目根目录下执行
bash scripts/hook/install.sh
```

### 手动安装

```bash
# 或者直接复制 hook
cp scripts/hook/pre-commit .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

## Hook 功能

### Pre-commit Hook (`pre-commit`)

在每次 `git commit` 前自动运行：

| 步骤 | 检查项 | 说明 |
|------|--------|------|
| 1 | `cargo fmt --check` | 代码格式化检查，自动修复 |
| 2 | `cargo check` | 编译检查 |
| 3 | `cargo clippy` | 代码质量检查 (警告视为错误) |

## 跳过 Hook

如果需要跳过 pre-commit 检查：

```bash
git commit --no-verify -m "Your message"
# 或
git commit -m "Your message" -n
```

## 文件结构

```
scripts/hook/
├── pre-commit      # 主 hook 脚本
├── install.sh      # 安装脚本
└── README.md       # 本文档
```

## 依赖

- Rust 1.80+
- Cargo
- rustfmt 组件
- clippy 组件

## CI 集成

GitHub Actions CI 也会运行相同的检查，确保本地和 CI 环境一致。

## 自定义

### 修改检查项

编辑 `pre-commit` 文件，可以：

- 添加/移除检查步骤
- 修改 clippy 警告级别
- 添加其他 linter（如 cargo-udeps）

### 示例：添加 cargo-udeps

```bash
# 在 pre-commit 中添加
echo "────────────────────────────────────────"
echo "Step 4/4: Checking for unused dependencies..."
echo "────────────────────────────────────────"
if cargo +nightly udeps --all-targets &> /dev/null; then
    echo -e "${GREEN}✅ No unused dependencies${NC}"
else
    echo -e "${YELLOW}⚠️  Unused dependencies found${NC}"
fi
```
