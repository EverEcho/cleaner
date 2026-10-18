use crate::{
    models::{
        ActionReport, CleanCategory, CleanItem, DuplicateGroup, FileScanReport, LargeFileItem,
        ScanOptions, format_size,
    },
    platform,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use walkdir::WalkDir;

// Only paths returned by the latest junk scan can be submitted for permanent cleanup.
#[derive(Default)]
struct JunkReceipt {
    paths: HashSet<PathBuf>,
    actions: HashMap<String, String>,
    action_paths: HashMap<String, Vec<PathBuf>>,
    developer: HashMap<PathBuf, (u64, String)>,
    developer_groups: HashMap<String, Vec<crate::models::CleanFileDetail>>,
}
static SCANNED_JUNK: LazyLock<Mutex<JunkReceipt>> =
    LazyLock::new(|| Mutex::new(JunkReceipt::default()));
static LARGE_FILES: LazyLock<Mutex<HashMap<PathBuf, (u64, std::time::SystemTime)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DUPLICATES: LazyLock<Mutex<Vec<DuplicateGroup>>> = LazyLock::new(|| Mutex::new(Vec::new()));

pub fn cache_root() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        platform::home().map(|h| h.join("Library/Caches"))
    } else {
        platform::xdg_dir("XDG_CACHE_HOME", ".cache")
    }
}

pub fn trash_root() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        platform::home().map(|h| h.join(".Trash"))
    } else {
        platform::xdg_dir("XDG_DATA_HOME", ".local/share").map(|p| p.join("Trash/files"))
    }
}

fn browser_names() -> &'static [(&'static str, &'static str)] {
    if cfg!(target_os = "macos") {
        &[
            ("Google Chrome", "Google/Chrome"),
            ("Firefox", "Firefox"),
            ("Microsoft Edge", "Microsoft Edge"),
            ("Safari", "com.apple.Safari"),
        ]
    } else {
        &[
            ("Google Chrome", "google-chrome"),
            ("Firefox", "mozilla"),
            ("Microsoft Edge", "microsoft-edge"),
            ("Chromium", "chromium"),
        ]
    }
}

fn item_controlled(
    path: &Path,
    reason: &str,
    selected: bool,
    cancelled: &dyn Fn() -> bool,
    report: &mut impl FnMut(&str, u64),
) -> Option<CleanItem> {
    let result = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => return None,
        Ok(_) => get_size_controlled(path, cancelled, report),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => Err(format!("{}: {error}", path.display())),
    };
    let (size, error) = match result {
        Ok(size) => (size, None),
        Err(error) => (0, Some(error)),
    };
    if size == 0 && error.is_none() {
        return None;
    }
    Some(CleanItem {
        id: path.to_string_lossy().into_owned(),
        path: path.to_string_lossy().into_owned(),
        name: path.file_name()?.to_string_lossy().into_owned(),
        size_bytes: size,
        size_str: if error.is_some() {
            "扫描失败".into()
        } else {
            format_size(size)
        },
        reason: if let Some(ref error) = error {
            format!("{reason}；扫描失败：{error}")
        } else {
            reason.into()
        },
        can_delete: error.is_none(),
        selected: selected && error.is_none(),
        size_known: error.is_none(),
        action: None,
        details: error,
        files: Vec::new(),
    })
}

fn normalize_excluded(paths: &[String]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter_map(|path| {
            let value = PathBuf::from(path);
            value
                .is_absolute()
                .then(|| fs::canonicalize(&value).unwrap_or(value))
        })
        .collect()
}
fn excluded_target(path: &Path, excluded: &[PathBuf]) -> bool {
    // Cleanup deletes the entire target; excluding a child must preserve its parent too.
    excluded
        .iter()
        .any(|root| path.starts_with(root) || root.starts_with(path))
}
fn children_controlled(
    root: &Path,
    reason: &str,
    selected: bool,
    excluded: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
    warnings: &mut Vec<String>,
    report: &mut impl FnMut(&str, u64),
) -> Vec<CleanItem> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            warnings.push(format!("无法读取 {}: {error}", root.display()));
            return Vec::new();
        }
    };
    let mut result = Vec::new();
    for entry in entries {
        if cancelled() {
            warnings.push("扫描已停止，当前结果仅覆盖已读取目录".into());
            break;
        }
        match entry {
            Ok(entry) => {
                if !excluded_target(&entry.path(), excluded)
                    && let Some(value) =
                        item_controlled(&entry.path(), reason, selected, cancelled, report)
                {
                    result.push(value);
                }
            }
            Err(error) => warnings.push(format!("{}: {error}", root.display())),
        }
    }
    result
}

fn category(id: &str, title: &str, desc: &str, mut items: Vec<CleanItem>) -> CleanCategory {
    items.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    let total_size_bytes = items.iter().map(|i| i.size_bytes).sum();
    let unknown = items.iter().any(|i| !i.size_known);
    CleanCategory {
        warning: None,
        scan_summary: None,
        id: id.into(),
        title: title.into(),
        desc: desc.into(),
        icon: id.into(),
        total_size_bytes,
        total_size_str: if !items.is_empty() && items.iter().all(|i| !i.can_delete) {
            "只读检查".into()
        } else if unknown && total_size_bytes == 0 {
            "扫描未完成".into()
        } else if unknown {
            format!("{}（已统计）", format_size(total_size_bytes))
        } else {
            format_size(total_size_bytes)
        },
        count: items.len(),
        items,
    }
}

pub fn scan_system_junk() -> Vec<CleanCategory> {
    scan_system_junk_with_progress(|_, _| {})
}

