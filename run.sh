#!/usr/bin/env bash
# Software Manager: common Linux / macOS development and run entry.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/src-tauri/target/release/tauri-app"
cd "$ROOT"
case "$(uname -s)" in Linux|Darwin) ;; *) echo '当前实现支持 Linux 和 macOS；Windows 尚在规划中。'; exit 1 ;; esac
ensure_bin() { [[ -x "$BIN" ]] || pnpm tauri build --no-bundle; }
run_direct() { ensure_bin; exec "$BIN" "$@"; }
run_installed() {
  "$ROOT/install.sh" "${1:-install}"
  if [[ "$(uname -s)" == Darwin ]]; then exec open "$HOME/Applications/软件管家.app"; fi
  run_direct
}
action="${1:-menu}"
[[ $# -eq 0 ]] || shift
case "$action" in
  run|direct|start) run_direct "$@" ;;
  dev) exec pnpm tauri dev "$@" ;;
  build) exec pnpm tauri build "$@" ;;
  install) run_installed install ;;
  reinstall|force) run_installed reinstall ;;
  uninstall) exec "$ROOT/install.sh" uninstall ;;
  authorize) exec "$ROOT/scripts/install-authorization.sh" "$@" ;;
  menu)
    echo '软件管家 · Linux / macOS'
    echo '1) 直接运行  2) 安装并运行  3) 重新安装  4) 开发模式  5) 卸载入口  6) 配置会话授权缓存  0) 退出'
    read -rp '请选择 [默认 1]: ' choice
    case "${choice:-1}" in
      1) run_direct ;; 2) run_installed install ;; 3) run_installed reinstall ;;
      4) exec pnpm tauri dev ;; 5) exec "$ROOT/install.sh" uninstall ;;
      6) exec "$ROOT/scripts/install-authorization.sh" ;; 0) exit 0 ;;
      *) echo '无效选项'; exit 1 ;;
    esac ;;
  help|--help|-h) echo "用法: $0 [run|dev|build|install|reinstall|uninstall|authorize]" ;;
  *) echo "未知操作: $action"; exit 1 ;;
esac
