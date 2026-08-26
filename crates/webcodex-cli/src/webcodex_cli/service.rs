use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use super::system::discover_named_binary_absolute;
use crate::ServiceScope;

pub(crate) const SERVER_SERVICE_FILE: &str = "/etc/systemd/system/webcodex.service";
pub(crate) const SERVER_SERVICE_UNIT: &str = "webcodex.service";
pub(crate) const SERVER_SOCKET_UNIT: &str = "webcodex.socket";
pub(crate) const AGENT_SERVICE_UNIT: &str = "webcodex-runner.service";
pub(crate) const DEFAULT_LOG_LINES: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessInvocation {
    pub(crate) operation: String,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) unit: Option<String>,
    pub(crate) inherit_stdio: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessOutput {
    pub(crate) success: bool,
    pub(crate) code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

pub(crate) trait ProcessExecutor {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, String>;
}

pub(crate) struct RealProcessExecutor;

impl ProcessExecutor for RealProcessExecutor {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, String> {
        let mut command = Command::new(&invocation.program);
        command.args(&invocation.args);
        if invocation.inherit_stdio {
            let status = command
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .map_err(|e| format!("failed to execute {}: {}", invocation.operation, e))?;
            return Ok(ProcessOutput {
                success: status.success(),
                code: status.code(),
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        let output = command
            .output()
            .map_err(|e| format!("failed to execute {}: {}", invocation.operation, e))?;
        Ok(ProcessOutput {
            success: output.status.success(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemdStatus {
    pub(crate) loaded: String,
    pub(crate) active: String,
    pub(crate) enabled: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ServiceControl {
    Start,
    Stop,
    Restart,
}

impl ServiceControl {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstallUnitResult {
    pub(crate) unit: String,
    pub(crate) started: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UninstallUnitResult {
    pub(crate) unit: String,
    pub(crate) removed: bool,
}

fn validate_systemd_value(field: &str, value: &str) -> Result<(), String> {
    if value.contains('\0') {
        return Err(format!("invalid systemd {field} value: contains NUL"));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(format!(
            "invalid systemd {field} value: contains a line break"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "invalid systemd {field} value: contains a control character"
        ));
    }
    Ok(())
}

pub(crate) fn encode_exec_argument(field: &str, value: &str) -> Result<String, String> {
    validate_systemd_value(field, value)?;
    if value.is_empty() {
        return Err(format!("invalid systemd {field} value: cannot be empty"));
    }
    let encoded = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%");
    Ok(format!("\"{encoded}\""))
}

fn utf8_absolute_path<'a>(field: &str, path: &'a Path) -> Result<&'a str, String> {
    if !path.is_absolute() {
        return Err(format!(
            "invalid systemd {field} value: path must be absolute"
        ));
    }
    let value = path
        .to_str()
        .ok_or_else(|| format!("invalid systemd {field} value: path is not valid UTF-8"))?;
    validate_systemd_value(field, value)?;
    if value.is_empty() {
        return Err(format!(
            "invalid systemd {field} value: path cannot be empty"
        ));
    }
    Ok(value)
}

pub(crate) fn encode_exec_program(field: &str, path: &Path) -> Result<String, String> {
    let value = utf8_absolute_path(field, path)?;
    if value.contains('"') {
        return Err(format!(
            "invalid systemd {field} value: executable path cannot contain a double quote"
        ));
    }
    if value.contains('\\') {
        return Err(format!(
            "invalid systemd {field} value: executable path cannot contain a backslash"
        ));
    }
    Ok(format!("\"{}\"", value.replace('%', "%%")))
}

pub(crate) fn encode_exec_path_argument(field: &str, path: &Path) -> Result<String, String> {
    let value = path
        .to_str()
        .ok_or_else(|| format!("invalid systemd {field} value: path is not valid UTF-8"))?;
    encode_exec_argument(field, value)
}

pub(crate) fn encode_unit_path_value(field: &str, path: &Path) -> Result<String, String> {
    let value = utf8_absolute_path(field, path)?;
    let mut encoded = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            ' ' => encoded.push_str("\\x20"),
            '"' => encoded.push_str("\\x22"),
            '\\' => encoded.push_str("\\x5c"),
            '%' => encoded.push_str("%%"),
            _ => encoded.push(ch),
        }
    }
    Ok(encoded)
}

pub(crate) fn validate_systemd_identity(field: &str, value: &str) -> Result<(), String> {
    validate_systemd_value(field, value)?;
    if value.is_empty() {
        return Err(format!("invalid systemd {field} value: cannot be empty"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!(
            "invalid systemd {field} value: use only ASCII letters, digits, '_', '-' or '.'"
        ));
    }
    Ok(())
}

pub(crate) fn systemctl_path() -> Result<PathBuf, String> {
    if !cfg!(target_os = "linux") {
        return Err("systemd service management is supported only on Linux".to_string());
    }
    discover_named_binary_absolute("systemctl").ok_or_else(|| {
        "systemctl was not found in an absolute PATH entry; install systemd or use a rendering-only mode"
            .to_string()
    })
}

pub(crate) fn journalctl_path() -> Result<PathBuf, String> {
    if !cfg!(target_os = "linux") {
        return Err("systemd journal access is supported only on Linux".to_string());
    }
    discover_named_binary_absolute("journalctl")
        .ok_or_else(|| "journalctl was not found in an absolute PATH entry".to_string())
}

pub(crate) fn service_unit_name(service_file: &Path, default: &str) -> String {
    service_file
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(default)
        .to_string()
}

fn systemctl_invocation(
    systemctl: &Path,
    operation: &str,
    args: Vec<String>,
    unit: Option<&str>,
) -> ProcessInvocation {
    ProcessInvocation {
        operation: operation.to_string(),
        program: systemctl.to_path_buf(),
        args,
        unit: unit.map(str::to_string),
        inherit_stdio: false,
    }
}

fn invocation_for_scope(scope: ServiceScope, invocation: &ProcessInvocation) -> ProcessInvocation {
    let mut invocation = invocation.clone();
    if scope == ServiceScope::User {
        invocation.args.insert(0, "--user".to_string());
        invocation.operation = invocation
            .operation
            .replacen("systemctl", "systemctl --user", 1);
        invocation.operation = invocation
            .operation
            .replacen("journalctl", "journalctl --user", 1);
    }
    invocation
}

struct ScopedProcessExecutor<'a, E> {
    inner: &'a mut E,
    scope: ServiceScope,
}

impl<E: ProcessExecutor> ProcessExecutor for ScopedProcessExecutor<'_, E> {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, String> {
        self.inner
            .execute(&invocation_for_scope(self.scope, invocation))
    }
}

pub(crate) fn plan_install(systemctl: &Path, unit: &str, no_start: bool) -> Vec<ProcessInvocation> {
    let mut enable_args = vec!["enable".to_string()];
    if !no_start {
        enable_args.push("--now".to_string());
    }
    enable_args.push(unit.to_string());
    let verify_action = if no_start { "is-enabled" } else { "is-active" };
    vec![
        systemctl_invocation(
            systemctl,
            "systemctl daemon-reload",
            vec!["daemon-reload".to_string()],
            Some(unit),
        ),
        systemctl_invocation(systemctl, "systemctl enable", enable_args, Some(unit)),
        systemctl_invocation(
            systemctl,
            &format!("systemctl {verify_action}"),
            vec![
                verify_action.to_string(),
                "--quiet".to_string(),
                unit.to_string(),
            ],
            Some(unit),
        ),
    ]
}

#[cfg(test)]
pub(crate) fn plan_install_for_scope(
    scope: ServiceScope,
    systemctl: &Path,
    unit: &str,
    no_start: bool,
) -> Vec<ProcessInvocation> {
    plan_install(systemctl, unit, no_start)
        .iter()
        .map(|invocation| invocation_for_scope(scope, invocation))
        .collect()
}

pub(crate) fn plan_control(
    systemctl: &Path,
    unit: &str,
    control: ServiceControl,
) -> Vec<ProcessInvocation> {
    let action = control.as_str();
    let mut plan = vec![systemctl_invocation(
        systemctl,
        &format!("systemctl {action}"),
        vec![action.to_string(), unit.to_string()],
        Some(unit),
    )];
    if matches!(control, ServiceControl::Start | ServiceControl::Restart) {
        plan.push(systemctl_invocation(
            systemctl,
            "systemctl is-active",
            vec![
                "is-active".to_string(),
                "--quiet".to_string(),
                unit.to_string(),
            ],
            Some(unit),
        ));
    }
    plan
}

#[cfg(test)]
pub(crate) fn plan_control_for_scope(
    scope: ServiceScope,
    systemctl: &Path,
    unit: &str,
    control: ServiceControl,
) -> Vec<ProcessInvocation> {
    plan_control(systemctl, unit, control)
        .iter()
        .map(|invocation| invocation_for_scope(scope, invocation))
        .collect()
}

pub(crate) fn plan_uninstall_before_remove(systemctl: &Path, unit: &str) -> Vec<ProcessInvocation> {
    vec![
        systemctl_invocation(
            systemctl,
            "systemctl stop",
            vec!["stop".to_string(), unit.to_string()],
            Some(unit),
        ),
        systemctl_invocation(
            systemctl,
            "systemctl disable",
            vec!["disable".to_string(), unit.to_string()],
            Some(unit),
        ),
    ]
}

#[cfg(test)]
pub(crate) fn plan_uninstall_before_remove_for_scope(
    scope: ServiceScope,
    systemctl: &Path,
    unit: &str,
) -> Vec<ProcessInvocation> {
    plan_uninstall_before_remove(systemctl, unit)
        .iter()
        .map(|invocation| invocation_for_scope(scope, invocation))
        .collect()
}

pub(crate) fn plan_uninstall_after_remove(systemctl: &Path, unit: &str) -> Vec<ProcessInvocation> {
    vec![
        systemctl_invocation(
            systemctl,
            "systemctl daemon-reload",
            vec!["daemon-reload".to_string()],
            Some(unit),
        ),
        systemctl_invocation(
            systemctl,
            "systemctl reset-failed",
            vec!["reset-failed".to_string(), unit.to_string()],
            Some(unit),
        ),
    ]
}

#[cfg(test)]
pub(crate) fn plan_uninstall_after_remove_for_scope(
    scope: ServiceScope,
    systemctl: &Path,
    unit: &str,
) -> Vec<ProcessInvocation> {
    plan_uninstall_after_remove(systemctl, unit)
        .iter()
        .map(|invocation| invocation_for_scope(scope, invocation))
        .collect()
}

pub(crate) fn journalctl_invocation(
    journalctl: &Path,
    unit: &str,
    lines: u32,
    since: Option<&str>,
    follow: bool,
) -> ProcessInvocation {
    let mut args = vec![
        "--unit".to_string(),
        unit.to_string(),
        "--lines".to_string(),
        lines.to_string(),
        "--no-pager".to_string(),
    ];
    if let Some(since) = since {
        args.push("--since".to_string());
        args.push(since.to_string());
    }
    if follow {
        args.push("--follow".to_string());
    }
    ProcessInvocation {
        operation: "journalctl logs".to_string(),
        program: journalctl.to_path_buf(),
        args,
        unit: Some(unit.to_string()),
        inherit_stdio: follow,
    }
}

#[cfg(test)]
pub(crate) fn journalctl_invocation_for_scope(
    scope: ServiceScope,
    journalctl: &Path,
    unit: &str,
    lines: u32,
    since: Option<&str>,
    follow: bool,
) -> ProcessInvocation {
    invocation_for_scope(
        scope,
        &journalctl_invocation(journalctl, unit, lines, since, follow),
    )
}

pub(crate) fn ensure_service_file_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| format!("{} must have a parent directory", path.display()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    let metadata = std::fs::symlink_metadata(parent)
        .map_err(|error| format!("failed to inspect {}: {error}", parent.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "refusing to use non-directory or symlinked service unit directory: {}",
            parent.display()
        ));
    }
    Ok(())
}

fn failure_detail(output: &ProcessOutput) -> String {
    let detail = output
        .stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .or_else(|| output.stdout.lines().find(|line| !line.trim().is_empty()))
        .unwrap_or("command returned a non-zero status");
    let mut detail = detail.trim().to_string();
    if detail.len() > 300 {
        detail.truncate(300);
        detail.push_str("...");
    }
    detail
}

pub(crate) fn execute_required<E: ProcessExecutor>(
    executor: &mut E,
    invocation: &ProcessInvocation,
) -> Result<ProcessOutput, String> {
    let output = executor.execute(invocation)?;
    if output.success {
        return Ok(output);
    }
    let unit = invocation.unit.as_deref().unwrap_or("systemd manager");
    Err(format!(
        "{} failed for {}: {}",
        invocation.operation,
        unit,
        failure_detail(&output)
    ))
}

pub(crate) fn execute_plan<E: ProcessExecutor>(
    executor: &mut E,
    plan: &[ProcessInvocation],
) -> Result<(), String> {
    for invocation in plan {
        execute_required(executor, invocation)?;
    }
    Ok(())
}

fn write_text_file_atomic(path: &Path, content: &str, overwrite: bool) -> Result<(), String> {
    if path.exists() && !overwrite {
        return Err(format!(
            "{} already exists; pass --overwrite to replace it",
            path.display()
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| format!("{} must have a parent directory", path.display()))?;
    let metadata = std::fs::metadata(parent)
        .map_err(|e| format!("failed to inspect {}: {}", parent.display(), e))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a directory", parent.display()));
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("{} has an invalid file name", path.display()))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));
    let result = (|| {
        use std::io::Write;
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o644);
        let mut file = options
            .open(&temporary)
            .map_err(|e| format!("failed to create {}: {}", temporary.display(), e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("failed to write {}: {}", temporary.display(), e))?;
        file.sync_all()
            .map_err(|e| format!("failed to sync {}: {}", temporary.display(), e))?;
        std::fs::rename(&temporary, path).map_err(|e| {
            format!(
                "failed to atomically replace {} from {}: {}",
                path.display(),
                temporary.display(),
                e
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingUnitKind {
    Absent,
    ManagedRegularFile,
}

fn preflight_unit_path(path: &Path, overwrite: bool) -> Result<ExistingUnitKind, String> {
    if !path.is_absolute() {
        return Err(format!(
            "systemd unit path must be absolute: {}",
            path.display()
        ));
    }
    let existing = match std::fs::symlink_metadata(path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!("failed to inspect {}: {}", path.display(), error));
        }
    };
    if let Some(metadata) = existing.as_ref() {
        if !overwrite {
            return Err(format!(
                "{} already exists; pass --overwrite to replace it",
                path.display()
            ));
        }
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            let masked = std::fs::read_link(path)
                .map(|target| target == Path::new("/dev/null"))
                .unwrap_or(false);
            let kind = if masked {
                "masked systemd unit"
            } else {
                "systemd unit symlink"
            };
            return Err(format!(
                "cannot safely overwrite {kind}: {}; replace or unmask the unit explicitly before retrying",
                path.display()
            ));
        }
        if !file_type.is_file() {
            return Err(format!(
                "cannot safely overwrite non-regular systemd unit: {}; replace it explicitly before retrying",
                path.display()
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let metadata = std::fs::metadata(parent)
        .map_err(|e| format!("failed to inspect {}: {}", parent.display(), e))?;
    if !metadata.is_dir() {
        return Err(format!("{} is not a directory", parent.display()));
    }
    Ok(if existing.is_some() {
        ExistingUnitKind::ManagedRegularFile
    } else {
        ExistingUnitKind::Absent
    })
}

fn restore_unit_file(path: &Path, previous: Option<&str>) -> Result<(), String> {
    match previous {
        Some(content) => write_text_file_atomic(path, content, true),
        None if path.exists() => std::fs::remove_file(path)
            .map_err(|e| format!("failed to remove {}: {}", path.display(), e)),
        None => Ok(()),
    }
}

fn write_server_unit_pair_files<W, R>(
    service_file: &Path,
    service_content: &str,
    service_previous: Option<&str>,
    socket_file: &Path,
    socket_content: &str,
    overwrite: bool,
    mut write: W,
    mut restore: R,
) -> Result<(), String>
where
    W: FnMut(&Path, &str, bool) -> Result<(), String>,
    R: FnMut(&Path, Option<&str>) -> Result<(), String>,
{
    write(service_file, service_content, overwrite)?;
    if let Err(error) = write(socket_file, socket_content, overwrite) {
        let mut rollback_errors = Vec::new();
        if let Err(rollback_error) = restore(service_file, service_previous) {
            rollback_errors.push(format!(
                "service unit file restore failed: {rollback_error}"
            ));
        }
        return Err(install_error_with_rollback(
            "webcodex Server unit pair",
            error,
            rollback_errors,
        ));
    }
    Ok(())
}

fn rollback_unit(path: &Path, previous: Option<&str>) {
    let _ = restore_unit_file(path, previous);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnitDiscovery {
    load_state: String,
    fragment_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InstallSnapshot {
    existing_kind: ExistingUnitKind,
    previous_content: Option<String>,
    active: String,
    enabled: String,
}

fn set_discovery_field(field: &mut Option<String>, key: &str, value: &str) -> Result<(), String> {
    match field {
        Some(previous) if previous != value => {
            Err(format!("conflicting {key} values in systemctl show output"))
        }
        Some(_) => Ok(()),
        None => {
            *field = Some(value.to_string());
            Ok(())
        }
    }
}

fn parse_unit_discovery(output: &str) -> Result<UnitDiscovery, String> {
    const MAX_DISCOVERY_OUTPUT: usize = 4096;
    if output.len() > MAX_DISCOVERY_OUTPUT {
        return Err("systemctl show output exceeded the discovery limit".to_string());
    }
    let mut load_state = None;
    let mut fragment_path = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "LoadState" => set_discovery_field(&mut load_state, key, value)?,
            "FragmentPath" => set_discovery_field(&mut fragment_path, key, value)?,
            _ => {}
        }
    }
    Ok(UnitDiscovery {
        load_state: load_state
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "systemctl show output did not contain LoadState".to_string())?,
        fragment_path: fragment_path
            .ok_or_else(|| "systemctl show output did not contain FragmentPath".to_string())?,
    })
}

fn discover_existing_unit<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    unit: &str,
) -> Result<UnitDiscovery, String> {
    let invocation = systemctl_invocation(
        systemctl,
        "systemctl show",
        vec![
            "show".to_string(),
            unit.to_string(),
            "--property=LoadState".to_string(),
            "--property=FragmentPath".to_string(),
            "--no-pager".to_string(),
        ],
        Some(unit),
    );
    let output = executor.execute(&invocation).map_err(|_| {
        format!("cannot determine whether systemd unit {unit} already exists; no changes were made")
    })?;
    if !output.success {
        return Err(format!(
            "cannot determine whether systemd unit {unit} already exists; no changes were made"
        ));
    }
    parse_unit_discovery(&output.stdout).map_err(|_| {
        format!("cannot determine whether systemd unit {unit} already exists; no changes were made")
    })
}

fn classify_existing_unit(
    unit: &str,
    service_file: &Path,
    target_kind: ExistingUnitKind,
    discovery: &UnitDiscovery,
) -> Result<ExistingUnitKind, String> {
    let fragment_matches_target =
        !discovery.fragment_path.is_empty() && Path::new(&discovery.fragment_path) == service_file;
    match target_kind {
        ExistingUnitKind::ManagedRegularFile => match discovery.load_state.as_str() {
            "loaded" if fragment_matches_target => Ok(ExistingUnitKind::ManagedRegularFile),
            "loaded" => Err(format!(
                "systemd unit {unit} resolves outside {}; refusing to overwrite while FragmentPath differs",
                service_file.display()
            )),
            "not-found" if discovery.fragment_path.is_empty() => {
                Ok(ExistingUnitKind::ManagedRegularFile)
            }
            _ => Err(format!(
                "cannot determine whether systemd unit {unit} can be overwritten safely; no changes were made"
            )),
        },
        ExistingUnitKind::Absent => {
            if discovery.load_state == "not-found" && discovery.fragment_path.is_empty() {
                return Ok(ExistingUnitKind::Absent);
            }
            if !discovery.fragment_path.is_empty() && !fragment_matches_target {
                let mut fragment = discovery.fragment_path.clone();
                if fragment.len() > 240 {
                    fragment.truncate(240);
                    fragment.push_str("...");
                }
                return Err(format!(
                    "systemd unit {unit} already exists outside {}; refusing to create a local override implicitly (FragmentPath={fragment}); use an explicit override option only after reviewing the existing unit",
                    service_file.display()
                ));
            }
            Err(format!(
                "cannot determine whether systemd unit {unit} already exists; no changes were made"
            ))
        }
    }
}

fn query_status_output<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    unit: &str,
    action: &str,
) -> String {
    let invocation = systemctl_invocation(
        systemctl,
        &format!("systemctl {action}"),
        vec![action.to_string(), unit.to_string()],
        Some(unit),
    );
    match executor.execute(&invocation) {
        Ok(output) => {
            let value = output.stdout.trim();
            if value.is_empty() {
                "unknown".to_string()
            } else {
                value.to_string()
            }
        }
        Err(_) => "unknown".to_string(),
    }
}

fn capture_install_snapshot<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    unit: &str,
    existing_kind: ExistingUnitKind,
) -> Result<InstallSnapshot, String> {
    let previous_content = match existing_kind {
        ExistingUnitKind::ManagedRegularFile => Some(
            std::fs::read_to_string(service_file)
                .map_err(|e| format!("failed to read {}: {}", service_file.display(), e))?,
        ),
        ExistingUnitKind::Absent => None,
    };
    let active = query_status_output(executor, systemctl, unit, "is-active");
    if matches!(existing_kind, ExistingUnitKind::ManagedRegularFile)
        && !matches!(active.as_str(), "active" | "inactive" | "failed")
    {
        return Err(format!(
            "cannot safely overwrite {unit} while systemctl is-active reports '{active}'; wait for the unit to leave its transitional or unknown state before retrying"
        ));
    }
    let enabled = query_status_output(executor, systemctl, unit, "is-enabled");
    if matches!(existing_kind, ExistingUnitKind::ManagedRegularFile)
        && !matches!(enabled.as_str(), "enabled" | "disabled")
    {
        return Err(format!(
            "cannot safely overwrite {unit} while systemctl is-enabled reports '{enabled}'; normalize the unit to enabled or disabled before retrying"
        ));
    }
    Ok(InstallSnapshot {
        existing_kind,
        previous_content,
        active,
        enabled,
    })
}

fn rollback_invocation(systemctl: &Path, operation: &str, unit: &str) -> ProcessInvocation {
    systemctl_invocation(
        systemctl,
        &format!("systemctl {operation}"),
        vec![operation.to_string(), unit.to_string()],
        Some(unit),
    )
}

fn push_rollback_error(errors: &mut Vec<String>, label: &str, error: String) {
    let mut summary = format!("{label}: {error}");
    if summary.len() > 180 {
        summary.truncate(180);
        summary.push_str("...");
    }
    errors.push(summary);
}

fn best_effort_execute<E: ProcessExecutor>(
    executor: &mut E,
    invocation: &ProcessInvocation,
    label: &str,
    errors: &mut Vec<String>,
) {
    if let Err(error) = execute_allow_missing(executor, invocation) {
        push_rollback_error(errors, label, error);
    }
}

fn rollback_failed_install<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    unit: &str,
    snapshot: &InstallSnapshot,
) -> Vec<String> {
    let mut errors = Vec::new();
    best_effort_execute(
        executor,
        &rollback_invocation(systemctl, "stop", unit),
        "stop failed",
        &mut errors,
    );

    if matches!(snapshot.existing_kind, ExistingUnitKind::Absent) {
        best_effort_execute(
            executor,
            &rollback_invocation(systemctl, "disable", unit),
            "disable failed",
            &mut errors,
        );
    }

    if let Err(error) = restore_unit_file(service_file, snapshot.previous_content.as_deref()) {
        push_rollback_error(&mut errors, "unit restore failed", error);
    }

    best_effort_execute(
        executor,
        &systemctl_invocation(
            systemctl,
            "systemctl daemon-reload",
            vec!["daemon-reload".to_string()],
            Some(unit),
        ),
        "daemon-reload failed",
        &mut errors,
    );

    if matches!(snapshot.existing_kind, ExistingUnitKind::ManagedRegularFile) {
        match snapshot.enabled.as_str() {
            "enabled" => best_effort_execute(
                executor,
                &rollback_invocation(systemctl, "enable", unit),
                "enabled state restore failed",
                &mut errors,
            ),
            "disabled" => best_effort_execute(
                executor,
                &rollback_invocation(systemctl, "disable", unit),
                "disabled state restore failed",
                &mut errors,
            ),
            _ => {}
        }
        if snapshot.active == "active" {
            best_effort_execute(
                executor,
                &rollback_invocation(systemctl, "start", unit),
                "active state restore failed",
                &mut errors,
            );
        }
    } else {
        best_effort_execute(
            executor,
            &rollback_invocation(systemctl, "reset-failed", unit),
            "reset-failed failed",
            &mut errors,
        );
    }
    errors
}

fn install_error_with_rollback(unit: &str, error: String, rollback_errors: Vec<String>) -> String {
    let mut message = format!("installation failed for {unit}: {error}");
    if !rollback_errors.is_empty() {
        let mut summary = rollback_errors.join("; ");
        if summary.len() > 600 {
            summary.truncate(600);
            summary.push_str("...");
        }
        message.push_str("; rollback also encountered: ");
        message.push_str(&summary);
    }
    message
}

fn uninstall_error_with_rollback(error: String, rollback_errors: Vec<String>) -> String {
    let mut message = format!("uninstallation failed for webcodex Server unit pair: {error}");
    if !rollback_errors.is_empty() {
        let mut summary = rollback_errors.join("; ");
        if summary.len() > 600 {
            summary.truncate(600);
            summary.push_str("...");
        }
        message.push_str("; rollback also encountered: ");
        message.push_str(&summary);
    }
    message
}

pub(crate) fn install_unit_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    unit: &str,
    content: &str,
    overwrite: bool,
    no_start: bool,
) -> Result<InstallUnitResult, String> {
    let target_kind = preflight_unit_path(service_file, overwrite)?;
    let discovery = discover_existing_unit(executor, systemctl, unit)?;
    let existing_kind = classify_existing_unit(unit, service_file, target_kind, &discovery)?;
    let snapshot =
        capture_install_snapshot(executor, systemctl, service_file, unit, existing_kind)?;
    write_text_file_atomic(service_file, content, overwrite)?;
    if let Err(error) = execute_plan(executor, &plan_install(systemctl, unit, no_start)) {
        let rollback_errors =
            rollback_failed_install(executor, systemctl, service_file, unit, &snapshot);
        return Err(install_error_with_rollback(unit, error, rollback_errors));
    }
    Ok(InstallUnitResult {
        unit: unit.to_string(),
        started: !no_start,
    })
}

pub(crate) fn install_unit_with_executor_for_scope<E: ProcessExecutor>(
    scope: ServiceScope,
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    unit: &str,
    content: &str,
    overwrite: bool,
    no_start: bool,
) -> Result<InstallUnitResult, String> {
    let mut executor = ScopedProcessExecutor {
        inner: executor,
        scope,
    };
    install_unit_with_executor(
        &mut executor,
        systemctl,
        service_file,
        unit,
        content,
        overwrite,
        no_start,
    )
}

fn pair_rollback<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    service_unit: &str,
    service_snapshot: &InstallSnapshot,
    socket_file: &Path,
    socket_unit: &str,
    socket_snapshot: &InstallSnapshot,
) -> Vec<String> {
    let mut errors = Vec::new();
    for (unit, label) in [
        (socket_unit, "socket stop failed"),
        (service_unit, "service stop failed"),
    ] {
        best_effort_execute(
            executor,
            &rollback_invocation(systemctl, "stop", unit),
            label,
            &mut errors,
        );
    }
    for (unit, snapshot, label) in [
        (socket_unit, socket_snapshot, "socket disable failed"),
        (service_unit, service_snapshot, "service disable failed"),
    ] {
        if matches!(snapshot.existing_kind, ExistingUnitKind::Absent) {
            best_effort_execute(
                executor,
                &rollback_invocation(systemctl, "disable", unit),
                label,
                &mut errors,
            );
        }
    }
    for (path, snapshot, label) in [
        (
            service_file,
            service_snapshot,
            "service unit restore failed",
        ),
        (socket_file, socket_snapshot, "socket unit restore failed"),
    ] {
        if let Err(error) = restore_unit_file(path, snapshot.previous_content.as_deref()) {
            push_rollback_error(&mut errors, label, error);
        }
    }
    best_effort_execute(
        executor,
        &systemctl_invocation(
            systemctl,
            "systemctl daemon-reload",
            vec!["daemon-reload".to_string()],
            Some(service_unit),
        ),
        "daemon-reload failed",
        &mut errors,
    );
    for (unit, snapshot, prefix) in [
        (socket_unit, socket_snapshot, "socket"),
        (service_unit, service_snapshot, "service"),
    ] {
        if matches!(snapshot.existing_kind, ExistingUnitKind::ManagedRegularFile) {
            let (operation, label) = match snapshot.enabled.as_str() {
                "enabled" => (
                    Some("enable"),
                    format!("{prefix} enabled state restore failed"),
                ),
                "disabled" => (
                    Some("disable"),
                    format!("{prefix} disabled state restore failed"),
                ),
                _ => (None, String::new()),
            };
            if let Some(operation) = operation {
                best_effort_execute(
                    executor,
                    &rollback_invocation(systemctl, operation, unit),
                    &label,
                    &mut errors,
                );
            }
        }
    }
    if socket_snapshot.active == "active" {
        best_effort_execute(
            executor,
            &rollback_invocation(systemctl, "start", socket_unit),
            "socket active state restore failed",
            &mut errors,
        );
    }
    if service_snapshot.active == "active" {
        best_effort_execute(
            executor,
            &rollback_invocation(systemctl, "start", service_unit),
            "service active state restore failed",
            &mut errors,
        );
    }
    errors
}

pub(crate) fn install_server_unit_pair_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    service_unit: &str,
    service_content: &str,
    socket_file: &Path,
    socket_unit: &str,
    socket_content: &str,
    overwrite: bool,
    no_start: bool,
) -> Result<InstallUnitResult, String> {
    let service_target = preflight_unit_path(service_file, overwrite)?;
    let socket_target = preflight_unit_path(socket_file, overwrite)?;
    let service_discovery = discover_existing_unit(executor, systemctl, service_unit)?;
    let socket_discovery = discover_existing_unit(executor, systemctl, socket_unit)?;
    let service_kind = classify_existing_unit(
        service_unit,
        service_file,
        service_target,
        &service_discovery,
    )?;
    let socket_kind =
        classify_existing_unit(socket_unit, socket_file, socket_target, &socket_discovery)?;
    let service_snapshot = capture_install_snapshot(
        executor,
        systemctl,
        service_file,
        service_unit,
        service_kind,
    )?;
    let socket_snapshot =
        capture_install_snapshot(executor, systemctl, socket_file, socket_unit, socket_kind)?;

    if service_snapshot.active == "active"
        && matches!(socket_snapshot.existing_kind, ExistingUnitKind::Absent)
    {
        return Err(format!(
            "cannot migrate an active legacy {service_unit} directly to socket activation: stop the legacy Server first, then rerun `webcodex server install --overwrite`; this one-time migration boundary is intentionally fail-closed"
        ));
    }

    write_server_unit_pair_files(
        service_file,
        service_content,
        service_snapshot.previous_content.as_deref(),
        socket_file,
        socket_content,
        overwrite,
        write_text_file_atomic,
        restore_unit_file,
    )?;

    let mut plan = vec![systemctl_invocation(
        systemctl,
        "systemctl daemon-reload",
        vec!["daemon-reload".to_string()],
        Some(service_unit),
    )];
    for unit in [socket_unit, service_unit] {
        plan.push(rollback_invocation(systemctl, "enable", unit));
    }
    if no_start {
        for unit in [socket_unit, service_unit] {
            plan.push(systemctl_invocation(
                systemctl,
                "systemctl is-enabled",
                vec![
                    "is-enabled".to_string(),
                    "--quiet".to_string(),
                    unit.to_string(),
                ],
                Some(unit),
            ));
        }
    } else {
        for unit in [socket_unit, service_unit] {
            plan.push(rollback_invocation(systemctl, "start", unit));
            plan.push(systemctl_invocation(
                systemctl,
                "systemctl is-active",
                vec![
                    "is-active".to_string(),
                    "--quiet".to_string(),
                    unit.to_string(),
                ],
                Some(unit),
            ));
        }
    }
    if let Err(error) = execute_plan(executor, &plan) {
        let rollback_errors = pair_rollback(
            executor,
            systemctl,
            service_file,
            service_unit,
            &service_snapshot,
            socket_file,
            socket_unit,
            &socket_snapshot,
        );
        return Err(install_error_with_rollback(
            "webcodex Server unit pair",
            error,
            rollback_errors,
        ));
    }
    Ok(InstallUnitResult {
        unit: service_unit.to_string(),
        started: !no_start,
    })
}

pub(crate) fn install_server_unit_pair(
    service_file: &Path,
    service_unit: &str,
    service_content: &str,
    socket_file: &Path,
    socket_unit: &str,
    socket_content: &str,
    overwrite: bool,
    no_start: bool,
) -> Result<InstallUnitResult, String> {
    preflight_unit_path(service_file, overwrite)?;
    preflight_unit_path(socket_file, overwrite)?;
    let systemctl = systemctl_path()?;
    let mut executor = RealProcessExecutor;
    install_server_unit_pair_with_executor(
        &mut executor,
        &systemctl,
        service_file,
        service_unit,
        service_content,
        socket_file,
        socket_unit,
        socket_content,
        overwrite,
        no_start,
    )
}

pub(crate) fn control_server_unit_pair_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_unit: &str,
    socket_unit: &str,
    control: ServiceControl,
) -> Result<(), String> {
    if control == ServiceControl::Stop {
        execute_allow_missing(
            executor,
            &rollback_invocation(systemctl, "stop", socket_unit),
        )?;
        execute_required(
            executor,
            &rollback_invocation(systemctl, "stop", service_unit),
        )?;
        return Ok(());
    }
    let plan = match control {
        ServiceControl::Start => vec![
            rollback_invocation(systemctl, "start", socket_unit),
            systemctl_invocation(
                systemctl,
                "systemctl is-active",
                vec![
                    "is-active".to_string(),
                    "--quiet".to_string(),
                    socket_unit.to_string(),
                ],
                Some(socket_unit),
            ),
            rollback_invocation(systemctl, "start", service_unit),
            systemctl_invocation(
                systemctl,
                "systemctl is-active",
                vec![
                    "is-active".to_string(),
                    "--quiet".to_string(),
                    service_unit.to_string(),
                ],
                Some(service_unit),
            ),
        ],
        ServiceControl::Restart => plan_control(systemctl, service_unit, ServiceControl::Restart),
        ServiceControl::Stop => unreachable!("stop is handled above"),
    };
    execute_plan(executor, &plan)
}

pub(crate) fn control_server_unit_pair(
    service_unit: &str,
    socket_unit: &str,
    control: ServiceControl,
) -> Result<(), String> {
    let systemctl = systemctl_path()?;
    let mut executor = RealProcessExecutor;
    control_server_unit_pair_with_executor(
        &mut executor,
        &systemctl,
        service_unit,
        socket_unit,
        control,
    )
}

fn read_managed_unit_for_uninstall(path: &Path) -> Result<Option<String>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("failed to inspect {}: {error}", path.display()));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(format!(
            "refusing to uninstall non-regular or symlinked systemd unit: {}",
            path.display()
        ));
    }
    std::fs::read_to_string(path)
        .map(Some)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn capture_uninstall_snapshot<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    unit: &str,
    previous_content: Option<String>,
) -> Result<InstallSnapshot, String> {
    let existing_kind = if previous_content.is_some() {
        ExistingUnitKind::ManagedRegularFile
    } else {
        ExistingUnitKind::Absent
    };
    let active = query_status_output(executor, systemctl, unit, "is-active");
    let enabled = query_status_output(executor, systemctl, unit, "is-enabled");
    if matches!(existing_kind, ExistingUnitKind::ManagedRegularFile) {
        if !matches!(active.as_str(), "active" | "inactive" | "failed") {
            return Err(format!(
                "cannot safely uninstall {unit} while systemctl is-active reports '{active}'; wait for the unit to leave its transitional or unknown state before retrying"
            ));
        }
        if !matches!(enabled.as_str(), "enabled" | "disabled") {
            return Err(format!(
                "cannot safely uninstall {unit} while systemctl is-enabled reports '{enabled}'; normalize the unit to enabled or disabled before retrying"
            ));
        }
    }
    Ok(InstallSnapshot {
        existing_kind,
        previous_content,
        active,
        enabled,
    })
}

