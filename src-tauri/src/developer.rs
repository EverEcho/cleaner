//! Developer cleanup: manifest-based project detection and aged generated artifacts.
use crate::platform;
use serde::Deserialize;
use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
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
    pub size_bytes: u64,
    pub fingerprint: String,
}

#[derive(Debug, Default)]
pub struct DevScan {
    pub targets: Vec<DevTarget>,
    pub warnings: Vec<String>,
    pub directories_scanned: usize,
    pub projects_found: usize,
    pub storage_skipped: usize,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct DevProgress {
    pub path: String,
    pub directories_scanned: usize,
    pub projects_found: usize,
}
impl DevScan {
    fn warn(&mut self, warning: String) {
        if self.warnings.len() < 20 {
            self.warnings.push(warning);
        }
    }
}

fn aged_snapshot(
    path: &Path,
    cutoff: SystemTime,
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<(u64, String)>, String> {
    let mut entries = Vec::new();
    let mut size = 0u64;
    for entry in WalkDir::new(path) {
        if cancelled() {
            return Err("扫描已停止".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        let modified = meta.modified().map_err(|e| e.to_string())?;
        if !meta.file_type().is_symlink() {
            if modified > cutoff
                || meta.is_file() && meta.accessed().is_ok_and(|time| time > cutoff)
            {
                return Ok(None);
            }
            if meta.is_file() {
                size = size.saturating_add(meta.len());
            }
        }
        entries.push((
            entry.path().to_path_buf(),
            meta.len(),
            modified
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        ));
    }
    entries.sort();
    use sha2::{Digest, Sha256};
    Ok(Some((
        size,
        hex::encode(Sha256::digest(
            serde_json::to_vec(&entries).map_err(|e| e.to_string())?,
        )),
    )))
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
    protected_process_paths_controlled(&|| false)
}
fn protected_process_paths_controlled(cancelled: &dyn Fn() -> bool) -> Vec<PathBuf> {
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
            if cancelled() {
                break;
            }
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

struct ScanSafety<'a> {
    protected: &'a [PathBuf],
    excluded: &'a [PathBuf],
    cancelled: &'a dyn Fn() -> bool,
}

fn collect_candidate(
    path: PathBuf,
    name: &str,
    reason: &str,
    cutoff: SystemTime,
    safety: &ScanSafety<'_>,
    seen: &mut HashSet<PathBuf>,
    output: &mut Vec<DevTarget>,
) -> Result<(), String> {
    let ScanSafety {
        protected,
        excluded,
        cancelled,
    } = safety;
    if cancelled()
        || excluded
            .iter()
            .any(|root| path.starts_with(root) || root.starts_with(&path))
    {
        return Ok(());
    }
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    if meta.file_type().is_symlink() || (!meta.is_dir() && !meta.is_file()) {
        return Ok(());
    }
    let canonical = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if contains_protected(&canonical, protected) || !seen.insert(canonical.clone()) {
        return Ok(());
    }
    if let Some((size_bytes, fingerprint)) = aged_snapshot(&canonical, cutoff, cancelled)? {
        output.push(DevTarget {
            name: name.into(),
            path: canonical,
            reason: reason.into(),
            size_bytes,
            fingerprint,
        });
    }
    Ok(())
}

pub fn scan(
    home: &Path,
    cache: &Path,
    project_root: Option<&Path>,
    min_age_days: u64,
) -> Result<DevScan, String> {
    scan_with_progress(home, cache, project_root, min_age_days, |_| {})
}
pub fn scan_with_progress(
    home: &Path,
    cache: &Path,
    project_root: Option<&Path>,
    min_age_days: u64,
    report: impl FnMut(DevProgress),
) -> Result<DevScan, String> {
    let roots = project_root
        .into_iter()
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    scan_controlled(home, cache, &roots, min_age_days, &[], &|| false, report)
}
pub fn scan_controlled(
    home: &Path,
    cache: &Path,
    project_roots: &[PathBuf],
    min_age_days: u64,
    excluded: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
    mut report: impl FnMut(DevProgress),
) -> Result<DevScan, String> {
    if !(1..=365).contains(&min_age_days) {
        return Err("保留时间需要在1至365天之间".into());
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(min_age_days * 86400))
        .ok_or("保留时间无效")?;
    let protected = protected_process_paths_controlled(cancelled);
    let safety = ScanSafety {
        protected: &protected,
        excluded,
        cancelled,
    };
    let mut result = DevScan::default();
    let mut seen = HashSet::new();
    for rule in rules() {
        for root in rule
            .home_paths
            .iter()
            .map(|path| home.join(path))
            .chain(rule.cache_paths.iter().map(|path| cache.join(path)))
        {
            if cancelled() {
                break;
            }
            if excluded.iter().any(|path| root.starts_with(path)) {
                continue;
            }
            report(DevProgress {
                path: root.to_string_lossy().into_owned(),
                directories_scanned: result.directories_scanned,
                projects_found: result.projects_found,
            });
            let entries = match fs::read_dir(&root) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => {
                    result.warn(format!("无法读取 {}: {error}", root.display()));
                    continue;
                }
            };
            result.directories_scanned += 1;
            for entry in entries {
                if cancelled() {
                    break;
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        result.warn(error.to_string());
                        continue;
                    }
                };
                if let Err(error) = collect_candidate(
                    entry.path(),
                    &rule.name,
                    &rule.description,
                    cutoff,
                    &safety,
                    &mut seen,
                    &mut result.targets,
                ) {
                    result.warn(error);
                }
            }
        }
    }
    for root in project_roots {
        if cancelled() {
            break;
        }
        let scanned_before = result.directories_scanned;
        let projects_before = result.projects_found;
        let mut project_report = |mut value: DevProgress| {
            value.directories_scanned += scanned_before;
            value.projects_found += projects_before;
            report(value);
        };
        match scan_projects_controlled(
            root,
            cutoff,
            &safety,
            &mut seen,
            ScanLimits::default(),
            &mut project_report,
        ) {
            Ok(projects) => {
                result.targets.extend(projects.targets);
                result.warnings.extend(projects.warnings);
                result.storage_skipped += projects.storage_skipped;
                result.directories_scanned += projects.directories_scanned;
                result.projects_found += projects.projects_found;
            }
            Err(error) => result.warn(format!("{}: {error}", root.display())),
        }
    }
    if cancelled() {
        result.warn("扫描已停止，当前仅包含已完成统计的项目".into());
    }
    Ok(result)
}

fn project_artifacts(root: &Path, names: &[std::ffi::OsString]) -> Vec<(&'static str, PathBuf)> {
    let has = |name: &str| names.iter().any(|file| file == name);
    let mut items = Vec::new();
    if has("package.json") {
        for name in ["node_modules", ".next", ".nuxt", ".turbo"] {
            items.push(("Node.js 项目缓存/依赖", root.join(name)));
        }
    }
    if has("Cargo.toml") {
        items.push(("Rust target 构建产物", root.join("target")));
    }
    if has("pom.xml") {
        items.push(("Maven target 构建产物", root.join("target")));
    }
    if has("pubspec.yaml") {
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
    if has("build.gradle") || has("build.gradle.kts") {
        for name in ["build", ".gradle"] {
            items.push(("Gradle 项目构建产物", root.join(name)));
        }
    }
    if names.iter().any(|name| {
        Path::new(name).extension().is_some_and(|ext| {
            ["csproj", "fsproj", "vbproj"]
                .iter()
                .any(|value| ext == *value)
        })
    }) {
        for name in ["bin", "obj"] {
            items.push((".NET 构建产物", root.join(name)));
        }
    }
    if names
        .iter()
        .any(|name| Path::new(name).extension().is_some_and(|ext| ext == "py"))
    {
        items.push(("Python 字节码缓存", root.join("__pycache__")));
    }
    items
}
#[derive(Clone, Copy)]
struct ScanLimits {
    directories: usize,
    depth: usize,
    duration: Duration,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            directories: 200000,
            depth: 16,
            duration: Duration::from_secs(90),
        }
    }
}
fn ignored_directory(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.')
        || name.ends_with(".app")
        || [
            "node_modules",
            "target",
            "build",
            "bin",
            "obj",
            "vendor",
            "Pods",
            "__pycache__",
            "dist",
            "coverage",
            "venv",
            "env",
            "site-packages",
            "sdk",
            "SDK",
            "ndk",
            "jbr",
            "platform-tools",
            "platforms",
            "system-images",
        ]
        .contains(&name.as_ref())
}

pub fn scan_projects(
    root: &Path,
    cutoff: SystemTime,
    protected: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
) -> Result<DevScan, String> {
    scan_projects_with_limits(
        root,
        cutoff,
        protected,
        seen,
        ScanLimits::default(),
        &mut |_| {},
    )
}
// Runtime storage is not a development artifact, even if it contains manifests or pycache.
const DATABASE_MARKERS: [&str; 6] = [
    "PG_VERSION",
    "ibdata1",
    "auto.cnf",
    "dump.rdb",
    "appendonly.aof",
    "WiredTiger",
];
const COMPOSE_FILES: [&str; 4] = [
    "compose.yml",
    "compose.yaml",
    "docker-compose.yml",
    "docker-compose.yaml",
];
fn database_storage(directory: &Path) -> bool {
    DATABASE_MARKERS
        .iter()
        .any(|marker| directory.join(marker).is_file())
}
fn compose_storage(service: &Path, compose: &Path) -> Vec<PathBuf> {
    let Ok(meta) = compose.metadata() else {
        return vec![];
    };
    if meta.len() > 256 * 1024 {
        return vec![];
    }
    let Ok(text) = fs::read_to_string(compose) else {
        return vec![];
    };
    let mut roots = Vec::new();
    let mut source = None;
    for line in text.lines() {
        let line = line
            .split('#')
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(['\"', '\'']);
        let pair = if let Some(value) = line.strip_prefix("- ") {
            value
                .trim_matches(['\"', '\''])
                .split_once(':')
                .map(|(a, b)| (a.to_string(), b.split(':').next().unwrap_or("").to_string()))
        } else if let Some(value) = line.strip_prefix("source:") {
            source = Some(value.trim().trim_matches(['\"', '\'']).to_string());
            None
        } else if let Some(value) = line.strip_prefix("target:") {
            source
                .take()
                .map(|source| (source, value.trim().trim_matches(['\"', '\'']).to_string()))
        } else {
            None
        };
        if let Some((source, target)) = pair {
            if ![
                "/var/lib/",
                "/var/log/",
                "/data",
                "/config",
                "/cache",
                "/storage",
                "/models",
                "/backends",
                "/uploads",
            ]
            .iter()
            .any(|prefix| {
                target == *prefix
                    || target.starts_with(&format!("{}/", prefix.trim_end_matches('/')))
            }) {
                continue;
            }
            let source = Path::new(&source);
            if !(source.is_absolute() || source.starts_with("."))
                || source
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                continue;
            }
            let path = if source.is_absolute() {
                source.to_path_buf()
            } else {
                service.join(source)
            };
            if let Ok(path) = fs::canonicalize(path)
                && path != service
                && path.is_dir()
            {
                roots.push(path);
            }
        }
    }
    // Prior PostgreSQL major-version data beside the current Compose mount stays protected.
    if roots
        .iter()
        .any(|root| root.file_name().is_some_and(|name| name == "data"))
        && text.contains("/var/lib/postgresql")
        && let Ok(children) = fs::read_dir(service)
    {
        for child in children.flatten() {
            if child
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("data_pg"))
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
                })
            {
                roots.push(child.path());
            }
        }
    }
    roots
}
fn storage_ancestor(root: &Path) -> bool {
    root.ancestors().take(6).any(|ancestor| {
        database_storage(ancestor)
            || COMPOSE_FILES.iter().any(|file| {
                compose_storage(ancestor, &ancestor.join(file))
                    .iter()
                    .any(|storage| root.starts_with(storage))
            })
    })
}

