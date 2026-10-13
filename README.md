# 软件管家

Tauri 2、Vue 3、Rust 实现的 Linux / macOS 桌面软件管家。两个平台共用参考 Lemon Cleaner 的白色、绿色、橙色界面。Windows 只做[支持计划](docs/windows-plan.md)，尚未实施。

## 功能

| 功能 | Linux | macOS |
| --- | --- | --- |
| 应用卸载 | DEB、Flatpak、Snap、RPM/Pacman；从桌面入口反查独立安装应用及图标 | 应用包、Homebrew formula/cask；读取应用图标，普通 `.app` 移入废纸篓 |
| 垃圾扫描 | 用户缓存、浏览器缓存、废纸篓；授权后扫描 APT、归档 journal、过期临时文件 | Library/Caches、Library/Logs、浏览器缓存、废纸篓 |
| 开发者深度清理 | 历史工具缓存、指定项目目录的依赖和构建产物 | 同一套规则，另含 Xcode DerivedData、CocoaPods 下载缓存 |
| 容器垃圾 | 本机 Docker 的停止容器、未使用镜像、构建缓存 | 本机 Docker Unix socket 可用时支持 |
| 大文件与重复文件 | 常用用户目录；大文件默认 50MB 起，重复文件按 SHA-256 校验；选中后移入废纸篓 | 同左，视频目录为 Movies |
| 启动项 | XDG autostart，用用户配置覆盖系统项 | 用户 LaunchAgent 后续启动状态；系统服务只读 |
| 应用菜单 | `.desktop` 可见性，用用户配置覆盖 | 应用目录列表只读 |
| 软件更新 | 系统包管理器、npm 全局包、uv/pipx 工具、Codex/Hermes 独立安装规则 | Homebrew 与同一套工具检测规则；不代管 App Store |
| 目录分析 | 逐层查看目录占用，不跨文件系统 | 同左 |

清理结果采用同一份分类列表和同一个清理按钮。可展开查看文件名、目录、大小和规则；普通文件支持在文件管理器中查看位置。扫描失败明确显示，不把未知空间当成 0B 或“待统计”。“空间占用检查”、已删除文件句柄诊断等不在垃圾扫描范围内。

### 桌面入口反向识别与卸载

Linux 同时读取 XDG 应用菜单与用户桌面目录中的 `.desktop`，解析 `Exec` 的引号、环境变量前缀和可执行文件路径，不执行启动命令；尊重用户覆盖和隐藏设置，并通过软件包归属及程序路径去重。独立应用显示实际程序路径、图标和安装类型。未知安装目录只展示，不猜测删除范围。

