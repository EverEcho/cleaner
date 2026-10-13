pub mod admin_actions;
#[cfg_attr(target_os = "macos", path = "macos/autostart.rs")]
pub mod autostart;
pub mod cleaner;
pub mod desktop;
pub mod developer;
#[cfg_attr(target_os = "macos", path = "macos/entries.rs")]
pub mod entries;
pub mod icons;
pub mod manual_apps;
pub mod models;
#[cfg_attr(target_os = "macos", path = "macos/packages.rs")]
pub mod packages;
pub mod platform;
pub mod privileged;
pub mod query;
pub mod software_updates;
pub mod storage;
pub mod system_scan;
#[cfg_attr(target_os = "macos", path = "macos/updates.rs")]
pub mod updates;

#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/mod.rs"]
mod macos_tests;

#[tauri::command]
fn get_platform_info() -> platform::PlatformInfo {
    platform::info()
}

use models::{
    AppInfo, AutostartItem, CleanCategory, DesktopEntryItem, DuplicateGroup, LargeFileItem,
};

#[tauri::command]
fn get_storage_roots() -> Vec<String> {
    storage::roots()
}

#[tauri::command]
async fn analyze_disk(root: String) -> Result<storage::DiskAnalysis, String> {
    tauri::async_runtime::spawn_blocking(move || storage::analyze(&root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn request_admin_authorization(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || privileged::authorize(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_developer_roots() -> Vec<String> {
    developer::default_roots()
}

#[tauri::command]
async fn scan_developer_junk(
    app: tauri::AppHandle,
    root: Option<String>,
    min_age_days: u64,
) -> Result<CleanCategory, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cleaner::scan_developer_junk(root.as_deref(), min_age_days, |progress| {
            let _ = app.emit("developer-scan-progress", progress);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// --- Tauri Commands ---

#[tauri::command]
async fn get_installed_apps() -> Result<Vec<AppInfo>, String> {
    tauri::async_runtime::spawn_blocking(packages::list_installed_apps)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn uninstall_app(
    app: tauri::AppHandle,
    pkg_type: String,
    key: String,
    request_id: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.emit("uninstall-progress",serde_json::json!({"request_id":request_id,"message":"正在核对应用并调用卸载工具，可能需要系统授权…","percentage":null}));
        let result = packages::uninstall_package(&pkg_type,&key);
        if result.is_ok() { let _ = app.emit("uninstall-progress",serde_json::json!({"request_id":request_id,"message":"卸载完成","percentage":100})); }
        result
    }).await.map_err(|e|e.to_string())?
}

#[tauri::command]
async fn get_manual_uninstall_plan(key: String) -> Result<manual_apps::UninstallPlan, String> {
    if !cfg!(target_os = "linux") {
        return Err("当前平台不使用桌面入口卸载".into());
    }
    tauri::async_runtime::spawn_blocking(move || manual_apps::plan(&key))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn uninstall_manual_app(
    app: tauri::AppHandle,
    plan_id: String,
    paths: Vec<String>,
    request_id: String,
) -> Result<String, String> {
    if !cfg!(target_os = "linux") {
        return Err("当前平台不使用桌面入口卸载".into());
    }
    tauri::async_runtime::spawn_blocking(move || manual_apps::uninstall(&plan_id,paths,|done,total,path| {
        let _ = app.emit("uninstall-progress",serde_json::json!({"request_id":request_id,"message":path,"done":done,"total":total,"percentage": done.saturating_mul(100).checked_div(total)}));
    })).await.map_err(|e|e.to_string())?
}

#[tauri::command]
async fn get_desktop_entries() -> Result<Vec<DesktopEntryItem>, String> {
    tauri::async_runtime::spawn_blocking(entries::scan_desktop_entries)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_desktop_visibility(path: String, visible: bool) -> Result<String, String> {
    entries::toggle_desktop_entry_visibility(&path, visible)
}

#[tauri::command]
async fn get_autostart_items() -> Result<Vec<AutostartItem>, String> {
    tauri::async_runtime::spawn_blocking(autostart::scan_autostart_items)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_autostart_item(path: String, enable: bool) -> Result<String, String> {
    autostart::toggle_autostart(&path, enable)
}

#[tauri::command]
async fn scan_junk(app: tauri::AppHandle) -> Result<Vec<CleanCategory>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cleaner::scan_system_junk_with_progress(|path, total| {
            let _ = app.emit(
                "scan-progress",
                serde_json::json!({ "path": path, "total_size_bytes": total }),
            );
        })
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_large_files(min_mb: u64) -> Result<Vec<LargeFileItem>, String> {
    tauri::async_runtime::spawn_blocking(move || cleaner::scan_large_files(min_mb))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn scan_duplicate_files() -> Result<Vec<DuplicateGroup>, String> {
    tauri::async_runtime::spawn_blocking(cleaner::scan_duplicate_files)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn clean_items(paths: Vec<String>) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || cleaner::delete_clean_items(paths))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn trash_files(paths: Vec<String>, kind: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || cleaner::trash_scanned_files(paths, &kind))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_clean_item_details(path: String) -> Result<Vec<models::CleanFileDetail>, String> {
    tauri::async_runtime::spawn_blocking(move || cleaner::clean_item_details(&path))
        .await
        .map_err(|e| e.to_string())?
}

use tauri::{AppHandle, Emitter};

#[tauri::command]
async fn check_software_updates() -> Result<models::SoftwareUpdateReport, String> {
    tauri::async_runtime::spawn_blocking(software_updates::check)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn upgrade_software(app: AppHandle, manager: String, name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.emit(
            "update-log",
            format!("正在调用 {} 升级软件包 {}...", manager.to_uppercase(), name),
        );
        updates::upgrade_package(&manager, &name)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn refresh_software_sources(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.emit(
            "update-log",
            "正在请求管理员权限并连接软件源更新索引...".to_string(),
        );
        updates::refresh_package_sources()
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_platform_info,
            request_admin_authorization,
            get_storage_roots,
            analyze_disk,
            get_installed_apps,
            uninstall_app,
            get_manual_uninstall_plan,
            uninstall_manual_app,
            get_desktop_entries,
            toggle_desktop_visibility,
            get_autostart_items,
            toggle_autostart_item,
            scan_junk,
            get_developer_roots,
            scan_developer_junk,
            scan_large_files,
            scan_duplicate_files,
            clean_items,
            get_clean_item_details,
            trash_files,
            check_software_updates,
            upgrade_software,
            refresh_software_sources
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
