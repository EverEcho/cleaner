fn main() {
    let start = std::time::Instant::now();
    let report = tauri_app_lib::software_updates::check();
    println!(
        "elapsed={:.2}s packages={} tools={} warnings={}",
        start.elapsed().as_secs_f64(),
        report.packages.len(),
        report.tools.len(),
        report.warnings.len()
    );
    for tool in report.tools {
        println!(
            "{} | {} | {} -> {} | {} | {}",
            tool.name,
            tool.provider,
            tool.current_version,
            tool.latest_version,
            tool.status,
            tool.note
        );
    }
    for warning in report.warnings {
        println!("warning: {warning}");
    }
}
