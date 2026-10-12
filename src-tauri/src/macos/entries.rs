use super::packages;
use crate::models::DesktopEntryItem;

pub fn scan_desktop_entries() -> Vec<DesktopEntryItem> {
    packages::scan_bundles(&packages::application_roots())
        .into_iter()
        .map(|a| DesktopEntryItem {
            basename: std::path::Path::new(&a.key)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: a.key.clone(),
            name: a.name,
            exec: a.key,
            icon: a.icon,
            scope: if a.is_system { "系统" } else { "应用" }.into(),
            visible: true,
            categories: String::new(),
        })
        .collect()
}

pub fn toggle_desktop_entry_visibility(_path: &str, _visible: bool) -> Result<String, String> {
    Err("macOS 应用目录只读；系统不提供与 .desktop 等价的菜单隐藏操作".into())
}
