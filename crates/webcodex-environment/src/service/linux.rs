use super::*;
use crate::process::CommandOutputExt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::process::Command;

const UNIT_DIR: &str = "/etc/systemd/system";
mod runner_policy;

fn check_runner_configuration(spec: &ServiceSpec) -> Result<(), ServiceError> {
    if spec.component != Component::Runner {
        return Ok(());
    }
    let ServiceAccount::SystemUser {
        expected_identity,
        home,
        ..
    } = &spec.account
    else {
        return Ok(());
    };
    // Do not execute a user-owned candidate as root. Cross-account installations
    // retain their normal account checks and native startup validation instead.
    if *expected_identity != unsafe { libc::geteuid() }.to_string() {
        return Ok(());
    }
    let configs: Vec<_> = spec
        .args
        .windows(2)
        .filter(|args| args[0] == "--config")
        .collect();
    if configs.len() != 1 {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Runner preflight requires one exact --config path",
        ));
    }
    let mut command = Command::new(&spec.program);
    command
        .arg("--check-config")
        .arg("--config")
        .arg(&configs[0][1])
        .current_dir(&spec.working_directory);
    if let Some(home) = home {
        command.env("HOME", home);
    }
    command.envs(&spec.environment);
    crate::runner_preflight::check_command(&mut command)
        .map_err(|message| ServiceError::new(ServiceErrorCode::InvalidSpec, message))
}

pub(super) fn current_account() -> Result<CurrentAccount, ServiceError> {
    current_unix_account()
}

pub(super) fn grant_service_directory(spec: &ServiceSpec, path: &Path) -> Result<(), ServiceError> {
    let ServiceAccount::SystemUser {
        expected_identity, ..
    } = &spec.account
    else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Linux service requires a real user",
        ));
    };
    let meta = fs::symlink_metadata(path).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service directory is absent",
        )
    })?;
    if !meta.is_dir()
        || meta.file_type().is_symlink()
        || meta.uid().to_string() != *expected_identity
    {
        return Err(ServiceError::new(
            ServiceErrorCode::PermissionDenied,
            "service directory owner must match the real service UID",
        ));
    }
    Ok(())
}

pub(super) fn unit_name(spec: &ServiceSpec) -> String {
    format!("{}.service", spec.id)
}
fn unit_path(spec: &ServiceSpec) -> Result<PathBuf, ServiceError> {
    let directory = if spec.scope == ServiceScope::User {
        scope::user_account(spec)?.1.join(".config/systemd/user")
    } else {
        PathBuf::from(UNIT_DIR)
    };
    Ok(directory.join(unit_name(spec)))
}
fn socket_name(spec: &ServiceSpec) -> String {
    format!("{}.socket", spec.id)
}
fn socket_path(spec: &ServiceSpec) -> PathBuf {
    Path::new(UNIT_DIR).join(socket_name(spec))
}

fn systemctl(spec: &ServiceSpec, args: &[&str]) -> Result<String, ServiceError> {
    if spec.scope == ServiceScope::User {
        scope::verify_current_user(spec)?;
    }
    let binary = ["/usr/bin/systemctl", "/bin/systemctl"]
        .iter()
        .find(|path| Path::new(path).is_file())
        .ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "systemctl is unavailable",
            )
        })?;
    let mut command = Command::new(binary);
    if spec.scope == ServiceScope::User {
        command.arg("--user");
    }
    let output = command.args(args).bounded_output().map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "systemctl could not be executed",
        )
    })?;
    if !output.status.success() {
        return Err(ServiceError::new(
            ServiceErrorCode::OperationFailed,
            format!(
                "systemctl {} failed (exit {:?})",
                args.first().unwrap_or(&"?"),
                output.status.code()
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn show(spec: &ServiceSpec, unit: &str) -> Result<BTreeMap<String, String>, ServiceError> {
    let raw = systemctl(
        spec,
        &[
            "show",
            "--no-pager",
            "--property=LoadState,FragmentPath,ActiveState,UnitFileState",
            unit,
        ],
    )?;
    Ok(raw
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect())
}

fn field<'a>(show: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, ServiceError> {
    show.get(key).map(String::as_str).ok_or_else(|| {
        ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            format!("systemd omitted {key}"),
        )
    })
}

fn unit_status(
    spec: &ServiceSpec,
    path: &Path,
    name: &str,
    expected: &str,
) -> Result<ServiceStatus, ServiceError> {
    let observed = show(spec, name)?;
    let load = field(&observed, "LoadState")?;
    let fragment = field(&observed, "FragmentPath")?;
    let active = field(&observed, "ActiveState")?;
    let enabled = field(&observed, "UnitFileState")?;
    let disk = match fs::symlink_metadata(path) {
        Ok(meta)
            if meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.uid()
                    == if spec.scope == ServiceScope::User {
                        unsafe { libc::geteuid() }
                    } else {
                        0
                    }
                && meta.permissions().mode() & 0o022 == 0 =>
        {
            Some(fs::read_to_string(path).map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::OwnershipUnknown,
                    "cannot read existing unit",
                )
            })?)
        }
        Ok(_) => {
            return Ok(ServiceStatus {
                id: name.into(),
                ownership: Ownership::Foreign,
                installed: true,
                enabled: None,
                running: None,
                detail: Some("unit path is not a regular file".into()),
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "cannot inspect unit path",
            ))
        }
    };
    let absent = disk.is_none() && load == "not-found" && fragment.is_empty();
    if absent {
        return Ok(ServiceStatus::absent(name));
    }
    let ownership = if disk
        .as_deref()
        .is_some_and(|current| runner_policy::matches(current, expected))
        && (fragment == path.to_string_lossy() || (load == "not-found" && fragment.is_empty()))
    {
        Ownership::Owned
    } else {
        Ownership::Foreign
    };
    Ok(ServiceStatus {
        id: name.into(),
        ownership,
        installed: true,
        enabled: Some(matches!(enabled, "enabled" | "enabled-runtime")),
        running: Some(active == "active"),
        detail: Some(format!("load={load}; fragment={fragment}")),
    })
}

