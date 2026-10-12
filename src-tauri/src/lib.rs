pub mod models;
pub mod packages;
pub mod entries;
pub mod autostart;
pub mod cleaner;
pub mod updates;

use models::{AppInfo, AutostartItem, DesktopEntryItem, CleanCategory, LargeFileItem, DuplicateGroup, UpgradablePackage};

// --- Tauri Commands ---

#[tauri::command]
fn get_installed_apps() -> Vec<AppInfo> {
    packages::list_installed_apps()
}

#[tauri::command]
fn uninstall_app(pkg_type: String, key: String) -> Result<String, String> {
    packages::uninstall_package(&pkg_type, &key)
}

#[tauri::command]
fn get_desktop_entries() -> Vec<DesktopEntryItem> {
    entries::scan_desktop_entries()
}

#[tauri::command]
fn toggle_desktop_visibility(path: String, visible: bool) -> Result<String, String> {
    entries::toggle_desktop_entry_visibility(&path, visible)
}

#[tauri::command]
fn get_autostart_items() -> Vec<AutostartItem> {
    autostart::scan_autostart_items()
}

#[tauri::command]
fn toggle_autostart_item(path: String, enable: bool) -> Result<String, String> {
    autostart::toggle_autostart(&path, enable)
}

#[tauri::command]
fn scan_junk() -> Vec<CleanCategory> {
    cleaner::scan_system_junk()
}

#[tauri::command]
fn scan_large_files(min_mb: u64) -> Vec<LargeFileItem> {
    cleaner::scan_large_files(min_mb)
}

#[tauri::command]
fn scan_duplicate_files() -> Vec<DuplicateGroup> {
    cleaner::scan_duplicate_files()
}

#[tauri::command]
fn clean_items(paths: Vec<String>) -> Result<usize, String> {
    cleaner::delete_clean_items(paths)
}

use tauri::{AppHandle, Emitter};

#[tauri::command]
async fn check_software_updates() -> Result<Vec<UpgradablePackage>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        updates::check_all_upgradable()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn upgrade_software(app: AppHandle, manager: String, name: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.emit("update-log", format!("正在调用 {} 升级软件包 {}...", manager.to_uppercase(), name));
        updates::upgrade_package(&manager, &name)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn refresh_software_sources(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = app.emit("update-log", "正在请求管理员权限并连接软件源更新索引...".to_string());
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
            get_installed_apps,
            uninstall_app,
            get_desktop_entries,
            toggle_desktop_visibility,
            get_autostart_items,
            toggle_autostart_item,
            scan_junk,
            scan_large_files,
            scan_duplicate_files,
            clean_items,
            check_software_updates,
            upgrade_software,
            refresh_software_sources
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
