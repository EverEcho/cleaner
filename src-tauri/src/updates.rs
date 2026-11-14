use crate::{
    models::{SoftwareKind, ToolUpdate, UpgradablePackage},
    platform::{self, QueryCommand},
    software_updates::{NativeUpdateReport, UpdateChannel},
};
use std::process::Command;

pub fn check_with_channels(cancelled: &impl Fn() -> bool) -> NativeUpdateReport {
    let mut list = Vec::new();
    let mut tools = Vec::new();
    let mut checked = false;
    let mut channels = Vec::new();
    let mut warnings = Vec::new();
    let rpm_manager = if platform::command_path("dnf").is_some() {
        "dnf"
    } else {
        "yum"
    };
    for (binary, manager, args, empty_code) in [
        ("apt", "apt", vec!["list", "--upgradable"], None),
        (
            rpm_manager,
            rpm_manager,
            vec!["check-update", "-q"],
            Some(100),
        ),
        ("checkupdates", "pacman", vec![], Some(2)),
        (
            "zypper",
            "zypper",
            vec!["--non-interactive", "list-updates"],
            None,
        ),
    ] {
        let Some(command) = platform::command_path(binary) else {
            channels.push(UpdateChannel::new(
                manager,
                "unsupported",
                "未安装对应查询工具",
                None,
            ));
            continue;
        };
        if cancelled() {
            channels.push(UpdateChannel::new(
                manager,
                "cancelled",
                "已停止，未查询此渠道",
                Some(&command),
            ));
            continue;
        }
        checked = true;
        let output = match Command::new(&command)
            .args(args)
            .env("LANG", "C")
            .query_output()
        {
            Ok(output) => output,
            Err(error) => {
                warnings.push(format!("{binary} 查询失败: {error}"));
                channels.push(UpdateChannel::new(
                    manager,
                    "failed",
                    "查询超时或无法启动",
                    Some(&command),
                ));
                continue;
            }
        };
        if !output.status.success() && output.status.code() != empty_code {
            warnings.push(format!("{binary} 查询失败，请检查网络、软件源或权限"));
            channels.push(UpdateChannel::new(
                manager,
                "failed",
                "查询失败，请检查网络、软件源或权限",
                Some(&command),
            ));
            continue;
        }
        let rows = parse_updates(manager, &String::from_utf8_lossy(&output.stdout));
        channels.push(UpdateChannel::new(
            manager,
            if rows.is_empty() {
                "no-update"
            } else {
                "completed"
            },
            "读取本机软件源候选版本；源索引时间由系统包管理器维护",
            Some(&command),
        ));
        list.extend(rows);
    }
    if let Some(binary) = platform::command_path("flatpak") {
        checked = true;
        for scope in ["user", "system"] {
            let mut channel = UpdateChannel::new(
                "flatpak",
                "cancelled",
                "已停止此安装范围查询",
                Some(&binary),
            );
            channel.id.push_str(&format!(":{scope}"));
            if !cancelled() {
                match check_flatpak_scope(&binary, scope, cancelled) {
                    Ok((rows, apps)) => {
                        channel.state = if rows.is_empty() {
                            "no-update"
                        } else {
                            "completed"
                        }
                        .into();
                        channel.message = format!(
                            "{scope} 安装范围：按完整应用 ref、原 remote 和提交检测；自定义 installation 不在本轮范围"
                        );
                        list.extend(rows);
                        tools.extend(apps);
                    }
                    Err(error) => {
                        channel.state = if cancelled() { "cancelled" } else { "failed" }.into();
                        channel.message = error.clone();
                        if !cancelled() {
                            warnings.push(format!("Flatpak {scope}: {error}"));
                        }
                    }
                }
            }
            channels.push(channel);
        }
    } else {
        channels.push(UpdateChannel::new(
            "flatpak",
            "unsupported",
            "未安装 Flatpak",
            None,
        ));
    }
    if !checked {
        warnings.push("未找到支持的系统软件源".into());
    }
    NativeUpdateReport {
        packages: list,
        tools,
        warnings,
        channels,
    }
}

