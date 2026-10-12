//! Read-only system cleanup plans, shared by the GUI and the root-owned helper.
use crate::query::QueryCommand;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::UNIX_EPOCH,
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanTarget {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub modified_ns: u128,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemPlan {
    pub id: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub files: Vec<ScanTarget>,
    pub size_bytes: u64,
    pub error: Option<String>,
}
impl SystemPlan {
    pub fn fingerprint(&self) -> String {
        hex::encode(Sha256::digest(
            serde_json::to_vec(&(self.id.as_str(), self.size_bytes, &self.files))
                .unwrap_or_default(),
        ))
    }
}

fn plan(
    id: &str,
    title: &str,
    description: &str,
    location: &str,
    result: Result<Vec<ScanTarget>, String>,
) -> SystemPlan {
    match result {
        Ok(mut files) => {
            files.sort_by(|a, b| a.path.cmp(&b.path));
            let size_bytes = files.iter().map(|f| f.size_bytes).sum();
            SystemPlan {
                id: id.into(),
                title: title.into(),
                description: description.into(),
                location: location.into(),
                files,
                size_bytes,
                error: None,
            }
        }
        Err(error) => SystemPlan {
            id: id.into(),
            title: title.into(),
            description: description.into(),
            location: location.into(),
            files: Vec::new(),
            size_bytes: 0,
            error: Some(error),
        },
    }
}
fn target(path: &Path, allocated: bool) -> Result<ScanTarget, String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let size_bytes = if meta.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if allocated {
                meta.blocks().saturating_mul(512)
            } else {
                meta.len()
            }
        }
        #[cfg(not(unix))]
        {
            let _ = allocated;
            meta.len()
        }
    } else {
        0
    };
    Ok(ScanTarget {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size_bytes,
        modified_ns: meta
            .modified()
            .map_err(|e| e.to_string())?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    })
}
fn command(program: &str, args: &[&str]) -> Result<std::process::Output, String> {
    let output = Command::new(program)
        .args(args)
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("SYSTEMD_LOG_LEVEL", "info")
        .query_output()
        .map_err(|e| format!("{program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output)
}

fn apt_files() -> Result<Vec<ScanTarget>, String> {
    let root = Path::new("/var/cache/apt/archives");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().is_file() && entry.file_name() != "lock" {
            files.push(target(entry.path(), false)?);
        }
    }
    Ok(files)
}

fn archive_order(path: &Path) -> Option<(String, u64, u64)> {
    let name = path.file_name()?.to_str()?;
    let (_, suffix) = name.rsplit_once('@')?;
    let suffix = suffix.strip_suffix(".journal")?;
    let parts: Vec<_> = suffix.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 32 {
        return None;
    }
    Some((
        parts[0].into(),
        u64::from_str_radix(parts[1], 16).ok()?,
        u64::from_str_radix(parts[2], 16).ok()?,
    ))
}

pub fn journal_files_in(roots: &[PathBuf], limit: u64) -> Result<Vec<ScanTarget>, String> {
    type JournalFiles = (u64, Vec<(ScanTarget, String, u64, u64)>);
    let mut directories: HashMap<PathBuf, JournalFiles> = HashMap::new();
    for root in roots.iter().filter(|r| r.exists()) {
        for entry in WalkDir::new(root) {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy();
            if !name.ends_with(".journal") && !name.ends_with(".journal~") {
                continue;
            }
            let record = target(path, true)?;
            let data = directories
                .entry(path.parent().unwrap_or(root).to_path_buf())
                .or_default();
            data.0 += record.size_bytes;
            if let Some((id, sequence, timestamp)) = archive_order(path) {
                data.1.push((record, id, sequence, timestamp));
            } else if name.ends_with(".journal~") && name.contains('@') {
                let timestamp = record.modified_ns as u64 / 1000;
                data.1.push((record, String::new(), 0, timestamp));
            }
        }
    }
    let mut files = Vec::new();
    for (_, (mut total, mut archived)) in directories {
        archived.sort_by(|a, b| {
            if a.1 == b.1 && !a.1.is_empty() {
                a.2.cmp(&b.2)
            } else {
                a.3.cmp(&b.3)
            }
        });
        for (record, _, _, _) in archived {
            if total <= limit {
                break;
            }
            total = total.saturating_sub(record.size_bytes);
            files.push(record);
        }
    }
    Ok(files)
}

pub fn tmpfiles_paths(text: &str) -> Result<Vec<PathBuf>, String> {
    let mut paths = HashSet::new();
    for line in text.lines().filter(|l| l.contains("Would remove")) {
        let start = line.find('"').ok_or("临时文件预览格式无法识别")?;
        let end = line
            .rfind('"')
            .filter(|end| *end > start)
            .ok_or("临时文件预览格式无法识别")?;
        let path = PathBuf::from(&line[start + 1..end]);
        if !(path.starts_with("/tmp") || path.starts_with("/var/tmp"))
            || path == Path::new("/tmp")
            || path == Path::new("/var/tmp")
        {
            return Err("临时文件预览超出允许范围".into());
        }
        paths.insert(path);
    }
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort();
    Ok(paths)
}
fn temp_files() -> Result<Vec<ScanTarget>, String> {
    let output = command(
        "/usr/bin/systemd-tmpfiles",
        &[
            "--clean",
            "--dry-run",
            "--prefix=/tmp",
            "--prefix=/var/tmp",
            "--no-pager",
        ],
    )?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    tmpfiles_paths(&text)?
        .iter()
        .map(|p| target(p, false))
        .collect()
}

pub fn scan_linux() -> Vec<SystemPlan> {
    let mut plans = Vec::new();
    if Path::new("/usr/bin/apt-get").exists() {
        plans.push(plan(
            "apt",
            "安装包缓存",
            "清理已下载的安装包，保留已安装软件",
            "/var/cache/apt/archives",
            apt_files(),
        ));
    }
    if Path::new("/usr/bin/journalctl").exists() {
        plans.push(plan(
            "journal",
            "系统历史日志",
            "按500MB保留规则清理归档日志；不轮转、不删除当前活动日志",
            "/var/log/journal；/run/log/journal",
            journal_files_in(
                &["/var/log/journal".into(), "/run/log/journal".into()],
                500 * 1024 * 1024,
            ),
        ));
    }
    if Path::new("/usr/bin/systemd-tmpfiles").exists() {
        plans.push(plan(
            "tmp",
            "过期临时文件",
            "由系统规则 Dry Run 列出过期对象，仅清理 /tmp 和 /var/tmp",
            "/tmp；/var/tmp",
            temp_files(),
        ));
    }
    if Path::new("/usr/bin/docker").exists() {
        plans.extend(scan_docker("/var/run/docker.sock", "/usr/bin/docker"));
    }
    plans
}

fn curl_json(socket: &str, resource: &str) -> Result<Value, String> {
    let url = format!("http://localhost/{resource}");
    let output = command(
        "/usr/bin/curl",
        &[
            "--disable",
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "20",
            "--noproxy",
            "*",
            "--unix-socket",
            socket,
            &url,
        ],
    )?;
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
fn byte_count(value: &str) -> Result<u64, String> {
    let value = value
        .split('(')
        .next()
        .unwrap_or(value)
        .trim()
        .replace(' ', "");
    let split = value
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(value.len());
    let number = value[..split].parse::<f64>().map_err(|e| e.to_string())?;
    let factor = match value[split..].to_ascii_lowercase().as_str() {
        "b" | "" => 1.,
        "kb" => 1000.,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "kib" => 1024.,
        "mib" => 1048576.,
        "gib" => 1073741824.,
        _ => return Err("Docker 大小单位无法识别".into()),
    };
    if !number.is_finite() || number < 0. {
        return Err("Docker 大小无效".into());
    }
    Ok((number * factor).round() as u64)
}

pub fn docker_plans(
    data: &Value,
    summary: &str,
    location: &str,
) -> Result<Vec<SystemPlan>, String> {
    let summaries: HashMap<String, u64> = summary
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
            let kind = row["Type"]
                .as_str()
                .ok_or("Docker 汇总类型缺失")?
                .to_string();
            let size = byte_count(row["Reclaimable"].as_str().ok_or("Docker 可回收空间缺失")?)?;
            Ok((kind, size))
        })
        .collect::<Result<_, String>>()?;
    let mut plans = Vec::new();
    for (id, title, section, kind, description) in [
        (
            "docker-containers",
            "已停止的容器",
            "Containers",
            "Containers",
            "执行 docker container prune，删除已停止的容器及其可写层；保留数据卷",
        ),
        (
            "docker-images",
            "未使用的镜像",
            "Images",
            "Images",
            "执行 docker image prune -a，删除未被容器引用的镜像；以后使用时需重新拉取",
        ),
        (
            "docker-builder",
            "未使用的构建缓存",
            "BuildCache",
            "Build Cache",
            "执行 docker builder prune -a，删除未使用的构建缓存，后续构建可能变慢",
        ),
    ] {
        let rows = data[section]
            .as_array()
            .ok_or_else(|| format!("Docker {section} 明细缺失"))?;
        let mut files = Vec::new();
        for row in rows {
            let eligible = match section {
                "Containers" => row["State"]
                    .as_str()
                    .is_some_and(|s| ["exited", "created", "dead"].contains(&s)),
                "Images" => row["Containers"].as_i64() == Some(0),
                _ => row["InUse"].as_bool() == Some(false),
            };
            if !eligible {
                continue;
            }
            let key = row["Id"]
                .as_str()
                .or(row["ID"].as_str())
                .ok_or("Docker 对象标识缺失")?;
            let name = match section {
                "Containers" => row["Names"].get(0).and_then(Value::as_str),
                "Images" => row["RepoTags"].get(0).and_then(Value::as_str),
                _ => row["Description"].as_str(),
            }
            .unwrap_or(key);
            let size = if section == "Containers" {
                row["SizeRw"].as_u64()
            } else {
                row["Size"].as_u64()
            }
            .ok_or("Docker 对象大小缺失")?;
            files.push(ScanTarget {
                path: format!("docker://{section}/{key}"),
                name: name.into(),
                size_bytes: size,
                modified_ns: 0,
            });
        }
        let mut item = plan(id, title, description, location, Ok(files));
        // Use the daemon's reclaimable aggregate; never sum image virtual sizes/shared layers.
        item.size_bytes = *summaries
            .get(kind)
            .ok_or_else(|| format!("Docker {kind} 可回收统计缺失"))?;
        plans.push(item);
    }
    Ok(plans)
}

