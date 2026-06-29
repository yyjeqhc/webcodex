use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::shell_protocol::{
    AgentPolicySummary, ShellAgentJobResult, ShellAgentJobUpdateRequest,
    ShellAgentJobUpdateResponse, ShellAgentPollRequest, ShellAgentPollResponse,
    ShellAgentProjectSummary, ShellAgentResultRequest, ShellAgentResultResponse,
    ShellAgentShellJobResult, ShellAgentShellRequest, ShellClientCapabilities,
    ShellClientJobLogRequest, ShellClientJobLogResponse, ShellClientJobStatusRequest,
    ShellClientJobStatusResponse, ShellClientJobStopRequest, ShellClientJobStopResponse,
    ShellClientJobsListRequest, ShellClientJobsListResponse, ShellClientRegisterRequest,
    ShellClientRegisterResponse, ShellClientView, ShellFileOpRequest, ShellFileOpResponse,
    ShellJobCodexMetadata, ShellJobInfo, ShellJobOpRequest, ShellJobOpResponse, ShellRunRequest,
    ShellRunResponse,
};
use salvo::prelude::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, Notify};
use uuid::Uuid;

const MAX_CLIENT_ID_LEN: usize = 80;
const MAX_CLIENT_FIELD_LEN: usize = 200;
/// Max length for `agent_instance_id`. A UUID v4 is 36 chars; allow headroom
/// for future formats but bound it so a malicious peer cannot stash huge
/// strings in the registry.
const MAX_AGENT_INSTANCE_ID_LEN: usize = 128;
const MAX_COMMAND_LEN: usize = 8_000;
const MAX_CWD_LEN: usize = 1_024;
const MAX_FILE_PATH_LEN: usize = 2_048;
const MAX_FILE_CONTENT_BYTES: usize = 512 * 1024;
const MAX_RUN_STDIN_BYTES: usize = 15 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
const MAX_SYNC_WAIT_SECS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECS: u64 = 24 * 60 * 60;
const CLIENT_ONLINE_WINDOW_SECS: i64 = 60;
/// Maximum number of pending requests queued for a single agent client.
/// Bounds memory when an agent is slow or disconnected: once a client's
/// queue reaches this depth, new enqueues are rejected with a structured
/// error instead of growing unboundedly. The WebSocket outbound channel
/// (`OUTGOING_CHANNEL_CAPACITY` in `agent_ws.rs`) is smaller than this, so a
/// slow WebSocket agent fills its outbound channel first and the request
/// pump applies natural backpressure; this cap is the hard ceiling that
/// protects the registry when even that backpressure cannot drain (e.g. a
/// dead socket the OS has not yet reported as closed).
const MAX_QUEUED_REQUESTS_PER_CLIENT: usize = 256;

/// Transport label for polling agents (HTTP `/api/shell/agent/poll`).
pub const TRANSPORT_POLLING: &str = "polling";
/// Transport label for agents connected over the WebSocket endpoint.
pub const TRANSPORT_WEBSOCKET: &str = "websocket";
/// Transport label for agents connected over the custom QUIC stream transport.
/// Reported in `ShellClientView.transport` and surfaced by `runtime_status` /
/// `listAgents`. New deployments should generally use `transport = "auto"`
/// with `[quic]` configured so QUIC is attempted before fallback transports.
pub const TRANSPORT_QUIC: &str = "quic";

#[derive(Debug, Clone)]
struct ShellClientRecord {
    client_id: String,
    /// Active agent process identity (UUID). Replacing this value is the lease
    /// hand-off: once changed, the previous instance can no longer poll or
    /// submit results/job_updates.
    agent_instance_id: String,
    display_name: Option<String>,
    owner: Option<String>,
    hostname: Option<String>,
    capabilities: ShellClientCapabilities,
    projects: Vec<ShellAgentProjectSummary>,
    last_seen: i64,
    agent_protocol_version: String,
    /// How this client is currently connected: `"polling"`, `"websocket"`,
    /// or `"quic"`.
    transport: String,
    /// Sanitized agent policy summary reported at registration. `None` for
    /// older agents that did not report a policy. Exposed in
    /// `runtime_status` / `listAgents`; never carries token/env/init_script.
    policy: Option<AgentPolicySummary>,
}

