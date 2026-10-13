//! Inventory from launchers; never execute a launcher's Exec or uninstall strings.
use crate::{
    desktop,
    models::{AppInfo, format_size},
    platform,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::SystemTime,
};
use walkdir::WalkDir;

#[derive(Clone, Debug)]
pub struct Launcher {
    pub path: PathBuf,
    pub name: String,
    pub icon: String,
    pub exec: String,
    pub comment: String,
}

pub fn launcher_roots() -> Vec<PathBuf> {
    let mut roots: Vec<_> = desktop::application_dirs()
        .into_iter()
        .map(|(path, _)| path)
        .collect();
    if let Some(home) = platform::home() {
        let desktop = platform::xdg_dir("XDG_CONFIG_HOME", ".config")
            .and_then(|config| fs::read_to_string(config.join("user-dirs.dirs")).ok())
            .and_then(|text| {
                text.lines().find_map(|line| {
                    line.strip_prefix("XDG_DESKTOP_DIR=")
                        .map(|value| value.trim().trim_matches('"').to_string())
                })
            })
            .map(|value| PathBuf::from(value.replace("$HOME", &home.to_string_lossy())))
            .unwrap_or_else(|| home.join("Desktop"));
        if desktop.is_absolute() && desktop != home {
            roots.push(desktop);
        }
    }
    roots
}

pub fn launchers_in(roots: &[PathBuf]) -> Vec<Launcher> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .max_depth(3)
            .sort_by_file_name()
            .into_iter()
            .flatten()
        {
            let path = entry.path();
            if path.extension().is_none_or(|s| s != "desktop") || !path.is_file() {
                continue;
            }
            let id = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('/', "-");
            // XDG user overrides (including Hidden=true) take precedence over system copies.
            if !seen.insert(id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            let fields = desktop::fields(&text);
            if fields.get("Type") != Some(&"Application")
                || fields.get("Hidden") == Some(&"true")
                || fields.get("NoDisplay") == Some(&"true")
            {
                continue;
            }
            let Some(name) = fields.get("Name").filter(|name| !name.is_empty()) else {
                continue;
            };
            result.push(Launcher {
                path: path.to_path_buf(),
                name: (*name).into(),
                icon: fields.get("Icon").copied().unwrap_or("").into(),
                exec: fields.get("Exec").copied().unwrap_or("").into(),
                comment: fields.get("Comment").copied().unwrap_or("").into(),
            });
        }
    }
    result
}

fn exec_words(value: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            '\\' => {
                word.push(chars.next()?);
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            _ => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if started {
        words.push(word);
    }
    Some(words)
}

pub fn executable(exec: &str) -> Option<PathBuf> {
    let words = exec_words(exec)?;
    let mut index = 0;
    if Path::new(words.first()?)
        .file_name()
        .is_some_and(|name| name == "env")
    {
        index = 1;
        while let Some(word) = words.get(index) {
            if word == "--" {
                index += 1;
                break;
            }
            if word == "-u" || word == "--unset" {
                index += 2;
                continue;
            }
            if word.contains('=') && !word.starts_with('/') || word.starts_with("--unset=") {
                index += 1;
                continue;
            }
            if word.starts_with('-') {
                return None;
            }
            break;
        }
    }
    let program = words.get(index)?;
    if program.contains(['%', '$', '`', '=', '\n']) {
        return None;
    }
    let path = if Path::new(program).is_absolute() {
        PathBuf::from(program)
    } else {
        platform::command_path(program)?
    };
    let path = fs::canonicalize(path).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(&path).ok()?.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    path.is_file().then_some(path)
}

#[derive(Clone)]
struct ManualApp {
    app: AppInfo,
    executable: PathBuf,
    target: Option<PathBuf>,
    toolbox: bool,
}
static APPS: LazyLock<Mutex<HashMap<String, ManualApp>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn user_path(path: &Path, home: &Path) -> bool {
    path.starts_with(home)
        && path
            .strip_prefix(home)
            .is_ok_and(|suffix| suffix.components().count() >= 2)
        && fs::canonicalize(home).is_ok_and(|h| path.starts_with(h))
}
fn project_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        [
            "Projects",
            "projects",
            "workspace",
            "Workspace",
            "src",
            "source",
        ]
        .iter()
        .any(|value| name == *value)
    }) || [
        ".git",
        "Cargo.toml",
        "package.json",
        "pom.xml",
        "pyproject.toml",
        "pubspec.yaml",
    ]
    .iter()
    .any(|name| path.join(name).exists())
}
fn ide_info(path: &Path) -> Option<Value> {
    if project_directory(path)
        || ["Projects", "projects", "workspace"]
            .iter()
            .any(|name| path.join(name).exists())
    {
        return None;
    }
    let value: Value =
        serde_json::from_slice(&fs::read(path.join("product-info.json")).ok()?).ok()?;
    let code = value["productCode"].as_str()?;
    if ![
        "IU", "IC", "AI", "PY", "PC", "WS", "GO", "CL", "RD", "RM", "PS", "DB", "RR",
    ]
    .contains(&code)
    {
        return None;
    }
    if !path.join("bin").is_dir() || !path.join("lib").is_dir() {
        return None;
    }
    let root = fs::canonicalize(path).ok()?;
    if !value["launch"].as_array().is_some_and(|rows| {
        rows.iter()
            .filter_map(|row| row["launcherPath"].as_str())
            .any(|launcher| {
                fs::canonicalize(path.join(launcher))
                    .is_ok_and(|binary| binary.starts_with(&root) && binary.is_file())
            })
    }) {
        return None;
    }
    Some(value)
}
fn electron_info(root: &Path, binary: &Path) -> Option<Value> {
    if project_directory(root)
        || ["Projects", "projects", "workspace", "data"]
            .iter()
            .any(|name| root.join(name).is_dir())
    {
        return None;
    }
    if binary.parent()? != root {
        return None;
    }
    let resources = fs::canonicalize(root.join("resources/app")).ok()?;
    if !resources.starts_with(root) {
        return None;
    }
    let read_resource = |name: &str| -> Option<Vec<u8>> {
        let path = fs::canonicalize(resources.join(name)).ok()?;
        if !path.starts_with(&resources) || fs::metadata(&path).ok()?.len() > 1024 * 1024 {
            return None;
        }
        fs::read(path).ok()
    };
    let product: Value = serde_json::from_slice(&read_resource("product.json")?).ok()?;
    let package: Value = serde_json::from_slice(&read_resource("package.json")?).ok()?;
    let application = product["applicationName"].as_str()?;
    if application.is_empty()
        || !application
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ['-', '_'].contains(&c))
        || binary.file_name()? != application
    {
        return None;
    }
    let name = package["name"].as_str()?;
    if ![product["nameShort"].as_str(), product["nameLong"].as_str()].contains(&Some(name)) {
        return None;
    }
    let main = Path::new(package["main"].as_str()?);
    if main.is_absolute()
        || main
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return None;
    }
    let entry = fs::canonicalize(resources.join(main)).ok()?;
    let resources = fs::canonicalize(resources).ok()?;
    if !entry.starts_with(&resources) || !entry.is_file() {
        return None;
    }
    if !root.join("resources.pak").is_file()
        || !root.join("icudtl.dat").is_file()
        || !root.join("locales").is_dir()
    {
        return None;
    }
    // A checkout or wrapper script is not enough evidence for a distributable application.
    use std::io::Read;
    let mut magic = [0u8; 4];
    fs::File::open(binary).ok()?.read_exact(&mut magic).ok()?;
    if magic != *b"\x7fELF" {
        return None;
    }
    Some(product)
}

