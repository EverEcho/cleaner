use std::process::Command;
use crate::models::UpgradablePackage;

/// 扫描系统中所有支持的包管理器的待更新软件
pub fn check_all_upgradable() -> Vec<UpgradablePackage> {
    let mut list = Vec::new();

    // 1. APT (Debian, Ubuntu, Deepin, UOS, Mint)
    if is_command_available("apt") {
        if let Ok(output) = Command::new("apt")
            .args(["list", "--upgradable"])
            .env("LANG", "C")
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(1) {
                    // 格式例如: ghostscript/stable-security 10.05.1 amd64 [upgradable from: 10.05.0]
                    if line.contains('/') && line.contains("[") {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 4 {
                            let name_repo = parts[0];
                            let name = name_repo.split('/').next().unwrap_or(name_repo).to_string();
                            let repo = name_repo.split('/').nth(1).unwrap_or("").to_string();
                            let new_ver = parts[1].to_string();
                            let arch = parts[2].to_string();

                            // 提取旧版本
                            let mut curr_ver = "".to_string();
                            if let Some(pos) = line.find("from:") {
                                curr_ver = line[pos + 5..].trim_end_matches(']').trim().to_string();
                            }

                            list.push(UpgradablePackage {
                                name,
                                manager: "apt".to_string(),
                                current_version: curr_ver,
                                new_version: new_ver,
                                arch,
                                repository: repo,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. DNF / YUM (RHEL, Fedora, CentOS, Rocky, Alma)
    if is_command_available("dnf") || is_command_available("yum") {
        let bin = if is_command_available("dnf") { "dnf" } else { "yum" };
        if let Ok(output) = Command::new(bin)
            .args(["check-update", "-q"])
            .env("LANG", "C")
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && !parts[0].starts_with("Security:") && !parts[0].starts_with("Obsoleting") {
                    let name_arch = parts[0];
                    let name = name_arch.split('.').next().unwrap_or(name_arch).to_string();
                    let arch = name_arch.split('.').nth(1).unwrap_or("").to_string();
                    let new_ver = parts[1].to_string();
                    let repo = parts[2].to_string();

                    list.push(UpgradablePackage {
                        name,
                        manager: bin.to_string(),
                        current_version: "已安装".to_string(),
                        new_version: new_ver,
                        arch,
                        repository: repo,
                    });
                }
            }
        }
    }

    // 3. Pacman (Arch Linux, Manjaro)
    if is_command_available("checkupdates") {
        if let Ok(output) = Command::new("checkupdates").output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    // 格式: pkgname 1.0.0-1 -> 1.0.1-1
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 4 {
                        list.push(UpgradablePackage {
                            name: parts[0].to_string(),
                            manager: "pacman".to_string(),
                            current_version: parts[1].to_string(),
                            new_version: parts[3].to_string(),
                            arch: "".to_string(),
                            repository: "arch-repo".to_string(),
                        });
                    }
                }
            }
        }
    }

    // 4. Zypper (openSUSE)
    if is_command_available("zypper") {
        if let Ok(output) = Command::new("zypper")
            .args(["--non-interactive", "list-updates"])
            .env("LANG", "C")
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.starts_with("v |") || line.starts_with("s |") {
                    let parts: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
                    if parts.len() >= 6 {
                        list.push(UpgradablePackage {
                            name: parts[2].to_string(),
                            manager: "zypper".to_string(),
                            current_version: parts[3].to_string(),
                            new_version: parts[4].to_string(),
                            arch: parts[5].to_string(),
                            repository: parts.get(1).unwrap_or(&"").to_string(),
                        });
                    }
                }
            }
        }
    }

    // 5. Flatpak
    if is_command_available("flatpak") {
        if let Ok(output) = Command::new("flatpak")
            .args(["remote-ls", "--updates", "--columns=name,application,version,branch"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 2 {
                        let name = parts[0].to_string();
                        let app_id = parts[1].to_string();
                        let ver = parts.get(2).unwrap_or(&"").to_string();
                        list.push(UpgradablePackage {
                            name: if name.is_empty() { app_id } else { name },
                            manager: "flatpak".to_string(),
                            current_version: "已安装".to_string(),
                            new_version: if ver.is_empty() { "有更新".to_string() } else { ver },
                            arch: "".to_string(),
                            repository: "Flathub".to_string(),
                        });
                    }
                }
            }
        }
    }

    list
}

/// 升级指定的软件包 (调用 pkexec 提权)
pub fn upgrade_package(manager: &str, name: &str) -> Result<String, String> {
    match manager {
        "apt" => {
            let status = Command::new("pkexec")
                .args(["apt-get", "install", "--only-upgrade", "-y", name])
                .status()
                .map_err(|e| format!("执行失败: {}", e))?;
            if status.success() {
                Ok(format!("软件包 {} 已升级完成", name))
            } else {
                Err(format!("升级失败: {:?}", status.code()))
            }
        }
        "dnf" | "yum" => {
            let status = Command::new("pkexec")
                .args([manager, "upgrade", "-y", name])
                .status()
                .map_err(|e| format!("执行失败: {}", e))?;
            if status.success() {
                Ok(format!("软件包 {} 已升级完成", name))
            } else {
                Err(format!("升级失败: {:?}", status.code()))
            }
        }
        "pacman" => {
            let status = Command::new("pkexec")
                .args(["pacman", "-S", "--noconfirm", name])
                .status()
                .map_err(|e| format!("执行失败: {}", e))?;
            if status.success() {
                Ok(format!("软件包 {} 已升级完成", name))
            } else {
                Err(format!("升级失败: {:?}", status.code()))
            }
        }
        "zypper" => {
            let status = Command::new("pkexec")
                .args(["zypper", "--non-interactive", "update", name])
                .status()
                .map_err(|e| format!("执行失败: {}", e))?;
            if status.success() {
                Ok(format!("软件包 {} 已升级完成", name))
            } else {
                Err(format!("升级失败: {:?}", status.code()))
            }
        }
        "flatpak" => {
            let status = Command::new("flatpak")
                .args(["update", "-y", name])
                .status()
                .map_err(|e| format!("执行失败: {}", e))?;
            if status.success() {
                Ok(format!("Flatpak 应用 {} 已升级完成", name))
            } else {
                Err(format!("升级失败: {:?}", status.code()))
            }
        }
        _ => Err("不支持的包管理器".to_string()),
    }
}

/// 刷新所有源 (如 apt update / dnf check-update)
pub fn refresh_package_sources() -> Result<String, String> {
    if is_command_available("apt") {
        let status = Command::new("pkexec")
            .args(["apt-get", "update"])
            .status()
            .map_err(|e| format!("执行失败: {}", e))?;
        if status.success() {
            return Ok("APT 软件源列表已成功刷新".to_string());
        }
    }
    if is_command_available("dnf") {
        let status = Command::new("pkexec")
            .args(["dnf", "makecache"])
            .status()
            .map_err(|e| format!("执行失败: {}", e))?;
        if status.success() {
            return Ok("DNF 元数据已成功刷新".to_string());
        }
    }
    Ok("刷新完成".to_string())
}

fn is_command_available(cmd: &str) -> bool {
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(':') {
            if std::path::Path::new(dir).join(cmd).is_file() {
                return true;
            }
        }
    }
    false
}

