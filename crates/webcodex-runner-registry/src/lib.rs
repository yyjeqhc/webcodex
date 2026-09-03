//! Server-side authoritative Runner registration and lifecycle contracts.
//!
//! The registry implementation is extracted into this crate in the following
//! refactor step. Neutral contracts land first so root authentication and
//! telemetry policy can depend on the registry domain, never the reverse.

mod access;
mod access_control;
mod agents;
mod capabilities;
mod job_status;
mod job_updates;
mod jobs;
mod polling;
mod project_inventory;
mod projects;
mod protocol;
mod reconciliation;
mod registry;
mod requests;
mod state;
mod telemetry;
mod validation;

pub use access::{DetachedInitiatorIdentity, RunnerAccess, RunnerAccessGroup};
pub use capabilities::{RunnerFeature, RunnerFeatureSet};
pub use job_status::job_status_is_active;
pub use job_updates::{
    JobLogWait, JobLogWaitOutcome, ShellJobLogObservation, ShellJobStartMetadata,
    StructuredJobExecution,
};
pub use jobs::{command_preview, process_preview, script_preview, COMMAND_PREVIEW_MAX_CHARS};
pub(crate) use protocol::AcceptedRunnerProtocol;
pub use reconciliation::recovery_timeout_sweep;
pub use registry::{
    job_recovery_grace_secs, AgentTransport, RunnerRegistry, CLIENT_ONLINE_WINDOW_SECS,
    DETACHED_IDEMPOTENCY_CONFLICT, DETACHED_IDEMPOTENCY_RECOVERY_PREFIX,
    JOB_RECOVERY_GRACE_MAX_SECS, JOB_RECOVERY_GRACE_MIN_SECS, JOB_RECOVERY_GRACE_SECS,
    RECOVERY_SWEEP_INTERVAL_SECS, TRANSPORT_POLLING, TRANSPORT_QUIC, TRANSPORT_WEBSOCKET,
};
pub(crate) use registry::{
    now_ts, MAX_OUTPUT_BYTES, MAX_QUEUED_REQUESTS_PER_CLIENT, MAX_RETIRED_INSTANCES_PER_CLIENT,
};
pub use requests::EnqueueLspError;
pub use state::{ShellClientSemanticView, ShellJobVisibility};
pub use telemetry::{NoopRunnerRegistryTelemetry, RunnerRegistryTelemetry};
