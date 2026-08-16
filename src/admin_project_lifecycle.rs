use crate::auth::AuthContext;
use crate::db::AdminProjectAudit;
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::ShellAgentProjectSummary;
use crate::tool_runtime::{ToolResult, ToolRuntime};
use crate::Database;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::Mutex;

const WAIT_SECS: u64 = 30;
pub(crate) const IDEMPOTENCY_KEY_MAX: usize = 128;
type IdempotencyLocks = Mutex<HashMap<String, Arc<Mutex<()>>>>;
static IDEMPOTENCY_LOCKS: OnceLock<IdempotencyLocks> = OnceLock::new();

fn idempotency_locks() -> &'static IdempotencyLocks {
    IDEMPOTENCY_LOCKS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegisterProjectRequest {
    pub client_id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub path: String,
    #[serde(default = "default_true")]
    pub allow_patch: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CreateProjectRequest {
    pub client_id: String,
    pub project_id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub path: String,
    #[serde(default = "default_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub git_init: bool,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub allow_existing_empty: bool,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectMutationRequest {
    pub project: String,
    pub expected_revision: String,
    pub idempotency_key: String,
    pub confirm: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone)]
pub(crate) struct ServiceResponse {
    pub status: u16,
    pub body: Value,
}

struct ProjectUnregisterFence {
    registry: Arc<ShellClientRegistry>,
    project: String,
}

impl Drop for ProjectUnregisterFence {
    fn drop(&mut self) {
        let registry = self.registry.clone();
        let project = self.project.clone();
        tokio::spawn(async move {
            registry.end_project_unregister(&project).await;
        });
    }
}

#[derive(Clone)]
pub(crate) struct AdminProjectLifecycleService {
    runtime: Arc<ToolRuntime>,
    db: Arc<Database>,
}

impl AdminProjectLifecycleService {
    pub(crate) fn new(runtime: Arc<ToolRuntime>, db: Arc<Database>) -> Self {
        Self { runtime, db }
    }

    pub(crate) async fn register(
        &self,
        auth: &AuthContext,
        request: RegisterProjectRequest,
    ) -> ServiceResponse {
        let target = format!("agent:{}:{}", request.client_id, request.project_id);
        self.idempotent(
            auth,
            "register",
            &target,
            &request,
            &request.idempotency_key,
            || async {
                validate_common(
                    &request.client_id,
                    &request.project_id,
                    &request.name,
                    request.description.as_deref(),
                    &request.path,
                )?;
                require_online_client(&self.runtime, auth, &request.client_id).await?;
                let result = self
                    .runtime
                    .register_project(
                        request.client_id.clone(),
                        request.project_id.clone(),
                        request.name.clone(),
                        request.path.clone(),
                        request.description.clone(),
                        request.allow_patch,
                        false,
                        Some(auth),
                    )
                    .await;
                map_create_result("register", "registered", &target, result)
            },
        )
        .await
    }

    pub(crate) async fn create(
        &self,
        auth: &AuthContext,
        request: CreateProjectRequest,
    ) -> ServiceResponse {
        let target = format!("agent:{}:{}", request.client_id, request.project_id);
        self.idempotent(
            auth,
            "create",
            &target,
            &request,
            &request.idempotency_key,
            || async {
                validate_common(
                    &request.client_id,
                    &request.project_id,
                    &request.name,
                    request.description.as_deref(),
                    &request.path,
                )?;
                require_online_client(&self.runtime, auth, &request.client_id).await?;
                if let Some(template) = request.template.as_deref() {
                    if template.len() > 32 {
                        return Err(api_error(400, "invalid_request"));
                    }
                }
                let result = self
                    .runtime
                    .create_project(
                        request.client_id.clone(),
                        request.project_id.clone(),
                        request.name.clone(),
                        request.path.clone(),
                        request.description.clone(),
                        request.allow_patch,
                        request.template.clone(),
                        request.git_init,
                        request.allow_existing_empty,
                        false,
                        Some(auth),
                    )
                    .await;
                map_create_result("create", "created", &target, result)
            },
        )
        .await
    }

    pub(crate) async fn mutate(
        &self,
        auth: &AuthContext,
        action: &'static str,
        request: ProjectMutationRequest,
    ) -> ServiceResponse {
        let target = request.project.clone();
        self.idempotent(
            auth,
            action,
            &target,
            &request,
            &request.idempotency_key,
            || async {
                if !request.confirm {
                    return Err(api_error(400, "invalid_request"));
                }
                Self::mutate_authorized_core(
                    self.runtime.as_ref(),
                    Some(auth),
                    action,
                    &request.project,
                    &request.expected_revision,
                    "admin_project_lifecycle",
                    false,
                )
                .await
            },
        )
        .await
    }

