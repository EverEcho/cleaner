//! Read-only update providers. The registry never supplies executable update scripts.
use crate::{
    models::{ToolUpdate, UpgradablePackage},
    platform::{self, QueryCommand},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Serialize)]
pub struct UpdateChannel {
    pub id: String,
    pub provider: String,
    pub state: String,
    pub message: String,
    pub checked_at: String,
    pub source_path: String,
    pub cache_seconds: u64,
}
impl UpdateChannel {
    pub fn new(provider: &str, state: &str, message: &str, binary: Option<&Path>) -> Self {
        let source_path = binary
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            id: instance_id(provider, &source_path),
            provider: provider.into(),
            state: state.into(),
            message: message.into(),
            checked_at: chrono::Utc::now().to_rfc3339(),
            source_path,
            cache_seconds: 0,
        }
    }
}
pub struct NativeUpdateReport {
    pub packages: Vec<UpgradablePackage>,
    pub warnings: Vec<String>,
    pub channels: Vec<UpdateChannel>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct UpdateCheckReport {
    pub packages: Vec<UpgradablePackage>,
    pub tools: Vec<ToolUpdate>,
    pub warnings: Vec<String>,
    pub channels: Vec<UpdateChannel>,
    pub checked_at: String,
    pub expires_at: String,
    pub executable_tools: Vec<String>,
    pub automatic_managers: Vec<String>,
}
#[derive(Debug, Clone)]
struct ToolPermission {
    id: String,
    provider: String,
    name: String,
    environment_key: String,
    binary: PathBuf,
    install_path: PathBuf,
    metadata_path: PathBuf,
    metadata_hash: String,
    current: String,
    target: String,
    prefix: PathBuf,
}
struct RecentReport {
    report: UpdateCheckReport,
    permissions: HashMap<String, ToolPermission>,
    time: Instant,
}
static RECENT: OnceLock<Mutex<Option<RecentReport>>> = OnceLock::new();
pub fn latest_report() -> Option<UpdateCheckReport> {
    RECENT
        .get()?
        .lock()
        .ok()?
        .as_ref()
        .map(|r| r.report.clone())
}
pub fn record_native_success(manager: &str, name: &str) {
    record_native_success_scoped(manager, name, None);
}
fn remove_completed_package(
    report: &mut UpdateCheckReport,
    manager: &str,
    name: &str,
    arch: Option<&str>,
) {
    if arch.is_none()
        && report
            .packages
            .iter()
            .filter(|package| package.manager == manager && package.name == name)
            .count()
            != 1
    {
        return;
    }
    report.packages.retain(|package| {
        !(package.manager == manager
            && package.name == name
            && arch.is_none_or(|arch| package.arch == arch))
    });
}
pub fn record_native_success_scoped(manager: &str, name: &str, arch: Option<&str>) {
    if let Ok(mut recent) = RECENT.get_or_init(Mutex::default).lock()
        && let Some(recent) = recent.as_mut()
    {
        remove_completed_package(&mut recent.report, manager, name, arch);
    }
}

fn instance_id(provider: &str, identity: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "{provider}:{}",
        &hex::encode(Sha256::digest(identity.as_bytes()))[..20]
    )
}
fn file_hash(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let meta = path.metadata().ok()?;
    if meta.len() > 1024 * 1024 {
        return None;
    }
    Some(hex::encode(Sha256::digest(fs::read(path).ok()?)))
}
#[derive(Default)]
struct ProviderResult {
    tools: Vec<ToolUpdate>,
    channels: Vec<UpdateChannel>,
    permissions: Vec<ToolPermission>,
    warnings: Vec<String>,
}
fn plain_name(name: &str, npm: bool) -> bool {
    let unscoped = if npm && name.starts_with('@') {
        let Some((scope, package)) = name[1..].split_once('/') else {
            return false;
        };
        if scope.is_empty()
            || !scope
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
        {
            return false;
        }
        package
    } else {
        name
    };
    !unscoped.is_empty()
        && !unscoped.starts_with(['-', '.'])
        && unscoped
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}
fn public_python_configuration() -> bool {
    // Unknown/private sources keep their original-channel instructions. Never expose config values.
    if std::env::vars_os().any(|(key, _)| {
        let key = key.to_string_lossy();
        (key.starts_with("UV_")
            && (key.contains("INDEX") || key.contains("CONFIG") || key.contains("FIND_LINKS")))
            || (key.starts_with("PIP_")
                && (key.contains("INDEX")
                    || key.contains("CONFIG")
                    || key.contains("FIND_LINKS")
                    || key.contains("CONSTRAINT")))
    }) {
        return false;
    }
    let mut files = vec![
        PathBuf::from("/etc/uv/uv.toml"),
        PathBuf::from("/etc/pip.conf"),
    ];
    if let Some(home) = platform::home() {
        files.extend(
            [
                ".config/uv/uv.toml",
                ".config/pip/pip.conf",
                ".pip/pip.conf",
            ]
            .map(|p| home.join(p)),
        );
    }
    if let Some(config) = platform::xdg_dir("XDG_CONFIG_HOME", ".config") {
        files.extend([config.join("uv/uv.toml"), config.join("pip/pip.conf")]);
    }
    if let Ok(cwd) = std::env::current_dir() {
        for directory in cwd.ancestors() {
            files.extend([directory.join("uv.toml"), directory.join("pyproject.toml")]);
        }
    }
    !files.iter().any(|p| p.exists())
}

