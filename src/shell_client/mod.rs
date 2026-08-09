use crate::action_audit::{ActionAudit, ActionAuditRecord};
#[cfg(test)]
use crate::shell_protocol::{
    AgentPolicySummary, ClaudeCodeProviderStatus, ShellAgentJobUpdateRequest,
    ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellClientCapabilities, ShellClientRegisterRequest, ShellClientView, ShellJobCodexMetadata,
    ToolProvidersStatus, SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
    SHELL_CLIENT_CAPABILITY_FILE_READ, SHELL_CLIENT_CAPABILITY_GIT, SHELL_CLIENT_CAPABILITY_NAMES,
    SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION, SHELL_CLIENT_CAPABILITY_SHELL,
};
use crate::shell_protocol::{
    ShellClientJobLogRequest, ShellClientJobLogResponse, ShellClientJobStatusRequest,
    ShellClientJobStatusResponse, ShellClientJobStopRequest, ShellClientJobStopResponse,
    ShellClientJobsListRequest, ShellClientJobsListResponse, ShellFileOpRequest,
    ShellFileOpResponse, ShellJobInfo, ShellJobOpRequest, ShellJobOpResponse, ShellRunRequest,
    ShellRunResponse,
};
use salvo::prelude::*;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::Notify;

mod agents;
mod auth;
mod handlers;
mod job_updates;
mod jobs;
mod polling;
mod projects;
mod reconciliation;
#[cfg(test)]
mod reconciliation_tests;
mod requests;
mod state;
mod validation;

#[cfg(test)]
pub(crate) use auth::assert_shell_client_owner;
#[cfg(test)]
pub(crate) use auth::ShellClientAuthGroup;
pub(crate) use auth::{
    effective_register_owner, enforce_agent_transport, enforce_register_owner,
    requested_by_from_auth, require_agent_transport_scope,
};
pub use handlers::{
    shell_agent_job_update, shell_agent_persistent_shell_result, shell_agent_poll,
    shell_agent_register, shell_agent_result,
};
#[cfg(test)]
pub(crate) use job_updates::JobLogWaitOutcome;
pub(crate) use job_updates::{ShellJobStartMetadata, StructuredJobExecution};
pub(crate) use jobs::{
    command_preview, process_preview, script_preview, COMMAND_PREVIEW_MAX_CHARS,
};
#[cfg(test)]
pub(crate) use projects::ShellClientLookupError;
pub(crate) use reconciliation::recovery_timeout_sweep;
pub(crate) use requests::EnqueueLspError;
use state::ShellClientRegistryInner;
pub(crate) use state::ShellJobVisibility;
use validation::sha256_hex;
#[cfg(test)]
use validation::{
    validate_file_request, validate_run_request, MAX_COMMAND_LEN, MAX_RUN_STDIN_BYTES,
};

const MAX_OUTPUT_BYTES: usize = 256 * 1024;
pub(crate) const CLIENT_ONLINE_WINDOW_SECS: i64 = 60;
pub(crate) const MAX_SHARED_KEY_RUNNERS_PER_GROUP: usize = 16;
pub(crate) const MAX_SHARED_KEY_RUNNERS_GLOBAL: usize = 1024;
pub(crate) const MAX_RUNNER_PROJECT_SUMMARIES: usize = 64;
pub(crate) const SHARED_KEY_OFFLINE_TTL_SECS: i64 = 24 * 60 * 60;
/// Same-process runners have this long to re-register and submit their
/// complete active inventory before a recovering job becomes terminal lost.
/// This is the documented production default; tests and operators may lower it
/// via `WEBCODEX_JOB_RECOVERY_GRACE_SECS` (clamped to
/// [`JOB_RECOVERY_GRACE_MIN_SECS`]..=[`JOB_RECOVERY_GRACE_MAX_SECS`]). The
/// resolved value is read once per process via [`job_recovery_grace_secs`].
pub(crate) const JOB_RECOVERY_GRACE_SECS: i64 = 120;
/// Lower bound for the resolved recovery grace. Anything shorter risks
/// mistaking a briefly-flapping transport for a permanently-gone runner, so
/// the override is refused below this floor even in tests.
pub(crate) const JOB_RECOVERY_GRACE_MIN_SECS: i64 = 5;
/// Upper bound for the resolved recovery grace. Prevents a misconfigured
/// operator from effectively disabling the deadline forever.
pub(crate) const JOB_RECOVERY_GRACE_MAX_SECS: i64 = 3600;

/// Period of the in-process recovery-timeout sweep, in seconds. Not
/// configurable: the grace window (`WEBCODEX_JOB_RECOVERY_GRACE_SECS`) is the
/// operator-facing deadline control, and this interval only bounds how long
/// after the deadline a job waits before being transitioned to `lost` (at most
/// one interval). `MissedTickBehavior::Delay` prevents burst catch-up.
pub(crate) const RECOVERY_SWEEP_INTERVAL_SECS: u64 = 30;

