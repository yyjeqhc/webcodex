//! Host service ownership and lifecycle. Installation never adopts an unmarked service.
//! `install` configures boot startup; `start` is a separate, explicit operation.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
mod linux;
mod log;
#[cfg(target_os = "macos")]
mod macos;
pub mod runtime;
pub use log::{ServiceLog, ServiceLogEvent, ServiceLogGuard, SERVICE_LOG_NAME};
#[cfg(windows)]
mod windows;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Component {
    Server,
    Runner,
    Tunnel,
}

impl Component {
    fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Runner => "runner",
            Self::Tunnel => "tunnel",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceAccount {
    /// A real account. `expected_identity` is the decimal UID on Unix or SID
    /// string on Windows, captured by setup from the selected project owner.
    SystemUser {
        name: String,
        group: Option<String>,
        expected_identity: String,
        home: Option<PathBuf>,
    },
    /// Server/Tunnel only. Windows SCM owns this passwordless account.
    WindowsVirtual { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinuxSocketSpec {
    /// Fixed IP:port from the existing Server environment file.
    pub listen: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceSpec {
    /// Platform-independent service basename, e.g. `webcodex` or
    /// `webcodex-runner`; the Linux unit is `<id>.service`.
    pub id: String,
    pub component: Component,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub account: ServiceAccount,
    /// Stable, non-secret identity of the Core configuration to which the
    /// service belongs. Changing it is an explicit ownership transition.
    pub config_identity: String,
    pub env_file: Option<PathBuf>,
    /// Non-secret environment values only. Windows services use the account's
    /// native profile; Windows-specific environment injection is unsupported.
    pub environment: BTreeMap<String, String>,
    pub linux_socket: Option<LinuxSocketSpec>,
}

impl fmt::Debug for ServiceSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceSpec")
            .field("id", &self.id)
            .field("component", &self.component)
            .field("program", &self.program)
            .field("args", &"[REDACTED]")
            .field("working_directory", &self.working_directory)
            .field("account", &self.account)
            .field("config_identity", &self.config_identity)
            .field("env_file", &self.env_file)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .field("linux_socket", &self.linux_socket)
            .finish()
    }
}

/// SCM credentials are provided only during first installation or explicit
/// account rotation. They are never placed in ServiceSpec or diagnostic text.
pub struct ServiceCredential(Vec<u16>);

impl ServiceCredential {
    pub fn from_password(password: &str) -> Self {
        Self(password.encode_utf16().chain(std::iter::once(0)).collect())
    }

    #[cfg(windows)]
    fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }
}

impl fmt::Debug for ServiceCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ServiceCredential([REDACTED])")
    }
}

