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
    WorkflowSessionConsoleList, WorkflowSessionConsoleListItem, DEFAULT_MAX_SESSIONS,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;

mod communication;
mod workspace;

use communication::{
    communication_agent_create, communication_agent_update, communication_agents,
    communication_conversation, communication_conversation_create, communication_conversations,
    communication_endpoint_attach, communication_endpoint_detach, communication_endpoint_renew,
    communication_inbox, communication_inbox_consume, communication_message_post,
};

// Runtime Console inventories are operator-facing and the underlying stores are
// already bounded. Avoid arbitrary 10/20/50/100-row presentation cliffs that make
// existing Projects and Sessions impossible to find in the WebUI.
const DEFAULT_PROJECT_LIMIT: usize = 2_000;
const MAX_PROJECT_LIMIT: usize = 2_000;
const MAX_PROJECT_ID_CHARS: usize = 512;
const MAX_PROJECT_NAME_CHARS: usize = 160;
const MAX_PROJECT_PATH_BYTES: usize = 4096;
const MAX_CLIENT_ID_CHARS: usize = 160;
const MAX_STATUS_CHARS: usize = 64;
const DEFAULT_RUNNER_PROJECT_LIMIT: usize = MAX_PROJECT_LIMIT;
const MAX_RUNNER_PROJECT_LIMIT: usize = MAX_PROJECT_LIMIT;
const CONSOLE_AGGREGATE_SESSION_LIMIT: usize = DEFAULT_MAX_SESSIONS;
const HOME_PROJECT_SCAN_LIMIT: usize = MAX_PROJECT_LIMIT;
const HOME_SESSIONS_PER_PROJECT_LIMIT: usize = DEFAULT_MAX_SESSIONS;
const HOME_RECENT_SESSION_LIMIT: usize = DEFAULT_MAX_SESSIONS;
const DEFAULT_MESSAGE_LIMIT: usize = 100;
const MAX_MESSAGE_LIMIT: usize = 100;
const MAX_OBSERVATION_TOKEN_CHARS: usize = 192;
const DEFAULT_WINDOW_LIMIT: usize = 2_000;
const MAX_WINDOW_LIMIT: usize = 2_000;
const DEFAULT_WINDOW_ACTIVITY_LIMIT: usize = 2_000;
const MAX_WINDOW_ACTIVITY_LIMIT: usize = 2_000;
const DEFAULT_WINDOW_SESSION_LIMIT: usize = DEFAULT_MAX_SESSIONS;
const MAX_WINDOW_SESSION_LIMIT: usize = DEFAULT_MAX_SESSIONS;
const MAX_WINDOW_KEY_CHARS: usize = 128;

