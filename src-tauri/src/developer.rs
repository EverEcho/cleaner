//! Developer cleanup: manifest-based project detection and aged generated artifacts.
use crate::platform;
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
pub struct DevRule {
    pub id: String,
    pub name: String,
    pub home_paths: Vec<String>,
    pub cache_paths: Vec<String>,
    pub description: String,
}
#[derive(Debug)]
pub struct DevTarget {
    pub name: String,
    pub path: PathBuf,
    pub reason: String,
}

pub fn rules() -> Vec<DevRule> {
    serde_json::from_str(include_str!("../rules/developer.json"))
        .expect("valid built-in developer rules")
}
pub fn cache_roots(home: &Path, cache: &Path) -> Vec<PathBuf> {
    rules()
        .iter()
        .flat_map(|rule| {
            rule.home_paths
                .iter()
                .map(|p| home.join(p))
                .chain(rule.cache_paths.iter().map(|p| cache.join(p)))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub fn old_enough(path: &Path, cutoff: SystemTime) -> Result<bool, String> {
    for entry in WalkDir::new(path) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_symlink() {
            continue;
        }
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        if meta.modified().map_err(|e| e.to_string())? > cutoff {
            return Ok(false);
        }
        if meta.is_file() && meta.accessed().is_ok_and(|t| t > cutoff) {
            return Ok(false);
        }
    }
    Ok(true)
}

fn protected_process_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        paths.push(exe);
    }
    #[cfg(target_os = "linux")]
    if let Ok(processes) = fs::read_dir("/proc") {
        for process in processes
            .flatten()
            .filter(|p| p.file_name().to_string_lossy().parse::<u32>().is_ok())
        {
            if let Ok(exe) = fs::read_link(process.path().join("exe")) {
                paths.push(exe);
            }
            if let Ok(cmd) = fs::read(process.path().join("cmdline")) {
                paths.extend(
                    cmd.split(|b| *b == 0)
                        .filter_map(|arg| std::str::from_utf8(arg).ok())
                        .filter(|arg| arg.starts_with('/'))
                        .map(PathBuf::from),
                );
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        use crate::query::QueryCommand;
        if let Ok(output) = std::process::Command::new("/bin/ps")
            .args(["-axo", "comm="])
            .query_output()
        {
            paths.extend(
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .filter(|line| line.starts_with('/'))
                    .map(PathBuf::from),
            );
        }
    }
    paths
}

fn contains_protected(path: &Path, processes: &[PathBuf]) -> bool {
    processes.iter().any(|p| p.starts_with(path))
}

fn collect_candidate(
    path: PathBuf,
    name: &str,
    reason: &str,
    cutoff: SystemTime,
    protected: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
    output: &mut Vec<DevTarget>,
) -> Result<(), String> {
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return Ok(());
    };
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Ok(());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if contains_protected(&canonical, protected) || !seen.insert(canonical.clone()) {
        return Ok(());
    }
    if old_enough(&canonical, cutoff)? {
        output.push(DevTarget {
            name: name.into(),
            path: canonical,
            reason: reason.into(),
        });
    }
    Ok(())
}

pub fn scan(
    home: &Path,
    cache: &Path,
    project_root: Option<&Path>,
    min_age_days: u64,
) -> Result<Vec<DevTarget>, String> {
    if !(1..=365).contains(&min_age_days) {
        return Err("保留时间需要在1至365天之间".into());
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(min_age_days * 86400))
        .ok_or("保留时间无效")?;
    let protected = protected_process_paths();
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    for rule in rules() {
        for root in rule
            .home_paths
            .iter()
            .map(|p| home.join(p))
            .chain(rule.cache_paths.iter().map(|p| cache.join(p)))
        {
            let entries = match fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(format!("无法读取 {}: {error}", root.display())),
            };
            for entry in entries {
                let entry = entry.map_err(|e| e.to_string())?;
                // Select separate historical cache entries; never offer an entire tool/SDK directory.
                let path = entry.path();
                if path.is_file() {
                    let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                    if !meta.file_type().is_symlink()
                        && meta.modified().is_ok_and(|t| t <= cutoff)
                        && meta.accessed().is_ok_and(|t| t <= cutoff)
                    {
                        let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
                        if !contains_protected(&canonical, &protected)
                            && seen.insert(canonical.clone())
                        {
                            targets.push(DevTarget {
                                name: rule.name.clone(),
                                path: canonical,
                                reason: rule.description.clone(),
                            });
                        }
                    }
                } else {
                    collect_candidate(
                        path,
                        &rule.name,
                        &rule.description,
                        cutoff,
                        &protected,
                        &mut seen,
                        &mut targets,
                    )?;
                }
            }
        }
    }
    if let Some(root) = project_root {
        targets.extend(scan_projects(root, cutoff, &protected, &mut seen)?);
    }
    Ok(targets)
}

fn project_artifacts(root: &Path) -> Vec<(&'static str, PathBuf)> {
    let mut items = Vec::new();
    if root.join("package.json").is_file() {
        for name in ["node_modules", ".next", ".nuxt", ".turbo"] {
            items.push(("Node.js 项目缓存/依赖", root.join(name)));
        }
    }
    if root.join("Cargo.toml").is_file() {
        items.push(("Rust target 构建产物", root.join("target")));
    }
    if root.join("pom.xml").is_file() {
        items.push(("Maven target 构建产物", root.join("target")));
    }
    if root.join("pubspec.yaml").is_file() {
        for name in [
            "build",
            ".dart_tool",
            "android/.gradle",
            "android/build",
            "android/app/build",
        ] {
            items.push(("Flutter 构建产物", root.join(name)));
        }
    }
    if root.join("build.gradle").is_file() || root.join("build.gradle.kts").is_file() {
        for name in ["build", ".gradle"] {
            items.push(("Gradle 项目构建产物", root.join(name)));
        }
    }
    if let Ok(entries) = fs::read_dir(root) {
        let paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        if paths.iter().any(|p| {
            p.extension()
                .is_some_and(|s| ["csproj", "fsproj", "vbproj"].iter().any(|ext| s == *ext))
        }) {
            for name in ["bin", "obj"] {
                items.push((".NET 构建产物", root.join(name)));
            }
        }
        if paths
            .iter()
            .any(|p| p.extension().is_some_and(|s| s == "py"))
        {
            items.push(("Python 字节码缓存", root.join("__pycache__")));
        }
    }
    items
}

