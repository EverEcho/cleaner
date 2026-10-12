// Shared by the GUI and the root-owned helper; no arbitrary commands or paths.
#[derive(Debug)]
pub struct AdminAction {
    pub program: &'static str,
    pub args: Vec<String>,
}

pub fn resolve(verb: &str, manager: &str, key: &str) -> Result<AdminAction, String> {
    let package = || {
        if key.is_empty()
            || key.starts_with('-')
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".+_:@-".contains(c))
        {
            Err("无效的软件包标识".to_string())
        } else {
            Ok(())
        }
    };
    let (program, args): (&str, Vec<&str>) = match (verb, manager) {
        ("remove", "deb") => {
            package()?;
            ("/usr/bin/apt-get", vec!["remove", "-y", "--", key])
        }
        ("remove", "snap") => {
            package()?;
            ("/usr/bin/snap", vec!["remove", "--", key])
        }
        ("remove", "rpm" | "dnf") => {
            package()?;
            ("/usr/bin/dnf", vec!["remove", "-y", "--", key])
        }
        ("remove", "yum") => {
            package()?;
            ("/usr/bin/yum", vec!["remove", "-y", "--", key])
        }
        ("remove", "zypper") => {
            package()?;
            (
                "/usr/bin/zypper",
                vec!["--non-interactive", "remove", "--", key],
            )
        }
        ("remove", "pacman") => {
            package()?;
            ("/usr/bin/pacman", vec!["-R", "--noconfirm", "--", key])
        }
        ("upgrade", "apt") => {
            package()?;
            (
                "/usr/bin/apt-get",
                vec!["install", "--only-upgrade", "-y", "--", key],
            )
        }
        ("upgrade", "dnf") => {
            package()?;
            ("/usr/bin/dnf", vec!["upgrade", "-y", "--", key])
        }
        ("upgrade", "yum") => {
            package()?;
            ("/usr/bin/yum", vec!["upgrade", "-y", "--", key])
        }
        ("upgrade", "zypper") => {
            package()?;
            (
                "/usr/bin/zypper",
                vec!["--non-interactive", "update", "--", key],
            )
        }
        // Arch requires a full system upgrade; never perform partial pacman -S updates.
        ("upgrade", "pacman") => {
            return Err("Arch 更新需要完整系统升级，请在终端执行 pacman -Syu".into());
        }
        ("refresh", "apt") if key.is_empty() => ("/usr/bin/apt-get", vec!["update"]),
        ("refresh", "dnf") if key.is_empty() => ("/usr/bin/dnf", vec!["makecache"]),
        ("refresh", "yum") if key.is_empty() => ("/usr/bin/yum", vec!["makecache"]),
        ("refresh", "zypper") if key.is_empty() => {
            ("/usr/bin/zypper", vec!["--non-interactive", "refresh"])
        }
        ("clean", "apt") if key.is_empty() => ("/usr/bin/apt-get", vec!["clean"]),
        ("clean", "journal") if key.is_empty() => {
            ("/usr/bin/journalctl", vec!["--vacuum-size=500M"])
        }
        ("clean", "tmp") if key.is_empty() => (
            "/usr/bin/systemd-tmpfiles",
            vec!["--clean", "--prefix=/tmp", "--prefix=/var/tmp"],
        ),
        ("clean", "docker-containers") if key.is_empty() => (
            "/usr/bin/docker",
            vec![
                "--host",
                "unix:///var/run/docker.sock",
                "container",
                "prune",
                "--force",
            ],
        ),
        ("clean", "docker-images") if key.is_empty() => (
            "/usr/bin/docker",
            vec![
                "--host",
                "unix:///var/run/docker.sock",
                "image",
                "prune",
                "--all",
                "--force",
            ],
        ),
        ("clean", "docker-builder") if key.is_empty() => (
            "/usr/bin/docker",
            vec![
                "--host",
                "unix:///var/run/docker.sock",
                "builder",
                "prune",
                "--all",
                "--force",
            ],
        ),
        _ => return Err("不支持的管理操作".into()),
    };
    // All branches above use literal absolute executable paths.
    let program = match program {
        "/usr/bin/apt-get" => "/usr/bin/apt-get",
        "/usr/bin/snap" => "/usr/bin/snap",
        "/usr/bin/dnf" => "/usr/bin/dnf",
        "/usr/bin/pacman" => "/usr/bin/pacman",
        "/usr/bin/yum" => "/usr/bin/yum",
        "/usr/bin/zypper" => "/usr/bin/zypper",
        "/usr/bin/docker" => "/usr/bin/docker",
        "/usr/bin/journalctl" => "/usr/bin/journalctl",
        _ => "/usr/bin/systemd-tmpfiles",
    };
    Ok(AdminAction {
        program,
        args: args.into_iter().map(str::to_string).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_options_unknown_commands_and_partial_arch_upgrades() {
        assert!(resolve("remove", "deb", "--allow-remove-essential").is_err());
        assert!(resolve("clean", "docker", "").is_err());
        assert!(resolve("clean", "tmp", "/home").is_err());
        assert!(resolve("upgrade", "pacman", "firefox").is_err());
        assert_eq!(
            resolve("clean", "journal", "").unwrap().args,
            ["--vacuum-size=500M"]
        );
    }
}