pub fn scan_docker(socket: &str, docker: &str) -> Vec<SystemPlan> {
    let result = (|| {
        let version = curl_json(socket, "version")?;
        let api = version["ApiVersion"]
            .as_str()
            .ok_or("Docker API 版本缺失")?;
        let minor = api
            .strip_prefix("1.")
            .and_then(|s| s.parse::<u32>().ok())
            .ok_or("Docker API 版本无法识别")?;
        let endpoint = format!("v1.{}/system/df", minor.min(44));
        let data = curl_json(socket, &endpoint)?;
        let info = curl_json(socket, "info")?;
        let location = info["DockerRootDir"]
            .as_str()
            .ok_or("Docker 存储目录缺失")?;
        let host = format!("unix://{socket}");
        let summary = command(
            docker,
            &["--host", &host, "system", "df", "--format", "{{json .}}"],
        )?;
        docker_plans(&data, &String::from_utf8_lossy(&summary.stdout), location)
    })();
    result.unwrap_or_else(|error| {
        vec![plan(
            "docker",
            "容器垃圾",
            "Docker 扫描未完成，请确认服务已启动且已授权",
            socket,
            Err(error),
        )]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journal_plan_excludes_active_logs_and_keeps_newest_archives() {
        let root =
            std::env::temp_dir().join(format!("appsteward-journal-test-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let old = root.join(
            "system@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-0000000000000001-0000000000000001.journal",
        );
        let new = root.join(
            "system@aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-0000000000000002-0000000000000002.journal",
        );
        let active = root.join("system.journal");
        for path in [&old, &new, &active] {
            fs::write(path, [1u8; 4096]).unwrap();
        }
        let occupied = target(&old, true).unwrap().size_bytes;
        let limit =
            target(&active, true).unwrap().size_bytes + target(&new, true).unwrap().size_bytes;
        let files = journal_files_in(std::slice::from_ref(&root), limit).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, old.to_string_lossy());
        assert_eq!(files[0].size_bytes, occupied);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tmpfiles_preview_uses_only_actual_removal_candidates() {
        let paths = tmpfiles_paths("Would remove file \"/tmp/a file\".\nWould remove directory \"/var/tmp/old\"\nWould restore access and modification time on \"/tmp\"\n").unwrap();
        assert_eq!(paths.len(), 2);
        assert!(tmpfiles_paths("Would remove \"/home/user/document\"").is_err());
    }
    #[test]
    fn docker_stats_preserve_volumes_and_do_not_sum_image_shared_sizes() {
        let data = serde_json::json!({"Images":[{"Id":"sha256:a","Containers":0,"Size":5000,"RepoTags":["app:old"]}],"Containers":[{"Id":"c1","State":"exited","SizeRw":500,"Names":["test"]},{"Id":"c2","State":"running","SizeRw":10000}],"BuildCache":[{"ID":"b1","InUse":false,"Size":200},{"ID":"b2","InUse":true,"Size":5000}],"Volumes":[{"Name":"database"}]});
        let summary = "{\"Type\":\"Images\",\"Reclaimable\":\"1.2kB (24%)\"}\n{\"Type\":\"Containers\",\"Reclaimable\":\"500B\"}\n{\"Type\":\"Build Cache\",\"Reclaimable\":\"200B\"}";
        let plans = docker_plans(&data, summary, "/var/lib/docker").unwrap();
        assert_eq!(plans.len(), 3);
        assert_eq!(plans[1].size_bytes, 1200);
        assert_eq!(plans[0].files.len(), 1);
        assert_eq!(plans[2].files.len(), 1);
        assert!(!plans.iter().any(|p| p.id.contains("volume")));
    }
}
