//! Project-bound application layer for hosted chat connectors.
//!
//! The connector exposes a deliberately small coding workflow while reusing
//! ToolRuntime for execution policy, agent ownership, and permission gates.
//! It owns project/task context, so model-visible calls never carry the legacy
//! executor project id or workflow-session state.

mod execution;
#[cfg(test)]
mod execution_tests;
pub(crate) mod http;
pub(crate) mod surface;
pub(crate) mod workspace;

use crate::auth::{
    AuthContext, AuthKind, ProjectCredentialVerifier, SCOPE_JOB_RUN, SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ,
};
use crate::db::{
    ConnectorApproval, ConnectorApprovalGate, ConnectorBinding, ConnectorEditOperationGate,
    ConnectorExecutionReservation, ConnectorTaskResult, ConnectorTaskSnapshot,
    ConnectorTaskStoreError, NewConnectorResult, NewConnectorTask,
};
use crate::shell_protocol::SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV;
use crate::tool_runtime::kernel::{
    ToolCallContext, ToolCallErrorStatus, ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::validation_profile::{
    resolve_validation_recipe, RecipeError, RecipeId, SemanticCheck,
};
use crate::tool_runtime::{ApplyFileChangeInput, SearchResultMode, ToolResult, ToolRuntime};
use crate::Database;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex as StdMutex, Weak};

