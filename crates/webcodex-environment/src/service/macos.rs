use super::*;
use crate::process::CommandOutputExt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::process::Command;

const DAEMON_DIR: &str = "/Library/LaunchDaemons";

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
            "LaunchDaemon requires a real user",
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

fn label(spec: &ServiceSpec) -> String {
    format!("org.webcodex.{}", spec.id)
}
fn plist_path(spec: &ServiceSpec) -> PathBuf {
    Path::new(DAEMON_DIR).join(format!("{}.plist", label(spec)))
}
fn launchctl(args: &[&str]) -> Result<String, ServiceError> {
    let output = Command::new("/bin/launchctl")
        .args(args)
        .bounded_output()
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "launchctl could not execute",
            )
        })?;
    if !output.status.success() {
        return Err(ServiceError::new(
            ServiceErrorCode::OperationFailed,
            format!(
                "launchctl {} failed (exit {:?})",
                args.first().unwrap_or(&"?"),
                output.status.code()
            ),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `None` is not loaded; `Some(false)` is loaded but has no live process.
fn load_state(spec: &ServiceSpec) -> Result<Option<bool>, ServiceError> {
    let target = format!("system/{}", label(spec));
    let output = Command::new("/bin/launchctl")
        .args(["print", &target])
        .bounded_output()
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OwnershipUnknown,
                "cannot inspect launchd service",
            )
        })?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        return Ok(Some(
            text.lines().any(|line| line.trim() == "state = running"),
        ));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Could not find service")
        || stderr.contains("not found")
        || stderr.contains("service not found")
    {
        return Ok(None);
    }
    Err(ServiceError::new(
        ServiceErrorCode::OwnershipUnknown,
        "launchd service status is indeterminate",
    ))
}

pub(super) fn inspect(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let expected = render_plist(spec)?;
    let path = plist_path(spec);
    let loaded = load_state(spec)?;
    let running = loaded.unwrap_or(false);
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && loaded.is_none() => {
            Ok(ServiceStatus::absent(&spec.id))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ServiceStatus {
            id: spec.id.clone(),
            ownership: Ownership::Foreign,
            installed: true,
            enabled: None,
            running: Some(running),
            detail: Some("loaded without expected plist".into()),
        }),
        Err(_) => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "cannot inspect LaunchDaemon plist",
        )),
        Ok(meta) => {
            let owned = meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.uid() == 0
                && meta.permissions().mode() & 0o022 == 0
                && fs::read_to_string(&path).ok().as_deref() == Some(&expected);
            Ok(ServiceStatus {
                id: spec.id.clone(),
                ownership: if owned {
                    Ownership::Owned
                } else {
                    Ownership::Foreign
                },
                installed: true,
                enabled: Some(true),
                running: Some(running),
                detail: None,
            })
        }
    }
}

pub(super) fn preflight(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
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
        expected_identity,
        home,
        ..
    } = &spec.account
    else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "macOS LaunchDaemon requires a real user account",
        ));
    };
    if expected_identity == "0" && spec.component == Component::Runner {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Runner must use a non-root project user",
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
    if let Some(home) = home {
        if !home.is_dir() {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "service user home is absent",
            ));
        }
    }
    if let Some(env_file) = &spec.env_file {
        if !spec
            .args
            .iter()
            .any(|arg| arg == &env_file.to_string_lossy())
        {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "LaunchDaemon does not load env files; runtime arguments must name the env file",
            ));
        }
    }
    let status = inspect(spec)?;
    match status.ownership {
        Ownership::Absent | Ownership::Owned => Ok(status),
        Ownership::Foreign => Err(ServiceError::new(
            ServiceErrorCode::ForeignService,
            "LaunchDaemon exists but is not owned by this configuration",
        )),
        Ownership::Unknown => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "LaunchDaemon ownership is unknown",
        )),
    }
}

pub(super) fn install(
    spec: &ServiceSpec,
    credential: Option<&ServiceCredential>,
) -> Result<ServiceStatus, ServiceError> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(ServiceError::new(
            ServiceErrorCode::PermissionDenied,
            "LaunchDaemon installation requires root authorization",
        ));
    }
    if credential.is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "launchd does not accept an SCM credential",
        ));
    }
    let status = preflight(spec)?;
    if status.ownership == Ownership::Owned {
        return Ok(status);
    }
    check_install_files(spec)?;
    let path = plist_path(spec);
    ensure_safe_new_path(&path)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&path)
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::OutcomeUnknown,
                "could not create LaunchDaemon plist",
            )
        })?;
    if file
        .write_all(render_plist(spec)?.as_bytes())
        .and_then(|_| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(&path);
        return Err(ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "plist write failed; inspect before retrying",
        ));
    }
    inspect(spec)
}

