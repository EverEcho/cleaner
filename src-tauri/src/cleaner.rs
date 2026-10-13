use crate::{
    models::{CleanCategory, CleanItem, DuplicateGroup, LargeFileItem, format_size},
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

fn item(path: &Path, reason: &str, selected: bool) -> Option<CleanItem> {
    if fs::symlink_metadata(path).ok()?.file_type().is_symlink() {
        return None;
    }
    let result = get_dir_or_file_size(path);
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

fn children(root: &Path, reason: &str, selected: bool) -> Vec<CleanItem> {
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| item(&e.path(), reason, selected))
        .collect()
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
    let mut categories = Vec::new();
    if let Some(cache) = cache_root() {
        report(&cache.to_string_lossy(), 0);
        let browser_roots: Vec<PathBuf> =
            browser_names().iter().map(|(_, p)| cache.join(p)).collect();
        let developer_roots = platform::home()
            .map(|home| crate::developer::cache_roots(&home, &cache))
            .unwrap_or_default();
        let app_items = children(&cache, "应用运行缓存；请先退出相关应用", false)
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
                    && !developer_roots.iter().any(|root| {
                        root.starts_with(&i.path) || Path::new(&i.path).starts_with(root)
                    })
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
                let mut i = item(
                    &cache.join(p),
                    "网页资源缓存；不包含密码、Cookie 和浏览记录",
                    false,
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
            children(
                &home.join("Library/Logs"),
                "运行日志，清理后无法用于历史故障排查",
                false,
            ),
        ));
    }
    if let Some(trash) = trash_root() {
        categories.push(category(
            "trash",
            "废纸篓",
            "清空后无法恢复，请检查所选文件",
            children(&trash, "已移入废纸篓的文件，将彻底删除", false),
        ));
    }
    report(
        "开发工具历史缓存",
        categories.iter().map(|c| c.total_size_bytes).sum(),
    );
    let developer_category = build_developer_category(None, 7, |_| {}).unwrap_or_else(|error| {
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
    categories.extend(scan_system_categories());
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

fn scan_files(names: &[&str], depth: usize) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    for root in names.iter().filter_map(|name| user_folder(name)) {
        for entry in WalkDir::new(root)
            .max_depth(depth)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                e.depth() == 0
                    || (!name.starts_with('.')
                        && name != "node_modules"
                        && name != "target"
                        && !name.ends_with(".app"))
            })
            .flatten()
        {
            if entry.file_type().is_file() {
                seen.insert(entry.path().to_path_buf());
            }
        }
    }
    let mut paths: Vec<_> = seen.into_iter().collect();
    paths.sort();
    paths
}

pub fn scan_large_files(min_mb: u64) -> Vec<LargeFileItem> {
    let min_bytes = min_mb.max(1).saturating_mul(1024 * 1024);
    let mut list: Vec<_> = scan_files(&["Downloads", "Documents", "Videos", "Desktop"], 6)
        .iter()
        .filter_map(|path| {
            let size = path.metadata().ok()?.len();
            if size < min_bytes {
                return None;
            }
            Some(LargeFileItem {
                path: path.to_string_lossy().into_owned(),
                name: path.file_name()?.to_string_lossy().into_owned(),
                size_bytes: size,
                size_str: format_size(size),
                ext: path
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            })
        })
        .collect();
    list.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    if let Ok(mut receipt) = LARGE_FILES.lock() {
        *receipt = list
            .iter()
            .filter_map(|i| {
                let path = fs::canonicalize(&i.path).ok()?;
                let meta = path.metadata().ok()?;
                Some((path, (meta.len(), meta.modified().ok()?)))
            })
            .collect();
    }
    list
}

