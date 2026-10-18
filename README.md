# 软件管家

Tauri 2、Vue 3、Rust 实现的 Linux / macOS 桌面软件管家。两个平台共用原创的深紫界面、紫青轨道主视觉与常驻工具导航。Windows 只做[支持计划](docs/windows-plan.md)，尚未实施。

界面以 CleanMyMac 的桌面工具质感作为方向参考，独立实现布局、SVG 与动画。切换工具时保留扫描结果、筛选和选择；开始扫描、检查详情、确认处理三个步骤分开。所有容量与列表均来自本机检测，浏览器预览明确显示桌面端未连接。支持减少动态效果，并适配最小 900 × 580 窗口。

## 功能

| 功能 | Linux | macOS |
| --- | --- | --- |
| 应用卸载 | DEB、Flatpak、Snap、Pacman 影响预览与受控卸载；RPM 识别及原渠道建议；从桌面入口反查独立应用 | 普通 `.app` 移入废纸篓；满足明确范围条件的 Homebrew formula/cask 预览卸载 |
| 垃圾扫描 | 用户缓存、浏览器缓存、废纸篓；授权后扫描 APT、归档 journal、过期临时文件 | Library/Caches、Library/Logs、浏览器缓存、废纸篓 |
| 开发者深度清理 | 历史工具缓存、指定项目目录的依赖和构建产物 | 同一套规则，另含 Xcode DerivedData、CocoaPods 下载缓存 |
| 容器垃圾 | 本机 Docker 的停止容器、未使用镜像、构建缓存 | 本机 Docker Unix socket 可用时支持 |
| 大文件与重复文件 | 多目录与持久保留名单，大文件门槛可调；重复文件按 SHA-256 校验，明确选择保留副本；选中后移入废纸篓 | 同左，视频目录为 Movies |
| 启动项 | XDG autostart，用用户配置覆盖系统项 | 用户 LaunchAgent 后续启动状态；系统服务只读 |
| 应用菜单 | `.desktop` 可见性，用用户配置覆盖 | 应用目录列表只读 |
| 软件更新 | 分渠道检测；APT、DNF4、Flatpak 与可靠 npm/uv/pipx 实例支持串行升级队列；其他渠道提供原渠道建议 | Homebrew 检测与原渠道建议；可靠 npm/uv/pipx 实例支持队列；不代管 App Store |
| 目录分析 | 逐层查看目录与文件逻辑大小、面包屑导航、转入指定目录的大文件扫描，不跨文件系统 | 同左 |
| 任务与资源监控 | 应用内状态栏、任务中心、CPU/内存/磁盘采样和近期曲线 | 同左；内存统计包含可回收缓存，非内存压力指标 |

清理结果采用同一份分类列表和同一个清理按钮。可展开查看文件名、目录、大小和规则；普通文件支持在文件管理器中查看位置。扫描失败明确显示，不把未知空间当成 0B 或“待统计”。“空间占用检查”、已删除文件句柄诊断等不在垃圾扫描范围内。

### 扫描范围、任务与处理结果

大文件、重复文件与开发者扫描可选择多个目录，也可手动输入绝对路径。扫描目录、保留名单、开发者保留天数和大文件门槛保存在本机。保留名单用于文件扫描与清理；处理前重新读取当前名单，保护命中目录及含受保护子路径的整体目标。系统清理按整类动作复核范围，无法证明避开保留名单的动作会跳过并说明原因；Docker 的原生 prune 不支持任意目录排除，因此存在保留名单时该类动作不会自动执行。

任务中心显示当前阶段、已处理数量和路径。扫描、更新检测与目录分析支持协作停止并保留已经发现的结果；正在进行的外部查询可能需等待其有界超时。已开始的清理、卸载或升级不强制中断，队列可以停止后续项。切换页面保留任务和结果，最近最多 60 个任务仅在本次应用会话中显示。

清理、移入废纸篓与卸载逐项报告成功、失败和保留原因。成功项局部移出列表，失败项保留供重试；包卸载另外回读安装状态，同步被连带卸载的已识别应用。重复文件可先指定保留副本，再选择其余，执行前再次确认至少有一份有效副本。开发产物按来源项目归组并说明重新下载或构建的影响。

