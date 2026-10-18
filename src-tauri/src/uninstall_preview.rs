use crate::{
    app_profile,
    models::format_size,
    package_plan::{self, RemovalPackage, RemovalReceipt},
    platform::{self, QueryCommand},
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    process::Command,
    sync::{LazyLock, Mutex},
    time::{Duration, Instant, SystemTime},
};

#[cfg(target_os = "macos")]
use std::{fs, path::Path};

#[derive(Clone, Debug, Serialize)]
pub struct PackageUninstallPlan {
    pub id: String,
    pub name: String,
    pub pkg_type: String,
    pub key: String,
    pub packages: Vec<RemovalPackage>,
    pub estimated_bytes: Option<u64>,
    pub estimated_size: String,
    pub mode: String,
    pub note: String,
    pub supported: bool,
    pub review_identity: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct AppIdentity {
    pub pkg_type: String,
    pub key: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PackageUninstallStatus {
    pub removed: Vec<AppIdentity>,
    pub remaining: Vec<AppIdentity>,
    pub unknown: Vec<String>,
}
#[derive(Clone)]
struct Receipt {
    plan: PackageUninstallPlan,
    fingerprint: String,
    native: Option<RemovalReceipt>,
    created: Instant,
}
static PLANS: LazyLock<Mutex<HashMap<String, Receipt>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn local_query(command: &mut Command) -> Result<String, String> {
    let output = command
        .env("LANG", "C")
        .query_output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn flatpak_scope(key: &str) -> Result<(String, String), String> {
    let (installation, reference) = key
        .split_once(':')
        .ok_or("Flatpak 缺少安装范围，请刷新应用列表")?;
    if !reference.starts_with("app/")
        || reference.split('/').count() != 4
        || reference
            .split('/')
            .any(|s| s.is_empty() || [".", ".."].contains(&s))
        || !reference
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
        || installation.is_empty()
        || installation.starts_with('-')
        || !installation
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err("Flatpak 应用身份无效".into());
    }
    let scope = match installation {
        "user" => "--user".into(),
        "system" | "default" => "--system".into(),
        name => format!("--installation={name}"),
    };
    Ok((scope, reference.into()))
}
fn flatpak_snapshot(key: &str) -> Result<String, String> {
    let (scope, reference) = flatpak_scope(key)?;
    let commit = local_query(Command::new("flatpak").args([
        "info",
        &scope,
        "--show-commit",
        "--",
        &reference,
    ]))?;
    if commit.trim().len() != 64 || !commit.trim().chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Flatpak 安装提交无法确认".into());
    }
    Ok(hex::encode(Sha256::digest(format!(
        "{scope}:{reference}:{}",
        commit.trim()
    ))))
}
#[cfg(target_os = "macos")]
fn mac_snapshot(key: &str) -> Result<String, String> {
    let path = fs::canonicalize(key).map_err(|e| e.to_string())?;
    if path != Path::new(key)
        || path.extension().is_none_or(|s| s != "app")
        || !(path.starts_with("/Applications")
            || platform::home().is_some_and(|h| path.starts_with(h.join("Applications"))))
    {
        return Err("应用安装路径不在允许范围内".into());
    }
    crate::developer::fingerprint(&path)
}
#[cfg(target_os = "macos")]
fn brew_snapshot(key: &str) -> Result<String, String> {
    platform::validate_package_key(key)?;
    let brew = platform::command_path("brew").ok_or("未安装 Homebrew")?;
    let uses = local_query(
        Command::new(&brew)
            .args(["uses", "--installed", "--", key])
            .env("HOMEBREW_NO_AUTO_UPDATE", "1"),
    )?;
    if !uses.trim().is_empty() {
        return Err(format!(
            "此软件被以下已安装软件依赖，需在 Homebrew 中处理：{}",
            uses.trim()
        ));
    }
    let info = local_query(
        Command::new(&brew)
            .args(["info", "--json=v2", "--formula", "--installed", "--", key])
            .env("HOMEBREW_NO_AUTO_UPDATE", "1"),
    )?;
    let value: serde_json::Value = serde_json::from_str(&info).map_err(|e| e.to_string())?;
    let rows = value["formulae"]
        .as_array()
        .ok_or("Homebrew 安装元数据无效")?;
    if rows.len() != 1
        || !rows[0]["installed"]
            .as_array()
            .is_some_and(|a| a.len() == 1 && a[0]["installed_on_request"] == true)
    {
        return Err("仅支持单个明确手动安装版本的 Homebrew 工具；其他范围请使用原渠道".into());
    }
    let files = local_query(
        Command::new(&brew)
            .args(["list", "--formula", "--", key])
            .env("HOMEBREW_NO_AUTO_UPDATE", "1"),
    )?;
    Ok(hex::encode(Sha256::digest(format!("{info}\n{files}"))))
}

#[cfg(any(target_os = "macos", test))]
fn simple_cask_app(data: &serde_json::Value) -> Result<String, String> {
    let artifacts = data["artifacts"].as_array().ok_or("Cask 安装产物未知")?;
    let mut apps = Vec::new();
    for artifact in artifacts {
        let map = artifact.as_object().ok_or("Cask 产物记录无效")?;
        if map.len() != 1 {
            return Err("Cask 包含无法确认范围的安装产物".into());
        }
        if let Some(app) = map.get("app") {
            let values = app
                .as_array()
                .filter(|v| v.len() == 1)
                .ok_or("Cask 的应用目标存在定制映射，请使用原渠道")?;
            apps.push(values[0].as_str().ok_or("Cask 应用路径无效")?.to_string());
        } else if !map.contains_key("zap") {
            return Err(
                "Cask 包含额外脚本、安装器或产物，请使用 Homebrew 原渠道卸载；本工具不执行 zap"
                    .into(),
            );
        }
    }
    if apps.len() != 1 {
        return Err("仅支持单个纯应用包的 Cask 卸载预览".into());
    }
    Ok(apps.remove(0))
}

#[cfg(target_os = "macos")]
fn cask_snapshot(key: &str, install: &str) -> Result<String, String> {
    platform::validate_package_key(key)?;
    let brew = platform::command_path("brew").ok_or("未安装 Homebrew")?;
    let info = local_query(
        Command::new(&brew)
            .args(["info", "--json=v2", "--cask", "--installed", "--", key])
            .env("HOMEBREW_NO_AUTO_UPDATE", "1"),
    )?;
    let value: serde_json::Value = serde_json::from_str(&info).map_err(|e| e.to_string())?;
    let rows = value["casks"]
        .as_array()
        .filter(|rows| rows.len() == 1)
        .ok_or("Cask 安装身份未知")?;
    if rows[0]["installed"].is_null() || rows[0]["installed"] == false {
        return Err("Cask 当前没有安装记录".into());
    }
    let declared = simple_cask_app(&rows[0])?;
    let installed = crate::packages::installed_cask_apps(key)?;
    if installed.len() != 1
        || fs::canonicalize(install).map_err(|e| e.to_string())? != installed[0]
        || Path::new(&declared).file_name() != Path::new(install).file_name()
    {
        return Err("Cask 元数据与实际应用路径不匹配，请使用原渠道卸载".into());
    }
    let app = mac_snapshot(install)?;
    Ok(hex::encode(Sha256::digest(format!("{info}\n{app}"))))
}

pub fn preview(kind: &str, key: &str) -> Result<PackageUninstallPlan, String> {
    let app = app_profile::find_app(kind, key)?;
    if app.is_system || !app.can_uninstall {
        return Err("系统组件或无法确认归属的应用受保护".into());
    }
    let id = hex::encode(Sha256::digest(format!(
        "{kind}:{key}:{:?}",
        SystemTime::now()
    )));
    let mut plan = PackageUninstallPlan {
        id: id.clone(),
        name: app.name.clone(),
        pkg_type: kind.into(),
        key: key.into(),
        packages: vec![],
        estimated_bytes: None,
        estimated_size: "未知".into(),
        mode: "unsupported".into(),
        note: "此渠道尚无法取得可靠的卸载影响范围，请在原渠道中卸载。".into(),
        supported: false,
        review_identity: String::new(),
    };
    let mut native = None;
    let snapshot=match kind{
        "deb"|"pacman"|"snap"=>match package_plan::simulate(kind,key){
            Ok(simulation)=>{plan.packages=simulation.packages;plan.mode=simulation.mode;plan.note=simulation.note;let hash=simulation.receipt.fingerprint.clone();native=Some(simulation.receipt);Ok(hash)},
            Err(error)=>Err(error),
        },
        "flatpak"=>flatpak_snapshot(key).inspect(|_|{plan.packages.push(RemovalPackage{name:key.into(),version:app.version.clone(),size_bytes:(app.size_bytes>0).then_some(app.size_bytes)});plan.mode="exact-scope".into();plan.note="精确限制为此安装范围的完整应用 ref；保留运行时、相关扩展和用户数据，不使用 --unused/--delete-data。这是本地安装身份计划，渠道没有仿真接口。".into();}),
        #[cfg(target_os="macos")]
        "app"=>mac_snapshot(key).inspect(|_|{plan.packages.push(RemovalPackage{name:key.into(),version:app.version.clone(),size_bytes:Some(app.size_bytes)});plan.mode="exact-scope".into();plan.note="仅将此应用包移入 Finder 废纸篓；应用文档、配置和账户保留。执行前再次核对实际路径及文件快照。".into();}),
        #[cfg(target_os="macos")]
        "brew"=>brew_snapshot(key).inspect(|_|{plan.packages.push(RemovalPackage{name:key.into(),version:app.version.clone(),size_bytes:None});plan.mode="exact-scope".into();plan.note="精确卸载单个手动安装版本的 Homebrew 工具；检查依赖且不强制移除，关闭自动依赖清理，不执行 autoremove。Homebrew uninstall 没有仿真接口。".into();}),
        #[cfg(target_os="macos")]
        "brew-cask"=>cask_snapshot(key,&app.install_path).inspect(|_|{plan.packages.push(RemovalPackage{name:app.install_path.clone(),version:app.version.clone(),size_bytes:Some(app.size_bytes)});plan.mode="exact-scope".into();plan.note="此 Cask 已确认只有单个应用包产物；通过 Homebrew 移除应用包，保留用户配置、账户与缓存。关闭自动依赖清理，不执行 zap。含脚本、安装器、额外产物或目标映射的 Cask 必须使用原渠道。".into();}),
        _=>Err(plan.note.clone()),
    };
    match snapshot {
        Ok(fingerprint) => {
            if native.is_none() {
                plan.review_identity = fingerprint.clone();
            }
            plan.supported = true;
            plan.estimated_bytes = plan.packages.iter().map(|p| p.size_bytes).sum();
            plan.estimated_size = plan
                .estimated_bytes
                .map(format_size)
                .unwrap_or_else(|| "未知（部分项目未报告体积）".into());
            let mut plans = PLANS.lock().map_err(|e| e.to_string())?;
            plans.retain(|_, r| r.created.elapsed() < Duration::from_secs(600));
            if plans.len() >= 64 {
                plans.clear();
            }
            plans.insert(
                id,
                Receipt {
                    plan: plan.clone(),
                    fingerprint,
                    native,
                    created: Instant::now(),
                },
            );
        }
        Err(error) => plan.note = error,
    }
    Ok(plan)
}

pub fn execute(id: &str) -> Result<String, String> {
    let receipt = PLANS
        .lock()
        .map_err(|e| e.to_string())?
        .get(id)
        .cloned()
        .ok_or("卸载计划已失效，请重新查看预览")?;
    if receipt.created.elapsed() > Duration::from_secs(600) {
        return Err("卸载计划超过 10 分钟，请重新查看预览".into());
    }
    let kind = receipt.plan.pkg_type.as_str();
    let key = receipt.plan.key.as_str();
    let app = app_profile::find_app(kind, key)?;
    if app.is_system || !app.can_uninstall {
        return Err("此应用受保护".into());
    }
    match kind {
        "deb" | "pacman" | "snap" => {
            let native = receipt.native.as_ref().ok_or("缺少卸载凭据")?;
            package_plan::verify(native)?;
            crate::privileged::remove_checked(native)?;
        }
        "flatpak" => {
            if flatpak_snapshot(key)? != receipt.fingerprint {
                return Err("安装身份已变化，请重新生成预览".into());
            }
            let (scope, reference) = flatpak_scope(key)?;
            platform::checked_output(Command::new("flatpak").args([
                "uninstall",
                &scope,
                "--app",
                "--no-related",
                "-y",
                "--",
                &reference,
            ]))?;
        }
        #[cfg(target_os = "macos")]
        "app" => {
            if mac_snapshot(key)? != receipt.fingerprint {
                return Err("应用文件已变化，请重新生成预览".into());
            }
            platform::checked_output(Command::new("/usr/bin/osascript").args(["-e","on run argv\ntell application \"Finder\" to delete POSIX file (item 1 of argv)\nend run"]).arg(key))?;
        }
        #[cfg(target_os = "macos")]
        "brew" => {
            if brew_snapshot(key)? != receipt.fingerprint {
                return Err("Homebrew 安装或依赖状态已变化，请重新生成预览".into());
            }
            platform::checked_output(
                Command::new(platform::command_path("brew").ok_or("未安装 Homebrew")?)
                    .args(["uninstall", "--formula", "--", key])
                    .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                    .env("HOMEBREW_NO_AUTOREMOVE", "1"),
            )?;
        }
        #[cfg(target_os = "macos")]
        "brew-cask" => {
            if cask_snapshot(key, &app.install_path)? != receipt.fingerprint {
                return Err("Cask 安装产物或应用文件已变化，请重新预览".into());
            }
            platform::checked_output(
                Command::new(platform::command_path("brew").ok_or("未安装 Homebrew")?)
                    .args(["uninstall", "--cask", "--", key])
                    .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                    .env("HOMEBREW_NO_AUTOREMOVE", "1"),
            )?;
        }
        _ => return Err("此渠道不支持受控卸载".into()),
    }
    // Retain the receipt for independent installed-state readback, including partial failures.
    Ok(format!("已卸载 {}；处理范围见确认预览", receipt.plan.name))
}

pub fn requires_authorization(id: &str) -> Result<bool, String> {
    let plans = PLANS.lock().map_err(|e| e.to_string())?;
    let receipt = plans.get(id).ok_or("卸载预览已失效")?;
    Ok(matches!(
        receipt.plan.pkg_type.as_str(),
        "deb" | "pacman" | "snap"
    ))
}

pub fn status(id: &str) -> Result<PackageUninstallStatus, String> {
    let receipt = PLANS
        .lock()
        .map_err(|e| e.to_string())?
        .get(id)
        .cloned()
        .ok_or("卸载预览已失效")?;
    let kind = receipt.plan.pkg_type.as_str();
    let keys: Vec<String> = if matches!(kind, "deb" | "pacman") {
        receipt
            .plan
            .packages
            .iter()
            .map(|p| p.name.clone())
            .collect()
    } else {
        vec![receipt.plan.key.clone()]
    };
    let installed: Result<Vec<String>, String> = match kind {
        "deb" => local_query(
            Command::new("/usr/bin/dpkg-query").args(["-W", "-f=${binary:Package}\t${Status}\n"]),
        )
        .and_then(|text| {
            let mut installed = Vec::new();
            for line in text.lines() {
                let fields: Vec<_> = line.split('\t').collect();
                if fields.len() != 2 || fields[1].split_whitespace().count() != 3 {
                    return Err("Deb 安装状态返回格式未知".into());
                }
                if fields[1].split_whitespace().last() == Some("installed") {
                    installed.push(fields[0].into());
                }
            }
            Ok(installed)
        }),
        "pacman" => local_query(Command::new("/usr/bin/pacman").arg("-Qq"))
            .map(|text| text.lines().map(str::to_string).collect()),
        "snap" => {
            local_query(Command::new("/usr/bin/snap").args(["list", "--all"])).and_then(|text| {
                let mut installed = Vec::new();
                for line in text.lines().skip(1) {
                    let fields: Vec<_> = line.split_whitespace().collect();
                    if fields.len() < 3 {
                        return Err("Snap 安装状态返回格式未知".into());
                    }
                    installed.push(fields[0].into());
                }
                Ok(installed)
            })
        }
        "flatpak" => flatpak_scope(&receipt.plan.key)
            .and_then(|(scope, _)| {
                local_query(Command::new("flatpak").args([
                    "list",
                    &scope,
                    "--app",
                    "--columns=ref",
                ]))
            })
            .map(|text| {
                let installation = receipt
                    .plan
                    .key
                    .split_once(':')
                    .map(|(i, _)| i)
                    .unwrap_or_default();
                text.lines()
                    .map(|reference| format!("{installation}:{}", reference.trim()))
                    .collect()
            }),
        #[cfg(target_os = "macos")]
        "app" => match fs::symlink_metadata(&receipt.plan.key) {
            Ok(_) => Ok(keys.clone()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(error) => Err(format!("应用包安装状态未知：{error}")),
        },
        #[cfg(target_os = "macos")]
        "brew" | "brew-cask" => platform::command_path("brew")
            .ok_or("未安装 Homebrew".into())
            .and_then(|brew| {
                local_query(
                    Command::new(brew)
                        .args([
                            "list",
                            if kind == "brew" {
                                "--formula"
                            } else {
                                "--cask"
                            },
                            "--versions",
                        ])
                        .env("HOMEBREW_NO_AUTO_UPDATE", "1"),
                )
            })
            .map(|text| {
                text.lines()
                    .filter_map(|line| line.split_whitespace().next().map(str::to_string))
                    .collect()
            }),
        _ => Err("此渠道无法独立确认安装状态".into()),
    };
    let mut report = PackageUninstallStatus {
        removed: Vec::new(),
        remaining: Vec::new(),
        unknown: Vec::new(),
    };
    match installed {
        Ok(installed) => {
            for key in keys {
                let found = installed.iter().any(|name| {
                    name == &key || kind == "deb" && name.split(':').next() == key.split(':').next()
                });
                let identity = AppIdentity {
                    pkg_type: kind.into(),
                    key,
                };
                if found {
                    report.remaining.push(identity);
                } else {
                    report.removed.push(identity);
                }
            }
        }
        Err(error) => report.unknown.push(error),
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cask_unknown_scripts_and_shared_artifacts_require_original_channel() {
        assert_eq!(simple_cask_app(&serde_json::json!({"artifacts":[{"app":["Editor.app"]},{"zap":[{"trash":"~/settings"}]}]})).unwrap(),"Editor.app");
        for artifact in [
            serde_json::json!({"uninstall":[{"script":"/bin/sh"}]}),
            serde_json::json!({"pkg":["installer.pkg"]}),
            serde_json::json!({"binary":["shared-tool"]}),
            serde_json::json!({"app":["Editor.app",{"target":"Custom.app"}]}),
        ] {
            assert!(simple_cask_app(&serde_json::json!({"artifacts":[artifact]})).is_err());
        }
    }
    #[test]
    fn flatpak_scope_requires_complete_identity() {
        assert_eq!(
            flatpak_scope("user:app/org.example.App/x86_64/stable").unwrap(),
            ("--user".into(), "app/org.example.App/x86_64/stable".into())
        );
        for key in [
            "org.example.App",
            "user:runtime/org.example.Platform/x86_64/stable",
            "--all:app/a/b/c",
            "user:app/a/b",
        ] {
            assert!(flatpak_scope(key).is_err());
        }
    }
}
