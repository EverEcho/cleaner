use crate::models::{AppInfo, format_size};
use crate::platform::QueryCommand;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

pub fn list_installed_apps() -> Vec<AppInfo> {
    let mut apps = Vec::new();

    let launchers = crate::manual_apps::launchers_in(&crate::manual_apps::launcher_roots());
    let desktop_map: HashMap<String, (String, String, String)> = launchers
        .iter()
        .map(|launcher| {
            let path = launcher.path.to_string_lossy().into_owned();
            (
                path.clone(),
                (launcher.name.clone(), launcher.icon.clone(), path),
            )
        })
        .collect();
    let binaries: HashMap<PathBuf, Vec<&crate::manual_apps::Launcher>> = {
        let mut map: HashMap<PathBuf, Vec<_>> = HashMap::new();
        for launcher in &launchers {
            if let Some(binary) = crate::manual_apps::executable(&launcher.exec) {
                map.entry(binary).or_default().push(launcher);
            }
        }
        map
    };
    let mut owned = HashSet::new();
    // 2. 将 .desktop 映射到归属的 deb 软件包 (通过 dpkg -S 批量解析)
    let mut deb_to_desktop: HashMap<String, (String, String, String)> = HashMap::new();
    let mut desktop_paths: Vec<PathBuf> = desktop_map.keys().map(PathBuf::from).collect();
    desktop_paths.extend(binaries.keys().cloned());

    if !desktop_paths.is_empty()
        && let Ok(output) = Command::new("dpkg")
            .arg("-S")
            .args(&desktop_paths)
            .query_output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if let Some((owners, file_path)) = line.split_once(": ") {
                let path = PathBuf::from(file_path.trim());
                let matched: Vec<_> = launchers
                    .iter()
                    .filter(|launcher| {
                        launcher.path == path
                            || binaries.get(&path).is_some_and(|items| {
                                items.iter().any(|item| item.path == launcher.path)
                            })
                    })
                    .collect();
                for launcher in matched {
                    owned.insert(launcher.path.clone());
                    for owner in owners.split(", ") {
                        let pkg = owner.split(':').next().unwrap_or(owner).trim();
                        if launcher.path == path {
                            deb_to_desktop.insert(
                                pkg.to_string(),
                                (
                                    launcher.name.clone(),
                                    launcher.icon.clone(),
                                    launcher.path.to_string_lossy().into_owned(),
                                ),
                            );
                        } else {
                            deb_to_desktop.entry(pkg.to_string()).or_insert_with(|| {
                                (
                                    launcher.name.clone(),
                                    launcher.icon.clone(),
                                    launcher.path.to_string_lossy().into_owned(),
                                )
                            });
                        }
                    }
                }
            }
        }
    }

    // 3. 获取用户手动安装的 deb 软件包 (过滤掉被作为依赖拉入的子库)
    let mut manual_pkgs: HashSet<String> = HashSet::new();
    if let Ok(output) = Command::new("apt-mark").arg("showmanual").query_output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let p = line.trim();
            if !p.is_empty() {
                manual_pkgs.insert(p.to_string());
            }
        }
    }

    // 4. 解析 dpkg 软件包列表
    // 规则：
    // a. 优先展示具有桌面菜单图标的「用户桌面应用」（has_ui = true）
    // b. 其次展示用户手动安装的独立工具（manual_pkgs 中，且非底层 libc/core 关键库）
    // c. 系统组件库（libs、自动安装的无UI依赖包）默认不混在主应用列表，避免卡死和误卸载导致系统崩溃
    if let Ok(output) = Command::new("dpkg-query")
        .args(["-W", "-f=${Package}\t${Version}\t${Installed-Size}\t${Status}\t${Priority}\t${Section}\t${Description}\n"])
        .query_output()
        && output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split('\t').collect();
                if parts.len() >= 6 {
                    let status = parts[3];
                    if !status.contains("installed") || status.contains("not-installed") {
                        continue;
                    }

                    let pkg = parts[0].to_string();
                    let ver = parts[1].to_string();
                    let size_kb: u64 = parts[2].trim().parse().unwrap_or(0);
                    let size_bytes = size_kb * 1024;
                    let priority = parts[4];
                    let section = parts[5];
                    let desc = if parts.len() > 6 { parts[6].lines().next().unwrap_or("").to_string() } else { String::new() };

                    let is_essential = priority == "required" || priority == "important";
                    let is_manual = manual_pkgs.contains(&pkg);
                    let has_desktop = deb_to_desktop.contains_key(&pkg);

                    // 核心过滤：只要满足【有界面图标】或【用户手动安装的独立工具（非纯底层依赖库）】才作为独立应用呈现
                    if has_desktop || (is_manual && !section.starts_with("lib") && !is_essential) {
                        let (display_name, icon, desktop_file) = if let Some(d) = deb_to_desktop.get(&pkg) {
                            (d.0.clone(), d.1.clone(), d.2.clone())
                        } else {
                            (pkg.clone(), "application-x-executable".to_string(), "".to_string())
                        };

                        let is_system = is_essential || section == "admin" || section == "base" || !is_manual;

                        apps.push(AppInfo {
                            key: pkg.clone(),
                            name: display_name,
                            pkg_type: "deb".to_string(),
                            version: ver,
                            size: format_size(size_bytes),
                            size_bytes,
                            install_time: "".to_string(),
                            description: desc,
                            icon,
                            desktop_file,
                            is_system,
                            has_ui: has_desktop,
                            can_uninstall: !is_system,
                            ..AppInfo::default()
                        });
                    }
                }
            }
        }

    // 5. 扫描 Flatpak 应用 (均是完整的用户应用)
    if let Ok(output) = Command::new("flatpak")
        .args([
            "list",
            "--app",
            "--columns=application,name,version,size,description",
        ])
        .query_output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                let app_id = parts[0].to_string();
                let name = parts[1].to_string();
                let ver = parts.get(2).unwrap_or(&"").to_string();
                let size_str = parts.get(3).unwrap_or(&"").to_string();
                let desc = parts.get(4).unwrap_or(&"").to_string();

                apps.push(AppInfo {
                    key: app_id.clone(),
                    name: if name.is_empty() {
                        app_id.clone()
                    } else {
                        name
                    },
                    pkg_type: "flatpak".to_string(),
                    version: ver,
                    size: size_str,
                    size_bytes: 0,
                    install_time: "".to_string(),
                    description: desc,
                    icon: "package-x-generic".to_string(),
                    desktop_file: "".to_string(),
                    is_system: false,
                    has_ui: true,
                    can_uninstall: true,
                    ..AppInfo::default()
                });
            }
        }
    }

    // 6. 扫描 Snap 应用
    if let Ok(output) = Command::new("snap").args(["list"]).query_output()
        && output.status.success()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let name = parts[0].to_string();
                let ver = parts[1].to_string();
                if name == "core" || name == "snapd" || name.starts_with("core") || name == "bare" {
                    continue;
                }
                apps.push(AppInfo {
                    key: name.clone(),
                    name: name.clone(),
                    pkg_type: "snap".to_string(),
                    version: ver,
                    size: "Snap 容器".to_string(),
                    size_bytes: 0,
                    install_time: "".to_string(),
                    description: "".to_string(),
                    icon: "snapcraft".to_string(),
                    desktop_file: "".to_string(),
                    is_system: false,
                    has_ui: true,
                    can_uninstall: true,
                    ..AppInfo::default()
                });
            }
        }
    }

    for (kind, owner_command, owner_args) in [
        (
            "rpm",
            "rpm",
            vec![
                "-qf",
                "--qf",
                "%{NAME}\t%{VERSION}-%{RELEASE}\t%{SIZE}\t%{SUMMARY}\n",
            ],
        ),
        ("pacman", "pacman", vec!["-Qoq"]),
    ] {
        if crate::platform::command_path(owner_command).is_none() {
            continue;
        }
        let mut seen = HashSet::new();
        for (path, (name, icon, desktop_file)) in &desktop_map {
            let Ok(output) = Command::new(owner_command)
                .args(&owner_args)
                .arg(path)
                .query_output()
            else {
                continue;
            };
            if !output.status.success() {
                continue;
            }
            owned.insert(PathBuf::from(path));
            let text = String::from_utf8_lossy(&output.stdout);
            let fields: Vec<_> = text.trim().split('\t').collect();
            let Some(key) = fields.first().filter(|k| !k.is_empty()) else {
                continue;
            };
            if !seen.insert(key.to_string()) {
                continue;
            }
            let size_bytes = fields
                .get(2)
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            apps.push(AppInfo {
                key: key.to_string(),
                name: name.clone(),
                pkg_type: kind.into(),
                version: fields.get(1).unwrap_or(&"").to_string(),
                size: if size_bytes == 0 {
                    "—".into()
                } else {
                    format_size(size_bytes)
                },
                size_bytes,
                install_time: String::new(),
                description: fields.get(3).unwrap_or(&"").to_string(),
                icon: icon.clone(),
                desktop_file: desktop_file.clone(),
                is_system: false,
                has_ui: true,
                can_uninstall: true,
                ..AppInfo::default()
            });
        }
    }

    let represented = apps
        .iter()
        .filter_map(|app| {
            launchers
                .iter()
                .find(|launcher| launcher.path == std::path::Path::new(&app.desktop_file))
        })
        .filter_map(|launcher| crate::manual_apps::executable(&launcher.exec))
        .collect();
    apps.extend(crate::manual_apps::discover(
        &launchers,
        &owned,
        &represented,
    ));

    // 排序：有图形界面的排在前面，再按名称字母升序
    apps.sort_by(|a, b| {
        b.has_ui
            .cmp(&a.has_ui)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    crate::icons::resolve_linux(&mut apps);
    apps
}

pub fn uninstall_package(pkg_type: &str, key: &str) -> Result<String, String> {
    if pkg_type == "manual" {
        return Err("独立安装应用需要先查看卸载详情，再确认移入废纸篓".into());
    }
    crate::platform::validate_package_key(key)?;
    if !list_installed_apps()
        .iter()
        .any(|a| a.pkg_type == pkg_type && a.key == key && !a.is_system)
    {
        return Err("只能卸载列表中的非系统应用".into());
    }
    if pkg_type == "flatpak" {
        crate::platform::checked_output(Command::new("flatpak").args([
            "uninstall",
            "-y",
            "--",
            key,
        ]))?;
    } else {
        let manager = if pkg_type == "rpm" {
            ["dnf", "yum", "zypper"]
                .into_iter()
                .find(|m| crate::platform::command_path(m).is_some())
                .ok_or("未找到 RPM 卸载工具")?
        } else {
            pkg_type
        };
        crate::privileged::run("remove", manager, key)?;
    }
    Ok(format!("已卸载 {key}"))
}
