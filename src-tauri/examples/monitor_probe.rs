fn main() {
    let initial = tauri_app_lib::monitor::sample().expect("read-only resource sampling");
    if cfg!(target_os = "linux") {
        assert!(
            initial.cpu_percent.is_none(),
            "first CPU interval is unknown"
        );
    }
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let sampled = tauri_app_lib::monitor::sample().expect("second resource sampling");
    println!("{}", serde_json::to_string_pretty(&sampled).unwrap());
}
