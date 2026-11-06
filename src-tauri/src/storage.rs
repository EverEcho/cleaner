use crate::{models::format_size, platform};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

#[derive(Clone, Serialize)]
pub struct DirectoryUsage {
    pub path: String,
    pub size_bytes: u64,
    pub size: String,
    pub is_dir: bool,
}
#[derive(Clone, Serialize)]
pub struct DiskAnalysis {
    pub root: String,
    pub directories: Vec<DirectoryUsage>,
    pub warning: String,
    pub cancelled: bool,
    pub cached: bool,
}
// Store each name once, without repeating absolute paths or formatted sizes for every file.
struct CachedEntry {
    name: OsString,
    bytes: u64,
    is_dir: bool,
}
impl CachedEntry {
    fn new(path: &Path, bytes: u64, is_dir: bool) -> Self {
        Self {
            name: path.file_name().unwrap_or_default().to_owned(),
            bytes,
            is_dir,
        }
    }
}
struct CachedPage {
    children: Vec<CachedEntry>,
    warning: String,
    cancelled: bool,
}
impl CachedPage {
    fn analysis(&self, root: &Path) -> DiskAnalysis {
        let mut result = report(
            root,
            self.children
                .iter()
                .map(|entry| usage(&root.join(&entry.name), entry.bytes, entry.is_dir))
                .collect(),
            &[],
            self.cancelled,
        );
        result.warning = self.warning.clone();
        result.cached = true;
        result
    }
}
struct AnalysisCache {
    excluded: Vec<PathBuf>,
    pages: HashMap<PathBuf, CachedPage>,
}
static CACHE: LazyLock<Mutex<Option<AnalysisCache>>> = LazyLock::new(|| Mutex::new(None));

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
    platform::normalized_absolute(path)
}

type Cancel<'a> = &'a (dyn Fn() -> bool + Sync);
fn warning(warnings: &mut Vec<String>, message: String) {
    if warnings.len() < 100 {
        warnings.push(message);
    }
}
fn usage(path: &Path, bytes: u64, is_dir: bool) -> DirectoryUsage {
    DirectoryUsage {
        path: path.to_string_lossy().into_owned(),
        size_bytes: bytes,
        size: format_size(bytes),
        is_dir,
    }
}
fn report(
    root: &Path,
    mut directories: Vec<DirectoryUsage>,
    warnings: &[String],
    cancelled: bool,
) -> DiskAnalysis {
    directories.sort_by_key(|entry| std::cmp::Reverse(entry.size_bytes));
    DiskAnalysis {
        root: root.to_string_lossy().into_owned(),
        directories,
        warning: warnings.join("\n"),
        cancelled,
        cached: false,
    }
}
struct Frame {
    path: PathBuf,
    entries: fs::ReadDir,
    bytes: u64,
    children: Vec<CachedEntry>,
    warnings_start: usize,
}
impl Frame {
    fn new(path: PathBuf, warnings_start: usize) -> std::io::Result<Self> {
        let entries = fs::read_dir(&path)?;
        Ok(Self {
            path,
            entries,
            bytes: 0,
            children: Vec::new(),
            warnings_start,
        })
    }
    fn add(&mut self, path: &Path, bytes: u64, is_dir: bool) {
        self.bytes = self.bytes.saturating_add(bytes);
        self.children.push(CachedEntry::new(path, bytes, is_dir));
    }
}
enum Event {
    Progress(String, u64),
    Done(DirectoryUsage, HashMap<PathBuf, CachedPage>, Vec<String>),
    Failed(Vec<String>),
}
// One metadata read per entry. No file contents or dependency manifests are read.
fn scan_branch(
    path: PathBuf,
    root_meta: &fs::Metadata,
    excluded: &[PathBuf],
    cancelled: Cancel<'_>,
    files: &AtomicU64,
    sender: &mpsc::SyncSender<Event>,
) {
    let mut warnings = Vec::new();
    let first = match Frame::new(path.clone(), 0) {
        Ok(frame) => frame,
        Err(error) => {
            let _ = sender.send(Event::Done(
                usage(&path, 0, true),
                HashMap::from([(
                    path.clone(),
                    CachedPage {
                        children: Vec::new(),
                        warning: format!("{}: {error}", path.display()),
                        cancelled: false,
                    },
                )]),
                vec![format!("{}: {error}", path.display())],
            ));
            return;
        }
    };
    let mut stack = vec![first];
    let mut pages = HashMap::new();
    loop {
        if cancelled() {
            let _ = sender.send(Event::Failed(warnings));
            return;
        }
        let frame = stack.last_mut().unwrap();
        if let Some(entry) = frame.entries.next() {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warning(&mut warnings, error.to_string());
                    continue;
                }
            };
            let child = entry.path();
            if excluded.iter().any(|excluded| child.starts_with(excluded)) {
                continue;
            }
            let meta = match fs::symlink_metadata(&child) {
                Ok(meta) => meta,
                Err(error) => {
                    warning(&mut warnings, format!("{}: {error}", child.display()));
                    continue;
                }
            };
            if meta.file_type().is_symlink() || !same_device(root_meta, &meta) {
                continue;
            }
            if meta.is_dir() {
                match Frame::new(child.clone(), warnings.len()) {
                    Ok(frame) => stack.push(frame),
                    Err(error) => {
                        warning(&mut warnings, format!("{}: {error}", child.display()));
                        pages.insert(
                            child.clone(),
                            CachedPage {
                                children: Vec::new(),
                                warning: format!("{}: {error}", child.display()),
                                cancelled: false,
                            },
                        );
                        stack.last_mut().unwrap().add(&child, 0, true);
                    }
                }
            } else if meta.is_file() {
                frame.add(&child, meta.len(), false);
                let count = files.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 256 == 0 {
                    let _ =
                        sender.send(Event::Progress(child.to_string_lossy().into_owned(), count));
                }
            }
            continue;
        }
        let frame = stack.pop().unwrap();
        let item = usage(&frame.path, frame.bytes, true);
        let mut notes = warnings[frame.warnings_start..].to_vec();
        if warnings.len() == 100 {
            notes.push("部分提示已省略，请查看分析根目录的结果提示".into());
        }
        pages.insert(
            frame.path,
            CachedPage {
                children: frame.children,
                warning: notes.join("\n"),
                cancelled: false,
            },
        );
        if let Some(parent) = stack.last_mut() {
            parent.add(Path::new(&item.path), item.size_bytes, true);
        } else {
            let _ = sender.send(Event::Done(item, pages, warnings));
            return;
        }
    }
}

