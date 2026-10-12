use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub use crate::query::QueryCommand;

#[derive(Serialize)]
pub struct PlatformInfo {
    pub os: &'static str,
    pub desktop_visibility: bool,
    pub autostart_note: &'static str,
    pub updates_note: &'static str,
    pub authorization_cache_available: bool,
}

pub fn info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        authorization_cache_available: cfg!(target_os = "linux")
            && crate::privileged::session_cache_installed(),
        desktop_visibility: cfg!(target_os = "linux"),
        autostart_note: if cfg!(target_os = "macos") {
            "管理用户 LaunchAgent 的下次登录启动状态；系统服务只读。登录项可在系统设置中管理。"
        } else {
            "管理桌面应用的登录启动项；系统项通过用户配置覆盖。"
        },
        updates_note: if cfg!(target_os = "macos") {
            "检测 Homebrew 管理的应用更新；App Store 和应用自带更新请在对应应用中操作。"
        } else {
            "检测已安装包管理器中的可用更新。"
        },
    }
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

pub fn xdg_dir(variable: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home().map(|p| p.join(fallback)))
}

pub fn command_path(name: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(name);
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    if matches!(name, "brew" | "docker") {
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let candidate = Path::new(dir).join(name);
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    if name == "docker" {
        let docker = PathBuf::from("/Applications/Docker.app/Contents/Resources/bin/docker");
        if executable(&docker) {
            return Some(docker);
        }
    }
    None
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

pub fn checked_output(command: &mut Command) -> Result<Output, String> {
    let output = command.output().map_err(|e| format!("执行失败: {e}"))?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(format!(
            "操作失败 ({:?}): {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

pub fn validate_package_key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.starts_with('-')
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".+_:@/-".contains(c))
    {
        return Err("无效的软件包标识".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_arguments_cannot_be_options_or_shell_commands() {
        for key in ["", "--all", "foo;rm", "foo bar", "$(id)", "foo\nbar"] {
            assert!(validate_package_key(key).is_err());
        }
        for key in [
            "libc6:amd64",
            "org.mozilla.firefox",
            "homebrew/core/git",
            "c++",
        ] {
            assert!(validate_package_key(key).is_ok());
        }
    }
}
