//! Exact, unprivileged handoff for a legacy CLI user-level Runner unit.
//! System units need the separate, privileged fixed-resource handoff; this
//! adapter deliberately has no path for `systemctl --system`.
use crate::migration::{LegacyOwner, LegacyOwnerSnapshot, LegacyProcess};
use crate::process::CommandOutputExt;
use crate::{LocalAccount, SetupDiagnostic, SetupResultValue};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn conflict(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(
        code,
        message,
        "Inspect the original unit and select an explicit migration path; no service was changed",
    )
}

fn simple_path(path: &Path) -> SetupResultValue<&str> {
    let value = path
        .to_str()
        .filter(|value| {
            path.is_absolute()
                && !value.is_empty()
                && !value.bytes().any(|byte| {
                    byte.is_ascii_whitespace() || matches!(byte, b'"' | b'\\' | b'%' | 0)
                })
        })
        .ok_or_else(|| {
            conflict(
                "legacy_unit_path",
                "The old unit uses a path that cannot be matched exactly",
            )
        })?;
    Ok(value)
}

fn expected_unit(program: &Path, config: &Path, working: &Path) -> SetupResultValue<String> {
    Ok(format!("[Unit]\nDescription=WebCodex Runner\n\n[Service]\nType=simple\nExecStart=\"{}\" \"--config\" \"{}\"\nExecReload=/bin/kill -HUP $MAINPID\nRestart=always\nRestartSec=5s\nStandardOutput=journal\nStandardError=journal\nEnvironment=RUST_LOG=info\nWorkingDirectory={}\n\n[Install]\nWantedBy=default.target\n",
        simple_path(program)?, simple_path(config)?, simple_path(working)?))
}

fn program_from_unit(content: &str, config: &Path) -> SetupResultValue<PathBuf> {
    let prefix = "ExecStart=\"";
    let suffix = format!("\" \"--config\" \"{}\"", simple_path(config)?);
    let line = content
        .lines()
        .find(|line| line.starts_with(prefix))
        .ok_or_else(|| {
            conflict(
                "legacy_unit_format",
                "The old unit has no expected Runner command",
            )
        })?;
    let program = line
        .strip_prefix(prefix)
        .and_then(|line| line.strip_suffix(&suffix))
        .ok_or_else(|| {
            conflict(
                "legacy_unit_format",
                "The old Runner command does not use the expected config",
            )
        })?;
    let path = PathBuf::from(program);
    if path.file_name().and_then(|name| name.to_str()) != Some("webcodex-runner") {
        return Err(conflict(
            "legacy_unit_program",
            "The old unit does not run webcodex-runner",
        ));
    }
    simple_path(&path)?;
    Ok(path)
}

fn private_unit(path: &Path, owner_uid: &str) -> SetupResultValue<String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        conflict(
            "legacy_unit_missing",
            "The expected old user unit is absent",
        )
    })?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.uid().to_string() != owner_uid
        || metadata.mode() & 0o022 != 0
        || metadata.nlink() != 1
        || metadata.len() > 16 * 1024
    {
        return Err(conflict(
            "legacy_unit_owner",
            "The old unit file owner or type is unsafe",
        ));
    }
    fs::read_to_string(path)
        .map_err(|_| conflict("legacy_unit_unreadable", "The old unit cannot be read"))
}

fn systemctl(args: &[&str]) -> SetupResultValue<String> {
    let binary = ["/usr/bin/systemctl", "/bin/systemctl"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| conflict("legacy_manager_missing", "systemctl is unavailable"))?;
    let result = Command::new(binary)
        .arg("--user")
        .args(args)
        .bounded_output()
        .map_err(|_| {
            conflict(
                "legacy_manager_unknown",
                "The user service manager did not produce a bounded result",
            )
        })?;
    if !result.status.success() {
        return Err(conflict(
            "legacy_manager_failed",
            "The user service manager rejected the operation",
        ));
    }
    String::from_utf8(result.stdout).map_err(|_| {
        conflict(
            "legacy_manager_unknown",
            "The user service manager returned invalid text",
        )
    })
}

#[derive(Debug)]
struct UnitStatus {
    fragment: PathBuf,
    load: String,
    active: String,
    enabled: String,
    pid: u32,
    dropins: String,
    reload: String,
}

