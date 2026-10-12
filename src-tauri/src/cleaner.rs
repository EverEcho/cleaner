use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use sha2::{Digest, Sha256};
use crate::models::{CleanCategory, CleanItem, LargeFileItem, DuplicateGroup, format_size};

/// 快速系统与垃圾扫描
pub fn scan_system_junk() -> Vec<CleanCategory> {
    let mut categories = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    // 1. 软件应用缓存 (如 ~/.cache)
    let mut cache_items = Vec::new();
    let mut cache_total_bytes = 0u64;

    if !home.is_empty() {
        let cache_dir = Path::new(&home).join(".cache");
        if cache_dir.is_dir() {
            if let Ok(entries) = fs::read_dir(&cache_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let size = get_dir_or_file_size(&path);
                    if size > 1024 * 1024 { // 大于 1MB
                        cache_total_bytes += size;
                        cache_items.push(CleanItem {
                            id: format!("cache_{}", name),
                            path: path.to_string_lossy().to_string(),
                            name: format!("缓存: {}", name),
                            size_bytes: size,
                            size_str: format_size(size),
                            reason: "用户级应用运行时产生的临时缓存数据，可安全清理".to_string(),
                            can_delete: true,
                            selected: true,
                        });
                    }
                }
            }
        }
    }

    cache_items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let cache_count = cache_items.len();
    categories.push(CleanCategory {
        id: "app_cache".to_string(),
        title: "应用临时缓存".to_string(),
        desc: "各应用程序运行过程中积累的临时缓存与日志".to_string(),
        icon: "folder-delete".to_string(),
        total_size_bytes: cache_total_bytes,
        total_size_str: format_size(cache_total_bytes),
        count: cache_count,
        items: cache_items,
    });

    // 2. 包管理器缓存 (Apt / Flatpak / Snap 缓存)
    let mut pkg_items = Vec::new();
    let mut pkg_total_bytes = 0u64;

    let apt_archives = Path::new("/var/cache/apt/archives");
    if apt_archives.is_dir() {
        let size = get_dir_or_file_size(apt_archives);
        if size > 0 {
            pkg_total_bytes += size;
            pkg_items.push(CleanItem {
                id: "apt_cache".to_string(),
                path: apt_archives.to_string_lossy().to_string(),
                name: "APT 下载包缓存 (/var/cache/apt/archives)".to_string(),
                size_bytes: size,
                size_str: format_size(size),
                reason: "apt 下载遗留的 .deb 安装包文件".to_string(),
                can_delete: true,
                selected: true,
            });
        }
    }

    let pkg_count = pkg_items.len();
    categories.push(CleanCategory {
        id: "pkg_cache".to_string(),
        title: "安装包与分发缓存".to_string(),
        desc: "系统软件源下载后残留的安装包文件".to_string(),
        icon: "box".to_string(),
        total_size_bytes: pkg_total_bytes,
        total_size_str: format_size(pkg_total_bytes),
        count: pkg_count,
        items: pkg_items,
    });

    // 3. 回收站垃圾
    let mut trash_items = Vec::new();
    let mut trash_total_bytes = 0u64;

    if !home.is_empty() {
        let trash_files = Path::new(&home).join(".local/share/Trash/files");
        if trash_files.is_dir() {
            if let Ok(entries) = fs::read_dir(&trash_files) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    let size = get_dir_or_file_size(&path);
                    trash_total_bytes += size;
                    trash_items.push(CleanItem {
                        id: format!("trash_{}", name),
                        path: path.to_string_lossy().to_string(),
                        name,
                        size_bytes: size,
                        size_str: format_size(size),
                        reason: "已移入回收站的文件".to_string(),
                        can_delete: true,
                        selected: true,
                    });
                }
            }
        }
    }

    let trash_count = trash_items.len();
    categories.push(CleanCategory {
        id: "trash".to_string(),
        title: "回收站残留".to_string(),
        desc: "桌面与文件管理器废纸篓中等待彻底删除的文件".to_string(),
        icon: "trash".to_string(),
        total_size_bytes: trash_total_bytes,
        total_size_str: format_size(trash_total_bytes),
        count: trash_count,
        items: trash_items,
    });

    // 4. 浏览器缓存与隐私数据 (如 Chrome / Firefox / Edge)
    let mut browser_items = Vec::new();
    let mut browser_total = 0u64;
    if !home.is_empty() {
        let browsers = [
            ("Google Chrome 缓存", format!("{}/.cache/google-chrome", home)),
            ("Mozilla Firefox 缓存", format!("{}/.cache/mozilla", home)),
            ("Microsoft Edge 缓存", format!("{}/.cache/microsoft-edge", home)),
            ("Chromium 缓存", format!("{}/.cache/chromium", home)),
        ];
        for (bname, bpath_str) in browsers {
            let bp = Path::new(&bpath_str);
            if bp.is_dir() {
                let size = get_dir_or_file_size(bp);
                if size > 0 {
                    browser_total += size;
                    browser_items.push(CleanItem {
                        id: format!("bcache_{}", bname),
                        path: bpath_str,
                        name: bname.to_string(),
                        size_bytes: size,
                        size_str: format_size(size),
                        reason: "浏览器网页历史、资源与媒体缓存".to_string(),
                        can_delete: true,
                        selected: true,
                    });
                }
            }
        }
    }

    let browser_count = browser_items.len();
    categories.push(CleanCategory {
        id: "browser_cache".to_string(),
        title: "浏览器缓存与隐私".to_string(),
        desc: "主流浏览器访问网页留存的资源缓存".to_string(),
        icon: "globe".to_string(),
        total_size_bytes: browser_total,
        total_size_str: format_size(browser_total),
        count: browser_count,
        items: browser_items,
    });

    // 5. 失效启动器与死链
    let mut deadlink_items = Vec::new();
    let entries = crate::entries::scan_desktop_entries();
    for e in entries {
        let exec_bin = e.exec.split_whitespace().next().unwrap_or("");
        if !exec_bin.is_empty() && !exec_bin.starts_with('/') {
            // Check in PATH or /usr/bin
            // If it's a direct missing binary
            if which_exists(exec_bin).is_none() && !exec_bin.contains('=') {
                deadlink_items.push(CleanItem {
                    id: format!("dead_{}", e.basename),
                    path: e.path.clone(),
                    name: format!("失效启动器: {}", e.name),
                    size_bytes: 1024,
                    size_str: "1 KB".to_string(),
                    reason: format!("启动命令 '{}' 在系统中已不存在", e.exec),
                    can_delete: true,
                    selected: true,
                });
            }
        }
    }

    let dead_count = deadlink_items.len();
    categories.push(CleanCategory {
        id: "dead_links".to_string(),
        title: "残留失效图标与死链".to_string(),
        desc: "对应软件卸载后遗留在菜单里的孤立启动图标".to_string(),
        icon: "link-2-off".to_string(),
        total_size_bytes: (dead_count as u64) * 1024,
        total_size_str: format_size((dead_count as u64) * 1024),
        count: dead_count,
        items: deadlink_items,
    });

    categories
}

