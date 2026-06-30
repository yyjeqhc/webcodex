//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

mod cargo;
mod codex;
pub(crate) mod files;
mod git;
mod helpers;
mod jobs;
pub(crate) mod kernel;
pub(crate) mod metadata;
mod patch;
mod projects;
mod registry;
pub(crate) mod sessions;
mod shell;
mod types;

// Re-export the public API so `crate::tool_runtime::ToolCall` etc. still work.
#[allow(unused_imports)]
pub use types::{
    default_true, is_known_tool_name, RuntimeInfo, SessionMode, ToolCall, ToolResult, ToolSpec,
    KNOWN_TOOL_NAMES,
};

use crate::auth::AuthContext;
use crate::config::CodexConfig;
use crate::projects::{Executor, ProjectConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{ShellAgentProjectSummary, ShellClientView};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

use helpers::normalize_local_status;
use types::{
    AgentCapability, LocalJobKiller, LocalJobRecord, SystemJobKiller, ACTIVE_JOB_STATUSES,
};

#[derive(Clone)]
pub struct ToolRuntime {
    pub projects: Arc<ProjectsState>,
    pub shell_clients: Arc<ShellClientRegistry>,
    pub codex: Arc<CodexConfig>,
    pub runtime_info: Arc<RuntimeInfo>,
    pub(crate) sessions: sessions::SessionStore,
    local_jobs: Arc<Mutex<HashMap<String, LocalJobRecord>>>,
    job_killer: Arc<dyn LocalJobKiller>,
}

#[derive(Debug, Clone)]
struct ProjectResolverCandidate {
    id: String,
    client_id: String,
    agent_project_id: String,
    name: Option<String>,
    path: String,
    allow_patch: bool,
    connected: bool,
    status: String,
    last_seen: i64,
}

#[derive(Debug, Clone)]
struct ResolvedProject {
    input: String,
    resolved_id: String,
    config: ProjectConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectResolverErrorKind {
    UnknownProject,
    AmbiguousProject,
}

impl ProjectResolverErrorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::UnknownProject => "unknown_project",
            Self::AmbiguousProject => "ambiguous_project",
        }
    }
}

#[derive(Debug, Clone)]
struct ProjectResolverError {
    kind: ProjectResolverErrorKind,
    project: String,
    candidates: Vec<ProjectResolverCandidate>,
}

impl ProjectResolverError {
    fn candidate_payload(candidate: &ProjectResolverCandidate) -> Value {
        json!({
            "id": candidate.id,
            "client_id": candidate.client_id,
            "agent_project_id": candidate.agent_project_id,
            "name": candidate.name,
            "path": candidate.path,
            "connected": candidate.connected,
            "status": candidate.status,
            "last_seen": candidate.last_seen,
        })
    }

    fn to_output(&self) -> Value {
        let candidates: Vec<Value> = self
            .candidates
            .iter()
            .map(Self::candidate_payload)
            .collect();
        json!({
            "error_kind": self.kind.as_str(),
            "project": self.project,
            "hint": "Use a full runtime project id in the form agent:<client_id>:<project_id> from list_projects.",
            "candidates": candidates,
        })
    }

    fn to_message(&self) -> String {
        let mut message = format!(
            "{} '{}'. Use a full runtime project id in the form agent:<client_id>:<project_id> from list_projects.",
            match self.kind {
                ProjectResolverErrorKind::UnknownProject => "unknown_project",
                ProjectResolverErrorKind::AmbiguousProject => "ambiguous_project",
            },
            self.project
        );
        if self.candidates.is_empty() {
            return message;
        }
        let candidate_summary = self
            .candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{} [{}] {} ({})",
                    candidate.id, candidate.client_id, candidate.path, candidate.status
                )
            })
            .collect::<Vec<_>>()
            .join("; ");
        message.push_str(" Candidates: ");
        message.push_str(&candidate_summary);
        message
    }

    fn into_tool_result(self) -> ToolResult {
        ToolResult::err_with_output(self.to_message(), self.to_output())
    }
}

impl std::fmt::Display for ProjectResolverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_message())
    }
}

impl From<ProjectResolverError> for String {
    fn from(value: ProjectResolverError) -> Self {
        value.to_message()
    }
}

fn unknown_session_result(session_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!("unknown_session_id: {}", session_id),
        json!({
            "error_kind": "unknown_session_id",
            "session_id": session_id,
        }),
    )
}

fn session_guard_denied_result(
    session_id: &str,
    tool_name: &str,
    denial: sessions::SessionGuardDenial,
) -> ToolResult {
    let mut output = json!({
        "error_kind": "session_guard_denied",
        "session_id": session_id,
        "tool_name": tool_name,
        "guard": denial.guard,
        "mode": denial.mode.as_str(),
    });
    if denial.guard == "deny_shell_tools" {
        output["command_started"] = Value::Bool(false);
    }
    ToolResult::err_with_output(
        format!(
            "session_guard_denied: {} blocked by {} session",
            tool_name,
            denial.mode.as_str()
        ),
        output,
    )
}

fn add_session_telemetry_hint(result: &mut ToolResult, session_id: &str, event_id: Option<String>) {
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(map) => map,
        other => {
            let mut map = serde_json::Map::new();
            map.insert("value".to_string(), other);
            map
        }
    };
    output.insert(
        "session_recorded".to_string(),
        Value::Bool(event_id.is_some()),
    );
    output.insert(
        "session_id".to_string(),
        Value::String(session_id.to_string()),
    );
    if let Some(event_id) = event_id {
        output.insert("session_event_id".to_string(), Value::String(event_id));
    }
    result.output = Value::Object(output);
}

impl ToolRuntime {
    pub fn new(
        projects: Arc<ProjectsState>,
        shell_clients: Arc<ShellClientRegistry>,
        codex: Arc<CodexConfig>,
        runtime_info: Arc<RuntimeInfo>,
    ) -> Self {
        Self {
            projects,
            shell_clients,
            codex,
            runtime_info,
            sessions: sessions::SessionStore::default(),
            local_jobs: Arc::new(Mutex::new(HashMap::new())),
            job_killer: Arc::new(SystemJobKiller),
        }
    }

    fn agent_project_runtime_id(client_id: &str, project_id: &str) -> String {
        format!("agent:{}:{}", client_id, project_id)
    }

    fn project_candidate_from_view(
        client: &ShellClientView,
        project: &ShellAgentProjectSummary,
    ) -> ProjectResolverCandidate {
        ProjectResolverCandidate {
            id: Self::agent_project_runtime_id(&client.client_id, &project.id),
            client_id: client.client_id.clone(),
            agent_project_id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
            allow_patch: project.allow_patch,
            connected: client.connected,
            status: client.status.clone(),
            last_seen: client.last_seen,
        }
    }

    fn project_config_from_candidate(candidate: &ProjectResolverCandidate) -> ProjectConfig {
        ProjectConfig {
            path: candidate.path.clone(),
            executor: Executor::Agent,
            client_id: Some(candidate.client_id.clone()),
            allow_patch: candidate.allow_patch,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: Vec::new(),
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    fn sort_resolver_candidates(candidates: &mut [ProjectResolverCandidate]) {
        candidates.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.status.cmp(&b.status))
                .then_with(|| b.last_seen.cmp(&a.last_seen))
                .then_with(|| a.id.cmp(&b.id))
        });
    }

    async fn agent_project_candidates(&self) -> Vec<ProjectResolverCandidate> {
        let mut candidates = Vec::new();
        for client in self.shell_clients.list_clients().await {
            for project in client.projects.iter().filter(|project| !project.disabled) {
                candidates.push(Self::project_candidate_from_view(&client, project));
            }
        }
        Self::sort_resolver_candidates(&mut candidates);
        candidates
    }

    async fn resolve_project_input(
        &self,
        project: &str,
    ) -> Result<ResolvedProject, ProjectResolverError> {
        let raw = project.trim();
        if raw.is_empty() {
            return Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: project.to_string(),
                candidates: self.agent_project_candidates().await,
            });
        }

        let all_candidates = self.agent_project_candidates().await;

        if raw.starts_with("agent:") {
            let Some(rest) = raw.strip_prefix("agent:") else {
                unreachable!();
            };
            let Some((client_id, agent_project_id)) = rest.split_once(':') else {
                return Err(ProjectResolverError {
                    kind: ProjectResolverErrorKind::UnknownProject,
                    project: raw.to_string(),
                    candidates: all_candidates,
                });
            };
            if client_id.trim().is_empty() || agent_project_id.trim().is_empty() {
                return Err(ProjectResolverError {
                    kind: ProjectResolverErrorKind::UnknownProject,
                    project: raw.to_string(),
                    candidates: all_candidates,
                });
            }
            if let Some(candidate) = all_candidates.iter().find(|candidate| candidate.id == raw) {
                return Ok(ResolvedProject {
                    input: project.to_string(),
                    resolved_id: candidate.id.clone(),
                    config: Self::project_config_from_candidate(candidate),
                });
            }
            let mut same_client: Vec<ProjectResolverCandidate> = all_candidates
                .iter()
                .filter(|candidate| candidate.client_id == client_id)
                .cloned()
                .collect();
            Self::sort_resolver_candidates(&mut same_client);
            return Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: raw.to_string(),
                candidates: same_client,
            });
        }

        if let Some((client_id, agent_project_id)) = raw.split_once(':') {
            if !client_id.trim().is_empty() && !agent_project_id.trim().is_empty() {
                let mut matches: Vec<ProjectResolverCandidate> = all_candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.client_id == client_id
                            && candidate.agent_project_id == agent_project_id
                    })
                    .cloned()
                    .collect();
                Self::sort_resolver_candidates(&mut matches);
                match matches.len() {
                    1 => {
                        let candidate = matches.remove(0);
                        return Ok(ResolvedProject {
                            input: project.to_string(),
                            resolved_id: candidate.id.clone(),
                            config: Self::project_config_from_candidate(&candidate),
                        });
                    }
                    0 => {
                        let mut same_client: Vec<ProjectResolverCandidate> = all_candidates
                            .iter()
                            .filter(|candidate| candidate.client_id == client_id)
                            .cloned()
                            .collect();
                        Self::sort_resolver_candidates(&mut same_client);
                        return Err(ProjectResolverError {
                            kind: ProjectResolverErrorKind::UnknownProject,
                            project: raw.to_string(),
                            candidates: same_client,
                        });
                    }
                    _ => {
                        return Err(ProjectResolverError {
                            kind: ProjectResolverErrorKind::AmbiguousProject,
                            project: raw.to_string(),
                            candidates: matches,
                        });
                    }
                }
            }
        }

        let mut short_id_matches: Vec<ProjectResolverCandidate> = all_candidates
            .iter()
            .filter(|candidate| candidate.agent_project_id == raw)
            .cloned()
            .collect();
        Self::sort_resolver_candidates(&mut short_id_matches);
        match short_id_matches.len() {
            1 => {
                let candidate = short_id_matches.remove(0);
                return Ok(ResolvedProject {
                    input: project.to_string(),
                    resolved_id: candidate.id.clone(),
                    config: Self::project_config_from_candidate(&candidate),
                });
            }
            n if n > 1 => {
                return Err(ProjectResolverError {
                    kind: ProjectResolverErrorKind::AmbiguousProject,
                    project: raw.to_string(),
                    candidates: short_id_matches,
                });
            }
            _ => {}
        }

        let mut name_matches: Vec<ProjectResolverCandidate> = all_candidates
            .iter()
            .filter(|candidate| candidate.name.as_deref() == Some(raw))
            .cloned()
            .collect();
        Self::sort_resolver_candidates(&mut name_matches);
        match name_matches.len() {
            1 => {
                let candidate = name_matches.remove(0);
                Ok(ResolvedProject {
                    input: project.to_string(),
                    resolved_id: candidate.id.clone(),
                    config: Self::project_config_from_candidate(&candidate),
                })
            }
            n if n > 1 => Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::AmbiguousProject,
                project: raw.to_string(),
                candidates: name_matches,
            }),
            _ => Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: raw.to_string(),
                candidates: all_candidates,
            }),
        }
    }

    async fn resolve_project(&self, project: &str) -> Result<ProjectConfig, ProjectResolverError> {
        self.resolve_project_input(project)
            .await
            .map(|resolved| resolved.config)
    }

    /// The capability an agent-backed tool variant requires from the agent
    /// client. Non-agent tools (and tools without a project) require nothing.
    fn required_agent_capability(call: &ToolCall) -> Option<AgentCapability> {
        match call {
            ToolCall::RunShell { .. }
            | ToolCall::ApplyPatch { .. }
            | ToolCall::ApplyPatchChecked { .. }
            | ToolCall::DeleteProjectFiles { .. }
            | ToolCall::GitRestorePaths { .. }
            | ToolCall::DiscardUntracked { .. } => Some(AgentCapability::Shell),
            // validate_patch runs read-only `git apply --check`/`--stat` via
            // the agent shell path; it requires the same shell capability as
            // apply_patch but never mutates the worktree.
            ToolCall::ValidatePatch { .. } => Some(AgentCapability::Shell),
            // Fixed helper commands over the shell path. Line edits use native
            // agent file ops and require file_write instead.
            ToolCall::ReplaceInFile { .. }
            | ToolCall::WriteProjectFile { .. }
            | ToolCall::SaveProjectArtifact { .. }
            | ToolCall::ReadProjectArtifactMetadata { .. }
            | ToolCall::ReadProjectArtifact { .. } => Some(AgentCapability::Shell),
            ToolCall::ReplaceLineRange { .. }
            | ToolCall::InsertAtLine { .. }
            | ToolCall::DeleteLineRange { .. }
            | ToolCall::ReplaceExactBlock { .. }
            | ToolCall::InsertBeforePattern { .. }
            | ToolCall::InsertAfterPattern { .. } => Some(AgentCapability::FileWrite),
            ToolCall::ReadFile { .. } | ToolCall::ListProjectFiles { .. } => {
                Some(AgentCapability::FileRead)
            }
            // Search runs a bounded `grep` via the agent shell path.
            ToolCall::SearchProjectText { .. } => Some(AgentCapability::Shell),
            ToolCall::GitStatus { .. }
            | ToolCall::GitDiff { .. }
            | ToolCall::GitDiffHunks { .. }
            | ToolCall::GitDiffSummary { .. }
            | ToolCall::ShowChanges { .. } => Some(AgentCapability::GitOrShell),
            ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. } => Some(AgentCapability::Shell),
            ToolCall::RunJob { .. } | ToolCall::RunCodex { .. } => Some(AgentCapability::AsyncJobs),
            ToolCall::ListTools
            | ToolCall::StartSession { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::ListProjects
            | ToolCall::RegisterProject { .. }
            | ToolCall::CreateProject { .. }
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus
            | ToolCall::JobStatus { .. }
            | ToolCall::JobLog { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. } => None,
        }
    }

    /// Enforce the owner boundary and capability requirements for agent-backed
    /// runtime tools before dispatching. This is the single place where the
    /// runtime paths (`/api/tools/call`, `/api/codex/run`, `/api/projects/*`,
    /// `/mcp`) check that the caller is allowed to drive an agent. Legacy
    /// `/api/shell/*` handlers keep their own `assert_shell_client_owner`
    /// checks; this method closes the gap for the runtime paths.
    ///
    /// Returns `Ok(())` for local-executor projects and project-less tools so
    /// they are unaffected.
    async fn authorize_agent_tool(
        &self,
        call: &ToolCall,
        auth: Option<&AuthContext>,
    ) -> Result<(), ToolResult> {
        let project = match call {
            ToolCall::RunShell { project, .. }
            | ToolCall::ApplyPatch { project, .. }
            | ToolCall::ApplyPatchChecked { project, .. }
            | ToolCall::DeleteProjectFiles { project, .. }
            | ToolCall::GitRestorePaths { project, .. }
            | ToolCall::DiscardUntracked { project, .. }
            | ToolCall::ValidatePatch { project, .. }
            | ToolCall::ReplaceInFile { project, .. }
            | ToolCall::ReplaceExactBlock { project, .. }
            | ToolCall::InsertBeforePattern { project, .. }
            | ToolCall::InsertAfterPattern { project, .. }
            | ToolCall::WriteProjectFile { project, .. }
            | ToolCall::SaveProjectArtifact { project, .. }
            | ToolCall::ReadProjectArtifactMetadata { project, .. }
            | ToolCall::ReadProjectArtifact { project, .. }
            | ToolCall::ReplaceLineRange { project, .. }
            | ToolCall::InsertAtLine { project, .. }
            | ToolCall::DeleteLineRange { project, .. }
            | ToolCall::GitStatus { project, .. }
            | ToolCall::GitDiff { project, .. }
            | ToolCall::GitDiffHunks { project, .. }
            | ToolCall::CargoFmt { project, .. }
            | ToolCall::CargoCheck { project, .. }
            | ToolCall::CargoTest { project, .. }
            | ToolCall::GitDiffSummary { project, .. }
            | ToolCall::ShowChanges { project, .. }
            | ToolCall::ReadFile { project, .. }
            | ToolCall::ListProjectFiles { project, .. }
            | ToolCall::SearchProjectText { project, .. }
            | ToolCall::RunJob { project, .. }
            | ToolCall::RunCodex { project, .. } => project,
            _ => return Ok(()),
        };
        let required = match Self::required_agent_capability(call) {
            Some(cap) => cap,
            None => return Ok(()),
        };
        let proj = self
            .resolve_project(project)
            .await
            .map_err(ProjectResolverError::into_tool_result)?;
        if !proj.is_agent() {
            return Ok(());
        }
        let client_id = proj.agent_client_id().map_err(ToolResult::err)?.to_string();
        let view = self
            .shell_clients
            .get_client_view(&client_id)
            .await
            .ok_or_else(|| ToolResult::err(format!("unknown shell client: {}", client_id)))?;
        // Owner boundary: bootstrap tokens and dev mode (auth disabled) pass.
        // Otherwise the API key username must match the agent's declared owner.
        crate::shell_client::assert_shell_client_owner(auth, &client_id, view.owner.as_deref())
            .map_err(ToolResult::err)?;
        // Capability check via the registry helper so the requirement is
        // expressed as a named capability, not a raw struct field access.
        let supported = match required {
            AgentCapability::Shell => self
                .shell_clients
                .client_supports(&client_id, "shell")
                .await
                .map_err(ToolResult::err)?,
            AgentCapability::FileRead => self
                .shell_clients
                .client_supports(&client_id, "file_read")
                .await
                .map_err(ToolResult::err)?,
            AgentCapability::FileWrite => self
                .shell_clients
                .client_supports(&client_id, "file_write")
                .await
                .map_err(ToolResult::err)?,
            AgentCapability::GitOrShell => {
                self.shell_clients
                    .client_supports(&client_id, "shell")
                    .await
                    .map_err(ToolResult::err)?
                    || self
                        .shell_clients
                        .client_supports(&client_id, "git")
                        .await
                        .map_err(ToolResult::err)?
            }
            AgentCapability::AsyncJobs => {
                self.shell_clients
                    .client_supports(&client_id, "async_jobs")
                    .await
                    .map_err(ToolResult::err)?
                    || self
                        .shell_clients
                        .client_supports(&client_id, "async_shell_jobs")
                        .await
                        .map_err(ToolResult::err)?
            }
        };
        if !supported {
            let label = match required {
                AgentCapability::Shell => "shell",
                AgentCapability::FileRead => "file_read",
                AgentCapability::FileWrite => "file_write",
                AgentCapability::GitOrShell => "shell or git",
                AgentCapability::AsyncJobs => "async shell jobs",
            };
            return Err(ToolResult::err(format!(
                "agent client {} does not support {}",
                client_id, label
            )));
        }
        Ok(())
    }

    /// Main dispatch — call from MCP handler or GPT Actions handler.
    ///
    /// This no-auth convenience defaults the caller context to `None`, which
    /// means agent-backed tools are rejected (no owner can be proven). HTTP
    /// wrappers should prefer `dispatch_with_auth` so the depot `AuthContext`
    /// is forwarded. `dispatch` is kept for internal/tests callers that only
    /// use local-executor projects.
    #[allow(dead_code)]
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        self.dispatch_with_auth(call, None).await
    }

    /// Dispatch carrying the caller's auth context. Agent-backed tools enforce
    /// the owner boundary and capability requirements through
    /// `authorize_agent_tool`; local-executor tools are unaffected. Wrappers
    /// stay thin: they only forward the depot `AuthContext` here.
    pub async fn dispatch_with_auth(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.dispatch_with_auth_transport(call, auth, sessions::SessionTransport::Api)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        let session_id = call.session_id().map(str::to_string);
        if let Some(session_id) = session_id.as_deref() {
            if !self.sessions.contains_session(session_id) {
                return unknown_session_result(session_id);
            }
            if let Some(denial) = self.sessions.guard_denial(session_id, call.tool_name()) {
                let session_start = self.sessions.record_tool_call_started(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                );
                let mut result = session_guard_denied_result(session_id, call.tool_name(), denial);
                let event_id = self.sessions.record_tool_call_finished(
                    session_start,
                    false,
                    &result.output,
                    result.error.as_deref(),
                    Some("session_guard_denied"),
                );
                add_session_telemetry_hint(&mut result, session_id, event_id);
                return result;
            }
        }
        let session_start = if session_id.is_some() {
            let resolved_project = match call.project() {
                Some(project) => self
                    .resolve_project_input(project)
                    .await
                    .ok()
                    .map(|resolved| resolved.resolved_id),
                None => None,
            };
            Some(self.sessions.record_tool_call_started_with_options(
                session_id.as_deref(),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                resolved_project,
            ))
        } else {
            None
        };
        if let Err(err) = self.authorize_agent_tool(&call, auth).await {
            let mut err = err;
            if let Some(session_id) = session_id.as_deref() {
                let event_id = self.sessions.record_tool_call_finished(
                    session_start.flatten(),
                    false,
                    &err.output,
                    err.error.as_deref(),
                    None,
                );
                add_session_telemetry_hint(&mut err, session_id, event_id);
            }
            return err;
        }
        let mut result = self.dispatch_authorized_inner(call, auth).await;
        if let Some(session_id) = session_id.as_deref() {
            let event_id = self.sessions.record_tool_call_finished(
                session_start.flatten(),
                result.success,
                &result.output,
                result.error.as_deref(),
                None,
            );
            add_session_telemetry_hint(&mut result, session_id, event_id);
        }
        result
    }

    async fn dispatch_authorized_inner(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::ListTools => ToolResult::ok(json!({ "tools": self.tool_specs() })),

            ToolCall::StartSession {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
            } => {
                let resolved = match project {
                    Some(project_input) => match self.resolve_project_input(&project_input).await {
                        Ok(resolved) => Some(resolved),
                        Err(err) => return err.into_tool_result(),
                    },
                    None => None,
                };
                let summary = self.sessions.start_session_with_guards(
                    resolved
                        .as_ref()
                        .map(|resolved| resolved.resolved_id.clone()),
                    title,
                    mode,
                    sessions::SessionGuards::effective(
                        mode,
                        sessions::SessionGuards {
                            deny_write_tools,
                            deny_shell_tools,
                        },
                    ),
                );
                ToolResult::ok(json!({
                    "success": true,
                    "session_id": summary.session_id,
                    "project": summary.project,
                    "project_input": resolved.as_ref().map(|resolved| resolved.input.clone()),
                    "resolved_project": resolved.as_ref().map(|resolved| resolved.resolved_id.clone()),
                    "title": summary.title,
                    "mode": summary.mode,
                    "guards": summary.guards,
                    "created_at": summary.created_at,
                }))
            }

            ToolCall::SessionSummary { session_id, limit } => {
                match self.sessions.summary(&session_id, limit) {
                    Some(summary) => ToolResult::ok(
                        serde_json::to_value(summary)
                            .unwrap_or_else(|_| json!({"session_id": session_id, "events": []})),
                    ),
                    None => ToolResult::err(format!("unknown session_id: {}", session_id)),
                }
            }

            ToolCall::ListProjects => self.list_projects().await,

            ToolCall::RegisterProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                overwrite,
            } => {
                self.register_project(
                    client_id,
                    id,
                    name,
                    path,
                    description,
                    allow_patch,
                    overwrite,
                    auth,
                )
                .await
            }

            ToolCall::CreateProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                template,
                git_init,
                allow_existing_empty,
                overwrite,
            } => {
                self.create_project(
                    client_id,
                    id,
                    name,
                    path,
                    description,
                    allow_patch,
                    template,
                    git_init,
                    allow_existing_empty,
                    overwrite,
                    auth,
                )
                .await
            }

            ToolCall::ListAgents => self.list_agents().await,

            ToolCall::RuntimeStatus => self.runtime_status().await,

            ToolCall::RunShell {
                project,
                command,
                session_id: _,
                timeout_secs,
                cwd,
            } => self.run_shell(project, command, timeout_secs, cwd).await,

            ToolCall::ApplyPatch {
                project,
                patch,
                session_id: _,
            } => self.apply_patch(project, patch).await,

            ToolCall::ApplyPatchChecked {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.apply_patch_checked(project, patch, deny_sensitive_paths)
                    .await
            }

            ToolCall::DeleteProjectFiles {
                project,
                paths,
                session_id: _,
            } => self.delete_project_files(project, paths).await,

            ToolCall::GitRestorePaths {
                project,
                paths,
                session_id: _,
            } => self.git_restore_paths(project, paths).await,

            ToolCall::DiscardUntracked {
                project,
                paths,
                session_id: _,
            } => self.discard_untracked(project, paths).await,

            ToolCall::ValidatePatch {
                project,
                patch,
                session_id: _,
                deny_sensitive_paths,
            } => {
                self.validate_patch(project, patch, deny_sensitive_paths)
                    .await
            }

            ToolCall::GitStatus {
                project,
                session_id: _,
            } => self.git_status(project).await,

            ToolCall::GitDiff {
                project,
                session_id: _,
                args,
            } => self.git_diff(project, args).await,

            ToolCall::GitDiffHunks {
                project,
                session_id: _,
                paths,
                max_hunks,
                max_hunk_lines,
                cached,
            } => {
                self.git_diff_hunks(project, paths, max_hunks, max_hunk_lines, cached)
                    .await
            }

            ToolCall::CargoFmt {
                project,
                session_id: _,
                cwd,
                check,
                timeout_secs,
            } => self.cargo_fmt(project, cwd, check, timeout_secs).await,

            ToolCall::CargoCheck {
                project,
                session_id: _,
                cwd,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                timeout_secs,
            } => {
                self.cargo_check(
                    project,
                    cwd,
                    all_targets,
                    all_features,
                    no_default_features,
                    features,
                    package,
                    timeout_secs,
                )
                .await
            }

            ToolCall::CargoTest {
                project,
                session_id: _,
                cwd,
                filter,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                no_run,
                timeout_secs,
            } => {
                self.cargo_test(
                    project,
                    cwd,
                    filter,
                    all_targets,
                    all_features,
                    no_default_features,
                    features,
                    package,
                    no_run,
                    timeout_secs,
                )
                .await
            }

            ToolCall::ReadFile {
                project,
                path,
                session_id: _,
                start_line,
                limit,
                with_line_numbers,
            } => {
                self.read_file(project, path, start_line, limit, with_line_numbers)
                    .await
            }

            ToolCall::RunJob {
                project,
                command,
                session_id: _,
                timeout_secs,
                cwd,
            } => self.run_job(project, command, timeout_secs, cwd).await,

            ToolCall::RunCodex {
                project,
                prompt,
                session_id: _,
                approval_mode,
                timeout_secs,
                cwd,
                extra_args,
            } => {
                self.run_codex(
                    project,
                    prompt,
                    approval_mode,
                    timeout_secs,
                    cwd,
                    extra_args,
                )
                .await
            }

            ToolCall::JobStatus { job_id } => self.job_status(job_id).await,

            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => self.job_log(job_id, offset, tail_lines).await,

            ToolCall::ListProjectFiles {
                project,
                session_id: _,
                path,
                limit,
            } => self.list_project_files(project, path, limit).await,

            ToolCall::SearchProjectText {
                project,
                pattern,
                session_id: _,
                path,
                limit,
                context_before,
                context_after,
            } => {
                self.search_project_text(
                    project,
                    pattern,
                    path,
                    limit,
                    context_before,
                    context_after,
                )
                .await
            }

            ToolCall::GitDiffSummary {
                project,
                session_id: _,
            } => self.git_diff_summary(project).await,

            ToolCall::ShowChanges {
                project,
                session_id,
                include_diff,
                max_hunks,
                max_hunk_lines,
                session_event_limit,
            } => {
                self.show_changes(
                    project,
                    session_id,
                    include_diff,
                    max_hunks,
                    max_hunk_lines,
                    session_event_limit,
                )
                .await
            }

            ToolCall::ListJobs { limit, status } => self.list_jobs(limit, status).await,

            ToolCall::JobTail { job_id, tail_lines } => self.job_tail(job_id, tail_lines).await,

            ToolCall::ReplaceInFile {
                project,
                path,
                old,
                new,
                session_id: _,
                expected_replacements,
                allow_multiple,
            } => {
                self.replace_in_file(
                    project,
                    path,
                    old,
                    new,
                    expected_replacements,
                    allow_multiple,
                )
                .await
            }

            ToolCall::ReplaceExactBlock {
                project,
                path,
                old_text,
                new_text,
                session_id: _,
                expected_old_sha256,
            } => {
                self.replace_exact_block(project, path, old_text, new_text, expected_old_sha256)
                    .await
            }

            ToolCall::InsertBeforePattern {
                project,
                path,
                pattern,
                text,
                session_id: _,
            } => {
                self.insert_around_pattern(project, path, pattern, text, "insert_before_pattern")
                    .await
            }

            ToolCall::InsertAfterPattern {
                project,
                path,
                pattern,
                text,
                session_id: _,
            } => {
                self.insert_around_pattern(project, path, pattern, text, "insert_after_pattern")
                    .await
            }

            ToolCall::WriteProjectFile {
                project,
                path,
                content,
                session_id: _,
                overwrite,
                expected_sha256,
                expected_content_prefix,
            } => {
                self.write_project_file(
                    project,
                    path,
                    content,
                    overwrite,
                    expected_sha256,
                    expected_content_prefix,
                )
                .await
            }

            ToolCall::SaveProjectArtifact {
                project,
                path,
                content_base64,
                session_id: _,
                mime_type,
                overwrite,
            } => {
                self.save_project_artifact(project, path, content_base64, mime_type, overwrite)
                    .await
            }

            ToolCall::ReadProjectArtifactMetadata {
                project,
                path,
                session_id: _,
            } => self.read_project_artifact_metadata(project, path).await,

            ToolCall::ReadProjectArtifact {
                project,
                path,
                session_id: _,
                encoding,
                offset,
                length,
                max_bytes,
            } => {
                self.read_project_artifact(project, path, encoding, offset, length, max_bytes)
                    .await
            }

            ToolCall::ReplaceLineRange {
                project,
                path,
                start_line,
                end_line,
                new_text,
                session_id: _,
                expected_old_sha256,
                expected_old_prefix,
            } => {
                self.replace_line_range(
                    project,
                    path,
                    start_line,
                    end_line,
                    new_text,
                    expected_old_sha256,
                    expected_old_prefix,
                )
                .await
            }

            ToolCall::InsertAtLine {
                project,
                path,
                line,
                text,
                session_id: _,
                expected_anchor_sha256,
                expected_anchor_prefix,
            } => {
                self.insert_at_line(
                    project,
                    path,
                    line,
                    text,
                    expected_anchor_sha256,
                    expected_anchor_prefix,
                )
                .await
            }

            ToolCall::DeleteLineRange {
                project,
                path,
                start_line,
                end_line,
                session_id: _,
                expected_old_sha256,
                expected_old_prefix,
            } => {
                self.delete_line_range(
                    project,
                    path,
                    start_line,
                    end_line,
                    expected_old_sha256,
                    expected_old_prefix,
                )
                .await
            }
        }
    }

    async fn list_projects(&self) -> ToolResult {
        let mut list: Vec<Value> = Vec::new();
        for client in self.shell_clients.list_clients().await {
            // Sanitized shell-profiles summary for this agent (carried inside
            // the registration policy). Used to resolve which profile a project
            // actually uses and whether that profile is configured. `None` for
            // older agents that did not report one.
            let shell_profiles = client
                .policy
                .as_ref()
                .and_then(|p| p.shell_profiles.as_ref());
            for project in client.projects.iter().filter(|p| !p.disabled) {
                let (resolved_shell_profile, shell_profile_status) =
                    resolve_project_shell_profile(project.shell_profile.as_deref(), shell_profiles);
                list.push(json!({
                    "id": Self::agent_project_runtime_id(&client.client_id, &project.id),
                    "agent_project_id": project.id,
                    "name": project.name,
                    "path": project.path,
                    "executor": "agent",
                    "client_id": client.client_id,
                    "allow_patch": project.allow_patch,
                    "source": "agent_registered",
                    "agent_status": client.status,
                    "connected": client.connected,
                    "last_seen": client.last_seen,
                    "shell_profile": project.shell_profile,
                    "resolved_shell_profile": resolved_shell_profile,
                    "shell_profile_status": shell_profile_status,
                }));
            }
        }
        list.sort_by(|a, b| {
            a["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["id"].as_str().unwrap_or_default())
        });
        ToolResult::ok(Value::Array(list))
    }

    async fn list_agents(&self) -> ToolResult {
        let clients = self.shell_clients.list_clients().await;
        let agents: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "hostname": c.hostname,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "pending_requests": c.pending_requests,
                    "capabilities": c.capabilities,
                    "projects": c.projects,
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                })
            })
            .collect();
        ToolResult::ok(json!({ "agents": agents }))
    }

    /// Build the runtime observability summary. Read-only; never exposes
    /// tokens, api keys, full env, complete project path lists, or
    /// stdout/stderr. Returns a structured JSON object with service metadata,
    /// project config status, agent client summaries, and job counts.
    async fn runtime_status(&self) -> ToolResult {
        // -- projects summary -------------------------------------------------
        let (projects_configured, projects_count, projects_load_error) =
            match self.projects.config.as_ref() {
                Some(cfg) => (true, cfg.projects.len(), None),
                None => (
                    false,
                    0,
                    self.projects
                        .load_error
                        .clone()
                        .or_else(|| Some("Projects not configured".to_string())),
                ),
            };
        let projects = json!({
            "configured": projects_configured,
            "count": projects_count,
            "config_path": self.projects.config_path,
            "load_error": projects_load_error,
        });

        // -- agents summary ---------------------------------------------------
        // Build a trimmed client list so the summary never leaks per-request
        // state. Only carry fields useful for observability. `last_seen` is a
        // unix timestamp (seconds) of the most recent heartbeat/result; the
        // console uses it to render how stale an agent is and to make a
        // websocket agent flipping `online` -> `stale` visually obvious.
        let clients = self.shell_clients.list_clients().await;
        let agent_count = clients.len();
        let online_count = clients.iter().filter(|c| c.connected).count();
        // `stale_count` = registered agents whose `last_seen` is older than the
        // online window (status == "stale"). Truly offline agents are removed
        // from the registry on disconnect, so they never appear here; the
        // legacy `offline_count` field is retained (it mirrors `stale_count`
        // for the registered set) for backward compatibility with existing
        // callers/tests.
        let stale_count = agent_count.saturating_sub(online_count);
        let offline_count = stale_count;
        let clients_summary: Vec<Value> = clients
            .iter()
            .map(|c| {
                json!({
                    "client_id": c.client_id,
                    "agent_instance_id": c.agent_instance_id,
                    "display_name": c.display_name,
                    "owner": c.owner,
                    "status": c.status,
                    "connected": c.connected,
                    "agent_protocol_version": c.agent_protocol_version,
                    "transport": c.transport,
                    "last_seen": c.last_seen,
                    "pending_requests": c.pending_requests,
                    "capabilities": c.capabilities,
                    "projects_count": c.projects.len(),
                    "policy": sanitized_policy_summary(c.policy.as_ref()),
                    "shell_profiles": sanitized_shell_profiles_summary(
                        c.policy.as_ref().and_then(|p| p.shell_profiles.as_ref())
                    ),
                })
            })
            .collect();
        let agents = json!({
            "count": agent_count,
            "online_count": online_count,
            "stale_count": stale_count,
            "offline_count": offline_count,
            "clients": clients_summary,
        });

        // -- jobs summary -----------------------------------------------------
        // Agent-known jobs come from the registry; local jobs come from the
        // in-memory map. Active = running/queued/agent_queued/stop_requested.
        let agent_jobs = self.shell_clients.list_jobs(None).await;
        let agent_known_count = agent_jobs.len();
        let local_job_dirs: Vec<PathBuf> = {
            let local_jobs_map = self.local_jobs.lock().await;
            local_jobs_map
                .values()
                .map(|record| record.dir.clone())
                .collect()
        };
        let local_known_count = local_job_dirs.len();
        // Avoid double-counting: agent jobs are tracked separately from local
        // jobs (local jobs are only in the in-memory map; agent jobs are only
        // in the registry). Count active across both.
        let agent_active = agent_jobs
            .iter()
            .filter(|j| ACTIVE_JOB_STATUSES.contains(&j.status.as_str()))
            .count();
        let mut local_active = 0usize;
        for dir in local_job_dirs {
            if let Some(status) = std::fs::read_to_string(dir.join("status"))
                .ok()
                .map(|s| s.trim().to_string())
            {
                let normalized = normalize_local_status(&status);
                if ACTIVE_JOB_STATUSES.contains(&normalized.as_str()) {
                    local_active += 1;
                }
            }
        }
        let active_count = agent_active + local_active;
        let jobs = json!({
            "agent_known_count": agent_known_count,
            "local_known_count": local_known_count,
            "active_count": active_count,
        });

        // -- tools summary ----------------------------------------------------
        let specs = self.tool_specs();
        let tools_count = specs.len();
        let tools_names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
        let tools = json!({
            "count": tools_count,
            "names": tools_names,
        });

        let quic = self.runtime_info.quic.as_ref().map(|status| {
            let status = status.lock().expect("quic runtime status mutex poisoned");
            json!({
                "enabled": status.enabled,
                "listen": status.listen,
                "alpn": status.alpn,
                "listener_started": status.listener_started,
                "last_error": status.last_error,
            })
        });

        let mut output = json!({
            "service": "webcodex",
            "version": env!("CARGO_PKG_VERSION"),
            "build": crate::build_info::runtime_build_info(),
            "server_time": chrono::Utc::now().timestamp(),
            "pid": std::process::id(),
            "auth_enabled": self.runtime_info.auth_enabled,
            "configured_public_url": self.runtime_info.configured_public_url,
            "projects": projects,
            "agents": agents,
            "jobs": jobs,
            "tools": tools,
        });
        if let Some(quic) = quic {
            output["quic"] = quic;
        }
        ToolResult::ok(output)
    }
}