fn parse_status(output: &str) -> SetupResultValue<UnitStatus> {
    let mut values = std::collections::BTreeMap::new();
    for line in output.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| conflict("legacy_manager_unknown", "The unit status is malformed"))?;
        if values.insert(key, value).is_some() {
            return Err(conflict(
                "legacy_manager_unknown",
                "The unit status repeats a field",
            ));
        }
    }
    let field = |key| {
        values.get(key).copied().ok_or_else(|| {
            conflict(
                "legacy_manager_unknown",
                "The unit status omits a required field",
            )
        })
    };
    Ok(UnitStatus {
        fragment: PathBuf::from(field("FragmentPath")?),
        load: field("LoadState")?.into(),
        active: field("ActiveState")?.into(),
        enabled: field("UnitFileState")?.into(),
        pid: field("MainPID")?
            .parse()
            .map_err(|_| conflict("legacy_manager_unknown", "The unit PID is invalid"))?,
        dropins: field("DropInPaths")?.into(),
        reload: field("NeedDaemonReload")?.into(),
    })
}

fn process_generation(pid: u32, program: &Path, config: &Path, uid: &str) -> SetupResultValue<u64> {
    if pid == 0 {
        return Err(conflict(
            "legacy_process_unknown",
            "The running old Runner has no PID",
        ));
    }
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let metadata = fs::symlink_metadata(&proc)
        .map_err(|_| conflict("legacy_process_unknown", "The old Runner PID disappeared"))?;
    if metadata.uid().to_string() != uid {
        return Err(conflict(
            "legacy_process_owner",
            "The old Runner PID belongs to another user",
        ));
    }
    let exe = fs::read_link(proc.join("exe")).map_err(|_| {
        conflict(
            "legacy_process_unknown",
            "The old Runner executable cannot be identified",
        )
    })?;
    if exe
        != program.canonicalize().map_err(|_| {
            conflict(
                "legacy_unit_program",
                "The old Runner binary is unavailable",
            )
        })?
    {
        return Err(conflict(
            "legacy_process_program",
            "The old Runner PID uses another executable",
        ));
    }
    let command = fs::read(proc.join("cmdline")).map_err(|_| {
        conflict(
            "legacy_process_unknown",
            "The old Runner command cannot be read",
        )
    })?;
    let args = command
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    if args.len() != 3 || args[1] != b"--config" || args[2] != config.as_os_str().as_encoded_bytes()
    {
        return Err(conflict(
            "legacy_process_command",
            "The old Runner PID uses another config",
        ));
    }
    let stat = fs::read_to_string(proc.join("stat")).map_err(|_| {
        conflict(
            "legacy_process_unknown",
            "The old Runner generation cannot be read",
        )
    })?;
    stat.rsplit_once(") ")
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| {
            conflict(
                "legacy_process_unknown",
                "The old Runner generation is invalid",
            )
        })
}