pub(super) fn start(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if status.running == Some(true) {
        return Ok(status);
    }
    let path = plist_path(spec);
    let action = if load_state(spec)?.is_some() {
        launchctl(&["kickstart", "-k", &format!("system/{}", label(spec))])
    } else {
        launchctl(&[
            "bootstrap",
            "system",
            path.to_str().ok_or_else(|| {
                ServiceError::new(ServiceErrorCode::InvalidSpec, "plist path is not UTF-8")
            })?,
        ])
    };
    action.map_err(|error| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("start failed ({error}); inspect before retrying"),
        )
    })?;
    inspect(spec)
}

pub(super) fn stop(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if load_state(spec)?.is_none() {
        return Ok(status);
    }
    launchctl(&["bootout", &format!("system/{}", label(spec))]).map_err(|error| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            format!("stop failed ({error}); inspect before retrying"),
        )
    })?;
    inspect(spec)
}

pub(super) fn restart(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
    let status = inspect(spec)?;
    check_owned(&status)?;
    if status.running == Some(false) {
        return start(spec);
    }
    launchctl(&["kickstart", "-k", &format!("system/{}", label(spec))]).map_err(|error| {
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
    if load_state(spec)?.is_some() {
        return Err(ServiceError::new(
            ServiceErrorCode::Busy,
            "unload LaunchDaemon explicitly before uninstall",
        ));
    }
    fs::remove_file(plist_path(spec)).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::OutcomeUnknown,
            "plist removal failed; inspect before retrying",
        )
    })?;
    inspect(spec)
}

pub(super) fn update_credential(
    _: &ServiceSpec,
    _: &ServiceCredential,
) -> Result<ServiceStatus, ServiceError> {
    Err(ServiceError::new(
        ServiceErrorCode::Unsupported,
        "LaunchDaemon accounts do not use an SCM credential",
    ))
}

fn xml(value: &str) -> Result<String, ServiceError> {
    if value.chars().any(|ch| ch.is_control() && ch != '\t') {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "plist value contains control characters",
        ));
    }
    Ok(value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;"))
}

pub(super) fn render_plist(spec: &ServiceSpec) -> Result<String, ServiceError> {
    let ServiceAccount::SystemUser {
        name, group, home, ..
    } = &spec.account
    else {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "LaunchDaemon requires a real user",
        ));
    };
    let path = |path: &Path| {
        path.to_str()
            .ok_or_else(|| {
                ServiceError::new(ServiceErrorCode::InvalidSpec, "plist path is not UTF-8")
            })
            .and_then(xml)
    };
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict>\n");
    out.push_str(&format!(
        "<key>Label</key><string>{}</string>\n",
        xml(&label(spec))?
    ));
    out.push_str(&format!("<!-- {} -->\n", xml(&ownership_marker(spec))?));
    out.push_str(&format!(
        "<key>UserName</key><string>{}</string>\n",
        xml(name)?
    ));
    if let Some(group) = group {
        out.push_str(&format!(
            "<key>GroupName</key><string>{}</string>\n",
            xml(group)?
        ));
    }
    out.push_str(&format!(
        "<key>WorkingDirectory</key><string>{}</string>\n",
        path(&spec.working_directory)?
    ));
    out.push_str("<key>ProgramArguments</key><array>\n");
    out.push_str(&format!("<string>{}</string>\n", path(&spec.program)?));
    for arg in &spec.args {
        out.push_str(&format!("<string>{}</string>\n", xml(arg)?));
    }
    // Runtime stdout can contain Tunnel IDs or request data. The managed
    // service writes only fixed lifecycle events to its private bounded log.
    out.push_str("</array>\n<key>RunAtLoad</key><true/>\n<key>KeepAlive</key><true/>\n<key>StandardOutPath</key><string>/dev/null</string>\n<key>StandardErrorPath</key><string>/dev/null</string>\n");
    if home.is_some() || !spec.environment.is_empty() {
        out.push_str("<key>EnvironmentVariables</key><dict>\n");
        if let Some(home) = home {
            out.push_str(&format!(
                "<key>HOME</key><string>{}</string>\n",
                path(home)?
            ));
        }
        for (key, value) in &spec.environment {
            out.push_str(&format!(
                "<key>{}</key><string>{}</string>\n",
                xml(key)?,
                xml(value)?
            ));
        }
        out.push_str("</dict>\n");
    }
    out.push_str("</dict></plist>\n");
    Ok(out)
}
