#!/bin/bash
# Install Git Hooks
# =================
# 将 hooks 安装到 .githooks/（与 git config hooksPath 一致）

set -e

PROJECT_ROOT="$(git rev-parse --show-toplevel)"

echo "🔧 Installing git hooks..."
echo ""

# 安装 pre-commit
PRE_COMMIT_SRC="$PROJECT_ROOT/scripts/hook/pre-commit"
PRE_COMMIT_DEST="$PROJECT_ROOT/.githooks/pre-commit"

if [ -f "$PRE_COMMIT_DEST" ]; then
    echo "⚠️  Pre-commit hook exists. Backup to .githooks/pre-commit.bak"
    cp "$PRE_COMMIT_DEST" "$PRE_COMMIT_DEST.bak"
fi
cp "$PRE_COMMIT_SRC" "$PRE_COMMIT_DEST"
chmod +x "$PRE_COMMIT_DEST"
echo "✅ Pre-commit hook installed"

# 安装 pre-push
PRE_PUSH_SRC="$PROJECT_ROOT/scripts/hook/pre-push"
PRE_PUSH_DEST="$PROJECT_ROOT/.githooks/pre-push"

if [ -f "$PRE_PUSH_DEST" ]; then
    echo "⚠️  Pre-push hook exists. Backup to .githooks/pre-push.bak"
    cp "$PRE_PUSH_DEST" "$PRE_PUSH_DEST.bak"
fi
cp "$PRE_PUSH_SRC" "$PRE_PUSH_DEST"
chmod +x "$PRE_PUSH_DEST"
echo "✅ Pre-push hook installed"

echo ""
echo "Installed hooks (in .githooks/):"
echo "  pre-commit: cargo fmt + cargo check"
echo "  pre-push:   cargo clippy (strict)"
echo ""
echo "To bypass hooks:"
echo "  git commit --no-verify"
echo "  git push --no-verify"
