#!/usr/bin/env bash
# 软件管家 · 运行/管理入口脚本 (Tauri + Vue 3 版本)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$ROOT/src-tauri/target/release/tauri-app"

ensure_bin() {
    if [[ ! -f "$BIN" ]]; then
        echo "未发现 release 二进制文件，正在自动构建..."
        (cd "$ROOT" && pnpm tauri build --no-bundle)
    fi
}

run_direct() {
    echo ">> 直接启动软件管家..."
    ensure_bin
    exec "$BIN" "$@"
}

run_dev() {
    echo ">> 启动开发模式 (Hot-Reload)..."
    exec pnpm tauri dev
}

install_and_run() {
    echo ">> 安装并启动软件管家..."
    "$ROOT/install.sh" install
    echo ">> 启动应用..."
    exec "$BIN" "$@"
}

force_reinstall() {
    echo ">> 强制重新安装桌面启动器..."
    "$ROOT/install.sh" reinstall
    echo ">> 启动应用..."
    exec "$BIN" "$@"
}

show_menu() {
    echo "======================================"
    echo "   软件管家 (Tauri 2 + Vue 3 版)"
    echo "======================================"
    echo "1) 直接启动 (Release 二进制)"
    echo "2) 安装后启动 (Install & Run)"
    echo "3) 强制重新安装后启动 (Reinstall & Run)"
    echo "4) 开发模式运行 (tauri dev 热重载)"
    echo "5) 卸载桌面快捷方式 (Uninstall)"
    echo "0) 退出 (Exit)"
    echo "======================================"
    read -rp "请选择操作 [1-5, 默认为 1]: " choice
    choice="${choice:-1}"
    case "$choice" in
        1) run_direct ;;
        2) install_and_run ;;
        3) force_reinstall ;;
        4) run_dev ;;
        5) "$ROOT/install.sh" uninstall ;;
        0) echo "已取消。"; exit 0 ;;
        *) echo "无效选项: $choice"; exit 1 ;;
    esac
}

action="${1:-}"

case "$action" in
    run|direct|start)
        shift || true
        run_direct "$@"
        ;;
    dev)
        shift || true
        run_dev "$@"
        ;;
    install)
        shift || true
        install_and_run "$@"
        ;;
    reinstall|force)
        shift || true
        force_reinstall "$@"
        ;;
    uninstall)
        "$ROOT/install.sh" uninstall
        ;;
    help|--help|-h)
        echo "用法: $0 [选项]"
        echo "选项:"
        echo "  (无参数)      显示交互菜单选择操作"
        echo "  run / direct  直接启动 release 编译的本地二进制"
        echo "  dev           进入开发热重载模式 (pnpm tauri dev)"
        echo "  install       安装系统桌面图标并启动"
        echo "  reinstall     强制重新安装图标并启动"
        echo "  uninstall     卸载已安装的桌面图标"
        exit 0
        ;;
    *)
        show_menu
        ;;
esac

