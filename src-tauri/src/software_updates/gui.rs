//! Codex desktop inventory and official metadata only; never launch or update a GUI.
use super::{ProviderResult, UpdateChannel, fetch_metadata, instance_id};
use crate::{
    models::{SoftwareKind, ToolUpdate, UpgradablePackage},
    platform::{self, QueryCommand},
};
use quick_xml::{Reader, events::Event};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const BASE: &str = "https://persistent.oaistatic.com/codex-app-prod/";
const GUIDE: &str = "https://learn.chatgpt.com/docs/linux/linux-app";
const PROVIDER: &str = "codex-gui";
const LIMIT: usize = 4 * 1024 * 1024;
type MetadataCache = Mutex<HashMap<String, (Instant, Vec<u8>)>>;
static CACHE: OnceLock<MetadataCache> = OnceLock::new();

#[derive(Default, Debug)]
struct Xml {
    name: String,
    attrs: HashMap<String, String>,
    text: String,
    children: Vec<Xml>,
}
impl Xml {
    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|child| child.name == name)
    }
    fn text(&self, name: &str) -> &str {
        self.child(name).map(|node| node.text.trim()).unwrap_or("")
    }
    fn attr(&self, name: &str) -> &str {
        self.attrs.get(name).map(String::as_str).unwrap_or("")
    }
}

// Bounded XML, without DTDs or external entities. Do not scrape version-like strings.
fn xml(bytes: &[u8]) -> Result<Xml, String> {
    if bytes.len() > LIMIT {
        return Err("更新元数据超过大小限制".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "更新元数据不是 UTF-8")?;
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);
    let mut stack = vec![Xml::default()];
    let mut count = 0;
    loop {
        let event = reader.read_event().map_err(|_| "更新 XML 格式无效")?;
        let empty = matches!(&event, Event::Empty(_));
        match event {
            Event::Start(start) | Event::Empty(start) => {
                count += 1;
                if count > 30000 || stack.len() > 32 {
                    return Err("更新 XML 超过解析限制".into());
                }
                let node = Xml {
                    name: start.local_name().as_ref().to_string(),
                    attrs: start
                        .attributes()
                        .map(|attr| {
                            let attr = attr.map_err(|_| "更新 XML 属性无效")?;
                            let key = attr.key.local_name().as_ref().to_string();
                            let value = attr
                                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                                .map_err(|_| "更新 XML 属性无效")?
                                .into_owned();
                            Ok((key, value))
                        })
                        .collect::<Result<_, String>>()?,
                    ..Default::default()
                };
                if empty {
                    stack.last_mut().unwrap().children.push(node);
                } else {
                    stack.push(node);
                }
            }
            Event::End(_) => {
                if stack.len() <= 1 {
                    return Err("更新 XML 结构无效".into());
                }
                let node = stack.pop().unwrap();
                stack.last_mut().unwrap().children.push(node);
            }
            Event::Text(text) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&text.xml10_content()),
            Event::CData(text) => stack
                .last_mut()
                .unwrap()
                .text
                .push_str(&text.xml10_content()),
            Event::GeneralRef(reference) => {
                let value = format!("&{};", reference.as_ref());
                stack.last_mut().unwrap().text.push_str(
                    &quick_xml::escape::unescape(&value).map_err(|_| "不支持的 XML 实体")?,
                );
            }
            Event::DocType(_) => return Err("更新 XML 不允许 DTD".into()),
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 || stack[0].children.len() != 1 {
        return Err("更新 XML 不完整".into());
    }
    Ok(stack.pop().unwrap().children.remove(0))
}

fn allowed_url(url: &str) -> bool {
    let Some(path) = url.strip_prefix(BASE) else {
        return false;
    };
    if path.contains("..")
        || !path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
    {
        return false;
    }
    if matches!(path, "appcast.xml" | "appcast-x64.xml") {
        return true;
    }
    for arch in ["amd64", "arm64"] {
        if path == format!("linux/deb/dists/stable/main/binary-{arch}/Packages") {
            return true;
        }
    }
    for arch in ["x86_64", "aarch64"] {
        if path == format!("linux/arch/{arch}/openai-chatgpt.db") {
            return true;
        }
        let prefix = format!("linux/rpm/{arch}/repodata/");
        if let Some(file) = path.strip_prefix(&prefix) {
            return file == "repomd.xml"
                || file.strip_suffix("-primary.xml.gz").is_some_and(|hash| {
                    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
                });
        }
    }
    false
}
fn metadata(url: &str) -> Result<Vec<u8>, String> {
    if !allowed_url(url) {
        return Err("未确认匹配的官方 GUI 更新源".into());
    }
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some((time, bytes)) = cache.lock().map_err(|e| e.to_string())?.get(url)
        && time.elapsed() < Duration::from_secs(300)
    {
        return Ok(bytes.clone());
    }
    // Official source only: do not follow redirects into third-party catalogs or downloads.
    let bytes = fetch_metadata(url, false)?;
    cache
        .lock()
        .map_err(|e| e.to_string())?
        .insert(url.into(), (Instant::now(), bytes.clone()));
    Ok(bytes)
}
fn read_small(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "无法读取安装元数据")?;
    if bytes.len() > limit {
        return Err("安装元数据超过大小限制".into());
    }
    Ok(bytes)
}
fn numbers(value: &str) -> Option<Vec<u64>> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    parts
        .into_iter()
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()) {
                None
            } else {
                part.parse().ok()
            }
        })
        .collect()
}
fn compare(current: &str, latest: &str) -> Option<Ordering> {
    let mut current = numbers(current)?;
    let mut latest = numbers(latest)?;
    let length = current.len().max(latest.len());
    current.resize(length, 0);
    latest.resize(length, 0);
    Some(current.cmp(&latest))
}
fn set_comparison(row: &mut ToolUpdate, current: &str, latest: &str) {
    row.status = match compare(current, latest) {
        Some(Ordering::Less) => "available",
        Some(Ordering::Equal) => "current",
        Some(Ordering::Greater) => "ahead",
        None => "unknown",
    }
    .into();
}

#[derive(Debug)]
enum Source {
    Mac {
        feed: String,
        build: String,
        system: String,
    },
    Linux {
        manager: String,
        package: String,
    },
    Unknown,
}
#[derive(Debug)]
struct Install {
    row: ToolUpdate,
    source: Source,
}
fn row(path: &Path, name: &str) -> ToolUpdate {
    ToolUpdate {
        id: instance_id(PROVIDER, &path.to_string_lossy()),
        name: name.into(),
        provider: PROVIDER.into(),
        kind: SoftwareKind::Gui,
        install_path: path.to_string_lossy().into_owned(),
        status: "unknown".into(),
        ..Default::default()
    }
}
fn arch(value: &str) -> Option<&'static str> {
    match value {
        "arm64" | "aarch64" => Some("arm64"),
        "x64" | "x86_64" | "amd64" => Some("x64"),
        _ => None,
    }
}