pub(super) fn inspect(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let rendered = render_unit(spec)?;
    let mut status = unit_status(spec, &unit_path(spec)?, &unit_name(spec), &rendered)?;
    status.id = spec.id.clone();
    if spec.linux_socket.is_some() {
        let socket = unit_status(
            spec,
            &socket_path(spec),
            &socket_name(spec),
            &render_socket(spec)?,
        )?;
        if socket.ownership == Ownership::Foreign {
            status.ownership = Ownership::Foreign;
        } else if socket.ownership == Ownership::Unknown
            || (socket.ownership == Ownership::Absent && status.ownership == Ownership::Owned)
            || (socket.ownership == Ownership::Owned && status.ownership == Ownership::Absent)
        {
            status.ownership = Ownership::Unknown;
        }
        status.installed |= socket.installed;
        status.enabled = match (status.enabled, socket.enabled) {
            (Some(a), Some(b)) => Some(a && b),
            _ => None,
        };
        status.running = match (status.running, socket.running) {
            (Some(a), Some(b)) => Some(a || b),
            _ => None,
        };
    }
    Ok(status)
}

pub(super) fn preflight_replacing(spec: &ServiceSpec) -> Result<(), ServiceError> {
    if spec.scope == ServiceScope::User {
        scope::verify_current_user(spec)?;
    }
    check_files(spec)?;
    if spec
        .program
        .metadata()
        .map(|m| m.permissions().mode() & 0o111 == 0)
        .unwrap_or(true)
    {
        return Err(ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service executable is not executable",
        ));
    }
    let ServiceAccount::SystemUser {
        name,
        group,
        expected_identity,
        home,
    } = &spec.account
    else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Linux services require an explicit real account",
        ));
    };
    if !expected_identity.bytes().all(|b| b.is_ascii_digit())
        || expected_identity == "0" && spec.component == Component::Runner
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Runner must name a non-root project UID",
        ));
    }
    let uid = Command::new("/usr/bin/id")
        .args(["-u", name])
        .bounded_output()
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "cannot resolve service user",
            )
        })?;
    if !uid.status.success() || String::from_utf8_lossy(&uid.stdout).trim() != expected_identity {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "service account UID differs from expected project owner",
        ));
    }
    if let Some(group) = group {
        let output = Command::new("/usr/bin/getent")
            .args(["group", group])
            .bounded_output()
            .map_err(|_| {
                ServiceError::new(
                    ServiceErrorCode::MissingPrerequisite,
                    "cannot resolve service group",
                )
            })?;
        if !output.status.success() {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "service group does not exist",
            ));
        }
    }
    if let Some(home) = home {
        if !home.is_dir() {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "service account home is absent",
            ));
        }
    }
    Ok(())
}

pub(super) fn preflight(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    preflight_replacing(spec)?;
    let status = inspect(spec)?;
    match status.ownership {
        Ownership::Absent | Ownership::Owned => Ok(status),
        Ownership::Foreign => Err(ServiceError::new(
            ServiceErrorCode::ForeignService,
            "systemd unit exists but is not owned by this configuration",
        )),
        Ownership::Unknown => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "systemd service/socket ownership is incomplete",
        )),
    }
}

