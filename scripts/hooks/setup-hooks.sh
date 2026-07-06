#!/usr/bin/env bash
# Copyright © 2026 Kirky.X
# 安装/卸载 Git Hooks（symlink 方式，Windows 兼容）
# 运行：bash scripts/hooks/setup-hooks.sh
# 卸载：bash scripts/hooks/setup-hooks.sh --uninstall

set -euo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

HOOKS_DIR=".git/hooks"
SCRIPTS_HOOKS="scripts/hooks"

# ── 卸载模式 ──────────────────────────────────
if [ "${1:-}" = "--uninstall" ]; then
    if [ -L "$HOOKS_DIR/pre-commit" ] || [ -f "$HOOKS_DIR/pre-commit" ]; then
        rm -f "$HOOKS_DIR/pre-commit"
        echo "✓ pre-commit hook 已卸载"
    else
        echo "⚠ pre-commit hook 未安装"
    fi
    exit 0
fi

echo "安装 Git Hooks..."

# ── 平台检测（Windows 需要 cp 替代 ln -sf） ──
OS="$(uname -s)"
case "$OS" in
    MINGW*|MSYS*|CYGWIN*)
        echo "  ⚠ Windows 环境检测到，使用复制模式安装"
        INSTALL_CMD="cp"
        ;;
    *)
        INSTALL_CMD="ln -sf"
        ;;
esac

# ── 检查可选工具 ─────────────────────────────
if ! cargo llvm-cov --version &>/dev/null; then
    echo "  ⚠ cargo-llvm-cov 未安装"
    echo "    安装: cargo install cargo-llvm-cov"
fi

if ! cargo audit --version &>/dev/null; then
    echo "  ⚠ cargo-audit 未安装"
    echo "    安装: cargo install cargo-audit --locked"
fi

if ! cargo deny --version &>/dev/null; then
    echo "  ⚠ cargo-deny 未安装"
    echo "    安装: cargo install cargo-deny --locked"
fi

# ── 安装 pre-commit hook ─────────────────────
if [[ -f "$SCRIPTS_HOOKS/pre-commit.sh" ]]; then
    if [ "$INSTALL_CMD" = "ln -sf" ]; then
        ln -sf "../../$SCRIPTS_HOOKS/pre-commit.sh" "$HOOKS_DIR/pre-commit"
    else
        cp "$SCRIPTS_HOOKS/pre-commit.sh" "$HOOKS_DIR/pre-commit"
    fi
    chmod +x "$HOOKS_DIR/pre-commit"

    # 安装后验证
    if [ -f "$HOOKS_DIR/pre-commit" ] && [ -x "$HOOKS_DIR/pre-commit" ]; then
        echo "✓ pre-commit hook 已安装"
    else
        echo "✗ pre-commit hook 安装失败"
        exit 1
    fi
else
    echo "✗ $SCRIPTS_HOOKS/pre-commit.sh 不存在"
    exit 1
fi

echo ""
echo "Hooks 安装完成"