pub fn scan_system_junk_with_progress(mut report: impl FnMut(&str, u64)) -> Vec<CleanCategory> {
    scan_system_junk_controlled(&[], &|| false, |path, bytes, _| report(path, bytes))
}
pub fn scan_system_junk_controlled(
    excluded: &[String],
    cancelled: &dyn Fn() -> bool,
    mut progress: impl FnMut(&str, u64, u64),
) -> Vec<CleanCategory> {
    let excluded = normalize_excluded(excluded);
    let mut warnings = Vec::new();
    let mut visited = 0;
    let mut report = |path: &str, _bytes: u64| {
        if Path::new(path).is_file() {
            visited += 1;
        }
        progress(path, visited, 0);
    };
    let mut categories = Vec::new();
    if let Some(cache) = cache_root() {
        report(&cache.to_string_lossy(), 0);
        let browser_roots: Vec<PathBuf> =
            browser_names().iter().map(|(_, p)| cache.join(p)).collect();
        let developer_roots = platform::home()
            .map(|home| crate::developer::cache_roots(&home, &cache))
            .unwrap_or_default();
        let mut app_excluded = excluded.clone();
        app_excluded.extend(browser_roots.iter().cloned());
        app_excluded.extend(developer_roots.iter().cloned());
        app_excluded.extend(
            [
                "node",
                "codex",
                "codex-runtimes",
                "corepack",
                "uv",
                "ms-playwright",
                "puppeteer",
                "electron",
            ]
            .iter()
            .map(|name| cache.join(name)),
        );
        let app_items = children_controlled(
            &cache,
            "应用运行缓存；请先退出相关应用",
            false,
            &app_excluded,
            cancelled,
            &mut warnings,
            &mut report,
        )
        .into_iter()
        // Never count a browser cache twice, including the Google parent on macOS.
        .filter(|i| {
            ![
                "node",
                "codex",
                "codex-runtimes",
                "corepack",
                "uv",
                "ms-playwright",
                "puppeteer",
                "electron",
            ]
            .contains(&i.name.as_str())
                && !developer_roots
                    .iter()
                    .any(|root| root.starts_with(&i.path) || Path::new(&i.path).starts_with(root))
                && !browser_roots
                    .iter()
                    .any(|b| b.starts_with(&i.path) || Path::new(&i.path).starts_with(b))
        })
        .collect();
        categories.push(category(
            "app_cache",
            "应用缓存",
            "各应用运行时产生的缓存文件",
            app_items,
        ));
        let browser_items = browser_names()
            .iter()
            .filter_map(|(name, p)| {
                let path = cache.join(p);
                if cancelled() || excluded_target(&path, &excluded) {
                    return None;
                }
                let mut i = item_controlled(
                    &path,
                    "网页资源缓存；不包含密码、Cookie 和浏览记录",
                    false,
                    cancelled,
                    &mut report,
                )?;
                i.name = format!("{name} 缓存");
                Some(i)
            })
            .collect();
        categories.push(category(
            "browser_cache",
            "上网缓存",
            "清理前请退出浏览器",
            browser_items,
        ));
    }
    if cfg!(target_os = "macos")
        && let Some(home) = platform::home()
    {
        categories.push(category(
            "logs",
            "应用日志",
            "应用记录的运行日志",
            children_controlled(
                &home.join("Library/Logs"),
                "运行日志，清理后无法用于历史故障排查",
                false,
                &excluded,
                cancelled,
                &mut warnings,
                &mut report,
            ),
        ));
    }
    if let Some(trash) = trash_root() {
        categories.push(category(
            "trash",
            "废纸篓",
            "清空后无法恢复，请检查所选文件",
            children_controlled(
                &trash,
                "已移入废纸篓的文件，将彻底删除",
                false,
                &excluded,
                cancelled,
                &mut warnings,
                &mut report,
            ),
        ));
    }
    report(
        "开发工具历史缓存",
        categories.iter().map(|c| c.total_size_bytes).sum(),
    );
    let developer_category =
        build_developer_category_controlled(&[], 7, &excluded, cancelled, |value| {
            report(&value.path, 0)
        })
        .unwrap_or_else(|error| {
            let mut failed = category(
                "developer",
                "开发者垃圾",
                "开发工具缓存扫描未完成",
                Vec::new(),
            );
            failed.items.push(CleanItem {
                id: "developer:error".into(),
                path: "developer:error".into(),
                name: "开发者缓存扫描未完成".into(),
                size_bytes: 0,
                size_str: "扫描失败".into(),
                reason: error.clone(),
                can_delete: false,
                selected: false,
                size_known: false,
                action: None,
                details: Some(error),
                files: Vec::new(),
            });
            failed.count = 1;
            failed.total_size_str = "扫描未完成".into();
            (failed, HashMap::new(), HashMap::new())
        });
    let developer_receipt = developer_category.1;
    let developer_groups = developer_category.2;
    categories.push(developer_category.0);
    report(
        "系统安装包、历史日志、过期临时文件与容器垃圾",
        categories.iter().map(|c| c.total_size_bytes).sum(),
    );
    if !cancelled() {
        categories.extend(scan_system_categories());
    }
    if cancelled() {
        warnings.push("扫描已停止；系统及未读取目录尚未覆盖".into());
    }
    if !warnings.is_empty() {
        let mut coverage = category("coverage", "扫描覆盖", "当前扫描覆盖信息", Vec::new());
        coverage.warning = Some(warnings.join("\n"));
        categories.push(coverage);
    }
    for cat in &mut categories {
        for item in &mut cat.items {
            if item.action.is_some() {
                let paths: Vec<_> = item
                    .files
                    .iter()
                    .filter_map(|file| {
                        let path = PathBuf::from(&file.path);
                        path.is_absolute()
                            .then(|| fs::canonicalize(&path).unwrap_or(path))
                    })
                    .collect();
                if let Some(reason) = system_exclusion_reason(Some(&paths), &excluded) {
                    item.can_delete = false;
                    item.selected = false;
                    item.reason = format!("{}；{reason}", item.reason);
                }
            }
        }
        cat.items.retain(|item| {
            item.action.is_some()
                || item.path.starts_with("developer:group:")
                || !excluded_target(Path::new(&item.path), &excluded)
        });
        cat.total_size_bytes = cat.items.iter().map(|item| item.size_bytes).sum();
        cat.count = cat.items.len();
    }
    report(
        "扫描完成",
        categories.iter().map(|c| c.total_size_bytes).sum(),
    );
    let mut allowed: HashSet<PathBuf> = categories
        .iter()
        .flat_map(|c| &c.items)
        .filter(|i| i.can_delete && i.action.is_none())
        .filter_map(|i| fs::canonicalize(&i.path).ok())
        .collect();
    allowed.extend(developer_receipt.keys().cloned());
    if let Ok(mut receipt) = SCANNED_JUNK.lock() {
        receipt.paths = allowed;
        receipt.developer = developer_receipt;
        receipt.developer_groups = developer_groups;
        receipt.action_paths = categories
            .iter()
            .flat_map(|category| &category.items)
            .filter(|item| item.can_delete)
            .filter_map(|item| {
                let (id, _) = item.action.as_ref()?.split_once(':')?;
                Some((
                    id.to_owned(),
                    item.files
                        .iter()
                        .filter_map(|file| {
                            let path = PathBuf::from(&file.path);
                            path.is_absolute()
                                .then(|| fs::canonicalize(&path).unwrap_or(path))
                        })
                        .collect(),
                ))
            })
            .collect();
        receipt.actions = categories
            .iter()
            .flat_map(|c| &c.items)
            .filter(|i| i.can_delete)
            .filter_map(|i| {
                i.action
                    .as_ref()?
                    .split_once(':')
                    .map(|(id, hash)| (id.to_string(), hash.to_string()))
            })
            .collect();
    }
    categories
}

