use crate::{models::format_size, platform};
use serde::Serialize;
use std::{path::Path, process::Command};

#[derive(Serialize)]
pub struct DirectoryUsage {
    pub path: String,
    pub size_bytes: u64,
    pub size: String,
}
#[derive(Serialize)]
pub struct DiskAnalysis {
    pub directories: Vec<DirectoryUsage>,
    pub warning: String,
}
pub fn roots() -> Vec<String> {
    let mut roots = Vec::new();
    if let Some(home) = platform::home() {
        roots.push(home.to_string_lossy().into_owned());
    }
    if cfg!(target_os = "linux") {
        roots.extend(
            [
                "/",
                "/var",
                "/var/log",
                "/var/cache",
                "/var/lib",
                "/tmp",
                "/var/tmp",
                "/home",
                "/var/crash",
                "/var/lib/systemd/coredump",
                "/usr/src",
                "/lib/modules",
            ]
            .map(str::to_string),
        );
    } else {
        roots.extend(["/Applications", "/Library", "/Users"].map(str::to_string));
    }
    roots.retain(|p| Path::new(p).is_dir());
    roots.dedup();
    roots
}

fn parse_du(text: &str, root: &str) -> Vec<DirectoryUsage> {
    let mut rows: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let (kb, path) = line.split_once('\t')?;
            if path == root {
                return None;
            }
            let size_bytes = kb.trim().parse::<u64>().ok()?.saturating_mul(1024);
            Some(DirectoryUsage {
                path: path.into(),
                size_bytes,
                size: format_size(size_bytes),
            })
        })
        .collect();
    rows.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    rows
}

pub fn analyze(root: &str) -> Result<DiskAnalysis, String> {
    if !Path::new(root).is_absolute() || !Path::new(root).is_dir() {
        return Err("请选择存在的目录".into());
    }
    let binary = platform::command_path("du").ok_or("系统未提供目录占用分析工具")?;
    let output = Command::new(binary)
        .args(["-x", "-k", "-d", "1", "--", root])
        .output()
        .map_err(|e| e.to_string())?;
    let warning = if output.status.success() {
        String::new()
    } else {
        "部分目录没有读取权限，当前显示可读取的结果；占用值可能不完整。".into()
    };
    Ok(DiskAnalysis {
        directories: parse_du(&String::from_utf8_lossy(&output.stdout), root),
        warning,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn du_parses_spaces_and_excludes_total() {
        let rows = parse_du("10\t/home/a folder\n20\t/home/other\n30\t/home\n", "/home");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].size_bytes, 20 * 1024);
        assert_eq!(rows[1].path, "/home/a folder");
    }
}