impl Drop for ServiceCredential {
    fn drop(&mut self) {
        for unit in &mut self.0 {
            // Volatile writes ensure the compiler does not elide the scrub.
            unsafe { std::ptr::write_volatile(unit, 0) };
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ownership {
    Absent,
    Owned,
    Foreign,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub id: String,
    pub ownership: Ownership,
    pub installed: bool,
    pub enabled: Option<bool>,
    pub running: Option<bool>,
    pub detail: Option<String>,
}

impl ServiceStatus {
    fn absent(id: &str) -> Self {
        Self {
            id: id.to_owned(),
            ownership: Ownership::Absent,
            installed: false,
            enabled: Some(false),
            running: Some(false),
            detail: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceErrorCode {
    InvalidSpec,
    MissingPrerequisite,
    ForeignService,
    OwnershipUnknown,
    CredentialRequired,
    PermissionDenied,
    Busy,
    OperationFailed,
    OutcomeUnknown,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceError {
    pub code: ServiceErrorCode,
    pub message: String,
    pub recovery_hint: &'static str,
}

impl ServiceError {
    pub(crate) fn new(code: ServiceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            recovery_hint: "Inspect the existing service and its owner before retrying.",
        }
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)
    }
}

impl std::error::Error for ServiceError {}

impl ServiceErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidSpec => "invalid_spec",
            Self::MissingPrerequisite => "missing_prerequisite",
            Self::ForeignService => "foreign_service",
            Self::OwnershipUnknown => "ownership_unknown",
            Self::CredentialRequired => "credential_required",
            Self::PermissionDenied => "permission_denied",
            Self::Busy => "service_busy",
            Self::OperationFailed => "operation_failed",
            Self::OutcomeUnknown => "outcome_unknown",
            Self::Unsupported => "unsupported_platform",
        }
    }
}

pub struct ServiceManager;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentAccount {
    pub name: String,
    /// Decimal UID on Unix; canonical SID string on Windows.
    pub identity: String,
    pub home: PathBuf,
}

/// Resolve the calling process's effective account without trusting a
/// caller-supplied `USER`/`USERNAME` string as its identity.
pub fn current_account() -> Result<CurrentAccount, ServiceError> {
    platform::current_account()
}

/// Grant the exact service identity access to a dedicated data directory.
/// Windows extends existing ACLs for each existing entry without changing
/// ownership; callers must stop the service before a populated-tree migration.
pub fn grant_service_directory(spec: &ServiceSpec, path: &Path) -> Result<(), ServiceError> {
    validate_spec(spec)?;
    platform::grant_service_directory(spec, path)
}

impl ServiceManager {
    /// Provision the Windows account in SCM before consuming a pairing code.
    /// The service remains disabled until configuration has been committed.
    #[cfg(windows)]
    pub fn prepare_runner(
        spec: &ServiceSpec,
        credential: Option<&ServiceCredential>,
    ) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::prepare_runner(spec, credential)
    }

    #[cfg(windows)]
    pub fn validate_credential(
        spec: &ServiceSpec,
        credential: &ServiceCredential,
        project: Option<&Path>,
    ) -> Result<(), ServiceError> {
        validate_spec(spec)?;
        platform::validate_credential(spec, credential, project)
    }
    pub fn inspect(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::inspect(spec)
    }

    pub fn preflight(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::preflight(spec)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn preflight_replacing(spec: &ServiceSpec) -> Result<(), ServiceError> {
        validate_spec(spec)?;
        linux::preflight_replacing(spec)
    }

    pub fn install(
        spec: &ServiceSpec,
        credential: Option<&ServiceCredential>,
    ) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::install(spec, credential)
    }

    pub fn start(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::start(spec)
    }

    pub fn stop(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::stop(spec)
    }

    pub fn restart(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::restart(spec)
    }

    pub fn uninstall(spec: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::uninstall(spec)
    }

    /// Explicit Windows project-account credential rotation. The password
    /// goes to SCM only; successful configuration should be followed by a
    /// separately authorized restart to verify next-boot logon.
    pub fn update_credential(
        spec: &ServiceSpec,
        credential: &ServiceCredential,
    ) -> Result<ServiceStatus, ServiceError> {
        validate_spec(spec)?;
        platform::update_credential(spec, credential)
    }
}

#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(windows)]
use windows as platform;

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod platform {
    use super::*;
    pub(super) fn inspect(_: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn preflight(_: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn install(
        _: &ServiceSpec,
        _: Option<&ServiceCredential>,
    ) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn start(_: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn stop(_: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn restart(_: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn uninstall(_: &ServiceSpec) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn update_credential(
        _: &ServiceSpec,
        _: &ServiceCredential,
    ) -> Result<ServiceStatus, ServiceError> {
        unsupported()
    }
    pub(super) fn current_account() -> Result<CurrentAccount, ServiceError> {
        Err(ServiceError::new(
            ServiceErrorCode::Unsupported,
            "account lookup is unsupported on this host",
        ))
    }
    pub(super) fn grant_service_directory(_: &ServiceSpec, _: &Path) -> Result<(), ServiceError> {
        Err(ServiceError::new(
            ServiceErrorCode::Unsupported,
            "service ACL is unsupported on this host",
        ))
    }
    fn unsupported() -> Result<ServiceStatus, ServiceError> {
        Err(ServiceError::new(
            ServiceErrorCode::Unsupported,
            "system services are unsupported on this host",
        ))
    }
}

fn validate_spec(spec: &ServiceSpec) -> Result<(), ServiceError> {
    if spec.id.is_empty()
        || spec.id.len() > 80
        || !spec
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        || spec.config_identity.is_empty()
        || spec.config_identity.len() > 512
        || spec.config_identity.chars().any(char::is_control)
        || !spec.program.is_absolute()
        || !spec.working_directory.is_absolute()
        || spec.env_file.as_ref().is_some_and(|p| !p.is_absolute())
        || spec.args.iter().any(|a| {
            a.contains('\0')
                || a.contains('\n')
                || a.contains('\r')
                || ["--password", "--token", "--secret", "--api-key"]
                    .iter()
                    .any(|flag| {
                        a.eq_ignore_ascii_case(flag)
                            || a.to_ascii_lowercase().starts_with(&format!("{flag}="))
                    })
                || a.to_ascii_uppercase().contains("OPENAI_API_KEY=")
                || a.to_ascii_uppercase().contains("OPENAI_ADMIN_KEY=")
        })
        || spec.environment.iter().any(|(key, value)| {
            key.is_empty()
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !(key == "RUST_LOG"
                    || cfg!(target_os = "macos") && key == "WEBCODEX_SERVICE_LOG_DIR")
                || key == "WEBCODEX_SERVICE_LOG_DIR"
                    && value.as_str() != spec.working_directory.to_string_lossy().as_ref()
                || value.chars().any(|ch| matches!(ch, '\0' | '\n' | '\r'))
        })
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "invalid service identity, executable, arguments, or environment",
        ));
    }
    if spec.component == Component::Runner
        && !matches!(spec.account, ServiceAccount::SystemUser { .. })
    {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "Runner service must use its real project user account",
        ));
    }
    if let ServiceAccount::SystemUser {
        name,
        expected_identity,
        home,
        ..
    } = &spec.account
    {
        if name.is_empty()
            || expected_identity.is_empty()
            || name.chars().any(char::is_control)
            || home.as_ref().is_some_and(|p| !p.is_absolute())
        {
            return Err(ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "service account identity is incomplete",
            ));
        }
    }
    if spec.linux_socket.is_some() && spec.component != Component::Server {
        return Err(ServiceError::new(
            ServiceErrorCode::InvalidSpec,
            "only Server may own a Linux socket unit",
        ));
    }
    Ok(())
}