fn scan_system_categories() -> Vec<CleanCategory> {
    let plans = if cfg!(target_os = "linux") {
        crate::privileged::scan_system()
    } else if crate::platform::command_path("docker").is_some() {
        crate::privileged::local_docker()
            .map(|(socket, docker)| crate::system_scan::scan_docker(&socket, &docker))
    } else {
        return Vec::new();
    };
    match plans {
        Ok(plans) => plans
            .into_iter()
            .map(|plan| {
                let failed = plan.error.is_some();
                let fingerprint = plan.fingerprint();
                let files = plan
                    .files
                    .iter()
                    .map(|f| crate::models::CleanFileDetail {
                        name: f.name.clone(),
                        path: f.path.clone(),
                        size_bytes: f.size_bytes,
                        size_str: format_size(f.size_bytes),
                    })
                    .collect();
                let details = format!(
                    "位置：{}\n{}{}",
                    plan.location,
                    plan.description,
                    plan.error
                        .as_ref()
                        .map(|e| format!("\n扫描失败：{e}"))
                        .unwrap_or_default()
                );
                let item = CleanItem {
                    id: format!("system:{}", plan.id),
                    path: format!("system:{}", plan.id),
                    name: plan.title.clone(),
                    size_bytes: plan.size_bytes,
                    size_str: if failed {
                        "扫描失败".into()
                    } else {
                        format_size(plan.size_bytes)
                    },
                    size_known: !failed,
                    reason: plan.description.clone(),
                    can_delete: !failed && !plan.files.is_empty(),
                    selected: false,
                    action: (!failed).then(|| format!("{}:{}", plan.id, fingerprint)),
                    details: Some(details),
                    files,
                };
                let id = if plan.id.starts_with("docker") {
                    "docker"
                } else {
                    &plan.id
                };
                let mut result = category(id, &plan.title, &plan.description, vec![item]);
                if failed {
                    result.total_size_str = "扫描失败".into();
                }
                result
            })
            .fold(Vec::<CleanCategory>::new(), |mut categories, cat| {
                if cat.id == "docker"
                    && let Some(existing) = categories.iter_mut().find(|c| c.id == "docker")
                {
                    existing.items.extend(cat.items);
                    existing.total_size_bytes += cat.total_size_bytes;
                    existing.total_size_str = format_size(existing.total_size_bytes);
                    existing.count = existing.items.len();
                } else {
                    let mut cat = cat;
                    if cat.id == "docker" {
                        cat.title = "容器垃圾".into();
                        cat.desc = "Docker 可回收空间；数据卷和运行中的容器保留".into();
                    }
                    categories.push(cat);
                }
                categories
            }),
        Err(error) => {
            let item = CleanItem {
                id: "system:scan-error".into(),
                path: "system:scan-error".into(),
                name: "系统文件扫描未完成".into(),
                size_bytes: 0,
                size_str: "需要授权".into(),
                size_known: false,
                reason: error.clone(),
                can_delete: false,
                selected: false,
                action: None,
                details: Some(error),
                files: Vec::new(),
            };
            let mut result = category(
                "system",
                "系统垃圾",
                "授权后扫描安装包、历史日志、临时文件及容器垃圾",
                vec![item],
            );
            result.total_size_str = "扫描未完成".into();
            vec![result]
        }
    }
}

// Linux honors user-dirs.dirs (including localized folder names); macOS uses Movies.
fn user_folder(name: &str) -> Option<PathBuf> {
    let home = platform::home()?;
    if cfg!(target_os = "linux")
        && let Some(config) = platform::xdg_dir("XDG_CONFIG_HOME", ".config")
        && let Ok(text) = fs::read_to_string(config.join("user-dirs.dirs"))
    {
        let key = format!("XDG_{}_DIR=", name.to_uppercase());
        if let Some(value) = text.lines().find_map(|l| l.trim().strip_prefix(&key)) {
            let value = value.trim().trim_matches('"');
            let path = if let Some(suffix) = value.strip_prefix("$HOME/") {
                home.join(suffix)
            } else {
                PathBuf::from(value)
            };
            if path.is_absolute() && path != home {
                return Some(path);
            }
        }
    }
    Some(home.join(if name == "Videos" && cfg!(target_os = "macos") {
        "Movies"
    } else {
        name
    }))
}