fn target_for(binary: &Path, home: &Path) -> (Option<PathBuf>, bool) {
    if !user_path(binary, home) {
        return (None, false);
    }
    if binary
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("AppImage"))
    {
        return (Some(binary.to_path_buf()), false);
    }
    let Some(bin) = binary.parent() else {
        return (None, false);
    };
    if binary.file_name().is_some_and(|s| s == "jetbrains-toolbox")
        && bin.file_name().is_some_and(|s| s == "bin")
        && bin.join("jetbrains-toolbox.desktop").is_file()
        && bin.join("lib/toolbox-core.jar").is_file()
    {
        return (Some(bin.to_path_buf()), true); // Parent may also contain IDEs and user scripts.
    }
    if bin.file_name().is_some_and(|s| s == "bin")
        && let Some(root) = bin.parent()
        && let Some(info) = ide_info(root)
    {
        let listed = info["launch"].as_array().is_some_and(|rows| {
            rows.iter()
                .filter_map(|row| row["launcherPath"].as_str())
                .any(|launcher| fs::canonicalize(root.join(launcher)).is_ok_and(|p| p == binary))
        });
        if listed && user_path(root, home) {
            return (Some(root.to_path_buf()), false);
        }
    }
    if electron_info(bin, binary).is_some() {
        return (Some(bin.to_path_buf()), false);
    }
    (None, false)
}