fn check_files(spec: &ServiceSpec) -> Result<(), ServiceError> {
    let program = std::fs::metadata(&spec.program).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service executable is absent",
        )
    })?;
    if !program.is_file() {
        return Err(ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service executable is not a regular file",
        ));
    }
    if let Ok(working) = std::fs::metadata(&spec.working_directory) {
        if !working.is_dir() {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "service working directory is not a directory",
            ));
        }
    }
    Ok(())
}

/// Setup preflight can run before config files are written; installation
/// cannot, because a boot service must never be enabled with missing inputs.
fn check_install_files(spec: &ServiceSpec) -> Result<(), ServiceError> {
    check_files(spec)?;
    if !spec.working_directory.is_dir() {
        return Err(ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service working directory is absent",
        ));
    }
    if let Some(path) = &spec.env_file {
        let env = std::fs::metadata(path).map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "service environment file is absent",
            )
        })?;
        if !env.is_file() {
            return Err(ServiceError::new(
                ServiceErrorCode::MissingPrerequisite,
                "service environment file is not regular",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn current_unix_account() -> Result<CurrentAccount, ServiceError> {
    use std::ffi::CStr;
    use std::mem::MaybeUninit;
    let uid = unsafe { libc::geteuid() };
    let mut record = MaybeUninit::<libc::passwd>::zeroed();
    let mut result = std::ptr::null_mut();
    let mut buffer = vec![0u8; 16384];
    let code = unsafe {
        libc::getpwuid_r(
            uid,
            record.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut result,
        )
    };
    if code != 0 || result.is_null() {
        return Err(ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "cannot resolve current Unix account",
        ));
    }
    let record = unsafe { record.assume_init() };
    let name = unsafe { CStr::from_ptr(record.pw_name) }
        .to_str()
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Unix account name is not UTF-8",
            )
        })?
        .to_owned();
    let home = unsafe { CStr::from_ptr(record.pw_dir) }
        .to_str()
        .map_err(|_| {
            ServiceError::new(
                ServiceErrorCode::InvalidSpec,
                "Unix account home is not UTF-8",
            )
        })?;
    Ok(CurrentAccount {
        name,
        identity: uid.to_string(),
        home: PathBuf::from(home),
    })
}

