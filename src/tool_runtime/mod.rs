//! Tool Runtime — unified execution layer for MCP and GPT Actions.
//!
//! Both protocol adapters call `ToolRuntime::dispatch()`.
//! No HTTP framework types here — pure Rust input/output.

mod cargo;
mod checkpoint;
mod codex;
pub(crate) mod files;
mod git;
mod handoff;
mod helpers;
mod hygiene;
mod jobs;
pub(crate) mod kernel;
pub(crate) mod metadata;
mod patch;
pub(crate) mod project_instructions;
mod projects;
mod registry;
pub(crate) mod sessions;
mod shell;
mod types;

// Re-export the public API so `crate::tool_runtime::ToolCall` etc. still work.
#[allow(unused_imports)]
pub use types::{
    default_true, is_known_tool_name, ApplyTextEditInput, ApplyTextEditKind, RuntimeInfo,
    SessionMode, ToolCall, ToolResult, ToolSpec, KNOWN_TOOL_NAMES,
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
    pub(crate) checkpoint_store: checkpoint::CheckpointStore,
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

fn session_message_error_result(
    session_id: &str,
    message_id: Option<&str>,
    error: sessions::SessionMessageError,
) -> ToolResult {
    match error {
        sessions::SessionMessageError::UnknownSession => unknown_session_result(session_id),
        sessions::SessionMessageError::UnknownMessage => ToolResult::err_with_output(
            match message_id {
                Some(message_id) => format!("unknown_message_id: {}", message_id),
                None => "unknown_message_id".to_string(),
            },
            json!({
                "error_kind": "unknown_message_id",
                "session_id": session_id,
                "message_id": message_id,
            }),
        ),
        sessions::SessionMessageError::InvalidInput(message) => ToolResult::err_with_output(
            message.clone(),
            json!({
                "error_kind": "invalid_session_message",
                "session_id": session_id,
                "error": message,
            }),
        ),
    }
}

fn current_session_unavailable_result(message: impl Into<String>) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        json!({
            "error_kind": "current_session_unavailable",
        }),
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
    // Preserve an existing business `session_id` in the tool output (e.g.
    // session_summary's required business input) instead of overwriting it
    // with the recorder session id. Only synthesize one when the tool output
    // does not already carry one.
    if !output.contains_key("session_id") {
        output.insert(
            "session_id".to_string(),
            Value::String(session_id.to_string()),
        );
    }
    if let Some(event_id) = event_id {
        output.insert("session_event_id".to_string(), Value::String(event_id));
    }
    result.output = Value::Object(output);
}

fn is_current_session_control_tool(call: &ToolCall) -> bool {
    matches!(
        call,
        ToolCall::BindCurrentSession { .. }
            | ToolCall::CurrentSession { .. }
            | ToolCall::UnbindCurrentSession { .. }
    )
}

fn is_current_session_eligible(call: &ToolCall) -> bool {
    // `session_handoff_summary` carries an optional project for workspace/
    // checkpoint enrichment, but its `session_id` is required business input
    // (the session to summarize), not a recorder session. It must never fall
    // back to the current-session binding.
    if matches!(call, ToolCall::SessionHandoffSummary { .. }) {
        return false;
    }
    call.project().is_some() && !is_current_session_control_tool(call)
}

fn current_session_key(
    auth: Option<&AuthContext>,
    transport: sessions::SessionTransport,
    resolved_project: &str,
) -> Result<sessions::CurrentSessionKey, String> {
    let (principal_kind, principal_id) = current_session_principal(auth)?;
    Ok(sessions::CurrentSessionKey {
        principal_kind,
        principal_id,
        transport: transport.as_str().to_string(),
        resolved_project: resolved_project.to_string(),
    })
}

