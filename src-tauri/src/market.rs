use crate::{platform, query::QueryCommand, software_updates::fetch_metadata};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::Path,
    process::{Command, Output, Stdio},
    sync::{LazyLock, Mutex},
};

const INDEX_URL: &str =
    "https://raw.githubusercontent.com/EverEcho/cleaner/market-index/index.json";
const BUNDLED: &str = include_str!("../../market/catalog.json");
const FLATHUB_REPO: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

#[derive(Deserialize)]
struct MarketIndex {
    schema_version: u32,
    apps: Vec<MarketApp>,
}

#[derive(Deserialize)]
struct MarketApp {
    id: String,
    name: String,
    category: String,
    description: String,
    upstream_version: Option<String>,
    install: Targets,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Targets {
    linux: Option<String>,
    macos: Option<String>,
}

fn valid_package(channel: Channel, package: &str) -> bool {
    if package.len() > 128 {
        return false;
    }
    match channel {
        Channel::Flathub => {
            let mut parts = package.rsplit('.');
            let valid_part = |part: &str, allow_dash| {
                let mut bytes = part.bytes();
                bytes
                    .next()
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
                    && bytes
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || allow_dash && b == b'-')
            };
            parts.next().is_some_and(|part| valid_part(part, true))
                && parts.clone().count() >= 2
                && parts.all(|part| valid_part(part, false))
        }
        Channel::Cask => {
            let mut bytes = package.bytes();
            bytes
                .next()
                .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
                && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }
    }
}

fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty()
        && value.encode_utf16().count() <= max
        && !value.bytes().any(|b| b.is_ascii_control())
}

fn parse_index(raw: &str) -> Result<MarketIndex, String> {
    if raw.len() > 512 * 1024 {
        return Err("应用市场索引超过大小限制".into());
    }
    let index: MarketIndex = serde_json::from_str(raw).map_err(|_| "应用市场索引格式无效")?;
    if index.schema_version != 1 || !(1..=500).contains(&index.apps.len()) {
        return Err("应用市场索引版本或条目数量无效".into());
    }
    let mut ids = HashSet::new();
    for app in &index.apps {
        if !valid_text(&app.id, 64)
            || !app.id.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
            })
            || !ids.insert(&app.id)
            || !valid_text(&app.name, 80)
            || !valid_text(&app.description, 200)
            || !matches!(
                app.category.as_str(),
                "development"
                    | "browsers"
                    | "productivity"
                    | "communication"
                    | "media"
                    | "security"
            )
            || !app
                .upstream_version
                .as_deref()
                .is_none_or(|value| valid_text(value, 80))
            || app.install.linux.is_none() && app.install.macos.is_none()
            || !app
                .install
                .linux
                .as_deref()
                .is_none_or(|package| valid_package(Channel::Flathub, package))
            || !app
                .install
                .macos
                .as_deref()
                .is_none_or(|package| valid_package(Channel::Cask, package))
        {
            return Err("应用市场索引包含无效条目".into());
        }
    }
    Ok(index)
}

static BUNDLED_INDEX: LazyLock<Result<MarketIndex, String>> =
    LazyLock::new(|| parse_index(BUNDLED));
static LATEST: Mutex<Option<(String, MarketIndex)>> = Mutex::new(None);

#[derive(Serialize)]
pub struct IndexReport {
    index: String,
    source: &'static str,
    warning: Option<String>,
}

