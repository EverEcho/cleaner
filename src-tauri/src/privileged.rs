use crate::{admin_actions, platform};
use std::{path::Path, process::Command};

const HELPER: &str = "/usr/local/libexec/appsteward-helper";

pub fn session_cache_installed() -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let secure = |p: &Path| {
            p.metadata()
                .is_ok_and(|m| m.uid() == 0 && m.permissions().mode() & 0o022 == 0)
        };
        secure(Path::new(HELPER))
            && secure(Path::new("/usr/local/libexec"))
            && secure(Path::new("/usr/local"))
            && secure(Path::new(
                "/usr/share/polkit-1/actions/cn.workbuddy.appsteward.policy",
            ))
    }
    #[cfg(not(unix))]
    {
        false
    }
}

pub fn run(verb: &str, manager: &str, key: &str) -> Result<String, String> {
    let action = admin_actions::resolve(verb, manager, key)?;
    let pkexec = platform::command_path("pkexec").ok_or("未找到系统授权程序 pkexec")?;
    let mut command = Command::new(pkexec);
    if session_cache_installed() {
        command.args([HELPER, verb, manager, key]);
    } else {
        command.arg(action.program).args(action.args);
    }
    let result = platform::checked_output(&mut command)?;
    Ok(String::from_utf8_lossy(&result.stdout).into_owned())
}

pub fn remove_checked(receipt: &crate::package_plan::RemovalReceipt) -> Result<String, String> {
    if !session_cache_installed() {
        return Err("请先配置管理员授权助手，影响预览卸载必须由助手复核".into());
    }
    let payload = serde_json::to_string(receipt).map_err(|e| e.to_string())?;
    let pkexec = platform::command_path("pkexec").ok_or("系统授权程序不可用")?;
    let output = platform::checked_output(Command::new(pkexec).args([
        HELPER,
        "remove-checked",
        "package",
        &payload,
    ]))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn upgrade_checked(
    receipt: &crate::native_update_plan::UpgradeReceipt,
) -> Result<String, String> {
    if !session_cache_installed() {
        return Err("请先配置管理员授权组件，再执行版本锁定升级".into());
    }
    let payload = serde_json::to_string(receipt).map_err(|e| e.to_string())?;
    let pkexec = platform::command_path("pkexec").ok_or("系统授权程序不可用")?;
    let output = platform::checked_output(Command::new(pkexec).args([
        HELPER,
        "upgrade-checked",
        "package",
        &payload,
    ]))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn clean_system_report(
    plans: &std::collections::HashMap<String, String>,
) -> Result<crate::models::ActionReport, String> {
    if cfg!(target_os = "macos") {
        let (socket, docker) = local_docker()?;
        let current = crate::system_scan::scan_docker(&socket, &docker);
        for (id, hash) in plans {
            let plan = current
                .iter()
                .find(|p| &p.id == id && p.error.is_none())
                .ok_or("Docker 清理项不可用")?;
            if hash != &plan.fingerprint() {
                return Err("Docker 对象或占用已改变，请重新扫描".into());
            }
        }
        let mut report = crate::models::ActionReport::default();
        let host = format!("unix://{socket}");
        for (id, kind) in [
            ("docker-images", "image"),
            ("docker-builder", "builder"),
            ("docker-containers", "container"),
        ] {
            if !plans.contains_key(id) {
                continue;
            }
            let mut command = Command::new(&docker);
            command.args(["--host", &host, kind, "prune", "--force"]);
            if kind != "container" {
                command.arg("--all");
            }
            match platform::checked_output(&mut command) {
                Ok(output) => report.push(
                    format!("system:{id}"),
                    "success",
                    String::from_utf8_lossy(&output.stdout),
                ),
                Err(error) => report.push(format!("system:{id}"), "failed", error),
            }
        }
        return Ok(report);
    }
    if !session_cache_installed() {
        return Err("管理员授权配置不可用，请重新配置".into());
    }
    let json = serde_json::to_string(plans).map_err(|e| e.to_string())?;
    let output = platform::checked_output(Command::new("pkexec").args([
        HELPER,
        "clean-batch-report",
        "system",
        &json,
    ]))?;
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "助手不支持逐项反馈，请更新管理员授权组件".into())
}

pub fn scan_system() -> Result<Vec<crate::system_scan::SystemPlan>, String> {
    if !session_cache_installed() {
        return Err("请点击上方“管理员授权”，完成后将重新扫描系统文件".into());
    }
    let output =
        platform::checked_output(Command::new("pkexec").args([HELPER, "scan", "system", ""]))?;
    serde_json::from_slice(&output.stdout)
        .map_err(|_| "系统扫描助手版本不匹配，请更新授权配置".into())
}

pub fn clean_system(plans: &std::collections::HashMap<String, String>) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        let (socket, docker) = local_docker()?;
        let current = crate::system_scan::scan_docker(&socket, &docker);
        for (id, hash) in plans {
            let plan = current
                .iter()
                .find(|p| &p.id == id && p.error.is_none())
                .ok_or("Docker 清理项不可用")?;
            if hash != &plan.fingerprint() {
                return Err("Docker 对象或占用已改变，请重新扫描".into());
            }
        }
        let host = format!("unix://{socket}");
        for (id, kind) in [
            ("docker-images", "image"),
            ("docker-builder", "builder"),
            ("docker-containers", "container"),
        ] {
            if plans.contains_key(id) {
                let mut command = Command::new(&docker);
                command.args(["--host", &host, kind, "prune", "--force"]);
                if kind != "container" {
                    command.arg("--all");
                }
                platform::checked_output(&mut command)?;
            }
        }
        return Ok(());
    }
    if !session_cache_installed() {
        return Err("管理员授权配置不可用，请重新配置".into());
    }
    let json = serde_json::to_string(plans).map_err(|e| e.to_string())?;
    platform::checked_output(Command::new("pkexec").args([
        HELPER,
        "clean-batch",
        "system",
        &json,
    ]))?;
    Ok(())
}