fn current_session_principal(auth: Option<&AuthContext>) -> Result<(String, String), String> {
    let Some(auth) = auth else {
        return Ok(("dev".to_string(), "dev".to_string()));
    };
    if auth.is_bootstrap {
        return Ok((
            "bootstrap".to_string(),
            auth.user_id
                .as_deref()
                .or(auth.username.as_deref())
                .unwrap_or("bootstrap")
                .to_string(),
        ));
    }
    let id = if matches!(auth.kind, crate::auth::AuthKind::OpenAnonymous) {
        Some("open-anonymous".to_string())
    } else {
        auth.api_key_id
            .as_deref()
            .or(auth.user_id.as_deref())
            .or(auth.username.as_deref())
            .or(auth.allowed_client_id.as_deref())
            .or(auth.shared_key_hash.as_deref())
            .map(str::to_string)
    };
    let Some(principal_id) = id else {
        return Err(
            "current_session_unavailable: authenticated caller has no stable principal id"
                .to_string(),
        );
    };
    let principal_kind = match auth.kind {
        crate::auth::AuthKind::ApiToken => auth.token_kind.as_deref().unwrap_or("api_token"),
        crate::auth::AuthKind::AgentToken => "agent_token",
        crate::auth::AuthKind::AccountCredential => "account_credential",
        crate::auth::AuthKind::OAuth2Token => "oauth2",
        crate::auth::AuthKind::Bootstrap => "bootstrap",
        crate::auth::AuthKind::SharedKey => "shared-key",
        crate::auth::AuthKind::OpenAnonymous => "open",
    };
    Ok((principal_kind.to_string(), principal_id))
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
            checkpoint_store: checkpoint::CheckpointStore::default(),
            sessions: sessions::SessionStore::default(),
            local_jobs: Arc::new(Mutex::new(HashMap::new())),
            job_killer: Arc::new(SystemJobKiller),
        }
    }

    pub fn with_session_ledger(mut self, path: impl Into<PathBuf>) -> Self {
        self.sessions = sessions::SessionStore::with_persistence(
            path,
            sessions::DEFAULT_MAX_SESSIONS,
            sessions::DEFAULT_MAX_EVENTS_PER_SESSION,
        );
        self
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

    async fn agent_project_candidates_for_auth(
        &self,
        auth: Option<&AuthContext>,
    ) -> Vec<ProjectResolverCandidate> {
        let mut candidates = Vec::new();
        for client in self.shell_clients.list_clients_for_auth(auth).await {
            for project in client.projects.iter().filter(|project| !project.disabled) {
                candidates.push(Self::project_candidate_from_view(&client, project));
            }
        }
        Self::sort_resolver_candidates(&mut candidates);
        candidates
    }

    async fn resolve_project_input_for_auth(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
    ) -> Result<ResolvedProject, ProjectResolverError> {
        let raw = project.trim();
        if raw.is_empty() {
            return Err(ProjectResolverError {
                kind: ProjectResolverErrorKind::UnknownProject,
                project: project.to_string(),
                candidates: self.agent_project_candidates_for_auth(auth).await,
            });
        }

        let all_candidates = self.agent_project_candidates_for_auth(auth).await;

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

    async fn resolve_project_input(
        &self,
        project: &str,
    ) -> Result<ResolvedProject, ProjectResolverError> {
        self.resolve_project_input_for_auth(project, None).await
    }

    async fn resolve_project(&self, project: &str) -> Result<ProjectConfig, ProjectResolverError> {
        self.resolve_project_input(project)
            .await
            .map(|resolved| resolved.config)
    }

    async fn resolve_project_for_auth(
        &self,
        project: &str,
        auth: Option<&AuthContext>,
    ) -> Result<ProjectConfig, ProjectResolverError> {
        self.resolve_project_input_for_auth(project, auth)
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
            | ToolCall::ApplyTextEdits { .. }
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
            | ToolCall::GitLog { .. }
            | ToolCall::GitDiffSummary { .. }
            | ToolCall::ShowChanges { .. }
            | ToolCall::WorkspaceHygieneCheck { .. } => Some(AgentCapability::GitOrShell),
            ToolCall::WorkspaceCheckpointCreate { .. }
            | ToolCall::WorkspaceCheckpointRestore { .. } => Some(AgentCapability::Shell),
            ToolCall::WorkspaceCheckpointList { .. }
            | ToolCall::WorkspaceCheckpointShow { .. }
            | ToolCall::WorkspaceCheckpointDelete { .. } => Some(AgentCapability::OwnerOnly),
            ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. } => Some(AgentCapability::Shell),
            ToolCall::RunJob { .. } | ToolCall::RunCodex { .. } => Some(AgentCapability::AsyncJobs),
            ToolCall::ListTools
            | ToolCall::StartSession { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }
            | ToolCall::SessionHandoffSummary { .. }
            | ToolCall::BindCurrentSession { .. }
            | ToolCall::CurrentSession { .. }
            | ToolCall::UnbindCurrentSession { .. }
            | ToolCall::ListProjects
            | ToolCall::RegisterProject { .. }
            | ToolCall::CreateProject { .. }
            | ToolCall::ListAgents
            | ToolCall::RuntimeStatus
            | ToolCall::ToolManifest { .. }
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
        let Some(project) = call.project() else {
            return Ok(());
        };
        let required = match Self::required_agent_capability(call) {
            Some(cap) => cap,
            None => return Ok(()),
        };
        let proj = self
            .resolve_project_for_auth(project, auth)
            .await
            .map_err(ProjectResolverError::into_tool_result)?;
        if !proj.is_agent() {
            return Ok(());
        }
        let client_id = proj.agent_client_id().map_err(ToolResult::err)?.to_string();
        if self
            .shell_clients
            .get_client_view_for_auth(&client_id, auth)
            .await
            .is_none()
        {
            return Err(ToolResult::err(format!(
                "unknown shell client: {}",
                client_id
            )));
        }
        self.shell_clients
            .assert_client_access(auth, &client_id)
            .await
            .map_err(ToolResult::err)?;
        if matches!(required, AgentCapability::OwnerOnly) {
            return Ok(());
        }
        // Capability check via the registry helper so the requirement is
        // expressed as a named capability, not a raw struct field access.
        let supported = match required {
            AgentCapability::OwnerOnly => true,
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
                AgentCapability::OwnerOnly => "owner boundary",
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
        self.dispatch_with_auth_transport_options(call, auth, transport, true)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options(
        &self,
        mut call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        use_current_session: bool,
    ) -> ToolResult {
        let mut resolved_project = match call.project() {
            Some(project) => self
                .resolve_project_input_for_auth(project, auth)
                .await
                .ok(),
            None => None,
        };
        if use_current_session && call.session_id().is_none() && is_current_session_eligible(&call)
        {
            if let Some(resolved) = resolved_project.as_ref() {
                match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => {
                        if let Some(session_id) = self.sessions.current_session_id(&key) {
                            call = call.with_effective_session_id(session_id);
                        }
                    }
                    Err(message) => return current_session_unavailable_result(message),
                }
            }
        }
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
            let resolved_project = resolved_project.take().map(|resolved| resolved.resolved_id);
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
        let mut result = self.dispatch_authorized_inner(call, auth, transport).await;
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
        transport: sessions::SessionTransport,
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
                    Some(project_input) => match self
                        .resolve_project_input_for_auth(&project_input, auth)
                        .await
                    {
                        Ok(resolved) => Some(resolved),
                        Err(err) => return err.into_tool_result(),
                    },
                    None => None,
                };
                // Best-effort load of project-local instruction files
                // (AGENTS.md, CLAUDE.md, ...). Any read failure is swallowed
                // and never fails start_session. `null` when no project was
                // provided; `loaded=false` when a project had no candidate.
                let project_instructions = match &resolved {
                    Some(resolved) => Some(self.load_project_instructions(&resolved.config).await),
                    None => None,
                };
                let summary =
                    self.sessions
                        .start_session_with_options(sessions::SessionCreateOptions {
                            project: resolved
                                .as_ref()
                                .map(|resolved| resolved.resolved_id.clone()),
                            title,
                            mode,
                            guards: sessions::SessionGuards::effective(
                                mode,
                                sessions::SessionGuards {
                                    deny_write_tools,
                                    deny_shell_tools,
                                },
                            ),
                            project_instructions: project_instructions.clone(),
                        });
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
                    "project_instructions": project_instructions,
                }))
            }

            ToolCall::SessionSummary { session_id, limit } => {
                match self.sessions.summary(&session_id, limit) {
                    Some(summary) => ToolResult::ok(
                        serde_json::to_value(summary)
                            .unwrap_or_else(|_| json!({"session_id": session_id, "events": []})),
                    ),
                    None => unknown_session_result(&session_id),
                }
            }

            ToolCall::PostSessionMessage {
                session_id,
                kind,
                message,
                tags,
                reply_to,
                priority,
            } => match self
                .sessions
                .post_message(sessions::PostSessionMessageInput {
                    session_id: session_id.clone(),
                    kind,
                    message,
                    tags,
                    reply_to,
                    priority,
                }) {
                Ok(message) => ToolResult::ok(json!({
                    "success": true,
                    "session_id": session_id,
                    "message_id": message.message_id,
                    "message": message,
                })),
                Err(err) => session_message_error_result(&session_id, None, err),
            },

            ToolCall::ListSessionMessages {
                session_id,
                kind,
                status,
                limit,
            } => match self.sessions.list_messages(
                &session_id,
                sessions::ListSessionMessagesFilter {
                    kind,
                    status,
                    limit,
                },
            ) {
                Ok(messages) => ToolResult::ok(json!({
                    "success": true,
                    "session_id": session_id,
                    "messages": messages,
                })),
                Err(err) => session_message_error_result(&session_id, None, err),
            },

            ToolCall::ResolveSessionMessage {
                session_id,
                message_id,
                resolution,
            } => match self
                .sessions
                .resolve_message(&session_id, &message_id, resolution)
            {
                Ok(message) => ToolResult::ok(json!({
                    "success": true,
                    "session_id": session_id,
                    "message_id": message.message_id,
                    "message": message,
                })),
                Err(err) => session_message_error_result(&session_id, Some(&message_id), err),
            },

            ToolCall::SessionDiscussionSummary { session_id, limit } => {
                match self.sessions.discussion_summary(&session_id, limit) {
                    Ok(summary) => ToolResult::ok(json!({
                        "success": true,
                        "session_id": session_id,
                        "counts": summary.counts,
                        "open_guidance": summary.open_guidance,
                        "open_questions": summary.open_questions,
                        "open_risks": summary.open_risks,
                        "open_todos": summary.open_todos,
                        "recent_progress": summary.recent_progress,
                        "recent_decisions": summary.recent_decisions,
                    })),
                    Err(err) => session_message_error_result(&session_id, None, err),
                }
            }

            ToolCall::SessionHandoffSummary {
                session_id,
                project,
                include_workspace,
                include_checkpoints,
                limit,
            } => {
                self.session_handoff_summary(
                    session_id,
                    project,
                    include_workspace,
                    include_checkpoints,
                    limit,
                )
                .await
            }

            ToolCall::BindCurrentSession {
                project,
                session_id,
            } => {
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(err) => return err.into_tool_result(),
                };
                let Some(summary) = self.sessions.summary(&session_id, None) else {
                    return unknown_session_result(&session_id);
                };
                if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
                    return ToolResult::err_with_output(
                        "session_project_mismatch",
                        json!({
                            "error_kind": "session_project_mismatch",
                            "session_id": session_id,
                            "session_project": summary.project,
                            "project": project,
                            "resolved_project": resolved.resolved_id,
                        }),
                    );
                }
                let key = match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => key,
                    Err(message) => return current_session_unavailable_result(message),
                };
                let Some(bound) = self.sessions.bind_current_session(key, &session_id) else {
                    return unknown_session_result(&session_id);
                };
                ToolResult::ok(json!({
                    "bound": true,
                    "session_id": bound.session_id,
                    "project": project,
                    "resolved_project": resolved.resolved_id,
                    "mode": bound.mode,
                    "guards": bound.guards,
                }))
            }

            ToolCall::CurrentSession { project } => {
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(err) => return err.into_tool_result(),
                };
                let key = match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => key,
                    Err(message) => return current_session_unavailable_result(message),
                };
                match self.sessions.current_session(&key) {
                    Some(summary) => ToolResult::ok(json!({
                        "found": true,
                        "session_id": summary.session_id,
                        "project": project,
                        "resolved_project": resolved.resolved_id,
                        "mode": summary.mode,
                        "guards": summary.guards,
                    })),
                    None => ToolResult::ok(json!({
                        "found": false,
                        "project": project,
                        "resolved_project": resolved.resolved_id,
                    })),
                }
            }

            ToolCall::UnbindCurrentSession { project } => {
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(err) => return err.into_tool_result(),
                };
                let key = match current_session_key(auth, transport, &resolved.resolved_id) {
                    Ok(key) => key,
                    Err(message) => return current_session_unavailable_result(message),
                };
                let had_binding = self.sessions.unbind_current_session(&key);
                ToolResult::ok(json!({
                    "unbound": true,
                    "had_binding": had_binding,
                    "project": project,
                    "resolved_project": resolved.resolved_id,
                }))
            }

            ToolCall::WorkspaceCheckpointCreate {
                project,
                title,
                note,
                include_untracked,
                kind,
                labels,
                validation,
                session_id: _,
            } => {
                self.workspace_checkpoint_create(
                    project,
                    title,
                    note,
                    include_untracked,
                    kind,
                    labels,
                    validation,
                )
                .await
            }

            ToolCall::WorkspaceCheckpointList {
                project,
                limit,
                session_id: _,
            } => self.workspace_checkpoint_list(project, limit).await,

            ToolCall::WorkspaceCheckpointShow {
                project,
                checkpoint_id,
                include_diff_stat,
                session_id: _,
            } => {
                self.workspace_checkpoint_show(project, checkpoint_id, include_diff_stat)
                    .await
            }

            ToolCall::WorkspaceCheckpointRestore {
                project,
                checkpoint_id,
                confirm,
                session_id: _,
            } => {
                self.workspace_checkpoint_restore(project, checkpoint_id, confirm)
                    .await
            }

            ToolCall::WorkspaceCheckpointDelete {
                project,
                checkpoint_id,
                confirm,
                session_id: _,
            } => {
                self.workspace_checkpoint_delete(project, checkpoint_id, confirm)
                    .await
            }

            ToolCall::ListProjects => self.list_projects(auth).await,

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

            ToolCall::ListAgents => self.list_agents(auth).await,

            ToolCall::RuntimeStatus => self.runtime_status(auth).await,

            ToolCall::ToolManifest {
                category,
                include_recommended_flows,
                include_risk_summary,
            } => {
                self.tool_manifest(category, include_recommended_flows, include_risk_summary)
                    .await
            }

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

            ToolCall::GitLog {
                project,
                limit,
                skip,
                session_id: _,
            } => self.git_log(project, limit, skip).await,

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

            ToolCall::JobStatus { job_id } => self.job_status_for_auth(job_id, auth).await,

            ToolCall::JobLog {
                job_id,
                offset,
                tail_lines,
            } => {
                self.job_log_for_auth(job_id, offset, tail_lines, auth)
                    .await
            }

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

            ToolCall::WorkspaceHygieneCheck {
                project,
                max_findings,
                include_tracked,
                session_id,
            } => {
                self.workspace_hygiene_check(project, max_findings, include_tracked, session_id)
                    .await
            }

            ToolCall::ListJobs { limit, status } => {
                self.list_jobs_for_auth(limit, status, auth).await
            }

            ToolCall::JobTail { job_id, tail_lines } => {
                self.job_tail_for_auth(job_id, tail_lines, auth).await
            }

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

            ToolCall::ApplyTextEdits {
                project,
                path,
                edits,
                dry_run,
                expected_file_sha256,
                session_id: _,
            } => {
                self.apply_text_edits(project, path, edits, dry_run, expected_file_sha256)
                    .await
            }
        }
    }

    async fn list_projects(&self, auth: Option<&AuthContext>) -> ToolResult {
        let mut list: Vec<Value> = Vec::new();
        for client in self.shell_clients.list_clients_for_auth(auth).await {
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

    async fn list_agents(&self, auth: Option<&AuthContext>) -> ToolResult {
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
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
    async fn runtime_status(&self, auth: Option<&AuthContext>) -> ToolResult {
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
        let clients = self.shell_clients.list_clients_for_auth(auth).await;
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
        let agent_jobs = self.shell_clients.list_jobs_for_auth(auth, None).await;
        let agent_known_count = agent_jobs.len();
        let local_job_dirs: Vec<PathBuf> = if Self::local_jobs_visible_to_auth(auth) {
            let local_jobs_map = self.local_jobs.lock().await;
            local_jobs_map
                .values()
                .map(|record| record.dir.clone())
                .collect()
        } else {
            Vec::new()
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
            "session_store": self.sessions.status(),
        });
        if let Some(quic) = quic {
            output["quic"] = quic;
        }
        ToolResult::ok(output)
    }

    /// Return a compact, bounded tool manifest with categories, risk summary,
    /// and recommended flows. Read-only runtime introspection; never exposes
    /// full input/output schemas, tokens, secrets, or internal paths.
    /// Intended as a lightweight alternative to `list_tools` for long-running
    /// tasks where the full schemas cause ResponseTooLargeError.
    async fn tool_manifest(
        &self,
        category: Option<String>,
        include_recommended_flows: bool,
        include_risk_summary: bool,
    ) -> ToolResult {
        let specs = self.tool_specs();
        let tool_count = specs.len();

        // Build compact tool entries from metadata — no input/output schemas,
        // no long descriptions.
        let all_tools: Vec<Value> = specs
            .iter()
            .map(|spec| {
                let name = spec.name.as_str();
                let m = metadata::tool_metadata(name);
                json!({
                    "name": name,
                    "category": tool_manifest_category(name),
                    "provider": m.provider_id,
                    "risk": m.risk.session_risk_class(),
                    "read_only": m.read_only,
                    "requires_project": m.requires_project,
                    "path_hint": path_hint_str(m.path_hint),
                    "destructive": m.destructive,
                    "shell_like": m.shell_like,
                    "oauth_scope": m.oauth_scope,
                })
            })
            .collect();

        // Build the categories map from the full tool set so the caller can
        // always see valid categories even when filtering.
        let categories = build_manifest_categories(&all_tools);

        // Apply the optional category filter.
        let filtered_tools: Vec<Value> = match &category {
            Some(cat) => all_tools
                .iter()
                .filter(|t| t["category"].as_str() == Some(cat.as_str()))
                .cloned()
                .collect(),
            None => all_tools,
        };
        let filtered_count = filtered_tools.len();

        let mut output = json!({
            "schema_version": 1,
            "tool_count": tool_count,
            "filtered_count": filtered_count,
            "category": category,
            "categories": categories,
            "tools": filtered_tools,
        });

        if include_risk_summary {
            output["risk_summary"] =
                build_risk_summary(output["tools"].as_array().unwrap_or(&Vec::new()));
        }

        if include_recommended_flows {
            output["recommended_flows"] = Value::Array(tool_manifest_recommended_flows());
        }

        ToolResult::ok(output)
    }
}

/// Map a tool name to its primary manifest category. This is the single
/// centralized classification function for `tool_manifest`; it must cover
/// every name in `KNOWN_TOOL_NAMES`.
fn tool_manifest_category(name: &str) -> &'static str {
    match name {
        // Runtime introspection / discovery
        "list_tools" | "tool_manifest" | "runtime_status" | "list_agents" => "runtime",
        // Session lifecycle and messaging
        "start_session"
        | "session_summary"
        | "post_session_message"
        | "list_session_messages"
        | "resolve_session_message"
        | "session_discussion_summary"
        | "session_handoff_summary"
        | "bind_current_session"
        | "current_session"
        | "unbind_current_session" => "session",
        // Workspace checkpoints
        "workspace_checkpoint_create"
        | "workspace_checkpoint_list"
        | "workspace_checkpoint_show"
        | "workspace_checkpoint_restore"
        | "workspace_checkpoint_delete" => "checkpoint",
        // Git read / review
        "git_status" | "git_diff" | "git_diff_hunks" | "git_log" | "git_diff_summary"
        | "show_changes" => "git",
        // Structured file edits
        "replace_in_file"
        | "replace_exact_block"
        | "insert_before_pattern"
        | "insert_after_pattern"
        | "write_project_file"
        | "replace_line_range"
        | "insert_at_line"
        | "delete_line_range"
        | "apply_text_edits" => "edit",
        // File read / list / search
        "read_file" | "list_project_files" | "search_project_text" => "file",
        // Patch apply / validate
        "apply_patch" | "apply_patch_checked" | "validate_patch" => "patch",
        // Validation
        "cargo_fmt" | "cargo_check" | "cargo_test" => "validation",
        // Shell / job execution
        "run_shell" | "run_job" | "job_status" | "job_log" | "list_jobs" | "job_tail" => "job",
        // Project management
        "list_projects" | "register_project" | "create_project" => "project",
        // Artifacts
        "save_project_artifact" | "read_project_artifact_metadata" | "read_project_artifact" => {
            "artifact"
        }
        // Cleanup / destructive
        "delete_project_files"
        | "git_restore_paths"
        | "discard_untracked"
        | "workspace_hygiene_check" => "cleanup",
        // Codex delegation
        "run_codex" => "codex",
        _ => "other",
    }
}