fn check_owned(status: &ServiceStatus) -> Result<(), ServiceError> {
    match status.ownership {
        Ownership::Owned => Ok(()),
        Ownership::Absent => Err(ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service is not installed",
        )),
        Ownership::Foreign => Err(ServiceError::new(
            ServiceErrorCode::ForeignService,
            "service exists but does not match WebCodex ownership and configuration",
        )),
        Ownership::Unknown => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "existing service ownership cannot be verified",
        )),
    }
}

fn hex_identity(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        use std::fmt::Write;
        let _ = write!(&mut out, "{byte:02x}");
    }
    out
}

fn ownership_marker(spec: &ServiceSpec) -> String {
    format!(
        "WebCodex managed v1 component={} identity={}",
        spec.component.as_str(),
        hex_identity(&spec.config_identity)
    )
}

#[cfg(unix)]
fn ensure_safe_new_path(path: &Path) -> Result<(), ServiceError> {
    let parent = path.parent().ok_or_else(|| {
        ServiceError::new(ServiceErrorCode::InvalidSpec, "service path lacks parent")
    })?;
    let meta = std::fs::symlink_metadata(parent).map_err(|_| {
        ServiceError::new(
            ServiceErrorCode::MissingPrerequisite,
            "service directory is absent",
        )
    })?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "service directory is not a real directory",
        ));
    }
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(ServiceError::new(
            ServiceErrorCode::ForeignService,
            "service path already exists",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(ServiceError::new(
            ServiceErrorCode::OwnershipUnknown,
            "cannot inspect service path",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_requires_real_account() {
        let mut spec = sample_spec();
        spec.account = ServiceAccount::WindowsVirtual {
            name: "NT SERVICE\\WebCodex".into(),
        };
        assert_eq!(
            validate_spec(&spec).unwrap_err().code,
            ServiceErrorCode::InvalidSpec
        );
    }

    #[test]
    fn credential_debug_is_redacted() {
        let credential = ServiceCredential::from_password("secret-123");
        assert!(!format!("{credential:?}").contains("secret-123"));
    }

    #[test]
    fn service_spec_never_debugs_arguments_or_environment_values() {
        let mut spec = sample_spec();
        spec.args.push("private-argument".into());
        spec.environment
            .insert("RUST_LOG".into(), "private-value".into());
        let debug = format!("{spec:?}");
        assert!(!debug.contains("private-argument"));
        assert!(!debug.contains("private-value"));
    }

    #[test]
    fn secret_arguments_and_environment_are_rejected() {
        let mut spec = sample_spec();
        spec.args.push("--password=hidden".into());
        assert_eq!(
            validate_spec(&spec).unwrap_err().code,
            ServiceErrorCode::InvalidSpec
        );
        spec.args.pop();
        spec.environment
            .insert("OPENAI_API_KEY".into(), "hidden".into());
        assert_eq!(
            validate_spec(&spec).unwrap_err().code,
            ServiceErrorCode::InvalidSpec
        );
    }

    pub(super) fn sample_spec() -> ServiceSpec {
        ServiceSpec {
            id: "webcodex-runner".into(),
            component: Component::Runner,
            program: PathBuf::from("/usr/bin/webcodex-runner"),
            args: vec!["--config".into(), "/var/lib/webcodex/runner.toml".into()],
            working_directory: PathBuf::from("/var/lib/webcodex"),
            account: ServiceAccount::SystemUser {
                name: "alice".into(),
                group: Some("alice".into()),
                expected_identity: "1000".into(),
                home: Some(PathBuf::from("/home/alice")),
            },
            config_identity: "runner:alpha".into(),
            env_file: None,
            environment: BTreeMap::new(),
            linux_socket: None,
        }
    }
}
