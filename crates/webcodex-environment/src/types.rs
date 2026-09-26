use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const ENVIRONMENT_SCHEMA: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ServiceOperation {
    PrepareRunner,
    Install,
    Start,
    Stop,
    Restart,
    Uninstall,
    UpdateCredential,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EnvironmentMode {
    Create { listen: String },
    Join,
}

/// Contains no credentials. This exact intent is persisted before any effect.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupRequest {
    pub mode: EnvironmentMode,
    pub server_url: String,
    pub project: Option<PathBuf>,
    pub account: LocalAccount,
    pub binaries: RuntimeBinaries,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalAccount {
    pub name: String,
    /// Numeric Unix uid or Windows SID, never an inferred display name.
    pub identity: String,
    pub home: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeBinaries {
    pub cli: PathBuf,
    pub server: PathBuf,
    pub runner: PathBuf,
}

impl SetupRequest {
    pub fn local_server(&self) -> bool {
        matches!(self.mode, EnvironmentMode::Create { .. })
    }

    pub fn local_runner(&self) -> bool {
        self.project.is_some()
    }

    pub fn steps(&self) -> Vec<SetupStep> {
        use SetupStep::*;
        let mut steps = vec![Preflight];
        if self.local_server() {
            steps.extend([
                ServerConfiguration,
                ServerServiceInstall,
                ServerServiceStart,
            ]);
        }
        steps.push(ServerReachability);
        steps.push(if self.local_runner() && !self.local_server() {
            RunnerEnrollment
        } else {
            UserAuthentication
        });
        if self.local_runner() {
            steps.extend([
                RunnerConfiguration,
                ProjectRegistration,
                RunnerServiceInstall,
                RunnerServiceStart,
            ]);
        }
        steps.push(Readiness);
        steps
    }
}

/// Deliberately neither serializable nor transparently printable.
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[redacted]")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        // Clear this owned allocation; copies are never put into diagnostic state.
        unsafe {
            for byte in self.0.as_bytes_mut() {
                std::ptr::write_volatile(byte, 0);
            }
        }
        std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    }
}

#[derive(Debug, Default)]
pub struct SetupSecrets {
    pub pairing_code: Option<Secret>,
    pub user_token: Option<Secret>,
    pub service_password: Option<Secret>,
    /// An explicit recovery action, never inferred from retrying configure.
    pub replacement_pairing_code: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SetupStep {
    Preflight,
    ServerConfiguration,
    ServerServiceInstall,
    ServerServiceStart,
    ServerReachability,
    UserAuthentication,
    RunnerEnrollment,
    RunnerConfiguration,
    ProjectRegistration,
    RunnerServiceInstall,
    RunnerServiceStart,
    Readiness,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Started,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupProgress {
    pub operation_id: String,
    pub step: SetupStep,
    pub state: StepState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRecord {
    pub id: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeObservation {
    pub server_reachable: bool,
    pub authenticated: bool,
    /// None means that no local Runner was requested, not that it is offline.
    pub runner_online: Option<bool>,
    pub projects_visible: Vec<String>,
    /// Server-authorized fleet snapshot; remote project paths are display-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fleet: Option<RuntimeFleet>,
    #[serde(default)]
    pub diagnostics: Vec<SetupDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeFleet {
    pub observed_at_ms: u64,
    pub projects_available: bool,
    pub projects_truncated: bool,
    pub runners: Vec<FleetRunner>,
    pub projects: Vec<FleetProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetRunner {
    pub client_id: String,
    pub connected: bool,
    pub status: Option<String>,
    pub computer_session_availability: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FleetProject {
    pub id: String,
    pub client_id: String,
    pub name: Option<String>,
    /// A path on the remote Runner, never interpreted on this machine.
    pub path: Option<String>,
    pub connected: bool,
    pub agent_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentRecord {
    pub schema_version: u16,
    pub environment_id: String,
    pub request: SetupRequest,
    pub username: Option<String>,
    pub runner_client_id: Option<String>,
    pub projects: Vec<ProjectRecord>,
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupJournal {
    pub schema_version: u16,
    pub operation_id: String,
    pub environment: EnvironmentRecord,
    pub steps: BTreeMap<SetupStep, StepState>,
    pub last_diagnostic: Option<SetupDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupResult {
    pub environment: EnvironmentRecord,
    pub observation: RuntimeObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SetupDiagnostic {
    pub code: String,
    pub message: String,
    pub recovery: String,
}

impl SetupDiagnostic {
    pub fn new(code: &str, message: &str, recovery: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recovery: recovery.into(),
        }
    }
    pub(crate) fn io() -> Self {
        Self::new(
            "state_io",
            "Could not safely read or save environment state",
            "Check directory ownership, permissions and free space; then resume setup",
        )
    }
}

impl std::fmt::Display for SetupDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}. {}", self.code, self.message, self.recovery)
    }
}
impl std::error::Error for SetupDiagnostic {}

pub type SetupResultValue<T> = Result<T, SetupDiagnostic>;