已支持用户目录中的 AppImage、能确认目录的 JetBrains IDE、JetBrains Toolbox，以及元数据明确匹配的 Electron 打包应用（包括 Antigravity IDE）的卸载预览。Toolbox 可按用户选择同时卸载其管理的 IDE：根据 `state.json` 的安装记录、自定义安装位置和 `product-info.json` 核对目录，再列出程序、配置、缓存、日志、桌面/自启入口、关联启动脚本和图标的实际路径与大小。项目目录不参与卸载，检测到项目标记的目录受保护。共享 JetBrains Daemon、系统凭据和无法确认归属的文件保留；不能宣称清除了所有外部痕迹。规则参考 [JetBrains 官方卸载说明](https://www.jetbrains.com/help/toolbox-app/installation.html#uninstall-the-toolbox-app)。

Electron 目录需同时匹配原生可执行文件、`resources/app/product.json` 的应用名、`package.json` 的名称及编译入口，并具有运行时资源；源码检出目录、带便携用户数据的目录或指向包外的入口不自动卸载。Antigravity IDE 仅使用产品元数据对应的配置名称，列出明确缓存与日志；配置文件默认不选，保留未保存备份、本地历史、工作区状态和用户扩展。

确认后程序及残留移入废纸篓，安装容器目录仅在为空时删除；不自动关闭进程，程序运行或文件变化时要求退出或重新预览。部分失败可使用同一预览重试未完成项。独立应用按实际处理条数显示进度；无法提供准确百分比的包管理器使用持续进度动画。成功后局部移除已卸载条目，保留搜索、筛选和列表位置，不自动整页重新扫描。

### 通用更新检测与软件规则库

更新页同时展示系统软件包与全局/独立工具，可筛选安装渠道、搜索安装路径、展开来源和检测说明、复制更新方式。一种安装渠道失败不会清空其他已完成结果。检测只读安装文件；包管理器可能写入自身查询缓存，不执行安装或更新。

| 检测器 | 范围与依据 |
| --- | --- |
| 原生包管理器 | Linux APT、DNF/YUM、Zypper、Flatpak、Arch checkupdates；macOS Homebrew。使用本地索引或管理器查询，按原渠道升级 |
| npm | 自动读取当前 npm 环境所有全局一级包，用 `npm outdated -g --json` 查询当前配置仓库，无需每个软件加入名单。参考 [npm 官方说明](https://docs.npmjs.com/cli/v11/commands/npm-outdated/) |
| uv / pipx | 读取工具清单并查询 PyPI 公开版本；不扫描项目虚拟环境、不执行第三方工具。私有源、固定版本、源码安装及非标准版本需按原安装渠道核对；pipx 明确的源码安装不按 PyPI 判断 |
| 独立二进制 | 软件规则库目前识别 Codex CLI，读取 `--version` 并对照官方 GitHub 稳定发布；预发布、本地较新和未知格式区分显示 |
| 官方源码安装 | 软件规则库目前识别 Hermes Agent，从活动命令或启动包装器反查安装目录，核对官方 origin 与 main 分支，再比较提交。固定版本、非 main、分叉或本地修改明确提示；不执行 `fetch`、`pull` 或重启服务 |

独立工具规则在 [software-updates.json](src-tauri/rules/software-updates.json)，新增同类软件可扩展命令名、官方仓库、标签前缀、安装位置线索、来源和更新说明后重新构建；特殊安装器需新增适配器。规则不接受任意安装脚本。当前界面仅提供独立工具的更新方式，实际更新仍使用原安装渠道；同一工具多份安装按检测器分别展示。CLI 与桌面应用不是同一更新目标。

没有能可靠覆盖所有安装方式的统一接口：系统包、语言工具包、独立二进制、源码和应用内更新各有渠道。npm 自动覆盖通过该环境安装的 Claude Code、Gemini CLI、Kilo 等工具，不必逐个硬编码；没有通过已支持环境安装的程序不会仅凭名称猜测版本。GUI 启动时也检查常见用户和 Homebrew 命令目录，但不自动枚举所有 Node 版本管理器的历史安装环境。

网络查询会向 npm 当前配置仓库发送包查询，向 PyPI 发送工具包名，向 GitHub 请求规则库内的公开仓库与用于比较的安装提交；不发送项目文件、应用配置或密钥。HTTP 查询限制大小与超时，同一进程成功响应缓存 5 分钟，Python 工具每轮最多 40 个、4 个并行请求并设时间预算；超时/未查询不显示“已是最新”。Hermes 的主干提交比较不代表最新稳定发行版，实际更新渠道以 [Hermes 官方更新说明](https://nousresearch.github.io/hermes-agent/docs/getting-started/updating/) 和应用自身检查为准；Codex 安装渠道见 [官方仓库](https://github.com/openai/codex)。

### 开发者清理

普通垃圾扫描会统计至少 7 天未修改或访问的工具缓存。项目发现只遍历目录，跳过依赖、构建输出、虚拟环境和 SDK 目录；保留时间、大小和指纹在一次遍历中统计。根据 Compose 中已识别的持久化挂载路径和数据库标记跳过服务数据（包括旧 PostgreSQL data_pg 目录），保留源码挂载的项目；清理前复核服务数据归属。超过目录/时间/深度限制或遇到权限问题时返回已完成的结果，并折叠展示未覆盖详情。大量同类结果聚合成分类，详情每次展示 100 项，可单独勾选目录，避免一次渲染上万行。“开发者深度扫描”可填写项目目录并设置 1/7/30/90 天的保留时间，扫描结果加入本页“开发者垃圾”，默认不勾选。

- 工具缓存：npm、Bun、Yarn、pip、Go build、ccache、sccache、Cargo crate 下载、Gradle、Maven，以及 macOS 的 Xcode DerivedData、CocoaPods。
- 项目识别：根据 `package.json`、`Cargo.toml`、`pom.xml`、Gradle、Flutter、.NET 工程文件或 Python 源文件，列出 `node_modules`、框架缓存、`target`、`build`、`bin/obj`、`__pycache__` 等对应产物。不会仅凭通用目录名删除未识别项目。
- 保留源码、锁文件、配置、SDK、工具链、虚拟环境、Xcode Archives 和签名证书。通用缓存扫描排除 Node、Codex runtimes、Corepack 等运行程序目录；扫描不跟随项目符号链接。
- 清理前再次检查访问时间、文件元数据与可见运行进程路径；发生变化要求重新扫描。文件访问时间依赖文件系统设置，不能单凭它判断程序是否正在使用缓存，清理前应退出相关开发工具。

规则在 [developer.json](src-tauri/rules/developer.json)。pnpm/uv 的原生 prune、IDE 本地历史、应用配置重置目前未接入，避免把完整存储大小误报为可回收空间。

### 系统与容器清理

Linux 系统项先授权再扫描，扫描中列出实际候选文件和大小，选中后由专用助手执行固定命令：

| 项目 | 统计与执行范围 |
| --- | --- |
| APT 安装包缓存 | 列出 archives 文件；执行 `apt-get clean`，不执行 autoremove |
| systemd 历史日志 | 估算超出 500MiB 保留目标的最旧归档文件；执行 `journalctl --vacuum-size=500M`，不主动轮转活动日志 |
| 过期临时文件 | 根据 `systemd-tmpfiles --clean --dry-run` 的系统规则统计候选路径；实际执行限定 `/tmp` 和 `/var/tmp` |
| Docker 容器 | 统计停止容器，执行 `docker container prune --force` |
| Docker 镜像 | 统计未使用镜像，执行 `docker image prune --all --force` |
| Docker 构建缓存 | 统计构建缓存，执行 `docker builder prune --all --force` |

Docker 分类总额取本机守护进程的可回收统计，镜像详情的大小可能包含共享层，不逐项相加作为可回收总额；journal/Docker 展示估算值，实际释放以工具执行结果为准。工具不存在、版本不支持或权限不足会显示失败。清理前重扫并核对系统计划指纹，拒绝已改变的范围。

不清理 Docker 数据卷、运行容器、containerd 目录、内核模块，不停止服务，不创建自动清理计划。浏览器仅清缓存，密码、Cookie、历史保留。相似照片、完整隐私清理、状态栏监控、通用应用残留识别尚未实现（目前支持 Toolbox/JetBrains IDE 的明确规则）。

## 运行与打包

需要 Rust stable、Node.js 24+、pnpm 11，及 [Tauri 系统依赖](https://tauri.app/start/prerequisites/)（Linux WebKitGTK 4.1；macOS Xcode Command Line Tools）。

```bash
pnpm install --frozen-lockfile
./run.sh dev
./run.sh run
./run.sh build
./run.sh install
./run.sh uninstall
```

脚本可从任意目录调用。Linux 安装用户级菜单入口；macOS 安装到 `~/Applications/软件管家.app`，旧应用移入废纸篓。Linux 支持 DEB/RPM/AppImage，macOS 支持 APP/DMG，须在对应系统构建。例如 `./run.sh build --bundles deb`。Mac 发布签名、公证和真实设备验收仍需完成。

## 主动授权与会话缓存

垃圾清理页面点击“管理员授权”。Linux 会通过系统认证安装或更新随应用打包的专用助手和 Polkit 策略，完成授权后自动重新扫描。无需在页面输入或保存密码。首次安装与后续业务授权可能分别弹出系统认证框。macOS 按钮打开系统隐私设置，受保护目录需用户在系统中授予权限后重新扫描。

Linux 也可使用终端安装或移除助手：

```bash
./run.sh authorize
./run.sh authorize uninstall
```

Polkit `auth_admin_keep` 仅对活动本地会话保留短期授权；期限由系统管理，过期重新认证，不写免密 sudo。助手及策略由 root 所有且普通用户不可写，仅允许固定操作，不接受任意脚本、程序或待删除路径。助手“已配置”不代表授权缓存当前有效。macOS 使用 Finder/TCC 的系统授权流程，Homebrew 以普通用户执行。

## 验证

Linux 的独立助手必须先构建，供 Tauri 资源打包使用：

```bash
bash scripts/build-helper.sh release
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml --all --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo test --manifest-path src-tauri/helper/Cargo.toml --locked
```

CI 包含 Linux、macOS 原生编译检查，不包含 Windows。当前验证边界见[平台支持与验收](docs/platform-support.md)。

## 参考项目

| 项目 | 参考内容 | 本次查看提交 |
| --- | --- | --- |
| [Tencent/lemon-cleaner](https://github.com/Tencent/lemon-cleaner) | 首页与清理结果布局、分类展开、文件详情、功能分层 | `7ff8676f1dbe624b039f6b5d50ca81b50963629e` |
| [jemishavasoya/dev-cleaner](https://github.com/jemishavasoya/dev-cleaner) | 开发工具缓存、构建产物识别、先统计后清理、运行环境保护 | `f4cb7bd40a160e69239bfd6a8c32c95eb9afd2bc` |
| [AdventDevInc/kudu](https://github.com/AdventDevInc/kudu) | 声明式规则、保留时间、受保护路径、可选清理、执行前验证 | `f7fbe86ff8b604349ff91cb0c47ebdafb67f01a9` |

本项目独立实现 Vue/Rust 代码及 SVG 主视觉，没有引入上述项目的代码或图片资源，也没有照搬其删除配置、锁文件或重置应用数据的动作。上游许可证以各仓库 LICENSE/README 为准：Lemon Cleaner 含 GPL 组件，dev-cleaner 和 Kudu 为 MIT。

系统行为参考 [Homebrew 手册](https://docs.brew.sh/Manpage)、[Docker 空间统计](https://docs.docker.com/reference/cli/docker/system/df/)、[Docker prune](https://docs.docker.com/reference/cli/docker/system/prune/) 和 [systemd-tmpfiles](https://www.freedesktop.org/software/systemd/man/latest/systemd-tmpfiles.html)。