#[derive(Debug)]
struct PendingShellRequest {
    request: ShellAgentShellRequest,
    waiter: Option<oneshot::Sender<ShellRunResponse>>,
    job_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ShellJobRecord {
    job_id: String,
    request_id: Option<String>,
    client_id: String,
    kind: String,
    project_id: Option<String>,
    cwd: Option<String>,
    command_preview: String,
    status: String,
    created_at: i64,
    started_at: Option<i64>,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    duration_ms: Option<u64>,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
    codex: Option<ShellJobCodexMetadata>,
}

#[derive(Debug, Default)]
struct ShellClientRegistryInner {
    clients: HashMap<String, ShellClientRecord>,
    pending_by_id: HashMap<String, PendingShellRequest>,
    queues_by_client: HashMap<String, VecDeque<String>>,
    jobs_by_id: HashMap<String, ShellJobRecord>,
    request_to_job: HashMap<String, String>,
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
    notifiers: HashMap<String, NotifierEntry>,
}

/// A registered push notifier plus the agent instance id that installed it.
#[derive(Debug, Clone)]
struct NotifierEntry {
    notify: Arc<Notify>,
    agent_instance_id: String,
}

#[derive(Debug, Default)]
pub struct ShellClientRegistry {
    inner: Mutex<ShellClientRegistryInner>,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn validate_id(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_CLIENT_ID_LEN {
        return Err(format!(
            "{} must be 1..={} characters",
            field, MAX_CLIENT_ID_LEN
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "{} may only contain ASCII letters, digits, '-', '_', and '.'",
            field
        ));
    }
    Ok(())
}

/// Validate `agent_instance_id`. It must be a non-empty, bounded ASCII string.
/// We accept the canonical UUID v4 format (`8-4-4-4-12` hex with dashes) and
/// also any short alphanumeric/dash string so future identity formats keep
/// working, but we reject empty / oversized / control-char values. This is not
/// a secret, so the value itself may appear in logs and `runtime_status`.
fn validate_agent_instance_id(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("agent_instance_id must not be empty".to_string());
    }
    if value.len() > MAX_AGENT_INSTANCE_ID_LEN {
        return Err(format!(
            "agent_instance_id is too long; maximum is {} characters",
            MAX_AGENT_INSTANCE_ID_LEN
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "agent_instance_id may only contain ASCII letters, digits, '-', and '_'".to_string(),
        );
    }
    Ok(())
}

pub(crate) fn requested_by_from_auth(auth: Option<&crate::auth::AuthContext>) -> String {
    if auth.map(|auth| auth.is_bootstrap).unwrap_or(false) {
        return "bootstrap".to_string();
    }
    auth.and_then(|auth| auth.username.as_deref())
        .filter(|username| !username.trim().is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

pub(crate) fn assert_shell_client_owner(
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
    owner: Option<&str>,
) -> Result<(), String> {
    if auth.map(|auth| auth.is_bootstrap).unwrap_or(false) {
        return Ok(());
    }
    let owner = owner
        .filter(|owner| !owner.trim().is_empty())
        .ok_or_else(|| format!("agent client {} has no owner", client_id))?;
    let username = auth
        .and_then(|auth| auth.username.as_deref())
        .filter(|username| !username.trim().is_empty());
    if username == Some(owner) {
        return Ok(());
    }
    let username = username.unwrap_or("anonymous");
    Err(format!(
        "agent client {} is owned by {}; current api key belongs to {}",
        client_id, owner, username
    ))
}

/// Enforce the owner/auth boundary at registration time. Mirrors
/// [`assert_shell_client_owner`] but is intentionally a no-op when no
/// `AuthContext` is present (unit tests that do not install `AuthMiddleware`).
/// In production every agent route is behind `AuthMiddleware`, which rejects
/// anonymous requests before the handler runs, so `auth` is always `Some`.
///
/// Rules:
/// - bootstrap token (or auth disabled) may register any owner;
/// - a normal API key may only register `owner == username`;
/// - a normal API key with a missing/empty owner is rejected, matching the
///   existing owner boundary enforced on later operations.
///
/// Phase 3 additions:
/// - an agent token may register only when its `allowed_client_id` matches
///   `client_id`;
/// - when an agent token authenticates owner "alice" and the request's
///   `owner` is `None`, the effective owner is "alice";
/// - when an agent token authenticates and `owner` is `Some("alice")`, it is
///   accepted;
/// - when an agent token authenticates and `owner` is `Some("bob")`, it is
///   rejected (agents may not claim another owner);
/// - a user token (Phase 2 personal API token) is rejected from agent transport
///   registration. Only bootstrap or agent tokens may use agent transport
///   endpoints.
pub(crate) fn enforce_register_owner(
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
    owner: Option<&str>,
) -> Result<(), String> {
    let Some(auth) = auth else {
        return Ok(());
    };
    // Bootstrap may register any owner.
    if auth.is_bootstrap {
        return Ok(());
    }
    // Phase 3: agent tokens are bound to an allowed_client_id and an owner.
    if auth.is_agent_token() {
        // allowed_client_id must match the registering client_id.
        match auth.allowed_client_id.as_deref() {
            Some(allowed) if allowed == client_id => {}
            _ => {
                return Err(format!(
                    "agent token is not bound to client_id '{}'",
                    client_id
                ));
            }
        }
        let token_username = auth
            .username
            .as_deref()
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| "agent token has no owner".to_string())?;
        // If owner is supplied, it must match the token's owner.
        if let Some(req_owner) = owner.filter(|o| !o.trim().is_empty()) {
            if req_owner != token_username {
                return Err(format!(
                    "agent token owner is '{}'; cannot register owner '{}'",
                    token_username, req_owner
                ));
            }
        }
        return Ok(());
    }
    // Phase 2 user token: rejected from agent transport endpoints. Only
    // bootstrap or agent tokens may register.
    Err("user tokens are not allowed on agent transport endpoints".to_string())
}

/// Resolve the effective owner for an agent register request. When the caller
/// is an agent token, the owner is the token's username regardless of the
/// request body. When the caller is bootstrap, the request body owner is used
/// (or `None` when absent). Returns the owner to store on the registry record.
pub(crate) fn effective_register_owner(
    auth: Option<&crate::auth::AuthContext>,
    owner: Option<&str>,
) -> Option<String> {
    let Some(auth) = auth else {
        return owner.map(str::to_string);
    };
    if auth.is_agent_token() {
        return auth.username.clone();
    }
    owner.filter(|o| !o.trim().is_empty()).map(str::to_string)
}

/// Enforce the agent transport boundary for poll/result/job_update endpoints.
/// These endpoints must only accept bootstrap or agent tokens, and an agent
/// token must be bound to the request's `client_id`. User tokens are rejected.
///
/// This complements [`enforce_register_owner`] which handles the register
/// endpoint. Poll/result/job_update do not carry an owner field; the registry
/// already knows the owner from registration, so we only need to verify the
/// client_id matches the token's `allowed_client_id`.
pub(crate) fn enforce_agent_transport(
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
) -> Result<(), String> {
    let Some(auth) = auth else {
        return Ok(());
    };
    if auth.is_bootstrap {
        return Ok(());
    }
    if auth.is_agent_token() {
        match auth.allowed_client_id.as_deref() {
            Some(allowed) if allowed == client_id => Ok(()),
            _ => Err(format!(
                "agent token is not bound to client_id '{}'",
                client_id
            )),
        }
    } else {
        Err("user tokens are not allowed on agent transport endpoints".to_string())
    }
}

/// Require the caller to hold `scope`. Used by agent transport endpoints to
/// check `agent:register` / `agent:poll` / `agent:result` / `agent:job_update`.
/// Bootstrap is always treated as holding every scope.
pub(crate) fn require_agent_transport_scope(
    auth: Option<&crate::auth::AuthContext>,
    scope: &str,
) -> Result<(), String> {
    let Some(auth) = auth else {
        return Ok(());
    };
    if auth.is_bootstrap {
        return Ok(());
    }
    if auth.is_agent_token() && auth.scopes.iter().any(|s| s == scope) {
        Ok(())
    } else {
        Err(format!("missing required scope: {}", scope))
    }
}

fn validate_optional_field(value: &Option<String>, field: &str) -> Result<(), String> {
    if let Some(value) = value {
        if value.chars().count() > MAX_CLIENT_FIELD_LEN {
            return Err(format!(
                "{} is too long; maximum is {} characters",
                field, MAX_CLIENT_FIELD_LEN
            ));
        }
        if value.contains('\0') {
            return Err(format!("{} cannot contain NUL bytes", field));
        }
    }
    Ok(())
}

fn validate_file_request(body: &ShellFileOpRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    match body.op.as_str() {
        "read" | "write" | "list" | "replace_line_range" | "insert_at_line"
        | "delete_line_range" => {}
        _ => {
            return Err(
                "op must be one of read, write, list, replace_line_range, insert_at_line, delete_line_range"
                    .to_string(),
            )
        }
    }
    let line_edit = matches!(
        body.op.as_str(),
        "replace_line_range" | "insert_at_line" | "delete_line_range"
    );
    let path = body.path.trim();
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if body.path.len() > MAX_FILE_PATH_LEN {
        return Err(format!(
            "path is too long; maximum is {} bytes",
            MAX_FILE_PATH_LEN
        ));
    }
    if body.path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    validate_sha256(&body.expected_sha256)?;
    if body.expected_sha256.is_some() && body.op != "write" && !line_edit {
        return Err("expected_sha256 is only allowed for op=write or line edit ops".to_string());
    }
    if let Some(prefix) = &body.expected_prefix {
        if !line_edit {
            return Err("expected_prefix is only allowed for line edit ops".to_string());
        }
        if prefix.contains('\0') {
            return Err("expected_prefix cannot contain NUL bytes".to_string());
        }
    }
    if body.create_dirs && body.op != "write" {
        return Err("create_dirs is only allowed for op=write".to_string());
    }
    if let Some(content) = &body.content {
        if content.len() > MAX_FILE_CONTENT_BYTES {
            return Err(format!(
                "content is too large; maximum is {} bytes",
                MAX_FILE_CONTENT_BYTES
            ));
        }
        if body.op != "write" && body.op != "replace_line_range" && body.op != "insert_at_line" {
            return Err(
                "content is only allowed for op=write or line edit insert/replace".to_string(),
            );
        }
    }
    if body.op == "write" && body.content.is_none() {
        return Err("content is required for op=write".to_string());
    }
    match body.op.as_str() {
        "read" => {
            match (body.start_line, body.end_line) {
                (Some(start), Some(end)) => {
                    if start == 0 || end < start {
                        return Err("invalid line range".to_string());
                    }
                }
                (Some(_), None) => {
                    return Err(
                        "end_line is required when start_line is set for op=read".to_string()
                    );
                }
                (None, Some(_)) => {
                    return Err(
                        "start_line is required when end_line is set for op=read".to_string()
                    );
                }
                (None, None) => {}
            }
            if body.line.is_some() {
                return Err("line is only allowed for op=insert_at_line".to_string());
            }
        }
        "replace_line_range" => {
            let start = body
                .start_line
                .ok_or_else(|| "start_line is required for op=replace_line_range".to_string())?;
            let end = body
                .end_line
                .ok_or_else(|| "end_line is required for op=replace_line_range".to_string())?;
            if start == 0 || end < start {
                return Err("invalid line range".to_string());
            }
            if body.line.is_some() {
                return Err("line is only allowed for op=insert_at_line".to_string());
            }
            if body.content.is_none() {
                return Err("content is required for op=replace_line_range".to_string());
            }
        }
        "delete_line_range" => {
            let start = body
                .start_line
                .ok_or_else(|| "start_line is required for op=delete_line_range".to_string())?;
            let end = body
                .end_line
                .ok_or_else(|| "end_line is required for op=delete_line_range".to_string())?;
            if start == 0 || end < start {
                return Err("invalid line range".to_string());
            }
            if body.line.is_some() || body.content.is_some() {
                return Err("delete_line_range only accepts start_line/end_line guards".to_string());
            }
        }
        "insert_at_line" => {
            let line = body
                .line
                .ok_or_else(|| "line is required for op=insert_at_line".to_string())?;
            if line == 0 {
                return Err("line out of range".to_string());
            }
            if body.start_line.is_some() || body.end_line.is_some() {
                return Err(
                    "start_line/end_line are only allowed for range line edit ops".to_string(),
                );
            }
            if body.content.is_none() {
                return Err("content is required for op=insert_at_line".to_string());
            }
        }
        _ => {
            if body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
            {
                return Err("line edit fields are only allowed for line edit ops".to_string());
            }
        }
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for shellFileOp",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

fn validate_run_request(body: &ShellRunRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    let command = body.command.trim();
    if command.is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if body.command.len() > MAX_COMMAND_LEN {
        return Err(format!(
            "command is too long; maximum is {} bytes",
            MAX_COMMAND_LEN
        ));
    }
    if body.command.contains('\0') {
        return Err("command cannot contain NUL bytes".to_string());
    }
    if let Some(stdin) = &body.stdin {
        if stdin.len() > MAX_RUN_STDIN_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {} bytes",
                MAX_RUN_STDIN_BYTES
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if body.timeout_secs == 0 || body.timeout_secs > MAX_COMMAND_TIMEOUT_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {}",
            MAX_COMMAND_TIMEOUT_SECS
        ));
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for synchronous runShell",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

fn trim_string(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn normalize_project_summaries(
    projects: Option<Vec<ShellAgentProjectSummary>>,
) -> Vec<ShellAgentProjectSummary> {
    let mut projects = projects.unwrap_or_default();
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    projects.dedup_by(|a, b| a.id == b.id);
    projects
}

fn command_preview(command: &str) -> String {
    let first_line = command.lines().next().unwrap_or_default().trim();
    const MAX_PREVIEW: usize = 120;
    if first_line.chars().count() <= MAX_PREVIEW {
        first_line.to_string()
    } else {
        let preview = first_line.chars().take(MAX_PREVIEW).collect::<String>();
        format!("{}…", preview)
    }
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn validate_sha256(value: &Option<String>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected_sha256 must be 64 hex characters".to_string());
    }
    Ok(())
}

fn truncate_output(value: Option<String>) -> Option<String> {
    value.map(|s| {
        if s.len() <= MAX_OUTPUT_BYTES {
            s
        } else {
            let mut start = s.len() - MAX_OUTPUT_BYTES;
            while start < s.len() && !s.is_char_boundary(start) {
                start += 1;
            }
            format!(
                "[output truncated to last {} bytes]\n{}",
                MAX_OUTPUT_BYTES,
                &s[start..]
            )
        }
    })
}

fn job_view(job: &ShellJobRecord) -> ShellJobInfo {
    let now = now_ts();
    let elapsed_secs = if let Some(duration_ms) = job.duration_ms {
        Some(duration_ms / 1000)
    } else {
        job.started_at
            .map(|started_at| job.ended_at.unwrap_or(now).saturating_sub(started_at) as u64)
    };
    let result = if is_final_job_status(&job.status) {
        Some(ShellAgentJobResult {
            shell: Some(ShellAgentShellJobResult {
                cwd: job.cwd.clone(),
                command_preview: job.command_preview.clone(),
                exit_code: job.exit_code,
                duration_ms: job.duration_ms,
                error: job.error.clone(),
            }),
        })
    } else {
        None
    };
    ShellJobInfo {
        job_id: job.job_id.clone(),
        request_id: job.request_id.clone(),
        client_id: job.client_id.clone(),
        kind: job.kind.clone(),
        project_id: job.project_id.clone(),
        cwd: job.cwd.clone(),
        command_preview: job.command_preview.clone(),
        status: job.status.clone(),
        created_at: job.created_at,
        started_at: job.started_at,
        ended_at: job.ended_at,
        exit_code: job.exit_code,
        duration_ms: job.duration_ms,
        elapsed_secs,
        error: job.error.clone(),
        codex: job.codex.clone(),
        result,
    }
}

fn select_lines(
    value: Option<&String>,
    since_line: Option<usize>,
    tail_lines: Option<usize>,
) -> (Option<String>, usize) {
    let Some(value) = value else {
        return (Some(String::new()), since_line.unwrap_or(1));
    };
    let lines = value.lines().collect::<Vec<_>>();
    if let Some(tail) = tail_lines.filter(|n| *n > 0) {
        let start = lines.len().saturating_sub(tail);
        let selected = lines[start..].join("\n");
        let text = if selected.is_empty() {
            selected
        } else {
            format!("{}\n", selected)
        };
        return (Some(text), lines.len() + 1);
    }
    let start_line = since_line.unwrap_or(1).max(1);
    let start_idx = start_line.saturating_sub(1).min(lines.len());
    let selected = lines[start_idx..].join("\n");
    let text = if selected.is_empty() {
        selected
    } else {
        format!("{}\n", selected)
    };
    (Some(text), lines.len() + 1)
}

fn append_limited(target: &mut Option<String>, chunk: Option<String>) {
    let Some(chunk) = chunk else {
        return;
    };
    let target_value = target.get_or_insert_with(String::new);
    target_value.push_str(&chunk);
    if target_value.len() > MAX_OUTPUT_BYTES {
        let mut start = target_value.len() - MAX_OUTPUT_BYTES;
        while start < target_value.len() && !target_value.is_char_boundary(start) {
            start += 1;
        }
        *target_value = format!(
            "[output truncated to last {} bytes]\n{}",
            MAX_OUTPUT_BYTES,
            &target_value[start..]
        );
    }
}

fn replace_limited(target: &mut Option<String>, value: Option<String>) {
    let Some(value) = value else {
        return;
    };
    *target = truncate_output(Some(value));
}

fn is_final_job_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "stopped" | "timeout" | "lost"
    )
}

fn client_is_connected_locked(inner: &ShellClientRegistryInner, client_id: &str) -> bool {
    inner
        .clients
        .get(client_id)
        .map(|client| now_ts().saturating_sub(client.last_seen) <= CLIENT_ONLINE_WINDOW_SECS)
        .unwrap_or(false)
}

fn offline_last_seen(now: i64) -> i64 {
    now.saturating_sub(CLIENT_ONLINE_WINDOW_SECS.saturating_add(1))
}

/// Verify that `client_id` exists and that `agent_instance_id` matches the
/// instance that currently holds the lease for it. A stale/replaced instance
/// (e.g. a second process that was rejected, or the previous process after a
/// stale replacement) is rejected so it can no longer poll or submit results.
/// Callers must already hold `inner`.
fn assert_active_instance_locked(
    inner: &ShellClientRegistryInner,
    client_id: &str,
    agent_instance_id: &str,
) -> Result<(), String> {
    let Some(client) = inner.clients.get(client_id) else {
        return Err(format!("unknown shell client: {}", client_id));
    };
    if client.agent_instance_id != agent_instance_id {
        return Err(format!(
            "agent client {} is no longer the active instance (stale or replaced)",
            client_id
        ));
    }
    Ok(())
}

/// Reject enqueue when a client's pending queue has reached
/// `MAX_QUEUED_REQUESTS_PER_CLIENT`. Callers must already hold `inner`.
fn ensure_queue_capacity_locked(
    inner: &ShellClientRegistryInner,
    client_id: &str,
) -> Result<(), String> {
    let len = inner
        .queues_by_client
        .get(client_id)
        .map(VecDeque::len)
        .unwrap_or(0);
    if len >= MAX_QUEUED_REQUESTS_PER_CLIENT {
        return Err(format!(
            "too many pending requests for shell client {} (limit {})",
            client_id, MAX_QUEUED_REQUESTS_PER_CLIENT
        ));
    }
    Ok(())
}

/// Ensure a request target exists before enqueueing work for the agent pump.
fn ensure_dispatch_supported_locked(
    inner: &ShellClientRegistryInner,
    client_id: &str,
) -> Result<(), String> {
    if !inner.clients.contains_key(client_id) {
        return Err(format!("unknown shell client: {}", client_id));
    }
    Ok(())
}

fn refresh_job_status_locked(inner: &mut ShellClientRegistryInner, job_id: &str) {
    let Some(job) = inner.jobs_by_id.get(job_id) else {
        return;
    };
    if is_final_job_status(&job.status)
        || !matches!(
            job.status.as_str(),
            "agent_queued" | "running" | "stop_requested"
        )
    {
        return;
    }
    let client_id = job.client_id.clone();
    if client_is_connected_locked(inner, &client_id) {
        return;
    }
    if let Some(job) = inner.jobs_by_id.get_mut(job_id) {
        job.status = "lost".to_string();
        job.ended_at = Some(now_ts());
        job.error = Some("shell client went stale while job was running".to_string());
    }
}

impl ShellClientRegistry {
    pub async fn register(
        &self,
        body: ShellClientRegisterRequest,
    ) -> Result<ShellClientView, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        validate_optional_field(&body.display_name, "display_name")?;
        validate_optional_field(&body.owner, "owner")?;
        validate_optional_field(&body.hostname, "hostname")?;

        let client_id = body.client_id.trim().to_string();
        let agent_instance_id = body.agent_instance_id.trim().to_string();
        let record = ShellClientRecord {
            client_id: client_id.clone(),
            agent_instance_id: agent_instance_id.clone(),
            display_name: trim_string(body.display_name),
            owner: trim_string(body.owner),
            hostname: trim_string(body.hostname),
            capabilities: body.capabilities.unwrap_or_default(),
            projects: normalize_project_summaries(body.projects),
            last_seen: now_ts(),
            agent_protocol_version: body
                .agent_protocol_version
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "unknown".to_string()),
            transport: TRANSPORT_POLLING.to_string(),
            policy: body.policy,
        };
        let mut inner = self.inner.lock().await;

        // Enforce the agent instance lease. `client_id` is the unique active
        // agent identity: at most one agent process may be online for it at a
        // time. Rules:
        //   - no existing client            -> accept (fresh registration)
        //   - existing client is stale/offline -> accept and replace the
        //     active instance (lease hand-off to the new process)
        //   - existing client is online and the same instance id reconnects
        //     -> accept as a refresh/reconnect
        //   - existing client is online and a *different* instance id tries to
        //     register -> reject with a clear error so two processes cannot
        //     steal each other's requests.
        if let Some(existing) = inner.clients.get(&client_id) {
            let online = now_ts().saturating_sub(existing.last_seen) <= CLIENT_ONLINE_WINDOW_SECS;
            let same_instance = existing.agent_instance_id == agent_instance_id;
            if online && !same_instance {
                return Err(format!(
                    "agent client {} is already online with a different instance",
                    client_id
                ));
            }
        }

        // When a different instance takes over the lease (stale replacement),
        // clear any notifier left by the previous instance so the request pump
        // for the dead process is not re-armed against the new one. A
        // same-instance refresh keeps its notifier in place.
        let replaced_instance = inner
            .clients
            .get(&client_id)
            .map(|existing| existing.agent_instance_id != agent_instance_id)
            .unwrap_or(false);
        if replaced_instance {
            inner.notifiers.remove(&client_id);
        }

        inner.clients.insert(client_id.clone(), record);
        Ok(Self::client_view_locked(&inner, &client_id).expect("client just inserted"))
    }

    /// Override the transport label for a registered client. Called by the
    /// WebSocket handler after a successful register so observability and
    /// `list_agents` reflect how the agent is actually connected. Polling
    /// agents keep the default `"polling"` label set during `register`.
    pub async fn set_transport(&self, client_id: &str, transport: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        client.transport = transport.to_string();
        Ok(())
    }

    /// Refresh `last_seen` for a registered client to "now" without performing
    /// any business operation. Used by the WebSocket reader so that idle
    /// keepalive traffic (`Ping`/`Pong`) keeps a connected agent inside the
    /// `CLIENT_ONLINE_WINDOW_SECS` online window. Without this, a WebSocket
    /// agent that has no pending requests would age out to `"stale"` after 60s
    /// even though its socket is still open.
    ///
    /// Returns an error (and mutates nothing) for an unknown `client_id` so
    /// callers can log a clear diagnostic; it is a no-op for the unknown path.
    /// `register`, `poll`, `complete`, and `update_job` already refresh
    /// `last_seen` on their own, so this is only needed for keepalive frames.
    ///
    /// `agent_instance_id` is required: a stale/replaced instance must not be
    /// able to refresh the active lease's `last_seen` via Ping/Pong. If the id
    /// does not match the currently active instance, the touch is rejected
    /// before mutating any state.
    pub async fn touch_client(
        &self,
        client_id: &str,
        agent_instance_id: &str,
    ) -> Result<(), String> {
        validate_agent_instance_id(agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        client.last_seen = now_ts();
        Ok(())
    }

    /// Test-only hook to force a client's `last_seen` so liveness/stale
    /// behavior can be exercised without sleeping for the full online window.
    #[cfg(test)]
    pub async fn set_last_seen_for_test(&self, client_id: &str, ts: i64) {
        let mut inner = self.inner.lock().await;
        if let Some(client) = inner.clients.get_mut(client_id) {
            client.last_seen = ts;
        }
    }

    /// Register a push notifier for a client. The WebSocket handler calls
    /// this after register; the server's request pump waits on the notifier
    /// between polls. Calling this replaces any previously registered
    /// notifier for the client (e.g. after a reconnect).
    ///
    /// `agent_instance_id` records which agent process owns the notifier. The
    /// caller is the instance that just successfully registered, so it is the
    /// active instance; this always installs/overwrites the notifier entry for
    /// `client_id` tagged with that instance id.
    pub async fn register_notifier(
        &self,
        client_id: &str,
        agent_instance_id: &str,
        notify: Arc<Notify>,
    ) -> Result<(), String> {
        validate_agent_instance_id(agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        // Only the currently active instance may install a notifier. A late
        // notifier registration from a stale instance (e.g. it registered,
        // then was replaced before reaching this call) must not overwrite the
        // active instance's notifier.
        if client.agent_instance_id != agent_instance_id {
            return Err(format!(
                "agent client {} is no longer the active instance (stale or replaced)",
                client_id
            ));
        }
        inner.notifiers.insert(
            client_id.to_string(),
            NotifierEntry {
                notify,
                agent_instance_id: agent_instance_id.to_string(),
            },
        );
        Ok(())
    }

    /// Reconcile state after an agent transport disconnects or sends a
    /// graceful offline notice. Active-instance strategy:
    ///
    /// - remove the push notifier so the request pump is not re-armed;
    /// - mark every non-final, running-like job owned by the client as
    ///   `"lost"` with a descriptive error, and drop its pending request (the
    ///   oneshot waiter resolves to a "dropped" error on the caller side);
    /// - the client record itself is retained so late results/updates can be
    ///   logged and runtime_status/list_agents keep observability history;
    /// - `last_seen` is moved just outside the online window so the active
    ///   lease is released immediately and a restarted agent can register
    ///   without waiting for the normal 60s timeout.
    ///
    /// `agent_instance_id` identifies *which* agent process disconnected. The
    /// cleanup only fires when that id matches the currently active instance
    /// for `client_id`; a stale disconnect (e.g. instance A's socket finally
    /// tearing down after instance B already replaced it) must NOT remove
    /// B's notifier or mark B's jobs lost.
    ///
    /// This is intentionally conservative about jobs: a reconnecting agent that keeps
    /// running the same job will see the server-side job as `"lost"` (final),
    /// so its late `job_update`/`result` is ignored by `update_job`/`complete`.
    /// Operators should treat `"lost"` as "the server no longer tracks this
    /// job; restart it if needed". A future phase may lift `JobManager` to
    /// agent-level so reconnects can resume in-flight jobs.
    pub async fn reconcile_disconnect(&self, client_id: &str, agent_instance_id: &str) {
        let mut inner = self.inner.lock().await;
        // Only reconcile when the disconnect belongs to the currently active
        // instance. A stale disconnect (a previous process whose socket finally
        // tore down after a newer instance already took over the lease) must
        // not touch the active instance's notifier or jobs.
        let is_active = inner
            .clients
            .get(client_id)
            .map(|client| client.agent_instance_id == agent_instance_id)
            .unwrap_or(false);
        if !is_active {
            return;
        }
        // Remove the notifier only if it still belongs to this instance.
        if inner
            .notifiers
            .get(client_id)
            .map(|entry| entry.agent_instance_id == agent_instance_id)
            .unwrap_or(false)
        {
            inner.notifiers.remove(client_id);
        }
        let lost_error = "agent transport disconnected".to_string();
        let now = now_ts();
        if let Some(client) = inner.clients.get_mut(client_id) {
            client.last_seen = offline_last_seen(now);
        }
        let lost_job_ids: Vec<String> = inner
            .jobs_by_id
            .iter()
            .filter_map(|(job_id, job)| {
                if job.client_id != client_id {
                    return None;
                }
                if is_final_job_status(&job.status)
                    || !matches!(
                        job.status.as_str(),
                        "queued" | "agent_queued" | "running" | "stop_requested"
                    )
                {
                    return None;
                }
                Some(job_id.clone())
            })
            .collect();
        for job_id in lost_job_ids {
            let request_id = inner
                .jobs_by_id
                .get(&job_id)
                .and_then(|j| j.request_id.clone());
            if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                job.status = "lost".to_string();
                job.ended_at = Some(now);
                job.error = Some(lost_error.clone());
            }
            if let Some(request_id) = request_id {
                inner.pending_by_id.remove(&request_id);
                inner.request_to_job.remove(&request_id);
                if let Some(queue) = inner.queues_by_client.get_mut(client_id) {
                    queue.retain(|id| id != &request_id);
                }
            }
        }
    }

    /// Wake the push notifier for a client if one is registered. Called by
    /// the enqueue paths (`enqueue_run`, `enqueue_file_op`, `start_job`,
    /// `stop_job`) so the WebSocket pump can immediately push the new
    /// request to the agent instead of waiting for a poll. Holds no lock of
    /// its own; callers must already hold `inner`.
    fn notify_client_locked(inner: &ShellClientRegistryInner, client_id: &str) {
        if let Some(entry) = inner.notifiers.get(client_id) {
            entry.notify.notify_one();
        }
    }

    pub async fn list_clients(&self) -> Vec<ShellClientView> {
        let inner = self.inner.lock().await;
        let mut ids = inner.clients.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        ids.into_iter()
            .filter_map(|id| Self::client_view_locked(&inner, &id))
            .collect()
    }

    pub async fn get_client_view(&self, client_id: &str) -> Option<ShellClientView> {
        let inner = self.inner.lock().await;
        Self::client_view_locked(&inner, client_id)
    }

    /// Return the capabilities advertised by a registered agent client.
    /// Errors with a structured `unknown shell client` message when the
    /// client is not registered.
    pub async fn get_client_capabilities(
        &self,
        client_id: &str,
    ) -> Result<ShellClientCapabilities, String> {
        let inner = self.inner.lock().await;
        let client = inner
            .clients
            .get(client_id)
            .ok_or_else(|| format!("unknown shell client: {}", client_id))?;
        Ok(client.capabilities.clone())
    }

    /// Check whether a registered agent client supports a named capability.
    /// Recognized capability names: `shell`, `file_read`, `file_write`,
    /// `git`, `jobs`, `async_jobs`, `async_shell_jobs`. Unknown capability
    /// names return `false`.
    pub async fn client_supports(&self, client_id: &str, capability: &str) -> Result<bool, String> {
        let caps = self.get_client_capabilities(client_id).await?;
        Ok(match capability {
            "shell" => caps.shell,
            "file_read" => caps.file_read,
            "file_write" => caps.file_write,
            "git" => caps.git,
            "jobs" => caps.jobs,
            "async_jobs" => caps.async_jobs,
            "async_shell_jobs" => caps.async_shell_jobs,
            _ => false,
        })
    }

    /// List the projects registered for a given shell client. Currently only
    /// exercised by tests; kept as a public accessor of the registry API.
    #[allow(dead_code)]
    pub async fn list_client_projects(
        &self,
        client_id: &str,
    ) -> Result<Vec<ShellAgentProjectSummary>, String> {
        validate_id(client_id, "client_id")?;
        let inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        Ok(client.projects.clone())
    }

    /// Insert or replace a single project summary in the cached project list
    /// for `client_id`. Called by the runtime after a successful
    /// `register_project` / `create_project` agent operation so that
    /// `listProjects` sees the new project immediately, without waiting for
    /// the agent's next register/poll cycle. If a project with the same id
    /// already exists it is replaced; otherwise the new summary is appended
    /// and the list is re-sorted by id (matching `normalize_project_summaries`).
    pub async fn upsert_client_project(
        &self,
        client_id: &str,
        project: ShellAgentProjectSummary,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get_mut(client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        if let Some(existing) = client.projects.iter_mut().find(|p| p.id == project.id) {
            *existing = project;
        } else {
            client.projects.push(project);
            client.projects.sort_by(|a, b| a.id.cmp(&b.id));
            client.projects.dedup_by(|a, b| a.id == b.id);
        }
        Ok(())
    }

    pub async fn enqueue_file_op(
        &self,
        body: ShellFileOpRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_file_request(&body)?;
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let kind = format!("file_{}", body.op);
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind,
            job_id: None,
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: Some(body.path.trim().to_string()),
            content: body.content.clone(),
            max_bytes: body.max_bytes,
            expected_sha256: body.expected_sha256.clone(),
            expected_prefix: body.expected_prefix.clone(),
            start_line: body.start_line,
            end_line: body.end_line,
            line: body.line,
            create_dirs: body.create_dirs,
            command: String::new(),
            stdin: None,
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        ensure_dispatch_supported_locked(&inner, &body.client_id)?;
        ensure_queue_capacity_locked(&inner, &body.client_id)?;
        inner
            .queues_by_client
            .entry(body.client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: Some(tx),
                job_id: None,
            },
        );
        Self::notify_client_locked(&inner, &body.client_id);
        Ok((request_id, rx))
    }

    pub async fn enqueue_run(
        &self,
        body: ShellRunRequest,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_run_request(&body)?;
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: body.client_id.clone(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: body.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: body.command.clone(),
            stdin: body.stdin.clone(),
            timeout_secs: body.timeout_secs,
            requested_by,
            created_at: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        ensure_dispatch_supported_locked(&inner, &body.client_id)?;
        ensure_queue_capacity_locked(&inner, &body.client_id)?;
        inner
            .queues_by_client
            .entry(body.client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: Some(tx),
                job_id: None,
            },
        );
        Self::notify_client_locked(&inner, &body.client_id);
        Ok((request_id, rx))
    }

    pub async fn cancel_request(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending_by_id.remove(request_id);
        for queue in inner.queues_by_client.values_mut() {
            queue.retain(|id| id != request_id);
        }
    }

    /// Enqueue a project-management agent request (`register_project` or
    /// `create_project`). The JSON payload is carried in `stdin` so the
    /// agent can parse it without shell interpolation. The `command` field is
    /// empty (unused for these kinds); the agent dispatches on `kind` and
    /// reads the payload from `stdin`. Returns a oneshot receiver for the
    /// `ShellRunResponse` (the agent returns structured JSON in `stdout`).
    pub async fn enqueue_project_op(
        &self,
        client_id: String,
        kind: &str,
        payload: String,
        requested_by: String,
    ) -> Result<(String, oneshot::Receiver<ShellRunResponse>), String> {
        validate_id(&client_id, "client_id")?;
        if kind != "register_project" && kind != "create_project" {
            return Err(format!("unsupported project op kind: {}", kind));
        }
        if payload.contains('\0') {
            return Err("project op payload must not contain NUL".to_string());
        }
        let request_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: kind.to_string(),
            job_id: None,
            cwd: None,
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command: String::new(),
            stdin: Some(payload),
            timeout_secs: 30,
            requested_by,
            created_at: now_ts(),
        };
        let mut inner = self.inner.lock().await;
        ensure_dispatch_supported_locked(&inner, &client_id)?;
        ensure_queue_capacity_locked(&inner, &client_id)?;
        inner
            .queues_by_client
            .entry(client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: Some(tx),
                job_id: None,
            },
        );
        Self::notify_client_locked(&inner, &client_id);
        Ok((request_id, rx))
    }