/// String representation of a `ToolPathHint` for the compact manifest.
fn path_hint_str(hint: metadata::ToolPathHint) -> &'static str {
    match hint {
        metadata::ToolPathHint::None => "none",
        metadata::ToolPathHint::SinglePath => "single_path",
        metadata::ToolPathHint::PathList => "path_list",
        metadata::ToolPathHint::Patch => "patch",
        metadata::ToolPathHint::Artifact => "artifact",
    }
}

/// Build the categories map from the compact tool entries. Each category
/// maps to a sorted list of tool names.
fn build_manifest_categories(tools: &[Value]) -> Value {
    let mut map: std::collections::BTreeMap<&str, Vec<String>> = std::collections::BTreeMap::new();
    for tool in tools {
        let name = tool["name"].as_str().unwrap_or("");
        let category = tool["category"].as_str().unwrap_or("other");
        map.entry(category).or_default().push(name.to_string());
    }
    let result: serde_json::Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                Value::Array(v.into_iter().map(Value::String).collect()),
            )
        })
        .collect();
    Value::Object(result)
}

/// Build the risk summary map from the compact tool entries.
fn build_risk_summary(tools: &[Value]) -> Value {
    let mut counts: std::collections::BTreeMap<&str, u64> = std::collections::BTreeMap::new();
    for tool in tools {
        let risk = tool["risk"].as_str().unwrap_or("unknown");
        *counts.entry(risk).or_insert(0) += 1;
    }
    let result: serde_json::Map<String, Value> = counts
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::from(v)))
        .collect();
    Value::Object(result)
}

