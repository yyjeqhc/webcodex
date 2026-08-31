//! Browser-only hosted Runtime Console API.
//!
//! This surface is intentionally not a model capability. It reuses the normal
//! runtime project authorization and the existing Workflow Session console
//! projection without creating a second store, parser, or observation authority.

use crate::auth::{
    AuthContext, SCOPE_COMMUNICATION_MANAGE, SCOPE_COMMUNICATION_READ, SCOPE_PROJECT_READ,
    SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE,
};
use crate::tool_runtime::sessions::{
    aggregate_console_list, is_valid_session_id, SessionMessageKind, SessionMessagePriority,
    WorkflowSessionConsoleAggregate, WorkflowSessionConsoleAttentionOverview,
    WorkflowSessionConsoleList, WorkflowSessionConsoleListItem,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

const DEFAULT_PROJECT_LIMIT: usize = 50;
const MAX_PROJECT_LIMIT: usize = 100;
const MAX_PROJECT_ID_CHARS: usize = 512;
const MAX_PROJECT_NAME_CHARS: usize = 160;
const MAX_PROJECT_PATH_BYTES: usize = 4096;
const MAX_CLIENT_ID_CHARS: usize = 160;
const MAX_STATUS_CHARS: usize = 64;
const DEFAULT_RUNNER_PROJECT_LIMIT: usize = 24;
const MAX_RUNNER_PROJECT_LIMIT: usize = 32;
const CONSOLE_AGGREGATE_SESSION_LIMIT: usize = 50;
const HOME_PROJECT_SCAN_LIMIT: usize = MAX_PROJECT_LIMIT;
const HOME_SESSIONS_PER_PROJECT_LIMIT: usize = CONSOLE_AGGREGATE_SESSION_LIMIT;
const HOME_RECENT_SESSION_LIMIT: usize = 10;
const DEFAULT_MESSAGE_LIMIT: usize = 100;
const MAX_MESSAGE_LIMIT: usize = 100;
const MAX_OBSERVATION_TOKEN_CHARS: usize = 192;

pub(crate) fn routes() -> Router {
    use crate::route_metadata::{api_path, RouteId};
    Router::new()
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleOverview)).post(overview))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleRunner)).post(runner))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleProjects)).post(projects))
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSessions))
                .post(workflow_sessions),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSession))
                .post(workflow_session),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSessionMessages))
                .post(workflow_session_messages),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSessionObserve))
                .post(workflow_session_observe),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSessionPostMessage))
                .post(workflow_session_post_message),
        )
        .push(
            Router::with_path(api_path(
                RouteId::RuntimeConsoleWorkflowSessionWithdrawMessage,
            ))
            .post(workflow_session_withdraw_message),
        )
        .push(
            Router::with_path(api_path(
                RouteId::RuntimeConsoleWorkflowSessionReplaceMessage,
            ))
            .post(workflow_session_replace_message),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationAgents))
                .post(communication_agents),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationAgentCreate))
                .post(communication_agent_create),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationAgentUpdate))
                .post(communication_agent_update),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationEndpointAttach))
                .post(communication_endpoint_attach),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationEndpointRenew))
                .post(communication_endpoint_renew),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationEndpointDetach))
                .post(communication_endpoint_detach),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationConversations))
                .post(communication_conversations),
        )
        .push(
            Router::with_path(api_path(
                RouteId::RuntimeConsoleCommunicationConversationCreate,
            ))
            .post(communication_conversation_create),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationConversation))
                .post(communication_conversation),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationMessagePost))
                .post(communication_message_post),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationInbox))
                .post(communication_inbox),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleCommunicationInboxConsume))
                .post(communication_inbox_consume),
        )
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectsInput {
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionsInput {
    project: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionInput {
    project: String,
    session_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OverviewInput {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunnerInput {
    client_id: String,
    #[serde(default)]
    project_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionMessagesInput {
    project: String,
    session_id: String,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionObserveInput {
    project: String,
    session_id: String,
    #[serde(default)]
    after_observation_token: Option<String>,
    #[serde(default)]
    wait_secs: Option<u64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionPostMessageInput {
    project: String,
    session_id: String,
    kind: SessionMessageKind,
    #[serde(default)]
    priority: SessionMessagePriority,
    message: String,
    #[serde(default)]
    reply_to: Option<String>,
    #[serde(default)]
    requires_ack: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionWithdrawMessageInput {
    project: String,
    session_id: String,
    message_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowSessionReplaceMessageInput {
    project: String,
    session_id: String,
    message_id: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationAgentsInput {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationAgentCreateInput {
    handle: String,
    display_name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    specialty_labels: Vec<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationAgentUpdateInput {
    agent_id: String,
    expected_profile_revision: i64,
    #[serde(default)]
    handle: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    specialty_labels: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointAttachInput {
    agent_id: String,
    host: String,
    #[serde(default)]
    client_attachment_id: Option<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointDetachInput {
    endpoint_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationEndpointRenewInput {
    endpoint_id: String,
    expected_controller_generation: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationConversationsInput {
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    endpoint_id: Option<String>,
    #[serde(default)]
    expected_controller_generation: Option<i64>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationConversationCreateInput {
    #[serde(default)]
    title: Option<String>,
    agent_ids: Vec<String>,
    idempotency_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationConversationInput {
    conversation_id: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    endpoint_id: Option<String>,
    #[serde(default)]
    expected_controller_generation: Option<i64>,
    #[serde(default)]
    after_seq: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationMessagePostInput {
    conversation_id: String,
    body: String,
    #[serde(default)]
    author_agent_id: Option<String>,
    #[serde(default)]
    endpoint_id: Option<String>,
    #[serde(default)]
    expected_controller_generation: Option<i64>,
    #[serde(default)]
    recipient_agent_ids: Option<Vec<String>>,
    #[serde(default)]
    reply_to: Option<String>,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(default)]
    wake_reply_id: Option<String>,
    #[serde(default)]
    reply_operation_index: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationInboxInput {
    agent_id: String,
    endpoint_id: String,
    expected_controller_generation: i64,
    #[serde(default)]
    after_delivery_order: Option<i64>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationInboxConsumeInput {
    agent_id: String,
    endpoint_id: String,
    expected_controller_generation: i64,
    delivery_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleOverview {
    service: Option<String>,
    version: Option<String>,
    build_git_commit: Option<String>,
    build_git_dirty: Option<bool>,
    runner_count: usize,
    runners_online: usize,
    runners_stale: usize,
    runners_unavailable: usize,
    source_mismatched_runners: usize,
    mixed_builds_present: bool,
    active_jobs: usize,
    projects_available: bool,
    visible_projects: usize,
    projects_truncated: bool,
    workflow_sessions: RuntimeConsoleWorkflowAggregate,
    recent_sessions: RuntimeConsoleRecentSessions,
    runners: Vec<RuntimeConsoleRunnerSummary>,
    projects: Vec<RuntimeConsoleProject>,
}

#[derive(Debug, Default, Serialize)]
struct RuntimeConsoleWorkflowAggregate {
    active: usize,
    running: usize,
    open_guidance: usize,
    open_questions: usize,
    open_risks: usize,
    open_todos: usize,
    projects_scanned: usize,
    projects_total: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleRecentSessions {
    sessions: Vec<RuntimeConsoleRecentSession>,
    returned: usize,
    candidate_count: usize,
    truncated: bool,
    scan_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleRecentSession {
    client_id: String,
    project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    #[serde(flatten)]
    session: WorkflowSessionConsoleListItem,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleRunnerSummary {
    client_id: String,
    connected: bool,
    status: Option<String>,
    transport: Option<String>,
    agent_protocol_version: Option<String>,
    protocol_compatibility: Option<String>,
    project_inventory_strategy: Option<String>,
    last_seen_age_secs: Option<i64>,
    version: Option<String>,
    build_git_commit: Option<String>,
    build_git_dirty: Option<bool>,
    source_alignment: Option<String>,
    version_matches_server: Option<bool>,
    active_jobs: usize,
    job_concurrency_limit: Option<u64>,
    jobs_running: usize,
    jobs_queued: usize,
    projects_scanned: usize,
    projects_scan_partial: bool,
    sessions: WorkflowSessionConsoleAggregate,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleRunner {
    client_id: String,
    connected: bool,
    status: Option<String>,
    version: Option<String>,
    build_git_commit: Option<String>,
    build_git_dirty: Option<bool>,
    source_alignment: Option<String>,
    active_jobs: usize,
    job_concurrency_limit: Option<u64>,
    jobs_running: usize,
    jobs_queued: usize,
    projects_available: bool,
    visible_project_count: usize,
    projects_returned: usize,
    projects_truncated: bool,
    projects: Vec<RuntimeConsoleRunnerProject>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleRunnerProject {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_status: Option<String>,
    sessions: WorkflowSessionConsoleAggregate,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleMessages {
    session_id: String,
    messages: Vec<RuntimeConsoleMessage>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleWithdrawMessage {
    message: RuntimeConsoleMessage,
    replayed: bool,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleReplaceMessage {
    original: RuntimeConsoleMessage,
    replacement: RuntimeConsoleMessage,
    replayed: bool,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleObservation {
    session_id: String,
    messages: Vec<RuntimeConsoleMessage>,
    observation_token: String,
    changed: bool,
    wait_outcome: String,
    waited_ms: u64,
    history_lost: bool,
    has_more: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleMessage {
    message_id: String,
    kind: String,
    status: String,
    priority: String,
    created_at: i64,
    message: String,
    requires_ack: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_ack_observed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    closure_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    superseded_by_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    supersedes_message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_by_message_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleProjects {
    projects: Vec<RuntimeConsoleProject>,
    total: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleProject {
    id: String,
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions: Option<WorkflowSessionConsoleAggregate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeConsoleError {
    Invalid,
    NotFound,
    Conflict,
    PersistenceUncertain,
    Internal,
    Request { status: u16, message: &'static str },
}

impl RuntimeConsoleError {
    fn status(self) -> StatusCode {
        match self {
            Self::Invalid => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Conflict => StatusCode::CONFLICT,
            Self::PersistenceUncertain => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Request { status, .. } => {
                StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST)
            }
        }
    }

    fn message(self) -> &'static str {
        match self {
            Self::Invalid => "Invalid request",
            Self::NotFound => "Not found",
            Self::Conflict => "Session message is no longer open or conflicts with retained state",
            Self::PersistenceUncertain => {
                "Outcome may have happened; refresh retained messages before retrying"
            }
            Self::Internal => "Runtime Console unavailable",
            Self::Request { message, .. } => message,
        }
    }
}

fn render_error(res: &mut Response, error: RuntimeConsoleError) {
    let status = error.status();
    res.status_code(status);
    res.render(crate::json_error(status, error.message()));
}

fn bounded_text(value: &Value, max_chars: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(max_chars).collect())
}

fn valid_project_id(project: &str) -> bool {
    !project.is_empty()
        && project.len() <= MAX_PROJECT_ID_CHARS
        && !project.chars().any(char::is_control)
}

fn bounded_client_id(value: &Value) -> Option<String> {
    let client_id = value.as_str()?;
    if client_id.is_empty()
        || client_id.chars().count() > MAX_CLIENT_ID_CHARS
        || client_id.chars().any(char::is_control)
    {
        return None;
    }
    Some(client_id.to_string())
}

fn bounded_project_path(value: &Value) -> Option<String> {
    let path = value.as_str()?;
    if path.is_empty() || path.len() > MAX_PROJECT_PATH_BYTES || path.chars().any(char::is_control)
    {
        return None;
    }
    Some(path.to_string())
}

async fn prepared(
    req: &Request,
    depot: &Depot,
) -> Result<(Arc<ToolRuntime>, AuthContext), RuntimeConsoleError> {
    crate::auth::require_json_same_origin(req)
        .map_err(|(status, _code, message)| RuntimeConsoleError::Request { status, message })?;
    let runtime = depot
        .obtain::<Arc<ToolRuntime>>()
        .cloned()
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let auth = depot
        .obtain::<AuthContext>()
        .cloned()
        .map_err(|_| RuntimeConsoleError::Internal)?;
    Ok((runtime, auth))
}

fn require_runtime_read(auth: &AuthContext) -> Result<(), RuntimeConsoleError> {
    if auth.has_scope(SCOPE_RUNTIME_READ) {
        Ok(())
    } else {
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Runtime read access required",
        })
    }
}

fn require_session_collaborate(auth: &AuthContext) -> Result<(), RuntimeConsoleError> {
    if auth.has_scope(SCOPE_SESSION_COLLABORATE) {
        Ok(())
    } else {
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Session collaboration access required",
        })
    }
}

fn require_communication_read(auth: &AuthContext) -> Result<(), RuntimeConsoleError> {
    if auth.has_scope(SCOPE_COMMUNICATION_READ) {
        Ok(())
    } else {
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Communication read access required",
        })
    }
}

fn require_communication_manage(auth: &AuthContext) -> Result<(), RuntimeConsoleError> {
    if auth.has_scope(SCOPE_COMMUNICATION_READ) && auth.has_scope(SCOPE_COMMUNICATION_MANAGE) {
        Ok(())
    } else {
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Communication read and manage access required",
        })
    }
}

fn render_communication_result(res: &mut Response, result: crate::tool_runtime::ToolResult) {
    if result.success {
        res.render(Json(result.output));
        return;
    }
    let error_kind = result
        .output
        .get("error_kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let status = match error_kind {
        "communication_store_unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "communication_idempotency_conflict" | "agent_profile_changed" | "conversation_closed" => {
            StatusCode::CONFLICT
        }
        "agent_not_found"
        | "endpoint_not_found"
        | "conversation_not_found"
        | "message_not_found"
        | "reply_message_not_found"
        | "delivery_not_found" => StatusCode::NOT_FOUND,
        "communication_principal_unavailable" => StatusCode::FORBIDDEN,
        _ => StatusCode::BAD_REQUEST,
    };
    res.status_code(status);
    res.render(Json(result.output));
}

fn require_project_read(auth: &AuthContext) -> Result<(), RuntimeConsoleError> {
    if project_read_available(auth) {
        Ok(())
    } else {
        Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Project read access required",
        })
    }
}

fn project_read_available(auth: &AuthContext) -> bool {
    auth.has_scope(SCOPE_PROJECT_READ)
}

fn safe_usize(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0)
}

fn safe_bool(value: Option<&Value>) -> bool {
    value.and_then(Value::as_bool).unwrap_or(false)
}

fn safe_string(value: Option<&Value>, max_chars: usize) -> Option<String> {
    value.and_then(|value| bounded_text(value, max_chars))
}

fn valid_runtime_message_id(message_id: &str) -> bool {
    message_id.strip_prefix("wc_msg_").is_some_and(|suffix| {
        !suffix.is_empty()
            && suffix
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    })
}

fn session_message_mutation_error(
    error: crate::tool_runtime::sessions::SessionMessageError,
) -> RuntimeConsoleError {
    use crate::tool_runtime::sessions::SessionMessageError;
    match error {
        SessionMessageError::UnknownSession | SessionMessageError::UnknownMessage => {
            RuntimeConsoleError::NotFound
        }
        SessionMessageError::MessageNotOpen
        | SessionMessageError::IdempotencyConflict
        | SessionMessageError::AlreadyCompleted { .. }
        | SessionMessageError::InvalidCompletionState
        | SessionMessageError::InvalidObservationState
        | SessionMessageError::AssignmentStale { .. }
        | SessionMessageError::AssignmentHistoryLost { .. }
        | SessionMessageError::AssignmentTooLarge { .. }
        | SessionMessageError::NotTodo
        | SessionMessageError::SessionClosed { .. } => RuntimeConsoleError::Conflict,
        SessionMessageError::PersistenceUncertain => RuntimeConsoleError::PersistenceUncertain,
        SessionMessageError::InvalidAssignmentFence | SessionMessageError::InvalidInput(_) => {
            RuntimeConsoleError::Invalid
        }
    }
}

fn message_from_value(value: &Value) -> Option<RuntimeConsoleMessage> {
    let message_id = safe_string(value.get("message_id"), 160)?;
    let kind = safe_string(value.get("kind"), 32)?;
    let status = safe_string(value.get("status"), 32)?;
    let priority = safe_string(value.get("priority"), 32)?;
    let created_at = value.get("created_at")?.as_i64()?;
    let message = value.get("message")?.as_str()?.to_string();
    Some(RuntimeConsoleMessage {
        message_id,
        kind,
        status,
        priority,
        created_at,
        message,
        requires_ack: safe_bool(value.get("requires_ack")),
        first_ack_observed_at: value.get("first_ack_observed_at").and_then(Value::as_i64),
        author_session_id: safe_string(value.get("author_session_id"), 160),
        reply_to: safe_string(value.get("reply_to"), 160),
        resolved_at: value.get("resolved_at").and_then(Value::as_i64),
        resolution: value
            .get("resolution")
            .and_then(Value::as_str)
            .map(str::to_string),
        closure_kind: safe_string(value.get("closure_kind"), 32),
        superseded_by_message_id: safe_string(value.get("superseded_by_message_id"), 160),
        supersedes_message_id: safe_string(value.get("supersedes_message_id"), 160),
        resolved_by_message_id: safe_string(value.get("resolved_by_message_id"), 160),
    })
}

fn messages_from_result(
    result: &crate::tool_runtime::ToolResult,
) -> Result<Vec<RuntimeConsoleMessage>, RuntimeConsoleError> {
    if !result.success {
        return Err(RuntimeConsoleError::NotFound);
    }
    let values = result
        .output
        .get("messages")
        .and_then(Value::as_array)
        .ok_or(RuntimeConsoleError::Internal)?;
    if values.len() > MAX_MESSAGE_LIMIT {
        return Err(RuntimeConsoleError::Internal);
    }
    values
        .iter()
        .map(|value| message_from_value(value).ok_or(RuntimeConsoleError::Internal))
        .collect()
}

async fn authorize_runtime_session_project(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    session_id: &str,
    tool_name: &str,
) -> Result<(), RuntimeConsoleError> {
    if !valid_project_id(project) || !is_valid_session_id(session_id) {
        return Err(RuntimeConsoleError::Invalid);
    }
    let resolved = runtime
        .authorize_session_target(session_id, tool_name, Some(auth))
        .await
        .map_err(|_| RuntimeConsoleError::NotFound)?;
    if resolved.as_ref().map(|value| value.resolved_id.as_str()) == Some(project) {
        Ok(())
    } else {
        Err(RuntimeConsoleError::NotFound)
    }
}

fn add_console_aggregate(
    target: &mut RuntimeConsoleWorkflowAggregate,
    aggregate: &WorkflowSessionConsoleAggregate,
) {
    target.active = target.active.saturating_add(aggregate.active_sessions);
    target.running = target.running.saturating_add(aggregate.running_sessions);
    target.open_guidance = target
        .open_guidance
        .saturating_add(aggregate.attention.open_guidance);
    target.open_questions = target
        .open_questions
        .saturating_add(aggregate.attention.open_questions);
    target.open_risks = target
        .open_risks
        .saturating_add(aggregate.attention.open_risks);
    target.open_todos = target
        .open_todos
        .saturating_add(aggregate.attention.open_todos);
    target.truncated |= aggregate.sessions_truncated;
}

fn empty_console_aggregate() -> WorkflowSessionConsoleAggregate {
    WorkflowSessionConsoleAggregate {
        retained_sessions: 0,
        returned_sessions: 0,
        sessions_truncated: false,
        active_sessions: 0,
        running_sessions: 0,
        latest_updated_at: None,
        attention: WorkflowSessionConsoleAttentionOverview {
            open_guidance: 0,
            open_questions: 0,
            open_risks: 0,
            open_todos: 0,
        },
    }
}

fn merge_console_aggregate(
    target: &mut WorkflowSessionConsoleAggregate,
    aggregate: &WorkflowSessionConsoleAggregate,
) {
    target.retained_sessions = target
        .retained_sessions
        .saturating_add(aggregate.retained_sessions);
    target.returned_sessions = target
        .returned_sessions
        .saturating_add(aggregate.returned_sessions);
    target.sessions_truncated |= aggregate.sessions_truncated;
    target.active_sessions = target
        .active_sessions
        .saturating_add(aggregate.active_sessions);
    target.running_sessions = target
        .running_sessions
        .saturating_add(aggregate.running_sessions);
    target.latest_updated_at = match (target.latest_updated_at, aggregate.latest_updated_at) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    };
    target.attention.open_guidance = target
        .attention
        .open_guidance
        .saturating_add(aggregate.attention.open_guidance);
    target.attention.open_questions = target
        .attention
        .open_questions
        .saturating_add(aggregate.attention.open_questions);
    target.attention.open_risks = target
        .attention
        .open_risks
        .saturating_add(aggregate.attention.open_risks);
    target.attention.open_todos = target
        .attention
        .open_todos
        .saturating_add(aggregate.attention.open_todos);
}

fn compare_recent_sessions(
    left: &RuntimeConsoleRecentSession,
    right: &RuntimeConsoleRecentSession,
) -> std::cmp::Ordering {
    let left_working = left.session.running_call || left.session.running_jobs > 0;
    let right_working = right.session.running_call || right.session.running_jobs > 0;
    right_working
        .cmp(&left_working)
        .then_with(|| right.session.updated_at.cmp(&left.session.updated_at))
        .then_with(|| left.client_id.cmp(&right.client_id))
        .then_with(|| left.project_id.cmp(&right.project_id))
        .then_with(|| left.session.session_id.cmp(&right.session.session_id))
}

fn finalize_recent_sessions(
    mut candidates: Vec<RuntimeConsoleRecentSession>,
    scan_truncated: bool,
) -> RuntimeConsoleRecentSessions {
    candidates.sort_by(compare_recent_sessions);
    let candidate_count = candidates.len();
    candidates.truncate(HOME_RECENT_SESSION_LIMIT);
    RuntimeConsoleRecentSessions {
        returned: candidates.len(),
        candidate_count,
        truncated: candidate_count > HOME_RECENT_SESSION_LIMIT,
        scan_truncated,
        sessions: candidates,
    }
}

struct RuntimeConsoleHomeScan {
    workflow: RuntimeConsoleWorkflowAggregate,
    recent_sessions: RuntimeConsoleRecentSessions,
    projects: Vec<RuntimeConsoleProject>,
    runner_sessions: HashMap<String, WorkflowSessionConsoleAggregate>,
    runner_projects_scanned: HashMap<String, usize>,
    project_scan_truncated: bool,
}

fn scan_runtime_home(
    runtime: &ToolRuntime,
    visible: &RuntimeConsoleProjects,
    running_jobs: &RunningJobSnapshot,
) -> RuntimeConsoleHomeScan {
    let mut workflow = RuntimeConsoleWorkflowAggregate {
        projects_total: visible.total,
        ..Default::default()
    };
    let mut recent_candidates = Vec::new();
    let mut projected_projects = Vec::new();
    let mut runner_sessions: HashMap<String, WorkflowSessionConsoleAggregate> = HashMap::new();
    let mut runner_projects_scanned: HashMap<String, usize> = HashMap::new();
    let project_scan_truncated =
        visible.truncated || visible.projects.len().min(HOME_PROJECT_SCAN_LIMIT) < visible.total;
    let mut session_scan_truncated = running_jobs.truncated;

    for project in visible.projects.iter().take(HOME_PROJECT_SCAN_LIMIT) {
        let mut list = runtime
            .workflow_sessions_console_list(&project.id, Some(HOME_SESSIONS_PER_PROJECT_LIMIT));
        apply_running_jobs_to_list(&mut list, &project.id, running_jobs);
        let aggregate = aggregate_console_list(&list);
        add_console_aggregate(&mut workflow, &aggregate);
        workflow.projects_scanned = workflow.projects_scanned.saturating_add(1);
        session_scan_truncated |= aggregate.sessions_truncated;

        merge_console_aggregate(
            runner_sessions
                .entry(project.client_id.clone())
                .or_insert_with(empty_console_aggregate),
            &aggregate,
        );
        runner_projects_scanned
            .entry(project.client_id.clone())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);

        recent_candidates.extend(list.sessions.into_iter().map(|session| {
            RuntimeConsoleRecentSession {
                client_id: project.client_id.clone(),
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                session,
            }
        }));

        let mut projected = project.clone();
        projected.sessions = Some(aggregate);
        projected_projects.push(projected);
    }

    workflow.truncated |= project_scan_truncated || session_scan_truncated;
    let recent_sessions = finalize_recent_sessions(
        recent_candidates,
        project_scan_truncated || session_scan_truncated,
    );
    RuntimeConsoleHomeScan {
        workflow,
        recent_sessions,
        projects: projected_projects,
        runner_sessions,
        runner_projects_scanned,
        project_scan_truncated,
    }
}

fn runner_fleet_rows(
    agents: &Value,
    status_clients: &[Value],
    scan: &RuntimeConsoleHomeScan,
) -> Vec<RuntimeConsoleRunnerSummary> {
    let mut rows = agents
        .get("agents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|agent| {
            let client_id = safe_string(agent.get("client_id"), MAX_CLIENT_ID_CHARS)?;
            let status = status_clients.iter().find(|candidate| {
                candidate.get("client_id").and_then(Value::as_str) == Some(client_id.as_str())
            });
            let build = agent.get("build").unwrap_or(&Value::Null);
            let concurrency = agent.get("job_concurrency").unwrap_or(&Value::Null);
            let projects_scanned = scan
                .runner_projects_scanned
                .get(&client_id)
                .copied()
                .unwrap_or(0);
            let mut sessions = scan
                .runner_sessions
                .get(&client_id)
                .cloned()
                .unwrap_or_else(empty_console_aggregate);
            sessions.sessions_truncated |= scan.project_scan_truncated;
            Some(RuntimeConsoleRunnerSummary {
                client_id: client_id.clone(),
                connected: safe_bool(agent.get("connected")),
                status: safe_string(agent.get("status"), MAX_STATUS_CHARS),
                transport: safe_string(agent.get("transport"), MAX_STATUS_CHARS),
                agent_protocol_version: safe_string(
                    agent.get("agent_protocol_version"),
                    MAX_STATUS_CHARS,
                ),
                protocol_compatibility: status
                    .and_then(|value| {
                        safe_string(value.get("protocol_compatibility"), MAX_STATUS_CHARS)
                    })
                    .or_else(|| safe_string(agent.get("protocol_compatibility"), MAX_STATUS_CHARS)),
                project_inventory_strategy: status
                    .and_then(|value| {
                        safe_string(value.get("project_inventory_strategy"), MAX_STATUS_CHARS)
                    })
                    .or_else(|| {
                        safe_string(agent.get("project_inventory_strategy"), MAX_STATUS_CHARS)
                    }),
                last_seen_age_secs: agent.get("last_seen_age_secs").and_then(Value::as_i64),
                version: safe_string(build.get("version"), 80),
                build_git_commit: status
                    .and_then(|value| safe_string(value.get("build_git_commit"), 80))
                    .or_else(|| safe_string(build.get("git_commit"), 80)),
                build_git_dirty: status
                    .and_then(|value| value.get("build_git_dirty"))
                    .and_then(Value::as_bool)
                    .or_else(|| build.get("git_dirty").and_then(Value::as_bool)),
                source_alignment: status.and_then(|value| {
                    safe_string(
                        value
                            .get("source_alignment")
                            .and_then(|alignment| alignment.get("status")),
                        MAX_STATUS_CHARS,
                    )
                }),
                version_matches_server: status
                    .and_then(|value| value.get("version_matches_server"))
                    .and_then(Value::as_bool),
                active_jobs: safe_usize(agent.get("active_jobs")),
                job_concurrency_limit: concurrency.get("limit").and_then(Value::as_u64),
                jobs_running: safe_usize(concurrency.get("running")),
                jobs_queued: safe_usize(concurrency.get("queued")),
                projects_scanned,
                projects_scan_partial: scan.project_scan_truncated,
                sessions,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.client_id.cmp(&right.client_id));
    rows
}

async fn listed_projects_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<String>,
    project: Option<String>,
    query: Option<String>,
    limit: usize,
) -> Result<(Vec<Value>, usize, bool), RuntimeConsoleError> {
    require_project_read(auth)?;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListProjects {
                client_id,
                project,
                query,
                limit: Some(limit),
                summary_only: false,
            },
            Some(auth),
        )
        .await;
    if !result.success {
        return Err(
            match result.output.get("error_kind").and_then(Value::as_str) {
                Some("invalid_client_id" | "invalid_project" | "invalid_query") => {
                    RuntimeConsoleError::Invalid
                }
                _ => RuntimeConsoleError::Internal,
            },
        );
    }
    let values = result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .ok_or(RuntimeConsoleError::Internal)?;
    let total = safe_usize(
        result
            .output
            .get("matched_count")
            .or_else(|| result.output.get("count")),
    )
    .max(values.len());
    let truncated = result
        .output
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(total > values.len());
    Ok((values, total, truncated))
}

fn project_selector_row(value: &Value) -> Option<RuntimeConsoleProject> {
    let id = bounded_text(value.get("id")?, MAX_PROJECT_ID_CHARS)?;
    if !valid_project_id(&id) {
        return None;
    }
    let client_id = bounded_client_id(value.get("client_id")?)?;
    Some(RuntimeConsoleProject {
        id,
        client_id,
        name: value
            .get("name")
            .and_then(|value| bounded_text(value, MAX_PROJECT_NAME_CHARS)),
        path: value.get("path").and_then(bounded_project_path),
        connected: value
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        agent_status: value
            .get("agent_status")
            .and_then(|value| bounded_text(value, MAX_STATUS_CHARS)),
        sessions: None,
    })
}

async fn projects_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
) -> Result<RuntimeConsoleProjects, RuntimeConsoleError> {
    projects_for_filters_auth(runtime, auth, None, None, limit).await
}

async fn projects_for_client_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<&str>,
    limit: usize,
) -> Result<RuntimeConsoleProjects, RuntimeConsoleError> {
    projects_for_filters_auth(runtime, auth, client_id, None, Some(limit)).await
}

async fn projects_for_filters_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<&str>,
    query: Option<&str>,
    limit: Option<usize>,
) -> Result<RuntimeConsoleProjects, RuntimeConsoleError> {
    let limit = limit
        .unwrap_or(DEFAULT_PROJECT_LIMIT)
        .clamp(1, MAX_PROJECT_LIMIT);
    let (visible, total, source_truncated) = listed_projects_for_auth(
        runtime,
        auth,
        client_id.map(str::to_string),
        None,
        query.map(str::to_string),
        limit,
    )
    .await?;
    let project_rows = visible
        .iter()
        .filter_map(project_selector_row)
        .collect::<Vec<_>>();
    let truncated = source_truncated || project_rows.len() < total;
    Ok(RuntimeConsoleProjects {
        projects: project_rows,
        total,
        truncated,
    })
}

async fn authorize_exact_project(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
) -> Result<(), RuntimeConsoleError> {
    if !valid_project_id(project) {
        return Err(RuntimeConsoleError::Invalid);
    }
    let (visible, _, _) =
        listed_projects_for_auth(runtime, auth, None, Some(project.to_string()), None, 1).await?;
    if visible
        .iter()
        .any(|value| value.get("id").and_then(Value::as_str) == Some(project))
    {
        Ok(())
    } else {
        Err(RuntimeConsoleError::NotFound)
    }
}

#[derive(Debug, Default)]
struct RunningJobSnapshot {
    counts: HashMap<(String, String), usize>,
    truncated: bool,
}

impl RunningJobSnapshot {
    fn count(&self, project: &str, session_id: &str) -> usize {
        self.counts
            .get(&(project.to_string(), session_id.to_string()))
            .copied()
            .unwrap_or(0)
    }
}

async fn running_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: Option<&str>,
) -> Result<RunningJobSnapshot, RuntimeConsoleError> {
    if !auth.has_scope(SCOPE_RUNTIME_READ) {
        return Ok(RunningJobSnapshot::default());
    }
    let result = runtime
        .list_jobs_for_auth_with_filters(
            Some(100),
            Some("running".to_string()),
            project.map(str::to_string),
            None,
            Some(auth),
        )
        .await;
    if !result.success {
        return Err(RuntimeConsoleError::Internal);
    }
    let mut snapshot = RunningJobSnapshot {
        truncated: safe_bool(result.output.get("truncated")),
        ..Default::default()
    };
    for job in result
        .output
        .get("jobs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(project) = safe_string(job.get("project"), MAX_PROJECT_ID_CHARS) else {
            continue;
        };
        let Some(session_id) = safe_string(job.get("session_id"), 160) else {
            continue;
        };
        if !valid_project_id(&project) || !is_valid_session_id(&session_id) {
            continue;
        }
        snapshot
            .counts
            .entry((project, session_id))
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }
    Ok(snapshot)
}

fn apply_running_jobs_to_list(
    list: &mut WorkflowSessionConsoleList,
    project: &str,
    jobs: &RunningJobSnapshot,
) {
    for session in &mut list.sessions {
        session.running_jobs = jobs.count(project, &session.session_id);
        session.running_jobs_complete = !jobs.truncated;
    }
}

async fn workflow_sessions_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    limit: Option<usize>,
) -> Result<WorkflowSessionConsoleList, RuntimeConsoleError> {
    authorize_exact_project(runtime, auth, project).await?;
    let mut list = runtime.workflow_sessions_console_list(project, limit);
    if auth.has_scope(SCOPE_RUNTIME_READ) {
        let session_ids = list
            .sessions
            .iter()
            .map(|session| session.session_id.clone())
            .collect::<Vec<_>>();
        runtime
            .materialize_validation_job_terminals_for_sessions(project, &session_ids, Some(auth))
            .await;
        list = runtime.workflow_sessions_console_list(project, limit);
        let jobs = running_jobs_for_auth(runtime, auth, Some(project)).await?;
        apply_running_jobs_to_list(&mut list, project, &jobs);
    }
    Ok(list)
}

async fn workflow_session_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<crate::tool_runtime::sessions::WorkflowSessionConsoleDetail, RuntimeConsoleError> {
    if !is_valid_session_id(session_id) {
        return Err(RuntimeConsoleError::Invalid);
    }
    authorize_exact_project(runtime, auth, project).await?;
    if auth.has_scope(SCOPE_RUNTIME_READ) {
        runtime
            .materialize_validation_job_terminals_for_sessions(
                project,
                &[session_id.to_string()],
                Some(auth),
            )
            .await;
    }
    let mut detail = runtime
        .workflow_session_console_detail(project, session_id, limit)
        .ok_or(RuntimeConsoleError::NotFound)?;
    if auth.has_scope(SCOPE_RUNTIME_READ) {
        let jobs = running_jobs_for_auth(runtime, auth, Some(project)).await?;
        detail.running_jobs = jobs.count(project, session_id);
        detail.running_jobs_complete = !jobs.truncated;
    }
    Ok(detail)
}

async fn runtime_status_value(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<String>,
) -> Result<Value, RuntimeConsoleError> {
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RuntimeStatus {
                compact: true,
                summary_only: true,
                client_id,
            },
            Some(auth),
        )
        .await;
    result
        .success
        .then_some(result.output)
        .ok_or(RuntimeConsoleError::Internal)
}

async fn list_agents_value(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<String>,
) -> Result<Value, RuntimeConsoleError> {
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListAgents {
                client_id,
                client_ids: None,
                include_projects: Some(false),
                summary_only: true,
            },
            Some(auth),
        )
        .await;
    result
        .success
        .then_some(result.output)
        .ok_or(RuntimeConsoleError::Internal)
}

async fn overview_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
) -> Result<RuntimeConsoleOverview, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    let status = runtime_status_value(runtime, auth, None).await?;
    let agents = list_agents_value(runtime, auth, None).await?;
    let summary = agents.get("summary").unwrap_or(&Value::Null);
    let build = status.get("build").unwrap_or(&Value::Null);
    let status_clients = status
        .get("agents")
        .and_then(|value| value.get("clients"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let source_mismatched_runners = status_clients
        .iter()
        .filter(|client| {
            client
                .get("source_alignment")
                .and_then(|value| value.get("status"))
                .and_then(Value::as_str)
                == Some("different")
        })
        .count();
    let mixed_builds_present = status_clients.iter().any(|client| {
        client
            .get("version_matches_server")
            .and_then(Value::as_bool)
            == Some(false)
            || client
                .get("source_alignment")
                .and_then(|value| value.get("status"))
                .and_then(Value::as_str)
                == Some("different")
    });
    let project_access = project_read_available(auth);
    let visible = if project_access {
        Some(projects_for_auth(runtime, auth, Some(HOME_PROJECT_SCAN_LIMIT)).await?)
    } else {
        None
    };
    let running_jobs = if visible.is_some() {
        running_jobs_for_auth(runtime, auth, None).await?
    } else {
        RunningJobSnapshot::default()
    };
    let home = visible.as_ref().map_or_else(
        || RuntimeConsoleHomeScan {
            workflow: RuntimeConsoleWorkflowAggregate::default(),
            recent_sessions: finalize_recent_sessions(Vec::new(), false),
            projects: Vec::new(),
            runner_sessions: HashMap::new(),
            runner_projects_scanned: HashMap::new(),
            project_scan_truncated: false,
        },
        |visible| scan_runtime_home(runtime, visible, &running_jobs),
    );
    let runners = runner_fleet_rows(&agents, &status_clients, &home);
    let runner_count = safe_usize(summary.get("count")).max(runners.len());
    let online = safe_usize(summary.get("online"));
    let stale = safe_usize(summary.get("stale"));
    let unavailable = runner_count.saturating_sub(online.saturating_add(stale));
    Ok(RuntimeConsoleOverview {
        service: safe_string(status.get("service"), 80),
        version: safe_string(status.get("version"), 80),
        build_git_commit: safe_string(build.get("git_commit"), 80),
        build_git_dirty: build.get("git_dirty").and_then(Value::as_bool),
        runner_count,
        runners_online: online,
        runners_stale: stale,
        runners_unavailable: unavailable,
        source_mismatched_runners,
        mixed_builds_present,
        active_jobs: safe_usize(
            status
                .get("jobs")
                .and_then(|value| value.get("active_count")),
        ),
        projects_available: project_access,
        visible_projects: visible.as_ref().map_or(0, |value| value.total),
        projects_truncated: home.project_scan_truncated,
        workflow_sessions: home.workflow,
        recent_sessions: home.recent_sessions,
        runners,
        projects: home.projects,
    })
}

async fn runner_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: &str,
    project_limit: Option<usize>,
) -> Result<RuntimeConsoleRunner, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if client_id.is_empty()
        || client_id.chars().count() > MAX_CLIENT_ID_CHARS
        || client_id.chars().any(char::is_control)
    {
        return Err(RuntimeConsoleError::Invalid);
    }
    let agents = list_agents_value(runtime, auth, Some(client_id.to_string())).await?;
    let agent = agents
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .ok_or(RuntimeConsoleError::NotFound)?;
    let status = runtime_status_value(runtime, auth, Some(client_id.to_string())).await?;
    let focus = status.get("focus").unwrap_or(&Value::Null);
    let build = agent.get("build").unwrap_or(&Value::Null);
    let concurrency = agent.get("job_concurrency").unwrap_or(&Value::Null);
    let project_access = project_read_available(auth);
    let project_limit = project_limit
        .unwrap_or(DEFAULT_RUNNER_PROJECT_LIMIT)
        .clamp(1, MAX_RUNNER_PROJECT_LIMIT);
    let visible_projects = if project_access {
        Some(projects_for_client_auth(runtime, auth, Some(client_id), MAX_PROJECT_LIMIT).await?)
    } else {
        None
    };
    let visible_project_count = visible_projects.as_ref().map_or(0, |visible| visible.total);
    let visible_projects_truncated = visible_projects
        .as_ref()
        .is_some_and(|visible| visible.truncated);
    let running_jobs = running_jobs_for_auth(runtime, auth, None).await?;
    let mut project_summaries = Vec::new();
    for project in visible_projects
        .map(|visible| visible.projects)
        .unwrap_or_default()
        .into_iter()
        .take(project_limit)
    {
        let mut list = runtime
            .workflow_sessions_console_list(&project.id, Some(CONSOLE_AGGREGATE_SESSION_LIMIT));
        apply_running_jobs_to_list(&mut list, &project.id, &running_jobs);
        project_summaries.push(RuntimeConsoleRunnerProject {
            id: project.id,
            name: project.name,
            path: project.path,
            connected: project.connected,
            agent_status: project.agent_status,
            sessions: aggregate_console_list(&list),
        });
    }
    let projects_returned = project_summaries.len();
    Ok(RuntimeConsoleRunner {
        client_id: client_id.to_string(),
        connected: agent
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: safe_string(agent.get("status"), MAX_STATUS_CHARS),
        version: safe_string(build.get("version"), 80),
        build_git_commit: safe_string(build.get("git_commit"), 80),
        build_git_dirty: build.get("git_dirty").and_then(Value::as_bool),
        source_alignment: safe_string(
            focus
                .get("source_alignment")
                .and_then(|value| value.get("status")),
            MAX_STATUS_CHARS,
        ),
        active_jobs: safe_usize(agent.get("active_jobs")),
        job_concurrency_limit: concurrency.get("limit").and_then(Value::as_u64),
        jobs_running: safe_usize(concurrency.get("running")),
        jobs_queued: safe_usize(concurrency.get("queued")),
        projects_available: project_access,
        visible_project_count,
        projects_returned,
        projects_truncated: visible_projects_truncated || projects_returned < visible_project_count,
        projects: project_summaries,
    })
}

async fn session_messages_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WorkflowSessionMessagesInput,
) -> Result<RuntimeConsoleMessages, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    authorize_runtime_session_project(
        runtime,
        auth,
        &input.project,
        &input.session_id,
        "list_session_messages",
    )
    .await?;
    let limit = input
        .limit
        .unwrap_or(DEFAULT_MESSAGE_LIMIT)
        .clamp(1, MAX_MESSAGE_LIMIT);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListSessionMessages {
                session_id: input.session_id.clone(),
                kind: None,
                status: None,
                message_id: None,
                reply_to: None,
                limit: Some(limit),
            },
            Some(auth),
        )
        .await;
    Ok(RuntimeConsoleMessages {
        session_id: input.session_id,
        messages: messages_from_result(&result)?,
    })
}

async fn session_observe_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WorkflowSessionObserveInput,
) -> Result<RuntimeConsoleObservation, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    authorize_runtime_session_project(
        runtime,
        auth,
        &input.project,
        &input.session_id,
        "observe_session_messages",
    )
    .await?;
    if input
        .after_observation_token
        .as_ref()
        .is_some_and(|token| token.chars().count() > MAX_OBSERVATION_TOKEN_CHARS)
        || input
            .wait_secs
            .is_some_and(|wait| !(1..=60).contains(&wait))
        || (input.wait_secs.is_some() && input.after_observation_token.is_none())
    {
        return Err(RuntimeConsoleError::Invalid);
    }
    let limit = input
        .limit
        .unwrap_or(DEFAULT_MESSAGE_LIMIT)
        .clamp(1, MAX_MESSAGE_LIMIT);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveSessionMessages {
                session_id: input.session_id.clone(),
                after_observation_token: input.after_observation_token,
                wait_secs: input.wait_secs,
                limit: Some(limit),
            },
            Some(auth),
        )
        .await;
    let messages = messages_from_result(&result)?;
    let observation_token = result
        .output
        .get("observation_token")
        .and_then(Value::as_str)
        .filter(|token| token.chars().count() <= MAX_OBSERVATION_TOKEN_CHARS)
        .ok_or(RuntimeConsoleError::Internal)?
        .to_string();
    Ok(RuntimeConsoleObservation {
        session_id: input.session_id,
        messages,
        observation_token,
        changed: safe_bool(result.output.get("changed")),
        wait_outcome: safe_string(result.output.get("wait_outcome"), 32)
            .ok_or(RuntimeConsoleError::Internal)?,
        waited_ms: result
            .output
            .get("waited_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        history_lost: safe_bool(result.output.get("history_lost")),
        has_more: safe_bool(result.output.get("has_more")),
    })
}

async fn session_post_message_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WorkflowSessionPostMessageInput,
) -> Result<RuntimeConsoleMessage, RuntimeConsoleError> {
    require_session_collaborate(auth)?;
    if !matches!(
        input.kind,
        SessionMessageKind::Note
            | SessionMessageKind::Guidance
            | SessionMessageKind::Question
            | SessionMessageKind::Todo
    ) {
        return Err(RuntimeConsoleError::Invalid);
    }
    authorize_runtime_session_project(
        runtime,
        auth,
        &input.project,
        &input.session_id,
        "post_session_message",
    )
    .await?;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::PostSessionMessage {
                session_id: input.session_id,
                kind: input.kind,
                message: input.message,
                tags: Vec::new(),
                reply_to: input.reply_to,
                priority: input.priority,
                requires_ack: input.requires_ack,
            },
            Some(auth),
        )
        .await;
    if !result.success {
        return Err(RuntimeConsoleError::Invalid);
    }
    result
        .output
        .get("message")
        .and_then(message_from_value)
        .ok_or(RuntimeConsoleError::Internal)
}

async fn session_withdraw_message_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WorkflowSessionWithdrawMessageInput,
) -> Result<RuntimeConsoleWithdrawMessage, RuntimeConsoleError> {
    require_session_collaborate(auth)?;
    if !valid_runtime_message_id(&input.message_id) {
        return Err(RuntimeConsoleError::Invalid);
    }
    authorize_runtime_session_project(
        runtime,
        auth,
        &input.project,
        &input.session_id,
        "runtime_console_withdraw_session_message",
    )
    .await?;
    let outcome = runtime
        .sessions
        .withdraw_message(&input.session_id, &input.message_id)
        .map_err(session_message_mutation_error)?;
    let value =
        serde_json::to_value(&outcome.message).map_err(|_| RuntimeConsoleError::Internal)?;
    let message = message_from_value(&value).ok_or(RuntimeConsoleError::Internal)?;
    Ok(RuntimeConsoleWithdrawMessage {
        message,
        replayed: outcome.replayed,
    })
}

