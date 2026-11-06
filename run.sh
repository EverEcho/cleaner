#!/usr/bin/env bash
# Software Manager: common Linux / macOS development and run entry.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/src-tauri/target/release/appclear"
OS="$(uname -s)"
cd "$ROOT"
case "$OS" in Linux|Darwin) ;; *) echo '当前实现支持 Linux 和 macOS；Windows 尚在规划中。'; exit 1 ;; esac
run_direct() {
  if [[ ! -x "$BIN" ]]; then
    echo '未找到已编译的程序，请选择“重新编译并运行”或执行 ./run.sh rebuild。'
    return 1
  fi
  exec "$BIN" "$@"
}
rebuild_run() { pnpm tauri build --no-bundle; run_direct "$@"; }
build_package() {
  pnpm tauri build "$@"
  if [[ $# -eq 0 ]]; then
    echo "打包完成，产物目录：$ROOT/src-tauri/target/release/bundle/"
  fi
}
run_installed() {
  "$ROOT/install.sh" "${1:-install}"
  if [[ "$OS" == Darwin ]]; then exec open "$HOME/Applications/AppClear.app"; fi
  run_direct
}
authorize_menu() {
  if [[ "$OS" != Linux ]]; then
    echo 'macOS 使用系统授权，无需配置 Linux 授权助手。'
    return
  fi
  local choice
  while true; do
    printf '\n授权管理\n  1) 配置授权组件\n  2) 移除授权组件\n  0) 返回主菜单\n\n'
    read -rp '请选择 [默认 0]: ' choice || return 0
    case "${choice:-0}" in
      1) "$ROOT/scripts/install-authorization.sh" install; return ;;
      2) "$ROOT/scripts/install-authorization.sh" uninstall; return ;;
      0) return ;;
      *) echo '无效选项' ;;
    esac
  done
}
confirm_uninstall() {
  local target answer
  if [[ "$OS" == Darwin ]]; then
    target="$HOME/Applications/AppClear.app"
  else
    target="${XDG_DATA_HOME:-$HOME/.local/share}/applications/cn.bit13.app.clear.desktop 及对应图标"
  fi
  echo "卸载目标：$target"
  read -rp '确认卸载？[y/N]: ' answer || return 0
  case "$answer" in
    y|Y|yes|YES) "$ROOT/install.sh" uninstall ;;
    *) echo '已取消卸载。' ;;
  esac
}
main_menu() {
  local choice
  while true; do
    cat <<'MENU'

AppClear · Linux / macOS

运行
  1) 重新编译并运行（默认）
  2) 运行现有版本
  3) 开发模式

打包与安装
  4) 编译并生成安装包
  5) 编译并安装到本机

管理
  6) 授权管理
  7) 卸载本机应用

  0) 退出

MENU
    read -rp '请选择 [默认 1]: ' choice || return 0
    case "${choice:-1}" in
      1) rebuild_run "$@" ;;
      2) run_direct "$@" ;;
      3) exec pnpm tauri dev "$@" ;;
      4) build_package "$@"; return ;;
      5) run_installed install ;;
      6) authorize_menu ;;
      7) confirm_uninstall ;;
      0) return ;;
      *) echo '无效选项' ;;
    esac
  done
}
action="${1:-menu}"
[[ $# -eq 0 ]] || shift
case "$action" in
  run|direct|start) run_direct "$@" ;;
  rebuild) rebuild_run "$@" ;;
  dev) exec pnpm tauri dev "$@" ;;
  build) build_package "$@" ;;
  install) run_installed install ;;
  reinstall|force) run_installed reinstall ;;
  uninstall) exec "$ROOT/install.sh" uninstall ;;
  authorize) exec "$ROOT/scripts/install-authorization.sh" "$@" ;;
  menu) main_menu "$@" ;;
  help|--help|-h)
    echo "用法: $0 [menu|rebuild|run|dev|build|install|reinstall|uninstall|authorize]"
    echo '不带参数打开菜单，默认重新编译并运行；run 仅运行现有版本。'
    ;;
  *) echo "未知操作: $action"; exit 1 ;;
esac