    pub async fn poll(
        &self,
        body: ShellAgentPollRequest,
    ) -> Result<Option<ShellAgentShellRequest>, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        {
            let Some(client) = inner.clients.get_mut(&body.client_id) else {
                return Err(format!("unknown shell client: {}", body.client_id));
            };
            if client.agent_instance_id != body.agent_instance_id {
                return Err(format!(
                    "agent client {} is no longer the active instance (stale or replaced)",
                    body.client_id
                ));
            }
            if body.projects.is_some() {
                client.projects = normalize_project_summaries(body.projects);
            }
            client.last_seen = now_ts();
        }
        loop {
            let request_id = {
                let Some(queue) = inner.queues_by_client.get_mut(&body.client_id) else {
                    return Ok(None);
                };
                queue.pop_front()
            };
            let Some(request_id) = request_id else {
                return Ok(None);
            };
            let Some((request, job_id)) = inner
                .pending_by_id
                .get(&request_id)
                .map(|pending| (pending.request.clone(), pending.job_id.clone()))
            else {
                continue;
            };
            if request.kind == "stop_job" {
                inner.pending_by_id.remove(&request_id);
                return Ok(Some(request));
            }
            if let Some(job_id) = job_id {
                if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                    if job.status == "queued" {
                        job.status = "agent_queued".to_string();
                        job.started_at = Some(now_ts());
                    }
                }
            }
            return Ok(Some(request));
        }
    }

    pub async fn complete(&self, body: ShellAgentResultRequest) -> Result<(), String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.request_id, "request_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        // Reject results from a stale/replaced instance before refreshing
        // liveness: a dead process must not update the active lease's
        // `last_seen` or resolve its waiters.
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
        if let Some(client) = inner.clients.get_mut(&body.client_id) {
            client.last_seen = now_ts();
        }
        let Some(mut pending) = inner.pending_by_id.remove(&body.request_id) else {
            return Err(format!(
                "unknown or expired shell request: {}",
                body.request_id
            ));
        };
        if pending.request.client_id != body.client_id {
            return Err("request_id does not belong to client_id".to_string());
        }
        let request_id = body.request_id.clone();
        let client_id = body.client_id.clone();
        let error = body.error.clone();
        let stdout = truncate_output(body.stdout);
        let stderr = truncate_output(body.stderr);
        if let Some(job_id) = pending.job_id.clone() {
            inner.request_to_job.remove(&request_id);
            if let Some(job) = inner.jobs_by_id.get_mut(&job_id) {
                job.status = if error.is_none() && body.exit_code == Some(0) {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                job.ended_at = Some(now_ts());
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                job.stdout = stdout.clone();
                job.stderr = stderr.clone();
                job.error = error.clone();
            }
        }
        let response = ShellRunResponse {
            success: error.is_none() && body.exit_code == Some(0),
            request_id,
            client_id,
            cwd: pending.request.cwd,
            command_preview: command_preview(&pending.request.command),
            exit_code: body.exit_code,
            stdout,
            stderr,
            duration_ms: body.duration_ms,
            error,
        };
        if let Some(waiter) = pending.waiter.take() {
            let _ = waiter.send(response);
        }
        Ok(())
    }

    pub async fn start_job(
        &self,
        body: ShellJobOpRequest,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        let client_id = body
            .client_id
            .clone()
            .ok_or_else(|| "client_id is required for op=start".to_string())?;
        let command = body
            .command
            .clone()
            .ok_or_else(|| "command is required for op=start".to_string())?;
        let run = ShellRunRequest {
            client_id: client_id.clone(),
            cwd: body.cwd.clone(),
            command: command.clone(),
            stdin: None,
            timeout_secs: body.timeout_secs.unwrap_or(120),
            wait_timeout_secs: 0,
        };
        validate_run_request(&run)?;
        let request_id = Uuid::new_v4().to_string();
        let job_id = Uuid::new_v4().to_string();
        let created_at = now_ts();
        let request = ShellAgentShellRequest {
            request_id: request_id.clone(),
            client_id: client_id.clone(),
            kind: "start_job".to_string(),
            job_id: Some(job_id.clone()),
            cwd: run.cwd.clone().map(|cwd| cwd.trim().to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            command,
            stdin: None,
            timeout_secs: run.timeout_secs,
            requested_by,
            created_at,
        };
        let mut inner = self.inner.lock().await;
        let Some(client) = inner.clients.get(&client_id) else {
            return Err(format!("unknown shell client: {}", client_id));
        };
        ensure_dispatch_supported_locked(&inner, &client_id)?;
        if !(client.capabilities.async_jobs || client.capabilities.async_shell_jobs) {
            return Err(format!(
                "agent client {} does not support async shell jobs",
                client_id
            ));
        }
        ensure_queue_capacity_locked(&inner, &client_id)?;
        inner
            .queues_by_client
            .entry(client_id.clone())
            .or_default()
            .push_back(request_id.clone());
        let job = ShellJobRecord {
            job_id: job_id.clone(),
            request_id: Some(request_id.clone()),
            client_id: client_id.clone(),
            kind: "shell".to_string(),
            project_id: None,
            cwd: run.cwd.clone(),
            command_preview: command_preview(&run.command),
            status: "queued".to_string(),
            created_at,
            started_at: None,
            ended_at: None,
            exit_code: None,
            duration_ms: None,
            stdout: None,
            stderr: None,
            error: None,
            codex: body.codex.clone(),
        };
        inner.pending_by_id.insert(
            request_id.clone(),
            PendingShellRequest {
                request,
                waiter: None,
                job_id: Some(job_id.clone()),
            },
        );
        inner.request_to_job.insert(request_id, job_id.clone());
        inner.jobs_by_id.insert(job_id.clone(), job);
        Self::notify_client_locked(&inner, &client_id);
        Ok(job_view(
            inner.jobs_by_id.get(&job_id).expect("job just inserted"),
        ))
    }

    pub async fn get_job(&self, job_id: &str) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        Ok(job_view(job))
    }

    pub async fn list_jobs(&self, limit: Option<usize>) -> Vec<ShellJobInfo> {
        let mut inner = self.inner.lock().await;
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner.jobs_by_id.values().cloned().collect::<Vec<_>>();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        jobs.into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect()
    }

    pub async fn list_jobs_for_client(
        &self,
        client_id: &str,
        status: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<ShellJobInfo>, String> {
        validate_id(client_id, "client_id")?;
        let mut inner = self.inner.lock().await;
        if !inner.clients.contains_key(client_id) {
            return Err(format!("unknown shell client: {}", client_id));
        }
        let job_ids = inner.jobs_by_id.keys().cloned().collect::<Vec<_>>();
        for job_id in job_ids {
            refresh_job_status_locked(&mut inner, &job_id);
        }
        let mut jobs = inner
            .jobs_by_id
            .values()
            .filter(|job| job.client_id == client_id)
            .filter(|job| status.map(|status| status == job.status).unwrap_or(true))
            .cloned()
            .collect::<Vec<_>>();
        jobs.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(jobs
            .into_iter()
            .take(limit.unwrap_or(20).clamp(1, 100))
            .map(|job| job_view(&job))
            .collect())
    }

    pub async fn job_log(
        &self,
        job_id: &str,
        since_stdout_line: Option<usize>,
        since_stderr_line: Option<usize>,
        tail_lines: Option<usize>,
    ) -> Result<(ShellJobInfo, Option<String>, Option<String>, usize, usize), String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        refresh_job_status_locked(&mut inner, job_id);
        let Some(job) = inner.jobs_by_id.get(job_id) else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        let (stdout, next_stdout_line) =
            select_lines(job.stdout.as_ref(), since_stdout_line, tail_lines);
        let (stderr, next_stderr_line) =
            select_lines(job.stderr.as_ref(), since_stderr_line, tail_lines);
        Ok((
            job_view(job),
            stdout,
            stderr,
            next_stdout_line,
            next_stderr_line,
        ))
    }

    pub async fn stop_job(
        &self,
        job_id: &str,
        requested_by: String,
    ) -> Result<ShellJobInfo, String> {
        validate_id(job_id, "job_id")?;
        let mut inner = self.inner.lock().await;
        let Some(job) = inner.jobs_by_id.get(job_id).cloned() else {
            return Err(format!("unknown shell job: {}", job_id));
        };
        match job.status.as_str() {
            "queued" => {
                if let Some(request_id) = &job.request_id {
                    inner.pending_by_id.remove(request_id);
                    inner.request_to_job.remove(request_id);
                    for queue in inner.queues_by_client.values_mut() {
                        queue.retain(|id| id != request_id);
                    }
                }
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stopped".to_string();
                job.ended_at = Some(now_ts());
                job.error = Some("job stopped before agent picked it up".to_string());
                Ok(job_view(job))
            }
            "agent_queued" | "running" | "stop_requested" => {
                let stop_request_id = Uuid::new_v4().to_string();
                let client_id = job.client_id.clone();
                let request = ShellAgentShellRequest {
                    request_id: stop_request_id.clone(),
                    client_id: client_id.clone(),
                    kind: "stop_job".to_string(),
                    job_id: Some(job_id.to_string()),
                    cwd: None,
                    path: None,
                    content: None,
                    max_bytes: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    command: String::new(),
                    stdin: None,
                    timeout_secs: 1,
                    requested_by,
                    created_at: now_ts(),
                };
                ensure_dispatch_supported_locked(&inner, &client_id)?;
                ensure_queue_capacity_locked(&inner, &client_id)?;
                inner
                    .queues_by_client
                    .entry(client_id)
                    .or_default()
                    .push_back(stop_request_id.clone());
                inner.pending_by_id.insert(
                    stop_request_id,
                    PendingShellRequest {
                        request,
                        waiter: None,
                        job_id: Some(job_id.to_string()),
                    },
                );
                let job = inner.jobs_by_id.get_mut(job_id).expect("job exists");
                job.status = "stop_requested".to_string();
                job.error = Some("stop requested".to_string());
                let notify_client_id = job.client_id.clone();
                Self::notify_client_locked(&inner, &notify_client_id);
                Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists")))
            }
            _ => Ok(job_view(inner.jobs_by_id.get(job_id).expect("job exists"))),
        }
    }

    pub async fn update_job(
        &self,
        body: ShellAgentJobUpdateRequest,
    ) -> Result<ShellJobInfo, String> {
        validate_id(&body.client_id, "client_id")?;
        validate_id(&body.job_id, "job_id")?;
        validate_agent_instance_id(&body.agent_instance_id)?;
        let mut inner = self.inner.lock().await;
        // Reject job updates from a stale/replaced instance before refreshing
        // liveness or mutating job state.
        assert_active_instance_locked(&inner, &body.client_id, &body.agent_instance_id)?;
        if let Some(client) = inner.clients.get_mut(&body.client_id) {
            client.last_seen = now_ts();
        }
        let mut request_id_to_remove = None;
        let view = {
            let Some(job) = inner.jobs_by_id.get_mut(&body.job_id) else {
                return Err(format!("unknown shell job: {}", body.job_id));
            };
            if job.client_id != body.client_id {
                return Err("job_id does not belong to client_id".to_string());
            }
            if is_final_job_status(&job.status) {
                return Ok(job_view(job));
            }
            replace_limited(&mut job.stdout, body.stdout_tail);
            replace_limited(&mut job.stderr, body.stderr_tail);
            append_limited(&mut job.stdout, body.stdout_chunk);
            append_limited(&mut job.stderr, body.stderr_chunk);
            if job.started_at.is_none()
                && matches!(
                    body.status.as_str(),
                    "running" | "completed" | "failed" | "stopped" | "timeout"
                )
            {
                job.started_at = Some(now_ts());
            }
            if !body.status.trim().is_empty() && !is_final_job_status(&job.status) {
                let incoming_status = body.status.trim();
                job.status = if incoming_status == "queued" && job.started_at.is_some() {
                    "agent_queued".to_string()
                } else {
                    incoming_status.to_string()
                };
            }
            if is_final_job_status(&body.status) {
                job.status = body.status;
                job.ended_at = Some(now_ts());
                job.exit_code = body.exit_code;
                job.duration_ms = body.duration_ms;
                job.error = body.error;
                request_id_to_remove = job.request_id.clone();
            } else if body.error.is_some() {
                job.error = body.error;
            }
            if body.finished && !is_final_job_status(&job.status) {
                job.status = if job.error.is_none() && job.exit_code == Some(0) {
                    "completed".to_string()
                } else {
                    "failed".to_string()
                };
                job.ended_at = Some(now_ts());
                request_id_to_remove = job.request_id.clone();
            }
            job_view(job)
        };
        if let Some(request_id) = request_id_to_remove {
            inner.pending_by_id.remove(&request_id);
            inner.request_to_job.remove(&request_id);
        }
        Ok(view)
    }

    fn client_view_locked(
        inner: &ShellClientRegistryInner,
        client_id: &str,
    ) -> Option<ShellClientView> {
        let client = inner.clients.get(client_id)?;
        let pending_requests = inner
            .queues_by_client
            .get(client_id)
            .map(VecDeque::len)
            .unwrap_or(0);
        let age = now_ts().saturating_sub(client.last_seen);
        let connected = age <= CLIENT_ONLINE_WINDOW_SECS;
        Some(ShellClientView {
            client_id: client.client_id.clone(),
            agent_instance_id: client.agent_instance_id.clone(),
            display_name: client.display_name.clone(),
            owner: client.owner.clone(),
            hostname: client.hostname.clone(),
            status: if connected { "online" } else { "stale" }.to_string(),
            connected,
            last_seen: client.last_seen,
            capabilities: client.capabilities.clone(),
            pending_requests,
            projects: client.projects.clone(),
            agent_protocol_version: client.agent_protocol_version.clone(),
            transport: client.transport.clone(),
            policy: client.policy.clone(),
        })
    }
}

