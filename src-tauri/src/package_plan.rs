//! Read-only native removal planning, shared with the root-owned helper.
use crate::query::QueryCommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, process::Command};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemovalReceipt {
    pub manager: String,
    pub key: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemovalPackage {
    pub name: String,
    pub version: String,
    pub size_bytes: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemovalSimulation {
    pub receipt: RemovalReceipt,
    pub packages: Vec<RemovalPackage>,
    pub mode: String,
    pub note: String,
}

fn validate(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.starts_with('-')
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".+_:@-".contains(c))
    {
        return Err("无效的软件包标识".into());
    }
    Ok(())
}
fn query(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("LANG", "C")
        .query_output()
        .map_err(|e| format!("卸载影响查询失败: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "卸载影响查询失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn parse_apt(text: &str) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for line in text.lines() {
        if line.starts_with("Inst ") || line.starts_with("Conf ") {
            return Err("卸载计划包含安装或配置操作，必须在原包管理器中处理".into());
        }
        if let Some(line) = line.strip_prefix("Remv ") {
            let name = line.split_whitespace().next().ok_or("卸载范围无效")?;
            validate(name)?;
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    if names.is_empty() || names.len() > 64 {
        return Err("未取得可靠且有限的卸载范围".into());
    }
    Ok(names)
}
fn safe_deb(priority: &str, section: &str, essential: &str, manual: bool) -> bool {
    essential != "yes"
        && !["required", "important"].contains(&priority)
        && !["base", "admin"].contains(&section)
        && manual
}

pub fn simulate(manager: &str, key: &str) -> Result<RemovalSimulation, String> {
    validate(key)?;
    let mut fingerprint = Sha256::new();
    fingerprint.update(format!("remove-v1:{manager}:{key}\n"));
    let (packages, mode, note) = match manager {
        "deb" => {
            let names = parse_apt(&query("/usr/bin/apt-get", &["-s", "remove", "--", key])?)?;
            if !names
                .iter()
                .any(|n| n == key || n.split(':').next() == Some(key))
            {
                return Err("计划中没有目标软件包".into());
            }
            let manual = query("/usr/bin/apt-mark", &["showmanual"])?;
            let mut packages = Vec::new();
            for name in names {
                let data = query(
                    "/usr/bin/dpkg-query",
                    &[
                        "-W",
                        "-f=${Package}\t${Version}\t${Installed-Size}\t${Priority}\t${Section}\t${Essential}\t${Status}\n",
                        "--",
                        &name,
                    ],
                )?;
                let fields: Vec<_> = data.trim_end().split('\t').collect();
                if fields.len() != 7 || fields[6] != "install ok installed" {
                    return Err(format!("{name} 安装状态无法确认"));
                }
                let manually_installed = manual.lines().any(|m| m == name || m == fields[0]);
                if !safe_deb(fields[3], fields[4], fields[5], manually_installed) {
                    return Err(format!(
                        "计划将移除受保护的系统组件或自动依赖 {name}，请在系统包管理器中处理"
                    ));
                }
                fingerprint.update(data.as_bytes());
                packages.push(RemovalPackage {
                    name,
                    version: fields[1].into(),
                    size_bytes: fields[2]
                        .parse::<u64>()
                        .ok()
                        .and_then(|b| b.checked_mul(1024)),
                });
            }
            for path in ["/var/lib/dpkg/status", "/var/lib/apt/extended_states"] {
                fingerprint
                    .update(fs::read(path).map_err(|e| format!("无法核对软件包状态 {path}: {e}"))?);
            }
            (
                packages,
                "simulation",
                "APT 仿真列出全部连带移除项；保留配置，不执行 autoremove。管理员助手在获得授权后再次核对范围及软件包状态。",
            )
        }
        "pacman" => {
            let data = query(
                "/usr/bin/pacman",
                &["-Rp", "--print-format", "%n\t%v\t%s", "--", key],
            )?;
            let mut packages = Vec::new();
            for line in data.lines().filter(|l| !l.is_empty()) {
                let fields: Vec<_> = line.split('\t').collect();
                if fields.len() != 3 || fields[0] != key {
                    return Err("Pacman 移除范围不等于所选应用".into());
                }
                packages.push(RemovalPackage {
                    name: fields[0].into(),
                    version: fields[1].into(),
                    size_bytes: fields[2].parse().ok(),
                });
            }
            if packages.len() != 1
                || [
                    "pacman",
                    "systemd",
                    "glibc",
                    "linux",
                    "filesystem",
                    "bash",
                    "sudo",
                ]
                .contains(&key)
            {
                return Err("未取得安全的 Pacman 单应用卸载计划".into());
            }
            let metadata = query("/usr/bin/pacman", &["-Qi", "--", key])?;
            if metadata.lines().any(|l| {
                l.starts_with("Groups")
                    && l.split_once(':').is_some_and(|(_, v)| {
                        v.split_whitespace()
                            .any(|g| ["base", "base-devel"].contains(&g))
                    })
            }) {
                return Err("基础系统组的软件包受保护".into());
            }
            fingerprint.update(data.as_bytes());
            fingerprint.update(metadata.as_bytes());
            (
                packages,
                "simulation",
                "Pacman --print 列出将移除的单个软件包；不递归清理依赖，不移除配置备份。",
            )
        }
        "snap" => {
            let data = query("/usr/bin/snap", &["list", "--all", key])?;
            let yaml = fs::read_to_string(format!("/snap/{key}/current/meta/snap.yaml"))
                .map_err(|e| format!("无法确认 Snap 应用类型: {e}"))?;
            let kind = yaml
                .lines()
                .find_map(|line| {
                    line.strip_prefix("type:")
                        .map(|v| v.trim().trim_matches(['\'', '"']))
                })
                .unwrap_or("app");
            if kind != "app" || ["core", "snapd", "bare"].contains(&key) || key.starts_with("core")
            {
                return Err("Snap 基础组件受保护".into());
            }
            let mut packages = Vec::new();
            for line in data.lines().skip(1) {
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() < 3 || fields[0] != key {
                    return Err("Snap 安装身份无法确认".into());
                }
                let path = format!("/var/lib/snapd/snaps/{key}_{}.snap", fields[2]);
                packages.push(RemovalPackage {
                    name: format!("{key} · revision {}", fields[2]),
                    version: fields[1].into(),
                    size_bytes: fs::metadata(path).ok().map(|m| m.len()),
                });
            }
            if packages.is_empty() {
                return Err("Snap 应用未安装".into());
            }
            fingerprint.update(data.as_bytes());
            fingerprint.update(yaml.as_bytes());
            (
                packages,
                "exact-scope",
                "精确移除该 Snap 的全部修订版本。Snap 的内部配置及数据随渠道卸载；渠道通常自动保存快照，保留期取决于系统配置。本工具不使用 --purge。此渠道没有仿真接口。",
            )
        }
        _ => return Err("此渠道无法提供可靠卸载影响预览，请使用原渠道卸载".into()),
    };
    Ok(RemovalSimulation {
        receipt: RemovalReceipt {
            manager: manager.into(),
            key: key.into(),
            fingerprint: hex::encode(fingerprint.finalize()),
        },
        packages,
        mode: mode.into(),
        note: note.into(),
    })
}

pub fn verify(receipt: &RemovalReceipt) -> Result<(), String> {
    if receipt.fingerprint.len() != 64
        || !receipt.fingerprint.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("卸载凭据无效".into());
    }
    if simulate(&receipt.manager, &receipt.key)?
        .receipt
        .fingerprint
        != receipt.fingerprint
    {
        return Err("卸载范围或软件包状态已改变，请重新查看预览".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn apt_plan_lists_dependants_and_rejects_mixed_transactions() {
        assert_eq!(
            parse_apt("Reading...\nRemv user-app [1.0]\nRemv ui-plugin:amd64 [2.0]\n").unwrap(),
            ["ui-plugin:amd64", "user-app"]
        );
        assert!(parse_apt("Remv user-app [1.0]\nInst dependency (2.0)\n").is_err());
        assert!(parse_apt("Nothing to remove\n").is_err());
        assert!(parse_apt("Remv --all [1]\n").is_err());
    }
    #[test]
    fn system_packages_and_options_are_never_removal_targets() {
        assert!(!safe_deb("required", "utils", "no", true));
        assert!(!safe_deb("optional", "admin", "no", true));
        assert!(!safe_deb("optional", "utils", "yes", true));
        assert!(!safe_deb("optional", "utils", "no", false));
        assert!(safe_deb("optional", "utils", "no", true));
        for key in ["--all", "a/b", "$(id)", "foo\nbar"] {
            assert!(validate(key).is_err());
        }
        assert!(
            verify(&RemovalReceipt {
                manager: "deb".into(),
                key: "foo".into(),
                fingerprint: "invalid".into()
            })
            .is_err()
        );
    }
}