pub(crate) fn uninstall_server_unit_pair_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    service_unit: &str,
    socket_file: &Path,
    socket_unit: &str,
) -> Result<UninstallUnitResult, String> {
    let service_previous = read_managed_unit_for_uninstall(service_file)?;
    let socket_previous = read_managed_unit_for_uninstall(socket_file)?;
    if service_previous.is_none() && socket_previous.is_none() {
        return Ok(UninstallUnitResult {
            unit: service_unit.to_string(),
            removed: false,
        });
    }
    let service_snapshot =
        capture_uninstall_snapshot(executor, systemctl, service_unit, service_previous)?;
    let socket_snapshot =
        capture_uninstall_snapshot(executor, systemctl, socket_unit, socket_previous)?;

    let reload = systemctl_invocation(
        systemctl,
        "systemctl daemon-reload",
        vec!["daemon-reload".to_string()],
        Some(service_unit),
    );
    let uninstall = (|| -> Result<(), String> {
        for unit in [socket_unit, service_unit] {
            execute_allow_missing(executor, &rollback_invocation(systemctl, "stop", unit))?;
            execute_allow_missing(executor, &rollback_invocation(systemctl, "disable", unit))?;
        }
        if service_snapshot.previous_content.is_some() {
            std::fs::remove_file(service_file)
                .map_err(|error| format!("failed to remove {}: {error}", service_file.display()))?;
        }
        if socket_snapshot.previous_content.is_some() {
            std::fs::remove_file(socket_file)
                .map_err(|error| format!("failed to remove {}: {error}", socket_file.display()))?;
        }
        execute_required(executor, &reload)?;
        Ok(())
    })();
    if let Err(error) = uninstall {
        let rollback_errors = pair_rollback(
            executor,
            systemctl,
            service_file,
            service_unit,
            &service_snapshot,
            socket_file,
            socket_unit,
            &socket_snapshot,
        );
        return Err(uninstall_error_with_rollback(error, rollback_errors));
    }
    for unit in [socket_unit, service_unit] {
        let _ = execute_allow_missing(
            executor,
            &rollback_invocation(systemctl, "reset-failed", unit),
        );
    }
    Ok(UninstallUnitResult {
        unit: service_unit.to_string(),
        removed: true,
    })
}