pub fn analyze(root: &str) -> Result<DiskAnalysis, String> {
    analyze_controlled(root, &[], &|| false, |_, _, _| {})
}
pub fn analyze_controlled(
    root: &str,
    excluded: &[String],
    cancelled: Cancel<'_>,
    progress: impl FnMut(&str, u64, u64),
) -> Result<DiskAnalysis, String> {
    analyze_with_cache(root, excluded, true, cancelled, progress, &Mutex::new(None))
}
pub fn analyze_cached(
    root: &str,
    excluded: &[String],
    refresh: bool,
    cancelled: Cancel<'_>,
    progress: impl FnMut(&str, u64, u64),
) -> Result<DiskAnalysis, String> {
    analyze_with_cache(root, excluded, refresh, cancelled, progress, &CACHE)
}
fn analyze_with_cache(
    root: &str,
    excluded: &[String],
    refresh: bool,
    cancelled: Cancel<'_>,
    mut progress: impl FnMut(&str, u64, u64),
    cache: &Mutex<Option<AnalysisCache>>,
) -> Result<DiskAnalysis, String> {
    let root = Path::new(root);
    if !root.is_absolute() {
        return Err("请选择绝对目录路径".into());
    }
    let mut excluded = excluded
        .iter()
        .map(|path| normalized_absolute(Path::new(path)))
        .collect::<Result<Vec<_>, _>>()?;
    excluded.sort();
    excluded.dedup();
    if !refresh {
        let stored = cache.lock().map_err(|error| error.to_string())?;
        let snapshot = stored.as_ref().ok_or("尚无分析结果，请先点击分析目录")?;
        if snapshot.excluded != excluded {
            return Err("分析范围已更改，请点击重新分析以更新结果".into());
        }
        // Navigation never falls back to a walk, even for deep, missing or unreadable paths.
        if let Some(page) = snapshot.pages.get(root) {
            return Ok(page.analysis(root));
        }
        return Err("该目录不在本次分析结果中，请选择目录后手动分析".into());
    }
    if !root.is_dir() {
        return Err("请选择存在的绝对目录路径".into());
    }
    let root = fs::canonicalize(root).map_err(|error| format!("无法核对分析目录: {error}"))?;
    if excluded.iter().any(|excluded| root.starts_with(excluded)) {
        let result = report(
            &root,
            Vec::new(),
            &["当前目录在保留名单内，已跳过分析".into()],
            false,
        );
        if let Ok(mut stored) = cache.lock() {
            *stored = Some(AnalysisCache {
                excluded,
                pages: HashMap::from([(
                    root,
                    CachedPage {
                        children: Vec::new(),
                        warning: result.warning.clone(),
                        cancelled: false,
                    },
                )]),
            });
        }
        return Ok(result);
    }
    let root_meta = root.metadata().map_err(|error| error.to_string())?;
    let mut warnings = Vec::new();
    let mut directories = Vec::new();
    let mut jobs = VecDeque::new();
    let files = AtomicU64::new(0);
    for child in fs::read_dir(&root).map_err(|error| error.to_string())? {
        if cancelled() {
            break;
        }
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                warning(&mut warnings, error.to_string());
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
                warning(&mut warnings, format!("{}: {error}", path.display()));
                continue;
            }
        };
        if meta.file_type().is_symlink() || !same_device(&root_meta, &meta) {
            continue;
        }
        if meta.is_dir() {
            jobs.push_back(path);
        } else if meta.is_file() {
            directories.push(usage(&path, meta.len(), false));
            let count = files.fetch_add(1, Ordering::Relaxed) + 1;
            if count % 256 == 0 {
                progress(&path.to_string_lossy(), count, 0);
            }
        }
    }
    let worker_count = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4)
        .min(jobs.len());
    let jobs = Mutex::new(jobs);
    let mut pages = HashMap::new();
    std::thread::scope(|scope| {
        let (sender, receiver) = mpsc::sync_channel(32);
        for _ in 0..worker_count {
            let sender = sender.clone();
            let (jobs, root_meta, excluded, files) = (&jobs, &root_meta, &excluded, &files);
            scope.spawn(move || {
                while !cancelled() {
                    let job = jobs.lock().unwrap().pop_front();
                    let Some(path) = job else {
                        break;
                    };
                    scan_branch(path, root_meta, excluded, cancelled, files, &sender);
                }
            });
        }
        drop(sender);
        let mut reported = files.load(Ordering::Relaxed);
        for event in receiver {
            match event {
                Event::Progress(path, count) => {
                    reported = reported.max(count);
                    progress(&path, reported, 0);
                }
                Event::Done(item, children, notes) => {
                    reported = reported.max(files.load(Ordering::Relaxed));
                    progress(&item.path, reported, 0);
                    directories.push(item);
                    pages.extend(children);
                    for note in notes {
                        warning(&mut warnings, note);
                    }
                }
                Event::Failed(notes) => {
                    for note in notes {
                        warning(&mut warnings, note);
                    }
                }
            }
        }
    });
    let stopped = cancelled();
    if stopped {
        warnings.push("分析已停止，当前仅显示已完整统计的项目".into());
    }
    progress(&root.to_string_lossy(), files.load(Ordering::Relaxed), 0);
    let result = report(&root, directories, &warnings, stopped);
    pages.insert(
        root,
        CachedPage {
            children: result
                .directories
                .iter()
                .map(|item| CachedEntry::new(Path::new(&item.path), item.size_bytes, item.is_dir))
                .collect(),
            warning: result.warning.clone(),
            cancelled: stopped,
        },
    );
    if let Ok(mut stored) = cache.lock() {
        *stored = Some(AnalysisCache { excluded, pages });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("cleaner-storage-{name}-{}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(fs::canonicalize(root).unwrap())
        }
        fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn navigation_uses_a_stable_snapshot_until_explicit_refresh() {
        let fixture = Fixture::new("snapshot");
        fs::create_dir_all(fixture.0.join("nested/deep")).unwrap();
        fs::write(fixture.0.join("nested/deep/file"), b"1234").unwrap();
        let cache = Mutex::new(None);
        let run = |path: &Path, excluded: &[String], refresh| {
            analyze_with_cache(
                path.to_str().unwrap(),
                excluded,
                refresh,
                &|| false,
                |_, _, _| panic!("navigation must not report scan progress"),
                &cache,
            )
        };
        assert!(run(&fixture.0, &[], false).is_err());
        let root =
            analyze_with_cache(fixture.path(), &[], true, &|| false, |_, _, _| {}, &cache).unwrap();
        assert!(!root.cached);
        assert_eq!(root.root, fixture.path());
        assert_eq!(root.directories[0].size_bytes, 4);
        fs::write(fixture.0.join("nested/deep/file"), b"12345678").unwrap();
        let nested = run(&fixture.0.join("nested"), &[], false).unwrap();
        assert!(nested.cached);
        assert_eq!(nested.directories[0].size_bytes, 4);
        assert!(run(&fixture.0.join("unscanned"), &[], false).is_err());
        let excluded = [fixture.0.join("nested/deep").to_string_lossy().into_owned()];
        assert!(run(&fixture.0, &excluded, false).is_err());
        let fresh =
            analyze_with_cache(fixture.path(), &[], true, &|| false, |_, _, _| {}, &cache).unwrap();
        assert_eq!(fresh.directories[0].size_bytes, 8);
        fs::remove_dir_all(fixture.0.join("nested")).unwrap();
        assert_eq!(
            run(&fixture.0.join("nested/deep"), &[], false)
                .unwrap()
                .directories[0]
                .size_bytes,
            8
        );
        assert_eq!(
            run(&fixture.0, &[], false).unwrap().directories[0].size_bytes,
            8
        );
        let empty =
            analyze_with_cache(fixture.path(), &[], true, &|| false, |_, _, _| {}, &cache).unwrap();
        assert!(empty.directories.is_empty());
        assert!(run(&fixture.0.join("nested"), &[], false).is_err());
    }

    #[test]
    fn snapshot_has_no_depth_or_twenty_thousand_entry_cutoff() {
        let fixture = Fixture::new("deep-wide");
        let deep = fixture.0.join("a/b/c/d/e/f/g/h");
        fs::create_dir_all(&deep).unwrap();
        for index in 0..20_010 {
            fs::write(deep.join(index.to_string()), b"x").unwrap();
        }
        let cache = Mutex::new(None);
        analyze_with_cache(fixture.path(), &[], true, &|| false, |_, _, _| {}, &cache).unwrap();
        fs::remove_dir_all(fixture.0.join("a")).unwrap();
        for path in [fixture.0.join("a"), fixture.0.join("a/b/c/d/e"), deep] {
            let result = analyze_with_cache(
                path.to_str().unwrap(),
                &[],
                false,
                &|| false,
                |_, _, _| panic!("must not rescan"),
                &cache,
            )
            .unwrap();
            assert!(result.cached);
            assert_eq!(
                result
                    .directories
                    .iter()
                    .map(|item| item.size_bytes)
                    .sum::<u64>(),
                20_010
            );
        }
    }

    #[test]
    fn parallel_totals_are_exact_and_progress_never_moves_backwards() {
        let fixture = Fixture::new("parallel");
        for branch in 0..8 {
            let path = fixture.0.join(format!("branch-{branch}/deep"));
            fs::create_dir_all(&path).unwrap();
            for index in 0..300 {
                fs::write(path.join(index.to_string()), b"1234").unwrap();
            }
        }
        let mut counts = Vec::new();
        let started = Instant::now();
        let result = analyze_controlled(fixture.path(), &[], &|| false, |_, count, _| {
            counts.push(count)
        })
        .unwrap();
        eprintln!("2400-file parallel analysis: {:?}", started.elapsed());
        assert_eq!(result.directories.len(), 8);
        assert_eq!(
            result
                .directories
                .iter()
                .map(|item| item.size_bytes)
                .sum::<u64>(),
            9600
        );
        assert!(counts.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(counts.last(), Some(&2400));
        assert!(!result.cancelled);
    }

    #[test]
    fn cancellation_keeps_only_completed_branches_browsable() {
        use std::sync::atomic::AtomicUsize;
        let fixture = Fixture::new("cancel");
        fs::create_dir_all(fixture.0.join("branch")).unwrap();
        for index in 0..1024 {
            fs::write(fixture.0.join(format!("branch/{index}")), b"1234").unwrap();
        }
        let checks = AtomicUsize::new(0);
        let cache = Mutex::new(None);
        let result = analyze_with_cache(
            fixture.path(),
            &[],
            true,
            &|| checks.fetch_add(1, Ordering::Relaxed) > 100,
            |_, _, _| {},
            &cache,
        )
        .unwrap();
        assert!(result.cancelled);
        assert!(result.warning.contains("已停止"));
        let snapshot = cache.lock().unwrap();
        let snapshot = snapshot.as_ref().unwrap();
        assert!(snapshot.pages.contains_key(&fixture.0));
        assert!(!snapshot.pages.contains_key(&fixture.0.join("branch")));
    }
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
                fs::canonicalize(actual.join("leaf"))
                    .unwrap()
                    .to_string_lossy()
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
            fs::canonicalize(&actual).unwrap().join("missing")
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
