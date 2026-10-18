use super::packages::brew_json;
use crate::{
    models::UpgradablePackage,
    platform,
    software_updates::{NativeUpdateReport, UpdateChannel},
};
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

pub fn check_with_channels(cancelled: &impl Fn() -> bool) -> NativeUpdateReport {
    let binary = platform::command_path("brew");
    if binary.is_none() || cancelled() {
        let state = if cancelled() {
            "cancelled"
        } else {
            "unsupported"
        };
        return NativeUpdateReport {
            packages: vec![],
            warnings: vec![],
            channels: vec![UpdateChannel::new(
                "brew",
                state,
                "Homebrew 未安装或检测已停止；App Store 需在系统中更新",
                binary.as_deref(),
            )],
        };
    }
    match check_all_upgradable() {
        Ok(rows) => {
            let channels = ["brew", "brew-cask"]
                .into_iter()
                .map(|manager| {
                    UpdateChannel::new(
                        manager,
                        if rows.iter().any(|row| row.manager == manager) {
                            "completed"
                        } else {
                            "no-update"
                        },
                        "读取 Homebrew 当前索引；不自动刷新软件源",
                        binary.as_deref(),
                    )
                })
                .collect();
            NativeUpdateReport {
                packages: rows,
                warnings: vec![],
                channels,
            }
        }
        Err(error) => NativeUpdateReport {
            packages: vec![],
            warnings: vec![error.clone()],
            channels: vec![UpdateChannel::new(
                "brew",
                "failed",
                &error,
                binary.as_deref(),
            )],
        },
    }
}
pub fn check_with_warnings() -> (Vec<UpgradablePackage>, Vec<String>) {
    let report = check_with_channels(&|| false);
    (report.packages, report.warnings)
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
    platform::validate_package_key(name)?;
    if !matches!(manager, "brew" | "brew-cask") {
        return Err("不支持的包管理器".into());
    }
    let rows = check_all_upgradable()?;
    crate::select_upgrade_candidate(&rows, manager, name, expected_version, arch)?;
    progress("Homebrew 安装渠道已确认；此渠道不能自动锁定本次确认版本");
    Err("请在原 Homebrew 渠道核对并升级；检测结果和原安装实例已保留".into())
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