fn get_registry(depot: &Depot) -> Option<Arc<ShellClientRegistry>> {
    depot.obtain::<Arc<ShellClientRegistry>>().ok().cloned()
}

async fn assert_registry_client_owner(
    registry: &ShellClientRegistry,
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
) -> Result<(), (StatusCode, String)> {
    let Some(client) = registry.get_client_view(client_id).await else {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unknown shell client: {}", client_id),
        ));
    };
    assert_shell_client_owner(auth, client_id, client.owner.as_deref())
        .map_err(|e| (StatusCode::FORBIDDEN, e))
}

fn record_shell_run_action(
    audit: &ActionAudit,
    response: &ShellRunResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("run", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"request_id": response.request_id}))
            .summary(json!({
                "client_id": response.client_id,
                "cwd": response.cwd,
                "command_preview": response.command_preview,
                "exit_code": response.exit_code,
                "duration_ms": response.duration_ms,
            })),
    );
}

fn record_shell_file_action(
    audit: &ActionAudit,
    response: &ShellFileOpResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new(response.op.clone(), response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"request_id": response.request_id}))
            .summary(json!({
                "client_id": response.client_id,
                "path": response.path,
                "cwd": response.cwd,
                "bytes": response.bytes,
                "sha256": response.sha256,
                "entries_count": response.entries.len(),
            })),
    );
}

fn record_shell_job_action(
    audit: &ActionAudit,
    response: &ShellJobOpResponse,
    http_status: StatusCode,
) {
    let job_id = response.job.as_ref().map(|job| job.job_id.clone());
    let job_ids = if response.jobs.is_empty() {
        Vec::<String>::new()
    } else {
        response.jobs.iter().map(|job| job.job_id.clone()).collect()
    };
    audit.record(
        ActionAuditRecord::new(response.op.clone(), response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"job_id": job_id, "job_ids": job_ids}))
            .summary(json!({
                "job_status": response.job.as_ref().map(|job| job.status.clone()),
                "client_id": response.job.as_ref().map(|job| job.client_id.clone()),
                "jobs_count": response.jobs.len(),
                "stdout_included": response.stdout.is_some(),
                "stderr_included": response.stderr.is_some(),
            })),
    );
}

fn record_shell_job_status_action(
    audit: &ActionAudit,
    response: &ShellClientJobStatusResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_status", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({
                "job_id": response.job_id,
                "client_id": response.client_id,
            }))
            .summary(json!({
                "kind": response.kind,
                "status": response.status,
                "exit_code": response.exit_code,
                "elapsed_secs": response.elapsed_secs,
            })),
    );
}

fn record_shell_job_log_action(
    audit: &ActionAudit,
    response: &ShellClientJobLogResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_log", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({
                "job_id": response.job_id,
                "client_id": response.client_id,
            }))
            .summary(json!({
                "stdout_included": response.stdout_tail.is_some(),
                "stderr_included": response.stderr_tail.is_some(),
                "next_stdout_line": response.next_stdout_line,
                "next_stderr_line": response.next_stderr_line,
            })),
    );
}

