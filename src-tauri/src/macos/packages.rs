use crate::platform::QueryCommand;
use crate::{
    models::{AppInfo, format_size},
    platform,
};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use walkdir::WalkDir;

pub fn read_plist(path: &Path) -> Result<Value, String> {
    let output = platform::checked_output(
        Command::new("/usr/bin/plutil")
            .args(["-convert", "json", "-o", "-", "--"])
            .arg(path),
    )?;
    serde_json::from_slice(&output.stdout).map_err(|e| format!("无法读取属性列表: {e}"))
}

pub fn brew_json(args: &[&str]) -> Result<Value, String> {
    let brew =
        platform::command_path("brew").ok_or("未安装 Homebrew；请使用应用自带更新或 App Store")?;
    let output = platform::checked_output(
        Command::new(brew)
            .args(args)
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("HOMEBREW_NO_ENV_HINTS", "1"),
    )?;
    serde_json::from_slice(&output.stdout).map_err(|e| format!("Homebrew 返回数据无效: {e}"))
}

pub fn application_roots() -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = platform::home() {
        roots.push(home.join("Applications"));
    }
    roots
}

pub fn installed_cask_apps(token: &str) -> Result<Vec<PathBuf>, String> {
    platform::validate_package_key(token)?;
    let brew = platform::command_path("brew").ok_or("未安装 Homebrew")?;
    let output = Command::new(brew)
        .args(["list", "--cask", "--", token])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .query_output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Homebrew 已安装产物查询失败".into());
    }
    Ok(cask_artifact_paths(&String::from_utf8_lossy(
        &output.stdout,
    )))
}
fn cask_artifact_paths(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.extension().is_some_and(|s| s == "app"))
        .filter_map(|path| fs::canonicalize(path).ok())
        .collect()
}
fn assign_casks(apps: &mut [AppInfo], ownership: &HashMap<PathBuf, String>) {
    for app in apps {
        if !app.is_system
            && let Ok(path) = fs::canonicalize(&app.install_path)
            && let Some(token) = ownership.get(&path)
        {
            app.key = token.clone();
            app.pkg_type = "brew-cask".into();
        }
    }
}

pub fn scan_bundles(roots: &[PathBuf]) -> Vec<AppInfo> {
    scan_bundles_report(roots, &|| false, &mut |_, _, _| {}).0
}

fn scan_bundles_report(
    roots: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&str, u64, u64),
) -> (Vec<AppInfo>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut apps = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .max_depth(4)
            .into_iter()
            .filter_entry(|e| {
                e.depth() == 0
                    || e.path()
                        .parent()
                        .and_then(Path::extension)
                        .is_none_or(|s| s != "app")
            })
        {
            if cancelled() {
                return (apps, warnings);
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    if error
                        .io_error()
                        .is_none_or(|e| e.kind() != std::io::ErrorKind::NotFound)
                    {
                        warnings.push(format!("应用目录未读取：{error}"));
                    }
                    continue;
                }
            };
            let path = entry.path();
            if !entry.file_type().is_dir() || path.extension().is_none_or(|s| s != "app") {
                continue;
            }
            progress(&path.to_string_lossy(), apps.len() as u64, 0);
            let plist = match read_plist(&path.join("Contents/Info.plist")) {
                Ok(data) => data,
                Err(error) => {
                    warnings.push(format!("{} 应用元数据未知：{error}", path.display()));
                    Value::Null
                }
            };
            let fallback = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let name = plist["CFBundleDisplayName"]
                .as_str()
                .or(plist["CFBundleName"].as_str())
                .unwrap_or(&fallback)
                .to_string();
            let version = plist["CFBundleShortVersionString"]
                .as_str()
                .or(plist["CFBundleVersion"].as_str())
                .unwrap_or("")
                .to_string();
            let mut size_bytes = 0;
            let mut size_known = true;
            for file in WalkDir::new(path) {
                if cancelled() {
                    return (apps, warnings);
                }
                match file {
                    Ok(file) if file.file_type().is_file() => match file.metadata() {
                        Ok(meta) => size_bytes += meta.len(),
                        Err(_) => size_known = false,
                    },
                    Err(_) => size_known = false,
                    _ => {}
                }
            }
            if !size_known {
                warnings.push(format!("{} 占用统计不完整", path.display()));
            }
            apps.push(AppInfo {
                key: path.to_string_lossy().into_owned(),
                name,
                pkg_type: "app".into(),
                version,
                size: if size_known {
                    format_size(size_bytes)
                } else {
                    "未知（部分目录未读取）".into()
                },
                size_bytes,
                install_time: String::new(),
                description: plist["CFBundleIdentifier"].as_str().unwrap_or("").into(),
                icon: crate::icons::bundle_icon(
                    path,
                    plist["CFBundleIconFile"].as_str().unwrap_or(""),
                ),
                desktop_file: path.to_string_lossy().into_owned(),
                is_system: path.starts_with("/System"),
                has_ui: true,
                can_uninstall: !path.starts_with("/System"),
                install_path: path.to_string_lossy().into_owned(),
                ..AppInfo::default()
            });
        }
    }
    (apps, warnings)
}

