pub mod admin_actions;
pub mod app_profile;
#[cfg_attr(target_os = "macos", path = "macos/autostart.rs")]
pub mod autostart;
pub mod cleaner;
pub mod desktop;
pub mod developer;
#[cfg_attr(target_os = "macos", path = "macos/entries.rs")]
pub mod entries;
pub mod icons;
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/mod.rs"]
mod macos_tests;
pub mod manual_apps;
pub mod models;
pub mod monitor;
pub mod native_update_plan;
pub mod package_plan;
#[cfg_attr(target_os = "macos", path = "macos/packages.rs")]
pub mod packages;
pub mod platform;
pub mod preferences;
pub mod privileged;
pub mod query;
pub mod software_updates;
pub mod storage;
pub mod system_scan;
pub mod tasks;
pub mod uninstall_preview;
#[cfg_attr(target_os = "macos", path = "macos/updates.rs")]
pub mod updates;

use models::{
    ActionReport, AppInfo, AutostartItem, CleanCategory, DesktopEntryItem, DuplicateGroup,
    FileScanReport, LargeFileItem, ScanOptions,
};
use tauri::{AppHandle, Emitter};

pub(crate) fn select_upgrade_candidate<'a>(
    rows: &'a [models::UpgradablePackage],
    manager: &str,
    name: &str,
    expected_version: Option<&str>,
    arch: Option<&str>,
) -> Result<&'a models::UpgradablePackage, String> {
    let identity = rows
        .iter()
        .filter(|row| row.manager == manager && row.name == name);
    // Older callers lack an installation qualifier, so even different candidate versions cannot disambiguate them.
    if arch.is_none() && identity.clone().count() != 1 {
        return Err("安装实例不存在或存在多个同名架构，请重新检测并指定安装架构".into());
    }
    let mut matches = identity.filter(|row| {
        arch.is_none_or(|arch| row.arch == arch)
            && expected_version.is_none_or(|expected| row.new_version == expected)
    });
    let selected = matches
        .next()
        .ok_or("安装架构或候选版本已经变化，请重新检测确认")?;
    if matches.next().is_some() {
        return Err("该架构和版本存在多个安装候选，请按原渠道核对后更新".into());
    }
    Ok(selected)
}

async fn tracked<T: Send + 'static>(
    app: AppHandle,
    kind: &'static str,
    task_id: Option<String>,
    work: impl FnOnce(&tasks::TaskContext) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let task = tasks::TaskContext::begin(kind, task_id, Some(app))?;
    tauri::async_runtime::spawn_blocking(move || {
        let result = work(&task);
        task.finish(
            if result.is_ok() {
                "completed"
            } else {
                "failed"
            },
            if result.is_ok() {
                "任务完成"
            } else {
                "任务未完成，请查看详情"
            },
        );
        result
    })
    .await
    .map_err(|e| e.to_string())?
}
fn scan_progress(task: &tasks::TaskContext, path: &str, current: u64, total: u64) {
    task.progress(
        "正在扫描",
        current,
        (total > 0).then_some(total),
        Some(path),
    );
}

