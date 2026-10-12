use crate::platform;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

pub fn application_dirs() -> Vec<(PathBuf, &'static str)> {
    let mut dirs = Vec::new();
    if let Some(data) = platform::xdg_dir("XDG_DATA_HOME", ".local/share") {
        dirs.push((data.join("applications"), "用户"));
        dirs.push((data.join("flatpak/exports/share/applications"), "Flatpak"));
    }
    let roots =
        std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(
        std::env::split_paths(&roots)
            .filter(|p| p.is_absolute())
            .map(|p| (p.join("applications"), "系统")),
    );
    dirs.push((
        "/var/lib/flatpak/exports/share/applications".into(),
        "Flatpak",
    ));
    dirs.push(("/var/lib/snapd/desktop/applications".into(), "Snap"));
    dirs
}

pub fn fields(text: &str) -> HashMap<&str, &str> {
    let mut in_main = false;
    let mut values = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
            continue;
        }
        if in_main
            && !line.starts_with('#')
            && let Some((k, v)) = line.split_once('=')
        {
            values.insert(k, v);
        }
    }
    values
}

pub fn rewrite(text: &str, values: &[(&str, &str)]) -> String {
    let mut output = Vec::new();
    let mut in_main = false;
    let mut inserted = false;
    for line in text.lines() {
        if line.starts_with('[') {
            if in_main {
                output.extend(values.iter().map(|(k, v)| format!("{k}={v}")));
                inserted = true;
            }
            in_main = line.trim() == "[Desktop Entry]";
        }
        if !in_main
            || !values
                .iter()
                .any(|(k, _)| line.trim().starts_with(&format!("{k}=")))
        {
            output.push(line.to_string());
        }
    }
    if in_main {
        output.extend(values.iter().map(|(k, v)| format!("{k}={v}")));
        inserted = true;
    }
    if !inserted {
        output.push("[Desktop Entry]".into());
        output.extend(values.iter().map(|(k, v)| format!("{k}={v}")));
    }
    output.join("\n") + "\n"
}

pub fn user_override(
    path: &Path,
    allowed_roots: &[PathBuf],
    user_root: &Path,
    values: &[(&str, &str)],
) -> Result<(), String> {
    if path.extension().is_none_or(|s| s != "desktop")
        || !allowed_roots
            .iter()
            .any(|root| path.parent() == Some(root.as_path()))
    {
        return Err("启动器不在允许的目录中".into());
    }
    if fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("不能修改符号链接启动器".into());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    fs::create_dir_all(user_root).map_err(|e| e.to_string())?;
    let target = user_root.join(path.file_name().ok_or("启动器文件名无效")?);
    if target
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("用户覆盖文件不能是符号链接".into());
    }
    fs::write(target, rewrite(&content, values)).map_err(|e| format!("保存失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preserves_action_sections_and_updates_both_visibility_keys() {
        let text = "[Desktop Entry]\nName=App\nHidden=true\nNoDisplay=true\n[Desktop Action New]\nName=New window\nExec=app --new\n";
        let edited = rewrite(text, &[("Hidden", "false"), ("NoDisplay", "false")]);
        assert_eq!(fields(&edited)["Hidden"], "false");
        assert_eq!(fields(&edited)["Name"], "App");
        assert!(edited.contains("[Desktop Action New]\nName=New window\nExec=app --new\n"));
    }
}