fn record_shell_job_stop_action(
    audit: &ActionAudit,
    response: &ShellClientJobStopResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_stop", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"job_id": response.job_id}))
            .summary(json!({"status": response.status})),
    );
}

fn record_shell_jobs_list_action(
    audit: &ActionAudit,
    response: &ShellClientJobsListResponse,
    http_status: StatusCode,
) {
    audit.record(
        ActionAuditRecord::new("shell_job_list", response.success, http_status)
            .error(response.error.clone())
            .ids(json!({"client_id": response.client_id}))
            .summary(json!({"jobs_count": response.jobs.len()})),
    );
}

fn render_shell_run(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellRunResponse,
) {
    res.status_code(status);
    record_shell_run_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job_status(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobStatusResponse,
) {
    res.status_code(status);
    record_shell_job_status_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job_log(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobLogResponse,
) {
    res.status_code(status);
    record_shell_job_log_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job_stop_response(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobStopResponse,
) {
    res.status_code(status);
    record_shell_job_stop_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_jobs_list(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellClientJobsListResponse,
) {
    res.status_code(status);
    record_shell_jobs_list_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_file(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellFileOpResponse,
) {
    res.status_code(status);
    record_shell_file_action(audit, &response, status);
    res.render(Json(response));
}

fn render_shell_job(
    res: &mut Response,
    audit: &ActionAudit,
    status: StatusCode,
    response: ShellJobOpResponse,
) {
    res.status_code(status);
    record_shell_job_action(audit, &response, status);
    res.render(Json(response));
}

#[handler]
pub async fn shell_agent_register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellClientRegisterRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    // Phase 3: agent transport endpoints require bootstrap or an agent token
    // with the agent:register scope. User tokens are rejected.
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_REGISTER)
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_register_owner(auth.as_ref(), &body.client_id, body.owner.as_deref()) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some(e),
        }));
        return;
    }
    // Resolve the effective owner: an agent token fills the owner from its
    // own username; bootstrap keeps the request body owner.
    let mut body = body;
    body.owner = effective_register_owner(auth.as_ref(), body.owner.as_deref());
    match registry.register(body).await {
        Ok(client) => res.render(Json(ShellClientRegisterResponse {
            success: true,
            client: Some(client),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_poll(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentPollRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_POLL) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_agent_transport(auth.as_ref(), &body.client_id) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some(e),
        }));
        return;
    }
    match registry.poll(body).await {
        Ok(request) => res.render(Json(ShellAgentPollResponse {
            success: true,
            request,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_result(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentResultRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_RESULT) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_agent_transport(auth.as_ref(), &body.client_id) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some(e),
        }));
        return;
    }
    match registry.complete(body).await {
        Ok(()) => res.render(Json(ShellAgentResultResponse {
            success: true,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/run", "runShell");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_run(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            ShellRunResponse {
                success: false,
                request_id: String::new(),
                client_id: String::new(),
                cwd: None,
                command_preview: String::new(),
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some("Shell client registry not configured".to_string()),
            },
        );
        return;
    };
    let body: ShellRunRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_run(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                ShellRunResponse {
                    success: false,
                    request_id: String::new(),
                    client_id: String::new(),
                    cwd: None,
                    command_preview: String::new(),
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: None,
                    error: Some(format!("Invalid JSON: {}", e)),
                },
            );
            return;
        }
    };
    let wait_timeout_secs = body.wait_timeout_secs;
    let client_id = body.client_id.clone();
    let cwd = body.cwd.clone();
    let preview = command_preview(&body.command);
    if let Err((status, e)) =
        assert_registry_client_owner(&registry, auth.as_ref(), &client_id).await
    {
        render_shell_run(
            res,
            &audit,
            status,
            ShellRunResponse {
                success: false,
                request_id: String::new(),
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some(e),
            },
        );
        return;
    }
    let requested_by = requested_by_from_auth(auth.as_ref());
    let (request_id, rx) = match registry.enqueue_run(body, requested_by).await {
        Ok(result) => result,
        Err(e) => {
            render_shell_run(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                ShellRunResponse {
                    success: false,
                    request_id: String::new(),
                    client_id,
                    cwd,
                    command_preview: preview,
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: None,
                    error: Some(e),
                },
            );
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => render_shell_run(res, &audit, StatusCode::OK, response),
        Ok(Err(_closed)) => render_shell_run(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            ShellRunResponse {
                success: false,
                request_id,
                client_id,
                cwd,
                command_preview: preview,
                exit_code: None,
                stdout: None,
                stderr: None,
                duration_ms: None,
                error: Some("shell request waiter was dropped".to_string()),
            },
        ),
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            render_shell_run(
                res,
                &audit,
                StatusCode::REQUEST_TIMEOUT,
                ShellRunResponse {
                    success: false,
                    request_id,
                    client_id,
                    cwd,
                    command_preview: preview,
                    exit_code: None,
                    stdout: None,
                    stderr: None,
                    duration_ms: None,
                    error: Some(format!(
                        "timed out waiting {} seconds for shell client result",
                        wait_timeout_secs
                    )),
                },
            );
        }
    }
}

fn shell_file_response_from_run(
    op: String,
    path: String,
    cwd: Option<String>,
    request_content: Option<String>,
    response: ShellRunResponse,
) -> ShellFileOpResponse {
    let success = response.error.is_none() && response.exit_code == Some(0);
    let stdout = response.stdout.unwrap_or_default();
    let entries = if op == "list" && success {
        stdout.lines().map(|line| line.to_string()).collect()
    } else {
        Vec::new()
    };
    let content = if op == "read" && success {
        Some(stdout.clone())
    } else {
        None
    };
    let bytes = match op.as_str() {
        "read" => content.as_ref().map(|s| s.len()),
        "write" if success => Some(stdout.trim().parse::<usize>().unwrap_or(0)),
        _ => None,
    };
    let sha256 = match op.as_str() {
        "read" if success => content.as_ref().map(|s| sha256_hex(s)),
        "write" if success => request_content.as_ref().map(|s| sha256_hex(s)),
        _ => None,
    };
    ShellFileOpResponse {
        success,
        op,
        request_id: response.request_id,
        client_id: response.client_id,
        path,
        cwd,
        content,
        entries,
        bytes,
        sha256,
        stderr: response.stderr,
        error: response.error,
    }
}

#[handler]
pub async fn shell_file_op(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/file", "shellFileOp");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        let response = shell_file_error_response(
            "unknown".to_string(),
            String::new(),
            String::new(),
            None,
            "Shell client registry not configured".to_string(),
        );
        render_shell_file(res, &audit, StatusCode::INTERNAL_SERVER_ERROR, response);
        return;
    };
    let body: ShellFileOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            let response = shell_file_error_response(
                "unknown".to_string(),
                String::new(),
                String::new(),
                None,
                format!("Invalid JSON: {}", e),
            );
            render_shell_file(res, &audit, StatusCode::BAD_REQUEST, response);
            return;
        }
    };
    let op = body.op.clone();
    let client_id = body.client_id.clone();
    let path = body.path.clone();
    let cwd = body.cwd.clone();
    let request_content = body.content.clone();
    let wait_timeout_secs = body.wait_timeout_secs;
    if let Err((status, e)) =
        assert_registry_client_owner(&registry, auth.as_ref(), &client_id).await
    {
        let response = shell_file_error_response(op, client_id, path, cwd, e);
        render_shell_file(res, &audit, status, response);
        return;
    }
    let requested_by = requested_by_from_auth(auth.as_ref());
    let (request_id, rx) = match registry.enqueue_file_op(body, requested_by).await {
        Ok(result) => result,
        Err(e) => {
            let response = shell_file_error_response(op, client_id, path, cwd, e);
            render_shell_file(res, &audit, StatusCode::BAD_REQUEST, response);
            return;
        }
    };
    match tokio::time::timeout(std::time::Duration::from_secs(wait_timeout_secs), rx).await {
        Ok(Ok(response)) => render_shell_file(
            res,
            &audit,
            StatusCode::OK,
            shell_file_response_from_run(op, path, cwd, request_content, response),
        ),
        Ok(Err(_closed)) => {
            let response = shell_file_error_response(
                op,
                client_id,
                path,
                cwd,
                "shell file request waiter was dropped".to_string(),
            );
            render_shell_file(res, &audit, StatusCode::INTERNAL_SERVER_ERROR, response);
        }
        Err(_elapsed) => {
            registry.cancel_request(&request_id).await;
            let response = shell_file_error_response(
                op,
                client_id,
                path,
                cwd,
                format!(
                    "timed out waiting {} seconds for shell file result",
                    wait_timeout_secs
                ),
            );
            render_shell_file(res, &audit, StatusCode::REQUEST_TIMEOUT, response);
        }
    }
}

fn shell_file_error_response(
    op: String,
    client_id: String,
    path: String,
    cwd: Option<String>,
    error: String,
) -> ShellFileOpResponse {
    ShellFileOpResponse {
        success: false,
        op,
        request_id: String::new(),
        client_id,
        path,
        cwd,
        content: None,
        entries: Vec::new(),
        bytes: None,
        sha256: None,
        stderr: None,
        error: Some(error),
    }
}

fn shell_job_error_response(op: String, error: String) -> ShellJobOpResponse {
    ShellJobOpResponse {
        success: false,
        op,
        job: None,
        jobs: Vec::new(),
        stdout: None,
        stderr: None,
        next_stdout_line: None,
        next_stderr_line: None,
        error: Some(error),
    }
}

fn shell_job_status_response_from_job(job: ShellJobInfo) -> ShellClientJobStatusResponse {
    ShellClientJobStatusResponse {
        success: true,
        job_id: Some(job.job_id.clone()),
        client_id: Some(job.client_id.clone()),
        kind: Some(job.kind.clone()),
        status: Some(job.status.clone()),
        elapsed_secs: job.elapsed_secs,
        exit_code: job.exit_code,
        result: job.result.clone(),
        job: Some(job),
        error: None,
    }
}

fn shell_job_status_error_response(error: String) -> ShellClientJobStatusResponse {
    ShellClientJobStatusResponse {
        success: false,
        job_id: None,
        client_id: None,
        kind: None,
        status: None,
        elapsed_secs: None,
        exit_code: None,
        result: None,
        job: None,
        error: Some(error),
    }
}

fn shell_job_log_error_response(error: String) -> ShellClientJobLogResponse {
    ShellClientJobLogResponse {
        success: false,
        job_id: None,
        client_id: None,
        stdout_tail: None,
        stderr_tail: None,
        next_stdout_line: None,
        next_stderr_line: None,
        job: None,
        error: Some(error),
    }
}

fn shell_job_stop_error_response(error: String) -> ShellClientJobStopResponse {
    ShellClientJobStopResponse {
        success: false,
        job_id: None,
        status: None,
        job: None,
        error: Some(error),
    }
}

fn shell_jobs_list_error_response(client_id: String, error: String) -> ShellClientJobsListResponse {
    ShellClientJobsListResponse {
        success: false,
        client_id,
        jobs: Vec::new(),
        error: Some(error),
    }
}

async fn authorize_job_access(
    registry: &ShellClientRegistry,
    auth: Option<&crate::auth::AuthContext>,
    job_id: &str,
    requested_client_id: Option<&str>,
) -> Result<ShellJobInfo, (StatusCode, String)> {
    let job = registry
        .get_job(job_id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    if let Some(requested_client_id) = requested_client_id {
        if requested_client_id != job.client_id {
            return Err((
                StatusCode::FORBIDDEN,
                format!(
                    "job_id {} belongs to client {}, not {}",
                    job_id, job.client_id, requested_client_id
                ),
            ));
        }
    }
    assert_registry_client_owner(registry, auth, &job.client_id).await?;
    Ok(job)
}

#[handler]
pub async fn shell_job(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/job", "runShellJob");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_error_response(
                "unknown".to_string(),
                "Shell client registry not configured".to_string(),
            ),
        );
        return;
    };
    let body: ShellJobOpRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_error_response("unknown".to_string(), format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    let op = body.op.clone();
    match op.as_str() {
        "start" => {
            let Some(client_id) = body.client_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "client_id is required for op=start".to_string()),
                );
                return;
            };
            if let Err((status, e)) =
                assert_registry_client_owner(&registry, auth.as_ref(), client_id).await
            {
                render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                return;
            }
            let requested_by = requested_by_from_auth(auth.as_ref());
            match registry.start_job(body, requested_by).await {
                Ok(job) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::OK,
                    ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout: None,
                        stderr: None,
                        next_stdout_line: None,
                        next_stderr_line: None,
                        error: None,
                    },
                ),
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        "status" => {
            let Some(job_id) = body.job_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "job_id is required for op=status".to_string()),
                );
                return;
            };
            match registry.get_job(job_id).await {
                Ok(job) => {
                    if let Err((status, e)) =
                        assert_registry_client_owner(&registry, auth.as_ref(), &job.client_id).await
                    {
                        render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                        return;
                    }
                    render_shell_job(
                        res,
                        &audit,
                        StatusCode::OK,
                        ShellJobOpResponse {
                            success: true,
                            op,
                            job: Some(job),
                            jobs: Vec::new(),
                            stdout: None,
                            stderr: None,
                            next_stdout_line: None,
                            next_stderr_line: None,
                            error: None,
                        },
                    )
                }
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        "list" => {
            let limit = body.limit.unwrap_or(20).clamp(1, 100);
            let mut jobs = Vec::new();
            for job in registry.list_jobs(Some(100)).await {
                if auth.as_ref().map(|auth| auth.is_bootstrap).unwrap_or(false) {
                    jobs.push(job);
                    continue;
                }
                let Some(client) = registry.get_client_view(&job.client_id).await else {
                    continue;
                };
                if assert_shell_client_owner(auth.as_ref(), &job.client_id, client.owner.as_deref())
                    .is_ok()
                {
                    jobs.push(job);
                }
            }
            jobs.truncate(limit);
            render_shell_job(
                res,
                &audit,
                StatusCode::OK,
                ShellJobOpResponse {
                    success: true,
                    op,
                    job: None,
                    jobs,
                    stdout: None,
                    stderr: None,
                    next_stdout_line: None,
                    next_stderr_line: None,
                    error: None,
                },
            );
        }
        "log" => {
            let Some(job_id) = body.job_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "job_id is required for op=log".to_string()),
                );
                return;
            };
            let job = match registry.get_job(job_id).await {
                Ok(job) => job,
                Err(e) => {
                    render_shell_job(
                        res,
                        &audit,
                        StatusCode::BAD_REQUEST,
                        shell_job_error_response(op, e),
                    );
                    return;
                }
            };
            if let Err((status, e)) =
                assert_registry_client_owner(&registry, auth.as_ref(), &job.client_id).await
            {
                render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                return;
            }
            match registry
                .job_log(
                    job_id,
                    body.since_stdout_line,
                    body.since_stderr_line,
                    body.tail_lines,
                )
                .await
            {
                Ok((job, stdout, stderr, next_stdout_line, next_stderr_line)) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::OK,
                    ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout,
                        stderr,
                        next_stdout_line: Some(next_stdout_line),
                        next_stderr_line: Some(next_stderr_line),
                        error: None,
                    },
                ),
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        "stop" => {
            let Some(job_id) = body.job_id.as_deref() else {
                render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, "job_id is required for op=stop".to_string()),
                );
                return;
            };
            let job = match registry.get_job(job_id).await {
                Ok(job) => job,
                Err(e) => {
                    render_shell_job(
                        res,
                        &audit,
                        StatusCode::BAD_REQUEST,
                        shell_job_error_response(op, e),
                    );
                    return;
                }
            };
            if let Err((status, e)) =
                assert_registry_client_owner(&registry, auth.as_ref(), &job.client_id).await
            {
                render_shell_job(res, &audit, status, shell_job_error_response(op, e));
                return;
            }
            let requested_by = requested_by_from_auth(auth.as_ref());
            match registry.stop_job(job_id, requested_by).await {
                Ok(job) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::OK,
                    ShellJobOpResponse {
                        success: true,
                        op,
                        job: Some(job),
                        jobs: Vec::new(),
                        stdout: None,
                        stderr: None,
                        next_stdout_line: None,
                        next_stderr_line: None,
                        error: None,
                    },
                ),
                Err(e) => render_shell_job(
                    res,
                    &audit,
                    StatusCode::BAD_REQUEST,
                    shell_job_error_response(op, e),
                ),
            }
        }
        _ => render_shell_job(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_job_error_response(
                op,
                "op must be one of start, status, log, stop, list".to_string(),
            ),
        ),
    }
}

