#!/bin/bash
# Install Pre-commit Hook
# =======================
# 将 pre-commit hook 安装到 .git/hooks/

set -e

PROJECT_ROOT="$(git rev-parse --show-toplevel)"
HOOK_SOURCE="$PROJECT_ROOT/scripts/hook/pre-commit"
HOOK_DEST="$PROJECT_ROOT/.git/hooks/pre-commit"

echo "🔧 Installing pre-commit hook..."
echo ""

# 检查是否存在
if [ -f "$HOOK_DEST" ]; then
    echo "⚠️  Pre-commit hook already exists."
    echo "   Backup to .git/hooks/pre-commit.bak"
    cp "$HOOK_DEST" "$HOOK_DEST.bak"
fi

# 复制 hook
cp "$HOOK_SOURCE" "$HOOK_DEST"
chmod +x "$HOOK_DEST"

echo "✅ Pre-commit hook installed!"
echo ""
echo "The hook will run before each commit:"
echo "  1. cargo fmt --check (format check)"
echo "  2. cargo check (compilation check)"
echo "  3. cargo clippy (code quality check)"
echo ""
echo "To bypass the hook, use: git commit --no-verify"
echo ""

# 提示用户设置执行权限
echo "📝 Note: The hook is executable."
echo "   If you have issues, try:"
echo "   chmod +x $HOOK_DEST"