pub use crate::app_profile::{AppChannel, AppScanReport};
pub fn list_installed_apps() -> Vec<AppInfo> {
    scan_installed_apps_report().apps
}
pub fn scan_installed_apps_report() -> AppScanReport {
    scan_installed_apps_report_controlled(&|| false, |_, _, _| {})
}
pub fn scan_installed_apps_report_controlled(
    cancelled: &dyn Fn() -> bool,
    mut progress: impl FnMut(&str, u64, u64),
) -> AppScanReport {
    let (mut apps, mut warnings) =
        scan_bundles_report(&application_roots(), cancelled, &mut progress);
    let mut channels = vec![AppChannel {
        id: "app".into(),
        status: if warnings.is_empty() {
            "ok".into()
        } else {
            "partial".into()
        },
        count: Some(apps.len()),
        note: "已读取 Applications 中的应用包；未读取目录的数量未知".into(),
    }];
    let brew = if cancelled() {
        Err("应用扫描已停止".into())
    } else {
        brew_json(&["info", "--json=v2", "--installed"])
    };
    if let Ok(data) = &brew {
        let mut cask_apps = HashMap::new();
        if let Some(casks) = data["casks"].as_array() {
            for cask in casks {
                if cancelled() {
                    break;
                }
                if let Some(token) = cask["token"].as_str().filter(|t| !t.is_empty()) {
                    match installed_cask_apps(token) {
                        Ok(paths) => {
                            if paths.len() == 1 {
                                cask_apps.insert(paths[0].clone(), token.to_string());
                            } else {
                                warnings.push(format!(
                                    "{token} 的应用包产物不唯一，不能将同一 Cask 标识分配给多个应用"
                                ));
                                for app in &mut apps {
                                    if fs::canonicalize(&app.install_path)
                                        .is_ok_and(|path| paths.contains(&path))
                                    {
                                        app.can_uninstall = false;
                                        app.uninstall_note="Homebrew 安装包含多个应用包，请在原渠道查看完整范围后卸载".into();
                                    }
                                }
                            }
                        }
                        Err(error) => warnings.push(format!("{token} Cask 安装产物未知：{error}")),
                    }
                }
            }
        }
        assign_casks(&mut apps, &cask_apps);
        if let Some(formulae) = data["formulae"].as_array() {
            for formula in formulae {
                let Some(name) = formula["name"].as_str() else {
                    continue;
                };
                // Only explicitly installed tools; dependencies stay out of the main list.
                if !formula["installed"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|i| i["installed_on_request"] == true))
                {
                    continue;
                }
                apps.push(AppInfo {
                    key: name.into(),
                    name: name.into(),
                    pkg_type: "brew".into(),
                    version: formula["installed"][0]["version"]
                        .as_str()
                        .unwrap_or("")
                        .into(),
                    size: "—".into(),
                    size_bytes: 0,
                    install_time: String::new(),
                    description: formula["desc"].as_str().unwrap_or("").into(),
                    icon: String::new(),
                    desktop_file: String::new(),
                    is_system: false,
                    has_ui: false,
                    can_uninstall: true,
                    ..AppInfo::default()
                });
            }
        }
    }
    apps.sort_by(|a, b| {
        b.has_ui
            .cmp(&a.has_ui)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    match brew {
        Ok(_) => channels.push(AppChannel {
            id: "brew".into(),
            status: "ok".into(),
            count: Some(
                apps.iter()
                    .filter(|a| a.pkg_type.starts_with("brew"))
                    .count(),
            ),
            note: "统计已匹配 Cask 与手动安装的 Formula".into(),
        }),
        Err(error) => {
            let status = if cancelled() {
                "cancelled"
            } else if platform::command_path("brew").is_none() {
                "unavailable"
            } else {
                "failed"
            };
            if status != "unavailable" {
                warnings.push(error.clone());
            }
            channels.push(AppChannel {
                id: "brew".into(),
                status: status.into(),
                count: None,
                note: error,
            });
        }
    }
    let report = AppScanReport {
        apps,
        channels,
        warnings,
        checked_at: chrono::Utc::now().to_rfc3339(),
    };
    crate::app_profile::save_inventory(&report);
    report
}

pub fn uninstall_package(_pkg_type: &str, _key: &str) -> Result<String, String> {
    Err("卸载需要先生成并确认影响预览，请刷新应用列表".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cask_ownership_uses_full_installed_path_and_keeps_same_name_copies() {
        let root = std::env::temp_dir().join(format!(
            "cleaner-cask-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let installed = root.join("Applications/Editor.app");
        let copy = root.join("UserApplications/Editor.app");
        fs::create_dir_all(&installed).unwrap();
        fs::create_dir_all(&copy).unwrap();
        let tracked = cask_artifact_paths(&format!(
            "{}\nrelative/Editor.app\nmetadata not a path\n",
            installed.display()
        ));
        assert_eq!(tracked, vec![fs::canonicalize(&installed).unwrap()]);
        let ownership = HashMap::from([(tracked[0].clone(), "editor".into())]);
        let mut apps = vec![
            AppInfo {
                key: installed.to_string_lossy().into_owned(),
                install_path: installed.to_string_lossy().into_owned(),
                pkg_type: "app".into(),
                ..AppInfo::default()
            },
            AppInfo {
                key: copy.to_string_lossy().into_owned(),
                install_path: copy.to_string_lossy().into_owned(),
                pkg_type: "app".into(),
                ..AppInfo::default()
            },
        ];
        assign_casks(&mut apps, &ownership);
        assert_eq!(apps[0].pkg_type, "brew-cask");
        assert_eq!(apps[0].key, "editor");
        assert_eq!(apps[1].pkg_type, "app");
        assert_eq!(apps[1].key, copy.to_string_lossy());
        fs::remove_dir_all(root).unwrap();
    }
}
