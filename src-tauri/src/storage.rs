use crate::{models::format_size, platform};
use serde::Serialize;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Serialize)]
pub struct DirectoryUsage {
    pub path: String,
    pub size_bytes: u64,
    pub size: String,
    pub is_dir: bool,
}
#[derive(Serialize)]
pub struct DiskAnalysis {
    pub directories: Vec<DirectoryUsage>,
    pub warning: String,
    pub cancelled: bool,
}
pub fn roots() -> Vec<String> {
    let mut roots = Vec::new();
    if let Some(home) = platform::home() {
        roots.push(home.to_string_lossy().into_owned());
    }
    if cfg!(target_os = "linux") {
        roots.extend(
            [
                "/",
                "/var",
                "/var/log",
                "/var/cache",
                "/var/lib",
                "/tmp",
                "/var/tmp",
                "/home",
                "/var/crash",
                "/var/lib/systemd/coredump",
                "/usr/src",
                "/lib/modules",
            ]
            .map(str::to_string),
        );
    } else {
        roots.extend(["/Applications", "/Library", "/Users"].map(str::to_string));
    }
    roots.retain(|path| Path::new(path).is_dir());
    roots.dedup();
    roots
}
#[cfg(unix)]
fn same_device(root: &fs::Metadata, entry: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    root.dev() == entry.dev()
}
#[cfg(not(unix))]
fn same_device(_: &fs::Metadata, _: &fs::Metadata) -> bool {
    true
}

fn normalized_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("保留名单需要绝对路径".into());
    }
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // Missing entries cannot contain counted files, but normalize their intended scope.
            let mut normalized = PathBuf::new();
            for component in path.components() {
                match component {
                    Component::ParentDir => {
                        normalized.pop();
                    }
                    Component::CurDir => {}
                    other => normalized.push(other.as_os_str()),
                }
            }
            Ok(normalized)
        }
        Err(error) => Err(format!("无法核对保留路径 {}: {error}", path.display())),
    }
}

