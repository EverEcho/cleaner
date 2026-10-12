use crate::{
    models::UpgradablePackage,
    platform::{self, QueryCommand},
};
use std::process::Command;

pub fn check_all_upgradable() -> Result<Vec<UpgradablePackage>, String> {
    let mut list = Vec::new();
    let mut checked = false;
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
        (
            "flatpak",
            "flatpak",
            vec![
                "remote-ls",
                "--updates",
                "--columns=application,version,branch",
            ],
            None,
        ),
    ] {
        let Some(command) = platform::command_path(binary) else {
            continue;
        };
        checked = true;
        let output = Command::new(command)
            .args(args)
            .env("LANG", "C")
            .query_output()
            .map_err(|e| format!("{binary} 查询失败: {e}"))?;
        if !output.status.success() && output.status.code() != empty_code {
            return Err(format!(
                "{binary} 查询失败: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        list.extend(parse_updates(
            manager,
            &String::from_utf8_lossy(&output.stdout),
        ));
    }
    if checked {
        Ok(list)
    } else {
        Err("未找到支持的软件源；请使用应用自带更新功能".into())
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
                "flatpak" => {
                    let parts: Vec<_> = line.split('\t').collect();
                    if parts.len() < 2 || !parts[0].contains('.') {
                        return None;
                    }
                    row.name = parts[0].into();
                    row.new_version = parts[1].into();
                    row.repository = parts.get(2).unwrap_or(&"").to_string();
                }
                _ => return None,
            }
            Some(row)
        })
        .collect()
}

pub fn upgrade_package(manager: &str, name: &str) -> Result<String, String> {
    platform::validate_package_key(name)?;
    if !check_all_upgradable()?
        .iter()
        .any(|p| p.manager == manager && p.name == name)
    {
        return Err("该软件不在待更新列表中，请重新检查更新".into());
    }
    if manager == "flatpak" {
        platform::checked_output(Command::new("flatpak").args(["update", "-y", "--", name]))?;
    } else {
        crate::privileged::run("upgrade", manager, name)?;
    }
    Ok(format!("软件包 {name} 已升级完成"))
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
        let flatpak = parse_updates("flatpak", "org.mozilla.firefox\t141\tstable\n");
        assert_eq!(flatpak[0].name, "org.mozilla.firefox");
        assert_eq!(
            parse_updates("pacman", "git 2.0 -> 2.1\n")[0].new_version,
            "2.1"
        );
    }
}