    /// Narrow project-authorized unregister entry used by ordinary runtime
    /// callers such as the hosted `webcodex disconnect` flow. Authorization is
    /// still resolved through the caller-visible Runner/project inventory; this
    /// does not grant access to any other admin lifecycle operation.
    pub(crate) async fn unregister_authorized(
        &self,
        auth: &AuthContext,
        project: &str,
        expected_revision: &str,
    ) -> ServiceResponse {
        unregister_project_runtime(
            self.runtime.as_ref(),
            Some(auth),
            project,
            expected_revision,
        )
        .await
    }

    async fn mutate_authorized_core(
        runtime: &ToolRuntime,
        auth: Option<&AuthContext>,
        action: &'static str,
        target: &str,
        expected_revision: &str,
        requester: &'static str,
        require_owner_access: bool,
    ) -> Result<ServiceResponse, ServiceResponse> {
        validate_revision(expected_revision)?;
        let (client_id, project_id) = parse_runtime_project(target)?;
        // Authenticated ordinary-runtime callers keep the explicit Runner owner/access
        // fence used by the HTTP unregister path. `auth=None` is the trusted in-process
        // / open-runtime path: visibility is intentionally unfiltered there, matching
        // other ToolRuntime operations, so requiring a user owner would incorrectly
        // reject otherwise reachable unowned Runners.
        if require_owner_access && auth.is_some() {
            runtime
                .shell_clients
                .assert_client_access(auth, &client_id)
                .await
                .map_err(|_| api_error(503, "agent_unavailable"))?;
        }
        let client = runtime
            .shell_clients
            .get_client_view_for_auth(&client_id, auth)
            .await
            .ok_or_else(|| api_error(503, "agent_unavailable"))?;
        if !client.connected || client.status != "online" {
            return Err(api_error(503, "agent_unavailable"));
        }
        if !client.capabilities.project_lifecycle {
            return Err(api_error(409, "unsupported_runner_version"));
        }
        let project = client.projects.iter().find(|p| p.id == project_id);
        if action != "unregister" && project.is_none() {
            return Err(api_error(404, "project_not_found"));
        }
        let (active_jobs, _unregister_fence) = if action == "unregister" {
            let active = runtime
                .shell_clients
                .begin_project_unregister(auth, target)
                .await
                .map_err(|_| api_error(500, "operation_failed"))?;
            if active > 0 {
                return Err(ServiceResponse {
                    status: 409,
                    body: json!({"error":{"code":"active_jobs_conflict"},"active_jobs":active}),
                });
            }
            (
                active,
                Some(ProjectUnregisterFence {
                    registry: runtime.shell_clients.clone(),
                    project: target.to_string(),
                }),
            )
        } else {
            (
                runtime
                    .shell_clients
                    .count_active_jobs_for_project(auth, target)
                    .await,
                None,
            )
        };
        let payload = serde_json::to_string(&json!({
            "project_id": project_id,
            "expected_revision": expected_revision,
        }))
        .map_err(|_| api_error(500, "operation_failed"))?;
        let kind = format!("project_lifecycle_{action}");
        let (request_id, receiver) = runtime
            .shell_clients
            .enqueue_project_op(client_id.clone(), &kind, payload, requester.to_string())
            .await
            .map_err(|_| api_error(503, "agent_unavailable"))?;
        let response = match tokio::time::timeout(Duration::from_secs(WAIT_SECS), receiver).await {
            Ok(Ok(value)) => value,
            Ok(Err(_)) | Err(_) => {
                runtime.shell_clients.cancel_request(&request_id).await;
                return Err(api_error(503, "operation_indeterminate"));
            }
        };
        if let Some(error) = response.error.as_deref() {
            return Err(map_agent_error(error));
        }
        let output: Value = serde_json::from_str(response.stdout.as_deref().unwrap_or(""))
            .map_err(|_| api_error(502, "operation_failed"))?;
        if let Some(code) = output.get("error_code").and_then(Value::as_str) {
            return Err(map_agent_error(code));
        }
        if response.exit_code != Some(0) {
            return Err(api_error(500, "operation_failed"));
        }
        let outcome = output
            .get("outcome")
            .and_then(Value::as_str)
            .ok_or_else(|| api_error(502, "operation_failed"))?;
        let changed = output
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let revision = output.get("revision").cloned().unwrap_or(Value::Null);
        if action == "unregister" && matches!(outcome, "unregistered" | "already_unregistered") {
            let _ = runtime
                .shell_clients
                .remove_client_project(&client_id, &project_id)
                .await;
        } else if let Some(summary) = lifecycle_summary(&output, &project_id) {
            let _ = runtime
                .shell_clients
                .upsert_client_project(&client_id, summary)
                .await;
        }
        let warnings = if active_jobs > 0 {
            json!([{"code":"active_jobs_present","active_jobs":active_jobs}])
        } else {
            json!([])
        };
        Ok(ServiceResponse {
            status: 200,
            body: json!({
                "operation": action,
                "project": target,
                "outcome": outcome,
                "changed": changed,
                "revision": revision,
                "active_jobs": active_jobs,
                "warnings": warnings
            }),
        })
    }

