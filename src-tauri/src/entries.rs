use std::collections::HashSet;
use std::fs;
use std::path::Path;
use crate::models::DesktopEntryItem;

pub fn scan_desktop_entries() -> Vec<DesktopEntryItem> {
    let mut items = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    let mut search_dirs = Vec::new();
    if !home.is_empty() {
        search_dirs.push((format!("{}/.local/share/applications", home), "用户"));
    }
    search_dirs.push(("/usr/share/applications".to_string(), "系统"));
    search_dirs.push(("/usr/local/share/applications".to_string(), "系统"));
    search_dirs.push(("/var/lib/flatpak/exports/share/applications".to_string(), "Flatpak"));
    if !home.is_empty() {
        search_dirs.push((format!("{}/.local/share/flatpak/exports/share/applications", home), "Flatpak"));
    }
    search_dirs.push(("/var/lib/snapd/desktop/applications".to_string(), "Snap"));

    let mut visited = HashSet::new();

    for (dir_str, scope) in search_dirs {
        let dir = Path::new(&dir_str);
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    let basename = path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                    if visited.contains(&basename) {
                        continue;
                    }
                    visited.insert(basename.clone());

                    if let Ok(content) = fs::read_to_string(&path) {
                        let mut name = String::new();
                        let mut exec = String::new();
                        let mut icon = String::new();
                        let mut visible = true;
                        let mut categories = String::new();

                        for line in content.lines() {
                            let line = line.trim();
                            if line.starts_with("Name=") && name.is_empty() {
                                name = line[5..].to_string();
                            } else if line.starts_with("Exec=") && exec.is_empty() {
                                exec = line[5..].to_string();
                            } else if line.starts_with("Icon=") && icon.is_empty() {
                                icon = line[5..].to_string();
                            } else if line == "NoDisplay=true" || line == "Hidden=true" {
                                visible = false;
                            } else if line.starts_with("Categories=") {
                                categories = line[11..].to_string();
                            }
                        }

                        if name.is_empty() {
                            name = basename.replace(".desktop", "");
                        }

                        items.push(DesktopEntryItem {
                            path: path.to_string_lossy().to_string(),
                            basename,
                            name,
                            exec,
                            icon,
                            scope: scope.to_string(),
                            visible,
                            categories,
                        });
                    }
                }
            }
        }
    }

    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    items
}

pub fn toggle_desktop_entry_visibility(path: &str, visible: bool) -> Result<String, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("文件不存在".to_string());
    }

    let content = fs::read_to_string(p).map_err(|e| e.to_string())?;
    let mut new_lines = Vec::new();
    let mut replaced = false;

    for line in content.lines() {
        if line.starts_with("NoDisplay=") {
            new_lines.push(format!("NoDisplay={}", !visible));
            replaced = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !replaced {
        new_lines.push(format!("NoDisplay={}", !visible));
    }

    let new_content = new_lines.join("\n") + "\n";

    if let Err(_) = fs::write(p, &new_content) {
        // If write fails (system permission), try pkexec tee
        use std::process::Command;
        use std::io::Write;
        let mut child = Command::new("pkexec")
            .args(["tee", path])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("提权写入失败: {}", e))?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(new_content.as_bytes());
        }
        child.wait().map_err(|e| format!("保存失败: {}", e))?;
    }

    Ok("已更新启动器可见性".to_string())
}