pub fn scan_projects(
    root: &Path,
    cutoff: SystemTime,
    protected: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
) -> Result<Vec<DevTarget>, String> {
    let root = fs::canonicalize(root).map_err(|e| format!("项目目录无法访问: {e}"))?;
    if !root.is_dir()
        || root.parent().is_none()
        || [
            "/proc",
            "/sys",
            "/dev",
            "/etc",
            "/usr",
            "/var/lib/docker",
            "/var/lib/containerd",
        ]
        .iter()
        .any(|p| root.starts_with(p))
    {
        return Err("请选择具体的开发项目目录".into());
    }
    let mut output = Vec::new();
    for (count, entry) in WalkDir::new(&root)
        .max_depth(8)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            entry.depth() == 0
                || (!name.starts_with('.')
                    && ![
                        "node_modules",
                        "target",
                        "build",
                        "bin",
                        "obj",
                        "vendor",
                        "Pods",
                    ]
                    .contains(&name.as_ref()))
        })
        .enumerate()
    {
        if count > 100000 {
            return Err("项目扫描范围过大，请选择更小的目录".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().is_dir() {
            continue;
        }
        for (name, path) in project_artifacts(entry.path()) {
            collect_candidate(
                path,
                name,
                "可重新生成的项目产物；保留源码、锁文件、配置和 SDK；清理后需重新安装依赖或构建",
                cutoff,
                protected,
                seen,
                &mut output,
            )?;
        }
    }
    Ok(output)
}

pub fn default_roots() -> Vec<String> {
    let Some(home) = platform::home() else {
        return Vec::new();
    };
    [
        "Projects",
        "Developer",
        "Code",
        "Workspace",
        "workspace",
        "Documents",
    ]
    .iter()
    .map(|p| home.join(p))
    .filter(|p| p.is_dir())
    .map(|p| p.to_string_lossy().into_owned())
    .collect()
}

pub fn fingerprint(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let mut entries = Vec::new();
    for entry in WalkDir::new(path) {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        let modified = metadata
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        entries.push((entry.path().to_path_buf(), metadata.len(), modified));
    }
    entries.sort();
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
    )))
}

pub fn revalidate(path: &Path, days: u64, expected: &str) -> Result<(), String> {
    let cutoff = SystemTime::now() - Duration::from_secs(days * 86400);
    if contains_protected(path, &protected_process_paths())
        || !old_enough(path, cutoff)?
        || fingerprint(path)? != expected
    {
        return Err(format!(
            "开发者清理项目已被使用或改变，请重新扫描：{}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_recognition_keeps_locks_sources_and_unmarked_build_folders() {
        let root = std::env::temp_dir().join(format!("appsteward-dev-test-{}", std::process::id()));
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("package-lock.json"), "keep").unwrap();
        fs::write(root.join("app.ts"), "keep").unwrap();
        let artifacts = project_artifacts(&root);
        assert!(artifacts.iter().any(|(_, p)| p.ends_with("node_modules")));
        assert!(!artifacts.iter().any(|(_, p)| p.ends_with("build")
            || p.ends_with("package-lock.json")
            || p.ends_with("app.ts")));
        let mut seen = HashSet::new();
        let targets = scan_projects(
            &root,
            SystemTime::now() + Duration::from_secs(1),
            &[root.join("node_modules/tool")],
            &mut seen,
        )
        .unwrap();
        assert!(targets.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn aged_artifacts_are_counted_but_new_or_changed_entries_are_kept() {
        let root = std::env::temp_dir().join(format!("appsteward-dev-aged-{}", std::process::id()));
        let cache = root.join(".cache");
        let old = cache.join("pip/old");
        let fresh = cache.join("pip/new");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&fresh).unwrap();
        fs::write(old.join("wheel"), "cached wheel").unwrap();
        fs::write(fresh.join("wheel"), "recent").unwrap();
        let time = SystemTime::now() - Duration::from_secs(8 * 86400);
        let times = fs::FileTimes::new().set_modified(time).set_accessed(time);
        fs::File::open(old.join("wheel"))
            .unwrap()
            .set_times(times)
            .unwrap();
        fs::File::open(&old).unwrap().set_times(times).unwrap();
        let targets = scan(&root, &cache, None, 7).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].path, fs::canonicalize(&old).unwrap());
        let hash = fingerprint(&old).unwrap();
        assert!(revalidate(&old, 7, &hash).is_ok());
        fs::write(old.join("new-file"), "changed after scan").unwrap();
        assert!(revalidate(&old, 7, &hash).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn rules_never_target_sdk_binary_or_auth_configuration() {
        let mut ids = HashSet::new();
        for rule in rules() {
            assert!(ids.insert(rule.id));
            for path in rule.home_paths.iter().chain(&rule.cache_paths) {
                assert!(!path.split('/').any(|p| {
                    ["bin", "sdk", ".claude", ".aws", ".fvm", "workspaceStorage"].contains(&p)
                }));
                assert!(!path.contains(".."));
            }
        }
    }
}
