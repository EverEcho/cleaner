use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppInfo {
    pub key: String,
    pub name: String,
    pub pkg_type: String, // "deb", "snap", "flatpak", "manual"
    pub version: String,
    pub size: String,
    pub size_bytes: u64,
    pub install_time: String,
    pub description: String,
    pub icon: String,
    pub desktop_file: String,
    pub is_system: bool,
    pub has_ui: bool,
    pub can_uninstall: bool,
    pub install_path: String,
    pub uninstall_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutostartItem {
    pub path: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub comment: String,
    pub enabled: bool,
    pub is_user: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopEntryItem {
    pub path: String,
    pub basename: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub scope: String,
    pub visible: bool,
    pub categories: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanCategory {
    pub warning: Option<String>,
    pub scan_summary: Option<String>,
    pub id: String,
    pub title: String,
    pub desc: String,
    pub icon: String,
    pub total_size_bytes: u64,
    pub total_size_str: String,
    pub count: usize,
    pub items: Vec<CleanItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanItem {
    pub id: String,
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub size_str: String,
    pub reason: String,
    pub can_delete: bool,
    pub selected: bool,
    pub size_known: bool,
    pub action: Option<String>,
    pub details: Option<String>,
    pub files: Vec<CleanFileDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanFileDetail {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub size_str: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFileItem {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub size_str: String,
    pub ext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub hash: String,
    pub size_bytes: u64,
    pub size_str: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradablePackage {
    pub name: String,
    pub manager: String, // "apt", "dnf", "yum", "pacman", "zypper", "flatpak"
    pub current_version: String,
    pub new_version: String,
    pub arch: String,
    pub repository: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SoftwareUpdateReport {
    pub packages: Vec<UpgradablePackage>,
    pub tools: Vec<ToolUpdate>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolUpdate {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub current_version: String,
    pub latest_version: String,
    pub status: String,
    pub install_path: String,
    pub source_url: String,
    pub update_hint: String,
    pub note: String,
}

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