fn fingerprint_files(
    unit_hash: &str,
    config: &Path,
    extra: &[PathBuf],
    uid: &str,
) -> SetupResultValue<String> {
    let mut digest = Sha256::new();
    digest.update(unit_hash.as_bytes());
    for path in std::iter::once(config).chain(extra.iter().map(PathBuf::as_path)) {
        crate::legacy_cli::real_ancestors(path)?;
        let meta = fs::symlink_metadata(path).map_err(|_| {
            conflict(
                "legacy_config_missing",
                "An old Runner identity file disappeared",
            )
        })?;
        let registry_dir = path
            == config
                .parent()
                .unwrap_or(Path::new("/"))
                .join("project-registry");
        if (registry_dir && !meta.is_dir())
            || (!registry_dir && (!meta.is_file() || meta.nlink() != 1))
            || meta.file_type().is_symlink()
            || meta.uid().to_string() != uid
            || meta.mode() & 0o077 != 0
        {
            return Err(conflict(
                "legacy_config_owner",
                "An old Runner identity file is not private to the project user",
            ));
        }
        digest.update(path.as_os_str().as_encoded_bytes());
        for value in [
            meta.ino(),
            meta.len(),
            meta.mtime() as u64,
            meta.mtime_nsec() as u64,
            meta.ctime() as u64,
            meta.ctime_nsec() as u64,
        ] {
            digest.update(value.to_le_bytes());
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) struct UserRunnerOwner {
    unit: String,
    unit_path: PathBuf,
    unit_sha256: String,
    config: PathBuf,
    program: PathBuf,
    account: LocalAccount,
    config_fingerprint: String,
    extra_paths: Vec<PathBuf>,
}

impl UserRunnerOwner {
    pub(crate) fn new(
        unit_path: PathBuf,
        config: PathBuf,
        account: LocalAccount,
        extra_paths: Vec<PathBuf>,
    ) -> SetupResultValue<Self> {
        crate::legacy_cli::real_ancestors(&unit_path)?;
        let unit = unit_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| name.starts_with("webcodex-runner") && name.ends_with(".service"))
            .ok_or_else(|| {
                conflict(
                    "legacy_unit_name",
                    "The old unit name is not a CLI Runner unit",
                )
            })?
            .to_owned();
        let content = private_unit(&unit_path, &account.identity)?;
        let program = program_from_unit(&content, &config)?;
        if content != expected_unit(&program, &config, &account.home)? {
            return Err(conflict(
                "legacy_unit_format",
                "The old unit differs from the CLI user-service template",
            ));
        }
        let unit_sha256 = format!("{:x}", Sha256::digest(content.as_bytes()));
        let config_fingerprint =
            fingerprint_files(&unit_sha256, &config, &extra_paths, &account.identity)?;
        Ok(Self {
            unit,
            unit_path,
            unit_sha256,
            config,
            program,
            account,
            config_fingerprint,
            extra_paths,
        })
    }

    fn status(&self) -> SetupResultValue<UnitStatus> {
        let output = systemctl(&["show", &self.unit, "--no-pager", "--property=LoadState,FragmentPath,ActiveState,UnitFileState,MainPID,DropInPaths,NeedDaemonReload"])?;
        let status = parse_status(&output)?;
        if status.load != "loaded"
            || status.fragment != self.unit_path
            || !matches!(status.active.as_str(), "active" | "inactive")
            || !matches!(status.enabled.as_str(), "enabled" | "disabled")
            || !status.dropins.is_empty()
            || status.reload != "no"
        {
            return Err(conflict(
                "legacy_unit_status",
                "The old unit is not a known loaded user service",
            ));
        }
        Ok(status)
    }

    pub(crate) fn require_active_enabled(&self) -> SetupResultValue<()> {
        let status = self.status()?;
        if status.active != "active" || status.enabled != "enabled" || status.pid == 0 {
            return Err(conflict(
                "legacy_unit_state",
                "Only an enabled, active old Runner can be migrated automatically",
            ));
        }
        process_generation(
            status.pid,
            &self.program,
            &self.config,
            &self.account.identity,
        )?;
        Ok(())
    }
}

impl LegacyOwner for UserRunnerOwner {
    async fn inspect(&mut self) -> SetupResultValue<LegacyOwnerSnapshot> {
        crate::legacy_cli::real_ancestors(&self.unit_path)?;
        let content = private_unit(&self.unit_path, &self.account.identity)?;
        if format!("{:x}", Sha256::digest(content.as_bytes())) != self.unit_sha256 {
            return Err(conflict(
                "legacy_unit_changed",
                "The old unit changed during migration",
            ));
        }
        let fingerprint = fingerprint_files(
            &self.unit_sha256,
            &self.config,
            &self.extra_paths,
            &self.account.identity,
        )?;
        if fingerprint != self.config_fingerprint {
            return Err(conflict(
                "legacy_config_changed",
                "The old Runner config changed during migration",
            ));
        }
        let status = self.status()?;
        let processes = if status.active == "active" {
            vec![LegacyProcess {
                kind: "runner".into(),
                pid: status.pid,
                generation: process_generation(
                    status.pid,
                    &self.program,
                    &self.config,
                    &self.account.identity,
                )?,
            }]
        } else if status.pid == 0 {
            Vec::new()
        } else {
            return Err(conflict(
                "legacy_process_unknown",
                "The inactive old unit still reports a PID",
            ));
        };
        Ok(LegacyOwnerSnapshot {
            owner_id: format!("systemd-user:{}:{}", self.account.identity, self.unit),
            configuration_fingerprint: fingerprint,
            processes,
        })
    }

    async fn stop(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        if self.inspect().await? != *expected {
            return Err(conflict(
                "legacy_process_changed",
                "The old Runner changed before stop",
            ));
        }
        let status = self.status()?;
        if status.enabled != "enabled" || status.active != "active" {
            return Err(conflict(
                "legacy_unit_state",
                "The old Runner state changed before stop",
            ));
        }
        systemctl(&["disable", "--now", &self.unit])?;
        let after = self.status()?;
        if after.enabled != "disabled" || after.active != "inactive" || after.pid != 0 {
            return Err(conflict(
                "legacy_stop_unconfirmed",
                "The old Runner did not confirm disabled and stopped",
            ));
        }
        Ok(())
    }

    async fn restore(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        let now = self.inspect().await?;
        if now.owner_id != expected.owner_id
            || now.configuration_fingerprint != expected.configuration_fingerprint
        {
            return Err(conflict(
                "legacy_unit_changed",
                "The old Runner changed before restore",
            ));
        }
        let status = self.status()?;
        if !now.processes.is_empty() {
            if status.enabled == "enabled" {
                return Ok(());
            }
            systemctl(&["enable", &self.unit])?;
        } else {
            if status.enabled != "disabled" {
                return Err(conflict(
                    "legacy_unit_state",
                    "The old Runner enablement is unknown",
                ));
            }
            systemctl(&["enable", "--now", &self.unit])?;
        }
        self.require_active_enabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_only_exact_legacy_user_unit() {
        let program = Path::new("/opt/webcodex/webcodex-runner");
        let config = Path::new("/home/alice/.config/webcodex/runner.toml");
        let working = Path::new("/home/alice");
        let text = expected_unit(program, config, working).unwrap();
        assert_eq!(program_from_unit(&text, config).unwrap(), program);
        assert!(expected_unit(program, config, working)
            .unwrap()
            .contains("WantedBy=default.target"));
        assert!(program_from_unit(&text.replace("--config", "--token"), config).is_err());
        assert!(simple_path(Path::new("/tmp/has space")).is_err());
    }
    #[test]
    fn rejects_unknown_or_ambiguous_status() {
        assert!(parse_status("LoadState=loaded\nLoadState=masked\n").is_err());
        assert!(parse_status("LoadState=loaded\n").is_err());
    }
    #[test]
    fn accepts_only_the_exact_default_user_unit_file() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("runner.toml");
        fs::write(&config, b"token = 'private'\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
        let program = temp.path().join("webcodex-runner");
        fs::write(&program, b"binary").unwrap();
        let unit = temp.path().join("webcodex-runner.service");
        let account = LocalAccount {
            name: "test".into(),
            identity: unsafe { libc::geteuid() }.to_string(),
            home: temp.path().to_path_buf(),
        };
        fs::write(
            &unit,
            expected_unit(&program, &config, &account.home).unwrap(),
        )
        .unwrap();
        fs::set_permissions(&unit, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            UserRunnerOwner::new(unit.clone(), config.clone(), account.clone(), vec![]).is_ok(),
            "{:?}",
            UserRunnerOwner::new(unit.clone(), config.clone(), account.clone(), vec![]).err()
        );
        fs::write(
            &unit,
            expected_unit(&program, &config, &account.home)
                .unwrap()
                .replace("Restart=always", "Restart=no"),
        )
        .unwrap();
        assert!(UserRunnerOwner::new(unit, config, account, vec![]).is_err());
    }
    #[test]
    fn registry_directory_change_invalidates_captured_identity() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("runner.toml");
        let registry = temp.path().join("project-registry");
        fs::write(&config, b"owner = 'alice'\n").unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
        fs::create_dir(&registry).unwrap();
        fs::set_permissions(&registry, fs::Permissions::from_mode(0o700)).unwrap();
        let uid = unsafe { libc::geteuid() }.to_string();
        let before = fingerprint_files("unit", &config, &[registry.clone()], &uid).unwrap();
        let added = registry.join("new.toml");
        fs::write(&added, b"id = 'new'\n").unwrap();
        assert_ne!(
            before,
            fingerprint_files("unit", &config, &[registry], &uid).unwrap()
        );
    }
}
