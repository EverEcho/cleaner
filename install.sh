#!/usr/bin/env bash
# Install desktop entry on Linux; an app bundle in ~/Applications on macOS.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/src-tauri/target/release/tauri-app"
OS="$(uname -s)"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
TARGET="$DATA_DIR/applications/cn.workbuddy.appsteward.desktop"
ICON_DIR="$DATA_DIR/icons/hicolor/128x128/apps"
MAC_APP="$HOME/Applications/软件管家.app"

uninstall() {
  if [[ "$OS" == Darwin ]]; then
    if [[ -d "$MAC_APP" ]]; then
      # Keep removal reversible, and let Finder handle name collisions in Trash.
      /usr/bin/osascript -e 'on run argv' -e 'tell application "Finder" to delete POSIX file (item 1 of argv)' -e 'end run' "$MAC_APP"
    fi
  else
    rm -f "$TARGET" "$ICON_DIR/cn.workbuddy.appsteward.png"
    command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$TARGET")" >/dev/null 2>&1 || true
  fi
  echo '已移除软件管家应用入口。会话授权助手需通过 run.sh authorize uninstall 单独移除。'
}
install() {
  if [[ "$OS" == Darwin ]]; then
    (cd "$ROOT" && pnpm tauri build --bundles app)
    mkdir -p "$HOME/Applications"
    # ditto merges bundles, so remove the old bundle first by moving it to Trash.
    [[ ! -e "$MAC_APP" ]] || uninstall
    /usr/bin/ditto "$ROOT/src-tauri/target/release/bundle/macos/软件管家.app" "$MAC_APP"
    echo "已安装: $MAC_APP"
  else
    [[ -x "$BIN" ]] || (cd "$ROOT" && pnpm tauri build --no-bundle)
    mkdir -p "$(dirname "$TARGET")" "$ICON_DIR"
    cp "$ROOT/src-tauri/icons/128x128.png" "$ICON_DIR/cn.workbuddy.appsteward.png"
    # Desktop Entry quoted argument escaping, including spaces and literal %.
    local quoted_bin="$BIN"
    quoted_bin="${quoted_bin//\\/\\\\}"; quoted_bin="${quoted_bin//\"/\\\"}"
    quoted_bin="${quoted_bin//\$/\\\$}"; quoted_bin="${quoted_bin//\`/\\\`}"
    quoted_bin="${quoted_bin//%/%%}"
    cat > "$TARGET" <<ENTRY
[Desktop Entry]
Type=Application
Name=软件管家
Comment=系统清理、空间分析、应用与启动项管理
Exec="$quoted_bin"
Icon=cn.workbuddy.appsteward
Terminal=false
Categories=System;Utility;
StartupNotify=true
ENTRY
    command -v update-desktop-database >/dev/null && update-desktop-database "$(dirname "$TARGET")" >/dev/null 2>&1 || true
    echo "已安装: $TARGET"
  fi
}
case "$OS" in Linux|Darwin) ;; *) echo 'Windows 尚在规划中。'; exit 1 ;; esac
case "${1:-install}" in
  install) install ;; uninstall|remove) uninstall ;; reinstall|force-reinstall) install ;;
  help|--help|-h) echo "用法: $0 [install|uninstall|reinstall]" ;;
  *) echo '未知操作'; exit 1 ;;
esac