fn macho_arch(bytes: &[u8], host: &str) -> Option<String> {
    let cpu = |bytes: &[u8], little: bool| -> Option<&str> {
        let bytes: [u8; 4] = bytes.try_into().ok()?;
        match if little {
            u32::from_le_bytes(bytes)
        } else {
            u32::from_be_bytes(bytes)
        } {
            0x0100000c => Some("arm64"),
            0x01000007 => Some("x64"),
            _ => None,
        }
    };
    match bytes.get(..4)? {
        [0xcf, 0xfa, 0xed, 0xfe] => cpu(bytes.get(4..8)?, true).map(str::to_string),
        [0xfe, 0xed, 0xfa, 0xcf] => cpu(bytes.get(4..8)?, false).map(str::to_string),
        magic @ ([0xca, 0xfe, 0xba, 0xbe] | [0xca, 0xfe, 0xba, 0xbf]) => {
            let count = u32::from_be_bytes(bytes.get(4..8)?.try_into().ok()?) as usize;
            if count > 8 {
                return None;
            }
            let stride = if magic[3] == 0xbf { 32 } else { 20 };
            let architectures: Vec<_> = (0..count)
                .filter_map(|i| cpu(bytes.get(8 + i * stride..12 + i * stride)?, false))
                .collect();
            architectures.contains(&host).then(|| host.to_string())
        }
        _ => None,
    }
}
fn mac_install(path: &Path, host: &str, system: &str) -> Option<Install> {
    let bytes = read_small(&path.join("Contents/Info.plist"), 1024 * 1024).ok()?;
    let plist = plist::Value::from_reader(Cursor::new(bytes)).ok()?;
    let dict = plist.as_dictionary()?;
    let get = |key: &str| {
        dict.get(key)
            .and_then(plist::Value::as_string)
            .unwrap_or("")
    };
    let id = get("CFBundleIdentifier");
    let declared = get("SUFeedURL");
    // The older native ChatGPT client has a different updater and is not Codex GUI.
    if id != "com.openai.codex" && !(id == "com.openai.chatgpt" && declared.starts_with(BASE)) {
        return None;
    }
    let executable = get("CFBundleExecutable");
    let mut row = row(
        path,
        if id == "com.openai.chatgpt"
            || matches!(get("CFBundleDisplayName"), "ChatGPT")
            || matches!(get("CFBundleName"), "ChatGPT")
        {
            "ChatGPT GUI（含 Codex）"
        } else {
            "Codex GUI"
        },
    );
    row.current_version = get("CFBundleShortVersionString").into();
    row.update_hint = "在此桌面应用的菜单中检查更新；使用原安装渠道更新 GUI".into();
    if !executable.is_empty() && Path::new(executable).components().count() == 1 {
        let binary = path.join("Contents/MacOS").join(executable);
        if let Ok(mut file) = fs::File::open(binary) {
            let mut bytes = [0; 272];
            if let Ok(size) = file.read(&mut bytes) {
                row.architecture = macho_arch(&bytes[..size], host).unwrap_or_default();
            }
        }
    }
    // Some bundles declare the common stable feed for both builds. Stay within that
    // channel, but select the manifest for the installed executable architecture.
    let stable_feed = matches!(
        declared.strip_prefix(BASE),
        Some("appcast.xml" | "appcast-x64.xml")
    );
    let feed = if declared.is_empty() || stable_feed {
        match row.architecture.as_str() {
            "arm64" => format!("{BASE}appcast.xml"),
            "x64" => format!("{BASE}appcast-x64.xml"),
            _ => String::new(),
        }
    } else {
        declared.into()
    };
    let supported = !row.architecture.is_empty()
        && allowed_url(&feed)
        && matches!(
            feed.strip_prefix(BASE),
            Some("appcast.xml" | "appcast-x64.xml")
        );
    row.release_channel = if supported { "stable" } else { "unknown" }.into();
    row.note = "读取应用 Bundle 标识、版本、Mach-O 架构与官方 Sparkle 清单；不启动应用".into();
    if !supported {
        row.note = "应用的架构或发布渠道未确认；不会改用稳定版或第三方软件源".into();
    }
    row.source_url = if supported {
        feed.clone()
    } else {
        "https://developers.openai.com/codex/app".into()
    };
    Some(Install {
        row,
        source: if supported {
            Source::Mac {
                feed,
                build: get("CFBundleVersion").into(),
                system: system.into(),
            }
        } else {
            Source::Unknown
        },
    })
}
fn mac_installs(
    roots: &[PathBuf],
    host: &str,
    system: &str,
    cancelled: &dyn Fn() -> bool,
) -> (Vec<Install>, Vec<String>) {
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        if !root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(root)
            .max_depth(4)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || entry
                        .path()
                        .parent()
                        .is_none_or(|parent| parent.extension().is_none_or(|s| s != "app"))
            })
        {
            if cancelled() || rows.len() >= 32 {
                break;
            }
            match entry {
                Ok(entry) if entry.path().extension().is_some_and(|s| s == "app") => {
                    if let Ok(path) = fs::canonicalize(entry.path())
                        && seen.insert(path.clone())
                        && let Some(install) = mac_install(&path, host, system)
                    {
                        rows.push(install);
                    }
                }
                Err(error) => warnings.push(format!("GUI 应用目录未完整读取：{error}")),
                _ => {}
            }
        }
    }
    (rows, warnings)
}