async fn session_replace_message_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WorkflowSessionReplaceMessageInput,
) -> Result<RuntimeConsoleReplaceMessage, RuntimeConsoleError> {
    require_session_collaborate(auth)?;
    if !valid_runtime_message_id(&input.message_id) {
        return Err(RuntimeConsoleError::Invalid);
    }
    authorize_runtime_session_project(
        runtime,
        auth,
        &input.project,
        &input.session_id,
        "runtime_console_replace_session_message",
    )
    .await?;
    let outcome = runtime
        .sessions
        .replace_message(crate::tool_runtime::sessions::ReplaceSessionMessageInput {
            session_id: input.session_id,
            message_id: input.message_id,
            message: input.message,
        })
        .map_err(session_message_mutation_error)?;
    let original_value =
        serde_json::to_value(&outcome.original).map_err(|_| RuntimeConsoleError::Internal)?;
    let replacement_value =
        serde_json::to_value(&outcome.replacement).map_err(|_| RuntimeConsoleError::Internal)?;
    Ok(RuntimeConsoleReplaceMessage {
        original: message_from_value(&original_value).ok_or(RuntimeConsoleError::Internal)?,
        replacement: message_from_value(&replacement_value).ok_or(RuntimeConsoleError::Internal)?,
        replayed: outcome.replayed,
    })
}