pub fn authorize(app: &tauri::AppHandle) -> Result<String, String> {
    if cfg!(target_os = "macos") {
        platform::checked_output(
            Command::new("/usr/bin/open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"),
        )?;
        return Ok("已打开系统隐私设置，请允许应用访问需要扫描的目录，随后重新扫描".into());
    }
    if !cfg!(target_os = "linux") {
        return Err("当前系统尚未实现授权支持".into());
    }
    let pkexec = platform::command_path("pkexec").ok_or("系统缺少管理员认证组件 Polkit")?;
    // Locate the shipped helper, never accept a program path from the frontend.
    use tauri::Manager;
    let bundled = app
        .path()
        .resource_dir()
        .ok()
        .map(|p| p.join("appsteward-helper"));
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("appsteward-helper")));
    let source = bundled.into_iter().chain(sibling).find(|p| p.is_file());
    if !session_cache_installed() {
        let source = source.ok_or("应用缺少授权组件，请重新构建或安装应用")?;
        platform::checked_output(Command::new(&pkexec).arg(source).args([
            "install",
            "authorization",
            "",
        ]))?;
    } else if let Some(source) = source {
        // Update the installed helper if this application contains a newer implementation.
        let shipped = std::fs::read(&source).map_err(|e| e.to_string())?;
        let installed = std::fs::read(HELPER).map_err(|e| e.to_string())?;
        if shipped != installed {
            platform::checked_output(Command::new(&pkexec).arg(source).args([
                "install",
                "authorization",
                "",
            ]))?;
        }
    }
    platform::checked_output(Command::new(pkexec).args([HELPER, "authorize", "session", ""]))?;
    Ok("管理员授权已获得，可继续扫描与清理；过期后系统会重新验证".into())
}

pub fn local_docker() -> Result<(String, String), String> {
    let docker = platform::command_path("docker").ok_or("未安装 Docker")?;
    let user_socket = platform::home().map(|h| h.join(".docker/run/docker.sock"));
    let socket = user_socket
        .filter(|p| p.exists())
        .unwrap_or_else(|| "/var/run/docker.sock".into());
    if !socket.exists() {
        return Err("本地 Docker 服务未启动".into());
    }
    Ok((
        socket.to_string_lossy().into_owned(),
        docker.to_string_lossy().into_owned(),
    ))
}