pub(crate) fn uninstall_server_unit_pair(
    service_file: &Path,
    service_unit: &str,
    socket_file: &Path,
    socket_unit: &str,
) -> Result<UninstallUnitResult, String> {
    let systemctl = systemctl_path()?;
    let mut executor = RealProcessExecutor;
    uninstall_server_unit_pair_with_executor(
        &mut executor,
        &systemctl,
        service_file,
        service_unit,
        socket_file,
        socket_unit,
    )
}

pub(crate) fn control_service_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    unit: &str,
    control: ServiceControl,
) -> Result<(), String> {
    execute_plan(executor, &plan_control(systemctl, unit, control))
}

pub(crate) fn control_service_with_executor_for_scope<E: ProcessExecutor>(
    scope: ServiceScope,
    executor: &mut E,
    systemctl: &Path,
    unit: &str,
    control: ServiceControl,
) -> Result<(), String> {
    let mut executor = ScopedProcessExecutor {
        inner: executor,
        scope,
    };
    control_service_with_executor(&mut executor, systemctl, unit, control)
}

fn missing_unit_failure(output: &ProcessOutput) -> bool {
    let detail = format!("{}\n{}", output.stdout, output.stderr).to_ascii_lowercase();
    [
        "not loaded",
        "not found",
        "does not exist",
        "could not be found",
    ]
    .iter()
    .any(|needle| detail.contains(needle))
}