fn scan_files_controlled(
    options: &ScanOptions,
    defaults: &[&str],
    default_depth: usize,
    cancelled: &dyn Fn() -> bool,
    report: &mut impl FnMut(&str, u64, u64),
) -> Result<FileScanReport<PathBuf>, String> {
    let mut roots: Vec<PathBuf> = if options.roots.is_empty() {
        defaults
            .iter()
            .filter_map(|name| user_folder(name))
            .collect()
    } else {
        options
            .roots
            .iter()
            .map(|root| {
                let path = PathBuf::from(root);
                if !path.is_absolute() || !path.is_dir() {
                    Err(format!("扫描目录不存在或不是绝对路径：{root}"))
                } else {
                    fs::canonicalize(path).map_err(|error| error.to_string())
                }
            })
            .collect::<Result<_, _>>()?
    };
    roots.sort();
    roots.dedup();
    let all_roots = roots.clone();
    roots.retain(|root| {
        !all_roots
            .iter()
            .any(|parent| parent != root && root.starts_with(parent))
    });
    let excluded = normalize_excluded(&options.excluded);
    let depth = options.max_depth.unwrap_or(default_depth).clamp(1, 32);
    let mut result = FileScanReport {
        items: Vec::new(),
        warnings: Vec::new(),
        roots: roots
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        files_scanned: 0,
        cancelled: false,
    };
    let mut seen = HashSet::new();
    let mut deep = 0;
    for root in roots {
        for entry in WalkDir::new(&root)
            .max_depth(depth)
            .into_iter()
            .filter_entry(|entry| {
                let name = entry.file_name().to_string_lossy();
                !cancelled()
                    && !excluded.iter().any(|root| entry.path().starts_with(root))
                    && (entry.depth() == 0
                        || (!name.starts_with('.')
                            && name != "node_modules"
                            && name != "target"
                            && !name.ends_with(".app")))
            })
        {
            if cancelled() {
                result.cancelled = true;
                break;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    if result.warnings.len() < 100 {
                        result.warnings.push(error.to_string());
                    }
                    continue;
                }
            };
            if entry.file_type().is_file() {
                result.files_scanned += 1;
                seen.insert(entry.path().to_path_buf());
            } else if entry.file_type().is_dir() && entry.depth() == depth {
                deep += 1;
            }
            if result.files_scanned % 100 == 0 || entry.file_type().is_dir() {
                report(&entry.path().to_string_lossy(), result.files_scanned, 0);
            }
        }
        if cancelled() {
            result.cancelled = true;
            break;
        }
    }
    result.items = seen.into_iter().collect();
    result.items.sort();
    if deep > 0 {
        result.warnings.push(format!(
            "有 {deep} 个目录达到 {depth} 层限制；可选其子目录继续扫描"
        ));
    }
    if result.cancelled {
        result.warnings.push("扫描已停止，保留已读取的结果".into());
    }
    Ok(result)
}
pub fn scan_large_files(min_mb: u64) -> Vec<LargeFileItem> {
    scan_large_files_with_options(
        &ScanOptions {
            min_mb: Some(min_mb),
            ..Default::default()
        },
        &|| false,
        |_, _, _| {},
    )
    .map(|result| result.items)
    .unwrap_or_default()
}
pub fn scan_large_files_with_options(
    options: &ScanOptions,
    cancelled: &dyn Fn() -> bool,
    mut report: impl FnMut(&str, u64, u64),
) -> Result<FileScanReport<LargeFileItem>, String> {
    let scan = scan_files_controlled(
        options,
        &["Downloads", "Documents", "Videos", "Desktop"],
        6,
        cancelled,
        &mut report,
    )?;
    let mut result = FileScanReport {
        items: Vec::new(),
        warnings: scan.warnings,
        roots: scan.roots,
        files_scanned: scan.files_scanned,
        cancelled: scan.cancelled,
    };
    let min_bytes = options
        .min_mb
        .unwrap_or(50)
        .max(1)
        .saturating_mul(1024 * 1024);
    for path in scan.items {
        let metadata = match path.metadata() {
            Ok(meta) => meta,
            Err(error) => {
                result.warnings.push(format!("{}: {error}", path.display()));
                continue;
            }
        };
        if metadata.len() >= min_bytes {
            result.items.push(LargeFileItem {
                name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: path.to_string_lossy().into_owned(),
                size_bytes: metadata.len(),
                size_str: format_size(metadata.len()),
                ext: path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            });
        }
    }
    result
        .items
        .sort_by_key(|item| std::cmp::Reverse(item.size_bytes));
    if let Ok(mut receipt) = LARGE_FILES.lock() {
        *receipt = result
            .items
            .iter()
            .filter_map(|item| {
                let path = fs::canonicalize(&item.path).ok()?;
                let meta = path.metadata().ok()?;
                Some((path, (meta.len(), meta.modified().ok()?)))
            })
            .collect();
    }
    Ok(result)
}
pub fn scan_duplicate_files() -> Vec<DuplicateGroup> {
    scan_duplicate_files_with_options(&ScanOptions::default(), &|| false, |_, _, _| {})
        .map(|result| result.items)
        .unwrap_or_default()
}
pub fn scan_duplicate_files_with_options(
    options: &ScanOptions,
    cancelled: &dyn Fn() -> bool,
    mut report: impl FnMut(&str, u64, u64),
) -> Result<FileScanReport<DuplicateGroup>, String> {
    let scan = scan_files_controlled(
        options,
        &["Downloads", "Documents", "Pictures"],
        5,
        cancelled,
        &mut report,
    )?;
    let mut result = FileScanReport {
        items: Vec::new(),
        warnings: scan.warnings,
        roots: scan.roots,
        files_scanned: scan.files_scanned,
        cancelled: scan.cancelled,
    };
    let min_bytes = options
        .min_mb
        .map(|mb| mb.max(1).saturating_mul(1024 * 1024))
        .unwrap_or(64 * 1024);
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    #[cfg(unix)]
    let mut identities = HashSet::new();
    #[cfg(unix)]
    let mut hardlinks = 0;
    for path in scan.items {
        match path.metadata() {
            Ok(meta) if meta.len() > min_bytes => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if !identities.insert((meta.dev(), meta.ino())) {
                        hardlinks += 1;
                        continue;
                    }
                }
                size_map.entry(meta.len()).or_default().push(path);
            }
            Err(error) => result.warnings.push(format!("{}: {error}", path.display())),
            _ => {}
        }
    }
    #[cfg(unix)]
    if hardlinks > 0 {
        result.warnings.push(format!(
            "已跳过 {hardlinks} 个硬链接引用；它们不占用额外文件空间"
        ));
    }
    let mut hashed = 0;
    for (size, paths) in size_map {
        if paths.len() < 2 {
            continue;
        }
        let mut hashes: HashMap<String, Vec<String>> = HashMap::new();
        for path in paths {
            if cancelled() {
                result.cancelled = true;
                break;
            }
            report(&path.to_string_lossy(), hashed, 0);
            match compute_file_sha256_controlled(&path, cancelled) {
                Ok(hash) => {
                    hashed += 1;
                    hashes
                        .entry(hash)
                        .or_default()
                        .push(path.to_string_lossy().into_owned());
                }
                Err(error) => {
                    if !cancelled() {
                        result
                            .warnings
                            .push(format!("无法比对 {}: {error}", path.display()));
                    }
                }
            }
        }
        for (hash, mut files) in hashes {
            if files.len() > 1 {
                files.sort();
                result.items.push(DuplicateGroup {
                    hash,
                    size_bytes: size,
                    size_str: format_size(size),
                    files,
                });
            }
        }
        if cancelled() {
            break;
        }
    }
    if cancelled() {
        result.cancelled = true;
        result
            .warnings
            .push("内容比对已停止，仅显示已完整比对的重复组".into());
    }
    result.items.sort_by_key(|group| {
        std::cmp::Reverse(
            group
                .size_bytes
                .saturating_mul(group.files.len().saturating_sub(1) as u64),
        )
    });
    if let Ok(mut receipt) = DUPLICATES.lock() {
        *receipt = result.items.clone();
    }
    Ok(result)
}

fn validate_duplicate_selection(
    selected: &HashSet<PathBuf>,
    receipt: &[DuplicateGroup],
) -> Result<(), String> {
    for path in selected {
        let group = receipt
            .iter()
            .find(|g| g.files.iter().any(|f| Path::new(f) == path))
            .ok_or("文件不在重复文件扫描结果中")?;
        if compute_file_sha256(path).map_err(|e| e.to_string())? != group.hash {
            return Err("文件内容已改变，请重新比对".into());
        }
        let retained = group.files.iter().any(|f| {
            !selected.contains(Path::new(f))
                && compute_file_sha256(Path::new(f)).is_ok_and(|h| h == group.hash)
        });
        if !retained {
            return Err("每组重复文件至少保留一份有效副本".into());
        }
    }
    Ok(())
}