    async fn idempotent<T, F, Fut>(
        &self,
        auth: &AuthContext,
        action: &str,
        target: &str,
        request: &T,
        key: &str,
        operation: F,
    ) -> ServiceResponse
    where
        T: Serialize,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<ServiceResponse, ServiceResponse>>,
    {
        if !valid_idempotency_key(key) {
            return api_error(400, "invalid_request");
        }
        let subject = subject_id(auth);
        let key_hash = digest(key.as_bytes());
        let request_hash = digest(&serde_json::to_vec(request).unwrap_or_default());
        let lock_scope = format!("{subject}\u{1f}{action}\u{1f}{target}\u{1f}{key_hash}");
        let operation_lock = {
            let mut locks = idempotency_locks().lock().await;
            if locks.len() > 2_048 {
                locks.retain(|_, lock| Arc::strong_count(lock) > 1);
            }
            locks
                .entry(lock_scope.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _operation_guard = operation_lock.lock().await;
        match self
            .db
            .get_admin_project_idempotency(&subject, action, target, &key_hash)
        {
            Ok(Some(stored)) => {
                let stored_response = ServiceResponse {
                    status: stored.http_status as u16,
                    body: serde_json::from_str(&stored.response_json)
                        .unwrap_or_else(|_| json!({"error":{"code":"operation_failed"}})),
                };
                match stored_idempotency_action(
                    &stored.request_hash,
                    &request_hash,
                    &stored_response,
                ) {
                    StoredIdempotencyAction::Conflict => {
                        return api_error(409, "idempotency_conflict");
                    }
                    StoredIdempotencyAction::DeleteAndRetry => {
                        if self
                            .db
                            .delete_admin_project_idempotency(&subject, action, target, &key_hash)
                            .is_err()
                        {
                            return api_error(500, "operation_failed");
                        }
                    }
                    StoredIdempotencyAction::Replay => return stored_response,
                }
            }
            Err(_) => return api_error(500, "operation_failed"),
            Ok(None) => {}
        }
        let response = match operation().await {
            Ok(v) | Err(v) => v,
        };
        let correlation_id = uuid::Uuid::new_v4().to_string();
        let outcome = response
            .body
            .get("outcome")
            .and_then(Value::as_str)
            .unwrap_or("failed");
        let changed = response
            .body
            .get("changed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let reason_code = response.body.pointer("/error/code").and_then(Value::as_str);
        let client_id = parse_runtime_project(target).ok().map(|(client, _)| client);
        let subject_type = if auth.is_bootstrap() {
            "bootstrap"
        } else {
            "admin_pat"
        };
        let _ = self
            .db
            .insert_admin_project_lifecycle_audit(&AdminProjectAudit {
                correlation_id: &correlation_id,
                subject_type,
                subject_id: &subject,
                operation: action,
                project: target,
                client_id: client_id.as_deref(),
                outcome,
                changed,
                reason_code,
                idempotency_digest: &key_hash,
            });
        if !is_persistable_terminal(&response) {
            return response;
        }
        let response_json = serde_json::to_string(&response.body)
            .unwrap_or_else(|_| "{\"error\":{\"code\":\"operation_failed\"}}".to_string());
        match self.db.insert_admin_project_idempotency(
            &subject,
            action,
            target,
            &key_hash,
            &request_hash,
            response.status as i64,
            &response_json,
        ) {
            Ok(true) => response,
            Ok(false) => match self
                .db
                .get_admin_project_idempotency(&subject, action, target, &key_hash)
            {
                Ok(Some(stored)) if stored.request_hash == request_hash => ServiceResponse {
                    status: stored.http_status as u16,
                    body: serde_json::from_str(&stored.response_json).unwrap_or(response.body),
                },
                _ => api_error(409, "idempotency_conflict"),
            },
            Err(_) => api_error(503, "operation_indeterminate"),
        }
    }
}

/// Shared ordinary-runtime unregister path used by both the dedicated HTTP
/// endpoint and the model-facing runtime tool. The lifecycle core owns exact
/// revision validation, owner filtering, active-Job fencing, Runner capability
/// checks, uncertain delivery semantics, and Server inventory removal.
pub(crate) async fn unregister_project_runtime(
    runtime: &ToolRuntime,
    auth: Option<&AuthContext>,
    project: &str,
    expected_revision: &str,
) -> ServiceResponse {
    match AdminProjectLifecycleService::mutate_authorized_core(
        runtime,
        auth,
        "unregister",
        project,
        expected_revision,
        "project_unregister",
        true,
    )
    .await
    {
        Ok(response) | Err(response) => response,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum StoredIdempotencyAction {
    Conflict,
    DeleteAndRetry,
    Replay,
}

fn stored_idempotency_action(
    stored_hash: &str,
    request_hash: &str,
    response: &ServiceResponse,
) -> StoredIdempotencyAction {
    if stored_hash != request_hash {
        StoredIdempotencyAction::Conflict
    } else if is_persistable_terminal(response) {
        StoredIdempotencyAction::Replay
    } else {
        StoredIdempotencyAction::DeleteAndRetry
    }
}

fn is_persistable_terminal(response: &ServiceResponse) -> bool {
    if !(200..300).contains(&response.status) {
        return false;
    }
    matches!(
        response.body.get("outcome").and_then(Value::as_str),
        Some(
            "registered"
                | "created"
                | "enabled"
                | "disabled"
                | "unregistered"
                | "already_enabled"
                | "already_disabled"
                | "already_unregistered"
        )
    )
}

async fn require_online_client(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: &str,
) -> Result<(), ServiceResponse> {
    let client = runtime
        .shell_clients
        .get_client_view_for_auth(client_id, Some(auth))
        .await
        .ok_or_else(|| api_error(503, "agent_unavailable"))?;
    if !client.connected || client.status != "online" {
        return Err(api_error(503, "agent_unavailable"));
    }
    Ok(())
}

fn map_create_result(
    operation: &str,
    outcome: &str,
    project: &str,
    result: ToolResult,
) -> Result<ServiceResponse, ServiceResponse> {
    if !result.success {
        let code = result
            .output
            .get("error_code")
            .and_then(Value::as_str)
            .or(result.error.as_deref())
            .unwrap_or("operation_failed");
        return Err(map_agent_error(code));
    }
    let actual_outcome = result
        .output
        .get("outcome")
        .and_then(Value::as_str)
        .ok_or_else(|| api_error(502, "operation_failed"))?;
    if actual_outcome != outcome {
        return Err(api_error(502, "operation_failed"));
    }
    let changed = result
        .output
        .get("changed")
        .and_then(Value::as_bool)
        .ok_or_else(|| api_error(502, "operation_failed"))?;
    let recovered = result
        .output
        .get("recovered")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let revision = result
        .output
        .get("revision")
        .cloned()
        .filter(|v| v.as_str().is_some())
        .ok_or_else(|| api_error(502, "operation_failed"))?;
    Ok(ServiceResponse {
        status: 200,
        body: json!({
            "operation": operation, "project": project, "outcome": actual_outcome,
            "changed": changed, "recovered": recovered, "revision": revision, "warnings": []
        }),
    })
}

fn lifecycle_summary(output: &Value, id: &str) -> Option<ShellAgentProjectSummary> {
    Some(ShellAgentProjectSummary {
        id: id.to_string(),
        name: output
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(id.to_string())),
        path: output.get("path")?.as_str()?.to_string(),
        allow_patch: output
            .get("allow_patch")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        kind: None,
        description: output
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        hooks: Vec::new(),
        disabled: output
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        revision: output
            .get("revision")
            .and_then(Value::as_str)
            .map(str::to_string),
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: chrono::Utc::now().timestamp(),
        shell_profile: None,
    })
}

fn validate_common(
    client: &str,
    project: &str,
    name: &str,
    description: Option<&str>,
    path: &str,
) -> Result<(), ServiceResponse> {
    if client.is_empty()
        || client.len() > 128
        || !client
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(api_error(400, "invalid_request"));
    }
    if project.is_empty()
        || project.len() > 64
        || !project
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(api_error(400, "invalid_request"));
    }
    if name.trim().is_empty()
        || name.len() > 120
        || path.is_empty()
        || path.len() > 4096
        || !path.starts_with('/')
        || description.is_some_and(|v| v.len() > 500)
    {
        return Err(api_error(400, "invalid_request"));
    }
    Ok(())
}

fn validate_revision(value: &str) -> Result<(), ServiceResponse> {
    if value.len() != 71
        || !value.starts_with("sha256:")
        || !value[7..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(api_error(400, "invalid_request"));
    }
    Ok(())
}

fn parse_runtime_project(value: &str) -> Result<(String, String), ServiceResponse> {
    let rest = value
        .strip_prefix("agent:")
        .ok_or_else(|| api_error(400, "invalid_request"))?;
    let (client, project) = rest
        .split_once(':')
        .ok_or_else(|| api_error(400, "invalid_request"))?;
    if client.is_empty() || project.is_empty() || project.contains(':') {
        return Err(api_error(400, "invalid_request"));
    }
    Ok((client.to_string(), project.to_string()))
}

fn valid_idempotency_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= IDEMPOTENCY_KEY_MAX
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}
fn subject_id(auth: &AuthContext) -> String {
    if auth.is_bootstrap() {
        "bootstrap".to_string()
    } else {
        auth.api_key_id
            .clone()
            .or_else(|| auth.user_id.clone())
            .unwrap_or_else(|| "admin".to_string())
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
fn api_error(status: u16, code: &str) -> ServiceResponse {
    ServiceResponse {
        status,
        body: json!({"error":{"code":code}}),
    }
}
fn map_agent_error(error: &str) -> ServiceResponse {
    match error {
        "revision_conflict" => api_error(409, "revision_conflict"),
        "project_not_found" => api_error(404, "project_not_found"),
        "path_outside_allowed_roots" => api_error(400, "path_outside_allowed_roots"),
        "path_not_empty" => api_error(409, "path_not_empty"),
        "project_already_exists" => api_error(409, "project_already_exists"),
        "unsupported_runner_version" => api_error(409, "unsupported_runner_version"),
        "agent_unavailable" => api_error(503, "agent_unavailable"),
        "operation_indeterminate" => api_error(503, "operation_indeterminate"),
        _ => api_error(500, "operation_failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;
    use crate::shell_client::ShellJobStartMetadata;
    use crate::shell_protocol::{
        ShellAgentProjectSummary, ShellClientCapabilities, ShellClientRegisterRequest,
        ShellJobOpRequest,
    };

    fn user_auth(username: &str) -> AuthContext {
        AuthContext {
            kind: AuthKind::ApiToken,
            user_id: Some(format!("user-{username}")),
            username: Some(username.to_string()),
            api_key_id: Some(format!("key-{username}")),
            role: Some("user".to_string()),
            scopes: vec![crate::auth::scopes::SCOPE_PROJECT_WRITE.to_string()],
            is_bootstrap: false,
            token_kind: Some("user".to_string()),
            allowed_client_id: None,
            shared_key_hash: None,
            project_grant_id: None,
        }
    }

    fn active_job_request(client_id: &str, command: &str) -> ShellJobOpRequest {
        ShellJobOpRequest {
            op: "start".to_string(),
            client_id: Some(client_id.to_string()),
            cwd: None,
            command: Some(command.to_string()),
            timeout_secs: Some(60),
            job_id: None,
            since_stdout_line: None,
            since_stderr_line: None,
            tail_lines: None,
            limit: None,
            codex: None,
        }
    }

    #[tokio::test]
    async fn project_unregister_rejects_cross_owner_before_active_job_fence() {
        let registry = Arc::new(ShellClientRegistry::default());
        let revision = format!("sha256:{}", "a".repeat(64));
        let target = "agent:owned-runner:demo";
        registry
            .register(ShellClientRegisterRequest {
                client_id: "owned-runner".to_string(),
                agent_instance_id: "instance-owned".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                host_context: None,
                capabilities: Some(ShellClientCapabilities {
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                    project_lifecycle: true,
                    ..Default::default()
                }),
                projects: Some(vec![ShellAgentProjectSummary {
                    id: "demo".to_string(),
                    name: Some("demo".to_string()),
                    path: "/tmp/demo".to_string(),
                    allow_patch: true,
                    kind: None,
                    description: None,
                    hooks: Vec::new(),
                    disabled: false,
                    revision: Some(revision.clone()),
                    git_branch: None,
                    git_head: None,
                    git_dirty: None,
                    updated_at: 1,
                    shell_profile: None,
                }]),
                agent_protocol_version: None,
                policy: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
            })
            .await
            .unwrap();

        let alice = user_auth("alice");
        let bob = user_auth("bob");
        registry
            .start_job_with_metadata_for_auth(
                active_job_request("owned-runner", "sleep 60"),
                "alice".to_string(),
                ShellJobStartMetadata {
                    project_id: Some(target.to_string()),
                    ..Default::default()
                },
                Some(&alice),
            )
            .await
            .unwrap();
        assert_eq!(
            registry
                .count_active_jobs_for_project(Some(&alice), target)
                .await,
            1
        );
        assert_eq!(
            registry.count_active_jobs_for_project(Some(&bob), target).await,
            0,
            "the regression requires the cross-owner principal to be unable to see the owner's active Job"
        );

        let runtime = Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
            registry.clone(),
        ));
        let (_tmp, db) = crate::test_support::test_db();
        let service = AdminProjectLifecycleService::new(runtime, db);
        let response = tokio::time::timeout(
            Duration::from_millis(250),
            service.unregister_authorized(&bob, target, &revision),
        )
        .await
        .expect(
            "cross-owner unregister must fail before enqueueing or installing an unregister fence",
        );
        assert_eq!(response.status, 503);
        assert_eq!(response.body["error"]["code"], "agent_unavailable");

        registry
            .start_job_with_metadata_for_auth(
                active_job_request("owned-runner", "echo still-allowed"),
                "alice".to_string(),
                ShellJobStartMetadata {
                    project_id: Some(target.to_string()),
                    ..Default::default()
                },
                Some(&alice),
            )
            .await
            .expect("rejected cross-owner unregister must not leave an unregister fence behind");
        assert_eq!(
            registry
                .begin_project_unregister(Some(&alice), target)
                .await
                .unwrap(),
            2,
            "the owner's active Jobs must remain authoritative for the unregister fence"
        );
    }

    #[test]
    fn project_lifecycle_error_mapping_is_stable_and_safe() {
        assert_eq!(map_agent_error("agent_unavailable").status, 503);
        assert_eq!(map_agent_error("revision_conflict").status, 409);
        assert_eq!(map_agent_error("secret internal backtrace").status, 500);
        assert_eq!(
            map_agent_error("secret internal backtrace").body["error"]["code"],
            "operation_failed"
        );
    }

    #[test]
    fn transient_and_conflict_results_are_not_persisted() {
        for code in [
            "agent_unavailable",
            "operation_indeterminate",
            "active_jobs_conflict",
        ] {
            assert!(!is_persistable_terminal(&api_error(503, code)));
        }
        assert!(!is_persistable_terminal(&ServiceResponse {
            status: 409,
            body: json!({"error":{"code":"active_jobs_conflict"}}),
        }));
        assert!(is_persistable_terminal(&ServiceResponse {
            status: 200,
            body: json!({"outcome":"already_disabled"}),
        }));
    }

    #[test]
    fn old_transient_with_different_payload_is_conflict_before_delete() {
        let transient = api_error(503, "agent_unavailable");
        assert_eq!(
            stored_idempotency_action("sha256:old", "sha256:new", &transient),
            StoredIdempotencyAction::Conflict
        );
        assert_eq!(
            stored_idempotency_action("sha256:same", "sha256:same", &transient),
            StoredIdempotencyAction::DeleteAndRetry
        );
    }

    #[test]
    fn create_result_preserves_recovery_metadata() {
        let result = ToolResult::ok(json!({
            "outcome":"created", "changed":false, "recovered":true,
            "revision":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }));
        let response = map_create_result("create", "created", "agent:oe:demo", result).unwrap();
        assert_eq!(response.body["changed"], false);
        assert_eq!(response.body["recovered"], true);
        let invalid = ToolResult::ok(json!({
            "outcome":"registered", "changed":true, "revision":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        }));
        assert_eq!(
            map_create_result("create", "created", "agent:oe:demo", invalid)
                .unwrap_err()
                .status,
            502
        );
    }
    #[test]
    fn project_lifecycle_idempotency_keys_are_bounded() {
        assert!(valid_idempotency_key("req-1:retry_2"));
        assert!(!valid_idempotency_key(""));
        assert!(!valid_idempotency_key("contains space"));
        assert!(!valid_idempotency_key(&"a".repeat(IDEMPOTENCY_KEY_MAX + 1)));
    }
}
