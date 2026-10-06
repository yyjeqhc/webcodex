mod coding_agents;
mod connections;
mod diagnostics;
mod environment;
mod environment_invitation;
pub use environment_invitation::{InvitationRequest, InvitationResponse};
mod managed_instructions;
mod mcp_providers;
mod operation_completion;
mod path_inventory;
pub use path_inventory::{ExportInventoryRequest, OpenInventoryRequest};
#[cfg(test)]
mod projectless_tests;
#[cfg(test)]
mod reconfiguration_tests;
mod runner_capability_grant;
mod runtime_shell;
mod ssh_resources;
pub(crate) mod updates;
mod workspace;
mod workspace_settings;
use crate::activity::{ActivityEventKind, ActivityLevel, ActivityLog};
use crate::connections::ConnectionRuntimes;
use crate::deadline::Deadline;
use crate::error::{DesktopError, DesktopResult};
use crate::models::{
    aggregate_readiness, ChatGptActivitySnapshot, DesktopOperationKind, DesktopStateSnapshot,
    Enrollment, Experience, Exposure, ExposureReadiness, ProjectInspection, ProjectReadiness,
    ProjectSelection, QuickShareState, ReadinessNextActionKind, ReadinessSummaryKind,
    RegularConnectionPreference, RunnerReadiness, RunnerTopology, RuntimeTopology, ServerReadiness,
    ServerTopology, StoredDesktopConfig, StoredRuntime, TunnelProxyConfig, TunnelProxyMode,
    TunnelProxySnapshot,
};
use crate::operation::{
    cancelled_error, CancellationContext, CancellationSignal, OperationAdmission,
    OperationController,
};
use crate::process::{MachineEventReceiver, ProcessKey, ProcessPhase, ProcessSupervisor};
use crate::tunnel_config::{TunnelConfig, TunnelConfigRequest};
use crate::webcodex::{
    inspect_project_path, ProjectRuntimeIdentity, QuickShareReadyEvent, RunnerRuntimeIdentity,
    WebCodexAdapter,
};
pub use connections::ConnectionAction;
use serde_json::Value;
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::Mutex;

const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(20);
const STALE_LOOPBACK_PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const RUNNER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const PROJECT_READY_TIMEOUT: Duration = Duration::from_secs(20);
const QUICK_SHARE_READY_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(300);
const READINESS_CLEANUP_SLACK: Duration = Duration::from_secs(2);
const SHUTDOWN_OPERATION_WAIT: Duration = Duration::from_secs(5);
const DESKTOP_STATE_MAX_BYTES: u64 = 256 * 1024;
const DESKTOP_SERVER_ENV_MAX_BYTES: u64 = 256 * 1024;
const DESKTOP_MCP_HOST_PROFILE: &str = "host_code_mode";
const DESKTOP_MCP_HOST_BUDGET_SECS: &str = "55";
const DESKTOP_MCP_COMPACT_SCHEMAS: &str = "true";
const DESKTOP_MCP_TRUST_LOOPBACK_API_TOKEN_FILE_IMPORT: &str = "true";
static NEXT_STATE_TEMP_ID: AtomicU64 = AtomicU64::new(1);

type SharedSupervisor = Arc<Mutex<ProcessSupervisor>>;

#[derive(Debug, Clone)]
struct ChatGptActivityProbe {
    identity: ProjectRuntimeIdentity,
    webcodex: PathBuf,
}

pub struct AppState {
    core: Mutex<Option<DesktopCore>>,
    desktop_data_dir: crate::desktop_data_dir::DesktopDataDir,
    managed_instructions: Arc<crate::managed_instructions::ManagedInstructions>,
    ssh_resources: Mutex<crate::ssh_resources::SshResourcesManager>,
    published: Arc<RwLock<DesktopStateSnapshot>>,
    supervisor: SharedSupervisor,
    activity: ActivityLog,
    operations: OperationController,
    shutdown_signal: CancellationSignal,
    shutdown_started: AtomicBool,
    connections: ConnectionRuntimes,
    update_check: tokio::sync::Mutex<()>,
    updates: Arc<crate::updates::UpdateManager>,
}

include!("state/app_state.rs");
include!("state/desktop_core.rs");
include!("state/readiness.rs");
include!("state/persistence.rs");
include!("state/tunnel.rs");
include!("state/tests.rs");