fn execute_allow_missing<E: ProcessExecutor>(
    executor: &mut E,
    invocation: &ProcessInvocation,
) -> Result<(), String> {
    let output = executor.execute(invocation)?;
    if output.success || missing_unit_failure(&output) {
        return Ok(());
    }
    let unit = invocation.unit.as_deref().unwrap_or("systemd manager");
    Err(format!(
        "{} failed for {}: {}",
        invocation.operation,
        unit,
        failure_detail(&output)
    ))
}

pub(crate) fn uninstall_unit_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    unit: &str,
) -> Result<UninstallUnitResult, String> {
    if !service_file.exists() {
        return Ok(UninstallUnitResult {
            unit: unit.to_string(),
            removed: false,
        });
    }
    let previous = std::fs::read_to_string(service_file)
        .map_err(|e| format!("failed to read {}: {}", service_file.display(), e))?;
    for invocation in plan_uninstall_before_remove(systemctl, unit) {
        execute_allow_missing(executor, &invocation)?;
    }
    std::fs::remove_file(service_file)
        .map_err(|e| format!("failed to remove {}: {}", service_file.display(), e))?;
    let after = plan_uninstall_after_remove(systemctl, unit);
    if let Err(error) = execute_required(executor, &after[0]) {
        rollback_unit(service_file, Some(&previous));
        return Err(error);
    }
    let _ = execute_allow_missing(executor, &after[1]);
    Ok(UninstallUnitResult {
        unit: unit.to_string(),
        removed: true,
    })
}

