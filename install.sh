#!/usr/bin/env bash
# 软件管家 · 安装/卸载/管理脚本 (Tauri 版本)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/src-tauri/target/release/tauri-app"
TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/applications/cn.workbuddy.appsteward.desktop"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/128x128/apps"

ensure_bin() {
    if [[ ! -f "$BIN" ]]; then
        echo "未发现 release 二进制文件，正在自动编译 (pnpm tauri build)..."
        (cd "$ROOT" && pnpm tauri build --no-bundle)
    fi
}

do_uninstall() {
    echo "正在卸载软件管家启动器..."
    if [[ -f "$TARGET" ]]; then
        rm -f "$TARGET"
        echo "已移除启动器: $TARGET"
    else
        echo "未发现已安装的启动器: $TARGET"
    fi

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$(dirname "$TARGET")" >/dev/null 2>&1 || true
    fi
    echo "卸载完成。"
}

do_install() {
    echo "正在安装软件管家..."
    ensure_bin

    chmod +x "$BIN"
    mkdir -p "$(dirname "$TARGET")" "$ICON_DIR"

    # 安装应用图标
    if [[ -f "$ROOT/src-tauri/icons/128x128.png" ]]; then
        cp "$ROOT/src-tauri/icons/128x128.png" "$ICON_DIR/cn.workbuddy.appsteward.png"
    fi

    # 写入 desktop 文件
    cat <<EOF > "$TARGET"
[Desktop Entry]
Type=Application
Name=软件管家
Comment=系统垃圾清理、大文件/重复文件查找、应用强力卸载与启动项管理
Exec=$BIN
Icon=cn.workbuddy.appsteward
Terminal=false
Categories=System;Settings;Utility;
Keywords=清理;卸载;启动项;大文件;重复文件;clean;uninstall;
StartupNotify=true
EOF

    chmod +x "$TARGET"

    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "$(dirname "$TARGET")" >/dev/null 2>&1 || true
    fi

    echo "已成功安装启动器：$TARGET"
    echo "现在可在系统应用菜单搜索「软件管家」或直接执行 $BIN"
}

action="${1:-install}"

case "$action" in
    install)
        do_install
        ;;
    uninstall|remove)
        do_uninstall
        ;;
    reinstall|force-reinstall)
        echo "执行重新安装..."
        do_uninstall
        do_install
        ;;
    *)
        echo "用法: $0 [install | uninstall | reinstall]"
        exit 1
        ;;
esac