fn command_text(name: &str, args: &[&str]) -> Option<String> {
    let binary = platform::command_path(name)?;
    let output = Command::new(binary)
        .args(args)
        .env("LANG", "C")
        .query_output()
        .ok()?;
    // dpkg-query may return 1 when one of several queried packages is absent.
    (output.status.success() || output.status.code() == Some(1))
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}
fn receipt(binary: &Path, package: &str) -> Option<(String, String, String)> {
    let path = binary.to_str()?;
    if let Some(owners) = command_text("dpkg-query", &["-S", path])
        && owners.lines().any(|line| {
            line.split_once(": ").is_some_and(|(owner, file)| {
                owner.split(':').next() == Some(package) && file == path
            })
        })
        && let Some(text) = command_text(
            "dpkg-query",
            &[
                "-W",
                "-f=${db:Status-Status}\t${Version}\t${Architecture}",
                package,
            ],
        )
        && let Some(("installed", version, architecture)) = text
            .trim()
            .split('\t')
            .collect::<Vec<_>>()
            .as_slice()
            .get(..3)
            .map(|parts| (parts[0], parts[1], parts[2]))
    {
        return Some(("deb".into(), version.into(), architecture.into()));
    }
    if let Some(text) = command_text(
        "rpm",
        &["-qf", "--qf", "%{NAME}\t%{VERSION}\t%{ARCH}", "--", path],
    ) {
        let fields: Vec<_> = text.trim().split('\t').collect();
        if fields.len() == 3 && fields[0] == package {
            return Some(("rpm".into(), fields[1].into(), fields[2].into()));
        }
    }
    if let Some(owner) = command_text("pacman", &["-Qqo", "--", path])
        && owner.trim() == package
        && let Some(info) = command_text("pacman", &["-Qi", "--", package])
    {
        let fields = info
            .lines()
            .filter_map(|line| {
                line.split_once(':')
                    .map(|(key, value)| (key.trim(), value.trim()))
            })
            .collect::<HashMap<_, _>>();
        let version = *fields.get("Version")?;
        let version = version
            .rsplit_once('-')
            .map(|(version, _)| version)
            .unwrap_or(version);
        return Some((
            "arch".into(),
            version.into(),
            fields.get("Architecture")?.to_string(),
        ));
    }
    None
}
fn linux_identity(bytes: &[u8], desktop_id: &str) -> Option<(String, String)> {
    let component = xml(bytes).ok()?;
    if component.name != "component"
        || component.attr("type") != "desktop-application"
        || !matches!(
            component.text("id"),
            "com.openai.codex" | "com.openai.chatgpt"
        )
        || !component.children.iter().any(|node| {
            node.name == "launchable"
                && node.attr("type") == "desktop-id"
                && node.text.trim() == desktop_id
        })
    {
        return None;
    }
    let package = component.text("pkgname");
    if !matches!(package, "chatgpt" | "chatgpt-bin" | "codex" | "codex-bin") {
        return None;
    }
    Some((component.text("id").into(), package.into()))
}
fn linux_installs(cancelled: &dyn Fn() -> bool) -> (Vec<Install>, Vec<String>) {
    let (launchers, warnings) =
        crate::manual_apps::launchers_with_warnings(&crate::manual_apps::launcher_roots());
    let mut rows = Vec::new();
    let mut seen = HashSet::new();
    for launcher in launchers {
        if cancelled() || rows.len() >= 32 {
            break;
        }
        if !matches!(
            launcher.name.to_ascii_lowercase().as_str(),
            "codex" | "chatgpt"
        ) {
            continue;
        }
        let Some(binary) = crate::manual_apps::executable(&launcher.exec) else {
            continue;
        };
        if !seen.insert(binary.clone()) {
            continue;
        }
        let desktop_id = launcher
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let mut roots = vec![
            PathBuf::from("/usr/share"),
            PathBuf::from("/usr/local/share"),
        ];
        roots.extend(
            launcher
                .path
                .parent()
                .and_then(Path::parent)
                .map(Path::to_path_buf),
        );
        let identity = roots.iter().find_map(|root| {
            ["metainfo", "appdata"].iter().find_map(|dir| {
                ["com.openai.codex", "com.openai.chatgpt"]
                    .iter()
                    .find_map(|id| {
                        read_small(
                            &root.join(dir).join(format!("{id}.metainfo.xml")),
                            1024 * 1024,
                        )
                        .ok()
                        .and_then(|bytes| linux_identity(&bytes, &desktop_id))
                    })
            })
        });
        let mut row = row(
            &binary,
            if launcher.name.eq_ignore_ascii_case("codex") {
                "Codex GUI"
            } else {
                "ChatGPT GUI（含 Codex）"
            },
        );
        row.source_url = GUIDE.into();
        row.update_hint = "请按此 GUI 的原安装渠道检查更新".into();
        let matched = identity.and_then(|(_, package)| {
            let candidates = if package.starts_with("chatgpt") {
                ["chatgpt", "chatgpt-bin"]
            } else {
                ["codex", "codex-bin"]
            };
            candidates
                .into_iter()
                .find_map(|package| receipt(&binary, package).map(|receipt| (package, receipt)))
        });
        let source = if let Some((package, (manager, version, architecture))) = matched {
            row.current_version = version;
            row.architecture = architecture;
            row.release_channel = "stable".into();
            row.note = "通过 AppStream 标识与包管理器文件归属识别 GUI；对照相同 Linux 包格式和架构的官方仓库".into();
            row.update_hint = match manager.as_str() {
                "deb" => format!("使用 APT 刷新软件源后按原渠道更新 {package}"),
                "rpm" => format!("使用 DNF 刷新软件源后按原渠道更新 {package}"),
                _ => "使用 pacman -Syu 完整系统升级；不要单独升级此应用".into(),
            };
            Source::Linux {
                manager,
                package: package.into(),
            }
        } else {
            // Standalone/repacked Linux GUIs are visible, but macOS or CLI releases cannot describe their update eligibility.
            let root = binary.parent().unwrap_or(Path::new("/"));
            if !launcher.name.eq_ignore_ascii_case("codex") {
                continue;
            }
            let package = read_small(&root.join("resources/app/package.json"), 65536)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
            let packaged = root.join("resources/app.asar").is_file()
                || package.as_ref().is_some_and(|data| {
                    matches!(data["productName"].as_str(), Some("Codex" | "ChatGPT"))
                });
            if !packaged {
                continue;
            }
            if let Some(package) = package {
                row.current_version = package["version"].as_str().unwrap_or("").into();
            }
            row.release_channel = "unknown".into();
            row.note = "独立或第三方 Linux GUI 打包，未确认匹配的官方发布渠道；不会与 CLI 或 macOS 版本比较".into();
            Source::Unknown
        };
        rows.push(Install { row, source });
    }
    (rows, warnings)
}

