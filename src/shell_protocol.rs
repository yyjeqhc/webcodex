use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_shell_true() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    120
}

fn default_wait_timeout_secs() -> u64 {
    30
}

fn default_agent_request_kind() -> String {
    "run_shell".to_string()
}

fn default_shell_job_kind() -> String {
    "shell".to_string()
}

/// Default `agent_protocol_version` used when a client registers without
/// declaring one. Old agents that predate the version field show up as
/// `"unknown"` so operators can distinguish them from agents that explicitly
/// announce `"polling-v1"`.
fn default_agent_protocol_version() -> String {
    "unknown".to_string()
}

/// Default `transport` for `ShellClientView` when deserializing views that
/// predate the transport field (e.g. older snapshots). Polling is the legacy
/// default.
fn default_transport_polling() -> String {
    "polling".to_string()
}

/// Protocol version announced by current `webcodex-agent` builds. Used by
/// the `webcodex-agent` binary target; allowed dead-code here because the
/// main server binary does not reference it directly.
#[allow(dead_code)]
pub const AGENT_PROTOCOL_VERSION_POLLING_V1: &str = "polling-v1";

/// Protocol version announced by `webcodex-agent` builds that connect over
/// WebSocket. Kept in the shared protocol module so both the server and the
/// agent binary reference the same literal.
#[allow(dead_code)]
pub const AGENT_PROTOCOL_VERSION_WEBSOCKET_V1: &str = "websocket-v1";

/// Protocol version announced by `webcodex-agent` builds that connect over the
/// custom QUIC stream transport. Kept in the shared protocol module so the
/// server and the agent binary reference the same literal.
///
/// `quic-v1` is the custom QUIC stream transport protocol version. It uses the
/// same `AgentEnvelope` model for registration, keepalive, request dispatch,
/// results, and job updates. The transport label remains `"quic"` (see
/// `TRANSPORT_QUIC`).
#[allow(dead_code)]
pub const AGENT_PROTOCOL_VERSION_QUIC_V1: &str = "quic-v1";