/// Build the sanitized policy summary JSON exposed in `runtime_status` and
/// `listAgents`. Only the safe fields are carried: `allow_raw_shell`,
/// `allow_cwd_anywhere`, `allowed_roots`, `max_timeout_secs`,
/// `max_output_bytes`. The agent token, shell env values, init_script
/// contents, and full agent.toml contents are NEVER included. Older agents
/// that registered without a policy produce `Value::Null` so the field is
/// present-but-null for clients that expect it.
fn sanitized_policy_summary(policy: Option<&crate::shell_protocol::AgentPolicySummary>) -> Value {
    match policy {
        Some(p) => json!({
            "allow_raw_shell": p.allow_raw_shell,
            "allow_cwd_anywhere": p.allow_cwd_anywhere,
            "allowed_roots": p.allowed_roots,
            "max_timeout_secs": p.max_timeout_secs,
            "max_output_bytes": p.max_output_bytes,
        }),
        None => Value::Null,
    }
}

/// Build the sanitized shell-profiles summary JSON exposed in
/// `runtime_status`, `listAgents`, and `listProjects`. Only safe metadata is
/// carried: default profile name, configured count, prepared-cache count, and
/// per-profile name / has_init_script (boolean) / env_keys_count / program /
/// args_count. NEVER includes init_script bodies, env values, tokens, or the
/// full env snapshot. Older agents that did not report a summary produce
/// `Value::Null`.
fn sanitized_shell_profiles_summary(
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> Value {
    match summary {
        Some(s) => {
            let profiles: Vec<Value> = s
                .profiles
                .iter()
                .map(|p| {
                    json!({
                        "name": p.name,
                        "has_init_script": p.has_init_script,
                        "env_keys_count": p.env_keys_count,
                        "program": p.program,
                        "args_count": p.args_count,
                    })
                })
                .collect();
            json!({
                "default_profile": s.default_profile,
                "configured_count": s.configured_count,
                "prepared_cache_count": s.prepared_cache_count,
                "profiles": profiles,
            })
        }
        None => Value::Null,
    }
}

/// Resolve which shell profile a project uses and whether it is configured.
/// Returns `(resolved_name, status)` where:
/// - `resolved_name` = `project_shell_profile` (if set) else the agent's
///   `default_profile` (if any) else `None`.
/// - `status` = `"configured"` if the resolved name exists in the agent's
///   configured profiles; `"missing"` if a name resolved but is not
///   configured; `"not_configured"` if no profile resolves at all; and
///   `"unknown"` if the agent did not report a shell-profiles summary so the
///   configured set cannot be checked.
fn resolve_project_shell_profile(
    project_shell_profile: Option<&str>,
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> (Option<String>, &'static str) {
    let resolved = project_shell_profile
        .map(str::to_string)
        .or_else(|| summary.and_then(|s| s.default_profile.clone()));
    match resolved {
        None => (None, "not_configured"),
        Some(name) => match summary {
            None => (Some(name), "unknown"),
            Some(s) => {
                if s.profiles.iter().any(|p| p.name == name) {
                    (Some(name), "configured")
                } else {
                    (Some(name), "missing")
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::cargo::*;
    use super::codex::*;
    use super::files::*;
    use super::git::*;
    use super::helpers::*;
    use super::patch::*;
    use super::types::*;
    use super::*;
    use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
    use crate::shell_client::ShellClientRegistry;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        let projects = Arc::new(ProjectsState::failed(
            "projects not configured for test".to_string(),
            "test".to_string(),
        ));
        let shell_clients = Arc::new(ShellClientRegistry::default());
        ToolRuntime::new(
            projects,
            shell_clients,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    // =========================================================================
    // Phase 1.1: ToolCall::from_tool_name
    // =========================================================================

    #[test]
    fn from_tool_name_parses_unit_tools_without_arguments() {
        for name in [
            "list_tools",
            "list_projects",
            "list_agents",
            "runtime_status",
        ] {
            let call =
                ToolCall::from_tool_name(name, Value::Null).unwrap_or_else(|e| panic!("{}", e));
            assert!(
                matches!(
                    call,
                    ToolCall::ListTools
                        | ToolCall::ListProjects
                        | ToolCall::ListAgents
                        | ToolCall::RuntimeStatus
                ),
                "unit tool {} should parse",
                name
            );
        }
    }

    #[test]
    fn from_tool_name_parses_unit_tools_with_empty_object() {
        let call = ToolCall::from_tool_name("list_tools", json!({})).unwrap();
        assert!(matches!(call, ToolCall::ListTools));
    }

    #[test]
    fn from_tool_name_parses_run_shell_with_required_fields() {
        let call = ToolCall::from_tool_name(
            "run_shell",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .unwrap();
        match call {
            ToolCall::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
                ..
            } => {
                assert_eq!(project, "demo");
                assert_eq!(command, "echo hi");
                assert_eq!(timeout_secs, None);
                assert_eq!(cwd, None);
            }
            other => panic!("expected RunShell, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_run_shell_with_optional_fields() {
        let call = ToolCall::from_tool_name(
            "run_shell",
            json!({"project": "demo", "command": "ls", "timeout_secs": 5, "cwd": "sub"}),
        )
        .unwrap();
        match call {
            ToolCall::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
                ..
            } => {
                assert_eq!(project, "demo");
                assert_eq!(command, "ls");
                assert_eq!(timeout_secs, Some(5));
                assert_eq!(cwd, Some("sub".to_string()));
            }
            other => panic!("expected RunShell, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_run_codex_with_all_fields() {
        let call = ToolCall::from_tool_name(
            "run_codex",
            json!({
                "project": "demo",
                "prompt": "fix tests",
                "approval_mode": "suggest",
                "timeout_secs": 120,
                "cwd": "src",
                "extra_args": ["--verbose"]
            }),
        )
        .unwrap();
        match call {
            ToolCall::RunCodex {
                project,
                prompt,
                approval_mode,
                timeout_secs,
                cwd,
                extra_args,
                ..
            } => {
                assert_eq!(project, "demo");
                assert_eq!(prompt, "fix tests");
                assert_eq!(approval_mode.as_deref(), Some("suggest"));
                assert_eq!(timeout_secs, Some(120));
                assert_eq!(cwd.as_deref(), Some("src"));
                assert_eq!(extra_args.unwrap(), vec!["--verbose".to_string()]);
            }
            other => panic!("expected RunCodex, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_job_status_and_job_log() {
        let call = ToolCall::from_tool_name("job_status", json!({"job_id": "abc"})).unwrap();
        assert!(matches!(call, ToolCall::JobStatus { ref job_id } if job_id == "abc"));

        let call =
            ToolCall::from_tool_name("job_log", json!({"job_id": "abc", "offset": 10})).unwrap();
        match call {
            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => {
                assert_eq!(job_id, "abc");
                assert_eq!(offset, Some(10));
                assert_eq!(tail_lines, None);
            }
            other => panic!("expected JobLog, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_parses_read_file_and_git_tools() {
        let call =
            ToolCall::from_tool_name("read_file", json!({"project": "demo", "path": "README.md"}))
                .unwrap();
        assert!(matches!(call, ToolCall::ReadFile { .. }));

        let call = ToolCall::from_tool_name(
            "read_file",
            json!({
                "project": "demo",
                "path": "src/main.rs",
                "start_line": 10,
                "limit": 3,
                "with_line_numbers": true
            }),
        )
        .unwrap();
        match call {
            ToolCall::ReadFile {
                project,
                path,
                start_line,
                limit,
                with_line_numbers,
                ..
            } => {
                assert_eq!(project, "demo");
                assert_eq!(path, "src/main.rs");
                assert_eq!(start_line, Some(10));
                assert_eq!(limit, Some(3));
                assert_eq!(with_line_numbers, Some(true));
            }
            other => panic!("expected ReadFile, got {:?}", other),
        }

        let call = ToolCall::from_tool_name("git_status", json!({"project": "demo"})).unwrap();
        assert!(matches!(call, ToolCall::GitStatus { .. }));

        let call =
            ToolCall::from_tool_name("git_diff", json!({"project": "demo", "args": ["--stat"]}))
                .unwrap();
        assert!(matches!(call, ToolCall::GitDiff { .. }));

        let call =
            ToolCall::from_tool_name("apply_patch", json!({"project": "demo", "patch": "diff"}))
                .unwrap();
        assert!(matches!(call, ToolCall::ApplyPatch { .. }));

        let call =
            ToolCall::from_tool_name("run_job", json!({"project": "demo", "command": "make"}))
                .unwrap();
        assert!(matches!(call, ToolCall::RunJob { .. }));
    }

    #[test]
    fn from_tool_name_rejects_unknown_tool_name() {
        let err = ToolCall::from_tool_name("not_a_tool", Value::Null).unwrap_err();
        assert!(err.contains("not_a_tool"));
    }

    #[test]
    fn from_tool_name_rejects_missing_required_field() {
        let err = ToolCall::from_tool_name("run_shell", json!({"command": "echo"})).unwrap_err();
        assert!(
            err.contains("project"),
            "error should mention missing field: {}",
            err
        );

        let err = ToolCall::from_tool_name("job_status", json!({})).unwrap_err();
        assert!(err.contains("job_id"));
    }

    #[test]
    fn from_tool_name_rejects_wrong_field_type() {
        let err = ToolCall::from_tool_name("run_shell", json!({"project": 123, "command": "echo"}))
            .unwrap_err();
        assert!(!err.is_empty());

        let err = ToolCall::from_tool_name("run_codex", json!({"project": "demo", "prompt": 42}))
            .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn from_tool_name_rejects_unknown_variant_field() {
        // extra_args must be an array, not a string.
        let err = ToolCall::from_tool_name(
            "run_codex",
            json!({"project": "demo", "prompt": "x", "extra_args": "--verbose"}),
        )
        .unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn from_tool_name_error_includes_tool_name() {
        let err = ToolCall::from_tool_name("run_shell", json!({})).unwrap_err();
        assert!(err.contains("run_shell"));
    }

    // =========================================================================
    // Phase 1.2: tool_specs() shape
    // =========================================================================

    #[test]
    fn tool_specs_names_are_unique() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let mut names = specs.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        names.sort();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "tool names must be unique");
    }

    #[test]
    fn tool_specs_names_are_snake_case() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            assert!(
                !spec.name.contains('-'),
                "tool name '{}' should use snake_case (no hyphens)",
                spec.name
            );
            assert!(
                spec.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "tool name '{}' should be snake_case",
                spec.name
            );
        }
    }

    #[test]
    fn tool_specs_input_schemas_are_objects() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            let schema = &spec.input_schema;
            assert_eq!(
                schema["type"].as_str(),
                Some("object"),
                "tool '{}' input schema must be type object",
                spec.name
            );
            assert!(
                schema["properties"].is_object(),
                "tool '{}' input schema must have properties object",
                spec.name
            );
            assert!(
                schema["required"].is_array(),
                "tool '{}' input schema must have required array",
                spec.name
            );
        }
    }

    #[test]
    fn tool_specs_output_schemas_are_objects() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            let schema = &spec.output_schema;
            assert_eq!(
                schema["type"].as_str(),
                Some("object"),
                "tool '{}' output schema must be type object",
                spec.name
            );
            assert!(
                schema["properties"].is_object(),
                "tool '{}' output schema must have properties object",
                spec.name
            );
            assert!(
                schema["required"]
                    .as_array()
                    .is_some_and(|required| required.iter().any(|v| v == "success")),
                "tool '{}' output schema must require success",
                spec.name
            );
        }
    }

    #[test]
    fn key_tool_output_schemas_include_expected_fields() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let has_output_field = |name: &str, field: &str| {
            let spec = spec_named(&specs, name);
            spec.output_schema["properties"]["output"]["properties"]
                .as_object()
                .is_some_and(|props| props.contains_key(field))
        };

        for field in [
            "duration_ms",
            "exit_code",
            "stdout",
            "stderr",
            "command_started",
            "command_completed",
            "command_ok",
            "failure_kind",
            "tool_failure",
        ] {
            assert!(
                has_output_field("run_shell", field),
                "run_shell missing {field}"
            );
        }
        for field in [
            "content",
            "start_line",
            "limit",
            "total_lines",
            "numbered_text",
            "lines",
        ] {
            assert!(
                has_output_field("read_file", field),
                "read_file missing {field}"
            );
        }
        for field in [
            "matches",
            "count",
            "truncated",
            "context_before",
            "context_after",
        ] {
            assert!(
                has_output_field("search_project_text", field),
                "search_project_text missing {field}"
            );
        }
        for field in ["job_id", "kind", "status", "project"] {
            assert!(
                has_output_field("run_job", field),
                "run_job missing {field}"
            );
        }
        for field in [
            "job_id",
            "status",
            "exit_code",
            "started_at",
            "ended_at",
            "error",
        ] {
            assert!(
                has_output_field("job_status", field),
                "job_status missing {field}"
            );
        }
        for field in [
            "job_id",
            "stdout",
            "stderr",
            "offset",
            "next_offset",
            "tail_lines",
        ] {
            assert!(
                has_output_field("job_log", field),
                "job_log missing {field}"
            );
        }
        for field in [
            "service",
            "version",
            "build",
            "auth_enabled",
            "configured_public_url",
            "agents",
            "projects",
            "jobs",
            "tools",
            "quic",
        ] {
            assert!(
                has_output_field("runtime_status", field),
                "runtime_status missing {field}"
            );
        }
    }

    #[test]
    fn tool_specs_required_fields_match_deserialization() {
        // For every tool spec, building arguments with only the required
        // fields must deserialize successfully, and omitting any required
        // field must fail.
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            let required: Vec<String> = spec.input_schema["required"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect();

            // Build a minimal valid args object using a placeholder for each
            // required field based on its declared type.
            let mut minimal = serde_json::Map::new();
            let properties = spec.input_schema["properties"].as_object().unwrap();
            for field in &required {
                let prop = &properties[field.as_str()];
                let kind = prop["type"].as_str().unwrap_or("string");
                let placeholder = match kind {
                    "integer" => json!(1),
                    "array" => json!([]),
                    "boolean" => json!(true),
                    _ => json!("value"),
                };
                minimal.insert(field.clone(), placeholder);
            }
            let args = Value::Object(minimal);
            ToolCall::from_tool_name(&spec.name, args)
                .unwrap_or_else(|e| panic!("tool '{}' minimal args failed: {}", spec.name, e));

            // Omitting each required field should fail.
            for field in &required {
                let mut partial = serde_json::Map::new();
                for f in &required {
                    if f != field {
                        let prop = &properties[f.as_str()];
                        let kind = prop["type"].as_str().unwrap_or("string");
                        let placeholder = match kind {
                            "integer" => json!(1),
                            "array" => json!([]),
                            "boolean" => json!(true),
                            _ => json!("value"),
                        };
                        partial.insert(f.clone(), placeholder);
                    }
                }
                let err = ToolCall::from_tool_name(&spec.name, Value::Object(partial))
                    .err()
                    .unwrap_or_else(|| {
                        panic!(
                            "tool '{}' should reject missing required field '{}'",
                            spec.name, field
                        )
                    });
                assert!(
                    err.contains(field),
                    "tool '{}' error for missing '{}' should mention field: {}",
                    spec.name,
                    field,
                    err
                );
            }
        }
    }

    #[test]
    fn tool_specs_optional_fields_are_not_required() {
        // Optional fields (e.g. timeout_secs, cwd) must not appear in required.
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let run_shell = specs.iter().find(|s| s.name == "run_shell").unwrap();
        let required: Vec<String> = run_shell.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"command".to_string()));
        assert!(!required.contains(&"timeout_secs".to_string()));
        assert!(!required.contains(&"cwd".to_string()));

        let read_file = specs.iter().find(|s| s.name == "read_file").unwrap();
        let required = required_fields(read_file);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"path".to_string()));
        assert!(!required.contains(&"with_line_numbers".to_string()));

        let search_project_text = specs
            .iter()
            .find(|s| s.name == "search_project_text")
            .unwrap();
        let required = required_fields(search_project_text);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"pattern".to_string()));
        assert!(!required.contains(&"context_before".to_string()));
        assert!(!required.contains(&"context_after".to_string()));
    }

    #[test]
    fn tool_specs_covers_expected_tool_set() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for expected in [
            "list_tools",
            "list_projects",
            "list_agents",
            "runtime_status",
            "run_shell",
            "run_job",
            "run_codex",
            "job_status",
            "job_log",
            "read_file",
            "git_status",
            "git_diff",
            "git_diff_summary",
            "git_diff_hunks",
            "show_changes",
            "apply_patch",
            "apply_patch_checked",
            "validate_patch",
            "delete_project_files",
            "git_restore_paths",
            "discard_untracked",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "expected tool '{}' in specs: {:?}",
                expected,
                names
            );
        }
    }

    #[test]
    fn tool_specs_descriptions_fit_gpt_action_limit() {
        let runtime = test_runtime();
        for spec in runtime.tool_specs() {
            assert!(
                spec.description.chars().count() <= 300,
                "{} description is too long: {} chars",
                spec.name,
                spec.description.chars().count()
            );
        }
    }

    // =========================================================================
    // Phase 2: schema coverage for the generic callRuntimeTool tool set
    // =========================================================================

    /// Helper: fetch a ToolSpec by name from the runtime.
    fn spec_named<'a>(specs: &'a [ToolSpec], name: &str) -> &'a ToolSpec {
        specs
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("tool '{}' missing from specs", name))
    }

    /// Helper: the `required` field of a tool's input schema, as Strings.
    fn required_fields(spec: &ToolSpec) -> Vec<String> {
        spec.input_schema["required"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|v| v.as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn tool_specs_apply_patch_checked_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "apply_patch_checked");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"patch".to_string()));
        assert!(!required.contains(&"deny_sensitive_paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_validate_patch_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "validate_patch");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"patch".to_string()));
        assert!(!required.contains(&"deny_sensitive_paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_git_diff_summary_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "git_diff_summary");
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_show_changes_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "show_changes");
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        let props = spec.input_schema["properties"].as_object().unwrap();
        for field in [
            "project",
            "session_id",
            "include_diff",
            "max_hunks",
            "max_hunk_lines",
            "session_event_limit",
        ] {
            assert!(props.contains_key(field), "missing {}", field);
        }
        let output_props = spec.output_schema["properties"]["output"]["properties"]
            .as_object()
            .unwrap();
        for field in [
            "project",
            "branch",
            "head",
            "clean",
            "counts",
            "files",
            "diff_stat",
            "untracked_previews",
            "untracked_previews_truncated",
            "warnings",
            "suggested_next_actions",
            "session",
        ] {
            assert!(output_props.contains_key(field), "missing {}", field);
        }
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn cargo_runtime_tools_are_known_and_parse() {
        for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
            assert!(KNOWN_TOOL_NAMES.contains(&name), "{name} missing");
        }
        assert!(matches!(
            ToolCall::from_tool_name(
                "cargo_fmt",
                json!({"project":"agent:oe:webcodex","check":true,"cwd":"crates/app"})
            )
            .unwrap(),
            ToolCall::CargoFmt {
                check: Some(true),
                ..
            }
        ));
        assert!(matches!(
            ToolCall::from_tool_name("cargo_check", json!({"project":"agent:oe:webcodex"}))
                .unwrap(),
            ToolCall::CargoCheck {
                all_targets: None,
                ..
            }
        ));
        assert!(matches!(
            ToolCall::from_tool_name(
                "cargo_test",
                json!({"project":"agent:oe:webcodex","filter":"tool_runtime"})
            )
            .unwrap(),
            ToolCall::CargoTest { filter: Some(filter), .. } if filter == "tool_runtime"
        ));
    }

    #[test]
    fn tool_specs_cargo_tools_schema_and_output() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
            let spec = spec_named(&specs, name);
            let required = required_fields(spec);
            assert_eq!(required, vec!["project".to_string()]);
            assert!(spec.input_schema["properties"]
                .as_object()
                .unwrap()
                .contains_key("cwd"));
            for field in [
                "exit_code",
                "duration_ms",
                "stdout_tail",
                "stderr_tail",
                "passed",
            ] {
                assert!(
                    spec.output_schema["properties"]["output"]["properties"]
                        .as_object()
                        .unwrap()
                        .contains_key(field),
                    "{} missing output field {}",
                    name,
                    field
                );
            }
        }
    }

    #[test]
    fn cargo_command_builders_use_expected_defaults_and_escaping() {
        assert_eq!(cargo_fmt_command(true), "cargo fmt -- --check");
        assert_eq!(
            cargo_check_command(None, None, None, None, None).unwrap(),
            "cargo check --all-targets"
        );
        assert_eq!(
            cargo_test_command(
                Some("tool_runtime".to_string()),
                None,
                None,
                None,
                None,
                None,
                None
            )
            .unwrap(),
            "cargo test 'tool_runtime'"
        );
        assert!(cargo_check_command(None, None, None, Some("feat\0x".to_string()), None).is_err());
    }

    #[tokio::test]
    async fn cargo_tools_reject_unsafe_cwd_before_project_dispatch() {
        let runtime = test_runtime();
        let fmt = runtime
            .cargo_fmt(
                "agent:oe:webcodex".to_string(),
                Some("../outside".to_string()),
                None,
                None,
            )
            .await;
        assert!(!fmt.success);
        assert!(fmt.error.unwrap().contains("parent traversal"));

        let check = runtime
            .cargo_check(
                "agent:oe:webcodex".to_string(),
                Some("/tmp".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!check.success);
        assert!(check.error.unwrap().contains("project-relative"));

        let test = runtime
            .cargo_test(
                "agent:oe:webcodex".to_string(),
                Some("src\0bad".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!test.success);
        assert!(test.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn cargo_check_failure_includes_stderr_tail_or_guidance() {
        let runtime = runtime_with_agent_project("cargo-checker");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "cargo-checker", None, caps).await;
        let project = agent_test_project_id("cargo-checker");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .cargo_check(project, None, None, None, None, None, None, Some(60))
                .await
        });
        let req = next_patch_agent_request(&runtime, "cargo-checker")
            .await
            .expect("cargo_check should enqueue a cargo command");
        assert_eq!(req.command, "cargo check --all-targets");
        complete_patch_agent_request(
            &runtime,
            "cargo-checker",
            &req.request_id,
            101,
            "",
            "error: simulated compile failure\n",
        )
        .await;
        let result = task.await.unwrap();
        assert!(!result.success);
        let error = result.error.as_deref().unwrap_or("");
        assert!(error.contains("cargo command failed"));
        assert!(error.contains("command was started"));
        assert!(error.contains("stdout_tail/stderr_tail"));
        assert!(error.contains("narrower cargo filter"));
        assert_eq!(result.output["passed"], false);
        assert!(result.output["stderr_tail"]
            .as_str()
            .unwrap_or("")
            .contains("simulated compile failure"));
    }

    #[tokio::test]
    async fn cargo_test_failure_includes_stderr_tail_or_guidance() {
        let runtime = runtime_with_agent_project("cargo-tester");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "cargo-tester", None, caps).await;
        let project = agent_test_project_id("cargo-tester");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .cargo_test(
                    project,
                    None,
                    Some("failing".to_string()),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(60),
                )
                .await
        });
        let req = next_patch_agent_request(&runtime, "cargo-tester")
            .await
            .expect("cargo_test should enqueue a cargo command");
        assert_eq!(req.command, "cargo test 'failing'");
        complete_patch_agent_request(
            &runtime,
            "cargo-tester",
            &req.request_id,
            101,
            "test result: FAILED. 0 passed; 1 failed\ncargo-test-stdout-tail\n",
            "",
        )
        .await;
        let result = task.await.unwrap();
        assert!(!result.success);
        let error = result.error.as_deref().unwrap_or("");
        assert!(error.contains("cargo command failed"));
        assert!(error.contains("command was started"));
        assert!(error.contains("stdout_tail/stderr_tail"));
        assert_eq!(result.output["passed"], false);
        assert!(result.output["stdout_tail"]
            .as_str()
            .unwrap_or("")
            .contains("cargo-test-stdout-tail"));
    }

    #[tokio::test]
    async fn cargo_fmt_failure_includes_stderr_tail_or_guidance() {
        let runtime = runtime_with_agent_project("cargo-formatter");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "cargo-formatter", None, caps).await;
        let project = agent_test_project_id("cargo-formatter");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .cargo_fmt(project, None, Some(true), Some(60))
                .await
        });
        let req = next_patch_agent_request(&runtime, "cargo-formatter")
            .await
            .expect("cargo_fmt should enqueue a cargo command");
        assert_eq!(req.command, "cargo fmt -- --check");
        complete_patch_agent_request(
            &runtime,
            "cargo-formatter",
            &req.request_id,
            1,
            "Diff in src/lib.rs\n",
            "",
        )
        .await;
        let result = task.await.unwrap();
        assert!(!result.success);
        let error = result.error.as_deref().unwrap_or("");
        assert!(error.contains("cargo command failed"));
        assert!(error.contains("command was started"));
        assert!(error.contains("stdout_tail/stderr_tail"));
        assert_eq!(result.output["passed"], false);
        assert!(result.output["stdout_tail"].is_string());
        assert!(result.output["stderr_tail"].is_string());
    }

    #[test]
    fn git_diff_hunks_tool_is_known_and_schema_is_bounded() {
        assert!(KNOWN_TOOL_NAMES.contains(&"git_diff_hunks"));
        let call = ToolCall::from_tool_name(
            "git_diff_hunks",
            json!({
                "project":"agent:oe:webcodex",
                "paths":["src/runtime_http.rs"],
                "max_hunks":20,
                "max_hunk_lines":120,
                "cached":true
            }),
        )
        .unwrap();
        assert!(matches!(
            call,
            ToolCall::GitDiffHunks { project, cached: Some(true), .. }
                if project == "agent:oe:webcodex"
        ));

        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "git_diff_hunks");
        let props = spec.input_schema["properties"].as_object().unwrap();
        for field in ["project", "paths", "max_hunks", "max_hunk_lines", "cached"] {
            assert!(props.contains_key(field), "missing {}", field);
        }
        let output_props = spec.output_schema["properties"]["output"]["properties"]
            .as_object()
            .unwrap();
        for field in ["files", "hunk_count", "truncated", "exit_code", "stderr"] {
            assert!(output_props.contains_key(field), "missing {}", field);
        }
    }

    #[test]
    fn show_changes_tool_is_known_and_parses() {
        assert!(KNOWN_TOOL_NAMES.contains(&"show_changes"));
        let call = ToolCall::from_tool_name(
            "show_changes",
            json!({
                "project": "agent:oe:webcodex",
                "include_diff": true,
                "max_hunks": 4,
                "max_hunk_lines": 12,
                "session_id": "wc_sess_1234",
                "session_event_limit": 8
            }),
        )
        .unwrap();
        assert!(matches!(
            call,
            ToolCall::ShowChanges {
                project,
                session_id: Some(session_id),
                include_diff: Some(true),
                max_hunks: Some(4),
                max_hunk_lines: Some(12),
                session_event_limit: Some(8)
            } if project == "agent:oe:webcodex" && session_id == "wc_sess_1234"
        ));
    }

    #[test]
    fn git_diff_hunks_parser_handles_modified_empty_and_limits() {
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,2 +1,3 @@ fn demo()
 line one
-old
+new
+added
";
        let (files, hunk_count, truncated) = parse_git_diff_hunks(diff, 10, 20);
        assert!(!truncated);
        assert_eq!(hunk_count, 1);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["path"], "src/lib.rs");
        assert_eq!(files[0]["status"], "modified");
        assert_eq!(files[0]["hunks"][0]["old_start"], 1);
        assert!(files[0]["hunks"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("+new"));

        let (files, hunk_count, truncated) = parse_git_diff_hunks("", 10, 20);
        assert!(files.is_empty());
        assert_eq!(hunk_count, 0);
        assert!(!truncated);

        let (_files, hunk_count, truncated) = parse_git_diff_hunks(diff, 0, 20);
        assert_eq!(hunk_count, 0);
        assert!(truncated);

        let (files, _hunk_count, truncated) = parse_git_diff_hunks(diff, 10, 2);
        assert!(truncated);
        assert_eq!(files[0]["hunks"][0]["truncated"], true);
    }

    #[test]
    fn show_changes_command_is_read_only() {
        let without_diff = show_changes_command(false);
        let with_diff = show_changes_command(true);
        for cmd in [without_diff, with_diff] {
            assert!(cmd.contains("git status --porcelain=v1 -b"));
            assert!(cmd.contains("git log -1"));
            assert!(cmd.contains("git diff --stat"));
            for forbidden in [
                " clean",
                " restore",
                " add",
                " commit",
                " reset",
                " checkout",
                " push",
                " stash",
                " merge",
                " rebase",
                " rm ",
            ] {
                assert!(
                    !cmd.contains(forbidden),
                    "show_changes command must not contain '{}': {}",
                    forbidden,
                    cmd
                );
            }
        }
    }

    #[test]
    fn show_changes_clean_worktree() {
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main...origin/main",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix: route anchor edit file ops through agent dispatch",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
        assert_eq!(output["clean"], true);
        assert_eq!(output["branch"], "main");
        assert_eq!(output["head"]["short"], "b47e4fb");
        assert_eq!(output["counts"]["modified"], 0);
        assert!(output["files"].as_array().unwrap().is_empty());
        assert!(output.get("hunks").is_none());
        assert!(output["session"].is_null());
        assert_eq!(output["suggested_next_actions"][0], "no changes detected");
    }

    #[test]
    fn show_changes_without_session_id_keeps_existing_behavior() {
        let mut output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n M src/lib.rs",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            " src/lib.rs | 2 +-",
            None,
            20,
            80,
            Some(0),
            "",
        );
        apply_show_changes_session(&mut output, None, None);
        assert_eq!(output["clean"], false);
        assert_eq!(output["counts"]["modified"], 1);
        assert!(output["session"].is_null());
        assert!(output["suggested_next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == "review diff"));
    }

    #[test]
    fn show_changes_with_session_id_includes_session_summary() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(
            Some("agent:oe:webcodex".to_string()),
            Some("finish task".to_string()),
        );
        let write_args = json!({"project": "agent:oe:webcodex", "path": "src/foo.rs"});
        let write = runtime.sessions.record_tool_call_started(
            Some(&session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "replace_line_range",
            &write_args,
        );
        runtime
            .sessions
            .record_tool_call_finished(write, true, &json!({}), None, None);
        let shell_args = json!({"project": "agent:oe:webcodex", "command": "cargo test"});
        let shell = runtime.sessions.record_tool_call_started(
            Some(&session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "run_shell",
            &shell_args,
        );
        runtime
            .sessions
            .record_tool_call_finished(shell, true, &json!({}), None, None);

        let mut output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n M src/foo.rs",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            " src/foo.rs | 2 +-",
            None,
            20,
            80,
            Some(0),
            "",
        );
        let summary = runtime.sessions.summary(&session.session_id, Some(30));
        apply_show_changes_session(&mut output, Some(&session.session_id), summary);

        assert_eq!(output["session"]["found"], true);
        assert_eq!(output["session"]["session_id"], session.session_id);
        assert_eq!(output["session"]["title"], "finish task");
        assert_eq!(output["session"]["counts"]["tool_calls"], 2);
        assert_eq!(output["session"]["counts"]["write_like"], 1);
        assert_eq!(output["session"]["counts"]["shell_like"], 1);
        assert_eq!(output["session"]["changed_paths"], json!(["src/foo.rs"]));
        assert!(output["session"]["recent_events"].as_array().unwrap().len() >= 2);
        let actions = output["suggested_next_actions"].as_array().unwrap();
        assert!(actions
            .iter()
            .any(|v| v == "review changed paths from this session"));
        assert!(actions
            .iter()
            .any(|v| v == "check command/test results before commit"));
    }

    #[test]
    fn show_changes_with_missing_session_id_returns_warning_not_panic() {
        let mut output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
        apply_show_changes_session(&mut output, Some("wc_sess_missing"), None);
        assert_eq!(output["session"]["found"], false);
        assert_eq!(output["session"]["session_id"], "wc_sess_missing");
        assert!(output["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["kind"] == "session_not_found"));
        assert_eq!(output["suggested_next_actions"][0], "no changes detected");
    }

    #[test]
    fn show_changes_session_changed_paths_are_deduped() {
        let runtime = test_runtime();
        let session = runtime.sessions.start_session(None, None);
        for path in ["src/foo.rs", "src/foo.rs", "src/bar.rs"] {
            let args = json!({"project": "agent:oe:webcodex", "path": path});
            let start = runtime.sessions.record_tool_call_started(
                Some(&session.session_id),
                crate::tool_runtime::sessions::SessionTransport::Api,
                "write_project_file",
                &args,
            );
            runtime
                .sessions
                .record_tool_call_finished(start, true, &json!({}), None, None);
        }
        let mut output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n M src/foo.rs",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            " src/foo.rs | 2 +-",
            None,
            20,
            80,
            Some(0),
            "",
        );
        let summary = runtime.sessions.summary(&session.session_id, Some(30));
        apply_show_changes_session(&mut output, Some(&session.session_id), summary);
        assert_eq!(
            output["session"]["changed_paths"],
            json!(["src/foo.rs", "src/bar.rs"])
        );
    }

    #[tokio::test]
    async fn show_changes_session_event_limit_is_bounded() {
        let runtime = runtime_with_agent_project("show");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "show", None, caps).await;
        let session = runtime.sessions.start_session(None, None);
        for idx in 0..250 {
            let args =
                json!({"project": agent_test_project_id("show"), "path": format!("src/{idx}.rs")});
            let start = runtime.sessions.record_tool_call_started(
                Some(&session.session_id),
                crate::tool_runtime::sessions::SessionTransport::Api,
                "write_project_file",
                &args,
            );
            runtime
                .sessions
                .record_tool_call_finished(start, true, &json!({}), None, None);
        }
        let runtime_for_task = runtime.clone();
        let project = agent_test_project_id("show");
        let session_id = session.session_id.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .show_changes(project, Some(session_id), None, None, None, Some(999))
                .await
        });
        let req = next_patch_agent_request(&runtime, "show")
            .await
            .expect("show_changes should enqueue an agent shell request");
        let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0test head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
        complete_patch_agent_request(&runtime, "show", &req.request_id, 0, stdout, "").await;
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        let len = result.output["session"]["recent_events"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(len, 200);
    }

    #[test]
    fn show_changes_reports_modified_file() {
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n M src/users_http.rs",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            " src/users_http.rs | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)",
            None,
            20,
            80,
            Some(0),
            "",
        );
        assert_eq!(output["clean"], false);
        assert_eq!(output["counts"]["modified"], 1);
        assert_eq!(output["counts"]["unstaged"], 1);
        assert_eq!(output["files"][0]["path"], "src/users_http.rs");
        assert_eq!(output["files"][0]["status"], "modified");
        assert_eq!(output["files"][0]["kind"], "tracked");
        assert!(output["diff_stat"]
            .as_str()
            .unwrap()
            .contains("1 file changed"));
    }

    #[test]
    fn show_changes_reports_untracked_file() {
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n?? webcodex-anchor-edit-smoke-c99f7de.txt",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
        assert_eq!(output["clean"], false);
        assert_eq!(output["counts"]["untracked"], 1);
        assert_eq!(output["files"][0]["status"], "untracked");
        assert_eq!(output["files"][0]["staged"], false);
        assert_eq!(output["warnings"][0]["kind"], "untracked_smoke_file");
        assert!(output["suggested_next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str().unwrap().contains("untracked")));
    }

    #[test]
    fn show_changes_include_diff_false_omits_hunks() {
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n M src/lib.rs",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            " src/lib.rs | 2 +-",
            None,
            20,
            80,
            Some(0),
            "",
        );
        assert!(output.get("hunks").is_none());
        assert!(output.get("hunk_count").is_none());
    }

    #[test]
    fn show_changes_include_diff_true_returns_bounded_hunks() {
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 line one
-old
+new
 line three
@@ -10,3 +10,3 @@
 alpha
-beta
+gamma
 omega
";
        let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main\n M src/lib.rs",
            "b47e4fb000000000000000000000000000000000\0b47e4fb\0fix",
            " src/lib.rs | 4 ++--",
            Some(diff),
            1,
            4,
            Some(0),
            "",
        );
        assert_eq!(output["hunk_count"], 1);
        assert_eq!(output["hunks_truncated"], true);
        let hunks = output["hunks"].as_array().unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0]["path"], "src/lib.rs");
        assert_eq!(hunks[0]["hunks"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn show_changes_clean_repo_include_diff_false_has_no_untracked_previews() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());

        let output = show_changes_output_from_command(tmp.path(), false);

        assert_eq!(output["clean"], true);
        assert_eq!(output["counts"]["untracked"], 0);
        assert!(output.get("untracked_previews").is_none());
    }

    #[tokio::test]
    async fn show_changes_untracked_text_include_diff_false_omits_preview() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());
        let content = "webcodex untracked preview body";
        fs::write(tmp.path().join("notes.txt"), content).unwrap();

        let output = show_changes_output_from_command(tmp.path(), false);

        assert_eq!(output["counts"]["untracked"], 1);
        assert!(output_has_file(&output, "notes.txt"));
        assert!(output.get("untracked_previews").is_none());
        let serialized = serde_json::to_string(&output).unwrap();
        assert!(
            !serialized.contains(content),
            "include_diff=false leaked untracked file content: {serialized}"
        );
    }

    #[tokio::test]
    async fn show_changes_untracked_text_include_diff_true_returns_bounded_preview() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());
        fs::write(tmp.path().join("notes.txt"), "alpha\nbeta\n").unwrap();

        let output = show_changes_output_from_command(tmp.path(), true);

        assert_eq!(output["counts"]["untracked"], 1);
        assert!(output_has_file(&output, "notes.txt"));
        let preview = preview_for_path(&output, "notes.txt");
        assert_eq!(preview["kind"], "text");
        assert_eq!(preview["line_count"], 2);
        assert_eq!(preview["truncated"], false);
        assert_eq!(preview["lines"][0]["line"], 1);
        assert_eq!(preview["lines"][0]["text"], "alpha");
        assert_eq!(preview["lines"][1]["line"], 2);
        assert_eq!(preview["lines"][1]["text"], "beta");
        assert_eq!(output["hunk_count"], 0);
    }

    #[tokio::test]
    async fn show_changes_untracked_large_file_preview_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());
        fs::write(tmp.path().join("large.txt"), vec![b'x'; 8193]).unwrap();

        let output = show_changes_output_from_command(tmp.path(), true);

        assert_eq!(output["counts"]["untracked"], 1);
        let preview = preview_for_path(&output, "large.txt");
        assert_eq!(preview["kind"], "skipped");
        assert_eq!(preview["reason"], "too_large");
        assert_eq!(preview["byte_count"], 8193);
    }

    #[tokio::test]
    async fn show_changes_untracked_binary_preview_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());
        fs::write(tmp.path().join("binary.bin"), [0, 159, 146, 150]).unwrap();

        let output = show_changes_output_from_command(tmp.path(), true);

        assert_eq!(output["counts"]["untracked"], 1);
        let preview = preview_for_path(&output, "binary.bin");
        assert_eq!(preview["kind"], "skipped");
        assert_eq!(preview["reason"], "binary_or_non_utf8");
    }

    #[tokio::test]
    async fn show_changes_untracked_sensitive_path_preview_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        init_git_repo(tmp.path());
        fs::write(tmp.path().join("agent.toml"), "API_TOKEN=secret\n").unwrap();

        let output = show_changes_output_from_command(tmp.path(), true);

        assert_eq!(output["counts"]["untracked"], 1);
        let preview = preview_for_path(&output, "agent.toml");
        assert_eq!(preview["kind"], "skipped");
        assert_eq!(preview["reason"], "sensitive_or_excluded_path");
        let serialized = serde_json::to_string(&output).unwrap();
        assert!(
            !serialized.contains("API_TOKEN=secret"),
            "sensitive file content leaked: {serialized}"
        );
    }

    #[test]
    fn git_diff_hunks_command_rejects_unsafe_paths() {
        assert!(git_diff_hunks_command(&["src/lib.rs".to_string()], false)
            .unwrap()
            .contains("git diff --unified=80 -- 'src/lib.rs'"));
        assert!(validate_project_relative_path("../outside").is_err());
    }

    #[tokio::test]
    async fn git_diff_hunks_rejects_unsafe_paths_before_project_dispatch() {
        let runtime = test_runtime();
        let result = runtime
            .git_diff_hunks(
                "agent:oe:webcodex".to_string(),
                Some(vec!["../outside".to_string()]),
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("parent traversal"));
    }

    #[test]
    fn tool_specs_delete_project_files_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "delete_project_files");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_git_restore_paths_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "git_restore_paths");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_discard_untracked_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "discard_untracked");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"paths".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_list_project_files_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "list_project_files");
        let required = required_fields(spec);
        assert_eq!(required, vec!["project".to_string()]);
        // path/limit are optional.
        assert!(!required.contains(&"path".to_string()));
        assert!(!required.contains(&"limit".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_search_project_text_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "search_project_text");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"pattern".to_string()));
        assert!(!required.contains(&"path".to_string()));
        assert!(!required.contains(&"limit".to_string()));
        assert!(!required.contains(&"context_before".to_string()));
        assert!(!required.contains(&"context_after".to_string()));
        let props = spec.input_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("context_before"));
        assert!(props.contains_key("context_after"));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_read_file_schema_includes_optional_line_numbers() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "read_file");
        let required = required_fields(spec);
        assert!(required.contains(&"project".to_string()));
        assert!(required.contains(&"path".to_string()));
        assert!(!required.contains(&"with_line_numbers".to_string()));
        let props = spec.input_schema["properties"].as_object().unwrap();
        assert!(props.contains_key("with_line_numbers"));
    }

    #[test]
    fn tool_specs_list_jobs_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "list_jobs");
        let required = required_fields(spec);
        // list_jobs has only optional fields.
        assert!(required.is_empty());
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_specs_job_tail_schema() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let spec = spec_named(&specs, "job_tail");
        let required = required_fields(spec);
        assert_eq!(required, vec!["job_id".to_string()]);
        assert!(!required.contains(&"tail_lines".to_string()));
        assert!(spec.description.chars().count() <= 300);
    }

    #[test]
    fn tool_categories_and_recommended_flows_are_well_formed() {
        let runtime = test_runtime();
        let categories = runtime.tool_categories();
        // Every declared category is a non-empty array of known tool names.
        let names = runtime.tool_names();
        for (cat, members) in categories.as_object().unwrap() {
            let arr = members.as_array().unwrap();
            assert!(!arr.is_empty(), "category '{}' must not be empty", cat);
            for m in arr {
                let name = m.as_str().unwrap();
                assert!(
                    names.iter().any(|n| n == name),
                    "category '{}' lists unknown tool '{}'",
                    cat,
                    name
                );
            }
        }
        // Each expected category is present.
        for cat in [
            "inspect",
            "git",
            "review",
            "validation",
            "patch",
            "shell",
            "jobs",
            "runtime",
            "cleanup",
        ] {
            assert!(
                categories.as_object().unwrap().contains_key(cat),
                "missing category {}",
                cat
            );
        }
        let validation = categories["validation"].as_array().unwrap();
        for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
            assert!(validation.iter().any(|v| v == name));
        }
        let review = categories["review"].as_array().unwrap();
        assert!(review.iter().any(|v| v == "git_diff_hunks"));
        // recommended_flows are short and non-empty.
        let flows = ToolRuntime::recommended_flows();
        assert!(!flows.is_empty());
        for flow in &flows {
            assert!(flow.chars().count() <= 300, "flow too long: {}", flow);
        }
        let joined_flows = flows.join("\n").to_lowercase();
        assert!(joined_flows.contains("source code edit"));
        for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
            assert!(
                joined_flows.contains(name),
                "recommended flows should mention {}",
                name
            );
        }
        assert!(joined_flows.contains("run_shell"));
        assert!(
            joined_flows.contains("validation") || joined_flows.contains("checks"),
            "run_shell guidance should mention validation/checks"
        );
        assert!(
            joined_flows.contains("not primary"),
            "run_shell should not be the primary edit path"
        );
        for name in ["cargo_fmt", "cargo_check", "cargo_test", "git_diff_hunks"] {
            assert!(
                joined_flows.contains(name),
                "recommended flows should mention {}",
                name
            );
        }
        let specs = runtime.tool_specs();
        for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
            let desc = spec_named(&specs, name).description.to_lowercase();
            assert!(desc.contains("preferred"), "{} should be preferred", name);
            assert!(desc.contains("source"), "{} should mention source", name);
            assert!(desc.contains("line"), "{} should mention line", name);
        }

        let run_shell_desc = spec_named(&specs, "run_shell").description.to_lowercase();
        assert!(run_shell_desc.contains("file editing path"));
        assert!(run_shell_desc.contains("not"));
    }

    #[test]
    fn tool_specs_annotations_cover_safety_hints() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for spec in &specs {
            let annotations = spec
                .annotations
                .as_object()
                .unwrap_or_else(|| panic!("{} annotations must be an object", spec.name));
            for field in [
                "readOnlyHint",
                "destructiveHint",
                "idempotentHint",
                "openWorldHint",
            ] {
                assert!(
                    annotations.contains_key(field),
                    "{} missing annotation {}",
                    spec.name,
                    field
                );
            }
        }

        for name in [
            "read_file",
            "git_status",
            "git_diff_summary",
            "git_diff_hunks",
            "show_changes",
        ] {
            assert_eq!(spec_named(&specs, name).annotations["readOnlyHint"], true);
        }
        for name in ["replace_line_range", "insert_at_line", "delete_line_range"] {
            let annotations = &spec_named(&specs, name).annotations;
            assert_eq!(annotations["readOnlyHint"], false);
            assert_eq!(annotations["openWorldHint"], false);
        }
        for name in ["run_shell", "run_job", "run_codex"] {
            assert_eq!(spec_named(&specs, name).annotations["openWorldHint"], true);
        }
        for name in [
            "delete_project_files",
            "discard_untracked",
            "git_restore_paths",
        ] {
            assert_eq!(
                spec_named(&specs, name).annotations["destructiveHint"],
                true
            );
        }
        for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
            let annotations = &spec_named(&specs, name).annotations;
            assert_eq!(annotations["readOnlyHint"], false);
            assert_eq!(annotations["destructiveHint"], false);
            assert_eq!(annotations["openWorldHint"], false);
        }
    }

    #[test]
    fn mcp_tool_annotations_use_metadata_for_read_write_tools() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for name in [
            "show_changes",
            "write_project_file",
            "delete_project_files",
            "run_shell",
            "cargo_test",
        ] {
            let metadata = crate::tool_runtime::metadata::lookup_tool_metadata(name).unwrap();
            let annotations = &spec_named(&specs, name).annotations;
            assert_eq!(annotations["readOnlyHint"], metadata.read_only, "{name}");
            assert_eq!(
                annotations["destructiveHint"], metadata.destructive,
                "{name}"
            );
            assert_eq!(annotations["openWorldHint"], metadata.shell_like, "{name}");
            assert_eq!(annotations["idempotentHint"], metadata.read_only, "{name}");
        }
    }

    #[test]
    fn from_tool_name_unknown_tool_lists_available_tools_and_hint() {
        let err = ToolCall::from_tool_name("definitely_not_a_tool", Value::Null).unwrap_err();
        assert!(err.contains("definitely_not_a_tool"));
        assert!(
            err.contains("listRuntimeTools") || err.contains("list_tools"),
            "unknown-tool error should hint at discovery: {}",
            err
        );
        // Should list at least a couple of known tool names.
        assert!(err.contains("git_diff_summary"));
        assert!(err.contains("apply_patch_checked"));
        // Must not leak secret/config artifacts.
        let lower = err.to_lowercase();
        for forbidden in [
            "token",
            "authorization",
            "agent.toml",
            "webcodex.env",
            "secret",
        ] {
            assert!(
                !lower.contains(&forbidden),
                "unknown-tool error must not leak '{}': {}",
                forbidden,
                err
            );
        }
    }

    #[test]
    fn known_tool_names_matches_spec_count() {
        let runtime = test_runtime();
        let spec_count = runtime.tool_specs().len();
        assert_eq!(
            KNOWN_TOOL_NAMES.len(),
            spec_count,
            "KNOWN_TOOL_NAMES must stay in sync with tool_specs()"
        );
        // Every known name must be recognized (i.e. must NOT yield the
        // "unknown tool" error). Unit tools parse with null args; non-unit
        // tools fail with a missing-field error, which is still a recognition
        // success (the variant matched).
        for name in KNOWN_TOOL_NAMES {
            assert!(
                is_known_tool_name(name),
                "known name '{}' not recognized by is_known_tool_name",
                name
            );
            let result = ToolCall::from_tool_name(name, Value::Null);
            match result {
                Ok(_) => {}
                Err(e) => {
                    assert!(
                        !e.contains("unknown tool"),
                        "known tool '{}' was treated as unknown: {}",
                        name,
                        e
                    );
                }
            }
        }
        // An unknown name must still produce the unknown-tool error.
        let err = ToolCall::from_tool_name("not_a_real_tool", Value::Null).unwrap_err();
        assert!(err.contains("unknown tool"));
    }

    // =========================================================================
    // Phase 2: local job recovery, path safety, status normalization, bounded logs
    // =========================================================================

    fn local_project_config(path: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    fn runtime_with_project(root: &Path, project_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            project_id.to_string(),
            local_project_config(&root.to_string_lossy()),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    fn init_git_repo(root: &Path) {
        for cmd in [
            "git init",
            "git config user.email webcodex-test@example.com",
            "git config user.name WebCodex Test",
        ] {
            let (exit_code, stdout, stderr, _) = run_command_sync(cmd, root, 30);
            assert_eq!(
                exit_code, 0,
                "git setup command failed: {cmd}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            );
        }
    }

    fn output_has_file(output: &Value, path: &str) -> bool {
        output["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["path"].as_str() == Some(path))
    }

    fn preview_for_path<'a>(output: &'a Value, path: &str) -> &'a Value {
        output["untracked_previews"]
            .as_array()
            .unwrap()
            .iter()
            .find(|preview| preview["path"].as_str() == Some(path))
            .unwrap_or_else(|| {
                panic!(
                    "missing preview for {path}: {}",
                    output["untracked_previews"]
                )
            })
    }

    fn show_changes_output_from_command(root: &Path, include_diff: bool) -> Value {
        let command = show_changes_command(include_diff);
        let (exit_code, stdout, stderr, _) = run_command_sync(&command, root, 30);
        assert_eq!(
            exit_code, 0,
            "show_changes command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
        );
        let (status_stdout, head_stdout, diff_stat, diff_stdout, untracked_preview_stdout) =
            split_show_changes_stdout(&stdout, include_diff);
        let mut output = parse_show_changes_output(
            "demo",
            &status_stdout,
            &head_stdout,
            &diff_stat,
            include_diff.then_some(diff_stdout.as_str()),
            20,
            80,
            Some(exit_code),
            &stderr,
        );
        if include_diff {
            apply_show_changes_untracked_previews(&mut output, &untracked_preview_stdout);
        }
        output
    }

    fn finished_event<'a>(
        summary: &'a crate::tool_runtime::sessions::SessionSummary,
        tool_name: &str,
    ) -> &'a crate::tool_runtime::sessions::SessionEvent {
        summary
            .events
            .iter()
            .rev()
            .find(|event| event.kind == "tool_call_finished" && event.tool_name == tool_name)
            .unwrap_or_else(|| {
                panic!(
                    "missing finished event for {tool_name}: {:?}",
                    summary.events
                )
            })
    }

    #[tokio::test]
    async fn read_file_with_session_id_records_event_without_content() {
        let runtime = runtime_with_agent_project("telemetry-read");
        register_agent(
            &runtime,
            "telemetry-read",
            None,
            ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("telemetry-read");
        let session = runtime
            .sessions
            .start_session(Some(project.clone()), Some("read telemetry".to_string()));
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project,
                            path: "README.md".to_string(),
                            session_id: Some(session_id),
                            start_line: None,
                            limit: Some(1),
                            with_line_numbers: Some(true),
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_instance(&runtime, "telemetry-read", "inst")
            .await
            .expect("read_file should enqueue an agent request");
        assert_eq!(req.kind, "file_read");
        complete_patch_agent_request(
            &runtime,
            "telemetry-read",
            &req.request_id,
            0,
            "secret line\nsecond\n",
            "",
        )
        .await;
        let result = task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["session_recorded"], true);
        assert_eq!(result.output["session_id"], session.session_id);
        assert!(result.output["session_event_id"].as_str().is_some());
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.succeeded, 1);
        assert_eq!(summary.counts.read_like, 1);
        let event = finished_event(&summary, "read_file");
        assert_eq!(event.status.as_deref(), Some("succeeded"));
        assert!(event.read_like);
        assert!(!event.write_like);
        let serialized = serde_json::to_string(&summary.events).unwrap();
        assert!(
            !serialized.contains("secret line"),
            "session event leaked read_file content: {serialized}"
        );
    }

    #[tokio::test]
    async fn git_status_with_session_id_records_git_read_event() {
        let runtime = runtime_with_agent_project("telemetry-git");
        let mut caps = ShellClientCapabilities::default();
        caps.git = true;
        caps.shell = false;
        register_agent(&runtime, "telemetry-git", None, caps).await;
        let project = agent_test_project_id("telemetry-git");
        let session = runtime.sessions.start_session(None, None);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::GitStatus {
                            project,
                            session_id: Some(session_id),
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "telemetry-git")
            .await
            .expect("git_status should enqueue an agent shell request");
        complete_patch_agent_request(&runtime, "telemetry-git", &req.request_id, 0, "", "").await;
        let result = task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["session_recorded"], true);
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 1);
        assert_eq!(summary.counts.read_like, 1);
        assert_eq!(summary.counts.git_like, 1);
        let event = finished_event(&summary, "git_status");
        assert!(event.git_like);
        assert!(event.read_like);
    }

    #[tokio::test]
    async fn run_shell_session_events_record_exit_without_stdio_bodies() {
        let runtime = runtime_with_agent_project("telemetry-shell");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "telemetry-shell", None, caps).await;
        let project = agent_test_project_id("telemetry-shell");
        let session = runtime.sessions.start_session(None, None);

        let ok_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::RunShell {
                            project,
                            command: "printf shell-secret-out; printf shell-secret-err >&2"
                                .to_string(),
                            session_id: Some(session_id),
                            timeout_secs: Some(30),
                            cwd: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "telemetry-shell")
            .await
            .expect("run_shell should enqueue success request");
        complete_patch_agent_request(
            &runtime,
            "telemetry-shell",
            &req.request_id,
            0,
            "shell-secret-out",
            "shell-secret-err",
        )
        .await;
        let ok = ok_task.await.unwrap();
        assert!(ok.success, "{:?}", ok.error);
        assert_eq!(ok.output["session_recorded"], true);

        let fail_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::RunShell {
                            project,
                            command: "printf fail-secret-out; printf fail-secret-err >&2; exit 7"
                                .to_string(),
                            session_id: Some(session_id),
                            timeout_secs: Some(30),
                            cwd: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "telemetry-shell")
            .await
            .expect("run_shell should enqueue failure request");
        complete_patch_agent_request(
            &runtime,
            "telemetry-shell",
            &req.request_id,
            7,
            "fail-secret-out",
            "fail-secret-err",
        )
        .await;
        let fail = fail_task.await.unwrap();
        assert!(!fail.success);
        assert_eq!(fail.output["failure_kind"], "command_exit_nonzero");
        assert_eq!(fail.output["session_recorded"], true);

        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 2);
        assert_eq!(summary.counts.succeeded, 1);
        assert_eq!(summary.counts.failed, 1);
        assert_eq!(summary.counts.shell_like, 2);
        let failed = summary
            .events
            .iter()
            .rev()
            .find(|event| {
                event.kind == "tool_call_finished"
                    && event.tool_name == "run_shell"
                    && event.status.as_deref() == Some("failed")
            })
            .unwrap();
        assert_eq!(failed.exit_code, Some(7));
        assert_eq!(failed.failure_kind.as_deref(), Some("command_exit_nonzero"));
        assert_eq!(failed.error_kind.as_deref(), Some("command_exit_nonzero"));
        let serialized = serde_json::to_string(&summary.events).unwrap();
        for leaked in [
            "shell-secret-out",
            "shell-secret-err",
            "fail-secret-out",
            "fail-secret-err",
        ] {
            assert!(
                !serialized.contains(leaked),
                "session event leaked shell output {leaked}: {serialized}"
            );
        }
        assert!(serialized.contains("\"command_present\":true"));
    }

    #[tokio::test]
    async fn write_project_file_with_session_id_records_changed_path_without_content() {
        let runtime = runtime_with_agent_project("telemetry-write");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "telemetry-write", None, caps).await;
        let project = agent_test_project_id("telemetry-write");
        let session = runtime.sessions.start_session(None, None);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::WriteProjectFile {
                            project,
                            path: "src/new.txt".to_string(),
                            content: "do-not-log-this-content\n".to_string(),
                            session_id: Some(session_id),
                            overwrite: None,
                            expected_sha256: None,
                            expected_content_prefix: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "telemetry-write")
            .await
            .expect("write_project_file should enqueue helper request");
        complete_patch_agent_request(
            &runtime,
            "telemetry-write",
            &req.request_id,
            0,
            r#"{"path":"src/new.txt","bytes_written":24,"sha256":"abc","changed":true}"#,
            "",
        )
        .await;
        let result = task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.write_like, 1);
        let event = finished_event(&summary, "write_project_file");
        assert!(event.write_like);
        assert_eq!(event.changed_paths, vec!["src/new.txt".to_string()]);
        let serialized = serde_json::to_string(&summary.events).unwrap();
        assert!(
            !serialized.contains("do-not-log-this-content"),
            "session event leaked write content: {serialized}"
        );
    }

    #[tokio::test]
    async fn show_changes_with_session_id_returns_session_block_and_records_call() {
        let runtime = runtime_with_agent_project("telemetry-show");
        let mut caps = ShellClientCapabilities::default();
        caps.file_read = true;
        caps.shell = true;
        register_agent(&runtime, "telemetry-show", None, caps).await;
        let project = agent_test_project_id("telemetry-show");
        let session = runtime.sessions.start_session(None, None);

        let read_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project,
                            path: "README.md".to_string(),
                            session_id: Some(session_id),
                            start_line: None,
                            limit: Some(1),
                            with_line_numbers: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_instance(&runtime, "telemetry-show", "inst")
            .await
            .expect("read_file should enqueue before show_changes");
        complete_patch_agent_request(
            &runtime,
            "telemetry-show",
            &req.request_id,
            0,
            "hello\n",
            "",
        )
        .await;
        let read = read_task.await.unwrap();
        assert!(read.success, "{:?}", read.error);

        let show_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::ShowChanges {
                            project,
                            session_id: Some(session_id),
                            include_diff: Some(false),
                            max_hunks: None,
                            max_hunk_lines: None,
                            session_event_limit: Some(20),
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "telemetry-show")
            .await
            .expect("show_changes should enqueue shell request");
        let stdout =
            "## main\n M README.md\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n README.md | 1 +\n";
        complete_patch_agent_request(&runtime, "telemetry-show", &req.request_id, 0, stdout, "")
            .await;
        let result = show_task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["session_recorded"], true);
        assert_eq!(result.output["session"]["found"], true);
        assert_eq!(result.output["session"]["counts"]["tool_calls"], 1);
        assert!(result.output["session"]["recent_events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["tool_name"] == "read_file"));
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.tool_calls, 2);
        assert_eq!(summary.counts.change_summary_like, 1);
        let event = finished_event(&summary, "show_changes");
        assert!(event.git_like);
        assert!(event.change_summary_like);
    }

    #[tokio::test]
    async fn unknown_session_id_fails_before_execution_or_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("README.md"), "hello\n").unwrap();
        let runtime = runtime_with_project(root, "demo");

        let read = runtime
            .dispatch(ToolCall::ReadFile {
                project: "demo".to_string(),
                path: "README.md".to_string(),
                session_id: Some("wc_sess_missing".to_string()),
                start_line: None,
                limit: None,
                with_line_numbers: None,
            })
            .await;
        assert!(!read.success);
        assert_eq!(read.output["error_kind"], "unknown_session_id");
        assert_eq!(read.output["session_id"], "wc_sess_missing");
        assert!(read
            .error
            .as_deref()
            .unwrap()
            .contains("unknown_session_id"));

        let write = runtime
            .dispatch(ToolCall::WriteProjectFile {
                project: "demo".to_string(),
                path: "should-not-exist.txt".to_string(),
                content: "nope".to_string(),
                session_id: Some("wc_sess_missing".to_string()),
                overwrite: None,
                expected_sha256: None,
                expected_content_prefix: None,
            })
            .await;
        assert!(!write.success);
        assert_eq!(write.output["error_kind"], "unknown_session_id");
        assert!(!root.join("should-not-exist.txt").exists());
    }

    #[tokio::test]
    async fn no_session_id_keeps_old_behavior_without_telemetry_hint() {
        let runtime = runtime_with_agent_project("telemetry-nosession");
        register_agent(
            &runtime,
            "telemetry-nosession",
            None,
            ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("telemetry-nosession");
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project,
                            path: "README.md".to_string(),
                            session_id: None,
                            start_line: None,
                            limit: None,
                            with_line_numbers: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_instance(&runtime, "telemetry-nosession", "inst")
            .await
            .expect("read_file should enqueue without session_id");
        complete_patch_agent_request(
            &runtime,
            "telemetry-nosession",
            &req.request_id,
            0,
            "hello\n",
            "",
        )
        .await;
        let result = task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["content"], "hello");
        assert!(result.output.get("session_recorded").is_none());
    }

    #[tokio::test]
    async fn start_session_defaults_to_normal_without_guards() {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::from_tool_name("start_session", json!({})).unwrap())
            .await;

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["mode"], "normal");
        assert_eq!(result.output["guards"]["deny_write_tools"], false);
        assert_eq!(result.output["guards"]["deny_shell_tools"], false);
        let session_id = result.output["session_id"].as_str().unwrap();
        let summary = runtime.sessions.summary(session_id, None).unwrap();
        assert_eq!(summary.mode, SessionMode::Normal);
        assert!(!summary.guards.deny_write_tools);
        assert!(!summary.guards.deny_shell_tools);
    }

    #[tokio::test]
    async fn start_session_read_only_enables_write_and_shell_guards() {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(
                ToolCall::from_tool_name(
                    "start_session",
                    json!({"mode": "read_only", "deny_shell_tools": false}),
                )
                .unwrap(),
            )
            .await;

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["mode"], "read_only");
        assert_eq!(result.output["guards"]["deny_write_tools"], true);
        assert_eq!(result.output["guards"]["deny_shell_tools"], true);
        let session_id = result.output["session_id"].as_str().unwrap();
        let summary = runtime.sessions.summary(session_id, None).unwrap();
        assert_eq!(summary.mode, SessionMode::ReadOnly);
        assert!(summary.guards.deny_write_tools);
        assert!(summary.guards.deny_shell_tools);
    }

    #[tokio::test]
    async fn read_only_session_allows_read_file_and_records_success() {
        let runtime = runtime_with_agent_project("guard-read");
        register_agent(
            &runtime,
            "guard-read",
            None,
            ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("guard-read");
        let session = runtime.sessions.start_session_with_guards(
            Some(project.clone()),
            Some("read only".to_string()),
            SessionMode::ReadOnly,
            sessions::SessionGuards::default(),
        );

        let task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project,
                            path: "README.md".to_string(),
                            session_id: Some(session_id),
                            start_line: None,
                            limit: Some(1),
                            with_line_numbers: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_instance(&runtime, "guard-read", "inst")
            .await
            .expect("read_file should be allowed in read_only session");
        assert_eq!(req.kind, "file_read");
        complete_patch_agent_request(&runtime, "guard-read", &req.request_id, 0, "hello\n", "")
            .await;
        let result = task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["session_recorded"], true);
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.succeeded, 1);
        assert_eq!(summary.counts.read_like, 1);
        assert_eq!(
            finished_event(&summary, "read_file").status.as_deref(),
            Some("succeeded")
        );
    }

    #[tokio::test]
    async fn read_only_session_rejects_write_project_file_before_mutation() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        let session = runtime.sessions.start_session_with_guards(
            Some("demo".to_string()),
            Some("read only".to_string()),
            SessionMode::ReadOnly,
            sessions::SessionGuards::default(),
        );

        let result = runtime
            .dispatch(ToolCall::WriteProjectFile {
                project: "demo".to_string(),
                path: "should-not-exist.txt".to_string(),
                content: "nope".to_string(),
                session_id: Some(session.session_id.clone()),
                overwrite: None,
                expected_sha256: None,
                expected_content_prefix: None,
            })
            .await;

        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "session_guard_denied");
        assert_eq!(result.output["guard"], "deny_write_tools");
        assert_eq!(result.output["mode"], "read_only");
        assert_eq!(result.output["session_recorded"], true);
        assert!(result.output["session_event_id"].as_str().is_some());
        assert!(!tmp.path().join("should-not-exist.txt").exists());
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.failed, 1);
        assert_eq!(summary.counts.write_like, 1);
        let event = finished_event(&summary, "write_project_file");
        assert_eq!(event.status.as_deref(), Some("failed"));
        assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
    }

    #[tokio::test]
    async fn read_only_session_rejects_run_shell_before_agent_enqueue() {
        let runtime = runtime_with_agent_project("guard-shell");
        register_agent(
            &runtime,
            "guard-shell",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("guard-shell");
        let session = runtime.sessions.start_session_with_guards(
            Some(project.clone()),
            Some("read only".to_string()),
            SessionMode::ReadOnly,
            sessions::SessionGuards::default(),
        );

        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project,
                    command: "echo should-not-run".to_string(),
                    session_id: Some(session.session_id.clone()),
                    timeout_secs: Some(30),
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;

        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "session_guard_denied");
        assert_eq!(result.output["guard"], "deny_shell_tools");
        assert_eq!(result.output["command_started"], false);
        assert_eq!(result.output["session_recorded"], true);
        assert!(
            next_patch_agent_request(&runtime, "guard-shell")
                .await
                .is_none(),
            "run_shell guard denial must not enqueue an agent request"
        );
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .unwrap();
        assert_eq!(summary.counts.failed, 1);
        assert_eq!(summary.counts.shell_like, 1);
        let event = finished_event(&summary, "run_shell");
        assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
    }

    #[tokio::test]
    async fn deny_write_only_allows_read_and_shell_tools() {
        let runtime = runtime_with_agent_project("guard-write-only");
        register_agent(
            &runtime,
            "guard-write-only",
            None,
            ShellClientCapabilities {
                file_read: true,
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("guard-write-only");
        let session = runtime.sessions.start_session_with_guards(
            Some(project.clone()),
            None,
            SessionMode::Normal,
            sessions::SessionGuards {
                deny_write_tools: true,
                deny_shell_tools: false,
            },
        );
        let bootstrap = auth_context(None, true);

        let denied = runtime
            .dispatch_with_auth(
                ToolCall::WriteProjectFile {
                    project: project.clone(),
                    path: "blocked.txt".to_string(),
                    content: "x".to_string(),
                    session_id: Some(session.session_id.clone()),
                    overwrite: None,
                    expected_sha256: None,
                    expected_content_prefix: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!denied.success);
        assert_eq!(denied.output["guard"], "deny_write_tools");

        let read_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project,
                            path: "README.md".to_string(),
                            session_id: Some(session_id),
                            start_line: None,
                            limit: Some(1),
                            with_line_numbers: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_instance(&runtime, "guard-write-only", "inst")
            .await
            .expect("read_file should be allowed with deny_write_tools only");
        complete_patch_agent_request(
            &runtime,
            "guard-write-only",
            &req.request_id,
            0,
            "hello\n",
            "",
        )
        .await;
        assert!(read_task.await.unwrap().success);

        let shell_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::RunShell {
                            project,
                            command: "exit 0".to_string(),
                            session_id: Some(session_id),
                            timeout_secs: Some(30),
                            cwd: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "guard-write-only")
            .await
            .expect("run_shell should be allowed when deny_shell_tools=false");
        complete_patch_agent_request(&runtime, "guard-write-only", &req.request_id, 0, "", "")
            .await;
        assert!(shell_task.await.unwrap().success);
    }

    #[tokio::test]
    async fn deny_shell_only_allows_write_tools() {
        let runtime = runtime_with_agent_project("guard-shell-only");
        register_agent(
            &runtime,
            "guard-shell-only",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("guard-shell-only");
        let session = runtime.sessions.start_session_with_guards(
            Some(project.clone()),
            None,
            SessionMode::Normal,
            sessions::SessionGuards {
                deny_write_tools: false,
                deny_shell_tools: true,
            },
        );
        let bootstrap = auth_context(None, true);

        let denied = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: project.clone(),
                    command: "echo blocked".to_string(),
                    session_id: Some(session.session_id.clone()),
                    timeout_secs: Some(30),
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!denied.success);
        assert_eq!(denied.output["guard"], "deny_shell_tools");

        let write_task = tokio::spawn({
            let runtime = runtime.clone();
            let project = project.clone();
            let session_id = session.session_id.clone();
            async move {
                let bootstrap = auth_context(None, true);
                runtime
                    .dispatch_with_auth(
                        ToolCall::WriteProjectFile {
                            project,
                            path: "allowed.txt".to_string(),
                            content: "x".to_string(),
                            session_id: Some(session_id),
                            overwrite: None,
                            expected_sha256: None,
                            expected_content_prefix: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_patch_agent_request(&runtime, "guard-shell-only")
            .await
            .expect("write_project_file should be allowed when deny_write_tools=false");
        complete_patch_agent_request(
            &runtime,
            "guard-shell-only",
            &req.request_id,
            0,
            r#"{"path":"allowed.txt","bytes_written":1,"sha256":"abc","changed":true}"#,
            "",
        )
        .await;
        assert!(write_task.await.unwrap().success);
    }

    #[test]
    fn project_tool_schemas_include_optional_session_id() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        let start_session = spec_named(&specs, "start_session");
        assert_eq!(
            start_session.input_schema["properties"]["mode"]["enum"],
            json!(["normal", "read_only"])
        );
        assert!(start_session.input_schema["properties"]
            .get("deny_write_tools")
            .is_some());
        assert!(start_session.input_schema["properties"]
            .get("deny_shell_tools")
            .is_some());
        assert!(
            start_session.output_schema["properties"]["output"]["properties"]
                .get("mode")
                .is_some()
        );
        assert!(
            start_session.output_schema["properties"]["output"]["properties"]
                .get("guards")
                .is_some()
        );
        let session_summary = spec_named(&specs, "session_summary");
        assert!(
            session_summary.output_schema["properties"]["output"]["properties"]
                .get("mode")
                .is_some()
        );
        assert!(
            session_summary.output_schema["properties"]["output"]["properties"]
                .get("guards")
                .is_some()
        );
        for name in [
            "read_file",
            "run_shell",
            "write_project_file",
            "replace_line_range",
            "git_status",
            "show_changes",
        ] {
            let spec = spec_named(&specs, name);
            assert!(
                spec.input_schema["properties"].get("session_id").is_some(),
                "{name} schema missing session_id"
            );
            assert!(
                !spec.input_schema["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|field| field == "session_id"),
                "{name} schema must not require session_id"
            );
        }
        for name in ["read_file", "run_shell", "write_project_file"] {
            let spec = spec_named(&specs, name);
            assert!(spec.output_schema["properties"]["output"]["properties"]
                .get("session_recorded")
                .is_some());
            assert!(spec.output_schema["properties"]["output"]["properties"]
                .get("session_event_id")
                .is_some());
        }
    }

    /// Write a fake on-disk local job simulating a job that survived a restart.
    fn write_fake_job(
        root: &Path,
        job_id: &str,
        project: &str,
        path: &str,
        status: &str,
        stdout: &str,
        stderr: &str,
        meta_extra: Value,
    ) -> PathBuf {
        let dir = root.join(format!(".codex/jobs/{}", job_id));
        fs::create_dir_all(&dir).unwrap();
        let mut meta = json!({
            "job_id": job_id,
            "project": project,
            "path": path,
            "command": "echo test",
            "status": "running",
            "created_at": 1000,
            "started_at": 1000,
            "max_runtime_secs": 3600,
            "executor": "local",
            "kind": "shell",
        });
        if let (Value::Object(ref mut m), Value::Object(extra)) = (&mut meta, meta_extra) {
            for (k, v) in extra {
                m.insert(k, v);
            }
        }
        fs::write(
            dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .unwrap();
        fs::write(dir.join("status"), status).unwrap();
        fs::write(dir.join("stdout.log"), stdout).unwrap();
        fs::write(dir.join("stderr.log"), stderr).unwrap();
        dir
    }

    #[test]
    fn is_safe_job_id_rejects_path_traversal_and_separators() {
        assert!(is_safe_job_id("11111111-2222-3333-4444-555555555555"));
        assert!(is_safe_job_id("job.1_2-3"));
        assert!(!is_safe_job_id("../escape"));
        assert!(!is_safe_job_id("a/b"));
        assert!(!is_safe_job_id("a\\b"));
        assert!(!is_safe_job_id(".."));
        assert!(!is_safe_job_id("a..b/../c"));
        assert!(!is_safe_job_id(""));
        assert!(!is_safe_job_id("a\0b"));
    }

    #[test]
    fn normalize_local_status_maps_known_and_unknown_values() {
        assert_eq!(normalize_local_status("running"), "running");
        assert_eq!(normalize_local_status("completed"), "completed");
        assert_eq!(normalize_local_status("failed"), "failed");
        assert_eq!(normalize_local_status("stopped"), "stopped");
        assert_eq!(normalize_local_status("queued"), "queued");
        assert_eq!(normalize_local_status("  failed  "), "failed");
        assert_eq!(normalize_local_status(""), "running");
        assert_eq!(normalize_local_status("weird-state"), "lost");
    }

    #[test]
    fn read_lines_from_is_bounded_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let (text, next) = read_lines_from(path, None, None);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
        assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
        // Default is tail: last 500 lines.
        assert_eq!(lines[0], "line 501");
        assert_eq!(lines.last().unwrap(), &"line 1000");
        assert_eq!(next, 1001);
    }

    #[test]
    fn read_lines_from_supports_offset_pagination() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=600)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let (text, next) = read_lines_from(path.clone(), Some(1), None);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
        assert_eq!(lines[0], "line 1");
        assert_eq!(lines.last().unwrap(), &"line 500");
        assert_eq!(next, 501);

        let (text2, next2) = read_lines_from(path, Some(501), None);
        let lines2: Vec<&str> = text2.lines().collect();
        assert_eq!(lines2.len(), 100);
        assert_eq!(lines2[0], "line 501");
        assert_eq!(lines2.last().unwrap(), &"line 600");
        assert_eq!(next2, 601);
    }

    #[test]
    fn read_lines_from_supports_tail_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        let (text, _next) = read_lines_from(path, None, Some(10));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[0], "line 991");
        assert_eq!(lines.last().unwrap(), &"line 1000");
    }

    #[test]
    fn read_lines_from_tail_is_capped_to_max() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("stdout.log");
        let content = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, &content).unwrap();
        // Requesting more than MAX returns at most MAX.
        let (text, _) = read_lines_from(path, None, Some(5000));
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), MAX_LOCAL_LOG_LINES);
    }

    #[tokio::test]
    async fn recover_local_job_status_after_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let project_id = "demo";
        let runtime = runtime_with_project(root, project_id);
        let job_id = "11111111-2222-3333-4444-555555555555";
        write_fake_job(
            root,
            job_id,
            project_id,
            &root.to_string_lossy(),
            "completed",
            "hello\n",
            "",
            json!({}),
        );
        // local_jobs is empty (simulating restart); recovery should find it.
        assert!(runtime.local_jobs.lock().await.is_empty());
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "completed");
        assert_eq!(result.output["project"], project_id);
        assert_eq!(result.output["executor"], "local");
        assert_eq!(result.output["kind"], "shell");
        // Recovered job is now cached in memory.
        assert!(runtime.local_jobs.lock().await.contains_key(job_id));
    }

    #[tokio::test]
    async fn recover_local_job_log_after_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "22222222-3333-4444-5555-666666666666";
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            "stdout line\n",
            "stderr line\n",
            json!({}),
        );
        let result = runtime.job_log(job_id.to_string(), None, None).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["stdout"], "stdout line");
        assert_eq!(result.output["stderr"], "stderr line");
        assert!(result.output["next_stdout_line"].is_number());
    }

    #[tokio::test]
    async fn recover_local_job_rejects_unsafe_job_id() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        // Path-traversal job ids must not reach the filesystem.
        let result = runtime.job_status("../escape".to_string()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown job"));
    }

    #[tokio::test]
    async fn recover_local_job_rejects_metadata_project_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "33333333-4444-5555-6666-777777777777";
        // Metadata claims project "other"; configured project is "demo".
        write_fake_job(
            root,
            job_id,
            "other",
            &root.to_string_lossy(),
            "running",
            "",
            "",
            json!({}),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(!result.success, "mismatched metadata must not be recovered");
    }

    #[tokio::test]
    async fn recover_local_job_rejects_metadata_path_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "44444444-5555-6666-7777-888888888888";
        // Metadata path points elsewhere even though project id matches.
        write_fake_job(
            root,
            job_id,
            "demo",
            "/some/other/path",
            "running",
            "",
            "",
            json!({}),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(
            !result.success,
            "mismatched metadata path must not be recovered"
        );
    }

    #[tokio::test]
    async fn recover_local_job_unknown_when_no_metadata_anywhere() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        let result = runtime
            .job_status("55555555-6666-7777-8888-999999999999".to_string())
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown job"));
    }

    #[tokio::test]
    async fn run_shell_failure_reports_command_started_and_output_tail() {
        let runtime = runtime_with_agent_project("shell-failer");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "shell-failer", None, caps).await;
        let project = agent_test_project_id("shell-failer");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .run_shell(
                    project,
                    "printf run-shell-out; printf run-shell-err >&2; exit 7".to_string(),
                    Some(30),
                    None,
                )
                .await
        });
        let req = next_patch_agent_request(&runtime, "shell-failer")
            .await
            .expect("run_shell should enqueue a shell command");
        complete_patch_agent_request(
            &runtime,
            "shell-failer",
            &req.request_id,
            7,
            "run-shell-out",
            "run-shell-err",
        )
        .await;
        let result = task.await.unwrap();
        assert!(!result.success);
        let error = result.error.as_deref().unwrap_or("");
        assert!(error.contains("Command exited with status 7"));
        assert!(error.contains("No files were modified by WebCodex itself"));
        assert!(error.contains("stdout_tail"));
        assert!(error.contains("stderr_tail"));
        assert!(error.contains("Retry guidance"));
        assert_eq!(result.output["exit_code"], 7);
        assert_eq!(result.output["stdout_tail"], "run-shell-out");
        assert_eq!(result.output["stderr_tail"], "run-shell-err");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], true);
        assert_eq!(result.output["command_ok"], false);
        assert_eq!(result.output["failure_kind"], "command_exit_nonzero");
        assert_eq!(result.output["tool_failure"], false);
    }

    #[tokio::test]
    async fn run_shell_rejection_reports_not_started_and_no_files_modified() {
        let result = test_runtime()
            .run_shell(
                "agent:missing:missing".to_string(),
                "printf should-not-run".to_string(),
                Some(30),
                None,
            )
            .await;
        assert!(!result.success);
        let error = result.error.as_deref().unwrap_or("");
        assert!(error.contains("Rejected before starting command"));
        assert!(error.contains("No command was started"));
        assert!(error.contains("No files were modified"));
        assert!(error.contains("Retry guidance"));
        assert_eq!(result.output["command_started"], false);
        assert_eq!(result.output["command_completed"], false);
        assert_eq!(result.output["command_ok"], false);
        assert_eq!(result.output["failure_kind"], "agent_offline");
        assert_eq!(result.output["tool_failure"], true);
    }

    #[tokio::test]
    async fn run_shell_exit_zero_reports_structured_command_success() {
        let runtime = runtime_with_agent_project("shell-ok");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "shell-ok", None, caps).await;
        let project = agent_test_project_id("shell-ok");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .run_shell(
                    project,
                    "printf ok; printf err >&2".to_string(),
                    Some(30),
                    None,
                )
                .await
        });
        let req = next_patch_agent_request(&runtime, "shell-ok")
            .await
            .expect("run_shell should enqueue a shell command");
        complete_patch_agent_request(&runtime, "shell-ok", &req.request_id, 0, "ok", "err").await;
        let result = task.await.unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["exit_code"], 0);
        assert_eq!(result.output["stdout"], "ok");
        assert_eq!(result.output["stderr"], "err");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], true);
        assert_eq!(result.output["command_ok"], true);
        assert!(result.output["failure_kind"].is_null());
        assert_eq!(result.output["tool_failure"], false);
    }

    #[tokio::test]
    async fn run_shell_exit_seven_reports_structured_command_nonzero() {
        let runtime = runtime_with_agent_project("shell-seven");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "shell-seven", None, caps).await;
        let project = agent_test_project_id("shell-seven");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .run_shell(
                    project,
                    "printf out; printf err >&2; exit 7".to_string(),
                    Some(30),
                    None,
                )
                .await
        });
        let req = next_patch_agent_request(&runtime, "shell-seven")
            .await
            .expect("run_shell should enqueue a shell command");
        complete_patch_agent_request(&runtime, "shell-seven", &req.request_id, 7, "out", "err")
            .await;
        let result = task.await.unwrap();

        assert!(!result.success);
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], true);
        assert_eq!(result.output["command_ok"], false);
        assert_eq!(result.output["exit_code"], 7);
        assert_eq!(result.output["failure_kind"], "command_exit_nonzero");
        assert_eq!(result.output["tool_failure"], false);
        assert_eq!(result.output["stdout_tail"], "out");
        assert_eq!(result.output["stderr_tail"], "err");
    }

    #[tokio::test]
    async fn run_shell_timeout_reports_structured_timeout_failure_kind() {
        let runtime = runtime_with_agent_project("shell-timeout");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "shell-timeout", None, caps).await;
        let project = agent_test_project_id("shell-timeout");
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .run_shell(project, "sleep 2".to_string(), Some(1), None)
                .await
        });
        let _req = next_patch_agent_request(&runtime, "shell-timeout")
            .await
            .expect("run_shell should enqueue a shell command");
        let result = task.await.unwrap();

        assert!(!result.success);
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], false);
        assert_eq!(result.output["command_ok"], false);
        assert!(result.output["exit_code"].is_null());
        assert_eq!(result.output["failure_kind"], "timeout");
        assert_eq!(result.output["tool_failure"], true);
    }

    #[tokio::test]
    async fn local_job_status_marks_over_time_running_job_lost() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "66666666-7777-8888-9999-000000000000";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            "",
            "",
            json!({ "started_at": past, "max_runtime_secs": 60 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success);
        assert_eq!(result.output["status"], "lost");
    }

    #[tokio::test]
    async fn local_job_status_keeps_completed_job_completed() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "77777777-8888-9999-0000-111111111111";
        let past = chrono::Utc::now().timestamp() - 100_000;
        // Completed jobs stay completed even if max_runtime would have passed.
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "completed",
            "",
            "",
            json!({ "started_at": past, "max_runtime_secs": 60 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success);
        assert_eq!(result.output["status"], "completed");
    }

    // =========================================================================
    // Phase 11: local job lifecycle hardening (process-group reclamation)
    // =========================================================================

    /// Test double for `LocalJobKiller` that records the (pid, pgid) pairs it
    /// was asked to terminate without touching any real process. Deterministic
    /// by construction — no real `kill` is invoked, so these tests never flake
    /// on process timing.
    #[derive(Default, Clone)]
    struct FakeJobKiller {
        calls: Arc<std::sync::Mutex<Vec<(i64, i64)>>>,
    }

    impl FakeJobKiller {
        fn calls(&self) -> Vec<(i64, i64)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl LocalJobKiller for FakeJobKiller {
        fn terminate_group(&self, pid: i64, pgid: i64) -> TerminateOutcome {
            self.calls.lock().unwrap().push((pid, pgid));
            // Fake pids are never alive; report AlreadyGone. The runtime still
            // persists a terminal status, which is what the tests assert.
            TerminateOutcome::AlreadyGone
        }
    }

    fn runtime_with_fake_killer(root: &Path, project_id: &str) -> (ToolRuntime, FakeJobKiller) {
        let mut runtime = runtime_with_project(root, project_id);
        let killer = FakeJobKiller::default();
        let killer_dyn: Arc<dyn LocalJobKiller> = Arc::new(killer.clone());
        runtime.job_killer = killer_dyn;
        (runtime, killer)
    }

    /// Write a fake on-disk local job plus a `pid` file and `process_group_id`
    /// metadata field, simulating a job spawned by the current code.
    fn write_fake_job_with_pgid(
        root: &Path,
        job_id: &str,
        project: &str,
        path: &str,
        status: &str,
        pid: i64,
        meta_extra: Value,
    ) -> PathBuf {
        let dir = write_fake_job(root, job_id, project, path, status, "", "", meta_extra);
        fs::write(dir.join("pid"), pid.to_string()).unwrap();
        dir
    }

    #[tokio::test]
    async fn run_job_rejects_server_configured_project_without_local_spawn() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let result = runtime
            .run_job("demo".to_string(), "true".to_string(), Some(10), None)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown_project"));
        assert!(runtime.local_jobs.lock().await.is_empty());
    }

    #[tokio::test]
    async fn timeout_terminates_recorded_process_group() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "12121212-3434-5656-7878-909090909090";
        let past = chrono::Utc::now().timestamp() - 100_000;
        let dir = write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            12345,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 12345 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "lost");
        assert!(result.output["note"]
            .as_str()
            .unwrap()
            .contains("process group 12345"));
        // The recorded pgid was targeted for termination.
        assert_eq!(killer.calls(), vec![(12345, 12345)]);
        // Terminal state persisted to disk.
        assert_eq!(read_trim(dir.join("status")).unwrap(), "lost");
        assert!(read_trim(dir.join("finished_at")).is_some());
    }

    #[tokio::test]
    async fn timeout_without_pid_only_marks_lost_no_kill() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "13131313-4545-6767-8989-101010101010";
        let past = chrono::Utc::now().timestamp() - 100_000;
        // No pid file, no process_group_id — simulates very old metadata that
        // predates pid/pgid tracking. We must NOT guess a pid to kill.
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            "",
            "",
            json!({ "started_at": past, "max_runtime_secs": 60 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "lost");
        // No kill attempted because no pid/pgid was recorded.
        assert!(killer.calls().is_empty());
        assert!(result.output["note"].as_str().unwrap().contains("no pid"));
    }

    #[tokio::test]
    async fn job_log_also_reclaims_timeout_process_group() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "14141414-5656-7878-9090-111111111111";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            4242,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 4242 }),
        );
        let result = runtime.job_log(job_id.to_string(), None, None).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "lost");
        assert_eq!(killer.calls(), vec![(4242, 4242)]);
    }

    #[tokio::test]
    async fn timeout_does_not_affect_completed_job() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "15151515-6767-8989-1010-121212121212";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "completed",
            9999,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 9999 }),
        );
        let result = runtime.job_status(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "completed");
        assert!(killer.calls().is_empty());
    }

    #[tokio::test]
    async fn stop_job_terminates_process_group() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "16161616-7878-9090-1111-131313131313";
        let now = chrono::Utc::now().timestamp();
        let dir = write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            7777,
            json!({ "started_at": now, "max_runtime_secs": 3600, "process_group_id": 7777 }),
        );
        let result = runtime.stop_job(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "stopped");
        assert_eq!(killer.calls(), vec![(7777, 7777)]);
        assert_eq!(read_trim(dir.join("status")).unwrap(), "stopped");
        assert!(read_trim(dir.join("finished_at")).is_some());
    }

    #[tokio::test]
    async fn stop_job_leaves_completed_job_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let (runtime, killer) = runtime_with_fake_killer(root, "demo");
        let job_id = "17171717-8989-1010-1212-141414141414";
        let past = chrono::Utc::now().timestamp() - 100_000;
        write_fake_job_with_pgid(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "completed",
            8888,
            json!({ "started_at": past, "max_runtime_secs": 60, "process_group_id": 8888 }),
        );
        let result = runtime.stop_job(job_id.to_string()).await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["status"], "completed");
        assert!(killer.calls().is_empty());
    }

    #[tokio::test]
    async fn stop_job_rejects_unsafe_job_id() {
        let tmp = tempfile::tempdir().unwrap();
        let (runtime, _killer) = runtime_with_fake_killer(tmp.path(), "demo");
        let result = runtime.stop_job("../escape".to_string()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("invalid job id"));
    }

    #[tokio::test]
    async fn stop_job_unknown_job_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (runtime, _killer) = runtime_with_fake_killer(tmp.path(), "demo");
        let result = runtime
            .stop_job("55555555-6666-7777-8888-999999999999".to_string())
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown job"));
    }

    #[tokio::test]
    async fn job_log_recovery_returns_bounded_output() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "88888888-9999-0000-1111-222222222222";
        let stdout = (1..=1000)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            &stdout,
            "",
            json!({}),
        );
        let result = runtime.job_log(job_id.to_string(), None, None).await;
        assert!(result.success);
        let out = result.output["stdout"].as_str().unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines.len() <= MAX_LOCAL_LOG_LINES);
        assert!(out.contains("line 1000"));
        assert!(!out.contains("line 1\n"));
    }

    #[tokio::test]
    async fn job_log_recovery_paginates_with_offset() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        let job_id = "99999999-0000-1111-2222-333333333333";
        let stdout = (1..=600)
            .map(|i| format!("line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        write_fake_job(
            root,
            job_id,
            "demo",
            &root.to_string_lossy(),
            "running",
            &stdout,
            "",
            json!({}),
        );
        let first = runtime.job_log(job_id.to_string(), Some(1), None).await;
        assert!(first.success);
        let out = first.output["stdout"].as_str().unwrap();
        assert!(out.contains("line 1"));
        assert!(out.contains("line 500"));
        assert!(!out.contains("line 501"));
        assert_eq!(first.output["next_stdout_line"], 501);

        let second = runtime.job_log(job_id.to_string(), Some(501), None).await;
        assert!(second.success);
        let out2 = second.output["stdout"].as_str().unwrap();
        assert!(out2.contains("line 501"));
        assert!(out2.contains("line 600"));
        assert_eq!(second.output["next_stdout_line"], 601);
    }

    // =========================================================================
    // Phase 3: harden run_codex — command construction, validation, output
    // =========================================================================

    fn codex_config_with_allowlist(allowlist: &[&str]) -> CodexConfig {
        CodexConfig {
            bin: "codex".to_string(),
            approval_mode: String::new(),
            default_timeout_secs: 3600,
            max_prompt_bytes: 100_000,
            allowed_extra_args: allowlist.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn runtime_with_codex(root: &Path, codex: CodexConfig) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            "demo".to_string(),
            local_project_config(&root.to_string_lossy()),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(codex),
            Arc::new(RuntimeInfo::default()),
        )
    }

    #[test]
    fn build_codex_command_uses_default_bin_and_approval_mode() {
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "fix tests", None, None).unwrap();
        // Default approval_mode is disabled (empty), so --approval-mode is not
        // emitted. This keeps the runtime compatible with Codex CLI builds
        // that do not support the flag.
        assert!(
            !cmd.contains("--approval-mode"),
            "default command must not include --approval-mode, got: {}",
            cmd
        );
        assert!(cmd.starts_with("'codex' "));
        assert!(cmd.ends_with("'fix tests'"));
    }

    #[test]
    fn build_codex_command_uses_configured_bin_and_approval_mode() {
        let codex = CodexConfig {
            bin: "/usr/local/bin/codex".to_string(),
            approval_mode: "suggest".to_string(),
            default_timeout_secs: 3600,
            max_prompt_bytes: 100_000,
            allowed_extra_args: vec![],
        };
        let cmd = build_codex_command(&codex, "hello", None, None).unwrap();
        assert!(cmd.starts_with("'/usr/local/bin/codex' --approval-mode 'suggest' "));
    }

    #[test]
    fn build_codex_command_config_suggest_emits_flag() {
        // CODEX_APPROVAL_MODE=suggest should include --approval-mode suggest.
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", None, None).unwrap();
        assert!(cmd.contains("--approval-mode 'suggest'"));
    }

    #[test]
    fn build_codex_command_config_none_omits_flag() {
        // CODEX_APPROVAL_MODE=none must not emit --approval-mode.
        for value in ["none", "off", "disabled", "NONE", "Off"] {
            let codex = CodexConfig {
                approval_mode: value.to_string(),
                ..CodexConfig::default()
            };
            let cmd = build_codex_command(&codex, "hi", None, None).unwrap();
            assert!(
                !cmd.contains("--approval-mode"),
                "CODEX_APPROVAL_MODE={:?} should omit --approval-mode, got: {}",
                value,
                cmd
            );
        }
    }

    #[test]
    fn build_codex_command_request_approval_mode_overrides_config() {
        // A config with suggest is overridden by an explicit request value.
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", Some("full-auto"), None).unwrap();
        assert!(cmd.contains("--approval-mode 'full-auto'"));
        assert!(!cmd.contains("'suggest'"));
    }

    #[test]
    fn build_codex_command_request_approval_mode_none_omits_flag() {
        // request approval_mode=none overrides a non-empty config and omits the
        // flag entirely.
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        for value in ["none", "off", "disabled", ""] {
            let cmd = build_codex_command(&codex, "hi", Some(value), None).unwrap();
            assert!(
                !cmd.contains("--approval-mode"),
                "request approval_mode={:?} should omit --approval-mode, got: {}",
                value,
                cmd
            );
        }
    }

    #[test]
    fn build_codex_command_request_approval_mode_blank_omits_flag() {
        // A blank request value means disabled (not "fall back to config").
        let codex = CodexConfig {
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", Some("   "), None).unwrap();
        assert!(!cmd.contains("--approval-mode"));
    }

    #[test]
    fn build_codex_command_shell_escapes_prompt() {
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "rm -rf /'; echo pwned", None, None).unwrap();
        // The single quote in the prompt must be escaped with '\'\'',
        // preventing the trailing "; echo pwned" from running as a command.
        assert!(cmd.contains("'\\''"));
        // The whole prompt is wrapped in single quotes, so the semicolon is
        // literal, not a command separator.
        assert!(cmd.contains("'; echo pwned'"));
    }

    #[test]
    fn build_codex_command_rejects_empty_prompt_via_validate() {
        // build_codex_command itself does not check emptiness (run_codex does),
        // but an empty prompt still gets escaped. Verify it doesn't panic.
        let codex = CodexConfig::default();
        let cmd = build_codex_command(&codex, "", None, None).unwrap();
        // Empty prompt produces a trailing ''.
        assert!(cmd.ends_with(" ''"));
    }

    #[test]
    fn build_codex_command_rejects_extra_args_by_default() {
        let codex = CodexConfig::default(); // empty allowlist
        let err = build_codex_command(&codex, "hi", None, Some(vec!["--verbose".to_string()]))
            .unwrap_err();
        assert!(err.contains("allowlist"));
        assert!(err.contains("--verbose"));
    }

    #[test]
    fn build_codex_command_allows_allowlisted_extra_args() {
        let codex = codex_config_with_allowlist(&["--verbose", "--json"]);
        let cmd = build_codex_command(
            &codex,
            "hi",
            None,
            Some(vec!["--verbose".to_string(), "--json".to_string()]),
        )
        .unwrap();
        assert!(cmd.contains("'--verbose'"));
        assert!(cmd.contains("'--json'"));
    }

    #[test]
    fn build_codex_command_rejects_non_allowlisted_extra_args() {
        let codex = codex_config_with_allowlist(&["--verbose"]);
        let err = build_codex_command(&codex, "hi", None, Some(vec!["--danger".to_string()]))
            .unwrap_err();
        assert!(err.contains("allowlist"));
        assert!(err.contains("--danger"));
    }

    #[test]
    fn build_codex_command_rejects_nul_in_extra_arg() {
        let codex = codex_config_with_allowlist(&["--verbose"]);
        let err = build_codex_command(&codex, "hi", None, Some(vec!["--ver\0bose".to_string()]))
            .unwrap_err();
        assert!(err.contains("NUL"));
    }

    #[test]
    fn build_codex_command_rejects_too_many_extra_args() {
        let allowed: Vec<String> = (0..40).map(|i| format!("--a{}", i)).collect();
        let codex = CodexConfig {
            allowed_extra_args: allowed.clone(),
            ..CodexConfig::default()
        };
        let too_many: Vec<String> = allowed;
        let err = build_codex_command(&codex, "hi", None, Some(too_many)).unwrap_err();
        assert!(err.contains("at most 32"));
    }

    #[tokio::test]
    async fn run_codex_rejects_empty_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "   ".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn run_codex_rejects_nul_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "fix\0tests".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn run_codex_rejects_oversized_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let codex = CodexConfig {
            max_prompt_bytes: 16,
            ..CodexConfig::default()
        };
        let runtime = runtime_with_codex(tmp.path(), codex);
        let big = "x".repeat(100);
        let result = runtime
            .run_codex("demo".to_string(), big, None, None, None, None)
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("too large"));
        assert!(err.contains("16"));
    }

    #[tokio::test]
    async fn run_codex_rejects_nul_approval_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "fix tests".to_string(),
                Some("full\0auto".to_string()),
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn run_codex_rejects_extra_args_without_allowlist() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "fix tests".to_string(),
                None,
                None,
                None,
                Some(vec!["--verbose".to_string()]),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("allowlist"));
    }

    #[tokio::test]
    async fn run_codex_agent_output_contains_structured_fields() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", None, caps).await;
        let project = agent_test_project_id("oe");
        let result = runtime
            .run_codex(
                project.clone(),
                "echo hello".to_string(),
                None,
                Some(10),
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert!(result.output["job_id"].is_string());
        assert_eq!(result.output["kind"], "codex");
        assert_eq!(result.output["project"], project);
        assert_eq!(result.output["status_endpoint"], "/api/jobs/status");
        assert_eq!(result.output["log_endpoint"], "/api/jobs/log");
        assert!(
            runtime.local_jobs.lock().await.is_empty(),
            "agent-backed Codex jobs must not create server-local job metadata"
        );
    }

    #[tokio::test]
    async fn run_codex_rejects_server_configured_project() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_codex(root, CodexConfig::default());
        let result = runtime
            .run_codex(
                "demo".to_string(),
                "echo hello".to_string(),
                None,
                Some(10),
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown_project"));
        assert!(runtime.local_jobs.lock().await.is_empty());
    }

    #[tokio::test]
    async fn run_codex_agent_uses_configured_command_builder() {
        let codex = CodexConfig {
            default_timeout_secs: 42,
            approval_mode: "suggest".to_string(),
            ..CodexConfig::default()
        };
        let mut runtime = runtime_with_agent_project("oe");
        runtime.codex = Arc::new(codex);
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", None, caps).await;
        let result = runtime
            .run_codex(
                agent_test_project_id("oe"),
                "echo hi".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        let jobs = runtime.shell_clients.list_jobs(None).await;
        assert_eq!(jobs.len(), 1);
        // The configured approval_mode flows through build_codex_command into
        // the agent job's command preview.
        assert!(
            jobs[0]
                .command_preview
                .contains("--approval-mode 'suggest'"),
            "{}",
            jobs[0].command_preview
        );
    }

    #[tokio::test]
    async fn run_codex_agent_omits_approval_mode_when_disabled() {
        // Default (disabled) approval_mode must not emit --approval-mode.
        let codex = CodexConfig::default();
        let mut runtime = runtime_with_agent_project("om");
        runtime.codex = Arc::new(codex);
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "om", None, caps).await;
        let result = runtime
            .run_codex(
                agent_test_project_id("om"),
                "echo hi".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        let jobs = runtime.shell_clients.list_jobs(None).await;
        assert_eq!(jobs.len(), 1);
        assert!(
            !jobs[0].command_preview.contains("--approval-mode"),
            "disabled approval_mode must omit --approval-mode, got: {}",
            jobs[0].command_preview
        );
    }

    // =========================================================================
    // Phase 6: agent capability checks, owner boundary, structured errors
    // =========================================================================

    use crate::shell_protocol::{
        ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
        ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
    };

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
            user_id: username.map(|u| format!("user-{}", u)),
            username: username.map(str::to_string),
            api_key_id: username.map(|u| format!("key-{}", u)),
            api_key_name: username.map(|u| format!("{} key", u)),
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

    fn agent_project_config(path: &str, client_id: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Agent,
            client_id: Some(client_id.to_string()),
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    fn runtime_with_agent_project(client_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            "agent-proj".to_string(),
            agent_project_config("/tmp/agent-proj", client_id),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    async fn register_agent(
        runtime: &ToolRuntime,
        client_id: &str,
        owner: Option<&str>,
        caps: ShellClientCapabilities,
    ) {
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: owner.map(str::to_string),
                hostname: None,
                capabilities: Some(caps),
                projects: Some(vec![registered_project("agent-proj", "/tmp/agent-proj")]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
    }

    fn agent_test_project_id(client_id: &str) -> String {
        ToolRuntime::agent_project_runtime_id(client_id, "agent-proj")
    }

    /// Build a ToolRuntime backed by a single server-configured (local) project
    /// rooted at `root`. Used to assert the runtime surface rejects
    /// server-configured projects in favor of agent-registered ones.
    fn runtime_with_local_project(root: &Path, project_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            project_id.to_string(),
            ProjectConfig {
                path: root.to_string_lossy().to_string(),
                executor: Executor::Local,
                client_id: None,
                allow_patch: true,
                allow_command_requests: false,
                allow_raw_command_requests: false,
                default_apply_patch_backend: None,
                allowed_checks: Vec::new(),
                checks: None,
                commands: HashMap::new(),
                hooks: HashMap::new(),
            },
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    fn registered_project(id: &str, path: &str) -> ShellAgentProjectSummary {
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(id.to_string()),
            path: path.to_string(),
            allow_patch: true,
            kind: Some("repo".to_string()),
            description: None,
            hooks: Vec::new(),
            disabled: false,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at: 123,
            shell_profile: None,
        }
    }

    fn named_registered_project(
        client_id: &str,
        id: &str,
        name: &str,
        path: &str,
        updated_at: i64,
    ) -> ShellAgentProjectSummary {
        let _ = client_id;
        ShellAgentProjectSummary {
            id: id.to_string(),
            name: Some(name.to_string()),
            path: path.to_string(),
            allow_patch: true,
            kind: Some("repo".to_string()),
            description: None,
            hooks: Vec::new(),
            disabled: false,
            git_branch: None,
            git_head: None,
            git_dirty: None,
            updated_at,
            shell_profile: None,
        }
    }

    async fn register_agent_projects(
        runtime: &ToolRuntime,
        client_id: &str,
        owner: Option<&str>,
        caps: ShellClientCapabilities,
        projects: Vec<ShellAgentProjectSummary>,
    ) {
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: format!("inst-{}", client_id),
                display_name: None,
                owner: owner.map(str::to_string),
                hostname: None,
                capabilities: Some(caps),
                projects: Some(projects),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
    }

    async fn next_agent_request_for_client(
        runtime: &ToolRuntime,
        client_id: &str,
    ) -> Option<ShellAgentShellRequest> {
        next_agent_request_for_instance(runtime, client_id, &format!("inst-{}", client_id)).await
    }

    async fn next_agent_request_for_instance(
        runtime: &ToolRuntime,
        client_id: &str,
        agent_instance_id: &str,
    ) -> Option<ShellAgentShellRequest> {
        for _ in 0..20 {
            let req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: client_id.to_string(),
                    agent_instance_id: agent_instance_id.to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if req.is_some() {
                return req;
            }
            tokio::task::yield_now().await;
        }
        None
    }

    async fn runtime_with_resolver_projects() -> ToolRuntime {
        let runtime = test_runtime();
        let mut file_caps = ShellClientCapabilities::default();
        file_caps.file_read = true;
        file_caps.git = true;
        file_caps.shell = true;
        register_agent_projects(
            &runtime,
            "workstation",
            None,
            file_caps.clone(),
            vec![
                named_registered_project(
                    "workstation",
                    "my-repo",
                    "My Repo",
                    "/root/git/workstation-my-repo",
                    200,
                ),
                named_registered_project(
                    "workstation",
                    "other-repo",
                    "Other Repo",
                    "/root/git/workstation-other-repo",
                    210,
                ),
            ],
        )
        .await;
        register_agent_projects(
            &runtime,
            "laptop",
            None,
            file_caps,
            vec![named_registered_project(
                "laptop",
                "my-repo",
                "My Repo",
                "/root/git/laptop-my-repo",
                190,
            )],
        )
        .await;
        runtime
    }

    #[tokio::test]
    async fn apply_patch_agent_does_not_require_server_local_project_root() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: "patcher".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(caps),
                projects: Some(vec![registered_project(
                    "agent-proj",
                    "/definitely/not/on/server/webcodex-agent-only",
                )]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();

        let project = agent_test_project_id("patcher");
        let patch = "diff --git a/REMOTE_ONLY.md b/REMOTE_ONLY.md\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/REMOTE_ONLY.md\n\
@@ -0,0 +1 @@\n\
+remote\n"
            .to_string();
        let runtime_for_task = runtime.clone();
        let apply_task =
            tokio::spawn(async move { runtime_for_task.apply_patch(project, patch).await });

        let mut check_req = None;
        for _ in 0..10 {
            check_req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "patcher".to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if check_req.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let check_req =
            check_req.expect("apply_patch should enqueue git apply --check for the agent");
        assert_eq!(check_req.command, "git apply --check - && echo OK");
        assert!(check_req
            .stdin
            .as_deref()
            .unwrap_or("")
            .contains("REMOTE_ONLY.md"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "patcher".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: check_req.request_id,
                exit_code: Some(0),
                stdout: Some("OK\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();

        let mut apply_req = None;
        for _ in 0..10 {
            apply_req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "patcher".to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if apply_req.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let apply_req = apply_req.expect("apply_patch should enqueue git apply for the agent");
        assert_eq!(apply_req.command, "git apply -");
        assert!(apply_req
            .stdin
            .as_deref()
            .unwrap_or("")
            .contains("REMOTE_ONLY.md"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "patcher".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: apply_req.request_id,
                exit_code: Some(0),
                stdout: Some(String::new()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();

        let result = apply_task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["success"], true);
        assert!(result.output["changed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v.as_str() == Some("REMOTE_ONLY.md")));
    }

    #[tokio::test]
    async fn project_resolver_resolves_full_id() {
        let runtime = runtime_with_resolver_projects().await;
        let resolved = runtime
            .resolve_project_input("agent:workstation:my-repo")
            .await
            .unwrap();
        assert_eq!(resolved.resolved_id, "agent:workstation:my-repo");
        assert_eq!(resolved.config.agent_client_id().unwrap(), "workstation");
        assert_eq!(resolved.config.path, "/root/git/workstation-my-repo");
    }

    #[tokio::test]
    async fn project_resolver_resolves_client_project_shorthand() {
        let runtime = runtime_with_resolver_projects().await;
        let resolved = runtime
            .resolve_project_input("workstation:my-repo")
            .await
            .unwrap();
        assert_eq!(resolved.resolved_id, "agent:workstation:my-repo");
    }

    #[tokio::test]
    async fn project_resolver_resolves_unique_short_id() {
        let runtime = runtime_with_resolver_projects().await;
        let resolved = runtime.resolve_project_input("other-repo").await.unwrap();
        assert_eq!(resolved.resolved_id, "agent:workstation:other-repo");
    }

    #[tokio::test]
    async fn project_resolver_ambiguous_short_id_returns_candidates() {
        let runtime = runtime_with_resolver_projects().await;
        let err = runtime.resolve_project_input("my-repo").await.unwrap_err();
        assert_eq!(err.kind, ProjectResolverErrorKind::AmbiguousProject);
        assert_eq!(err.project, "my-repo");
        let ids: Vec<String> = err
            .candidates
            .iter()
            .map(|candidate| candidate.id.clone())
            .collect();
        assert_eq!(
            ids,
            vec![
                "agent:laptop:my-repo".to_string(),
                "agent:workstation:my-repo".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn project_resolver_unknown_id_returns_candidates() {
        let runtime = runtime_with_resolver_projects().await;
        let err = runtime
            .resolve_project_input("missing-repo")
            .await
            .unwrap_err();
        assert_eq!(err.kind, ProjectResolverErrorKind::UnknownProject);
        assert_eq!(err.project, "missing-repo");
        assert!(err.candidates.len() >= 3);
        assert!(err
            .candidates
            .iter()
            .any(|candidate| candidate.id == "agent:workstation:other-repo"));
    }

    #[tokio::test]
    async fn start_session_without_project_is_allowed() {
        let runtime = test_runtime();
        let result = runtime
            .dispatch_with_auth(
                ToolCall::StartSession {
                    project: None,
                    title: Some("probe".to_string()),
                    mode: SessionMode::Normal,
                    deny_write_tools: false,
                    deny_shell_tools: false,
                },
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["project"], Value::Null);
        assert_eq!(result.output["project_input"], Value::Null);
        assert_eq!(result.output["resolved_project"], Value::Null);
    }

    #[tokio::test]
    async fn start_session_valid_full_id_stores_resolved_project() {
        let runtime = runtime_with_resolver_projects().await;
        let result = runtime
            .dispatch_with_auth(
                ToolCall::StartSession {
                    project: Some("agent:workstation:my-repo".to_string()),
                    title: Some("probe".to_string()),
                    mode: SessionMode::Normal,
                    deny_write_tools: false,
                    deny_shell_tools: false,
                },
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["project"], "agent:workstation:my-repo");
        assert_eq!(result.output["project_input"], "agent:workstation:my-repo");
        assert_eq!(
            result.output["resolved_project"],
            "agent:workstation:my-repo"
        );
    }

    #[tokio::test]
    async fn start_session_valid_short_id_stores_resolved_project() {
        let runtime = runtime_with_resolver_projects().await;
        let result = runtime
            .dispatch_with_auth(
                ToolCall::StartSession {
                    project: Some("other-repo".to_string()),
                    title: Some("probe".to_string()),
                    mode: SessionMode::Normal,
                    deny_write_tools: false,
                    deny_shell_tools: false,
                },
                None,
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["project"], "agent:workstation:other-repo");
        assert_eq!(result.output["project_input"], "other-repo");
        assert_eq!(
            result.output["resolved_project"],
            "agent:workstation:other-repo"
        );
    }

    #[tokio::test]
    async fn start_session_ambiguous_project_fails_with_candidates() {
        let runtime = runtime_with_resolver_projects().await;
        let result = runtime
            .dispatch_with_auth(
                ToolCall::StartSession {
                    project: Some("my-repo".to_string()),
                    title: Some("probe".to_string()),
                    mode: SessionMode::Normal,
                    deny_write_tools: false,
                    deny_shell_tools: false,
                },
                None,
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "ambiguous_project");
        let candidates = result.output["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0]["id"], "agent:laptop:my-repo");
        assert_eq!(candidates[1]["id"], "agent:workstation:my-repo");
    }

    #[tokio::test]
    async fn start_session_unknown_project_fails_with_candidates() {
        let runtime = runtime_with_resolver_projects().await;
        let result = runtime
            .dispatch_with_auth(
                ToolCall::StartSession {
                    project: Some("missing-repo".to_string()),
                    title: Some("probe".to_string()),
                    mode: SessionMode::Normal,
                    deny_write_tools: false,
                    deny_shell_tools: false,
                },
                None,
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "unknown_project");
        assert_eq!(result.output["project"], "missing-repo");
        assert!(result.output["candidates"].as_array().unwrap().len() >= 3);
    }

    #[tokio::test]
    async fn read_file_accepts_unique_short_id() {
        let runtime = runtime_with_resolver_projects().await;
        let bootstrap = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project: "other-repo".to_string(),
                            path: "README.md".to_string(),
                            session_id: None,
                            start_line: None,
                            limit: None,
                            with_line_numbers: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_client(&runtime, "workstation")
            .await
            .expect("read_file should enqueue an agent file_read request");
        assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "workstation".to_string(),
                agent_instance_id: "inst-workstation".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
    }

    #[tokio::test]
    async fn git_status_accepts_unique_short_id() {
        let runtime = runtime_with_resolver_projects().await;
        let bootstrap = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::GitStatus {
                            project: "other-repo".to_string(),
                            session_id: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_client(&runtime, "workstation")
            .await
            .expect("git_status should enqueue an agent shell request");
        assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "workstation".to_string(),
                agent_instance_id: "inst-workstation".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(String::new()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
    }

    #[tokio::test]
    async fn show_changes_accepts_unique_short_id() {
        let runtime = runtime_with_resolver_projects().await;
        let bootstrap = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::ShowChanges {
                            project: "other-repo".to_string(),
                            session_id: None,
                            include_diff: Some(false),
                            max_hunks: None,
                            max_hunk_lines: None,
                            session_event_limit: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_client(&runtime, "workstation")
            .await
            .expect("show_changes should enqueue an agent shell request");
        assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
        let stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0head\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "workstation".to_string(),
                agent_instance_id: "inst-workstation".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(stdout.to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["project"], "other-repo");
    }

    #[tokio::test]
    async fn ambiguous_short_id_returns_candidates_for_project_tools() {
        let runtime = runtime_with_resolver_projects().await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ReadFile {
                    project: "my-repo".to_string(),
                    path: "README.md".to_string(),
                    session_id: None,
                    start_line: None,
                    limit: None,
                    with_line_numbers: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "ambiguous_project");
        assert_eq!(result.output["project"], "my-repo");
        assert_eq!(result.output["candidates"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn full_id_remains_compatible_for_project_tools() {
        let runtime = runtime_with_resolver_projects().await;
        let bootstrap = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::ReadFile {
                            project: "agent:workstation:other-repo".to_string(),
                            path: "README.md".to_string(),
                            session_id: None,
                            start_line: None,
                            limit: None,
                            with_line_numbers: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_client(&runtime, "workstation")
            .await
            .expect("full id should still enqueue an agent request");
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "workstation".to_string(),
                agent_instance_id: "inst-workstation".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some("hello\n".to_string()),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
    }

    // -------------------------------------------------------------------------
    // Patch chain hardening (agent-backed apply/validate invariants)
    // -------------------------------------------------------------------------
    //
    // These tests pin the agent-backed patch execution invariants:
    //   * patch content travels over `ShellRunRequest.stdin`, never inside the
    //     command string (no `echo <patch>`, no heredoc, no raw interpolation);
    //   * the working directory is supplied via the shell request `cwd` field,
    //     never via a `cd <path> && ...` prefix in the command;
    //   * `apply_patch_checked` checks before applying and skips the apply step
    //     when the preflight fails (no partial application);
    //   * `validate_patch` only ever enqueues read-only `git apply --check` /
    //     `--stat` commands, never a bare mutating `git apply -`;
    //   * server-configured (non-agent) projects are rejected by every patch
    //     tool, so the server never touches the filesystem directly.

    async fn next_patch_agent_request(
        runtime: &ToolRuntime,
        client_id: &str,
    ) -> Option<ShellAgentShellRequest> {
        for _ in 0..20 {
            let req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: client_id.to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if req.is_some() {
                return req;
            }
            tokio::task::yield_now().await;
        }
        None
    }

    async fn complete_patch_agent_request(
        runtime: &ToolRuntime,
        client_id: &str,
        request_id: &str,
        exit_code: i32,
        stdout: &str,
        stderr: &str,
    ) {
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: request_id.to_string(),
                exit_code: Some(exit_code),
                stdout: Some(stdout.to_string()),
                stderr: Some(stderr.to_string()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    }

    /// A small patch carrying a distinctive marker line so tests can prove the
    /// patch body never leaks into the shell `command` string.
    fn marker_patch(filename: &str, marker: &str) -> String {
        format!(
            "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n\
             @@ -0,0 +1 @@\n+{m}\n",
            f = filename,
            m = marker,
        )
    }

    /// A patch deliberately larger than the agent shell command limit
    /// (`MAX_COMMAND_LEN` = 8000 bytes) so tests can prove the patch still
    /// validates/applies via `stdin` rather than the command string.
    fn large_marker_patch(filename: &str, marker: &str) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "diff --git a/{f} b/{f}\nnew file mode 100644\n--- /dev/null\n+++ b/{f}\n\
             @@ -0,0 +1,200 @@\n",
            f = filename,
        ));
        s.push_str(&format!("+{m}\n", m = marker));
        for i in 0..199 {
            s.push_str(&format!("+line-{:04}-{}\n", i, "x".repeat(48)));
        }
        s
    }

    /// Assert a patch-related agent command is one of the fixed, known-safe
    /// invocations and never carries patch content, a `cd` prefix, a heredoc,
    /// or an `echo`/`cat` splice of the patch body.
    fn assert_safe_patch_command(command: &str, marker: &str) {
        let allowed = [
            "git apply --check -",
            "git apply --check - && echo OK",
            "git apply --stat -",
            "git apply -",
        ];
        assert!(
            allowed.contains(&command),
            "unexpected patch command (must be a fixed git apply invocation): {}",
            command
        );
        assert!(
            !command.contains(marker),
            "patch content leaked into command: {}",
            command
        );
        assert!(
            !command.contains("cd "),
            "command must not use a cd prefix (cwd is supplied via the shell request): {}",
            command
        );
        assert!(
            !command.contains("<<"),
            "command must not use a heredoc: {}",
            command
        );
        // The only permitted `echo` is the fixed `echo OK` success marker; it
        // never carries patch content. `cat` must never appear (no splicing).
        if command.contains("echo ") {
            assert_eq!(command, "git apply --check - && echo OK");
        }
        assert!(
            !command.contains("cat "),
            "command must not splice the patch via cat: {}",
            command
        );
    }

    #[tokio::test]
    async fn apply_patch_agent_command_excludes_patch_content_and_uses_stdin_and_cwd() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "patcher", None, caps).await;

        let project = agent_test_project_id("patcher");
        let marker = "ZZZ_PATCH_MARKER_APPLY_ZZZ";
        let patch = marker_patch("APPLY_MARKER.md", marker);
        let runtime_for_task = runtime.clone();
        let patch_for_apply = patch.clone();
        let apply_task =
            tokio::spawn(
                async move { runtime_for_task.apply_patch(project, patch_for_apply).await },
            );

        // 1) preflight check: `git apply --check - && echo OK`
        let check_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("apply_patch should enqueue a git apply --check request");
        assert_safe_patch_command(&check_req.command, marker);
        assert_eq!(check_req.command, "git apply --check - && echo OK");
        assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
        assert_eq!(check_req.cwd.as_deref(), Some("/tmp/agent-proj"));
        complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 0, "OK\n", "")
            .await;

        // 2) apply: `git apply -`
        let apply_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("apply_patch should enqueue a git apply request");
        assert_safe_patch_command(&apply_req.command, marker);
        assert_eq!(apply_req.command, "git apply -");
        assert_eq!(apply_req.stdin.as_deref(), Some(patch.as_str()));
        assert_eq!(apply_req.cwd.as_deref(), Some("/tmp/agent-proj"));
        complete_patch_agent_request(&runtime, "patcher", &apply_req.request_id, 0, "", "").await;

        let result = apply_task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["success"], true);
    }

    #[tokio::test]
    async fn apply_patch_rejects_nul_byte_patch() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "patcher", None, caps).await;
        let project = agent_test_project_id("patcher");
        let patch = "diff --git a/A b/A\n--- a/A\n+++ b/A\n@@ -1 +1 @@\n-a\n\0+b\n";
        let result = runtime.apply_patch(project, patch.to_string()).await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn apply_patch_checked_does_not_apply_when_check_fails() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "patcher", None, caps).await;

        let project = agent_test_project_id("patcher");
        let marker = "ZZZ_PATCH_MARKER_CHECKFAIL_ZZZ";
        let patch = marker_patch("CHECKFAIL_PROBE.md", marker);
        let runtime_for_task = runtime.clone();
        let patch_for_task = patch.clone();
        let checked_task = tokio::spawn(async move {
            runtime_for_task
                .apply_patch_checked(project, patch_for_task, Some(true))
                .await
        });

        // 1) validate preflight check: fails (exit 1) -> can_apply=false.
        let check_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("apply_patch_checked should enqueue a validate check request");
        assert_safe_patch_command(&check_req.command, marker);
        assert_eq!(check_req.command, "git apply --check -");
        assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
        complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 1, "", "bad")
            .await;

        // 2) validate stat summary still runs (read-only, regardless of can_apply).
        let stat_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("validate_patch should enqueue a git apply --stat request");
        assert_safe_patch_command(&stat_req.command, marker);
        assert_eq!(stat_req.command, "git apply --stat -");
        complete_patch_agent_request(&runtime, "patcher", &stat_req.request_id, 0, "stat", "")
            .await;

        // 3) No apply step must be enqueued because the preflight failed.
        let leaked_apply = next_patch_agent_request(&runtime, "patcher").await;
        assert!(
            leaked_apply.is_none(),
            "apply_patch_checked must not apply when the check fails (got: {:?})",
            leaked_apply.map(|r| r.command)
        );

        let result = checked_task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["applied"], false);
        assert_eq!(result.output["validate"]["can_apply"], false);
    }

    #[tokio::test]
    async fn apply_patch_checked_applies_large_patch_over_command_limit_via_stdin() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "patcher", None, caps).await;

        let project = agent_test_project_id("patcher");
        let marker = "ZZZ_PATCH_MARKER_LARGE_CHECKED_ZZZ";
        let patch = large_marker_patch("LARGE_CHECKED_PROBE.md", marker);
        // Prove the patch exceeds the agent shell command length limit; it must
        // still validate + apply because it travels over stdin, not the command.
        assert!(patch.len() > 8_000, "patch must exceed command limit");
        assert!(patch.len() <= MAX_VALIDATE_PATCH_BYTES);

        let runtime_for_task = runtime.clone();
        let patch_for_task = patch.clone();
        let checked_task = tokio::spawn(async move {
            runtime_for_task
                .apply_patch_checked(project, patch_for_task, Some(true))
                .await
        });

        // 1) validate check.
        let check_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("validate check request");
        assert_safe_patch_command(&check_req.command, marker);
        assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
        complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 0, "", "").await;

        // 2) validate stat.
        let stat_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("validate stat request");
        assert_safe_patch_command(&stat_req.command, marker);
        complete_patch_agent_request(&runtime, "patcher", &stat_req.request_id, 0, "stat", "")
            .await;

        // 3) apply preflight check.
        let apply_check_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("apply check request");
        assert_safe_patch_command(&apply_check_req.command, marker);
        assert_eq!(apply_check_req.command, "git apply --check - && echo OK");
        assert_eq!(apply_check_req.stdin.as_deref(), Some(patch.as_str()));
        complete_patch_agent_request(
            &runtime,
            "patcher",
            &apply_check_req.request_id,
            0,
            "OK\n",
            "",
        )
        .await;

        // 4) apply.
        let apply_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("apply request");
        assert_safe_patch_command(&apply_req.command, marker);
        assert_eq!(apply_req.command, "git apply -");
        assert_eq!(apply_req.stdin.as_deref(), Some(patch.as_str()));
        complete_patch_agent_request(&runtime, "patcher", &apply_req.request_id, 0, "", "").await;

        // 5) post-apply git_diff_summary (drain + complete generically).
        if let Some(diff_req) = next_patch_agent_request(&runtime, "patcher").await {
            complete_patch_agent_request(&runtime, "patcher", &diff_req.request_id, 0, "", "")
                .await;
        }

        let result = checked_task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["applied"], true);
        assert_eq!(result.output["validate"]["can_apply"], true);
    }

    #[tokio::test]
    async fn validate_patch_never_enqueues_mutating_apply_command() {
        let runtime = runtime_with_agent_project("patcher");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "patcher", None, caps).await;

        let project = agent_test_project_id("patcher");
        let marker = "ZZZ_PATCH_MARKER_VALIDATE_ZZZ";
        let patch = marker_patch("VALIDATE_MARKER.md", marker);
        let runtime_for_task = runtime.clone();
        let patch_for_task = patch.clone();
        let validate_task = tokio::spawn(async move {
            runtime_for_task
                .validate_patch(project, patch_for_task, None)
                .await
        });

        // 1) `git apply --check -` (read-only applicability test).
        let check_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("validate_patch should enqueue a check request");
        assert_safe_patch_command(&check_req.command, marker);
        assert_eq!(check_req.command, "git apply --check -");
        assert_ne!(check_req.command, "git apply -");
        assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
        complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 0, "", "").await;

        // 2) `git apply --stat -` (read-only summary).
        let stat_req = next_patch_agent_request(&runtime, "patcher")
            .await
            .expect("validate_patch should enqueue a stat request");
        assert_safe_patch_command(&stat_req.command, marker);
        assert_eq!(stat_req.command, "git apply --stat -");
        complete_patch_agent_request(&runtime, "patcher", &stat_req.request_id, 0, "stat", "")
            .await;

        // 3) No mutating apply must be enqueued — validate_patch is dry-run only.
        let leaked_apply = next_patch_agent_request(&runtime, "patcher").await;
        assert!(
            leaked_apply.is_none(),
            "validate_patch enqueued a mutating command (got: {:?})",
            leaked_apply.map(|r| r.command)
        );

        let result = validate_task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["can_apply"], true);
    }

    #[tokio::test]
    async fn patch_tools_reject_server_configured_project() {
        // A server-configured (local) project must NOT be a runtime surface for
        // any patch tool: the server never reads/writes its filesystem directly.
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_local_project(tmp.path(), "local-proj");
        let patch = marker_patch("LOCAL_PROBE.md", "marker");

        let apply = runtime
            .apply_patch("local-proj".to_string(), patch.clone())
            .await;
        assert!(!apply.success);
        let apply_err = apply.error.unwrap();
        assert!(
            apply_err.contains("agent-registered")
                || apply_err.contains("server-configured")
                || apply_err.contains("Unknown project")
                || apply_err.contains("unknown_project"),
            "apply_patch should reject a server-configured project: {}",
            apply_err
        );

        let checked = runtime
            .apply_patch_checked("local-proj".to_string(), patch.clone(), Some(true))
            .await;
        assert!(!checked.success);
        let checked_err = checked.error.unwrap();
        assert!(
            checked_err.contains("agent-registered")
                || checked_err.contains("server-configured")
                || checked_err.contains("Unknown project")
                || checked_err.contains("unknown_project"),
            "apply_patch_checked should reject a server-configured project: {}",
            checked_err
        );

        let validate = runtime
            .validate_patch("local-proj".to_string(), patch.clone(), None)
            .await;
        assert!(!validate.success);
        let validate_err = validate.error.unwrap();
        assert!(
            validate_err.contains("agent-registered")
                || validate_err.contains("server-configured")
                || validate_err.contains("Unknown project")
                || validate_err.contains("unknown_project"),
            "validate_patch should reject a server-configured project: {}",
            validate_err
        );
    }

    async fn register_agent_with_projects(
        runtime: &ToolRuntime,
        client_id: &str,
        owner: Option<&str>,
        caps: ShellClientCapabilities,
        projects: Vec<ShellAgentProjectSummary>,
    ) {
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: owner.map(str::to_string),
                hostname: None,
                capabilities: Some(caps),
                projects: Some(projects),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn list_projects_returns_agent_registered_projects_without_server_config() {
        let runtime = test_runtime();
        register_agent_with_projects(
            &runtime,
            "workstation-1",
            None,
            ShellClientCapabilities::default(),
            vec![registered_project("webcodex", "/root/git/webcodex")],
        )
        .await;

        let result = runtime.dispatch(ToolCall::ListProjects).await;
        assert!(result.success, "{:?}", result.error);
        let projects = result.output.as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["id"], "agent:workstation-1:webcodex");
        assert_eq!(projects[0]["agent_project_id"], "webcodex");
        assert_eq!(projects[0]["executor"], "agent");
        assert_eq!(projects[0]["source"], "agent_registered");
    }

    /// Helper: register an agent carrying a sanitized shell-profiles summary
    /// (inside its policy) plus a set of projects with optional per-project
    /// `shell_profile`. Used by the shell-profile observability tests.
    async fn register_agent_with_shell_profiles(
        runtime: &ToolRuntime,
        client_id: &str,
        policy: Option<crate::shell_protocol::AgentPolicySummary>,
        projects: Vec<ShellAgentProjectSummary>,
    ) {
        use crate::shell_protocol::ShellClientRegisterRequest;
        runtime
            .shell_clients
            .register(ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(ShellClientCapabilities::default()),
                projects: Some(projects),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy,
            })
            .await
            .unwrap();
    }

    fn profile_summary_entry(
        name: &str,
        has_init_script: bool,
        env_keys_count: usize,
    ) -> crate::shell_protocol::ShellProfileSummaryEntry {
        crate::shell_protocol::ShellProfileSummaryEntry {
            name: name.to_string(),
            has_init_script,
            env_keys_count,
            program: "sh".to_string(),
            args_count: 1,
        }
    }

    #[tokio::test]
    async fn list_projects_shows_shell_profile_resolution() {
        use crate::shell_protocol::{AgentPolicySummary, ShellProfilesSummary};
        let runtime = test_runtime();
        let summary = ShellProfilesSummary {
            default_profile: Some("rust".to_string()),
            configured_count: 1,
            prepared_cache_count: 0,
            profiles: vec![profile_summary_entry("rust", false, 2)],
        };
        let policy = AgentPolicySummary {
            allow_raw_shell: true,
            allow_cwd_anywhere: true,
            allowed_roots: Vec::new(),
            max_timeout_secs: 3600,
            max_output_bytes: 262144,
            shell_profiles: Some(summary),
        };
        let mut configured = registered_project("rust-proj", "/root/git/rust");
        configured.shell_profile = Some("rust".to_string());
        let mut missing = registered_project("bad-proj", "/root/git/bad");
        missing.shell_profile = Some("nope".to_string());
        let mut fallback = registered_project("default-proj", "/root/git/default");
        // No explicit shell_profile: should resolve to default_profile "rust".
        let _ = fallback.shell_profile.take();
        register_agent_with_shell_profiles(
            &runtime,
            "ws-1",
            Some(policy),
            vec![configured, missing, fallback],
        )
        .await;

        let result = runtime.dispatch(ToolCall::ListProjects).await;
        assert!(result.success, "{:?}", result.error);
        let projects = result.output.as_array().unwrap();
        let by_id: std::collections::HashMap<&str, &Value> = projects
            .iter()
            .map(|p| (p["agent_project_id"].as_str().unwrap(), p))
            .collect();
        // Explicit profile that is configured.
        let cfg = by_id["rust-proj"];
        assert_eq!(cfg["shell_profile"], "rust");
        assert_eq!(cfg["resolved_shell_profile"], "rust");
        assert_eq!(cfg["shell_profile_status"], "configured");
        // Explicit profile that is missing.
        let miss = by_id["bad-proj"];
        assert_eq!(miss["shell_profile"], "nope");
        assert_eq!(miss["resolved_shell_profile"], "nope");
        assert_eq!(miss["shell_profile_status"], "missing");
        // No explicit profile: resolves to default_profile "rust".
        let def = by_id["default-proj"];
        assert_eq!(def["shell_profile"], Value::Null);
        assert_eq!(def["resolved_shell_profile"], "rust");
        assert_eq!(def["shell_profile_status"], "configured");
        // Agent liveness fields are surfaced for each project.
        assert_eq!(def["agent_status"], "online");
        assert_eq!(def["connected"], true);
    }

    #[tokio::test]
    async fn list_projects_shell_profile_status_unknown_without_summary() {
        // An older agent that did not report a shell-profiles summary (policy
        // is None): a project with a shell_profile resolves but its configured
        // state is "unknown" because the configured set cannot be checked.
        let runtime = test_runtime();
        let mut project = registered_project("proj", "/root/git/proj");
        project.shell_profile = Some("rust".to_string());
        register_agent_with_shell_profiles(&runtime, "legacy", None, vec![project]).await;

        let result = runtime.dispatch(ToolCall::ListProjects).await;
        assert!(result.success);
        let projects = result.output.as_array().unwrap();
        assert_eq!(projects[0]["resolved_shell_profile"], "rust");
        assert_eq!(projects[0]["shell_profile_status"], "unknown");
    }

    #[tokio::test]
    async fn runtime_status_shell_profiles_summary_is_sanitized() {
        use crate::shell_protocol::{
            AgentPolicySummary, ShellProfileSummaryEntry, ShellProfilesSummary,
        };
        let registry = Arc::new(ShellClientRegistry::default());
        let secret_env_value = "DO_NOT_LEAK_THIS_ENV_VALUE";
        let secret_script = "DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY";
        let summary = ShellProfilesSummary {
            default_profile: Some("rust".to_string()),
            configured_count: 1,
            prepared_cache_count: 0,
            profiles: vec![ShellProfileSummaryEntry {
                name: "rust".to_string(),
                has_init_script: true,
                env_keys_count: 3,
                program: "sh".to_string(),
                args_count: 1,
            }],
        };
        // The summary itself never carries env values or init_script bodies;
        // the secrets below are only carried in local test variables to prove
        // they never reach the status JSON.
        let _ = (secret_env_value, secret_script);
        registry
            .register(crate::shell_protocol::ShellClientRegisterRequest {
                client_id: "profile-agent".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("websocket-v1".to_string()),
                policy: Some(AgentPolicySummary {
                    allow_raw_shell: true,
                    allow_cwd_anywhere: false,
                    allowed_roots: Vec::new(),
                    max_timeout_secs: 3600,
                    max_output_bytes: 262144,
                    shell_profiles: Some(summary),
                }),
            })
            .await
            .unwrap();
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let client = &result.output["agents"]["clients"][0];
        let sp = &client["shell_profiles"];
        assert_eq!(sp["default_profile"], "rust");
        assert_eq!(sp["configured_count"], 1);
        assert_eq!(sp["profiles"][0]["name"], "rust");
        assert_eq!(sp["profiles"][0]["has_init_script"], true);
        assert_eq!(sp["profiles"][0]["env_keys_count"], 3);
        assert_eq!(sp["profiles"][0]["program"], "sh");
        assert_eq!(sp["profiles"][0]["args_count"], 1);
        // Sanitization: never expose init_script bodies or env values.
        let rendered = sp.to_string();
        assert!(!rendered.contains("DO_NOT_LEAK_THIS_ENV_VALUE"));
        assert!(!rendered.contains("DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY"));
        assert!(sp["profiles"][0].get("init_script").is_none());
        assert!(sp["profiles"][0].get("env").is_none());
    }

    #[tokio::test]
    async fn unique_short_agent_project_id_is_resolved_by_runtime_surface() {
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::RunShell {
                            project: "agent-proj".to_string(),
                            command: "echo hi".to_string(),
                            session_id: None,
                            timeout_secs: Some(1),
                            cwd: None,
                        },
                        Some(&bootstrap),
                    )
                    .await
            }
        });
        let req = next_agent_request_for_instance(&runtime, "oe", "inst")
            .await
            .expect("unique short id should resolve to the owning agent");
        assert_eq!(req.cwd.as_deref(), Some("/tmp/agent-proj"));
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "oe".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some("hi\n".to_string()),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
    }

    #[tokio::test]
    async fn agent_run_shell_without_shell_capability_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support shell"), "{}", err);
        assert!(err.contains("agent client oe"), "{}", err);
    }

    #[tokio::test]
    async fn agent_read_file_without_file_read_capability_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        // Default caps: shell=true, file_read=false.
        register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ReadFile {
                    project: agent_test_project_id("oe"),
                    path: "README.md".to_string(),
                    session_id: None,
                    start_line: None,
                    limit: None,
                    with_line_numbers: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support file_read"), "{}", err);
    }

    #[tokio::test]
    async fn agent_run_job_without_async_capability_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        // Default caps: async_jobs=false, async_shell_jobs=false.
        register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support async shell jobs"), "{}", err);
    }

    #[tokio::test]
    async fn agent_git_status_without_shell_or_git_is_rejected() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false; // git stays false by default
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::GitStatus {
                    project: agent_test_project_id("oe"),
                    session_id: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("does not support shell or git"), "{}", err);
    }

    #[tokio::test]
    async fn agent_tool_unknown_client_returns_unknown_project_error() {
        // Project points at client "ghost" which never registered.
        let runtime = runtime_with_agent_project("ghost");
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: agent_test_project_id("ghost"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("unknown_project"), "{}", err);
        assert!(err.contains("ghost"), "{}", err);
        assert_eq!(result.output["error_kind"], "unknown_project");
        assert_eq!(result.output["project"], agent_test_project_id("ghost"));
    }

    #[tokio::test]
    async fn agent_tool_rejects_non_owner_api_key() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        let bob = auth_context(Some("bob"), false);
        // Use run_job (async) so the test does not hang if owner check leaked.
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bob),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("owned by alice"), "{}", err);
        assert!(err.contains("belongs to bob"), "{}", err);
    }

    #[tokio::test]
    async fn agent_tool_rejects_missing_auth_context() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        // dispatch_with_auth(None): no owner can be proven for an owned agent.
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunShell {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                None,
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.contains("owned by alice") || err.contains("belongs to anonymous"),
            "{}",
            err
        );
    }

    #[tokio::test]
    async fn agent_tool_allows_owner_api_key_for_run_job() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        let alice = auth_context(Some("alice"), false);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&alice),
            )
            .await;
        assert!(result.success, "{:?}", result.error);
        assert!(result.output["job_id"].is_string());
    }

    #[tokio::test]
    async fn agent_tool_allows_bootstrap_token_for_run_job() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.async_shell_jobs = true;
        register_agent(&runtime, "oe", Some("alice"), caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunJob {
                    project: agent_test_project_id("oe"),
                    command: "echo hi".to_string(),
                    session_id: None,
                    timeout_secs: None,
                    cwd: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(result.success, "{:?}", result.error);
    }

    #[tokio::test]
    async fn server_configured_local_project_is_not_runtime_surface() {
        // The ChatGPT runtime surface is agent-registered only. A server-side
        // local project config may still exist in older internal modules, but
        // ToolRuntime must not treat it as an exposed project.
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
        let result = runtime
            .dispatch_with_auth(
                ToolCall::RunCodex {
                    project: "demo".to_string(),
                    prompt: "echo hi".to_string(),
                    session_id: None,
                    approval_mode: None,
                    timeout_secs: Some(10),
                    cwd: None,
                    extra_args: None,
                },
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("unknown_project"));
    }

    // =========================================================================
    // Phase 7: runtime_status observability tool
    // =========================================================================

    fn runtime_with_info(info: RuntimeInfo) -> ToolRuntime {
        let projects = Arc::new(ProjectsState::failed(
            "projects not configured for test".to_string(),
            "test".to_string(),
        ));
        ToolRuntime::new(
            projects,
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(info),
        )
    }

    #[test]
    fn runtime_status_is_in_tool_specs() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n == "runtime_status"),
            "runtime_status must be in tool_specs: {:?}",
            names
        );
    }

    #[test]
    fn from_tool_name_parses_runtime_status() {
        let call = ToolCall::from_tool_name("runtime_status", Value::Null).unwrap();
        assert!(matches!(call, ToolCall::RuntimeStatus));
        // Also accepts an empty object.
        let call = ToolCall::from_tool_name("runtime_status", json!({})).unwrap();
        assert!(matches!(call, ToolCall::RuntimeStatus));
    }

    #[tokio::test]
    async fn runtime_status_with_no_projects_returns_configured_false() {
        let runtime = test_runtime();
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let out = &result.output;
        assert_eq!(out["service"], "webcodex");
        assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
        assert!(out["build"].is_object());
        assert!(out["build"].get("git_commit").is_some());
        assert!(out["build"].get("git_dirty").is_some());
        assert!(out["build"].get("built_at").is_some());
        assert!(out["server_time"].is_i64());
        assert!(out["pid"].is_i64());
        // No projects.toml -> configured false, load_error present.
        assert_eq!(out["projects"]["configured"], false);
        assert_eq!(out["projects"]["count"], 0);
        assert!(out["projects"]["load_error"].is_string());
    }

    #[tokio::test]
    async fn runtime_status_includes_build_metadata() {
        let runtime = test_runtime();
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let build = &result.output["build"];
        assert!(build.is_object());
        assert!(build.get("git_commit").is_some());
        assert!(build.get("git_dirty").is_some());
        assert!(build.get("built_at").is_some());
    }

    #[tokio::test]
    async fn runtime_status_with_loaded_project_returns_configured_true() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_project(tmp.path(), "demo");
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let out = &result.output;
        assert_eq!(out["projects"]["configured"], true);
        assert_eq!(out["projects"]["count"], 1);
        assert!(out["projects"]["load_error"].is_null());
    }

    #[tokio::test]
    async fn runtime_status_does_not_expose_tokens_or_secrets() {
        let info = RuntimeInfo {
            auth_enabled: true,
            configured_public_url: Some("https://example.com".to_string()),
            quic: Some(Arc::new(std::sync::Mutex::new(
                crate::config::QuicServerConfig::default().runtime_status(),
            ))),
        };
        let runtime = runtime_with_info(info);
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let serialized = serde_json::to_string(&result.output).unwrap();
        // The summary must never contain secret-like field names.
        for forbidden in [
            "token",
            "WEBCODEX_TOKEN",
            "api_key",
            "apikey",
            "secret",
            "password",
            "authorization",
            "bearer",
        ] {
            assert!(
                !serialized
                    .to_lowercase()
                    .contains(&forbidden.to_lowercase()),
                "runtime_status output must not contain '{}': {}",
                forbidden,
                serialized
            );
        }
        // auth_enabled is a bool, not the token value.
        assert_eq!(result.output["auth_enabled"], true);
    }

    #[tokio::test]
    async fn runtime_status_quic_disabled_is_non_sensitive() {
        let runtime = runtime_with_info(RuntimeInfo::default());
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["quic"]["enabled"], false);
        assert_eq!(result.output["quic"]["listen"], "0.0.0.0:8443");
        assert_eq!(result.output["quic"]["alpn"], "webcodex-agent/1");
        assert_eq!(result.output["quic"]["listener_started"], false);
        assert!(result.output["quic"]["last_error"].is_null());
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert!(!serialized.contains("WEBCODEX_QUIC_CERT"));
        assert!(!serialized.contains("WEBCODEX_QUIC_KEY"));
        assert!(!serialized.to_ascii_lowercase().contains("token"));
    }

    #[tokio::test]
    async fn runtime_status_quic_enabled_error_is_sanitized() {
        let quic_cfg = crate::config::QuicServerConfig {
            enabled: true,
            listen: "0.0.0.0:8443".to_string(),
            cert: PathBuf::from("/secret/certs/fullchain.pem"),
            key: PathBuf::from("/secret/certs/privkey.pem"),
            alpn: "webcodex-agent/1".to_string(),
        };
        let status = Arc::new(std::sync::Mutex::new(quic_cfg.runtime_status()));
        status
            .lock()
            .unwrap()
            .mark_error("WEBCODEX_QUIC_KEY path does not exist: /secret/certs/privkey.pem");
        let runtime = runtime_with_info(RuntimeInfo {
            auth_enabled: false,
            configured_public_url: None,
            quic: Some(status),
        });
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["quic"]["enabled"], true);
        assert_eq!(result.output["quic"]["listener_started"], false);
        assert_eq!(
            result.output["quic"]["last_error"],
            "WEBCODEX_QUIC_KEY path does not exist"
        );
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert!(!serialized.contains("/secret/certs"));
        assert!(!serialized.contains("privkey.pem"));
    }

    #[tokio::test]
    async fn runtime_status_quic_started_reports_listen_and_alpn() {
        let quic_cfg = crate::config::QuicServerConfig {
            enabled: true,
            listen: "127.0.0.1:9443".to_string(),
            cert: PathBuf::from("/hidden/cert.pem"),
            key: PathBuf::from("/hidden/key.pem"),
            alpn: "webcodex-agent/1".to_string(),
        };
        let status = Arc::new(std::sync::Mutex::new(quic_cfg.runtime_status()));
        status.lock().unwrap().mark_started();
        let runtime = runtime_with_info(RuntimeInfo {
            auth_enabled: false,
            configured_public_url: None,
            quic: Some(status),
        });
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["quic"]["enabled"], true);
        assert_eq!(result.output["quic"]["listen"], "127.0.0.1:9443");
        assert_eq!(result.output["quic"]["alpn"], "webcodex-agent/1");
        assert_eq!(result.output["quic"]["listener_started"], true);
        assert!(result.output["quic"]["last_error"].is_null());
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert!(!serialized.contains("/hidden"));
    }

    #[tokio::test]
    async fn runtime_status_auth_enabled_reflects_runtime_info() {
        let runtime = runtime_with_info(RuntimeInfo {
            auth_enabled: false,
            configured_public_url: None,
            quic: Some(Arc::new(std::sync::Mutex::new(
                crate::config::QuicServerConfig::default().runtime_status(),
            ))),
        });
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["auth_enabled"], false);
        assert!(result.output["configured_public_url"].is_null());

        let runtime = runtime_with_info(RuntimeInfo {
            auth_enabled: true,
            configured_public_url: Some("https://webcodex.example.com".to_string()),
            quic: Some(Arc::new(std::sync::Mutex::new(
                crate::config::QuicServerConfig::default().runtime_status(),
            ))),
        });
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        assert_eq!(result.output["auth_enabled"], true);
        assert_eq!(
            result.output["configured_public_url"],
            "https://webcodex.example.com"
        );
    }

    #[test]
    fn runtime_info_from_env_reads_webcodex_public_url() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_TOKEN", "token");
        std::env::set_var("WEBCODEX_PUBLIC_URL", "https://new.example.com");

        let info = RuntimeInfo::from_env();
        assert!(info.auth_enabled);
        assert_eq!(
            info.configured_public_url.as_deref(),
            Some("https://new.example.com")
        );

        std::env::remove_var("WEBCODEX_TOKEN");
        std::env::remove_var("WEBCODEX_PUBLIC_URL");
    }

    #[tokio::test]
    async fn runtime_status_agent_summary_includes_protocol_version() {
        use crate::shell_protocol::ShellClientRegisterRequest;
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "agent-1".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: Some("Workstation".to_string()),
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let agents = &result.output["agents"];
        assert_eq!(agents["count"], 1);
        assert_eq!(agents["online_count"], 1);
        assert_eq!(agents["offline_count"], 0);
        assert_eq!(agents["stale_count"], 0);
        let clients = agents["clients"].as_array().unwrap();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0]["client_id"], "agent-1");
        assert_eq!(clients[0]["agent_protocol_version"], "polling-v1");
        assert_eq!(clients[0]["transport"], "polling");
        assert_eq!(clients[0]["connected"], true);
        assert!(clients[0]["capabilities"].is_object());
        assert_eq!(clients[0]["projects_count"], 0);
        // last_seen must be present as an integer unix timestamp (seconds).
        assert!(
            clients[0]["last_seen"].is_i64(),
            "last_seen must be an integer timestamp: {:?}",
            clients[0]["last_seen"]
        );
    }

    #[tokio::test]
    async fn runtime_status_includes_sanitized_policy_summary() {
        use crate::shell_protocol::{AgentPolicySummary, ShellClientRegisterRequest};
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "policy-agent".to_string(),
                agent_instance_id: "inst-p".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("websocket-v1".to_string()),
                policy: Some(AgentPolicySummary {
                    allow_raw_shell: true,
                    allow_cwd_anywhere: false,
                    allowed_roots: vec![std::path::PathBuf::from("/root")],
                    max_timeout_secs: 3600,
                    max_output_bytes: 262144,
                    shell_profiles: None,
                }),
            })
            .await
            .unwrap();
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let clients = result.output["agents"]["clients"].as_array().unwrap();
        let policy = &clients[0]["policy"];
        assert_eq!(policy["allow_raw_shell"], true);
        assert_eq!(policy["allow_cwd_anywhere"], false);
        assert_eq!(policy["allowed_roots"], json!(["/root"]));
        assert_eq!(policy["max_timeout_secs"], 3600);
        assert_eq!(policy["max_output_bytes"], 262144);
        // Sanitization: never expose token/env/init_script.
        assert!(policy.get("token").is_none());
        assert!(policy.get("env").is_none());
        assert!(policy.get("init_script").is_none());
    }

    #[tokio::test]
    async fn runtime_status_policy_summary_is_null_for_older_agents() {
        use crate::shell_protocol::ShellClientRegisterRequest;
        let registry = Arc::new(ShellClientRegistry::default());
        // Older agent: no policy field (None).
        registry
            .register(ShellClientRegisterRequest {
                client_id: "legacy-agent".to_string(),
                agent_instance_id: "inst-l".to_string(),
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
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let clients = result.output["agents"]["clients"].as_array().unwrap();
        // Older/minimal payload -> policy is null, not a fatal error.
        assert!(clients[0]["policy"].is_null());
    }

    #[tokio::test]
    async fn list_agents_includes_sanitized_policy_summary() {
        use crate::shell_protocol::{AgentPolicySummary, ShellClientRegisterRequest};
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "list-policy-agent".to_string(),
                agent_instance_id: "inst-lp".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("websocket-v1".to_string()),
                policy: Some(AgentPolicySummary {
                    allow_raw_shell: false,
                    allow_cwd_anywhere: true,
                    allowed_roots: vec![],
                    max_timeout_secs: 120,
                    max_output_bytes: 4096,
                    shell_profiles: None,
                }),
            })
            .await
            .unwrap();
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::ListAgents).await;
        assert!(result.success);
        let agents = result.output["agents"].as_array().unwrap();
        assert_eq!(agents.len(), 1);
        let policy = &agents[0]["policy"];
        assert_eq!(policy["allow_raw_shell"], false);
        assert_eq!(policy["allow_cwd_anywhere"], true);
        assert_eq!(policy["max_timeout_secs"], 120);
        assert_eq!(policy["max_output_bytes"], 4096);
        // No secret fields leak through listAgents either.
        assert!(policy.get("token").is_none());
        assert!(policy.get("env").is_none());
        assert!(policy.get("init_script").is_none());
    }

    #[tokio::test]
    async fn runtime_status_marks_stale_websocket_agent_with_last_seen() {
        use crate::shell_client::TRANSPORT_WEBSOCKET;
        use crate::shell_protocol::ShellClientRegisterRequest;
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "ws-stale".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: Some("Stale WS".to_string()),
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: Some(vec![]),
                agent_protocol_version: Some("websocket-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        registry
            .set_transport("ws-stale", TRANSPORT_WEBSOCKET)
            .await
            .unwrap();
        // Force the agent past the 60s online window so it reads as stale.
        let stale_ts = chrono::Utc::now().timestamp() - 120;
        registry.set_last_seen_for_test("ws-stale", stale_ts).await;

        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry,
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let agents = &result.output["agents"];
        assert_eq!(agents["count"], 1);
        assert_eq!(agents["online_count"], 0);
        assert_eq!(agents["stale_count"], 1);
        assert_eq!(agents["offline_count"], 1);
        let entry = &agents["clients"][0];
        assert_eq!(entry["client_id"], "ws-stale");
        assert_eq!(entry["transport"], "websocket");
        assert_eq!(entry["status"], "stale");
        assert_eq!(entry["connected"], false);
        assert_eq!(entry["last_seen"], stale_ts);
    }

    #[tokio::test]
    async fn runtime_status_reflects_websocket_transport_label() {
        let registry = Arc::new(ShellClientRegistry::default());
        let runtime = ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "none".to_string(),
                "test".to_string(),
            )),
            registry.clone(),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        );
        registry
            .register(ShellClientRegisterRequest {
                client_id: "ws-agent".to_string(),
                agent_instance_id: "inst".to_string(),
                display_name: None,
                owner: Some("alice".to_string()),
                hostname: None,
                capabilities: None,
                projects: None,
                agent_protocol_version: Some("websocket-v1".to_string()),
                policy: None,
            })
            .await
            .unwrap();
        // Flip the transport label the same way the WebSocket handler does.
        registry
            .set_transport("ws-agent", crate::shell_client::TRANSPORT_WEBSOCKET)
            .await
            .unwrap();

        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let clients = &result.output["agents"]["clients"];
        let entry = clients
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["client_id"] == "ws-agent")
            .expect("ws-agent present");
        assert_eq!(entry["transport"], "websocket");
        assert_eq!(entry["agent_protocol_version"], "websocket-v1");
    }

    #[tokio::test]
    async fn runtime_status_counts_local_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        // Write a fake local job in "running" state and register it in the
        // in-memory map so runtime_status counts it.
        let job_dir = root.join(".codex/jobs/job-active");
        fs::create_dir_all(&job_dir).unwrap();
        fs::write(job_dir.join("status"), "running").unwrap();
        let meta_json = json!({
            "job_id": "job-active",
            "project": "demo",
            "command": "sleep 10",
            "status": "running",
            "created_at": 1,
            "started_at": 1,
            "max_runtime_secs": 600,
            "executor": "local",
            "path": root.to_string_lossy(),
            "kind": "shell",
        });
        fs::write(
            job_dir.join("metadata.json"),
            serde_json::to_string_pretty(&meta_json).unwrap(),
        )
        .unwrap();
        runtime.local_jobs.lock().await.insert(
            "job-active".to_string(),
            LocalJobRecord {
                project: "demo".to_string(),
                dir: job_dir,
            },
        );
        // Also write a completed job to verify it's not counted as active.
        let done_dir = root.join(".codex/jobs/job-done");
        fs::create_dir_all(&done_dir).unwrap();
        fs::write(done_dir.join("status"), "completed").unwrap();
        fs::write(
            done_dir.join("metadata.json"),
            serde_json::to_string(&json!({
                "job_id": "job-done",
                "project": "demo",
                "command": "true",
                "status": "completed",
                "created_at": 1,
                "started_at": 1,
                "executor": "local",
                "path": root.to_string_lossy(),
                "kind": "shell",
            }))
            .unwrap(),
        )
        .unwrap();
        runtime.local_jobs.lock().await.insert(
            "job-done".to_string(),
            LocalJobRecord {
                project: "demo".to_string(),
                dir: done_dir,
            },
        );

        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success, "{:?}", result.error);
        let jobs = &result.output["jobs"];
        assert_eq!(jobs["local_known_count"], 2);
        // Only the running job is active.
        assert_eq!(jobs["active_count"], 1);
        assert_eq!(jobs["agent_known_count"], 0);
    }

    #[tokio::test]
    async fn runtime_status_tools_summary_lists_names() {
        let runtime = test_runtime();
        let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
        assert!(result.success);
        let tools = &result.output["tools"];
        let names = tools["names"].as_array().unwrap();
        assert!(names.len() > 0);
        assert!(
            names.iter().any(|n| n == "runtime_status"),
            "tools.names must include runtime_status: {:?}",
            names
        );
        assert_eq!(tools["count"], names.len() as i64);
    }

    // =========================================================================
    // Phase A read-only console tools
    // =========================================================================

    #[test]
    fn from_tool_name_parses_phase_a_tools() {
        let call =
            ToolCall::from_tool_name("list_project_files", json!({"project": "demo"})).unwrap();
        match call {
            ToolCall::ListProjectFiles {
                project,
                path,
                limit,
                ..
            } => {
                assert_eq!(project, "demo");
                assert_eq!(path, None);
                assert_eq!(limit, None);
            }
            other => panic!("expected ListProjectFiles, got {:?}", other),
        }

        let call = ToolCall::from_tool_name(
            "search_project_text",
            json!({
                "project": "demo",
                "pattern": "fn main",
                "limit": 5,
                "context_before": 3,
                "context_after": 8
            }),
        )
        .unwrap();
        match call {
            ToolCall::SearchProjectText {
                project,
                pattern,
                path,
                limit,
                context_before,
                context_after,
                ..
            } => {
                assert_eq!(project, "demo");
                assert_eq!(pattern, "fn main");
                assert_eq!(path, None);
                assert_eq!(limit, Some(5));
                assert_eq!(context_before, Some(3));
                assert_eq!(context_after, Some(8));
            }
            other => panic!("expected SearchProjectText, got {:?}", other),
        }

        let call =
            ToolCall::from_tool_name("git_diff_summary", json!({"project": "demo"})).unwrap();
        assert!(matches!(call, ToolCall::GitDiffSummary { project, .. } if project == "demo"));

        // list_jobs has only optional fields; null arguments must still parse.
        let call = ToolCall::from_tool_name("list_jobs", Value::Null).unwrap();
        assert!(matches!(
            call,
            ToolCall::ListJobs {
                limit: None,
                status: None
            }
        ));
        let call = ToolCall::from_tool_name("list_jobs", json!({"limit": 3, "status": "running"}))
            .unwrap();
        match call {
            ToolCall::ListJobs { limit, status } => {
                assert_eq!(limit, Some(3));
                assert_eq!(status.as_deref(), Some("running"));
            }
            other => panic!("expected ListJobs, got {:?}", other),
        }

        let call = ToolCall::from_tool_name("job_tail", json!({"job_id": "abc", "tail_lines": 10}))
            .unwrap();
        match call {
            ToolCall::JobTail { job_id, tail_lines } => {
                assert_eq!(job_id, "abc");
                assert_eq!(tail_lines, Some(10));
            }
            other => panic!("expected JobTail, got {:?}", other),
        }
    }

    #[test]
    fn from_tool_name_list_jobs_with_null_arguments_parses() {
        // Regression: a non-unit tool with all-optional fields must deserialize
        // when a caller passes `null` arguments (normalized to an empty object).
        let call = ToolCall::from_tool_name("list_jobs", Value::Null)
            .unwrap_or_else(|e| panic!("list_jobs with null args should parse: {}", e));
        assert!(matches!(call, ToolCall::ListJobs { .. }));
    }

    #[test]
    fn tool_specs_include_phase_a_tools() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for expected in [
            "list_project_files",
            "search_project_text",
            "git_diff_summary",
            "list_jobs",
            "job_tail",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "tool_specs must include '{}': {:?}",
                expected,
                names
            );
        }
    }

    // =========================================================================
    // validate_patch (patch preflight / dry-run)
    // =========================================================================

    #[test]
    fn from_tool_name_parses_checked_and_cleanup_tools() {
        let checked = ToolCall::from_tool_name(
            "apply_patch_checked",
            json!({"project":"agent:c:p","patch":"diff","deny_sensitive_paths":true}),
        )
        .unwrap();
        assert!(matches!(
            checked,
            ToolCall::ApplyPatchChecked { project, patch, deny_sensitive_paths, .. }
                if project == "agent:c:p" && patch == "diff" && deny_sensitive_paths == Some(true)
        ));

        let delete = ToolCall::from_tool_name(
            "delete_project_files",
            json!({"project":"agent:c:p","paths":["tmp.txt"]}),
        )
        .unwrap();
        assert!(
            matches!(delete, ToolCall::DeleteProjectFiles { project, paths, .. } if project == "agent:c:p" && paths == vec!["tmp.txt"])
        );

        let restore = ToolCall::from_tool_name(
            "git_restore_paths",
            json!({"project":"agent:c:p","paths":["README.md"]}),
        )
        .unwrap();
        assert!(
            matches!(restore, ToolCall::GitRestorePaths { project, paths, .. } if project == "agent:c:p" && paths == vec!["README.md"])
        );

        let discard = ToolCall::from_tool_name(
            "discard_untracked",
            json!({"project":"agent:c:p","paths":["tmp.txt"]}),
        )
        .unwrap();
        assert!(
            matches!(discard, ToolCall::DiscardUntracked { project, paths, .. } if project == "agent:c:p" && paths == vec!["tmp.txt"])
        );
    }

    #[test]
    fn parse_porcelain_summary_buckets_untracked_files() {
        let summary = parse_porcelain_summary(
            " M README.md\n?? tmp.txt\nR  old.rs -> new.rs\n!! ignored.log\n",
        );
        assert_eq!(summary.tracked_changed_files, vec!["README.md", "new.rs"]);
        assert_eq!(summary.untracked_files, vec!["tmp.txt"]);
        assert_eq!(summary.ignored_files, vec!["ignored.log"]);
        assert_eq!(summary.changed_files_count, 4);
    }

    #[test]
    fn cleanup_paths_reject_sensitive_and_project_root() {
        let root = vec![".".to_string()];
        assert!(validate_limited_cleanup_paths(&root, true).is_err());
        let sensitive = vec!["agent.toml".to_string()];
        assert!(validate_limited_cleanup_paths(&sensitive, true).is_err());
        let safe = vec!["tmp_web_codex_smoke.txt".to_string()];
        assert_eq!(
            validate_limited_cleanup_paths(&safe, true).unwrap(),
            vec!["tmp_web_codex_smoke.txt".to_string()]
        );
    }
    #[test]
    fn from_tool_name_parses_validate_patch() {
        let call = ToolCall::from_tool_name(
            "validate_patch",
            json!({"project": "agent:c:p", "patch": "diff"}),
        )
        .unwrap();
        assert!(
            matches!(call, ToolCall::ValidatePatch { project, patch, .. } if project == "agent:c:p" && patch == "diff")
        );
    }

    #[test]
    fn tool_specs_include_validate_patch() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n == "validate_patch"),
            "tool_specs must include validate_patch: {:?}",
            names
        );
    }

    #[test]
    fn validate_preflight_path_rejects_boundary_escapes() {
        // In-bounds relative paths are accepted.
        assert!(validate_preflight_path("README.md").is_ok());
        assert!(validate_preflight_path("src/main.rs").is_ok());
        // Absolute paths, traversal, empty, and NUL are hard rejects.
        assert!(validate_preflight_path("").is_err());
        assert!(validate_preflight_path("/etc/passwd").is_err());
        assert!(validate_preflight_path("../outside").is_err());
        assert!(validate_preflight_path("src/../../outside").is_err());
        assert!(validate_preflight_path("src\0main.rs").is_err());
        // Sensitive filenames are NOT hard-rejected here (they become warnings).
        assert!(validate_preflight_path(".env").is_ok());
        assert!(validate_preflight_path("agent.toml").is_ok());
    }

    #[test]
    fn sensitive_path_warnings_flags_sensitive_names() {
        assert!(sensitive_path_warnings(".env")
            .iter()
            .any(|w| w.contains(".env")));
        assert!(sensitive_path_warnings("config/agent.toml")
            .iter()
            .any(|w| w.contains("agent.toml")));
        assert!(sensitive_path_warnings("webcodex.env")
            .iter()
            .any(|w| w.contains("webcodex.env")));
        assert!(sensitive_path_warnings("projects.d/x.toml")
            .iter()
            .any(|w| w.contains("projects.d")));
        assert!(sensitive_path_warnings(".git/config")
            .iter()
            .any(|w| w.contains(".git")));
        assert!(sensitive_path_warnings("target/debug/x")
            .iter()
            .any(|w| w.contains("target")));
        assert!(sensitive_path_warnings("node_modules/x")
            .iter()
            .any(|w| w.contains("node_modules")));
        // A normal source file produces no warnings.
        assert!(sensitive_path_warnings("src/main.rs").is_empty());
        // Matching is case-insensitive.
        assert!(sensitive_path_warnings("TARGET/foo")
            .iter()
            .any(|w| w.contains("target")));
    }

    #[tokio::test]
    async fn validate_patch_rejects_empty_patch() {
        let runtime = test_runtime();
        let result = runtime
            .validate_patch("agent:c:p".to_string(), "".to_string(), None)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn validate_patch_rejects_nul_byte_patch() {
        let runtime = test_runtime();
        let result = runtime
            .validate_patch("agent:c:p".to_string(), "diff\0--- a/f\n".to_string(), None)
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
    }

    #[tokio::test]
    async fn validate_patch_rejects_oversized_patch() {
        let runtime = test_runtime();
        // Build a patch one byte over the limit.
        let oversized = "x".repeat(MAX_VALIDATE_PATCH_BYTES + 1);
        let result = runtime
            .validate_patch("agent:c:p".to_string(), oversized, None)
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("too large"), "got: {}", err);
    }

    #[tokio::test]
    async fn validate_patch_rejects_non_agent_project() {
        // A server-configured (local) project is not a supported runtime
        // surface for validate_patch. resolve_project rejects it before the
        // agent dry-run path, and the server never reads the filesystem.
        let runtime = test_runtime();
        let patch = "--- a/README.md\n+++ b/README.md\n@@ -1 +1,2 @@\nhello\n+world\n";
        let result = runtime
            .validate_patch("agent:nope:nope".to_string(), patch.to_string(), None)
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.to_lowercase().contains("unknown") || err.to_lowercase().contains("agent"),
            "expected a routing/rejection error for non-agent project, got: {}",
            err
        );
    }

    #[test]
    fn max_validate_patch_bytes_is_conservative() {
        // Pin the conservative upper bound so it is not accidentally raised.
        assert_eq!(MAX_VALIDATE_PATCH_BYTES, 256 * 1024);
        assert!(MAX_VALIDATE_PATCH_BYTES <= 1024 * 1024);
    }

    #[test]
    fn parse_file_list_entries_is_bounded_and_marks_truncation() {
        // Simulate agent file_list stdout: dirs suffixed with '/'.
        let stdout = "Cargo.toml\nsrc/\nREADME.md\ntarget/\nCargo.lock\n";
        // First, without truncation, verify kinds and project-relative paths.
        let (all, truncated_full) = parse_file_list_entries(stdout, ".", 10);
        assert!(!truncated_full);
        assert_eq!(all.len(), 5);
        let src = all.iter().find(|e| e["path"] == "src").expect("src entry");
        assert_eq!(src["kind"], "dir");
        let cargo = all
            .iter()
            .find(|e| e["path"] == "Cargo.toml")
            .expect("Cargo.toml entry");
        assert_eq!(cargo["kind"], "file");

        // With a tight bound, output is truncated and sorted alphabetically.
        let (bounded, truncated) = parse_file_list_entries(stdout, ".", 3);
        assert_eq!(bounded.len(), 3);
        assert!(truncated);
        let paths: Vec<&str> = bounded
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        // Sorted: Cargo.lock, Cargo.toml, README.md come first.
        assert_eq!(paths, vec!["Cargo.lock", "Cargo.toml", "README.md"]);
    }

    #[test]
    fn parse_file_list_entries_prepends_subpath_for_relative_paths() {
        let stdout = "main.rs\nlib.rs\n";
        let (entries, truncated) = parse_file_list_entries(stdout, "src", 10);
        assert!(!truncated);
        let paths: Vec<&str> = entries
            .iter()
            .map(|e| e["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[test]
    fn validate_project_relative_path_rejects_absolute_and_parent_traversal() {
        assert!(validate_project_relative_path(".").is_ok());
        assert!(validate_project_relative_path("src").is_ok());
        assert!(validate_project_relative_path("src/main.rs").is_ok());
        assert!(validate_project_relative_path("/etc").is_err());
        assert!(validate_project_relative_path("../outside").is_err());
        assert!(validate_project_relative_path("src/../../outside").is_err());
        assert!(validate_project_relative_path("src\0main.rs").is_err());
    }

    #[test]
    fn parse_search_matches_is_bounded_and_strips_dot_slash() {
        let stdout = "./src/main.rs:10:fn main() {}\n./src/lib.rs:3:pub fn x()\n./src/a:1:1\n";
        let (matches, truncated) = parse_search_matches(stdout, 2);
        assert_eq!(matches.len(), 2);
        assert!(truncated);
        assert_eq!(matches[0]["path"], "src/main.rs");
        assert_eq!(matches[0]["line"], 10);
        assert_eq!(matches[0]["preview"], "fn main() {}");
        assert_eq!(matches[1]["path"], "src/lib.rs");
    }

    #[test]
    fn parse_search_matches_skips_lines_without_line_number() {
        // Binary file matches or malformed lines are skipped, not counted.
        let stdout = "binary:file\nsrc/main.rs:5:hit\n";
        let (matches, _truncated) = parse_search_matches(stdout, 10);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/main.rs");
    }

    #[test]
    fn parse_porcelain_files_handles_basic_rename_and_quoted_paths() {
        let porcelain =
            " M src/main.rs\nA  new_file.rs\nR  old_name.rs -> new_name.rs\n?? \"quoted path.rs\"";
        let files = parse_porcelain_files(porcelain);
        assert_eq!(
            files,
            vec![
                "src/main.rs",
                "new_file.rs",
                "new_name.rs",
                "quoted path.rs",
            ]
        );
    }

    #[test]
    fn split_diff_summary_separates_porcelain_and_stat() {
        let stdout = format!(
            " M src/a.rs\nA  src/b.rs\n\n{}\n src/a.rs | 2 +-\n 1 file changed",
            DIFF_SUMMARY_SENTINEL,
        );
        let (porcelain, diff_stat) = split_diff_summary(&stdout);
        assert!(porcelain.contains("src/a.rs"));
        assert!(porcelain.contains("src/b.rs"));
        assert!(!porcelain.contains(DIFF_SUMMARY_SENTINEL));
        assert!(diff_stat.contains("1 file changed"));
        assert!(!diff_stat.contains(DIFF_SUMMARY_SENTINEL));
    }

    #[test]
    fn split_diff_summary_without_sentinel_returns_all_as_porcelain() {
        let (porcelain, diff_stat) = split_diff_summary("just status lines");
        assert_eq!(porcelain, "just status lines");
        assert_eq!(diff_stat, "");
    }

    #[test]
    fn search_project_text_command_excludes_sensitive_dirs_and_bounds_output() {
        let cmd = search_project_text_command("fn main", "src", 25);
        assert!(cmd.contains("--exclude-dir=.git"));
        assert!(cmd.contains("--exclude-dir=target"));
        assert!(cmd.contains("--exclude-dir=node_modules"));
        assert!(cmd.contains("head -n 26"));
        assert!(cmd.contains("grep -rnI"));
    }

    #[test]
    fn git_diff_summary_command_is_read_only() {
        let cmd = git_diff_summary_command();
        // Must run only read-only git inspection subcommands.
        assert!(cmd.contains("git status --porcelain"));
        assert!(cmd.contains("git diff --stat"));
        // No mutating subcommands may appear.
        for forbidden in [
            "apply", "commit", "checkout", "reset", "push", "stash", "merge", "rebase", "rm ",
        ] {
            assert!(
                !cmd.contains(forbidden),
                "git_diff_summary command must not contain '{}': {}",
                forbidden,
                cmd
            );
        }
    }

    #[tokio::test]
    async fn list_jobs_returns_bounded_summaries_without_stdout_stderr() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        // Seed a local job whose on-disk logs contain sensitive-looking text.
        let dir = write_fake_job(
            root,
            "job-secret",
            "demo",
            &root.to_string_lossy(),
            "completed",
            "WEBCODEX_TOKEN=supersecret\nline2",
            "Authorization: Bearer xyz",
            json!({}),
        );
        runtime.local_jobs.lock().await.insert(
            "job-secret".to_string(),
            LocalJobRecord {
                project: "demo".to_string(),
                dir,
            },
        );
        let result = runtime
            .dispatch(ToolCall::ListJobs {
                limit: None,
                status: None,
            })
            .await;
        assert!(result.success, "{:?}", result.error);
        let jobs = result.output["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job["job_id"], "job-secret");
        assert_eq!(job["status"], "completed");
        assert_eq!(job["executor"], "local");
        // Summaries must never carry stdout/stderr bodies.
        assert!(
            job.get("stdout").is_none(),
            "list_jobs summary must not include stdout"
        );
        assert!(
            job.get("stderr").is_none(),
            "list_jobs summary must not include stderr"
        );
        // And the serialized summary must not leak the secret log text.
        let serialized = serde_json::to_string(job).unwrap();
        assert!(
            !serialized.contains("supersecret"),
            "list_jobs summary leaked stdout secret: {}",
            serialized
        );
        assert!(
            !serialized.contains("Bearer xyz"),
            "list_jobs summary leaked stderr secret: {}",
            serialized
        );
    }

    #[tokio::test]
    async fn list_jobs_respects_limit_bound() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let runtime = runtime_with_project(root, "demo");
        for i in 0..5 {
            let dir = write_fake_job(
                root,
                &format!("job-{}", i),
                "demo",
                &root.to_string_lossy(),
                "completed",
                "",
                "",
                json!({}),
            );
            runtime.local_jobs.lock().await.insert(
                format!("job-{}", i),
                LocalJobRecord {
                    project: "demo".to_string(),
                    dir,
                },
            );
        }
        let result = runtime
            .dispatch(ToolCall::ListJobs {
                limit: Some(2),
                status: None,
            })
            .await;
        assert!(result.success);
        let jobs = result.output["jobs"].as_array().unwrap();
        assert_eq!(jobs.len(), 2);
        assert_eq!(result.output["truncated"], true);
    }

    #[tokio::test]
    async fn list_jobs_requires_no_agent_capability() {
        // list_jobs has no project and no agent capability requirement, so it
        // succeeds even with no registered agent.
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::ListJobs {
                limit: None,
                status: None,
            })
            .await;
        assert!(result.success);
        assert!(result.output["jobs"].is_array());
    }

    #[tokio::test]
    async fn job_tail_reaches_job_logic_without_agent_auth() {
        // job_tail bypasses agent authorization (no project). An unknown job
        // returns a structured "unknown job" error, proving it reached the job
        // layer rather than an authorization gate.
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::JobTail {
                job_id: "no-such-job".to_string(),
                tail_lines: None,
            })
            .await;
        assert!(!result.success);
        assert!(
            result.error.unwrap().contains("unknown job"),
            "job_tail should report unknown job"
        );
    }

    #[tokio::test]
    async fn list_project_files_requires_file_read_capability() {
        let runtime = runtime_with_agent_project("oe");
        // Default capabilities have file_read = false.
        register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ListProjectFiles {
                    project: agent_test_project_id("oe"),
                    session_id: None,
                    path: None,
                    limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(
            result.error.unwrap().contains("file_read"),
            "list_project_files should require file_read capability"
        );
    }

    #[tokio::test]
    async fn search_project_text_requires_shell_capability() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::SearchProjectText {
                    project: agent_test_project_id("oe"),
                    pattern: "fn".to_string(),
                    session_id: None,
                    path: None,
                    limit: None,
                    context_before: None,
                    context_after: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(
            result.error.unwrap().contains("shell"),
            "search_project_text should require shell capability"
        );
    }

    #[tokio::test]
    async fn git_diff_summary_requires_git_or_shell_capability() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::GitDiffSummary {
                    project: agent_test_project_id("oe"),
                    session_id: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        // GitOrShell accepts `shell` or `git`; with both off it is rejected.
        let err = result.error.unwrap();
        assert!(
            err.contains("shell") || err.contains("git"),
            "git_diff_summary should require shell or git capability: {}",
            err
        );
    }

    #[tokio::test]
    async fn show_changes_requires_git_or_shell_capability() {
        let runtime = runtime_with_agent_project("oe");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = false;
        register_agent(&runtime, "oe", None, caps).await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ShowChanges {
                    project: agent_test_project_id("oe"),
                    session_id: None,
                    include_diff: None,
                    max_hunks: None,
                    max_hunk_lines: None,
                    session_event_limit: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.contains("shell") || err.contains("git"),
            "show_changes should require shell or git capability: {}",
            err
        );
    }

    #[tokio::test]
    async fn list_project_files_rejects_non_agent_project_id() {
        // A bare project id (not agent:<client>:<project>) is not resolved by
        // the runtime surface — proving routing goes through the owning agent.
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::ListProjectFiles {
                project: "some-local-id".to_string(),
                session_id: None,
                path: None,
                limit: None,
            })
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(err.contains("agent") || err.contains("projects.toml"));
    }

    #[tokio::test]
    async fn list_project_files_rejects_absolute_or_parent_paths_before_agent_request() {
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                file_read: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        for path in ["/etc", "../outside"] {
            let result = runtime
                .dispatch_with_auth(
                    ToolCall::ListProjectFiles {
                        project: agent_test_project_id("oe"),
                        session_id: None,
                        path: Some(path.to_string()),
                        limit: None,
                    },
                    Some(&bootstrap),
                )
                .await;
            assert!(!result.success, "path {} should be rejected", path);
            let err = result.error.unwrap();
            assert!(
                err.contains("project-relative") || err.contains("parent traversal"),
                "unexpected error for {}: {}",
                path,
                err
            );
        }
    }

    #[tokio::test]
    async fn search_project_text_rejects_empty_pattern() {
        // Authorization runs before the tool body, so register an agent with
        // shell capability to reach the empty-pattern validation.
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        let result = runtime
            .dispatch_with_auth(
                ToolCall::SearchProjectText {
                    project: agent_test_project_id("oe"),
                    pattern: "   ".to_string(),
                    session_id: None,
                    path: None,
                    limit: None,
                    context_before: None,
                    context_after: None,
                },
                Some(&bootstrap),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("pattern"));
    }

    #[tokio::test]
    async fn search_project_text_rejects_absolute_or_parent_paths_before_agent_request() {
        let runtime = runtime_with_agent_project("oe");
        register_agent(
            &runtime,
            "oe",
            None,
            ShellClientCapabilities {
                shell: true,
                ..Default::default()
            },
        )
        .await;
        let bootstrap = auth_context(None, true);
        for path in ["/etc", "../outside"] {
            let result = runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectText {
                        project: agent_test_project_id("oe"),
                        pattern: "needle".to_string(),
                        session_id: None,
                        path: Some(path.to_string()),
                        limit: None,
                        context_before: None,
                        context_after: None,
                    },
                    Some(&bootstrap),
                )
                .await;
            assert!(!result.success, "path {} should be rejected", path);
            let err = result.error.unwrap();
            assert!(
                err.contains("project-relative") || err.contains("parent traversal"),
                "unexpected error for {}: {}",
                path,
                err
            );
        }
    }

    // =========================================================================
    // Phase 4: replace_in_file / write_project_file
    // =========================================================================

    #[test]
    fn from_tool_name_parses_phase4_edit_tools() {
        let replace = ToolCall::from_tool_name(
            "replace_in_file",
            json!({
                "project": "agent:c:p",
                "path": "src/main.rs",
                "old": "foo",
                "new": "bar",
                "expected_replacements": 3,
                "allow_multiple": true
            }),
        )
        .unwrap();
        assert!(matches!(
            replace,
            ToolCall::ReplaceInFile { project, path, old, new, expected_replacements, allow_multiple, .. }
                if project == "agent:c:p"
                && path == "src/main.rs"
                && old == "foo"
                && new == "bar"
                && expected_replacements == Some(3)
                && allow_multiple == Some(true)
        ));

        let write = ToolCall::from_tool_name(
            "write_project_file",
            json!({
                "project": "agent:c:p",
                "path": "new.txt",
                "content": "hello"
            }),
        )
        .unwrap();
        assert!(matches!(
            write,
            ToolCall::WriteProjectFile { project, path, content, overwrite, expected_sha256, expected_content_prefix, .. }
                if project == "agent:c:p"
                && path == "new.txt"
                && content == "hello"
                && overwrite.is_none()
                && expected_sha256.is_none()
                && expected_content_prefix.is_none()
        ));

        let replace_lines = ToolCall::from_tool_name(
            "replace_line_range",
            json!({
                "project": "agent:c:p",
                "path": "src/main.rs",
                "start_line": 2,
                "end_line": 4,
                "new_text": "replacement",
                "expected_old_prefix": "old"
            }),
        )
        .unwrap();
        assert!(matches!(
            replace_lines,
            ToolCall::ReplaceLineRange { project, path, start_line, end_line, new_text, expected_old_prefix, .. }
                if project == "agent:c:p"
                && path == "src/main.rs"
                && start_line == 2
                && end_line == 4
                && new_text == "replacement"
                && expected_old_prefix.as_deref() == Some("old")
        ));

        let insert = ToolCall::from_tool_name(
            "insert_at_line",
            json!({"project": "agent:c:p", "path": "src/main.rs", "line": 1, "text": "use x;"}),
        )
        .unwrap();
        assert!(matches!(insert, ToolCall::InsertAtLine { line: 1, .. }));

        let delete = ToolCall::from_tool_name(
            "delete_line_range",
            json!({"project": "agent:c:p", "path": "src/main.rs", "start_line": 8, "end_line": 9}),
        )
        .unwrap();
        assert!(matches!(
            delete,
            ToolCall::DeleteLineRange {
                start_line: 8,
                end_line: 9,
                ..
            }
        ));
    }

    #[test]
    fn from_tool_name_parses_replace_exact_block() {
        let call = ToolCall::from_tool_name(
            "replace_exact_block",
            json!({
                "project": "agent:c:p",
                "path": "src/main.rs",
                "old_text": "old",
                "new_text": "new",
                "expected_old_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            }),
        )
        .unwrap();
        assert!(matches!(
            call,
            ToolCall::ReplaceExactBlock { project, path, old_text, new_text, expected_old_sha256, .. }
                if project == "agent:c:p"
                && path == "src/main.rs"
                && old_text == "old"
                && new_text == "new"
                && expected_old_sha256.is_some()
        ));
    }

    #[test]
    fn from_tool_name_parses_insert_before_pattern() {
        let call = ToolCall::from_tool_name(
            "insert_before_pattern",
            json!({"project": "agent:c:p", "path": "src/main.rs", "pattern": "fn main", "text": "// before\n"}),
        )
        .unwrap();
        assert!(matches!(
            call,
            ToolCall::InsertBeforePattern { project, path, pattern, text, .. }
                if project == "agent:c:p" && path == "src/main.rs" && pattern == "fn main" && text == "// before\n"
        ));
    }

    #[test]
    fn from_tool_name_parses_insert_after_pattern() {
        let call = ToolCall::from_tool_name(
            "insert_after_pattern",
            json!({"project": "agent:c:p", "path": "src/main.rs", "pattern": "fn main", "text": " // after"}),
        )
        .unwrap();
        assert!(matches!(
            call,
            ToolCall::InsertAfterPattern { project, path, pattern, text, .. }
                if project == "agent:c:p" && path == "src/main.rs" && pattern == "fn main" && text == " // after"
        ));
    }

    #[test]
    fn known_tool_names_includes_phase4_edit_tools() {
        assert!(KNOWN_TOOL_NAMES.contains(&"replace_in_file"));
        assert!(KNOWN_TOOL_NAMES.contains(&"replace_exact_block"));
        assert!(KNOWN_TOOL_NAMES.contains(&"insert_before_pattern"));
        assert!(KNOWN_TOOL_NAMES.contains(&"insert_after_pattern"));
        assert!(KNOWN_TOOL_NAMES.contains(&"write_project_file"));
        assert!(KNOWN_TOOL_NAMES.contains(&"replace_line_range"));
        assert!(KNOWN_TOOL_NAMES.contains(&"insert_at_line"));
        assert!(KNOWN_TOOL_NAMES.contains(&"delete_line_range"));
    }

    #[test]
    fn tool_specs_include_anchor_edit_tools() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for required in [
            "replace_exact_block",
            "insert_before_pattern",
            "insert_after_pattern",
        ] {
            let spec = specs
                .iter()
                .find(|s| s.name == required)
                .expect("anchor edit spec");
            assert!(spec.description.contains("literal"), "{}", spec.description);
            assert!(
                spec.description.contains("no regex"),
                "{}",
                spec.description
            );
        }
    }

    #[test]
    fn tool_specs_include_phase4_edit_tools() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        for required in [
            "replace_in_file",
            "replace_exact_block",
            "insert_before_pattern",
            "insert_after_pattern",
            "write_project_file",
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
        ] {
            assert!(
                names.iter().any(|n| n == required),
                "tool_specs must include {}: {:?}",
                required,
                names
            );
        }
        for spec in runtime.tool_specs() {
            assert!(
                spec.description.chars().count() <= 300,
                "{} description too long ({} chars)",
                spec.name,
                spec.description.chars().count()
            );
        }
    }

    #[test]
    fn tool_categories_include_edit_group() {
        let runtime = test_runtime();
        let cats = runtime.tool_categories();
        let edit = cats["edit"].as_array().expect("edit category present");
        assert!(edit.iter().any(|v| v == "replace_in_file"));
        assert!(edit.iter().any(|v| v == "write_project_file"));
        assert!(edit.iter().any(|v| v == "replace_line_range"));
        assert!(edit.iter().any(|v| v == "insert_at_line"));
        assert!(edit.iter().any(|v| v == "delete_line_range"));
    }

    #[test]
    fn validate_edit_file_path_rejects_unsafe_and_sensitive_paths() {
        // Safe relative paths accepted.
        assert!(validate_edit_file_path("README.md").is_ok());
        assert!(validate_edit_file_path("src/main.rs").is_ok());
        assert!(validate_edit_file_path("a/b/c.txt").is_ok());
        // Empty / NUL / absolute / traversal rejected.
        assert!(validate_edit_file_path("").is_err());
        assert!(validate_edit_file_path("src\0main.rs").is_err());
        assert!(validate_edit_file_path("/etc/passwd").is_err());
        assert!(validate_edit_file_path("../outside").is_err());
        assert!(validate_edit_file_path("src/../../outside").is_err());
        // Sensitive paths hard-rejected.
        for sensitive in [
            "agent.toml",
            "config/agent.toml",
            "agent.toml.bak",
            "webcodex.env",
            ".env",
            ".env.local",
            "secrets/projects.d/x",
            "projects.d",
            ".git/config",
            "target/debug/bin",
            "node_modules/pkg/index.js",
        ] {
            assert!(
                validate_edit_file_path(sensitive).is_err(),
                "sensitive path should be rejected: {}",
                sensitive
            );
        }
    }

    #[test]
    fn is_sensitive_edit_path_is_component_wise_not_substring() {
        // Component-wise: a filename that merely contains a sensitive token
        // as a substring is NOT rejected.
        assert!(!is_sensitive_edit_path("targeting.md"));
        assert!(!is_sensitive_edit_path("enviroment.rs"));
        assert!(!is_sensitive_edit_path("docs/agent-toml-notes.md"));
        // Exact component matches ARE rejected.
        assert!(is_sensitive_edit_path("target/foo"));
        assert!(is_sensitive_edit_path(".git/HEAD"));
        assert!(is_sensitive_edit_path("node_modules/x"));
        assert!(is_sensitive_edit_path("a/b/.env"));
    }

    #[test]
    fn is_hex_sha256_validates_lowercase_digest() {
        assert!(is_hex_sha256(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ));
        assert!(!is_hex_sha256("abc"));
        assert!(!is_hex_sha256(
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        ));
        assert!(!is_hex_sha256(
            "z3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ));
    }

    #[test]
    fn replace_line_range_content_replaces_middle_multiline() {
        let (updated, out) = files::apply_line_edit_content(
            "one\ntwo\nthree\nfour\n",
            "src/example.rs",
            files::LineEditOperation::Replace,
            Some(2),
            Some(3),
            None,
            "TWO\nTHREE",
            None,
            None,
        )
        .unwrap();
        assert_eq!(updated, "one\nTWO\nTHREE\nfour\n");
        assert_eq!(out["path"], "src/example.rs");
        assert_eq!(out["start_line"], 2);
        assert_eq!(out["end_line"], 3);
        assert_eq!(out["old_line_count"], 2);
        assert_eq!(out["new_line_count"], 2);
        assert_eq!(out["changed"], true);
    }

    #[test]
    fn replace_line_range_content_rejects_sha_mismatch_without_write() {
        let original = "one\ntwo\nthree\n";
        let err = files::apply_line_edit_content(
            original,
            "src/example.rs",
            files::LineEditOperation::Replace,
            Some(2),
            Some(2),
            None,
            "TWO",
            Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            None,
        )
        .unwrap_err();
        assert!(err.contains("Rejected before write"));
        assert!(err.contains("expected_old_sha256 mismatch"));
        assert!(err.contains("No files were modified"));
        assert!(err.contains("Retry guidance"));
        assert_eq!(original, "one\ntwo\nthree\n");
    }

    #[test]
    fn line_edit_guard_failure_reports_no_files_modified_and_retry_guidance() {
        let err = files::apply_line_edit_content(
            "one\ntwo\nthree\n",
            "src/example.rs",
            files::LineEditOperation::Insert,
            None,
            None,
            Some(2),
            "inserted",
            None,
            Some("not-the-anchor"),
        )
        .unwrap_err();
        assert!(err.contains("Rejected before write"));
        assert!(err.contains("expected_anchor_prefix mismatch"));
        assert!(err.contains("No files were modified"));
        assert!(err.contains("Retry guidance"));
        assert!(err.contains("read the file again"));
    }

    #[test]
    fn replace_line_range_content_rejects_out_of_range() {
        let err = files::apply_line_edit_content(
            "one\ntwo\n",
            "src/example.rs",
            files::LineEditOperation::Replace,
            Some(2),
            Some(3),
            None,
            "x",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err, "invalid line range");
    }

    #[test]
    fn insert_at_line_content_inserts_start_middle_and_eof() {
        let (start, out) = files::apply_line_edit_content(
            "one\ntwo\n",
            "src/example.rs",
            files::LineEditOperation::Insert,
            None,
            None,
            Some(1),
            "zero",
            None,
            None,
        )
        .unwrap();
        assert_eq!(start, "zero\none\ntwo\n");
        assert_eq!(out["line"], 1);
        assert_eq!(out["old_line_count"], 1);

        let (middle, _) = files::apply_line_edit_content(
            "one\ntwo\n",
            "src/example.rs",
            files::LineEditOperation::Insert,
            None,
            None,
            Some(2),
            "middle\n",
            None,
            None,
        )
        .unwrap();
        assert_eq!(middle, "one\nmiddle\ntwo\n");

        let (eof, out) = files::apply_line_edit_content(
            "one\ntwo\n",
            "src/example.rs",
            files::LineEditOperation::Insert,
            None,
            None,
            Some(3),
            "three",
            None,
            None,
        )
        .unwrap();
        assert_eq!(eof, "one\ntwo\nthree\n");
        assert_eq!(out["old_line_count"], 0);
    }

    #[test]
    fn insert_at_line_content_rejects_anchor_prefix_mismatch() {
        let err = files::apply_line_edit_content(
            "one\ntwo\n",
            "src/example.rs",
            files::LineEditOperation::Insert,
            None,
            None,
            Some(2),
            "middle",
            None,
            Some("three"),
        )
        .unwrap_err();
        assert!(err.contains("expected_anchor_prefix mismatch"));
        assert!(err.contains("No files were modified"));
    }

    #[test]
    fn delete_line_range_content_deletes_single_and_multiple_lines() {
        let (single, out) = files::apply_line_edit_content(
            "one\ntwo\nthree\n",
            "src/example.rs",
            files::LineEditOperation::Delete,
            Some(2),
            Some(2),
            None,
            "",
            None,
            None,
        )
        .unwrap();
        assert_eq!(single, "one\nthree\n");
        assert_eq!(out["old_line_count"], 1);
        assert_eq!(out["new_line_count"], 0);

        let (multi, _) = files::apply_line_edit_content(
            "one\ntwo\nthree\nfour\n",
            "src/example.rs",
            files::LineEditOperation::Delete,
            Some(2),
            Some(3),
            None,
            "",
            None,
            None,
        )
        .unwrap();
        assert_eq!(multi, "one\nfour\n");
    }

    #[test]
    fn delete_line_range_content_rejects_out_of_range() {
        let err = files::apply_line_edit_content(
            "one\n",
            "src/example.rs",
            files::LineEditOperation::Delete,
            Some(1),
            Some(2),
            None,
            "",
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err, "invalid line range");
    }

    #[tokio::test]
    async fn line_edit_tools_reject_oversized_expected_prefix_before_agent_dispatch() {
        let runtime = test_runtime();
        let big_prefix = "x".repeat(MAX_EXPECTED_PREFIX_BYTES + 1);

        let result = runtime
            .replace_line_range(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                1,
                1,
                "new".to_string(),
                None,
                Some(big_prefix.clone()),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("expected prefix too large"));

        let result = runtime
            .insert_at_line(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                1,
                "new".to_string(),
                None,
                Some(big_prefix.clone()),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("expected prefix too large"));

        let result = runtime
            .delete_line_range(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                1,
                1,
                None,
                Some(big_prefix),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("expected prefix too large"));
    }

    #[tokio::test]
    async fn line_edit_dispatch_uses_agent_native_file_op_not_python_helper() {
        let runtime = runtime_with_agent_project("editor");
        register_agent(&runtime, "editor", None, ShellClientCapabilities::default()).await;
        let project = agent_test_project_id("editor");

        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .replace_line_range(
                    project,
                    "EDIT_PROBE.txt".to_string(),
                    2,
                    2,
                    "new".to_string(),
                    None,
                    Some("old".to_string()),
                )
                .await
        });

        let mut req = None;
        for _ in 0..20 {
            req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "editor".to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if req.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let req = req.expect("replace_line_range should enqueue an agent file op");
        assert_eq!(req.kind, "file_replace_line_range");
        assert_eq!(req.command, "");
        assert!(req.stdin.is_none());
        assert_eq!(req.path.as_deref(), Some("EDIT_PROBE.txt"));
        assert_eq!(req.content.as_deref(), Some("new"));
        assert_eq!(req.start_line, Some(2));
        assert_eq!(req.end_line, Some(2));
        assert_eq!(req.expected_prefix.as_deref(), Some("old"));

        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "editor".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(
                    "{\"changed\":true,\"path\":\"EDIT_PROBE.txt\",\
                     \"old_sha256\":\"b\",\"new_sha256\":\"a\",\
                     \"old_line_count\":1,\"new_line_count\":1,\"bytes_written\":4,\
                     \"start_line\":2,\"end_line\":2}"
                        .to_string(),
                ),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();

        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["changed"], true);
        assert_eq!(result.output["path"], "EDIT_PROBE.txt");
    }

    #[tokio::test]
    async fn replace_in_file_rejects_invalid_input_before_agent_dispatch() {
        // Call the method directly (bypassing authorize_agent_tool, which would
        // otherwise resolve the project first) so we prove input validation
        // fires before any agent request is enqueued. A test_runtime() has no
        // registered agents, so a request that reached dispatch would hang;
        // these all return early with a validation error.
        let runtime = test_runtime();
        let cases: Vec<(String, String, String)> = vec![
            // empty old
            (
                "EDIT_PROBE.txt".to_string(),
                "".to_string(),
                "x".to_string(),
            ),
            // NUL in old
            (
                "EDIT_PROBE.txt".to_string(),
                "a\0b".to_string(),
                "x".to_string(),
            ),
            // NUL in new
            (
                "EDIT_PROBE.txt".to_string(),
                "a".to_string(),
                "x\0y".to_string(),
            ),
            // sensitive path
            ("agent.toml".to_string(), "a".to_string(), "b".to_string()),
            // absolute path
            ("/etc/passwd".to_string(), "a".to_string(), "b".to_string()),
            // traversal path
            ("../x".to_string(), "a".to_string(), "b".to_string()),
        ];
        for (path, old, new) in cases {
            let result = runtime
                .replace_in_file("agent:c:p".to_string(), path, old, new, None, None)
                .await;
            assert!(!result.success, "expected validation failure");
            let err = result.error.unwrap();
            // Must NOT be the project-resolution error — proves early reject.
            assert!(
                !err.contains("shell client") && !err.contains("projects.toml"),
                "should fail input validation before project resolution: {}",
                err
            );
        }
        // expected_replacements < 1 rejected.
        let result = runtime
            .replace_in_file(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                "a".to_string(),
                "b".to_string(),
                Some(0),
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("expected_replacements"));

        // expected_replacements > 1 requires allow_multiple=true, otherwise
        // the caller's requested count would be ambiguous.
        let result = runtime
            .replace_in_file(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                "a".to_string(),
                "b".to_string(),
                Some(2),
                Some(false),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("allow_multiple"));
    }

    #[tokio::test]
    async fn write_project_file_rejects_invalid_input_before_agent_dispatch() {
        let runtime = test_runtime();
        // NUL content
        let result = runtime
            .write_project_file(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                "a\0b".to_string(),
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("NUL"));
        // sensitive path
        let result = runtime
            .write_project_file(
                "agent:c:p".to_string(),
                ".env".to_string(),
                "x".to_string(),
                None,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("sensitive"));
        // bad expected_sha256 format
        let result = runtime
            .write_project_file(
                "agent:c:p".to_string(),
                "EDIT_PROBE.txt".to_string(),
                "x".to_string(),
                Some(true),
                Some("not-a-hash".to_string()),
                None,
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("expected_sha256"));
    }

    #[tokio::test]
    async fn replace_in_file_rejects_server_configured_project() {
        // A server-configured (local) project is not an agent-registered
        // runtime surface; replace_in_file must refuse it.
        let tmp = tempfile::tempdir().unwrap();
        let runtime = runtime_with_local_project(tmp.path(), "demo");
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "hello").unwrap();
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ReplaceInFile {
                    project: "demo".to_string(),
                    path: "EDIT_PROBE.txt".to_string(),
                    old: "hello".to_string(),
                    new: "world".to_string(),
                    session_id: None,
                    expected_replacements: None,
                    allow_multiple: None,
                },
                None,
            )
            .await;
        assert!(!result.success);
        let err = result.error.unwrap();
        assert!(
            err.contains("agent-registered") || err.contains("unknown_project"),
            "should reject server-configured project: {}",
            err
        );
        // File must be unchanged — the server never wrote it.
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "hello"
        );
    }

    #[tokio::test]
    async fn replace_in_file_routes_to_owning_agent_with_fixed_helper() {
        let runtime = runtime_with_agent_project("editor");
        let mut caps = ShellClientCapabilities::default();
        caps.shell = true;
        register_agent(&runtime, "editor", None, caps).await;
        let project = agent_test_project_id("editor");

        let runtime_for_task = runtime.clone();
        let project_for_task = project.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .replace_in_file(
                    project_for_task,
                    "EDIT_PROBE.txt".to_string(),
                    "foo".to_string(),
                    "bar".to_string(),
                    None,
                    None,
                )
                .await
        });

        // Drain requests until the helper run arrives.
        let mut req = None;
        for _ in 0..20 {
            req = runtime
                .shell_clients
                .poll(ShellAgentPollRequest {
                    client_id: "editor".to_string(),
                    agent_instance_id: "inst".to_string(),
                    projects: None,
                })
                .await
                .unwrap();
            if req.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
        let req = req.expect("replace_in_file should enqueue a helper run for the agent");
        // The command is the FIXED python3 helper — no caller content interpolated.
        assert!(
            req.command.starts_with("python3 -c '"),
            "command must be the fixed helper, got: {}",
            req.command
        );
        assert!(
            !req.command.contains("foo") && !req.command.contains("EDIT_PROBE"),
            "caller content must not be interpolated into the command: {}",
            req.command
        );
        // old/new/path travel over stdin as JSON.
        let stdin = req.stdin.expect("helper payload on stdin");
        assert!(stdin.contains("EDIT_PROBE.txt"));
        assert!(stdin.contains("foo"));
        assert!(stdin.contains("bar"));
        assert!(stdin.contains("\"expected_replacements\":1"));
        assert!(stdin.contains("\"allow_multiple\":false"));
        // The agent (server side) never reads the agent fs: respond with a
        // canned JSON result that the runtime forwards verbatim.
        runtime
            .shell_clients
            .complete(ShellAgentResultRequest {
                client_id: "editor".to_string(),
                agent_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(
                    "{\"changed\":true,\"path\":\"EDIT_PROBE.txt\",\"replacements\":1,\
                     \"before_sha256\":\"b\",\"after_sha256\":\"a\",\"bytes_written\":3}"
                        .to_string(),
                ),
                stderr: Some(String::new()),
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();

        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["changed"], true);
        assert_eq!(result.output["replacements"], 1);
        assert_eq!(result.output["path"], "EDIT_PROBE.txt");
    }

    #[tokio::test]
    async fn replace_in_file_requires_shell_capability() {
        let runtime = runtime_with_agent_project("editor");
        // Register WITHOUT shell capability (default has shell=true, so set
        // shell=false explicitly).
        register_agent(
            &runtime,
            "editor",
            None,
            ShellClientCapabilities {
                shell: false,
                ..Default::default()
            },
        )
        .await;
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ReplaceInFile {
                    project: agent_test_project_id("editor"),
                    path: "EDIT_PROBE.txt".to_string(),
                    old: "foo".to_string(),
                    new: "bar".to_string(),
                    session_id: None,
                    expected_replacements: None,
                    allow_multiple: None,
                },
                Some(&auth_context(None, true)),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("shell"));
    }

    #[tokio::test]
    async fn line_edit_tools_require_file_write_capability() {
        let runtime = runtime_with_agent_project("editor");
        register_agent(&runtime, "editor", None, ShellClientCapabilities::default()).await;
        let result = runtime
            .dispatch_with_auth(
                ToolCall::ReplaceLineRange {
                    project: agent_test_project_id("editor"),
                    path: "EDIT_PROBE.txt".to_string(),
                    start_line: 1,
                    end_line: 1,
                    new_text: "new".to_string(),
                    session_id: None,
                    expected_old_sha256: None,
                    expected_old_prefix: None,
                },
                Some(&auth_context(None, true)),
            )
            .await;
        assert!(!result.success);
        assert!(result.error.unwrap().contains("file_write"));
    }

    // -------------------------------------------------------------------------
    // Python helper integration: run the actual fixed helper scripts locally
    // against temp files (python3 is required by the e2e suite and the agent
    // host; these tests skip gracefully when python3 is not on PATH so cargo
    // test stays green on minimal CI).
    // -------------------------------------------------------------------------

    fn python3_available() -> bool {
        std::process::Command::new("python3")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Run a fixed helper script locally with a JSON payload on stdin, in the
    /// given cwd, and return the parsed JSON object the helper prints.
    fn run_helper_locally(helper: &str, payload: &Value, cwd: &Path) -> Value {
        let stdin = serde_json::to_string(payload).unwrap();
        let mut child = std::process::Command::new("python3")
            .arg("-c")
            .arg(helper)
            .current_dir(cwd)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("spawn python3");
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "helper exited {:?}: stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!(
                "helper returned invalid JSON: {} (got: {})",
                e,
                stdout.trim()
            )
        })
    }

    #[test]
    fn helper_replace_in_file_single_replacement_success() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "world",
            "new": "rust",
            "expected_replacements": 1,
            "allow_multiple": false
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], true);
        assert_eq!(out["replacements"], 1);
        assert_eq!(out["before_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(out["after_sha256"].as_str().unwrap().len(), 64);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "hello rust"
        );
    }

    #[test]
    fn helper_replace_in_file_old_missing_leaves_file_unchanged() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "missing",
            "new": "x",
            "expected_replacements": 1,
            "allow_multiple": false
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], false);
        assert!(out["error"].as_str().unwrap().contains("not found"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn helper_replace_in_file_multiple_without_allow_multiple_fails() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "a",
            "new": "b",
            "expected_replacements": 1,
            "allow_multiple": false
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], false);
        assert!(out["error"].as_str().unwrap().contains("multiple"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "a a a"
        );
    }

    #[test]
    fn helper_replace_in_file_rejects_expected_multiple_without_allow_multiple() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "world",
            "new": "rust",
            "expected_replacements": 2,
            "allow_multiple": false
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], false);
        assert!(out["error"].as_str().unwrap().contains("allow_multiple"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn helper_replace_in_file_allow_multiple_exact_count_succeeds() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "a",
            "new": "b",
            "expected_replacements": 3,
            "allow_multiple": true
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], true);
        assert_eq!(out["replacements"], 3);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "b b b"
        );
    }

    #[test]
    fn helper_replace_in_file_allow_multiple_count_mismatch_fails() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "a",
            "new": "b",
            "expected_replacements": 2,
            "allow_multiple": true
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], false);
        assert!(out["error"].as_str().unwrap().contains("mismatch"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "a a a"
        );
    }

    #[test]
    fn helper_replace_in_file_rejects_empty_old_and_nul() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.txt"), "x").unwrap();
        let payload = json!({
            "path": "f.txt",
            "old": "",
            "new": "y",
            "expected_replacements": 1,
            "allow_multiple": false
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], false);
        assert!(out["error"].as_str().unwrap().contains("old"));
        // File unchanged.
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
            "x"
        );
    }

    #[test]
    fn helper_replace_in_file_rejects_non_utf8_file() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("f.bin"), [0xFF, 0xFE, 0xFD]).unwrap();
        let payload = json!({
            "path": "f.bin",
            "old": "x",
            "new": "y",
            "expected_replacements": 1,
            "allow_multiple": false
        });
        let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["changed"], false);
        assert!(out["error"].as_str().unwrap().contains("UTF-8"));
    }

    #[test]
    fn validate_artifact_file_path_rejects_sensitive_paths() {
        assert!(validate_artifact_file_path("docs/assets/generated.png").is_ok());
        for path in [
            "../evil.png",
            ".git/config",
            ".env",
            "secrets/key.pem",
            "tokens/api.txt",
            "target/out.bin",
            "node_modules/pkg/file",
        ] {
            assert!(
                validate_artifact_file_path(path).is_err(),
                "{} should be rejected",
                path
            );
        }
    }

    #[test]
    fn helper_save_project_artifact_writes_binary_and_blocks_overwrite() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let payload = json!({
            "path": "artifacts/imports/tiny.png",
            "content_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0x89, b'P', b'N', b'G']),
            "mime_type": "image/png",
            "overwrite": false,
            "max_bytes": 1024
        });
        let out = run_helper_locally(SAVE_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
        assert_eq!(out["bytes_written"], 4);
        assert_eq!(out["mime_type"], "image/png");
        assert!(out["sha256"].as_str().unwrap().len() == 64);
        assert_eq!(
            std::fs::read(tmp.path().join("artifacts/imports/tiny.png")).unwrap(),
            vec![0x89, b'P', b'N', b'G']
        );

        let out2 = run_helper_locally(SAVE_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
        assert!(out2["error"]
            .as_str()
            .unwrap()
            .contains("overwrite is false"));
    }

    #[test]
    fn helper_read_project_artifact_metadata_counts_zip_without_extracting() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("sample.zip");
        let status = std::process::Command::new("python3")
            .arg("-c")
            .arg("import zipfile; z=zipfile.ZipFile('sample.zip','w'); z.writestr('a.txt','a'); z.writestr('b.txt','b'); z.close()")
            .current_dir(tmp.path())
            .status()
            .unwrap();
        assert!(status.success());
        assert!(zip_path.exists());
        let payload = json!({"path": "sample.zip", "max_bytes": 1024 * 1024});
        let out = run_helper_locally(READ_PROJECT_ARTIFACT_METADATA_HELPER, &payload, tmp.path());
        assert_eq!(out["mime_type"], "application/zip");
        assert_eq!(out["archive_entries_count"], 2);
        assert!(!tmp.path().join("a.txt").exists());
        assert!(!tmp.path().join("b.txt").exists());
    }

    #[test]
    fn helper_read_project_artifact_reads_small_png_single_chunk_and_matches_metadata() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let png = [
            0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 13, b'I', b'H', b'D', b'R',
            0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89,
        ];
        std::fs::write(tmp.path().join("tiny.png"), png).unwrap();
        let metadata_payload = json!({"path": "tiny.png", "max_bytes": 1024});
        let metadata = run_helper_locally(
            READ_PROJECT_ARTIFACT_METADATA_HELPER,
            &metadata_payload,
            tmp.path(),
        );
        let payload = json!({"path": "tiny.png", "offset": 0, "length": 1024});
        let out = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
        assert_eq!(out["mime_type"], "image/png");
        assert_eq!(out["file_bytes"], png.len());
        assert_eq!(out["sha256"], metadata["sha256"]);
        assert_eq!(out["offset"], 0);
        assert_eq!(out["bytes_returned"], png.len());
        assert_eq!(out["next_offset"], png.len());
        assert_eq!(out["truncated"], false);
        assert_eq!(
            out["content_base64"],
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
        );
    }

    #[test]
    fn helper_read_project_artifact_reads_multiple_chunks() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let bytes = b"abcdefghijkl";
        std::fs::write(tmp.path().join("data.bin"), bytes).unwrap();

        let first_payload = json!({"path": "data.bin", "offset": 0, "length": 5});
        let first = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &first_payload, tmp.path());
        assert_eq!(first["file_bytes"], bytes.len());
        assert_eq!(first["offset"], 0);
        assert_eq!(first["bytes_returned"], 5);
        assert_eq!(first["next_offset"], 5);
        assert_eq!(first["truncated"], true);
        assert_eq!(
            first["content_base64"],
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..5])
        );

        let second_payload = json!({"path": "data.bin", "offset": 5, "length": 20});
        let second = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &second_payload, tmp.path());
        assert_eq!(second["sha256"], first["sha256"]);
        assert_eq!(second["offset"], 5);
        assert_eq!(second["bytes_returned"], bytes.len() - 5);
        assert_eq!(second["next_offset"], bytes.len());
        assert_eq!(second["truncated"], false);
        assert_eq!(
            second["content_base64"],
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[5..])
        );
    }

    #[test]
    fn helper_read_project_artifact_offset_at_eof_returns_empty_chunk() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("data.bin"), b"abc").unwrap();
        let payload = json!({"path": "data.bin", "offset": 3, "length": 10});
        let out = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
        assert_eq!(out["file_bytes"], 3);
        assert_eq!(out["offset"], 3);
        assert_eq!(out["bytes_returned"], 0);
        assert_eq!(out["content_base64"], "");
        assert_eq!(out["next_offset"], 3);
        assert_eq!(out["truncated"], false);
    }

    #[test]
    fn helper_read_project_artifact_rejects_invalid_offset_and_length() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("data.bin"), b"abc").unwrap();
        let bad_offset = run_helper_locally(
            READ_PROJECT_ARTIFACT_HELPER,
            &json!({"path": "data.bin", "offset": -1, "length": 1}),
            tmp.path(),
        );
        assert!(bad_offset["error"].as_str().unwrap().contains("offset"));
        let bad_length = run_helper_locally(
            READ_PROJECT_ARTIFACT_HELPER,
            &json!({"path": "data.bin", "offset": 0, "length": 0}),
            tmp.path(),
        );
        assert!(bad_length["error"].as_str().unwrap().contains("length"));
    }

    #[tokio::test]
    async fn read_project_artifact_rejects_sensitive_path_before_resolving_project() {
        let out = test_runtime()
            .read_project_artifact(
                "agent:missing:missing".to_string(),
                ".env".to_string(),
                None,
                None,
                None,
                None,
            )
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("sensitive artifact path"));
    }

    #[tokio::test]
    async fn read_project_artifact_rejects_invalid_length_before_resolving_project() {
        let out = test_runtime()
            .read_project_artifact(
                "agent:missing:missing".to_string(),
                "docs/assets/file.png".to_string(),
                None,
                None,
                Some(crate::tool_runtime::files::MAX_READ_PROJECT_ARTIFACT_LENGTH + 1),
                None,
            )
            .await;
        assert!(!out.success);
        assert!(out.error.unwrap().contains("length too large"));
    }

    #[test]
    fn helper_write_project_file_creates_new_file() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "line1\nline2\n",
            "overwrite": false,
            "expected_sha256": null,
            "expected_content_prefix": null
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["created"], true);
        assert_eq!(out["overwritten"], false);
        assert_eq!(out["bytes_written"], 12);
        assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(out["warning"], Value::Null);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "line1\nline2\n"
        );
    }

    #[test]
    fn helper_write_project_file_existing_without_overwrite_fails() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "new",
            "overwrite": false,
            "expected_sha256": null,
            "expected_content_prefix": null
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["created"], false);
        assert!(out["error"].as_str().unwrap().contains("overwrite"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "original"
        );
    }

    #[test]
    fn helper_write_project_file_overwrite_with_matching_sha256_succeeds() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
        let sha = sha256_hex("original");
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "replaced",
            "overwrite": true,
            "expected_sha256": sha,
            "expected_content_prefix": null
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["overwritten"], true);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "replaced"
        );
    }

    #[test]
    fn helper_write_project_file_overwrite_with_mismatched_sha256_fails() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "replaced",
            "overwrite": true,
            "expected_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "expected_content_prefix": null
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["created"], false);
        assert!(out["error"].as_str().unwrap().contains("sha256"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "original"
        );
    }

    #[test]
    fn helper_write_project_file_overwrite_with_matching_prefix_succeeds() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "v1 content").unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "v1 replaced",
            "overwrite": true,
            "expected_sha256": null,
            "expected_content_prefix": "v1 "
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["overwritten"], true);
        assert_eq!(out["warning"], Value::Null);
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "v1 replaced"
        );
    }

    #[test]
    fn helper_write_project_file_overwrite_with_mismatched_prefix_fails() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "v2 content").unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "x",
            "overwrite": true,
            "expected_sha256": null,
            "expected_content_prefix": "v1 "
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["created"], false);
        assert!(out["error"].as_str().unwrap().contains("prefix"));
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "v2 content"
        );
    }

    #[test]
    fn helper_write_project_file_overwrite_without_guards_warns() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "replaced",
            "overwrite": true,
            "expected_sha256": null,
            "expected_content_prefix": null
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["overwritten"], true);
        assert!(
            out["warning"].as_str().unwrap().contains("expected_sha256"),
            "should warn about missing guard: {:?}",
            out["warning"]
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
            "replaced"
        );
    }

    #[test]
    fn helper_write_project_file_rejects_nul_content() {
        if !python3_available() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let payload = json!({
            "path": "EDIT_PROBE.txt",
            "content": "a\u{0000}b",
            "overwrite": false,
            "expected_sha256": null,
            "expected_content_prefix": null
        });
        let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
        assert_eq!(out["created"], false);
        assert!(out["error"].as_str().unwrap().contains("NUL"));
        assert!(!tmp.path().join("EDIT_PROBE.txt").exists());
    }

    /// Compute a lowercase hex sha256 of a string (test helper).
    fn sha256_hex(s: &str) -> String {
        // Use the same approach as the python helper: sha256 of utf-8 bytes.
        // We shell out to python3 to avoid pulling a sha256 crate into tests.
        let child = std::process::Command::new("python3")
            .arg("-c")
            .arg("import sys,hashlib;sys.stdout.write(hashlib.sha256(sys.argv[1].encode()).hexdigest())")
            .arg(s)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("spawn python3");
        let output = child.wait_with_output().unwrap();
        String::from_utf8(output.stdout).unwrap()
    }

    // =========================================================================
    // Project management tools (register_project / create_project)
    // =========================================================================

    #[test]
    fn known_tool_names_includes_project_management_tools() {
        assert!(
            KNOWN_TOOL_NAMES.contains(&"register_project"),
            "KNOWN_TOOL_NAMES must include register_project"
        );
        assert!(
            KNOWN_TOOL_NAMES.contains(&"create_project"),
            "KNOWN_TOOL_NAMES must include create_project"
        );
    }

    #[test]
    fn tool_specs_include_project_management_tools() {
        let runtime = test_runtime();
        let names: Vec<String> = runtime
            .tool_specs()
            .iter()
            .map(|s| s.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n == "register_project"),
            "tool_specs must include register_project: {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n == "create_project"),
            "tool_specs must include create_project: {:?}",
            names
        );
        // Verify the specs carry the required-field schema.
        for spec in runtime.tool_specs() {
            if spec.name == "register_project" || spec.name == "create_project" {
                let required = spec.input_schema["required"]
                    .as_array()
                    .unwrap_or_else(|| panic!("{} must have required array", spec.name));
                for field in ["client_id", "id", "name", "path"] {
                    assert!(
                        required.iter().any(|v| v == field),
                        "{} input_schema must require '{}'",
                        spec.name,
                        field
                    );
                }
            }
        }
    }

    #[test]
    fn tool_categories_include_projects_with_management_tools() {
        let runtime = test_runtime();
        let cats = runtime.tool_categories();
        let projects = cats["projects"]
            .as_array()
            .expect("projects category present");
        assert!(
            projects.iter().any(|v| v == "register_project"),
            "projects category must include register_project"
        );
        assert!(
            projects.iter().any(|v| v == "create_project"),
            "projects category must include create_project"
        );
    }

    #[test]
    fn from_tool_name_parses_project_management_tools() {
        let register = ToolCall::from_tool_name(
            "register_project",
            json!({
                "client_id":"oe",
                "id":"my-project",
                "name":"My Project",
                "path":"/root/git/my-project"
            }),
        )
        .unwrap();
        assert!(matches!(
            register,
            ToolCall::RegisterProject { ref client_id, ref id, ref name, ref path, .. }
                if client_id == "oe" && id == "my-project" && name == "My Project"
                && path == "/root/git/my-project"
        ));

        let create = ToolCall::from_tool_name(
            "create_project",
            json!({
                "client_id":"oe",
                "id":"hello",
                "name":"Hello",
                "path":"/root/git/hello",
                "template":"basic",
                "git_init":true
            }),
        )
        .unwrap();
        assert!(matches!(
            create,
            ToolCall::CreateProject { ref client_id, ref id, ref name, ref path, ref template, git_init, .. }
                if client_id == "oe" && id == "hello" && name == "Hello"
                && path == "/root/git/hello" && template.as_deref() == Some("basic")
                && git_init
        ));
    }

    #[tokio::test]
    async fn dispatch_register_project_rejects_unknown_client_id() {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::RegisterProject {
                client_id: "no-such-agent".to_string(),
                id: "my-project".to_string(),
                name: "My Project".to_string(),
                path: "/root/git/my-project".to_string(),
                description: None,
                allow_patch: true,
                overwrite: false,
            })
            .await;
        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("unknown agent"),
            "register_project should reject unknown client_id: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn dispatch_create_project_rejects_unknown_client_id() {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::CreateProject {
                client_id: "no-such-agent".to_string(),
                id: "hello".to_string(),
                name: "Hello".to_string(),
                path: "/root/git/hello".to_string(),
                description: None,
                allow_patch: true,
                template: None,
                git_init: false,
                allow_existing_empty: false,
                overwrite: false,
            })
            .await;
        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("unknown agent"),
            "create_project should reject unknown client_id: {:?}",
            result.error
        );
    }

    #[tokio::test]
    async fn dispatch_register_project_rejects_unsafe_id() {
        let runtime = test_runtime();
        for bad_id in ["", "a/b", "a\\b", "..", "a..b", "a\0b"] {
            let result = runtime
                .dispatch(ToolCall::RegisterProject {
                    client_id: "oe".to_string(),
                    id: bad_id.to_string(),
                    name: "Test".to_string(),
                    path: "/root/git/test".to_string(),
                    description: None,
                    allow_patch: true,
                    overwrite: false,
                })
                .await;
            assert!(
                !result.success,
                "register_project should reject unsafe id '{:?}'",
                bad_id
            );
        }
    }

    #[tokio::test]
    async fn dispatch_create_project_rejects_relative_path() {
        let runtime = test_runtime();
        let result = runtime
            .dispatch(ToolCall::CreateProject {
                client_id: "oe".to_string(),
                id: "hello".to_string(),
                name: "Hello".to_string(),
                path: "relative/path".to_string(),
                description: None,
                allow_patch: true,
                template: None,
                git_init: false,
                allow_existing_empty: false,
                overwrite: false,
            })
            .await;
        assert!(!result.success);
        assert!(
            result.error.as_deref().unwrap_or("").contains("absolute"),
            "create_project should reject relative path: {:?}",
            result.error
        );
    }
}