fn appcast(row: &mut ToolUpdate, bytes: &[u8], build: &str, system: &str) -> Result<(), String> {
    let document = xml(bytes)?;
    if document.name != "rss" {
        return Err("官方 Appcast 不是 RSS 清单".into());
    }
    let channel = document.child("channel").ok_or("官方 Appcast 缺少频道")?;
    if numbers(&row.current_version).is_none() || numbers(system).is_none() {
        return Err("本地应用版本或 macOS 版本未确认".into());
    }
    let selected = channel
        .children
        .iter()
        .filter(|item| {
            if item.name != "item" || !matches!(item.text("channel"), "" | "stable") {
                return false;
            }
            let minimum = item.text("minimumSystemVersion");
            if !minimum.is_empty()
                && !compare(system, minimum).is_some_and(|order| order != Ordering::Less)
            {
                return false;
            }
            let Some(enclosure) = item.child("enclosure") else {
                return false;
            };
            let hardware = item.text("hardwareRequirements");
            if !hardware.is_empty()
                && !hardware
                    .split([',', ' '])
                    .any(|value| arch(value) == Some(row.architecture.as_str()))
            {
                return false;
            }
            let url = enclosure.attr("url");
            url.starts_with(BASE)
                && url.contains(&format!("-darwin-{}-", row.architecture))
                && url.ends_with(".zip")
                && numbers(item.text("shortVersionString")).is_some()
                && numbers(item.text("version")).is_some()
        })
        .max_by(|a, b| compare(a.text("version"), b.text("version")).unwrap_or(Ordering::Equal));
    let selected = selected.ok_or("官方清单中没有匹配架构、系统与稳定渠道的版本")?;
    row.latest_version = selected.text("shortVersionString").into();
    if !build.is_empty() {
        if numbers(build).is_none() {
            return Err("本地 GUI 构建号无法可靠比较".into());
        }
        set_comparison(row, build, selected.text("version"));
    } else {
        let current = row.current_version.clone();
        let latest = row.latest_version.clone();
        set_comparison(row, &current, &latest);
    }
    Ok(())
}
fn latest_deb(bytes: &[u8], package: &str, architecture: &str) -> Result<String, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "官方 DEB 索引编码无效")?
        .replace("\r\n", "\n");
    text.split("\n\n")
        .filter_map(|section| {
            let fields: HashMap<_, _> = section
                .lines()
                .filter_map(|line| line.split_once(": "))
                .collect();
            let version = *fields.get("Version")?;
            (fields.get("Package") == Some(&package)
                && fields.get("Architecture") == Some(&architecture)
                && numbers(version).is_some())
            .then(|| version.to_string())
        })
        .max_by(|a, b| compare(a, b).unwrap())
        .ok_or("官方 DEB 索引没有匹配的软件包与架构".into())
}
fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .take(LIMIT as u64 + 1)
        .read_to_end(&mut data)
        .map_err(|_| "官方压缩元数据无效")?;
    if data.len() > LIMIT {
        return Err("官方元数据解压后超过限制".into());
    }
    Ok(data)
}
fn rpm_primary(bytes: &[u8]) -> Result<(String, String), String> {
    let document = xml(bytes)?;
    let primary = document
        .children
        .iter()
        .find(|data| data.name == "data" && data.attr("type") == "primary")
        .ok_or("RPM 索引缺少 primary 元数据")?;
    let checksum = primary
        .child("checksum")
        .filter(|sum| sum.attr("type") == "sha256")
        .ok_or("RPM 索引缺少 SHA256")?
        .text
        .trim();
    if checksum.len() != 64 || !checksum.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("RPM 元数据校验值无效".into());
    }
    Ok((
        primary
            .child("location")
            .ok_or("RPM 索引缺少元数据位置")?
            .attr("href")
            .into(),
        checksum.into(),
    ))
}
fn latest_rpm(bytes: &[u8], package: &str, architecture: &str) -> Result<String, String> {
    let document = xml(bytes)?;
    document
        .children
        .iter()
        .filter_map(|node| {
            let version = node.child("version")?;
            (node.name == "package"
                && node.text("name") == package
                && node.text("arch") == architecture
                && matches!(version.attr("epoch"), "" | "0")
                && numbers(version.attr("ver")).is_some())
            .then(|| version.attr("ver").to_string())
        })
        .max_by(|a, b| compare(a, b).unwrap())
        .ok_or("官方 RPM 索引没有匹配的软件包与架构".into())
}
fn latest_arch(bytes: &[u8], package: &str, architecture: &str) -> Result<String, String> {
    // Read tar records in memory. Never extract paths or create files from a repository archive.
    let mut offset = 0usize;
    let mut versions = Vec::new();
    while let Some(header) = bytes.get(offset..offset + 512) {
        if header.iter().all(|byte| *byte == 0) {
            break;
        }
        let octal = |bytes: &[u8]| -> Option<usize> {
            usize::from_str_radix(
                std::str::from_utf8(bytes).ok()?.trim_matches(['\0', ' ']),
                8,
            )
            .ok()
        };
        let size = octal(&header[124..136]).ok_or("Arch 索引 tar 长度无效")?;
        let checksum = octal(&header[148..156]).ok_or("Arch 索引 tar 校验值无效")?;
        let actual: usize = header
            .iter()
            .enumerate()
            .map(|(index, byte)| {
                if (148..156).contains(&index) {
                    32
                } else {
                    *byte as usize
                }
            })
            .sum();
        if checksum != actual {
            return Err("Arch 索引 tar 校验失败".into());
        }
        let begin = offset + 512;
        let end = begin
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or("Arch 索引不完整")?;
        let name = std::str::from_utf8(&header[..100])
            .map_err(|_| "Arch 索引路径无效")?
            .trim_end_matches('\0');
        if name.ends_with("/desc") && matches!(header[156], 0 | b'0') {
            let text =
                std::str::from_utf8(&bytes[begin..end]).map_err(|_| "Arch 软件包描述无效")?;
            let fields: HashMap<_, _> = text
                .split("\n\n")
                .filter_map(|field| field.split_once('\n'))
                .collect();
            if fields.get("%NAME%") == Some(&package)
                && fields.get("%ARCH%") == Some(&architecture)
                && let Some(version) = fields.get("%VERSION%")
            {
                let version = version
                    .rsplit_once('-')
                    .map(|(version, _)| version)
                    .unwrap_or(version);
                if numbers(version).is_some() {
                    versions.push(version.to_string());
                }
            }
        }
        offset = end
            .checked_add((512 - size % 512) % 512)
            .ok_or("Arch 索引长度溢出")?;
    }
    versions
        .into_iter()
        .max_by(|a, b| compare(a, b).unwrap())
        .ok_or("官方 Arch 索引没有匹配的软件包与架构".into())
}
fn linux_latest(
    row: &mut ToolUpdate,
    manager: &str,
    package: &str,
    fetch: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let architecture = arch(&row.architecture).ok_or("不支持的 Linux GUI 架构")?;
    if numbers(&row.current_version).is_none() {
        return Err("安装版本不是可确认的稳定 GUI 版本".into());
    }
    let linux_arch = if architecture == "x64" {
        "x86_64"
    } else {
        "aarch64"
    };
    let latest = match manager {
        "deb" => {
            let deb_arch = if architecture == "x64" {
                "amd64"
            } else {
                "arm64"
            };
            row.source_url =
                format!("{BASE}linux/deb/dists/stable/main/binary-{deb_arch}/Packages");
            latest_deb(&fetch(&row.source_url)?, package, deb_arch)?
        }
        "rpm" => {
            let base = format!("{BASE}linux/rpm/{linux_arch}/");
            row.source_url = format!("{base}repodata/repomd.xml");
            let (location, checksum) = rpm_primary(&fetch(&row.source_url)?)?;
            let url = format!("{base}{location}");
            if !allowed_url(&url) {
                return Err("RPM 元数据位置不在官方允许范围".into());
            }
            let compressed = fetch(&url)?;
            use sha2::Digest;
            if hex::encode(sha2::Sha256::digest(&compressed)) != checksum {
                return Err("RPM 元数据已变化或校验失败，请重试".into());
            }
            latest_rpm(&gunzip(&compressed)?, package, linux_arch)?
        }
        "arch" => {
            row.source_url = format!("{BASE}linux/arch/{linux_arch}/openai-chatgpt.db");
            latest_arch(&gunzip(&fetch(&row.source_url)?)?, package, linux_arch)?
        }
        _ => return Err("未确认此安装格式的官方更新源".into()),
    };
    row.latest_version = latest;
    let current = row.current_version.clone();
    let latest = row.latest_version.clone();
    set_comparison(row, &current, &latest);
    Ok(())
}