fn scan_projects_with_limits(
    root: &Path,
    cutoff: SystemTime,
    protected: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
    limits: ScanLimits,
    report: &mut impl FnMut(DevProgress),
) -> Result<DevScan, String> {
    scan_projects_controlled(
        root,
        cutoff,
        &ScanSafety {
            protected,
            excluded: &[],
            cancelled: &|| false,
        },
        seen,
        limits,
        report,
    )
}
fn scan_projects_controlled(
    root: &Path,
    cutoff: SystemTime,
    safety: &ScanSafety<'_>,
    seen: &mut HashSet<PathBuf>,
    limits: ScanLimits,
    report: &mut impl FnMut(DevProgress),
) -> Result<DevScan, String> {
    let ScanSafety {
        excluded,
        cancelled,
        ..
    } = safety;
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
        .any(|path| root.starts_with(path))
    {
        return Err("请选择具体的开发项目目录".into());
    }
    let mut result = DevScan::default();
    if storage_ancestor(&root) {
        result.storage_skipped = 1;
        return Ok(result);
    }
    let mut storage_roots = Vec::<PathBuf>::new();
    let mut queue = VecDeque::from([(root, 0usize)]);
    let start = Instant::now();
    let mut last_report = start;
    let mut deep = 0;
    while let Some((directory, depth)) = queue.pop_front() {
        if cancelled() {
            result.warn("扫描已停止，尚有项目目录未覆盖".into());
            break;
        }
        if excluded.iter().any(|root| directory.starts_with(root)) {
            continue;
        }
        if storage_roots.iter().any(|root| directory.starts_with(root)) {
            result.storage_skipped += 1;
            continue;
        }
        if result.directories_scanned >= limits.directories || start.elapsed() >= limits.duration {
            result.warn(format!("已扫描 {} 个目录、识别 {} 个候选项目目录；还有约 {} 个目录未扫描，已保留当前结果。可选择其中的子目录继续扫描。",result.directories_scanned,result.projects_found,queue.len()+1));
            break;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                result.warn(format!("无法读取 {}: {error}", directory.display()));
                continue;
            }
        };
        result.directories_scanned += 1;
        let mut names = Vec::new();
        let mut compose_files = Vec::new();
        let mut child_directories = Vec::new();
        let mut has_database_marker = false;
        for entry in entries {
            if cancelled() {
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    result.warn(error.to_string());
                    continue;
                }
            };
            let kind = match entry.file_type() {
                Ok(kind) => kind,
                Err(error) => {
                    result.warn(error.to_string());
                    continue;
                }
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                if !ignored_directory(&entry.file_name()) {
                    if depth < limits.depth {
                        child_directories.push((entry.path(), depth + 1));
                    } else {
                        deep += 1;
                    }
                }
            } else if kind.is_file() {
                let name = entry.file_name();
                has_database_marker |= DATABASE_MARKERS.iter().any(|marker| name == *marker);
                if COMPOSE_FILES.iter().any(|file| name == *file) {
                    compose_files.push(entry.path());
                }
                if [
                    "package.json",
                    "Cargo.toml",
                    "pom.xml",
                    "pubspec.yaml",
                    "build.gradle",
                    "build.gradle.kts",
                ]
                .iter()
                .any(|value| name == *value)
                    || Path::new(&name).extension().is_some_and(|ext| {
                        ["csproj", "fsproj", "vbproj", "py"]
                            .iter()
                            .any(|value| ext == *value)
                    })
                {
                    names.push(name);
                }
            }
        }
        if has_database_marker {
            result.storage_skipped += 1;
            continue;
        }
        queue.extend(child_directories);
        for compose in compose_files {
            storage_roots.extend(compose_storage(&directory, &compose));
        }
        let artifacts = project_artifacts(&directory, &names);
        if !artifacts.is_empty() {
            result.projects_found += 1;
        }
        for (name, path) in artifacts {
            if cancelled() {
                break;
            }
            if storage_roots.iter().any(|root| path.starts_with(root)) {
                result.storage_skipped += 1;
                continue;
            }
            if let Err(error) = collect_candidate(
                path,
                name,
                "可重新生成的项目产物；保留源码、锁文件、配置和 SDK；清理后需重新安装依赖或构建",
                cutoff,
                safety,
                seen,
                &mut result.targets,
            ) {
                result.warn(error);
            }
        }
        if last_report.elapsed() >= Duration::from_millis(150) {
            report(DevProgress {
                path: directory.to_string_lossy().into_owned(),
                directories_scanned: result.directories_scanned,
                projects_found: result.projects_found,
            });
            last_report = Instant::now();
        }
    }
    if deep > 0 {
        result.warn(format!(
            "有 {deep} 个目录超过 {} 层，未继续深入；可单独选择这些子目录扫描",
            limits.depth
        ));
    }
    report(DevProgress {
        path: "项目扫描完成".into(),
        directories_scanned: result.directories_scanned,
        projects_found: result.projects_found,
    });
    Ok(result)
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
    if storage_ancestor(path)
        || contains_protected(path, &protected_process_paths())
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
    fn excluded_artifact_children_preserve_whole_target_and_snapshot_cancels_inside_walk() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::env::temp_dir().join(format!("cleaner-dev-control-{}", std::process::id()));
        fs::create_dir_all(root.join("node_modules/nested")).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("node_modules/nested/dependency"), b"generated").unwrap();
        let report = scan_projects_controlled(
            &root,
            SystemTime::now() + Duration::from_secs(1),
            &ScanSafety {
                protected: &[],
                excluded: &[root.join("node_modules/nested")],
                cancelled: &|| false,
            },
            &mut HashSet::new(),
            ScanLimits::default(),
            &mut |_| {},
        )
        .unwrap();
        assert!(report.targets.is_empty());
        assert_eq!(report.projects_found, 1);
        let checks = AtomicUsize::new(0);
        assert!(
            aged_snapshot(
                &root.join("node_modules"),
                SystemTime::now() + Duration::from_secs(1),
                &|| checks.fetch_add(1, Ordering::SeqCst) >= 1
            )
            .unwrap_err()
            .contains("停止")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn project_recognition_keeps_locks_sources_and_unmarked_build_folders() {
        let root = std::env::temp_dir().join(format!("appsteward-dev-test-{}", std::process::id()));
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("package-lock.json"), "keep").unwrap();
        fs::write(root.join("app.ts"), "keep").unwrap();
        let names = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        let artifacts = project_artifacts(&root, &names);
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
        assert!(targets.targets.is_empty());
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
        assert_eq!(targets.targets.len(), 1);
        assert_eq!(targets.targets[0].path, fs::canonicalize(&old).unwrap());
        let hash = fingerprint(&old).unwrap();
        assert!(revalidate(&old, 7, &hash).is_ok());
        fs::write(old.join("new-file"), "changed after scan").unwrap();
        assert!(revalidate(&old, 7, &hash).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn source_file_count_does_not_consume_directory_budget_and_generated_trees_are_pruned() {
        let root =
            std::env::temp_dir().join(format!("appsteward-dir-budget-{}", std::process::id()));
        fs::create_dir_all(root.join("project/node_modules/deep/nested")).unwrap();
        fs::create_dir_all(root.join("sdk/nested")).unwrap();
        fs::write(root.join("project/package.json"), "{}").unwrap();
        fs::write(root.join("project/node_modules/cache"), "generated").unwrap();
        fs::write(root.join("sdk/nested/Cargo.toml"), "SDK component").unwrap();
        for index in 0..300 {
            fs::write(root.join(format!("source-{index}.ts")), "source").unwrap();
        }
        let mut events = Vec::new();
        let report = scan_projects_with_limits(
            &root,
            SystemTime::now() + Duration::from_secs(1),
            &[],
            &mut HashSet::new(),
            ScanLimits {
                directories: 2,
                depth: 16,
                duration: Duration::from_secs(10),
            },
            &mut |progress| events.push(progress),
        )
        .unwrap();
        assert!(report.warnings.is_empty());
        assert_eq!(report.directories_scanned, 2);
        assert_eq!(report.projects_found, 1);
        assert_eq!(report.targets.len(), 1);
        assert_eq!(report.targets[0].size_bytes, 9);
        assert_eq!(
            report.targets[0].fingerprint,
            fingerprint(&report.targets[0].path).unwrap()
        );
        assert_eq!(events.last().unwrap().directories_scanned, 2);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn discovery_limits_return_completed_targets_and_explain_partial_coverage() {
        let root =
            std::env::temp_dir().join(format!("appsteward-partial-dev-{}", std::process::id()));
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join("subproject")).unwrap();
        fs::write(root.join("Cargo.toml"), "project marker").unwrap();
        fs::write(root.join("target/artifact"), "generated").unwrap();
        let report = scan_projects_with_limits(
            &root,
            SystemTime::now() + Duration::from_secs(1),
            &[],
            &mut HashSet::new(),
            ScanLimits {
                directories: 1,
                depth: 16,
                duration: Duration::from_secs(10),
            },
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(report.targets.len(), 1);
        assert_eq!(report.directories_scanned, 1);
        assert!(report.warnings[0].contains("已保留当前结果"));
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
    #[test]
    fn compose_runtime_storage_is_skipped_but_source_projects_remain() {
        let root =
            std::env::temp_dir().join(format!("appsteward-dev-storage-{}", std::process::id()));
        let service = root.join("service");
        for path in ["data", "data_pg15", "src"] {
            fs::create_dir_all(service.join(path)).unwrap();
            fs::write(service.join(path).join("Cargo.toml"), "[package]").unwrap();
            fs::create_dir_all(service.join(path).join("target")).unwrap();
        }
        fs::write(service.join("docker-compose.yml"), "services:\n  db:\n    volumes:\n      - ./data:/var/lib/postgresql/data\n      - ./src:/workspace\n").unwrap();
        let mut seen = HashSet::new();
        let report = scan_projects_with_limits(
            &root,
            SystemTime::now() + Duration::from_secs(1),
            &[],
            &mut seen,
            ScanLimits::default(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(report.projects_found, 1);
        assert_eq!(report.storage_skipped, 2);
        assert!(
            report
                .targets
                .iter()
                .all(|item| item.path.starts_with(service.join("src")))
        );
        assert!(storage_ancestor(&service.join("data")));
        fs::remove_dir_all(root).unwrap();
    }
}