#[derive(Debug, Deserialize)]
struct Rule {
    id: String,
    name: String,
    command: String,
    provider: String,
    repository: String,
    tag_prefix: String,
    home_roots: Vec<String>,
    update_hint: String,
    source_url: String,
}
fn rules() -> Vec<Rule> {
    serde_json::from_str(include_str!("../rules/software-updates.json"))
        .expect("valid software rules")
}
fn tool_query(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    let mut paths = binary
        .parent()
        .map(Path::to_path_buf)
        .into_iter()
        .collect::<Vec<_>>();
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    command
        .env("NO_COLOR", "1")
        .env("NPM_CONFIG_UPDATE_NOTIFIER", "false");
    command
}
fn query(binary: &Path, args: &[&str], allow_one: bool) -> Result<Value, String> {
    let output = tool_query(binary)
        .args(args)
        .query_output()
        .map_err(|_| "查询超时或无法启动".to_string())?;
    if !output.status.success() && !(allow_one && output.status.code() == Some(1)) {
        return Err("查询失败；请检查网络、安装状态和工具权限".into());
    }
    let value: Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "返回数据无法解析".to_string())?;
    if value.get("error").is_some() {
        return Err("工具返回错误；请检查网络或仓库配置".into());
    }
    Ok(value)
}
type ResponseCache = Mutex<std::collections::HashMap<String, (Instant, Value)>>;
static CACHE: OnceLock<ResponseCache> = OnceLock::new();
fn remote_json(url: &str) -> Result<Value, String> {
    if !["https://api.github.com/repos/", "https://pypi.org/pypi/"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
    {
        return Err("不支持的软件源".into());
    }
    if let Some((time, data)) = CACHE
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|e| e.to_string())?
        .get(url)
        && time.elapsed() < Duration::from_secs(300)
    {
        return Ok(data.clone());
    }
    let curl = platform::command_path("curl").ok_or("未找到 curl，无法查询软件源")?;
    let output = Command::new(curl)
        .args([
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "4",
            "--max-time",
            "8",
            "--retry",
            "1",
            "--retry-delay",
            "1",
            "--retry-max-time",
            "15",
            "--max-filesize",
            "4194304",
            "--write-out",
            "\n%{http_code}",
            "--user-agent",
            "software-manager-update-check",
            url,
        ])
        .query_output()
        .map_err(|_| "软件源查询超时".to_string())?;
    let boundary = output
        .stdout
        .iter()
        .rposition(|byte| *byte == b'\n')
        .ok_or("软件源响应缺少状态")?;
    let (body, http) = (&output.stdout[..boundary], &output.stdout[boundary + 1..]);
    if !output.status.success() {
        let http = std::str::from_utf8(http).unwrap_or("000");
        return Err(format!(
            "软件源查询未完成（HTTP {http}，网络状态 {:?}）；请检查网络或软件源限流",
            output.status.code()
        ));
    }
    let data: Value =
        serde_json::from_slice(body).map_err(|_| "软件源返回数据无法解析".to_string())?;
    CACHE
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|e| e.to_string())?
        .insert(url.into(), (Instant::now(), data.clone()));
    Ok(data)
}
fn version(value: &str) -> Option<semver::Version> {
    let value = value.trim().trim_start_matches('v');
    let components = value.split('.').count();
    let normalized = if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
        match components {
            1 => format!("{value}.0.0"),
            2 => format!("{value}.0"),
            _ => value.into(),
        }
    } else {
        value.into()
    };
    semver::Version::parse(&normalized).ok()
}
fn compare_versions(row: &mut ToolUpdate) {
    row.status = match (version(&row.current_version), version(&row.latest_version)) {
        (Some(current), Some(latest)) if latest > current => "available",
        (Some(current), Some(latest)) if latest == current => "current",
        (Some(_), Some(_)) => "ahead",
        _ => "unknown",
    }
    .into();
    if row.status == "unknown" {
        row.note = "版本格式或安装渠道无法可靠比较，请按原渠道核对".into();
    }
}
fn parse_npm(installed: &Value, outdated: Result<&Value, &str>) -> Vec<ToolUpdate> {
    installed["dependencies"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, package)| {
            let mut row = ToolUpdate {
                id: format!("npm:{name}"),
                name: name.clone(),
                provider: "npm".into(),
                current_version: package["version"].as_str().unwrap_or("").into(),
                install_path: package["path"].as_str().unwrap_or("").into(),
                source_url: format!("https://www.npmjs.com/package/{name}"),
                update_hint: format!("npm install -g {name}@latest"),
                ..Default::default()
            };
            let non_registry = package["link"].as_bool().unwrap_or(false)
                || package["resolved"].as_str().is_some_and(|url| {
                    url.starts_with("git")
                        || url.starts_with("file:")
                        || url.starts_with("http:")
                        || url.starts_with("https://github.com/")
                });
            if non_registry {
                row.status = "unknown".into();
                row.update_hint =
                    "请按原源码或本地链接的安装方式更新；此处不替换为 npm 公开包".into();
                row.note = "源码/链接安装，保留原来源".into();
                return row;
            }
            match outdated {
                Ok(data) if data.get(name).is_some() => {
                    row.latest_version = data[name]["latest"].as_str().unwrap_or("").into();
                    if row.install_path.is_empty() {
                        row.install_path = data[name]["location"].as_str().unwrap_or("").into();
                    }
                    compare_versions(&mut row);
                }
                Ok(_)
                    if !package["resolved"]
                        .as_str()
                        .is_some_and(|url| url.starts_with("git") || url.starts_with("file:"))
                        && !package["link"].as_bool().unwrap_or(false)
                        && version(&row.current_version).is_some() =>
                {
                    row.status = "no-update".into();
                    row.note = "当前 npm 配置的软件源未报告更新".into();
                }
                Ok(_) => {
                    row.status = "unknown".into();
                    row.note = "本地链接或源码包需按原安装方式更新".into();
                }
                Err(error) => {
                    row.status = "failed".into();
                    row.note = error.into();
                }
            }
            row
        })
        .collect()
}
fn read_text(binary: &Path, args: &[&str]) -> Result<String, String> {
    let output = tool_query(binary)
        .args(args)
        .query_output()
        .map_err(|_| "本地安装清单读取超时或无法启动".to_string())?;
    if !output.status.success() {
        return Err("本地安装清单读取失败".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
fn effective_npm_registry(default: &Value, scoped: Option<&Value>) -> Option<String> {
    let value = match scoped {
        None | Some(Value::Null) => default,
        Some(Value::String(value)) if ["undefined", "null", ""].contains(&value.as_str()) => {
            default
        }
        Some(value) => value,
    };
    value
        .as_str()
        .map(|value| value.trim_end_matches('/').to_string())
}
fn npm_public_registry(binary: &Path, name: &str, default: Option<&Value>) -> bool {
    let Some(default) = default else {
        return false;
    };
    // A public scope on a private default still has private dependency-resolution rules.
    if effective_npm_registry(default, None).as_deref() != Some("https://registry.npmjs.org") {
        return false;
    }
    let scoped = if name.starts_with('@') {
        let Some((scope, _)) = name.split_once('/') else {
            return false;
        };
        let key = format!("{scope}:registry");
        match query(binary, &["config", "get", &key, "--json"], false) {
            Ok(value) => Some(value),
            Err(_) => return false,
        }
    } else {
        None
    };
    effective_npm_registry(default, scoped.as_ref()).as_deref()
        == Some("https://registry.npmjs.org")
}

fn npm_tools(cancelled: &dyn Fn() -> bool) -> ProviderResult {
    let mut result = ProviderResult::default();
    let binaries = tool_commands("npm");
    if binaries.is_empty() {
        result
            .channels
            .push(UpdateChannel::new("npm", "unsupported", "未安装 npm", None));
    }
    for npm in binaries {
        if cancelled() {
            result.channels.push(UpdateChannel::new(
                "npm",
                "cancelled",
                "已停止此渠道检测",
                Some(&npm),
            ));
            continue;
        }
        let installed = match query(&npm, &["ls", "-g", "--depth=0", "--long", "--json"], true) {
            Ok(data) => data,
            Err(error) => {
                result
                    .channels
                    .push(UpdateChannel::new("npm", "failed", &error, Some(&npm)));
                result.warnings.push(format!("npm: {error}"));
                continue;
            }
        };
        if cancelled() {
            result.channels.push(UpdateChannel::new(
                "npm",
                "cancelled",
                "已读取清单，未查询上游",
                Some(&npm),
            ));
            continue;
        }
        let outdated = query(
            &npm,
            &[
                "outdated",
                "-g",
                "--depth=0",
                "--json",
                "--fetch-retries=0",
                "--fetch-timeout=10000",
                "--ignore-scripts",
            ],
            true,
        );
        let mut rows = parse_npm(&installed, outdated.as_ref().map_err(String::as_str));
        let registry = query(&npm, &["config", "get", "registry", "--json"], false).ok();
        let prefix = read_text(&npm, &["prefix", "-g"]).ok().map(PathBuf::from);
        for row in &mut rows {
            let public = npm_public_registry(&npm, &row.name, registry.as_ref());
            if !public {
                row.source_url.clear();
            }
            row.id = instance_id(
                "npm",
                &format!(
                    "{}:{}:{}",
                    npm.display(),
                    prefix.as_deref().unwrap_or(Path::new("")).display(),
                    row.name
                ),
            );
            if row.install_path.is_empty()
                && let Some(prefix) = &prefix
            {
                row.install_path = prefix
                    .join("lib/node_modules")
                    .join(&row.name)
                    .to_string_lossy()
                    .into_owned();
            }
            let metadata_path = Path::new(&row.install_path).join("package.json");
            let metadata = fs::read_to_string(&metadata_path)
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());
            let pkg = &installed["dependencies"][&row.name];
            let resolved = pkg["resolved"]
                .as_str()
                .or_else(|| metadata.as_ref()?.get("_resolved")?.as_str())
                .unwrap_or("");
            let original = metadata
                .as_ref()
                .and_then(|m| m["_from"].as_str())
                .unwrap_or("");
            let unconstrained = original.is_empty()
                || original == row.name
                || original == format!("{}@latest", row.name);
            let path_matches = prefix.as_ref().is_some_and(|prefix| {
                match (
                    fs::canonicalize(&row.install_path),
                    fs::canonicalize(prefix.join("lib/node_modules").join(&row.name)),
                ) {
                    (Ok(actual), Ok(expected)) => actual == expected && actual.is_dir(),
                    _ => false,
                }
            });
            let trusted = public
                && plain_name(&row.name, true)
                && path_matches
                && unconstrained
                && resolved.starts_with("https://registry.npmjs.org/")
                && !pkg["link"].as_bool().unwrap_or(false)
                && metadata.as_ref().is_some_and(|m| {
                    m["name"].as_str() == Some(&row.name)
                        && m["version"].as_str() == Some(&row.current_version)
                });
            if trusted
                && row.status == "available"
                && let (Some(hash), Some(prefix)) = (file_hash(&metadata_path), &prefix)
            {
                result.permissions.push(ToolPermission {
                    id: row.id.clone(),
                    provider: "npm".into(),
                    name: row.name.clone(),
                    environment_key: row.name.clone(),
                    binary: npm.clone(),
                    install_path: PathBuf::from(&row.install_path),
                    metadata_path,
                    metadata_hash: hash,
                    current: row.current_version.clone(),
                    target: row.latest_version.clone(),
                    prefix: prefix.clone(),
                });
                row.note =
                    "已确认此 npm 全局实例与公开原软件源；升级锁定检测到的版本；npm 将按标准流程执行包的安装脚本"
                        .into();
            } else if row.status == "available" {
                row.note = "原来源、版本约束或安装实例未可靠确认；保留原渠道更新方式".into();
                if !unconstrained {
                    row.update_hint =
                        "此实例具有版本约束，请在原 npm 安装渠道确认约束后更新".into();
                }
                if !public {
                    row.source_url.clear();
                    row.update_hint =
                        "请在此安装实例的原 npm 软件源配置中核对并更新；保留私有源与版本约束"
                            .into();
                }
            }
        }
        let state = match &outdated {
            Err(_) => "failed",
            Ok(_) if rows.iter().any(|r| r.status == "available") => "completed",
            Ok(_) => "no-update",
        };
        result.channels.push(UpdateChannel::new(
            "npm",
            state,
            outdated
                .as_ref()
                .err()
                .map(String::as_str)
                .unwrap_or("查询此 npm 实例的配置软件源"),
            Some(&npm),
        ));
        if let Err(error) = outdated {
            result.warnings.push(format!("npm: {error}"));
        }
        result.tools.extend(rows);
    }
    result
}

fn python_row(provider: &str, name: &str, installed: &str, path: String) -> ToolUpdate {
    ToolUpdate {
        id: format!("{provider}:{name}"),
        name: name.into(),
        provider: provider.into(),
        current_version: installed.into(),
        install_path: path,
        status: "unknown".into(),
        source_url: format!("https://pypi.org/project/{name}/"),
        update_hint: format!(
            "{provider} {} {name}",
            if provider == "uv" {
                "tool upgrade"
            } else {
                "upgrade"
            }
        ),
        ..Default::default()
    }
}
fn parse_uv(text: &str) -> Vec<ToolUpdate> {
    text.lines()
        .filter_map(|line| {
            if line.starts_with(char::is_whitespace) || line.starts_with('-') {
                return None;
            }
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let installed = parts.next()?.strip_prefix('v')?;
            let path = line
                .split_once(" (")
                .map(|(_, p)| p.trim_end_matches(')').to_string())
                .unwrap_or_default();
            Some(python_row("uv", name, installed, path))
        })
        .collect()
}
fn plain_uv_receipt(text: &str, name: &str) -> bool {
    // Deliberately accept one documented, unconstrained receipt shape; unknown shapes are advisory.
    let wanted = format!("requirements=[{{name=\"{name}\"}}]");
    let requirement = text
        .lines()
        .find(|line| line.trim_start().starts_with("requirements"));
    requirement.is_some_and(|line| {
        line.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            == wanted
    }) && text
        .lines()
        .filter(|line| line.trim_start().starts_with('['))
        .all(|line| line.trim() == "[tool]")
        && !text.contains("specifier")
        && !text.contains("git+")
        && !text.contains("editable")
}
fn pipx_install_path(main: &Value) -> Option<PathBuf> {
    let app = main["app_paths"].as_array()?.first()?;
    let path = app.as_str().or_else(|| app["__Path__"].as_str())?;
    Path::new(path).parent()?.parent().map(Path::to_path_buf)
}
fn plain_pipx_source(main: &Value, name: &str) -> bool {
    main["package_or_url"].as_str() == Some(name)
        && !main["pinned"].as_bool().unwrap_or(false)
        && main["lock_file"].is_null()
        && main["suffix"].as_str().unwrap_or("").is_empty()
        && main["pip_args"]
            .as_array()
            .is_some_and(|args| args.is_empty())
}
fn python_tools(cancelled: &dyn Fn() -> bool) -> ProviderResult {
    let mut result = ProviderResult::default();
    let public = public_python_configuration();
    for provider in ["uv", "pipx"] {
        let binaries = tool_commands(provider);
        if binaries.is_empty() {
            result.channels.push(UpdateChannel::new(
                provider,
                "unsupported",
                "未安装对应工具管理器",
                None,
            ));
        }
        for binary in binaries {
            if cancelled() {
                result.channels.push(UpdateChannel::new(
                    provider,
                    "cancelled",
                    "已停止此渠道检测",
                    Some(&binary),
                ));
                continue;
            }
            let mut rows = Vec::new();
            let mut candidates = Vec::new();
            let inventory = if provider == "uv" {
                read_text(&binary, &["tool", "list", "--show-paths"]).map(|text| {
                    rows = parse_uv(&text);
                    for row in &mut rows {
                        let root = PathBuf::from(&row.install_path);
                        let receipt = root.join("uv-receipt.toml");
                        let safe = public
                            && root.is_absolute()
                            && fs::read_to_string(&receipt)
                                .is_ok_and(|text| plain_uv_receipt(&text, &row.name));
                        row.id = instance_id(
                            provider,
                            &format!("{}:{}:{}", binary.display(), root.display(), row.name),
                        );
                        if safe && let Some(hash) = file_hash(&receipt) {
                            candidates.push(ToolPermission {
                                id: row.id.clone(),
                                provider: provider.into(),
                                name: row.name.clone(),
                                environment_key: row.name.clone(),
                                binary: binary.clone(),
                                install_path: root.clone(),
                                metadata_path: receipt,
                                metadata_hash: hash,
                                current: row.current_version.clone(),
                                target: String::new(),
                                prefix: root.parent().unwrap_or(Path::new("")).to_path_buf(),
                            });
                        } else {
                            row.note =
                                "安装来源、固定版本或自定义配置未可靠确认；按原 uv 安装配置更新"
                                    .into();
                        }
                    }
                })
            } else {
                query(&binary, &["list", "--json"], false).map(|data| {
                    for (key, info) in data["venvs"].as_object().into_iter().flatten() {
                        let main = &info["metadata"]["main_package"];
                        let name = main["package"].as_str().unwrap_or(key);
                        let path = pipx_install_path(main).unwrap_or_default();
                        let mut row = python_row(
                            provider,
                            name,
                            main["package_version"].as_str().unwrap_or(""),
                            path.to_string_lossy().into_owned(),
                        );
                        row.id = instance_id(
                            provider,
                            &format!("{}:{}:{key}", binary.display(), path.display()),
                        );
                        let metadata_path = path.join("pipx_metadata.json");
                        if public
                            && path.is_absolute()
                            && plain_pipx_source(main, name)
                            && let Some(hash) = file_hash(&metadata_path)
                        {
                            candidates.push(ToolPermission {
                                id: row.id.clone(),
                                provider: provider.into(),
                                name: name.into(),
                                environment_key: key.clone(),
                                binary: binary.clone(),
                                install_path: path.clone(),
                                metadata_path,
                                metadata_hash: hash,
                                current: row.current_version.clone(),
                                target: String::new(),
                                prefix: path
                                    .parent()
                                    .and_then(Path::parent)
                                    .unwrap_or(Path::new(""))
                                    .to_path_buf(),
                            });
                        } else {
                            row.note =
                                "私有源、固定版本、源码或安装元数据未可靠确认；按原 pipx 配置更新"
                                    .into();
                        }
                        rows.push(row);
                    }
                })
            };
            if let Err(error) = inventory {
                result.channels.push(UpdateChannel::new(
                    provider,
                    "failed",
                    &error,
                    Some(&binary),
                ));
                result.warnings.push(format!("{provider}: {error}"));
                continue;
            }
            let start = Instant::now();
            for (batch_number, batch) in rows.chunks_mut(4).enumerate() {
                if cancelled() || batch_number >= 10 || start.elapsed() >= Duration::from_secs(30) {
                    for row in batch {
                        row.note = "本轮已停止或查询预算用完，尚未检测上游".into();
                    }
                    continue;
                }
                std::thread::scope(|scope| {
                    for row in batch {
                        scope.spawn(move || {
                            if !row.note.is_empty() || !plain_name(&row.name, false) {
                                return;
                            }
                            match remote_json(&format!("https://pypi.org/pypi/{}/json", row.name)) {
                                Ok(data) => {
                                    row.latest_version =
                                        data["info"]["version"].as_str().unwrap_or("").into();
                                    compare_versions(row);
                                }
                                Err(error) => {
                                    row.status = "failed".into();
                                    row.note = error;
                                }
                            }
                        });
                    }
                });
            }
            for row in &mut rows {
                if row.status == "available"
                    && let Some(mut permission) =
                        candidates.iter().find(|p| p.id == row.id).cloned()
                {
                    permission.target = row.latest_version.clone();
                    result.permissions.push(permission);
                    row.note =
                        "已确认原渠道无版本约束；升级限定本次检测版本，并重新核对安装实例".into();
                }
            }
            let state = if cancelled() {
                "cancelled"
            } else if rows.iter().any(|row| row.status == "failed") {
                "failed"
            } else if rows.iter().any(|row| row.status == "unknown") {
                "unsupported"
            } else if rows.iter().any(|row| row.status == "available") {
                "completed"
            } else {
                "no-update"
            };
            let mut channel = UpdateChannel::new(
                provider,
                state,
                "按本机安装收据核对来源与版本；上游响应最多缓存 5 分钟",
                Some(&binary),
            );
            channel.cache_seconds = 300;
            result.channels.push(channel);
            result.tools.extend(rows);
        }
    }
    result
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let binary = platform::command_path("git").ok_or("未找到 git")?;
    let output = Command::new(binary)
        .args(["-c", "core.fsmonitor=false", "--no-optional-locks"])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .query_output()
        .map_err(|_| "Git 元数据读取失败")?;
    if !output.status.success() {
        return Err("Git 元数据读取失败".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
fn official_origin(origin: &str, repository: &str) -> bool {
    let origin = origin.trim_end_matches('/').trim_end_matches(".git");
    [
        format!("https://github.com/{repository}"),
        format!("git@github.com:{repository}"),
        format!("ssh://git@github.com/{repository}"),
    ]
    .iter()
    .any(|allowed| origin.eq_ignore_ascii_case(allowed))
}
fn source_root(rule: &Rule, binary: &Path) -> Option<PathBuf> {
    let mut anchors = fs::canonicalize(binary).into_iter().collect::<Vec<_>>();
    // Read launchers as data. Associate the active command with its install, not leftover roots.
    if let Ok(meta) = binary.metadata()
        && meta.len() < 8192
        && let Ok(text) = fs::read_to_string(binary)
    {
        for token in text.split(['"', '\'']) {
            let path = Path::new(token);
            if path.is_absolute()
                && path.is_file()
                && let Ok(path) = fs::canonicalize(path)
            {
                anchors.push(path);
            }
        }
    }
    let mut roots = anchors
        .iter()
        .flat_map(|anchor| anchor.ancestors().skip(1).take(4).map(Path::to_path_buf))
        .collect::<Vec<_>>();
    if let Some(home) = platform::home() {
        roots.extend(rule.home_roots.iter().map(|p| home.join(p)));
    }
    roots.into_iter().find(|root| {
        anchors.iter().any(|anchor| anchor.starts_with(root))
            && root.join(".git").exists()
            && git(root, &["config", "--get", "remote.origin.url"])
                .is_ok_and(|origin| official_origin(&origin, &rule.repository))
    })
}

fn source_check(row: &mut ToolUpdate, rule: &Rule, binary: &Path) -> Result<(), String> {
    let root = source_root(rule, binary).ok_or("未确认官方源码安装目录；请按原安装渠道检查更新")?;
    row.install_path = root.to_string_lossy().into_owned();
    let head = git(&root, &["rev-parse", "HEAD"])?;
    let branch = git(&root, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_default();
    row.current_version = head.get(..12).unwrap_or(&head).into();
    if branch != "main" {
        return Err("安装处于固定版本或非 main 分支；保留当前渠道，请使用应用自身更新检查".into());
    }
    if head.len() != 40 || !head.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("安装提交无法识别".into());
    }
    let latest = remote_json(&format!(
        "https://api.github.com/repos/{}/commits/main",
        rule.repository
    ))?;
    let sha = latest["sha"]
        .as_str()
        .filter(|sha| sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or("上游提交无法识别")?;
    row.latest_version = sha[..12].into();
    if sha == head {
        row.status = "current".into();
    } else {
        let comparison = remote_json(&format!(
            "https://api.github.com/repos/{}/compare/{head}...{sha}?per_page=1",
            rule.repository
        ))?;
        row.status = match comparison["status"].as_str() {
            Some("ahead") => "available",
            Some("behind") => "ahead",
            Some("identical") => "current",
            _ => "unknown",
        }
        .into();
        if row.status == "unknown" {
            row.note = "本地与上游已分叉，请在更新前处理本地修改".into();
        }
    }
    if !git(&root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        row.note = "安装目录存在本地修改，更新前请检查并保留修改".into();
    }
    if row.note.is_empty() {
        row.note = "比较官方 origin/main 的提交；不是发布版本号，不执行 fetch 或修改源码".into();
    }
    Ok(())
}
fn registry_tools(cancelled: &dyn Fn() -> bool) -> ProviderResult {
    let mut result = ProviderResult::default();
    for rule in rules() {
        let Some(binary) = tool_command(&rule.command) else {
            result.channels.push(UpdateChannel::new(
                &rule.provider,
                "unsupported",
                &format!("未找到 {} 的活动安装", rule.name),
                None,
            ));
            continue;
        };
        if cancelled() {
            result.channels.push(UpdateChannel::new(
                &rule.provider,
                "cancelled",
                "已停止此渠道检测",
                Some(&binary),
            ));
            continue;
        }
        let mut row = ToolUpdate {
            id: instance_id(&rule.provider, &format!("{}:{}", rule.id, binary.display())),
            name: rule.name.clone(),
            provider: rule.provider.clone(),
            install_path: binary.to_string_lossy().into_owned(),
            source_url: rule.source_url.clone(),
            update_hint: rule.update_hint.clone(),
            status: "unknown".into(),
            ..Default::default()
        };
        let check = if rule.provider == "github-branch" {
            source_check(&mut row, &rule, &binary)
        } else {
            release_check(&mut row, &rule, &binary)
        };
        if let Err(error) = check {
            row.status = "failed".into();
            row.note = error;
        }
        let mut channel = UpdateChannel::new(
            &rule.provider,
            match row.status.as_str() {
                "failed" => "failed",
                "unknown" => "unsupported",
                "available" => "completed",
                _ => "no-update",
            },
            &format!("{}：{}", rule.name, row.note),
            Some(&binary),
        );
        channel.id = instance_id(&rule.provider, &format!("{}:{}", rule.id, binary.display()));
        channel.cache_seconds = 300;
        result.channels.push(channel);
        result.tools.push(row);
    }
    result
}

fn tool_commands(name: &str) -> Vec<PathBuf> {
    let mut paths = std::env::var_os("PATH")
        .map(|value| {
            std::env::split_paths(&value)
                .map(|p| p.join(name))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if let Some(home) = platform::home() {
        paths.extend([".local/bin", ".cargo/bin", ".bun/bin"].map(|p| home.join(p).join(name)));
    }
    paths.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(|p| Path::new(p).join(name)));
    let mut seen = std::collections::HashSet::new();
    paths
        .into_iter()
        .filter(|path| path.is_file())
        .filter(|path| seen.insert(fs::canonicalize(path).unwrap_or_else(|_| path.clone())))
        .collect()
}
fn tool_command(name: &str) -> Option<PathBuf> {
    platform::command_path(name)
        .or_else(|| {
            platform::home().and_then(|home| {
                [".local/bin", ".cargo/bin", ".bun/bin"]
                    .iter()
                    .map(|p| home.join(p).join(name))
                    .find(|p| p.is_file())
            })
        })
        .or_else(|| {
            ["/opt/homebrew/bin", "/usr/local/bin"]
                .iter()
                .map(|dir| Path::new(dir).join(name))
                .find(|path| path.is_file())
        })
}
fn release_check(row: &mut ToolUpdate, rule: &Rule, binary: &Path) -> Result<(), String> {
    // Only built-in release rules permit --version. Unknown launchers are never probed.
    let output = tool_query(binary)
        .arg("--version")
        .query_output()
        .map_err(|_| "读取本地版本失败")?;
    if !output.status.success() {
        return Err("读取本地版本失败".into());
    }
    row.current_version = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .find(|token| version(token).is_some())
        .unwrap_or("")
        .into();
    let data = remote_json(&format!(
        "https://api.github.com/repos/{}/releases/latest",
        rule.repository
    ))?;
    if data["draft"].as_bool().unwrap_or(true) || data["prerelease"].as_bool().unwrap_or(true) {
        return Err("上游未提供稳定发布版本".into());
    }
    row.latest_version = data["tag_name"]
        .as_str()
        .and_then(|tag| tag.strip_prefix(&rule.tag_prefix))
        .ok_or("上游版本标签无法识别")?
        .into();
    compare_versions(row);
    Ok(())
}
pub fn check() -> UpdateCheckReport {
    check_with_progress(|| false, |_, _| {})
}
pub fn check_with_progress(
    cancelled: impl Fn() -> bool + Sync,
    progress: impl Fn(&str, u64) + Sync,
) -> UpdateCheckReport {
    let checked_at = chrono::Utc::now();
    progress("读取系统软件源候选版本", 0);
    let native = crate::updates::check_with_channels(&cancelled);
    let mut report = UpdateCheckReport {
        packages: native.packages,
        warnings: native.warnings,
        channels: native.channels,
        checked_at: checked_at.to_rfc3339(),
        expires_at: (checked_at + chrono::Duration::minutes(10)).to_rfc3339(),
        ..Default::default()
    };
    report.automatic_managers = report
        .channels
        .iter()
        .filter(|channel| {
            matches!(channel.state.as_str(), "completed" | "no-update")
                && matches!(channel.provider.as_str(), "apt" | "flatpak")
        })
        .map(|channel| channel.provider.clone())
        .collect();
    if !cancelled()
        && report.channels.iter().any(|channel| {
            channel.provider == "dnf" && matches!(channel.state.as_str(), "completed" | "no-update")
        })
        && crate::native_update_plan::dnf4_supported()
    {
        report.automatic_managers.push("dnf".into());
    }
    report.automatic_managers.sort();
    report.automatic_managers.dedup();
    let mut permissions = HashMap::new();
    for (label, provider) in [
        (
            "检测各 npm 全局安装实例",
            npm_tools as fn(&dyn Fn() -> bool) -> ProviderResult,
        ),
        (
            "检测 uv / pipx 安装收据与公开版本",
            python_tools as fn(&dyn Fn() -> bool) -> ProviderResult,
        ),
        (
            "核对独立安装与官方源码渠道",
            registry_tools as fn(&dyn Fn() -> bool) -> ProviderResult,
        ),
    ] {
        progress(label, report.channels.len() as u64);
        let result = provider(&cancelled);
        report.tools.extend(result.tools);
        report.channels.extend(result.channels);
        report.warnings.extend(result.warnings);
        for permission in result.permissions {
            report.executable_tools.push(permission.id.clone());
            permissions.insert(permission.id.clone(), permission);
        }
    }
    report.tools.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    progress(
        if cancelled() {
            "检测已停止，保留已完成渠道"
        } else {
            "安装渠道检测完成"
        },
        report.channels.len() as u64,
    );
    if let Ok(mut recent) = RECENT.get_or_init(Mutex::default).lock() {
        *recent = Some(RecentReport {
            report: report.clone(),
            permissions,
            time: Instant::now(),
        });
    }
    report
}
fn permission_args(
    permission: &ToolPermission,
    constraint: Option<&Path>,
) -> Result<Vec<String>, String> {
    if !plain_name(&permission.name, permission.provider == "npm")
        || !plain_name(&permission.environment_key, permission.provider == "npm")
        || version(&permission.current).is_none()
        || version(&permission.target).is_none()
        || permission.target.starts_with('-')
    {
        return Err("软件名或版本不可可靠执行；请按原渠道更新".into());
    }
    let target = format!("{}=={}", permission.name, permission.target);
    Ok(match permission.provider.as_str() {
        "npm" => vec![
            "install".into(),
            "-g".into(),
            "--prefix".into(),
            permission.prefix.to_string_lossy().into_owned(),
            "--registry=https://registry.npmjs.org/".into(),
            "--".into(),
            format!("{}@{}", permission.name, permission.target),
        ],
        "uv" => vec![
            "tool".into(),
            "upgrade".into(),
            "--no-config".into(),
            "--default-index=https://pypi.org/simple".into(),
            "--upgrade-package".into(),
            target,
            "--".into(),
            permission.environment_key.clone(),
        ],
        "pipx" => vec![
            "upgrade".into(),
            permission.environment_key.clone(),
            format!(
                "--pip-args=--constraint={}",
                constraint.ok_or("缺少版本约束文件")?.display()
            ),
        ],
        _ => return Err("此安装渠道仅提供更新建议".into()),
    })
}
fn local_version(permission: &ToolPermission) -> Result<String, String> {
    match permission.provider.as_str() {
        "npm" => {
            let data = query(
                &permission.binary,
                &["ls", "-g", "--depth=0", "--long", "--json"],
                true,
            )?;
            data["dependencies"][&permission.name]["version"]
                .as_str()
                .map(str::to_string)
                .ok_or("该 npm 安装实例已不存在".into())
        }
        "uv" => parse_uv(&read_text(
            &permission.binary,
            &["tool", "list", "--show-paths"],
        )?)
        .into_iter()
        .find(|row| {
            row.name == permission.name && Path::new(&row.install_path) == permission.install_path
        })
        .map(|row| row.current_version)
        .ok_or("该 uv 安装实例已变化".into()),
        "pipx" => {
            let data = query(&permission.binary, &["list", "--json"], false)?;
            let main = &data["venvs"][&permission.environment_key]["metadata"]["main_package"];
            if pipx_install_path(main).as_ref() != Some(&permission.install_path)
                || !plain_pipx_source(main, &permission.name)
            {
                return Err("该 pipx 安装实例或版本约束已变化".into());
            }
            main["package_version"]
                .as_str()
                .map(str::to_string)
                .ok_or("无法读取 pipx 已安装版本".into())
        }
        _ => Err("不支持的工具管理器".into()),
    }
}
pub fn upgrade_tool(id: &str) -> Result<String, String> {
    upgrade_tool_with_progress(id, |_| {})
}
pub fn upgrade_tool_with_progress(
    id: &str,
    mut progress: impl FnMut(&str),
) -> Result<String, String> {
    static EXECUTING: Mutex<()> = Mutex::new(());
    let _gate = EXECUTING
        .try_lock()
        .map_err(|_| "已有工具正在升级，请等待队列完成")?;
    progress("核对最近检测许可、原安装实例与版本约束");
    let permission = {
        let recent = RECENT
            .get_or_init(Mutex::default)
            .lock()
            .map_err(|e| e.to_string())?;
        let recent = recent.as_ref().ok_or("请先检查更新")?;
        if recent.time.elapsed() > Duration::from_secs(600) {
            return Err("检测结果已超过 10 分钟，请重新检查更新".into());
        }
        recent
            .permissions
            .get(id)
            .cloned()
            .ok_or("此安装实例仅支持按原渠道更新建议")?
    };
    if !permission.binary.is_file()
        || file_hash(&permission.metadata_path).as_deref() != Some(&permission.metadata_hash)
        || local_version(&permission)? != permission.current
    {
        return Err("安装位置、来源或当前版本已变化，请重新检查更新".into());
    }
    if permission.provider != "npm" && !public_python_configuration() {
        return Err("Python 软件源配置已变化，请按原渠道更新".into());
    }
    if permission.provider == "npm" {
        let registry = query(
            &permission.binary,
            &["config", "get", "registry", "--json"],
            false,
        )?;
        let prefix = read_text(&permission.binary, &["prefix", "-g"])?;
        if !npm_public_registry(&permission.binary, &permission.name, Some(&registry))
            || Path::new(&prefix) != permission.prefix
        {
            return Err("npm 来源或全局安装实例已变化，请重新检查更新".into());
        }
    }
    let constraint = if permission.provider == "pipx" {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "cleaner-update-{}-{}.txt",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .map_err(|_| "无法创建固定版本约束文件")?;
        file.write_all(format!("{}=={}\n", permission.name, permission.target).as_bytes())
            .map_err(|_| "无法写入版本约束")?;
        Some(path)
    } else {
        None
    };
    let args = permission_args(&permission, constraint.as_deref())?;
    let mut command = tool_query(&permission.binary);
    command.args(args).stdin(std::process::Stdio::null());
    if permission.provider == "uv" {
        command.env("UV_TOOL_DIR", &permission.prefix);
    }
    if permission.provider == "pipx" {
        command
            .env("PIPX_HOME", &permission.prefix)
            .env("PIP_CONFIG_FILE", "/dev/null")
            .env("PIP_INDEX_URL", "https://pypi.org/simple");
    }
    progress(&format!(
        "通过原 {} 管理器升级 {} 至 {}",
        permission.provider, permission.name, permission.target
    ));
    let execution = command.output();
    if let Some(path) = constraint {
        let _ = fs::remove_file(path);
    }
    let output = execution.map_err(|e| format!("无法启动原工具管理器：{e}"))?;
    let logs = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let logs = logs.chars().take(12000).collect::<String>();
    if !logs.trim().is_empty() {
        progress(logs.trim());
    }
    if !output.status.success() {
        return Err(format!(
            "{} 升级失败（退出码 {:?}）；仅重试本安装实例",
            permission.name,
            output.status.code()
        ));
    }
    progress("重新读取原渠道清单，核对实际安装版本");
    let actual = local_version(&permission)?;
    if actual != permission.target {
        return Err(format!(
            "命令已结束，但实际版本为 {actual}，未达到检测版本 {}；请重新检查更新",
            permission.target
        ));
    }
    if let Ok(mut recent) = RECENT.get_or_init(Mutex::default).lock()
        && let Some(recent) = recent.as_mut()
    {
        recent.permissions.remove(id);
        recent.report.executable_tools.retain(|key| key != id);
        if let Some(row) = recent.report.tools.iter_mut().find(|row| row.id == id) {
            row.current_version = actual;
            row.status = "current".into();
        }
    }
    Ok(format!(
        "{} 已通过 {} 更新至 {}",
        permission.name, permission.provider, permission.target
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_success_keeps_other_architectures_and_channels() {
        let package = |manager: &str, arch: &str| UpgradablePackage {
            manager: manager.into(),
            name: "fixture-tool".into(),
            arch: arch.into(),
            current_version: "1.0-1".into(),
            new_version: "2.0-1".into(),
            repository: "updates".into(),
        };
        let mut report = UpdateCheckReport {
            packages: vec![
                package("dnf", "x86_64"),
                package("dnf", "i686"),
                package("apt", "amd64"),
            ],
            ..Default::default()
        };
        remove_completed_package(&mut report, "dnf", "fixture-tool", None);
        assert_eq!(
            report.packages.len(),
            3,
            "legacy acknowledgement cannot erase ambiguous instances"
        );
        remove_completed_package(&mut report, "dnf", "fixture-tool", Some("x86_64"));
        assert_eq!(report.packages.len(), 2);
        assert!(
            report
                .packages
                .iter()
                .any(|package| package.manager == "dnf" && package.arch == "i686")
        );
        assert!(
            report
                .packages
                .iter()
                .any(|package| package.manager == "apt" && package.arch == "amd64")
        );
        remove_completed_package(&mut report, "dnf", "fixture-tool", None);
        assert_eq!(report.packages.len(), 1);
        assert_eq!(report.packages[0].manager, "apt");
    }
    fn permission(provider: &str) -> ToolPermission {
        ToolPermission {
            id: "fixture".into(),
            provider: provider.into(),
            name: "ruff".into(),
            environment_key: "ruff".into(),
            binary: "/tools/bin/manager".into(),
            install_path: "/tools/ruff".into(),
            metadata_path: "/tools/ruff/receipt".into(),
            metadata_hash: "hash".into(),
            current: "0.9.0".into(),
            target: "0.10.0".into(),
            prefix: "/tools".into(),
        }
    }
    #[test]
    fn execution_commands_use_original_instances_and_exact_versions() {
        let mut npm = permission("npm");
        npm.name = "@scope/tool".into();
        npm.environment_key = npm.name.clone();
        let args = permission_args(&npm, None).unwrap();
        assert!(!args.contains(&"--ignore-scripts".into()));
        assert!(args.contains(&"--prefix".into()));
        assert_eq!(args.last().unwrap(), "@scope/tool@0.10.0");
        assert!(!args.iter().any(|arg| arg.contains("latest")));
        let uv = permission("uv");
        assert!(
            permission_args(&uv, None)
                .unwrap()
                .contains(&"ruff==0.10.0".into())
        );
        let pipx = permission("pipx");
        assert!(permission_args(&pipx, None).is_err());
        assert_eq!(
            permission_args(&pipx, Some(Path::new("/tmp/constraint.txt")))
                .unwrap()
                .last()
                .unwrap(),
            "--pip-args=--constraint=/tmp/constraint.txt"
        );
        assert!(permission_args(&permission("shell"), None).is_err());
        let mut bad_version = permission("uv");
        bad_version.target = "0.10.0; rm".into();
        assert!(permission_args(&bad_version, None).is_err());
        for bad in [
            "--force",
            "../tool",
            "name;rm",
            "name$(id)",
            "name/extra",
            "-bad",
        ] {
            let mut row = permission("uv");
            row.name = bad.into();
            assert!(permission_args(&row, None).is_err());
        }
    }
    #[test]
    fn npm_scope_configuration_overrides_default_registry() {
        let public = serde_json::json!("https://registry.npmjs.org/");
        let private = serde_json::json!("https://private.example/npm/");
        assert_eq!(
            effective_npm_registry(&public, Some(&private)).as_deref(),
            Some("https://private.example/npm")
        );
        assert_eq!(
            effective_npm_registry(&private, Some(&public)).as_deref(),
            Some("https://registry.npmjs.org")
        );
        for missing in [
            Value::Null,
            serde_json::json!("undefined"),
            serde_json::json!(""),
        ] {
            assert_eq!(
                effective_npm_registry(&public, Some(&missing)).as_deref(),
                Some("https://registry.npmjs.org")
            );
        }
        assert!(
            effective_npm_registry(&public, Some(&serde_json::json!({"unexpected":"shape"})))
                .is_none()
        );
    }
    #[test]
    fn stale_and_unknown_capabilities_reject_before_starting_commands() {
        let previous = RECENT.get_or_init(Mutex::default).lock().unwrap().take();
        let mut permissions = HashMap::new();
        let item = permission("uv");
        permissions.insert(item.id.clone(), item);
        *RECENT.get_or_init(Mutex::default).lock().unwrap() = Some(RecentReport {
            report: UpdateCheckReport::default(),
            permissions,
            time: Instant::now() - Duration::from_secs(601),
        });
        assert!(upgrade_tool("fixture").unwrap_err().contains("10 分钟"));
        RECENT
            .get_or_init(Mutex::default)
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .time = Instant::now();
        assert!(upgrade_tool("arbitrary-id").unwrap_err().contains("原渠道"));
        *RECENT.get_or_init(Mutex::default).lock().unwrap() = previous;
    }
    #[test]
    fn python_sources_reject_pins_private_options_and_unknown_metadata() {
        assert!(plain_uv_receipt(
            "[tool]\nrequirements = [{ name = \"ruff\" }]\nentrypoints = []",
            "ruff"
        ));
        for text in [
            "[tool]\nrequirements = [{ name = \"ruff\", specifier = \"==0.9.0\" }]",
            "[tool]\nrequirements = [{ name = \"ruff\" }]\n[tool.options]\nindex = []",
            "[tool]\nrequirements = [{ name = \"ruff\" }, { name = \"other\" }]",
        ] {
            assert!(!plain_uv_receipt(text, "ruff"));
        }
        let main = serde_json::json!({"package_or_url":"ruff", "pip_args":[], "pinned":false, "app_paths":[{"__type__":"Path","__Path__":"/home/test/pipx/venvs/ruff/bin/ruff"}]});
        assert!(plain_pipx_source(&main, "ruff"));
        assert_eq!(
            pipx_install_path(&main).unwrap(),
            Path::new("/home/test/pipx/venvs/ruff")
        );
        for patch in [
            serde_json::json!({"pinned":true}),
            serde_json::json!({"package_or_url":"ruff==0.9.0"}),
            serde_json::json!({"pip_args":["--index-url=private"]}),
            serde_json::json!({"lock_file":"/tmp/lock"}),
        ] {
            let mut copy = main.clone();
            for (key, value) in patch.as_object().unwrap() {
                copy[key] = value.clone();
            }
            assert!(!plain_pipx_source(&copy, "ruff"));
        }
    }
    #[test]
    fn channel_states_and_installations_do_not_collapse_by_name() {
        let a = instance_id("uv", "/tools/a/ruff");
        let b = instance_id("uv", "/tools/b/ruff");
        let c = instance_id("pipx", "/tools/a/ruff");
        assert_ne!(a, b);
        assert_ne!(a, c);
        let report = UpdateCheckReport {
            channels: vec![
                UpdateChannel::new(
                    "npm",
                    "no-update",
                    "completed with zero rows",
                    Some(Path::new("/a/npm")),
                ),
                UpdateChannel::new(
                    "npm",
                    "failed",
                    "network failure",
                    Some(Path::new("/b/npm")),
                ),
            ],
            ..Default::default()
        };
        assert!(report.tools.is_empty());
        assert_eq!(report.channels.len(), 2);
        assert_ne!(report.channels[0].id, report.channels[1].id);
        assert!(serde_json::to_value(report).unwrap()["channels"][0]["checked_at"].is_string());
    }
    #[test]
    fn npm_failure_is_unknown_and_latest_can_be_lower_than_installed() {
        let installed = serde_json::json!({"dependencies":{"@openai/codex":{"version":"1.2.0"},"local":{"version":"2.0.0","link":true}}});
        let failed = parse_npm(&installed, Err("timeout"));
        assert_eq!(failed[0].status, "failed");
        assert_eq!(failed[1].status, "unknown");
        let data = serde_json::json!({"@openai/codex":{"latest":"1.1.0"}});
        let rows = parse_npm(&installed, Ok(&data));
        assert_eq!(rows[0].status, "ahead");
        assert_eq!(rows[1].status, "unknown");
    }
    #[test]
    fn conservative_versions_and_python_inventory() {
        let mut row = python_row("uv", "ruff", "0.9.0", "/tools/ruff".into());
        row.latest_version = "0.10.0".into();
        compare_versions(&mut row);
        assert_eq!(row.status, "available");
        row.current_version = "1.0rc1".into();
        compare_versions(&mut row);
        assert_eq!(row.status, "unknown");
        let rows = parse_uv("ruff v0.9.0 (/home/test/tools/ruff)\n- ruff\n");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].install_path, "/home/test/tools/ruff");
    }
    #[test]
    fn source_identity_and_registry_are_explicit() {
        assert!(official_origin(
            "git@github.com:NousResearch/hermes-agent.git",
            "NousResearch/hermes-agent"
        ));
        assert!(!official_origin(
            "https://github.com/attacker/hermes-agent.git",
            "NousResearch/hermes-agent"
        ));
        assert!(!official_origin(
            "https://github.com/NousResearch/hermes-agent/extra",
            "NousResearch/hermes-agent"
        ));
        assert_eq!(rules().len(), 2);
    }
    #[test]
    fn source_install_is_associated_with_active_wrapper_and_verified_origin() {
        let base =
            std::env::temp_dir().join(format!("appsteward-updates-source-{}", std::process::id()));
        let root = base.join("custom-hermes");
        fs::create_dir_all(root.join(".git/refs/heads")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join("venv/bin")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            root.join(".git/refs/heads/main"),
            format!("{}\n", "a".repeat(40)),
        )
        .unwrap();
        fs::write(
            root.join(".git/config"),
            "[remote \"origin\"]\nurl = https://github.com/NousResearch/hermes-agent.git\n",
        )
        .unwrap();
        fs::write(root.join("venv/bin/python"), "not executable").unwrap();
        fs::write(root.join("hermes"), "not executable").unwrap();
        let launcher = base.join("hermes-launcher");
        fs::write(
            &launcher,
            format!(
                "#!/bin/sh\nexec \"{}\" \"{}\" \"$@\"\n",
                root.join("venv/bin/python").display(),
                root.join("hermes").display()
            ),
        )
        .unwrap();
        let rule = rules()
            .into_iter()
            .find(|rule| rule.id == "hermes")
            .unwrap();
        assert_eq!(source_root(&rule, &launcher), Some(root.clone()));
        fs::write(
            root.join(".git/config"),
            "[remote \"origin\"]\nurl = https://github.com/other/hermes-agent.git\n",
        )
        .unwrap();
        assert_eq!(source_root(&rule, &launcher), None);
        fs::remove_dir_all(base).unwrap();
    }
}
