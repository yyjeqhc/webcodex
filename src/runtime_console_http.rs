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
#[cfg(all(test, feature = "experimental-code-mode"))]
use crate::tool_runtime::window_activity_projection::project_code_mode_composition;
use crate::tool_runtime::window_activity_projection::{
    project_visible_window_activity, project_window_activity, project_window_loop_timings,
    RuntimeConsoleWindowActivity,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use webcodex_core::runner_job_lifecycle::RunnerJobLifecycle;

mod communication;
mod goals;
mod job_projection;
mod window_collaboration;
mod workspace;

use communication::{
    communication_agent_create, communication_agent_update, communication_agents,
    communication_conversation, communication_conversation_create, communication_conversations,
    communication_endpoint_attach, communication_endpoint_detach, communication_endpoint_renew,
    communication_inbox, communication_inbox_consume, communication_message_post,
};
use goals::{goal_handler, goals_handler};
use job_projection::{
    running_jobs_for_auth, session_jobs_for_auth, RunningJobSnapshot, RuntimeConsoleSessionJob,
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
const MAX_WINDOW_JOB_LIMIT: usize = 32;
const MAX_WINDOW_KEY_CHARS: usize = 128;
const PRIMARY_WINDOW_ACTIVITY_SCAN_MULTIPLIER: usize = 4;
const PRIMARY_WINDOW_ACTIVITY_SCAN_FLOOR: usize = 64;

pub(crate) fn routes() -> Router {
    use crate::route_metadata::{api_path, RouteId};
    Router::new()
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleOverview)).post(overview))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleRunner)).post(runner))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleWindows)).post(windows))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleWindow)).post(window))
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWindowCollaboration))
                .post(window_collaboration::list),
        )
        .push(
            Router::with_path(api_path(RouteId::RuntimeConsoleWindowCollaborationPost))
                .post(window_collaboration::post),
        )
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleProjects)).post(projects))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleGoals)).post(goals_handler))
        .push(Router::with_path(api_path(RouteId::RuntimeConsoleGoal)).post(goal_handler))
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
struct OverviewInput {
    #[serde(default)]
    include_sessions: Option<bool>,
}

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

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WindowDetailLevel {
    Primary,
    #[default]
    Full,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WindowInput {
    client_window_key: String,
    #[serde(default)]
    activity_limit: Option<usize>,
    #[serde(default)]
    session_limit: Option<usize>,
    #[serde(default)]
    detail_level: WindowDetailLevel,
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
    detail_level: &'static str,
    authenticated_user: Option<String>,
    effective_config: Value,
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
    active_windows: usize,
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
    first_seen_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_tool_call_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_meaningful_activity_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_activity_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_activity_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_activity_meaningful: Option<bool>,
    active_count: usize,
    linked_session_count: usize,
    recorder_gap_count: usize,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleWindowDetail {
    client_window_key: String,
    detail_level: WindowDetailLevel,
    source: String,
    last_seen_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    first_seen_at_ms: Option<i64>,
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
    jobs: Vec<RuntimeConsoleWindowJob>,
    jobs_truncated: bool,
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
struct RuntimeConsoleWindowJob {
    job_id: String,
    status: String,
    active: bool,
    terminal: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elapsed_secs: Option<u64>,
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
struct RuntimeConsoleWorkspaceActivity {
    created_at: i64,
    tool: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    computer_session_availability: Option<bool>,
    client_id: String,
    connected: bool,
    status: Option<String>,
    transport: Option<String>,
    runner_protocol_generation: Option<u64>,
    last_seen_age_secs: Option<i64>,
    version: Option<String>,
    build_git_commit: Option<String>,
    build_git_dirty: Option<bool>,
    source_alignment: Option<String>,
    protocol_compatibility: String,
    build_alignment: Option<String>,
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
    server: Value,
    tool_request_trace_mode: Option<String>,
    runner_protocol_generation: Option<u64>,
    capabilities: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    computer_session_availability: Option<bool>,
    client_id: String,
    connected: bool,
    coding_agent_providers: Vec<webcodex_core::coding_agent::CodingAgentProviderSummary>,
    status: Option<String>,
    version: Option<String>,
    build_git_commit: Option<String>,
    build_git_dirty: Option<bool>,
    source_alignment: Option<String>,
    protocol_compatibility: String,
    build_alignment: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    registration_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lineage: Option<RuntimeConsoleProjectLineage>,
    connected: bool,
    #[serde(rename = "agent_status", skip_serializing_if = "Option::is_none")]
    runner_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sessions: Option<WorkflowSessionConsoleAggregate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RuntimeConsoleProjectLineage {
    ManagedWorktreeSource {
        source_project_id: String,
        base_sha: String,
    },
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
    bounded_text_str(value.as_str()?, max_chars)
}

fn bounded_text_str(text: &str, max_chars: usize) -> Option<String> {
    let text = text.trim();
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
        | SessionMessageError::DeliveryKeyConflict
        | SessionMessageError::AlreadyCompleted { .. }
        | SessionMessageError::InvalidCompletionState
        | SessionMessageError::InvalidObservationState
        | SessionMessageError::AssignmentStale { .. }
        | SessionMessageError::AssignmentHistoryLost { .. }
        | SessionMessageError::AssignmentTooLarge { .. }
        | SessionMessageError::NotTodo
        | SessionMessageError::SessionClosed { .. } => RuntimeConsoleError::Conflict,
        SessionMessageError::DeliveryPersistenceUncertain
        | SessionMessageError::PersistenceUncertain => RuntimeConsoleError::PersistenceUncertain,
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

    let project_ids = visible
        .projects
        .iter()
        .take(HOME_PROJECT_SCAN_LIMIT)
        .map(|project| project.id.as_str())
        .collect::<Vec<_>>();
    let mut lists = runtime
        .workflow_sessions_console_lists(&project_ids, Some(HOME_SESSIONS_PER_PROJECT_LIMIT));
    for project in visible.projects.iter().take(HOME_PROJECT_SCAN_LIMIT) {
        let mut list = lists
            .remove(&project.id)
            .expect("visible project has a console list");
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
        .get("runners")
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
                computer_session_availability: runner_value
                    .get("computer_session_availability")
                    .and_then(Value::as_bool),
                protocol_compatibility: status
                    .and_then(|value| value.get("protocol_compatibility"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                build_alignment: status
                    .and_then(|value| safe_string(value.get("build_alignment"), MAX_STATUS_CHARS)),
                client_id: client_id.clone(),
                connected: safe_bool(runner_value.get("connected")),
                status: safe_string(runner_value.get("status"), MAX_STATUS_CHARS),
                transport: safe_string(runner_value.get("transport"), MAX_STATUS_CHARS),
                runner_protocol_generation: runner_value
                    .get("runner_protocol_generation")
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

fn project_lineage(value: &Value) -> Option<RuntimeConsoleProjectLineage> {
    let lineage = value.get("lineage")?;
    if lineage.get("kind")?.as_str()? != "managed_worktree_source" {
        return None;
    }
    let source_project_id = bounded_text(lineage.get("source_project_id")?, MAX_PROJECT_ID_CHARS)?;
    let base_sha = bounded_text(lineage.get("base_sha")?, 64)?;
    Some(RuntimeConsoleProjectLineage::ManagedWorktreeSource {
        source_project_id,
        base_sha,
    })
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
        registration_source: value
            .get("registration_source")
            .and_then(|value| bounded_text(value, MAX_STATUS_CHARS)),
        lineage: project_lineage(value),
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

fn window_summary_internal_tool(tool: Option<&str>) -> bool {
    matches!(
        tool,
        Some(
            "present_work_result"
                | "work_result_state"
                | "work_result_send_message"
                | "changes_file_diff"
        )
    )
}

async fn visible_window_summary_for_auth_bounded(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    window_key: &str,
    visibility_cache: &mut HashMap<String, bool>,
    project_filter: Option<&str>,
    activity_scan_limit: usize,
    include_relation_count: bool,
) -> Result<Option<RuntimeConsoleWindowSummary>, RuntimeConsoleError> {
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    #[cfg(feature = "experimental-code-mode")]
    let events = db.list_window_activity_events_with_code_mode_composition(
        window_key,
        principal,
        activity_scan_limit,
    );
    #[cfg(not(feature = "experimental-code-mode"))]
    let events = db.list_window_activity_events(window_key, principal, activity_scan_limit);
    let events = events.map_err(|_| RuntimeConsoleError::Internal)?;
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let mut source = None;
    let mut last_seen_at_ms = None;
    let mut first_seen_at_ms = None;
    let mut last_tool_call_at_ms = None;
    let mut last_meaningful_activity_at_ms = None;
    let mut recorder_gap_count = 0usize;
    let mut last_project = None;
    let mut project_observed_at = i64::MIN;
    let mut last_activity_name = None;
    let mut last_activity_status = None;
    let mut last_activity_meaningful = None;
    let mut last_activity_observed_at = i64::MIN;
    let mut latest_active_started_at = i64::MIN;
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
        if window_summary_internal_tool(event.operation.as_deref()) {
            continue;
        }
        source = Some(event.client_window_source.clone());
        last_seen_at_ms = Some(last_seen_at_ms.unwrap_or(i64::MIN).max(event.ended_at_ms));
        first_seen_at_ms = Some(first_seen_at_ms.unwrap_or(i64::MAX).min(event.ended_at_ms));
        if event.action_name == "toolsCall" {
            last_tool_call_at_ms = Some(
                last_tool_call_at_ms
                    .unwrap_or(i64::MIN)
                    .max(event.ended_at_ms),
            );
        }
        if event.project.is_some() && event.ended_at_ms > project_observed_at {
            last_project = event.project.clone();
            project_observed_at = event.ended_at_ms;
        }
        if event.ended_at_ms > last_activity_observed_at {
            last_activity_name = event
                .operation
                .clone()
                .or_else(|| Some(event.action_name.clone()));
            last_activity_status = Some(event.status.clone());
            last_activity_meaningful = Some(event.meaningful);
            last_activity_observed_at = event.ended_at_ms;
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
    let mut linked_session_count = 0usize;
    if include_relation_count {
        let relation_rows = db
            .list_window_workflow_sessions(window_key, principal, MAX_WINDOW_SESSION_LIMIT)
            .map_err(|_| RuntimeConsoleError::Internal)?;
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
        if window_summary_internal_tool(request.tool_name.as_deref()) {
            continue;
        }
        source = Some(request.client_window_source.clone());
        last_seen_at_ms = Some(
            last_seen_at_ms
                .unwrap_or(i64::MIN)
                .max(request.started_at_ms),
        );
        first_seen_at_ms = Some(
            first_seen_at_ms
                .unwrap_or(i64::MAX)
                .min(request.started_at_ms),
        );
        if request.started_at_ms >= latest_active_started_at {
            if request.project.is_some() {
                last_project = request.project.clone();
            }
            last_activity_name = request
                .tool_name
                .clone()
                .or_else(|| Some(request.method.clone()));
            last_activity_status = Some("running".to_string());
            last_activity_meaningful = Some(request.is_meaningful());
            latest_active_started_at = request.started_at_ms;
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
        first_seen_at_ms: if include_relation_count {
            first_seen_at_ms
        } else {
            None
        },
        last_tool_call_at_ms,
        last_meaningful_activity_at_ms,
        last_activity_name,
        last_activity_status,
        last_activity_meaningful,
        active_count,
        linked_session_count,
        recorder_gap_count,
    }))
}

async fn visible_window_summary_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    principal: Option<(&str, &str)>,
    window_key: &str,
    visibility_cache: &mut HashMap<String, bool>,
    project_filter: Option<&str>,
) -> Result<Option<RuntimeConsoleWindowSummary>, RuntimeConsoleError> {
    visible_window_summary_for_auth_bounded(
        runtime,
        auth,
        principal,
        window_key,
        visibility_cache,
        project_filter,
        MAX_WINDOW_ACTIVITY_LIMIT,
        true,
    )
    .await
}

fn primary_window_activity_scan_limit(activity_limit: usize) -> usize {
    activity_limit
        .saturating_mul(PRIMARY_WINDOW_ACTIVITY_SCAN_MULTIPLIER)
        .max(PRIMARY_WINDOW_ACTIVITY_SCAN_FLOOR)
        .min(MAX_WINDOW_ACTIVITY_LIMIT)
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
                    first_seen_at_ms: Some(summary.first_seen_at_ms),
                    last_tool_call_at_ms: summary.last_tool_call_at_ms,
                    last_meaningful_activity_at_ms: summary.last_meaningful_activity_at_ms,
                    last_activity_name: None,
                    last_activity_status: None,
                    last_activity_meaningful: None,
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
                        first_seen_at_ms: Some(summary.first_seen_at_ms),
                        last_tool_call_at_ms: summary.last_tool_call_at_ms,
                        last_meaningful_activity_at_ms: summary.last_meaningful_activity_at_ms,
                        last_activity_name: None,
                        last_activity_status: None,
                        last_activity_meaningful: None,
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
                        first_seen_at_ms: Some(live.last_started_at_ms),
                        last_tool_call_at_ms: None,
                        last_meaningful_activity_at_ms: None,
                        last_activity_name: None,
                        last_activity_status: Some("running".to_string()),
                        last_activity_meaningful: None,
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
                row.last_activity_name = observed.last_activity_name;
                row.last_activity_status = observed.last_activity_status;
                row.last_activity_meaningful = observed.last_activity_meaningful;
                row.active_count = observed.active_count;
                row.last_seen_at_ms = observed.last_seen_at_ms;
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

async fn active_window_count_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
) -> Result<usize, RuntimeConsoleError> {
    let principal = window_principal_filter(auth)?;
    let principal_ref = window_principal_ref(&principal);
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let mut visibility_cache = HashMap::new();
    let mut visible_windows = 0usize;

    for active_window in runtime.window_activity.active_windows(principal_ref) {
        let mut visible = false;
        for request in runtime
            .window_activity
            .list_for_window(&active_window.client_window_key, principal_ref)
        {
            if console_active_window_request_visible_cached(
                runtime,
                auth,
                principal_ref,
                caller_principal_ref,
                &mut visibility_cache,
                &request,
            )
            .await
            {
                visible = true;
                break;
            }
        }
        if visible {
            visible_windows = visible_windows.saturating_add(1);
        }
    }
    Ok(visible_windows)
}

async fn window_jobs_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    activity: &[RuntimeConsoleWindowActivity],
) -> (Vec<RuntimeConsoleWindowJob>, bool) {
    let mut seen = HashSet::new();
    let mut job_ids = Vec::new();
    for event in activity {
        for job_id in event
            .async_job_id
            .iter()
            .chain(event.observed_job_ids.iter())
        {
            if seen.insert(job_id.clone()) {
                job_ids.push(job_id.clone());
            }
        }
    }
    let truncated = job_ids.len() > MAX_WINDOW_JOB_LIMIT;
    job_ids.truncate(MAX_WINDOW_JOB_LIMIT);

    let access = crate::runner_http::runner_access_from_auth(Some(auth));
    let mut jobs = Vec::new();
    for job_id in job_ids {
        let Ok(job) = runtime
            .runner_registry
            .get_job_for_auth(access.as_ref(), &job_id)
            .await
        else {
            continue;
        };
        let terminal =
            RunnerJobLifecycle::from_wire(&job.status).is_ok_and(RunnerJobLifecycle::is_terminal);
        jobs.push(RuntimeConsoleWindowJob {
            job_id: job.job_id,
            status: job.status.clone(),
            active: webcodex_runner_registry::job_status_is_active(&job.status),
            terminal,
            started_at: job.started_at,
            ended_at: job.ended_at,
            duration_ms: job.duration_ms,
            elapsed_secs: job.elapsed_secs,
        });
    }
    (jobs, truncated)
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
    let durable_first_seen_at_ms = if auth.is_admin_caller() {
        db.get_window_activity_summary(&input.client_window_key, principal_ref)
            .map_err(|_| RuntimeConsoleError::Internal)?
            .map(|summary| summary.first_seen_at_ms)
    } else {
        None
    };
    let caller_principal = if principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let detail_level = input.detail_level;
    let primary_detail = detail_level == WindowDetailLevel::Primary;
    let activity_limit = input
        .activity_limit
        .unwrap_or(DEFAULT_WINDOW_ACTIVITY_LIMIT)
        .clamp(1, MAX_WINDOW_ACTIVITY_LIMIT);
    let session_limit = input
        .session_limit
        .unwrap_or(DEFAULT_WINDOW_SESSION_LIMIT)
        .clamp(1, MAX_WINDOW_SESSION_LIMIT);

    let mut visibility_cache = HashMap::new();
    let summary = if primary_detail {
        let primary_scan_limit = primary_window_activity_scan_limit(activity_limit);
        let quick = visible_window_summary_for_auth_bounded(
            runtime,
            auth,
            principal_ref,
            &input.client_window_key,
            &mut visibility_cache,
            None,
            primary_scan_limit,
            false,
        )
        .await?;
        if quick.is_some() {
            quick
        } else {
            // A bounded primary scan can land entirely on currently-hidden Project
            // history. Fall back to the canonical full authority scan rather than
            // turning a performance optimization into a false 404/existence signal.
            visible_window_summary_for_auth(
                runtime,
                auth,
                principal_ref,
                &input.client_window_key,
                &mut visibility_cache,
                None,
            )
            .await?
        }
    } else {
        visible_window_summary_for_auth(
            runtime,
            auth,
            principal_ref,
            &input.client_window_key,
            &mut visibility_cache,
            None,
        )
        .await?
    };
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

    let activity_scan_limit = if primary_detail {
        primary_window_activity_scan_limit(activity_limit)
    } else if auth.is_admin_caller() {
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
    for ((event, visible), timing) in raw_activity.into_iter().zip(activity_visible).zip(timing) {
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

    let (jobs, jobs_truncated) = if primary_detail {
        (Vec::new(), false)
    } else {
        window_jobs_for_auth(runtime, auth, &activity).await
    };

    let (linked_sessions, sessions_truncated) = if primary_detail {
        // Activity rows already carry exact Workflow Session links, which is
        // sufficient for immediate Session chips/filtering. Titles/lifecycle and
        // canonical relation history are hydrated by the full follow-up request.
        (Vec::new(), false)
    } else {
        let session_scan_limit = if auth.is_admin_caller() {
            session_limit
                .saturating_add(1)
                .min(MAX_WINDOW_SESSION_LIMIT)
        } else {
            MAX_WINDOW_SESSION_LIMIT
        };
        let raw_sessions = db
            .list_window_workflow_sessions(
                &input.client_window_key,
                principal_ref,
                session_scan_limit,
            )
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
            let detail = runtime.workflow_session_console_detail(
                project,
                &link.workflow_session_id,
                Some(1),
            );
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
        (linked_sessions, sessions_truncated)
    };

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
        detail_level,
        source,
        last_seen_at_ms,
        first_seen_at_ms: durable_first_seen_at_ms.or_else(|| {
            summary
                .as_ref()
                .and_then(|summary| summary.first_seen_at_ms)
        }),
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
        jobs,
        jobs_truncated,
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

#[cfg(test)]
async fn overview_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
) -> Result<RuntimeConsoleOverview, RuntimeConsoleError> {
    overview_for_auth_detail(runtime, auth, true).await
}

async fn overview_for_auth_detail(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    include_sessions: bool,
) -> Result<RuntimeConsoleOverview, RuntimeConsoleError> {
    require_runtime_read(auth)?;
    let (status, runners_value) = tokio::try_join!(
        runtime_status_value(runtime, auth, None),
        list_runners_value(runtime, auth, None),
    )?;
    let summary = runners_value.get("summary").unwrap_or(&Value::Null);
    let build = status.get("build").unwrap_or(&Value::Null);
    let status_clients = status
        .get("runners")
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
    let running_jobs = if include_sessions && visible.is_some() {
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
        |visible| {
            if include_sessions {
                scan_runtime_home(runtime, visible, &running_jobs)
            } else {
                // Registry-only first paint. Unscanned Sessions are not proven empty.
                RuntimeConsoleHomeScan {
                    workflow: RuntimeConsoleWorkflowAggregate {
                        projects_total: visible.total,
                        truncated: visible.total > 0,
                        ..Default::default()
                    },
                    recent_sessions: finalize_recent_sessions(Vec::new(), visible.total > 0),
                    projects: visible.projects.clone(),
                    runner_sessions: HashMap::new(),
                    runner_projects_scanned: HashMap::new(),
                    project_scan_truncated: visible.truncated,
                }
            }
        },
    );
    let runners = runner_fleet_rows(&runners_value, &status_clients, &home);
    let runner_count = safe_usize(summary.get("count")).max(runners.len());
    let online = safe_usize(summary.get("online"));
    let stale = safe_usize(summary.get("stale"));
    let unavailable = runner_count.saturating_sub(online.saturating_add(stale));
    let active_windows = active_window_count_for_auth(runtime, auth).await?;
    Ok(RuntimeConsoleOverview {
        detail_level: if include_sessions { "full" } else { "primary" },
        authenticated_user: auth.username.clone(),
        effective_config: runtime.effective_config_status(),
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
        active_windows,
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
        .get("runners")
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
        server: status.get("server").cloned().unwrap_or(Value::Null),
        tool_request_trace_mode: safe_string(
            status.pointer("/effective_config/tool_request_trace_mode"),
            16,
        ),
        runner_protocol_generation: runner_value
            .get("runner_protocol_generation")
            .and_then(Value::as_u64),
        capabilities: runner_value
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        computer_session_availability: runner_value
            .get("computer_session_availability")
            .and_then(Value::as_bool),
        protocol_compatibility: focus
            .get("protocol_compatibility")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        build_alignment: safe_string(focus.get("build_alignment"), MAX_STATUS_CHARS),
        client_id: client_id.to_string(),
        coding_agent_providers: runner_value
            .get("coding_agent_providers")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
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
                delivery_key: None,
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
    let input = match req.parse_json::<OverviewInput>().await {
        Ok(input) => input,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match overview_for_auth_detail(&runtime, &auth, input.include_sessions.unwrap_or(true)).await {
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
mod tests;
