# AppClear

Tauri 2、Vue 3、Rust 实现的 Linux / macOS 桌面清理工具 AppClear。两个平台共用紫青轨道主视觉与常驻工具导航。左侧底部可切换深色、浅色或跟随系统主题，选择在本机保存；浅色使用白底深色字，深色提高辅助文字和控件对比度。Windows 只做[支持计划](docs/windows-plan.md)，尚未实施。

应用名称为 `AppClear`，应用标识与 Linux Polkit 操作 ID 为 `cn.bit13.app.clear`，主程序为 `appclear`。更改标识后，设置使用新标识对应的目录，不自动迁移旧设置；已有 Linux 安装需重新生成菜单入口和授权组件。

界面以 CleanMyMac 的桌面工具质感作为方向参考，独立实现布局、SVG 与动画。页面标题与主要操作统一放在窗口标题栏；扫描时同一按钮切换为停止，进度提示位于按钮左侧。统计采用紧凑摘要，扫描设置、检测范围与技术详情按需展开，批量处理按钮在选中内容后显示。切换工具时保留扫描结果、筛选和选择；开始扫描、检查详情、确认处理三个步骤分开。所有容量与列表均来自本机检测，浏览器预览明确显示桌面端未连接。支持减少动态效果，并适配最小 900 × 580 窗口。

## 功能

| 功能 | Linux | macOS |
| --- | --- | --- |
| 应用卸载 | DEB、Flatpak、Snap、Pacman 影响预览与受控卸载；RPM 识别及原渠道建议；从桌面入口反查独立应用 | 普通 `.app` 移入废纸篓；满足明确范围条件的 Homebrew formula/cask 预览卸载 |
| 应用市场 | 按分类和关键词查找应用，通过 Flathub 下载并安装到当前用户 | 通过本机 Homebrew 安装支持的官方 Cask |
| 垃圾扫描 | 用户缓存、浏览器缓存、废纸篓；授权后扫描 APT、归档 journal、过期临时文件 | Library/Caches、Library/Logs、浏览器缓存、废纸篓 |
| 开发者深度清理 | 历史工具缓存、指定项目目录的依赖和构建产物 | 同一套规则，另含 Xcode DerivedData、CocoaPods 下载缓存 |
| 容器垃圾 | 本机 Docker 的停止容器、未使用镜像、构建缓存 | 本机 Docker Unix socket 可用时支持 |
| 大文件与重复文件 | 多目录与持久保留名单，大文件门槛可调；重复文件按 SHA-256 校验，明确选择保留副本；选中后移入废纸篓 | 同左，视频目录为 Movies |
| 启动项 | XDG autostart，用用户配置覆盖系统项 | 用户 LaunchAgent 后续启动状态；系统服务只读 |
| 应用菜单 | `.desktop` 可见性，用用户配置覆盖 | 隐藏“应用入口”，旧链接转到应用管理 |
| 软件更新 | 分渠道检测；APT、DNF4、Flatpak 与可靠 npm/uv/pipx 实例支持串行升级队列；Codex GUI 查询官方更新源并关联可靠 APT 候选直接更新；其他渠道提供原渠道建议 | Homebrew、Codex GUI 官方更新源检测与原渠道建议；可靠 npm/uv/pipx 实例支持队列；不代管 App Store |
| 目录分析 | 目录排行与矩形面积图切换，点击目录下钻；面包屑导航、转入指定目录的大文件扫描，不跨文件系统 | 同左 |
| 任务与资源监控 | 应用内状态栏、任务中心、CPU/内存/网速/磁盘采样和近期曲线 | 同左；原生菜单栏指标与快捷面板，支持自定义；内存百分比不是内存压力 |

清理结果采用同一份分类列表和同一个清理按钮。可展开查看文件名、目录、大小和规则；普通文件支持在文件管理器中查看位置。扫描失败明确显示，不把未知空间当成 0B 或“待统计”。“空间占用检查”、已删除文件句柄诊断等不在垃圾扫描范围内。

### 应用市场索引

在左侧打开「应用市场」即可浏览。客户端进入页面时同步 GitHub 索引。同步失败时显示本次运行中上次同步的目录或随包内置的目录，可以用「刷新索引」重试。

点击「安装」并确认渠道后，由本机包管理器下载和安装。Linux 需要已安装 Flatpak，首次安装可以配置用户级 Flathub，并使用稳定分支。macOS 需要已安装 Homebrew，支持可直接部署应用的 Cask。安装状态按所显示的软件渠道核对，进度和错误显示在应用市场及任务中心。上游版本标签供浏览，实际版本由包管理器解析。