#[tauri::command]
fn get_platform_info() -> platform::PlatformInfo {
    platform::info()
}
#[tauri::command]
fn get_tasks() -> Vec<tasks::TaskSnapshot> {
    tasks::list()
}
#[tauri::command]
fn cancel_task(app: AppHandle, id: String) -> Result<(), String> {
    tasks::cancel(&id, Some(&app))
}
#[tauri::command]
fn get_preferences(app: AppHandle) -> Result<preferences::Preferences, String> {
    preferences::load(&app)
}
#[tauri::command]
fn save_preferences(
    app: AppHandle,
    preferences: preferences::Preferences,
) -> Result<preferences::Preferences, String> {
    preferences::save(&app, preferences)
}
#[tauri::command]
async fn pick_directory(title: Option<String>) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || preferences::pick_directory(title))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn get_monitor_snapshot() -> Result<monitor::MonitorSnapshot, String> {
    tauri::async_runtime::spawn_blocking(monitor::sample)
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn get_storage_roots() -> Vec<String> {
    storage::roots()
}
#[tauri::command]
async fn analyze_disk(
    app: AppHandle,
    root: String,
    options: Option<ScanOptions>,
    task_id: Option<String>,
) -> Result<storage::DiskAnalysis, String> {
    tracked(app, "analyze_disk", task_id, move |task| {
        storage::analyze_controlled(
            &root,
            &options.unwrap_or_default().excluded,
            &|| task.is_cancelled(),
            |path, current, total| scan_progress(task, path, current, total),
        )
    })
    .await
}
#[tauri::command]
async fn request_admin_authorization(app: AppHandle) -> Result<String, String> {
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
    app: AppHandle,
    root: Option<String>,
    roots: Option<Vec<String>>,
    excluded: Option<Vec<String>>,
    min_age_days: u64,
    task_id: Option<String>,
) -> Result<CleanCategory, String> {
    let emitter = app.clone();
    tracked(app, "scan_developer_junk", task_id, move |task| {
        let roots = roots.unwrap_or_else(|| root.into_iter().collect());
        cleaner::scan_developer_junk_controlled(
            &roots,
            min_age_days,
            &excluded.unwrap_or_default(),
            &|| task.is_cancelled(),
            |progress| {
                task.progress(
                    "识别项目与统计产物",
                    progress.directories_scanned as u64,
                    None,
                    Some(&progress.path),
                );
                let _ = emitter.emit("developer-scan-progress", progress);
            },
        )
    })
    .await
}
#[tauri::command]
async fn get_installed_apps() -> Result<Vec<AppInfo>, String> {
    tauri::async_runtime::spawn_blocking(packages::list_installed_apps)
        .await
        .map_err(|e| e.to_string())
}
#[tauri::command]
async fn get_installed_apps_report(
    app: AppHandle,
    task_id: Option<String>,
) -> Result<app_profile::AppScanReport, String> {
    tracked(app, "get_installed_apps_report", task_id, move |task| {
        Ok(packages::scan_installed_apps_report_controlled(
            &|| task.is_cancelled(),
            |path, current, total| scan_progress(task, path, current, total),
        ))
    })
    .await
}
#[tauri::command]
async fn get_app_profile(pkg_type: String, key: String) -> Result<app_profile::AppProfile, String> {
    tauri::async_runtime::spawn_blocking(move || app_profile::profile(&pkg_type, &key))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn get_package_uninstall_plan(
    pkg_type: String,
    key: String,
) -> Result<uninstall_preview::PackageUninstallPlan, String> {
    tauri::async_runtime::spawn_blocking(move || uninstall_preview::preview(&pkg_type, &key))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn uninstall_app_plan(
    app: AppHandle,
    plan_id: String,
    task_id: Option<String>,
) -> Result<String, String> {
    let authorizer = app.clone();
    tracked(app, "uninstall_app_plan", task_id, move |task| {
        if uninstall_preview::requires_authorization(&plan_id)? {
            task.progress("更新授权组件并等待系统授权", 0, None, None);
            privileged::authorize(&authorizer)?;
        }
        task.progress("复核卸载影响并等待原渠道处理", 0, None, None);
        uninstall_preview::execute(&plan_id)
    })
    .await
}
#[tauri::command]
async fn get_package_uninstall_status(
    plan_id: String,
) -> Result<uninstall_preview::PackageUninstallStatus, String> {
    tauri::async_runtime::spawn_blocking(move || uninstall_preview::status(&plan_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
fn uninstall_app() -> Result<String, String> {
    Err("请先查看并确认卸载影响预览，再使用预览计划执行".into())
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
fn get_manual_uninstall_status(plan_id: String) -> Result<manual_apps::UninstallStatus, String> {
    manual_apps::uninstall_status(&plan_id)
}
#[tauri::command]
async fn uninstall_manual_app(
    app: AppHandle,
    plan_id: String,
    paths: Vec<String>,
    request_id: String,
    task_id: Option<String>,
) -> Result<String, String> {
    if !cfg!(target_os = "linux") {
        return Err("当前平台不使用桌面入口卸载".into());
    }
    let emitter = app.clone();
    tracked(app,"uninstall_manual_app",task_id,move|task|manual_apps::uninstall(&plan_id,paths,|done,total,path|{task.progress("移入废纸篓",done as u64,Some(total as u64),Some(path));let _=emitter.emit("uninstall-progress",serde_json::json!({"request_id":request_id,"message":path,"done":done,"total":total,"percentage":done.saturating_mul(100).checked_div(total)}));})).await
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
async fn scan_junk(
    app: AppHandle,
    excluded: Option<Vec<String>>,
    task_id: Option<String>,
) -> Result<Vec<CleanCategory>, String> {
    tracked(app, "scan_junk", task_id, move |task| {
        Ok(cleaner::scan_system_junk_controlled(
            &excluded.unwrap_or_default(),
            &|| task.is_cancelled(),
            |path, current, total| scan_progress(task, path, current, total),
        ))
    })
    .await
}
#[tauri::command]
async fn scan_large_files(
    app: AppHandle,
    min_mb: Option<u64>,
    options: Option<ScanOptions>,
    task_id: Option<String>,
) -> Result<FileScanReport<LargeFileItem>, String> {
    tracked(app, "scan_large_files", task_id, move |task| {
        let mut opts = options.unwrap_or_default();
        if opts.min_mb.is_none() {
            opts.min_mb = min_mb;
        }
        cleaner::scan_large_files_with_options(
            &opts,
            &|| task.is_cancelled(),
            |path, current, total| scan_progress(task, path, current, total),
        )
    })
    .await
}
#[tauri::command]
async fn scan_duplicate_files(
    app: AppHandle,
    options: Option<ScanOptions>,
    task_id: Option<String>,
) -> Result<FileScanReport<DuplicateGroup>, String> {
    tracked(app, "scan_duplicate_files", task_id, move |task| {
        cleaner::scan_duplicate_files_with_options(
            &options.unwrap_or_default(),
            &|| task.is_cancelled(),
            |path, current, total| scan_progress(task, path, current, total),
        )
    })
    .await
}
#[tauri::command]
async fn clean_items(
    app: AppHandle,
    paths: Vec<String>,
    task_id: Option<String>,
) -> Result<ActionReport, String> {
    let authorizer = app.clone();
    tracked(app, "clean_items", task_id, move |task| {
        if paths.iter().any(|path| path.starts_with("system:")) && cfg!(target_os = "linux") {
            task.progress("更新授权组件并等待系统授权", 0, None, None);
            privileged::authorize(&authorizer)?;
        }
        task.progress("复核范围并处理已选项目", 0, None, None);
        let excluded = preferences::load(&authorizer)?.exclusions;
        let result =
            cleaner::delete_clean_items_controlled(paths, &excluded, |path, done, total| {
                task.progress("处理所选项目", done, Some(total), Some(path))
            })?;
        task.finish(
            if result.failed > 0 {
                "failed"
            } else {
                "completed"
            },
            &format!(
                "成功 {} 项，失败 {} 项，跳过 {} 项",
                result.succeeded, result.failed, result.skipped
            ),
        );
        Ok(result)
    })
    .await
}
#[tauri::command]
async fn trash_files(
    app: AppHandle,
    paths: Vec<String>,
    kind: String,
    task_id: Option<String>,
) -> Result<ActionReport, String> {
    let settings = app.clone();
    tracked(app, "trash_files", task_id, move |task| {
        task.progress("复核文件与有效保留副本", 0, None, None);
        let excluded = preferences::load(&settings)?.exclusions;
        let result = cleaner::trash_scanned_files_controlled(
            paths,
            &kind,
            &excluded,
            |path, done, total| task.progress("移入废纸篓", done, Some(total), Some(path)),
        )?;
        task.finish(
            if result.failed > 0 {
                "failed"
            } else {
                "completed"
            },
            &format!(
                "成功 {} 项，失败 {} 项，跳过 {} 项",
                result.succeeded, result.failed, result.skipped
            ),
        );
        Ok(result)
    })
    .await
}
#[tauri::command]
async fn get_clean_item_details(path: String) -> Result<Vec<models::CleanFileDetail>, String> {
    tauri::async_runtime::spawn_blocking(move || cleaner::clean_item_details(&path))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn check_software_updates(
    app: AppHandle,
    task_id: Option<String>,
) -> Result<software_updates::UpdateCheckReport, String> {
    tracked(app, "check_software_updates", task_id, move |task| {
        Ok(software_updates::check_with_progress(
            || task.is_cancelled(),
            |phase, current| task.progress(phase, current, None, None),
        ))
    })
    .await
}
#[tauri::command]
async fn upgrade_software(
    app: AppHandle,
    manager: String,
    name: String,
    expected_version: Option<String>,
    arch: Option<String>,
    task_id: Option<String>,
) -> Result<String, String> {
    let emitter = app.clone();
    tracked(app, "upgrade_software", task_id, move |task| {
        if expected_version.as_deref().is_none_or(str::is_empty) {
            return Err("请先检查并确认候选版本，再执行升级".into());
        }
        if matches!(manager.as_str(), "apt" | "dnf") && cfg!(target_os = "linux") {
            task.progress("更新授权组件并等待系统授权", 0, None, None);
            privileged::authorize(&emitter)?;
        }
        updates::upgrade_package_scoped_with_progress(
            &manager,
            &name,
            expected_version.as_deref(),
            arch.as_deref(),
            |message| {
                task.progress(message, 0, None, None);
                let _ = emitter.emit("update-log", message);
            },
        )
    })
    .await
}
#[tauri::command]
async fn upgrade_tool(
    app: AppHandle,
    id: String,
    task_id: Option<String>,
) -> Result<String, String> {
    let emitter = app.clone();
    tracked(app, "upgrade_tool", task_id, move |task| {
        software_updates::upgrade_tool_with_progress(&id, |message| {
            task.progress(message, 0, None, None);
            let _ = emitter.emit("update-log", message);
        })
    })
    .await
}
#[tauri::command]
async fn refresh_software_sources(
    app: AppHandle,
    task_id: Option<String>,
) -> Result<String, String> {
    tracked(app, "refresh_software_sources", task_id, move |task| {
        task.progress("等待授权并刷新软件源", 0, None, None);
        updates::refresh_package_sources()
    })
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_platform_info,
            get_tasks,
            cancel_task,
            get_preferences,
            save_preferences,
            pick_directory,
            get_monitor_snapshot,
            get_storage_roots,
            analyze_disk,
            request_admin_authorization,
            get_developer_roots,
            scan_developer_junk,
            get_installed_apps,
            get_installed_apps_report,
            get_app_profile,
            get_package_uninstall_plan,
            get_package_uninstall_status,
            uninstall_app_plan,
            uninstall_app,
            get_manual_uninstall_plan,
            get_manual_uninstall_status,
            uninstall_manual_app,
            get_desktop_entries,
            toggle_desktop_visibility,
            get_autostart_items,
            toggle_autostart_item,
            scan_junk,
            scan_large_files,
            scan_duplicate_files,
            clean_items,
            get_clean_item_details,
            trash_files,
            check_software_updates,
            upgrade_software,
            upgrade_tool,
            refresh_software_sources
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod upgrade_selection_tests {
    use super::*;
    fn candidate(arch: &str, version: &str) -> models::UpgradablePackage {
        models::UpgradablePackage {
            manager: "dnf".into(),
            name: "fixture-tool".into(),
            arch: arch.into(),
            current_version: "1.0-1".into(),
            new_version: version.into(),
            repository: "updates".into(),
        }
    }
    #[test]
    fn upgrade_selection_requires_the_confirmed_architecture_and_version() {
        let rows = vec![candidate("x86_64", "2.0-1"), candidate("i686", "3.0-1")];
        let selected =
            select_upgrade_candidate(&rows, "dnf", "fixture-tool", Some("3.0-1"), Some("i686"))
                .unwrap();
        assert_eq!(selected.arch, "i686");
        assert_eq!(selected.new_version, "3.0-1");
        assert!(
            select_upgrade_candidate(&rows, "dnf", "fixture-tool", Some("3.0-1"), None).is_err()
        );
        assert!(select_upgrade_candidate(&rows, "dnf", "fixture-tool", None, None).is_err());
        assert!(
            select_upgrade_candidate(&rows, "dnf", "fixture-tool", Some("3.0-1"), Some("x86_64"))
                .is_err()
        );
        assert!(
            select_upgrade_candidate(&rows, "dnf", "fixture-tool", Some("3.0-1"), Some("aarch64"))
                .is_err()
        );
        assert!(
            select_upgrade_candidate(&rows, "apt", "fixture-tool", Some("3.0-1"), Some("i686"))
                .is_err()
        );
        let duplicated = vec![candidate("i686", "3.0-1"), candidate("i686", "3.0-1")];
        assert!(
            select_upgrade_candidate(
                &duplicated,
                "dnf",
                "fixture-tool",
                Some("3.0-1"),
                Some("i686")
            )
            .is_err()
        );
    }
    #[test]
    fn legacy_upgrade_selection_allows_only_a_unique_candidate() {
        let rows = vec![candidate("x86_64", "2.0-1")];
        assert_eq!(
            select_upgrade_candidate(&rows, "dnf", "fixture-tool", None, None)
                .unwrap()
                .arch,
            "x86_64"
        );
        assert!(
            select_upgrade_candidate(&rows, "dnf", "fixture-tool", Some("9.0-1"), None).is_err()
        );
    }
}
