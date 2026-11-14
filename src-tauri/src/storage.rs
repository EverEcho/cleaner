use crate::{
    models::{ActionReport, format_size},
    platform,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    #[serde(skip)]
    identity: FileIdentity,
}
#[derive(Clone, PartialEq, Eq)]
struct FileIdentity {
    is_dir: bool,
    len: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}
impl FileIdentity {
    fn from_metadata(meta: &fs::Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            is_dir: meta.is_dir(),
            // Directory contents are checked against their cached children instead.
            len: if meta.is_dir() { 0 } else { meta.len() },
            modified: if meta.is_dir() {
                None
            } else {
                meta.modified().ok()
            },
            #[cfg(unix)]
            device: meta.dev(),
            #[cfg(unix)]
            inode: meta.ino(),
        }
    }
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
    identity: FileIdentity,
}
impl CachedEntry {
    fn new(path: &Path, bytes: u64, identity: FileIdentity) -> Self {
        Self {
            name: path.file_name().unwrap_or_default().to_owned(),
            bytes,
            identity,
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
                .map(|entry| usage(&root.join(&entry.name), entry.bytes, entry.identity.clone()))
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
impl AnalysisCache {
    fn entry(&self, path: &Path) -> Option<&CachedEntry> {
        self.pages
            .get(path.parent()?)?
            .children
            .iter()
            .find(|entry| Some(entry.name.as_os_str()) == path.file_name())
    }
    fn validate_target(&self, path: &Path) -> Result<(), String> {
        // The analysis root has no parent entry and can never be deleted from its own view.
        let expected = self
            .entry(path)
            .ok_or("项目不在本次分析结果中，请重新分析")?;
        if platform::home().is_some_and(|home| home.starts_with(path)) {
            return Err("不能移除用户主目录或其祖先目录".into());
        }
        let mut pending = vec![(path.to_path_buf(), &expected.identity)];
        while let Some((path, identity)) = pending.pop() {
            let meta = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
            if meta.file_type().is_symlink()
                || (!meta.is_file() && !meta.is_dir())
                || fs::canonicalize(&path).map_err(|error| error.to_string())? != path
                || &FileIdentity::from_metadata(&meta) != identity
            {
                return Err("文件或目录已改变，请重新分析后再处理".into());
            }
            if meta.is_dir() {
                let page = self.pages.get(&path).ok_or("目录未完整分析，请重新分析")?;
                if page.cancelled || !page.warning.is_empty() {
                    return Err("目录包含未完成的分析结果，请重新分析后再处理".into());
                }
                let children = fs::read_dir(&path)
                    .map_err(|error| error.to_string())?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?;
                // Also catches newly added files, excluded children, links and mount points.
                if children.len() != page.children.len() {
                    return Err("目录内容已改变或含未分析的项目，请重新分析后再处理".into());
                }
                let names: HashSet<_> = children
                    .into_iter()
                    .map(|entry| entry.file_name())
                    .collect();
                for child in &page.children {
                    if !names.contains(&child.name) {
                        return Err("目录内容已改变，请重新分析后再处理".into());
                    }
                    pending.push((path.join(&child.name), &child.identity));
                }
            }
        }
        Ok(())
    }
    fn remove_target(&mut self, path: &Path) {
        let Some(bytes) = self.entry(path).map(|entry| entry.bytes) else {
            return;
        };
        self.pages.retain(|root, _| !root.starts_with(path));
        if let Some(parent) = path.parent().and_then(|parent| self.pages.get_mut(parent)) {
            parent
                .children
                .retain(|entry| Some(entry.name.as_os_str()) != path.file_name());
        }
        let mut child = path.parent();
        while let Some(path) = child {
            if let Some(page) = path.parent().and_then(|parent| self.pages.get_mut(parent))
                && let Some(entry) = page
                    .children
                    .iter_mut()
                    .find(|entry| Some(entry.name.as_os_str()) == path.file_name())
            {
                entry.bytes = entry.bytes.saturating_sub(bytes);
            }
            child = path.parent();
        }
    }
}
static CACHE: LazyLock<Mutex<Option<AnalysisCache>>> = LazyLock::new(|| Mutex::new(None));

pub fn trash_analyzed(
    paths: Vec<String>,
    excluded: &[String],
    progress: impl FnMut(&str, u64, u64),
) -> Result<ActionReport, String> {
    trash_with_cache(paths, excluded, progress, &CACHE, |path| {
        platform::move_to_trash(path).map(|_| ())
    })
}
fn trash_with_cache(
    paths: Vec<String>,
    excluded: &[String],
    mut progress: impl FnMut(&str, u64, u64),
    cache: &Mutex<Option<AnalysisCache>>,
    mut trash: impl FnMut(&Path) -> Result<(), String>,
) -> Result<ActionReport, String> {
    if paths.len() != 1 {
        return Err("请一次选择一个文件或目录".into());
    }
    let excluded = excluded
        .iter()
        .map(|path| normalized_absolute(Path::new(path)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut stored = cache.lock().map_err(|error| error.to_string())?;
    let snapshot = stored.as_mut().ok_or("尚无分析结果，请先分析目录")?;
    let value = &paths[0];
    let path = Path::new(value);
    let mut report = ActionReport::default();
    progress(value, 0, 1);
    if excluded
        .iter()
        .chain(&snapshot.excluded)
        .any(|keep| path.starts_with(keep) || keep.starts_with(path))
    {
        report.push(
            value.clone(),
            "skipped",
            "保留名单命中此项目或其子目录，已保留",
        );
    } else {
        match snapshot.validate_target(path).and_then(|()| trash(path)) {
            Ok(()) => {
                snapshot.remove_target(path);
                report.push(value.clone(), "success", "已移入废纸篓");
            }
            Err(error) => report.push(value.clone(), "failed", error),
        }
    }
    progress(value, 1, 1);
    Ok(report)
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
    platform::normalized_absolute(path)
}

type Cancel<'a> = &'a (dyn Fn() -> bool + Sync);
fn warning(warnings: &mut Vec<String>, message: String) {
    if warnings.len() < 100 {
        warnings.push(message);
    }
}
fn usage(path: &Path, bytes: u64, identity: FileIdentity) -> DirectoryUsage {
    DirectoryUsage {
        path: path.to_string_lossy().into_owned(),
        size_bytes: bytes,
        size: format_size(bytes),
        is_dir: identity.is_dir,
        identity,
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
    identity: FileIdentity,
}
impl Frame {
    fn new(path: PathBuf, warnings_start: usize, identity: FileIdentity) -> std::io::Result<Self> {
        let entries = fs::read_dir(&path)?;
        Ok(Self {
            path,
            entries,
            bytes: 0,
            children: Vec::new(),
            warnings_start,
            identity,
        })
    }
    fn add(&mut self, path: &Path, bytes: u64, identity: FileIdentity) {
        self.bytes = self.bytes.saturating_add(bytes);
        self.children.push(CachedEntry::new(path, bytes, identity));
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
    identity: FileIdentity,
    root_meta: &fs::Metadata,
    excluded: &[PathBuf],
    cancelled: Cancel<'_>,
    files: &AtomicU64,
    sender: &mpsc::SyncSender<Event>,
) {
    let mut warnings = Vec::new();
    let first = match Frame::new(path.clone(), 0, identity.clone()) {
        Ok(frame) => frame,
        Err(error) => {
            let _ = sender.send(Event::Done(
                usage(&path, 0, identity),
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
                let identity = FileIdentity::from_metadata(&meta);
                match Frame::new(child.clone(), warnings.len(), identity.clone()) {
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
                        stack.last_mut().unwrap().add(&child, 0, identity);
                    }
                }
            } else if meta.is_file() {
                frame.add(&child, meta.len(), FileIdentity::from_metadata(&meta));
                let count = files.fetch_add(1, Ordering::Relaxed) + 1;
                if count.is_multiple_of(256) {
                    let _ =
                        sender.send(Event::Progress(child.to_string_lossy().into_owned(), count));
                }
            }
            continue;
        }
        let frame = stack.pop().unwrap();
        let item = usage(&frame.path, frame.bytes, frame.identity);
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
            parent.add(Path::new(&item.path), item.size_bytes, item.identity);
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
            jobs.push_back((path, FileIdentity::from_metadata(&meta)));
        } else if meta.is_file() {
            directories.push(usage(&path, meta.len(), FileIdentity::from_metadata(&meta)));
            let count = files.fetch_add(1, Ordering::Relaxed) + 1;
            if count.is_multiple_of(256) {
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
                    let Some((path, identity)) = job else {
                        break;
                    };
                    scan_branch(
                        path, identity, root_meta, excluded, cancelled, files, &sender,
                    );
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
                .map(|item| {
                    CachedEntry::new(
                        Path::new(&item.path),
                        item.size_bytes,
                        item.identity.clone(),
                    )
                })
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
    fn trash_updates_nested_and_ancestor_snapshots_without_rescanning() {
        let fixture = Fixture::new("trash-snapshot");
        fs::create_dir_all(fixture.0.join("scan/nested/deep")).unwrap();
        fs::write(fixture.0.join("scan/nested/deep/file"), b"1234").unwrap();
        fs::write(fixture.0.join("scan/nested/keep"), b"12").unwrap();
        let root = fixture.0.join("scan");
        let cache = Mutex::new(None);
        analyze_with_cache(
            root.to_str().unwrap(),
            &[],
            true,
            &|| false,
            |_, _, _| {},
            &cache,
        )
        .unwrap();
        let deep = root.join("nested/deep");
        let result = trash_with_cache(
            vec![deep.to_str().unwrap().into()],
            &[],
            |_, _, _| {},
            &cache,
            |path| fs::rename(path, fixture.0.join("trashed")).map_err(|error| error.to_string()),
        )
        .unwrap();
        assert_eq!(result.succeeded, 1);
        let stored = cache.lock().unwrap();
        let snapshot = stored.as_ref().unwrap();
        assert!(!snapshot.pages.contains_key(&deep));
        assert_eq!(
            snapshot.pages[&root].analysis(&root).directories[0].size_bytes,
            2
        );
        assert_eq!(snapshot.pages[&root.join("nested")].children.len(), 1);
        assert!(snapshot.validate_target(&deep).is_err());
        assert!(
            snapshot.validate_target(&root.join("nested")).is_ok(),
            "known deletions allow further cleanup without rescanning"
        );
        drop(stored);
        let keep = root.join("nested/keep");
        let failed = trash_with_cache(
            vec![keep.to_str().unwrap().into()],
            &[],
            |_, _, _| {},
            &cache,
            |_| Err("fixture trash failure".into()),
        )
        .unwrap();
        assert_eq!(failed.failed, 1);
        assert!(
            cache
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .entry(&keep)
                .is_some()
        );
    }

    #[test]
    fn trash_rejects_changes_links_unscanned_paths_and_protected_descendants() {
        let fixture = Fixture::new("trash-guards");
        let root = fixture.0.join("scan");
        fs::create_dir_all(root.join("nested/deep")).unwrap();
        let file = root.join("nested/deep/file");
        fs::write(&file, b"1234").unwrap();
        let cache = Mutex::new(None);
        let scan = || {
            analyze_with_cache(
                root.to_str().unwrap(),
                &[],
                true,
                &|| false,
                |_, _, _| {},
                &cache,
            )
            .unwrap()
        };
        scan();
        let remove = |path: &Path, excluded: &[String]| {
            trash_with_cache(
                vec![path.to_str().unwrap().into()],
                excluded,
                |_, _, _| {},
                &cache,
                |_| panic!("rejected paths must never reach trash"),
            )
            .unwrap()
        };
        assert_eq!(remove(&root, &[]).failed, 1, "analysis root is protected");
        let outside = fixture.0.join("outside");
        fs::write(&outside, b"outside").unwrap();
        assert_eq!(remove(&outside, &[]).failed, 1);
        assert_eq!(
            remove(&root.join("nested"), &[file.to_str().unwrap().into()]).skipped,
            1
        );
        fs::write(&file, b"changed").unwrap();
        assert_eq!(remove(&file, &[]).failed, 1);
        assert_eq!(
            remove(&root.join("nested"), &[]).failed,
            1,
            "nested modifications are rechecked"
        );
        scan();
        fs::write(root.join("nested/deep/new"), b"new").unwrap();
        assert_eq!(
            remove(&root.join("nested"), &[]).failed,
            1,
            "new descendants must be analyzed first"
        );
        scan();
        #[cfg(unix)]
        {
            fs::remove_file(&file).unwrap();
            std::os::unix::fs::symlink(&outside, &file).unwrap();
            assert_eq!(remove(&file, &[]).failed, 1);
            assert_eq!(remove(&root.join("nested"), &[]).failed, 1);
        }
        scan();
        assert_eq!(
            remove(&root.join("nested"), &[]).failed,
            1,
            "unscanned links inside a directory stay protected"
        );
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