/// Clamp a raw recovery-grace value to the safe `[min, max]` window. Pure so it
/// can be unit-tested without mutating the process env or the resolved cache.
pub(crate) fn clamp_grace(raw: i64) -> i64 {
    raw.clamp(JOB_RECOVERY_GRACE_MIN_SECS, JOB_RECOVERY_GRACE_MAX_SECS)
}

/// Resolve the recovery grace once per process. Reads
/// `WEBCODEX_JOB_RECOVERY_GRACE_SECS`, clamps it to
/// [`JOB_RECOVERY_GRACE_MIN_SECS`]-[`JOB_RECOVERY_GRACE_MAX_SECS`], and falls
/// back to [`JOB_RECOVERY_GRACE_SECS`] when unset or unparseable. Production
/// never mutates the env after startup, so the first read is cached; tests do
/// not rely on this cached value for deadline logic — they manipulate a job's
/// `recovering_since` directly and compute expectations from the documented
/// default, matching the existing test idiom and avoiding env-mutation races.
pub(crate) fn job_recovery_grace_secs() -> i64 {
    static JOB_RECOVERY_GRACE: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *JOB_RECOVERY_GRACE.get_or_init(|| {
        std::env::var("WEBCODEX_JOB_RECOVERY_GRACE_SECS")
            .ok()
            .and_then(|raw| raw.trim().parse::<i64>().ok())
            .map(clamp_grace)
            .unwrap_or(JOB_RECOVERY_GRACE_SECS)
    })
}
const MAX_RETIRED_INSTANCES_PER_CLIENT: usize = 16;
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

#[derive(Debug, Clone, Copy)]
struct SharedKeyRegistrationLimits {
    per_group: usize,
    global: usize,
    offline_ttl_secs: i64,
}

impl Default for SharedKeyRegistrationLimits {
    fn default() -> Self {
        Self {
            per_group: MAX_SHARED_KEY_RUNNERS_PER_GROUP,
            global: MAX_SHARED_KEY_RUNNERS_GLOBAL,
            offline_ttl_secs: SHARED_KEY_OFFLINE_TTL_SECS,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ShellClientRegistry {
    inner: Arc<Mutex<ShellClientRegistryInner>>,
    observation_epoch: Arc<str>,
    shared_key_limits: SharedKeyRegistrationLimits,
    /// Cancellation intents recorded synchronously by Drop guards before any
    /// asynchronous stop delivery. The periodic registry lifecycle drains this
    /// map, so cleanup does not depend on one detached task getting polled.
    cleanup_intents:
        Arc<std::sync::Mutex<std::collections::HashMap<String, Option<crate::auth::AuthContext>>>>,
}

impl Default for ShellClientRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(ShellClientRegistryInner::default())),
            observation_epoch: Arc::from(crate::job_observation::new_epoch()),
            shared_key_limits: SharedKeyRegistrationLimits::default(),
            cleanup_intents: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }
}

#[cfg(test)]
impl ShellClientRegistry {
    fn with_shared_key_limits_for_test(
        per_group: usize,
        global: usize,
        offline_ttl_secs: i64,
    ) -> Self {
        Self {
            shared_key_limits: SharedKeyRegistrationLimits {
                per_group,
                global,
                offline_ttl_secs,
            },
            ..Self::default()
        }
    }
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn get_registry(depot: &Depot) -> Option<Arc<ShellClientRegistry>> {
    depot.obtain::<Arc<ShellClientRegistry>>().ok().cloned()
}

async fn assert_registry_client_owner(
    registry: &ShellClientRegistry,
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
) -> Result<(), (StatusCode, String)> {
    if registry.get_client_view(client_id).await.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("unknown shell client: {}", client_id),
        ));
    }
    registry
        .assert_client_access(auth, client_id)
        .await
        .map_err(|e| {
            let status = if e.contains("unknown shell client") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::FORBIDDEN
            };
            (status, e)
        })
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
                request_dispatched: None,
                command_execution_state: None,
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
                    request_dispatched: None,
                    command_execution_state: None,
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
                request_dispatched: None,
                command_execution_state: None,
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
                    request_dispatched: None,
                    command_execution_state: None,
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
                request_dispatched: None,
                command_execution_state: None,
            },
        ),
        Err(_elapsed) => {
            let request_dispatched = registry.cancel_request_dispatch_state(&request_id).await;
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
                    request_dispatched,
                    command_execution_state: None,
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
                if auth.as_ref().map(|auth| auth.is_admin()).unwrap_or(false) {
                    jobs.push(job);
                    continue;
                }
                if registry
                    .assert_client_access(auth.as_ref(), &job.client_id)
                    .await
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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
