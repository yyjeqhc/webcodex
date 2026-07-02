use super::auth::ShellClientAuthGroup;
use crate::shell_protocol::{
    AgentPolicySummary, ShellAgentProjectSummary, ShellAgentShellRequest, ShellClientCapabilities,
    ShellJobCodexMetadata, ShellRunResponse,
};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{oneshot, Notify};

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
    pub(super) capabilities: ShellClientCapabilities,
    pub(super) projects: Vec<ShellAgentProjectSummary>,
    pub(super) last_seen: i64,
    pub(super) agent_protocol_version: String,
    /// How this client is currently connected: `"polling"`, `"websocket"`,
    /// or `"quic"`.
    pub(super) transport: String,
    /// Sanitized agent policy summary reported at registration. `None` for
    /// older agents that did not report a policy. Exposed in
    /// `runtime_status` / `listAgents`; never carries token/env/init_script.
    pub(super) policy: Option<AgentPolicySummary>,
    /// Lightweight quick-start isolation group captured at registration. This
    /// is intentionally not exposed in `ShellClientView`.
    pub(super) auth_group: Option<ShellClientAuthGroup>,
}

#[derive(Debug)]
pub(super) struct PendingShellRequest {
    pub(super) request: ShellAgentShellRequest,
    pub(super) waiter: Option<oneshot::Sender<ShellRunResponse>>,
    pub(super) job_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(super) struct ShellJobRecord {
    pub(super) job_id: String,
    pub(super) request_id: Option<String>,
    pub(super) client_id: String,
    pub(super) kind: String,
    pub(super) project_id: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) command_preview: String,
    pub(super) status: String,
    pub(super) created_at: i64,
    pub(super) started_at: Option<i64>,
    pub(super) ended_at: Option<i64>,
    pub(super) exit_code: Option<i32>,
    pub(super) duration_ms: Option<u64>,
    pub(super) stdout: Option<String>,
    pub(super) stderr: Option<String>,
    pub(super) error: Option<String>,
    pub(super) codex: Option<ShellJobCodexMetadata>,
}

#[derive(Debug, Default)]
pub(super) struct ShellClientRegistryInner {
    pub(super) clients: HashMap<String, ShellClientRecord>,
    pub(super) pending_by_id: HashMap<String, PendingShellRequest>,
    pub(super) queues_by_client: HashMap<String, VecDeque<String>>,
    pub(super) jobs_by_id: HashMap<String, ShellJobRecord>,
    pub(super) request_to_job: HashMap<String, String>,
    /// Optional push notifiers for agents connected over a long-lived
    /// transport (WebSocket). When a request is enqueued for a client that
    /// has a registered notifier, the server pumps the request immediately
    /// instead of waiting for the agent to poll. Polling agents never
    /// register a notifier and are unaffected.
    ///
    /// The stored `agent_instance_id` records which agent process owns the
    /// notifier. On disconnect, the WebSocket handler passes its own instance
    /// id to `reconcile_disconnect`; the notifier (and running jobs) are only
    /// cleared when that id matches the stored one, so a stale disconnect
    /// cannot tear down a newer active instance's notifier.
    pub(super) notifiers: HashMap<String, NotifierEntry>,
}

/// A registered push notifier plus the agent instance id that installed it.
#[derive(Debug, Clone)]
pub(super) struct NotifierEntry {
    pub(super) notify: Arc<Notify>,
    pub(super) agent_instance_id: String,
}