### 桌面入口反向识别与卸载

Linux 同时读取 XDG 应用菜单与用户桌面目录中的 `.desktop`，解析 `Exec` 的引号、环境变量前缀和可执行文件路径，不执行启动命令；尊重用户覆盖和隐藏设置，并通过软件包归属及程序路径去重。独立应用显示实际程序路径、图标和安装类型。未知安装目录只展示，不猜测删除范围。

已支持用户目录中的 AppImage、能确认目录的 JetBrains IDE、JetBrains Toolbox，以及元数据明确匹配的 Electron 打包应用（包括 Antigravity IDE）的卸载预览。Toolbox 可按用户选择同时卸载其管理的 IDE：根据 `state.json` 的安装记录、自定义安装位置和 `product-info.json` 核对目录，再列出程序、配置、缓存、日志、桌面/自启入口、关联启动脚本和图标的实际路径与大小。项目目录不参与卸载，检测到项目标记的目录受保护。共享 JetBrains Daemon、系统凭据和无法确认归属的文件保留；不能宣称清除了所有外部痕迹。规则参考 [JetBrains 官方卸载说明](https://www.jetbrains.com/help/toolbox-app/installation.html#uninstall-the-toolbox-app)。

Electron 目录需同时匹配原生可执行文件、`resources/app/product.json` 的应用名、`package.json` 的名称及编译入口，并具有运行时资源；源码检出目录、带便携用户数据的目录或指向包外的入口不自动卸载。Antigravity IDE 仅使用产品元数据对应的配置名称，列出明确缓存与日志；配置文件默认不选，保留未保存备份、本地历史、工作区状态和用户扩展。

确认后程序及残留移入废纸篓，安装容器目录仅在为空时删除；不自动关闭进程，程序运行或文件变化时要求退出或重新预览。部分失败可使用同一预览重试未完成项。独立应用按实际处理条数显示进度；无法提供准确百分比的包管理器使用持续进度动画。成功后局部移除已卸载条目，保留搜索、筛选和列表位置，不自动整页重新扫描。

Toolbox 中取消某个 IDE 时，其缓存与关联入口同步取消；个人配置与账户项默认保留，后端再次验证归属及选择。原生包卸载先预览影响范围、依赖与容量，再执行同一份有效计划；危险基础软件、范围变化或身份变化拒绝执行。Homebrew 仅处理可确认单应用产物的 cask，以及无依赖影响、单个已安装版本的手动 formula；复杂安装器、脚本与多产物交给原渠道。Snap 卸载遵循渠道自身的数据处理行为，不能保证应用配置保留。

应用详情关联可靠归属的缓存路径、实际程序路径匹配的启动项与最近已检测更新。无法可靠匹配的关联不展示，未扫描的缓存容量显示未知。

### 通用更新检测与软件规则库

更新页同时展示系统软件包与全局/独立工具，可筛选安装渠道、搜索安装路径、展开来源和检测说明、复制更新方式。一种安装渠道失败不会清空其他已完成结果。检测只读安装文件；包管理器可能写入自身查询缓存，不执行安装或更新。

| 检测器 | 范围与依据 |
| --- | --- |
| 原生包管理器 | Linux APT、DNF/YUM、Zypper、Flatpak、Arch checkupdates；macOS Homebrew。使用本地索引或管理器查询，按原渠道升级 |
| npm | 自动读取当前 npm 环境所有全局一级包，用 `npm outdated -g --json` 查询当前配置仓库，无需每个软件加入名单。参考 [npm 官方说明](https://docs.npmjs.com/cli/v11/commands/npm-outdated/) |
| uv / pipx | 读取工具清单并查询 PyPI 公开版本；不扫描项目虚拟环境、不执行第三方工具。私有源、固定版本、源码安装及非标准版本需按原安装渠道核对；pipx 明确的源码安装不按 PyPI 判断 |
| 独立二进制 | 软件规则库目前识别 Codex CLI，读取 `--version` 并对照官方 GitHub 稳定发布；预发布、本地较新和未知格式区分显示 |
| 官方源码安装 | 软件规则库目前识别 Hermes Agent，从活动命令或启动包装器反查安装目录，核对官方 origin 与 main 分支，再比较提交。固定版本、非 main、分叉或本地修改明确提示；不执行 `fetch`、`pull` 或重启服务 |

独立工具规则在 [software-updates.json](src-tauri/rules/software-updates.json)，新增同类软件可扩展命令名、官方仓库、标签前缀、安装位置线索、来源和更新说明后重新构建；特殊安装器需新增适配器。规则不接受任意安装脚本。同一工具多份安装按检测器分别展示，CLI 与桌面应用不是同一更新目标。

更新队列逐个执行并显示日志、目标版本、架构与失败原因，可仅重试失败项或停止后续。同名多架构按确认的架构与版本匹配，成功后保留其他架构的候选项。APT/DNF4 锁定确认版本，Flatpak 锁定安装范围、完整引用与提交；管理员助手执行前复核。npm/uv/pipx 的直接升级要求安装实例、公开原来源及版本约束可靠，检测许可有效 10 分钟，执行前再验证、执行后回读；npm 使用正常安装流程，包括包的安装脚本。私有源、固定版本、源码安装与未知来源仅提供建议。Homebrew、YUM、Zypper 和 DNF5 暂无可靠精确版本锁，保留原渠道建议；Arch 要求完整系统升级。

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

不清理 Docker 数据卷、运行容器、containerd 目录、内核模块，不停止服务，不创建自动清理计划。浏览器仅清缓存，密码、Cookie、历史保留。相似照片、完整隐私清理、通用应用残留识别尚未实现（目前支持 Toolbox/JetBrains IDE 的明确规则）。

### 轻量资源监控

应用内底部状态栏显示 CPU 使用率、内存使用和用户目录所在磁盘的可用空间，点击进入监控页查看最近曲线。默认启用，可在状态栏任务中心或监控页关闭并保存设置。每 2.5 秒采样，窗口隐藏时暂停定时采样，磁盘统计每 30 秒更新，最近最多 60 次采样仅保留在本次会话中；关闭资源采样不影响任务进度显示。

Linux CPU 用累计计数差值计算，首次采样未知；内存使用量为总量减可用内存。macOS CPU 读取系统 `top` 的采样间隔统计，内存按非空闲页面估算，包含可回收缓存，不等同于内存压力。指标读取失败显示未知并说明原因。数据仅在本机读取和显示。当前没有系统托盘、浮球、自动告警、进程终止或长期历史，应用退出后不继续采样。macOS 的 `top` 参数依据 [Apple 源码中的手册](https://github.com/apple-oss-distributions/top/blob/main/top.1)。

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
| [Tencent/lemon-cleaner](https://github.com/Tencent/lemon-cleaner) | 早期版本的功能分层、分类展开与文件详情参考；现行界面已重新设计 | `7ff8676f1dbe624b039f6b5d50ca81b50963629e` |
| [jemishavasoya/dev-cleaner](https://github.com/jemishavasoya/dev-cleaner) | 开发工具缓存、构建产物识别、先统计后清理、运行环境保护 | `f4cb7bd40a160e69239bfd6a8c32c95eb9afd2bc` |
| [AdventDevInc/kudu](https://github.com/AdventDevInc/kudu) | 声明式规则、保留时间、受保护路径、可选清理、执行前验证 | `f7fbe86ff8b604349ff91cb0c47ebdafb67f01a9` |

本项目独立实现 Vue/Rust 代码及 SVG 主视觉，没有引入上述项目的代码或图片资源，也没有照搬其删除配置、锁文件或重置应用数据的动作。上游许可证以各仓库 LICENSE/README 为准：Lemon Cleaner 含 GPL 组件，dev-cleaner 和 Kudu 为 MIT。

系统行为参考 [Homebrew 手册](https://docs.brew.sh/Manpage)、[Docker 空间统计](https://docs.docker.com/reference/cli/docker/system/df/)、[Docker prune](https://docs.docker.com/reference/cli/docker/system/prune/) 和 [systemd-tmpfiles](https://www.freedesktop.org/software/systemd/man/latest/systemd-tmpfiles.html)。
