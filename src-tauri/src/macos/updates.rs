use super::packages::brew_json;
use crate::{models::UpgradablePackage, platform};
use serde_json::Value;
use std::process::Command;

pub fn parse_outdated(data: &Value) -> Vec<UpgradablePackage> {
    let mut list = Vec::new();
    for (section, manager) in [("formulae", "brew"), ("casks", "brew-cask")] {
        if let Some(items) = data[section].as_array() {
            for item in items {
                let Some(name) = item["name"].as_str() else {
                    continue;
                };
                let current = if let Some(versions) = item["installed_versions"].as_array() {
                    versions
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                } else {
                    item["installed_versions"].as_str().unwrap_or("").into()
                };
                list.push(UpgradablePackage {
                    name: name.into(),
                    manager: manager.into(),
                    current_version: current,
                    new_version: item["current_version"].as_str().unwrap_or("").into(),
                    arch: String::new(),
                    repository: "Homebrew".into(),
                });
            }
        }
    }
    list
}

pub fn check_all_upgradable() -> Result<Vec<UpgradablePackage>, String> {
    brew_json(&["outdated", "--json=v2"]).map(|v| parse_outdated(&v))
}

pub fn check_with_warnings() -> (Vec<UpgradablePackage>, Vec<String>) {
    match check_all_upgradable() {
        Ok(rows) => (rows, vec![]),
        Err(error) => (vec![], vec![error]),
    }
}

pub fn upgrade_package(manager: &str, name: &str) -> Result<String, String> {
    platform::validate_package_key(name)?;
    if !matches!(manager, "brew" | "brew-cask") {
        return Err("不支持的包管理器".into());
    }
    let brew = platform::command_path("brew").ok_or("未安装 Homebrew")?;
    platform::checked_output(
        Command::new(brew)
            .args([
                "upgrade",
                if manager == "brew-cask" {
                    "--cask"
                } else {
                    "--formula"
                },
                "--",
                name,
            ])
            .env("HOMEBREW_NO_AUTO_UPDATE", "1"),
    )?;
    Ok(format!("{name} 已更新"))
}

pub fn refresh_package_sources() -> Result<String, String> {
    let brew = platform::command_path("brew").ok_or("未安装 Homebrew")?;
    platform::checked_output(Command::new(brew).arg("update"))?;
    Ok("Homebrew 软件源已刷新".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_formula_and_cask_versions() {
        let data = serde_json::json!({"formulae":[{"name":"git","installed_versions":["2.0"],"current_version":"2.1"}],"casks":[{"name":"firefox","installed_versions":"140","current_version":"141"}]});
        let rows = parse_outdated(&data);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].current_version, "2.0");
        assert_eq!(rows[1].manager, "brew-cask");
        assert_eq!(rows[1].current_version, "140");
    }
}
