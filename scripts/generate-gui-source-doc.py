#!/usr/bin/env python3
"""Render the reviewed source catalog; performs no network access."""
import json
from collections import Counter
from pathlib import Path

root = Path(__file__).resolve().parent.parent
catalog = json.loads((root / "docs/gui-update-sources.json").read_text())
sources = catalog["sources"]
categories = {
    "development": "开发工具",
    "browser": "浏览器",
    "productivity": "办公与笔记",
    "communication": "通信与协作",
    "security": "密码与安全",
    "media": "影音与创作",
    "utility": "macOS 工具",
}
formats = {
    "appcast-and-repository": "Appcast / 包仓库",
    "json": "JSON",
    "github-releases": "GitHub Releases JSON",
    "github-releases-list": "GitHub Releases 列表 JSON",
    "sparkle": "Sparkle XML",
    "yaml": "更新 YAML",
    "release-page": "官方发行页",
    "download-page": "官方下载入口",
}
counts = Counter(source["verification"]["status"] for source in sources)
machine = sum(source["source_type"] not in ("release-page", "download-page") for source in sources)
lines = [
    "# 常用 GUI 软件官方更新源",
    "",
    f"核实日期：{catalog['collected_on']}。共 {len(sources)} 个条目（JetBrains 产品合并记录），其中 {machine} 个包含机器可读更新信息；{counts['verified']} 个主入口已核实，{counts['unconfirmed']} 个待复核。",
    "",
    "结构化数据见 [gui-update-sources.json](gui-update-sources.json)。此文档由 `python3 scripts/generate-gui-source-doc.py` 生成，脚本不联网。",
    "",
    "先匹配本机安装身份、安装渠道、平台和架构，再查询对应官方源。GUI 与 CLI 独立识别，稳定版、预览版、App Store 与官网发行分别处理。上游有新版本不代表本机渠道或灰度已可升级。",
    "",
    "“已核实”仅指当日访问了该条目的主入口并核对返回类型；补充文档和 URL 模板不自动视为逐一核实。网页入口需要专门解析，不能直接作为更新 API。机器接口也可能变化，安装器签名与真实升级适用性需要在安装流程中另行验证。",
    "",
    "目前只接入 Codex GUI 检测；其他条目是后续适配的资料，不会因为收录而自动查询、下载或安装软件。",
    "",
]
for key, title in categories.items():
    lines.extend([f"## {title}", "", "| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |", "| --- | --- | --- | --- | --- |"])
    for source in sources:
        if source["category"] != key:
            continue
        status = "已核实" if source["verification"]["status"] == "verified" else "待复核"
        notes = source["notes"]
        if source["verification"]["status"] != "verified":
            notes += " " + source["verification"].get("detail", "")
        notes = notes.replace("|", " / ").replace("\n", " ")
        platforms = " / ".join("macOS" if value == "macos" else "Linux" for value in source["platforms"])
        lines.append(f"| {source['name']} | {platforms} | [{formats[source['source_type']]}]({source['official_url']}) | {status} | {notes} |")
    lines.append("")
lines.extend(["## Codex GUI 已接入的官方元数据", "", "| 平台 | 架构 | 格式 | 更新地址 |", "| --- | --- | --- | --- |"])
for endpoint in next(source for source in sources if source["id"] == "codex-gui")["endpoints"]:
    lines.append(f"| {endpoint['platform']} | {endpoint['architecture']} | {endpoint['format']} | [官方元数据]({endpoint['url']}) |")
lines.extend([
    "",
    "这些地址已通过实际检测器只读访问和解析。macOS 从实际 Bundle 读取版本、构建号和可执行文件架构，过滤最低系统版本与发布渠道；Linux 从 AppStream 与管理器文件归属确认安装，再匹配相同格式、架构的源。RPM primary 元数据同时核对 SHA256；压缩索引在内存中限量解压，不提取文件。",
    "",
    "Codex GUI 的不受支持渠道和第三方 Linux 打包保留未知状态，不退回 CLI、macOS 版本或第三方目录。Linux APT 安装若与本机唯一候选的包名、当前版本、目标版本及架构一致，可直接在 GUI 行选择“更新此项”，复用系统升级队列、管理员授权、固定版本助手和安装后回读；候选尚未匹配时提供“刷新软件源”。其他 GUI 渠道仍保留原渠道建议。Linux 官方仓库提供的应用包目前名为 ChatGPT，包含 Codex 桌面能力。",
    "",
    "验证命令：",
    "",
    "```bash",
    "cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests",
    "pnpm test:updates",
    "# 以下两项联网，只读元数据；第一项要求当前主机存在可识别的 GUI 安装。",
    "cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests::live_installed_gui_probe -- --ignored --nocapture",
    "cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests::live_official_formats_probe -- --ignored --nocapture",
    "# 只读核对 APT GUI 升级计划，不授权、不安装。",
    "cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests::live_apt_gui_upgrade_plan -- --ignored --nocapture",
    "```",
    "",
])
(root / "docs/gui-update-sources.md").write_text("\n".join(lines))
