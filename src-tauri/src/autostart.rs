use crate::{desktop, models::AutostartItem, platform};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

fn roots() -> Vec<(PathBuf, bool)> {
    let mut roots = Vec::new();
    if let Some(user) = user_root() {
        roots.push((user, true));
    }
    let config_dirs = std::env::var_os("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".into());
    roots.extend(
        std::env::split_paths(&config_dirs)
            .filter(|p| p.is_absolute())
            .map(|p| (p.join("autostart"), false)),
    );
    roots
}

pub(crate) fn user_root() -> Option<PathBuf> {
    platform::xdg_dir("XDG_CONFIG_HOME", ".config").map(|config| config.join("autostart"))
}

fn is_missing_absolute(program: &str) -> bool {
    let path = Path::new(program);
    path.is_absolute() && fs::metadata(path).is_err_and(|error| error.kind() == ErrorKind::NotFound)
}

fn missing_program(fields: &HashMap<&str, &str>) -> Option<String> {
    let exec = fields
        .get("Exec")
        .and_then(|exec| crate::manual_apps::exec_program(exec));
    exec.as_deref()
        .into_iter()
        .chain(fields.get("TryExec").copied())
        .find(|program| is_missing_absolute(program))
        .map(str::to_owned)
}

#[cfg(target_os = "linux")]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct StaleUserEntry {
    pub path: PathBuf,
    pub canonical: PathBuf,
    pub name: String,
    pub program: String,
    content: String,
}

#[cfg(target_os = "linux")]
impl StaleUserEntry {
    pub fn size(&self) -> u64 {
        self.content.len() as u64
    }

    pub fn still_matches(&self) -> bool {
        stale_user_entry(&self.path).as_ref() == Some(self)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn stale_user_entry(path: &Path) -> Option<StaleUserEntry> {
    use std::io::Read;

    let user = user_root()?;
    let filename = path.file_name()?;
    if path.parent() != Some(user.as_path()) || path.extension()? != "desktop" {
        return None;
    }
    // A user entry can override a system entry with the same basename.
    if roots().into_iter().any(|(root, is_user)| {
        !is_user
            && !fs::symlink_metadata(root.join(filename))
                .is_err_and(|error| error.kind() == ErrorKind::NotFound)
    }) {
        return None;
    }
    const MAX_ENTRY_BYTES: u64 = 128 * 1024;
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ENTRY_BYTES {
        return None;
    }
    let mut content = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_ENTRY_BYTES + 1)
        .read_to_string(&mut content)
        .ok()?;
    let fields = desktop::fields(&content);
    if content.len() as u64 > MAX_ENTRY_BYTES
        || fields.get("Type") != Some(&"Application")
        || fields.get("DBusActivatable") == Some(&"true")
    {
        return None;
    }
    let program = crate::manual_apps::exec_program(fields.get("Exec")?)?;
    if !is_missing_absolute(&program) {
        return None;
    }
    Some(StaleUserEntry {
        path: path.to_path_buf(),
        canonical: fs::canonicalize(path).ok()?,
        name: fields
            .get("Name")
            .copied()
            .unwrap_or(filename.to_str()?)
            .into(),
        program,
        content,
    })
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
                    missing_program: missing_program(&fields),
                });
            }
        }
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));
    items
}

pub fn toggle_autostart(path: &str, enable: bool) -> Result<String, String> {
    let user = user_root().ok_or("无法识别用户目录")?;
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
