//! Read-only update providers. The registry never supplies executable update scripts.
use crate::{
    models::{SoftwareUpdateReport, ToolUpdate},
    platform::{self, QueryCommand},
};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

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
fn npm_tools() -> Result<Vec<ToolUpdate>, String> {
    let Some(npm) = tool_command("npm") else {
        return Ok(vec![]);
    };
    let installed = query(&npm, &["ls", "-g", "--depth=0", "--long", "--json"], true)?;
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
    Ok(parse_npm(
        &installed,
        outdated.as_ref().map_err(String::as_str),
    ))
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
fn python_tools() -> (Vec<ToolUpdate>, Vec<String>) {
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    if let Some(uv) = tool_command("uv") {
        match tool_query(&uv)
            .args(["tool", "list", "--show-paths"])
            .query_output()
        {
            Ok(output) if output.status.success() => {
                rows.extend(parse_uv(&String::from_utf8_lossy(&output.stdout)))
            }
            _ => warnings.push("uv 工具清单读取失败，请检查工具目录权限".into()),
        }
    }
    if let Some(pipx) = tool_command("pipx") {
        match query(&pipx, &["list", "--json"], false) {
            Ok(data) => {
                for (key, info) in data["venvs"].as_object().into_iter().flatten() {
                    let main = &info["metadata"]["main_package"];
                    let name = main["package"].as_str().unwrap_or(key);
                    let mut row = python_row(
                        "pipx",
                        name,
                        main["package_version"].as_str().unwrap_or(""),
                        String::new(),
                    );
                    if main["package_or_url"].as_str().is_some_and(|p| {
                        p.contains("git+") || p.contains('/') || p.starts_with('.')
                    }) {
                        row.note = "源码或本地路径安装，未按 PyPI 版本判断".into();
                    }
                    rows.push(row);
                }
            }
            Err(error) => warnings.push(format!("pipx: {error}")),
        }
    }
    // Four concurrent requests, with a total provider budget. Unvisited tools stay unknown.
    let start = Instant::now();
    for (batch_number, batch) in rows.chunks_mut(4).enumerate() {
        if batch_number >= 10 || start.elapsed() >= Duration::from_secs(30) {
            for row in batch {
                row.note = "本轮查询预算已用完，尚未检测上游版本".into();
            }
            continue;
        }
        std::thread::scope(|scope| {
            for row in batch {
                scope.spawn(move || {
                    if !row.note.is_empty()
                        || !row
                            .name
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
                    {
                        return;
                    }
                    match remote_json(&format!("https://pypi.org/pypi/{}/json", row.name)) {
                        Ok(data) => {
                            row.latest_version =
                                data["info"]["version"].as_str().unwrap_or("").into();
                            compare_versions(row);
                            if row.note.is_empty() {
                                row.note =
                                    "参考 PyPI 公开版本；私有源或固定版本请按原安装配置核对".into();
                            }
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
    (rows, warnings)
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
fn registry_tools() -> Vec<ToolUpdate> {
    rules()
        .into_iter()
        .filter_map(|rule| {
            let binary = tool_command(&rule.command)?;
            let mut row = ToolUpdate {
                id: format!("registry:{}", rule.id),
                name: rule.name.clone(),
                provider: rule.provider.clone(),
                install_path: binary.to_string_lossy().into_owned(),
                source_url: rule.source_url.clone(),
                update_hint: rule.update_hint.clone(),
                status: "unknown".into(),
                ..Default::default()
            };
            let result = if rule.provider == "github-branch" {
                source_check(&mut row, &rule, &binary)
            } else {
                release_check(&mut row, &rule, &binary)
            };
            if let Err(error) = result {
                row.status = "failed".into();
                row.note = error;
            }
            Some(row)
        })
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
pub fn check() -> SoftwareUpdateReport {
    std::thread::scope(|scope| {
        let native = scope.spawn(crate::updates::check_with_warnings);
        let npm = scope.spawn(npm_tools);
        let python = scope.spawn(python_tools);
        let registry = scope.spawn(registry_tools);
        let (packages, mut warnings) = native
            .join()
            .unwrap_or_else(|_| (vec![], vec!["系统软件更新检测异常".into()]));
        let mut tools = registry.join().unwrap_or_default();
        match npm.join() {
            Ok(Ok(rows)) => tools.extend(rows),
            Ok(Err(error)) => warnings.push(format!("npm: {error}")),
            Err(_) => warnings.push("npm 检测异常".into()),
        }
        if let Ok((rows, notices)) = python.join() {
            tools.extend(rows);
            warnings.extend(notices);
        }
        tools.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.id.cmp(&b.id))
        });
        SoftwareUpdateReport {
            packages,
            tools,
            warnings,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn npm_failure_is_unknown_and_latest_can_be_lower_than_installed() {
        let installed = serde_json::json!({"dependencies":{"@openai/codex":{"version":"1.2.0"},"local":{"version":"2.0.0","link":true}}});
        let failed = parse_npm(&installed, Err("timeout"));
        assert!(failed.iter().all(|row| row.status == "failed"));
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