fn write_new(path: &Path, body: &str) -> Result<(), ServiceError> {
    ensure_safe_new_path(path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "could not create unit without replacement",
            )
        })?;
    if file
        .write_all(body.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(path);
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "unit write failed; inspect before retrying",
        ));
    }
    Ok(())
}

pub(super) fn install(
    spec: &ServiceSpec,
    credential: Option<&ServiceCredential>,
) -> Result<ServiceStatus, ServiceError> {
    if spec.scope == ServiceScope::System && unsafe { libc::geteuid() } != 0 {
        return Err(ServiceError::new(
            ServiceErrorCode::PermissionDenied,
            "system service installation requires root authorization",
        ));
    }
    if credential.is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Linux systemd does not accept an SCM credential",
        ));
    }
    let current = preflight(spec)?;
    check_runner_configuration(spec)?;
    if current.ownership == Ownership::Owned && spec.component == Component::Runner {
        let owner = if spec.scope == ServiceScope::User {
            unsafe { libc::geteuid() }
        } else {
            0
        };
        if runner_policy::upgrade(&unit_path(spec)?, &render_unit(spec)?, owner)? {
            systemctl(spec, &["daemon-reload"])?;
        }
    }
    if current.ownership == Ownership::Owned {
        if current.enabled != Some(true) {
            check_install_files(spec)?;
            if spec.linux_socket.is_some() {
                systemctl(spec, &["enable", &socket_name(spec)])?;
            }
            systemctl(spec, &["enable", &unit_name(spec)])?;
            return inspect(spec);
        }
        return Ok(current);
    }
    check_install_files(spec)?;
    let service_path = unit_path(spec)?;
    if spec.scope == ServiceScope::User {
        scope::ensure_user_service_directory(
            service_path.parent().ok_or_else(|| {
                ServiceError::new(ServiceErrorCode::InvalidSpec, "User unit parent missing")
            })?,
            unsafe { libc::geteuid() },
        )?;
    }
    let socket_path = spec.linux_socket.as_ref().map(|_| socket_path(spec));
    let service = render_unit(spec)?;
    let socket = if socket_path.is_some() {
        Some(render_socket(spec)?)
    } else {
        None
    };
    write_new(&service_path, &service)?;
    if let (Some(path), Some(body)) = (&socket_path, &socket) {
        if let Err(error) = write_new(path, body) {
            let _ = fs::remove_file(&service_path);
            return Err(error);
        }
    }
    let result = (|| {
        systemctl(spec, &["daemon-reload"])?;
        if socket_path.is_some() {
            systemctl(spec, &["enable", &socket_name(spec)])?;
        }
        systemctl(spec, &["enable", &unit_name(spec)])?;
        Ok::<(), ServiceError>(())
    })();
    if let Err(error) = result {
        let _ = systemctl(spec, &["disable", &unit_name(spec)]);
        if socket_path.is_some() {
            let _ = systemctl(spec, &["disable", &socket_name(spec)]);
        }
        if let Some(path) = socket_path {
            let _ = fs::remove_file(path);
        }
        let _ = fs::remove_file(service_path);
        let _ = systemctl(spec, &["daemon-reload"]);
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("install failed ({error}); rollback attempted; inspect before retrying"),
        ));
    }
    inspect(spec)
}

pub(super) fn start(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if status.running == Some(true) && spec.linux_socket.is_none() {
        return Ok(status);
    }
    check_runner_configuration(spec)?;
    let result = if spec.linux_socket.is_some() {
        systemctl(spec, &["start", &socket_name(spec)])
            .and_then(|_| systemctl(spec, &["start", &unit_name(spec)]))
    } else {
        systemctl(spec, &["start", &unit_name(spec)])
    };
    if let Err(error) = result {
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("start failed ({error}); inspect before retrying"),
        ));
    }
    inspect(spec)
}

pub(super) fn stop(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if status.running == Some(false) && spec.linux_socket.is_none() {
        return Ok(status);
    }
    let result = if spec.linux_socket.is_some() {
        systemctl(spec, &["stop", &socket_name(spec)])
            .and_then(|_| systemctl(spec, &["stop", &unit_name(spec)]))
    } else {
        systemctl(spec, &["stop", &unit_name(spec)])
    };
    if let Err(error) = result {
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("stop failed ({error}); inspect before retrying"),
        ));
    }
    inspect(spec)
}