pub fn scan_duplicate_files() -> Vec<DuplicateGroup> {
    let mut groups = Vec::new();
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for path in scan_files(&["Downloads", "Documents", "Pictures"], 5) {
        if let Ok(m) = path.metadata()
            && m.len() > 64 * 1024
        {
            size_map.entry(m.len()).or_default().push(path);
        }
    }
    for (size, paths) in size_map {
        if paths.len() < 2 {
            continue;
        }
        let mut hashes: HashMap<String, Vec<String>> = HashMap::new();
        for path in paths {
            if let Ok(hash) = compute_file_sha256(&path) {
                hashes
                    .entry(hash)
                    .or_default()
                    .push(path.to_string_lossy().into_owned());
            }
        }
        for (hash, files) in hashes {
            if files.len() > 1 {
                groups.push(DuplicateGroup {
                    hash,
                    size_bytes: size,
                    size_str: format_size(size),
                    files,
                });
            }
        }
    }
    groups.sort_by(|a, b| {
        b.size_bytes
            .cmp(&a.size_bytes)
            .then_with(|| a.hash.cmp(&b.hash))
    });
    if let Ok(mut receipt) = DUPLICATES.lock() {
        *receipt = groups.clone();
    }
    groups
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

pub fn trash_scanned_files(paths: Vec<String>, kind: &str) -> Result<usize, String> {
    let selected: HashSet<PathBuf> = paths.iter().map(PathBuf::from).collect();
    if selected.is_empty() {
        return Ok(0);
    }
    for path in &selected {
        if !fs::symlink_metadata(path)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("只允许处理扫描到的普通文件".into());
        }
    }
    match kind {
        "large" => {
            let receipt = LARGE_FILES.lock().map_err(|e| e.to_string())?;
            for path in &selected {
                let canonical = fs::canonicalize(path).map_err(|e| e.to_string())?;
                let metadata = path.metadata().map_err(|e| e.to_string())?;
                if receipt.get(&canonical)
                    != Some(&(
                        metadata.len(),
                        metadata.modified().map_err(|e| e.to_string())?,
                    ))
                {
                    return Err("文件已改变或不在扫描结果中，请重新扫描".into());
                }
            }
        }
        "duplicates" => {
            let receipt = DUPLICATES.lock().map_err(|e| e.to_string())?;
            validate_duplicate_selection(&selected, &receipt)?;
        }
        _ => return Err("无效的扫描类型".into()),
    }
    let mut count = 0;
    for path in selected {
        let result = if cfg!(target_os = "macos") {
            platform::checked_output(std::process::Command::new("/usr/bin/osascript").args(["-e", "on run argv\ntell application \"Finder\" to delete POSIX file (item 1 of argv)\nend run"]).arg(&path))
        } else {
            let gio = platform::command_path("gio").ok_or("系统未提供回收站工具 gio")?;
            platform::checked_output(
                std::process::Command::new(gio)
                    .args(["trash", "--"])
                    .arg(&path),
            )
        };
        result.map_err(|e| format!("已移入废纸篓 {count} 个文件；{}: {e}", path.display()))?;
        count += 1;
    }
    Ok(count)
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

pub fn delete_clean_items(paths: Vec<String>) -> Result<usize, String> {
    let receipt = SCANNED_JUNK.lock().map_err(|e| e.to_string())?;
    let expanded = expand_developer_groups(&paths, &receipt.developer_groups)?;
    let (actions, files): (Vec<_>, Vec<_>) =
        expanded.into_iter().partition(|p| p.starts_with("system:"));
    let ids: HashSet<_> = actions
        .iter()
        .map(|p| p.strip_prefix("system:").unwrap_or_default().to_string())
        .collect();
    let mut plans = HashMap::new();
    for id in ids {
        let fingerprint = receipt
            .actions
            .get(&id)
            .ok_or("系统清理范围已改变，请重新扫描")?;
        plans.insert(id, fingerprint.clone());
    }
    let validated = validated_cleanup_paths(&files, &receipt.paths)?;
    for path in &validated {
        if let Some((days, fingerprint)) = receipt.developer.get(path) {
            crate::developer::revalidate(path, *days, fingerprint)?;
        }
    }
    let mut count = 0;
    if !plans.is_empty() {
        crate::privileged::clean_system(&plans)?;
        count += plans.len();
    }
    for path in validated {
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        result.map_err(|e| format!("已清理 {count} 项；无法清理 {}: {e}", path.display()))?;
        count += 1;
    }
    Ok(count)
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
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub(crate) fn get_dir_or_file_size(path: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    for entry in WalkDir::new(path) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file() {
            total = total.saturating_add(entry.metadata().map_err(|e| e.to_string())?.len());
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
    if items.len() > 200 {
        let mut by_name: HashMap<String, Vec<CleanItem>> = HashMap::new();
        for item in items {
            by_name.entry(item.name.clone()).or_default().push(item);
        }
        items = Vec::new();
        for (name, targets) in by_name {
            let mut files: Vec<_> = targets
                .iter()
                .map(|target| crate::models::CleanFileDetail {
                    name: Path::new(&target.path)
                        .parent()
                        .and_then(Path::file_name)
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
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
            items.push(CleanItem{id:key.clone(),path:key.clone(),name:format!("{name}（{count} 项）"),size_bytes:bytes,size_str:format_size(bytes),reason:targets[0].reason.clone(),can_delete:true,selected:false,size_known:true,action:None,details:Some(format!("共 {count} 个已统计的文件或目录。勾选此分类会清理全部条目；展开后可单独选择文件。保留源码、锁文件和工具链。")),files:Vec::new()});
            groups.insert(key, files);
        }
    } else {
        for item in &mut items {
            item.name = format!(
                "{} · {}",
                item.name,
                Path::new(&item.path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            );
        }
    }
    (items, receipt, groups)
}
fn build_developer_category(
    root: Option<&str>,
    days: u64,
    report: impl FnMut(crate::developer::DevProgress),
) -> Result<(CleanCategory, DeveloperReceipt, DeveloperGroups), String> {
    let home = platform::home().ok_or("无法找到用户目录")?;
    let cache = cache_root().ok_or("无法找到缓存目录")?;
    let scan =
        crate::developer::scan_with_progress(&home, &cache, root.map(Path::new), days, report)?;
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
    let (category, developer, groups) = build_developer_category(root, days, report)?;
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
        assert!(items[0].files.is_empty());
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
        assert!(delete_clean_items(vec!["system:docker-usage".into()]).is_err());
        assert!(delete_clean_items(vec!["system:system:apt".into()]).is_err());
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
