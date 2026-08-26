use super::auth::ShellClientAuthGroup;
use super::{AcceptedRunnerProtocol, AgentTransport, RunnerFeature, RunnerFeatureSet};
use crate::mcp_gateway::McpGatewayResponse;
use crate::shell_protocol::{
    AgentBuildInfo, AgentHostContext, AgentPolicySummary, PersistentShellResult,
    ShellAgentProjectSummary, ShellAgentShellRequest, ShellClientCapabilities, ShellClientView,
    ShellCommandExecutionState, ShellJobCodexMetadata, ShellJobStructuredExecutionMetadata,
    ShellJobValidationProgress, ShellProcessArgv, ShellProjectInventoryStatus, ShellRunResponse,
    JOB_INVENTORY_MAX_TERMINAL_JOBS, JOB_TERMINAL_RETENTION_SECS,
};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::{oneshot, watch, Notify};
use webcodex_core::coding_agent::{
    CodingAgentProvider, CodingAgentResponse, CodingAgentRunInventory,
};

#[derive(Debug, Clone)]
pub(super) struct ProjectInventoryStaging {
    pub(super) generation: String,
    pub(super) snapshot_sequence: u64,
    pub(super) total_reported: usize,
    pub(super) next_page_index: u32,
    pub(super) projects: Vec<ShellAgentProjectSummary>,
    pub(super) seen_ids: HashSet<String>,
    pub(super) serialized_bytes: usize,
    pub(super) started_at: i64,
}

#[derive(Debug, Clone)]
pub(super) struct ProjectInventoryState {
    pub(super) status: ShellProjectInventoryStatus,
    pub(super) staging: Option<ProjectInventoryStaging>,
    pub(super) retired_generations: VecDeque<String>,
    /// Monotonic freshness fence for the active Runner process. Reset only
    /// when `agent_instance_id` changes.
    pub(super) highest_snapshot_sequence: u64,
    pub(super) last_page_generation: Option<String>,
    pub(super) last_page_index: Option<u32>,
    pub(super) last_page_digest: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ShellClientRecord {
    pub(super) client_id: String,
    /// Active agent process identity (UUID). Replacing this value is the lease
    /// hand-off: once changed, the previous instance can no longer poll or
    /// submit results/job_updates.
    pub(super) agent_instance_id: String,
    pub(super) display_name: Option<String>,
    pub(super) owner: Option<String>,
    pub(super) hostname: Option<String>,
    /// Bounded Runner-configured planning metadata. This is not policy or live
    /// state and is replaced by each successful registration.
    pub(super) host_context: Option<AgentHostContext>,
    /// Accepted legacy wire snapshot retained only for wire-compatible public
    /// projection and diagnostics. Server capability authority lives in
    /// `runner_features` below.
    pub(super) capabilities: ShellClientCapabilities,
    /// Canonical Server capability truth normalized once from `capabilities`
    /// during registration. This set has no independent mutation path.
    pub(super) runner_features: RunnerFeatureSet,
    pub(super) projects: Vec<ShellAgentProjectSummary>,
    /// Authoritative project snapshot plus bounded in-progress staging. A
    /// staging failure never changes liveness or partially publishes projects.
    pub(super) project_inventory: ProjectInventoryState,
    pub(super) last_seen: i64,
    pub(super) agent_protocol_version: String,
    /// Supported generation + project-inventory semantics accepted once at
    /// registration ingress. Unsupported raw generation/label states cannot be
    /// represented in a successful record.
    pub(super) accepted_protocol: AcceptedRunnerProtocol,
    /// Authoritative transport from the concrete ingress path. External
    /// projections serialize this typed state as `polling`, `websocket`, or `quic`.
    pub(super) transport: AgentTransport,
    /// Sanitized agent policy summary reported at registration. `None` for
    /// older agents that did not report a policy. Exposed in
    /// `runtime_status` / `listAgents`; never carries token/env/init_script.
    pub(super) policy: Option<AgentPolicySummary>,
    /// Lightweight quick-start isolation group captured at registration. This
    /// is intentionally not exposed in `ShellClientView`.
    pub(super) auth_group: Option<ShellClientAuthGroup>,
    /// When the current agent instance first registered under this client_id.
    /// Preserved across same-instance re-registrations (transport reconnects).
    pub(super) registered_at: i64,
    /// When the current transport connection was established (latest register
    /// for this instance).
    pub(super) connected_at: i64,
    /// Server-generated lease for one concrete WebSocket/QUIC connection.
    /// Polling registrations use `None`. This is internal and prevents a late
    /// disconnect from an older same-instance transport from tearing down the
    /// newer connection.
    pub(super) connection_id: Option<String>,
    /// When the server observed the last transport disconnect for the current
    /// instance. Cleared on re-register.
    pub(super) disconnected_at: Option<i64>,
    /// Runner-reported process start timestamp (register payload).
    pub(super) process_started_at: Option<i64>,
    /// Runner-reported build identity (register payload).
    pub(super) build: Option<AgentBuildInfo>,
    /// Runner-reported effective static Job execution concurrency. This is
    /// safe operational metadata and remains unknown for older Runners.
    pub(super) job_concurrency_limit: Option<usize>,
    /// Sanitized startup-owned ACP providers for this exact Runner process.
    pub(super) coding_agent_providers: Vec<CodingAgentProvider>,
    /// Authoritative active/recent-terminal CodingAgentRun inventory from this
    /// Runner. Bodies/events are deliberately absent from this durable projection.
    pub(super) coding_agent_inventory: CodingAgentRunInventory,
    /// Same-Server evidence that a hidden structured terminal Job was already
    /// projected into its initiating tool result and deliberately discarded.
    /// This stays process-local and is preserved only across registrations by
    /// the same runner instance.
    pub(super) projected_structured_terminal_suppressions:
        VecDeque<ProjectedStructuredTerminalSuppression>,
}

/// Internal-only atomic observation of one active Runner record.
///
/// `view` preserves the existing compatibility/diagnostic projection while
/// feature decisions use the canonical set cloned from the same registry lock.
/// This type is never serialized or exposed through the wire protocol.
#[derive(Debug, Clone)]
pub(crate) struct ShellClientSemanticView {
    pub(crate) view: ShellClientView,
    pub(super) runner_features: RunnerFeatureSet,
}

impl ShellClientSemanticView {
    pub(crate) fn supports(&self, feature: RunnerFeature) -> bool {
        self.runner_features.supports(feature)
    }