新增或调整条目时编辑 [market/catalog.json](market/catalog.json)。`install.linux` 填写 Flathub 应用 ID，`install.macos` 填写 `homebrew/cask` 的短名称。只有能用 GitHub 最新正式版唯一对应到该应用时，才设置 `release_repo`。使用 Node.js 24 运行 `node scripts/build-market-index.mjs --check` 可在本地校验。

[App market index](.github/workflows/market-index.yml) 在 PR 中校验目录，在相关改动进入 `main`、每日定时或手动触发时同步上游版本，内容有变化才更新 `market-index` 分支的 `index.json`。发布失败会保留上一份已发布索引。首次运行会自动创建该分支，仓库需允许工作流的 `GITHUB_TOKEN` 写入该分支。客户端从本仓库的公开 raw 地址读取索引。

### 扫描范围、任务与处理结果

启动项与应用入口进入页面即自动读取本机列表，顶部提供刷新与失败重试；往返页面保留结果及筛选。进入垃圾清理、空间透视、应用管理、大文件、重复文件和软件更新页面时不自动扫描；首次操作按钮放在页面中间的提示下方，标题栏先隐藏扫描入口。扫描中的停止按钮与进度也放在中间提示下方，结果表格和筛选栏暂时隐藏；完成后标题栏显示重新扫描或刷新，失败后可重试。选择分析目录或切换扫描范围只设置下次任务的范围，已有结果保留。首页“扫描垃圾”是显式启动入口，启动后会移除一次性请求，返回页面不重复触发。

检测范围、结果提示和处理结果收在顶部，点击弹窗查看；监控页标题栏的 tips 包含采样说明与统计口径。长路径以省略号显示，悬停查看完整内容。

macOS 菜单栏默认显示应用图标和上下行速度。左键打开资源面板，右键可打开主窗口或退出。面板“自定义”或本机监控“菜单栏设置”中可配置指标、顺序、图标、精简/完整标签、1/2/3/5 秒刷新、字节/比特单位、指定网卡、趋势图和关闭窗口后的驻留行为；设置即时保存并同步两个窗口。关闭菜单栏时会显示主窗口，可从本机监控重新开启。

CPU、内存与网卡通过原生计数器读取，不启动 `top`；网速使用累计流量和实际时间差，自动模式合计物理网卡，排除回环、VPN 隧道和桥接接口的重复计数，亦可手动指定单个接口。首次采样、网卡变化或计数器重置时显示 —，不会用 0 代替未知数据。暂停后保留上次结果并显示暂停状态；菜单栏启用时隐藏主窗口仍会采样。测试：`pnpm test:tray` 与 `cargo test --manifest-path src-tauri/Cargo.toml --lib`。

空间透视按条目读取一次元数据，最多用 4 个工作线程统计独立目录。本次分析保留完整目录快照，按相对文件名保存，直到手动重新分析或退出应用；下钻、返回与面包屑导航只读取快照，不启动扫描任务，也不因深度、条目数量或时间自动重新扫描。快照范围外的目录需要手动分析；保留名单的修改在下一次手动分析时生效。中途停止仅保留完整统计的项目供浏览，未完成目录不会自动补扫。矩形图复用 D3 的 treemap 布局，最多展示 120 个独立方块，其余合并；完整列表按每页 100 项展示。容量按逻辑大小统计，共享依赖与硬链接不代表可重复回收的空间。

矩形图、目录排行和完整列表支持右键“移入废纸篓”，列表“更多”提供相同操作。确认时显示完整路径，目录会连同全部内容处理。执行前重新读取保留名单并核对分析时的文件身份及目录内容；变化、符号链接、未完整分析的目录和包含保留子路径的目录会保留。分析根目录、用户主目录及其祖先不能通过此操作移除。成功后同步扣除快照中的占用，返回祖先目录不会恢复已删除项；失败保留条目及处理原因。实际释放空间需清空废纸篓。