fn flatpak_identity(key: &str) -> Result<(&str, &str), String> {
    let (scope, reference) = key.split_once(':').ok_or("缺少 Flatpak 安装范围")?;
    if !matches!(scope, "user" | "system")
        || !reference.starts_with("app/")
        || reference.split('/').count() != 4
        || reference
            .split('/')
            .any(|part| part.is_empty() || [".", ".."].contains(&part))
        || !reference
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
    {
        return Err("Flatpak 身份无法可靠区分；请使用原安装渠道更新".into());
    }
    Ok((
        if scope == "user" {
            "--user"
        } else {
            "--system"
        },
        reference,
    ))
}
fn valid_commit(commit: &str) -> bool {
    commit.len() == 64 && commit.chars().all(|c| c.is_ascii_hexdigit())
}
pub(crate) fn flatpak_app_ref(reference: &str) -> Option<String> {
    let reference = reference.strip_prefix("app/").unwrap_or(reference);
    let mut parts = reference.split('/');
    let valid = (0..3).all(|_| {
        parts.next().is_some_and(|part| {
            !part.is_empty()
                && !matches!(part, "." | "..")
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        })
    });
    (valid && parts.next().is_none()).then(|| format!("app/{reference}"))
}

fn parse_flatpak_scope(
    scope: &str,
    local: &str,
    remote: &str,
    mut commits: impl FnMut(&str, &str) -> Result<(String, String), String>,
) -> Result<(Vec<UpgradablePackage>, Vec<ToolUpdate>), String> {
    let mut offered = std::collections::HashSet::new();
    for line in remote.lines().filter(|line| !line.trim().is_empty()) {
        let (reference, origin) = line.split_once('\t').ok_or("Flatpak 候选缺少来源")?;
        let reference = flatpak_app_ref(reference).ok_or("Flatpak 候选应用标识无效")?;
        offered.insert((reference, origin));
    }
    let mut packages = Vec::new();
    let mut tools = Vec::new();
    for line in local.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.splitn(4, '\t');
        let reference = fields
            .next()
            .and_then(flatpak_app_ref)
            .ok_or("Flatpak 本地应用标识无效")?;
        let origin = fields
            .next()
            .filter(|value| !value.is_empty())
            .ok_or("Flatpak 本地应用缺少来源")?;
        let key = format!("{scope}:{reference}");
        flatpak_identity(&key)?;
        let mut identity = reference.split('/').skip(1);
        let app_id = identity.next().unwrap_or_default();
        let arch = identity.next().unwrap_or_default();
        let branch = identity.next().unwrap_or_default();
        let name = fields
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or(app_id);
        let version = fields.next().unwrap_or_default();
        let mut tool = ToolUpdate {
            id: format!("flatpak:{key}"),
            name: name.into(),
            provider: "flatpak".into(),
            kind: SoftwareKind::Gui,
            architecture: arch.into(),
            release_channel: branch.into(),
            current_version: version.into(),
            install_path: key.clone(),
            status: "no-update".into(),
            note: format!("安装范围 {scope} · 原软件源 {origin}"),
            update_hint: "使用原 Flatpak 软件源检查更新".into(),
            ..Default::default()
        };
        if offered.contains(&(reference.clone(), origin)) {
            let (current, target) = commits(origin, &reference)?;
            if !valid_commit(&current) || !valid_commit(&target) {
                return Err("Flatpak 本地或上游提交无法可靠确认".into());
            }
            if current != target {
                let package = UpgradablePackage {
                    name: key,
                    manager: "flatpak".into(),
                    current_version: current,
                    new_version: target,
                    arch: arch.into(),
                    repository: format!("{scope} · {origin} · {branch}"),
                };
                tool.latest_version = format!("提交 {}", &package.new_version[..12]);
                tool.status = "available".into();
                tool.native_update = Some(package.clone());
                packages.push(package);
            }
        }
        tools.push(tool);
    }
    Ok((packages, tools))
}
fn check_flatpak_scope(
    binary: &std::path::Path,
    scope: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<(Vec<UpgradablePackage>, Vec<ToolUpdate>), String> {
    let flag = if scope == "user" {
        "--user"
    } else {
        "--system"
    };
    let local = Command::new(binary)
        .args([
            "list",
            flag,
            "--app",
            "--columns=ref:full,origin:full,name:full,version:full",
        ])
        .env("LANG", "C")
        .query_output()
        .map_err(|_| "Flatpak 本地清单读取超时")?;
    if !local.status.success() {
        return Err("Flatpak 本地安装范围读取失败".into());
    }
    if cancelled() {
        return Err("读取清单后已停止，未查询此范围软件源".into());
    }
    if local.stdout.iter().all(u8::is_ascii_whitespace) {
        return Ok((Vec::new(), Vec::new()));
    }
    let remote = Command::new(binary)
        .args([
            "remote-ls",
            flag,
            "--app",
            "--updates",
            "--columns=ref:full,origin:full",
        ])
        .env("LANG", "C")
        .query_output()
        .map_err(|_| "Flatpak 软件源查询超时")?;
    if !remote.status.success() {
        return Err("Flatpak 软件源查询失败".into());
    }
    parse_flatpak_scope(
        scope,
        &String::from_utf8_lossy(&local.stdout),
        &String::from_utf8_lossy(&remote.stdout),
        |origin, reference| {
            if cancelled() {
                return Err("已停止 Flatpak 提交查询".into());
            }
            let read_commit = |args: &[&str]| -> Result<String, String> {
                let output = Command::new(binary)
                    .args(args)
                    .query_output()
                    .map_err(|_| "Flatpak 完整提交查询超时")?;
                if !output.status.success() {
                    return Err("Flatpak 完整提交查询失败".into());
                }
                Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
            };
            Ok((
                read_commit(&["info", flag, "--show-commit", "--", reference])?,
                read_commit(&[
                    "remote-info",
                    flag,
                    "--show-commit",
                    "--",
                    origin,
                    reference,
                ])?,
            ))
        },
    )
}

pub fn check_with_warnings() -> (Vec<UpgradablePackage>, Vec<String>) {
    let report = check_with_channels(&|| false);
    (report.packages, report.warnings)
}

pub fn check_all_upgradable() -> Result<Vec<UpgradablePackage>, String> {
    let (list, warnings) = check_with_warnings();
    if list.is_empty() && !warnings.is_empty() {
        Err(warnings.join("；"))
    } else {
        Ok(list)
    }
}

fn parse_updates(manager: &str, text: &str) -> Vec<UpgradablePackage> {
    text.lines()
        .filter_map(|line| {
            let parts: Vec<_> = line.split_whitespace().collect();
            let mut row = UpgradablePackage {
                name: String::new(),
                manager: manager.into(),
                current_version: String::new(),
                new_version: String::new(),
                arch: String::new(),
                repository: String::new(),
            };
            match manager {
                "apt" => {
                    if parts.len() < 4 || !line.contains("[upgradable from:") {
                        return None;
                    }
                    let (name, repo) = parts[0].split_once('/')?;
                    row.name = name.into();
                    row.repository = repo.into();
                    row.new_version = parts[1].into();
                    row.arch = parts[2].into();
                    row.current_version = line
                        .split_once("from:")?
                        .1
                        .trim()
                        .trim_end_matches(']')
                        .into();
                }
                "dnf" | "yum" => {
                    if parts.len() != 3 {
                        return None;
                    }
                    let (name, arch) = parts[0].rsplit_once('.')?;
                    if ![
                        "x86_64", "aarch64", "noarch", "i686", "ppc64le", "s390x", "armv7hl",
                    ]
                    .contains(&arch)
                    {
                        return None;
                    }
                    row.name = name.into();
                    row.arch = arch.into();
                    row.new_version = parts[1].into();
                    row.repository = parts[2].into();
                }
                "pacman" => {
                    if parts.len() != 4 || parts[2] != "->" {
                        return None;
                    }
                    row.name = parts[0].into();
                    row.current_version = parts[1].into();
                    row.new_version = parts[3].into();
                }
                "zypper" => {
                    let parts: Vec<_> = line.split('|').map(str::trim).collect();
                    if parts.len() < 6 || !["v", "s"].contains(&parts[0]) {
                        return None;
                    }
                    row.repository = parts[1].into();
                    row.name = parts[2].into();
                    row.current_version = parts[3].into();
                    row.new_version = parts[4].into();
                    row.arch = parts[5].into();
                }
                _ => return None,
            }
            Some(row)
        })
        .collect()
}

pub fn upgrade_package(manager: &str, name: &str) -> Result<String, String> {
    upgrade_package_checked(manager, name, None)
}
pub fn upgrade_package_checked(
    manager: &str,
    name: &str,
    expected_version: Option<&str>,
) -> Result<String, String> {
    upgrade_package_with_progress(manager, name, expected_version, |_| {})
}
pub fn upgrade_package_with_progress(
    manager: &str,
    name: &str,
    expected_version: Option<&str>,
    progress: impl FnMut(&str),
) -> Result<String, String> {
    upgrade_package_scoped_with_progress(manager, name, expected_version, None, progress)
}
pub fn upgrade_package_scoped_with_progress(
    manager: &str,
    name: &str,
    expected_version: Option<&str>,
    arch: Option<&str>,
    mut progress: impl FnMut(&str),
) -> Result<String, String> {
    static EXECUTING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _gate = EXECUTING.try_lock().map_err(|_| "已有系统软件正在升级")?;
    platform::validate_package_key(name)?;
    if manager == "pacman" {
        return Err("Arch 不支持单包升级；请在终端执行 pacman -Syu".into());
    }
    if !matches!(manager, "apt" | "dnf" | "flatpak") {
        return Err("不支持的包管理器".into());
    }
    progress("重新核对软件源候选版本与安装渠道");
    let rows = check_all_upgradable()?;
    let row = crate::select_upgrade_candidate(&rows, manager, name, expected_version, arch)?;
    progress(&format!(
        "调用 {} 更新 {}；等待原渠道完成",
        manager.to_uppercase(),
        name
    ));
    let logs = if manager == "flatpak" {
        let flatpak = platform::command_path("flatpak").ok_or("未找到 Flatpak")?;
        let (scope, reference) = flatpak_identity(name)?;
        let output = platform::checked_output(
            Command::new(flatpak)
                .args(["update", scope, "-y"])
                .arg(format!("--commit={}", row.new_version))
                .args(["--", reference]),
        )?;
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    } else {
        let receipt =
            crate::native_update_plan::prepare(manager, name, &row.new_version, &row.arch)?;
        progress("请求管理员授权；助手将再次核对并锁定此版本执行");
        crate::privileged::upgrade_checked(&receipt)?
    };
    if !logs.trim().is_empty() {
        progress(&logs.chars().take(12000).collect::<String>());
    }
    progress("重新读取已安装版本，核对升级结果");
    let actual = installed_version(manager, name, &row.arch)?;
    if actual.trim_start_matches("0:") != row.new_version.trim_start_matches("0:") {
        return Err(format!(
            "更新命令已结束，但实际版本 {actual} 与候选版本 {} 不符，请重新检查",
            row.new_version
        ));
    }
    crate::software_updates::record_native_success_scoped(manager, name, Some(&row.arch));
    Ok(format!("软件包 {name} 已通过 {manager} 更新至 {actual}"))
}
fn installed_version(manager: &str, name: &str, arch: &str) -> Result<String, String> {
    let package_key = match manager {
        "apt" => format!("{name}:{arch}"),
        "dnf" | "yum" | "zypper" => format!("{name}.{arch}"),
        _ => name.into(),
    };
    let (binary, args): (&str, Vec<&str>) = match manager {
        "apt" => (
            "dpkg-query",
            vec!["-W", "-f=${Version}", "--", &package_key],
        ),
        "dnf" | "yum" | "zypper" => (
            "rpm",
            vec![
                "-q",
                "--qf",
                "%{EPOCHNUM}:%{VERSION}-%{RELEASE}",
                "--",
                &package_key,
            ],
        ),
        "flatpak" => {
            let (scope, reference) = flatpak_identity(name)?;
            (
                "flatpak",
                vec!["info", scope, "--show-commit", "--", reference],
            )
        }
        _ => return Err("无法核对该渠道的已安装版本".into()),
    };
    let binary = platform::command_path(binary).ok_or("已安装版本查询工具不可用")?;
    let output = Command::new(binary)
        .args(args)
        .query_output()
        .map_err(|_| "已安装版本查询超时")?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if output.status.success() && !text.is_empty() {
        Ok(text)
    } else {
        Err("升级后已安装版本无法确认，请重新检查更新".into())
    }
}

pub fn refresh_package_sources() -> Result<String, String> {
    for manager in ["apt", "dnf", "yum", "zypper"] {
        if platform::command_path(manager).is_some() {
            crate::privileged::run("refresh", manager, "")?;
            return Ok(format!("{} 软件源已刷新", manager.to_uppercase()));
        }
    }
    Err("未找到可刷新的软件源；Arch 请在终端执行 pacman -Syu".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flatpak_update_matches_scope_full_ref_origin_and_commit() {
        let old = "a".repeat(64);
        let new = "b".repeat(64);
        let reference = "app/org.example.App/x86_64/stable";
        let local = "org.example.App/x86_64/stable\tflathub\tExample Editor\t1.2.3\n";
        let remote = format!("{reference}\tflathub\n{reference}\tother-remote\n");
        let commits = |origin: &str, target: &str| {
            assert_eq!((origin, target), ("flathub", reference));
            Ok((old.clone(), new.clone()))
        };
        let (user, apps) = parse_flatpak_scope("user", local, &remote, commits).unwrap();
        let (system, _) = parse_flatpak_scope("system", local, &remote, commits).unwrap();
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].name, format!("user:{reference}"));
        assert_ne!(user[0].name, system[0].name);
        assert_eq!(user[0].new_version, new);
        assert_eq!(apps[0].name, "Example Editor");
        assert_eq!(apps[0].current_version, "1.2.3");
        assert_eq!(apps[0].native_update.as_ref().unwrap().new_version, new);
        let (_, apps) =
            parse_flatpak_scope("system", local, "", |_, _| panic!("no candidate")).unwrap();
        assert_eq!(apps[0].status, "no-update");
        assert_eq!(
            flatpak_identity(&user[0].name).unwrap(),
            ("--user", reference)
        );
        assert!(
            parse_flatpak_scope("user", local, &remote, |_, _| Ok((
                "short".into(),
                new.clone()
            )))
            .is_err()
        );
        assert_eq!(flatpak_app_ref(reference).as_deref(), Some(reference));
        assert!(flatpak_app_ref("runtime/org.example.App/x86_64/stable").is_none());
        for name in [
            "org.example.App",
            "user:app/org.example.App",
            "user:app/../x86_64/stable",
            "unknown:app/org.example.App/x86_64/stable",
        ] {
            assert!(flatpak_identity(name).is_err());
        }
    }
    #[test]
    fn native_updates_keep_package_ids_and_ignore_headers() {
        let apt = parse_updates(
            "apt",
            "Listing...\ngit/stable 2.1 amd64 [upgradable from: 2.0]\n",
        );
        assert_eq!(apt[0].current_version, "2.0");
        let rpm = parse_updates(
            "dnf",
            "Last metadata expiration check: today\nmy.tool.x86_64 2.1 updates\n",
        );
        assert_eq!(rpm.len(), 1);
        assert_eq!(rpm[0].name, "my.tool");

        assert_eq!(
            parse_updates("pacman", "git 2.0 -> 2.1\n")[0].new_version,
            "2.1"
        );
    }
}