pub(super) fn is_gui_binary(path: &Path) -> bool {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let electron = path.parent().is_some_and(|root| {
        root.join("resources/app.asar").is_file()
            || root.join("resources/app/package.json").is_file()
    });
    if electron {
        return true;
    }
    let Some(macos) = path
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "MacOS"))
    else {
        return false;
    };
    let Some(contents) = macos.parent() else {
        return false;
    };
    read_small(&contents.join("Info.plist"), 1024 * 1024)
        .ok()
        .and_then(|bytes| plist::Value::from_reader(Cursor::new(bytes)).ok())
        .and_then(|value| {
            value
                .as_dictionary()?
                .get("CFBundleExecutable")?
                .as_string()
                .map(str::to_string)
        })
        .is_some_and(|executable| {
            path.file_name()
                .is_some_and(|name| name == executable.as_str())
        })
}
fn check_installs(
    installs: Vec<Install>,
    cancelled: &dyn Fn() -> bool,
    fetch: &dyn Fn(&str) -> Result<Vec<u8>, String>,
) -> ProviderResult {
    let mut result = ProviderResult::default();
    for mut install in installs {
        let row = &mut install.row;
        if cancelled() {
            row.status = "cancelled".into();
            row.note = "已停止此 GUI 安装实例的检测".into();
        } else {
            let checked = match install.source {
                Source::Mac {
                    feed,
                    build,
                    system,
                } => fetch(&feed).and_then(|bytes| appcast(row, &bytes, &build, &system)),
                Source::Linux { manager, package } => {
                    let checked = linux_latest(row, &manager, &package, fetch);
                    if checked.is_ok() && row.status == "available" && manager == "deb" {
                        set_apt_target(row, &package);
                    }
                    checked
                }
                Source::Unknown => Ok(()),
            };
            if let Err(error) = checked {
                row.status = "failed".into();
                row.note = error;
            }
        }
        let state = match row.status.as_str() {
            "available" => "completed",
            "current" | "ahead" => "no-update",
            "failed" => "failed",
            "cancelled" => "cancelled",
            _ => "unsupported",
        };
        let mut channel = UpdateChannel::new(
            PROVIDER,
            state,
            &format!("{}：{}", row.name, row.note),
            Some(Path::new(&row.install_path)),
        );
        channel.cache_seconds = 300;
        result.channels.push(channel);
        result.tools.push(install.row);
    }
    if result.channels.is_empty() {
        result.channels.push(UpdateChannel::new(
            PROVIDER,
            if cancelled() {
                "cancelled"
            } else {
                "unsupported"
            },
            "未发现可识别的 Codex GUI 或包含 Codex 的 ChatGPT 桌面安装；CLI 单独检测",
            None,
        ));
    }
    result
}

fn set_apt_target(row: &mut ToolUpdate, package: &str) {
    row.native_update = Some(UpgradablePackage {
        name: package.into(),
        manager: "apt".into(),
        current_version: row.current_version.clone(),
        new_version: row.latest_version.clone(),
        arch: row.architecture.clone(),
        repository: row.source_url.clone(),
    });
}

