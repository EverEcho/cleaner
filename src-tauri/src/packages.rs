use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::Command;
use crate::models::{AppInfo, format_size};

pub fn list_installed_apps() -> Vec<AppInfo> {
    let mut apps = Vec::new();

    // 1. 扫描系统 .desktop 桌面启动器，提取应用名称与图标
    // 这是用户能感知到的“真实应用”的核心标识
    let mut desktop_map: HashMap<String, (String, String, String)> = HashMap::new(); // path -> (name, icon, desktop_path)
    let home = std::env::var("HOME").unwrap_or_default();
    let search_dirs = [
        "/usr/share/applications".to_string(),
        "/usr/local/share/applications".to_string(),
        format!("{}/.local/share/applications", home),
    ];

    for dir_str in search_dirs {
        let dir = Path::new(&dir_str);
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let mut name = String::new();
                        let mut icon = String::new();
                        let mut no_display = false;

                        for line in content.lines() {
                            let line = line.trim();
                            if line.starts_with("Name=") && name.is_empty() {
                                name = line[5..].to_string();
                            } else if line.starts_with("Icon=") && icon.is_empty() {
                                icon = line[5..].to_string();
                            } else if line == "NoDisplay=true" || line == "Hidden=true" {
                                no_display = true;
                            }
                        }

                        if !no_display && !name.is_empty() {
                            let path_str = path.to_string_lossy().to_string();
                            desktop_map.insert(path_str.clone(), (name, icon, path_str));
                        }
                    }
                }
            }
        }
    }

    // 2. 将 .desktop 映射到归属的 deb 软件包 (通过 dpkg -S 批量解析)
    let mut deb_to_desktop: HashMap<String, (String, String, String)> = HashMap::new();
    let desktop_paths: Vec<&str> = desktop_map.keys().map(|s| s.as_str()).collect();

    if !desktop_paths.is_empty() {
        if let Ok(output) = Command::new("dpkg")
            .arg("-S")
            .args(&desktop_paths)
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split(": ").collect();
                if parts.len() == 2 {
                    let pkg_name = parts[0].trim().to_string();
                    let file_path = parts[1].trim();
                    if let Some(info) = desktop_map.get(file_path) {
                        deb_to_desktop.insert(pkg_name, info.clone());
                    }
                }
            }
        }
    }

    // 3. 获取用户手动安装的 deb 软件包 (过滤掉被作为依赖拉入的子库)
    let mut manual_pkgs: HashSet<String> = HashSet::new();
    if let Ok(output) = Command::new("apt-mark")
        .arg("showmanual")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let p = line.trim();
                if !p.is_empty() {
                    manual_pkgs.insert(p.to_string());
                }
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
        .output()
    {
        if output.status.success() {
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
                        });
                    }
                }
            }
        }
    }

    // 5. 扫描 Flatpak 应用 (均是完整的用户应用)
    if let Ok(output) = Command::new("flatpak")
        .args(["list", "--app", "--columns=application,name,version,size,description"])
        .output()
    {
        if output.status.success() {
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
                        name: if name.is_empty() { app_id.clone() } else { name },
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
                    });
                }
            }
        }
    }

    // 6. 扫描 Snap 应用
    if let Ok(output) = Command::new("snap")
        .args(["list"])
        .output()
    {
        if output.status.success() {
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
                    });
                }
            }
        }
    }

    // 排序：有图形界面的排在前面，再按名称字母升序
    apps.sort_by(|a, b| {
        b.has_ui.cmp(&a.has_ui)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    apps
}

pub fn uninstall_package(pkg_type: &str, key: &str) -> Result<String, String> {
    match pkg_type {
        "deb" => {
            let status = Command::new("pkexec")
                .args(["apt-get", "remove", "--autoremove", "-y", key])
                .status()
                .map_err(|e| format!("提权执行失败: {}", e))?;
            if status.success() {
                Ok(format!("已成功卸载 deb 软件包: {}", key))
            } else {
                Err(format!("卸载失败，退出代码: {:?}", status.code()))
            }
        }
        "flatpak" => {
            let status = Command::new("flatpak")
                .args(["uninstall", "-y", key])
                .status()
                .map_err(|e| format!("flatpak 执行失败: {}", e))?;
            if status.success() {
                Ok(format!("已成功卸载 flatpak 应用: {}", key))
            } else {
                Err(format!("卸载失败: {:?}", status.code()))
            }
        }
        "snap" => {
            let status = Command::new("pkexec")
                .args(["snap", "remove", key])
                .status()
                .map_err(|e| format!("snap 执行失败: {}", e))?;
            if status.success() {
                Ok(format!("已成功卸载 snap 应用: {}", key))
            } else {
                Err(format!("卸载失败: {:?}", status.code()))
            }
        }
        _ => Err("不支持的包管理器类型".to_string()),
    }
}