pub(crate) fn uninstall_unit_with_executor_for_scope<E: ProcessExecutor>(
    scope: ServiceScope,
    executor: &mut E,
    systemctl: &Path,
    service_file: &Path,
    unit: &str,
) -> Result<UninstallUnitResult, String> {
    let mut executor = ScopedProcessExecutor {
        inner: executor,
        scope,
    };
    uninstall_unit_with_executor(&mut executor, systemctl, service_file, unit)
}

pub(crate) fn run_logs_with_executor<E: ProcessExecutor>(
    executor: &mut E,
    journalctl: &Path,
    unit: &str,
    lines: u32,
    since: Option<&str>,
    follow: bool,
) -> Result<String, String> {
    let invocation = journalctl_invocation(journalctl, unit, lines, since, follow);
    let output = execute_required(executor, &invocation)?;
    Ok(output.stdout)
}

pub(crate) fn run_logs_with_executor_for_scope<E: ProcessExecutor>(
    scope: ServiceScope,
    executor: &mut E,
    journalctl: &Path,
    unit: &str,
    lines: u32,
    since: Option<&str>,
    follow: bool,
) -> Result<String, String> {
    let mut executor = ScopedProcessExecutor {
        inner: executor,
        scope,
    };
    run_logs_with_executor(&mut executor, journalctl, unit, lines, since, follow)
}

pub(crate) fn install_unit_for_scope(
    scope: ServiceScope,
    service_file: &Path,
    unit: &str,
    content: &str,
    overwrite: bool,
    no_start: bool,
) -> Result<InstallUnitResult, String> {
    preflight_unit_path(service_file, overwrite)?;
    let systemctl = systemctl_path()?;
    let mut executor = RealProcessExecutor;
    install_unit_with_executor_for_scope(
        scope,
        &mut executor,
        &systemctl,
        service_file,
        unit,
        content,
        overwrite,
        no_start,
    )
}

pub(crate) fn control_service_for_scope(
    scope: ServiceScope,
    unit: &str,
    control: ServiceControl,
) -> Result<(), String> {
    let systemctl = systemctl_path()?;
    let mut executor = RealProcessExecutor;
    control_service_with_executor_for_scope(scope, &mut executor, &systemctl, unit, control)
}

pub(crate) fn uninstall_unit_for_scope(
    scope: ServiceScope,
    service_file: &Path,
    unit: &str,
) -> Result<UninstallUnitResult, String> {
    let systemctl = systemctl_path()?;
    let mut executor = RealProcessExecutor;
    uninstall_unit_with_executor_for_scope(scope, &mut executor, &systemctl, service_file, unit)
}

pub(crate) fn run_logs(
    unit: &str,
    lines: u32,
    since: Option<&str>,
    follow: bool,
) -> Result<String, String> {
    let journalctl = journalctl_path()?;
    let mut executor = RealProcessExecutor;
    run_logs_with_executor(&mut executor, &journalctl, unit, lines, since, follow)
}

pub(crate) fn run_logs_for_scope(
    scope: ServiceScope,
    unit: &str,
    lines: u32,
    since: Option<&str>,
    follow: bool,
) -> Result<String, String> {
    let journalctl = journalctl_path()?;
    let mut executor = RealProcessExecutor;
    run_logs_with_executor_for_scope(
        scope,
        &mut executor,
        &journalctl,
        unit,
        lines,
        since,
        follow,
    )
}

pub(crate) fn run_internal_binary(path: &Path, args: &[String]) -> Result<i32, String> {
    let mut command = Command::new(path);
    command.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = command.exec();
        Err(format!("failed to execute {}: {}", path.display(), error))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|e| format!("failed to execute {}: {}", path.display(), e))?;
        Ok(status.code().unwrap_or(1))
    }
}

fn query_load_state_output<E: ProcessExecutor>(
    executor: &mut E,
    systemctl: &Path,
    unit: &str,
) -> String {
    let invocation = systemctl_invocation(
        systemctl,
        "systemctl show LoadState",
        vec![
            "show".to_string(),
            unit.to_string(),
            "--property=LoadState".to_string(),
            "--value".to_string(),
            "--no-pager".to_string(),
        ],
        Some(unit),
    );
    match executor.execute(&invocation) {
        Ok(output) if output.success => {
            let value = output.stdout.trim();
            if value.is_empty() {
                "unknown".to_string()
            } else {
                value.to_string()
            }
        }
        _ => "unknown".to_string(),
    }
}

pub(crate) fn query_systemd_service_status(service_name: &str) -> SystemdStatus {
    let Ok(systemctl) = systemctl_path() else {
        return SystemdStatus {
            loaded: "unknown".to_string(),
            active: "unknown".to_string(),
            enabled: "unknown".to_string(),
        };
    };
    let mut executor = RealProcessExecutor;
    SystemdStatus {
        loaded: query_load_state_output(&mut executor, &systemctl, service_name),
        active: query_status_output(&mut executor, &systemctl, service_name, "is-active"),
        enabled: query_status_output(&mut executor, &systemctl, service_name, "is-enabled"),
    }
}

pub(crate) fn query_systemd_service_status_for_scope(
    scope: ServiceScope,
    service_name: &str,
) -> SystemdStatus {
    let Ok(systemctl) = systemctl_path() else {
        return SystemdStatus {
            loaded: "unknown".to_string(),
            active: "unknown".to_string(),
            enabled: "unknown".to_string(),
        };
    };
    let mut executor = RealProcessExecutor;
    let mut executor = ScopedProcessExecutor {
        inner: &mut executor,
        scope,
    };
    SystemdStatus {
        loaded: query_load_state_output(&mut executor, &systemctl, service_name),
        active: query_status_output(&mut executor, &systemctl, service_name, "is-active"),
        enabled: query_status_output(&mut executor, &systemctl, service_name, "is-enabled"),
    }
}

