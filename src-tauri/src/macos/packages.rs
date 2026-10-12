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

pub fn scan_bundles(roots: &[PathBuf]) -> Vec<AppInfo> {
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
            .flatten()
        {
            let path = entry.path();
            if !entry.file_type().is_dir() || path.extension().is_none_or(|s| s != "app") {
                continue;
            }
            let plist = read_plist(&path.join("Contents/Info.plist")).unwrap_or(Value::Null);
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
            let size_bytes = WalkDir::new(path)
                .into_iter()
                .flatten()
                .filter(|e| e.file_type().is_file())
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum();
            apps.push(AppInfo {
                key: path.to_string_lossy().into_owned(),
                name,
                pkg_type: "app".into(),
                version,
                size: format_size(size_bytes),
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
            });
        }
    }
    apps
}

pub fn list_installed_apps() -> Vec<AppInfo> {
    let mut apps = scan_bundles(&application_roots());
    if let Ok(data) = brew_json(&["info", "--json=v2", "--installed"]) {
        let mut cask_apps = HashMap::new();
        if let Some(casks) = data["casks"].as_array() {
            for cask in casks {
                if let Some(artifacts) = cask["artifacts"].as_array() {
                    for artifact in artifacts {
                        if let Some(name) = artifact["app"].get(0).and_then(Value::as_str) {
                            cask_apps.insert(
                                PathBuf::from(name)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                                cask["token"].as_str().unwrap_or("").to_string(),
                            );
                        }
                    }
                }
            }
        }
        for app in &mut apps {
            if let Some(token) = Path::new(&app.key)
                .file_name()
                .and_then(|n| cask_apps.get(n.to_string_lossy().as_ref()))
                && !app.is_system
                && !token.is_empty()
            {
                app.key = token.clone();
                app.pkg_type = "brew-cask".into();
            }
        }
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
                });
            }
        }
    }
    apps.sort_by(|a, b| {
        b.has_ui
            .cmp(&a.has_ui)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    apps
}

pub fn uninstall_package(pkg_type: &str, key: &str) -> Result<String, String> {
    if pkg_type == "app" {
        let path = fs::canonicalize(key).map_err(|e| e.to_string())?;
        let permitted = path.starts_with("/Applications")
            || platform::home().is_some_and(|h| path.starts_with(h.join("Applications")));
        if !permitted
            || path.extension().is_none_or(|s| s != "app")
            || !list_installed_apps()
                .iter()
                .any(|a| a.pkg_type == "app" && !a.is_system && Path::new(&a.key) == Path::new(key))
        {
            return Err("只能卸载应用列表中的非系统应用".into());
        }
        // Arguments are passed to AppleScript directly; no shell or script interpolation.
        platform::checked_output(Command::new("/usr/bin/osascript").args(["-e", "on run argv\ntell application \"Finder\" to delete POSIX file (item 1 of argv)\nend run"]).arg(path))?;
        return Ok("应用已移入废纸篓；应用文档及配置保留".into());
    }
    platform::validate_package_key(key)?;
    if !matches!(pkg_type, "brew" | "brew-cask")
        || !list_installed_apps()
            .iter()
            .any(|a| a.pkg_type == pkg_type && a.key == key && !a.is_system)
    {
        return Err("未找到可卸载的 Homebrew 应用".into());
    }
    let brew = platform::command_path("brew").ok_or("未安装 Homebrew")?;
    platform::checked_output(Command::new(brew).args([
        "uninstall",
        if pkg_type == "brew-cask" {
            "--cask"
        } else {
            "--formula"
        },
        "--",
        key,
    ]))?;
    Ok(format!("已卸载 {key}"))
}
