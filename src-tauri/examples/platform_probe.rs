fn main() {
    use tauri_app_lib::{autostart, entries, packages, platform, storage};
    println!(
        "platform={}",
        serde_json::to_string(&platform::info()).unwrap()
    );
    let apps = packages::list_installed_apps();
    println!(
        "apps={} icons={}",
        apps.len(),
        apps.iter()
            .filter(|app| app.icon.starts_with("data:image/"))
            .count()
    );
    println!("entries={}", entries::scan_desktop_entries().len());
    println!("autostart={}", autostart::scan_autostart_items().len());
    println!("storage_roots={}", storage::roots().len());
    println!(
        "disk_report_ok={}",
        std::process::Command::new("df")
            .arg("-h")
            .output()
            .is_ok_and(|o| o.status.success())
    );
    let junk = tauri_app_lib::cleaner::scan_system_junk();
    println!("junk_categories={}", junk.len());
    let actions: Vec<_> = junk
        .iter()
        .flat_map(|c| &c.items)
        .filter_map(|i| i.action.as_deref())
        .collect();
    println!("integrated_system_actions={}", actions.join(","));
    println!(
        "legacy_diagnostics_removed={}",
        junk.iter()
            .filter(|c| c.id == "system_inspection")
            .flat_map(|c| &c.items)
            .count()
            == 0
    );
    let report = storage::analyze("/usr/share/applications").unwrap();
    println!("du_ok=true partial={}", !report.warning.is_empty());
}
