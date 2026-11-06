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
            "检测 Homebrew、npm、uv/pipx 和规则库中的独立工具；App Store 使用系统更新。"
        } else {
            "检测系统包管理器、npm、uv/pipx 和规则库中的独立工具更新。"
        },
    }
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

// Resolve existing aliases even when the final path has not been created yet.
pub(crate) fn normalized_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("保留路径需要绝对路径".into());
    }
    match std::fs::canonicalize(path) {
        Ok(path) => return Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("无法核对保留路径 {}: {error}", path.display())),
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            other => {
                normalized.push(other.as_os_str());
                match std::fs::canonicalize(&normalized) {
                    Ok(path) => normalized = path,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(format!("无法核对保留路径 {}: {error}", path.display()));
                    }
                }
            }
        }
    }
    Ok(normalized)
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

const MACOS_TRASH_SCRIPT: &str = "on run argv\nset targetItem to (POSIX file (item 1 of argv)) as alias\ntell application \"Finder\" to delete targetItem\nend run";

pub(crate) fn move_to_trash(path: &Path) -> Result<Output, String> {
    if cfg!(target_os = "macos") {
        // Resolve the file in AppleScript's scope before sending its alias to Finder.
        // Pass the path as an argument so spaces, quotes and Unicode stay literal.
        checked_output(
            Command::new("/usr/bin/osascript")
                .args(["-e", MACOS_TRASH_SCRIPT])
                .arg(path),
        )
    } else {
        let gio = command_path("gio").ok_or("系统未提供回收站工具 gio")?;
        checked_output(Command::new(gio).args(["trash", "--"]).arg(path))
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
    #[cfg(target_os = "macos")]
    #[test]
    fn trash_script_resolves_literal_paths_before_finder() {
        let root =
            std::env::temp_dir().join(format!("appclear-trash-alias-{}", std::process::id()));
        let app = root.join("测试 IntelliJ \"IDEA\" 'sample'.app");
        std::fs::create_dir_all(&app).unwrap();
        // Exercise the production script's path conversion without moving anything.
        let script = MACOS_TRASH_SCRIPT.replace(
            "tell application \"Finder\" to delete targetItem",
            "return POSIX path of targetItem",
        );
        assert_ne!(script, MACOS_TRASH_SCRIPT);
        let output = checked_output(
            Command::new("/usr/bin/osascript")
                .args(["-e", &script])
                .arg(&app),
        )
        .unwrap();
        assert_eq!(
            std::fs::canonicalize(String::from_utf8(output.stdout).unwrap().trim()).unwrap(),
            std::fs::canonicalize(&app).unwrap()
        );
        assert!(
            checked_output(
                Command::new("/usr/bin/osascript")
                    .args(["-e", &script])
                    .arg(root.join("missing.app"))
            )
            .is_err()
        );
        assert!(app.is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "Moves only generated fixtures to Finder Trash; requires a desktop session"]
    fn finder_trashes_disposable_fixtures() {
        let root =
            std::env::temp_dir().join(format!("appclear-trash-fixtures-{}", std::process::id()));
        let app = root.join("AppClear 测试 \"IDEA\".app");
        let file = root.join("AppClear 测试 'file'.txt");
        std::fs::create_dir_all(app.join("Contents")).unwrap();
        std::fs::write(app.join("Contents/fixture"), b"test app").unwrap();
        std::fs::write(&file, b"test file").unwrap();
        for path in [&app, &file] {
            move_to_trash(path).unwrap();
            assert!(!path.exists(), "Fixture was not moved: {}", path.display());
        }
        // Finder may leave .DS_Store in the fixture's parent directory.
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn missing_protected_paths_resolve_existing_directory_aliases() {
        let root = std::env::temp_dir().join(format!("appclear-paths-{}", std::process::id()));
        std::fs::create_dir_all(root.join("actual/keep")).unwrap();
        std::os::unix::fs::symlink(root.join("actual"), root.join("alias")).unwrap();
        let actual = std::fs::canonicalize(root.join("actual")).unwrap();
        assert_eq!(
            normalized_absolute(&root.join("alias/keep/../missing/child")).unwrap(),
            actual.join("missing/child")
        );
        assert_eq!(
            normalized_absolute(&root.join("alias/keep")).unwrap(),
            actual.join("keep")
        );
        assert!(normalized_absolute(Path::new("relative/missing")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

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
