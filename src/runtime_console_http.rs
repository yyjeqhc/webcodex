//! Browser-only hosted Runtime Console API.
//!
//! This surface is intentionally not a model capability. It reuses the normal
//! runtime project authorization and the existing Workflow Session console
//! projection without creating a second store, parser, or observation authority.

use crate::auth::{AuthContext, SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ};
use crate::tool_runtime::sessions::{
    aggregate_console_list, is_valid_session_id, WorkflowSessionConsoleAggregate,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use salvo::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

const DEFAULT_PROJECT_LIMIT: usize = 50;
const MAX_PROJECT_LIMIT: usize = 100;
const MAX_PROJECT_ID_CHARS: usize = 512;
const MAX_PROJECT_NAME_CHARS: usize = 160;
const MAX_CLIENT_ID_CHARS: usize = 160;
const MAX_STATUS_CHARS: usize = 64;
const DEFAULT_RUNNER_PROJECT_LIMIT: usize = 24;
const MAX_RUNNER_PROJECT_LIMIT: usize = 32;
const CONSOLE_AGGREGATE_SESSION_LIMIT: usize = 50;
const DEFAULT_MESSAGE_LIMIT: usize = 100;
const MAX_MESSAGE_LIMIT: usize = 100;
const MAX_OBSERVATION_TOKEN_CHARS: usize = 192;

pub(crate) fn routes() -> Router {
    Router::with_path("runtime-console")
        .push(Router::with_path("overview").post(overview))
        .push(Router::with_path("runner").post(runner))
        .push(Router::with_path("projects").post(projects))
        .push(Router::with_path("workflow-sessions").post(workflow_sessions))
        .push(Router::with_path("workflow-session").post(workflow_session))
        .push(Router::with_path("workflow-session-messages").post(workflow_session_messages))
        .push(Router::with_path("workflow-session-observe").post(workflow_session_observe))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectsInput {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    author_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_by_message_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleProjects {
    projects: Vec<RuntimeConsoleProject>,
    total: usize,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct RuntimeConsoleProject {
    id: String,
    client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeConsoleError {
    Invalid,
    NotFound,
    Internal,
    Request { status: u16, message: &'static str },
}

impl RuntimeConsoleError {
    fn status(self) -> StatusCode {
        match self {
            Self::Invalid => StatusCode::BAD_REQUEST,
            Self::NotFound => StatusCode::NOT_FOUND,
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
        author_session_id: safe_string(value.get("author_session_id"), 160),
        reply_to: safe_string(value.get("reply_to"), 160),
        resolved_at: value.get("resolved_at").and_then(Value::as_i64),
        resolution: value
            .get("resolution")
            .and_then(Value::as_str)
            .map(str::to_string),
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

async fn listed_projects_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<String>,
    project: Option<String>,
    limit: usize,
) -> Result<(Vec<Value>, usize, bool), RuntimeConsoleError> {
    require_project_read(auth)?;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListProjects {
                client_id,
                project,
                query: None,
                limit: Some(limit),
                summary_only: false,
            },
            Some(auth),
        )
        .await;
    if !result.success {
        return Err(RuntimeConsoleError::Internal);
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
        connected: value
            .get("connected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        agent_status: value
            .get("agent_status")
            .and_then(|value| bounded_text(value, MAX_STATUS_CHARS)),
    })
}

async fn projects_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    limit: Option<usize>,
) -> Result<RuntimeConsoleProjects, RuntimeConsoleError> {
    let limit = limit
        .unwrap_or(DEFAULT_PROJECT_LIMIT)
        .clamp(1, MAX_PROJECT_LIMIT);
    projects_for_client_auth(runtime, auth, None, limit).await
}

async fn projects_for_client_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    client_id: Option<&str>,
    limit: usize,
) -> Result<RuntimeConsoleProjects, RuntimeConsoleError> {
    let (visible, total, source_truncated) =
        listed_projects_for_auth(runtime, auth, client_id.map(str::to_string), None, limit).await?;
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
        listed_projects_for_auth(runtime, auth, None, Some(project.to_string()), 1).await?;
    if visible
        .iter()
        .any(|value| value.get("id").and_then(Value::as_str) == Some(project))
    {
        Ok(())
    } else {
        Err(RuntimeConsoleError::NotFound)
    }
}

async fn workflow_sessions_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    limit: Option<usize>,
) -> Result<crate::tool_runtime::sessions::WorkflowSessionConsoleList, RuntimeConsoleError> {
    authorize_exact_project(runtime, auth, project).await?;
    Ok(runtime.workflow_sessions_console_list(project, limit))
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
    runtime
        .workflow_session_console_detail(project, session_id, limit)
        .ok_or(RuntimeConsoleError::NotFound)
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
        Some(projects_for_auth(runtime, auth, Some(MAX_PROJECT_LIMIT)).await?)
    } else {
        None
    };
    let mut workflow_aggregate = RuntimeConsoleWorkflowAggregate::default();
    if let Some(visible) = visible.as_ref() {
        workflow_aggregate.projects_total = visible.total;
        for project in visible.projects.iter().take(DEFAULT_RUNNER_PROJECT_LIMIT) {
            let list = runtime
                .workflow_sessions_console_list(&project.id, Some(CONSOLE_AGGREGATE_SESSION_LIMIT));
            let aggregate = aggregate_console_list(&list);
            add_console_aggregate(&mut workflow_aggregate, &aggregate);
            workflow_aggregate.projects_scanned += 1;
        }
        workflow_aggregate.truncated |= visible.truncated
            || workflow_aggregate.projects_scanned < workflow_aggregate.projects_total;
    }
    let runner_count = safe_usize(summary.get("count")).max(status_clients.len());
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
        projects_truncated: visible.as_ref().is_some_and(|value| value.truncated),
        workflow_sessions: workflow_aggregate,
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
    let mut project_summaries = Vec::new();
    for project in visible_projects
        .map(|visible| visible.projects)
        .unwrap_or_default()
        .into_iter()
        .take(project_limit)
    {
        let list = runtime
            .workflow_sessions_console_list(&project.id, Some(CONSOLE_AGGREGATE_SESSION_LIMIT));
        project_summaries.push(RuntimeConsoleRunnerProject {
            id: project.id,
            name: project.name,
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
    match projects_for_auth(&runtime, &auth, input.limit).await {
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
    use crate::tool_runtime::{RuntimeInfo, SessionMode};
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;

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
                    client_id: client_id.to_string(),
                    agent_instance_id: format!("inst-{client_id}"),
                    display_name: Some(format!("Device {client_id}")),
                    owner: None,
                    hostname: Some(format!("private-host-{client_id}")),
                    host_context: None,
                    capabilities: Some(ShellClientCapabilities::default()),
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

    #[test]
    fn selector_uses_bounded_authoritative_client_id_without_parsing_project_id() {
        let projected = project_selector_row(&serde_json::json!({
            "id": "agent:not-the-device:project",
            "client_id": "device-real",
            "name": "Demo",
            "connected": true,
            "agent_status": "online"
        }))
        .unwrap();
        assert_eq!(projected.client_id, "device-real");
        assert_ne!(projected.client_id, "not-the-device");

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
        let selector = body["projects"][0].as_object().unwrap();
        assert!(selector.keys().all(|key| matches!(
            key.as_str(),
            "id" | "client_id" | "name" | "connected" | "agent_status"
        )));
        let serialized = serde_json::to_string(&body).unwrap();
        for private in [
            "/root/private/webcodex",
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
        assert_eq!(
            serde_json::to_value(hosted_list).unwrap(),
            serde_json::to_value(direct_list).unwrap()
        );

        let hosted_detail =
            workflow_session_for_auth(&runtime, &auth, project_id, &session.session_id, Some(20))
                .await
                .unwrap();
        let direct_detail = runtime
            .workflow_session_console_detail(project_id, &session.session_id, Some(20))
            .unwrap();
        assert_eq!(
            serde_json::to_value(&hosted_detail).unwrap(),
            serde_json::to_value(&direct_detail).unwrap()
        );
        let serialized = serde_json::to_string(&hosted_detail).unwrap();
        assert!(!serialized.contains("/root/private/source.rs"));
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
        for private in [
            "/private/a",
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