pub fn refresh_index() -> Result<IndexReport, String> {
    let mut latest = LATEST.lock().map_err(|error| error.to_string())?;
    let result = fetch_metadata(INDEX_URL, false).and_then(|bytes| {
        let raw = String::from_utf8(bytes).map_err(|_| "应用市场索引不是 UTF-8")?;
        let index = parse_index(&raw)?;
        Ok((raw, index))
    });
    let (index, source, warning) = match result {
        Ok((raw, index)) => {
            *latest = Some((raw.clone(), index));
            (raw, "remote", None)
        }
        Err(error) => match latest.as_ref() {
            Some((raw, _)) => (raw.clone(), "cached", Some(error)),
            None => (BUNDLED.into(), "bundled", Some(error)),
        },
    };
    Ok(IndexReport {
        index,
        source,
        warning,
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Channel {
    Flathub,
    Cask,
}

impl Channel {
    fn local() -> Result<Self, String> {
        match std::env::consts::OS {
            "linux" => Ok(Self::Flathub),
            "macos" => Ok(Self::Cask),
            _ => Err("本机平台暂不支持市场安装".into()),
        }
    }

    fn executable(self) -> &'static str {
        match self {
            Self::Flathub => "flatpak",
            Self::Cask => "brew",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Flathub => "Flathub",
            Self::Cask => "Homebrew Cask",
        }
    }

    fn package(self, targets: &Targets) -> Option<&str> {
        match self {
            Self::Flathub => targets.linux.as_deref(),
            Self::Cask => targets.macos.as_deref(),
        }
    }

    fn command(self, binary: &Path, args: &[&str]) -> Command {
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .env("LC_ALL", "C")
            .env("NO_COLOR", "1");
        if self == Self::Cask {
            command
                .env("HOMEBREW_NO_ASK", "1")
                .env("HOMEBREW_NO_AUTO_UPDATE", "1")
                .env("HOMEBREW_NO_INSTALL_CLEANUP", "1")
                .env("HOMEBREW_NO_INSTALL_UPGRADE", "1")
                .env("HOMEBREW_NO_SUDO", "1");
        }
        command
    }
}

fn output_text(output: Output, step: &str) -> Result<String, String> {
    if !output.status.success() {
        let text = String::from_utf8_lossy(if output.stderr.is_empty() {
            &output.stdout
        } else {
            &output.stderr
        });
        let mut tail = text.trim().chars().rev().take(1200).collect::<Vec<_>>();
        tail.reverse();
        return Err(format!(
            "{step}失败（{}）\n{}",
            output.status,
            tail.into_iter().collect::<String>()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn query(mut command: Command, step: &str) -> Result<String, String> {
    let output = command
        .query_output()
        .map_err(|error| format!("{step}失败：{error}"))?;
    output_text(output, step)
}

fn installed(channel: Channel, binary: &Path) -> Result<Vec<String>, String> {
    let args: &[&str] = match channel {
        Channel::Flathub => &["list", "--app", "--columns=application"],
        Channel::Cask => &["list", "--cask", "-1"],
    };
    let stdout = query(channel.command(binary, args), "读取本机已安装软件")?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .filter(|name| valid_package(channel, name))
        .map(str::to_string)
        .collect())
}

#[derive(Serialize)]
pub struct Environment {
    installed: Vec<String>,
    issue: Option<String>,
}

fn local_manager() -> Result<(Channel, std::path::PathBuf), String> {
    let channel = Channel::local()?;
    let binary = platform::command_path(channel.executable()).ok_or_else(|| match channel {
        Channel::Flathub => "本机需要先配置 Flatpak".to_string(),
        Channel::Cask => "本机需要先配置 Homebrew".to_string(),
    })?;
    Ok((channel, binary))
}

pub fn environment() -> Environment {
    match local_manager().and_then(|(channel, binary)| installed(channel, &binary)) {
        Ok(installed) => Environment {
            installed,
            issue: None,
        },
        Err(error) => Environment {
            installed: Vec::new(),
            issue: Some(error),
        },
    }
}

fn flathub_remote(binary: &Path) -> Result<bool, String> {
    let stdout = query(
        Channel::Flathub.command(
            binary,
            &[
                "remotes",
                "--user",
                "--show-disabled",
                "--columns=name,url,options",
            ],
        ),
        "读取用户软件源",
    )?;
    for line in stdout.lines() {
        let mut fields = line.split('\t');
        if fields.next() != Some("flathub") {
            continue;
        }
        let url = fields.next().unwrap_or_default().trim_end_matches('/');
        let options = fields.next().unwrap_or_default();
        if !matches!(
            url,
            "https://dl.flathub.org/repo" | "https://flathub.org/repo"
        ) || options
            .split(',')
            .any(|item| matches!(item.trim(), "disabled" | "no-gpg-verify"))
        {
            return Err(
                "用户 Flathub 软件源已停用、地址不匹配或禁用了签名验证，请先检查 Flatpak 配置"
                    .into(),
            );
        }
        return Ok(true);
    }
    Ok(false)
}

fn confirm_cask(binary: &Path, target: &str, package: &str) -> Result<(), String> {
    let stdout = query(
        Channel::Cask.command(binary, &["info", "--json=v2", "--cask", "--", target]),
        "读取 Homebrew 安装方式",
    )?;
    let metadata: serde_json::Value =
        serde_json::from_str(&stdout).map_err(|_| "无法确认 Homebrew 安装方式")?;
    let cask = &metadata["casks"][0];
    let allowed = cask["artifacts"].as_array().is_some_and(|artifacts| {
        artifacts.iter().any(|item| item.get("app").is_some())
            && artifacts.iter().all(|item| {
                item.as_object().is_some_and(|parts| {
                    parts.keys().all(|key| {
                        matches!(
                            key.as_str(),
                            "app"
                                | "target"
                                | "binary"
                                | "command_wrapper"
                                | "manpage"
                                | "generate_completions_from_executable"
                                | "uninstall"
                                | "zap"
                        )
                    })
                })
            })
    });
    if cask["tap"] != "homebrew/cask"
        || cask["token"] != package
        || cask["disabled"] != false
        || cask["depends_on"]["cask"]
            .as_array()
            .is_some_and(|casks| !casks.is_empty())
        || !allowed
    {
        return Err("该 Homebrew Cask 当前需要额外的安装程序，应用内安装不可用".into());
    }
    Ok(())
}

pub fn install(id: &str, package: &str, progress: impl FnMut(&str)) -> Result<String, String> {
    let (channel, binary) = local_manager()?;
    if !valid_package(channel, package) {
        return Err("软件包标识无效".into());
    }
    {
        let latest = LATEST.lock().map_err(|error| error.to_string())?;
        let catalog = match latest.as_ref() {
            Some((_, index)) => index,
            None => BUNDLED_INDEX.as_ref().map_err(Clone::clone)?,
        };
        if catalog
            .apps
            .iter()
            .find(|app| app.id == id)
            .and_then(|app| channel.package(&app.install))
            != Some(package)
        {
            return Err("索引中的安装渠道已变化，请刷新后重新确认".into());
        }
    }

    install_package(channel, &binary, package, progress)
}

fn install_package(
    channel: Channel,
    binary: &Path,
    package: &str,
    mut progress: impl FnMut(&str),
) -> Result<String, String> {
    progress("核对本机安装状态");
    if installed(channel, binary)?
        .iter()
        .any(|name| name == package)
    {
        return Ok(format!("{package} 已安装"));
    }

    let mut command = match channel {
        Channel::Flathub => {
            if !flathub_remote(binary)? {
                progress("配置当前用户的 Flathub 软件源");
                let output = channel
                    .command(
                        binary,
                        &[
                            "remote-add",
                            "--user",
                            "--if-not-exists",
                            "flathub",
                            FLATHUB_REPO,
                        ],
                    )
                    .output()
                    .map_err(|error| format!("无法配置 Flathub：{error}"))?;
                output_text(output, "配置 Flathub 软件源")?;
                if !flathub_remote(binary)? {
                    return Err("无法确认 Flathub 软件源配置".into());
                }
            }
            let reference = format!("{package}//stable");
            channel.command(
                binary,
                &[
                    "install",
                    "--user",
                    "--noninteractive",
                    "--assumeyes",
                    "--app",
                    "--",
                    "flathub",
                    &reference,
                ],
            )
        }
        Channel::Cask => {
            let target = format!("homebrew/cask/{package}");
            confirm_cask(binary, &target, package)?;
            channel.command(binary, &["install", "--cask", "--", &target])
        }
    };
    let label = channel.label();
    progress(&format!("正在通过 {label} 下载并安装"));
    let output = command
        .output()
        .map_err(|error| format!("无法启动 {label}：{error}"))?;
    output_text(output, "下载或安装")?;
    progress("核对安装结果");
    if !installed(channel, binary)?
        .iter()
        .any(|name| name == package)
    {
        return Err("安装命令已结束，但未在包管理器清单中找到该应用".into());
    }
    Ok(format!("{package} 已通过 {label} 安装"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_executable_input_and_unknown_channels() {
        let index = parse_index(BUNDLED).unwrap();
        assert!(index.apps.len() > 10);
        for bad in [
            "--reinstall",
            "https://example.com/installer",
            "app/org.mozilla.firefox/x86_64/stable",
            "org.mozilla.firefox//stable",
        ] {
            assert!(!valid_package(Channel::Flathub, bad));
            assert!(!valid_package(Channel::Cask, bad));
        }
        assert!(!valid_package(Channel::Cask, "another/tap/firefox"));
        assert!(parse_index(&BUNDLED.replace("org.mozilla.firefox", "--reinstall")).is_err());
        assert!(parse_index(&BUNDLED.replace("\"linux\"", "\"windows\"")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn flatpak_install_uses_stable_user_scope_and_reconciles_retries() {
        use std::os::unix::fs::PermissionsExt;
        let root =
            std::env::temp_dir().join(format!("appclear-market-test-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let binary = root.join("flatpak");
        std::fs::write(
            &binary,
            r#"#!/bin/sh
set -eu
cd "$(dirname "$0")"
case "$*" in
  'list --app --columns=application')
    if [ -e installed ]; then echo org.mozilla.firefox; fi ;;
  'remotes --user --show-disabled --columns=name,url,options')
    if [ -e remote ]; then cat remote; fi ;;
  'remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo')
    printf 'flathub\thttps://dl.flathub.org/repo/\n' > remote ;;
  'install --user --noninteractive --assumeyes --app -- flathub org.mozilla.firefox//stable')
    [ ! -e installed ]
    [ -e remote ]
    touch installed ;;
  *) exit 19 ;;
esac
"#,
        )
        .unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let first = install_package(Channel::Flathub, &binary, "org.mozilla.firefox", |_| {});
        let retry = install_package(Channel::Flathub, &binary, "org.mozilla.firefox", |_| {});
        std::fs::remove_file(root.join("installed")).unwrap();
        std::fs::write(
            root.join("remote"),
            "flathub\thttps://dl.flathub.org/repo/\tno-gpg-verify\n",
        )
        .unwrap();
        let unsafe_remote =
            install_package(Channel::Flathub, &binary, "org.mozilla.firefox", |_| {});
        let executed_unsafe_install = root.join("installed").exists();
        std::fs::remove_dir_all(root).unwrap();
        assert!(first.unwrap().contains("已通过 Flathub 安装"));
        assert_eq!(retry.unwrap(), "org.mozilla.firefox 已安装");
        assert!(unsafe_remote.unwrap_err().contains("禁用了签名验证"));
        assert!(!executed_unsafe_install);
    }
}
