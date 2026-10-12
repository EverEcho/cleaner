#!/usr/bin/env bash
# Install the fixed-operation helper and Polkit session authorization cache.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[[ "$(uname -s)" == Linux ]] || { echo '此授权助手仅用于 Linux；macOS 使用系统授权。'; exit 1; }
[[ -d /usr/share/polkit-1/actions ]] || { echo '请先安装 Polkit 和桌面认证代理。'; exit 1; }
case "${1:-install}" in
  install)
    bash "$ROOT/scripts/build-helper.sh" release
    sudo -v
    sudo install -d -o root -g root -m 755 /usr/local/libexec
    sudo install -o root -g root -m 755 "$ROOT/src-tauri/helper/target/release/appsteward-helper" /usr/local/libexec/appsteward-helper
    sudo install -o root -g root -m 644 "$ROOT/packaging/linux/cn.workbuddy.appsteward.policy" /usr/share/polkit-1/actions/cn.workbuddy.appsteward.policy
    echo '会话授权缓存已配置；首次管理操作仍需认证，过期后会再次提示。未保存密码。'
    ;;
  uninstall)
    sudo -v
    sudo rm -f /usr/share/polkit-1/actions/cn.workbuddy.appsteward.policy /usr/local/libexec/appsteward-helper
    echo '会话授权配置已移除。'
    ;;
  *) echo "用法: $0 [install|uninstall]"; exit 1 ;;
esac
