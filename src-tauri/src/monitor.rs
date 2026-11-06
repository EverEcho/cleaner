use crate::platform::QueryCommand;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use sysinfo::{Networks, System};

#[derive(Clone, Debug, Default, Serialize)]
pub struct MonitorSnapshot {
    pub sampled_at: u64,
    pub cpu_percent: Option<f64>,
    pub memory_total: Option<u64>,
    pub memory_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub disk_available: Option<u64>,
    pub disk_path: String,
    pub network_down: Option<u64>,
    pub network_up: Option<u64>,
    pub network_interface: String,
    pub network_interfaces: Vec<String>,
    pub warnings: Vec<String>,
}
type Counters = HashMap<String, (u64, u64)>;
#[derive(Default)]
struct State {
    system: System,
    networks: Networks,
    cpu_primed: bool,
    network: Option<(Instant, String, Counters)>,
    last: Option<(Instant, String, MonitorSnapshot)>,
    disk: Option<(Instant, u64, u64)>,
}
static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));
fn disk(text: &str) -> Option<(u64, u64)> {
    let fields: Vec<_> = text.lines().nth(1)?.split_whitespace().collect();
    let total = fields.get(1)?.parse::<u64>().ok()?.checked_mul(1024)?;
    let available = fields.get(3)?.parse::<u64>().ok()?.checked_mul(1024)?;
    (total > 0 && available <= total).then_some((total, available))
}
fn physical_interface(name: &str) -> bool {
    // Physical Wi-Fi / Ethernet only. Tunnel, bridge and peer-to-peer counters would double count traffic.
    name.starts_with("en") || name.starts_with("eth") || name.starts_with("wl")
}
fn network_rates(previous: &Counters, current: &Counters, seconds: f64) -> Option<(u64, u64)> {
    if seconds <= 0.0
        || !seconds.is_finite()
        || current.is_empty()
        || previous.len() != current.len()
    {
        return None;
    }
    let (mut down, mut up) = (0u64, 0u64);
    for (name, &(rx, tx)) in current {
        let &(old_rx, old_tx) = previous.get(name)?;
        down = down.checked_add(rx.checked_sub(old_rx)?)?;
        up = up.checked_add(tx.checked_sub(old_tx)?)?;
    }
    Some((
        (down as f64 / seconds).round() as u64,
        (up as f64 / seconds).round() as u64,
    ))
}
fn read_disk(path: &str) -> Option<(u64, u64)> {
    let mut command = std::process::Command::new("df");
    command.arg("-kP");
    if !cfg!(target_os = "macos") {
        command.arg("--");
    }
    let out = command.arg(path).env("LC_ALL", "C").query_output().ok()?;
    if !out.status.success() || out.stdout.len() > 1024 * 1024 {
        return None;
    }
    disk(&String::from_utf8_lossy(&out.stdout))
}
pub fn sample(interface: &str) -> Result<MonitorSnapshot, String> {
    let mut state = STATE.lock().map_err(|e| e.to_string())?;
    if let Some((time, selection, snapshot)) = &state.last
        && selection == interface
        && time.elapsed().as_secs_f64() < 0.8
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
    // Native counters avoid spawning `top` and waiting a second for every menu-bar update.
    state.system.refresh_cpu_usage();
    state.system.refresh_memory();
    if state.cpu_primed && !state.system.cpus().is_empty() {
        let value = state.system.global_cpu_usage() as f64;
        if value.is_finite() {
            snapshot.cpu_percent = Some(value.clamp(0.0, 100.0));
        }
    }
    state.cpu_primed = true;
    let total = state.system.total_memory();
    if total > 0 {
        snapshot.memory_total = Some(total);
        snapshot.memory_used = Some(state.system.used_memory().min(total));
    } else {
        snapshot.warnings.push("内存统计不可读取".into());
    }
    if cfg!(target_os = "macos") {
        snapshot
            .warnings
            .push("macOS 内存为系统已用内存统计；百分比不等同于内存压力。".into());
    }
    state.networks.refresh(true);
    snapshot.network_interfaces = state
        .networks
        .iter()
        .map(|(name, _)| name.clone())
        .filter(|name| name != "lo" && name != "lo0")
        .collect();
    snapshot.network_interfaces.sort();
    let counters: Counters = state
        .networks
        .iter()
        .filter(|(name, _)| {
            if interface == "auto" {
                physical_interface(name)
            } else {
                name.as_str() == interface
            }
        })
        .map(|(name, data)| {
            (
                name.clone(),
                (data.total_received(), data.total_transmitted()),
            )
        })
        .collect();
    let mut active: Vec<_> = counters.keys().cloned().collect();
    active.sort();
    snapshot.network_interface = active.join(" + ");
    let now = Instant::now();
    if let Some((time, selection, previous)) = &state.network
        && selection == interface
        && let Some((down, up)) = network_rates(previous, &counters, time.elapsed().as_secs_f64())
    {
        snapshot.network_down = Some(down);
        snapshot.network_up = Some(up);
    }
    if counters.is_empty() {
        snapshot.warnings.push(if interface == "auto" {
            "未找到物理网卡，可在菜单栏设置中指定网卡。".into()
        } else {
            format!("网卡 {interface} 当前不可用，等待重新连接。")
        });
    }
    state.network = Some((now, interface.into(), counters));
    snapshot.disk_path = crate::platform::home()
        .unwrap_or_else(|| "/".into())
        .to_string_lossy()
        .into_owned();
    if state
        .disk
        .as_ref()
        .is_none_or(|(time, _, _)| time.elapsed().as_secs() >= 30)
    {
        state.disk = read_disk(&snapshot.disk_path)
            .map(|(total, available)| (Instant::now(), total, available));
        if state.disk.is_none() {
            snapshot
                .warnings
                .push("用户目录所在磁盘统计不可读取".into());
        }
    }
    if let Some((_, total, available)) = state.disk {
        snapshot.disk_total = Some(total);
        snapshot.disk_available = Some(available);
    }
    state.last = Some((Instant::now(), interface.into(), snapshot.clone()));
    Ok(snapshot)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn counters(rows: &[(&str, u64, u64)]) -> Counters {
        rows.iter()
            .map(|(name, rx, tx)| (name.to_string(), (*rx, *tx)))
            .collect()
    }
    #[test]
    fn traffic_is_a_real_time_delta_and_sums_physical_interfaces() {
        let before = counters(&[("en0", 100, 300), ("en1", 200, 500)]);
        let after = counters(&[("en0", 500, 500), ("en1", 800, 700)]);
        assert_eq!(network_rates(&before, &after, 2.0), Some((500, 200)));
        assert!(physical_interface("en0"));
        assert!(physical_interface("eth0"));
        for name in ["lo0", "utun0", "awdl0", "bridge0", "llw0"] {
            assert!(!physical_interface(name));
        }
    }
    #[test]
    fn network_warmup_reset_disconnect_and_adapter_changes_stay_unknown() {
        let old = counters(&[("en0", 200, 200)]);
        assert_eq!(
            network_rates(&old, &counters(&[("en0", 100, 300)]), 1.0),
            None
        );
        assert_eq!(
            network_rates(&old, &counters(&[("en1", 300, 300)]), 1.0),
            None
        );
        assert_eq!(network_rates(&old, &Counters::new(), 1.0), None);
        assert_eq!(network_rates(&old, &old, 0.0), None);
        assert_eq!(network_rates(&old, &old, 2.0), Some((0, 0)));
    }
    #[test]
    fn disk_unknown_stays_unknown() {
        assert_eq!(
            disk("Filesystem 1024-blocks Used Available Capacity Mounted\n/dev/x 100 20 80 20% /"),
            Some((102400, 81920))
        );
        assert_eq!(disk("invalid"), None);
    }
    #[test]
    fn native_sampler_returns_bounded_resource_values_without_subprocess_cpu_queries() {
        let first = sample("auto").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(950));
        let second = sample("auto").unwrap();
        assert!(second.sampled_at > first.sampled_at);
        assert!(second.memory_total.is_some_and(|total| total > 0));
        assert!(second.memory_used <= second.memory_total);
        assert!(
            second
                .cpu_percent
                .is_some_and(|cpu| cpu.is_finite() && (0.0..=100.0).contains(&cpu))
        );
        assert_eq!(sample("auto").unwrap().sampled_at, second.sampled_at);
    }
}
