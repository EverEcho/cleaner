//! Associations use installed package identifiers and verified filesystem paths, never display names.
use crate::{models::AppInfo, platform};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

#[derive(Clone, Debug, Serialize)]
pub struct AppChannel {
    pub id: String,
    pub status: String,
    pub count: Option<usize>,
    pub note: String,
}
#[derive(Clone, Debug, Default, Serialize)]
pub struct AppScanReport {
    pub apps: Vec<AppInfo>,
    pub channels: Vec<AppChannel>,
    pub warnings: Vec<String>,
    pub checked_at: String,
}
static INVENTORY: LazyLock<Mutex<Option<AppScanReport>>> = LazyLock::new(|| Mutex::new(None));
pub fn save_inventory(report: &AppScanReport) {
    if let Ok(mut slot) = INVENTORY.lock() {
        *slot = Some(report.clone());
    }
}
pub fn find_app(kind: &str, key: &str) -> Result<AppInfo, String> {
    INVENTORY
        .lock()
        .map_err(|e| e.to_string())?
        .as_ref()
        .and_then(|report| {
            report
                .apps
                .iter()
                .find(|app| app.pkg_type == kind && app.key == key)
        })
        .cloned()
        .ok_or("应用不在当前扫描结果中，请刷新列表".into())
}

#[derive(Serialize)]
pub struct RelatedPath {
    pub path: String,
    pub size_bytes: Option<u64>,
    pub kind: String,
    pub note: String,
}
#[derive(Serialize)]
pub struct RelatedStartup {
    pub path: String,
    pub name: String,
    pub enabled: bool,
}
#[derive(Serialize)]
pub struct RelatedUpdate {
    pub current: String,
    pub latest: String,
    pub channel: String,
    pub checked_at: String,
}
#[derive(Serialize)]
pub struct AppProfile {
    pub app: AppInfo,
    pub caches: Vec<RelatedPath>,
    pub startup: Vec<RelatedStartup>,
    pub update: Option<RelatedUpdate>,
    pub notes: Vec<String>,
}

fn related_binary(app: &AppInfo, binary: &Path) -> bool {
    if !app.install_path.is_empty() {
        let install = Path::new(&app.install_path);
        if binary == install || install.is_dir() && binary.starts_with(install) {
            return true;
        }
    }
    #[cfg(not(target_os = "macos"))]
    if !app.desktop_file.is_empty()
        && let Ok(text) = fs::read_to_string(&app.desktop_file)
    {
        return crate::desktop::fields(&text)
            .get("Exec")
            .and_then(|exec| crate::manual_apps::executable(exec))
            .is_some_and(|actual| actual == binary);
    }
    false
}

