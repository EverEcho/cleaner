use crate::models::{AppInfo, format_size};
use crate::platform::QueryCommand;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Command;

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
    let mut report = AppScanReport {
        checked_at: chrono::Utc::now().to_rfc3339(),
        ..AppScanReport::default()
    };
    report.apps = inventory(&mut report, cancelled, &mut progress);
    if cancelled() {
        report
            .warnings
            .push("应用扫描已停止；展示已读取部分，未读取渠道的数量未知".into());
        report.channels.push(AppChannel {
            id: "remaining".into(),
            status: "cancelled".into(),
            count: None,
            note: "未完成的扫描范围未知".into(),
        });
    }
    for channel in &mut report.channels {
        if channel.status == "ok" && channel.id != "deb-manual" {
            channel.count = Some(
                report
                    .apps
                    .iter()
                    .filter(|a| a.pkg_type == channel.id)
                    .count(),
            );
        }
    }
    crate::app_profile::save_inventory(&report);
    report
}

fn inventory(
    report: &mut AppScanReport,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&str, u64, u64),
) -> Vec<AppInfo> {
    let mut apps = Vec::new();

    progress("读取应用入口", 0, 0);
    let (launchers, launcher_warnings) =
        crate::manual_apps::launchers_with_warnings(&crate::manual_apps::launcher_roots());
    report.warnings.extend(launcher_warnings);
    if cancelled() {
        return apps;
    }
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

    let ownership = if !desktop_paths.is_empty() && crate::platform::command_path("dpkg").is_some()
    {
        match Command::new("dpkg")
            .arg("-S")
            .args(&desktop_paths)
            .env("LANG", "C")
            .query_output()
        {
            Ok(output) => {
                if !output.status.success() && output.status.code() != Some(1) {
                    report
                        .warnings
                        .push("Deb 入口归属查询未完成，部分应用来源未知".into());
                }
                Some(output)
            }
            Err(error) => {
                report
                    .warnings
                    .push(format!("Deb 入口归属查询失败：{error}；部分应用来源未知"));
                None
            }
        }
    } else {
        None
    };
    if let Some(output) = ownership {
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

    if cancelled() {
        return apps;
    }
    progress("读取系统软件包", 0, 0);
    // 3. 获取用户手动安装的 deb 软件包 (过滤掉被作为依赖拉入的子库)
    let mut manual_pkgs: HashSet<String> = HashSet::new();
    if let Some(output) = inventory_query(
        Command::new("apt-mark").arg("showmanual"),
        "deb-manual",
        report,
    ) {
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
    if let Some(output) = inventory_query(Command::new("dpkg-query")
        .args(["-W", "-f=${Package}\t${Version}\t${Installed-Size}\t${Status}\t${Priority}\t${Section}\t${Description}\n"]), "deb", report) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if cancelled() { break; }
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

    if cancelled() {
        return apps;
    }
    progress("读取 Flatpak", apps.len() as u64, 0);
    // Include installation and full ref: the same app can have several branches or scopes.
    if let Some(output) = inventory_query(
        Command::new("flatpak").args([
            "list",
            "--app",
            "--columns=application,ref,installation,size,version,name,description",
        ]),
        "flatpak",
        report,
    ) {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let fields: Vec<_> = line.split('\t').collect();
            if fields.len() < 6 || !fields[1].starts_with("app/") || fields[2].is_empty() {
                report
                    .warnings
                    .push("Flatpak 返回了无法确认完整身份的条目，已跳过".into());
                continue;
            }
            apps.push(AppInfo {
                key: format!("{}:{}", fields[2], fields[1]),
                name: if fields[5].is_empty() {
                    fields[0].into()
                } else {
                    fields[5].into()
                },
                pkg_type: "flatpak".into(),
                version: fields[4].into(),
                size: fields[3].into(),
                size_bytes: parse_size(fields[3]).unwrap_or(0),
                description: fields.get(6).unwrap_or(&"").to_string(),
                icon: "package-x-generic".into(),
                has_ui: true,
                can_uninstall: true,
                uninstall_note: format!("安装范围 {} · {}", fields[2], fields[1]),
                ..AppInfo::default()
            });
        }
    }

    if cancelled() {
        return apps;
    }
    progress("读取 Snap", apps.len() as u64, 0);
    // 6. 扫描 Snap 应用
    if let Some(output) = inventory_query(Command::new("snap").args(["list"]), "snap", report) {
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

    if cancelled() {
        return apps;
    }
    progress("匹配桌面软件包归属", apps.len() as u64, 0);
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
            report.channels.push(AppChannel {
                id: kind.into(),
                status: "unavailable".into(),
                count: None,
                note: "渠道未安装".into(),
            });
            continue;
        }
        report.channels.push(AppChannel {
            id: kind.into(),
            status: "ok".into(),
            count: Some(0),
            note: "仅统计已匹配桌面入口的应用；终端工具数量未知".into(),
        });
        let mut seen = HashSet::new();
        for (path, (name, icon, desktop_file)) in &desktop_map {
            if cancelled() {
                break;
            }
            progress(path, apps.len() as u64, 0);
            let Ok(output) = Command::new(owner_command)
                .args(&owner_args)
                .arg(path)
                .query_output()
            else {
                report
                    .warnings
                    .push(format!("{kind} 的入口归属查询失败：{path}"));
                if let Some(channel) = report.channels.iter_mut().find(|c| c.id == kind) {
                    channel.status = "partial".into();
                    channel.count = None;
                }
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

    if cancelled() {
        return apps;
    }
    progress("识别独立应用与安装目录", apps.len() as u64, 0);
    let represented = apps
        .iter()
        .filter_map(|app| {
            launchers
                .iter()
                .find(|launcher| launcher.path == std::path::Path::new(&app.desktop_file))
        })
        .filter_map(|launcher| crate::manual_apps::executable(&launcher.exec))
        .collect();
    let (manual, manual_warnings) = crate::manual_apps::discover_controlled(
        &launchers,
        &owned,
        &represented,
        cancelled,
        progress,
    );
    apps.extend(manual);
    report.warnings.extend(manual_warnings);

    // 排序：有图形界面的排在前面，再按名称字母升序
    apps.sort_by(|a, b| {
        b.has_ui
            .cmp(&a.has_ui)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    report.channels.push(AppChannel {
        id: "manual".into(),
        status: "ok".into(),
        count: Some(apps.iter().filter(|a| a.pkg_type == "manual").count()),
        note: "仅识别有入口且安装结构可验证的独立应用；未发现入口的程序数量未知".into(),
    });
    crate::icons::resolve_linux(&mut apps);
    apps
}

fn inventory_query(
    command: &mut Command,
    id: &str,
    report: &mut AppScanReport,
) -> Option<std::process::Output> {
    let program = command.get_program().to_string_lossy().into_owned();
    if crate::platform::command_path(&program).is_none() {
        report.channels.push(AppChannel {
            id: id.into(),
            status: "unavailable".into(),
            count: None,
            note: "渠道未安装；数量未知".into(),
        });
        return None;
    }
    match command.env("LANG", "C").query_output() {
        Ok(output) if output.status.success() => {
            report.channels.push(AppChannel {
                id: id.into(),
                status: "ok".into(),
                count: None,
                note: if id == "deb-manual" {
                    "已读取手动安装标记".into()
                } else {
                    "已查询此渠道".into()
                },
            });
            Some(output)
        }
        result => {
            let note = match result {
                Ok(out) => String::from_utf8_lossy(&out.stderr).trim().to_string(),
                Err(error) => error.to_string(),
            };
            let note = format!("{program} 查询失败：{note}；此渠道数量未知");
            report.channels.push(AppChannel {
                id: id.into(),
                status: "failed".into(),
                count: None,
                note: note.clone(),
            });
            report.warnings.push(note);
            None
        }
    }
}

fn parse_size(value: &str) -> Option<u64> {
    let parts: Vec<_> = value.split_whitespace().collect();
    let quantity: f64 = parts.first()?.parse().ok()?;
    let multiplier = match *parts.get(1)? {
        "B" => 1.,
        "kB" | "KB" => 1000.,
        "MB" => 1000_f64.powi(2),
        "GB" => 1000_f64.powi(3),
        "KiB" => 1024.,
        "MiB" => 1024_f64.powi(2),
        "GiB" => 1024_f64.powi(3),
        _ => return None,
    };
    (quantity.is_finite() && quantity >= 0.).then_some((quantity * multiplier) as u64)
}

pub fn uninstall_package(_pkg_type: &str, _key: &str) -> Result<String, String> {
    Err("卸载需要先生成并确认影响预览，请刷新应用列表".into())
}
