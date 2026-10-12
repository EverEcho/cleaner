use super::packages::read_plist;
use crate::{models::AutostartItem, platform};
use std::{fs, path::Path, process::Command};

fn domain() -> Result<String, String> {
    let output = platform::checked_output(Command::new("/usr/bin/id").arg("-u"))?;
    let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
    uid.parse::<u32>().map_err(|_| "无法识别当前用户")?;
    Ok(format!("gui/{uid}"))
}

fn disabled_labels(text: &str) -> std::collections::HashMap<String, bool> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once("=>")?;
            Some((
                key.trim().trim_matches('"').to_string(),
                value.trim().trim_end_matches(';') == "true",
            ))
        })
        .collect()
}

pub fn scan_autostart_items() -> Vec<AutostartItem> {
    let mut roots = vec![
        ("/Library/LaunchAgents".into(), false),
        ("/Library/LaunchDaemons".into(), false),
    ];
    if let Some(home) = platform::home() {
        roots.insert(0, (home.join("Library/LaunchAgents"), true));
    }
    let overrides = domain()
        .ok()
        .and_then(|d| {
            Command::new("/bin/launchctl")
                .args(["print-disabled", &d])
                .output()
                .ok()
        })
        .filter(|o| o.status.success())
        .map(|o| disabled_labels(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    let mut items = Vec::new();
    for (root, is_user) in roots {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|s| s != "plist") {
                    continue;
                }
                let Ok(data) = read_plist(&path) else {
                    continue;
                };
                let Some(label) = data["Label"].as_str() else {
                    continue;
                };
                let exec = data["Program"]
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        data["ProgramArguments"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_default()
                    });
                let enabled = if is_user {
                    !overrides
                        .get(label)
                        .copied()
                        .unwrap_or(data["Disabled"].as_bool().unwrap_or(false))
                } else {
                    !data["Disabled"].as_bool().unwrap_or(false)
                };
                items.push(AutostartItem {
                    path: path.to_string_lossy().into_owned(),
                    name: label.into(),
                    exec,
                    icon: String::new(),
                    comment: if is_user {
                        "更改下次登录启动状态；当前进程不受影响"
                    } else {
                        "系统服务配置（只读，非运行状态）"
                    }
                    .into(),
                    enabled,
                    is_user,
                });
            }
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

pub fn toggle_autostart(path: &str, enable: bool) -> Result<String, String> {
    let home = platform::home().ok_or("无法识别用户目录")?;
    let root = fs::canonicalize(home.join("Library/LaunchAgents")).map_err(|e| e.to_string())?;
    let target = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if target.parent() != Some(root.as_path())
        || target.extension().is_none_or(|s| s != "plist")
        || fs::symlink_metadata(Path::new(path))
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("只能修改用户 LaunchAgents 目录中的启动项".into());
    }
    let data = read_plist(&target)?;
    let label = data["Label"]
        .as_str()
        .filter(|s| !s.is_empty() && !s.contains('/'))
        .ok_or("启动项 Label 无效")?;
    let service = format!("{}/{label}", domain()?);
    platform::checked_output(
        Command::new("/bin/launchctl").args([if enable { "enable" } else { "disable" }, &service]),
    )?;
    Ok("已更新下次登录启动状态".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launchctl_disabled_overrides() {
        let values = disabled_labels(
            "disabled services = {\n\t\"com.example.agent\" => true\n\t\"com.example.other\" => false\n}",
        );
        assert_eq!(values.get("com.example.agent"), Some(&true));
        assert_eq!(values.get("com.example.other"), Some(&false));
    }
}
