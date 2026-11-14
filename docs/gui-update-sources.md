# 常用 GUI 软件官方更新源

核实日期：2026-11-06。共 50 个条目（JetBrains 产品合并记录），其中 26 个包含机器可读更新信息；48 个主入口已核实，2 个待复核。

结构化数据见 [gui-update-sources.json](gui-update-sources.json)。此文档由 `python3 scripts/generate-gui-source-doc.py` 生成，脚本不联网。

先匹配本机安装身份、安装渠道、平台和架构，再查询对应官方源。GUI 与 CLI 独立识别，稳定版、预览版、App Store 与官网发行分别处理。上游有新版本不代表本机渠道或灰度已可升级。

“已核实”仅指当日访问了该条目的主入口并核对返回类型；补充文档和 URL 模板不自动视为逐一核实。网页入口需要专门解析，不能直接作为更新 API。机器接口也可能变化，安装器签名与真实升级适用性需要在安装流程中另行验证。

目前只接入 Codex GUI 检测；其他条目是后续适配的资料，不会因为收录而自动查询、下载或安装软件。

## 开发工具

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| Codex GUI / ChatGPT 桌面版（含 Codex） | Linux / macOS | [Appcast / 包仓库](https://persistent.oaistatic.com/codex-app-prod/appcast.xml) | 已核实 | macOS 按应用架构选择 Appcast；Linux 按 DEB/RPM/Arch 及架构查询官方仓库。独立或第三方重新打包的 Linux Codex 不与官方版本直接比较。 |
| Visual Studio Code | Linux / macOS | [JSON](https://update.code.visualstudio.com/api/update/linux-x64/stable/latest) | 已核实 | productVersion 是用户版本，version 是提交号。平台可用 linux-x64、linux-arm64、darwin、darwin-arm64；Stable 与 Insiders 分开。 |
| Cursor | Linux / macOS | [官方下载入口](https://cursor.com/download) | 已核实 | 保留官方 Linux/macOS 下载页；未确认公开稳定的机器更新 API，不使用猜测的客户端接口。 |
| Claude Desktop | macOS | [JSON](https://downloads.claude.ai/releases/darwin/universal/RELEASES.json) | 已核实 | 仅记录 macOS 官方桌面发行，不把 Claude Code CLI 或非官方 Linux 客户端混入。 |
| Windsurf | Linux / macOS | [JSON](https://windsurf-stable.codeium.com/api/update/linux-x64/stable/latest) | 已核实 | 逐平台验证后使用；失败时以官方发布页为准。 |
| Google Antigravity | Linux / macOS | [官方下载入口](https://antigravity.google/download) | 已核实 | 仅记录官方入口；未确认通用稳定更新 API。 |
| JetBrains Toolbox / IntelliJ IDEA / PyCharm / WebStorm / Rider / GoLand / CLion / DataGrip | Linux / macOS | [JSON](https://data.services.jetbrains.com/products/releases?code=IIU&latest=true&type=release) | 已核实 | 本次核实 IDEA 的 IIU 入口；其他产品替换 code 参数后需核实。Toolbox=TBA，PyCharm=PCP，WebStorm=WS，Rider=RD，GoLand=GO，CLion=CL，DataGrip=DG。不要把 EAP 当稳定版。 |
| Docker Desktop | Linux / macOS | [Sparkle XML](https://desktop.docker.com/mac/main/arm64/appcast.xml) | 已核实 | 当前入口仅为 macOS ARM64；Linux 单独使用官方发行说明与原包仓库，不套用 macOS 清单。 |
| GitHub Desktop | macOS | [GitHub Releases JSON](https://api.github.com/repos/desktop/desktop/releases/latest) | 已核实 | 官方 macOS 发行；Linux 社区移植单独归类。 |
| Zed | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/zed-industries/zed/releases/latest) | 已核实 | 稳定版与 Preview 分开，按 release assets 的系统和架构筛选。 |
| Sublime Text | Linux / macOS | [官方下载入口](https://www.sublimetext.com/download) | 已核实 | 官方稳定版下载页包含完整 Changelog 和 Linux/macOS 下载项；按 build 比较，Dev Builds 单独处理。 |
| Postman | Linux / macOS | [官方发行页](https://www.postman.com/downloads/release-notes/) | 已核实 | 官方发行说明；当前未收录经核实的机器可读更新接口。 |
| Insomnia | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/Kong/insomnia/releases/latest) | 已核实 |  |
| DBeaver Community | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/dbeaver/dbeaver/releases/latest) | 已核实 | Community 与付费产品分开。 |
| TablePlus | Linux / macOS | [官方下载入口](https://tableplus.com/download) | 已核实 | macOS 与 Linux 分别发行；未确认机器接口前不合并版本。 |

## 浏览器

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| Google Chrome | Linux / macOS | [JSON](https://versionhistory.googleapis.com/v1/chrome/platforms/linux/channels/stable/versions) | 已核实 | 版本历史 API；macOS 使用 mac 或 mac_arm64 平台。此 API 是上游发行历史，不代表本机渠道/灰度必然可升级。 |
| Mozilla Firefox | Linux / macOS | [JSON](https://product-details.mozilla.org/1.0/firefox_versions.json) | 已核实 | LATEST_FIREFOX_VERSION 与 ESR、Beta、Nightly 字段分开。 |
| Microsoft Edge | Linux / macOS | [JSON](https://edgeupdates.microsoft.com/api/products?view=enterprise) | 已核实 | 按 Product=Stable、Platform、Architecture 筛选 Releases。 |
| Brave Browser | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/brave/brave-browser/releases/latest) | 已核实 | Stable、Beta、Nightly 属于不同发行渠道。 |
| Vivaldi | Linux / macOS | [官方发行页](https://vivaldi.com/blog/desktop/) | 已核实 | Stable 与 Snapshot 分开；此处是官方网页，不承诺统一 API。 |
| Opera | Linux / macOS | [官方发行页](https://blogs.opera.com/desktop/) | 已核实 | 按桌面 Stable、Beta、Developer 分类筛选，不从下载重定向推测版本。 |
| Arc | macOS | [官方下载入口](https://arc.net/) | 待复核 | 保留官方入口；Appcast 访问受限，未将其作为可用机器接口收录。 官方入口请求返回 HTTP 403；待复核，不作为可自动检测接口。 |

## 办公与笔记

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| Obsidian | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/obsidianmd/obsidian-releases/releases/latest) | 已核实 | 桌面安装器版本与应用内自动更新版本可能不同，Catalyst 预览需单独处理。 |
| Notion | macOS | [官方下载入口](https://www.notion.com/desktop) | 已核实 | 仅记录官方桌面版，不包含非官方 Linux 封装。 |
| WPS Office | Linux / macOS | [官方下载入口](https://linux.wps.cn/) | 已核实 | Linux 与 macOS 分别发行；不从下载文件名推断完整更新适用性。 |

## 通信与协作

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| Slack | Linux / macOS | [官方发行页](https://slack.com/release-notes/mac) | 已核实 | 本次使用官方发行说明；未确认可访问的机器更新清单。Linux 应按官方 DEB/RPM 包源检测。 |
| Discord | Linux / macOS | [官方下载入口](https://discord.com/download) | 已核实 | Stable、PTB、Canary 分开。当前保留官方入口，不承诺接口稳定。 |
| Telegram Desktop | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/telegramdesktop/tdesktop/releases/latest) | 已核实 | 对应跨平台 Telegram Desktop；macOS 原生 Telegram 是不同客户端。 |
| Signal Desktop | Linux / macOS | [更新 YAML](https://updates.signal.org/desktop/latest-mac.yml) | 已核实 | 此清单用于 macOS；Linux 使用官方 APT 软件源。 |
| Zoom Workplace | Linux / macOS | [官方下载入口](https://www.zoom.com/en/download) | 已核实 | 仅记录官方入口；更新渠道与平台需单独适配。 |
| Microsoft Teams | macOS | [官方发行页](https://learn.microsoft.com/en-us/microsoftteams/teams-client-update) | 已核实 | 这里只收录官方 macOS 桌面端；Linux Web/PWA 与原生应用分开。 |
| 微信 / WeChat | Linux / macOS | [官方下载入口](https://linux.weixin.qq.com/) | 已核实 | Linux 与 macOS 分别发行；当前只收录官方入口。 |
| QQ | Linux / macOS | [官方下载入口](https://im.qq.com/linuxqq/index.shtml) | 已核实 | Linux 与 macOS 分别发行，不能直接比较两个平台版本。 |
| 飞书 | Linux / macOS | [官方下载入口](https://www.feishu.cn/download) | 已核实 | 官方安装入口；按平台、架构和组织发布策略适配。 |

## 密码与安全

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| KeePassXC | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/keepassxreboot/keepassxc/releases/latest) | 已核实 |  |
| Bitwarden Desktop | Linux / macOS | [GitHub Releases 列表 JSON](https://api.github.com/repos/bitwarden/clients/releases?per_page=30) | 已核实 | 仓库同时发行多个客户端，必须筛选 desktop-v 前缀并排除 draft/prerelease，不能直接使用 releases/latest。 |
| 1Password | Linux / macOS | [官方发行页](https://releases.1password.com/) | 已核实 | 按产品、系统、稳定/测试渠道筛选；Linux 使用官方发行与原包源。 |

## 影音与创作

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| VLC | Linux / macOS | [Sparkle XML](https://update.videolan.org/vlc/sparkle/vlc-intel64.xml) | 已核实 | 此清单用于 macOS Intel；Linux 使用官方发行页/原软件源。ARM64 需另核实。 |
| IINA | macOS | [GitHub Releases JSON](https://api.github.com/repos/iina/iina/releases/latest) | 已核实 |  |
| OBS Studio | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/obsproject/obs-studio/releases/latest) | 已核实 |  |
| HandBrake | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/HandBrake/HandBrake/releases/latest) | 已核实 |  |
| Audacity | Linux / macOS | [GitHub Releases JSON](https://api.github.com/repos/audacity/audacity/releases/latest) | 已核实 |  |
| GIMP | Linux / macOS | [JSON](https://www.gimp.org/gimp_versions.json) | 已核实 | 稳定版与开发版分开，下载项分别带平台与架构。 |
| Blender | Linux / macOS | [官方发行页](https://www.blender.org/download/releases/) | 已核实 | Stable、LTS 与实验构建分开。 |
| Inkscape | Linux / macOS | [官方发行页](https://inkscape.org/release/) | 待复核 | 官方发行页；按安装渠道核对可用包。 官方发行页请求返回 HTTP 502；待复核，不作为可自动检测接口。 |

## macOS 工具

| 软件 | 软件平台 | 官方主入口 | 状态 | 适配说明 |
| --- | --- | --- | --- | --- |
| Rectangle | macOS | [GitHub Releases JSON](https://api.github.com/repos/rxhanson/Rectangle/releases/latest) | 已核实 | Rectangle 与 Rectangle Pro 是不同产品。 |
| Raycast | macOS | [官方发行页](https://www.raycast.com/changelog) | 已核实 | 官方变更日志；未确认公开机器更新协议。 |
| Alfred | macOS | [官方发行页](https://www.alfredapp.com/changelog/) | 已核实 | 正式版与预发布版分开。 |
| AppCleaner | macOS | [官方下载入口](https://freemacsoft.net/appcleaner/) | 已核实 | 核对最低 macOS 要求，旧系统仍可能只适用旧版本。 |
| CleanMyMac | macOS | [官方发行页](https://macpaw.com/cleanmymac/whats-new) | 已核实 | Mac App Store 与官网发行渠道分开。 |

## Codex GUI 已接入的官方元数据

| 平台 | 架构 | 格式 | 更新地址 |
| --- | --- | --- | --- |
| macos | arm64 | sparkle | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/appcast.xml) |
| macos | x64 | sparkle | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/appcast-x64.xml) |
| linux | amd64 | deb-packages | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/linux/deb/dists/stable/main/binary-amd64/Packages) |
| linux | arm64 | deb-packages | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/linux/deb/dists/stable/main/binary-arm64/Packages) |
| linux | x86_64 | rpm-repomd | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/linux/rpm/x86_64/repodata/repomd.xml) |
| linux | aarch64 | rpm-repomd | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/linux/rpm/aarch64/repodata/repomd.xml) |
| linux | x86_64 | arch-db-gzip | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/linux/arch/x86_64/openai-chatgpt.db) |
| linux | aarch64 | arch-db-gzip | [官方元数据](https://persistent.oaistatic.com/codex-app-prod/linux/arch/aarch64/openai-chatgpt.db) |

这些地址已通过实际检测器只读访问和解析。macOS 从实际 Bundle 读取版本、构建号和可执行文件架构，过滤最低系统版本与发布渠道；Linux 从 AppStream 与管理器文件归属确认安装，再匹配相同格式、架构的源。RPM primary 元数据同时核对 SHA256；压缩索引在内存中限量解压，不提取文件。

Codex GUI 的不受支持渠道和第三方 Linux 打包保留未知状态，不退回 CLI、macOS 版本或第三方目录。Linux APT 安装若与本机唯一候选的包名、当前版本、目标版本及架构一致，可直接在 GUI 行选择“更新此项”，复用系统升级队列、管理员授权、固定版本助手和安装后回读；候选尚未匹配时提供“刷新软件源”。其他 GUI 渠道仍保留原渠道建议。Linux 官方仓库提供的应用包目前名为 ChatGPT，包含 Codex 桌面能力。

验证命令：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests
pnpm test:updates
# 以下两项联网，只读元数据；第一项要求当前主机存在可识别的 GUI 安装。
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests::live_installed_gui_probe -- --ignored --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests::live_official_formats_probe -- --ignored --nocapture
# 只读核对 APT GUI 升级计划，不授权、不安装。
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib software_updates::gui::tests::live_apt_gui_upgrade_plan -- --ignored --nocapture
```
