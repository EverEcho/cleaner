use crate::{
    monitor::MonitorSnapshot,
    preferences::{Preferences, TrayMetric, TrayPreferences},
};

pub fn format_rate(value: Option<u64>, bits: bool, compact: bool) -> String {
    let Some(value) = value else {
        return "—".into();
    };
    let mut value = value as f64 * if bits { 8.0 } else { 1.0 };
    let base = if bits { 1000.0 } else { 1024.0 };
    let mut power = 0;
    while value >= base && power < 3 {
        value /= base;
        power += 1;
    }
    let unit = if bits {
        ["b", "Kb", "Mb", "Gb"][power]
    } else {
        ["B", "K", "M", "G"][power]
    };
    let number = if value < 10.0 && power > 0 {
        format!("{value:.1}")
    } else {
        format!("{value:.0}")
    };
    format!("{number}{unit}{}", if compact { "" } else { "/s" })
}
pub fn title(
    settings: &TrayPreferences,
    snapshot: Option<&MonitorSnapshot>,
    enabled: bool,
) -> String {
    if !enabled {
        return "已暂停".into();
    }
    settings
        .metrics
        .iter()
        .map(|metric| {
            let prefix = match metric {
                TrayMetric::Cpu => {
                    if settings.labeled {
                        "CPU "
                    } else {
                        "C "
                    }
                }
                TrayMetric::Memory => {
                    if settings.labeled {
                        "内存 "
                    } else {
                        "M "
                    }
                }
                TrayMetric::Download => "↓",
                TrayMetric::Upload => "↑",
                TrayMetric::Disk => {
                    if settings.labeled {
                        "可用 "
                    } else {
                        "D "
                    }
                }
            };
            let percent = match metric {
                TrayMetric::Cpu => snapshot.and_then(|s| s.cpu_percent),
                TrayMetric::Memory => snapshot
                    .and_then(|s| s.memory_total.zip(s.memory_used))
                    .and_then(|(total, used)| {
                        (total > 0).then_some(used as f64 / total as f64 * 100.0)
                    }),
                _ => None,
            };
            let value = match metric {
                TrayMetric::Cpu | TrayMetric::Memory => percent
                    .map(|value| format!("{value:.0}%"))
                    .unwrap_or_else(|| "—".into()),
                TrayMetric::Download => format_rate(
                    snapshot.and_then(|s| s.network_down),
                    settings.rate_unit == "bits",
                    !settings.labeled,
                ),
                TrayMetric::Upload => format_rate(
                    snapshot.and_then(|s| s.network_up),
                    settings.rate_unit == "bits",
                    !settings.labeled,
                ),
                TrayMetric::Disk => snapshot
                    .and_then(|s| s.disk_available)
                    .map(|value| format!("{:.0}G", value as f64 / 1024f64.powi(3)))
                    .unwrap_or_else(|| "—".into()),
            };
            format!("{prefix}{value}")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use std::{
        sync::{
            Condvar, Mutex,
            atomic::{AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };
    use tauri::{
        Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
        WindowEvent,
        menu::{Menu, MenuItem},
        tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    };
    pub struct Runtime {
        preferences: Mutex<Preferences>,
        last_hide: Mutex<Option<Instant>>,
        wake: Condvar,
        revision: AtomicU64,
    }
    fn hide_panel(app: &tauri::AppHandle) {
        if let Some(panel) = app.get_webview_window("tray") {
            let _ = panel.hide();
        }
        if let Ok(mut last) = app.state::<Runtime>().last_hide.lock() {
            *last = Some(Instant::now());
        }
    }
    pub fn open_main(app: &tauri::AppHandle, route: &str) -> Result<(), String> {
        if !["/", "/monitor", "/clean", "/storage"].contains(&route) {
            return Err("不支持的页面".into());
        }
        hide_panel(app);
        let window = app.get_webview_window("main").ok_or("主窗口不可用")?;
        window.show().map_err(|e| e.to_string())?;
        window.unminimize().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        app.emit_to("main", "tray-navigate", route)
            .map_err(|e| e.to_string())
    }
    pub fn update(app: &tauri::AppHandle, preferences: &Preferences) -> Result<(), String> {
        let state = app.state::<Runtime>();
        *state.preferences.lock().map_err(|e| e.to_string())? = preferences.clone();
        state.revision.fetch_add(1, Ordering::SeqCst);
        state.wake.notify_one();
        if let Some(tray) = app.tray_by_id("resources") {
            tray.set_visible(preferences.tray.enabled)
                .map_err(|e| e.to_string())?;
            let icon = if preferences.tray.show_icon {
                Some(
                    tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png"))
                        .map_err(|e| e.to_string())?,
                )
            } else {
                None
            };
            tray.set_icon(icon).map_err(|e| e.to_string())?;
            tray.set_icon_as_template(true).map_err(|e| e.to_string())?;
            tray.set_title(Some(title(
                &preferences.tray,
                None,
                preferences.monitor_enabled,
            )))
            .map_err(|e| e.to_string())?;
        }
        if !preferences.tray.enabled {
            open_main(app, "/monitor")?;
        }
        Ok(())
    }
    fn toggle_panel(
        app: &tauri::AppHandle,
        rect: tauri::Rect,
        position: PhysicalPosition<f64>,
    ) -> tauri::Result<()> {
        let Some(panel) = app.get_webview_window("tray") else {
            return Ok(());
        };
        if panel.is_visible()? {
            hide_panel(app);
            return Ok(());
        }
        if app
            .state::<Runtime>()
            .last_hide
            .lock()
            .ok()
            .and_then(|last| *last)
            .is_some_and(|last| last.elapsed() < Duration::from_millis(200))
        {
            return Ok(());
        }
        let monitor = app
            .monitor_from_point(position.x, position.y)?
            .or(app.primary_monitor()?);
        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let area = monitor.work_area();
            let width = (380.0 * scale).min(area.size.width as f64);
            let height = (600.0 * scale)
                .min(area.size.height as f64 - 12.0 * scale)
                .max(200.0 * scale);
            let origin = rect.position.to_physical::<f64>(scale);
            let size = rect.size.to_physical::<f64>(scale);
            let x = (origin.x + size.width / 2.0 - width / 2.0).clamp(
                area.position.x as f64,
                (area.position.x as f64 + area.size.width as f64 - width)
                    .max(area.position.x as f64),
            );
            let y = (origin.y + size.height + 6.0 * scale).clamp(
                area.position.y as f64,
                (area.position.y as f64 + area.size.height as f64 - height)
                    .max(area.position.y as f64),
            );
            panel.set_size(PhysicalSize::new(width as u32, height as u32))?;
            panel.set_position(PhysicalPosition::new(x as i32, y as i32))?;
        }
        panel.show()?;
        panel.set_focus()?;
        Ok(())
    }
    pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
        let preferences = crate::preferences::load(app.handle()).unwrap_or_default();
        app.manage(Runtime {
            preferences: Mutex::new(preferences.clone()),
            last_hide: Mutex::new(None),
            wake: Condvar::new(),
            revision: AtomicU64::new(0),
        });
        let panel =
            WebviewWindowBuilder::new(app, "tray", WebviewUrl::App("index.html#/tray".into()))
                .title("AppClear · 菜单栏")
                .inner_size(380.0, 600.0)
                .visible(false)
                .decorations(false)
                .resizable(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .shadow(true)
                .visible_on_all_workspaces(true)
                .build()?;
        let handle = app.handle().clone();
        panel.on_window_event(move |event| {
            if matches!(event, WindowEvent::Focused(false)) {
                hide_panel(&handle);
            }
        });
        let show = MenuItem::with_id(app, "show", "打开 AppClear", true, None::<&str>)?;
        let monitor =
            MenuItem::with_id(app, "monitor", "资源监控与菜单栏设置", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "退出 AppClear", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &monitor, &quit])?;
        TrayIconBuilder::with_id("resources")
            .icon(tauri::image::Image::from_bytes(include_bytes!(
                "../icons/tray-template.png"
            ))?)
            .icon_as_template(true)
            .tooltip("AppClear · 本机资源")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(|app, event| match event.id().as_ref() {
                "show" => {
                    let _ = open_main(app, "/");
                }
                "monitor" => {
                    let _ = open_main(app, "/monitor");
                }
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    rect,
                    position,
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    let _ = toggle_panel(tray.app_handle(), rect, position);
                }
            })
            .build(app)?;
        update(app.handle(), &preferences)?;
        if let Some(main) = app.get_webview_window("main") {
            let handle = app.handle().clone();
            main.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let state = handle.state::<Runtime>();
                    if state
                        .preferences
                        .lock()
                        .is_ok_and(|p| p.tray.enabled && p.tray.close_to_tray)
                    {
                        api.prevent_close();
                        if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    } else {
                        handle.exit(0);
                    }
                }
            });
        }
        let app = app.handle().clone();
        std::thread::spawn(move || {
            loop {
                let state = app.state::<Runtime>();
                let revision = state.revision.load(Ordering::SeqCst);
                let preferences = match state.preferences.lock() {
                    Ok(p) => p.clone(),
                    Err(_) => break,
                };
                if preferences.tray.enabled {
                    let snapshot = if preferences.monitor_enabled {
                        crate::monitor::sample(&preferences.tray.network_interface).ok()
                    } else {
                        None
                    };
                    if let Some(ref snapshot) = snapshot {
                        let _ = app.emit("monitor-snapshot", snapshot);
                    }
                    if let Some(tray) = app.tray_by_id("resources") {
                        let _ = tray.set_title(Some(title(
                            &preferences.tray,
                            snapshot.as_ref(),
                            preferences.monitor_enabled,
                        )));
                        let tooltip = snapshot
                            .as_ref()
                            .map(|s| {
                                format!(
                                    "AppClear · {}\n下载 {} · 上传 {}",
                                    s.network_interface,
                                    format_rate(
                                        s.network_down,
                                        preferences.tray.rate_unit == "bits",
                                        false
                                    ),
                                    format_rate(
                                        s.network_up,
                                        preferences.tray.rate_unit == "bits",
                                        false
                                    )
                                )
                            })
                            .unwrap_or_else(|| "AppClear · 资源采样已暂停".into());
                        let _ = tray.set_tooltip(Some(tooltip));
                    }
                }
                let guard = match state.preferences.lock() {
                    Ok(p) => p,
                    Err(_) => break,
                };
                if state.revision.load(Ordering::SeqCst) != revision {
                    continue;
                }
                let interval = if preferences.tray.enabled && preferences.monitor_enabled {
                    preferences.tray.interval_ms
                } else {
                    60_000
                };
                if state
                    .wake
                    .wait_timeout(guard, Duration::from_millis(interval))
                    .is_err()
                {
                    break;
                }
            }
        });
        Ok(())
    }
    pub fn hide(app: &tauri::AppHandle) {
        hide_panel(app);
    }
    pub fn show_panel(app: &tauri::AppHandle) -> Result<(), String> {
        let tray = app.tray_by_id("resources").ok_or("菜单栏尚未初始化")?;
        if !app
            .state::<Runtime>()
            .preferences
            .lock()
            .map_err(|e| e.to_string())?
            .tray
            .enabled
        {
            return Err("请先启用菜单栏".into());
        }
        if let Some(panel) = app.get_webview_window("tray")
            && panel.is_visible().unwrap_or(false)
        {
            return panel.set_focus().map_err(|e| e.to_string());
        }
        let rect = tray
            .rect()
            .map_err(|e| e.to_string())?
            .ok_or("菜单栏图标尚不可见")?;
        let scale = app
            .primary_monitor()
            .map_err(|e| e.to_string())?
            .map(|monitor| monitor.scale_factor())
            .unwrap_or(1.0);
        let position = rect.position.to_physical::<f64>(scale);
        toggle_panel(app, rect, position).map_err(|e| e.to_string())
    }
}
#[cfg(target_os = "macos")]
pub use native::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_title_respects_order_units_and_unknown_values() {
        let mut settings = TrayPreferences::default();
        settings.metrics = vec![TrayMetric::Memory, TrayMetric::Download, TrayMetric::Cpu];
        let snapshot = MonitorSnapshot {
            memory_used: Some(25),
            memory_total: Some(100),
            network_down: Some(1024),
            cpu_percent: Some(7.3),
            ..Default::default()
        };
        assert_eq!(
            title(&settings, Some(&snapshot), true),
            "M 25%  ↓1.0K  C 7%"
        );
        assert_eq!(title(&settings, None, true), "M —  ↓—  C —");
        settings.rate_unit = "bits".into();
        settings.labeled = true;
        assert!(title(&settings, Some(&snapshot), true).contains("↓8.2Kb/s"));
        assert_eq!(title(&settings, Some(&snapshot), false), "已暂停");
        assert_eq!(format_rate(Some(0), false, false), "0B/s");
    }
}