fn report_action(
    report: &mut ActionReport,
    total: u64,
    progress: &mut impl FnMut(&str, u64, u64),
    path: impl Into<String>,
    status: &str,
    message: impl Into<String>,
) {
    let path = path.into();
    report.push(path.clone(), status, message);
    progress(&path, report.items.len() as u64, total);
}
fn system_exclusion_reason(paths: Option<&[PathBuf]>, excluded: &[PathBuf]) -> Option<String> {
    if excluded.is_empty() {
        return None;
    }
    match paths {
        Some(paths) if !paths.is_empty() => paths
            .iter()
            .any(|path| excluded_target(path, excluded))
            .then(|| "当前保留名单命中此系统清理范围，已跳过整类操作".into()),
        _ => {
            Some("系统操作没有可核对的真实文件范围，无法证明避开当前保留名单，已跳过此操作".into())
        }
    }
}
pub fn trash_scanned_files(paths: Vec<String>, kind: &str) -> Result<ActionReport, String> {
    trash_scanned_files_controlled(paths, kind, &[], |_, _, _| {})
}
pub fn trash_scanned_files_controlled(
    paths: Vec<String>,
    kind: &str,
    excluded: &[String],
    mut progress: impl FnMut(&str, u64, u64),
) -> Result<ActionReport, String> {
    let excluded = normalize_excluded(excluded);
    let total = paths.iter().collect::<HashSet<_>>().len() as u64;
    let selected: HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if selected.is_empty() {
        return Ok(ActionReport::default());
    }
    let mut report = ActionReport::default();
    let mut valid = HashSet::new();
    let large_receipt = LARGE_FILES
        .lock()
        .map_err(|error| error.to_string())?
        .clone();
    for path in &selected {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if excluded_target(path, &excluded) || excluded_target(&canonical, &excluded) {
            report_action(
                &mut report,
                total,
                &mut progress,
                path.to_string_lossy(),
                "skipped",
                "当前保留名单命中此文件，已保留",
            );
            continue;
        }
        let check = (|| -> Result<(), String> {
            if !fs::symlink_metadata(path)
                .map_err(|error| error.to_string())?
                .is_file()
            {
                return Err("只允许处理扫描到的普通文件；符号链接保留".into());
            }
            if kind == "large" {
                let canonical = fs::canonicalize(path).map_err(|error| error.to_string())?;
                let metadata = path.metadata().map_err(|error| error.to_string())?;
                if large_receipt.get(&canonical)
                    != Some(&(
                        metadata.len(),
                        metadata.modified().map_err(|error| error.to_string())?,
                    ))
                {
                    return Err("文件已改变或不在扫描结果中，请重新扫描".into());
                }
            }
            Ok(())
        })();
        match check {
            Ok(()) => {
                valid.insert(path.clone());
            }
            Err(error) => report_action(
                &mut report,
                total,
                &mut progress,
                path.to_string_lossy(),
                "failed",
                error,
            ),
        }
    }
    match kind {
        "large" => {}
        "duplicates" => {
            let receipt = DUPLICATES
                .lock()
                .map_err(|error| error.to_string())?
                .clone();
            // Recheck the full intended selection before moving any member of a group.
            if let Err(error) = validate_duplicate_selection(
                &selected
                    .iter()
                    .filter(|path| {
                        !excluded_target(path, &excluded)
                            && !excluded_target(
                                &fs::canonicalize(path).unwrap_or_else(|_| (*path).clone()),
                                &excluded,
                            )
                    })
                    .cloned()
                    .collect(),
                &receipt,
            ) {
                for path in valid {
                    report_action(
                        &mut report,
                        total,
                        &mut progress,
                        path.to_string_lossy(),
                        "skipped",
                        format!("比对保护阻止本次处理：{error}"),
                    );
                }
                return Ok(report);
            }
        }
        _ => return Err("无效的扫描类型".into()),
    }
    let mut selected: Vec<_> = valid.into_iter().collect();
    selected.sort();
    for path in selected {
        progress(&path.to_string_lossy(), report.items.len() as u64, total);
        let result = if cfg!(target_os = "macos") {
            platform::checked_output(std::process::Command::new("/usr/bin/osascript").args(["-e", "on run argv\ntell application \"Finder\" to delete POSIX file (item 1 of argv)\nend run"]).arg(&path))
        } else if let Some(gio) = platform::command_path("gio") {
            platform::checked_output(
                std::process::Command::new(gio)
                    .args(["trash", "--"])
                    .arg(&path),
            )
        } else {
            Err("系统未提供回收站工具 gio".into())
        };
        match result {
            Ok(_) => {
                report_action(
                    &mut report,
                    total,
                    &mut progress,
                    path.to_string_lossy(),
                    "success",
                    "已移入废纸篓",
                );
                if kind == "large"
                    && let Ok(mut receipt) = LARGE_FILES.lock()
                {
                    receipt.remove(&path);
                }
            }
            Err(error) => report_action(
                &mut report,
                total,
                &mut progress,
                path.to_string_lossy(),
                "failed",
                error,
            ),
        }
    }
    Ok(report)
}

