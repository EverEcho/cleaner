use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use tauri::Manager;
static SAVE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub scan_roots: Vec<String>,
    pub exclusions: Vec<String>,
    pub developer_roots: Vec<String>,
    pub developer_days: u64,
    pub large_min_mb: u64,
    pub monitor_enabled: bool,
    pub tray: TrayPreferences,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayMetric {
    Cpu,
    Memory,
    Download,
    Upload,
    Disk,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TrayPreferences {
    pub enabled: bool,
    pub metrics: Vec<TrayMetric>,
    pub show_icon: bool,
    pub labeled: bool,
    pub interval_ms: u64,
    pub network_interface: String,
    pub rate_unit: String,
    pub close_to_tray: bool,
    pub show_sparklines: bool,
}
impl Default for TrayPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics: vec![TrayMetric::Download, TrayMetric::Upload],
            show_icon: true,
            labeled: false,
            interval_ms: 2000,
            network_interface: "auto".into(),
            rate_unit: "bytes".into(),
            close_to_tray: true,
            show_sparklines: true,
        }
    }
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            scan_roots: Vec::new(),
            exclusions: Vec::new(),
            developer_roots: Vec::new(),
            developer_days: 7,
            large_min_mb: 50,
            monitor_enabled: true,
            tray: TrayPreferences::default(),
        }
    }
}
fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("preferences.json"))
}
pub fn load(app: &tauri::AppHandle) -> Result<Preferences, String> {
    read(&path(app)?)
}
fn read(path: &Path) -> Result<Preferences, String> {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data).map_err(|e| format!("设置文件读取失败: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(app: &tauri::AppHandle, value: Preferences) -> Result<Preferences, String> {
    write(&path(app)?, value)
}
fn validate(value: &Preferences) -> Result<(), String> {
    let tray = &value.tray;
    if ![1000, 2000, 3000, 5000].contains(&tray.interval_ms)
        || !["bytes", "bits"].contains(&tray.rate_unit.as_str())
        || tray.network_interface.is_empty()
        || tray.network_interface.len() > 128
        || tray.network_interface.chars().any(char::is_control)
        || tray.metrics.len() > 5
        || tray
            .metrics
            .iter()
            .enumerate()
            .any(|(i, metric)| tray.metrics[..i].contains(metric))
        || (!tray.show_icon && tray.metrics.is_empty())
    {
        return Err("菜单栏设置无效，请保留图标或至少一个指标".into());
    }
    if value.developer_days == 0
        || value.developer_days > 3650
        || value.large_min_mb == 0
        || value.large_min_mb > 1_000_000
    {
        return Err("设置数值超出支持范围".into());
    }
    for paths in [&value.scan_roots, &value.exclusions, &value.developer_roots] {
        if paths.len() > 64 {
            return Err("最多保存64个目录".into());
        }
        for p in paths {
            if p.len() > 4096 || !Path::new(p).is_absolute() || p.contains('\0') {
                return Err("目录必须是有效绝对路径".into());
            }
        }
    }
    Ok(())
}
fn write(path: &Path, mut value: Preferences) -> Result<Preferences, String> {
    validate(&value)?;
    for paths in [
        &mut value.scan_roots,
        &mut value.exclusions,
        &mut value.developer_roots,
    ] {
        paths.sort();
        paths.dedup();
    }
    let _guard = SAVE_LOCK.lock().map_err(|e| e.to_string())?;
    let parent = path.parent().ok_or("设置目录无效")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let data = serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?;
    fs::write(&tmp, data).map_err(|e| e.to_string())?;
    fs::rename(tmp, path).map_err(|e| e.to_string())?;
    Ok(value)
}
pub fn pick_directory(title: Option<String>) -> Result<Option<String>, String> {
    let title = title.unwrap_or_else(|| "选择扫描目录".into());
    let mut command = if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("/usr/bin/osascript");
        c.args([
            "-e",
            "on run argv\nPOSIX path of (choose folder with prompt (item 1 of argv))\nend run",
            "--",
            &title,
        ]);
        c
    } else if let Some(bin) = crate::platform::command_path("zenity") {
        let mut c = std::process::Command::new(bin);
        c.args(["--file-selection", "--directory", "--title", &title]);
        c
    } else if let Some(bin) = crate::platform::command_path("kdialog") {
        let mut c = std::process::Command::new(bin);
        c.args([
            "--getexistingdirectory",
            &crate::platform::home()
                .unwrap_or_else(|| "/".into())
                .to_string_lossy(),
            "--title",
            &title,
        ]);
        c
    } else {
        return Err("系统没有目录选择器，请输入绝对路径".into());
    };
    let out = command.output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        if out.status.code() == Some(1) || String::from_utf8_lossy(&out.stderr).contains("-128") {
            return Ok(None);
        }
        return Err(format!(
            "目录选择器失败: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if value.is_empty() {
        return Ok(None);
    }
    let canonical = fs::canonicalize(&value).map_err(|e| e.to_string())?;
    if !canonical.is_dir() {
        return Err("请选择目录".into());
    }
    Ok(Some(canonical.to_string_lossy().into_owned()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_settings_gain_tray_defaults_and_keep_existing_preferences() {
        let value: Preferences =
            serde_json::from_str(r#"{"monitor_enabled":false,"exclusions":["/keep"]}"#).unwrap();
        assert!(!value.monitor_enabled);
        assert_eq!(value.exclusions, vec!["/keep"]);
        assert_eq!(
            value.tray.metrics,
            vec![TrayMetric::Download, TrayMetric::Upload]
        );
        assert_eq!(value.tray.interval_ms, 2000);
        assert!(validate(&value).is_ok());
    }
    #[test]
    fn tray_configuration_validates_visibility_order_and_sampling_range() {
        let mut value = Preferences::default();
        value.tray.metrics = vec![TrayMetric::Memory, TrayMetric::Cpu];
        let roundtrip: Preferences =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(roundtrip.tray.metrics, value.tray.metrics);
        value.tray.metrics.push(TrayMetric::Cpu);
        assert!(validate(&value).is_err());
        value.tray.metrics.clear();
        value.tray.show_icon = false;
        assert!(validate(&value).is_err());
        value.tray.show_icon = true;
        assert!(validate(&value).is_ok());
        value.tray.interval_ms = 0;
        assert!(validate(&value).is_err());
        value.tray.interval_ms = 1000;
        value.tray.rate_unit = "invalid".into();
        assert!(validate(&value).is_err());
        value.tray.rate_unit = "bits".into();
        value.tray.network_interface = "en0\ninvalid".into();
        assert!(validate(&value).is_err());
    }
    #[test]
    fn validates_ranges_and_paths() {
        let mut p = Preferences::default();
        p.scan_roots.push("relative".into());
        assert!(validate(&p).is_err());
        p.scan_roots.clear();
        p.large_min_mb = 0;
        assert!(validate(&p).is_err());
    }
}