pub fn discover(
    launchers: &[Launcher],
    owned: &HashSet<PathBuf>,
    represented: &HashSet<PathBuf>,
) -> Vec<AppInfo> {
    let Some(home) = platform::home().and_then(|home| fs::canonicalize(home).ok()) else {
        return Vec::new();
    };
    let mut seen = represented.clone();
    let mut records = HashMap::new();
    let mut result = Vec::new();
    for launcher in launchers {
        if owned.contains(&launcher.path) {
            continue;
        }
        let Some(binary) = executable(&launcher.exec) else {
            continue;
        };
        if !seen.insert(binary.clone()) {
            continue;
        }
        let (target, toolbox) = target_for(&binary, &home);
        let size = target
            .as_ref()
            .and_then(|p| crate::cleaner::get_dir_or_file_size(p).ok());
        let note = if target.is_some() {
            "将展示程序、自启与已确认的残留路径，确认后移入废纸篓"
        } else {
            "已通过启动入口识别；无法确认独立安装目录，请使用应用自带卸载方式"
        };
        let app = AppInfo {
            key: launcher.path.to_string_lossy().into_owned(),
            name: launcher.name.clone(),
            pkg_type: "manual".into(),
            version: String::new(),
            size: size.map(format_size).unwrap_or_else(|| "—".into()),
            size_bytes: size.unwrap_or(0),
            install_time: String::new(),
            description: launcher.comment.clone(),
            icon: launcher.icon.clone(),
            desktop_file: launcher.path.to_string_lossy().into_owned(),
            is_system: false,
            has_ui: true,
            can_uninstall: target.is_some(),
            install_path: binary.to_string_lossy().into_owned(),
            uninstall_note: note.into(),
        };
        records.insert(
            app.key.clone(),
            ManualApp {
                app: app.clone(),
                executable: binary,
                target,
                toolbox,
            },
        );
        result.push(app);
    }
    if let Ok(mut inventory) = APPS.lock() {
        *inventory = records;
    }
    result
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UninstallItem {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size_bytes: u64,
    pub size: String,
    pub selected: bool,
    pub required: bool,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UninstallPlan {
    pub id: String,
    pub name: String,
    pub note: String,
    pub items: Vec<UninstallItem>,
}
#[derive(Clone)]
struct Receipt {
    plan: UninstallPlan,
    app: ManualApp,
    fingerprints: HashMap<PathBuf, String>,
    completed: HashSet<PathBuf>,
}
static PLANS: LazyLock<Mutex<HashMap<String, Receipt>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn append(
    items: &mut Vec<UninstallItem>,
    home: &Path,
    path: &Path,
    name: &str,
    kind: &str,
    required: bool,
) -> Result<(), String> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() && project_directory(path) {
        if required {
            return Err(format!(
                "安装目录内检测到工程源码，不能自动卸载：{}",
                path.display()
            ));
        }
        return Ok(());
    }
    // Preserve symlink targets; only known launcher/script links can be recycled themselves.
    if meta.file_type().is_symlink() && !["launcher", "autostart", "script"].contains(&kind) {
        return Ok(());
    }
    let parent = fs::canonicalize(path.parent().ok_or("路径无效")?).map_err(|e| e.to_string())?;
    let normalized = parent.join(path.file_name().ok_or("路径无效")?);
    if !user_path(&normalized, home) {
        return Ok(());
    }
    if let Some(existing) = items
        .iter_mut()
        .find(|item| Path::new(&item.path) == normalized)
    {
        existing.required |= required;
        return Ok(());
    }
    let bytes = if meta.file_type().is_symlink() {
        meta.len()
    } else {
        crate::cleaner::get_dir_or_file_size(&normalized)?
    };
    items.push(UninstallItem {
        path: normalized.to_string_lossy().into_owned(),
        name: name.into(),
        kind: kind.into(),
        size_bytes: bytes,
        size: format_size(bytes),
        selected: true,
        required,
    });
    Ok(())
}

fn toolbox_ides(home: &Path, data: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut registered_codes = HashSet::new();
    if let Ok(bytes) = fs::read(data.join("state.json"))
        && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
        && let Some(tools) = state["tools"].as_array()
    {
        for tool in tools {
            if let Some(path) = tool["installLocation"].as_str()
                && let Ok(path) = fs::canonicalize(path)
                && user_path(&path, home)
                && let Some(info) = ide_info(&path)
                && tool["productCode"] == info["productCode"]
            {
                if let Some(code) = info["productCode"].as_str() {
                    registered_codes.insert(code.to_string());
                }
                candidates.push(path);
            }
        }
    }
    // Include older versions only inside Toolbox's explicitly configured install location.
    let settings = fs::read(data.join(".settings.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
    let mut roots = vec![data.join("apps")];
    if let Some(path) = settings
        .as_ref()
        .and_then(|s| s["install_location"].as_str())
    {
        roots.push(PathBuf::from(path));
    }
    for root in roots {
        let Ok(root) = fs::canonicalize(root) else {
            continue;
        };
        if !user_path(&root, home) {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(4)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !["bin", "lib", "plugins", "jbr", ".git", "node_modules"]
                        .contains(&entry.file_name().to_string_lossy().as_ref())
            })
            .flatten()
        {
            if entry.file_type().is_dir()
                && ide_info(entry.path()).is_some_and(|info| {
                    info["productCode"]
                        .as_str()
                        .is_some_and(|code| registered_codes.contains(code))
                })
            {
                candidates.push(entry.path().to_path_buf());
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn ide_residuals(items: &mut Vec<UninstallItem>, home: &Path, root: &Path) -> Result<(), String> {
    let Some(info) = ide_info(root) else {
        return Ok(());
    };
    let Some(dir) = info["dataDirectoryName"].as_str().filter(|value| {
        !value.is_empty()
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ['.', '-', '_'].contains(&c))
    }) else {
        return Ok(());
    };
    let vendor = if info["productCode"] == "AI" {
        "Google"
    } else {
        "JetBrains"
    };
    for (env, fallback, kind) in [
        ("XDG_CONFIG_HOME", ".config", "config"),
        ("XDG_CACHE_HOME", ".cache", "cache"),
        ("XDG_DATA_HOME", ".local/share", "config"),
    ] {
        let base = platform::xdg_dir(env, fallback).unwrap_or_else(|| home.join(fallback));
        append(
            items,
            home,
            &base.join(vendor).join(dir),
            &format!("{dir} 配置/缓存"),
            kind,
            false,
        )?;
    }
    Ok(())
}

fn electron_residuals(
    items: &mut Vec<UninstallItem>,
    home: &Path,
    root: &Path,
    binary: &Path,
) -> Result<(), String> {
    let Some(product) = electron_info(root, binary) else {
        return Ok(());
    };
    let Some(name) = product["nameShort"].as_str().filter(|name| {
        !name.is_empty()
            && ![".", ".."].contains(name)
            && !name.contains(['/', '\\'])
            && !name.chars().any(char::is_control)
    }) else {
        return Ok(());
    };
    let profile = platform::xdg_dir("XDG_CONFIG_HOME", ".config")
        .unwrap_or_else(|| home.join(".config"))
        .join(name);
    if profile.join("User").is_dir() {
        // Do not infer sibling profiles from a similar name, or remove unsaved source/history/workspace state.
        for cache in [
            "Cache",
            "Code Cache",
            "GPUCache",
            "DawnWebGPUCache",
            "DawnGraphiteCache",
            "CachedData",
            "CachedExtensionVSIXs",
            "logs",
            "Crashpad",
        ] {
            append(
                items,
                home,
                &profile.join(cache),
                &format!("{name} {cache}"),
                "cache",
                false,
            )?;
        }
        for config in ["User/settings.json", "User/keybindings.json"] {
            let before = items.len();
            append(
                items,
                home,
                &profile.join(config),
                &format!("{name} 配置（可选）"),
                "config",
                false,
            )?;
            for item in &mut items[before..] {
                item.selected = false;
            }
        }
    }
    if let Some(cache) = platform::xdg_dir("XDG_CACHE_HOME", ".cache") {
        append(
            items,
            home,
            &cache.join(name),
            &format!("{name} 缓存"),
            "cache",
            false,
        )?;
    }
    Ok(())
}

fn scoped_links(
    items: &mut Vec<UninstallItem>,
    home: &Path,
    targets: &[PathBuf],
) -> Result<(), String> {
    let mut roots = launcher_roots();
    roots.push(
        platform::xdg_dir("XDG_CONFIG_HOME", ".config")
            .unwrap_or_else(|| home.join(".config"))
            .join("autostart"),
    );
    for root in roots {
        for entry in WalkDir::new(&root).max_depth(3).into_iter().flatten() {
            if entry.path().extension().is_none_or(|s| s != "desktop") {
                continue;
            }
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            let fields = desktop::fields(&text);
            let Some(binary) = fields.get("Exec").and_then(|exec| executable(exec)) else {
                continue;
            };
            if targets.iter().any(|target| binary.starts_with(target)) {
                append(
                    items,
                    home,
                    entry.path(),
                    "桌面/菜单或自启入口",
                    if root.file_name().is_some_and(|s| s == "autostart") {
                        "autostart"
                    } else {
                        "launcher"
                    },
                    false,
                )?;
            }
        }
    }
    Ok(())
}

fn append_empty(items: &mut Vec<UninstallItem>, home: &Path, path: &Path) -> Result<(), String> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Ok(());
    }
    let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !user_path(&path, home) || items.iter().any(|item| Path::new(&item.path) == path) {
        return Ok(());
    }
    items.push(UninstallItem {
        path: path.to_string_lossy().into_owned(),
        name: "安装容器目录（卸载后仅为空时清理）".into(),
        kind: "empty".into(),
        size_bytes: 0,
        size: "仅清空目录".into(),
        selected: true,
        required: false,
    });
    Ok(())
}
fn snapshot(item: &UninstallItem) -> Result<String, String> {
    let path = Path::new(&item.path);
    if item.kind != "empty" {
        return fingerprint(path);
    }
    if fs::canonicalize(path).map_err(|e| e.to_string())? != path {
        return Err("容器目录路径已改变".into());
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("容器目录已改变".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(format!("directory:{}:{}", meta.dev(), meta.ino()))
    }
    #[cfg(not(unix))]
    {
        Ok(format!("directory:{:?}", meta.created()))
    }
}

pub fn plan(key: &str) -> Result<UninstallPlan, String> {
    let app = APPS
        .lock()
        .map_err(|e| e.to_string())?
        .get(key)
        .cloned()
        .ok_or("应用不在最新扫描列表中，请刷新")?;
    let home = platform::home()
        .and_then(|h| fs::canonicalize(h).ok())
        .ok_or("无法读取用户目录")?;
    let target = app.target.as_ref().ok_or(&app.app.uninstall_note)?;
    if executable(
        desktop::fields(&fs::read_to_string(key).map_err(|e| e.to_string())?)
            .get("Exec")
            .copied()
            .unwrap_or(""),
    ) != Some(app.executable.clone())
    {
        return Err("启动入口已改变，请刷新应用列表".into());
    }
    let current = target_for(&app.executable, &home);
    if current.0.as_ref() != Some(target) || current.1 != app.toolbox {
        return Err("应用安装结构已改变，请刷新应用列表后再查看卸载详情".into());
    }
    let mut items = Vec::new();
    append(&mut items, &home, target, "应用程序", "program", true)?;
    let mut targets = vec![target.clone()];
    if app.toolbox {
        let data = platform::xdg_dir("XDG_DATA_HOME", ".local/share")
            .unwrap_or_else(|| home.join(".local/share"))
            .join("JetBrains/Toolbox");
        for ide in toolbox_ides(&home, &data) {
            let info = ide_info(&ide).unwrap_or(Value::Null);
            append(
                &mut items,
                &home,
                &ide,
                info["name"].as_str().unwrap_or("Toolbox 管理的 IDE"),
                "ide",
                false,
            )?;
            ide_residuals(&mut items, &home, &ide)?;
            targets.push(ide);
        }
        // Never remove Toolbox's apps ancestor wholesale: it can contain IDEs and project files.
        if let Ok(entries) = fs::read_dir(&data) {
            for entry in entries
                .flatten()
                .filter(|entry| entry.file_name() != "apps")
            {
                append(
                    &mut items,
                    &home,
                    &entry.path(),
                    "Toolbox 配置、账户、日志或缓存",
                    "config",
                    false,
                )?;
            }
        }
        for (env, fallback) in [("XDG_CACHE_HOME", ".cache"), ("XDG_CONFIG_HOME", ".config")] {
            if let Some(root) = platform::xdg_dir(env, fallback) {
                append(
                    &mut items,
                    &home,
                    &root.join("JetBrains/Toolbox"),
                    "Toolbox 缓存/配置",
                    "cache",
                    false,
                )?;
            }
        }
        let settings = fs::read(data.join(".settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        if let Some(location) = settings
            .as_ref()
            .and_then(|s| s["shell_scripts"]["location"].as_str())
            && let Ok(entries) = fs::read_dir(location)
        {
            for entry in entries.flatten() {
                let path = entry.path();
                let related = fs::read_link(&path)
                    .ok()
                    .and_then(|_| fs::canonicalize(&path).ok())
                    .is_some_and(|p| targets.iter().any(|target| p.starts_with(target)))
                    || fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() < 65536)
                        && fs::read_to_string(&path).is_ok_and(|text| {
                            targets
                                .iter()
                                .any(|target| text.contains(&format!("{}/bin/", target.display())))
                        });
                if related {
                    append(
                        &mut items,
                        &home,
                        &path,
                        "IDE 命令行启动脚本",
                        "script",
                        false,
                    )?;
                }
            }
        }
        // Toolbox's own icon only; shared JetBrains daemon and IDE icons are not inferred by name.
        if let Some(root) = platform::xdg_dir("XDG_DATA_HOME", ".local/share") {
            append(
                &mut items,
                &home,
                &root.join("icons/hicolor/scalable/apps/jetbrains-toolbox.svg"),
                "Toolbox 图标",
                "icon",
                false,
            )?;
        }
    } else if target.is_dir() {
        ide_residuals(&mut items, &home, target)?;
        electron_residuals(&mut items, &home, target, &app.executable)?;
    }
    if app.toolbox {
        let application_root = target.parent().ok_or("安装路径无效")?;
        let data = platform::xdg_dir("XDG_DATA_HOME", ".local/share")
            .unwrap_or_else(|| home.join(".local/share"))
            .join("JetBrains/Toolbox");
        for ide in targets.iter().skip(1) {
            for boundary in [application_root, data.as_path()] {
                if ide.starts_with(boundary) {
                    let mut parent = ide.parent();
                    while let Some(path) = parent.filter(|path| path.starts_with(boundary)) {
                        append_empty(&mut items, &home, path)?;
                        if path == boundary {
                            break;
                        }
                        parent = path.parent();
                    }
                }
            }
        }
        append_empty(&mut items, &home, &application_root.join("scripts/scripts"))?;
        append_empty(&mut items, &home, &application_root.join("scripts"))?;
        append_empty(&mut items, &home, application_root)?;
        append_empty(&mut items, &home, &data)?;
    }
    scoped_links(&mut items, &home, &targets)?;
    // Required original launcher, even if duplicates point at the same program.
    append(
        &mut items,
        &home,
        Path::new(key),
        "应用启动入口",
        "launcher",
        true,
    )?;
    if !items
        .iter()
        .any(|item| item.kind == "program" && item.required)
    {
        return Err("应用目录权限或归属不明确，无法自动卸载".into());
    }
    let mut fingerprints = HashMap::new();
    for item in &items {
        fingerprints.insert(PathBuf::from(&item.path), snapshot(item)?);
    }
    let id = hex::encode(Sha256::digest(format!(
        "{key}:{:?}:{:?}",
        SystemTime::now(),
        fingerprints
    )));
    let note = if app.toolbox {
        "请先退出所选应用和 IDE。选中的程序、入口与残留文件移入废纸篓；IDE 可单独取消勾选。只处理已确认的安装与数据目录，项目代码目录不参与扫描。共享 JetBrains Daemon、系统凭据和无法确认归属的残留保留。"
    } else if electron_info(target, &app.executable).is_some() {
        "请先退出应用。选中的程序、入口与已确认的缓存移入废纸篓；配置默认不选。项目代码、未保存备份、本地历史、工作区状态、用户扩展及无法确认归属的残留保留。"
    } else {
        "请先退出所选应用。选中的程序、入口与配置缓存移入废纸篓；项目代码和无法确认归属的残留保留。"
    };
    let plan = UninstallPlan {
        id: id.clone(),
        name: app.app.name.clone(),
        note: note.into(),
        items,
    };
    let mut receipts = PLANS.lock().map_err(|e| e.to_string())?;
    receipts.clear();
    receipts.insert(
        id,
        Receipt {
            plan: plan.clone(),
            app,
            fingerprints,
            completed: HashSet::new(),
        },
    );
    Ok(plan)
}

fn fingerprint(path: &Path) -> Result<String, String> {
    let parent = path.parent().ok_or("路径无效")?;
    if fs::canonicalize(parent).map_err(|e| e.to_string())? != parent {
        return Err("安装目录的父路径已改变，请重新查看卸载详情".into());
    }
    if fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Ok(format!(
            "link:{}",
            fs::read_link(path).map_err(|e| e.to_string())?.display()
        ));
    }
    crate::developer::fingerprint(path)
}
fn ensure_stopped(_paths: &[PathBuf]) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    for entry in fs::read_dir("/proc")
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().parse::<u32>().is_ok())
    {
        let exec = fs::read_link(entry.path().join("exe")).ok();
        let command = fs::read(entry.path().join("cmdline"))
            .ok()
            .unwrap_or_default();
        let args = command
            .split(|b| *b == 0)
            .filter_map(|arg| std::str::from_utf8(arg).ok())
            .filter_map(|arg| {
                if arg.starts_with('/') {
                    Some(PathBuf::from(arg))
                } else if arg.starts_with("-Didea.") {
                    arg.split_once('=')
                        .filter(|(_, value)| value.starts_with('/'))
                        .map(|(_, value)| PathBuf::from(value))
                } else {
                    None
                }
            });
        if exec
            .into_iter()
            .chain(args)
            .any(|binary| _paths.iter().any(|path| binary.starts_with(path)))
        {
            return Err("所选程序仍在运行，请退出所选应用或 IDE 后重试".into());
        }
    }
    Ok(())
}

fn validated_selection(receipt: &Receipt, paths: &[String]) -> Result<Vec<PathBuf>, String> {
    let selected: HashSet<_> = paths.iter().map(PathBuf::from).collect();
    for item in receipt.plan.items.iter().filter(|item| item.required) {
        if !selected.contains(Path::new(&item.path))
            && !receipt
                .completed
                .iter()
                .any(|path| Path::new(&item.path).starts_with(path))
        {
            return Err("必须选择应用程序和启动入口".into());
        }
    }
    for path in &selected {
        let expected = receipt
            .fingerprints
            .get(path)
            .ok_or("路径不在本次卸载预览中")?;
        if receipt
            .completed
            .iter()
            .any(|parent| path.starts_with(parent))
        {
            continue;
        }
        let item = receipt
            .plan
            .items
            .iter()
            .find(|item| Path::new(&item.path) == path)
            .ok_or("卸载项目无效")?;
        if snapshot(item)? != *expected {
            return Err(format!(
                "文件已变化，请重新查看卸载详情：{}",
                path.display()
            ));
        }
    }
    let mut result: Vec<_> = selected
        .into_iter()
        .filter(|path| {
            !receipt
                .completed
                .iter()
                .any(|parent| path.starts_with(parent))
        })
        .collect();
    // Remove redundant descendants and move launchers last, after program/data operations succeed.
    let all: Vec<_> = result
        .iter()
        .filter(|path| {
            receipt
                .plan
                .items
                .iter()
                .any(|item| Path::new(&item.path) == **path && item.kind != "empty")
        })
        .cloned()
        .collect();
    result.retain(|path| {
        !all.iter()
            .any(|parent| path != parent && path.starts_with(parent))
    });
    result.sort_by_key(|path| {
        let kind = receipt
            .plan
            .items
            .iter()
            .find(|item| Path::new(&item.path) == path)
            .map(|item| item.kind.as_str())
            .unwrap_or("");
        (
            if kind == "empty" {
                2
            } else if ["launcher", "autostart", "script"].contains(&kind) {
                1
            } else {
                0
            },
            std::cmp::Reverse(path.components().count()),
        )
    });
    Ok(result)
}

pub fn uninstall(
    id: &str,
    paths: Vec<String>,
    mut report: impl FnMut(usize, usize, &str),
) -> Result<String, String> {
    let receipt = PLANS
        .lock()
        .map_err(|e| e.to_string())?
        .get(id)
        .cloned()
        .ok_or("卸载预览已失效，请重新查看")?;
    if !APPS
        .lock()
        .map_err(|e| e.to_string())?
        .contains_key(&receipt.app.app.key)
    {
        return Err("应用扫描结果已改变，请重新查看卸载详情".into());
    }
    let selected = validated_selection(&receipt, &paths)?;
    let processes: Vec<_> = selected
        .iter()
        .filter(|path| {
            receipt
                .plan
                .items
                .iter()
                .any(|item| Path::new(&item.path) == **path && item.kind != "empty")
        })
        .cloned()
        .collect();
    ensure_stopped(&processes)?;
    let mut preserved_containers = 0;
    let mut removed_empty = 0;
    let gio = platform::command_path("gio").ok_or("系统未提供废纸篓工具 gio")?;
    report(0, selected.len(), "正在检查所选程序是否退出");
    for (count, path) in selected.iter().enumerate() {
        report(count, selected.len(), &path.to_string_lossy());
        // Revalidate immediately before each operation as well as before starting the batch.
        let item = receipt
            .plan
            .items
            .iter()
            .find(|item| Path::new(&item.path) == path)
            .ok_or("卸载项目无效")?;
        if snapshot(item)? != receipt.fingerprints[path] {
            return Err(format!(
                "已处理 {count} 项；文件发生变化，请重新查看卸载详情"
            ));
        }
        if item.kind == "empty" {
            // remove_dir is atomic and refuses nonempty directories, including files created after preview.
            match fs::remove_dir(path) {
                Ok(()) => {
                    removed_empty += 1;
                    if let Some(saved) = PLANS.lock().map_err(|e| e.to_string())?.get_mut(id) {
                        saved.completed.insert(path.clone());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                    preserved_containers += 1;
                }
                Err(error) => return Err(format!("无法清理空目录 {}：{error}", path.display())),
            }
            report(count + 1, selected.len(), &path.to_string_lossy());
            continue;
        }
        platform::checked_output(
            std::process::Command::new(&gio)
                .args(["trash", "--"])
                .arg(path),
        )
        .map_err(|error| {
            format!(
                "已移入废纸篓 {count} 项；{}：{error}；可再次点击确认重试未完成项",
                path.display()
            )
        })?;
        if let Some(saved) = PLANS.lock().map_err(|e| e.to_string())?.get_mut(id) {
            saved.completed.insert(path.clone());
        }
        report(count + 1, selected.len(), &path.to_string_lossy());
    }
    PLANS.lock().map_err(|e| e.to_string())?.remove(id);
    Ok(format!(
        "已将 {} 项程序与残留移入废纸篓，清理 {removed_empty} 个空目录；项目代码保留{}",
        selected.len() - preserved_containers - removed_empty,
        if preserved_containers > 0 {
            format!("；保留 {preserved_containers} 个非空容器目录")
        } else {
            String::new()
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "appsteward-manual-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn file(&self, name: &str, text: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
        fn binary(&self, name: &str) -> PathBuf {
            let path = self.file(name, "#!/bin/sh\nexit 0\n");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        }
        fn electron(&self, name: &str) -> PathBuf {
            let binary = self.binary(&format!("opt/{name}/{name}"));
            fs::write(&binary, b"\x7fELFtest-distribution").unwrap();
            self.file(&format!("opt/{name}/resources/app/product.json"),&serde_json::json!({"applicationName":name,"nameShort":"Test IDE","nameLong":"Test IDE"}).to_string());
            self.file(
                &format!("opt/{name}/resources/app/package.json"),
                &serde_json::json!({"name":"Test IDE","main":"./out/main.js"}).to_string(),
            );
            self.file(
                &format!("opt/{name}/resources/app/out/main.js"),
                "compiled application",
            );
            self.file(&format!("opt/{name}/resources.pak"), "resources");
            self.file(&format!("opt/{name}/icudtl.dat"), "data");
            fs::create_dir_all(binary.parent().unwrap().join("locales")).unwrap();
            binary
        }
        fn ide(&self, name: &str, code: &str) -> PathBuf {
            let root = self.0.join(name);
            fs::create_dir_all(root.join("lib")).unwrap();
            self.binary(&format!("{name}/bin/idea"));
            self.file(&format!("{name}/product-info.json"),&serde_json::json!({"name":"Test IDE","productCode":code,"dataDirectoryName":"TestIDE2026","launch":[{"launcherPath":"bin/idea"}]}).to_string());
            root
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn resolves_quoted_program_and_env_without_executing_shell() {
        let f = Fixture::new();
        let binary = f.binary("an app/bin/program");
        assert_eq!(
            executable(&format!("env NAME=value \"{}\" %u", binary.display())),
            Some(binary)
        );
        assert!(executable("\"$(touch /tmp/do-not-execute)\" %u").is_none());
        assert!(exec_words("\"unterminated").is_none());
    }
    #[test]
    fn user_override_hides_system_copy_and_actions_do_not_replace_main_fields() {
        let f = Fixture::new();
        let user = f.0.join("user");
        let system = f.0.join("system");
        f.file(
            "user/app.desktop",
            "[Desktop Entry]\nType=Application\nName=User\nHidden=true\n",
        );
        f.file(
            "system/app.desktop",
            "[Desktop Entry]\nType=Application\nName=System\nExec=tool\n",
        );
        assert!(launchers_in(&[user.clone(), system]).is_empty());
        f.file("user/app.desktop","[Desktop Entry]\nType=Application\nName=User\nExec=tool\n[Desktop Action New]\nName=New\nNoDisplay=true\nExec=other\n");
        let entries = launchers_in(&[user]);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "User");
        assert_eq!(entries[0].exec, "tool");
    }
    #[test]
    fn toolbox_program_scope_keeps_ide_parent_and_unmarked_folders() {
        let f = Fixture::new();
        let binary = f.binary("opt/toolbox/bin/jetbrains-toolbox");
        f.file(
            "opt/toolbox/bin/jetbrains-toolbox.desktop",
            "[Desktop Entry]\n",
        );
        f.file("opt/toolbox/bin/lib/toolbox-core.jar", "jar");
        let (target, toolbox) = target_for(&binary, &f.0);
        assert!(toolbox);
        assert_eq!(target, Some(f.0.join("opt/toolbox/bin")));
        let unrelated = f.binary("Projects/my-project/bin/program");
        assert_eq!(target_for(&unrelated, &f.0), (None, false));
    }
    #[test]
    fn electron_distribution_is_identified_from_matching_binary_and_compiled_metadata() {
        let f = Fixture::new();
        let binary = f.electron("test-ide");
        assert_eq!(
            target_for(&binary, &f.0),
            (Some(binary.parent().unwrap().to_path_buf()), false)
        );
        f.file(
            "opt/test-ide/resources/app/product.json",
            &serde_json::json!({"applicationName":"different-binary","nameShort":"Test IDE"})
                .to_string(),
        );
        assert_eq!(target_for(&binary, &f.0), (None, false));
    }
    #[test]
    fn electron_detection_does_not_promote_checkouts_or_portable_user_data_into_program_scope() {
        let f = Fixture::new();
        let binary = f.electron("test-ide");
        f.file("opt/test-ide/.git/config", "keep source repository");
        assert_eq!(target_for(&binary, &f.0), (None, false));
        fs::remove_dir_all(binary.parent().unwrap().join(".git")).unwrap();
        f.file(
            "opt/test-ide/data/User/project.py",
            "keep portable user data",
        );
        assert_eq!(target_for(&binary, &f.0), (None, false));
        fs::remove_dir_all(binary.parent().unwrap().join("data")).unwrap();
        fs::write(
            &binary,
            "#!/bin/sh
exit 0
",
        )
        .unwrap();
        assert_eq!(target_for(&binary, &f.0), (None, false));
    }
    #[test]
    fn electron_entry_cannot_escape_distribution_resources() {
        let f = Fixture::new();
        let binary = f.electron("test-ide");
        f.file(
            "opt/test-ide/resources/app/package.json",
            &serde_json::json!({"name":"Test IDE","main":"../../../project/main.js"}).to_string(),
        );
        assert_eq!(target_for(&binary, &f.0), (None, false));
    }
    #[test]
    fn custom_toolbox_records_require_ide_metadata_and_keep_project_code() {
        let f = Fixture::new();
        let ide = f.ide("custom/apps/idea", "IU");
        let project = f.ide("custom/apps/project", "IU");
        f.file("custom/apps/project/Cargo.toml", "[package]\nname='keep'\n");
        let data = f.0.join("data/Toolbox");
        f.file("data/Toolbox/state.json",&serde_json::json!({"tools":[{"productCode":"IU","installLocation":ide},{"productCode":"IU","installLocation":project}]}).to_string());
        f.file(
            "data/Toolbox/.settings.json",
            &serde_json::json!({"install_location":f.0.join("custom/apps")}).to_string(),
        );
        assert_eq!(toolbox_ides(&f.0, &data), vec![ide]);
        assert!(project.join("Cargo.toml").is_file());
    }
    #[test]
    fn empty_container_cleanup_does_not_replace_program_selection_or_remove_nonempty_contents() {
        let f = Fixture::new();
        let program = f.binary("opt/toolbox/bin/program");
        let project = f.file("opt/toolbox/projects/keep.rs", "project code");
        let mut items = Vec::new();
        append(
            &mut items,
            &f.0,
            program.parent().unwrap(),
            "program",
            "program",
            true,
        )
        .unwrap();
        append_empty(&mut items, &f.0, &f.0.join("opt/toolbox")).unwrap();
        let hashes = items
            .iter()
            .map(|item| (PathBuf::from(&item.path), snapshot(item).unwrap()))
            .collect();
        let receipt = Receipt {
            plan: UninstallPlan {
                id: "test".into(),
                name: "App".into(),
                note: String::new(),
                items: items.clone(),
            },
            app: ManualApp {
                app: AppInfo::default(),
                executable: program,
                target: None,
                toolbox: false,
            },
            fingerprints: hashes,
            completed: HashSet::new(),
        };
        let selected: Vec<_> = items.iter().map(|item| item.path.clone()).collect();
        let result = validated_selection(&receipt, &selected).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result.last().unwrap(), &f.0.join("opt/toolbox"));
        assert!(
            fs::read_dir(result.last().unwrap())
                .unwrap()
                .next()
                .is_some()
        );
        assert!(project.is_file());
    }
    #[test]
    fn preview_rejects_unknown_changed_and_missing_required_targets_and_allows_retry() {
        let f = Fixture::new();
        let program = f.binary("opt/app/program");
        let launcher = f.file("menu/app.desktop", "[Desktop Entry]\nType=Application\n");
        let mut items = Vec::new();
        append(&mut items, &f.0, &program, "program", "program", true).unwrap();
        append(&mut items, &f.0, &launcher, "entry", "launcher", false).unwrap();
        append(&mut items, &f.0, &launcher, "entry", "launcher", true).unwrap();
        assert!(
            items
                .iter()
                .find(|item| Path::new(&item.path) == launcher)
                .unwrap()
                .required
        );
        let hashes = items
            .iter()
            .map(|item| {
                (
                    PathBuf::from(&item.path),
                    fingerprint(Path::new(&item.path)).unwrap(),
                )
            })
            .collect();
        let mut receipt = Receipt {
            plan: UninstallPlan {
                id: "test".into(),
                name: "App".into(),
                note: String::new(),
                items,
            },
            app: ManualApp {
                app: AppInfo::default(),
                executable: program.clone(),
                target: Some(program.clone()),
                toolbox: false,
            },
            fingerprints: hashes,
            completed: HashSet::new(),
        };
        let paths = vec![
            program.to_string_lossy().into_owned(),
            launcher.to_string_lossy().into_owned(),
        ];
        assert!(validated_selection(&receipt, &paths).is_ok());
        assert!(validated_selection(&receipt, &paths[0..1]).is_err());
        let mut unknown = paths.clone();
        unknown.push(f.0.join("Projects").to_string_lossy().into_owned());
        assert!(validated_selection(&receipt, &unknown).is_err());
        fs::write(&launcher, "changed").unwrap();
        assert!(validated_selection(&receipt, &paths).is_err());
        fs::write(&launcher, "[Desktop Entry]\nType=Application\n").unwrap();
        receipt
            .fingerprints
            .insert(launcher.clone(), fingerprint(&launcher).unwrap());
        fs::remove_file(&program).unwrap();
        receipt.completed.insert(program);
        assert_eq!(
            validated_selection(&receipt, &paths).unwrap(),
            vec![launcher]
        );
    }
}