pub fn analyze(root: &str) -> Result<DiskAnalysis, String> {
    analyze_controlled(root, &[], &|| false, |_, _, _| {})
}
pub fn analyze_controlled(
    root: &str,
    excluded: &[String],
    cancelled: &dyn Fn() -> bool,
    mut progress: impl FnMut(&str, u64, u64),
) -> Result<DiskAnalysis, String> {
    let root = Path::new(root);
    if !root.is_absolute() || !root.is_dir() {
        return Err("请选择存在的绝对目录路径".into());
    }
    let root = fs::canonicalize(root).map_err(|error| format!("无法核对分析目录: {error}"))?;
    let excluded = excluded
        .iter()
        .map(|path| normalized_absolute(Path::new(path)))
        .collect::<Result<Vec<_>, _>>()?;
    let root_meta = root.metadata().map_err(|error| error.to_string())?;
    let mut result = DiskAnalysis {
        directories: Vec::new(),
        warning: String::new(),
        cancelled: false,
    };
    if excluded.iter().any(|excluded| root.starts_with(excluded)) {
        result.warning = "当前目录在保留名单内，已跳过分析".into();
        return Ok(result);
    }
    let mut warnings = Vec::new();
    let mut files = 0;
    for child in fs::read_dir(&root).map_err(|error| error.to_string())? {
        if cancelled() {
            result.cancelled = true;
            break;
        }
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                warnings.push(error.to_string());
                continue;
            }
        };
        let path = child.path();
        if excluded.iter().any(|excluded| path.starts_with(excluded)) {
            continue;
        }
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(error) => {
                warnings.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        if meta.file_type().is_symlink() || !same_device(&root_meta, &meta) {
            continue;
        }
        let mut bytes = 0;
        for entry in WalkDir::new(&path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                !cancelled()
                    && !excluded
                        .iter()
                        .any(|excluded| entry.path().starts_with(excluded))
                    && entry
                        .metadata()
                        .map_or(true, |meta| same_device(&root_meta, &meta))
            })
        {
            if cancelled() {
                result.cancelled = true;
                break;
            }
            match entry {
                Ok(entry) if entry.file_type().is_file() => match entry.metadata() {
                    Ok(meta) => {
                        bytes += meta.len();
                        files += 1;
                        if files % 100 == 0 {
                            progress(&entry.path().to_string_lossy(), files, 0);
                        }
                    }
                    Err(error) => {
                        if warnings.len() < 100 {
                            warnings.push(error.to_string());
                        }
                    }
                },
                Err(error) if warnings.len() < 100 => warnings.push(error.to_string()),
                _ => {}
            }
        }
        if cancelled() {
            result.cancelled = true;
            break;
        }
        progress(&path.to_string_lossy(), files, 0);
        result.directories.push(DirectoryUsage {
            path: path.to_string_lossy().into_owned(),
            size_bytes: bytes,
            size: format_size(bytes),
            is_dir: meta.is_dir(),
        });
    }
    if result.cancelled {
        warnings.push("分析已停止，当前仅显示已完整统计的项目".into());
    }
    result.warning = warnings.join("\n");
    result
        .directories
        .sort_by_key(|entry| std::cmp::Reverse(entry.size_bytes));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn alias_roots_and_exclusions_share_canonical_scope_without_following_child_links() {
        use std::os::unix::fs::symlink;
        let base =
            std::env::temp_dir().join(format!("cleaner-storage-alias-{}", std::process::id()));
        let actual = base.join("actual");
        fs::create_dir_all(actual.join("keep")).unwrap();
        fs::create_dir_all(base.join("outside")).unwrap();
        fs::write(actual.join("leaf"), b"123456").unwrap();
        fs::write(actual.join("keep/cache"), b"protected").unwrap();
        fs::write(base.join("outside/unrelated"), b"must not follow").unwrap();
        symlink(&actual, base.join("alias")).unwrap();
        symlink(actual.join("keep"), base.join("kept-alias")).unwrap();
        symlink(base.join("outside"), actual.join("child-link")).unwrap();
        for excluded in [base.join("kept-alias"), base.join("alias/keep/../keep")] {
            let report = analyze_controlled(
                base.join("alias").to_str().unwrap(),
                &[excluded.to_string_lossy().into_owned()],
                &|| false,
                |_, _, _| {},
            )
            .unwrap();
            assert_eq!(report.directories.len(), 1);
            assert_eq!(
                report.directories[0].path,
                actual.join("leaf").to_string_lossy()
            );
            assert_eq!(report.directories[0].size_bytes, 6);
        }
        let protected = analyze_controlled(
            actual.join("keep").to_str().unwrap(),
            &[base.join("kept-alias").to_string_lossy().into_owned()],
            &|| false,
            |_, _, _| {},
        )
        .unwrap();
        assert!(protected.directories.is_empty());
        assert!(protected.warning.contains("保留名单"));
        assert_eq!(
            normalized_absolute(&actual.join("keep/../missing")).unwrap(),
            actual.join("missing")
        );
        assert!(normalized_absolute(Path::new("relative/path")).is_err());
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn analysis_includes_leaf_files_and_excludes_subtrees() {
        let root = std::env::temp_dir().join(format!("cleaner-storage-{}", std::process::id()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("leaf"), b"123456").unwrap();
        fs::write(root.join("nested/skip"), b"1234").unwrap();
        let result = analyze_controlled(
            root.to_str().unwrap(),
            &[root.join("nested").to_string_lossy().into_owned()],
            &|| false,
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(result.directories.len(), 1);
        assert_eq!(result.directories[0].size_bytes, 6);
        assert!(!result.directories[0].is_dir);
        let cancelled =
            analyze_controlled(root.to_str().unwrap(), &[], &|| true, |_, _, _| {}).unwrap();
        assert!(cancelled.cancelled);
        assert!(cancelled.directories.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
