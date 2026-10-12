use crate::{desktop, models::AutostartItem, platform};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

fn roots() -> Vec<(PathBuf, bool)> {
    let mut roots = Vec::new();
    if let Some(config) = platform::xdg_dir("XDG_CONFIG_HOME", ".config") {
        roots.push((config.join("autostart"), true));
    }
    let config_dirs = std::env::var_os("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
    roots.extend(
        std::env::split_paths(&config_dirs)
            .filter(|p| p.is_absolute())
            .map(|p| (p.join("autostart"), false)),
    );
    roots
}

pub fn scan_autostart_items() -> Vec<AutostartItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for (root, is_user) in roots() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|s| s != "desktop")
                    || !seen.insert(path.file_name().unwrap_or_default().to_owned())
                {
                    continue;
                }
                let Ok(content) = fs::read_to_string(&path) else {
                    continue;
                };
                let fields = desktop::fields(&content);
                items.push(AutostartItem {
                    path: path.to_string_lossy().into_owned(),
                    name: fields.get("Name").copied().unwrap_or("未知应用").into(),
                    exec: fields.get("Exec").copied().unwrap_or("").into(),
                    icon: fields.get("Icon").copied().unwrap_or("").into(),
                    comment: fields.get("Comment").copied().unwrap_or("").into(),
                    enabled: fields.get("Hidden") != Some(&"true")
                        && fields.get("X-GNOME-Autostart-enabled") != Some(&"false"),
                    is_user,
                });
            }
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

pub fn toggle_autostart(path: &str, enable: bool) -> Result<String, String> {
    let user = platform::xdg_dir("XDG_CONFIG_HOME", ".config")
        .ok_or("无法识别用户目录")?
        .join("autostart");
    let allowed = roots().into_iter().map(|(p, _)| p).collect::<Vec<_>>();
    desktop::user_override(
        Path::new(path),
        &allowed,
        &user,
        &[
            ("Hidden", if enable { "false" } else { "true" }),
            (
                "X-GNOME-Autostart-enabled",
                if enable { "true" } else { "false" },
            ),
        ],
    )?;
    Ok("已通过用户配置更新登录启动状态".into())
}