pub(crate) fn routes() -> Router {
    use crate::route_metadata::{api_path, RouteId};
    Router::new()
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleOverview)).post(overview))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleRunner)).post(runner))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleWindows)).post(windows))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleWindow)).post(window))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleProjects)).post(projects))
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleExtensions))
                .post(workspace::extensions),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleInstruction))
                .post(workspace::instruction),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleProjectGit))
                .post(workspace::project_git),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsolePluginReload))
                .post(workspace::plugin_reload),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSessions))
                .post(workflow_sessions),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWorkflowSessionLocate))
                .post(workflow_session_locate),
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
struct WorkflowSessionLocateInput {
    session_id: String,
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
struct WindowsInput {
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    project: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowInput {
    client_window_key: String,
    #[serde(default)]
    activity_limit: Option<usize>,
    #[serde(default)]
    session_limit: Option<usize>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimeConsoleWindowVisibilityScope {
    Global,
    Principal,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeConsoleWindowVisibility {
    pub scope: RuntimeConsoleWindowVisibilityScope,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleWindows {
    windows: Vec<RuntimeConsoleWindowSummary>,
    returned: usize,
    total: usize,
    truncated: bool,
    visibility: RuntimeConsoleWindowVisibility,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleWindowSummary {
    client_window_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_project: Option<String>,
    source: String,
    last_seen_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_tool_call_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_meaningful_activity_at_ms: Option<i64>,
    active_count: usize,
    linked_session_count: usize,
    recorder_gap_count: usize,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleWindowDetail {
    client_window_key: String,
    source: String,
    last_seen_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_tool_call_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_meaningful_activity_at_ms: Option<i64>,
    active_count: usize,
    active_requests: Vec<RuntimeConsoleActiveWindowRequest>,
    linked_sessions: Vec<RuntimeConsoleWindowSession>,
    sessions_returned: usize,
    sessions_truncated: bool,
    activity: Vec<RuntimeConsoleWindowActivity>,
    activity_returned: usize,
    activity_truncated: bool,
    visibility: RuntimeConsoleWindowVisibility,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleActiveWindowRequest {
    server_trace_id: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    started_at_ms: i64,
    elapsed_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleWindowSession {
    workflow_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    first_linked_at_ms: i64,
    last_linked_at_ms: i64,
    relations: Vec<String>,
    relation_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lifecycle: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleWindowActivity {
    started_at_ms: i64,
    ended_at_ms: i64,
    duration_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    service_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_call_gap_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cycle_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_transition_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_streaming: Option<bool>,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_presentation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    status: String,
    meaningful: bool,
    #[cfg(feature = "experimental-code-mode")]
    #[serde(skip_serializing_if = "Option::is_none")]
    code_mode_composition: Option<RuntimeConsoleCodeModeComposition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recorder_gap_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_trace_id: Option<String>,
    workflow_sessions: Vec<RuntimeConsoleWindowActivitySession>,
}

#[derive(Debug, Clone, Copy, Default)]
struct WindowActivityTimingProjection {
    service_ms: Option<i64>,
    next_call_gap_ms: Option<i64>,
    cycle_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleWindowActivitySession {
    workflow_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project: Option<String>,
    relation: String,
}

#[cfg(feature = "experimental-code-mode")]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RuntimeConsoleCodeModeComposition {
    nested_calls: usize,
    nested_successes: usize,
    nested_failures: usize,
    max_in_flight: usize,
    duration_ms: u64,
    slot_wait_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    input_bytes: Option<usize>,
    returned_bytes: usize,
    nested_raw_result_bytes_total: usize,
    nested_tool_counts: BTreeMap<String, usize>,
    consequential_calls: usize,
    known_results: usize,
    job_handoffs: usize,
    outcome_unknown: usize,
}

#[cfg(feature = "experimental-code-mode")]
fn project_code_mode_composition(value: &Value) -> Option<RuntimeConsoleCodeModeComposition> {
    let projection =
        serde_json::from_value::<RuntimeConsoleCodeModeComposition>(value.clone()).ok()?;
    let counted = projection
        .nested_tool_counts
        .values()
        .try_fold(0usize, |total, value| total.checked_add(*value))?;
    let consequential_counted = projection
        .known_results
        .checked_add(projection.job_handoffs)
        .and_then(|total| total.checked_add(projection.outcome_unknown))?;
    if projection.nested_calls > 32
        || projection.max_in_flight > 8
        || projection
            .nested_successes
            .saturating_add(projection.nested_failures)
            != projection.nested_calls
        || counted != projection.nested_calls
        || consequential_counted != projection.consequential_calls
        || projection.consequential_calls > projection.nested_calls
        || projection.nested_tool_counts.len() > 32
        || projection
            .nested_tool_counts
            .keys()
            .any(|tool| !crate::tool_runtime::code_mode_nested_tool_is_admitted(tool))
    {
        return None;
    }
    Some(projection)
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleWorkspaceActivity {
    created_at: i64,
    tool: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleSessionJob {
    job_id: String,
    kind: String,
    status: String,
    terminal: bool,
    created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    activity_phase: Option<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleWorkflowSessionDetail {
    #[serde(flatten)]
    session: crate::tool_runtime::sessions::WorkflowSessionConsoleDetail,
    window_activity_available: bool,
    linked_windows: Vec<RuntimeConsoleSessionWindow>,
    window_activity_after_last_session_record: Vec<RuntimeConsoleWindowActivity>,
    window_activity_after_last_session_record_truncated: bool,
    workspace_activity_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_last_activity: Option<RuntimeConsoleWorkspaceActivity>,
    job_activity_available: bool,
    jobs: Vec<RuntimeConsoleSessionJob>,
    jobs_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeConsoleSessionWindow {
    client_window_key: String,
    source: String,
    first_linked_at_ms: i64,
    last_linked_at_ms: i64,
    last_seen_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_meaningful_activity_at_ms: Option<i64>,
    active_count: usize,
    relations: Vec<String>,
    relation_count: usize,
    recorder_gap_count: usize,
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
struct RuntimeConsoleLocatedSession {
    client_id: String,
    project_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    #[serde(flatten)]
    session: crate::tool_runtime::sessions::WorkflowSessionConsoleDetail,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleRunnerSummary {
    client_id: String,
    connected: bool,
    status: Option<String>,
    transport: Option<String>,
    #[serde(rename = "agent_protocol_generation")]
    runner_protocol_generation: Option<u64>,
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
    recent_sessions: RuntimeConsoleRecentSessions,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleRunnerProject {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    connected: bool,
    #[serde(rename = "agent_status", skip_serializing_if = "Option::is_none")]
    runner_status: Option<String>,
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
    project_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    connected: bool,
    #[serde(rename = "agent_status", skip_serializing_if = "Option::is_none")]
    runner_status: Option<String>,
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
    webcodex_core::workflow_session_contract::is_valid_session_message_id(message_id)
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
    runners: &Value,
    status_clients: &[Value],
    scan: &RuntimeConsoleHomeScan,
) -> Vec<RuntimeConsoleRunnerSummary> {
    let mut rows = runners
        .get("agents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|runner_value| {
            let client_id = safe_string(runner_value.get("client_id"), MAX_CLIENT_ID_CHARS)?;
            let status = status_clients.iter().find(|candidate| {
                candidate.get("client_id").and_then(Value::as_str) == Some(client_id.as_str())
            });
            let build = runner_value.get("build").unwrap_or(&Value::Null);
            let concurrency = runner_value.get("job_concurrency").unwrap_or(&Value::Null);
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
                connected: safe_bool(runner_value.get("connected")),
                status: safe_string(runner_value.get("status"), MAX_STATUS_CHARS),
                transport: safe_string(runner_value.get("transport"), MAX_STATUS_CHARS),
                runner_protocol_generation: runner_value
                    .get("agent_protocol_generation")
                    .and_then(Value::as_u64),
                last_seen_age_secs: runner_value
                    .get("last_seen_age_secs")
                    .and_then(Value::as_i64),
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
                active_jobs: safe_usize(runner_value.get("active_jobs")),
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
        .list_projects_for_runtime_console(Some(auth), client_id, project, query, limit)
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

async fn exact_console_project_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
) -> Result<RuntimeConsoleProject, RuntimeConsoleError> {
    let (values, _, _) =
        listed_projects_for_auth(runtime, auth, None, Some(project.to_string()), None, 1).await?;
    values
        .into_iter()
        .find_map(|value| project_selector_row(&value))
        .ok_or(RuntimeConsoleError::NotFound)
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
        project_ref: value
            .get("project_ref")
            .and_then(|value| bounded_text(value, MAX_PROJECT_ID_CHARS)),
        name: value
            .get("name")
            .and_then(|value| bounded_text(value, MAX_PROJECT_NAME_CHARS)),
        path: value.get("path").and_then(bounded_project_path),
        connected: value
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        runner_status: value
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
    require_project_read(auth)?;
    if runtime.exact_project_visible_to_auth(auth, project).await {
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

async fn session_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    session_id: &str,
) -> Result<(Vec<RuntimeConsoleSessionJob>, bool), RuntimeConsoleError> {
    let result = runtime
        .list_jobs_for_auth_with_filters(
            Some(100),
            None,
            Some(project.to_string()),
            Some(session_id.to_string()),
            Some(auth),
        )
        .await;
    if !result.success {
        return Err(RuntimeConsoleError::Internal);
    }
    let truncated = safe_bool(result.output.get("truncated"));
    let mut jobs = Vec::new();
    for job in result
        .output
        .get("jobs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(job_id) = safe_string(job.get("job_id"), 160) else {
            continue;
        };
        let Some(kind) = safe_string(job.get("kind"), 80) else {
            continue;
        };
        let Some(status) = safe_string(job.get("status"), 80) else {
            continue;
        };
        let activity = job.get("activity");
        jobs.push(RuntimeConsoleSessionJob {
            job_id,
            kind,
            terminal: RunnerJobLifecycle::from_wire(&status)
                .is_ok_and(RunnerJobLifecycle::is_terminal),
            status,
            created_at: job.get("created_at").and_then(Value::as_i64).unwrap_or(0),
            started_at: job.get("started_at").and_then(Value::as_i64),
            ended_at: job.get("ended_at").and_then(Value::as_i64),
            activity_state: activity
                .and_then(|value| value.get("state"))
                .and_then(|value| safe_string(Some(value), 80)),
            activity_phase: activity
                .and_then(|value| value.get("phase"))
                .and_then(|value| safe_string(Some(value), 120)),
        });
    }
    Ok((jobs, truncated))
}

fn workspace_activity_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &RuntimeConsoleProject,
) -> Result<(bool, Option<RuntimeConsoleWorkspaceActivity>), RuntimeConsoleError> {
    let Some(db) = runtime.window_activity_db.as_ref() else {
        return Ok((false, None));
    };
    let visibility = if auth.is_project_scoped_model_subject() {
        let grant = auth
            .project_grant_id
            .as_deref()
            .ok_or(RuntimeConsoleError::Internal)?;
        webcodex_core::activity_contract::ActivityVisibility::ProjectGrant(grant)
    } else {
        webcodex_core::activity_contract::ActivityVisibility::Global
    };
    let allowed_clients = vec![project.client_id.clone()];
    let row = db
        .latest_workspace_activity_for_project(
            &project.id,
            Some(&project.client_id),
            visibility,
            &allowed_clients,
        )
        .map_err(|_| RuntimeConsoleError::Internal)?;
    Ok((
        true,
        row.map(|row| RuntimeConsoleWorkspaceActivity {
            created_at: row.created_at,
            tool: row.tool,
            success: row.success,
            client_id: row.client,
            session_id: row.session_id,
        }),
    ))
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

async fn workflow_session_locate_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    session_id: &str,
) -> Result<RuntimeConsoleLocatedSession, RuntimeConsoleError> {
    if !is_valid_session_id(session_id) {
        return Err(RuntimeConsoleError::Invalid);
    }
    let summary = runtime
        .sessions
        .summary(session_id, Some(1))
        .ok_or(RuntimeConsoleError::NotFound)?;
    let project_id = summary.project.ok_or(RuntimeConsoleError::NotFound)?;
    let project = exact_console_project_for_auth(runtime, auth, &project_id).await?;
    let session =
        workflow_session_for_auth(runtime, auth, &project_id, session_id, Some(1)).await?;
    Ok(RuntimeConsoleLocatedSession {
        client_id: project.client_id,
        project_id,
        project_name: project.name,
        session,
    })
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

fn window_principal_filter(
    auth: &AuthContext,
) -> Result<Option<(String, String)>, RuntimeConsoleError> {
    // Runtime Console is an operator surface. A caller with runtime:read should
    // discover Window candidates across credentials and then have every projected
    // event re-authorized through current Project visibility. Restrict only an
    // explicitly Project-scoped model credential to its own durable principal;
    // that narrow credential must never become a cross-Project observation key.
    if !auth.is_project_scoped_model_subject() {
        return Ok(None);
    }
    crate::tool_runtime::runtime_observation_principal(Some(auth))
        .map(Some)
        .map_err(|_| RuntimeConsoleError::Internal)
}

fn window_principal_ref(principal: &Option<(String, String)>) -> Option<(&str, &str)> {
    principal
        .as_ref()
        .map(|(kind, id)| (kind.as_str(), id.as_str()))
}

fn valid_window_key(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn window_event_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    event: &webcodex_store::models::WindowActivityEventRecord,
) -> bool {
    crate::tool_runtime::window_activity::window_event_visible_cached(runtime, auth, cache, event)
        .await
}

async fn active_window_request_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    request: &crate::tool_runtime::ActiveWindowRequest,
) -> bool {
    crate::tool_runtime::window_activity::active_window_request_visible_cached(
        runtime, auth, cache, request,
    )
    .await
}

async fn console_window_event_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    discovery_principal: Option<(&str, &str)>,
    caller_principal: Option<(&str, &str)>,
    cache: &mut HashMap<String, bool>,
    event: &webcodex_store::models::WindowActivityEventRecord,
) -> bool {
    if discovery_principal.is_none() && !auth.is_admin_caller() {
        if let Some(project) = event.project.as_deref() {
            return window_project_visible_cached(runtime, auth, cache, Some(project)).await;
        }
        let mut has_project_anchor = false;
        for link in &event.workflow_links {
            if let Some(project) = link.project.as_deref() {
                has_project_anchor = true;
                if window_project_visible_cached(runtime, auth, cache, Some(project)).await {
                    return true;
                }
            }
        }
        if has_project_anchor {
            return false;
        }
        return caller_principal.is_some_and(|(kind, id)| {
            event.principal_correlation_kind.as_deref() == Some(kind)
                && event.principal_correlation_id.as_deref() == Some(id)
        });
    }
    window_event_visible_cached(runtime, auth, cache, event).await
}

async fn console_active_window_request_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    discovery_principal: Option<(&str, &str)>,
    caller_principal: Option<(&str, &str)>,
    cache: &mut HashMap<String, bool>,
    request: &crate::tool_runtime::ActiveWindowRequest,
) -> bool {
    if discovery_principal.is_none() && !auth.is_admin_caller() && request.project.is_none() {
        if !caller_principal.is_some_and(|principal| {
            crate::tool_runtime::window_activity::active_window_request_matches_principal(
                request, principal,
            )
        }) {
            return false;
        }
    }
    active_window_request_visible_cached(runtime, auth, cache, request).await
}

async fn console_window_project_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    cache: &mut HashMap<String, bool>,
    project: Option<&str>,
) -> bool {
    if principal.is_none() && !auth.is_admin_caller() && project.is_none() {
        return false;
    }
    window_project_visible_cached(runtime, auth, cache, project).await
}

fn project_window_loop_timings(
    events: &[webcodex_store::models::WindowActivityEventRecord],
    visible: &[bool],
) -> Vec<WindowActivityTimingProjection> {
    let mut projections = vec![WindowActivityTimingProjection::default(); events.len()];
    let mut previous_meaningful = BTreeMap::<(String, String), usize>::new();

    for index in (0..events.len()).rev() {
        let event = &events[index];
        if event.response_streaming == Some(false) {
            if let (Some(started), Some(handed)) =
                (event.request_observed_at_ms, event.response_handed_at_ms)
            {
                if handed >= started {
                    projections[index].service_ms = Some(handed - started);
                }
            }
        }
        if !event.meaningful {
            continue;
        }
        let Some(principal_kind) = event.principal_correlation_kind.as_ref() else {
            continue;
        };
        let Some(principal_id) = event.principal_correlation_id.as_ref() else {
            continue;
        };
        let key = (principal_kind.clone(), principal_id.clone());
        let previous_index = previous_meaningful.remove(&key);
        if event.window_transition_kind.as_deref() == Some("serial") {
            if let Some(previous_index) = previous_index {
                let previous = &events[previous_index];
                // Do not bridge over an event whose Project is hidden/revoked:
                // exposing a derived timestamp across that boundary would turn
                // Window timing into a Project-existence oracle.
                if visible.get(previous_index) == Some(&true) && visible.get(index) == Some(&true) {
                    if let (Some(current_started), Some(previous_handed), Some(previous_started)) = (
                        event.request_observed_at_ms,
                        previous.response_handed_at_ms,
                        previous.request_observed_at_ms,
                    ) {
                        if current_started >= previous_handed {
                            projections[previous_index].next_call_gap_ms =
                                Some(current_started - previous_handed);
                        }
                        if current_started >= previous_started {
                            projections[previous_index].cycle_ms =
                                Some(current_started - previous_started);
                        }
                    }
                }
            }
        }
        if event.window_continuity_eligible == Some(true) {
            previous_meaningful.insert(key, index);
        }
    }
    projections
}

async fn project_visible_window_activity(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    visibility_cache: &mut HashMap<String, bool>,
    event: webcodex_store::models::WindowActivityEventRecord,
    timing: WindowActivityTimingProjection,
) -> RuntimeConsoleWindowActivity {
    #[cfg(feature = "experimental-code-mode")]
    let code_mode_composition = event
        .code_mode_composition
        .as_ref()
        .and_then(project_code_mode_composition);
    let mut activity_sessions = Vec::new();
    for link in event.workflow_links {
        if !window_project_visible_cached(runtime, auth, visibility_cache, link.project.as_deref())
            .await
        {
            continue;
        }
        activity_sessions.push(RuntimeConsoleWindowActivitySession {
            workflow_session_id: link.workflow_session_id,
            project: link.project,
            relation: link.relation,
        });
    }
    let activity_semantics = event
        .operation
        .as_deref()
        .map(webcodex_tool_contracts::runtime_tool_activity_semantics);
    RuntimeConsoleWindowActivity {
        started_at_ms: event.started_at_ms,
        ended_at_ms: event.ended_at_ms,
        duration_ms: event.duration_ms,
        service_ms: timing.service_ms,
        next_call_gap_ms: timing.next_call_gap_ms,
        cycle_ms: timing.cycle_ms,
        window_transition_kind: event.window_transition_kind,
        response_streaming: event.response_streaming,
        method: match event.action_name.as_str() {
            "toolsCall" => "tools/call".to_string(),
            "toolsList" => "tools/list".to_string(),
            other => other.to_string(),
        },
        tool_name: event.operation,
        activity_presentation: activity_semantics
            .map(|semantics| semantics.presentation.as_str().to_string()),
        activity_kind: activity_semantics
            .and_then(|semantics| semantics.kind.as_str().map(str::to_string)),
        project: event.project,
        status: event.status,
        // Persisted event-time truth: never recompute historical meaningfulness
        // from the current ToolDefinition activity policy.
        meaningful: event.meaningful,
        #[cfg(feature = "experimental-code-mode")]
        code_mode_composition,
        recorder_gap_session_id: event.recorder_gap_session_id,
        server_trace_id: event.server_trace_id,
        workflow_sessions: activity_sessions,
    }
}

async fn project_window_activity(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    visibility_cache: &mut HashMap<String, bool>,
    event: webcodex_store::models::WindowActivityEventRecord,
) -> Option<RuntimeConsoleWindowActivity> {
    if !window_event_visible_cached(runtime, auth, visibility_cache, &event).await {
        return None;
    }
    let service_ms = if event.response_streaming == Some(false) {
        event
            .request_observed_at_ms
            .zip(event.response_handed_at_ms)
            .and_then(|(started, handed)| handed.checked_sub(started))
            .filter(|elapsed| *elapsed >= 0)
    } else {
        None
    };
    Some(
        project_visible_window_activity(
            runtime,
            auth,
            visibility_cache,
            event,
            WindowActivityTimingProjection {
                service_ms,
                ..WindowActivityTimingProjection::default()
            },
        )
        .await,
    )
}

async fn window_project_visible_cached(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    cache: &mut HashMap<String, bool>,
    project: Option<&str>,
) -> bool {
    crate::tool_runtime::window_activity::window_project_visible_cached(
        runtime, auth, cache, project,
    )
    .await
}

async fn visible_window_summary_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    window_key: &str,
    visibility_cache: &mut HashMap<String, bool>,
    project_filter: Option<&str>,
) -> Result<Option<RuntimeConsoleWindowSummary>, RuntimeConsoleError> {
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    #[cfg(feature = "experimental-code-mode")]
    let events = db.list_window_activity_events_with_code_mode_composition(
        window_key,
        principal,
        MAX_WINDOW_ACTIVITY_LIMIT,
    );
    #[cfg(not(feature = "experimental-code-mode"))]
    let events = db.list_window_activity_events(window_key, principal, MAX_WINDOW_ACTIVITY_LIMIT);
    let events = events.map_err(|_| RuntimeConsoleError::Internal)?;
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let mut source = None;
    let mut last_seen_at_ms = None;
    let mut last_tool_call_at_ms = None;
    let mut last_meaningful_activity_at_ms = None;
    let mut recorder_gap_count = 0usize;
    let mut last_project = None;
    let mut project_observed_at = i64::MIN;
    for event in events {
        if !console_window_event_visible_cached(
            runtime,
            auth,
            principal,
            caller_principal_ref,
            visibility_cache,
            &event,
        )
        .await
            || project_filter.is_some_and(|project| event.project.as_deref() != Some(project))
        {
            continue;
        }
        source = Some(event.client_window_source.clone());
        last_seen_at_ms = Some(last_seen_at_ms.unwrap_or(i64::MIN).max(event.ended_at_ms));
        if event.action_name == "toolsCall" {
            last_tool_call_at_ms = Some(
                last_tool_call_at_ms
                    .unwrap_or(i64::MIN)
                    .max(event.ended_at_ms),
            );
        }
        if event.meaningful && event.project.is_some() && event.ended_at_ms > project_observed_at {
            last_project = event.project.clone();
            project_observed_at = event.ended_at_ms;
        }
        if event.meaningful {
            last_meaningful_activity_at_ms = Some(
                last_meaningful_activity_at_ms
                    .unwrap_or(i64::MIN)
                    .max(event.ended_at_ms),
            );
        }
        if event.recorder_gap_session_id.is_some() {
            recorder_gap_count = recorder_gap_count.saturating_add(1);
        }
    }

    // Session-link history has its own bounded relation query. Do not derive
    // the cardinality only from the latest activity-page events: a busy Window
    // may have >500 later calls while an older authoritative Session relation
    // remains part of its many-to-many history.
    let relation_rows = db
        .list_window_workflow_sessions(window_key, principal, MAX_WINDOW_SESSION_LIMIT)
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let mut linked_session_count = 0usize;
    for link in relation_rows {
        if project_filter.is_some_and(|project| link.project.as_deref() != Some(project)) {
            continue;
        }
        if console_window_project_visible_cached(
            runtime,
            auth,
            principal,
            visibility_cache,
            link.project.as_deref(),
        )
        .await
        {
            linked_session_count = linked_session_count.saturating_add(1);
        }
    }

    let mut active_count = 0usize;
    for request in runtime
        .window_activity
        .list_for_window(window_key, principal)
    {
        if !console_active_window_request_visible_cached(
            runtime,
            auth,
            principal,
            caller_principal_ref,
            visibility_cache,
            &request,
        )
        .await
            || project_filter.is_some_and(|project| request.project.as_deref() != Some(project))
        {
            continue;
        }
        source = Some(request.client_window_source.clone());
        last_seen_at_ms = Some(
            last_seen_at_ms
                .unwrap_or(i64::MIN)
                .max(request.started_at_ms),
        );
        if request.project.is_some() && request.started_at_ms > project_observed_at {
            last_project = request.project.clone();
            project_observed_at = request.started_at_ms;
        }
        active_count = active_count.saturating_add(1);
    }

    let Some(last_seen_at_ms) = last_seen_at_ms else {
        return Ok(None);
    };
    Ok(Some(RuntimeConsoleWindowSummary {
        client_window_key: window_key.to_string(),
        last_project,
        source: source.unwrap_or_default(),
        last_seen_at_ms,
        last_tool_call_at_ms,
        last_meaningful_activity_at_ms,
        active_count,
        linked_session_count,
        recorder_gap_count,
    }))
}

async fn windows_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
    project_filter: Option<&str>,
) -> Result<RuntimeConsoleWindows, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if let Some(project) = project_filter {
        authorize_exact_project(runtime, auth, project).await?;
    }
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let limit = limit
        .unwrap_or(DEFAULT_WINDOW_LIMIT)
        .clamp(1, MAX_WINDOW_LIMIT);
    let durable = db
        .list_window_activity_summaries(principal_ref, MAX_WINDOW_LIMIT)
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let active = runtime.window_activity.active_windows(principal_ref);
    let mut by_key = BTreeMap::<String, RuntimeConsoleWindowSummary>::new();
    let total;
    let source_truncated;
    if auth.is_admin_caller() && project_filter.is_none() {
        let durable_total = db
            .count_window_activity_summaries(principal_ref)
            .map_err(|_| RuntimeConsoleError::Internal)?;
        for summary in durable {
            by_key.insert(
                summary.client_window_key.clone(),
                RuntimeConsoleWindowSummary {
                    client_window_key: summary.client_window_key,
                    last_project: None,
                    source: summary.client_window_source,
                    last_seen_at_ms: summary.last_seen_at_ms,
                    last_tool_call_at_ms: summary.last_tool_call_at_ms,
                    last_meaningful_activity_at_ms: summary.last_meaningful_activity_at_ms,
                    active_count: 0,
                    linked_session_count: summary.linked_session_count,
                    recorder_gap_count: summary.recorder_gap_count,
                },
            );
        }
        let mut active_only = 0usize;
        for live in active {
            if let Some(existing) = by_key.get_mut(&live.client_window_key) {
                existing.active_count = live.active_count;
                existing.last_seen_at_ms = existing.last_seen_at_ms.max(live.last_started_at_ms);
            } else if let Some(summary) = db
                .get_window_activity_summary(&live.client_window_key, principal_ref)
                .map_err(|_| RuntimeConsoleError::Internal)?
            {
                // A live Window can be older than the bounded durable page.
                // It is already included in durable_total and retains its history.
                by_key.insert(
                    live.client_window_key.clone(),
                    RuntimeConsoleWindowSummary {
                        client_window_key: live.client_window_key,
                        last_project: None,
                        source: live.client_window_source,
                        last_seen_at_ms: summary.last_seen_at_ms.max(live.last_started_at_ms),
                        last_tool_call_at_ms: summary.last_tool_call_at_ms,
                        last_meaningful_activity_at_ms: summary.last_meaningful_activity_at_ms,
                        active_count: live.active_count,
                        linked_session_count: summary.linked_session_count,
                        recorder_gap_count: summary.recorder_gap_count,
                    },
                );
            } else {
                active_only = active_only.saturating_add(1);
                by_key.insert(
                    live.client_window_key.clone(),
                    RuntimeConsoleWindowSummary {
                        client_window_key: live.client_window_key,
                        last_project: None,
                        source: live.client_window_source,
                        last_seen_at_ms: live.last_started_at_ms,
                        last_tool_call_at_ms: None,
                        last_meaningful_activity_at_ms: None,
                        active_count: live.active_count,
                        linked_session_count: 0,
                        recorder_gap_count: 0,
                    },
                );
            }
        }
        total = durable_total.saturating_add(active_only);
        source_truncated = durable_total > MAX_WINDOW_LIMIT;
    } else {
        // Principal equality is necessary but not sufficient: a historical
        // Project grant may have been revoked after the event was written. Use
        // the principal-filtered rows only as bounded candidate keys, then
        // re-project every visible field through current canonical Project
        // authority. This keeps a known Window hash from becoming an existence
        // or timestamp oracle for an inaccessible Project.
        let mut candidate_keys = durable
            .iter()
            .map(|summary| summary.client_window_key.clone())
            .collect::<std::collections::BTreeSet<_>>();
        candidate_keys.extend(
            active
                .iter()
                .map(|summary| summary.client_window_key.clone()),
        );
        let mut visibility_cache = HashMap::new();
        for window_key in candidate_keys {
            if let Some(summary) = visible_window_summary_for_auth(
                runtime,
                auth,
                principal_ref,
                &window_key,
                &mut visibility_cache,
                project_filter,
            )
            .await?
            {
                by_key.insert(window_key, summary);
            }
        }
        total = by_key.len();
        // Candidate discovery itself is bounded. Conservatively report
        // truncation whenever that principal-filtered candidate scan reaches
        // the hard cap; hidden rows are never identified or counted in the
        // response, but an older currently-visible Window may exist beyond it.
        source_truncated = durable.len() == MAX_WINDOW_LIMIT;
    }
    let mut window_rows = by_key.into_values().collect::<Vec<_>>();
    window_rows.sort_by(|left, right| {
        right
            .last_seen_at_ms
            .cmp(&left.last_seen_at_ms)
            .then_with(|| left.client_window_key.cmp(&right.client_window_key))
    });
    window_rows.truncate(limit);
    if auth.is_admin_caller() && project_filter.is_none() {
        let mut visibility_cache = HashMap::new();
        for row in &mut window_rows {
            if let Some(observed) = visible_window_summary_for_auth(
                runtime,
                auth,
                principal_ref,
                &row.client_window_key,
                &mut visibility_cache,
                None,
            )
            .await?
            {
                row.last_project = observed.last_project;
            }
        }
    }
    let visibility = RuntimeConsoleWindowVisibility {
        scope: if principal.is_none() {
            RuntimeConsoleWindowVisibilityScope::Global
        } else {
            RuntimeConsoleWindowVisibilityScope::Principal
        },
    };
    Ok(RuntimeConsoleWindows {
        returned: window_rows.len(),
        truncated: source_truncated || total > window_rows.len(),
        total,
        windows: window_rows,
        visibility,
    })
}

async fn window_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: WindowInput,
) -> Result<RuntimeConsoleWindowDetail, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if input.client_window_key.chars().count() > MAX_WINDOW_KEY_CHARS
        || !valid_window_key(&input.client_window_key)
    {
        return Err(RuntimeConsoleError::Invalid);
    }
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let activity_limit = input
        .activity_limit
        .unwrap_or(DEFAULT_WINDOW_ACTIVITY_LIMIT)
        .clamp(1, MAX_WINDOW_ACTIVITY_LIMIT);
    let session_limit = input
        .session_limit
        .unwrap_or(DEFAULT_WINDOW_SESSION_LIMIT)
        .clamp(1, MAX_WINDOW_SESSION_LIMIT);

    let mut visibility_cache = HashMap::new();
    let summary = visible_window_summary_for_auth(
        runtime,
        auth,
        principal_ref,
        &input.client_window_key,
        &mut visibility_cache,
        None,
    )
    .await?;
    let mut active_requests = Vec::new();
    let now_ms = chrono::Utc::now().timestamp_millis();
    for request in runtime
        .window_activity
        .list_for_window(&input.client_window_key, principal_ref)
    {
        if !console_active_window_request_visible_cached(
            runtime,
            auth,
            principal_ref,
            caller_principal_ref,
            &mut visibility_cache,
            &request,
        )
        .await
        {
            continue;
        }
        active_requests.push(RuntimeConsoleActiveWindowRequest {
            server_trace_id: request.server_trace_id,
            method: request.method,
            tool_name: request.tool_name,
            project: request.project,
            started_at_ms: request.started_at_ms,
            elapsed_ms: now_ms.saturating_sub(request.started_at_ms),
        });
    }

    let active_count = active_requests.len();
    active_requests.truncate(crate::tool_runtime::MAX_ACTIVE_REQUESTS_PER_WINDOW);

    let activity_scan_limit = if auth.is_admin_caller() {
        activity_limit
            .saturating_add(1)
            .min(MAX_WINDOW_ACTIVITY_LIMIT)
    } else {
        MAX_WINDOW_ACTIVITY_LIMIT
    };
    #[cfg(feature = "experimental-code-mode")]
    let raw_activity = db.list_window_activity_events_with_code_mode_composition(
        &input.client_window_key,
        principal_ref,
        activity_scan_limit,
    );
    #[cfg(not(feature = "experimental-code-mode"))]
    let raw_activity = db.list_window_activity_events(
        &input.client_window_key,
        principal_ref,
        activity_scan_limit,
    );
    let raw_activity = raw_activity.map_err(|_| RuntimeConsoleError::Internal)?;
    let raw_activity_at_cap = raw_activity.len() == activity_scan_limit;
    let mut activity_visible = Vec::with_capacity(raw_activity.len());
    for event in &raw_activity {
        activity_visible.push(
            console_window_event_visible_cached(
                runtime,
                auth,
                principal_ref,
                caller_principal_ref,
                &mut visibility_cache,
                event,
            )
            .await,
        );
    }
    let timing = project_window_loop_timings(&raw_activity, &activity_visible);
    let mut activity = Vec::new();
    for ((event, visible), timing) in raw_activity
        .into_iter()
        .zip(activity_visible.into_iter())
        .zip(timing.into_iter())
    {
        if !visible {
            continue;
        }
        activity.push(
            project_visible_window_activity(runtime, auth, &mut visibility_cache, event, timing)
                .await,
        );
    }
    let activity_truncated = activity.len() > activity_limit || raw_activity_at_cap;
    activity.truncate(activity_limit);

    let session_scan_limit = if auth.is_admin_caller() {
        session_limit
            .saturating_add(1)
            .min(MAX_WINDOW_SESSION_LIMIT)
    } else {
        MAX_WINDOW_SESSION_LIMIT
    };
    let raw_sessions = db
        .list_window_workflow_sessions(&input.client_window_key, principal_ref, session_scan_limit)
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let raw_sessions_at_cap = raw_sessions.len() == session_scan_limit;
    let mut linked_sessions = Vec::new();
    for link in raw_sessions {
        let Some(project) = link.project.as_deref() else {
            continue;
        };
        if authorize_exact_project(runtime, auth, project)
            .await
            .is_err()
        {
            continue;
        }
        let detail =
            runtime.workflow_session_console_detail(project, &link.workflow_session_id, Some(1));
        linked_sessions.push(RuntimeConsoleWindowSession {
            workflow_session_id: link.workflow_session_id,
            project: link.project,
            first_linked_at_ms: link.first_linked_at_ms,
            last_linked_at_ms: link.last_linked_at_ms,
            relations: link.relations,
            relation_count: link.relation_count,
            title: detail.as_ref().map(|detail| detail.title.clone()),
            lifecycle: detail.as_ref().map(|detail| detail.lifecycle.clone()),
        });
    }
    let sessions_truncated = linked_sessions.len() > session_limit || raw_sessions_at_cap;
    linked_sessions.truncate(session_limit);

    if summary.is_none()
        && active_requests.is_empty()
        && activity.is_empty()
        && linked_sessions.is_empty()
    {
        return Err(RuntimeConsoleError::NotFound);
    }
    let active_last_seen = active_requests
        .iter()
        .map(|request| request.started_at_ms)
        .max();
    let last_seen_at_ms = summary
        .as_ref()
        .map(|summary| summary.last_seen_at_ms)
        .into_iter()
        .chain(active_last_seen)
        .chain(linked_sessions.iter().map(|link| link.last_linked_at_ms))
        .max()
        .unwrap_or(0);
    let source = summary
        .as_ref()
        .map(|summary| summary.source.clone())
        .unwrap_or_else(|| "openai-session".to_string());
    Ok(RuntimeConsoleWindowDetail {
        client_window_key: input.client_window_key,
        source,
        last_seen_at_ms,
        last_tool_call_at_ms: summary
            .as_ref()
            .and_then(|summary| summary.last_tool_call_at_ms),
        last_meaningful_activity_at_ms: summary
            .as_ref()
            .and_then(|summary| summary.last_meaningful_activity_at_ms),
        active_count,
        active_requests,
        sessions_returned: linked_sessions.len(),
        sessions_truncated,
        linked_sessions,
        activity_returned: activity.len(),
        activity_truncated,
        activity,
        visibility: RuntimeConsoleWindowVisibility {
            scope: if principal.is_none() {
                RuntimeConsoleWindowVisibilityScope::Global
            } else {
                RuntimeConsoleWindowVisibilityScope::Principal
            },
        },
    })
}

async fn workflow_session_detail_with_windows(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    session_id: &str,
    limit: Option<usize>,
) -> Result<RuntimeConsoleWorkflowSessionDetail, RuntimeConsoleError> {
    let session = workflow_session_for_auth(runtime, auth, project, session_id, limit).await?;
    if !auth.has_scope(SCOPE_RUNTIME_READ) {
        return Ok(RuntimeConsoleWorkflowSessionDetail {
            session,
            window_activity_available: false,
            linked_windows: Vec::new(),
            window_activity_after_last_session_record: Vec::new(),
            window_activity_after_last_session_record_truncated: false,
            workspace_activity_available: false,
            workspace_last_activity: None,
            job_activity_available: false,
            jobs: Vec::new(),
            jobs_truncated: false,
        });
    }
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let project_row = exact_console_project_for_auth(runtime, auth, project).await?;
    let (workspace_activity_available, workspace_last_activity) =
        workspace_activity_for_auth(runtime, auth, &project_row)?;
    let (jobs, jobs_truncated) = session_jobs_for_auth(runtime, auth, project, session_id).await?;
    let links = db
        .list_session_linked_windows(session_id, principal_ref, 32)
        .map_err(|_| RuntimeConsoleError::Internal)?;
    let mut visibility_cache = HashMap::new();
    let mut linked_windows = Vec::with_capacity(links.len());
    for link in &links {
        // The link itself is safe because the enclosing Session project was
        // already authorized above. Re-project Window-wide liveness through
        // current Project visibility so later activity on a revoked/hidden
        // Project cannot leak its timestamp through this reverse panel.
        let visible_summary = visible_window_summary_for_auth(
            runtime,
            auth,
            principal_ref,
            &link.client_window_key,
            &mut visibility_cache,
            Some(project),
        )
        .await?;
        linked_windows.push(RuntimeConsoleSessionWindow {
            client_window_key: link.client_window_key.clone(),
            source: visible_summary
                .as_ref()
                .map(|summary| summary.source.clone())
                .unwrap_or_else(|| link.client_window_source.clone()),
            first_linked_at_ms: link.first_linked_at_ms,
            last_linked_at_ms: link.last_linked_at_ms,
            last_seen_at_ms: visible_summary
                .as_ref()
                .map(|summary| summary.last_seen_at_ms.max(link.last_linked_at_ms))
                .unwrap_or(link.last_linked_at_ms),
            last_meaningful_activity_at_ms: visible_summary
                .as_ref()
                .and_then(|summary| summary.last_meaningful_activity_at_ms),
            active_count: visible_summary
                .as_ref()
                .map(|summary| summary.active_count)
                .unwrap_or(0),
            relations: link.relations.clone(),
            relation_count: link.relation_count,
            recorder_gap_count: link.recorder_gap_count,
        });
    }
    let mut gap_activity = Vec::new();
    let mut window_activity_source_truncated = false;
    for link in &links {
        #[cfg(feature = "experimental-code-mode")]
        let events = db.list_window_activity_events_with_code_mode_composition(
            &link.client_window_key,
            principal_ref,
            MAX_WINDOW_ACTIVITY_LIMIT,
        );
        #[cfg(not(feature = "experimental-code-mode"))]
        let events = db.list_window_activity_events(
            &link.client_window_key,
            principal_ref,
            MAX_WINDOW_ACTIVITY_LIMIT,
        );
        let events = events.map_err(|_| RuntimeConsoleError::Internal)?;
        if events.len() == MAX_WINDOW_ACTIVITY_LIMIT {
            // The durable Window event scan is itself bounded. Hitting the cap
            // means older same-Project activity may exist even when the filtered
            // projection below returns at most the public limit.
            window_activity_source_truncated = true;
        }
        for event in events {
            // Window liveness is intentionally wider than Session provenance.
            // Once a Window has an authorized relation to this Session, later
            // WebCodex actions in the same Project prove Window/model activity
            // even when no newer action_event_workflow_link was recorded.
            if event.started_at_ms <= link.last_linked_at_ms
                || event.project.as_deref() != Some(project)
            {
                continue;
            }
            if let Some(event) =
                project_window_activity(runtime, auth, &mut visibility_cache, event).await
            {
                gap_activity.push(event);
            }
        }
    }
    gap_activity.sort_by(|left, right| {
        left.started_at_ms
            .cmp(&right.started_at_ms)
            .then_with(|| left.method.cmp(&right.method))
    });
    let projection_overflow = gap_activity.len() > MAX_WINDOW_ACTIVITY_LIMIT;
    let window_activity_after_last_session_record_truncated =
        window_activity_source_truncated || projection_overflow;
    if projection_overflow {
        gap_activity.drain(0..gap_activity.len() - MAX_WINDOW_ACTIVITY_LIMIT);
    }
    Ok(RuntimeConsoleWorkflowSessionDetail {
        session,
        window_activity_available: true,
        linked_windows,
        window_activity_after_last_session_record: gap_activity,
        window_activity_after_last_session_record_truncated,
        workspace_activity_available,
        workspace_last_activity,
        job_activity_available: true,
        jobs,
        jobs_truncated,
    })
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

async fn list_runners_value(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<String>,
) -> Result<Value, RuntimeConsoleError> {
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListRunners {
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
    let runners_value = list_runners_value(runtime, auth, None).await?;
    let summary = runners_value.get("summary").unwrap_or(&Value::Null);
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
    let runners = runner_fleet_rows(&runners_value, &status_clients, &home);
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
    let runners = list_runners_value(runtime, auth, Some(client_id.to_string())).await?;
    let runner_value = runners
        .get("agents")
        .and_then(Value::as_array)
        .and_then(|values| values.first())
        .ok_or(RuntimeConsoleError::NotFound)?;
    let status = runtime_status_value(runtime, auth, Some(client_id.to_string())).await?;
    let focus = status.get("focus").unwrap_or(&Value::Null);
    let build = runner_value.get("build").unwrap_or(&Value::Null);
    let concurrency = runner_value.get("job_concurrency").unwrap_or(&Value::Null);
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
    let mut recent_sessions = Vec::new();
    let mut session_scan_truncated = false;
    for project in visible_projects
        .map(|visible| visible.projects)
        .unwrap_or_default()
        .into_iter()
        .take(project_limit)
    {
        let mut list = runtime
            .workflow_sessions_console_list(&project.id, Some(CONSOLE_AGGREGATE_SESSION_LIMIT));
        apply_running_jobs_to_list(&mut list, &project.id, &running_jobs);
        session_scan_truncated |= list.truncated;
        recent_sessions.extend(list.sessions.iter().cloned().map(|session| {
            RuntimeConsoleRecentSession {
                client_id: client_id.to_string(),
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                session,
            }
        }));
        project_summaries.push(RuntimeConsoleRunnerProject {
            id: project.id,
            name: project.name,
            path: project.path,
            connected: project.connected,
            runner_status: project.runner_status,
            sessions: aggregate_console_list(&list),
        });
    }
    let projects_returned = project_summaries.len();
    recent_sessions.sort_by(|a, b| b.session.updated_at.cmp(&a.session.updated_at));
    let candidate_count = recent_sessions.len();
    recent_sessions.truncate(DEFAULT_MAX_SESSIONS);
    let recent_sessions = RuntimeConsoleRecentSessions {
        returned: recent_sessions.len(),
        candidate_count,
        truncated: candidate_count > recent_sessions.len(),
        scan_truncated: session_scan_truncated
            || visible_projects_truncated
            || projects_returned < visible_project_count,
        sessions: recent_sessions,
    };
    Ok(RuntimeConsoleRunner {
        client_id: client_id.to_string(),
        connected: runner_value
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        status: safe_string(runner_value.get("status"), MAX_STATUS_CHARS),
        version: safe_string(build.get("version"), 80),
        build_git_commit: safe_string(build.get("git_commit"), 80),
        build_git_dirty: build.get("git_dirty").and_then(Value::as_bool),
        source_alignment: safe_string(
            focus
                .get("source_alignment")
                .and_then(|value| value.get("status")),
            MAX_STATUS_CHARS,
        ),
        active_jobs: safe_usize(runner_value.get("active_jobs")),
        job_concurrency_limit: concurrency.get("limit").and_then(Value::as_u64),
        jobs_running: safe_usize(concurrency.get("running")),
        jobs_queued: safe_usize(concurrency.get("queued")),
        projects_available: project_access,
        visible_project_count,
        projects_returned,
        projects_truncated: visible_projects_truncated || projects_returned < visible_project_count,
        projects: project_summaries,
        recent_sessions,
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
async fn windows(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WindowsInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match windows_for_auth(&runtime, &auth, input.limit, input.project.as_deref()).await {
        Ok(output) => res.render(Json(output)),
        Err(error) => render_error(res, error),
    }
}

#[handler]
async fn window(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WindowInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match window_for_auth(&runtime, &auth, input).await {
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
async fn workflow_session_locate(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<WorkflowSessionLocateInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match workflow_session_locate_for_auth(&runtime, &auth, &input.session_id).await {
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
    match workflow_session_detail_with_windows(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;
    use crate::runner_protocol::{RunnerCapabilities, RunnerProjectSummary, RunnerRegisterRequest};
    use crate::tool_runtime::sessions::{
        CompleteSessionMessageInput, PostSessionMessageInput, SessionCreateOptions, SessionGuards,
        SessionMessageKind, SessionMessagePriority,
    };
    use crate::tool_runtime::{RecoveryKind, RuntimeInfo, SessionMode, ToolResult};
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use serde_json::json;

    fn project(id: &str, private_path: &str) -> RunnerProjectSummary {
        RunnerProjectSummary {
            id: id.to_string(),
            name: Some(format!("Project {id}")),
            path: private_path.to_string(),
            allow_patch: true,
            kind: None,
            registration_source: None,
            description: Some("private description".to_string()),
            hooks: vec!["private-hook".to_string()],
            disabled: false,
            revision: Some(format!("sha256:{}", "1".repeat(64))),
            root_fingerprint: None,
            lineage: None,
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
        let runner_instance_id = format!("inst-{client_id}");
        let access = auth.map(crate::test_support::runner_access);
        runtime
            .runner_registry
            .register_with_auth(
                RunnerRegisterRequest {
                    process_started_at: None,
                    build: None,
                    job_concurrency_limit: None,
                    job_inventory: None,
                    coding_agent_providers: None,
                    coding_agent_inventory: None,
                    client_id: client_id.to_string(),
                    runner_instance_id: runner_instance_id.clone(),
                    runner_protocol_generation:
                        crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                    display_name: Some(format!("Device {client_id}")),
                    owner: auth.and_then(|auth| auth.username.clone()),
                    hostname: Some(format!("private-host-{client_id}")),
                    host_context: None,
                    capabilities: crate::test_support::current_runner_capabilities(
                        RunnerCapabilities::default(),
                    ),
                    policy: None,
                },
                access.as_ref(),
            )
            .await
            .unwrap();
        crate::test_support::apply_project_inventory_snapshot(
            &runtime.runner_registry,
            client_id,
            &runner_instance_id,
            vec![project(project_id, private_path)],
        )
        .await;
    }

    fn test_runtime() -> Arc<ToolRuntime> {
        Arc::new(ToolRuntime::new(
            Arc::new(crate::RunnerRegistry::default()),
            Arc::new(RuntimeInfo::default()),
        ))
    }

    fn test_runtime_with_window_db() -> (tempfile::TempDir, Arc<crate::Database>, Arc<ToolRuntime>)
    {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::Database::open(&tmp.path().join("window-console.db")).unwrap());
        let runtime = Arc::new(
            ToolRuntime::new(
                Arc::new(crate::RunnerRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_window_activity_database(db.clone()),
        );
        (tmp, db, runtime)
    }

    fn record_window_event(
        db: &Arc<crate::Database>,
        auth: &AuthContext,
        window_key: &str,
        project: Option<&str>,
        workflow_link: Option<(&str, &str)>,
        at_ms: i64,
    ) {
        record_window_event_with_activity(
            db,
            auth,
            window_key,
            project,
            workflow_link,
            at_ms,
            "workspace_hygiene_check",
            true,
        );
    }

    fn record_window_event_with_activity(
        db: &Arc<crate::Database>,
        auth: &AuthContext,
        window_key: &str,
        project: Option<&str>,
        workflow_link: Option<(&str, &str)>,
        at_ms: i64,
        operation: &str,
        window_meaningful: bool,
    ) {
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
        crate::action_audit_sessions::record_action_event(
            db,
            crate::action_audit_sessions::ActionAuditEventInput {
                explicit_session_id: None,
                session_title: None,
                endpoint: "/mcp".to_string(),
                action_name: "toolsCall".to_string(),
                operation: Some(operation.to_string()),
                project: project.map(str::to_string),
                principal_kind: None,
                principal_user_id: None,
                oauth_client_id: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at: at_ms / 1000,
                ended_at: at_ms / 1000,
                duration_ms: 1,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({}),
                request_bytes: None,
                response_bytes: None,
                client_window_key: Some(window_key.to_string()),
                client_window_source: Some("openai-session".to_string()),
                server_trace_id: Some(format!("trace-{at_ms}")),
                principal_correlation_kind: Some(principal_kind),
                principal_correlation_id: Some(principal_id),
                window_started_at_ms: Some(at_ms),
                window_ended_at_ms: Some(at_ms + 1),
                request_observed_at_ms: None,
                response_handed_at_ms: None,
                window_transition_kind: None,
                response_streaming: None,
                window_continuity_eligible: None,
                window_meaningful,
                recorder_gap_session_id: None,
                workflow_links: workflow_link
                    .map(|(session_id, project)| {
                        vec![crate::action_audit_sessions::ActionAuditWorkflowLinkInput {
                            workflow_session_id: session_id.to_string(),
                            relation:
                                crate::action_audit_sessions::WorkflowSessionRelation::Recording,
                            project: Some(project.to_string()),
                        }]
                    })
                    .unwrap_or_default(),
            },
        );
    }

    fn record_timed_window_event(
        db: &Arc<crate::Database>,
        auth: &AuthContext,
        window_key: &str,
        project: Option<&str>,
        request_observed_at_ms: i64,
        response_handed_at_ms: i64,
        legacy_window_ended_at_ms: i64,
        transition: &str,
    ) {
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(auth)).unwrap();
        crate::action_audit_sessions::record_action_event(
            db,
            crate::action_audit_sessions::ActionAuditEventInput {
                explicit_session_id: None,
                session_title: None,
                endpoint: "/mcp".to_string(),
                action_name: "toolsCall".to_string(),
                operation: Some("read_files".to_string()),
                project: project.map(str::to_string),
                principal_kind: None,
                principal_user_id: None,
                oauth_client_id: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at: request_observed_at_ms / 1000,
                ended_at: legacy_window_ended_at_ms / 1000,
                duration_ms: response_handed_at_ms - request_observed_at_ms,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({}),
                request_bytes: None,
                response_bytes: None,
                client_window_key: Some(window_key.to_string()),
                client_window_source: Some("openai-session".to_string()),
                server_trace_id: Some(format!("timed-trace-{request_observed_at_ms}")),
                principal_correlation_kind: Some(principal_kind),
                principal_correlation_id: Some(principal_id),
                window_started_at_ms: Some(request_observed_at_ms),
                window_ended_at_ms: Some(legacy_window_ended_at_ms),
                request_observed_at_ms: Some(request_observed_at_ms),
                response_handed_at_ms: Some(response_handed_at_ms),
                window_transition_kind: Some(transition.to_string()),
                response_streaming: Some(false),
                window_continuity_eligible: Some(true),
                window_meaningful: true,
                recorder_gap_session_id: None,
                workflow_links: Vec::new(),
            },
        );
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
                Arc::new(crate::RunnerRegistry::default()),
                Arc::new(RuntimeInfo::default()),
            )
            .with_communication_database(db.clone()),
        );
        let router = Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
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
            .with_recovery(RecoveryKind::FixInput);
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
                    project_ref: None,
                    name: Some(format!("Project {index}")),
                    path: None,
                    connected: true,
                    runner_status: Some("online".to_string()),
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
    fn runtime_home_projects_all_retained_sessions_without_extra_presentation_truncation() {
        let runtime = test_runtime();
        let project_id = "agent:runner:busy";
        for index in 0..HOME_SESSIONS_PER_PROJECT_LIMIT {
            runtime.sessions.start_session(
                Some(project_id.to_string()),
                Some(format!("Session {index}")),
            );
        }
        let visible = RuntimeConsoleProjects {
            projects: vec![RuntimeConsoleProject {
                id: project_id.to_string(),
                client_id: "runner".to_string(),
                project_ref: None,
                name: Some("Busy".to_string()),
                path: Some("/root/git/busy".to_string()),
                connected: true,
                runner_status: Some("online".to_string()),
                sessions: None,
            }],
            total: 1,
            truncated: false,
        };
        let scan = scan_runtime_home(&runtime, &visible, &RunningJobSnapshot::default());
        let project_sessions = scan.projects[0].sessions.as_ref().unwrap();
        assert_eq!(
            project_sessions.retained_sessions,
            HOME_SESSIONS_PER_PROJECT_LIMIT
        );
        assert_eq!(
            project_sessions.returned_sessions,
            HOME_SESSIONS_PER_PROJECT_LIMIT
        );
        assert!(!project_sessions.sessions_truncated);

        let rows = runner_fleet_rows(
            &serde_json::json!({"agents": [{"client_id": "runner", "connected": true}]}),
            &[],
            &scan,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].projects_scanned, 1);
        assert!(!rows[0].projects_scan_partial);
        assert!(!rows[0].sessions.sessions_truncated);
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
        let runners = serde_json::json!({
            "agents": [{
                "client_id": "runner-a",
                "connected": true,
                "status": "online",
                "transport": "websocket",
                "agent_protocol_generation": 2,
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
            "source_alignment": {"status": "different"}
        })];
        let rows = runner_fleet_rows(&runners, &status, &scan);
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
        assert_eq!(row.runner_protocol_generation, Some(2));
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
        let runner_view = runner_for_auth(&runtime, &auth, "runner-a", Some(20))
            .await
            .unwrap();
        assert_eq!(runner_view.recent_sessions.sessions.len(), 1);
        assert_eq!(
            runner_view.recent_sessions.sessions[0].project_id,
            "agent:runner-a:proj-a"
        );
        assert!(!runner_view.recent_sessions.scan_truncated);
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
    async fn hosted_runtime_console_uses_ordinary_runtime_and_projects_are_safe() {
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
    async fn product_routes_reject_unknown_effect_selectors_and_invisible_projects() {
        let (_tmp, service) = hosted_service(test_runtime());
        for route in ["extensions", "project-git"] {
            let invalid = TestClient::post(format!("http://localhost/api/runtime-console/{route}"))
                .json(&serde_json::json!({"project":"agent:missing:project","tool":"run_shell"}))
                .send(&service)
                .await;
            assert_eq!(invalid.status_code, Some(StatusCode::BAD_REQUEST));
            let hidden = TestClient::post(format!("http://localhost/api/runtime-console/{route}"))
                .json(&serde_json::json!({"project":"agent:missing:project"}))
                .send(&service)
                .await;
            assert_eq!(hidden.status_code, Some(StatusCode::NOT_FOUND));
        }
        let instruction = TestClient::post("http://localhost/api/runtime-console/instruction")
            .json(&serde_json::json!({"project":"agent:missing:project","source_scope":"runner","path":"/private/secret","fingerprint":"old"}))
            .send(&service).await;
        assert_eq!(instruction.status_code, Some(StatusCode::NOT_FOUND));
        let retarget = TestClient::post("http://localhost/api/runtime-console/plugin-reload")
            .json(&serde_json::json!({"project":"agent:missing:project","plugin":"provider","runner":"other-runner"}))
            .send(&service).await;
        assert_eq!(retarget.status_code, Some(StatusCode::BAD_REQUEST));
    }

    #[test]
    fn runtime_console_project_projection_preserves_short_project_ref() {
        let row = project_selector_row(&serde_json::json!({
            "id": "agent:special:webcodex",
            "client_id": "special",
            "project_ref": "~p118",
            "name": "WebCodex",
            "path": "/root/git/webcodex",
            "connected": true,
            "agent_status": "online"
        }))
        .unwrap();
        assert_eq!(row.project_ref.as_deref(), Some("~p118"));
        assert!(serde_json::to_string(&row)
            .unwrap()
            .contains("\"project_ref\":\"~p118\""));
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

        let full = projects_for_auth(&runtime, &auth, Some(MAX_PROJECT_LIMIT))
            .await
            .unwrap();
        assert_eq!(full.total, 101);
        assert_eq!(full.projects.len(), 101);
        assert!(!full.truncated);
        assert!(full
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

        let local = runtime.sessions.start_session(
            Some("agent:client-a:proj-a".to_string()),
            Some("locatable".to_string()),
        );
        let located = workflow_session_locate_for_auth(&runtime, &auth_a, &local.session_id)
            .await
            .unwrap();
        assert_eq!(located.project_id, "agent:client-a:proj-a");
        assert_eq!(located.client_id, "client-a");
        assert_eq!(located.session.session_id, local.session_id);
        assert_eq!(located.session.title, "locatable");
        assert_eq!(
            workflow_session_locate_for_auth(&runtime, &auth_b, &local.session_id)
                .await
                .unwrap_err(),
            RuntimeConsoleError::NotFound
        );
        assert_eq!(
            workflow_session_locate_for_auth(&runtime, &auth_a, "wc_sess_invalid")
                .await
                .unwrap_err(),
            RuntimeConsoleError::Invalid
        );
    }

    #[cfg(feature = "experimental-code-mode")]
    #[tokio::test]
    async fn code_mode_composition_projects_on_one_outer_window_activity() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = test_bootstrap_auth();
        let client_window = crate::client_window::ClientWindow::for_test("code-mode-composition");
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
        crate::action_audit_sessions::record_action_event(
            &db,
            crate::action_audit_sessions::ActionAuditEventInput {
                explicit_session_id: Some("code-mode-window-audit".to_string()),
                session_title: None,
                endpoint: "/mcp".to_string(),
                action_name: "toolsCall".to_string(),
                operation: Some("code_mode_exec".to_string()),
                project: None,
                principal_kind: None,
                principal_user_id: None,
                oauth_client_id: None,
                status: "success".to_string(),
                http_status: Some(200),
                started_at: 1,
                ended_at: 1,
                duration_ms: 13,
                error_summary: None,
                warning_summary: None,
                changed_files: Vec::new(),
                ids: json!({}),
                summary: json!({
                    "transport": "mcp",
                    "code_mode_composition": {
                        "nested_calls": 3,
                        "nested_successes": 2,
                        "nested_failures": 1,
                        "max_in_flight": 2,
                        "duration_ms": 11,
                        "slot_wait_ms": 3,
                        "returned_bytes": 19,
                        "nested_raw_result_bytes_total": 31,
                        "nested_tool_counts": {
                            "git_status": 1,
                            "read_files": 1,
                            "search_project_texts": 1
                        },
                        "consequential_calls": 1,
                        "known_results": 1,
                        "job_handoffs": 0,
                        "outcome_unknown": 0
                    }
                }),
                request_bytes: None,
                response_bytes: None,
                client_window_key: Some(client_window.key().to_string()),
                client_window_source: Some("openai-session".to_string()),
                server_trace_id: Some("trace-code-mode-composition".to_string()),
                principal_correlation_kind: Some(principal_kind),
                principal_correlation_id: Some(principal_id),
                window_started_at_ms: Some(1_000),
                window_ended_at_ms: Some(1_013),
                request_observed_at_ms: Some(1_000),
                response_handed_at_ms: Some(1_013),
                window_transition_kind: Some("unavailable".to_string()),
                response_streaming: Some(false),
                window_continuity_eligible: Some(true),
                window_meaningful: true,
                recorder_gap_session_id: None,
                workflow_links: Vec::new(),
            },
        );

        let detail = window_for_auth(
            &runtime,
            &auth,
            WindowInput {
                client_window_key: client_window.key().to_string(),
                activity_limit: None,
                session_limit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(
            detail.activity.len(),
            1,
            "nested canonical calls must not fabricate Window activity rows"
        );
        let activity = &detail.activity[0];
        assert_eq!(activity.tool_name.as_deref(), Some("code_mode_exec"));
        assert!(activity.meaningful);
        let composition = activity
            .code_mode_composition
            .as_ref()
            .expect("bounded Code Mode composition projection");
        assert_eq!(composition.nested_calls, 3);
        assert_eq!(composition.nested_successes, 2);
        assert_eq!(composition.nested_failures, 1);
        assert_eq!(composition.max_in_flight, 2);
        assert_eq!(composition.duration_ms, 11);
        assert_eq!(composition.slot_wait_ms, 3);
        assert_eq!(composition.returned_bytes, 19);
        assert_eq!(composition.nested_raw_result_bytes_total, 31);
        assert_eq!(composition.nested_tool_counts.len(), 3);
        assert_eq!(composition.consequential_calls, 1);
        assert_eq!(composition.known_results, 1);
        assert_eq!(composition.job_handoffs, 0);
        assert_eq!(composition.outcome_unknown, 0);

        let invalid = json!({
            "nested_calls": 1,
            "nested_successes": 1,
            "nested_failures": 0,
            "max_in_flight": 1,
            "duration_ms": 1,
            "slot_wait_ms": 0,
            "returned_bytes": 1,
            "nested_raw_result_bytes_total": 1,
            "nested_tool_counts": {"run_shell": 1},
            "consequential_calls": 0,
            "known_results": 0,
            "job_handoffs": 0,
            "outcome_unknown": 0
        });
        assert!(project_code_mode_composition(&invalid).is_none());
        let events = db.list_action_events("code-mode-window-audit", 10).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[tokio::test]
    async fn window_activity_counts_all_visible_requests_before_bounding_details() {
        let (_tmp, _db, runtime) = test_runtime_with_window_db();
        let auth = test_bootstrap_auth();
        let client_window = crate::client_window::ClientWindow::for_test("concurrent-window");
        let guards = (0..12)
            .map(|index| {
                runtime.window_activity.start(
                    &client_window,
                    &format!("trace-{index}"),
                    "tools/list",
                    None,
                )
            })
            .collect::<Vec<_>>();
        let detail = window_for_auth(
            &runtime,
            &auth,
            WindowInput {
                client_window_key: client_window.key().to_string(),
                activity_limit: None,
                session_limit: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(detail.active_count, guards.len());
        assert_eq!(
            detail.active_requests.len(),
            crate::tool_runtime::MAX_ACTIVE_REQUESTS_PER_WINDOW
        );
        let list = windows_for_auth(&runtime, &auth, None, None).await.unwrap();
        assert_eq!(list.windows[0].active_count, detail.active_count);
    }

    #[tokio::test]
    async fn window_activity_live_history_outside_durable_page_is_not_counted_twice() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = test_bootstrap_auth();
        let old = crate::client_window::ClientWindow::for_test("old-active-window");
        record_window_event(&db, &auth, old.key(), None, None, 1_000);
        for index in 0..MAX_WINDOW_LIMIT {
            record_window_event(
                &db,
                &auth,
                &format!("{index:064x}"),
                None,
                None,
                2_000 + index as i64,
            );
        }
        let _active = runtime
            .window_activity
            .start(&old, "old-active", "tools/call", None);
        let list = windows_for_auth(&runtime, &auth, None, None).await.unwrap();
        assert_eq!(list.total, MAX_WINDOW_LIMIT + 1);
        assert!(list.truncated);
        let row = list
            .windows
            .iter()
            .find(|row| row.client_window_key == old.key())
            .unwrap();
        assert_eq!(row.active_count, 1);
        assert_eq!(row.last_tool_call_at_ms, Some(1_001));
        assert_eq!(row.last_meaningful_activity_at_ms, Some(1_001));
    }

    #[tokio::test]
    async fn window_activity_project_filter_returns_only_exact_project_evidence() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = crate::auth::shared_key_context("window-filter");
        let project_a = "agent:window-filter-a:proj-a";
        let project_b = "agent:window-filter-b:proj-b";
        register_project(
            &runtime,
            "window-filter-a",
            "proj-a",
            "/private/window-filter-a",
            Some(&auth),
        )
        .await;
        register_project(
            &runtime,
            "window-filter-b",
            "proj-b",
            "/private/window-filter-b",
            Some(&auth),
        )
        .await;
        let window_a = "a".repeat(64);
        let window_b = "b".repeat(64);
        record_window_event(&db, &auth, &window_a, Some(project_a), None, 1_000);
        record_window_event(&db, &auth, &window_b, Some(project_b), None, 2_000);

        let all = windows_for_auth(&runtime, &auth, Some(20), None)
            .await
            .unwrap();
        assert_eq!(all.total, 2);

        let filtered = windows_for_auth(&runtime, &auth, Some(20), Some(project_a))
            .await
            .unwrap();
        assert_eq!(filtered.total, 1);
        assert_eq!(filtered.returned, 1);
        assert_eq!(filtered.windows[0].client_window_key, window_a);
        assert_eq!(filtered.windows[0].last_project.as_deref(), Some(project_a));
        assert!(all
            .windows
            .iter()
            .any(|row| row.last_project.as_deref() == Some(project_b)));
        assert_eq!(
            filtered.windows[0].last_meaningful_activity_at_ms,
            Some(1_001)
        );
    }

    #[tokio::test]
    async fn window_activity_projection_keeps_persisted_meaningful_and_projects_current_activity_semantics(
    ) {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = crate::auth::shared_key_context("window-activity-semantics");
        let project = "agent:window-activity-semantics:project";
        register_project(
            &runtime,
            "window-activity-semantics",
            "project",
            "/private/window-activity-semantics",
            Some(&auth),
        )
        .await;
        let activity_window_key = "e".repeat(64);
        // Deliberately model historical event-time truth that disagrees with the
        // current definition. The read projection must not rewrite it.
        record_window_event_with_activity(
            &db,
            &auth,
            &activity_window_key,
            Some(project),
            None,
            1_000,
            "goal_plan_state",
            true,
        );

        let detail = window_for_auth(
            &runtime,
            &auth,
            WindowInput {
                client_window_key: activity_window_key,
                activity_limit: Some(20),
                session_limit: Some(20),
            },
        )
        .await
        .unwrap();
        let activity = detail.activity.first().expect("projected activity");
        assert_eq!(activity.tool_name.as_deref(), Some("goal_plan_state"));
        assert!(activity.meaningful, "persisted event-time bit must win");
        assert_eq!(activity.activity_presentation.as_deref(), Some("transport"));
        assert_eq!(activity.activity_kind, None);
    }

    #[test]
    fn window_activity_lookup_is_runtime_management_and_current_project_authority_bounded() {
        // This multi-principal integration fixture overflows the default libtest
        // stack in workspace builds, even when selected alone with one test thread.
        // Match the bounded stack isolation used by the large MCP fixtures without
        // changing production runtime stacks or weakening any authority assertions.
        std::thread::Builder::new()
            .name("runtime-console-window-authority".to_string())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("build window authority test runtime")
                    .block_on(
                        window_activity_lookup_is_runtime_management_and_current_project_authority_bounded_body(),
                    );
            })
            .expect("spawn window authority test thread")
            .join()
            .expect("window authority test thread panicked");
    }

    async fn window_activity_lookup_is_runtime_management_and_current_project_authority_bounded_body(
    ) {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth_a = crate::auth::shared_key_context("window-group-a");
        let auth_b = crate::auth::shared_key_context("window-group-b");
        let project_a = "agent:window-a:proj-a";
        let project_b = "agent:window-b:proj-b";
        register_project(
            &runtime,
            "window-a",
            "proj-a",
            "/private/window-a",
            Some(&auth_a),
        )
        .await;
        register_project(
            &runtime,
            "window-b",
            "proj-b",
            "/private/window-b",
            Some(&auth_b),
        )
        .await;

        let window_a = "a".repeat(64);
        let window_b = "b".repeat(64);
        let revoked_project_window = "c".repeat(64);
        let revoked_session_window = "d".repeat(64);
        let foreign_unscoped_window = "e".repeat(64);
        record_window_event(&db, &auth_a, &window_a, Some(project_a), None, 1_000);
        record_window_event(&db, &auth_b, &window_b, Some(project_b), None, 2_000);
        // Model a historical event that was legitimate for this principal before
        // a Project grant was revoked. The current registry intentionally exposes
        // project_b only to auth_b, so auth_a must not retain Window metadata for it.
        record_window_event(
            &db,
            &auth_a,
            &revoked_project_window,
            Some(project_b),
            None,
            3_000,
        );
        // Session/collaboration events can be projectless at the business-call
        // layer while their authoritative Workflow link carries the Project.
        // That link must still enforce current Project visibility.
        record_window_event(
            &db,
            &auth_a,
            &revoked_session_window,
            None,
            Some(("wc_sess_hidden", project_b)),
            4_000,
        );
        // Cross-credential management discovery must not turn a projectless
        // historical event from another principal into shared runtime evidence.
        record_window_event(&db, &auth_b, &foreign_unscoped_window, None, None, 4_500);

        // A meaningful tools/call is not exposed to a non-admin during the
        // tiny pre-resolution interval where its exact Project is not known yet.
        let pre_resolution_window =
            crate::client_window::ClientWindow::for_test("runtime-console-pre-resolution-hidden");
        let pre_resolution_key = pre_resolution_window.key().to_string();
        let (principal_kind, principal_id) =
            crate::tool_runtime::runtime_observation_principal(Some(&auth_a)).unwrap();
        let _pre_resolution = runtime.window_activity.start(
            &pre_resolution_window,
            "trace-pre-resolution",
            "tools/call",
            Some((&principal_kind, &principal_id)),
        );
        runtime.window_activity.update(
            "trace-pre-resolution",
            Some("workspace_hygiene_check"),
            None,
        );

        // Presentation is not visibility authority. observe_jobs is Transport
        // presentation but still Meaningful interaction, so it must fail closed
        // during the same unresolved-Project interval.
        let transport_window = crate::client_window::ClientWindow::for_test(
            "runtime-console-pre-resolution-transport",
        );
        let transport_key = transport_window.key().to_string();
        let _transport = runtime.window_activity.start(
            &transport_window,
            "trace-pre-resolution-transport",
            "tools/call",
            Some((&principal_kind, &principal_id)),
        );
        runtime.window_activity.update(
            "trace-pre-resolution-transport",
            Some("observe_jobs"),
            None,
        );

        // NonMeaningful controller/status traffic keeps the existing bounded
        // diagnostic visibility before exact Project resolution.
        let diagnostic_window = crate::client_window::ClientWindow::for_test(
            "runtime-console-pre-resolution-diagnostic",
        );
        let diagnostic_key = diagnostic_window.key().to_string();
        let _diagnostic = runtime.window_activity.start(
            &diagnostic_window,
            "trace-pre-resolution-diagnostic",
            "tools/call",
            Some((&principal_kind, &principal_id)),
        );
        runtime.window_activity.update(
            "trace-pre-resolution-diagnostic",
            Some("goal_plan_state"),
            None,
        );

        let visible = windows_for_auth(&runtime, &auth_a, Some(20), None)
            .await
            .unwrap();
        assert_eq!(visible.total, 2);
        assert_eq!(visible.returned, 2);
        let visible_keys = visible
            .windows
            .iter()
            .map(|row| row.client_window_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            visible_keys,
            std::collections::BTreeSet::from([window_a.as_str(), diagnostic_key.as_str()])
        );
        assert_eq!(
            visible.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        let serialized = serde_json::to_string(&visible).unwrap();
        assert!(!serialized.contains(&window_b));
        assert!(!serialized.contains(&revoked_project_window));
        assert!(!serialized.contains(&revoked_session_window));
        assert!(!serialized.contains(&foreign_unscoped_window));
        assert!(!serialized.contains(&pre_resolution_key));
        assert!(!serialized.contains(&transport_key));
        assert!(serialized.contains(&diagnostic_key));
        assert!(!serialized.contains(project_b));
        assert!(serialized.contains("\"scope\":\"global\""));

        let own = window_for_auth(
            &runtime,
            &auth_a,
            WindowInput {
                client_window_key: window_a.clone(),
                activity_limit: Some(20),
                session_limit: Some(20),
            },
        )
        .await
        .unwrap();
        assert_eq!(own.client_window_key, window_a);
        assert_eq!(own.last_seen_at_ms, 1_001);
        assert_eq!(
            own.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );

        for hidden_key in [
            &window_b,
            &revoked_project_window,
            &revoked_session_window,
            &foreign_unscoped_window,
            &pre_resolution_key,
            &transport_key,
        ] {
            assert_eq!(
                window_for_auth(
                    &runtime,
                    &auth_a,
                    WindowInput {
                        client_window_key: hidden_key.clone(),
                        activity_limit: Some(20),
                        session_limit: Some(20),
                    },
                )
                .await
                .unwrap_err(),
                RuntimeConsoleError::NotFound
            );
        }

        let admin = test_bootstrap_auth();
        let global = windows_for_auth(&runtime, &admin, Some(20), None)
            .await
            .unwrap();
        assert_eq!(
            global.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        let global_serialized = serde_json::to_string(&global).unwrap();
        assert!(global_serialized.contains("\"scope\":\"global\""));
        let keys = global
            .windows
            .iter()
            .map(|row| row.client_window_key.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            keys,
            std::collections::BTreeSet::from([
                window_a.as_str(),
                window_b.as_str(),
                revoked_project_window.as_str(),
                revoked_session_window.as_str(),
                foreign_unscoped_window.as_str(),
                pre_resolution_key.as_str(),
                transport_key.as_str(),
                diagnostic_key.as_str(),
            ])
        );
    }

    #[tokio::test]
    async fn window_visibility_scope_distinguishes_management_and_project_scoped_credentials() {
        let (_tmp, _db, runtime) = test_runtime_with_window_db();
        let admin = test_bootstrap_auth();
        let ordinary = crate::auth::shared_key_context("window-vis-test");
        let mut project_scoped = AuthContext::new(AuthKind::ProjectCredential);
        project_scoped.project_grant_id = Some("window-project-grant".to_string());
        project_scoped.scopes = vec![
            SCOPE_RUNTIME_READ.to_string(),
            SCOPE_PROJECT_READ.to_string(),
        ];

        for management in [&admin, &ordinary] {
            let list = windows_for_auth(&runtime, management, Some(10), None)
                .await
                .unwrap();
            assert_eq!(
                list.visibility.scope,
                RuntimeConsoleWindowVisibilityScope::Global
            );
            let encoded = serde_json::to_string(&list).unwrap();
            assert!(encoded.contains("\"visibility\":{\"scope\":\"global\"}"));
            assert!(!encoded.contains("window-vis-test"));
        }

        let project_list = windows_for_auth(&runtime, &project_scoped, Some(10), None)
            .await
            .unwrap();
        assert_eq!(
            project_list.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Principal
        );
        assert!(serde_json::to_string(&project_list)
            .unwrap()
            .contains("\"visibility\":{\"scope\":\"principal\"}"));
    }

    #[tokio::test]
    async fn window_management_view_survives_oauth_access_token_rotation() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let mut writer = scoped_oauth(&[SCOPE_RUNTIME_READ, SCOPE_PROJECT_READ]);
        writer.api_key_id = Some("oauth-window-token-a".to_string());
        let mut reader = writer.clone();
        reader.api_key_id = Some("oauth-window-token-b".to_string());
        assert_ne!(
            crate::tool_runtime::runtime_observation_principal(Some(&writer)).unwrap(),
            crate::tool_runtime::runtime_observation_principal(Some(&reader)).unwrap(),
            "fixture must model the historical token-specific observation principal"
        );

        let project = "agent:window-user:shared-project";
        register_project(
            &runtime,
            "window-user",
            "shared-project",
            "/private/window-user",
            Some(&writer),
        )
        .await;
        let window_key = "9".repeat(64);
        record_window_event(&db, &writer, &window_key, Some(project), None, 5_000);

        let list = windows_for_auth(&runtime, &reader, Some(10), None)
            .await
            .unwrap();
        assert_eq!(
            list.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        assert_eq!(list.total, 1);
        assert_eq!(list.windows[0].client_window_key, window_key);
        assert_eq!(list.windows[0].last_project.as_deref(), Some(project));

        let detail = window_for_auth(
            &runtime,
            &reader,
            WindowInput {
                client_window_key: window_key,
                activity_limit: Some(20),
                session_limit: Some(20),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            detail.visibility.scope,
            RuntimeConsoleWindowVisibilityScope::Global
        );
        assert_eq!(detail.activity.len(), 1);
        assert_eq!(detail.activity[0].project.as_deref(), Some(project));
    }

    #[tokio::test]
    async fn window_timing_uses_response_handoff_not_legacy_audit_end() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = crate::auth::shared_key_context("window-timing");
        let project = "agent:window-timing:visible";
        register_project(
            &runtime,
            "window-timing",
            "visible",
            "/private/window-timing",
            Some(&auth),
        )
        .await;
        let window_key = "e".repeat(64);
        record_timed_window_event(
            &db,
            &auth,
            &window_key,
            Some(project),
            1_000,
            1_100,
            1_900,
            "unavailable",
        );
        record_timed_window_event(
            &db,
            &auth,
            &window_key,
            Some(project),
            1_500,
            1_550,
            1_600,
            "serial",
        );

        let detail = window_for_auth(
            &runtime,
            &auth,
            WindowInput {
                client_window_key: window_key,
                activity_limit: Some(20),
                session_limit: Some(20),
            },
        )
        .await
        .unwrap();
        assert_eq!(detail.activity.len(), 2);
        let first = detail
            .activity
            .iter()
            .find(|event| event.service_ms == Some(100))
            .expect("first canonical-timing event");
        assert_eq!(
            first.ended_at_ms, 1_900,
            "legacy audit boundary remains distinct"
        );
        assert_eq!(first.next_call_gap_ms, Some(400));
        assert_eq!(first.cycle_ms, Some(500));
        let second = detail
            .activity
            .iter()
            .find(|event| event.service_ms == Some(50))
            .expect("second canonical-timing event");
        assert_eq!(second.window_transition_kind.as_deref(), Some("serial"));
    }

    #[test]
    fn window_timing_does_not_bridge_an_ineligible_meaningful_event() {
        let (_tmp, db, _runtime) = test_runtime_with_window_db();
        let auth = crate::auth::shared_key_context("interrupted-timing");
        let window_key = "a".repeat(64);
        for (start, transition) in [(1_000, "unavailable"), (1_500, "serial"), (2_000, "serial")] {
            record_timed_window_event(
                &db,
                &auth,
                &window_key,
                None,
                start,
                start + 100,
                start + 100,
                transition,
            );
        }
        let mut events = db
            .list_window_activity_events(&window_key, None, 20)
            .unwrap();
        assert_eq!(events.len(), 3);
        // A retained pre-fix sequence can still label the third request serial.
        // Neither a visible nor a revoked stream may be skipped to pair it with A.
        events[1].window_continuity_eligible = Some(false);
        events[1].response_streaming = Some(true);
        for visible in [[true, true, true], [true, false, true]] {
            let timings = project_window_loop_timings(&events, &visible);
            assert_eq!(timings[2].next_call_gap_ms, visible[1].then_some(400));
            assert_eq!(timings[2].cycle_ms, visible[1].then_some(500));
            assert!(timings[1].next_call_gap_ms.is_none());
            assert!(timings[1].service_ms.is_none());
        }
    }

    #[tokio::test]
    async fn revoked_project_event_cannot_be_bridged_by_window_gap_projection() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth_a = crate::auth::shared_key_context("timing-visible-a");
        let auth_b = crate::auth::shared_key_context("timing-hidden-b");
        let project_a = "agent:timing-a:visible";
        let project_b = "agent:timing-b:hidden";
        register_project(
            &runtime,
            "timing-a",
            "visible",
            "/private/timing-a",
            Some(&auth_a),
        )
        .await;
        register_project(
            &runtime,
            "timing-b",
            "hidden",
            "/private/timing-b",
            Some(&auth_b),
        )
        .await;
        let window_key = "f".repeat(64);
        record_timed_window_event(
            &db,
            &auth_a,
            &window_key,
            Some(project_a),
            1_000,
            1_100,
            1_101,
            "unavailable",
        );
        // Historical same-principal event whose Project is no longer visible.
        record_timed_window_event(
            &db,
            &auth_a,
            &window_key,
            Some(project_b),
            1_500,
            1_550,
            1_551,
            "serial",
        );
        record_timed_window_event(
            &db,
            &auth_a,
            &window_key,
            Some(project_a),
            2_000,
            2_050,
            2_051,
            "serial",
        );

        let detail = window_for_auth(
            &runtime,
            &auth_a,
            WindowInput {
                client_window_key: window_key,
                activity_limit: Some(20),
                session_limit: Some(20),
            },
        )
        .await
        .unwrap();
        assert_eq!(detail.activity.len(), 2);
        assert!(detail
            .activity
            .iter()
            .all(|event| event.project.as_deref() == Some(project_a)));
        assert!(detail
            .activity
            .iter()
            .all(|event| event.next_call_gap_ms.is_none() && event.cycle_ms.is_none()));
        let serialized = serde_json::to_string(&detail).unwrap();
        assert!(!serialized.contains(project_b));
    }

    #[tokio::test]
    async fn session_window_liveness_survives_sparse_workflow_relations() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = crate::auth::shared_key_context("sparse-session-window");
        let project = "agent:sparse-runner:webcodex";
        register_project(
            &runtime,
            "sparse-runner",
            "webcodex",
            "/private/sparse",
            Some(&auth),
        )
        .await;
        let other_project = "agent:other-runner:other";
        register_project(
            &runtime,
            "other-runner",
            "other",
            "/private/other",
            Some(&auth),
        )
        .await;
        let session = runtime.sessions.start_session(
            Some(project.to_string()),
            Some("sparse relation".to_string()),
        );
        let window_key = "0cae4d71e62fe073f130a0e5da2c83425866e4aba9ac5746c043e2ea66000471";

        record_window_event_with_activity(
            &db,
            &auth,
            window_key,
            Some(project),
            Some((&session.session_id, project)),
            1_000,
            "work_on_project",
            true,
        );
        for (at_ms, tool) in [
            (2_000, "observe_jobs"),
            (3_000, "run_shell"),
            (4_000, "observe_jobs"),
        ] {
            record_window_event_with_activity(
                &db,
                &auth,
                window_key,
                Some(project),
                None,
                at_ms,
                tool,
                true,
            );
        }
        // Activity in another currently-visible Project must not advance this
        // Session's Window/Model liveness projection.
        record_window_event_with_activity(
            &db,
            &auth,
            window_key,
            Some(other_project),
            None,
            5_000,
            "run_shell",
            true,
        );
        db.insert_workspace_activity(
            3,
            &webcodex_core::activity_contract::ActivityRecord {
                tool: "run_shell",
                project: Some(project),
                surface: "mcp",
                client: Some("sparse-runner"),
                success: true,
                session_id: Some(&session.session_id),
                command: None,
                paths: Vec::new(),
                error_summary: None,
                scope: webcodex_core::activity_contract::ActivityScope::Unscoped,
            },
            None,
            2_000,
        )
        .unwrap();

        let detail = workflow_session_detail_with_windows(
            &runtime,
            &auth,
            project,
            &session.session_id,
            Some(20),
        )
        .await
        .unwrap();
        assert!(detail.window_activity_available);
        assert_eq!(detail.linked_windows.len(), 1);
        assert_eq!(detail.linked_windows[0].last_linked_at_ms, 1_001);
        assert_eq!(detail.linked_windows[0].last_seen_at_ms, 4_001);
        assert_eq!(
            detail.linked_windows[0].last_meaningful_activity_at_ms,
            Some(4_001)
        );
        assert_eq!(detail.linked_windows[0].active_count, 0);
        assert_eq!(detail.window_activity_after_last_session_record.len(), 3);
        assert_eq!(
            detail
                .window_activity_after_last_session_record
                .iter()
                .filter_map(|activity| activity.tool_name.as_deref())
                .collect::<Vec<_>>(),
            vec!["observe_jobs", "run_shell", "observe_jobs"]
        );
        assert!(!detail.window_activity_after_last_session_record_truncated);
        assert!(detail.workspace_activity_available);
        let workspace = detail.workspace_last_activity.expect("workspace action");
        assert_eq!(workspace.created_at, 3);
        assert_eq!(workspace.tool, "run_shell");
        assert!(workspace.success);
        assert!(detail.job_activity_available);
        assert!(detail.jobs.is_empty());
        assert!(!detail.jobs_truncated);
    }

    #[tokio::test]
    async fn session_window_activity_reports_bounded_source_truncation() {
        let (_tmp, db, runtime) = test_runtime_with_window_db();
        let auth = crate::auth::shared_key_context("session-window-truncation");
        let project = "agent:truncation-runner:webcodex";
        register_project(
            &runtime,
            "truncation-runner",
            "webcodex",
            "/private/truncation",
            Some(&auth),
        )
        .await;
        let other_project = "agent:truncation-other:other";
        register_project(
            &runtime,
            "truncation-other",
            "other",
            "/private/truncation-other",
            Some(&auth),
        )
        .await;
        let session = runtime.sessions.start_session(
            Some(project.to_string()),
            Some("bounded window activity".to_string()),
        );
        let window_key = "1cae4d71e62fe073f130a0e5da2c83425866e4aba9ac5746c043e2ea66000471";
        record_window_event_with_activity(
            &db,
            &auth,
            window_key,
            Some(project),
            Some((&session.session_id, project)),
            1_000,
            "work_on_project",
            true,
        );
        for index in 0..MAX_WINDOW_ACTIVITY_LIMIT {
            record_window_event_with_activity(
                &db,
                &auth,
                window_key,
                Some(other_project),
                None,
                2_000 + index as i64,
                "observe_jobs",
                true,
            );
        }

        let detail = workflow_session_detail_with_windows(
            &runtime,
            &auth,
            project,
            &session.session_id,
            Some(20),
        )
        .await
        .unwrap();
        assert!(detail.window_activity_after_last_session_record.is_empty());
        assert!(detail.window_activity_after_last_session_record_truncated);
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
                session_id: "wc_sess_missing000000000".to_string(),
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
                session_id: "wc_sess_missing000000000".to_string(),
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
                author_session_id: Some("wc_sess_worker0000000000".to_string()),
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
        let ack_required_note = session_post_message_for_auth(
            &runtime,
            &auth_a,
            WorkflowSessionPostMessageInput {
                project: project_id.to_string(),
                session_id: session.session_id.clone(),
                kind: SessionMessageKind::Note,
                priority: SessionMessagePriority::High,
                message: "ack-required note".to_string(),
                reply_to: None,
                requires_ack: true,
            },
        )
        .await
        .unwrap();
        assert_eq!(ack_required_note.kind, "note");
        assert_eq!(ack_required_note.priority, "high");
        assert!(ack_required_note.requires_ack);
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
                    session_id: "wc_sess_missing000000000".to_string(),
                    message_id: "wc_msg_missing000000000".to_string(),
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
                    message_id: "wc_msg_missing000000000".to_string(),
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
            "session_id": "wc_sess_missing000000000",
            "message_id": "wc_msg_missing000000000",
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
            Arc::new(crate::RunnerRegistry::default()),
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