#[handler]
pub async fn shell_job_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/shell/jobs/status",
        "getShellClientJobStatus",
    );
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job_status(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_status_error_response("Shell client registry not configured".to_string()),
        );
        return;
    };
    let body: ShellClientJobStatusRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job_status(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_status_error_response(format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    match authorize_job_access(
        &registry,
        auth.as_ref(),
        &body.job_id,
        body.client_id.as_deref(),
    )
    .await
    {
        Ok(job) => render_shell_job_status(
            res,
            &audit,
            StatusCode::OK,
            shell_job_status_response_from_job(job),
        ),
        Err((status, e)) => {
            render_shell_job_status(res, &audit, status, shell_job_status_error_response(e))
        }
    }
}

#[handler]
pub async fn shell_job_log(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/jobs/log", "getShellClientJobLog");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job_log(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_log_error_response("Shell client registry not configured".to_string()),
        );
        return;
    };
    let body: ShellClientJobLogRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job_log(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_log_error_response(format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    let job = match authorize_job_access(
        &registry,
        auth.as_ref(),
        &body.job_id,
        body.client_id.as_deref(),
    )
    .await
    {
        Ok(job) => job,
        Err((status, e)) => {
            render_shell_job_log(res, &audit, status, shell_job_log_error_response(e));
            return;
        }
    };
    match registry
        .job_log(
            &body.job_id,
            body.since_stdout_line,
            body.since_stderr_line,
            body.tail_lines,
        )
        .await
    {
        Ok((job, stdout_tail, stderr_tail, next_stdout_line, next_stderr_line)) => {
            render_shell_job_log(
                res,
                &audit,
                StatusCode::OK,
                ShellClientJobLogResponse {
                    success: true,
                    job_id: Some(job.job_id.clone()),
                    client_id: Some(job.client_id.clone()),
                    stdout_tail,
                    stderr_tail,
                    next_stdout_line: Some(next_stdout_line),
                    next_stderr_line: Some(next_stderr_line),
                    job: Some(job),
                    error: None,
                },
            );
        }
        Err(e) => render_shell_job_log(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_job_log_error_response(e),
        ),
    }
    let _ = job;
}

#[handler]
pub async fn shell_job_stop(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/jobs/stop", "stopShellClientJob");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_job_stop_response(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_job_stop_error_response("Shell client registry not configured".to_string()),
        );
        return;
    };
    let body: ShellClientJobStopRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_job_stop_response(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_job_stop_error_response(format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    if let Err((status, e)) = authorize_job_access(
        &registry,
        auth.as_ref(),
        &body.job_id,
        body.client_id.as_deref(),
    )
    .await
    {
        render_shell_job_stop_response(res, &audit, status, shell_job_stop_error_response(e));
        return;
    }
    let requested_by = requested_by_from_auth(auth.as_ref());
    match registry.stop_job(&body.job_id, requested_by).await {
        Ok(job) => render_shell_job_stop_response(
            res,
            &audit,
            StatusCode::OK,
            ShellClientJobStopResponse {
                success: true,
                job_id: Some(job.job_id.clone()),
                status: Some(job.status.clone()),
                job: Some(job),
                error: None,
            },
        ),
        Err(e) => render_shell_job_stop_response(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_job_stop_error_response(e),
        ),
    }
}

#[handler]
pub async fn shell_jobs_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/shell/jobs/list", "listShellClientJobs");
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let Some(registry) = get_registry(depot) else {
        render_shell_jobs_list(
            res,
            &audit,
            StatusCode::INTERNAL_SERVER_ERROR,
            shell_jobs_list_error_response(
                String::new(),
                "Shell client registry not configured".to_string(),
            ),
        );
        return;
    };
    let body: ShellClientJobsListRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            render_shell_jobs_list(
                res,
                &audit,
                StatusCode::BAD_REQUEST,
                shell_jobs_list_error_response(String::new(), format!("Invalid JSON: {}", e)),
            );
            return;
        }
    };
    let client_id = body.client_id.clone();
    if let Err((status, e)) =
        assert_registry_client_owner(&registry, auth.as_ref(), &client_id).await
    {
        render_shell_jobs_list(
            res,
            &audit,
            status,
            shell_jobs_list_error_response(client_id, e),
        );
        return;
    }
    match registry
        .list_jobs_for_client(
            &client_id,
            body.status.as_deref(),
            Some(body.limit.unwrap_or(20).clamp(1, 100)),
        )
        .await
    {
        Ok(jobs) => render_shell_jobs_list(
            res,
            &audit,
            StatusCode::OK,
            ShellClientJobsListResponse {
                success: true,
                client_id,
                jobs,
                error: None,
            },
        ),
        Err(e) => render_shell_jobs_list(
            res,
            &audit,
            StatusCode::BAD_REQUEST,
            shell_jobs_list_error_response(client_id, e),
        ),
    }
}