fn validated_cleanup_paths(
    paths: &[String],
    allowed: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, String> {
    let mut validated = HashSet::new();
    for value in paths {
        let path = Path::new(value);
        let meta = fs::symlink_metadata(path).map_err(|e| format!("{value}: {e}"))?;
        let canonical = fs::canonicalize(path).map_err(|e| format!("{value}: {e}"))?;
        if meta.file_type().is_symlink() || !allowed.contains(&canonical) {
            return Err(format!("清理范围已改变，请重新扫描: {value}"));
        }
        validated.insert(canonical);
    }
    Ok(validated.into_iter().collect())
}

pub fn delete_clean_items(paths: Vec<String>) -> Result<ActionReport, String> {
    delete_clean_items_controlled(paths, &[], |_, _, _| {})
}
pub fn delete_clean_items_controlled(
    paths: Vec<String>,
    excluded: &[String],
    mut progress: impl FnMut(&str, u64, u64),
) -> Result<ActionReport, String> {
    let excluded = normalize_excluded(excluded);
    let receipt = SCANNED_JUNK.lock().map_err(|e| e.to_string())?;
    let expanded = expand_developer_groups(&paths, &receipt.developer_groups)?;
    let allowed = receipt.paths.clone();
    let developer = receipt.developer.clone();
    let actions = receipt.actions.clone();
    let action_paths = receipt.action_paths.clone();
    let total = expanded.len() as u64;
    drop(receipt);
    let mut report = ActionReport::default();
    let mut plans = HashMap::new();
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for value in expanded {
        if !seen.insert(value.clone()) {
            report_action(
                &mut report,
                total,
                &mut progress,
                value,
                "skipped",
                "重复选择已合并",
            );
            continue;
        }
        if let Some(id) = value.strip_prefix("system:") {
            if let Some(fingerprint) = actions.get(id) {
                if let Some(reason) =
                    system_exclusion_reason(action_paths.get(id).map(Vec::as_slice), &excluded)
                {
                    report_action(&mut report, total, &mut progress, value, "skipped", reason);
                    continue;
                }
                plans.insert(id.to_owned(), fingerprint.clone());
            } else {
                report_action(
                    &mut report,
                    total,
                    &mut progress,
                    value,
                    "failed",
                    "系统清理范围已改变，请重新扫描",
                );
            }
        } else {
            files.push(value);
        }
    }
    if !plans.is_empty() {
        progress(
            "正在复核并执行所选系统操作",
            report.items.len() as u64,
            total,
        );
        match crate::privileged::clean_system_report(&plans) {
            Ok(result) => {
                for item in result.items {
                    report_action(
                        &mut report,
                        total,
                        &mut progress,
                        item.path,
                        &item.status,
                        item.message,
                    );
                }
            }
            Err(error) => {
                for id in plans.keys() {
                    report_action(
                        &mut report,
                        total,
                        &mut progress,
                        format!("system:{id}"),
                        "failed",
                        &error,
                    );
                }
            }
        }
    }
    for value in files {
        let original = Path::new(&value);
        let canonical = fs::canonicalize(original).unwrap_or_else(|_| original.to_path_buf());
        if excluded_target(original, &excluded) || excluded_target(&canonical, &excluded) {
            report_action(
                &mut report,
                total,
                &mut progress,
                value,
                "skipped",
                "当前保留名单命中此目录或其中的文件，已保留整个项目",
            );
            continue;
        }
        progress(&value, report.items.len() as u64, total);
        let validated = match validated_cleanup_paths(std::slice::from_ref(&value), &allowed) {
            Ok(paths) => paths,
            Err(error) => {
                report_action(&mut report, total, &mut progress, value, "failed", error);
                continue;
            }
        };
        let path = &validated[0];
        if let Some((days, fingerprint)) = developer.get(path)
            && let Err(error) = crate::developer::revalidate(path, *days, fingerprint)
        {
            report_action(&mut report, total, &mut progress, value, "failed", error);
            continue;
        }
        let result = if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match result {
            Ok(_) => {
                report_action(
                    &mut report,
                    total,
                    &mut progress,
                    value,
                    "success",
                    "已清理；容量为扫描时逻辑大小",
                );
                if let Ok(mut receipt) = SCANNED_JUNK.lock() {
                    receipt.paths.remove(path);
                    receipt.developer.remove(path);
                    for files in receipt.developer_groups.values_mut() {
                        files.retain(|file| Path::new(&file.path) != path);
                    }
                }
            }
            Err(error) => report_action(
                &mut report,
                total,
                &mut progress,
                value,
                "failed",
                format!("{error}；目录可能已部分处理，请重新扫描确认剩余内容"),
            ),
        }
    }
    Ok(report)
}

pub fn clean_item_details(path: &str) -> Result<Vec<crate::models::CleanFileDetail>, String> {
    let receipt = SCANNED_JUNK.lock().map_err(|e| e.to_string())?;
    if path.starts_with("developer:group:") {
        return receipt
            .developer_groups
            .get(path)
            .cloned()
            .ok_or("该清理分类已失效，请重新扫描".into());
    }
    let canonical = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !receipt.paths.contains(&canonical) {
        return Err("该目录不在当前扫描结果中".into());
    }
    let paths: Vec<PathBuf> = if canonical.is_dir() {
        fs::read_dir(&canonical)
            .map_err(|e| e.to_string())?
            .map(|e| e.map(|e| e.path()))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    } else {
        vec![canonical]
    };
    let mut files = Vec::new();
    for path in paths {
        let size = get_dir_or_file_size(&path)?;
        files.push(crate::models::CleanFileDetail {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            path: path.to_string_lossy().into_owned(),
            size_bytes: size,
            size_str: format_size(size),
        });
    }
    files.sort_by_key(|f| std::cmp::Reverse(f.size_bytes));
    Ok(files)
}

fn compute_file_sha256(path: &Path) -> Result<String, std::io::Error> {
    compute_file_sha256_controlled(path, &|| false)
}
fn compute_file_sha256_controlled(
    path: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<String, std::io::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        if cancelled() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "扫描已停止",
            ));
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub(crate) fn get_dir_or_file_size(path: &Path) -> Result<u64, String> {
    get_size_controlled(path, &|| false, &mut |_, _| {})
}
fn get_size_controlled(
    path: &Path,
    cancelled: &dyn Fn() -> bool,
    report: &mut impl FnMut(&str, u64),
) -> Result<u64, String> {
    let mut total = 0u64;
    for entry in WalkDir::new(path) {
        if cancelled() {
            return Err("扫描已停止，容量尚未完整统计".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file() {
            total = total.saturating_add(entry.metadata().map_err(|e| e.to_string())?.len());
            report(&entry.path().to_string_lossy(), total);
        }
    }
    Ok(total)
}

type DeveloperReceipt = HashMap<PathBuf, (u64, String)>;
type DeveloperGroups = HashMap<String, Vec<crate::models::CleanFileDetail>>;
fn expand_developer_groups(
    paths: &[String],
    groups: &DeveloperGroups,
) -> Result<Vec<String>, String> {
    let mut expanded = Vec::new();
    for path in paths {
        if path.starts_with("developer:group:") {
            let files = groups.get(path).ok_or("开发者清理分类已失效，请重新扫描")?;
            expanded.extend(files.iter().map(|file| file.path.clone()));
        } else {
            expanded.push(path.clone());
        }
    }
    Ok(expanded)
}
fn developer_items(
    targets: Vec<crate::developer::DevTarget>,
    days: u64,
) -> (Vec<CleanItem>, DeveloperReceipt, DeveloperGroups) {
    let mut items = Vec::new();
    let mut receipt = HashMap::new();
    let mut groups = HashMap::new();
    for target in targets {
        if target.size_bytes == 0 {
            continue;
        }
        let path = target.path.to_string_lossy().into_owned();
        receipt.insert(target.path.clone(), (days, target.fingerprint));
        items.push(CleanItem {
            id: path.clone(),
            path,
            name: target.name,
            size_bytes: target.size_bytes,
            size_str: format_size(target.size_bytes),
            reason: format!("{}；至少 {days} 天未修改或访问", target.reason),
            can_delete: true,
            selected: false,
            size_known: true,
            action: None,
            details: None,
            files: Vec::new(),
        });
    }
    // Project output is grouped by its actual source directory, regardless of list size.
    let mut by_source: HashMap<String, Vec<CleanItem>> = HashMap::new();
    for mut item in items {
        let project = item.reason.contains("项目产物");
        let source = if project {
            Path::new(&item.path)
                .parent()
                .unwrap_or(Path::new(&item.path))
                .to_string_lossy()
                .into_owned()
        } else {
            format!("工具缓存 · {}", item.name)
        };
        item.details = Some(format!(
            "来源：{source}\n{}\n清理影响：{}",
            item.reason,
            if project {
                "保留源码和锁文件；下次启动需重新安装依赖或构建，首次运行会更慢。"
            } else {
                "保留配置和工具链；下次使用可能重新下载包或资源。"
            }
        ));
        by_source.entry(source).or_default().push(item);
    }
    items = Vec::new();
    for (source, mut targets) in by_source {
        if targets.len() == 1 {
            let mut item = targets.remove(0);
            item.name = format!(
                "{} · {}",
                item.name,
                Path::new(&source)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
            items.push(item);
            continue;
        }
        let mut files: Vec<_> = targets
            .iter()
            .map(|target| crate::models::CleanFileDetail {
                name: format!(
                    "{} · {}",
                    target.name,
                    Path::new(&target.path)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                ),
                path: target.path.clone(),
                size_bytes: target.size_bytes,
                size_str: target.size_str.clone(),
            })
            .collect();
        files.sort_by_key(|file| std::cmp::Reverse(file.size_bytes));
        let key = format!(
            "developer:group:{}",
            hex::encode(Sha256::digest(
                serde_json::to_vec(&files).unwrap_or_default()
            ))
        );
        let bytes = files.iter().map(|file| file.size_bytes).sum();
        let count = files.len();
        items.push(CleanItem { id: key.clone(), path: key.clone(), name: format!("{}（{count} 项）", Path::new(&source).file_name().unwrap_or_default().to_string_lossy()), size_bytes: bytes, size_str: format_size(bytes), reason: targets[0].reason.clone(), can_delete: true, selected: false, size_known: true, action: None, details: Some(format!("来源：{source}\n共 {count} 个已统计产物。勾选分类会清理全部；展开后可单独选择。清理后需重新安装依赖或构建，保留源码、锁文件和工具链。")), files: files.clone() });
        groups.insert(key, files);
    }
    (items, receipt, groups)
}
fn build_developer_category_controlled(
    roots: &[String],
    days: u64,
    excluded: &[PathBuf],
    cancelled: &dyn Fn() -> bool,
    report: impl FnMut(crate::developer::DevProgress),
) -> Result<(CleanCategory, DeveloperReceipt, DeveloperGroups), String> {
    let home = platform::home().ok_or("无法找到用户目录")?;
    let cache = cache_root().ok_or("无法找到缓存目录")?;
    let roots: Vec<_> = roots.iter().map(PathBuf::from).collect();
    let scan = crate::developer::scan_controlled(
        &home, &cache, &roots, days, excluded, cancelled, report,
    )?;
    let (items, receipt, groups) = developer_items(scan.targets, days);
    let count = receipt.len();
    let mut result = category(
        "developer",
        "开发者垃圾",
        "历史包缓存和可重新生成的项目产物",
        items,
    );
    result.count = count;
    result.warning = (!scan.warnings.is_empty()).then(|| scan.warnings.join("\n"));
    result.scan_summary = Some(format!(
        "已检查 {} 个目录，识别 {} 个候选项目目录，跳过 {} 个服务数据目录",
        scan.directories_scanned, scan.projects_found, scan.storage_skipped
    ));
    Ok((result, receipt, groups))
}

pub fn scan_developer_junk(
    root: Option<&str>,
    days: u64,
    report: impl FnMut(crate::developer::DevProgress),
) -> Result<CleanCategory, String> {
    scan_developer_junk_controlled(
        &root.map(str::to_owned).into_iter().collect::<Vec<_>>(),
        days,
        &[],
        &|| false,
        report,
    )
}
pub fn scan_developer_junk_controlled(
    roots: &[String],
    days: u64,
    excluded: &[String],
    cancelled: &dyn Fn() -> bool,
    report: impl FnMut(crate::developer::DevProgress),
) -> Result<CleanCategory, String> {
    let (category, developer, groups) = build_developer_category_controlled(
        roots,
        days,
        &normalize_excluded(excluded),
        cancelled,
        report,
    )?;
    let mut receipt = SCANNED_JUNK.lock().map_err(|e| e.to_string())?;
    let old: Vec<_> = receipt.developer.keys().cloned().collect();
    for path in old {
        receipt.paths.remove(&path);
    }
    receipt.paths.extend(developer.keys().cloned());
    receipt.developer = developer;
    receipt.developer_groups = groups;
    Ok(category)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_exclusions_protect_stale_receipts_and_report_completed_progress() {
        let root = std::env::temp_dir().join(format!("cleaner-execution-{}", std::process::id()));
        fs::create_dir_all(root.join("generated/keep")).unwrap();
        fs::write(root.join("generated/keep/file"), b"keep").unwrap();
        let target = fs::canonicalize(root.join("generated")).unwrap();
        SCANNED_JUNK.lock().unwrap().paths.insert(target.clone());
        let mut progress = Vec::new();
        let report = delete_clean_items_controlled(
            vec![target.to_string_lossy().into_owned()],
            &[target.join("keep").to_string_lossy().into_owned()],
            |path, done, total| progress.push((path.to_owned(), done, total)),
        )
        .unwrap();
        assert_eq!((report.succeeded, report.failed, report.skipped), (0, 0, 1));
        assert!(target.join("keep/file").exists());
        assert_eq!(
            progress.last().map(|(_, done, total)| (*done, *total)),
            Some((1, 1))
        );
        let file = target.join("keep/file");
        let trash = trash_scanned_files_controlled(
            vec![file.to_string_lossy().into_owned()],
            "large",
            &[target.to_string_lossy().into_owned()],
            |_, _, _| {},
        )
        .unwrap();
        assert_eq!(trash.skipped, 1);
        assert!(file.exists());
        let paths = vec![target.clone()];
        assert!(system_exclusion_reason(Some(&paths), &[target.join("keep")]).is_some());
        assert!(system_exclusion_reason(None, std::slice::from_ref(&target)).is_some());
        assert!(system_exclusion_reason(Some(&[]), &[]).is_none());
        assert!(system_exclusion_reason(Some(&paths), &[root.join("unrelated")]).is_none());
        SCANNED_JUNK.lock().unwrap().paths.remove(&target);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn scoped_file_scan_honors_threshold_exclusions_and_cancellation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::env::temp_dir().join(format!("cleaner-scope-{}", std::process::id()));
        fs::create_dir_all(root.join("keep")).unwrap();
        fs::write(root.join("large"), vec![1u8; 1024 * 1024]).unwrap();
        fs::write(root.join("small"), b"small").unwrap();
        fs::write(root.join("keep/large"), vec![1u8; 1024 * 1024]).unwrap();
        let options = ScanOptions {
            roots: vec![root.to_string_lossy().into_owned()],
            excluded: vec![root.join("keep").to_string_lossy().into_owned()],
            min_mb: Some(1),
            max_depth: None,
        };
        let report = scan_large_files_with_options(&options, &|| false, |_, _, _| {}).unwrap();
        assert_eq!(report.items.len(), 1);
        assert!(report.items[0].path.ends_with("/large"));
        assert_eq!(report.files_scanned, 2);
        assert!(!report.cancelled);
        let checks = AtomicUsize::new(0);
        let cancelled = scan_files_controlled(
            &options,
            &[],
            6,
            &|| checks.fetch_add(1, Ordering::SeqCst) > 4,
            &mut |_, _, _| {},
        )
        .unwrap();
        assert!(cancelled.cancelled);
        assert!(!cancelled.warnings.is_empty());
        assert!(checks.load(Ordering::SeqCst) < 20);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cleanup_report_preserves_success_and_only_removes_receipted_fixture() {
        let root = std::env::temp_dir().join(format!("cleaner-report-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let cached = root.join("cache");
        let outside = root.join("original");
        fs::write(&cached, b"cache").unwrap();
        fs::write(&outside, b"keep").unwrap();
        SCANNED_JUNK
            .lock()
            .unwrap()
            .paths
            .insert(fs::canonicalize(&cached).unwrap());
        let report = delete_clean_items(vec![
            cached.to_string_lossy().into_owned(),
            outside.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert_eq!((report.succeeded, report.failed), (1, 1));
        assert!(!cached.exists());
        assert!(outside.exists());
        assert!(
            report
                .items
                .iter()
                .any(|item| item.path == outside.to_string_lossy() && item.status == "failed")
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn duplicate_hash_can_stop_inside_read_and_hardlinks_are_not_copies() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let root = std::env::temp_dir().join(format!("cleaner-hash-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let original = root.join("original");
        fs::write(&original, vec![7u8; 256 * 1024]).unwrap();
        let checks = AtomicUsize::new(0);
        assert_eq!(
            compute_file_sha256_controlled(&original, &|| checks.fetch_add(1, Ordering::SeqCst)
                >= 1)
            .unwrap_err()
            .kind(),
            std::io::ErrorKind::Interrupted
        );
        #[cfg(unix)]
        {
            fs::hard_link(&original, root.join("link")).unwrap();
            let options = ScanOptions {
                roots: vec![root.to_string_lossy().into_owned()],
                ..Default::default()
            };
            let report =
                scan_duplicate_files_with_options(&options, &|| false, |_, _, _| {}).unwrap();
            assert!(report.items.is_empty());
            assert!(
                report
                    .warnings
                    .iter()
                    .any(|warning| warning.contains("硬链接"))
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn grouped_developer_results_keep_exact_sizes_paths_and_do_not_accept_unknown_groups() {
        let targets = (0..201)
            .map(|index| crate::developer::DevTarget {
                name: "Python 字节码缓存".into(),
                path: PathBuf::from(format!("/tmp/projects/project-{index}/__pycache__")),
                reason: "generated".into(),
                size_bytes: 10,
                fingerprint: format!("hash-{index}"),
            })
            .collect();
        let (items, receipt, groups) = developer_items(targets, 7);
        assert_eq!(items.len(), 1);
        assert_eq!(receipt.len(), 201);
        assert_eq!(items[0].size_bytes, 2010);
        assert_eq!(items[0].files.len(), 201);
        let paths = expand_developer_groups(&[items[0].path.clone()], &groups).unwrap();
        assert_eq!(paths.len(), 201);
        assert!(
            paths
                .iter()
                .all(|path| receipt.contains_key(Path::new(path)))
        );
        assert!(expand_developer_groups(&["developer:group:unscanned".into()], &groups).is_err());
        let single = expand_developer_groups(&[paths[0].clone()], &groups).unwrap();
        assert_eq!(single, vec![paths[0].clone()]);
    }
    #[test]
    fn system_cleanup_rejects_unscanned_and_diagnostic_actions() {
        assert_eq!(
            delete_clean_items(vec!["system:docker-usage".into()])
                .unwrap()
                .failed,
            1
        );
        assert_eq!(
            delete_clean_items(vec!["system:system:apt".into()])
                .unwrap()
                .failed,
            1
        );
    }

    #[test]
    fn duplicate_cleanup_requires_an_unchanged_retained_copy() {
        let root =
            std::env::temp_dir().join(format!("appsteward-duplicate-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let a = root.join("a");
        let b = root.join("b");
        fs::write(&a, "same").unwrap();
        fs::write(&b, "same").unwrap();
        let groups = vec![DuplicateGroup {
            hash: compute_file_sha256(&a).unwrap(),
            size_bytes: 4,
            size_str: "4 B".into(),
            files: vec![
                a.to_string_lossy().into_owned(),
                b.to_string_lossy().into_owned(),
            ],
        }];
        assert!(
            validate_duplicate_selection(&HashSet::from([a.clone(), b.clone()]), &groups).is_err()
        );
        assert!(validate_duplicate_selection(&HashSet::from([b.clone()]), &groups).is_ok());
        fs::write(&a, "changed").unwrap();
        assert!(validate_duplicate_selection(&HashSet::from([b]), &groups).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cleanup_rejects_unscanned_paths_and_symlinks() {
        let root =
            std::env::temp_dir().join(format!("appsteward-clean-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let cached = root.join("cache");
        let other = root.join("document");
        fs::write(&cached, "cache").unwrap();
        fs::write(&other, "keep").unwrap();
        let allowed = HashSet::from([fs::canonicalize(&cached).unwrap()]);
        assert!(
            validated_cleanup_paths(&[other.to_string_lossy().into_owned()], &allowed).is_err()
        );
        assert_eq!(
            validated_cleanup_paths(&vec![cached.to_string_lossy().into_owned(); 2], &allowed)
                .unwrap()
                .len(),
            1
        );
        #[cfg(unix)]
        {
            let link = root.join("link");
            std::os::unix::fs::symlink(&cached, &link).unwrap();
            assert!(
                validated_cleanup_paths(&[link.to_string_lossy().into_owned()], &allowed).is_err()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