    #[cfg(test)]
    pub(crate) fn from_public_view_for_test(view: ShellClientView) -> Self {
        let runner_features = RunnerFeatureSet::from_legacy_wire_for_test(&view.capabilities);
        Self {
            view,
            runner_features,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectedStructuredTerminalSuppression {
    pub(super) client_id: String,
    pub(super) agent_instance_id: String,
    pub(super) job_id: String,
    pub(super) request_id: String,
    pub(super) expires_at: i64,
}

impl ShellClientRecord {
    pub(super) fn prune_projected_structured_terminal_suppressions(&mut self, now: i64) {
        self.projected_structured_terminal_suppressions
            .retain(|suppression| suppression.expires_at > now);
    }

    pub(super) fn remember_projected_structured_terminal(
        &mut self,
        job_id: String,
        request_id: String,
        now: i64,
    ) {
        self.prune_projected_structured_terminal_suppressions(now);
        self.projected_structured_terminal_suppressions
            .retain(|suppression| {
                suppression.job_id != job_id || suppression.request_id != request_id
            });
        self.projected_structured_terminal_suppressions.push_back(
            ProjectedStructuredTerminalSuppression {
                client_id: self.client_id.clone(),
                agent_instance_id: self.agent_instance_id.clone(),
                job_id,
                request_id,
                expires_at: now.saturating_add(JOB_TERMINAL_RETENTION_SECS),
            },
        );
        while self.projected_structured_terminal_suppressions.len()
            > JOB_INVENTORY_MAX_TERMINAL_JOBS
        {
            self.projected_structured_terminal_suppressions.pop_front();
        }
    }

    pub(super) fn suppresses_projected_structured_terminal(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        job_id: &str,
        request_id: &str,
        now: i64,
    ) -> bool {
        self.projected_structured_terminal_suppressions
            .iter()
            .any(|suppression| {
                suppression.expires_at > now
                    && suppression.client_id == client_id
                    && suppression.agent_instance_id == agent_instance_id
                    && suppression.job_id == job_id
                    && suppression.request_id == request_id
            })
    }
}

#[derive(Debug)]
pub(super) struct PendingShellRequest {
    pub(super) request: ShellAgentShellRequest,
    pub(super) waiter: Option<oneshot::Sender<ShellRunResponse>>,
    pub(super) job_id: Option<String>,
    /// Optional Control-side project-placement fence for synchronous requests
    /// whose filesystem authority must still match the active registration at
    /// the instant the request is handed to the Runner.
    pub(super) expected_client_owner: Option<String>,
    pub(super) expected_project_id: Option<String>,
    pub(super) expected_project_cwd: Option<String>,
    /// Exact Runner process lease captured for an MCP gateway request. This is
    /// revalidated under the registry lock immediately before dequeue so a
    /// replacement Runner cannot consume stale bridge work.
    pub(super) expected_mcp_gateway_agent_instance_id: Option<String>,
    /// Exact provider lease captured with the Runner lease. Both logical id
    /// and opaque provider instance must still match registration immediately
    /// before dequeue.
    pub(super) expected_mcp_gateway_provider_id: Option<String>,
    pub(super) expected_mcp_gateway_provider_instance_id: Option<String>,
    pub(super) dispatched: bool,
}

#[derive(Debug, Clone)]
pub(super) struct CodingAgentDispatchFence {
    pub(super) agent_instance_id: String,
    pub(super) provider_id: String,
    pub(super) provider_instance_id: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ShellJobVisibility {
    #[default]
    Public,
    HiddenUntilHandoff,
    CleanupPending,
}

/// Server-process-local exact intent retained only to prove same-key detached
/// replays. It is never serialized into Runner protocol, durable state, audit,
/// or Session evidence; restart reconstruction deliberately restores `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DetachedIdempotencyIntent {
    pub(super) project_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) project_cwd: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) purpose: Option<String>,
    pub(super) shell: Option<String>,
    pub(super) process: ShellProcessArgv,
    pub(super) stdin: Option<String>,
    pub(super) timeout_secs: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ShellJobRecord {
    pub(super) job_id: String,
    pub(super) request_id: Option<String>,
    pub(super) client_id: String,
    /// Non-secret authorization partition captured when the Job is created.
    /// Shared-key runners store only the existing key hash group, never the
    /// plaintext key. Keeping this on the Job preserves authorization after
    /// the originating client registration is removed.
    pub(super) auth_group: Option<ShellClientAuthGroup>,
    /// Internal lease owner. Never exposed through public job tools.
    pub(super) agent_instance_id: String,
    pub(super) kind: String,
    pub(super) project_id: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) ssh_resource: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) project_cwd: Option<String>,
    pub(super) purpose: Option<String>,
    pub(super) shell: Option<String>,
    pub(super) command_preview: String,
    /// Exact detached replay intent is process-local only. Reconstructed Jobs
    /// keep this `None`, forcing exact logical-Job recovery instead of guessing
    /// that a resent body matches after Server restart.
    pub(super) detached_idempotency_intent: Option<DetachedIdempotencyIntent>,
    pub(super) status: String,
    pub(super) created_at: i64,
    pub(super) started_at: Option<i64>,
    pub(super) ended_at: Option<i64>,
    /// Server-process-local lifecycle clock: the first time this Server
    /// observed the Job in a terminal state. Runner-reported execution
    /// timestamps remain in `ended_at` for public results and diagnostics,
    /// but never control Server registry retention.
    pub(super) terminal_observed_at: Option<i64>,
    pub(super) exit_code: Option<i32>,
    pub(super) duration_ms: Option<u64>,
    pub(super) stdout: ShellJobLogState,
    pub(super) stderr: ShellJobLogState,
    pub(super) error: Option<String>,
    pub(super) command_execution_state: Option<ShellCommandExecutionState>,
    pub(super) structured_execution: Option<ShellJobStructuredExecutionMetadata>,
    pub(super) codex: Option<ShellJobCodexMetadata>,
    pub(super) validation_steps: Vec<String>,
    pub(super) validation: Option<crate::shell_protocol::ShellJobValidationMetadata>,
    pub(super) validation_progress: Option<ShellJobValidationProgress>,
    pub(super) visibility: ShellJobVisibility,
    pub(super) last_update_seq: u64,
    pub(super) recovery_state: Option<String>,
    pub(super) recovered_after_server_restart: bool,
    pub(super) reconciled_at: Option<i64>,
    pub(super) recovery_reason_code: Option<String>,
    pub(super) recovering_since: Option<i64>,
    pub(super) recovery_original_status: Option<String>,
    /// Server-owned generation for the complete public Job snapshot. Unlike
    /// `last_update_seq`, this also advances for accepted legacy/unsequenced
    /// updates and server-side recovery/status changes. It is intentionally
    /// process-local: waiters use it only while this record is alive, while the
    /// Runner-owned sequence retains its protocol and restart semantics.
    /// Process-local server epoch used to invalidate observation tokens after restart.
    pub(super) observation_epoch: Arc<str>,
    pub(super) public_revision: Arc<AtomicU64>,
    /// Observer update notifier. Shared with every snapshot of the record and
    /// notified (broadcast) whenever anything that changes the public Job
    /// snapshot or `last_update_seq` happens: an accepted `agent:job_update`,
    /// log/validation progress, terminal transitions, disconnect/recovery,
    /// reconciliation, or a stop request that changed status. Bounded
    /// `job_log`/`job_tail` waiters re-check the authoritative snapshot after
    /// every wake, so spurious wakes are harmless. Never persisted.
    pub(super) update_notify: Arc<Notify>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ShellJobLogState {
    pub(super) tail: String,
    pub(super) first_retained_line: usize,
    pub(super) next_line: usize,
    pub(super) truncated: bool,
}

impl Default for ShellJobLogState {
    fn default() -> Self {
        Self {
            tail: String::new(),
            first_retained_line: 1,
            next_line: 1,
            truncated: false,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct ShellClientRegistryInner {
    pub(super) clients: HashMap<String, ShellClientRecord>,
    pub(super) pending_by_id: HashMap<String, PendingShellRequest>,
    /// Waiters for explicit persistent-shell lifecycle results. Kept separate
    /// from synchronous `ShellRunResponse` waiters so PersistentShell never
    /// enters the Job/run_shell model.
    pub(super) persistent_waiters: HashMap<String, oneshot::Sender<PersistentShellResult>>,
    /// Waiters for the closed MCP gateway response contract. They remain
    /// separate from shell stdout/stderr so bridge calls cannot become a raw
    /// result tunnel.
    pub(super) mcp_gateway_waiters: HashMap<String, oneshot::Sender<McpGatewayResponse>>,
    /// Waiters and exact process/provider dispatch fences for CodingAgentRun
    /// operations. They are independent from shell/Job/MCP result channels.
    pub(super) coding_agent_waiters: HashMap<String, oneshot::Sender<CodingAgentResponse>>,
    pub(super) coding_agent_fences: HashMap<String, CodingAgentDispatchFence>,
    pub(super) queues_by_client: HashMap<String, VecDeque<String>>,
    pub(super) jobs_by_id: HashMap<String, ShellJobRecord>,
    pub(super) request_to_job: HashMap<String, String>,
    /// Bounded stale-instance tombstones prevent a replaced runner process
    /// from reclaiming the same client lease after the replacement later
    /// becomes stale.
    pub(super) retired_instances: HashMap<String, VecDeque<String>>,
    /// Runtime project ids temporarily fenced while unregister validates and
    /// removes the Agent registry entry. Job enqueue checks this set while
    /// holding the same registry mutex, closing the check/start TOCTOU window.
    pub(super) unregistering_projects: HashMap<String, usize>,
    /// Optional push notifiers for agents connected over a long-lived
    /// transport (WebSocket/QUIC). When a request is enqueued for a client that
    /// has a registered notifier, the server pumps the request immediately
    /// instead of waiting for the agent to poll. Polling agents never
    /// register a notifier and are unaffected.
    ///
    /// The stored instance and connection ids record which concrete transport
    /// owns the notifier. Disconnect cleanup is applied only when both leases
    /// still match, so neither a replaced process nor an older same-process
    /// socket can tear down the current notifier and jobs.
    pub(super) notifiers: HashMap<String, NotifierEntry>,
}

/// A registered push notifier plus the exact streaming connection lifecycle
/// that installed it. `cancel` is process-local and is signalled only after a
/// successful authoritative replacement has committed.
#[derive(Debug, Clone)]
pub(super) struct NotifierEntry {
    pub(super) notify: Arc<Notify>,
    pub(super) cancel: watch::Sender<bool>,
    pub(super) agent_instance_id: String,
    pub(super) connection_id: Option<String>,
}