pub const SHELL_CLIENT_CAPABILITY_SHELL: &str = "shell";
pub const SHELL_CLIENT_CAPABILITY_FILE_READ: &str = "file_read";
pub const SHELL_CLIENT_CAPABILITY_FILE_WRITE: &str = "file_write";
pub const SHELL_CLIENT_CAPABILITY_GIT: &str = "git";
pub const SHELL_CLIENT_CAPABILITY_JOBS: &str = "jobs";
pub const SHELL_CLIENT_CAPABILITY_ASYNC_JOBS: &str = "async_jobs";
pub const SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS: &str = "async_shell_jobs";
/// Explicit capability for agent-side read-only LSP navigation. Missing on
/// older agents and defaults to `false` so the server never dispatches typed
/// LSP requests to agents that cannot handle them.
pub const SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION: &str = "lsp_read_only_navigation";
#[cfg(test)]
pub const SHELL_CLIENT_CAPABILITY_NAMES: &[&str] = &[
    SHELL_CLIENT_CAPABILITY_SHELL,
    SHELL_CLIENT_CAPABILITY_FILE_READ,
    SHELL_CLIENT_CAPABILITY_FILE_WRITE,
    SHELL_CLIENT_CAPABILITY_GIT,
    SHELL_CLIENT_CAPABILITY_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientCapabilities {
    #[serde(default = "default_shell_true")]
    pub shell: bool,
    #[serde(default)]
    pub file_read: bool,
    #[serde(default)]
    pub file_write: bool,
    #[serde(default)]
    pub git: bool,
    #[serde(default)]
    pub jobs: bool,
    #[serde(default)]
    pub async_jobs: bool,
    #[serde(default)]
    pub async_shell_jobs: bool,
    /// Read-only semantic navigation via agent-side rust-analyzer. Defaults to
    /// false for wire compatibility with older agents.
    #[serde(default)]
    pub lsp_read_only_navigation: bool,
}

impl Default for ShellClientCapabilities {
    fn default() -> Self {
        Self {
            shell: true,
            file_read: false,
            file_write: false,
            git: false,
            jobs: false,
            async_jobs: false,
            async_shell_jobs: false,
            lsp_read_only_navigation: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentProjectSummary {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub path: String,
    #[serde(default = "default_shell_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_head: Option<String>,
    #[serde(default)]
    pub git_dirty: Option<bool>,
    pub updated_at: i64,
    /// Project-bound shell profile name (`project.shell_profile`). Non-secret:
    /// just a profile name. `None` means the project did not override the
    /// profile, so the agent falls back to `shell.default_profile`. Carried so
    /// `listProjects` / `runtime_status` can show which profile a project uses
    /// without exposing env values or init_script contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_profile: Option<String>,
}

/// Sanitized summary of one configured shell profile. Exposes ONLY safe
/// metadata: whether an init_script is set (boolean, never the body), the
/// number of env keys (never the values), the resolved program, and the arg
/// count. Used by `ShellProfilesSummary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellProfileSummaryEntry {
    pub name: String,
    pub has_init_script: bool,
    pub env_keys_count: usize,
    pub program: String,
    pub args_count: usize,
}

/// Sanitized summary of an agent's prepared-shell-profile configuration.
/// Reported by the agent at registration (carried inside `AgentPolicySummary`)
/// and exposed in `runtime_status` / `listAgents` / `listProjects` so users can
/// see which profiles are configured and which one a project resolves to.
///
/// This summary NEVER includes: init_script bodies, env values, tokens,
/// Authorization headers, full agent.toml, the full env snapshot, or stderr
/// tails. `prepared_cache_count` reflects the number of prepared snapshots at
/// the last registration (snapshots are prepared lazily on first use, so this
/// is typically 0 right after agent start; it is not a live counter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellProfilesSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    pub configured_count: usize,
    pub prepared_cache_count: usize,
    pub profiles: Vec<ShellProfileSummaryEntry>,
}

/// Sanitized agent policy summary. Carried in the registration payload and
/// exposed in `runtime_status` / `listAgents`. Contains ONLY non-secret
/// fields: it never includes the agent token, shell env values, init_script
/// contents, or full agent.toml contents. `allowed_roots` is intentionally
/// exposed as a path-policy summary. `shell_profiles` carries the sanitized
/// prepared-shell-profile configuration summary (profile names, default
/// profile, counts) so observability can show which profile a project uses;
/// it never carries env values or init_script bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicySummary {
    #[serde(default = "default_shell_true")]
    pub allow_raw_shell: bool,
    #[serde(default)]
    pub allow_cwd_anywhere: bool,
    #[serde(default)]
    pub allowed_roots: Vec<PathBuf>,
    #[serde(default = "default_policy_max_timeout_secs")]
    pub max_timeout_secs: u64,
    #[serde(default = "default_policy_max_output_bytes")]
    pub max_output_bytes: usize,
    /// Sanitized prepared-shell-profile summary. `None` for older agents that
    /// did not report one. Never carries env values or init_script bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_profiles: Option<ShellProfilesSummary>,
}

impl Default for AgentPolicySummary {
    fn default() -> Self {
        Self {
            allow_raw_shell: true,
            allow_cwd_anywhere: false,
            allowed_roots: Vec::new(),
            max_timeout_secs: default_policy_max_timeout_secs(),
            max_output_bytes: default_policy_max_output_bytes(),
            shell_profiles: None,
        }
    }
}

fn default_policy_max_timeout_secs() -> u64 {
    3600
}

fn default_policy_max_output_bytes() -> usize {
    256 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientRegisterRequest {
    pub client_id: String,
    /// Stable per-process identity for the registering agent. Generated once
    /// by `webcodex-agent` at startup and reused for the whole process
    /// lifetime (including WebSocket reconnects). The server treats this as
    /// the active agent lease identity: a second agent process with the same
    /// `client_id` but a different `agent_instance_id` is rejected while the
    /// first is online, and a stale/replaced instance can no longer poll or
    /// submit results. It is not a secret.
    pub agent_instance_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub capabilities: Option<ShellClientCapabilities>,
    #[serde(default)]
    pub projects: Option<Vec<ShellAgentProjectSummary>>,
    /// Protocol version announced by the agent during registration. Older
    /// agents that omit this field are treated as `"unknown"` by the server.
    #[serde(default)]
    pub agent_protocol_version: Option<String>,
    /// Sanitized agent policy summary. Older agents that omit this field
    /// register with `None`; `runtime_status` / `listAgents` then expose
    /// `null` for the policy so older/minimal payloads stay compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<AgentPolicySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientView {
    pub client_id: String,
    /// Active agent process identity (UUID) for this client. Empty for views
    /// that predate the instance id field. Not a secret.
    #[serde(default)]
    pub agent_instance_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    pub status: String,
    pub connected: bool,
    pub last_seen: i64,
    pub capabilities: ShellClientCapabilities,
    pub pending_requests: usize,
    #[serde(default)]
    pub projects: Vec<ShellAgentProjectSummary>,
    /// Agent-announced protocol version. Defaults to `"unknown"` for agents
    /// that registered before this field existed.
    #[serde(default = "default_agent_protocol_version")]
    pub agent_protocol_version: String,
    /// Transport the agent is currently connected over: `"polling"`,
    /// `"websocket"`, or `"quic"`. Defaults to `"polling"` for older
    /// agents/views.
    #[serde(default = "default_transport_polling")]
    pub transport: String,
    /// Sanitized agent policy summary reported at registration. `None`
    /// (serialized as `null`/omitted) for older agents that did not report a
    /// policy. Never contains token/env/init_script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<AgentPolicySummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellClientRegisterResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ShellClientView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunRequest {
    pub client_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_wait_timeout_secs")]
    pub wait_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunResponse {
    pub success: bool,
    pub request_id: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub command_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentPollRequest {
    pub client_id: String,
    /// Active agent process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    pub agent_instance_id: String,
    #[serde(default)]
    pub projects: Option<Vec<ShellAgentProjectSummary>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobStatusRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobLogRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
    #[serde(default)]
    pub tail_lines: Option<usize>,
    #[serde(default)]
    pub since_stdout_line: Option<usize>,
    #[serde(default)]
    pub since_stderr_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobStopRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobsListRequest {
    pub client_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentShellRequest {
    pub request_id: String,
    pub client_id: String,
    #[serde(default = "default_agent_request_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default)]
    pub create_dirs: bool,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    pub timeout_secs: u64,
    pub requested_by: String,
    pub created_at: i64,
    /// Typed read-only LSP navigation payload. Present only for `kind = "lsp"`.
    /// Defaults to `None` so older request bodies continue to deserialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsp: Option<crate::lsp_bridge::AgentLspPayload>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentPollResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<ShellAgentShellRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentResultRequest {
    pub client_id: String,
    /// Active agent process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    pub agent_instance_id: String,
    pub request_id: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentResultResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentJobUpdateRequest {
    pub client_id: String,
    /// Active agent process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    pub agent_instance_id: String,
    pub job_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
    pub status: String,
    #[serde(default)]
    pub stdout_chunk: Option<String>,
    #[serde(default)]
    pub stderr_chunk: Option<String>,
    #[serde(default)]
    pub stdout_tail: Option<String>,
    #[serde(default)]
    pub stderr_tail: Option<String>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub finished: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentJobUpdateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFileOpRequest {
    pub op: String,
    pub client_id: String,
    pub path: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    #[serde(default)]
    pub old_text: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub expected_prefix: Option<String>,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub line: Option<usize>,
    #[serde(default)]
    pub create_dirs: bool,
    #[serde(default = "default_wait_timeout_secs")]
    pub wait_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFileOpResponse {
    pub success: bool,
    pub op: String,
    pub request_id: String,
    pub client_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobCodexMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runtime_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobOpRequest {
    pub op: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub since_stdout_line: Option<usize>,
    #[serde(default)]
    pub since_stderr_line: Option<usize>,
    #[serde(default)]
    pub tail_lines: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ShellJobCodexMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentShellJobResult {
    #[serde(default)]
    pub cwd: Option<String>,
    pub command_preview: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentJobResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellAgentShellJobResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobInfo {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub client_id: String,
    #[serde(default = "default_shell_job_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub command_preview: String,
    pub status: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ShellJobCodexMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ShellAgentJobResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobOpResponse {
    pub success: bool,
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stdout_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stderr_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobStatusResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<ShellAgentJobResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobLogResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stdout_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stderr_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobStopResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobsListResponse {
    pub success: bool,
    pub client_id: String,
    pub jobs: Vec<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// Transport-neutral agent message envelope
// ============================================================================
//
// A single message format used by the WebSocket agent transport (and future
// QUIC transport). It wraps the existing polling protocol payloads so the
// server and agent never duplicate business logic: register/request/result/
// job_update reuse the same structs as the HTTP polling endpoints.
//
// Wire format is JSON with an internal `type` tag:
//
//   {"type":"register","client_id":"...","projects":[...]}
//   {"type":"registered","success":true,"client":{...}}
//   {"type":"request","request_id":"...","client_id":"...","kind":"run_shell",...}
//   {"type":"result","client_id":"...","request_id":"...","exit_code":0,...}
//   {"type":"job_update","client_id":"...","job_id":"...","status":"running",...}
//   {"type":"ping","ts":1700000000}
//   {"type":"pong","ts":1700000000}
//   {"type":"goodbye","reason":"shutdown"}
//   {"type":"error","code":"bad_request","message":"..."}
//
// The envelope is transport-neutral: it carries no WebSocket-specific fields
// and could be framed over QUIC streams unchanged.

/// One agent transport message. Used by both the server WebSocket handler and
/// the `webcodex-agent` WebSocket client mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEnvelope {
    /// Agent -> server. First message after the WebSocket handshake. Carries
    /// the same payload as `POST /api/shell/agent/register`.
    Register {
        #[serde(flatten)]
        payload: ShellClientRegisterRequest,
        /// Optional agent/bearer token carried inline. Used ONLY by the QUIC
        /// transport, which has no HTTP middleware to inject an
        /// `Authorization` header. WebSocket always leaves this `None` (auth
        /// is enforced by `AuthMiddleware` on the HTTP handshake) and the
        /// server ignores it on that path. The field is
        /// `skip_serializing_if = None` so the WebSocket wire format is
        /// byte-identical to before. Never logged: the QUIC handler reads it
        /// once and drops it before any tracing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        auth_token: Option<String>,
    },
    /// Server -> agent. Acknowledgement of `Register`.
    Registered {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        client: Option<ShellClientView>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Server -> agent. A pending shell/file/job request pushed to the agent.
    /// Same payload as the `request` field of the polling response.
    Request {
        #[serde(flatten)]
        request: ShellAgentShellRequest,
    },
    /// Agent -> server. Result of a synchronous shell/file request. Same
    /// payload as `POST /api/shell/agent/result`.
    Result {
        #[serde(flatten)]
        payload: ShellAgentResultRequest,
    },
    /// Agent -> server. Incremental or final update for an async job. Same
    /// payload as `POST /api/shell/agent/job_update`.
    JobUpdate {
        #[serde(flatten)]
        payload: ShellAgentJobUpdateRequest,
    },
    /// Either direction. Liveness keepalive.
    Ping { ts: i64 },
    /// Either direction. Reply to `Ping`.
    Pong { ts: i64 },
    /// Agent -> server. Best-effort graceful shutdown notice. Older agents do
    /// not send this frame; transports still reconcile on observed disconnect.
    Goodbye {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Server -> agent. Fatal protocol error; the agent should reconnect.
    Error { code: String, message: String },
}

impl AgentEnvelope {
    /// Short discriminator string for a variant, e.g. `"register"`. Useful
    /// for logging and tests.
    #[allow(dead_code)]
    pub fn kind(&self) -> &'static str {
        match self {
            AgentEnvelope::Register { .. } => "register",
            AgentEnvelope::Registered { .. } => "registered",
            AgentEnvelope::Request { .. } => "request",
            AgentEnvelope::Result { .. } => "result",
            AgentEnvelope::JobUpdate { .. } => "job_update",
            AgentEnvelope::Ping { .. } => "ping",
            AgentEnvelope::Pong { .. } => "pong",
            AgentEnvelope::Goodbye { .. } => "goodbye",
            AgentEnvelope::Error { .. } => "error",
        }
    }

    /// Encode the envelope as a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Decode an envelope from a JSON byte slice.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

// ============================================================================
// QUIC length-prefixed frame codec
// ============================================================================
//
// The custom QUIC agent transport frames each [`AgentEnvelope`] as:
//
//   u32_be length (big-endian)
//   JSON bytes
//
// Length-prefixing (rather than newline-delimited JSON) avoids boundary
// problems when a payload contains embedded newlines. The codec lives in this
// shared module so the server (`agent_quic.rs`) and the `webcodex-agent`
// binary (which inlines this file) use byte-identical framing.
//
// This is a custom QUIC *stream* transport, NOT HTTP/3. It is transport-
// neutral framing over a single QUIC bidirectional stream.

/// Maximum frame body size. Matches the WebSocket `WS_MAX_MESSAGE_SIZE` head
/// room and the registry output cap; bounds memory per peer.
pub const QUIC_FRAME_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Errors produced by the QUIC frame codec.
#[derive(Debug)]
pub enum QuicFrameError {
    /// Underlying I/O error reading/writing the stream.
    Io(std::io::Error),
    /// JSON encode/decode failure.
    Json(serde_json::Error),
    /// Announced frame length exceeds `QUIC_FRAME_MAX_BYTES`. `len` is the
    /// announced (attacker-controlled) length; rejected before allocation.
    Oversized { len: usize, max: usize },
    /// The peer closed the stream cleanly before any frame was read.
    EmptyStream,
    /// A frame header announced a length but the body was short / invalid.
    Malformed(&'static str),
}

impl std::fmt::Display for QuicFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuicFrameError::Io(e) => write!(f, "quic frame io error: {}", e),
            QuicFrameError::Json(e) => write!(f, "quic frame json error: {}", e),
            QuicFrameError::Oversized { len, max } => write!(
                f,
                "quic frame oversized: announced {} bytes, max {}",
                len, max
            ),
            QuicFrameError::EmptyStream => write!(f, "quic stream closed before any frame"),
            QuicFrameError::Malformed(msg) => write!(f, "quic frame malformed: {}", msg),
        }
    }
}

impl std::error::Error for QuicFrameError {}

/// Encode an envelope as a length-prefixed frame: `u32_be(len) || json`.
pub fn encode_quic_frame(env: &AgentEnvelope) -> Result<Vec<u8>, QuicFrameError> {
    let json = serde_json::to_vec(env).map_err(QuicFrameError::Json)?;
    // u32 cap is far above QUIC_FRAME_MAX_BYTES, but guard anyway so a
    // pathological payload can never overflow the length prefix.
    if json.len() > QUIC_FRAME_MAX_BYTES {
        return Err(QuicFrameError::Oversized {
            len: json.len(),
            max: QUIC_FRAME_MAX_BYTES,
        });
    }
    let len = u32::try_from(json.len()).expect("checked against MAX");
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Write a single length-prefixed frame to an async sink.
pub async fn write_quic_frame<W>(w: &mut W, env: &AgentEnvelope) -> Result<(), QuicFrameError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let buf = encode_quic_frame(env)?;
    w.write_all(&buf).await.map_err(QuicFrameError::Io)?;
    Ok(())
}

/// Read a single length-prefixed frame from an async source and decode it.
pub async fn read_quic_frame<R>(r: &mut R) -> Result<AgentEnvelope, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(QuicFrameError::EmptyStream);
        }
        Err(e) => return Err(QuicFrameError::Io(e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > QUIC_FRAME_MAX_BYTES {
        // Reject before allocating. `len` is peer-controlled.
        return Err(QuicFrameError::Oversized {
            len,
            max: QUIC_FRAME_MAX_BYTES,
        });
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            QuicFrameError::Malformed("announced frame length but stream ended early")
        } else {
            QuicFrameError::Io(e)
        }
    })?;
    AgentEnvelope::from_slice(&buf).map_err(QuicFrameError::Json)
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    fn sample_register() -> ShellClientRegisterRequest {
        ShellClientRegisterRequest {
            client_id: "ws-1".to_string(),
            agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
            display_name: Some("WS Agent".to_string()),
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                file_write: false,
                git: false,
                jobs: true,
                async_jobs: true,
                async_shell_jobs: true,
                lsp_read_only_navigation: false,
            }),
            projects: None,
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1.to_string()),
            policy: None,
        }
    }

    #[test]
    fn register_envelope_round_trips_with_type_tag() {
        let env = AgentEnvelope::Register {
            payload: sample_register(),
            auth_token: None,
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"register""#), "json was: {json}");
        assert!(json.contains(r#""client_id":"ws-1""#));
        // WebSocket sets auth_token=None; the field must not appear on the
        // wire so the websocket format is unchanged.
        assert!(!json.contains(r#""auth_token""#), "json was: {json}");
        let back = AgentEnvelope::from_slice(json.as_bytes()).unwrap();
        match back {
            AgentEnvelope::Register { payload, .. } => {
                assert_eq!(payload.client_id, "ws-1");
                assert_eq!(
                    payload.agent_protocol_version.as_deref(),
                    Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1),
                );
                let caps = payload.capabilities.expect("capabilities");
                assert!(caps.shell);
                assert!(!caps.file_write);
            }
            other => panic!("expected register, got {:?}", other.kind()),
        }
    }

    #[test]
    fn request_envelope_flattens_shell_request_fields() {
        let request = ShellAgentShellRequest {
            request_id: "req-1".to_string(),
            client_id: "ws-1".to_string(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: Some("/tmp".to_string()),
            path: None,
            content: None,
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: "echo hi".to_string(),
            stdin: Some("input".to_string()),
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 123,
            lsp: None,
        };
        let env = AgentEnvelope::Request { request };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"request""#));
        assert!(json.contains(r#""request_id":"req-1""#));
        assert!(json.contains(r#""kind":"run_shell""#));
        assert!(json.contains(r#""command":"echo hi""#));
        assert!(json.contains(r#""stdin":"input""#));
        let back = AgentEnvelope::from_slice(json.as_bytes()).unwrap();
        match back {
            AgentEnvelope::Request { request } => {
                assert_eq!(request.request_id, "req-1");
                assert_eq!(request.command, "echo hi");
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }
    }

    #[test]
    fn result_and_job_update_envelopes_round_trip() {
        let result_env = AgentEnvelope::Result {
            payload: ShellAgentResultRequest {
                client_id: "ws-1".to_string(),
                agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                request_id: "req-1".to_string(),
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(5),
                error: None,
            },
        };
        let json = result_env.to_json().unwrap();
        assert!(json.contains(r#""type":"result""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Result { payload } => assert_eq!(payload.exit_code, Some(0)),
            other => panic!("expected result, got {:?}", other.kind()),
        }

        let job_env = AgentEnvelope::JobUpdate {
            payload: ShellAgentJobUpdateRequest {
                client_id: "ws-1".to_string(),
                agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                job_id: "job-1".to_string(),
                request_id: Some("req-1".to_string()),
                status: "running".to_string(),
                stdout_chunk: Some("out".to_string()),
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                finished: false,
            },
        };
        let json = job_env.to_json().unwrap();
        assert!(json.contains(r#""type":"job_update""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::JobUpdate { payload } => assert_eq!(payload.job_id, "job-1"),
            other => panic!("expected job_update, got {:?}", other.kind()),
        }
    }

    #[test]
    fn ping_pong_error_envelopes_round_trip() {
        let ping = AgentEnvelope::Ping { ts: 1700000000 };
        let json = ping.to_json().unwrap();
        assert_eq!(json, r#"{"type":"ping","ts":1700000000}"#);
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Ping { ts } => assert_eq!(ts, 1700000000),
            other => panic!("expected ping, got {:?}", other.kind()),
        }

        let err = AgentEnvelope::Error {
            code: "bad_request".to_string(),
            message: "nope".to_string(),
        };
        let json = err.to_json().unwrap();
        assert!(json.contains(r#""type":"error""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Error { code, message } => {
                assert_eq!(code, "bad_request");
                assert_eq!(message, "nope");
            }
            other => panic!("expected error, got {:?}", other.kind()),
        }
    }

    #[test]
    fn goodbye_envelope_round_trips_and_reason_is_optional() {
        let env = AgentEnvelope::Goodbye {
            reason: Some("shutdown".to_string()),
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"goodbye""#));
        assert!(json.contains(r#""reason":"shutdown""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Goodbye { reason } => assert_eq!(reason.as_deref(), Some("shutdown")),
            other => panic!("expected goodbye, got {:?}", other.kind()),
        }

        let env = AgentEnvelope::Goodbye { reason: None };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"goodbye""#));
        assert!(!json.contains(r#""reason""#));
        assert!(matches!(
            AgentEnvelope::from_slice(json.as_bytes()).unwrap(),
            AgentEnvelope::Goodbye { reason: None }
        ));
    }

    #[test]
    fn invalid_envelope_type_is_rejected() {
        let json = r#"{"type":"not_a_real_variant"}"#;
        assert!(AgentEnvelope::from_slice(json.as_bytes()).is_err());
    }

    #[test]
    fn registered_envelope_omits_none_fields() {
        let env = AgentEnvelope::Registered {
            success: true,
            client: None,
            error: None,
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"registered""#));
        assert!(json.contains(r#""success":true"#));
        // client/error are skip_serializing_if None.
        assert!(!json.contains(r#""client""#));
        assert!(!json.contains(r#""error""#));
    }

    #[test]
    fn register_request_round_trips_agent_instance_id() {
        let req = sample_register();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""agent_instance_id":"11111111-1111-1111-1111-111111111111""#));
        let back: ShellClientRegisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "11111111-1111-1111-1111-111111111111"
        );
    }

    #[test]
    fn register_request_without_agent_instance_id_is_rejected() {
        // An old agent that omits agent_instance_id must be rejected at
        // deserialization: the field is now required for correctness.
        let json = r#"{
            "client_id": "oe",
            "capabilities": {"shell": true}
        }"#;
        let err = serde_json::from_str::<ShellClientRegisterRequest>(json);
        assert!(err.is_err(), "missing agent_instance_id must be rejected");
    }

    #[test]
    fn poll_result_job_update_round_trip_agent_instance_id() {
        let poll = ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
            projects: None,
        };
        let json = serde_json::to_string(&poll).unwrap();
        let back: ShellAgentPollRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "22222222-2222-2222-2222-222222222222"
        );

        let result = ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
            request_id: "req-1".to_string(),
            exit_code: Some(0),
            stdout: None,
            stderr: None,
            duration_ms: None,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ShellAgentResultRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "22222222-2222-2222-2222-222222222222"
        );

        let job = ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
            job_id: "job-1".to_string(),
            request_id: None,
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            finished: false,
        };
        let json = serde_json::to_string(&job).unwrap();
        let back: ShellAgentJobUpdateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "22222222-2222-2222-2222-222222222222"
        );
    }

    #[test]
    fn poll_result_job_update_without_agent_instance_id_are_rejected() {
        assert!(serde_json::from_str::<ShellAgentPollRequest>(r#"{"client_id":"oe"}"#).is_err());
        assert!(serde_json::from_str::<ShellAgentResultRequest>(
            r#"{"client_id":"oe","request_id":"r1"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ShellAgentJobUpdateRequest>(
            r#"{"client_id":"oe","job_id":"j1","status":"running"}"#
        )
        .is_err());
    }

    #[test]
    fn quic_frame_encode_prefixes_u32_be_length() {
        let env = AgentEnvelope::Ping { ts: 42 };
        let frame = encode_quic_frame(&env).unwrap();
        // First 4 bytes are the big-endian JSON length.
        let len = u32::from_be_bytes(frame[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, frame.len() - 4);
        // The body is valid JSON containing the ping.
        let body = &frame[4..];
        assert!(std::str::from_utf8(body)
            .unwrap()
            .contains(r#""type":"ping""#));
    }

    #[tokio::test]
    async fn quic_frame_round_trips_through_read_write() {
        use tokio::io::AsyncReadExt;
        let env = AgentEnvelope::Pong { ts: 99 };
        let mut buf: Vec<u8> = Vec::new();
        write_quic_frame(&mut buf, &env).await.unwrap();
        // Drain the written bytes through a slice reader.
        let mut reader: &[u8] = &buf;
        let back = read_quic_frame(&mut reader).await.unwrap();
        assert!(matches!(back, AgentEnvelope::Pong { ts: 99 }));
        // The stream is fully consumed.
        let mut tail = Vec::new();
        let n = reader.read_to_end(&mut tail).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn quic_frame_rejects_oversized_announced_length() {
        // Craft a header announcing a length far above the cap, followed by a
        // tiny body. The codec must reject *before* allocating/reading the
        // announced body.
        let huge = (QUIC_FRAME_MAX_BYTES as u32 + 1).to_be_bytes();
        let mut bad: Vec<u8> = Vec::new();
        bad.extend_from_slice(&huge);
        bad.extend_from_slice(b"{}");
        let mut reader: &[u8] = &bad;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        match err {
            QuicFrameError::Oversized { len, max } => {
                assert_eq!(len, QUIC_FRAME_MAX_BYTES + 1);
                assert_eq!(max, QUIC_FRAME_MAX_BYTES);
            }
            other => panic!("expected Oversized, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn quic_frame_rejects_malformed_body_and_short_stream() {
        // Announced length says 5 bytes but the body is invalid JSON.
        let mut bad: Vec<u8> = Vec::new();
        bad.extend_from_slice(&5u32.to_be_bytes());
        bad.extend_from_slice(b"notjs");
        let mut reader: &[u8] = &bad;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, QuicFrameError::Json(_)), "got {err:?}");

        // Announced length says 10 bytes but the stream ends after 2.
        let mut short: Vec<u8> = Vec::new();
        short.extend_from_slice(&10u32.to_be_bytes());
        short.extend_from_slice(b"ab");
        let mut reader: &[u8] = &short;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, QuicFrameError::Malformed(_)), "got {err:?}");

        // Empty stream -> EmptyStream, not Malformed.
        let empty: Vec<u8> = Vec::new();
        let mut reader: &[u8] = &empty;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, QuicFrameError::EmptyStream), "got {err:?}");
    }

    #[test]
    fn register_envelope_with_auth_token_serializes_when_some() {
        // QUIC sets auth_token=Some; the field appears on the wire so the
        // server can authenticate the agent. WebSocket leaves it None (tested
        // separately) so its wire format is unchanged.
        let env = AgentEnvelope::Register {
            payload: ShellClientRegisterRequest {
                client_id: "q-1".to_string(),
                agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
                policy: None,
            },
            auth_token: Some("wc_agent_secret".to_string()),
        };
        let json = env.to_json().unwrap();
        assert!(
            json.contains(r#""auth_token":"wc_agent_secret""#),
            "json was: {json}"
        );
        let back = AgentEnvelope::from_slice(json.as_bytes()).unwrap();
        match back {
            AgentEnvelope::Register { auth_token, .. } => {
                assert_eq!(auth_token.as_deref(), Some("wc_agent_secret"));
            }
            other => panic!("expected register, got {:?}", other.kind()),
        }
    }
}