pub(super) fn record_native_success(tools: &mut [ToolUpdate], completed: &UpgradablePackage) {
    for row in tools {
        if row.kind == SoftwareKind::Gui
            && let Some(target) = row.native_update.as_ref().filter(|target| {
                target.manager == completed.manager
                    && target.name == completed.name
                    && target.arch == completed.arch
                    && target.current_version == completed.current_version
            })
        {
            if target.new_version == completed.new_version {
                row.current_version = row.latest_version.clone();
                row.status = "current".into();
            } else {
                row.current_version = completed.new_version.clone();
                let current = row.current_version.clone();
                let latest = row.latest_version.clone();
                set_comparison(row, &current, &latest);
            }
            row.native_update = None;
        }
    }
}
pub(super) fn check(cancelled: &dyn Fn() -> bool) -> ProviderResult {
    let (installs, warnings) = if cfg!(target_os = "macos") {
        let mut roots = vec![PathBuf::from("/Applications")];
        roots.extend(platform::home().map(|home| home.join("Applications")));
        let system = command_text("sw_vers", &["-productVersion"]).unwrap_or_default();
        mac_installs(
            &roots,
            arch(std::env::consts::ARCH).unwrap_or(""),
            system.trim(),
            cancelled,
        )
    } else if cfg!(target_os = "linux") {
        linux_installs(cancelled)
    } else {
        (Vec::new(), Vec::new())
    };
    let mut result = check_installs(installs, cancelled, &metadata);
    result.warnings.extend(warnings);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(items: &str) -> Vec<u8> {
        format!("<rss xmlns:sparkle='http://www.andymatuschak.org/xml-namespaces/sparkle'><channel>{items}</channel></rss>").into_bytes()
    }
    fn item(
        version: &str,
        build: &str,
        architecture: &str,
        minimum: &str,
        channel: &str,
    ) -> String {
        format!(
            "<item><sparkle:version>{build}</sparkle:version><sparkle:shortVersionString>{version}</sparkle:shortVersionString><sparkle:minimumSystemVersion>{minimum}</sparkle:minimumSystemVersion><sparkle:channel>{channel}</sparkle:channel><enclosure url='{BASE}Codex-darwin-{architecture}-{version}.zip'/><sparkle:deltas><enclosure url='https://example.invalid/delta'/></sparkle:deltas></item>"
        )
    }
    fn mac_row() -> ToolUpdate {
        let mut row = row(Path::new("/Applications/Codex.app"), "Codex GUI");
        row.current_version = "26.1002.1".into();
        row.architecture = "arm64".into();
        row
    }
    #[test]
    fn gui_dates_and_builds_are_not_cli_semver() {
        assert_eq!(
            compare("26.1002.1", "26.930.99999"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare("26.1103.31037.1", "26.1103.31037"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare("1.2", "1.2.0"), Some(Ordering::Equal));
        for invalid in [
            "",
            "1..2",
            "26.1103-beta",
            "1.2.3.4.5",
            "v1.0",
            "18446744073709551616",
        ] {
            assert!(numbers(invalid).is_none());
        }
        let mut row = mac_row();
        appcast(
            &mut row,
            &feed(&item("26.1002.1", "201", "arm64", "13.0", "")),
            "200",
            "14.0",
        )
        .unwrap();
        assert_eq!(row.status, "available", "same display version, newer build");
        appcast(
            &mut row,
            &feed(&item("26.1002.1", "201", "arm64", "13.0", "")),
            "201",
            "14.0",
        )
        .unwrap();
        assert_eq!(row.status, "current");
        appcast(
            &mut row,
            &feed(&item("26.1002.1", "201", "arm64", "13.0", "")),
            "202",
            "14.0",
        )
        .unwrap();
        assert_eq!(row.status, "ahead");
    }
    #[test]
    fn appcast_selects_supported_system_architecture_and_stable_channel() {
        let items = [
            item("26.1103.1", "301", "x64", "13", ""),
            item("26.1104.1", "302", "arm64", "99", ""),
            item("26.1105.1", "303", "arm64", "13", "beta"),
            item("26.1102.1", "299", "arm64", "13", ""),
            item("26.1101.1", "298", "arm64", "13", ""),
        ]
        .join("");
        let mut row = mac_row();
        appcast(&mut row, &feed(&items), "200", "14.0").unwrap();
        assert_eq!(row.latest_version, "26.1102.1");
        assert_eq!(row.status, "available");
        assert!(
            appcast(
                &mut row,
                &feed(&item("26.1103.1", "301", "x64", "13", "")),
                "200",
                "14"
            )
            .is_err()
        );
    }
    #[test]
    fn official_metadata_urls_never_accept_downloads_or_external_redirect_targets() {
        for path in [
            "appcast.xml",
            "appcast-x64.xml",
            "linux/deb/dists/stable/main/binary-arm64/Packages",
            "linux/rpm/x86_64/repodata/repomd.xml",
            "linux/arch/aarch64/openai-chatgpt.db",
        ] {
            assert!(allowed_url(&format!("{BASE}{path}")));
        }
        for url in [
            "http://persistent.oaistatic.com/codex-app-prod/appcast.xml",
            "https://persistent.oaistatic.com.evil.test/codex-app-prod/appcast.xml",
            "https://user@persistent.oaistatic.com/codex-app-prod/appcast.xml",
        ] {
            assert!(!allowed_url(url));
        }
        for path in [
            "appcast-beta.xml",
            "appcast.xml?token=x",
            "../appcast.xml",
            "linux/deb/../../appcast.xml",
            "Codex.zip",
            "linux/rpm/x86_64/repodata/other.xml.gz",
        ] {
            assert!(!allowed_url(&format!("{BASE}{path}")));
        }
        assert!(metadata("https://example.invalid/appcast.xml").is_err());
    }
    #[test]
    fn corrupt_and_external_entity_xml_is_rejected() {
        for bytes in [
            b"<rss><channel>".as_slice(),
            b"<a></b>",
            b"<a/><b/>",
            b"<!DOCTYPE a [<!ENTITY e SYSTEM 'file:///etc/passwd'>]><a>&e;</a>",
        ] {
            assert!(xml(bytes).is_err());
        }
        assert_eq!(xml(b"<a x='a&amp;b'>a &amp; b</a>").unwrap().text, "a&b");
        assert_eq!(xml(b"<a x='a&amp;b'/>").unwrap().attr("x"), "a&b");
        assert!(xml(&vec![b'x'; LIMIT + 1]).is_err());
    }
    #[test]
    fn deb_versions_require_exact_gui_package_and_architecture() {
        let bytes = b"Package: codex-cli\nVersion: 99.0.0\nArchitecture: amd64\n\nPackage: chatgpt\nVersion: 26.1103.9\nArchitecture: arm64\n\nPackage: chatgpt\nVersion: 26.1102.9\nArchitecture: amd64\n\nPackage: chatgpt\nVersion: 26.1103.1-beta\nArchitecture: amd64\n";
        assert_eq!(latest_deb(bytes, "chatgpt", "amd64").unwrap(), "26.1102.9");
        assert_eq!(latest_deb(bytes, "chatgpt", "arm64").unwrap(), "26.1103.9");
        assert!(latest_deb(bytes, "codex", "amd64").is_err());
        assert!(latest_deb(bytes, "chatgpt", "riscv64").is_err());
    }
    #[test]
    fn linux_identity_uses_appstream_not_display_name_or_cli_package() {
        let component = |id: &str, package: &str| {
            format!(
                "<component type='desktop-application'><id>{id}</id><pkgname>{package}</pkgname><launchable type='desktop-id'>chatgpt.desktop</launchable></component>"
            )
        };
        assert!(
            linux_identity(
                component("com.openai.chatgpt", "chatgpt").as_bytes(),
                "chatgpt.desktop"
            )
            .is_some()
        );
        assert!(
            linux_identity(
                component("com.example.chatgpt", "chatgpt").as_bytes(),
                "chatgpt.desktop"
            )
            .is_none()
        );
        assert!(
            linux_identity(
                component("com.openai.chatgpt", "codex-cli").as_bytes(),
                "chatgpt.desktop"
            )
            .is_none()
        );
        assert!(
            linux_identity(
                component("com.openai.chatgpt", "chatgpt").as_bytes(),
                "other.desktop"
            )
            .is_none()
        );
    }
    #[test]
    fn rpm_metadata_is_checked_and_selected_per_architecture() {
        let index = format!(
            "<repomd><data type='primary'><checksum type='sha256'>{}</checksum><location href='repodata/{}-primary.xml.gz'/></data></repomd>",
            "a".repeat(64),
            "a".repeat(64)
        );
        assert_eq!(rpm_primary(index.as_bytes()).unwrap().1, "a".repeat(64));
        let packages = b"<metadata><package><name>chatgpt</name><arch>x86_64</arch><version epoch='0' ver='26.1103.1' rel='1'/></package><package><name>chatgpt</name><arch>aarch64</arch><version epoch='0' ver='26.1104.1' rel='1'/></package></metadata>";
        assert_eq!(
            latest_rpm(packages, "chatgpt", "x86_64").unwrap(),
            "26.1103.1"
        );
        assert!(latest_rpm(packages, "codex", "x86_64").is_err());
        let mut row = mac_row();
        row.architecture = "x86_64".into();
        let bad_index = index.replace("repodata/", "../../repodata/");
        assert!(
            linux_latest(&mut row, "rpm", "chatgpt", &|_| Ok(bad_index
                .as_bytes()
                .to_vec()))
            .unwrap_err()
            .contains("允许范围")
        );
    }
    fn tar_desc(text: &str) -> Vec<u8> {
        let mut header = [0u8; 512];
        let name = b"chatgpt-bin-26.1103.1-1/desc";
        header[..name.len()].copy_from_slice(name);
        header[124..136].copy_from_slice(format!("{:011o}\0", text.len()).as_bytes());
        header[148..156].fill(b' ');
        header[156] = b'0';
        let checksum: usize = header.iter().map(|byte| *byte as usize).sum();
        header[148..156].copy_from_slice(format!("{checksum:06o}\0 ").as_bytes());
        let mut bytes = header.to_vec();
        bytes.extend(text.as_bytes());
        bytes.resize(bytes.len().div_ceil(512) * 512 + 1024, 0);
        bytes
    }
    #[test]
    fn arch_archive_is_read_in_memory_and_does_not_confuse_pkgrel_with_app_version() {
        let mut bytes =
            tar_desc("%NAME%\nchatgpt-bin\n\n%VERSION%\n26.1103.1-2\n\n%ARCH%\nx86_64\n\n");
        assert_eq!(
            latest_arch(&bytes, "chatgpt-bin", "x86_64").unwrap(),
            "26.1103.1"
        );
        assert!(latest_arch(&bytes, "chatgpt-bin", "aarch64").is_err());
        bytes[0] = b'x';
        assert!(latest_arch(&bytes, "chatgpt-bin", "x86_64").is_err());
        assert!(gunzip(b"not gzip").is_err());
    }
    #[test]
    fn macho_uses_installed_binary_architecture() {
        let mut arm = vec![0xcf, 0xfa, 0xed, 0xfe];
        arm.extend(0x0100000cu32.to_le_bytes());
        assert_eq!(macho_arch(&arm, "x64").as_deref(), Some("arm64"));
        assert_eq!(macho_arch(b"not executable metadata", "arm64"), None);
        let mut universal = vec![0xca, 0xfe, 0xba, 0xbe, 0, 0, 0, 2];
        for cpu in [0x01000007u32, 0x0100000cu32] {
            universal.extend(cpu.to_be_bytes());
            universal.extend([0; 16]);
        }
        assert_eq!(macho_arch(&universal, "arm64").as_deref(), Some("arm64"));
        assert_eq!(macho_arch(&universal, "x64").as_deref(), Some("x64"));
    }
    #[test]
    fn binary_plist_multiple_bundles_and_unsupported_feed_are_preserved() {
        let base = std::env::temp_dir().join(format!("appclear-gui-plist-{}", std::process::id()));
        fs::create_dir_all(&base).unwrap();
        for (name, url, id) in [
            ("Codex", format!("{BASE}appcast.xml"), "com.openai.codex"),
            (
                "Codex Beta",
                format!("{BASE}appcast-beta.xml"),
                "com.openai.codex",
            ),
            (
                "Old ChatGPT",
                "https://example.invalid/appcast.xml".into(),
                "com.openai.chatgpt",
            ),
        ] {
            let root = base.join(format!("{name}.app/Contents"));
            fs::create_dir_all(root.join("MacOS")).unwrap();
            let mut dict = plist::Dictionary::new();
            for (key, value) in [
                ("CFBundleIdentifier", id),
                ("SUFeedURL", &url),
                ("CFBundleShortVersionString", "26.1002.1"),
                ("CFBundleVersion", "200"),
                ("CFBundleExecutable", "Codex"),
            ] {
                dict.insert(key.into(), plist::Value::String(value.into()));
            }
            plist::Value::Dictionary(dict)
                .to_file_binary(root.join("Info.plist"))
                .unwrap();
            fs::write(
                root.join("MacOS/Codex"),
                [0xcf, 0xfa, 0xed, 0xfe, 12, 0, 0, 1],
            )
            .unwrap();
            fs::create_dir_all(root.join("Resources/bin")).unwrap();
            fs::write(root.join("Resources/bin/codex"), b"embedded CLI fixture").unwrap();
            assert!(is_gui_binary(&root.join("MacOS/Codex")));
            assert!(
                !is_gui_binary(&root.join("Resources/bin/codex")),
                "a bundled CLI is not the main GUI executable"
            );
        }
        let (installs, warnings) =
            mac_installs(std::slice::from_ref(&base), "arm64", "14.0", &|| false);
        assert!(warnings.is_empty());
        assert_eq!(
            installs.len(),
            2,
            "old native ChatGPT is a separate application"
        );
        let report = check_installs(installs, &|| false, &|_| {
            Ok(feed(&item("26.1103.1", "300", "arm64", "13", "")))
        });
        assert_eq!(
            report
                .tools
                .iter()
                .filter(|row| row.status == "available")
                .count(),
            1
        );
        assert_eq!(
            report
                .tools
                .iter()
                .filter(|row| row.status == "unknown")
                .count(),
            1
        );
        assert_ne!(report.tools[0].id, report.tools[1].id);
        assert!(report.permissions.is_empty());
        let intel_bundle = base.join("Codex.app");
        fs::write(
            intel_bundle.join("Contents/MacOS/Codex"),
            [0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1],
        )
        .unwrap();
        let intel = mac_install(&intel_bundle, "arm64", "14.0").unwrap();
        assert_eq!(intel.row.architecture, "x64");
        assert_eq!(
            intel.row.source_url,
            format!("{BASE}appcast-x64.xml"),
            "the installed Intel build uses the Intel feed even on an ARM host"
        );
        fs::remove_dir_all(base).unwrap();
    }
    #[test]
    fn failures_and_cancellation_never_become_current_or_upgrade_permissions() {
        let install = || Install {
            row: mac_row(),
            source: Source::Mac {
                feed: format!("{BASE}appcast.xml"),
                build: "200".into(),
                system: "14.0".into(),
            },
        };
        let failed = check_installs(vec![install()], &|| false, &|_| {
            Err("network timeout".into())
        });
        assert_eq!(failed.tools[0].status, "failed");
        assert_eq!(failed.channels[0].state, "failed");
        let cancelled = check_installs(vec![install()], &|| true, &|_| {
            panic!("cancelled checks must not query the network")
        });
        assert_eq!(cancelled.tools[0].status, "cancelled");
        assert!(cancelled.permissions.is_empty());
        let unknown = check_installs(
            vec![Install {
                row: mac_row(),
                source: Source::Unknown,
            }],
            &|| false,
            &|_| panic!("unknown sources must not fall back"),
        );
        assert_eq!(unknown.tools[0].status, "unknown");
        assert_eq!(
            serde_json::to_value(&unknown.tools[0]).unwrap()["kind"],
            "gui"
        );
        assert_eq!(
            serde_json::to_value(ToolUpdate::default()).unwrap()["kind"],
            "cli"
        );
        assert_ne!(
            instance_id("github-release", "/Applications/Codex.app"),
            unknown.tools[0].id
        );
    }
    #[test]
    fn linux_checks_use_platform_feed_and_gui_receipt_version() {
        let mut row = row(
            Path::new("/usr/lib/chatgpt/ChatGPT"),
            "ChatGPT GUI（含 Codex）",
        );
        row.current_version = "26.1002.1".into();
        row.architecture = "amd64".into();
        let report = check_installs(
            vec![Install {
                row,
                source: Source::Linux {
                    manager: "deb".into(),
                    package: "chatgpt".into(),
                },
            }],
            &|| false,
            &|url| {
                assert_eq!(
                    url,
                    format!("{BASE}linux/deb/dists/stable/main/binary-amd64/Packages")
                );
                Ok(b"Package: chatgpt\nVersion: 26.1103.1\nArchitecture: amd64\n".to_vec())
            },
        );
        assert_eq!(report.tools[0].status, "available");
        assert!(report.permissions.is_empty());
        let target = report.tools[0].native_update.as_ref().unwrap();
        assert_eq!(target.manager, "apt");
        assert_eq!(target.name, "chatgpt");
        assert_eq!(target.arch, "amd64");
        assert_eq!(target.current_version, "26.1002.1");
        assert_eq!(target.new_version, "26.1103.1");
    }

    #[test]
    fn native_success_updates_only_the_matching_gui_installation() {
        let completed = UpgradablePackage {
            manager: "apt".into(),
            name: "chatgpt".into(),
            arch: "amd64".into(),
            current_version: "26.1002.1".into(),
            new_version: "26.1103.1".into(),
            repository: "stable".into(),
        };
        let mut gui = row(
            Path::new("/usr/lib/chatgpt/codex-launcher"),
            "ChatGPT GUI（含 Codex）",
        );
        gui.current_version = completed.current_version.clone();
        gui.latest_version = completed.new_version.clone();
        gui.status = "available".into();
        gui.native_update = Some(completed.clone());
        let mut other_arch = gui.clone();
        other_arch.native_update.as_mut().unwrap().arch = "arm64".into();
        let mut cli = gui.clone();
        cli.kind = SoftwareKind::Cli;
        let mut tools = vec![gui, other_arch, cli];
        record_native_success(&mut tools, &completed);
        assert_eq!(tools[0].status, "current");
        assert_eq!(tools[0].current_version, completed.new_version);
        assert!(tools[0].native_update.is_none());
        assert_eq!(tools[1].status, "available");
        assert_eq!(tools[2].status, "available");
    }

    #[test]
    #[ignore = "Reads installed GUI, official metadata and APT exact-version plan; never installs or authorizes"]
    fn live_apt_gui_upgrade_plan() {
        let report = check(&|| false);
        let target = report
            .tools
            .iter()
            .find_map(|row| row.native_update.as_ref())
            .expect("this host needs an available APT GUI update");
        let candidates = crate::updates::check_all_upgradable().unwrap();
        let candidate = crate::select_upgrade_candidate(
            &candidates,
            &target.manager,
            &target.name,
            Some(&target.new_version),
            Some(&target.arch),
        )
        .unwrap();
        assert_eq!(candidate.current_version, target.current_version);
        let receipt = crate::native_update_plan::prepare(
            &target.manager,
            &target.name,
            &target.new_version,
            &target.arch,
        )
        .unwrap();
        assert_eq!(receipt.current_version, target.current_version);
        let command = crate::native_update_plan::resolve(&receipt).unwrap();
        println!(
            "GUI route: {} / {} / {} -> {}; read-only plan: {} {:?}",
            target.manager,
            target.name,
            target.arch,
            target.new_version,
            command.program,
            command.args
        );
    }
    #[test]
    #[ignore = "Reads installed GUI metadata and official HTTPS feeds; no installation or GUI launch"]
    fn live_installed_gui_probe() {
        let report = check(&|| false);
        for row in &report.tools {
            println!(
                "{} | {} | {} -> {} | {} | {} | {}",
                row.name,
                row.architecture,
                row.current_version,
                row.latest_version,
                row.status,
                row.source_url,
                row.note
            );
        }
        assert!(
            !report.tools.is_empty(),
            "no installed GUI fixture on this host"
        );
        assert!(
            report.tools.iter().all(|row| row.status != "failed"),
            "{:?}",
            report.tools
        );
    }

    #[test]
    #[ignore = "Reads public official macOS/DEB/RPM/Arch metadata; never downloads installers"]
    fn live_official_formats_probe() {
        for (manager, package, architecture) in [
            ("deb", "chatgpt", "amd64"),
            ("deb", "chatgpt", "arm64"),
            ("rpm", "chatgpt", "x86_64"),
            ("rpm", "chatgpt", "aarch64"),
            ("arch", "chatgpt-bin", "x86_64"),
            ("arch", "chatgpt-bin", "aarch64"),
        ] {
            let mut row = mac_row();
            row.current_version = "1.0.0".into();
            row.architecture = architecture.into();
            linux_latest(&mut row, manager, package, &metadata)
                .unwrap_or_else(|error| panic!("{manager}/{architecture}: {error}"));
            println!(
                "{manager}/{architecture}: {} {}",
                row.latest_version, row.source_url
            );
            assert_eq!(row.status, "available");
        }
        for (architecture, file) in [("arm64", "appcast.xml"), ("x64", "appcast-x64.xml")] {
            let mut row = mac_row();
            row.architecture = architecture.into();
            appcast(
                &mut row,
                &metadata(&format!("{BASE}{file}")).unwrap(),
                "1",
                "99.0",
            )
            .unwrap();
            println!("macos/{architecture}: {}", row.latest_version);
            assert_eq!(row.status, "available");
        }
    }
}
