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
    sudo install -o root -g root -m 755 "$ROOT/src-tauri/helper/target/release/appclear-helper" /usr/local/libexec/appclear-helper
    sudo install -o root -g root -m 644 "$ROOT/packaging/linux/cn.bit13.app.clear.policy" /usr/share/polkit-1/actions/cn.bit13.app.clear.policy
    echo '会话授权缓存已配置；首次管理操作仍需认证，过期后会再次提示。未保存密码。'
    ;;
  uninstall)
    sudo -v
    sudo rm -f /usr/share/polkit-1/actions/cn.bit13.app.clear.policy /usr/local/libexec/appclear-helper
    echo '会话授权配置已移除。'
    ;;
  *) echo "用法: $0 [install|uninstall]"; exit 1 ;;
esac