pub fn profile(kind: &str, key: &str) -> Result<AppProfile, String> {
    let app = find_app(kind, key)?;
    let mut caches = Vec::new();
    let mut notes =
        vec!["关联结果来自当前应用扫描与已检测的更新报告；显示名相似不会建立关联。".into()];
    let mut roots: Vec<PathBuf> = Vec::new();
    if kind == "flatpak"
        && let Some((_, reference)) = key.split_once(':')
        && let Some(id) = reference
            .strip_prefix("app/")
            .and_then(|r| r.split('/').next())
        && let Some(home) = platform::home()
    {
        roots.push(home.join(".var/app").join(id).join("cache"));
    }
    #[cfg(not(target_os = "macos"))]
    if kind == "manual" {
        match crate::manual_apps::known_caches(key) {
            Ok(paths) => roots.extend(paths),
            Err(error) => notes.push(format!("缓存归属尚未确认：{error}")),
        }
    }
    #[cfg(target_os = "macos")]
    if !app.description.is_empty()
        && matches!(kind, "app" | "brew-cask")
        && app
            .description
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-_".contains(c))
    {
        if let Some(home) = platform::home() {
            roots.push(home.join("Library/Caches").join(&app.description));
            roots.push(
                home.join("Library/Containers")
                    .join(&app.description)
                    .join("Data/Library/Caches"),
            );
        }
    }
    for root in roots {
        if root.is_dir() && fs::symlink_metadata(&root).is_ok_and(|m| !m.file_type().is_symlink()) {
            caches.push(RelatedPath {
                size_bytes: None,
                path: root.to_string_lossy().into_owned(),
                kind: "cache".into(),
                note: "仅展示已确认的缓存路径；清理请到垃圾清理页复核。".into(),
            });
        }
    }
    if caches.is_empty() {
        notes.push("暂无可可靠关联的缓存；这不代表应用没有缓存。".into());
    }
    let startup = crate::autostart::scan_autostart_items()
        .into_iter()
        .filter(|item| {
            #[cfg(not(target_os = "macos"))]
            {
                crate::manual_apps::executable(&item.exec)
                    .is_some_and(|binary| related_binary(&app, &binary))
            }
            #[cfg(target_os = "macos")]
            {
                crate::packages::read_plist(Path::new(&item.path))
                    .ok()
                    .and_then(|data| {
                        data["Program"]
                            .as_str()
                            .map(PathBuf::from)
                            .or_else(|| data["ProgramArguments"][0].as_str().map(PathBuf::from))
                    })
                    .and_then(|p| fs::canonicalize(p).ok())
                    .is_some_and(|p| related_binary(&app, &p))
            }
        })
        .map(|item| RelatedStartup {
            path: item.path,
            name: item.name,
            enabled: item.enabled,
        })
        .collect();
    let update = crate::software_updates::latest_report().and_then(|report| {
        if !chrono::DateTime::parse_from_rfc3339(&report.expires_at)
            .is_ok_and(|expiry| expiry >= chrono::Utc::now())
        {
            notes.push("最近的更新报告已过期，请在软件更新页重新检测。".into());
            return None;
        }
        let manager = match kind {
            "deb" => "apt",
            other => other,
        };
        let candidates: Vec<_> = report
            .packages
            .iter()
            .filter(|p| p.manager == manager && p.name == key)
            .collect();
        let instance_count = 1;
        if candidates.len() == 1 && instance_count == 1 {
            let package = candidates[0];
            return Some(RelatedUpdate {
                current: package.current_version.clone(),
                latest: package.new_version.clone(),
                channel: package.manager.clone(),
                checked_at: report.checked_at.clone(),
            });
        }
        if candidates.len() > 1 || instance_count > 1 {
            notes.push("同一软件存在多个安装实例，当前更新报告无法可靠区分关联范围。".into());
        }
        let tools: Vec<_> = report
            .tools
            .iter()
            .filter(|tool| {
                tool.status == "available"
                    && !tool.install_path.is_empty()
                    && fs::canonicalize(&tool.install_path).is_ok_and(|p| related_binary(&app, &p))
            })
            .collect();
        (tools.len() == 1).then(|| {
            let tool = tools[0];
            RelatedUpdate {
                current: tool.current_version.clone(),
                latest: tool.latest_version.clone(),
                channel: tool.provider.clone(),
                checked_at: report.checked_at.clone(),
            }
        })
    });
    if update.is_none() {
        notes.push("尚无本次已检测且可靠匹配的可用更新；请在软件更新页检查。".into());
    }
    Ok(AppProfile {
        app,
        caches,
        startup,
        update,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn association_requires_exact_install_identity() {
        let app = AppInfo {
            install_path: "/opt/same-name".into(),
            ..AppInfo::default()
        };
        assert!(related_binary(&app, Path::new("/opt/same-name")));
        assert!(!related_binary(&app, Path::new("/opt/same-name-copy")));
        assert!(!related_binary(
            &AppInfo {
                name: "Editor".into(),
                ..AppInfo::default()
            },
            Path::new("/bin/editor")
        ));
    }
}