#[handler]
async fn overview(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if req.parse_json::<OverviewInput>().await.is_err() {
        return render_error(res, RuntimeConsoleError::Invalid);
    }
    match overview_for_auth(&runtime, &auth).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn runner(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<RunnerInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match runner_for_auth(&runtime, &auth, &input.client_id, input.project_limit).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn projects(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<ProjectsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match projects_for_filters_auth(
        &runtime,
        &auth,
        input.client_id.as_deref(),
        input.query.as_deref(),
        input.limit,
    )
    .await
    {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_sessions(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match workflow_sessions_for_auth(&runtime, &auth, &input.project, input.limit).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match workflow_session_for_auth(
        &runtime,
        &auth,
        &input.project,
        &input.session_id,
        input.limit,
    )
    .await
    {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session_messages(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionMessagesInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match session_messages_for_auth(&runtime, &auth, input).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session_observe(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionObserveInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match session_observe_for_auth(&runtime, &auth, input).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session_post_message(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionPostMessageInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match session_post_message_for_auth(&runtime, &auth, input).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session_withdraw_message(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req
        .parse_json::<WorkflowSessionWithdrawMessageInput>()
        .await
    {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match session_withdraw_message_for_auth(&runtime, &auth, input).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn workflow_session_replace_message(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionReplaceMessageInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match session_replace_message_for_auth(&runtime, &auth, input).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn communication_agents(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_read(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationAgentsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.list_agent_identities(Some(&auth), input.agent_id, input.offset, input.limit),
    );
}

#[handler]
async fn communication_agent_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationAgentCreateInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.create_agent_identity(
            Some(&auth),
            input.handle,
            input.display_name,
            input.description,
            input.specialty_labels,
            input.idempotency_key,
        ),
    );
}

#[handler]
async fn communication_agent_update(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationAgentUpdateInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.update_agent_identity(
            Some(&auth),
            input.agent_id,
            input.expected_profile_revision,
            input.handle,
            input.display_name,
            input.description,
            input.specialty_labels,
        ),
    );
}

#[handler]
async fn communication_endpoint_attach(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationEndpointAttachInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.attach_agent_endpoint(
            Some(&auth),
            input.agent_id,
            input.host,
            input.client_attachment_id,
            input.idempotency_key,
        ),
    );
}

#[handler]
async fn communication_endpoint_renew(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationEndpointRenewInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.renew_agent_endpoint(
            Some(&auth),
            input.endpoint_id,
            input.expected_controller_generation,
        ),
    );
}

#[handler]
async fn communication_endpoint_detach(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationEndpointDetachInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.detach_agent_endpoint(Some(&auth), input.endpoint_id),
    );
}

#[handler]
async fn communication_conversations(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_read(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationConversationsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.list_conversations(
            Some(&auth),
            input.agent_id,
            input.endpoint_id,
            input.expected_controller_generation,
            input.offset,
            input.limit,
        ),
    );
}

#[handler]
async fn communication_conversation_create(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req
        .parse_json::<CommunicationConversationCreateInput>()
        .await
    {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.create_conversation(
            Some(&auth),
            input.title,
            input.agent_ids,
            input.idempotency_key,
        ),
    );
}

#[handler]
async fn communication_conversation(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_read(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationConversationInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.read_conversation(
            Some(&auth),
            input.conversation_id,
            input.agent_id,
            input.endpoint_id,
            input.expected_controller_generation,
            input.after_seq,
            input.limit,
        ),
    );
}

#[handler]
async fn communication_message_post(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationMessagePostInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.post_conversation_message(
            Some(&auth),
            input.conversation_id,
            input.body,
            input.author_agent_id,
            input.endpoint_id,
            input.expected_controller_generation,
            input.recipient_agent_ids,
            input.reply_to,
            input.idempotency_key,
            input.wake_reply_id,
            input.reply_operation_index,
        ),
    );
}

#[handler]
async fn communication_inbox(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_read(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationInboxInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.list_agent_inbox(
            Some(&auth),
            input.agent_id,
            input.endpoint_id,
            input.expected_controller_generation,
            input.after_delivery_order,
            input.limit,
        ),
    );
}

#[handler]
async fn communication_inbox_consume(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    if let Err(error) = require_communication_manage(&auth) {
        return render_error(res, error);
    }
    let input = match req.parse_json::<CommunicationInboxConsumeInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    render_communication_result(
        res,
        runtime.consume_agent_deliveries(
            Some(&auth),
            input.agent_id,
            input.endpoint_id,
            input.expected_controller_generation,
            input.delivery_ids,
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;
    use crate::shell_protocol::{
        ShellAgentProjectSummary, ShellClientCapabilities, ShellClientRegisterRequest,
    };
    use crate::tool_runtime::sessions::{
        CompleteSessionMessageInput, PostSessionMessageInput, SessionCreateOptions, SessionGuards,
        SessionMessageKind, SessionMessagePriority,
    };
    use crate::tool_runtime::{RecoveryKind, RuntimeInfo, SessionMode, ToolResult};
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use serde_json::json;

    fn project(id: &str, private_path: &str) -> ShellAgentProjectSummary {
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(format!("Project {id}")),
            path: private_path.to_string(),
            allow_patch: true,
            kind: None,
            description: Some("private description".to_string()),
            hooks: vec!["private-hook".to_string()],
            disabled: false,
            revision: Some(format!("sha256:{}", "1".repeat(64))),
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 1,
            shell_profile: Some("private-shell-profile".to_string()),
        }
    }

    async fn register_project(
        runtime: &ToolRuntime,
        client_id: &str,
        project_id: &str,
        private_path: &str,
        auth: Option<&AuthContext>,
    ) {
        runtime
            .shell_clients
            .register_with_auth(
                ShellClientRegisterRequest {
                    process_started_at: None,
                    build: None,
                    job_concurrency_limit: None,
                    job_inventory: None,
                    coding_agent_providers: None,
                    coding_agent_inventory: None,
                    client_id: client_id.to_string(),
                    agent_instance_id: format!("inst-{client_id}"),
                    display_name: Some(format!("Device {client_id}")),
                    owner: auth.and_then(|auth| auth.username.clone()),
                    hostname: Some(format!("private-host-{client_id}")),
                    host_context: None,
                    capabilities: Some(crate::test_support::current_runner_capabilities(
                        ShellClientCapabilities::default(),
                    )),
                    projects: Some(vec![project(project_id, private_path)]),
                    agent_protocol_version: Some("polling-v1".to_string()),
                    policy: None,
                },
                auth,
            )
            .await
            .unwrap();
    }

    fn test_runtime() -> Arc<ToolRuntime> {
        Arc::new(ToolRuntime::new(
            Arc::new(crate::ShellClientRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        ))
    }

    fn scoped_oauth(scopes: &[&str]) -> AuthContext {
        let mut auth = AuthContext::new(AuthKind::OAuth2Token);
        auth.user_id = Some("runtime-console-test-user".to_string());
        auth.username = Some("runtime-console-test-user".to_string());
        auth.scopes = scopes.iter().map(|scope| (*scope).to_string()).collect();
        auth
    }

    fn test_bootstrap_auth() -> AuthContext {
        let mut auth = AuthContext::new(AuthKind::Bootstrap);
        auth.role = Some("admin".to_string());
        auth.is_bootstrap = true;
        auth
    }

    fn start_authorized_session(
        runtime: &ToolRuntime,
        project: &str,
        auth: &AuthContext,
    ) -> crate::tool_runtime::sessions::SessionSummary {
        let fingerprint = crate::tool_runtime::workflow_session_authority_fingerprint(Some(auth))
            .expect("stable test authority");
        runtime
            .sessions
            .start_session_with_options(
                SessionCreateOptions::new(
                    Some(project.to_string()),
                    Some("runtime console collaboration".to_string()),
                    SessionMode::Normal,
                    SessionGuards::default(),
                )
                .with_owner_authority_fingerprint(Some(fingerprint)),
            )
            .unwrap()
    }

    fn hosted_service(runtime: Arc<ToolRuntime>) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(None);
        let (tmp, db) = crate::test_support::test_db();
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .hoop(affix_state::inject(
                crate::connector_runtime::ConnectorRuntimeSlot::default(),
            ))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    fn hosted_service_with_shared_key(
        runtime: Arc<ToolRuntime>,
        shared_key: &str,
    ) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(Some(shared_key));
        let (tmp, db) = crate::test_support::test_db();
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .hoop(affix_state::inject(
                crate::connector_runtime::ConnectorRuntimeSlot::default(),
            ))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    fn hosted_communication_service(shared_key: &str) -> (tempfile::TempDir, Service) {
        let config = crate::test_support::test_config(Some(shared_key));
        let (tmp, db) = crate::test_support::test_db();
        let runtime = Arc::new(
            ToolRuntime::new(
                Arc::new(crate::ShellClientRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_communication_database(db.clone()),
        );
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .hoop(affix_state::inject(
                crate::connector_runtime::ConnectorRuntimeSlot::default(),
            ))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(routes()),
            );
        (tmp, Service::new(router))
    }

    async fn post_communication(
        service: &Service,
        shared_key: &str,
        route: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut response = TestClient::post(format!(
            "http://localhost/api/runtime-console/communication/{route}"
        ))
        .bearer_auth(shared_key)
        .json(&body)
        .send(service)
        .await;
        let status = response.status_code.unwrap_or(StatusCode::OK);
        let body = response.take_json::<Value>().await.unwrap_or_default();
        (status, body)
    }

    #[tokio::test]
    async fn communication_canonical_not_found_errors_render_as_http_404() {
        for error_kind in [
            "agent_not_found",
            "endpoint_not_found",
            "conversation_not_found",
            "message_not_found",
            "reply_message_not_found",
            "delivery_not_found",
        ] {
            let output = json!({
                "error_kind": error_kind,
                "message": "Communication resource does not exist",
                "current_profile_revision": null,
                "state_changed": false,
            });
            let result = ToolResult::err_with_output(
                "Communication resource does not exist",
                output.clone(),
            )
            .with_recovery(RecoveryKind::FixInput, None);
            let expected_output = result.output.clone();
            let mut response = Response::new();
            render_communication_result(&mut response, result);
            assert_eq!(response.status_code, Some(StatusCode::NOT_FOUND));
            let body = response.take_json::<Value>().await.unwrap();
            assert_eq!(body, expected_output, "{error_kind}");
        }
    }

    fn recent_test_row(
        client_id: &str,
        project_id: &str,
        session_id: &str,
        updated_at: i64,
        running: bool,
        attention: bool,
        active: bool,
    ) -> RuntimeConsoleRecentSession {
        let runtime = test_runtime();
        runtime.sessions.start_session(
            Some(project_id.to_string()),
            Some(format!("Session {session_id}")),
        );
        let mut session = runtime
            .workflow_sessions_console_list(project_id, Some(1))
            .sessions
            .remove(0);
        session.session_id = session_id.to_string();
        session.updated_at = updated_at;
        session.running_call = running;
        session.lifecycle = if active { "active" } else { "closed" }.to_string();
        session.overview.attention.open_todos = usize::from(attention);
        RuntimeConsoleRecentSession {
            client_id: client_id.to_string(),
            project_id: project_id.to_string(),
            project_name: Some(format!("Project {project_id}")),
            session,
        }
    }

    #[test]
    fn selector_uses_bounded_authoritative_client_id_without_parsing_project_id() {
        let projected = project_selector_row(&serde_json::json!({
            "id": "agent:not-the-device:project",
            "client_id": "device-real",
            "name": "Demo",
            "path": "C:\\Users\\demo\\worktree",
            "connected": true,
            "agent_status": "online"
        }))
        .unwrap();
        assert_eq!(projected.client_id, "device-real");
        assert_ne!(projected.client_id, "not-the-device");
        assert_eq!(projected.path.as_deref(), Some("C:\\Users\\demo\\worktree"));

        let invalid_path = project_selector_row(&serde_json::json!({
            "id": "agent:looks-valid:project",
            "client_id": "device-real",
            "path": "/private/bad\npath",
            "connected": true
        }))
        .unwrap();
        assert!(invalid_path.path.is_none());
        let overlong_path = format!("/{}", "x".repeat(MAX_PROJECT_PATH_BYTES));
        let invalid_path = project_selector_row(&serde_json::json!({
            "id": "agent:looks-valid:project",
            "client_id": "device-real",
            "path": overlong_path,
            "connected": true
        }))
        .unwrap();
        assert!(invalid_path.path.is_none());

        let overlong = "x".repeat(MAX_CLIENT_ID_CHARS + 1);
        assert!(project_selector_row(&serde_json::json!({
            "id": "agent:looks-valid:project",
            "client_id": overlong,
            "connected": true
        }))
        .is_none());
        assert!(project_selector_row(&serde_json::json!({
            "id": "agent:looks-valid:project",
            "client_id": "bad\nclient",
            "connected": true
        }))
        .is_none());
    }

    #[test]
    fn runtime_home_recent_ranking_is_working_then_updated_at_then_identity() {
        let rows = vec![
            recent_test_row("z", "agent:z:newest", "newest", 400, false, false, false),
            recent_test_row(
                "a",
                "agent:a:attention",
                "attention",
                200,
                false,
                true,
                true,
            ),
            recent_test_row("b", "agent:b:active", "active", 300, false, false, true),
            recent_test_row("c", "agent:c:working", "working", 100, true, false, false),
            recent_test_row("a", "agent:a:tie", "tie-b", 50, false, false, false),
            recent_test_row("a", "agent:a:tie", "tie-a", 50, false, false, false),
        ];
        let ranked = finalize_recent_sessions(rows, false);
        assert_eq!(
            ranked
                .sessions
                .iter()
                .map(|row| row.session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["working", "newest", "active", "attention", "tie-a", "tie-b"]
        );
        assert!(!ranked.truncated);
        assert!(!ranked.scan_truncated);
    }

    #[test]
    fn runtime_home_recent_and_project_scans_are_explicitly_bounded() {
        let recent = finalize_recent_sessions(
            (0..HOME_RECENT_SESSION_LIMIT + 3)
                .map(|index| {
                    recent_test_row(
                        "runner",
                        "agent:runner:project",
                        &format!("session-{index:02}"),
                        index as i64,
                        false,
                        false,
                        false,
                    )
                })
                .collect(),
            true,
        );
        assert_eq!(recent.returned, HOME_RECENT_SESSION_LIMIT);
        assert_eq!(recent.candidate_count, HOME_RECENT_SESSION_LIMIT + 3);
        assert!(recent.truncated);
        assert!(recent.scan_truncated);

        let runtime = test_runtime();
        let visible = RuntimeConsoleProjects {
            projects: (0..HOME_PROJECT_SCAN_LIMIT)
                .map(|index| RuntimeConsoleProject {
                    id: format!("agent:runner:project-{index}"),
                    client_id: "runner".to_string(),
                    name: Some(format!("Project {index}")),
                    path: None,
                    connected: true,
                    agent_status: Some("online".to_string()),
                    sessions: None,
                })
                .collect(),
            total: HOME_PROJECT_SCAN_LIMIT + 1,
            truncated: true,
        };
        let scan = scan_runtime_home(&runtime, &visible, &RunningJobSnapshot::default());
        assert_eq!(scan.projects.len(), HOME_PROJECT_SCAN_LIMIT);
        assert_eq!(scan.workflow.projects_scanned, HOME_PROJECT_SCAN_LIMIT);
        assert_eq!(scan.workflow.projects_total, HOME_PROJECT_SCAN_LIMIT + 1);
        assert!(scan.project_scan_truncated);
        assert!(scan.workflow.truncated);
        assert!(scan.recent_sessions.scan_truncated);

        let rows = runner_fleet_rows(
            &serde_json::json!({"agents": [{"client_id": "runner", "connected": true}]}),
            &[],
            &scan,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].projects_scanned, HOME_PROJECT_SCAN_LIMIT);
        assert!(rows[0].projects_scan_partial);
        assert!(rows[0].sessions.sessions_truncated);
        let serialized = serde_json::to_string(&rows[0]).unwrap();
        assert!(!serialized.contains("visible_project_count"));
    }

    #[test]
    fn runtime_home_project_session_bound_propagates_partial_to_runner() {
        let runtime = test_runtime();
        let project_id = "agent:runner:busy";
        for index in 0..HOME_SESSIONS_PER_PROJECT_LIMIT + 3 {
            runtime.sessions.start_session(
                Some(project_id.to_string()),
                Some(format!("Session {index}")),
            );
        }
        let visible = RuntimeConsoleProjects {
            projects: vec![RuntimeConsoleProject {
                id: project_id.to_string(),
                client_id: "runner".to_string(),
                name: Some("Busy".to_string()),
                path: Some("/root/git/busy".to_string()),
                connected: true,
                agent_status: Some("online".to_string()),
                sessions: None,
            }],
            total: 1,
            truncated: false,
        };
        let scan = scan_runtime_home(&runtime, &visible, &RunningJobSnapshot::default());
        let project_sessions = scan.projects[0].sessions.as_ref().unwrap();
        assert_eq!(
            project_sessions.retained_sessions,
            HOME_SESSIONS_PER_PROJECT_LIMIT + 3
        );
        assert_eq!(
            project_sessions.returned_sessions,
            HOME_SESSIONS_PER_PROJECT_LIMIT
        );
        assert!(project_sessions.sessions_truncated);

        let rows = runner_fleet_rows(
            &serde_json::json!({"agents": [{"client_id": "runner", "connected": true}]}),
            &[],
            &scan,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].projects_scanned, 1);
        assert!(!rows[0].projects_scan_partial);
        assert!(rows[0].sessions.sessions_truncated);
    }

    #[test]
    fn runtime_home_runner_fleet_joins_health_build_jobs_projects_and_sessions() {
        let mut sessions = empty_console_aggregate();
        sessions.active_sessions = 2;
        sessions.running_sessions = 1;
        sessions.attention.open_todos = 3;
        let scan = RuntimeConsoleHomeScan {
            workflow: RuntimeConsoleWorkflowAggregate::default(),
            recent_sessions: finalize_recent_sessions(Vec::new(), false),
            projects: Vec::new(),
            runner_sessions: HashMap::from([("runner-a".to_string(), sessions)]),
            runner_projects_scanned: HashMap::from([("runner-a".to_string(), 4)]),
            project_scan_truncated: false,
        };
        let agents = serde_json::json!({
            "agents": [{
                "client_id": "runner-a",
                "connected": true,
                "status": "online",
                "transport": "websocket",
                "agent_protocol_version": "polling-v2",
                "protocol_compatibility": "v1",
                "project_inventory_strategy": "paged",
                "last_seen_age_secs": 2,
                "active_jobs": 3,
                "job_concurrency": {"limit": 8, "running": 2, "queued": 1},
                "build": {"version": "0.3.8", "git_commit": "agent-commit", "git_dirty": false}
            }]
        });
        let status = vec![serde_json::json!({
            "client_id": "runner-a",
            "build_git_commit": "status-commit",
            "build_git_dirty": true,
            "version_matches_server": false,
            "protocol_compatibility": "v1",
            "project_inventory_strategy": "paged",
            "source_alignment": {"status": "different"}
        })];
        let rows = runner_fleet_rows(&agents, &status, &scan);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.client_id, "runner-a");
        assert_eq!(row.active_jobs, 3);
        assert_eq!(row.job_concurrency_limit, Some(8));
        assert_eq!(row.jobs_running, 2);
        assert_eq!(row.jobs_queued, 1);
        assert_eq!(row.projects_scanned, 4);
        assert!(!row.projects_scan_partial);
        assert!(!row.sessions.sessions_truncated);
        assert_eq!(row.sessions.active_sessions, 2);
        assert_eq!(row.sessions.running_sessions, 1);
        assert_eq!(row.sessions.attention.open_todos, 3);
        assert_eq!(row.build_git_commit.as_deref(), Some("status-commit"));
        assert_eq!(row.build_git_dirty, Some(true));
        assert_eq!(row.source_alignment.as_deref(), Some("different"));
        assert_eq!(row.version_matches_server, Some(false));
        assert_eq!(row.transport.as_deref(), Some("websocket"));
        assert_eq!(row.agent_protocol_version.as_deref(), Some("polling-v2"));
        assert_eq!(row.protocol_compatibility.as_deref(), Some("v1"));
        assert_eq!(row.project_inventory_strategy.as_deref(), Some("paged"));
    }

    #[test]
    fn communication_scope_checks_are_independent_from_project_and_session_authority() {
        let project_and_session = scoped_oauth(&[
            SCOPE_PROJECT_READ,
            SCOPE_RUNTIME_READ,
            SCOPE_SESSION_COLLABORATE,
        ]);
        assert_eq!(
            require_communication_read(&project_and_session),
            Err(RuntimeConsoleError::Request {
                status: 403,
                message: "Communication read access required",
            })
        );
        assert_eq!(
            require_communication_manage(&project_and_session),
            Err(RuntimeConsoleError::Request {
                status: 403,
                message: "Communication read and manage access required",
            })
        );

        let read_only = scoped_oauth(&[SCOPE_COMMUNICATION_READ]);
        assert_eq!(require_communication_read(&read_only), Ok(()));
        assert_eq!(
            require_communication_manage(&read_only),
            Err(RuntimeConsoleError::Request {
                status: 403,
                message: "Communication read and manage access required",
            })
        );

        let communication = scoped_oauth(&[SCOPE_COMMUNICATION_READ, SCOPE_COMMUNICATION_MANAGE]);
        assert_eq!(require_communication_read(&communication), Ok(()));
        assert_eq!(require_communication_manage(&communication), Ok(()));
    }

    #[tokio::test]
    async fn durable_agent_chat_http_vertical_slice_preserves_provenance_and_inbox_state() {
        let shared_key = "communication-http-secret";
        let (_tmp, service) = hosted_communication_service(shared_key);

        let (status, first_agent) = post_communication(
            &service,
            shared_key,
            "agent/create",
            json!({
                "handle": "reviewer",
                "display_name": "Reviewer",
                "description": "Reviews durable architecture",
                "specialty_labels": ["rust", "architecture"],
                "idempotency_key": "http-agent-a"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let agent_a = first_agent["agent"]["agent_id"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, second_agent) = post_communication(
            &service,
            shared_key,
            "agent/create",
            json!({
                "handle": "reviewer",
                "display_name": "Reviewer",
                "description": "Same mutable card, different canonical identity",
                "specialty_labels": ["review"],
                "idempotency_key": "http-agent-b"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let agent_b = second_agent["agent"]["agent_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(agent_a, agent_b);

        let (status, endpoint_a_body) = post_communication(
            &service,
            shared_key,
            "endpoint/attach",
            json!({
                "agent_id": agent_a,
                "host": "Runtime Console Test",
                "client_attachment_id": "window-a",
                "idempotency_key": "http-endpoint-a"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let endpoint_a = endpoint_a_body["endpoint"]["endpoint_id"]
            .as_str()
            .unwrap()
            .to_string();
        let generation_a = endpoint_a_body["endpoint"]["controller_generation"]
            .as_i64()
            .unwrap();

        let (status, endpoint_b_body) = post_communication(
            &service,
            shared_key,
            "endpoint/attach",
            json!({
                "agent_id": agent_b,
                "host": "Runtime Console Test",
                "client_attachment_id": "window-b",
                "idempotency_key": "http-endpoint-b"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let endpoint_b = endpoint_b_body["endpoint"]["endpoint_id"]
            .as_str()
            .unwrap()
            .to_string();
        let generation_b = endpoint_b_body["endpoint"]["controller_generation"]
            .as_i64()
            .unwrap();

        let (status, conversation_body) = post_communication(
            &service,
            shared_key,
            "conversation/create",
            json!({
                "title": "HTTP architecture room",
                "agent_ids": [agent_a, agent_b],
                "idempotency_key": "http-conversation"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let conversation_id = conversation_body["conversation"]["conversation"]["conversation_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_eq!(
            conversation_body["conversation"]["participants"]
                .as_array()
                .unwrap()
                .len(),
            3
        );

        let human_payload = json!({
            "conversation_id": conversation_id,
            "body": "Human to both Agents",
            "idempotency_key": "http-human-message"
        });
        let (status, human_message) =
            post_communication(&service, shared_key, "message/post", human_payload.clone()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(human_message["message"]["seq"], 1);
        assert_eq!(
            human_message["message"]["author"]["participant_kind"],
            "human"
        );
        assert_eq!(
            human_message["message"]["deliveries"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let human_message_id = human_message["message"]["message_id"]
            .as_str()
            .unwrap()
            .to_string();

        let (status, replay) =
            post_communication(&service, shared_key, "message/post", human_payload).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["state_changed"], false);
        assert_eq!(replay["message"]["message_id"], human_message_id);

        let (status, agent_message) = post_communication(
            &service,
            shared_key,
            "message/post",
            json!({
                "conversation_id": conversation_id,
                "body": "Agent A to Agent B",
                "author_agent_id": agent_a,
                "endpoint_id": endpoint_a,
                "expected_controller_generation": generation_a,
                "recipient_agent_ids": [agent_b],
                "reply_to": human_message_id,
                "idempotency_key": "http-agent-message"
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(agent_message["message"]["seq"], 2);
        assert_eq!(
            agent_message["message"]["author"]["participant_kind"],
            "agent"
        );
        assert_eq!(agent_message["message"]["author"]["agent_id"], agent_a);
        assert_eq!(
            agent_message["message"]["deliveries"][0]["recipient_agent_id"],
            agent_b
        );

        let (status, transcript) = post_communication(
            &service,
            shared_key,
            "conversation",
            json!({
                "conversation_id": conversation_id,
                "after_seq": 0,
                "limit": 10
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(transcript["conversation"]["message_count"], 2);
        let sequences = transcript["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|message| message["seq"].as_i64().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(sequences, vec![1, 2]);

        let (status, inbox) = post_communication(
            &service,
            shared_key,
            "inbox",
            json!({
                "agent_id": agent_b,
                "endpoint_id": endpoint_b,
                "expected_controller_generation": generation_b,
                "after_delivery_order": 0,
                "limit": 10
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(inbox["total_queued_count"], 2);
        let delivery_ids = inbox["deliveries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|delivery| delivery["delivery_id"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();

        let consume_payload = json!({
            "agent_id": agent_b,
            "endpoint_id": endpoint_b,
            "expected_controller_generation": generation_b,
            "delivery_ids": delivery_ids
        });
        let (status, consumed) = post_communication(
            &service,
            shared_key,
            "inbox/consume",
            consume_payload.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(consumed["state_changed"], true);
        assert_eq!(
            consumed["consumed_delivery_ids"].as_array().unwrap().len(),
            2
        );
        let (status, consumed_retry) =
            post_communication(&service, shared_key, "inbox/consume", consume_payload).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(consumed_retry["state_changed"], false);
        assert_eq!(
            consumed_retry["already_consumed_delivery_ids"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        let (status, agents) = post_communication(
            &service,
            shared_key,
            "agents",
            json!({"offset": 0, "limit": 10}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let agent_rows = agents["agents"].as_array().unwrap();
        assert_eq!(
            agent_rows
                .iter()
                .find(|agent| agent["agent_id"] == agent_a)
                .unwrap()["queued_delivery_count"],
            1
        );
        assert_eq!(
            agent_rows
                .iter()
                .find(|agent| agent["agent_id"] == agent_b)
                .unwrap()["queued_delivery_count"],
            0
        );

        let cross_origin =
            TestClient::post("http://localhost/api/runtime-console/communication/agents")
                .bearer_auth(shared_key)
                .add_header("host", "localhost", true)
                .add_header("origin", "http://attacker.example", true)
                .json(&json!({}))
                .send(&service)
                .await;
        assert_eq!(cross_origin.status_code, Some(StatusCode::FORBIDDEN));
    }

    #[tokio::test]
    async fn runtime_home_projects_and_recent_sessions_span_visible_runners() {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("runtime-home-fleet");
        register_project(&runtime, "runner-a", "proj-a", "/private/a", Some(&auth)).await;
        register_project(&runtime, "runner-b", "proj-b", "/private/b", Some(&auth)).await;
        start_authorized_session(&runtime, "agent:runner-a:proj-a", &auth);
        start_authorized_session(&runtime, "agent:runner-b:proj-b", &auth);

        let home = overview_for_auth(&runtime, &auth).await.unwrap();
        let recent_clients = home
            .recent_sessions
            .sessions
            .iter()
            .map(|row| row.client_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            recent_clients,
            std::collections::BTreeSet::from(["runner-a", "runner-b"])
        );
        assert_eq!(home.projects.len(), 2);
        assert!(home
            .projects
            .iter()
            .all(|project| project.sessions.is_some()));
        assert_eq!(home.runners.len(), 2);
        assert_eq!(home.workflow_sessions.projects_scanned, 2);
        assert!(!home.projects_truncated);
        assert!(!home.recent_sessions.scan_truncated);
    }

    #[tokio::test]
    async fn runtime_home_recent_sessions_follow_project_authority() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-home-a");
        let auth_b = crate::auth::shared_key_context("runtime-home-b");
        register_project(&runtime, "runner-a", "proj-a", "/private/a", Some(&auth_a)).await;
        register_project(&runtime, "runner-b", "proj-b", "/private/b", Some(&auth_b)).await;
        start_authorized_session(&runtime, "agent:runner-a:proj-a", &auth_a);
        start_authorized_session(&runtime, "agent:runner-b:proj-b", &auth_b);

        let home = overview_for_auth(&runtime, &auth_a).await.unwrap();
        assert_eq!(home.projects.len(), 1);
        assert_eq!(home.projects[0].id, "agent:runner-a:proj-a");
        assert_eq!(home.recent_sessions.sessions.len(), 1);
        assert_eq!(home.recent_sessions.sessions[0].client_id, "runner-a");
        let serialized = serde_json::to_string(&home).unwrap();
        assert!(!serialized.contains("runner-b"));
        assert!(!serialized.contains("agent:runner-b:proj-b"));
        assert!(serialized.contains("/private/a"));
        assert!(!serialized.contains("/private/b"));
    }

    #[tokio::test]
    async fn hosted_runtime_console_works_without_connector_runtime_and_projects_are_safe() {
        let runtime = test_runtime();
        register_project(
            &runtime,
            "special",
            "webcodex",
            "/root/private/webcodex",
            None,
        )
        .await;
        let (_tmp, service) = hosted_service(runtime);
        let mut response = TestClient::post("http://localhost/api/runtime-console/projects")
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(response.status_code, Some(StatusCode::OK));
        let body: Value = response.take_json().await.unwrap();
        assert_eq!(body["projects"][0]["id"], "agent:special:webcodex");
        assert_eq!(body["projects"][0]["client_id"], "special");
        assert_eq!(body["projects"][0]["path"], "/root/private/webcodex");
        let selector = body["projects"][0].as_object().unwrap();
        assert!(selector.keys().all(|key| matches!(
            key.as_str(),
            "id" | "client_id" | "name" | "path" | "connected" | "agent_status"
        )));
        let serialized = serde_json::to_string(&body).unwrap();
        for private in [
            "private-host-special",
            "private-shell-profile",
            "private-hook",
            &format!("sha256:{}", "1".repeat(64)),
            "private description",
        ] {
            assert!(
                !serialized.contains(private),
                "leaked {private}: {serialized}"
            );
        }

        let mut filtered = TestClient::post("http://localhost/api/runtime-console/projects")
            .json(&serde_json::json!({
                "client_id": "special",
                "query": "webcodex",
                "limit": 100
            }))
            .send(&service)
            .await;
        assert_eq!(filtered.status_code, Some(StatusCode::OK));
        let filtered_body: Value = filtered.take_json().await.unwrap();
        assert_eq!(filtered_body["total"], 1);
        assert_eq!(filtered_body["truncated"], false);
        assert_eq!(filtered_body["projects"][0]["id"], "agent:special:webcodex");

        let invalid_query = TestClient::post("http://localhost/api/runtime-console/projects")
            .json(&serde_json::json!({"query": "   "}))
            .send(&service)
            .await;
        assert_eq!(invalid_query.status_code, Some(StatusCode::BAD_REQUEST));
    }

    #[tokio::test]
    async fn project_filters_apply_before_bounded_runtime_console_limit() {
        let runtime = test_runtime();
        let auth = test_bootstrap_auth();
        for index in 0..100 {
            register_project(
                &runtime,
                &format!("a-{index:03}"),
                "project",
                &format!("/private/a-{index:03}"),
                None,
            )
            .await;
        }
        register_project(
            &runtime,
            "special",
            "webcodex",
            "/root/private/webcodex",
            None,
        )
        .await;

        let global = projects_for_auth(&runtime, &auth, Some(100)).await.unwrap();
        assert_eq!(global.total, 101);
        assert_eq!(global.projects.len(), 100);
        assert!(global.truncated);
        assert!(!global
            .projects
            .iter()
            .any(|project| project.id == "agent:special:webcodex"));

        let by_runner =
            projects_for_filters_auth(&runtime, &auth, Some("special"), None, Some(100))
                .await
                .unwrap();
        assert_eq!(by_runner.total, 1);
        assert!(!by_runner.truncated);
        assert_eq!(by_runner.projects[0].id, "agent:special:webcodex");

        let by_query =
            projects_for_filters_auth(&runtime, &auth, None, Some("webcodex"), Some(100))
                .await
                .unwrap();
        assert_eq!(by_query.total, 1);
        assert!(!by_query.truncated);
        assert_eq!(by_query.projects[0].id, "agent:special:webcodex");

        let combined = projects_for_filters_auth(
            &runtime,
            &auth,
            Some("special"),
            Some("webcodex"),
            Some(100),
        )
        .await
        .unwrap();
        assert_eq!(combined.total, 1);
        assert_eq!(combined.projects[0].id, "agent:special:webcodex");
    }

    #[tokio::test]
    async fn runtime_console_preserves_browser_same_origin_and_json_errors() {
        let (_tmp, service) = hosted_service(test_runtime());
        let cross_origin = TestClient::post("http://localhost/api/runtime-console/projects")
            .add_header("host", "localhost", true)
            .add_header("origin", "http://attacker.example", true)
            .json(&serde_json::json!({}))
            .send(&service)
            .await;
        assert_eq!(cross_origin.status_code, Some(StatusCode::FORBIDDEN));

        let unsupported = TestClient::post("http://localhost/api/runtime-console/projects")
            .add_header("host", "localhost", true)
            .body("{}")
            .send(&service)
            .await;
        assert_eq!(
            unsupported.status_code,
            Some(StatusCode::UNSUPPORTED_MEDIA_TYPE)
        );
    }

    #[tokio::test]
    async fn selector_and_session_access_follow_authoritative_project_visibility() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("group-a");
        let auth_b = crate::auth::shared_key_context("group-b");
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        register_project(&runtime, "client-b", "proj-b", "/private/b", Some(&auth_b)).await;

        let direct = runtime
            .dispatch_with_auth(
                ToolCall::ListProjects {
                    client_id: None,
                    project: None,
                    query: None,
                    limit: None,
                    summary_only: false,
                },
                Some(&auth_a),
            )
            .await;
        let projected = projects_for_auth(&runtime, &auth_a, Some(100))
            .await
            .unwrap();
        let direct_ids = direct.output["projects"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|value| value["id"].as_str())
            .collect::<Vec<_>>();
        let projected_ids = projected
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(projected_ids, direct_ids);
        assert_eq!(projected_ids, vec!["agent:client-a:proj-a"]);
        assert_eq!(projected.projects[0].client_id, "client-a");
        assert_eq!(projected.projects[0].path.as_deref(), Some("/private/a"));
        assert_eq!(
            projected.projects[0].client_id,
            direct.output["projects"][0]["client_id"].as_str().unwrap()
        );

        let foreign = runtime.sessions.start_session(
            Some("agent:client-b:proj-b".to_string()),
            Some("foreign".to_string()),
        );
        assert_eq!(
            workflow_session_for_auth(
                &runtime,
                &auth_a,
                "agent:client-b:proj-b",
                &foreign.session_id,
                Some(20),
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            workflow_session_for_auth(
                &runtime,
                &auth_a,
                "agent:client-a:proj-a",
                &foreign.session_id,
                Some(20),
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
    }

    #[tokio::test]
    async fn runtime_console_reuses_workflow_session_projection_and_sanitizer() {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("group-a");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = runtime
            .sessions
            .start_session(Some(project_id.to_string()), Some("observe".to_string()));
        runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Progress,
                message: "working in /root/private/source.rs".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();

        let hosted_list = workflow_sessions_for_auth(&runtime, &auth, project_id, Some(20))
            .await
            .unwrap();
        let direct_list = runtime.workflow_sessions_console_list(project_id, Some(20));
        let mut hosted_list_value = serde_json::to_value(&hosted_list).unwrap();
        let mut direct_list_value = serde_json::to_value(&direct_list).unwrap();
        for value in [&mut hosted_list_value, &mut direct_list_value] {
            for session in value["sessions"].as_array_mut().unwrap() {
                let session = session.as_object_mut().unwrap();
                session.remove("running_jobs");
                session.remove("running_jobs_complete");
            }
        }
        assert_eq!(hosted_list_value, direct_list_value);
        assert_eq!(hosted_list.sessions[0].running_jobs, 0);
        assert!(hosted_list.sessions[0].running_jobs_complete);

        let hosted_detail =
            workflow_session_for_auth(&runtime, &auth, project_id, &session.session_id, Some(20))
                .await
                .unwrap();
        let direct_detail = runtime
            .workflow_session_console_detail(project_id, &session.session_id, Some(20))
            .unwrap();
        let mut hosted_detail_value = serde_json::to_value(&hosted_detail).unwrap();
        let mut direct_detail_value = serde_json::to_value(&direct_detail).unwrap();
        for value in [&mut hosted_detail_value, &mut direct_detail_value] {
            let detail = value.as_object_mut().unwrap();
            detail.remove("running_jobs");
            detail.remove("running_jobs_complete");
        }
        assert_eq!(hosted_detail_value, direct_detail_value);
        assert_eq!(hosted_detail.running_jobs, 0);
        assert!(hosted_detail.running_jobs_complete);
        let home = overview_for_auth(&runtime, &auth).await.unwrap();
        assert_eq!(home.recent_sessions.sessions.len(), 1);
        let serialized = format!(
            "{}{}",
            serde_json::to_string(&hosted_detail).unwrap(),
            serde_json::to_string(&home).unwrap()
        );
        assert!(!serialized.contains("/root/private/source.rs"));
        assert!(serialized.contains("/private/a"));
        assert!(serialized.contains("[private path]"));
    }

    #[tokio::test]
    async fn project_read_routes_survive_without_runtime_read_but_runtime_views_fail_closed() {
        let runtime = test_runtime();
        let auth = scoped_oauth(&[SCOPE_PROJECT_READ]);
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;

        let project_view = projects_for_auth(&runtime, &auth, Some(20)).await.unwrap();
        assert_eq!(project_view.projects.len(), 1);
        assert_eq!(project_view.projects[0].id, "agent:client-a:proj-a");
        assert_eq!(project_view.projects[0].path.as_deref(), Some("/private/a"));
        let runtime_only = scoped_oauth(&[SCOPE_RUNTIME_READ]);
        let runtime_only_view = overview_for_auth(&runtime, &runtime_only).await.unwrap();
        assert!(!runtime_only_view.projects_available);
        assert!(runtime_only_view.projects.is_empty());
        assert!(!serde_json::to_string(&runtime_only_view)
            .unwrap()
            .contains("/private/a"));

        assert_eq!(
            overview_for_auth(&runtime, &auth).await.unwrap_err(),
            RuntimeConsoleError::Request {
                status: 403,
                message: "Runtime read access required",
            }
        );
        assert_eq!(
            runner_for_auth(&runtime, &auth, "client-a", Some(20))
                .await
                .unwrap_err(),
            RuntimeConsoleError::Request {
                status: 403,
                message: "Runtime read access required",
            }
        );
    }

    #[tokio::test]
    async fn server_and_runner_overviews_stay_within_caller_authorization_and_safe_projection() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-overview-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-overview-b");
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        register_project(&runtime, "client-b", "proj-b", "/private/b", Some(&auth_b)).await;

        let overview_view = overview_for_auth(&runtime, &auth_a).await.unwrap();
        assert_eq!(overview_view.runner_count, 1);
        assert_eq!(overview_view.visible_projects, 1);
        assert!(!overview_view.projects_truncated);

        let runner_view = runner_for_auth(&runtime, &auth_a, "client-a", Some(20))
            .await
            .unwrap();
        assert_eq!(runner_view.client_id, "client-a");
        assert_eq!(runner_view.visible_project_count, 1);
        assert_eq!(runner_view.projects.len(), 1);
        assert_eq!(runner_view.projects[0].id, "agent:client-a:proj-a");
        assert_eq!(runner_view.projects[0].path.as_deref(), Some("/private/a"));
        assert_eq!(
            runner_for_auth(&runtime, &auth_a, "client-b", Some(20))
                .await
                .unwrap_err(),
            RuntimeConsoleError::NotFound
        );

        let serialized = format!(
            "{}{}",
            serde_json::to_string(&overview_view).unwrap(),
            serde_json::to_string(&runner_view).unwrap()
        );
        assert!(serialized.contains("/private/a"));
        for private in [
            "/private/b",
            "private-host-client-a",
            "private-host-client-b",
            "private-shell-profile",
            "private-hook",
            "private description",
        ] {
            assert!(
                !serialized.contains(private),
                "leaked {private}: {serialized}"
            );
        }
        assert!(!serialized.contains("agent:client-b:proj-b"));
    }

    #[tokio::test]
    async fn collaboration_routes_require_runtime_read_before_session_lookup() {
        let runtime = test_runtime();
        let auth = scoped_oauth(&[SCOPE_PROJECT_READ]);
        let error = session_messages_for_auth(
            &runtime,
            &auth,
            WorkflowSessionMessagesInput {
                project: "agent:missing:project".to_string(),
                session_id: "wc_sess_missing".to_string(),
                limit: Some(20),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            RuntimeConsoleError::Request {
                status: 403,
                message: "Runtime read access required",
            }
        );
    }

    #[tokio::test]
    async fn collaboration_mutations_require_session_collaborate_before_session_lookup() {
        let runtime = test_runtime();
        let runtime_read_only = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
        let error = session_post_message_for_auth(
            &runtime,
            &runtime_read_only,
            WorkflowSessionPostMessageInput {
                project: "agent:missing:project".to_string(),
                session_id: "wc_sess_missing".to_string(),
                kind: SessionMessageKind::Guidance,
                priority: SessionMessagePriority::High,
                message: "must not be injected by runtime:read".to_string(),
                reply_to: None,
                requires_ack: true,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            RuntimeConsoleError::Request {
                status: 403,
                message: "Session collaboration access required",
            }
        );
    }

    #[tokio::test]
    async fn collaboration_message_projection_reuses_authority_fence_and_hides_completion_identity()
    {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-group-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-group-b");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        let session = start_authorized_session(&runtime, project_id, &auth_a);
        let todo = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Todo,
                message: "safe todo body".to_string(),
                tags: vec!["private-tag".to_string()],
                reply_to: None,
                priority: SessionMessagePriority::High,
            })
            .unwrap();
        let assignment_fence = runtime
            .sessions
            .get_assignment(&session.session_id, &todo.message_id)
            .unwrap()
            .assignment_fence;
        runtime
            .sessions
            .complete_message(CompleteSessionMessageInput {
                session_id: session.session_id.clone(),
                message_id: todo.message_id,
                answer: "done".to_string(),
                tags: vec!["answer-tag".to_string()],
                priority: SessionMessagePriority::Normal,
                completion_id: "a".repeat(64),
                author_session_id: Some("wc_sess_worker".to_string()),
                expected_assignment_fence: assignment_fence,
            })
            .unwrap();

        let board = session_messages_for_auth(
            &runtime,
            &auth_a,
            WorkflowSessionMessagesInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert_eq!(board.messages.len(), 2);
        let serialized = serde_json::to_string(&board).unwrap();
        assert!(serialized.contains("safe todo body"));
        assert!(serialized.contains("done"));
        assert!(!serialized.contains(&"a".repeat(64)));
        assert!(!serialized.contains("private-tag"));
        assert!(!serialized.contains("answer-tag"));
        assert!(!serialized.contains("completion_id"));
        assert!(!serialized.contains("observation_revision"));

        assert_eq!(
            session_messages_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionMessagesInput {
                    project: "agent:client-a:wrong".to_string(),
                    session_id: session.session_id.clone(),
                    limit: Some(20),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_messages_for_auth(
                &runtime,
                &auth_b,
                WorkflowSessionMessagesInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    limit: Some(20),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
    }

    #[tokio::test]
    async fn human_join_reuses_formal_session_authority_and_ack_validation() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-human-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-human-b");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        let session = start_authorized_session(&runtime, project_id, &auth_a);

        let posted = session_post_message_for_auth(
            &runtime,
            &auth_a,
            WorkflowSessionPostMessageInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Guidance,
                priority: SessionMessagePriority::High,
                message: "Please preserve the exact authority fence.".to_string(),
                reply_to: None,
                requires_ack: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(posted.kind, "guidance");
        assert_eq!(posted.priority, "high");
        assert!(posted.requires_ack);
        assert!(posted.first_ack_observed_at.is_none());

        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: "agent:client-a:wrong".to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    priority: SessionMessagePriority::Normal,
                    message: "wrong project".to_string(),
                    reply_to: None,
                    requires_ack: false,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_b,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    priority: SessionMessagePriority::Normal,
                    message: "foreign authority".to_string(),
                    reply_to: None,
                    requires_ack: false,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    priority: SessionMessagePriority::High,
                    message: "invalid ack mode".to_string(),
                    reply_to: None,
                    requires_ack: true,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Progress,
                    priority: SessionMessagePriority::Normal,
                    message: "progress is not a Human Join kind".to_string(),
                    reply_to: None,
                    requires_ack: false,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );
        assert_eq!(
            session_post_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionPostMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Guidance,
                    priority: SessionMessagePriority::High,
                    message: "x".repeat(8001),
                    reply_to: None,
                    requires_ack: true,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );
        let openapi = crate::openapi::build_openapi_spec();
        assert!(openapi["paths"]
            .get("/api/runtime-console/workflow-session-post-message")
            .is_none());
    }

    #[tokio::test]
    async fn browser_message_mutation_routes_succeed_and_retain_history() {
        let runtime = test_runtime();
        let shared_key = "runtime-console-browser-mutate";
        let auth = test_bootstrap_auth();
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);
        let withdraw_target = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "mistyped retained note".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        let replace_target = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Question,
                message: "wrong retained question".to_string(),
                tags: Vec::new(),
                reply_to: Some(withdraw_target.message_id.clone()),
                priority: SessionMessagePriority::High,
            })
            .unwrap();
        let (_tmp, service) = hosted_service_with_shared_key(runtime.clone(), shared_key);

        let mut withdrawn = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-withdraw-message",
        )
        .bearer_auth(shared_key)
        .json(&serde_json::json!({
            "project": project_id,
            "session_id": session.session_id,
            "message_id": withdraw_target.message_id,
        }))
        .send(&service)
        .await;
        assert_eq!(withdrawn.status_code, Some(StatusCode::OK));
        let withdrawn_body: Value = withdrawn.take_json().await.unwrap();
        assert_eq!(
            withdrawn_body["message"]["message_id"],
            withdraw_target.message_id
        );
        assert_eq!(withdrawn_body["message"]["status"], "resolved");
        assert_eq!(withdrawn_body["message"]["closure_kind"], "withdrawn");
        assert_eq!(
            withdrawn_body["message"]["message"],
            "mistyped retained note"
        );
        assert_eq!(withdrawn_body["replayed"], false);

        let mut replaced = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-replace-message",
        )
        .bearer_auth(shared_key)
        .json(&serde_json::json!({
            "project": project_id,
            "session_id": session.session_id,
            "message_id": replace_target.message_id,
            "message": "correct retained question",
        }))
        .send(&service)
        .await;
        assert_eq!(replaced.status_code, Some(StatusCode::OK));
        let replaced_body: Value = replaced.take_json().await.unwrap();
        assert_eq!(
            replaced_body["original"]["message_id"],
            replace_target.message_id
        );
        assert_eq!(
            replaced_body["original"]["message"],
            "wrong retained question"
        );
        assert_eq!(replaced_body["original"]["status"], "resolved");
        assert_eq!(replaced_body["original"]["closure_kind"], "superseded");
        assert_eq!(
            replaced_body["replacement"]["message"],
            "correct retained question"
        );
        assert_eq!(replaced_body["replacement"]["status"], "open");
        assert_eq!(replaced_body["replacement"]["kind"], "question");
        assert_eq!(replaced_body["replacement"]["priority"], "high");
        assert_eq!(
            replaced_body["replacement"]["reply_to"],
            withdraw_target.message_id
        );
        assert_eq!(
            replaced_body["original"]["superseded_by_message_id"],
            replaced_body["replacement"]["message_id"]
        );
        assert_eq!(
            replaced_body["replacement"]["supersedes_message_id"],
            replace_target.message_id
        );
        assert_eq!(replaced_body["replayed"], false);
    }

    #[tokio::test]
    async fn browser_message_mutations_fail_closed_on_authority_and_state_conflicts() {
        let runtime = test_runtime();
        let auth_a = crate::auth::shared_key_context("runtime-console-mutate-a");
        let auth_b = crate::auth::shared_key_context("runtime-console-mutate-b");
        let runtime_read_only = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth_a)).await;
        let session = start_authorized_session(&runtime, project_id, &auth_a);
        let note = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "authority target".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();

        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &runtime_read_only,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: note.message_id.clone(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Request {
                status: 403,
                message: "Session collaboration access required",
            }
        );
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: "agent:client-a:wrong".to_string(),
                    session_id: session.session_id.clone(),
                    message_id: note.message_id.clone(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_replace_message_for_auth(
                &runtime,
                &auth_b,
                WorkflowSessionReplaceMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: note.message_id.clone(),
                    message: "foreign edit".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: "wc_sess_missing".to_string(),
                    message_id: "wc_msg_missing".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: "wc_msg_missing".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::NotFound
        );

        let risk = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Risk,
                message: "unsupported operator mutation".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::High,
            })
            .unwrap();
        assert_eq!(
            session_replace_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionReplaceMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: risk.message_id,
                    message: "must stay unsupported".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Invalid
        );

        let todo = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Todo,
                message: "completion wins".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        let assignment_fence = runtime
            .sessions
            .get_assignment(&session.session_id, &todo.message_id)
            .unwrap()
            .assignment_fence;
        runtime
            .sessions
            .complete_message(CompleteSessionMessageInput {
                session_id: session.session_id.clone(),
                message_id: todo.message_id.clone(),
                answer: "done".to_string(),
                tags: Vec::new(),
                priority: SessionMessagePriority::Normal,
                completion_id: "b".repeat(64),
                author_session_id: None,
                expected_assignment_fence: assignment_fence,
            })
            .unwrap();
        assert_eq!(
            session_replace_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionReplaceMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id.clone(),
                    message_id: todo.message_id,
                    message: "too late".to_string(),
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Conflict
        );

        let closed_note = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "closed target".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        runtime.sessions.close_session(&session.session_id).unwrap();
        assert_eq!(
            session_withdraw_message_for_auth(
                &runtime,
                &auth_a,
                WorkflowSessionWithdrawMessageInput {
                    project: project_id.to_string(),
                    session_id: session.session_id,
                    message_id: closed_note.message_id,
                },
            )
            .await
            .unwrap_err(),
            RuntimeConsoleError::Conflict
        );
    }

    #[tokio::test]
    async fn browser_message_mutation_json_and_uncertain_errors_are_distinct() {
        let (_tmp, service) = hosted_service(test_runtime());
        let unknown_field = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-withdraw-message",
        )
        .json(&serde_json::json!({
            "project": "agent:missing:project",
            "session_id": "wc_sess_missing",
            "message_id": "wc_msg_missing",
            "unexpected": true,
        }))
        .send(&service)
        .await;
        assert_eq!(unknown_field.status_code, Some(StatusCode::BAD_REQUEST));

        assert_eq!(
            session_message_mutation_error(
                crate::tool_runtime::sessions::SessionMessageError::PersistenceUncertain,
            ),
            RuntimeConsoleError::PersistenceUncertain
        );
        let mut response = Response::new();
        render_error(&mut response, RuntimeConsoleError::PersistenceUncertain);
        assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        let body = response.take_string().await.unwrap();
        assert!(body.contains("Outcome may have happened"));
        assert!(body.contains("refresh retained messages before retrying"));

        let openapi = crate::openapi::build_openapi_spec();
        for id in [
            crate::route_metadata::RouteId::RuntimeConsoleWorkflowSessionWithdrawMessage,
            crate::route_metadata::RouteId::RuntimeConsoleWorkflowSessionReplaceMessage,
        ] {
            let path = crate::route_metadata::path(id);
            assert!(
                openapi["paths"].get(path).is_none(),
                "{path} leaked into OpenAPI"
            );
        }
    }

    #[tokio::test]
    async fn browser_message_mutation_persistence_uncertain_is_503_and_retains_live_state() {
        let root = tempfile::tempdir().unwrap();
        let ledger_dir = root.path().join("session-ledger");
        std::fs::create_dir_all(&ledger_dir).unwrap();
        let ledger = ledger_dir.join("sessions.json");
        let mut runtime = ToolRuntime::new(
            Arc::new(crate::ShellClientRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        );
        runtime.sessions =
            crate::tool_runtime::sessions::SessionStore::with_persistence(&ledger, 10, 50);
        let runtime = Arc::new(runtime);
        let token = "runtime-console-persistence-uncertain";
        let auth = test_bootstrap_auth();
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);
        let message = runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                message: "uncertain withdraw".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        runtime.sessions.flush_persistence();
        std::fs::remove_dir_all(&ledger_dir).unwrap();
        std::fs::write(&ledger_dir, b"block durable ledger recreation").unwrap();
        let (_tmp, service) = hosted_service_with_shared_key(runtime.clone(), token);

        let mut response = TestClient::post(
            "http://localhost/api/runtime-console/workflow-session-withdraw-message",
        )
        .bearer_auth(token)
        .json(&serde_json::json!({
            "project": project_id,
            "session_id": session.session_id,
            "message_id": message.message_id,
        }))
        .send(&service)
        .await;
        assert_eq!(response.status_code, Some(StatusCode::SERVICE_UNAVAILABLE));
        let body = response.take_string().await.unwrap();
        assert!(body.contains("Outcome may have happened"));
        assert!(body.contains("refresh retained messages before retrying"));

        let retained = runtime
            .sessions
            .list_messages(
                &session.session_id,
                crate::tool_runtime::sessions::ListSessionMessagesFilter {
                    message_id: Some(message.message_id),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(
            retained[0].status,
            crate::tool_runtime::sessions::SessionMessageStatus::Resolved
        );
        assert_eq!(
            serde_json::to_value(&retained[0]).unwrap()["closure_kind"],
            "withdrawn"
        );
    }

    #[tokio::test]
    async fn collaboration_observation_route_preserves_baseline_update_timeout_and_paging_semantics(
    ) {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("runtime-console-observe");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);

        let baseline = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: None,
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(!baseline.changed);
        assert!(baseline.messages.is_empty());
        assert!(!baseline.history_lost);
        assert!(!baseline.has_more);

        runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Question,
                message: "first update".to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
        let updated = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: Some(baseline.observation_token),
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(updated.changed);
        assert_eq!(updated.messages.len(), 1);
        assert_eq!(updated.messages[0].message, "first update");

        let timed_out = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: Some(updated.observation_token.clone()),
                wait_secs: Some(1),
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert_eq!(timed_out.wait_outcome, "timeout");
        assert!(!timed_out.changed);

        for body in ["page one", "page two"] {
            runtime
                .sessions
                .post_message(PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Guidance,
                    message: body.to_string(),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: SessionMessagePriority::Normal,
                })
                .unwrap();
        }
        let page_one = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: Some(updated.observation_token),
                wait_secs: None,
                limit: Some(1),
            },
        )
        .await
        .unwrap();
        assert!(page_one.has_more);
        assert_eq!(page_one.messages.len(), 1);
        let page_two = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id,
                after_observation_token: Some(page_one.observation_token),
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(!page_two.has_more);
        assert_eq!(page_two.messages.len(), 1);
    }

    #[tokio::test]
    async fn collaboration_observation_route_surfaces_history_loss_from_authoritative_retention() {
        let runtime = test_runtime();
        let auth = crate::auth::shared_key_context("runtime-console-history-loss");
        let project_id = "agent:client-a:proj-a";
        register_project(&runtime, "client-a", "proj-a", "/private/a", Some(&auth)).await;
        let session = start_authorized_session(&runtime, project_id, &auth);
        let baseline = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                after_observation_token: None,
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();

        let retention_limit = runtime.sessions.status().max_messages_per_session;
        for index in 0..=retention_limit {
            runtime
                .sessions
                .post_message(PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind: SessionMessageKind::Note,
                    message: format!("retention filler {index}"),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: SessionMessagePriority::Normal,
                })
                .unwrap();
        }

        let observed = session_observe_for_auth(
            &runtime,
            &auth,
            WorkflowSessionObserveInput {
                project: project_id.to_string(),
                session_id: session.session_id,
                after_observation_token: Some(baseline.observation_token),
                wait_secs: None,
                limit: Some(100),
            },
        )
        .await
        .unwrap();
        assert!(observed.changed);
        assert!(observed.history_lost);
        assert!(observed.has_more);
        assert_eq!(observed.messages.len(), 100);
    }
}
