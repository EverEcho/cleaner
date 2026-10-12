#[path = "../../src/admin_actions.rs"]
mod admin_actions;
#[path = "../../src/query.rs"]
mod query;
#[path = "../../src/system_scan.rs"]
mod system_scan;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("管理助手仅用于 Linux".into());
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("管理助手需要操作、类型、标识三个参数".into());
    }
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|e| e.to_string())?;
    if !uid.status.success() || uid.stdout != b"0\n" {
        return Err("请通过系统授权运行管理助手".into());
    }
    if args[0] == "install" && args[1] == "authorization" && args[2].is_empty() {
        return install_authorization();
    }
    if args[0] == "authorize" && args[1] == "session" && args[2].is_empty() {
        println!("管理员会话授权已获得");
        return Ok(());
    }
    if args[0] == "scan" && args[1] == "system" && args[2].is_empty() {
        println!(
            "{}",
            serde_json::to_string(&system_scan::scan_linux()).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if args[0] == "clean-batch" && args[1] == "system" {
        let requested: std::collections::HashMap<String, String> =
            serde_json::from_str(&args[2]).map_err(|_| "清理计划无效")?;
        if requested.is_empty() || requested.len() > 6 {
            return Err("清理计划范围无效".into());
        }
        let current = system_scan::scan_linux();
        for (id, fingerprint) in &requested {
            let plan = current
                .iter()
                .find(|p| &p.id == id && p.error.is_none())
                .ok_or("清理项不可用，请重新扫描")?;
            if fingerprint != &plan.fingerprint() {
                return Err(format!("{id} 的文件或空间占用已改变，请重新扫描"));
            }
            admin_actions::resolve("clean", id, "")?;
        }
        // Images first, stopped containers last, so container removal cannot expand image scope.
        for id in [
            "apt",
            "journal",
            "tmp",
            "docker-images",
            "docker-builder",
            "docker-containers",
        ] {
            if requested.contains_key(id) {
                execute(admin_actions::resolve("clean", id, "")?)?;
            }
        }
        return Ok(());
    }
    let action = admin_actions::resolve(&args[0], &args[1], &args[2])?;
    execute(action)
}

fn execute(action: admin_actions::AdminAction) -> Result<(), String> {
    let status = std::process::Command::new(action.program)
        .args(action.args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("HOME", "/root")
        .env("LANG", "C.UTF-8")
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("管理操作失败: {:?}", status.code()))
    }
}

fn install_authorization() -> Result<(), String> {
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        path::Path,
    };
    for parent in ["/usr/local", "/usr/share/polkit-1/actions"] {
        let meta = fs::symlink_metadata(parent).map_err(|e| e.to_string())?;
        if !meta.is_dir() || meta.uid() != 0 || meta.permissions().mode() & 0o022 != 0 {
            return Err("授权组件目标目录不安全".into());
        }
    }
    let dir = Path::new("/usr/local/libexec");
    if !dir.exists() {
        fs::create_dir(dir).map_err(|e| e.to_string())?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    let meta = fs::symlink_metadata(dir).map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.uid() != 0 || meta.permissions().mode() & 0o022 != 0 {
        return Err("授权助手目录不安全".into());
    }
    let binary = fs::read("/proc/self/exe").map_err(|e| e.to_string())?;
    atomic_install(dir, "appsteward-helper", &binary, 0o755)?;
    atomic_install(
        Path::new("/usr/share/polkit-1/actions"),
        "cn.workbuddy.appsteward.policy",
        include_bytes!("../../../packaging/linux/cn.workbuddy.appsteward.policy"),
        0o644,
    )?;
    println!("管理员授权组件已配置");
    Ok(())
}

fn atomic_install(
    dir: &std::path::Path,
    name: &str,
    bytes: &[u8],
    mode: u32,
) -> Result<(), String> {
    use std::{
        fs,
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
    };
    let temporary = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(mode)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(mode))
            .map_err(|e| e.to_string())?;
        fs::rename(&temporary, dir.join(name)).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