pub(crate) fn query_systemd_socket_status(socket_unit: &str) -> SystemdStatus {
    query_systemd_service_status(socket_unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_plan_has_stable_order_and_no_start_semantics() {
        let systemctl = Path::new("/usr/bin/systemctl");
        let start = plan_install(systemctl, SERVER_SERVICE_UNIT, false);
        assert_eq!(start.len(), 3);
        assert_eq!(start[0].args, ["daemon-reload"]);
        assert_eq!(start[1].args, ["enable", "--now", SERVER_SERVICE_UNIT]);
        assert_eq!(start[2].args, ["is-active", "--quiet", SERVER_SERVICE_UNIT]);

        let no_start = plan_install(systemctl, AGENT_SERVICE_UNIT, true);
        assert_eq!(no_start[1].args, ["enable", AGENT_SERVICE_UNIT]);
        assert_eq!(
            no_start[2].args,
            ["is-enabled", "--quiet", AGENT_SERVICE_UNIT]
        );
    }

    #[test]
    fn server_pair_control_keeps_restart_socket_out_of_target_and_stop_prevents_activation() {
        let systemctl = Path::new("/usr/bin/systemctl");

        let mut restart = FakeExecutor::with_outputs(vec![ok(), ok()]);
        control_server_unit_pair_with_executor(
            &mut restart,
            systemctl,
            SERVER_SERVICE_UNIT,
            SERVER_SOCKET_UNIT,
            ServiceControl::Restart,
        )
        .unwrap();
        assert_eq!(restart.calls[0], ["restart", SERVER_SERVICE_UNIT]);
        assert_eq!(
            restart.calls[1],
            ["is-active", "--quiet", SERVER_SERVICE_UNIT]
        );
        assert!(restart
            .calls
            .iter()
            .all(|args| !args.contains(&SERVER_SOCKET_UNIT.to_string())));

        let mut stop = FakeExecutor::with_outputs(vec![ok(), ok()]);
        control_server_unit_pair_with_executor(
            &mut stop,
            systemctl,
            SERVER_SERVICE_UNIT,
            SERVER_SOCKET_UNIT,
            ServiceControl::Stop,
        )
        .unwrap();
        assert_eq!(
            stop.calls,
            [["stop", SERVER_SOCKET_UNIT], ["stop", SERVER_SERVICE_UNIT]]
        );

        let mut legacy_stop =
            FakeExecutor::with_outputs(vec![failed("Unit webcodex.socket not found."), ok()]);
        control_server_unit_pair_with_executor(
            &mut legacy_stop,
            systemctl,
            SERVER_SERVICE_UNIT,
            SERVER_SOCKET_UNIT,
            ServiceControl::Stop,
        )
        .unwrap();
        assert_eq!(
            legacy_stop.calls,
            [["stop", SERVER_SOCKET_UNIT], ["stop", SERVER_SERVICE_UNIT]]
        );

        let mut start = FakeExecutor::with_outputs(vec![ok(), ok(), ok(), ok()]);
        control_server_unit_pair_with_executor(
            &mut start,
            systemctl,
            SERVER_SERVICE_UNIT,
            SERVER_SOCKET_UNIT,
            ServiceControl::Start,
        )
        .unwrap();
        assert_eq!(start.calls[0], ["start", SERVER_SOCKET_UNIT]);
        assert_eq!(start.calls[1], ["is-active", "--quiet", SERVER_SOCKET_UNIT]);
        assert_eq!(start.calls[2], ["start", SERVER_SERVICE_UNIT]);
        assert_eq!(
            start.calls[3],
            ["is-active", "--quiet", SERVER_SERVICE_UNIT]
        );
    }

    #[test]
    fn server_pair_second_file_write_failure_restores_first_file() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&service_file, "old service").unwrap();
        std::fs::write(&socket_file, "old socket").unwrap();

        let error = write_server_unit_pair_files(
            &service_file,
            "new service",
            Some("old service"),
            &socket_file,
            "new socket",
            true,
            |path, content, overwrite| {
                if path == socket_file.as_path() {
                    Err("socket write rejected".to_string())
                } else {
                    write_text_file_atomic(path, content, overwrite)
                }
            },
            restore_unit_file,
        )
        .unwrap_err();

        assert!(error.contains("socket write rejected"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&service_file).unwrap(),
            "old service"
        );
        assert_eq!(std::fs::read_to_string(&socket_file).unwrap(), "old socket");
    }

    #[test]
    fn server_pair_second_file_write_failure_reports_restore_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&service_file, "old service").unwrap();
        std::fs::write(&socket_file, "old socket").unwrap();

        let error = write_server_unit_pair_files(
            &service_file,
            "new service",
            Some("old service"),
            &socket_file,
            "new socket",
            true,
            |path, content, overwrite| {
                if path == socket_file.as_path() {
                    Err("socket write rejected".to_string())
                } else {
                    write_text_file_atomic(path, content, overwrite)
                }
            },
            |_path, _previous| Err("restore rejected".to_string()),
        )
        .unwrap_err();

        assert!(error.contains("socket write rejected"), "{error}");
        assert!(error.contains("rollback also encountered"), "{error}");
        assert!(
            error.contains("service unit file restore failed: restore rejected"),
            "{error}"
        );
        assert_eq!(
            std::fs::read_to_string(&service_file).unwrap(),
            "new service"
        );
        assert_eq!(std::fs::read_to_string(&socket_file).unwrap(), "old socket");
    }

    #[test]
    fn server_pair_no_start_enables_both_without_starting() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            status("inactive"),
            status("disabled"),
            ok(),
            ok(),
            ok(),
            status("enabled"),
            status("enabled"),
        ]);
        let result = install_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "service unit",
            &socket_file,
            SERVER_SOCKET_UNIT,
            "socket unit",
            false,
            true,
        )
        .unwrap();
        assert!(!result.started);
        assert_eq!(
            std::fs::read_to_string(&service_file).unwrap(),
            "service unit"
        );
        assert_eq!(
            std::fs::read_to_string(&socket_file).unwrap(),
            "socket unit"
        );
        assert!(executor
            .calls
            .iter()
            .any(|args| args == &["enable", SERVER_SOCKET_UNIT]));
        assert!(executor
            .calls
            .iter()
            .any(|args| args == &["enable", SERVER_SERVICE_UNIT]));
        assert!(!executor
            .calls
            .iter()
            .any(|args| args.first().map(String::as_str) == Some("start")));
    }

    #[test]
    fn server_pair_install_failure_rolls_back_both_unit_files() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            status("inactive"),
            status("disabled"),
            ok(),
            ok(),
            failed("service enable rejected"),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);
        let error = install_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "service unit",
            &socket_file,
            SERVER_SOCKET_UNIT,
            "socket unit",
            false,
            true,
        )
        .unwrap_err();
        assert!(error.contains("service enable rejected"), "{error}");
        assert!(!service_file.exists());
        assert!(!socket_file.exists());
        assert_eq!(executor.calls[9], ["stop", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[10], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[11], ["disable", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[12], ["disable", SERVER_SERVICE_UNIT]);
    }

    #[test]
    fn active_legacy_server_migration_fails_before_pair_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&service_file, "legacy unit").unwrap();
        let mut executor = FakeExecutor::with_outputs(vec![
            discovery("loaded", service_file.to_str().unwrap()),
            absent_discovery(),
            status("active"),
            status("enabled"),
            status("inactive"),
            status("disabled"),
        ]);
        let error = install_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new service",
            &socket_file,
            SERVER_SOCKET_UNIT,
            "new socket",
            true,
            false,
        )
        .unwrap_err();
        assert!(error.contains("active legacy"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&service_file).unwrap(),
            "legacy unit"
        );
        assert!(!socket_file.exists());
        assert_eq!(executor.calls.len(), 6);
    }

    #[test]
    fn server_pair_uninstall_stops_socket_before_service_and_removes_both() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&service_file, "service").unwrap();
        std::fs::write(&socket_file, "socket").unwrap();
        let mut executor = FakeExecutor::with_outputs(vec![
            status("active"),
            status("enabled"),
            status("active"),
            status("enabled"),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);
        let result = uninstall_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            &socket_file,
            SERVER_SOCKET_UNIT,
        )
        .unwrap();
        assert!(result.removed);
        assert_eq!(executor.calls[4], ["stop", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[5], ["disable", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[6], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[7], ["disable", SERVER_SERVICE_UNIT]);
        assert!(!service_file.exists());
        assert!(!socket_file.exists());
    }

    #[test]
    fn server_pair_uninstall_control_failure_restores_prior_pair_state() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&service_file, "service").unwrap();
        std::fs::write(&socket_file, "socket").unwrap();
        let mut executor = FakeExecutor::with_outputs(vec![
            status("active"),
            status("enabled"),
            status("active"),
            status("enabled"),
            ok(),
            ok(),
            ok(),
            failed("service disable rejected"),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);

        let error = uninstall_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            &socket_file,
            SERVER_SOCKET_UNIT,
        )
        .unwrap_err();

        assert!(error.contains("service disable rejected"), "{error}");
        assert_eq!(std::fs::read_to_string(&service_file).unwrap(), "service");
        assert_eq!(std::fs::read_to_string(&socket_file).unwrap(), "socket");
        assert_eq!(executor.calls[8], ["stop", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[9], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[10], ["daemon-reload"]);
        assert_eq!(executor.calls[11], ["enable", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[12], ["enable", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[13], ["start", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[14], ["start", SERVER_SERVICE_UNIT]);
    }

    #[test]
    fn server_pair_uninstall_reload_failure_restores_files_and_prior_pair_state() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&service_file, "service").unwrap();
        std::fs::write(&socket_file, "socket").unwrap();
        let mut executor = FakeExecutor::with_outputs(vec![
            status("active"),
            status("enabled"),
            status("active"),
            status("enabled"),
            ok(),
            ok(),
            ok(),
            ok(),
            failed("daemon reload rejected"),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);

        let error = uninstall_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            &socket_file,
            SERVER_SOCKET_UNIT,
        )
        .unwrap_err();

        assert!(error.contains("daemon reload rejected"), "{error}");
        assert_eq!(std::fs::read_to_string(&service_file).unwrap(), "service");
        assert_eq!(std::fs::read_to_string(&socket_file).unwrap(), "socket");
        assert_eq!(executor.calls[9], ["stop", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[10], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[11], ["daemon-reload"]);
        assert_eq!(executor.calls[12], ["enable", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[13], ["enable", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[14], ["start", SERVER_SOCKET_UNIT]);
        assert_eq!(executor.calls[15], ["start", SERVER_SERVICE_UNIT]);
    }

    #[cfg(unix)]
    #[test]
    fn server_pair_uninstall_rejects_symlink_before_systemctl() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real.service");
        let service_file = tmp.path().join("webcodex.service");
        let socket_file = tmp.path().join("webcodex.socket");
        std::fs::write(&target, "service").unwrap();
        symlink(&target, &service_file).unwrap();
        std::fs::write(&socket_file, "socket").unwrap();
        let mut executor = FakeExecutor::default();
        let error = uninstall_server_unit_pair_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            &socket_file,
            SERVER_SOCKET_UNIT,
        )
        .unwrap_err();
        assert!(error.contains("symlinked systemd unit"), "{error}");
        assert!(executor.calls.is_empty());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "service");
        assert!(socket_file.exists());
    }

    #[test]
    fn user_scope_covers_install_status_control_logs_and_uninstall_manager_args() {
        let systemctl = Path::new("/usr/bin/systemctl");
        let journalctl = Path::new("/usr/bin/journalctl");
        let unit = "webcodex-runner-work.service";

        for invocation in plan_install_for_scope(ServiceScope::User, systemctl, unit, false) {
            assert_eq!(invocation.args.first().map(String::as_str), Some("--user"));
        }
        for control in [
            ServiceControl::Start,
            ServiceControl::Stop,
            ServiceControl::Restart,
        ] {
            for invocation in plan_control_for_scope(ServiceScope::User, systemctl, unit, control) {
                assert_eq!(invocation.args.first().map(String::as_str), Some("--user"));
            }
        }
        for invocation in
            plan_uninstall_before_remove_for_scope(ServiceScope::User, systemctl, unit)
                .into_iter()
                .chain(plan_uninstall_after_remove_for_scope(
                    ServiceScope::User,
                    systemctl,
                    unit,
                ))
        {
            assert_eq!(invocation.args.first().map(String::as_str), Some("--user"));
        }
        let logs =
            journalctl_invocation_for_scope(ServiceScope::User, journalctl, unit, 50, None, false);
        assert_eq!(logs.args.first().map(String::as_str), Some("--user"));

        let mut executor = FakeExecutor::with_outputs(vec![status("active")]);
        let mut scoped = ScopedProcessExecutor {
            inner: &mut executor,
            scope: ServiceScope::User,
        };
        assert_eq!(
            query_status_output(&mut scoped, systemctl, unit, "is-active"),
            "active"
        );
        assert_eq!(executor.calls, [["--user", "is-active", unit]]);
    }

    #[test]
    fn system_scope_never_adds_user_manager_flags() {
        let systemctl = Path::new("/usr/bin/systemctl");
        let unit = "webcodex-runner.service";
        for invocation in plan_install_for_scope(ServiceScope::System, systemctl, unit, false)
            .into_iter()
            .chain(plan_control_for_scope(
                ServiceScope::System,
                systemctl,
                unit,
                ServiceControl::Restart,
            ))
            .chain(plan_uninstall_before_remove_for_scope(
                ServiceScope::System,
                systemctl,
                unit,
            ))
            .chain(plan_uninstall_after_remove_for_scope(
                ServiceScope::System,
                systemctl,
                unit,
            ))
        {
            assert_ne!(invocation.args.first().map(String::as_str), Some("--user"));
        }
        let logs = journalctl_invocation_for_scope(
            ServiceScope::System,
            Path::new("/usr/bin/journalctl"),
            unit,
            50,
            None,
            false,
        );
        assert_ne!(logs.args.first().map(String::as_str), Some("--user"));
    }

    #[test]
    fn lifecycle_and_logs_are_argv_plans_without_shell_strings() {
        let systemctl = Path::new("/usr/bin/systemctl");
        let restart = plan_control(systemctl, SERVER_SERVICE_UNIT, ServiceControl::Restart);
        assert_eq!(restart[0].program, systemctl);
        assert_eq!(restart[0].args, ["restart", SERVER_SERVICE_UNIT]);
        assert_eq!(
            restart[1].args,
            ["is-active", "--quiet", SERVER_SERVICE_UNIT]
        );

        let logs = journalctl_invocation(
            Path::new("/usr/bin/journalctl"),
            "webcodex-runner-work.service",
            75,
            Some("yesterday 12:00"),
            true,
        );
        assert_eq!(
            logs.args,
            [
                "--unit",
                "webcodex-runner-work.service",
                "--lines",
                "75",
                "--no-pager",
                "--since",
                "yesterday 12:00",
                "--follow"
            ]
        );
        assert!(logs.inherit_stdio);
    }

    #[derive(Default)]
    struct FakeExecutor {
        outputs: std::collections::VecDeque<Result<ProcessOutput, String>>,
        calls: Vec<Vec<String>>,
    }

    impl FakeExecutor {
        fn with_outputs(outputs: Vec<Result<ProcessOutput, String>>) -> Self {
            Self {
                outputs: outputs.into(),
                calls: Vec::new(),
            }
        }
    }

    impl ProcessExecutor for FakeExecutor {
        fn execute(&mut self, invocation: &ProcessInvocation) -> Result<ProcessOutput, String> {
            self.calls.push(invocation.args.clone());
            self.outputs
                .pop_front()
                .unwrap_or_else(|| panic!("missing fake output for {:?}", invocation.args))
        }
    }

    fn output(success: bool, stdout: &str, stderr: &str) -> Result<ProcessOutput, String> {
        Ok(ProcessOutput {
            success,
            code: Some(if success { 0 } else { 1 }),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        })
    }

    fn ok() -> Result<ProcessOutput, String> {
        output(true, "", "")
    }

    fn status(value: &str) -> Result<ProcessOutput, String> {
        output(value == "active" || value == "enabled", value, "")
    }

    fn discovery(load_state: &str, fragment_path: &str) -> Result<ProcessOutput, String> {
        output(
            true,
            &format!("LoadState={load_state}\nFragmentPath={fragment_path}\n"),
            "",
        )
    }

    fn absent_discovery() -> Result<ProcessOutput, String> {
        discovery("not-found", "")
    }

    fn failed(message: &str) -> Result<ProcessOutput, String> {
        output(false, "", message)
    }

    #[test]
    fn new_install_daemon_reload_failure_removes_unit_and_reloads_again() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            failed("reload rejected"),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);
        let error = install_unit_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new unit",
            false,
            false,
        )
        .unwrap_err();
        assert!(error.contains("installation failed for webcodex.service"));
        assert!(!service_file.exists());
        assert_eq!(
            executor.calls[0],
            [
                "show",
                SERVER_SERVICE_UNIT,
                "--property=LoadState",
                "--property=FragmentPath",
                "--no-pager"
            ]
        );
        assert_eq!(executor.calls[3], ["daemon-reload"]);
        assert_eq!(executor.calls[4], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[5], ["disable", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[6], ["daemon-reload"]);
        assert_eq!(executor.calls[7], ["reset-failed", SERVER_SERVICE_UNIT]);
    }

    #[test]
    fn new_install_verification_failure_stops_disables_and_removes_unit() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            ok(),
            ok(),
            failed("not active"),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);
        install_unit_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new unit",
            false,
            false,
        )
        .unwrap_err();
        assert!(!service_file.exists());
        assert_eq!(executor.calls[6], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[7], ["disable", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[8], ["daemon-reload"]);
    }

    #[test]
    fn no_start_verification_failure_disables_without_starting() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            ok(),
            ok(),
            failed("not enabled"),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);
        install_unit_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new unit",
            false,
            true,
        )
        .unwrap_err();
        assert!(!service_file.exists());
        assert!(executor
            .calls
            .iter()
            .any(|args| args == &["disable", SERVER_SERVICE_UNIT]));
        assert!(!executor
            .calls
            .iter()
            .any(|args| args == &["start", SERVER_SERVICE_UNIT]));
    }

    #[test]
    fn overwrite_failure_restores_unit_and_prior_active_enabled_state() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        std::fs::write(&service_file, "old unit").unwrap();
        let mut executor = FakeExecutor::with_outputs(vec![
            discovery("loaded", service_file.to_str().unwrap()),
            status("active"),
            status("enabled"),
            ok(),
            ok(),
            failed("verification failed"),
            ok(),
            ok(),
            ok(),
            ok(),
        ]);
        install_unit_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new unit",
            true,
            false,
        )
        .unwrap_err();
        assert_eq!(std::fs::read_to_string(&service_file).unwrap(), "old unit");
        assert_eq!(executor.calls[6], ["stop", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[7], ["daemon-reload"]);
        assert_eq!(executor.calls[8], ["enable", SERVER_SERVICE_UNIT]);
        assert_eq!(executor.calls[9], ["start", SERVER_SERVICE_UNIT]);
    }

    #[test]
    fn rollback_failures_are_bounded_and_do_not_replace_install_error() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let long = "x".repeat(500);
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            failed("primary install failure"),
            failed(&long),
            failed(&long),
            failed(&long),
            failed(&long),
        ]);
        let error = install_unit_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new unit",
            false,
            false,
        )
        .unwrap_err();
        assert!(error.contains("primary install failure"));
        assert!(error.contains("rollback also encountered"));
        assert!(error.len() < 1000, "{}", error.len());
    }

    /// Unix-only: systemd unit encoders validate Unix absolute-path rules.
    #[cfg(unix)]
    #[test]
    fn systemd_encoders_keep_distinct_program_argument_and_path_rules() {
        assert_eq!(
            encode_exec_argument("ExecStart argument", "/opt/web codex/a\"b\\c%p").unwrap(),
            "\"/opt/web codex/a\\\"b\\\\c%%p\""
        );
        assert_eq!(
            encode_exec_program("ExecStart", Path::new("/opt/web codex/server%p")).unwrap(),
            "\"/opt/web codex/server%%p\""
        );
        assert!(encode_exec_program("ExecStart", Path::new("/opt/a\"b")).is_err());
        assert!(encode_exec_program("ExecStart", Path::new("/opt/a\\b")).is_err());
        assert_eq!(
            encode_unit_path_value("WorkingDirectory", Path::new("/srv/web codex/a\"b\\c%p"))
                .unwrap(),
            "/srv/web\\x20codex/a\\x22b\\x5cc%%p"
        );
        for value in ["bad\nvalue", "bad\rvalue", "bad\0value", "bad\tvalue"] {
            assert!(encode_exec_argument("ExecStart argument", value).is_err());
        }
        for value in ["webcodex", "web_codex-1.service", "group.name"] {
            validate_systemd_identity("User", value).unwrap();
        }
        for value in [
            "",
            "bad user",
            "bad/group",
            "bad\\group",
            "bad=group",
            "bad\"group",
        ] {
            assert!(validate_systemd_identity("User", value).is_err());
        }
    }

    #[test]
    fn user_unit_parent_is_created_without_touching_systemd() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("xdg/systemd/user/webcodex-runner.service");
        ensure_service_file_parent(&service_file).unwrap();
        assert!(service_file.parent().unwrap().is_dir());
        assert!(!service_file.exists());
    }

    #[cfg(unix)]
    #[test]
    fn user_unit_parent_rejects_a_symlinked_final_directory() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let linked = tmp.path().join("user");
        symlink(&target, &linked).unwrap();
        let error =
            ensure_service_file_parent(&linked.join("webcodex-runner.service")).unwrap_err();
        assert!(
            error.contains("symlinked service unit directory"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn overwrite_rejects_symlink_masked_and_non_regular_units_before_systemctl() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let systemctl = Path::new("/usr/bin/systemctl");
        let target = tmp.path().join("target.service");
        std::fs::write(&target, "old").unwrap();

        let linked = tmp.path().join("linked.service");
        symlink(&target, &linked).unwrap();
        let mut executor = FakeExecutor::default();
        let error = install_unit_with_executor(
            &mut executor,
            systemctl,
            &linked,
            SERVER_SERVICE_UNIT,
            "new",
            true,
            false,
        )
        .unwrap_err();
        assert!(error.contains("systemd unit symlink"));
        assert!(executor.calls.is_empty());

        let masked = tmp.path().join("masked.service");
        symlink("/dev/null", &masked).unwrap();
        let error = install_unit_with_executor(
            &mut executor,
            systemctl,
            &masked,
            SERVER_SERVICE_UNIT,
            "new",
            true,
            false,
        )
        .unwrap_err();
        assert!(error.contains("masked systemd unit"));
        assert!(executor.calls.is_empty());

        let directory = tmp.path().join("directory.service");
        std::fs::create_dir(&directory).unwrap();
        let error = install_unit_with_executor(
            &mut executor,
            systemctl,
            &directory,
            SERVER_SERVICE_UNIT,
            "new",
            true,
            false,
        )
        .unwrap_err();
        assert!(error.contains("non-regular systemd unit"));
        assert!(executor.calls.is_empty());
    }

    #[test]
    fn overwrite_rejects_transitional_or_unknown_active_states_before_writing() {
        for state in ["activating", "deactivating", "reloading", "unknown"] {
            let tmp = tempfile::tempdir().unwrap();
            let service_file = tmp.path().join("webcodex.service");
            std::fs::write(&service_file, "old unit").unwrap();
            let mut executor = FakeExecutor::with_outputs(vec![
                discovery("loaded", service_file.to_str().unwrap()),
                status(state),
            ]);
            let error = install_unit_with_executor(
                &mut executor,
                Path::new("/usr/bin/systemctl"),
                &service_file,
                SERVER_SERVICE_UNIT,
                "new unit",
                true,
                false,
            )
            .unwrap_err();
            assert!(
                error.contains("cannot safely overwrite"),
                "{state}: {error}"
            );
            assert!(error.contains("systemctl is-active"), "{state}: {error}");
            assert_eq!(std::fs::read_to_string(&service_file).unwrap(), "old unit");
            assert_eq!(executor.calls.len(), 2);
        }
    }

    #[test]
    fn overwrite_rejects_special_enabled_states_before_writing() {
        for state in [
            "enabled-runtime",
            "linked",
            "linked-runtime",
            "alias",
            "masked",
            "masked-runtime",
            "unknown",
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let service_file = tmp.path().join("webcodex.service");
            std::fs::write(&service_file, "old unit").unwrap();
            let mut executor = FakeExecutor::with_outputs(vec![
                discovery("loaded", service_file.to_str().unwrap()),
                status("inactive"),
                status(state),
            ]);
            let error = install_unit_with_executor(
                &mut executor,
                Path::new("/usr/bin/systemctl"),
                &service_file,
                SERVER_SERVICE_UNIT,
                "new unit",
                true,
                false,
            )
            .unwrap_err();
            assert!(
                error.contains("cannot safely overwrite"),
                "{state}: {error}"
            );
            assert_eq!(std::fs::read_to_string(&service_file).unwrap(), "old unit");
            assert_eq!(executor.calls.len(), 3);
        }
    }

    #[test]
    fn genuinely_absent_unit_installs_after_explicit_not_found_discovery() {
        let tmp = tempfile::tempdir().unwrap();
        let service_file = tmp.path().join("webcodex.service");
        let mut executor = FakeExecutor::with_outputs(vec![
            absent_discovery(),
            status("inactive"),
            status("disabled"),
            ok(),
            ok(),
            ok(),
        ]);
        let result = install_unit_with_executor(
            &mut executor,
            Path::new("/usr/bin/systemctl"),
            &service_file,
            SERVER_SERVICE_UNIT,
            "new unit",
            false,
            false,
        )
        .unwrap();
        assert!(result.started);
        assert_eq!(std::fs::read_to_string(&service_file).unwrap(), "new unit");
        assert_eq!(executor.calls.len(), 6);
    }

    #[test]
    fn external_vendor_runtime_and_generated_units_are_rejected_before_side_effects() {
        for (load_state, fragment) in [
            ("loaded", "/usr/lib/systemd/system/webcodex.service"),
            ("loaded", "/run/systemd/system/webcodex.service"),
            ("loaded", "/run/systemd/generator/webcodex.service"),
            ("generated", "/run/systemd/generator.late/webcodex.service"),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let service_file = tmp.path().join("webcodex.service");
            let mut executor = FakeExecutor::with_outputs(vec![discovery(load_state, fragment)]);
            let error = install_unit_with_executor(
                &mut executor,
                Path::new("/usr/bin/systemctl"),
                &service_file,
                SERVER_SERVICE_UNIT,
                "new unit",
                false,
                false,
            )
            .unwrap_err();
            assert!(error.contains("already exists outside"), "{error}");
            assert!(
                error.contains("refusing to create a local override"),
                "{error}"
            );
            assert!(!service_file.exists());
            assert_eq!(executor.calls.len(), 1);
            assert_eq!(executor.calls[0][0], "show");
        }
    }

    #[test]
    fn failed_or_ambiguous_unit_discovery_is_conservatively_rejected() {
        let cases = vec![
            Err("systemctl unavailable".to_string()),
            output(false, "", "manager unavailable"),
            output(true, "", ""),
            output(true, "LoadState=not-found\n", ""),
            output(true, "FragmentPath=\n", ""),
            output(
                true,
                "LoadState=not-found\nLoadState=loaded\nFragmentPath=\n",
                "",
            ),
        ];
        for discovery_output in cases {
            let tmp = tempfile::tempdir().unwrap();
            let service_file = tmp.path().join("webcodex.service");
            let mut executor = FakeExecutor::with_outputs(vec![discovery_output]);
            let error = install_unit_with_executor(
                &mut executor,
                Path::new("/usr/bin/systemctl"),
                &service_file,
                SERVER_SERVICE_UNIT,
                "new unit",
                false,
                false,
            )
            .unwrap_err();
            assert!(
                error.contains("cannot determine whether systemd unit"),
                "{error}"
            );
            assert!(error.contains("no changes were made"), "{error}");
            assert!(!service_file.exists());
            assert_eq!(executor.calls.len(), 1);
        }
    }

    #[test]
    fn discovery_parser_ignores_unknown_fields_and_rejects_conflicting_keys() {
        assert_eq!(
            parse_unit_discovery(
                "Description=ignored\nLoadState=loaded\nFragmentPath=/usr/lib/systemd/system/a.service\n"
            )
            .unwrap(),
            UnitDiscovery {
                load_state: "loaded".to_string(),
                fragment_path: "/usr/lib/systemd/system/a.service".to_string(),
            }
        );
        assert!(
            parse_unit_discovery("LoadState=loaded\nFragmentPath=/a\nFragmentPath=/b\n").is_err()
        );
    }

    #[test]
    fn unit_name_comes_from_selected_service_file() {
        assert_eq!(
            service_unit_name(
                Path::new("/etc/systemd/system/webcodex-runner-special.service"),
                AGENT_SERVICE_UNIT
            ),
            "webcodex-runner-special.service"
        );
    }
}