pub(super) fn restart(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    check_runner_configuration(spec)?;
    if spec.linux_socket.is_some() {
        stop(spec)?;
        return start(spec);
    }
    systemctl(spec, &["restart", &unit_name(spec)]).map_err(|error| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("restart failed ({error}); inspect before retrying"),
        )
    })?;
    inspect(spec)
}

pub(super) fn uninstall(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    if status.ownership == Ownership::Absent {
        return Ok(status);
    }
    check_owned(&status)?;
    if status.running != Some(false) {
        return Err(ServiceError::new(
            ServiceErrorCode::Busy,
            "stop service explicitly before uninstall",
        ));
    }
    let mut units = vec![(unit_name(spec), unit_path(spec)?, render_unit(spec)?)];
    if spec.linux_socket.is_some() {
        units.push((socket_name(spec), socket_path(spec), render_socket(spec)?));
    }
    let mut prior_enabled = Vec::new();
    for (name, path, body) in &units {
        let observed = unit_status(spec, path, name, body)?;
        check_owned(&observed)?;
        if observed.running != Some(false) {
            return Err(ServiceError::new(
                ServiceErrorCode::Busy,
                "stop all service units before uninstall",
            ));
        }
        prior_enabled.push(observed.enabled == Some(true));
    }
    let result = (|| {
        for (name, _, _) in &units {
            systemctl(spec, &["disable", name])?;
        }
        for (_, path, _) in &units {
            fs::remove_file(path).map_err(|_| {
                ServiceError::new(ServiceErrorCode::OutcomeUnknown, "unit removal failed")
            })?;
        }
        systemctl(spec, &["daemon-reload"])?;
        Ok::<(), ServiceError>(())
    })();
    if let Err(error) = result {
        // Exact rendered contents were validated before the first mutation.
        // Restore files before restoring their prior enabled state.
        for (_, path, body) in &units {
            if !path.exists() {
                let _ = write_new(path, body);
            }
        }
        let _ = systemctl(spec, &["daemon-reload"]);
        for ((name, _, _), enabled) in units.iter().zip(&prior_enabled) {
            if *enabled {
                let _ = systemctl(spec, &["enable", name]);
            }
        }
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("uninstall failed ({error}); restore attempted; inspect before retrying"),
        ));
    }
    inspect(spec)
}

pub(super) fn update_credential(
    _: &ServiceSpec,
    _: &ServiceCredential,
) -> Result<ServiceStatus, ServiceError> {
    Err(ServiceError::new(
        ServiceErrorCode::Unsupported,
        "systemd service accounts do not use an SCM credential",
    ))
}

fn encode_arg(value: &str) -> Result<String, ServiceError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "invalid systemd argument",
        ));
    }
    Ok(format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('%', "%%")
    ))
}
fn encode_path(path: &Path) -> Result<String, ServiceError> {
    let value = path
        .to_str()
        .filter(|s| !s.contains(['\n', '\r', '\0']))
        .ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "systemd path is not valid UTF-8",
            )
        })?;
    Ok(value
        .chars()
        .map(|ch| match ch {
            ' ' => "\\x20".into(),
            '"' => "\\x22".into(),
            '\\' => "\\x5c".into(),
            '%' => "%%".into(),
            _ => ch.to_string(),
        })
        .collect())
}
fn validate_account_name(name: &str) -> Result<(), ServiceError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "invalid systemd account name",
        ));
    }
    Ok(())
}

