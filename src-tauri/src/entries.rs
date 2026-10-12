use crate::{desktop, models::DesktopEntryItem, platform};
use std::{collections::HashSet, fs, path::Path};

pub fn scan_desktop_entries() -> Vec<DesktopEntryItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for (root, scope) in desktop::application_dirs() {
        if let Ok(entries) = fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|s| s != "desktop") {
                    continue;
                }
                let basename = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                if !seen.insert(basename.clone()) {
                    continue;
                }
                let Ok(content) = fs::read_to_string(&path) else {
                    continue;
                };
                let fields = desktop::fields(&content);
                items.push(DesktopEntryItem {
                    path: path.to_string_lossy().into_owned(),
                    name: fields.get("Name").copied().unwrap_or(&basename).into(),
                    basename,
                    exec: fields.get("Exec").copied().unwrap_or("").into(),
                    icon: fields.get("Icon").copied().unwrap_or("").into(),
                    scope: scope.into(),
                    visible: fields.get("NoDisplay") != Some(&"true")
                        && fields.get("Hidden") != Some(&"true"),
                    categories: fields.get("Categories").copied().unwrap_or("").into(),
                });
            }
        }
    }
    items.sort_by_key(|a| a.name.to_lowercase());
    items
}

pub fn toggle_desktop_entry_visibility(path: &str, visible: bool) -> Result<String, String> {
    let user = platform::xdg_dir("XDG_DATA_HOME", ".local/share")
        .ok_or("无法识别用户目录")?
        .join("applications");
    let roots = desktop::application_dirs()
        .into_iter()
        .map(|(p, _)| p)
        .collect::<Vec<_>>();
    desktop::user_override(
        Path::new(path),
        &roots,
        &user,
        &[
            ("NoDisplay", if visible { "false" } else { "true" }),
            ("Hidden", "false"),
        ],
    )?;
    Ok("已通过用户配置更新启动器可见性".into())
}