/// 扫描大于指定大小的文件 (参考 Lemon Cleaner 算法：默认 50MB)
pub fn scan_large_files(min_mb: u64) -> Vec<LargeFileItem> {
    let mut list = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return list;
    }

    let min_bytes = min_mb * 1024 * 1024;
    let scan_roots = [
        format!("{}/Downloads", home),
        format!("{}/Documents", home),
        format!("{}/Videos", home),
        format!("{}/Desktop", home),
    ];

    for root in scan_roots {
        let p = Path::new(&root);
        if !p.exists() {
            continue;
        }

        for entry in WalkDir::new(p)
            .max_depth(6)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.') && name != "node_modules" && name != "target"
            })
            .flatten()
        {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    let size = meta.len();
                    if size >= min_bytes {
                        let path = entry.path();
                        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_string();

                        list.push(LargeFileItem {
                            path: path.to_string_lossy().to_string(),
                            name,
                            size_bytes: size,
                            size_str: format_size(size),
                            ext,
                        });
                    }
                }
            }
        }
    }

    list.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    list
}

/// 重复文件扫描 (基于 Lemon Cleaner 算法：先按文件大小分组，相同大小再计算 SHA256 哈希)
pub fn scan_duplicate_files() -> Vec<DuplicateGroup> {
    let mut groups = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return groups;
    }

    let scan_roots = [
        format!("{}/Downloads", home),
        format!("{}/Documents", home),
        format!("{}/Pictures", home),
    ];

    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    // 步骤 1：收集常规文件并按文件体积粗筛分组（排除空文件与小于 64KB 的极小文件）
    for root in scan_roots {
        let p = Path::new(&root);
        if !p.exists() {
            continue;
        }

        for entry in WalkDir::new(p)
            .max_depth(5)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .flatten()
        {
            if entry.file_type().is_file() {
                if let Ok(meta) = entry.metadata() {
                    let len = meta.len();
                    if len > 64 * 1024 { // 大于 64KB
                        size_map.entry(len).or_default().push(entry.path().to_path_buf());
                    }
                }
            }
        }
    }

    // 步骤 2：对体积相同的一组计算 Sha256 精确对比
    for (size, paths) in size_map {
        if paths.len() < 2 {
            continue;
        }

        let mut hash_map: HashMap<String, Vec<String>> = HashMap::new();
        for p in paths {
            if let Ok(h) = compute_file_sha256(&p) {
                hash_map.entry(h).or_default().push(p.to_string_lossy().to_string());
            }
        }

        for (hash, files) in hash_map {
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

    groups.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    groups
}

pub fn delete_clean_items(paths: Vec<String>) -> Result<usize, String> {
    let mut count = 0;
    for path_str in paths {
        let p = Path::new(&path_str);
        if p.exists() {
            if p.is_dir() {
                let _ = fs::remove_dir_all(p);
            } else {
                let _ = fs::remove_file(p);
            }
            count += 1;
        }
    }
    Ok(count)
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

fn get_dir_or_file_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0;
    for entry in WalkDir::new(path).into_iter().flatten() {
        if entry.file_type().is_file() {
            if let Ok(m) = entry.metadata() {
                total += m.len();
            }
        }
    }
    total
}

fn which_exists(binary: &str) -> Option<PathBuf> {
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(':') {
            let candidate = Path::new(dir).join(binary);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
