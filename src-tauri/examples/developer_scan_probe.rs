fn main() {
    let root = std::env::args().nth(1).expect("project root");
    let start = std::time::Instant::now();
    let category = tauri_app_lib::cleaner::scan_developer_junk(Some(&root), 7, |_| {}).unwrap();
    let warning = category.warning.clone();
    println!(
        "root={root} elapsed_seconds={:.2} targets={} displayed_groups={} known_bytes={} warning={} summary={}",
        start.elapsed().as_secs_f64(),
        category.count,
        category.items.len(),
        category.total_size_bytes,
        category.warning.is_some(),
        category.scan_summary.unwrap_or_default()
    );
    if let Some(warning) = warning {
        println!("{warning}");
    }
}