const CONNECTOR_SURFACE_ENV: &str = "WEBCODEX_CONNECTOR_SURFACE";
const CONNECTOR_SURFACE_TASK_V1: &str = "task-v1";
const MAX_EVENT_COUNT: usize = 50;
const COMMAND_APPROVAL_TTL_SECS: i64 = 60 * 60;
const CONNECTOR_PATCH_PREVIEW_BYTES: usize = 128 * 1024;
const CONNECTOR_SEARCH_WINDOW: usize = 200;
#[cfg(test)]
type FinishTestHook = (Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConnectorContext {
    pub project_id: String,
    pub project_name: String,
    pub workspace_id: String,
    pub executor_project: String,
    pub executor_root: String,
    pub runs_root: String,
    pub results_root: String,
    pub projects_dir: String,
    pub profile: String,
    pub project_grant_id: String,
}

impl ConnectorContext {
    pub(crate) fn from_env() -> Result<Option<Self>, String> {
        let Some(surface) = nonempty_env(CONNECTOR_SURFACE_ENV) else {
            return Ok(None);
        };
        if surface != CONNECTOR_SURFACE_TASK_V1 {
            return Err(format!(
                "unsupported {CONNECTOR_SURFACE_ENV} '{surface}'; expected {CONNECTOR_SURFACE_TASK_V1}"
            ));
        }
        let context = Self {
            project_id: required_env("WEBCODEX_CONNECTOR_PROJECT_ID")?,
            project_name: required_env("WEBCODEX_CONNECTOR_PROJECT_NAME")?,
            workspace_id: required_env("WEBCODEX_CONNECTOR_WORKSPACE_ID")?,
            executor_project: required_env("WEBCODEX_CONNECTOR_EXECUTOR_PROJECT")?,
            executor_root: required_env("WEBCODEX_CONNECTOR_EXECUTOR_ROOT")?,
            runs_root: required_env("WEBCODEX_CONNECTOR_RUNS_ROOT")?,
            results_root: required_env("WEBCODEX_CONNECTOR_RESULTS_ROOT")?,
            projects_dir: required_env("WEBCODEX_CONNECTOR_PROJECTS_DIR")?,
            profile: required_env("WEBCODEX_CONNECTOR_PROFILE")?,
            project_grant_id: required_env("WEBCODEX_CONNECTOR_PROJECT_GRANT_ID")?,
        };
        context.validate()?;
        Ok(Some(context))
    }

    fn validate(&self) -> Result<(), String> {
        validate_opaque_id(&self.project_id, "wc_proj_", "connector project id")?;
        validate_opaque_id(&self.workspace_id, "wc_ws_", "connector workspace id")?;
        if !self.executor_project.starts_with("agent:") {
            return Err(
                "WEBCODEX_CONNECTOR_EXECUTOR_PROJECT must be an agent-backed runtime id".into(),
            );
        }
        if !Path::new(&self.executor_root).is_absolute() || self.executor_root == "/" {
            return Err(
                "WEBCODEX_CONNECTOR_EXECUTOR_ROOT must be an absolute non-root project path".into(),
            );
        }
        if self.project_name.trim().is_empty() || self.project_name.len() > 200 {
            return Err("WEBCODEX_CONNECTOR_PROJECT_NAME must be 1..=200 bytes".into());
        }
        if self.profile.trim().is_empty() || self.profile.len() > 100 {
            return Err("WEBCODEX_CONNECTOR_PROFILE must be 1..=100 bytes".into());
        }
        validate_opaque_id(
            &self.project_grant_id,
            "wc_pgrant_",
            "connector project grant id",
        )?;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct ConnectorRuntimeSlot(pub(crate) Option<Arc<ConnectorRuntime>>);

pub(crate) struct ConnectorRuntime {
    tools: Arc<ToolRuntime>,
    db: Arc<Database>,
    context: ConnectorContext,
    workspace: workspace::WorkspaceManager,
    executions: execution::ExecutionService,
    credential: ProjectCredentialVerifier,
    workspace_ops: tokio::sync::Mutex<()>,
    task_locks: StdMutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
    #[cfg(test)]
    finish_after_fingerprint: StdMutex<Option<FinishTestHook>>,
    #[cfg(test)]
    mutation_before_task_lock: StdMutex<Option<Arc<tokio::sync::Semaphore>>>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectorTransport {
    Api,
    Mcp,
}

impl From<ConnectorTransport> for ToolTransport {
    fn from(value: ConnectorTransport) -> Self {
        match value {
            ConnectorTransport::Api => ToolTransport::Api,
            ConnectorTransport::Mcp => ToolTransport::Mcp,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ConnectorCallOutcome {
    pub ok: bool,
    pub body: Value,
    pub http_status: u16,
    pub required_scope: Option<&'static str>,
    /// Invalid capability names and malformed inputs are JSON-RPC parameter
    /// errors. Task state and executor failures are normal tool errors.
    pub protocol_error: bool,
}

impl ConnectorRuntime {
    pub(crate) fn new(
        tools: Arc<ToolRuntime>,
        db: Arc<Database>,
        context: ConnectorContext,
        credential: ProjectCredentialVerifier,
    ) -> Result<Self, String> {
        context.validate()?;
        if credential.grant_id() != context.project_grant_id {
            return Err("project credential does not match connector grant identity".to_string());
        }
        let workspace = workspace::WorkspaceManager::new(&context)?;
        let executions =
            execution::ExecutionService::new(tools.clone(), db.clone(), workspace.clone());
        let (runs_recovered, executions_recovered) = executions
            .reconcile_startup(&context.project_id, chrono::Utc::now().timestamp())
            .map_err(|error| format!("failed to recover connector runs: {error}"))?;
        if runs_recovered > 0 || executions_recovered > 0 {
            tracing::warn!(
                project_id = %context.project_id,
                runs = runs_recovered,
                executions = executions_recovered,
                "Recovered unfinished connector executions as interrupted"
            );
        }
        let preserved = db
            .connector_preserved_workspaces(&context.project_id)
            .map_err(|error| format!("failed to inspect connector workspaces: {error}"))?;
        for warning in workspace.recover(&context, &preserved) {
            tracing::warn!(project_id = %context.project_id, warning = %warning, "Connector workspace recovery was incomplete");
        }
        Ok(Self {
            tools,
            db,
            context,
            workspace,
            executions,
            credential,
            workspace_ops: tokio::sync::Mutex::new(()),
            task_locks: StdMutex::new(HashMap::new()),
            #[cfg(test)]
            finish_after_fingerprint: StdMutex::new(None),
            #[cfg(test)]
            mutation_before_task_lock: StdMutex::new(None),
        })
    }

    pub(crate) fn from_env(
        tools: Arc<ToolRuntime>,
        db: Arc<Database>,
    ) -> Result<ConnectorRuntimeSlot, String> {
        let Some(context) = ConnectorContext::from_env()? else {
            return Ok(ConnectorRuntimeSlot::default());
        };
        let credential_path = required_env("WEBCODEX_PROJECT_CREDENTIAL_FILE")?;
        let credential = ProjectCredentialVerifier::from_file(
            context.project_grant_id.clone(),
            Path::new(&credential_path),
        )?;
        Ok(ConnectorRuntimeSlot(Some(Arc::new(Self::new(
            tools, db, context, credential,
        )?))))
    }

    pub(crate) fn context(&self) -> &ConnectorContext {
        &self.context
    }

    pub(crate) fn authenticate_project_credential(&self, token: &str) -> Option<AuthContext> {
        self.credential.authenticate(token)
    }

    fn project_access_allowed(&self, auth: &AuthContext) -> bool {
        auth.is_bootstrap()
            || auth.project_grant_id.as_deref() == Some(self.context.project_grant_id.as_str())
    }

    pub(crate) async fn readiness(
        &self,
        auth: &AuthContext,
    ) -> Option<crate::project_entry::ProjectReadiness> {
        use crate::project_entry::{runtime_readiness, RemoteProbe};

        if !self.project_access_allowed(auth) {
            return None;
        }
        let Some((client_id, project_id)) = self
            .context
            .executor_project
            .strip_prefix("agent:")
            .and_then(|value| value.split_once(':'))
        else {
            return Some(runtime_readiness(
                Some(self.context.project_name.clone()),
                RemoteProbe::ProjectMissing,
            ));
        };
        let Some(agent) = self
            .tools
            .shell_clients
            .get_client_view_for_auth(client_id, Some(auth))
            .await
        else {
            return Some(runtime_readiness(
                Some(self.context.project_name.clone()),
                RemoteProbe::AgentOffline,
            ));
        };
        if agent.status != "online" || !agent.connected {
            return Some(runtime_readiness(
                Some(self.context.project_name.clone()),
                RemoteProbe::AgentOffline,
            ));
        }
        if !agent
            .projects
            .iter()
            .any(|project| project.id == project_id && !project.disabled)
        {
            return Some(runtime_readiness(
                Some(self.context.project_name.clone()),
                RemoteProbe::ProjectMissing,
            ));
        }
        let capabilities = &agent.capabilities;
        if !(capabilities.shell
            && capabilities.file_read
            && capabilities.file_write
            && capabilities.jobs
            && capabilities.async_jobs
            && capabilities.async_shell_jobs)
        {
            return Some(runtime_readiness(
                Some(self.context.project_name.clone()),
                RemoteProbe::RequiredCapabilityMissing,
            ));
        }
        if !capabilities.structured_validation_argv {
            return Some(runtime_readiness(
                Some(self.context.project_name.clone()),
                RemoteProbe::StructuredValidationMissing,
            ));
        }
        Some(runtime_readiness(
            Some(self.context.project_name.clone()),
            RemoteProbe::Ready,
        ))
    }

    pub(crate) async fn call(
        &self,
        capability: &str,
        arguments: Value,
        auth: Option<&AuthContext>,
        transport: ConnectorTransport,
    ) -> ConnectorCallOutcome {
        if surface::capability_spec(capability).is_none() {
            return ConnectorCallOutcome::error(
                400,
                "unknown_capability",
                format!(
                    "'{capability}' is not available in the project connector; use one of: {}",
                    surface::CAPABILITY_NAMES.join(", ")
                ),
                false,
                false,
                Some("Call task_start first, then use the returned task_id."),
                None,
                true,
            );
        }

        let Some(auth) = auth else {
            return ConnectorCallOutcome::error(
                401,
                "authentication_required",
                "connector capabilities require an authenticated identity",
                false,
                true,
                Some("Configure Bearer authentication in the connector client."),
                None,
                false,
            );
        };
        if !self.project_access_allowed(auth) {
            return ConnectorCallOutcome::error(
                403,
                "project_credential_rejected",
                "the authenticated credential is not authorized for this project",
                false,
                true,
                Some("Use the credential generated by setup for this project."),
                None,
                false,
            );
        }
        let required_scope = required_scope(capability);
        if !auth.has_scope(required_scope) {
            return ConnectorCallOutcome::scope_denied(required_scope);
        }
        let subject_id = match stable_subject_id(auth) {
            Ok(subject) => subject,
            Err(message) => {
                return ConnectorCallOutcome::error(
                    403,
                    "identity_not_supported",
                    message,
                    false,
                    true,
                    Some("Use a user, OAuth, shared-key, or bootstrap connector credential."),
                    None,
                    false,
                )
            }
        };

        let now = chrono::Utc::now().timestamp();
        if let Err(error) = self.db.ensure_connector_binding(ConnectorBinding {
            project_id: &self.context.project_id,
            project_name: &self.context.project_name,
            workspace_id: &self.context.workspace_id,
            executor_ref: &self.context.executor_project,
            subject_id: &subject_id,
            profile: &self.context.profile,
            now,
        }) {
            return store_error_outcome(error, None);
        }

        // Read operations coordinate with lifecycle transitions, while every
        // mutation/reservation method owns its narrower task-lock boundary.
        let task_lock = if matches!(capability, "files_read" | "files_search") {
            arguments
                .get("task_id")
                .and_then(Value::as_str)
                .map(|task_id| self.task_lock(task_id))
        } else {
            None
        };
        let _task_guard = match task_lock.as_ref() {
            Some(lock) => Some(lock.lock().await),
            None => None,
        };
        match capability {
            "task_start" => {
                self.task_start(arguments, &subject_id, auth, transport, now)
                    .await
            }
            "files_read" => {
                self.files_read(arguments, &subject_id, auth, transport, now)
                    .await
            }
            "files_search" => {
                self.files_search(arguments, &subject_id, auth, transport, now)
                    .await
            }
            "edits_apply" => {
                self.edits_apply(arguments, &subject_id, auth, transport, now)
                    .await
            }
            "checks_run" => {
                self.checks_run(arguments, &subject_id, auth, transport, now)
                    .await
            }
            "commands_run" => {
                self.commands_run(arguments, &subject_id, auth, transport, now)
                    .await
            }
            "task_review" => {
                self.task_review(arguments, &subject_id, auth, transport)
                    .await
            }
            "task_cancel" => self.task_cancel(arguments, &subject_id, auth).await,
            "task_finish" => {
                self.task_finish(arguments, &subject_id, auth, transport, now)
                    .await
            }
            _ => unreachable!("capability registry checked before dispatch"),
        }
    }

    fn task_lock(&self, task_id: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.task_locks.lock().unwrap();
        locks.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = locks.get(task_id).and_then(Weak::upgrade) {
            return lock;
        }
        let lock = Arc::new(tokio::sync::Mutex::new(()));
        locks.insert(task_id.to_string(), Arc::downgrade(&lock));
        lock
    }

    async fn workspace_fingerprint(
        &self,
        task: &ConnectorTaskSnapshot,
        capability: &'static str,
    ) -> Result<String, ConnectorCallOutcome> {
        let manager = self.workspace.clone();
        let task_for_fingerprint = task.clone();
        match tokio::task::spawn_blocking(move || {
            manager.action_precondition(&task_for_fingerprint)
        })
        .await
        {
            Ok(Ok(fingerprint)) => Ok(fingerprint),
            Ok(Err(message)) => Err(ConnectorCallOutcome::error_for_task(
                409,
                "workspace_fingerprint_failed",
                self.sanitize_task_string(task, &message),
                false,
                true,
                Some("Resolve the Git workspace issue, then retry the operation."),
                task,
                Value::Null,
            )),
            Err(error) => {
                tracing::error!(error = %error, capability, "connector workspace fingerprint task failed");
                Err(ConnectorCallOutcome::error_for_task(
                    500,
                    "workspace_fingerprint_failed",
                    "connector could not fingerprint the current workspace",
                    false,
                    true,
                    Some("Inspect server logs before retrying the operation."),
                    task,
                    Value::Null,
                ))
            }
        }
    }

    async fn task_start(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        _transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: TaskStartInput = match parse_input("task_start", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        let goal = input.goal.trim();
        if goal.is_empty() || goal.len() > 4000 {
            return invalid_input("task_start", "goal must be 1..=4000 bytes");
        }
        let mode = input.mode.as_str();
        if mode == "normal" && !auth.has_scope(SCOPE_PROJECT_WRITE) {
            return ConnectorCallOutcome::scope_denied(SCOPE_PROJECT_WRITE);
        }
        let task_id = format!("wc_task_{}", uuid::Uuid::new_v4().simple());
        let run_id = format!("wc_run_{}", uuid::Uuid::new_v4().simple());
        let read_only = mode == "read_only";
        let _workspace_guard = if read_only {
            None
        } else {
            Some(self.workspace_ops.lock().await)
        };
        let manager = self.workspace.clone();
        let context = self.context.clone();
        let task_for_prepare = task_id.clone();
        let run_for_prepare = run_id.clone();
        let prepared = match tokio::task::spawn_blocking(move || {
            manager.prepare(&context, &task_for_prepare, &run_for_prepare, read_only)
        })
        .await
        {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(message)) => {
                let guidance = if message.contains("workspace slot is occupied") {
                    "Run 'webcodex task list', then finish, resume, or reject the task occupying the writable slot."
                } else {
                    "Resolve the Git/workspace issue, then start a new task."
                };
                return ConnectorCallOutcome::error(
                    409,
                    "workspace_preparation_failed",
                    self.sanitize_executor_string(&message),
                    false,
                    true,
                    Some(guidance),
                    None,
                    false,
                );
            }
            Err(error) => {
                tracing::error!(error = %error, "connector workspace preparation task failed");
                return ConnectorCallOutcome::error(
                    500,
                    "workspace_preparation_failed",
                    "connector could not prepare the isolated execution workspace",
                    false,
                    true,
                    Some("Inspect server logs, then start a new task."),
                    None,
                    false,
                );
            }
        };
        if prepared.isolated {
            let registration = self
                .tools
                .register_project(
                    prepared.agent_client_id.clone(),
                    prepared.agent_project_id.clone(),
                    format!("WebCodex {}", prepared.agent_project_id),
                    prepared.execution_root.clone(),
                    Some("WebCodex managed isolated task worktree".to_string()),
                    true,
                    false,
                    Some(auth),
                )
                .await;
            if !registration.success {
                let cleanup = self
                    .workspace
                    .discard_prepared(&self.context.executor_root, &prepared);
                let message = registration
                    .error
                    .as_deref()
                    .map(|message| self.sanitize_executor_string(message))
                    .unwrap_or_else(|| {
                        "executor could not register the isolated workspace".to_string()
                    });
                if let Some(cleanup) = cleanup {
                    tracing::warn!(cleanup = %cleanup, "failed to fully clean rejected workspace preparation");
                }
                return ConnectorCallOutcome::error(
                    400,
                    "workspace_registration_failed",
                    message,
                    false,
                    true,
                    Some("Inspect the executor policy and retry task_start."),
                    None,
                    false,
                );
            }
        }
        let task = match self.db.start_connector_task(NewConnectorTask {
            task_id: &task_id,
            run_id: &run_id,
            project_id: &self.context.project_id,
            workspace_id: &self.context.workspace_id,
            subject_id,
            goal,
            mode,
            target_executor_ref: &self.context.executor_project,
            execution_executor_ref: &prepared.execution_executor_ref,
            target_root: &self.context.executor_root,
            execution_root: &prepared.execution_root,
            baseline_commit: prepared.baseline_commit.as_deref(),
            baseline_tree: prepared.baseline_tree.as_deref(),
            isolated: prepared.isolated,
            now,
        }) {
            Ok(task) => task,
            Err(error) => {
                if let Some(cleanup) = self
                    .workspace
                    .discard_prepared(&self.context.executor_root, &prepared)
                {
                    tracing::warn!(cleanup = %cleanup, "failed to fully clean unpersisted workspace");
                }
                return store_error_outcome(error, None);
            }
        };
        let brief = project_brief(
            &task,
            prepared.project_overview.as_ref(),
            prepared.git_dirty,
            prepared.git_conflict_count,
        );
        ConnectorCallOutcome::success(
            &task,
            json!({
                "project": {
                    "id": self.context.project_id,
                    "name": self.context.project_name
                },
                "goal": goal,
                "mode": mode,
                "status": task.task_status,
                "brief": brief,
                "next": "Use the brief to choose the first targeted read; edit with returned sha256 guards, validate, review, and finish."
            }),
        )
    }

    async fn files_read(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: FilesReadInput = match parse_input("files_read", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if input.files.is_empty() || input.files.len() > 8 {
            return invalid_input("files_read", "files must contain 1..=8 entries");
        }
        let task = match self.active_task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };

        let mut results = Vec::with_capacity(input.files.len());
        for file in &input.files {
            if let Err(message) = validate_path(&file.path) {
                return invalid_input("files_read", message);
            }
            if file.limit.is_some_and(|limit| !(1..=500).contains(&limit)) {
                return invalid_input("files_read", "file limit must be 1..=500");
            }
            let args = json!({
                "project": task.execution_executor_ref,
                "path": file.path,
                "start_line": file.start_line,
                "limit": file.limit.unwrap_or(200),
                "with_line_numbers": file.with_line_numbers.unwrap_or(true)
            });
            match self
                .invoke_kernel("read_file", args, &task, auth, transport)
                .await
            {
                Ok(mut output) => {
                    output["path"] = json!(file.path);
                    results.push(output);
                }
                Err(error) => {
                    let cursor = self.record_event(
                        &task,
                        "files_read",
                        json!({ "ok": false, "requested": input.files.len(), "completed": results.len() }),
                        now,
                    );
                    return self.kernel_error_outcome(
                        error,
                        &task,
                        cursor,
                        json!({ "files": results }),
                    );
                }
            }
        }
        let cursor = match self.record_event(
            &task,
            "files_read",
            json!({ "ok": true, "file_count": results.len() }),
            now,
        ) {
            Ok(cursor) => cursor,
            Err(outcome) => return outcome,
        };
        ConnectorCallOutcome::success_at(&task, cursor, json!({ "files": results }))
    }

    async fn files_search(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: FilesSearchInput = match parse_input("files_search", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if input.pattern.trim().is_empty() || input.pattern.len() > 500 {
            return invalid_input("files_search", "pattern must be 1..=500 bytes");
        }
        if let Some(path) = input.path.as_deref() {
            if let Err(message) = validate_path(path) {
                return invalid_input("files_search", message);
            }
        }
        if input.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
            return invalid_input("files_search", "limit must be 1..=100");
        }
        if input.context_before.unwrap_or(0) > 5 || input.context_after.unwrap_or(0) > 5 {
            return invalid_input("files_search", "search context must be 0..=5 lines");
        }
        if input.include_globs.len() > 20 || input.exclude_globs.len() > 20 {
            return invalid_input(
                "files_search",
                "include/exclude globs are limited to 20 each",
            );
        }
        let task = match self.active_task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let page_limit = input.limit.unwrap_or(50);
        let signature = search_cursor_signature(&input, page_limit);
        let offset = match input.cursor.as_deref() {
            Some(cursor) => match parse_search_cursor(cursor, &signature) {
                Ok(offset) if offset < CONNECTOR_SEARCH_WINDOW => offset,
                _ => {
                    return invalid_input(
                        "files_search",
                        "cursor is invalid, belongs to a different query, or exceeds the bounded search window",
                    )
                }
            },
            None => 0,
        };
        let fetch_limit = offset
            .saturating_add(page_limit)
            .min(CONNECTOR_SEARCH_WINDOW);
        let args = json!({
            "project": task.execution_executor_ref,
            "pattern": input.pattern,
            "path": input.path,
            "limit": fetch_limit,
            "context_before": input.context_before.unwrap_or(0),
            "context_after": input.context_after.unwrap_or(0),
            "include_globs": input.include_globs,
            "exclude_globs": input.exclude_globs,
            "result_mode": input.result_mode.unwrap_or(SearchResultMode::Matches),
            "timeout_secs": 20
        });
        match self
            .invoke_kernel("search_project_text", args, &task, auth, transport)
            .await
        {
            Ok(output) => {
                let output = paginate_search_output(
                    output,
                    input.result_mode.unwrap_or(SearchResultMode::Matches),
                    offset,
                    page_limit,
                    &signature,
                );
                let cursor = match self.record_event(
                    &task,
                    "files_search",
                    json!({ "ok": true, "offset": offset, "limit": page_limit }),
                    now,
                ) {
                    Ok(cursor) => cursor,
                    Err(outcome) => return outcome,
                };
                ConnectorCallOutcome::success_at(&task, cursor, output)
            }
            Err(error) => {
                let cursor = self.record_event(&task, "files_search", json!({ "ok": false }), now);
                self.kernel_error_outcome(error, &task, cursor, Value::Null)
            }
        }
    }

    async fn edits_apply(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: EditsApplyInput = match parse_input("edits_apply", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if let Err(message) = validate_operation_id(&input.operation_id) {
            return invalid_input("edits_apply", message);
        }
        if input.changes.is_empty() || input.changes.len() > 16 {
            return invalid_input("edits_apply", "changes must contain 1..=16 entries");
        }
        for change in &input.changes {
            if let Err(message) = validate_path(&change.path) {
                return invalid_input("edits_apply", message);
            }
            if let Some(to_path) = change.to_path.as_deref() {
                if let Err(message) = validate_path(to_path) {
                    return invalid_input("edits_apply", message);
                }
            }
        }
        let change_bytes = serde_json::to_vec(&input.changes)
            .map(|bytes| bytes.len())
            .unwrap_or(usize::MAX);
        if change_bytes > 1024 * 1024 {
            return invalid_input("edits_apply", "serialized changes exceed 1 MiB");
        }
        #[cfg(test)]
        if let Some(entered) = self.mutation_before_task_lock.lock().unwrap().clone() {
            entered.add_permits(1);
        }
        let task_lock = self.task_lock(&input.task_id);
        let _task_guard = task_lock.lock().await;
        let task = match self.active_writable_task(&input.task_id, subject_id, "edits_apply", now) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let request_sha256 =
            edit_operation_hash(&task, &input.changes, input.dry_run.unwrap_or(false));
        match self.db.begin_connector_edit_operation(
            &task.task_id,
            &self.context.project_id,
            &task.owner_subject_id,
            &input.operation_id,
            &request_sha256,
            now,
        ) {
            Ok(ConnectorEditOperationGate::Started) => {}
            Ok(ConnectorEditOperationGate::Replay(mut output)) => {
                output["operation_id"] = json!(input.operation_id);
                output["idempotent_replay"] = json!(true);
                let cursor = match self.record_event(
                    &task,
                    "edits_apply",
                    json!({ "ok": true, "replay": true, "operation_id": input.operation_id }),
                    now,
                ) {
                    Ok(cursor) => cursor,
                    Err(outcome) => return outcome,
                };
                return ConnectorCallOutcome::success_at(&task, cursor, output);
            }
            Ok(ConnectorEditOperationGate::Pending) => {
                let cursor = self.record_event(
                    &task,
                    "edits_apply",
                    json!({ "ok": false, "operation_pending": true, "operation_id": input.operation_id }),
                    now,
                );
                return ConnectorCallOutcome::error_for_task_at(
                    409,
                    "edit_operation_uncertain",
                    "this operation did not reach a durable result; it will not be replayed automatically",
                    false,
                    true,
                    Some("Inspect task_review and the affected files, then use a new operation_id with fresh hashes only if another edit is needed."),
                    &task,
                    match cursor { Ok(cursor) => cursor, Err(outcome) => return outcome },
                    json!({ "operation_id": input.operation_id }),
                );
            }
            Ok(ConnectorEditOperationGate::Conflict) => {
                return ConnectorCallOutcome::error_for_task(
                    409,
                    "operation_id_conflict",
                    "operation_id was already used with different changes or preconditions",
                    false,
                    false,
                    Some("Use a new operation_id for a logically different edit batch."),
                    &task,
                    json!({ "operation_id": input.operation_id }),
                )
            }
            Err(error) => return store_error_outcome(error, Some(&task)),
        }
        let args = json!({
            "project": task.execution_executor_ref,
            "changes": input.changes,
            "dry_run": input.dry_run.unwrap_or(false)
        });
        match self
            .invoke_kernel("apply_text_edits", args, &task, auth, transport)
            .await
        {
            Ok(mut output) => {
                output["operation_id"] = json!(input.operation_id);
                output["idempotent_replay"] = json!(false);
                if let Err(error) = self.db.complete_connector_edit_operation(
                    &task.task_id,
                    &self.context.project_id,
                    &task.owner_subject_id,
                    &input.operation_id,
                    &request_sha256,
                    &output,
                    now,
                ) {
                    return store_error_outcome(error, Some(&task));
                }
                let cursor = match self.record_event(
                    &task,
                    "edits_apply",
                    json!({
                        "ok": true,
                        "dry_run": input.dry_run.unwrap_or(false),
                        "operation_id": input.operation_id,
                        "change_count": input.changes.len()
                    }),
                    now,
                ) {
                    Ok(cursor) => cursor,
                    Err(outcome) => return outcome,
                };
                ConnectorCallOutcome::success_at(&task, cursor, output)
            }
            Err(error) => {
                let uncertain = kernel_failure_may_have_applied(&error);
                if !uncertain {
                    if let Err(store_error) = self.db.fail_connector_edit_operation(
                        &task.task_id,
                        &input.operation_id,
                        &request_sha256,
                        now,
                    ) {
                        return store_error_outcome(store_error, Some(&task));
                    }
                }
                let cursor = self.record_event(
                    &task,
                    "edits_apply",
                    json!({
                        "ok": false,
                        "dry_run": input.dry_run.unwrap_or(false),
                        "operation_id": input.operation_id,
                        "operation_uncertain": uncertain
                    }),
                    now,
                );
                if uncertain {
                    return ConnectorCallOutcome::error_for_task_at(
                        409,
                        "edit_operation_uncertain",
                        "the edit did not reach a confirmed completed or fully rolled-back state; automatic replay is disabled",
                        false,
                        true,
                        Some("Inspect task_review and affected files before issuing any new edit operation."),
                        &task,
                        match cursor { Ok(cursor) => cursor, Err(outcome) => return outcome },
                        json!({ "operation_id": input.operation_id }),
                    );
                }
                self.kernel_error_outcome(error, &task, cursor, Value::Null)
            }
        }
    }

    async fn checks_run(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        _transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: ChecksRunInput = match parse_input("checks_run", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if let Err(message) = validate_operation_id(&input.operation_id) {
            return invalid_input("checks_run", message);
        }
        if input.checks.is_empty() || input.checks.len() > 3 {
            return invalid_input("checks_run", "checks must contain 1..=3 entries");
        }
        let unique = input.checks.iter().copied().collect::<HashSet<_>>();
        if unique.len() != input.checks.len() {
            return invalid_input("checks_run", "checks must not contain duplicates");
        }
        if input
            .timeout_secs
            .is_some_and(|value| !(1..=120).contains(&value))
        {
            return invalid_input("checks_run", "timeout_secs must be 1..=120");
        }
        if let Some(cwd) = input.cwd.as_deref() {
            if let Err(message) = validate_path(cwd) {
                return invalid_input("checks_run", message);
            }
        }
        if input
            .test_filter
            .as_deref()
            .is_some_and(|filter| filter.len() > 500)
        {
            return invalid_input("checks_run", "test_filter must be at most 500 bytes");
        }
        #[cfg(test)]
        if let Some(entered) = self.mutation_before_task_lock.lock().unwrap().clone() {
            entered.add_permits(1);
        }
        let task_lock = self.task_lock(&input.task_id);
        let task_guard = task_lock.lock().await;
        let task = match self.active_writable_task(&input.task_id, subject_id, "checks_run", now) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let resolved = match resolve_validation_recipe(
            Path::new(&task.execution_root),
            input.cwd.as_deref(),
            input.recipe,
            &input.checks,
            input.test_filter.as_deref(),
        ) {
            Ok(resolved) => resolved,
            Err(error) => return validation_recipe_error(&task, error),
        };
        let validation_steps = resolved.steps.clone();
        let recipe_identity = resolved.durable_identity();
        let timeout_secs = input.timeout_secs.unwrap_or(120);
        let request_sha256 = check_request_hash(
            &task,
            &recipe_identity,
            input.cwd.as_deref(),
            resolved.test_filter.as_deref(),
            timeout_secs,
        );
        let existing = match self.db.latest_connector_execution(
            &task.task_id,
            &self.context.project_id,
            subject_id,
            Some(&input.operation_id),
        ) {
            Ok(Some(execution)) if execution.request_sha256 != request_sha256 => {
                return store_error_outcome(
                    ConnectorTaskStoreError::OperationIdConflict(input.operation_id),
                    Some(&task),
                )
            }
            Ok(execution) => execution.map(ConnectorExecutionReservation::Existing),
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        let plan = input
            .checks
            .iter()
            .map(|check| check.as_str().to_string())
            .collect::<Vec<_>>();
        let reservation = match existing {
            Some(existing) => existing,
            None => {
                let client_id = task
                    .execution_executor_ref
                    .strip_prefix("agent:")
                    .and_then(|rest| rest.split_once(':'))
                    .map(|(client_id, _)| client_id);
                let supported = match client_id {
                    Some(client_id) => self
                        .tools
                        .shell_clients
                        .client_supports_for_auth(
                            client_id,
                            SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV,
                            Some(auth),
                        )
                        .await
                        .unwrap_or(false),
                    None => false,
                };
                if !supported {
                    return ConnectorCallOutcome::error_for_task(
                        409,
                        "structured_validation_unavailable",
                        "the selected local Agent does not support structured validation jobs",
                        false,
                        true,
                        Some("Upgrade and reconnect the WebCodex Agent, then retry checks_run."),
                        &task,
                        json!({
                            "required_capability":
                                SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV
                        }),
                    );
                }
                let check_workspace_sha256 =
                    match self.workspace_fingerprint(&task, "checks_run").await {
                        Ok(fingerprint) => fingerprint,
                        Err(outcome) => return outcome,
                    };
                match self.executions.reserve(
                    &task,
                    "check",
                    &input.operation_id,
                    &request_sha256,
                    &plan,
                    Some(&recipe_identity),
                    Some(&check_workspace_sha256),
                    timeout_secs,
                    now,
                ) {
                    Ok(reservation) => reservation,
                    Err(error) => return store_error_outcome(error, Some(&task)),
                }
            }
        };
        let execution_cwd = Path::new(&task.execution_root)
            .join(&resolved.recipe_root_relative)
            .to_string_lossy()
            .into_owned();
        drop(task_guard);
        self.execution_outcome(
            self.executions
                .execute(
                    reservation,
                    task.clone(),
                    "structured validation".to_string(),
                    Some(execution_cwd),
                    timeout_secs,
                    auth.clone(),
                    validation_steps,
                )
                .await,
            &task,
            auth,
        )
        .await
    }

    async fn commands_run(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        _transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: CommandsRunInput = match parse_input("commands_run", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if let Err(message) = validate_operation_id(&input.operation_id) {
            return invalid_input("commands_run", message);
        }
        if input.command.trim().is_empty() || input.command.len() > 32768 {
            return invalid_input("commands_run", "command must be 1..=32768 bytes");
        }
        if input
            .timeout_secs
            .is_some_and(|value| !(1..=120).contains(&value))
        {
            return invalid_input("commands_run", "timeout_secs must be 1..=120");
        }
        if let Some(cwd) = input.cwd.as_deref() {
            if let Err(message) = validate_path(cwd) {
                return invalid_input("commands_run", message);
            }
        }
        #[cfg(test)]
        if let Some(entered) = self.mutation_before_task_lock.lock().unwrap().clone() {
            entered.add_permits(1);
        }
        let task_lock = self.task_lock(&input.task_id);
        let task_guard = task_lock.lock().await;
        let task = match self.active_writable_task(&input.task_id, subject_id, "commands_run", now)
        {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let timeout_secs = input.timeout_secs.unwrap_or(120);
        let request_sha256 =
            command_request_hash(&task, &input.command, input.cwd.as_deref(), timeout_secs);
        let existing = match self.db.latest_connector_execution(
            &task.task_id,
            &self.context.project_id,
            subject_id,
            Some(&input.operation_id),
        ) {
            Ok(Some(execution)) if execution.request_sha256 != request_sha256 => {
                return store_error_outcome(
                    ConnectorTaskStoreError::OperationIdConflict(input.operation_id),
                    Some(&task),
                )
            }
            Ok(execution) => execution.map(ConnectorExecutionReservation::Existing),
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        let reservation = match existing {
            Some(existing) => existing,
            None => {
                let manager = self.workspace.clone();
                let task_for_precondition = task.clone();
                let precondition = match tokio::task::spawn_blocking(move || {
                    manager.action_precondition(&task_for_precondition)
                })
                .await
                {
                    Ok(Ok(precondition)) => precondition,
                    Ok(Err(message)) => {
                        let cursor = self.record_event(
                            &task,
                            "commands_run",
                            json!({ "ok": false, "stage": "approval_precondition" }),
                            now,
                        );
                        let cursor = cursor.unwrap_or(task.event_cursor);
                        return ConnectorCallOutcome::error_for_task_at(
                            409,
                            "approval_precondition_failed",
                            self.sanitize_task_string(&task, &message),
                            false,
                            true,
                            Some("Resolve the Git workspace issue, then retry."),
                            &task,
                            cursor,
                            Value::Null,
                        );
                    }
                    Err(error) => {
                        tracing::error!(error = %error, "connector approval precondition task failed");
                        return ConnectorCallOutcome::error_for_task(
                            500,
                            "approval_precondition_failed",
                            "connector could not capture the command precondition",
                            false,
                            true,
                            Some("Inspect server logs before retrying the command request."),
                            &task,
                            Value::Null,
                        );
                    }
                };
                let action_hash = command_action_hash(&request_sha256, &precondition);
                let action_summary = format!(
                    "raw project command ({} bytes{}, workspace {})",
                    input.command.len(),
                    input
                        .cwd
                        .as_deref()
                        .map(|cwd| format!(", cwd {cwd}"))
                        .unwrap_or_default(),
                    short_oid(&precondition)
                );
                let gate = match self.db.request_or_consume_connector_approval(
                    &task.task_id,
                    &self.context.project_id,
                    subject_id,
                    "commands_run",
                    &action_hash,
                    &action_summary,
                    now,
                    now + COMMAND_APPROVAL_TTL_SECS,
                ) {
                    Ok(gate) => gate,
                    Err(error) => return store_error_outcome(error, Some(&task)),
                };
                if !matches!(&gate, ConnectorApprovalGate::Authorized(_)) {
                    let current = self.task(&task.task_id, subject_id).unwrap_or(task);
                    return approval_gate_outcome(gate, &current);
                }
                match self.executions.reserve(
                    &task,
                    "command",
                    &input.operation_id,
                    &request_sha256,
                    &[],
                    None,
                    None,
                    timeout_secs,
                    chrono::Utc::now().timestamp(),
                ) {
                    Ok(reservation) => reservation,
                    Err(error) => return store_error_outcome(error, Some(&task)),
                }
            }
        };
        drop(task_guard);
        self.execution_outcome(
            self.executions
                .execute(
                    reservation,
                    task.clone(),
                    input.command,
                    input.cwd,
                    timeout_secs,
                    auth.clone(),
                    Vec::new(),
                )
                .await,
            &task,
            auth,
        )
        .await
    }

    async fn execution_outcome(
        &self,
        result: Result<crate::db::ConnectorExecution, ConnectorTaskStoreError>,
        task: &ConnectorTaskSnapshot,
        auth: &AuthContext,
    ) -> ConnectorCallOutcome {
        let current = self
            .task(&task.task_id, &task.owner_subject_id)
            .unwrap_or_else(|_| task.clone());
        match result {
            Ok(execution) => {
                let projection = self.executions.projection(&execution, auth, true).await;
                ConnectorCallOutcome::success_blocking_at(
                    &current,
                    current.event_cursor,
                    json!({ "execution": projection }),
                    execution.blocks_finish(),
                )
            }
            Err(error) => store_error_outcome(error, Some(&current)),
        }
    }

    async fn task_review(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        transport: ConnectorTransport,
    ) -> ConnectorCallOutcome {
        let input: TaskReviewInput = match parse_input("task_review", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if input.after_cursor.is_some_and(|cursor| cursor < 0) {
            return invalid_input("task_review", "after_cursor must be non-negative");
        }
        if input.wait_ms.is_some_and(|wait| wait > 15_000) {
            return invalid_input("task_review", "wait_ms must be 0..=15000");
        }
        if input
            .max_events
            .is_some_and(|count| count == 0 || count > MAX_EVENT_COUNT)
        {
            return invalid_input("task_review", "max_events must be 1..=50");
        }
        let initial_task = match self.task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let review = match self
            .executions
            .wait_for_review(initial_task, input.after_cursor, input.wait_ms.unwrap_or(0))
            .await
        {
            Ok(review) => review,
            Err(error) => return store_error_outcome(error, None),
        };
        let task = review.task;
        let result =
            match self
                .db
                .connector_task_result(&task.task_id, &self.context.project_id, subject_id)
            {
                Ok(result) => result,
                Err(error) => return store_error_outcome(error, Some(&task)),
            };
        let changes = if let Some(result) = result.as_ref() {
            let diff_preview = if input.include_diff.unwrap_or(true) {
                match workspace::WorkspaceManager::patch_preview(
                    result,
                    CONNECTOR_PATCH_PREVIEW_BYTES,
                ) {
                    Ok(preview) => preview,
                    Err(message) => {
                        return ConnectorCallOutcome::error_for_task(
                            409,
                            "result_artifact_unavailable",
                            self.sanitize_task_string(&task, &message),
                            false,
                            true,
                            Some("Inspect the local task state before accepting this result."),
                            &task,
                            Value::Null,
                        )
                    }
                }
            } else {
                None
            };
            self.sanitize_task_value(
                &task,
                json!({
                    "source": "stable_task_result",
                    "patch_sha256": result.patch_sha256,
                    "patch_bytes": result.patch_bytes,
                    "changed_paths": result.changed_paths,
                    "warnings": result.warnings,
                    "diff_preview": diff_preview
                }),
            )
        } else if task.task_status == "cancelled" {
            json!({
                "source": "cancelled_task",
                "changed_paths": [],
                "diff_preview": null
            })
        } else if review
            .execution
            .as_ref()
            .is_some_and(crate::db::ConnectorExecution::is_active)
        {
            json!({
                "source": "live_workspace_deferred",
                "reason": "execution_active",
                "changed_paths": [],
                "diff_preview": null
            })
        } else {
            match self
                .invoke_kernel(
                    "show_changes",
                    json!({
                        "project": task.execution_executor_ref,
                        "include_diff": input.include_diff.unwrap_or(true),
                        "max_hunks": 20,
                        "max_hunk_lines": 80,
                        "session_event_limit": 0
                    }),
                    &task,
                    auth,
                    transport,
                )
                .await
            {
                Ok(output) => output,
                Err(error) => {
                    return self.kernel_error_outcome(
                        error,
                        &task,
                        Ok(task.event_cursor),
                        Value::Null,
                    )
                }
            }
        };
        let events = match self.db.connector_task_events(
            &task.task_id,
            &task.project_id,
            &task.owner_subject_id,
            MAX_EVENT_COUNT,
        ) {
            Ok(events) => events,
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        let max_events = input.max_events.unwrap_or(MAX_EVENT_COUNT);
        let mut events = events
            .into_iter()
            .filter(|event| {
                input
                    .after_cursor
                    .is_none_or(|cursor| event.sequence > cursor)
            })
            .collect::<Vec<_>>();
        events.drain(..events.len().saturating_sub(max_events));
        let execution = match review.execution.as_ref() {
            Some(execution) => Some(
                self.executions
                    .projection(execution, auth, input.include_output_tail.unwrap_or(true))
                    .await,
            ),
            None => None,
        };
        let blocking = review
            .execution
            .as_ref()
            .is_some_and(|execution| execution.blocks_finish());
        let next_action = execution
            .as_ref()
            .and_then(|value| value["next_action"].as_str())
            .unwrap_or(if task.task_status == "cancelled" {
                "start_a_new_task"
            } else if task.run_status == "interrupted" {
                "resume_or_reject_on_the_host"
            } else {
                "continue_or_finish"
            });
        ConnectorCallOutcome::success_blocking_at(
            &task,
            task.event_cursor,
            json!({
                "goal": task.goal,
                "mode": task.mode,
                "status": task.task_status,
                "run_status": task.run_status,
                "changes": changes,
                "result": result.as_ref().map(result_projection),
                "active_execution": execution.as_ref().filter(|_| {
                    review.execution.as_ref().is_some_and(
                        crate::db::ConnectorExecution::is_active
                    )
                }),
                "recent_execution": execution,
                "recent_events": events,
                "heartbeat": review.heartbeat,
                "next_action": next_action
            }),
            blocking,
        )
    }

    async fn task_cancel(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
    ) -> ConnectorCallOutcome {
        let input: TaskCancelInput = match parse_input("task_cancel", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if input
            .reason
            .as_deref()
            .is_some_and(|reason| reason.trim().is_empty() || reason.len() > 500)
        {
            return invalid_input("task_cancel", "reason must be 1..=500 bytes when provided");
        }
        let task_lock = self.task_lock(&input.task_id);
        let _task_guard = task_lock.lock().await;
        let task = match self.task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let execution = match self
            .executions
            .cancel_task(task.clone(), input.reason.as_deref(), auth.clone())
            .await
        {
            Ok(execution) => execution,
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        let current = self.task(&task.task_id, subject_id).unwrap_or(task);
        let projection = match execution.as_ref() {
            Some(execution) => Some(self.executions.projection(execution, auth, true).await),
            None => None,
        };
        let blocking = execution
            .as_ref()
            .is_some_and(|execution| execution.blocks_finish());
        ConnectorCallOutcome::success_blocking_at(
            &current,
            current.event_cursor,
            json!({
                "status": current.task_status,
                "run_status": current.run_status,
                "execution": projection,
                "cancellation": if blocking { "requested" } else { "terminal" },
                "next_action": if blocking {
                    "wait_with_task_review"
                } else {
                    "start_a_new_task_if_more_work_is_needed"
                }
            }),
            blocking,
        )
    }

    async fn task_finish(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        _transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: TaskFinishInput = match parse_input("task_finish", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
        if input.summary.trim().is_empty() || input.summary.len() > 4000 {
            return invalid_input("task_finish", "summary must be 1..=4000 bytes");
        }
        let task_lock = self.task_lock(&input.task_id);
        let task_guard = task_lock.lock().await;
        let visible_task = match self.task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let blocker = match self.db.connector_finish_blocker(&input.task_id) {
            Ok(blocker) => blocker,
            Err(error) => return store_error_outcome(error, Some(&visible_task)),
        };
        if let Some(execution) = blocker {
            let projection = self.executions.projection(&execution, auth, true).await;
            return ConnectorCallOutcome::error_for_task(
                409,
                "execution_not_terminal",
                "task_finish is blocked until the active execution reaches a known terminal state",
                true,
                execution.state == "unknown",
                Some(if execution.state == "unknown" {
                    "Inspect the executor state on the host before finishing this task."
                } else {
                    "Use task_review to wait for completion or task_cancel to stop the execution."
                }),
                &visible_task,
                json!({ "execution": projection }),
            );
        }
        let task = match self.active_task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let _workspace_guard = if task.isolated {
            Some(self.workspace_ops.lock().await)
        } else {
            None
        };
        let check_execution = match self.db.latest_connector_execution_by_kind(
            &task.task_id,
            &self.context.project_id,
            subject_id,
            "check",
        ) {
            Ok(execution) => execution,
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        if task.mode == "normal" && check_execution.is_none() {
            return ConnectorCallOutcome::error_for_task(
                409,
                "checks_required",
                "a normal coding result must run structured checks before task_finish",
                false,
                true,
                Some("Call checks_run with a new operation_id, then retry task_finish."),
                &task,
                json!({}),
            );
        }
        if let Some(check) = check_execution
            .as_ref()
            .filter(|check| check.state == "succeeded")
        {
            let Some(validated) = check.validated_workspace_sha256.as_deref() else {
                return checks_stale_outcome(
                    &task,
                    check,
                    "the latest successful check has no trusted workspace provenance",
                );
            };
            let current = match self.workspace_fingerprint(&task, "task_finish").await {
                Ok(current) => current,
                Err(outcome) => return outcome,
            };
            if current != validated {
                return checks_stale_outcome(
                    &task,
                    check,
                    "the workspace changed after the latest successful check",
                );
            }
            #[cfg(test)]
            let finish_hook = { self.finish_after_fingerprint.lock().unwrap().clone() };
            #[cfg(test)]
            if let Some((reached, resume)) = finish_hook {
                reached.notify_one();
                resume.notified().await;
            }
        }
        let manager = self.workspace.clone();
        let task_for_capture = task.clone();
        let captured =
            match tokio::task::spawn_blocking(move || manager.capture_result(&task_for_capture))
                .await
            {
                Ok(Ok(captured)) => captured,
                Ok(Err(message)) => {
                    let cursor = self.record_event(
                        &task,
                        "task_finish",
                        json!({ "ok": false, "stage": "capture_result" }),
                        now,
                    );
                    let cursor = cursor.unwrap_or(task.event_cursor);
                    return ConnectorCallOutcome::error_for_task_at(
                        409,
                        "result_capture_failed",
                        self.sanitize_task_string(&task, &message),
                        false,
                        true,
                        Some("Resolve the reported workspace issue, then retry task_finish."),
                        &task,
                        cursor,
                        Value::Null,
                    );
                }
                Err(error) => {
                    tracing::error!(error = %error, "connector result capture task failed");
                    return ConnectorCallOutcome::error_for_task(
                        500,
                        "result_capture_failed",
                        "connector could not capture a stable task result",
                        false,
                        true,
                        Some("Inspect server logs before retrying task_finish."),
                        &task,
                        Value::Null,
                    );
                }
            };
        let validation = validation_projection(check_execution.as_ref());
        let result_id = format!("wc_result_{}", uuid::Uuid::new_v4().simple());
        let mut cursor = match self.db.finish_connector_task(
            &task.task_id,
            &self.context.project_id,
            subject_id,
            NewConnectorResult {
                result_id: &result_id,
                summary: input.summary.trim(),
                patch_artifact: captured.patch_artifact.as_deref(),
                patch_sha256: captured.patch_sha256.as_deref(),
                patch_bytes: captured.patch_bytes,
                changed_paths: &captured.changed_paths,
                validation: &validation,
                warnings: &captured.warnings,
            },
            now,
        ) {
            Ok(cursor) => cursor,
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        drop(task_guard);
        let cleanup_warning = if task.isolated {
            let manager = self.workspace.clone();
            let task_for_release = task.clone();
            match tokio::task::spawn_blocking(move || {
                manager.release_task_workspace(&task_for_release)
            })
            .await
            {
                Ok(warning) => warning,
                Err(error) => {
                    tracing::error!(error = %error, "connector workspace release task failed");
                    Some("connector could not release the reusable execution workspace".to_string())
                }
            }
        } else {
            None
        }
        .map(|warning| self.sanitize_task_string(&task, &warning));
        if task.isolated {
            match self.db.record_connector_workspace_release(
                &task.task_id,
                &self.context.project_id,
                subject_id,
                cleanup_warning.is_none(),
                cleanup_warning.as_deref(),
                now,
            ) {
                Ok(release_cursor) => cursor = release_cursor,
                Err(error) => {
                    tracing::warn!(error = %error, task_id = %task.task_id, "Could not record connector workspace release");
                }
            }
        }
        let workspace_released = !task.isolated || cleanup_warning.is_none();
        ConnectorCallOutcome::success_at(
            &task,
            cursor,
            json!({
                "status": "ready_for_review",
                "run_status": "completed",
                "summary": input.summary.trim(),
                "result": {
                    "result_id": result_id,
                    "patch_sha256": captured.patch_sha256,
                    "patch_bytes": captured.patch_bytes,
                    "changed_paths": captured.changed_paths,
                    "validation": validation,
                    "warnings": captured.warnings,
                    "decision_status": "pending",
                    "cleanup_warning": cleanup_warning.clone()
                },
                "workspace": {
                    "strategy": if task.isolated { "reusable_slot" } else { "target_checkout" },
                    "released": workspace_released
                },
                "human_action": format!(
                    "Run 'webcodex task show {}', then accept or reject the result locally.",
                    task.task_id
                )
            }),
        )
    }

    fn task(
        &self,
        task_id: &str,
        subject_id: &str,
    ) -> Result<ConnectorTaskSnapshot, ConnectorCallOutcome> {
        validate_task_id(task_id).map_err(|message| invalid_input("task", message))?;
        self.db
            .connector_task(task_id, &self.context.project_id, subject_id)
            .map_err(|error| store_error_outcome(error, None))
    }

    fn active_task(
        &self,
        task_id: &str,
        subject_id: &str,
    ) -> Result<ConnectorTaskSnapshot, ConnectorCallOutcome> {
        let task = self.task(task_id, subject_id)?;
        if task.run_status == "interrupted" {
            return Err(ConnectorCallOutcome::error_for_task(
                409,
                "task_interrupted",
                "this task was interrupted when the local connector runtime stopped",
                false,
                true,
                Some("Review the task, then resume it from the WebCodex host before continuing."),
                &task,
                json!({
                    "local_command": format!("webcodex task resume {}", task.task_id)
                }),
            ));
        }
        if task.task_status != "active" || task.run_status != "running" {
            return Err(ConnectorCallOutcome::error_for_task(
                409,
                "task_not_active",
                "this task is already ready for review; start a new task for additional work",
                false,
                true,
                Some("Call task_start with the next requested outcome."),
                &task,
                Value::Null,
            ));
        }
        Ok(task)
    }

    fn active_writable_task(
        &self,
        task_id: &str,
        subject_id: &str,
        capability: &str,
        now: i64,
    ) -> Result<ConnectorTaskSnapshot, ConnectorCallOutcome> {
        let task = self.active_task(task_id, subject_id)?;
        if task.mode == "read_only" {
            let cursor = self.record_event(
                &task,
                capability,
                json!({ "ok": false, "denied": "read_only" }),
                now,
            );
            let cursor = cursor.unwrap_or(task.event_cursor);
            return Err(ConnectorCallOutcome::error_for_task_at(
                403,
                "read_only_task",
                format!("{capability} is unavailable because this task is read_only"),
                false,
                true,
                Some("Start a normal task only after the user authorizes changes or execution."),
                &task,
                cursor,
                Value::Null,
            ));
        }
        Ok(task)
    }

    fn record_event(
        &self,
        task: &ConnectorTaskSnapshot,
        capability: &str,
        payload: Value,
        now: i64,
    ) -> Result<i64, ConnectorCallOutcome> {
        self.db
            .append_connector_task_event(
                &task.task_id,
                &self.context.project_id,
                &task.owner_subject_id,
                capability,
                &payload,
                now,
            )
            .map_err(|error| store_error_outcome(error, Some(task)))
    }

    async fn invoke_kernel(
        &self,
        tool_name: &str,
        arguments: Value,
        task: &ConnectorTaskSnapshot,
        auth: &AuthContext,
        transport: ConnectorTransport,
    ) -> Result<Value, KernelFailure> {
        let outcome = self
            .tools
            .call_tool_with_context(
                KernelToolCallRequest {
                    tool_name: tool_name.to_string(),
                    arguments,
                },
                ToolCallContext {
                    transport: transport.into(),
                    session_id: None,
                    auth: Some(auth),
                    record_oauth_scope_denials: false,
                },
            )
            .await;
        match outcome.error_status {
            Some(ToolCallErrorStatus::InsufficientScope {
                required_scope,
                description,
            }) => Err(KernelFailure::Scope {
                required_scope,
                message: description,
            }),
            Some(ToolCallErrorStatus::InvalidArguments { message }) => {
                Err(KernelFailure::Adapter(message))
            }
            None => {
                let result = outcome
                    .result
                    .expect("tool kernel outcome without error must include result");
                if result.success {
                    Ok(self.sanitize_task_value(task, result.output))
                } else {
                    Err(KernelFailure::Tool(result))
                }
            }
        }
    }

    fn kernel_error_outcome(
        &self,
        error: KernelFailure,
        task: &ConnectorTaskSnapshot,
        cursor: Result<i64, ConnectorCallOutcome>,
        partial_data: Value,
    ) -> ConnectorCallOutcome {
        let cursor = match cursor {
            Ok(cursor) => cursor,
            Err(outcome) => return outcome,
        };
        match error {
            KernelFailure::Scope {
                required_scope,
                message,
            } => ConnectorCallOutcome::error_for_task_at_with_scope(
                403,
                "insufficient_scope",
                message,
                false,
                true,
                Some(
                    "Grant the required connector scope and retry only after checking task_review.",
                ),
                task,
                cursor,
                partial_data,
                required_scope,
            ),
            KernelFailure::Adapter(message) => ConnectorCallOutcome::error_for_task_at(
                500,
                "connector_adapter_error",
                format!(
                    "connector could not translate the capability: {}",
                    self.sanitize_task_string(task, &message)
                ),
                false,
                true,
                Some("Inspect server logs; do not retry a consequential call automatically."),
                task,
                cursor,
                partial_data,
            ),
            KernelFailure::Tool(result) => {
                let message = result
                    .error
                    .as_deref()
                    .map(|message| self.sanitize_task_string(task, message))
                    .unwrap_or_else(|| "executor rejected the capability".to_string());
                let output = self.sanitize_task_value(task, result.output);
                ConnectorCallOutcome::error_for_task_at(
                    400,
                    "capability_failed",
                    message,
                    false,
                    false,
                    Some("Use the returned diagnostics, inspect if needed, then retry with a corrected call."),
                    task,
                    cursor,
                    json!({ "partial": partial_data, "executor": output }),
                )
            }
        }
    }

    fn sanitize_executor_string(&self, value: &str) -> String {
        value
            .replace(&self.context.executor_project, &self.context.project_id)
            .replace(&self.context.executor_root, ".")
            .replace(&self.context.runs_root, "<managed-runs>")
            .replace(&self.context.results_root, "<managed-results>")
            .replace(&self.context.projects_dir, "<managed-projects>")
    }

    fn sanitize_task_value(&self, task: &ConnectorTaskSnapshot, mut value: Value) -> Value {
        sanitize_value(
            &mut value,
            &task.execution_executor_ref,
            &self.context.project_id,
            &task.execution_root,
        );
        value
    }

    fn sanitize_task_string(&self, task: &ConnectorTaskSnapshot, value: &str) -> String {
        value
            .replace(&task.execution_executor_ref, &self.context.project_id)
            .replace(&task.execution_root, ".")
            .replace(&self.context.runs_root, "<managed-runs>")
            .replace(&self.context.results_root, "<managed-results>")
            .replace(&self.context.projects_dir, "<managed-projects>")
    }
}

// NOTE: The subject is intentionally passed explicitly rather than stored as
// current connector state. Two devices for one user share a subject; two users
// do not share task ids even when requests interleave on the same connector.

#[derive(Debug)]
enum KernelFailure {
    Scope {
        required_scope: Option<&'static str>,
        message: String,
    },
    Adapter(String),
    Tool(ToolResult),
}

impl ConnectorCallOutcome {
    fn success(task: &ConnectorTaskSnapshot, data: Value) -> Self {
        Self::success_at(task, task.event_cursor, data)
    }

    fn success_at(task: &ConnectorTaskSnapshot, cursor: i64, data: Value) -> Self {
        Self::success_blocking_at(task, cursor, data, false)
    }

    fn success_blocking_at(
        task: &ConnectorTaskSnapshot,
        cursor: i64,
        data: Value,
        blocking: bool,
    ) -> Self {
        Self {
            ok: true,
            body: json!({
                "ok": true,
                "task_id": task.task_id,
                "run_id": task.run_id,
                "event_cursor": cursor,
                "data": data,
                "warnings": [],
                "blocking": blocking
            }),
            http_status: 200,
            required_scope: None,
            protocol_error: false,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn error(
        http_status: u16,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
        suggested_action: Option<&str>,
        required_scope: Option<&'static str>,
        protocol_error: bool,
    ) -> Self {
        Self {
            ok: false,
            body: error_envelope(
                None,
                None,
                None,
                Value::Null,
                code,
                message,
                retryable,
                user_action_required,
                suggested_action,
            ),
            http_status,
            required_scope,
            protocol_error,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn error_for_task(
        http_status: u16,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
        suggested_action: Option<&str>,
        task: &ConnectorTaskSnapshot,
        data: Value,
    ) -> Self {
        Self::error_for_task_at(
            http_status,
            code,
            message,
            retryable,
            user_action_required,
            suggested_action,
            task,
            task.event_cursor,
            data,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn error_for_task_at(
        http_status: u16,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
        suggested_action: Option<&str>,
        task: &ConnectorTaskSnapshot,
        cursor: i64,
        data: Value,
    ) -> Self {
        Self::error_for_task_at_with_scope(
            http_status,
            code,
            message,
            retryable,
            user_action_required,
            suggested_action,
            task,
            cursor,
            data,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn error_for_task_at_with_scope(
        http_status: u16,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        user_action_required: bool,
        suggested_action: Option<&str>,
        task: &ConnectorTaskSnapshot,
        cursor: i64,
        data: Value,
        required_scope: Option<&'static str>,
    ) -> Self {
        Self {
            ok: false,
            body: error_envelope(
                Some(&task.task_id),
                Some(&task.run_id),
                Some(cursor),
                data,
                code,
                message,
                retryable,
                user_action_required,
                suggested_action,
            ),
            http_status,
            required_scope,
            protocol_error: false,
        }
    }

    fn scope_denied(scope: &'static str) -> Self {
        Self::error(
            403,
            "insufficient_scope",
            format!("missing required scope: {scope}"),
            false,
            true,
            Some("Grant the required scope to this connector credential."),
            Some(scope),
            false,
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn error_envelope(
    task_id: Option<&str>,
    run_id: Option<&str>,
    event_cursor: Option<i64>,
    data: Value,
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
    user_action_required: bool,
    suggested_action: Option<&str>,
) -> Value {
    json!({
        "ok": false,
        "task_id": task_id,
        "run_id": run_id,
        "event_cursor": event_cursor,
        "data": data,
        "warnings": [],
        "blocking": true,
        "error": {
            "code": code.into(),
            "message": message.into(),
            "retryable": retryable,
            "user_action_required": user_action_required,
            "suggested_action": suggested_action
        }
    })
}

fn parse_input<T: DeserializeOwned>(
    capability: &str,
    arguments: Value,
) -> Result<T, ConnectorCallOutcome> {
    serde_json::from_value(arguments)
        .map_err(|error| invalid_input(capability, format!("invalid input: {error}")))
}

fn invalid_input(capability: &str, message: impl Into<String>) -> ConnectorCallOutcome {
    ConnectorCallOutcome::error(
        400,
        "invalid_arguments",
        format!("{capability}: {}", message.into()),
        false,
        false,
        Some("Correct the capability arguments using its advertised schema."),
        None,
        true,
    )
}

fn store_error_outcome(
    error: ConnectorTaskStoreError,
    task: Option<&ConnectorTaskSnapshot>,
) -> ConnectorCallOutcome {
    match error {
        ConnectorTaskStoreError::NotFound => ConnectorCallOutcome::error(
            404,
            "task_not_found",
            "task was not found in this connector project and identity context",
            false,
            false,
            Some("Use the task_id returned by task_start for this connector."),
            None,
            false,
        ),
        ConnectorTaskStoreError::OperationIdConflict(operation_id) => match task {
            Some(task) => ConnectorCallOutcome::error_for_task(
                409,
                "operation_id_conflict",
                "operation_id was already used with a different execution request",
                false,
                false,
                Some("Reuse operation_id only for an exact retry; use a new value for an intentional rerun or different request."),
                task,
                json!({ "operation_id": operation_id }),
            ),
            None => ConnectorCallOutcome::error(
                409,
                "operation_id_conflict",
                "operation_id was already used with a different execution request",
                false,
                false,
                Some("Reuse operation_id only for an exact retry; use a new value for an intentional rerun or different request."),
                None,
                false,
            ),
        },
        ConnectorTaskStoreError::InvalidState(message) => match task {
            Some(task) => ConnectorCallOutcome::error_for_task(
                409,
                "task_not_active",
                message,
                false,
                true,
                Some("Start a new task for additional work."),
                task,
                Value::Null,
            ),
            None => ConnectorCallOutcome::error(
                409,
                "task_not_active",
                message,
                false,
                true,
                Some("Start a new task for additional work."),
                None,
                false,
            ),
        },
        ConnectorTaskStoreError::Storage(error) => {
            tracing::error!(error = %error, "connector task store operation failed");
            match task {
                Some(task) => ConnectorCallOutcome::error_for_task(
                    500,
                    "task_store_error",
                    "connector could not durably record task state",
                    false,
                    true,
                    Some("Inspect server logs and task_review before retrying any consequential call."),
                    task,
                    Value::Null,
                ),
                None => ConnectorCallOutcome::error(
                    500,
                    "task_store_error",
                    "connector could not durably record task state",
                    false,
                    true,
                    Some("Inspect server logs before retrying."),
                    None,
                    false,
                ),
            }
        }
    }
}

fn approval_gate_outcome(
    gate: ConnectorApprovalGate,
    task: &ConnectorTaskSnapshot,
) -> ConnectorCallOutcome {
    let (approval, code, message, suggested_action) = match gate {
        ConnectorApprovalGate::Pending(approval) => (
            approval,
            "approval_required",
            "this raw command is waiting for one-time approval on the WebCodex host",
            "Ask the user to approve this exact action locally, then retry commands_run unchanged.",
        ),
        ConnectorApprovalGate::Denied(approval) => (
            approval,
            "approval_denied",
            "the user denied this exact raw command",
            "Choose a safer action or ask the user for revised instructions.",
        ),
        ConnectorApprovalGate::Expired(approval) => (
            approval,
            "approval_expired",
            "the one-time approval request expired",
            "Retry commands_run unchanged to create a fresh local approval window.",
        ),
        ConnectorApprovalGate::Consumed(approval) => (
            approval,
            "approval_consumed",
            "the approval for this exact raw command was already consumed",
            "Review the task state before proposing a different action; approvals cannot be replayed.",
        ),
        ConnectorApprovalGate::Authorized(_) => {
            unreachable!("authorized commands continue to executor dispatch")
        }
    };
    ConnectorCallOutcome::error_for_task_at(
        409,
        code,
        message,
        false,
        true,
        Some(suggested_action),
        task,
        task.event_cursor,
        json!({
            "approval": approval_projection(&approval),
            "local_command": format!(
                "webcodex task approve {} {}",
                task.task_id, approval.approval_id
            )
        }),
    )
}

fn approval_projection(approval: &ConnectorApproval) -> Value {
    json!({
        "approval_id": approval.approval_id,
        "action_kind": approval.action_kind,
        "action_hash": approval.action_hash,
        "action_summary": approval.action_summary,
        "state": approval.state,
        "requested_at": approval.requested_at,
        "expires_at": approval.expires_at
    })
}

fn validation_recipe_error(
    task: &ConnectorTaskSnapshot,
    error: RecipeError,
) -> ConnectorCallOutcome {
    ConnectorCallOutcome::error_for_task(
        409,
        error.code,
        "validation recipe planning failed; inspect the stable code and safe details",
        false,
        true,
        Some("Resolve the reported recipe, manifest, cwd, or package-manager evidence and retry."),
        task,
        error.details.unwrap_or(Value::Null),
    )
}

fn command_request_hash(
    task: &ConnectorTaskSnapshot,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
) -> String {
    let mut hasher = Sha256::new();
    for field in [
        b"webcodex.commands_run.v2".as_slice(),
        task.task_id.as_bytes(),
        task.run_id.as_bytes(),
        command.as_bytes(),
        cwd.unwrap_or("").as_bytes(),
        &timeout_secs.to_be_bytes(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    format!("{:x}", hasher.finalize())
}

fn check_request_hash(
    task: &ConnectorTaskSnapshot,
    recipe_identity: &Value,
    cwd: Option<&str>,
    test_filter: Option<&str>,
    timeout_secs: u64,
) -> String {
    let recipe_identity = serde_json::to_vec(recipe_identity).unwrap_or_default();
    let mut hasher = Sha256::new();
    for field in [
        b"webcodex.checks_run.v3".as_slice(),
        task.task_id.as_bytes(),
        task.run_id.as_bytes(),
        recipe_identity.as_slice(),
        cwd.unwrap_or("").as_bytes(),
        test_filter.unwrap_or("").as_bytes(),
        &timeout_secs.to_be_bytes(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    format!("{:x}", hasher.finalize())
}

fn command_action_hash(request_sha256: &str, precondition: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"approval\0");
    hasher.update(request_sha256);
    hasher.update(b"\0");
    hasher.update(precondition);
    format!("{:x}", hasher.finalize())
}

fn edit_operation_hash(
    task: &ConnectorTaskSnapshot,
    changes: &[ApplyFileChangeInput],
    dry_run: bool,
) -> String {
    let serialized = serde_json::to_vec(changes).unwrap_or_default();
    let mut hasher = Sha256::new();
    for field in [
        b"webcodex.edits_apply.v2".as_slice(),
        task.task_id.as_bytes(),
        task.run_id.as_bytes(),
        &[u8::from(dry_run)],
        serialized.as_slice(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    format!("{:x}", hasher.finalize())
}

fn search_cursor_signature(input: &FilesSearchInput, page_limit: usize) -> String {
    let canonical = json!({
        "version": 1,
        "task_id": input.task_id,
        "pattern": input.pattern,
        "path": input.path,
        "page_limit": page_limit,
        "context_before": input.context_before.unwrap_or(0),
        "context_after": input.context_after.unwrap_or(0),
        "include_globs": input.include_globs,
        "exclude_globs": input.exclude_globs,
        "result_mode": input.result_mode.unwrap_or(SearchResultMode::Matches),
    });
    let bytes = serde_json::to_vec(&canonical).unwrap_or_default();
    format!("{:x}", Sha256::digest(bytes))
}

fn parse_search_cursor(cursor: &str, expected_signature: &str) -> Result<usize, ()> {
    let payload = cursor.strip_prefix("wc_search_").ok_or(())?;
    let (offset, signature) = payload.split_once('_').ok_or(())?;
    if signature != expected_signature {
        return Err(());
    }
    offset.parse::<usize>().map_err(|_| ())
}

fn paginate_search_output(
    mut output: Value,
    result_mode: SearchResultMode,
    offset: usize,
    page_limit: usize,
    signature: &str,
) -> Value {
    let key = if result_mode == SearchResultMode::Matches {
        "matches"
    } else {
        "files"
    };
    let records = output[key].as_array().cloned().unwrap_or_default();
    let page = records
        .iter()
        .skip(offset)
        .take(page_limit)
        .cloned()
        .collect::<Vec<_>>();
    let executor_truncated = output["truncated"].as_bool().unwrap_or(false);
    let more_in_records = records.len() > offset.saturating_add(page.len());
    let has_more = !page.is_empty() && (more_in_records || executor_truncated);
    let next_offset = offset.saturating_add(page.len());
    let next_cursor = (has_more && next_offset < CONNECTOR_SEARCH_WINDOW)
        .then(|| format!("wc_search_{next_offset}_{signature}"));
    let window_exhausted = has_more && next_cursor.is_none();
    output[key] = json!(page);
    output["truncated"] = json!(has_more);
    output["truncation_reason"] = if window_exhausted {
        json!("window_limit")
    } else if has_more {
        json!("page")
    } else {
        Value::Null
    };
    let returned = output[key].as_array().map(Vec::len).unwrap_or(0);
    if result_mode == SearchResultMode::Matches {
        output["count"] = json!(returned);
    } else {
        output["returned_file_count"] = json!(returned);
    }
    if result_mode == SearchResultMode::Count {
        let returned_match_count = output[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|item| item["match_count"].as_u64())
            .sum::<u64>();
        let complete = offset == 0 && !has_more;
        output["returned_match_count"] = json!(returned_match_count);
        output["count_complete"] = json!(complete);
        output["total_matches"] = if complete {
            json!(returned_match_count)
        } else {
            Value::Null
        };
    }
    output["page"] = json!({
        "offset": offset,
        "limit": page_limit,
        "returned": returned,
        "next_cursor": next_cursor,
        "window_limit": CONNECTOR_SEARCH_WINDOW,
        "window_exhausted": window_exhausted,
        "view": "live_sorted"
    });
    output
}

fn kernel_failure_may_have_applied(error: &KernelFailure) -> bool {
    let KernelFailure::Tool(result) = error else {
        return false;
    };
    if result
        .output
        .get("rollback_complete")
        .and_then(Value::as_bool)
        == Some(false)
        || result.output.get("changed").and_then(Value::as_bool) == Some(true)
    {
        return true;
    }
    result.error.as_deref().is_some_and(|message| {
        let message = message.to_ascii_lowercase();
        [
            "timed out",
            "request was dropped",
            "waiter was dropped",
            "disconnect",
        ]
        .iter()
        .any(|needle| message.contains(needle))
    })
}

fn result_projection(result: &ConnectorTaskResult) -> Value {
    json!({
        "result_id": result.result_id,
        "summary": result.summary,
        "patch_sha256": result.patch_sha256,
        "patch_bytes": result.patch_bytes,
        "changed_paths": result.changed_paths,
        "validation": result.validation,
        "warnings": result.warnings,
        "decision_status": result.decision_status,
        "decided_at": result.decided_at,
        "cleanup_warning": result.cleanup_warning
    })
}

fn project_brief(
    task: &ConnectorTaskSnapshot,
    overview: Option<&Value>,
    git_dirty: Option<bool>,
    git_conflict_count: Option<usize>,
) -> Value {
    let languages = overview
        .and_then(|value| value["project_types"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item["kind"].as_str())
        .take(8)
        .collect::<Vec<_>>();
    let manifests = overview
        .and_then(|value| value["manifests"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|item| item["path"].as_str())
        .take(12)
        .collect::<Vec<_>>();
    let instructions = overview
        .and_then(|value| value["key_files"].as_array())
        .into_iter()
        .flatten()
        .filter(|item| item["kind"] == "agent_instructions")
        .filter_map(|item| item["path"].as_str())
        .take(5)
        .collect::<Vec<_>>();
    let mut recommended_checks = Vec::new();
    for language in &languages {
        let checks: &[&str] = match *language {
            "rust" => &[
                "cargo fmt --check",
                "cargo check --all-targets",
                "cargo test",
            ],
            "node" => &["npm test"],
            "python" => &["python -m pytest"],
            "go" => &["go test ./..."],
            "jvm" => &["project test task"],
            "dotnet" => &["dotnet test"],
            "ruby" => &["bundle exec rake test"],
            "php" => &["composer test"],
            "cpp" => &["project build and test"],
            _ => &[],
        };
        for check in checks {
            if !recommended_checks.contains(check) {
                recommended_checks.push(*check);
            }
        }
    }
    recommended_checks.truncate(5);
    let mut warnings = Vec::new();
    if overview.is_none() {
        warnings.push("project_overview_unavailable");
    }
    if git_dirty.is_none() {
        warnings.push("git_status_unavailable");
    }
    json!({
        "git": {
            "baseline_commit": task.baseline_commit.as_deref().map(short_oid),
            "baseline_tree": task.baseline_tree.as_deref().map(short_oid),
            "dirty": git_dirty,
            "conflict_count": git_conflict_count
        },
        "workspace": {
            "isolated": task.isolated,
            "strategy": if task.isolated { "reusable_slot" } else { "target_checkout" }
        },
        "languages": languages,
        "manifests": manifests,
        "instructions": instructions,
        "recommended_checks": recommended_checks,
        "warnings": warnings
    })
}

fn validation_projection(execution: Option<&crate::db::ConnectorExecution>) -> Value {
    let Some(execution) = execution else {
        return json!({ "status": "not_run", "execution_id": null, "checks": [] });
    };
    let projection =
        execution::execution_projection(execution, chrono::Utc::now().timestamp(), None);
    json!({
        "status": projection["assertion_status"],
        "execution_id": execution.execution_id,
        "checks": projection["checks"],
        "recipe": projection["recipe"],
        "assertion_evidence": projection["assertion_evidence"]
    })
}

fn checks_stale_outcome(
    task: &ConnectorTaskSnapshot,
    execution: &crate::db::ConnectorExecution,
    message: &str,
) -> ConnectorCallOutcome {
    ConnectorCallOutcome::error_for_task(
        409,
        "checks_stale",
        message,
        false,
        true,
        Some(
            "Call checks_run with a new operation_id to validate the current workspace, then retry task_finish.",
        ),
        task,
        json!({ "execution_id": execution.execution_id }),
    )
}

fn short_oid(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn required_scope(capability: &str) -> &'static str {
    match capability {
        "task_start" => SCOPE_RUNTIME_READ,
        "files_read" | "files_search" | "task_review" => SCOPE_PROJECT_READ,
        "edits_apply" | "task_finish" => SCOPE_PROJECT_WRITE,
        "checks_run" | "commands_run" | "task_cancel" => SCOPE_JOB_RUN,
        _ => SCOPE_RUNTIME_READ,
    }
}

fn stable_subject_id(auth: &AuthContext) -> Result<String, String> {
    if let Some(user_id) = auth.user_id.as_deref() {
        return Ok(format!("user:{user_id}"));
    }
    if let Some(hash) = auth.shared_key_hash.as_deref() {
        return Ok(format!("shared:{hash}"));
    }
    if let Some(grant_id) = auth.project_grant_id.as_deref() {
        return Ok(format!("project:{grant_id}"));
    }
    match auth.kind {
        AuthKind::Bootstrap => Ok("bootstrap".to_string()),
        AuthKind::OpenAnonymous => Ok("open:anonymous".to_string()),
        AuthKind::ApiToken
        | AuthKind::OAuth2Token
        | AuthKind::SharedKey
        | AuthKind::ProjectCredential
        | AuthKind::AgentToken
        | AuthKind::AccountCredential => {
            Err("authenticated identity has no stable connector subject".to_string())
        }
    }
}

fn validate_task_id(task_id: &str) -> Result<(), &'static str> {
    let suffix = task_id.strip_prefix("wc_task_").unwrap_or_default();
    if suffix.len() != 32
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("task_id must be the opaque wc_task_* id returned by task_start");
    }
    Ok(())
}

fn validate_operation_id(operation_id: &str) -> Result<(), &'static str> {
    let mut bytes = operation_id.bytes();
    if operation_id.len() > 100
        || !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err("operation_id must be 1..=100 ASCII letters, digits, '-', '_', '.', or ':'");
    }
    Ok(())
}

fn validate_path(path: &str) -> Result<(), &'static str> {
    if path.trim().is_empty() || path.len() > 1024 {
        return Err("path must be 1..=1024 bytes");
    }
    if path.starts_with('/') || path.contains('\0') {
        return Err("path must be project-relative and contain no NUL byte");
    }
    Ok(())
}

fn validate_opaque_id(value: &str, prefix: &str, label: &str) -> Result<(), String> {
    let suffix = value.strip_prefix(prefix).unwrap_or_default();
    if suffix.len() < 10
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(format!("{label} must use the {prefix}<lowercase-id> form"));
    }
    Ok(())
}

fn required_env(name: &str) -> Result<String, String> {
    nonempty_env(name)
        .ok_or_else(|| format!("{name} is required when connector surface is enabled"))
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn sanitize_value(
    value: &mut Value,
    executor_project: &str,
    logical_project: &str,
    executor_root: &str,
) {
    match value {
        Value::String(string) => {
            if string.contains(executor_project) {
                *string = string.replace(executor_project, logical_project);
            }
            if string.contains(executor_root) {
                *string = string.replace(executor_root, ".");
            }
        }
        Value::Array(items) => {
            for item in items {
                sanitize_value(item, executor_project, logical_project, executor_root);
            }
        }
        Value::Object(object) => {
            for transport_field in [
                "client_id",
                "agent_instance_id",
                "executor",
                "executor_id",
                "request_id",
                "runtime_project_id",
            ] {
                object.remove(transport_field);
            }
            for item in object.values_mut() {
                sanitize_value(item, executor_project, logical_project, executor_root);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskStartInput {
    goal: String,
    #[serde(default)]
    mode: ConnectorTaskMode,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConnectorTaskMode {
    #[default]
    Normal,
    ReadOnly,
}

impl ConnectorTaskMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ReadOnly => "read_only",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesReadInput {
    task_id: String,
    files: Vec<FileReadInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileReadInput {
    path: String,
    #[serde(default)]
    start_line: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    with_line_numbers: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilesSearchInput {
    task_id: String,
    pattern: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    context_before: Option<usize>,
    #[serde(default)]
    context_after: Option<usize>,
    #[serde(default)]
    include_globs: Vec<String>,
    #[serde(default)]
    exclude_globs: Vec<String>,
    #[serde(default)]
    result_mode: Option<SearchResultMode>,
    #[serde(default)]
    cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditsApplyInput {
    task_id: String,
    operation_id: String,
    changes: Vec<ApplyFileChangeInput>,
    #[serde(default)]
    dry_run: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChecksRunInput {
    task_id: String,
    operation_id: String,
    checks: Vec<SemanticCheck>,
    #[serde(default)]
    recipe: Option<RecipeId>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    test_filter: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandsRunInput {
    task_id: String,
    operation_id: String,
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskReviewInput {
    task_id: String,
    #[serde(default)]
    include_diff: Option<bool>,
    #[serde(default)]
    after_cursor: Option<i64>,
    #[serde(default)]
    wait_ms: Option<u64>,
    #[serde(default)]
    max_events: Option<usize>,
    #[serde(default)]
    include_output_tail: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskCancelInput {
    task_id: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskFinishInput {
    task_id: String,
    summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_client::ShellClientRegistry;
    use crate::shell_protocol::{
        ShellAgentProjectSummary, ShellClientCapabilities, ShellClientRegisterRequest,
    };

    pub(super) const PROJECT_GRANT_ID: &str = "wc_pgrant_1111111111111111";
    pub(super) const PROJECT_SUBJECT_ID: &str = "project:wc_pgrant_1111111111111111";
    const PROJECT_CREDENTIAL: &str =
        "webcodex_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    pub(super) fn credential() -> ProjectCredentialVerifier {
        ProjectCredentialVerifier::new(PROJECT_GRANT_ID.to_string(), PROJECT_CREDENTIAL).unwrap()
    }

    async fn register_agent(registry: &ShellClientRegistry, project_id: &str, path: &str) {
        registry
            .register_with_auth(
                ShellClientRegisterRequest {
                    client_id: "hosted".to_string(),
                    agent_instance_id: "instance".to_string(),
                    display_name: None,
                    owner: Some("owner".to_string()),
                    hostname: None,
                    capabilities: Some(ShellClientCapabilities {
                        shell: true,
                        file_read: true,
                        file_write: true,
                        git: true,
                        jobs: true,
                        async_jobs: true,
                        async_shell_jobs: true,
                        structured_validation_argv: true,
                        lsp_read_only_navigation: false,
                    }),
                    projects: Some(vec![ShellAgentProjectSummary {
                        id: project_id.to_string(),
                        name: Some(project_id.to_string()),
                        path: path.to_string(),
                        allow_patch: true,
                        kind: Some("auto".to_string()),
                        description: None,
                        hooks: Vec::new(),
                        disabled: false,
                        git_branch: Some("main".to_string()),
                        git_head: None,
                        git_dirty: Some(false),
                        updated_at: 1,
                        shell_profile: None,
                    }]),
                    agent_protocol_version: Some("test".to_string()),
                    policy: None,
                },
                Some(&auth("u1")),
            )
            .await
            .unwrap();
    }

    pub(super) fn init_repo(project: &Path) {
        std::fs::create_dir(project).unwrap();
        let run = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(project)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {:?} failed: {}",
                args,
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&["init", "-q"]);
        std::fs::write(project.join("README.md"), "fixture\n").unwrap();
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname = \"connector-fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        run(&["add", "README.md", "Cargo.toml"]);
        run(&[
            "-c",
            "user.name=WebCodex Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ]);
    }

    pub(super) fn auth(user_id: &str) -> AuthContext {
        let project_grant_id = if user_id == "u1" {
            PROJECT_GRANT_ID.to_string()
        } else {
            "wc_pgrant_2222222222222222".to_string()
        };
        AuthContext {
            role: Some("project".to_string()),
            scopes: vec![
                SCOPE_RUNTIME_READ.to_string(),
                SCOPE_PROJECT_READ.to_string(),
                SCOPE_PROJECT_WRITE.to_string(),
                SCOPE_JOB_RUN.to_string(),
            ],
            token_kind: Some("project".to_string()),
            project_grant_id: Some(project_grant_id),
            ..AuthContext::new(AuthKind::ProjectCredential)
        }
    }

    pub(super) fn connector() -> (tempfile::TempDir, ConnectorRuntime) {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        init_repo(&project);
        let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
        let runtime = Arc::new(ToolRuntime::new_for_tests());
        let connector = ConnectorRuntime::new(
            runtime,
            db,
            ConnectorContext {
                project_id: "wc_proj_1234567890".to_string(),
                project_name: "demo".to_string(),
                workspace_id: "wc_ws_1234567890".to_string(),
                executor_project: "agent:hosted:demo".to_string(),
                executor_root: project.to_string_lossy().to_string(),
                runs_root: temp.path().join("runs").to_string_lossy().to_string(),
                results_root: temp.path().join("results").to_string_lossy().to_string(),
                projects_dir: temp
                    .path()
                    .join("agent/projects.d")
                    .to_string_lossy()
                    .to_string(),
                profile: "personal".to_string(),
                project_grant_id: PROJECT_GRANT_ID.to_string(),
            },
            credential(),
        )
        .unwrap();
        (temp, connector)
    }

    #[tokio::test]
    async fn writable_start_registers_and_releases_a_reusable_git_worktree() {
        use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        init_repo(&project);
        let registry = Arc::new(ShellClientRegistry::default());
        register_agent(&registry, "project", &project.to_string_lossy()).await;
        let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
        let connector = ConnectorRuntime::new(
            Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
                registry.clone(),
            )),
            db,
            ConnectorContext {
                project_id: "wc_proj_1234567890".to_string(),
                project_name: "project".to_string(),
                workspace_id: "wc_ws_1234567890".to_string(),
                executor_project: "agent:hosted:project".to_string(),
                executor_root: project.to_string_lossy().to_string(),
                runs_root: temp.path().join("state/runs").to_string_lossy().to_string(),
                results_root: temp
                    .path()
                    .join("state/results")
                    .to_string_lossy()
                    .to_string(),
                projects_dir: temp
                    .path()
                    .join("state/agent/projects.d")
                    .to_string_lossy()
                    .to_string(),
                profile: "personal".to_string(),
                project_grant_id: PROJECT_GRANT_ID.to_string(),
            },
            credential(),
        )
        .unwrap();
        let agent_registry = registry.clone();
        let responder = tokio::spawn(async move {
            for _ in 0..1_000 {
                if let Some(request) = agent_registry
                    .poll(ShellAgentPollRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
                        projects: None,
                    })
                    .await
                    .unwrap()
                {
                    assert_eq!(request.kind, "register_project");
                    let payload: Value =
                        serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
                    assert_eq!(payload["id"], "wc-slot-write-01");
                    assert!(Path::new(payload["path"].as_str().unwrap()).is_dir());
                    let stdout = json!({
                        "agent_project_id": payload["id"],
                        "client_id": "hosted",
                        "name": payload["name"],
                        "path": payload["path"],
                        "allow_patch": true
                    })
                    .to_string();
                    agent_registry
                        .complete(ShellAgentResultRequest {
                            client_id: "hosted".to_string(),
                            agent_instance_id: "instance".to_string(),
                            request_id: request.request_id,
                            exit_code: Some(0),
                            stdout: Some(stdout),
                            stderr: Some(String::new()),
                            duration_ms: Some(1),
                            error: None,
                        })
                        .await
                        .unwrap();
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            panic!("connector did not register its isolated execution project");
        });
        let owner = auth("u1");
        let outcome = connector
            .call(
                "task_start",
                json!({ "goal": "make an isolated change", "mode": "normal" }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        responder.await.unwrap();
        assert!(outcome.ok, "{}", outcome.body);
        assert_eq!(outcome.body["data"]["brief"]["workspace"]["isolated"], true);
        assert_eq!(outcome.body["data"]["brief"]["languages"], json!(["rust"]));
        assert_eq!(outcome.body["data"]["brief"]["git"]["dirty"], false);
        let task_id = outcome.body["task_id"].as_str().unwrap();
        let task = connector
            .db
            .connector_task(task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
            .unwrap();
        assert_ne!(task.execution_root, task.target_root);
        assert!(Path::new(&task.execution_root).is_dir());
        assert!(task.baseline_commit.is_some());
        std::fs::write(
            Path::new(&task.execution_root).join("README.md"),
            "isolated result\n",
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join("README.md")).unwrap(),
            "fixture\n"
        );
        let check_registry = registry.clone();
        let check_responder = tokio::spawn(async move {
            for _ in 0..1_000 {
                if let Some(request) = check_registry
                    .poll(ShellAgentPollRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
                        projects: None,
                    })
                    .await
                    .unwrap()
                {
                    assert_eq!(request.kind, "start_validation_job");
                    check_registry
                        .update_job(crate::shell_protocol::ShellAgentJobUpdateRequest {
                            client_id: "hosted".to_string(),
                            agent_instance_id: "instance".to_string(),
                            job_id: request.job_id.unwrap(),
                            request_id: Some(request.request_id),
                            status: "completed".to_string(),
                            stdout_chunk: None,
                            stderr_chunk: None,
                            stdout_tail: None,
                            stderr_tail: None,
                            exit_code: Some(0),
                            duration_ms: Some(1),
                            error: None,
                            validation_progress: Some(
                                crate::shell_protocol::ShellJobValidationProgress {
                                    completed: 1,
                                    current_step: None,
                                    failed_step: None,
                                },
                            ),
                            finished: true,
                        })
                        .await
                        .unwrap();
                    return;
                }
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
            panic!("connector did not dispatch structured validation");
        });
        let checked = connector
            .call(
                "checks_run",
                json!({
                    "task_id": task_id,
                    "operation_id": "worktree-check-1",
                    "checks": ["check"]
                }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        check_responder.await.unwrap();
        assert!(checked.ok, "{}", checked.body);
        let finished = connector
            .call(
                "task_finish",
                json!({ "task_id": task_id, "summary": "updated the readme" }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        assert!(finished.ok, "{}", finished.body);
        assert_eq!(finished.body["data"]["status"], "ready_for_review");
        assert_eq!(finished.body["data"]["workspace"]["released"], true);
        assert_eq!(
            finished.body["data"]["result"]["changed_paths"],
            json!(["README.md"])
        );
        assert_eq!(
            std::fs::read_to_string(project.join("README.md")).unwrap(),
            "fixture\n"
        );
        let review = connector
            .call(
                "task_review",
                json!({ "task_id": task_id, "include_diff": true }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        assert!(review.ok, "{}", review.body);
        assert_eq!(
            review.body["data"]["changes"]["source"],
            "stable_task_result"
        );
        assert!(review.body["data"]["changes"]["diff_preview"]["text"]
            .as_str()
            .unwrap()
            .contains("isolated result"));
        let decided_task = connector
            .db
            .connector_task(task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
            .unwrap();
        let result = connector
            .db
            .connector_task_result(task_id, &connector.context.project_id, PROJECT_SUBJECT_ID)
            .unwrap()
            .unwrap();
        let cleanup = workspace::WorkspaceManager::reject(&decided_task, &result).unwrap();
        connector
            .db
            .decide_connector_result(
                task_id,
                &connector.context.project_id,
                "rejected",
                "test",
                cleanup.cleanup_warning.as_deref(),
                chrono::Utc::now().timestamp(),
            )
            .unwrap();
        assert!(Path::new(&task.execution_root).exists());
        let resources = workspace::WorkspaceManager::resource_status(
            Path::new(&connector.context.runs_root),
            temp.path().join("cargo-target").as_path(),
        );
        assert_eq!(resources.slot_state, "idle");
    }

    #[tokio::test]
    async fn canonical_read_reaches_bound_executor_and_advances_event_cursor() {
        use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};

        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
        let registry = Arc::new(ShellClientRegistry::default());
        register_agent(&registry, "demo", "/workspace/demo").await;
        let tool_runtime = Arc::new(ToolRuntime::new_for_tests_with_shell_clients(
            registry.clone(),
        ));
        let connector = ConnectorRuntime::new(
            tool_runtime,
            db,
            ConnectorContext {
                project_id: "wc_proj_1234567890".to_string(),
                project_name: "demo".to_string(),
                workspace_id: "wc_ws_1234567890".to_string(),
                executor_project: "agent:hosted:demo".to_string(),
                executor_root: "/workspace/demo".to_string(),
                runs_root: temp.path().join("runs").to_string_lossy().to_string(),
                results_root: temp.path().join("results").to_string_lossy().to_string(),
                projects_dir: temp
                    .path()
                    .join("agent/projects.d")
                    .to_string_lossy()
                    .to_string(),
                profile: "personal".to_string(),
                project_grant_id: PROJECT_GRANT_ID.to_string(),
            },
            credential(),
        )
        .unwrap();
        let owner = auth("u1");
        let started = connector
            .call(
                "task_start",
                json!({ "goal": "read the entry point", "mode": "read_only" }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        let task_id = started.body["task_id"].as_str().unwrap().to_string();

        let agent_registry = registry.clone();
        let responder = tokio::spawn(async move {
            for _ in 0..100 {
                if let Some(request) = agent_registry
                    .poll(ShellAgentPollRequest {
                        client_id: "hosted".to_string(),
                        agent_instance_id: "instance".to_string(),
                        projects: None,
                    })
                    .await
                    .unwrap()
                {
                    assert_eq!(request.kind, "file_read");
                    assert_eq!(request.path.as_deref(), Some("src/lib.rs"));
                    agent_registry
                        .complete(ShellAgentResultRequest {
                            client_id: "hosted".to_string(),
                            agent_instance_id: "instance".to_string(),
                            request_id: request.request_id,
                            exit_code: Some(0),
                            stdout: Some(
                                json!({
                                    "format": "webcodex.file_read_range.v1",
                                    "path": "src/lib.rs",
                                    "content": "fn entry() {}\n",
                                    "start_line": 1,
                                    "total_lines": 1,
                                    "truncated": false
                                })
                                .to_string(),
                            ),
                            stderr: Some(String::new()),
                            duration_ms: Some(1),
                            error: None,
                        })
                        .await
                        .unwrap();
                    return;
                }
                tokio::task::yield_now().await;
            }
            panic!("connector did not dispatch the read to its bound executor");
        });
        let outcome = connector
            .call(
                "files_read",
                json!({
                    "task_id": task_id,
                    "files": [{ "path": "src/lib.rs", "limit": 50 }]
                }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        responder.await.unwrap();
        assert!(outcome.ok, "{}", outcome.body);
        assert_eq!(outcome.body["event_cursor"], 2);
        assert!(outcome.body["data"]["files"][0]["content"]
            .as_str()
            .unwrap()
            .contains("fn entry"));
        assert!(!serde_json::to_string(&outcome.body)
            .unwrap()
            .contains("agent:hosted:demo"));
    }

    #[test]
    fn search_cursor_is_query_bound_and_pages_a_sorted_window() {
        let mut input = FilesSearchInput {
            task_id: "wc_task_0123456789abcdef0123456789abcdef".to_string(),
            pattern: "needle".to_string(),
            path: Some("src".to_string()),
            limit: Some(2),
            context_before: Some(0),
            context_after: Some(0),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            result_mode: Some(SearchResultMode::Matches),
            cursor: None,
        };
        let signature = search_cursor_signature(&input, 2);
        let first = paginate_search_output(
            json!({
                "matches": [
                    {"path": "src/a.rs", "line": 1},
                    {"path": "src/b.rs", "line": 2}
                ],
                "truncated": true
            }),
            SearchResultMode::Matches,
            0,
            2,
            &signature,
        );
        let cursor = first["page"]["next_cursor"].as_str().unwrap();
        assert_eq!(parse_search_cursor(cursor, &signature), Ok(2));
        assert_eq!(first["page"]["returned"], 2);
        let second = paginate_search_output(
            json!({
                "matches": [
                    {"path": "src/a.rs", "line": 1},
                    {"path": "src/b.rs", "line": 2},
                    {"path": "src/c.rs", "line": 3},
                    {"path": "src/d.rs", "line": 4}
                ],
                "truncated": false
            }),
            SearchResultMode::Matches,
            2,
            2,
            &signature,
        );
        assert_eq!(second["matches"][0]["path"], "src/c.rs");
        assert_eq!(second["matches"][1]["path"], "src/d.rs");
        assert!(second["page"]["next_cursor"].is_null());

        input.pattern = "different".to_string();
        let other_signature = search_cursor_signature(&input, 2);
        assert_eq!(parse_search_cursor(cursor, &other_signature), Err(()));
    }
}
