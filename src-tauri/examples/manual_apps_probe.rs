fn main() {
    let apps = tauri_app_lib::packages::list_installed_apps();
    let manual: Vec<_> = apps.iter().filter(|app| app.pkg_type == "manual").collect();
    println!("manual_apps={}", manual.len());
    for app in &manual {
        println!(
            "name={} can_uninstall={} icon={} executable={}",
            app.name,
            app.can_uninstall,
            app.icon.starts_with("data:image/"),
            app.install_path
        );
    }
    for app in manual.iter().filter(|app| app.can_uninstall) {
        let plan = tauri_app_lib::manual_apps::plan(&app.key).expect("read-only uninstall preview");
        println!(
            "preview_app={} items={} managed_ides={}",
            app.name,
            plan.items.len(),
            plan.items.iter().filter(|item| item.kind == "ide").count()
        );
        for item in plan
            .items
            .iter()
            .filter(|item| item.kind == "program" || item.kind == "ide")
        {
            println!("preview {} {} {}", item.name, item.size, item.path);
        }
    }
}
