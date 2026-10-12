//! Resolve local application artwork once per inventory, without exposing file access to the UI.
use crate::{models::AppInfo, platform};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

fn image_uri(path: &Path) -> Option<String> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > 1024 * 1024 {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        "image/jpeg"
    } else if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("svg"))
        && std::str::from_utf8(&bytes).ok()?.contains("<svg")
    {
        "image/svg+xml"
    } else {
        return None;
    };
    Some(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
}

pub fn resolve_linux(apps: &mut [AppInfo]) {
    let mut roots = Vec::new();
    if let Some(home) = platform::home() {
        roots.push(home.join(".icons"));
    }
    if let Some(data) = platform::xdg_dir("XDG_DATA_HOME", ".local/share") {
        roots.push(data.join("icons"));
        roots.push(data.join("flatpak/exports/share/icons"));
    }
    let dirs =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for dir in std::env::split_paths(&dirs).filter(|p| p.is_absolute()) {
        roots.push(dir.join("icons"));
        roots.push(dir.join("pixmaps"));
    }
    roots.push("/var/lib/flatpak/exports/share/icons".into());
    let mut index: HashMap<String, (u8, PathBuf)> = HashMap::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .max_depth(7)
            .into_iter()
            .take(60000)
            .flatten()
        {
            let path = entry.path();
            if !path.is_file()
                || path.extension().is_none_or(|s| {
                    !["png", "svg", "jpg", "jpeg"]
                        .iter()
                        .any(|e| s.eq_ignore_ascii_case(e))
                })
            {
                continue;
            }
            let key = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let text = path.to_string_lossy();
            let score = if text.contains("/48x48/") {
                0
            } else if text.contains("/64x64/") {
                1
            } else if text.contains("/128x128/") {
                2
            } else if text.contains("/scalable/") {
                3
            } else {
                4
            };
            if index.get(&key).is_none_or(|(old, _)| score < *old) {
                index.insert(key, (score, path.to_path_buf()));
            }
        }
    }
    let mut cache = HashMap::new();
    for app in apps {
        let raw = app.icon.clone();
        let key = if raw.is_empty() || raw == "package-x-generic" || raw == "snapcraft" {
            Path::new(&app.desktop_file)
                .file_stem()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.key.clone())
        } else {
            raw
        };
        app.icon = cache
            .entry(key.clone())
            .or_insert_with(|| {
                let path = if Path::new(&key).is_absolute() {
                    Some(PathBuf::from(&key))
                } else {
                    index
                        .get(&key)
                        .or_else(|| index.get(Path::new(&key).file_stem()?.to_str()?))
                        .map(|(_, p)| p.clone())
                };
                path.and_then(|p| image_uri(&p)).unwrap_or_default()
            })
            .clone();
    }
}

pub fn bundle_icon(bundle: &Path, name: &str) -> String {
    let resources = bundle.join("Contents/Resources");
    let candidate = if name.is_empty() {
        let Some(path) = fs::read_dir(&resources).ok().and_then(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .find(|path| path.extension().is_some_and(|s| s == "icns"))
        }) else {
            return String::new();
        };
        path
    } else {
        resources.join(name)
    };
    let source = if candidate.extension().is_none() {
        candidate.with_extension("icns")
    } else {
        candidate
    };
    let Ok(source) = fs::canonicalize(source) else {
        return String::new();
    };
    let Ok(resources) = fs::canonicalize(resources) else {
        return String::new();
    };
    if !source.starts_with(resources) {
        return String::new();
    }
    if let Some(uri) = image_uri(&source) {
        return uri;
    }
    if source.extension().is_none_or(|s| s != "icns")
        || fs::metadata(&source).is_ok_and(|m| m.len() > 16 * 1024 * 1024)
    {
        return String::new();
    }
    // sips is a macOS system utility. Each conversion uses an exclusive private directory.
    use crate::query::QueryCommand;
    use std::{
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let temp = std::env::temp_dir().join(format!(
        "appsteward-icon-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    if builder.create(&temp).is_err() {
        return String::new();
    }
    let output = temp.join("icon.png");
    let result = Command::new("/usr/bin/sips")
        .args(["-s", "format", "png", "-Z", "64"])
        .arg(&source)
        .arg("--out")
        .arg(&output)
        .query_output();
    let uri = result
        .ok()
        .filter(|o| o.status.success())
        .and_then(|_| image_uri(&output))
        .unwrap_or_default();
    let _ = fs::remove_dir_all(temp);
    uri
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_non_images_and_missing_files() {
        assert!(image_uri(Path::new("/does-not-exist/icon.png")).is_none());
        assert!(
            image_uri(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("Cargo.toml")
                    .as_path()
            )
            .is_none()
        );
    }
}
