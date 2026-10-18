//! Shared GUI/helper upgrade receipts. Only proven exact-version native operations are enabled.
use crate::query::QueryCommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeReceipt {
    pub manager: String,
    pub name: String,
    pub target_version: String,
    pub arch: String,
    pub current_version: String,
    pub fingerprint: String,
}
#[derive(Debug)]
pub struct UpgradeCommand {
    pub program: &'static str,
    pub args: Vec<String>,
}
fn token(value: &str, allowed: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || allowed.contains(c))
}
fn validate(receipt: &UpgradeReceipt) -> Result<(), String> {
    if !matches!(receipt.manager.as_str(), "apt" | "dnf")
        || !token(&receipt.name, ".+_-")
        || !token(&receipt.arch, "_-")
        || !token(&receipt.target_version, ".:+~_-^")
        || !token(&receipt.current_version, ".:+~_-^")
        || receipt.fingerprint.len() != 64
        || !receipt.fingerprint.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err("此渠道或版本不能可靠锁定，请按原渠道更新".into());
    }
    Ok(())
}
fn query(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .query_output()
        .map_err(|_| "原渠道更新计划读取超时或查询工具不可用")?;
    if !output.status.success() || output.stdout.len() > 4 * 1024 * 1024 {
        return Err("原渠道更新计划读取失败；未执行升级".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn apt_candidate(policy: &str) -> Option<&str> {
    policy
        .lines()
        .find_map(|line| line.trim().strip_prefix("Candidate:").map(str::trim))
        .filter(|version| *version != "(none)")
}
fn installed_record(text: &str, arch: &str, apt: bool) -> Result<String, String> {
    let lines = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if lines.len() != 1 {
        return Err("安装实例存在多个架构或当前状态无法确认".into());
    }
    let fields = lines[0].split('\t').collect::<Vec<_>>();
    if fields.len() < 2
        || fields[1] != arch
        || (apt && !fields.get(2).is_some_and(|status| status.starts_with("ii")))
    {
        return Err("原安装实例或架构已经变化，请重新检查".into());
    }
    Ok(fields[0].into())
}
fn rpm_candidate<'a>(text: &'a str, name: &str, arch: &str) -> Result<(&'a str, &'a str), String> {
    let key = format!("{name}.{arch}");
    let rows = text
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            (fields.len() == 3 && fields[0] == key).then(|| (fields[1], fields[2]))
        })
        .collect::<Vec<_>>();
    if rows.len() != 1 {
        return Err("原仓库候选版本无法唯一确认，请按原渠道更新".into());
    }
    Ok(rows[0])
}
pub fn dnf4_supported() -> bool {
    query("/usr/bin/dnf", &["--version"])
        .ok()
        .is_some_and(|text| {
            text.lines()
                .next()
                .is_some_and(|line| line.trim().starts_with("4."))
        })
}
pub fn prepare(
    manager: &str,
    name: &str,
    target_version: &str,
    arch: &str,
) -> Result<UpgradeReceipt, String> {
    let mut receipt = UpgradeReceipt {
        manager: manager.into(),
        name: name.into(),
        target_version: target_version.into(),
        arch: arch.into(),
        current_version: "placeholder".into(),
        fingerprint: "0".repeat(64),
    };
    validate(&receipt)?;
    let (current, evidence) = match manager {
        "apt" => {
            let key = format!("{name}:{arch}");
            let installed = query(
                "/usr/bin/dpkg-query",
                &[
                    "-W",
                    "-f=${Version}\t${Architecture}\t${db:Status-Abbrev}\n",
                    "--",
                    &key,
                ],
            )?;
            let current = installed_record(&installed, arch, true)?;
            let policy = query("/usr/bin/apt-cache", &["policy", "--", &key])?;
            if apt_candidate(&policy) != Some(target_version) {
                return Err("APT 候选版本已经变化，请重新检测确认".into());
            }
            (current, format!("{installed}\n{policy}"))
        }
        "dnf" => {
            if !dnf4_supported() {
                return Err("此 DNF 版本未确认精确 NEVRA 执行支持，请按原渠道更新".into());
            }
            let key = format!("{name}.{arch}");
            let installed = query(
                "/usr/bin/rpm",
                &[
                    "-q",
                    "--qf",
                    "%{EPOCHNUM}:%{VERSION}-%{RELEASE}\t%{ARCH}\n",
                    "--",
                    &key,
                ],
            )?;
            let current = installed_record(&installed, arch, false)?;
            let candidates = query(
                "/usr/bin/dnf",
                &["--cacheonly", "-q", "list", "--upgrades", "--", &key],
            )?;
            let (version, repository) = rpm_candidate(&candidates, name, arch)?;
            if version != target_version {
                return Err("DNF 原仓库候选版本已经变化，请重新检测确认".into());
            }
            (
                current,
                format!("{installed}\n{key}\t{version}\t{repository}"),
            )
        }
        _ => return Err("此渠道仅支持原渠道更新建议".into()),
    };
    if current.trim_start_matches("0:") == target_version.trim_start_matches("0:") {
        return Err("该版本已经安装，无需执行升级".into());
    }
    receipt.current_version = current;
    receipt.fingerprint = hex::encode(Sha256::digest(evidence.as_bytes()));
    validate(&receipt)?;
    Ok(receipt)
}
pub fn verify(receipt: &UpgradeReceipt) -> Result<(), String> {
    validate(receipt)?;
    let current = prepare(
        &receipt.manager,
        &receipt.name,
        &receipt.target_version,
        &receipt.arch,
    )?;
    if current != *receipt {
        return Err("授权期间软件源、候选版本或安装状态已变化；请重新检查确认".into());
    }
    Ok(())
}
pub fn resolve(receipt: &UpgradeReceipt) -> Result<UpgradeCommand, String> {
    validate(receipt)?;
    Ok(match receipt.manager.as_str() {
        "apt" => UpgradeCommand {
            program: "/usr/bin/apt-get",
            args: vec![
                "install".into(),
                "--only-upgrade".into(),
                "--no-remove".into(),
                "-y".into(),
                "--".into(),
                format!(
                    "{}:{}={}",
                    receipt.name, receipt.arch, receipt.target_version
                ),
            ],
        },
        "dnf" => UpgradeCommand {
            program: "/usr/bin/dnf",
            args: vec![
                "install-nevra".into(),
                "-y".into(),
                "--".into(),
                format!(
                    "{}-{}.{}",
                    receipt.name, receipt.target_version, receipt.arch
                ),
            ],
        },
        _ => return Err("此渠道无法可靠固定更新版本".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn receipt(manager: &str) -> UpgradeReceipt {
        UpgradeReceipt {
            manager: manager.into(),
            name: "my.tool".into(),
            target_version: "1:2.1-3".into(),
            arch: "x86_64".into(),
            current_version: "1:2.0-1".into(),
            fingerprint: "a".repeat(64),
        }
    }
    #[test]
    fn exact_commands_never_fall_back_to_unversioned_package_names() {
        let apt = resolve(&receipt("apt")).unwrap();
        assert_eq!(apt.program, "/usr/bin/apt-get");
        assert_eq!(apt.args.last().unwrap(), "my.tool:x86_64=1:2.1-3");
        assert!(apt.args.contains(&"--no-remove".into()));
        let dnf = resolve(&receipt("dnf")).unwrap();
        assert_eq!(dnf.args[0], "install-nevra");
        assert_eq!(dnf.args.last().unwrap(), "my.tool-1:2.1-3.x86_64");
        for manager in ["pacman", "yum", "zypper", "shell"] {
            assert!(resolve(&receipt(manager)).is_err());
        }
        for bad in [
            "-x",
            "$(id)",
            "tool;rm",
            "../tool",
            "foo*",
            "https://example.com",
        ] {
            let mut item = receipt("apt");
            item.name = bad.into();
            assert!(resolve(&item).is_err());
        }
        let mut item = receipt("apt");
        item.target_version = "1.0 --force".into();
        assert!(resolve(&item).is_err());
    }
    #[test]
    fn candidate_and_installation_parsers_reject_ambiguity() {
        assert_eq!(
            apt_candidate("tool:\n  Installed: 2.0\n  Candidate: 2.1\n"),
            Some("2.1")
        );
        assert_eq!(apt_candidate("  Candidate: (none)\n"), None);
        assert_eq!(
            installed_record("2.0\tamd64\tii \n", "amd64", true).unwrap(),
            "2.0"
        );
        assert!(installed_record("2.0\tamd64\trc \n", "amd64", true).is_err());
        assert!(installed_record("2.0\tamd64\tii \n2.0\ti386\tii \n", "amd64", true).is_err());
        assert_eq!(
            rpm_candidate("my.tool.x86_64 1:2.1-3 updates\n", "my.tool", "x86_64").unwrap(),
            ("1:2.1-3", "updates")
        );
        assert!(
            rpm_candidate(
                "my.tool.x86_64 1:2.1-3 updates\nmy.tool.x86_64 1:2.1-3 other\n",
                "my.tool",
                "x86_64"
            )
            .is_err()
        );
    }
}