Linux 使用 AppIndicator 系统托盘，托盘菜单提供资源面板、主窗口、监控设置和退出；后台采样及关闭窗口后驻留与 macOS 共用设置。Linux 始终保留彩色应用图标，指标文字是否显示取决于桌面环境，资源面板提供完整数据。Linux 不提供托盘点击事件和图标位置，因此从菜单打开居中的资源面板，见 [Tauri 托盘说明](https://v2.tauri.app/learn/system-tray/)。

Debian 安装包声明 `libayatana-appindicator3-1` 与 `libglib2.0-bin` 运行依赖。GNOME 还需安装并启用 AppIndicator 扩展（Debian 包名 `gnome-shell-extension-appindicator`），安装后可能需要重新登录；KDE 等桌面需有可用的系统托盘区域。编译环境另需 `libayatana-appindicator3-dev`。可在“本机监控 → 托盘设置”检查开关并打开资源面板。

大文件、重复文件与开发者扫描可选择多个目录，也可手动输入绝对路径。扫描目录、保留名单、开发者保留天数和大文件门槛保存在本机。保留名单用于文件扫描与清理；处理前重新读取当前名单。普通缓存含有保留子路径时，拆出其余可清理项目，并同步更新列表与容量；保留路径及其祖先容器不删除。开发者产物和系统清理按整体目标复核范围，无法证明避开保留名单的动作会跳过并说明原因；Docker 的原生 prune 不支持任意目录排除，因此存在保留名单时该类动作不会自动执行。

任务中心显示当前阶段、已处理数量和路径。扫描、更新检测与目录分析支持协作停止并保留已经发现的结果；正在进行的外部查询可能需等待其有界超时。已开始的清理、卸载或升级不强制中断，队列可以停止后续项。切换页面保留任务和结果，最近最多 60 个任务仅在本次应用会话中显示。

清理、移入废纸篓与卸载逐项报告成功、失败和保留原因。成功项局部移出列表，失败项保留供重试；包卸载另外回读安装状态，同步被连带卸载的已识别应用。重复文件可先指定保留副本，再选择其余，执行前再次确认至少有一份有效副本。开发产物按来源项目归组并说明重新下载或构建的影响。

### 桌面入口反向识别与卸载

Linux 同时读取 XDG 应用菜单与用户桌面目录中的 `.desktop`，解析 `Exec` 的引号、环境变量前缀和可执行文件路径，不执行启动命令；尊重用户覆盖和隐藏设置，并通过软件包归属及程序路径去重。独立应用显示实际程序路径、图标和安装类型。未知安装目录只展示，不猜测删除范围。

已支持用户目录中的 AppImage、能确认目录的 JetBrains IDE、JetBrains Toolbox，以及元数据明确匹配的 Electron 打包应用（包括 Antigravity IDE）的卸载预览。Toolbox 可按用户选择同时卸载其管理的 IDE：根据 `state.json` 的安装记录、自定义安装位置和 `product-info.json` 核对目录，再列出程序、配置、缓存、日志、桌面/自启入口、关联启动脚本和图标的实际路径与大小。项目目录不参与卸载，检测到项目标记的目录受保护。共享 JetBrains Daemon、系统凭据和无法确认归属的文件保留；不能宣称清除了所有外部痕迹。规则参考 [JetBrains 官方卸载说明](https://www.jetbrains.com/help/toolbox-app/installation.html#uninstall-the-toolbox-app)。

Electron 目录需同时匹配原生可执行文件、`resources/app/product.json` 的应用名、`package.json` 的名称及编译入口，并具有运行时资源；源码检出目录、带便携用户数据的目录或指向包外的入口不自动卸载。Antigravity IDE 仅使用产品元数据对应的配置名称，列出明确缓存与日志；配置文件默认不选，保留未保存备份、本地历史、工作区状态和用户扩展。

Linux 用户目录中的独立程序包也可预览卸载。AppClear 根据 `bin/` 与 `share/applications/` 布局核对包内清单、原生程序和图标，并按实际目标关联启动入口和命令行链接。

确认后程序及残留移入废纸篓，安装容器目录仅在为空时删除；不自动关闭进程，程序运行或文件变化时要求退出或重新预览。部分失败可使用同一预览重试未完成项。独立应用按实际处理条数显示进度；无法提供准确百分比的包管理器使用持续进度动画。成功后局部移除已卸载条目，保留搜索、筛选和列表位置，不自动整页重新扫描。

Toolbox 中取消某个 IDE 时，其缓存与关联入口同步取消；个人配置与账户项默认保留，后端再次验证归属及选择。原生包卸载先预览影响范围、依赖与容量，再执行同一份有效计划；危险基础软件、范围变化或身份变化拒绝执行。Homebrew 仅处理可确认单应用产物的 cask，以及无依赖影响、单个已安装版本的手动 formula；复杂安装器、脚本与多产物交给原渠道。Snap 卸载遵循渠道自身的数据处理行为，不能保证应用配置保留。

应用详情关联可靠归属的缓存路径、实际程序路径匹配的启动项与最近已检测更新。无法可靠匹配的关联不展示，未扫描的缓存容量显示未知。

### 通用更新检测与软件规则库

更新页同时展示系统软件包、桌面应用与命令行工具，CLI / GUI 类型分开标识，可筛选安装渠道、搜索安装路径、展开来源和检测说明、复制更新方式。一种安装渠道失败不会清空其他已完成结果。检测只读安装文件；包管理器可能写入自身查询缓存，不执行安装或更新。

APT 安装的桌面软件（例如 VS Code）按桌面入口或启动程序的包归属匹配本机候选版本。识别成功后，桌面应用行可直接使用「更新此项」，并沿用 APT 的完整版本、架构和原软件源。

| 检测器 | 范围与依据 |
| --- | --- |
| 原生包管理器 | Linux APT、DNF/YUM、Zypper、Flatpak、Arch checkupdates；macOS Homebrew。使用本地索引或管理器查询，按原渠道升级 |
| npm | 自动读取当前 npm 环境所有全局一级包，用 `npm outdated -g --json` 查询当前配置仓库，无需每个软件加入名单。参考 [npm 官方说明](https://docs.npmjs.com/cli/v11/commands/npm-outdated/) |
| uv / pipx | 读取工具清单并查询 PyPI 公开版本；不扫描项目虚拟环境、不执行第三方工具。私有源、固定版本、源码安装及非标准版本需按原安装渠道核对；pipx 明确的源码安装不按 PyPI 判断 |
| 独立二进制 | 软件规则库目前识别 Codex CLI，读取 `--version` 并对照官方 GitHub 稳定发布；预发布、本地较新和未知格式区分显示 |
| Codex GUI | 与 CLI 分开识别，也兼容新版包含 Codex 的 ChatGPT 桌面应用。macOS 读取 Bundle 标识、版本、构建号和 Mach-O 架构，查询官方 Sparkle Appcast 并过滤最低系统要求；Linux 通过 AppStream 标识、桌面入口与包管理器文件归属核对安装实例，查询同包格式、同架构的官方 DEB / RPM / Arch 仓库。支持 x64 / ARM64；已确认的 APT 安装在包名、当前版本、目标版本和架构与本机唯一候选一致时，可直接“更新此项”；其他 GUI 渠道提供原渠道建议 |
| 官方源码安装 | 软件规则库目前识别 Hermes Agent，从活动命令或启动包装器反查安装目录，核对官方 origin 与 main 分支，再比较提交。固定版本、非 main、分叉或本地修改明确提示；不执行 `fetch`、`pull` 或重启服务 |

独立工具规则在 [software-updates.json](src-tauri/rules/software-updates.json)，新增同类软件可扩展命令名、官方仓库、标签前缀、安装位置线索、来源和更新说明后重新构建；特殊安装器需新增适配器。规则不接受任意安装脚本。同一工具多份安装按检测器分别展示，CLI 与桌面应用不是同一更新目标。

常用 GUI 官方更新地址已整理为 [更新源清单](docs/gui-update-sources.md) 和 [结构化 JSON](docs/gui-update-sources.json)，包含开发工具、浏览器、办公通信、安全、影音及 macOS 工具。清单分别记录机器接口、官方发行页和下载入口、核实日期与可访问状态；收录不代表全部已接入检测。目前 GUI 适配器只接入 Codex。未知渠道、第三方 Linux 重打包、无法匹配架构/系统和读取失败都不会显示“已是最新”，也不会退回 CLI、其他平台或第三方源。

更新队列逐个执行并显示日志、目标版本、架构与失败原因，可仅重试失败项或停止后续。同名多架构按确认的架构与版本匹配，成功后保留其他架构的候选项。APT/DNF4 锁定确认版本，Flatpak 锁定安装范围、完整引用与提交；管理员助手执行前复核。npm/uv/pipx 的直接升级要求安装实例、公开原来源及版本约束可靠，检测许可有效 10 分钟，执行前再验证、执行后回读；npm 使用正常安装流程，包括包的安装脚本。私有源、固定版本、源码安装与未知来源仅提供建议。Homebrew、YUM、Zypper 和 DNF5 暂无可靠精确版本锁，保留原渠道建议；Arch 要求完整系统升级。

Codex GUI 的 APT 更新复用系统升级队列、管理员授权、固定版本助手和安装后回读，不执行 CLI 升级命令。候选索引尚未匹配官方版本时，该行提供“刷新软件源”；确认更新后再请求系统授权。系统包与 GUI 同时选中只执行一次，成功后两个列表同步；运行中的桌面应用可能需要重新打开才能使用新版本。相关队列回归验证：`pnpm test:updates`。

没有能可靠覆盖所有安装方式的统一接口：系统包、语言工具包、独立二进制、源码和应用内更新各有渠道。npm 自动覆盖通过该环境安装的 Claude Code、Gemini CLI、Kilo 等工具，不必逐个硬编码；没有通过已支持环境安装的程序不会仅凭名称猜测版本。GUI 启动时也检查常见用户和 Homebrew 命令目录，但不自动枚举所有 Node 版本管理器的历史安装环境。

网络查询会向 npm 当前配置仓库发送包查询，向 PyPI 发送工具包名，向 GitHub 请求规则库内的公开仓库与用于比较的安装提交；Codex GUI 只向 OpenAI 官方静态域名请求对应平台/架构的更新元数据，不跟随到其他域名。不发送项目文件、应用配置或密钥。HTTP 查询限制大小与超时，同一进程成功响应缓存 5 分钟，Python 工具每轮最多 40 个、4 个并行请求并设时间预算；超时/未查询不显示“已是最新”。Hermes 的主干提交比较不代表最新稳定发行版，实际更新渠道以 [Hermes 官方更新说明](https://nousresearch.github.io/hermes-agent/docs/getting-started/updating/) 和应用自身检查为准；Codex CLI 安装渠道见 [官方仓库](https://github.com/openai/codex)，GUI 安装渠道见 [官方桌面文档](https://developers.openai.com/codex/app)。

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

应用内底部状态栏显示 CPU 使用率、内存使用和用户目录所在磁盘的可用空间，点击进入监控页查看资源与网速曲线。默认启用，可在状态栏任务中心或监控页关闭并保存设置。默认每 2 秒采样，刷新频率可配置；macOS 菜单栏或 Linux 托盘启用时，隐藏主窗口仍会后台采样。磁盘统计每 30 秒更新，最近最多 60 次采样仅保留在本次会话中；关闭资源采样不影响任务进度显示。

Linux CPU 用累计计数差值计算，首次采样未知；内存使用量为总量减可用内存。macOS CPU、内存和网络通过原生计数器读取，内存按非空闲页面估算，包含可回收缓存，不等同于内存压力。指标读取失败显示未知并说明原因。数据仅在本机读取和显示。当前没有浮球、自动告警、进程终止或长期历史，应用退出后不继续采样。

## 运行与打包

需要 Rust stable、Node.js 24+、pnpm 11，及 [Tauri 系统依赖](https://tauri.app/start/prerequisites/)（Linux WebKitGTK 4.1；macOS Xcode Command Line Tools）。

```bash
pnpm install --frozen-lockfile
./run.sh              # 交互菜单，默认重新编译并运行
./run.sh rebuild      # 重新编译并运行，不生成安装包
./run.sh dev
./run.sh run
./run.sh build
./run.sh install
./run.sh uninstall
```

交互菜单按“运行”“打包与安装”“管理”分组：1 重新编译并运行（默认）、2 运行现有版本、3 开发模式、4 编译并生成安装包、5 编译并安装到本机、6 授权管理、7 卸载本机应用、0 退出。授权管理提供配置、移除组件和返回主菜单；macOS 使用系统授权。菜单卸载会显示目标并要求确认。

`./run.sh rebuild` 增量编译当前源码，成功后启动；`./run.sh run` 仅运行已有程序，程序不存在时提示先编译。菜单“编译并生成安装包”和 `./run.sh build` 都会重新构建前端、主程序与 Linux 授权助手，生成当前平台的应用包，默认产物目录为 `src-tauri/target/release/bundle/`。只编译可执行文件可用 `./run.sh build --no-bundle`。`install` 每次编译并安装后启动，`reinstall`、`force` 保留兼容。

脚本可从任意目录调用。Linux 安装用户级菜单入口；macOS 安装到 `~/Applications/AppClear.app`，旧应用移入废纸篓。Linux 支持 DEB/RPM/AppImage，macOS 支持 APP/DMG，须在对应系统构建。例如 `./run.sh build --bundles deb`。Mac 发布签名、公证和真实设备验收仍需完成。

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