pub(super) fn render_unit(spec: &ServiceSpec) -> Result<String, ServiceError> {
    let ServiceAccount::SystemUser {
        name, group, home, ..
    } = &spec.account
    else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Linux requires a real account",
        ));
    };
    validate_account_name(name)?;
    if let Some(group) = group {
        validate_account_name(group)?;
    }
    let binary = spec
        .program
        .to_str()
        .filter(|s| !s.contains(['"', '\\', '%', '\n', '\r', '\0']))
        .ok_or_else(|| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "invalid systemd executable path",
            )
        })?;
    let mut out = format!(
        "# {}\n[Unit]\nDescription=WebCodex {}\n",
        ownership_marker(spec),
        spec.component.as_str()
    );
    if spec.linux_socket.is_some() {
        out.push_str(&format!(
            "Requires={}\nAfter=network-online.target {}\n",
            socket_name(spec),
            socket_name(spec)
        ));
    } else if spec.scope == ServiceScope::System {
        out.push_str("After=network-online.target\n");
    }
    if spec.scope == ServiceScope::System {
        out.push_str("Wants=network-online.target\n");
    }
    out.push_str("\n[Service]\nType=simple\n");
    if let Some(path) = &spec.env_file {
        out.push_str(&format!("EnvironmentFile={}\n", encode_path(path)?));
    }
    if let Some(home) = home {
        out.push_str(&format!(
            "Environment=HOME={}\n",
            encode_arg(home.to_str().ok_or_else(|| ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "HOME is not UTF-8"
            ))?)?
        ));
    }
    for (key, value) in &spec.environment {
        out.push_str(&format!(
            "Environment={}\n",
            encode_arg(&format!("{key}={value}"))?
        ));
    }
    out.push_str(&format!("ExecStart=\"{binary}\""));
    for arg in &spec.args {
        out.push(' ');
        out.push_str(&encode_arg(arg)?);
    }
    out.push_str(
        "\nRestart=on-failure\nRestartSec=5s\nStandardOutput=journal\nStandardError=journal\n",
    );
    if spec.component == Component::Runner {
        out.push_str(runner_policy::GUARD);
    }
    out.push_str(&format!(
        "WorkingDirectory={}\n",
        encode_path(&spec.working_directory)?
    ));
    if spec.scope == ServiceScope::System {
        out.push_str(&format!("User={name}\n"));
        if let Some(group) = group {
            out.push_str(&format!("Group={group}\n"));
        }
    }
    out.push_str(if spec.scope == ServiceScope::User {
        "\n[Install]\nWantedBy=default.target\n"
    } else {
        "\n[Install]\nWantedBy=multi-user.target\n"
    });
    Ok(out)
}

pub(super) fn render_socket(spec: &ServiceSpec) -> Result<String, ServiceError> {
    let socket = spec
        .linux_socket
        .as_ref()
        .ok_or_else(|| ServiceError::new(ServiceErrorCode::InvalidSpec, "socket is absent"))?;
    socket.listen.parse::<SocketAddr>().map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "socket address must be a fixed IP:port",
        )
    })?;
    Ok(format!("# {}\n[Unit]\nDescription=WebCodex HTTP Socket\n\n[Socket]\nListenStream={}\nService={}\nFileDescriptorName=webcodex-http\n\n[Install]\nWantedBy=sockets.target\n", ownership_marker(spec), socket.listen, unit_name(spec)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn user_unit_is_a_distinct_unprivileged_manager_namespace() {
        let mut spec = crate::service::tests::sample_spec();
        let system_path = unit_path(&spec).unwrap();
        spec.scope = ServiceScope::User;
        if let ServiceAccount::SystemUser { group, .. } = &mut spec.account {
            *group = None;
        }
        validate_spec(&spec).unwrap();
        let text = render_unit(&spec).unwrap();
        assert_eq!(
            system_path,
            Path::new("/etc/systemd/system/webcodex-runner.service")
        );
        assert_eq!(
            unit_path(&spec).unwrap(),
            Path::new("/home/alice/.config/systemd/user/webcodex-runner.service")
        );
        assert!(text.contains("WantedBy=default.target"));
        for forbidden in [
            "User=",
            "Group=",
            "multi-user.target",
            "network-online.target",
        ] {
            assert!(!text.contains(forbidden));
        }
        assert!(text.contains("--config"));
        assert!(text.contains("WebCodex managed v1"));
    }
    #[test]
    fn runner_unit_preserves_owner_and_escapes_arguments() {
        let mut spec = crate::service::tests::sample_spec();
        spec.args.push("a%b c".into());
        let body = render_unit(&spec).unwrap();
        assert!(body.contains("User=alice\n"));
        assert!(body.contains("\"a%%b c\""));
        assert!(body.contains("WebCodex managed v1 component=runner"));
        assert!(body.contains("RestartPreventExitStatus=2\n"));
        assert!(body.contains("Restart=on-failure\nRestartSec=5s\n"));
        spec.component = Component::Server;
        assert!(!render_unit(&spec)
            .unwrap()
            .contains("RestartPreventExitStatus"));
    }
    #[test]
    fn server_socket_matches_existing_unit_name() {
        let mut spec = crate::service::tests::sample_spec();
        spec.id = "webcodex".into();
        spec.component = Component::Server;
        spec.linux_socket = Some(LinuxSocketSpec {
            listen: "127.0.0.1:8765".into(),
        });
        assert!(render_socket(&spec)
            .unwrap()
            .contains("Service=webcodex.service"));
        assert_eq!(socket_name(&spec), "webcodex.socket");
    }
}
