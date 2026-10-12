use std::fs;
use std::path::Path;
use crate::models::AutostartItem;

pub fn scan_autostart_items() -> Vec<AutostartItem> {
    let mut items = Vec::new();
    let home = std::env::var("HOME").unwrap_or_default();

    let mut dirs = Vec::new();
    if !home.is_empty() {
        dirs.push((format!("{}/.config/autostart", home), true));
    }
    dirs.push(("/etc/xdg/autostart".to_string(), false));

    for (dir_path, is_user) in dirs {
        let dir = Path::new(&dir_path);
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let mut name = String::new();
                        let mut exec = String::new();
                        let mut icon = String::new();
                        let mut comment = String::new();
                        let mut enabled = true;

                        for line in content.lines() {
                            let line = line.trim();
                            if line.starts_with("Name=") && name.is_empty() {
                                name = line[5..].to_string();
                            } else if line.starts_with("Exec=") && exec.is_empty() {
                                exec = line[5..].to_string();
                            } else if line.starts_with("Icon=") && icon.is_empty() {
                                icon = line[5..].to_string();
                            } else if line.starts_with("Comment=") && comment.is_empty() {
                                comment = line[8..].to_string();
                            } else if line == "X-GNOME-Autostart-enabled=false" || line == "Hidden=true" {
                                enabled = false;
                            }
                        }

                        if name.is_empty() {
                            name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("未知").to_string();
                        }

                        items.push(AutostartItem {
                            path: path.to_string_lossy().to_string(),
                            name,
                            exec,
                            icon,
                            comment,
                            enabled,
                            is_user,
                        });
                    }
                }
            }
        }
    }

    items
}

pub fn toggle_autostart(path: &str, enable: bool) -> Result<String, String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("开机启动项文件不存在".to_string());
    }

    let content = fs::read_to_string(p).map_err(|e| e.to_string())?;
    let mut new_lines = Vec::new();
    let mut found = false;

    for line in content.lines() {
        if line.starts_with("X-GNOME-Autostart-enabled=") {
            new_lines.push(format!("X-GNOME-Autostart-enabled={}", enable));
            found = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if !found {
        new_lines.push(format!("X-GNOME-Autostart-enabled={}", enable));
    }

    let new_content = new_lines.join("\n") + "\n";
    fs::write(p, new_content).map_err(|e| format!("写入失败: {}", e))?;
    Ok("状态已更新".to_string())
}