/// Short, bounded list of recommended tool flows for common tasks. Each
/// entry references only known tool names. Kept under 10 entries.
fn tool_manifest_recommended_flows() -> Vec<Value> {
    vec![
        json!({
            "name": "large_refactor",
            "purpose": "Safely perform large single-file refactors.",
            "tools": ["workspace_checkpoint_create", "read_file", "apply_text_edits", "cargo_test", "show_changes"]
        }),
        json!({
            "name": "deployment_smoke",
            "purpose": "Check runtime health and persistent session behavior.",
            "tools": ["runtime_status", "start_session", "post_session_message", "git_log", "session_summary"]
        }),
        json!({
            "name": "small_line_edit",
            "purpose": "Make small targeted edits by stable line numbers.",
            "tools": ["read_file", "replace_line_range", "insert_at_line", "delete_line_range", "show_changes"]
        }),
        json!({
            "name": "patch_review",
            "purpose": "Validate a patch before applying it safely.",
            "tools": ["validate_patch", "apply_patch_checked", "show_changes", "cargo_test"]
        }),
        json!({
            "name": "discovery",
            "purpose": "Discover projects, agents, and available tools.",
            "tools": ["tool_manifest", "list_projects", "runtime_status"]
        }),
        json!({
            "name": "handoff",
            "purpose": "Quickly understand task state before taking over a session.",
            "tools": ["session_handoff_summary", "show_changes", "workspace_checkpoint_create"]
        }),
    ]
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
mod tests;
