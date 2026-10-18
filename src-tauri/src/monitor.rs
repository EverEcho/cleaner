use crate::platform::QueryCommand;
use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    sync::{LazyLock, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Default, Serialize)]
pub struct MonitorSnapshot {
    pub sampled_at: u64,
    pub cpu_percent: Option<f64>,
    pub memory_total: Option<u64>,
    pub memory_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub disk_available: Option<u64>,
    pub disk_path: String,
    pub warnings: Vec<String>,
}
#[derive(Default)]
struct State {
    cpu: Option<(u64, u64)>,
    last: Option<(Instant, MonitorSnapshot)>,
    disk: Option<(Instant, u64, u64)>,
}
static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));
fn cpu_ticks(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let v: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .take(8)
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if v.len() < 4 {
        return None;
    }
    Some((v.iter().sum(), v[3] + v.get(4).copied().unwrap_or(0)))
}
fn cpu_delta(previous: (u64, u64), current: (u64, u64)) -> Option<f64> {
    let total = current.0.checked_sub(previous.0)?;
    let idle = current.1.checked_sub(previous.1)?;
    if total == 0 || idle > total {
        return None;
    }
    Some((100.0 * (total - idle) as f64 / total as f64).clamp(0.0, 100.0))
}
fn linux_memory(text: &str) -> Option<(u64, u64)> {
    let rows: HashMap<_, _> = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((
                key,
                value
                    .split_whitespace()
                    .next()?
                    .parse::<u64>()
                    .ok()?
                    .checked_mul(1024)?,
            ))
        })
        .collect();
    let total = *rows.get("MemTotal")?;
    let available = *rows.get("MemAvailable")?;
    if available > total {
        return None;
    }
    Some((total, total - available))
}
fn disk(text: &str) -> Option<(u64, u64)> {
    let fields: Vec<_> = text.lines().nth(1)?.split_whitespace().collect();
    let total = fields.get(1)?.parse::<u64>().ok()?.checked_mul(1024)?;
    let available = fields.get(3)?.parse::<u64>().ok()?.checked_mul(1024)?;
    if total == 0 || available > total {
        return None;
    }
    Some((total, available))
}
fn mac_cpu(text: &str) -> Option<f64> {
    let idle = text
        .lines()
        .rev()
        .find(|line| line.starts_with("CPU usage:"))?
        .split(',')
        .find(|part| part.contains("idle"))?
        .split('%')
        .next()?
        .split_whitespace()
        .last()?
        .parse::<f64>()
        .ok()?;
    idle.is_finite().then(|| (100.0 - idle).clamp(0.0, 100.0))
}
fn read_command(program: &str, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .query_output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("{program} 读取失败"));
    }
    if output.stdout.len() > 1024 * 1024 {
        return Err("监控响应超过大小限制".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
pub fn sample() -> Result<MonitorSnapshot, String> {
    let mut state = STATE.lock().map_err(|e| e.to_string())?;
    if let Some((time, snapshot)) = &state.last
        && time.elapsed().as_secs_f64() < 1.0
    {
        return Ok(snapshot.clone());
    }
    let mut snapshot = MonitorSnapshot {
        sampled_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        ..Default::default()
    };
    if cfg!(target_os = "linux") {
        match fs::read_to_string("/proc/stat")
            .ok()
            .and_then(|text| cpu_ticks(&text))
        {
            Some(ticks) => {
                snapshot.cpu_percent = state.cpu.and_then(|previous| cpu_delta(previous, ticks));
                state.cpu = Some(ticks);
            }
            None => snapshot.warnings.push("CPU 统计不可读取".into()),
        }
        match fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|text| linux_memory(&text))
        {
            Some((total, used)) => {
                snapshot.memory_total = Some(total);
                snapshot.memory_used = Some(used);
            }
            None => snapshot.warnings.push("内存统计不可读取".into()),
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(text) = read_command(
            "/usr/bin/top",
            &["-l", "2", "-s", "1", "-n", "0", "-F", "-R"],
        ) {
            snapshot.cpu_percent = mac_cpu(&text);
        }
        let total = read_command("/usr/sbin/sysctl", &["-n", "hw.memsize"])
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok());
        if let (Some(total), Ok(stats)) = (total, read_command("/usr/bin/vm_stat", &[])) {
            let page = stats
                .lines()
                .next()
                .and_then(|line| line.split("page size of ").nth(1))
                .and_then(|s| s.split_whitespace().next())
                .and_then(|s| s.parse::<u64>().ok());
            let free = stats
                .lines()
                .find(|line| line.starts_with("Pages free:"))
                .and_then(|line| line.split(':').nth(1))
                .and_then(|s| s.trim().trim_end_matches('.').parse::<u64>().ok());
            if let (Some(page), Some(free)) = (page, free) {
                snapshot.memory_total = Some(total);
                snapshot.memory_used = Some(total.saturating_sub(page.saturating_mul(free)));
                snapshot
                    .warnings
                    .push("macOS 内存按非空闲页面统计，包含可回收缓存；不等同于内存压力".into());
            }
        }
        if snapshot.cpu_percent.is_none() {
            snapshot.warnings.push("CPU 统计不可读取".into());
        }
        if snapshot.memory_total.is_none() {
            snapshot.warnings.push("内存统计不可读取".into());
        }
    } else {
        snapshot.warnings.push("当前系统尚未提供资源监控".into());
    }
    snapshot.disk_path = crate::platform::home()
        .unwrap_or_else(|| "/".into())
        .to_string_lossy()
        .into_owned();
    if state
        .disk
        .as_ref()
        .is_none_or(|(time, _, _)| time.elapsed().as_secs() >= 30)
    {
        let args = if cfg!(target_os = "macos") {
            vec!["-kP", snapshot.disk_path.as_str()]
        } else {
            vec!["-kP", "--", snapshot.disk_path.as_str()]
        };
        match read_command("df", &args).ok().and_then(|s| disk(&s)) {
            Some((total, available)) => state.disk = Some((Instant::now(), total, available)),
            None => {
                state.disk = None;
                snapshot
                    .warnings
                    .push("用户目录所在磁盘统计不可读取".into());
            }
        }
    }
    if let Some((_, total, available)) = state.disk {
        snapshot.disk_total = Some(total);
        snapshot.disk_available = Some(available);
    }
    state.last = Some((Instant::now(), snapshot.clone()));
    Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_excludes_guest_double_count_and_uses_delta() {
        assert_eq!(cpu_ticks("cpu  10 2 3 100 5 1 2 0 50 0"), Some((123, 105)));
        assert_eq!(cpu_delta((100, 80), (200, 100)), Some(80.));
        assert_eq!(cpu_delta((200, 100), (100, 80)), None);
    }
    #[test]
    fn mac_cpu_uses_latest_interval_and_rejects_unknown() {
        assert_eq!(
            mac_cpu("CPU usage: 0% user, 0% sys, 100% idle\nCPU usage: 12% user, 8% sys, 80% idle"),
            Some(20.0)
        );
        assert_eq!(mac_cpu("CPU usage: N/A"), None);
        assert_eq!(mac_cpu("CPU usage: NaN% idle"), None);
    }
    #[test]
    fn memory_and_disk_unknown_stay_unknown() {
        assert_eq!(
            linux_memory("MemTotal: 100 kB\nMemAvailable: 40 kB"),
            Some((102400, 61440))
        );
        assert_eq!(linux_memory("MemTotal: 100 kB"), None);
        assert_eq!(
            disk("Filesystem 1024-blocks Used Available Capacity Mounted\n/dev/x 100 20 80 20% /"),
            Some((102400, 81920))
        );
    }
}