#[handler]
pub async fn shell_agent_job_update(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentJobUpdateRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentJobUpdateResponse {
                success: false,
                job: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if let Err(e) =
        require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_JOB_UPDATE)
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_agent_transport(auth.as_ref(), &body.client_id) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some(e),
        }));
        return;
    }
    match registry.update_job(body).await {
        Ok(job) => res.render(Json(ShellAgentJobUpdateResponse {
            success: true,
            job: Some(job),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentJobUpdateResponse {
                success: false,
                job: None,
                error: Some(e),
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_protocol::AGENT_PROTOCOL_VERSION_QUIC_V1;

    fn auth_context(username: Option<&str>, is_bootstrap: bool) -> crate::auth::AuthContext {
        let (role, scopes) = if is_bootstrap {
            ("admin".to_string(), vec!["admin".to_string()])
        } else {
            ("user".to_string(), Vec::new())
        };
        crate::auth::AuthContext {
            kind: if is_bootstrap {
                crate::auth::AuthKind::Bootstrap
            } else {
                crate::auth::AuthKind::ApiToken
            },
            user_id: username.map(|username| format!("user-{}", username)),
            username: username.map(str::to_string),
            api_key_id: username.map(|username| format!("key-{}", username)),
            api_key_name: username.map(|username| format!("{} key", username)),
            role: Some(role),
            scopes,
            is_bootstrap,
            token_kind: if is_bootstrap {
                None
            } else {
                Some("user".to_string())
            },
            allowed_client_id: None,
        }
    }

    /// Phase 3 test helper: build an agent-token AuthContext bound to
    /// `username` and `allowed_client_id`, carrying the given agent scopes.
    fn agent_auth_context(
        username: &str,
        allowed_client_id: &str,
        scopes: Vec<&str>,
    ) -> crate::auth::AuthContext {
        crate::auth::AuthContext {
            kind: crate::auth::AuthKind::AgentToken,
            user_id: Some(format!("user-{}", username)),
            username: Some(username.to_string()),
            api_key_id: Some("key-agent".to_string()),
            api_key_name: Some("agent key".to_string()),
            role: Some("user".to_string()),
            scopes: scopes.into_iter().map(str::to_string).collect(),
            is_bootstrap: false,
            token_kind: Some("agent".to_string()),
            allowed_client_id: Some(allowed_client_id.to_string()),
        }
    }

    fn project_summary(id: &str, path: &str) -> ShellAgentProjectSummary {
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(id.to_string()),
            path: path.to_string(),
            allow_patch: true,
            kind: Some("rust".to_string()),
            description: Some("test project".to_string()),
            hooks: vec!["doctor".to_string(), "precommit".to_string()],
            disabled: false,
            git_branch: Some("codex".to_string()),
            git_head: Some("9a7d3ce".to_string()),
            git_dirty: Some(false),
            updated_at: 123456,
            shell_profile: None,
        }
    }

    fn async_job_capabilities() -> ShellClientCapabilities {
        let mut capabilities = ShellClientCapabilities::default();
        capabilities.async_jobs = true;
        capabilities.async_shell_jobs = true;
        capabilities.jobs = true;
        capabilities
    }

    fn file_request(op: &str) -> ShellFileOpRequest {
        ShellFileOpRequest {
            op: op.to_string(),
            client_id: "oe".to_string(),
            path: "src/auth/scopes.rs".to_string(),
            cwd: Some("/root/git/webcodex".to_string()),
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            wait_timeout_secs: 0,
        }
    }

    #[test]
    fn validate_file_request_allows_read_with_start_and_end_line() {
        let mut req = file_request("read");
        req.start_line = Some(10);
        req.end_line = Some(20);

        validate_file_request(&req).unwrap();
    }

    #[test]
    fn validate_file_request_rejects_read_with_only_start_line() {
        let mut req = file_request("read");
        req.start_line = Some(10);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(
            err,
            "end_line is required when start_line is set for op=read"
        );
    }

    #[test]
    fn validate_file_request_rejects_read_with_only_end_line() {
        let mut req = file_request("read");
        req.end_line = Some(20);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(
            err,
            "start_line is required when end_line is set for op=read"
        );
    }

    #[test]
    fn validate_file_request_rejects_read_with_invalid_range() {
        let mut req = file_request("read");
        req.start_line = Some(20);
        req.end_line = Some(10);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "invalid line range");

        req.start_line = Some(0);
        req.end_line = Some(10);
        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "invalid line range");
    }

    #[test]
    fn validate_file_request_rejects_read_with_line_field() {
        let mut req = file_request("read");
        req.line = Some(10);

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "line is only allowed for op=insert_at_line");
    }

    #[test]
    fn validate_file_request_rejects_read_with_expected_prefix() {
        let mut req = file_request("read");
        req.expected_prefix = Some("pub fn".to_string());

        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, "expected_prefix is only allowed for line edit ops");
    }

    #[test]
    fn requested_by_from_auth_uses_bootstrap_username_or_anonymous() {
        let bootstrap = auth_context(None, true);
        assert_eq!(requested_by_from_auth(Some(&bootstrap)), "bootstrap");

        let alice = auth_context(Some("alice"), false);
        assert_eq!(requested_by_from_auth(Some(&alice)), "alice");

        assert_eq!(requested_by_from_auth(None), "anonymous");
    }

    #[test]
    fn assert_shell_client_owner_enforces_owner_boundary() {
        let bootstrap = auth_context(None, true);
        assert!(assert_shell_client_owner(Some(&bootstrap), "client-1", None).is_ok());

        let alice = auth_context(Some("alice"), false);
        assert!(assert_shell_client_owner(Some(&alice), "client-1", Some("alice")).is_ok());

        let mismatch =
            assert_shell_client_owner(Some(&alice), "client-1", Some("bob")).unwrap_err();
        assert!(mismatch.contains("owned by bob"));
        assert!(mismatch.contains("belongs to alice"));

        let missing = assert_shell_client_owner(Some(&alice), "client-1", None).unwrap_err();
        assert_eq!(missing, "agent client client-1 has no owner");

        let anonymous = assert_shell_client_owner(None, "client-1", Some("anonymous")).unwrap_err();
        assert!(anonymous.contains("belongs to anonymous"));
    }

    #[tokio::test]
    async fn registry_registers_and_lists_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: Some("XRH".to_string()),
                owner: Some("yyjeqhc".to_string()),
                hostname: Some("fineserver".to_string()),
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].client_id, "xrh");
        assert!(clients[0].connected);
        assert_eq!(clients[0].pending_requests, 0);
    }

    #[tokio::test]
    async fn registry_register_saves_projects() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("webcodex", "/root/git/webcodex")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].projects.len(), 1);
        assert_eq!(clients[0].projects[0].id, "webcodex");

        let projects = registry.list_client_projects("oe").await.unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].path, "/root/git/webcodex");
    }

    #[tokio::test]
    async fn registry_poll_updates_projects() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("one", "/tmp/one")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: Some(vec![
                    project_summary("one", "/tmp/one"),
                    project_summary("two", "/tmp/two"),
                ]),
            })
            .await
            .unwrap();
        assert!(polled.is_none());

        let projects = registry.list_client_projects("oe").await.unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].id, "one");
        assert_eq!(projects[1].id, "two");
    }

    #[tokio::test]
    async fn registry_project_owner_check_enforces_boundary() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "alice-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("webcodex", "/root/git/webcodex")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "bob-client".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("bob".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![project_summary("secret", "/tmp/secret")]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();

        let alice = auth_context(Some("alice"), false);
        assert!(
            assert_registry_client_owner(&registry, Some(&alice), "alice-client")
                .await
                .is_ok()
        );
        let projects = registry.list_client_projects("alice-client").await.unwrap();
        assert_eq!(projects.len(), 1);

        let mismatch = assert_registry_client_owner(&registry, Some(&alice), "bob-client")
            .await
            .unwrap_err();
        assert_eq!(mismatch.0, StatusCode::FORBIDDEN);
        assert!(mismatch.1.contains("owned by bob"));
    }

    #[test]
    fn protocol_async_capability_defaults_false() {
        let capabilities = ShellClientCapabilities::default();
        assert!(!capabilities.async_jobs);
        assert!(!capabilities.async_shell_jobs);

        let request: ShellClientRegisterRequest = serde_json::from_str(
            r#"{
                "client_id": "oe",
                "agent_instance_id": "inst-1",
                "capabilities": {"shell": true}
            }"#,
        )
        .unwrap();
        let capabilities = request.capabilities.unwrap();
        assert!(!capabilities.async_jobs);
        assert!(!capabilities.async_shell_jobs);
    }

    #[test]
    fn protocol_serde_keeps_old_register_compatible() {
        let request: ShellClientRegisterRequest = serde_json::from_str(
            r#"{
                "client_id": "oe",
                "agent_instance_id": "inst-1",
                "capabilities": {"shell": true, "file_read": true}
            }"#,
        )
        .unwrap();
        assert_eq!(request.client_id, "oe");
        assert!(request.projects.is_none());
        // Old agents omit agent_protocol_version; the field deserializes as None.
        assert!(request.agent_protocol_version.is_none());
    }

    #[test]
    fn protocol_serde_parses_agent_protocol_version() {
        let request: ShellClientRegisterRequest = serde_json::from_str(
            r#"{
                "client_id": "oe",
                "agent_instance_id": "inst-1",
                "agent_protocol_version": "polling-v1"
            }"#,
        )
        .unwrap();
        assert_eq!(
            request.agent_protocol_version.as_deref(),
            Some("polling-v1")
        );
    }

    #[tokio::test]
    async fn register_without_protocol_version_defaults_to_unknown() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].agent_protocol_version, "unknown");
    }

    #[tokio::test]
    async fn register_with_protocol_version_is_exposed_in_view() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].client_id, "xrh");
        assert_eq!(clients[0].agent_protocol_version, "polling-v1");
        let view = registry.get_client_view("xrh").await.unwrap();
        assert_eq!(view.agent_protocol_version, "polling-v1");
    }

    #[tokio::test]
    async fn register_blank_protocol_version_falls_back_to_unknown() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("   ".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].agent_protocol_version, "unknown");
    }

    #[tokio::test]
    async fn client_supports_reflects_registered_capabilities() {
        let registry = ShellClientRegistry::default();
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        caps.file_read = true;
        caps.async_shell_jobs = true;
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(caps),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        assert!(registry.client_supports("oe", "shell").await.unwrap());
        assert!(registry.client_supports("oe", "file_read").await.unwrap());
        assert!(registry
            .client_supports("oe", "async_shell_jobs")
            .await
            .unwrap());
        assert!(!registry.client_supports("oe", "git").await.unwrap());
        // Unknown capability name is false, not an error.
        assert!(!registry.client_supports("oe", "teleport").await.unwrap());
        // Unknown client is a structured error.
        let err = registry
            .client_supports("ghost", "shell")
            .await
            .unwrap_err();
        assert!(err.contains("unknown shell client"));
        let err = registry.get_client_capabilities("ghost").await.unwrap_err();
        assert!(err.contains("unknown shell client"));
    }

    #[tokio::test]
    async fn registry_enqueues_polls_and_completes_shell_request() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let (request_id, rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "xrh".to_string(),
                    cwd: Some("/tmp".to_string()),
                    command: "echo hello".to_string(),
                    stdin: Some("hello stdin".to_string()),
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.request_id, request_id);
        assert_eq!(polled.command, "echo hello");
        assert_eq!(polled.stdin.as_deref(), Some("hello stdin"));
        registry
            .complete(ShellAgentResultRequest {
                client_id: "xrh".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(12),
                error: None,
            })
            .await
            .unwrap();
        let response = rx.await.unwrap();
        assert!(response.success);
        assert_eq!(response.stdout.as_deref(), Some("hello\n"));
    }

    #[tokio::test]
    async fn registry_rejects_unknown_client_run() {
        let registry = ShellClientRegistry::default();
        let err = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "missing".to_string(),
                    cwd: None,
                    command: "pwd".to_string(),
                    stdin: None,
                    timeout_secs: 10,
                    wait_timeout_secs: 1,
                },
                "test".to_string(),
            )
            .await
            .unwrap_err();
        assert!(err.contains("unknown shell client"));
    }

    async fn register_quic_v1_client(registry: &ShellClientRegistry, client_id: &str) {
        registry
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: Some(vec![project_summary("webcodex", "/tmp/webcodex")]),
                agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
                policy: None,
            })
            .await
            .unwrap();
        registry
            .set_transport(client_id, TRANSPORT_QUIC)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_run_queueing() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "quic-run").await;

        let (_request_id, _rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "quic-run".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
        let view = registry.get_client_view("quic-run").await.unwrap();
        assert_eq!(view.transport, TRANSPORT_QUIC);
        assert_eq!(view.agent_protocol_version, AGENT_PROTOCOL_VERSION_QUIC_V1);
        assert_eq!(view.pending_requests, 1);
        assert!(view.capabilities.shell);
        assert!(view.capabilities.async_shell_jobs);
    }

    #[tokio::test]
    async fn enqueue_file_op_allows_read_with_line_range() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "oe").await;

        let mut req = file_request("read");
        req.start_line = Some(7);
        req.end_line = Some(12);
        let (request_id, _rx) = registry
            .enqueue_file_op(req, "tester".to_string())
            .await
            .unwrap();

        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.request_id, request_id);
        assert_eq!(polled.kind, "file_read");
        assert_eq!(polled.path.as_deref(), Some("src/auth/scopes.rs"));
        assert_eq!(polled.start_line, Some(7));
        assert_eq!(polled.end_line, Some(12));
        assert_eq!(polled.line, None);
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_file_and_project_ops_queueing() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "quic-ops").await;

        let (_file_request_id, _file_rx) = registry
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "read".to_string(),
                    client_id: "quic-ops".to_string(),
                    path: "README.md".to_string(),
                    cwd: None,
                    content: None,
                    max_bytes: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        let (_project_request_id, _project_rx) = registry
            .enqueue_project_op(
                "quic-ops".to_string(),
                "register_project",
                "{}".to_string(),
                "tester".to_string(),
            )
            .await
            .unwrap();

        let view = registry.get_client_view("quic-ops").await.unwrap();
        assert_eq!(view.pending_requests, 2);
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_start_job_queueing() {
        let registry = ShellClientRegistry::default();
        register_quic_v1_client(&registry, "quic-job").await;

        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("quic-job".to_string()),
                    cwd: None,
                    command: Some("sleep 1".to_string()),
                    timeout_secs: Some(5),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        let view = registry.get_client_view("quic-job").await.unwrap();
        assert_eq!(view.pending_requests, 1);
        assert_eq!(job.status, "queued");
        assert_eq!(registry.list_jobs(Some(10)).await.len(), 1);
    }

    #[tokio::test]
    async fn registry_allows_quic_v1_stop_job_delivery_queueing() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "quic-stop".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("quic-stop".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "quic-stop".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        registry
            .set_transport("quic-stop", TRANSPORT_QUIC)
            .await
            .unwrap();

        let stopped = registry
            .stop_job(&job.job_id, "tester".to_string())
            .await
            .unwrap();
        let view = registry.get_client_view("quic-stop").await.unwrap();
        assert_eq!(view.pending_requests, 1);
        assert_eq!(stopped.status, "stop_requested");
    }

    #[test]
    fn validate_run_request_allows_bounded_stdin_beyond_command_limit() {
        let body = ShellRunRequest {
            client_id: "client-1".to_string(),
            cwd: None,
            command: "cat >/dev/null".to_string(),
            stdin: Some("x".repeat(MAX_COMMAND_LEN + 1024)),
            timeout_secs: 10,
            wait_timeout_secs: 1,
        };
        validate_run_request(&body).expect("stdin has its own larger bound");
    }

    #[test]
    fn validate_run_request_rejects_oversized_stdin() {
        let body = ShellRunRequest {
            client_id: "client-1".to_string(),
            cwd: None,
            command: "cat >/dev/null".to_string(),
            stdin: Some("x".repeat(MAX_RUN_STDIN_BYTES + 1)),
            timeout_secs: 10,
            wait_timeout_secs: 1,
        };
        let err = validate_run_request(&body).unwrap_err();
        assert!(err.contains("stdin is too large"), "got: {}", err);
    }

    #[tokio::test]
    async fn registry_shell_job_start_poll_complete_and_log() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: Some("/tmp".to_string()),
                    command: Some("printf hello".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: Some(ShellJobCodexMetadata {
                        project: Some("demo".to_string()),
                        goal_id: Some("goal-1".to_string()),
                        client_request_id: Some("crid-1".to_string()),
                        command: Some("printf hello".to_string()),
                        kind: Some("command".to_string()),
                        suite: None,
                        script_path: None,
                        reason: Some("test job".to_string()),
                        max_runtime_secs: Some(10),
                    }),
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(job.status, "queued");
        assert_eq!(
            job.codex
                .as_ref()
                .and_then(|codex| codex.client_request_id.as_deref()),
            Some("crid-1")
        );
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(polled.command, "printf hello");
        let running = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(running.status, "agent_queued");
        registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: polled.request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(20),
                error: None,
            })
            .await
            .unwrap();
        let done = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(done.status, "completed");
        assert_eq!(done.exit_code, Some(0));
        assert_eq!(
            done.codex
                .as_ref()
                .and_then(|codex| codex.project.as_deref()),
            Some("demo")
        );
        let listed = registry.list_jobs(Some(10)).await;
        assert_eq!(
            listed
                .iter()
                .find(|listed| listed.job_id == job.job_id)
                .and_then(|listed| listed.codex.as_ref())
                .and_then(|codex| codex.goal_id.as_deref()),
            Some("goal-1")
        );
        let (_info, stdout, stderr, next_stdout, next_stderr) = registry
            .job_log(&job.job_id, Some(1), Some(1), None)
            .await
            .unwrap();
        assert_eq!(stdout.as_deref(), Some("hello\n"));
        assert_eq!(stderr.as_deref(), Some(""));
        assert_eq!(next_stdout, 2);
        assert_eq!(next_stderr, 1);
    }

    #[tokio::test]
    async fn registry_shell_job_stop_cancels_queued_job() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let stopped = registry
            .stop_job(&job.job_id, "test".to_string())
            .await
            .unwrap();
        assert_eq!(stopped.status, "stopped");
        let polled = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        assert!(polled.is_none());
    }

    #[tokio::test]
    async fn registry_shell_job_stop_running_delivers_stop_to_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let started = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(started.kind, "start_job");

        let stop_requested = registry
            .stop_job(&job.job_id, "test".to_string())
            .await
            .unwrap();
        assert_eq!(stop_requested.status, "stop_requested");
        let stop = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stop.kind, "stop_job");
        assert_eq!(stop.job_id.as_deref(), Some(job.job_id.as_str()));
    }

    #[tokio::test]
    async fn registry_marks_running_job_lost_when_client_stale() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();
        {
            let mut inner = registry.inner.lock().await;
            let client = inner.clients.get_mut("oe").unwrap();
            client.last_seen = now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1;
        }
        let lost = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(lost.status, "lost");
        assert!(lost.error.unwrap().contains("stale"));
    }

    #[tokio::test]
    async fn touch_client_refreshes_stale_client_back_to_online() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();

        // Age the client past the online window so it reads as stale.
        registry
            .set_last_seen_for_test("oe", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
            .await;
        let stale = registry.get_client_view("oe").await.unwrap();
        assert!(!stale.connected);
        assert_eq!(stale.status, "stale");

        // A keepalive touch must bring it back online.
        registry.touch_client("oe", "inst").await.unwrap();
        let fresh = registry.get_client_view("oe").await.unwrap();
        assert!(fresh.connected);
        assert_eq!(fresh.status, "online");

        // Unknown client_id is a clear error and does not mutate state.
        assert!(registry.touch_client("nope", "inst").await.is_err());
    }

    #[tokio::test]
    async fn touch_client_refreshes_websocket_transport_client() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "ws-1".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        registry
            .set_transport("ws-1", TRANSPORT_WEBSOCKET)
            .await
            .unwrap();

        registry
            .set_last_seen_for_test("ws-1", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
            .await;
        let stale = registry.get_client_view("ws-1").await.unwrap();
        assert_eq!(stale.transport, "websocket");
        assert!(!stale.connected);

        registry.touch_client("ws-1", "inst").await.unwrap();
        let fresh = registry.get_client_view("ws-1").await.unwrap();
        assert_eq!(fresh.transport, "websocket");
        assert!(fresh.connected);
        assert_eq!(fresh.status, "online");
    }

    #[tokio::test]
    async fn touch_client_rejects_stale_instance_and_accepts_active() {
        // Regression: a stale/replaced instance must not refresh the active
        // lease's `last_seen` via Ping/Pong keepalive.
        let registry = ShellClientRegistry::default();
        // Instance A registers and is online.
        let view_a = register_with_instance(&registry, "oe", "inst-a").await;
        assert!(view_a.connected);

        // Age A out so a newer instance may take over the lease.
        registry
            .set_last_seen_for_test("oe", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
            .await;
        // Instance B replaces A.
        let view_b = register_with_instance(&registry, "oe", "inst-b").await;
        assert_eq!(view_b.agent_instance_id, "inst-b");
        assert!(view_b.connected);

        // Capture B's last_seen right after registration.
        let before = registry.get_client_view("oe").await.unwrap().last_seen;
        // Sleep a moment so a successful touch would observably advance
        // last_seen.
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;

        // Stale instance A's keepalive must be rejected and must NOT advance
        // last_seen for B.
        let err = registry.touch_client("oe", "inst-a").await.unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );
        let after_a = registry.get_client_view("oe").await.unwrap().last_seen;
        assert_eq!(
            after_a, before,
            "stale instance touch must not refresh active last_seen"
        );
        // A stale instance must not resurrect the client to online either.
        let view_after_a = registry.get_client_view("oe").await.unwrap();
        assert!(view_after_a.connected);

        // Active instance B's keepalive succeeds and refreshes last_seen.
        registry.touch_client("oe", "inst-b").await.unwrap();
        let after_b = registry.get_client_view("oe").await.unwrap().last_seen;
        assert!(
            after_b > before,
            "active instance touch must refresh last_seen"
        );
        assert!(registry.get_client_view("oe").await.unwrap().connected);

        // An empty agent_instance_id is rejected by validation.
        assert!(registry.touch_client("oe", "").await.is_err());
    }

    #[test]
    fn enforce_register_owner_skips_when_no_auth() {
        // No AuthMiddleware (unit tests): defer to the middleware, which in
        // production rejects anonymous requests before the handler runs.
        assert!(enforce_register_owner(None, "client-1", Some("anyone")).is_ok());
        assert!(enforce_register_owner(None, "client-1", None).is_ok());
    }

    #[test]
    fn enforce_register_owner_bootstrap_allows_any_owner() {
        let bootstrap = auth_context(None, true);
        assert!(enforce_register_owner(Some(&bootstrap), "client-1", None).is_ok());
        assert!(enforce_register_owner(Some(&bootstrap), "client-1", Some("bob")).is_ok());
    }

    #[test]
    fn enforce_register_owner_user_token_is_rejected() {
        // Phase 3: user tokens (Phase 2 personal API tokens) are no longer
        // allowed on agent transport endpoints. Only bootstrap or agent tokens
        // may register.
        let alice = auth_context(Some("alice"), false);
        let err = enforce_register_owner(Some(&alice), "client-1", Some("alice")).unwrap_err();
        assert!(err.contains("user tokens are not allowed"), "got: {}", err);
    }

    #[test]
    fn enforce_register_owner_agent_token_matching_client_id_succeeds() {
        let alice = agent_auth_context(
            "alice",
            "alice-laptop",
            vec![
                "agent:register",
                "agent:poll",
                "agent:result",
                "agent:job_update",
            ],
        );
        // Matching client_id + matching owner -> Ok.
        assert!(enforce_register_owner(Some(&alice), "alice-laptop", Some("alice")).is_ok());
        // Matching client_id + missing owner -> Ok (owner filled in by the
        // caller via effective_register_owner).
        assert!(enforce_register_owner(Some(&alice), "alice-laptop", None).is_ok());
    }

    #[test]
    fn enforce_register_owner_agent_token_wrong_client_id_rejected() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
        let err = enforce_register_owner(Some(&alice), "other-laptop", None).unwrap_err();
        assert!(err.contains("not bound to client_id"), "got: {}", err);
    }

    #[test]
    fn enforce_register_owner_agent_token_owner_mismatch_rejected() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
        let err = enforce_register_owner(Some(&alice), "alice-laptop", Some("bob")).unwrap_err();
        assert!(err.contains("agent token owner is 'alice'"), "got: {}", err);
        assert!(err.contains("bob"), "got: {}", err);
    }

    #[test]
    fn effective_register_owner_agent_token_fills_username() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
        // Missing owner -> filled with the token's username.
        assert_eq!(
            effective_register_owner(Some(&alice), None),
            Some("alice".to_string())
        );
        // Matching owner preserved.
        assert_eq!(
            effective_register_owner(Some(&alice), Some("alice")),
            Some("alice".to_string())
        );
        // Bootstrap keeps the request owner.
        let bootstrap = auth_context(None, true);
        assert_eq!(
            effective_register_owner(Some(&bootstrap), Some("bob")),
            Some("bob".to_string())
        );
    }

    #[test]
    fn enforce_agent_transport_rejects_user_token() {
        let alice = auth_context(Some("alice"), false);
        let err = enforce_agent_transport(Some(&alice), "client-1").unwrap_err();
        assert!(err.contains("user tokens are not allowed"), "got: {}", err);
    }

    #[test]
    fn enforce_agent_transport_agent_token_matching_client_succeeds() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
        assert!(enforce_agent_transport(Some(&alice), "alice-laptop").is_ok());
        let err = enforce_agent_transport(Some(&alice), "other").unwrap_err();
        assert!(err.contains("not bound"), "got: {}", err);
    }

    #[test]
    fn enforce_agent_transport_bootstrap_succeeds() {
        let bootstrap = auth_context(None, true);
        assert!(enforce_agent_transport(Some(&bootstrap), "any-client").is_ok());
    }

    #[test]
    fn require_agent_transport_scope_agent_token_with_scope_succeeds() {
        let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
        assert!(require_agent_transport_scope(Some(&alice), "agent:poll").is_ok());
        assert!(require_agent_transport_scope(Some(&alice), "agent:register").is_err());
    }

    #[test]
    fn require_agent_transport_scope_bootstrap_always_succeeds() {
        let bootstrap = auth_context(None, true);
        assert!(require_agent_transport_scope(Some(&bootstrap), "agent:register").is_ok());
    }

    #[test]
    fn require_agent_transport_scope_user_token_rejected() {
        let alice = auth_context(Some("alice"), false);
        let err = require_agent_transport_scope(Some(&alice), "agent:register").unwrap_err();
        assert!(err.contains("missing required scope"), "got: {}", err);
    }

    #[tokio::test]
    async fn registry_rejects_enqueue_when_queue_full() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "full".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        // Fill the queue to the limit without any consumer draining it.
        for _ in 0..MAX_QUEUED_REQUESTS_PER_CLIENT {
            registry
                .enqueue_run(
                    ShellRunRequest {
                        client_id: "full".to_string(),
                        cwd: None,
                        command: "echo hi".to_string(),
                        stdin: None,
                        timeout_secs: 5,
                        wait_timeout_secs: 0,
                    },
                    "tester".to_string(),
                )
                .await
                .unwrap();
        }
        // The next enqueue must be rejected with a structured error instead
        // of growing the queue unboundedly.
        let err = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "full".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap_err();
        assert!(err.contains("too many pending requests"));
        assert!(err.contains("full"));
        // The queue is exactly at the cap; memory is bounded.
        let view = registry.get_client_view("full").await.unwrap();
        assert_eq!(view.pending_requests, MAX_QUEUED_REQUESTS_PER_CLIENT);
    }

    #[tokio::test]
    async fn reconcile_disconnect_marks_running_jobs_lost() {
        let registry = ShellClientRegistry::default();
        registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "test".to_string(),
            )
            .await
            .unwrap();
        // Job is "queued" with its request sitting in the client's queue.
        let before = registry.get_client_view("oe").await.unwrap();
        assert_eq!(before.pending_requests, 1);
        // Transport disconnects (e.g. WebSocket dropped).
        registry.reconcile_disconnect("oe", "inst").await;
        let lost = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(lost.status, "lost");
        assert!(lost.error.unwrap().contains("disconnected"));
        // Pending request was dropped: no dangling waiter / queue entry.
        let after = registry.get_client_view("oe").await.unwrap();
        assert_eq!(after.pending_requests, 0);
    }

    #[tokio::test]
    async fn reconcile_disconnect_releases_active_lease_immediately() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;

        registry.reconcile_disconnect("oe", "inst-a").await;

        let offline = registry.get_client_view("oe").await.unwrap();
        assert!(
            !offline.connected,
            "active disconnect must immediately leave online window"
        );
        assert!(now_ts().saturating_sub(offline.last_seen) > CLIENT_ONLINE_WINDOW_SECS);

        let new_view = register_with_instance(&registry, "oe", "inst-b").await;
        assert_eq!(new_view.agent_instance_id, "inst-b");
        assert!(
            new_view.connected,
            "new instance should register without waiting 60 seconds"
        );
    }

    // ------------------------------------------------------------------------
    // Agent instance identity / lease model (Phase 1)
    // ------------------------------------------------------------------------

    /// Helper: register a client with an explicit `agent_instance_id`.
    async fn register_with_instance(
        registry: &ShellClientRegistry,
        client_id: &str,
        instance: &str,
    ) -> ShellClientView {
        registry
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: instance.to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn lease_first_register_accepts_instance() {
        let registry = ShellClientRegistry::default();
        let view = register_with_instance(&registry, "oe", "inst-a").await;
        assert_eq!(view.agent_instance_id, "inst-a");
        assert!(view.connected);
        // The view/list path exposes the instance id.
        let clients = registry.list_clients().await;
        assert_eq!(clients[0].agent_instance_id, "inst-a");
    }

    #[tokio::test]
    async fn lease_same_instance_reregister_accepts() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Same client_id + same instance id is a reconnect/refresh: accepted.
        let _ = register_with_instance(&registry, "oe", "inst-a").await;
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
        assert!(view.connected);
    }

    #[tokio::test]
    async fn lease_different_online_instance_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // A second process with the same client_id but a different instance
        // must be rejected while the first is online.
        let err = registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: Some(async_job_capabilities()),
                projects: None,
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap_err();
        assert!(err.contains("already online"), "error was: {err}");
        assert!(err.contains("different instance"), "error was: {err}");
        // The active instance is unchanged.
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
    }

    #[tokio::test]
    async fn lease_stale_replaced_by_different_instance_accepts() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Age the first instance past the online window so it reads as stale.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        // A different instance may now take over the lease.
        let _ = register_with_instance(&registry, "oe", "inst-b").await;
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-b");
        assert!(view.connected);
    }

    #[tokio::test]
    async fn lease_stale_instance_poll_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Replace with a newer instance after aging out.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;

        // The stale instance A can no longer poll.
        let err = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                projects: None,
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );

        // The active instance B can still poll.
        registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                projects: None,
            })
            .await
            .expect("active instance must poll");
    }

    #[tokio::test]
    async fn lease_stale_instance_result_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Enqueue a request and let instance A poll it.
        let (request_id, _rx) = registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "oe".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
        let _ = registry
            .poll(ShellAgentPollRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                projects: None,
            })
            .await
            .unwrap()
            .unwrap();

        // Replace instance A with B after aging out.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;

        // The stale instance A cannot submit the result.
        let err = registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                request_id: request_id.clone(),
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );

        // The active instance B can submit the result.
        registry
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                request_id,
                exit_code: Some(0),
                stdout: Some("hi".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .expect("active instance must submit result");
    }

    #[tokio::test]
    async fn lease_stale_instance_job_update_rejected() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // Replace instance A with B after aging out.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;

        // The stale instance A cannot update the job.
        let err = registry
            .update_job(ShellAgentJobUpdateRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-a".to_string(),
                job_id: job.job_id.clone(),
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
            })
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );

        // The active instance B can update the job.
        registry
            .update_job(ShellAgentJobUpdateRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst-b".to_string(),
                job_id: job.job_id.clone(),
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
            })
            .await
            .expect("active instance must update job");
    }

    #[tokio::test]
    async fn lease_list_clients_exposes_instance_id() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        let clients = registry.list_clients().await;
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].agent_instance_id, "inst-a");
        let view = registry.get_client_view("oe").await.unwrap();
        assert_eq!(view.agent_instance_id, "inst-a");
    }

    #[tokio::test]
    async fn lease_reconcile_disconnect_stale_instance_is_noop() {
        // A stale instance disconnecting after a newer instance has taken over
        // must NOT clear the active notifier or mark the active instance's
        // jobs lost.
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Install a notifier for instance A.
        let notify_a = Arc::new(Notify::new());
        registry
            .register_notifier("oe", "inst-a", notify_a.clone())
            .await
            .unwrap();
        // Start a job under instance A.
        let job = registry
            .start_job(
                ShellJobOpRequest {
                    op: "start".to_string(),
                    client_id: Some("oe".to_string()),
                    cwd: None,
                    command: Some("sleep 10".to_string()),
                    timeout_secs: Some(10),
                    job_id: None,
                    since_stdout_line: None,
                    since_stderr_line: None,
                    tail_lines: None,
                    limit: None,
                    codex: None,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();

        // Age out A and let B take over.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;
        // B installs its own notifier.
        let notify_b = Arc::new(Notify::new());
        registry
            .register_notifier("oe", "inst-b", notify_b.clone())
            .await
            .unwrap();

        // A's transport finally disconnects. This must be a no-op: B's notifier
        // stays and B's job is not marked lost.
        registry.reconcile_disconnect("oe", "inst-a").await;
        let job_view = registry.get_job(&job.job_id).await.unwrap();
        assert_ne!(
            job_view.status, "lost",
            "stale disconnect must not mark active instance job lost"
        );
        // B's disconnect, however, does reconcile.
        registry.reconcile_disconnect("oe", "inst-b").await;
        let job_view = registry.get_job(&job.job_id).await.unwrap();
        assert_eq!(job_view.status, "lost");
    }

    #[tokio::test]
    async fn lease_register_notifier_rejects_stale_instance() {
        let registry = ShellClientRegistry::default();
        register_with_instance(&registry, "oe", "inst-a").await;
        // Replace A with B.
        registry
            .set_last_seen_for_test("oe", chrono::Utc::now().timestamp() - 120)
            .await;
        register_with_instance(&registry, "oe", "inst-b").await;
        // A's late notifier registration must be rejected so it cannot
        // overwrite B's notifier.
        let err = registry
            .register_notifier("oe", "inst-a", Arc::new(Notify::new()))
            .await
            .unwrap_err();
        assert!(
            err.contains("no longer the active instance"),
            "error was: {err}"
        );
        // B can still install its notifier.
        registry
            .register_notifier("oe", "inst-b", Arc::new(Notify::new()))
            .await
            .expect("active instance must install notifier");
    }

    #[tokio::test]
    async fn lease_register_rejects_empty_instance_id() {
        let registry = ShellClientRegistry::default();
        let err = registry
            .register(ShellClientRegisterRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap_err();
        assert!(err.contains("agent_instance_id"), "error was: {err}");
    }
}
