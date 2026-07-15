//! Project-bound application layer for hosted chat connectors.
//!
//! The connector exposes a deliberately small coding workflow while reusing
//! ToolRuntime for execution policy, agent ownership, and permission gates.
//! It owns project/task context, so model-visible calls never carry the legacy
//! executor project id or workflow-session state.

pub(crate) mod http;
pub(crate) mod surface;
pub(crate) mod workspace;

use crate::auth::{
    AuthContext, AuthKind, SCOPE_JOB_RUN, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE,
    SCOPE_RUNTIME_READ,
};
use crate::db::{
    ConnectorApproval, ConnectorApprovalGate, ConnectorBinding, ConnectorTaskEvent,
    ConnectorTaskResult, ConnectorTaskSnapshot, ConnectorTaskStoreError, NewConnectorResult,
    NewConnectorTask,
};
use crate::tool_runtime::kernel::{
    ToolCallContext, ToolCallErrorStatus, ToolCallRequest as KernelToolCallRequest, ToolTransport,
};
use crate::tool_runtime::{ApplyTextEditInput, SearchResultMode, ToolResult, ToolRuntime};
use crate::Database;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
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
    task_locks: StdMutex<HashMap<String, Weak<tokio::sync::Mutex<()>>>>,
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
    ) -> Result<Self, String> {
        context.validate()?;
        let workspace = workspace::WorkspaceManager::new(&context)?;
        let interrupted = db
            .interrupt_connector_runs(&context.project_id, chrono::Utc::now().timestamp())
            .map_err(|error| format!("failed to recover connector runs: {error}"))?;
        if interrupted > 0 {
            tracing::warn!(
                project_id = %context.project_id,
                interrupted,
                "Recovered unfinished connector runs as interrupted"
            );
        }
        Ok(Self {
            tools,
            db,
            context,
            workspace,
            task_locks: StdMutex::new(HashMap::new()),
        })
    }

    pub(crate) fn from_env(
        tools: Arc<ToolRuntime>,
        db: Arc<Database>,
    ) -> Result<ConnectorRuntimeSlot, String> {
        let Some(context) = ConnectorContext::from_env()? else {
            return Ok(ConnectorRuntimeSlot::default());
        };
        Ok(ConnectorRuntimeSlot(Some(Arc::new(Self::new(
            tools, db, context,
        )?))))
    }

    pub(crate) fn context(&self) -> &ConnectorContext {
        &self.context
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

        // Requests for one task are serialized across devices so finish cannot
        // race an in-flight edit or command. Different tasks/users keep
        // independent locks and may execute concurrently.
        let task_lock = arguments
            .get("task_id")
            .and_then(Value::as_str)
            .map(|task_id| self.task_lock(task_id));
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
        let manager = self.workspace.clone();
        let context = self.context.clone();
        let run_for_prepare = run_id.clone();
        let prepared = match tokio::task::spawn_blocking(move || {
            manager.prepare(&context, &run_for_prepare, read_only)
        })
        .await
        {
            Ok(Ok(prepared)) => prepared,
            Ok(Err(message)) => {
                return ConnectorCallOutcome::error(
                    409,
                    "workspace_preparation_failed",
                    self.sanitize_executor_string(&message),
                    false,
                    true,
                    Some("Resolve the Git/workspace issue, then start a new task."),
                    None,
                    false,
                )
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
        match self.db.start_connector_task(NewConnectorTask {
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
            Ok(task) => ConnectorCallOutcome::success(
                &task,
                json!({
                    "project": {
                        "id": self.context.project_id,
                        "name": self.context.project_name
                    },
                    "goal": goal,
                    "mode": mode,
                    "status": task.task_status,
                    "workspace": {
                        "isolated": task.isolated,
                        "baseline_commit": task.baseline_commit.as_deref().map(short_oid)
                    },
                    "next": "Inspect only the files needed for this goal; then edit, validate, review, and finish."
                }),
            ),
            Err(error) => {
                if let Some(cleanup) = self
                    .workspace
                    .discard_prepared(&self.context.executor_root, &prepared)
                {
                    tracing::warn!(cleanup = %cleanup, "failed to fully clean unpersisted workspace");
                }
                store_error_outcome(error, None)
            }
        }
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
                Ok(output) => results.push(output),
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
        let args = json!({
            "project": task.execution_executor_ref,
            "pattern": input.pattern,
            "path": input.path,
            "limit": input.limit.unwrap_or(50),
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
                let cursor =
                    match self.record_event(&task, "files_search", json!({ "ok": true }), now) {
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
        if let Err(message) = validate_path(&input.path) {
            return invalid_input("edits_apply", message);
        }
        if input.edits.is_empty() || input.edits.len() > 32 {
            return invalid_input("edits_apply", "edits must contain 1..=32 entries");
        }
        let edit_bytes = serde_json::to_vec(&input.edits)
            .map(|bytes| bytes.len())
            .unwrap_or(usize::MAX);
        if edit_bytes > 512 * 1024 {
            return invalid_input("edits_apply", "serialized edits exceed 512 KiB");
        }
        if input.expected_file_sha256.as_deref().is_some_and(|hash| {
            hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        }) {
            return invalid_input(
                "edits_apply",
                "expected_file_sha256 must be 64 lowercase hexadecimal characters",
            );
        }
        let task = match self.active_writable_task(&input.task_id, subject_id, "edits_apply", now) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let args = json!({
            "project": task.execution_executor_ref,
            "path": input.path,
            "edits": input.edits,
            "dry_run": input.dry_run.unwrap_or(false),
            "expected_file_sha256": input.expected_file_sha256
        });
        match self
            .invoke_kernel("apply_text_edits", args, &task, auth, transport)
            .await
        {
            Ok(output) => {
                let cursor = match self.record_event(
                    &task,
                    "edits_apply",
                    json!({ "ok": true, "dry_run": input.dry_run.unwrap_or(false) }),
                    now,
                ) {
                    Ok(cursor) => cursor,
                    Err(outcome) => return outcome,
                };
                ConnectorCallOutcome::success_at(&task, cursor, output)
            }
            Err(error) => {
                let cursor = self.record_event(
                    &task,
                    "edits_apply",
                    json!({ "ok": false, "dry_run": input.dry_run.unwrap_or(false) }),
                    now,
                );
                self.kernel_error_outcome(error, &task, cursor, Value::Null)
            }
        }
    }

    async fn checks_run(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: ChecksRunInput = match parse_input("checks_run", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
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
        let task = match self.active_writable_task(&input.task_id, subject_id, "checks_run", now) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let mut results = Vec::new();
        for check in input.checks.iter().copied() {
            let (tool_name, args) = match check {
                StandardCheck::Format => (
                    "cargo_fmt",
                    json!({
                        "project": task.execution_executor_ref,
                        "cwd": input.cwd,
                        "check": true,
                        "timeout_secs": input.timeout_secs.unwrap_or(120)
                    }),
                ),
                StandardCheck::Check => (
                    "cargo_check",
                    json!({
                        "project": task.execution_executor_ref,
                        "cwd": input.cwd,
                        "all_targets": true,
                        "timeout_secs": input.timeout_secs.unwrap_or(120)
                    }),
                ),
                StandardCheck::Test => (
                    "cargo_test",
                    json!({
                        "project": task.execution_executor_ref,
                        "cwd": input.cwd,
                        "filter": input.test_filter,
                        "timeout_secs": input.timeout_secs.unwrap_or(120)
                    }),
                ),
            };
            match self
                .invoke_kernel(tool_name, args, &task, auth, transport)
                .await
            {
                Ok(output) => results.push(json!({ "check": check, "output": output })),
                Err(error) => {
                    let cursor = self.record_event(
                        &task,
                        "checks_run",
                        json!({ "ok": false, "completed": results.len(), "failed_check": check }),
                        now,
                    );
                    return self.kernel_error_outcome(
                        error,
                        &task,
                        cursor,
                        json!({ "checks": results }),
                    );
                }
            }
        }
        let cursor = match self.record_event(
            &task,
            "checks_run",
            json!({ "ok": true, "checks": input.checks }),
            now,
        ) {
            Ok(cursor) => cursor,
            Err(outcome) => return outcome,
        };
        ConnectorCallOutcome::success_at(&task, cursor, json!({ "checks": results }))
    }

    async fn commands_run(
        &self,
        arguments: Value,
        subject_id: &str,
        auth: &AuthContext,
        transport: ConnectorTransport,
        now: i64,
    ) -> ConnectorCallOutcome {
        let input: CommandsRunInput = match parse_input("commands_run", arguments) {
            Ok(input) => input,
            Err(outcome) => return outcome,
        };
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
        let task = match self.active_writable_task(&input.task_id, subject_id, "commands_run", now)
        {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let timeout_secs = input.timeout_secs.unwrap_or(120);
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
                    Some("Resolve the Git workspace issue before requesting command approval."),
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
        let action_hash = command_action_hash(
            &task,
            &input.command,
            input.cwd.as_deref(),
            timeout_secs,
            &precondition,
        );
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
        let args = json!({
            "project": task.execution_executor_ref,
            "command": input.command,
            "cwd": input.cwd,
            "timeout_secs": timeout_secs as i64
        });
        match self
            .invoke_kernel("run_shell", args, &task, auth, transport)
            .await
        {
            Ok(output) => {
                let cursor =
                    match self.record_event(&task, "commands_run", json!({ "ok": true }), now) {
                        Ok(cursor) => cursor,
                        Err(outcome) => return outcome,
                    };
                ConnectorCallOutcome::success_at(&task, cursor, output)
            }
            Err(error) => {
                let cursor = self.record_event(&task, "commands_run", json!({ "ok": false }), now);
                self.kernel_error_outcome(error, &task, cursor, Value::Null)
            }
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
        let task = match self.task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
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
            &self.context.project_id,
            subject_id,
            MAX_EVENT_COUNT,
        ) {
            Ok(events) => events,
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
        ConnectorCallOutcome::success_at(
            &task,
            task.event_cursor,
            json!({
                "goal": task.goal,
                "mode": task.mode,
                "status": task.task_status,
                "run_status": task.run_status,
                "changes": changes,
                "result": result.as_ref().map(result_projection),
                "timeline": events
            }),
        )
    }

    async fn task_finish(
        &self,
        arguments: Value,
        subject_id: &str,
        _auth: &AuthContext,
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
        let task = match self.active_task(&input.task_id, subject_id) {
            Ok(task) => task,
            Err(outcome) => return outcome,
        };
        let events = match self.db.connector_task_events(
            &task.task_id,
            &self.context.project_id,
            subject_id,
            MAX_EVENT_COUNT,
        ) {
            Ok(events) => events,
            Err(error) => return store_error_outcome(error, Some(&task)),
        };
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
        let validation = validation_projection(&events);
        let result_id = format!("wc_result_{}", uuid::Uuid::new_v4().simple());
        let cursor = match self.db.finish_connector_task(
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
                    "decision_status": "pending"
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
        Self {
            ok: true,
            body: json!({
                "ok": true,
                "task_id": task.task_id,
                "run_id": task.run_id,
                "event_cursor": cursor,
                "data": data,
                "warnings": [],
                "blocking": false
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

fn command_action_hash(
    task: &ConnectorTaskSnapshot,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
    precondition: &str,
) -> String {
    let mut hasher = Sha256::new();
    for field in [
        b"webcodex.commands_run.v1".as_slice(),
        task.task_id.as_bytes(),
        task.run_id.as_bytes(),
        command.as_bytes(),
        cwd.unwrap_or("").as_bytes(),
        &timeout_secs.to_be_bytes(),
        precondition.as_bytes(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    format!("{:x}", hasher.finalize())
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

fn validation_projection(events: &[ConnectorTaskEvent]) -> Value {
    let runs = events
        .iter()
        .filter(|event| event.kind == "checks_run")
        .map(|event| {
            json!({
                "sequence": event.sequence,
                "created_at": event.created_at,
                "outcome": event.payload
            })
        })
        .collect::<Vec<_>>();
    json!({
        "status": if runs.is_empty() { "not_run" } else { "recorded" },
        "runs": runs
    })
}

fn short_oid(value: &str) -> &str {
    value.get(..12).unwrap_or(value)
}

fn required_scope(capability: &str) -> &'static str {
    match capability {
        "task_start" => SCOPE_RUNTIME_READ,
        "files_read" | "files_search" | "task_review" => SCOPE_PROJECT_READ,
        "edits_apply" | "task_finish" => SCOPE_PROJECT_WRITE,
        "checks_run" | "commands_run" => SCOPE_JOB_RUN,
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
    match auth.kind {
        AuthKind::Bootstrap => Ok("bootstrap".to_string()),
        AuthKind::OpenAnonymous => Ok("open:anonymous".to_string()),
        AuthKind::ApiToken
        | AuthKind::OAuth2Token
        | AuthKind::SharedKey
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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditsApplyInput {
    task_id: String,
    path: String,
    edits: Vec<ApplyTextEditInput>,
    #[serde(default)]
    dry_run: Option<bool>,
    #[serde(default)]
    expected_file_sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChecksRunInput {
    task_id: String,
    checks: Vec<StandardCheck>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    test_filter: Option<String>,
    #[serde(default)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum StandardCheck {
    Format,
    Check,
    Test,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandsRunInput {
    task_id: String,
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

    fn init_repo(project: &Path) {
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
        run(&["add", "README.md"]);
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

    fn auth(user_id: &str) -> AuthContext {
        AuthContext {
            kind: AuthKind::ApiToken,
            user_id: Some(user_id.to_string()),
            username: Some("owner".to_string()),
            api_key_id: Some("key".to_string()),
            api_key_name: Some("connector".to_string()),
            role: Some("user".to_string()),
            scopes: vec![
                SCOPE_RUNTIME_READ.to_string(),
                SCOPE_PROJECT_READ.to_string(),
                SCOPE_PROJECT_WRITE.to_string(),
                SCOPE_JOB_RUN.to_string(),
            ],
            is_bootstrap: false,
            token_kind: Some("user".to_string()),
            allowed_client_id: None,
            shared_key_hash: None,
        }
    }

    fn connector() -> (tempfile::TempDir, ConnectorRuntime) {
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
            },
        )
        .unwrap();
        (temp, connector)
    }

    #[tokio::test]
    async fn start_returns_small_project_bound_envelope() {
        let (_temp, connector) = connector();
        let outcome = connector
            .call(
                "task_start",
                json!({ "goal": "understand the parser", "mode": "read_only" }),
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await;
        assert!(outcome.ok);
        assert!(outcome.body["task_id"]
            .as_str()
            .unwrap()
            .starts_with("wc_task_"));
        assert_eq!(outcome.body["data"]["project"]["id"], "wc_proj_1234567890");
        let serialized = serde_json::to_string(&outcome.body).unwrap();
        assert!(!serialized.contains("agent:hosted:demo"));
        assert!(!serialized.contains("session"));
    }

    #[tokio::test]
    async fn hidden_legacy_tool_is_rejected_without_falling_through() {
        let (_temp, connector) = connector();
        let outcome = connector
            .call(
                "runtime_status",
                json!({}),
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await;
        assert!(!outcome.ok);
        assert!(outcome.protocol_error);
        assert_eq!(outcome.body["error"]["code"], "unknown_capability");
    }

    #[tokio::test]
    async fn read_only_task_denies_consequential_capability_before_executor_dispatch() {
        let (_temp, connector) = connector();
        let owner = auth("u1");
        let started = connector
            .call(
                "task_start",
                json!({ "goal": "inspect only", "mode": "read_only" }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        let task_id = started.body["task_id"].as_str().unwrap();
        let outcome = connector
            .call(
                "edits_apply",
                json!({
                    "task_id": task_id,
                    "path": "src/lib.rs",
                    "edits": [{
                        "kind": "replace_exact",
                        "old_text": "old",
                        "new_text": "new"
                    }]
                }),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        assert!(!outcome.ok);
        assert_eq!(outcome.http_status, 403);
        assert_eq!(outcome.body["error"]["code"], "read_only_task");
        assert_eq!(outcome.body["event_cursor"], 2);
    }

    #[tokio::test]
    async fn raw_command_waits_for_local_one_time_approval() {
        let (_temp, connector) = connector();
        let owner = auth("u1");
        let now = chrono::Utc::now().timestamp();
        connector
            .db
            .ensure_connector_binding(ConnectorBinding {
                project_id: &connector.context.project_id,
                project_name: &connector.context.project_name,
                workspace_id: &connector.context.workspace_id,
                executor_ref: &connector.context.executor_project,
                subject_id: "user:u1",
                profile: &connector.context.profile,
                now,
            })
            .unwrap();
        let task_id = "wc_task_0123456789abcdef0123456789abcdef";
        let run_id = "wc_run_0123456789abcdef0123456789abcdef";
        let prepared = connector
            .workspace
            .prepare(&connector.context, run_id, false)
            .unwrap();
        connector
            .db
            .start_connector_task(NewConnectorTask {
                task_id,
                run_id,
                project_id: &connector.context.project_id,
                workspace_id: &connector.context.workspace_id,
                subject_id: "user:u1",
                goal: "run a special generator",
                mode: "normal",
                target_executor_ref: &connector.context.executor_project,
                execution_executor_ref: &prepared.execution_executor_ref,
                target_root: &connector.context.executor_root,
                execution_root: &prepared.execution_root,
                baseline_commit: prepared.baseline_commit.as_deref(),
                baseline_tree: prepared.baseline_tree.as_deref(),
                isolated: true,
                now,
            })
            .unwrap();
        let arguments = json!({
            "task_id": task_id,
            "command": "special-generator --write",
            "timeout_secs": 30
        });
        let waiting = connector
            .call(
                "commands_run",
                arguments.clone(),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        assert_eq!(waiting.body["error"]["code"], "approval_required");
        let approval_id = waiting.body["data"]["approval"]["approval_id"]
            .as_str()
            .unwrap();
        connector
            .db
            .decide_connector_approval(
                task_id,
                &connector.context.project_id,
                approval_id,
                true,
                "local_cli",
                now + 1,
            )
            .unwrap();

        let dispatched = connector
            .call(
                "commands_run",
                arguments.clone(),
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        assert_ne!(dispatched.body["error"]["code"], "approval_required");
        let replay = connector
            .call(
                "commands_run",
                arguments,
                Some(&owner),
                ConnectorTransport::Mcp,
            )
            .await;
        assert_eq!(replay.body["error"]["code"], "approval_consumed");
        assert_eq!(
            connector
                .workspace
                .discard_prepared(&connector.context.executor_root, &prepared),
            None
        );
    }

    #[tokio::test]
    async fn another_user_cannot_observe_or_use_a_task_id() {
        let (_temp, connector) = connector();
        let started = connector
            .call(
                "task_start",
                json!({ "goal": "private work", "mode": "read_only" }),
                Some(&auth("u1")),
                ConnectorTransport::Mcp,
            )
            .await;
        let task_id = started.body["task_id"].as_str().unwrap();
        let outcome = connector
            .call(
                "files_read",
                json!({ "task_id": task_id, "files": [{ "path": "src/lib.rs" }] }),
                Some(&auth("u2")),
                ConnectorTransport::Mcp,
            )
            .await;
        assert!(!outcome.ok);
        assert_eq!(outcome.http_status, 404);
        assert_eq!(outcome.body["error"]["code"], "task_not_found");
        assert!(outcome.body["task_id"].is_null());
    }

    #[tokio::test]
    async fn writable_start_registers_a_run_specific_git_worktree() {
        use crate::shell_client::ShellClientRegistry;
        use crate::shell_protocol::{
            ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
            ShellClientCapabilities, ShellClientRegisterRequest,
        };

        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        init_repo(&project);
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
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
                    lsp_read_only_navigation: false,
                }),
                projects: Some(vec![ShellAgentProjectSummary {
                    id: "project".to_string(),
                    name: Some("project".to_string()),
                    path: project.to_string_lossy().to_string(),
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
            })
            .await
            .unwrap();
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
            },
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
                    assert!(payload["id"].as_str().unwrap().starts_with("wc-run-"));
                    assert!(Path::new(payload["path"].as_str().unwrap()).is_dir());
                    agent_registry
                        .complete(ShellAgentResultRequest {
                            client_id: "hosted".to_string(),
                            agent_instance_id: "instance".to_string(),
                            request_id: request.request_id,
                            exit_code: Some(0),
                            stdout: Some(
                                json!({
                                    "agent_project_id": payload["id"],
                                    "client_id": "hosted",
                                    "name": payload["name"],
                                    "path": payload["path"],
                                    "allow_patch": true
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
        assert_eq!(outcome.body["data"]["workspace"]["isolated"], true);
        let task_id = outcome.body["task_id"].as_str().unwrap();
        let task = connector
            .db
            .connector_task(task_id, &connector.context.project_id, "user:u1")
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
            .connector_task(task_id, &connector.context.project_id, "user:u1")
            .unwrap();
        let result = connector
            .db
            .connector_task_result(task_id, &connector.context.project_id, "user:u1")
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
        assert!(!Path::new(&task.execution_root).exists());
    }

    #[tokio::test]
    async fn canonical_read_reaches_bound_executor_and_advances_event_cursor() {
        use crate::shell_client::ShellClientRegistry;
        use crate::shell_protocol::{
            ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
            ShellClientCapabilities, ShellClientRegisterRequest,
        };

        let temp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&temp.path().join("connector.db")).unwrap());
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
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
                    lsp_read_only_navigation: false,
                }),
                projects: Some(vec![ShellAgentProjectSummary {
                    id: "demo".to_string(),
                    name: Some("demo".to_string()),
                    path: "/workspace/demo".to_string(),
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
            })
            .await
            .unwrap();
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
            },
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
    fn executor_ids_are_recursively_replaced() {
        let mut value = json!({
            "project": "agent:hosted:demo",
            "client_id": "hosted-secret-routing-id",
            "request_id": "transport-request-id",
            "message": "failed in agent:hosted:demo at /workspace/demo/src/lib.rs",
            "nested": ["agent:hosted:demo"]
        });
        sanitize_value(
            &mut value,
            "agent:hosted:demo",
            "wc_proj_demo123456",
            "/workspace/demo",
        );
        let serialized = serde_json::to_string(&value).unwrap();
        assert!(!serialized.contains("agent:hosted:demo"));
        assert!(!serialized.contains("/workspace/demo"));
        assert!(!serialized.contains("hosted-secret-routing-id"));
        assert!(!serialized.contains("transport-request-id"));
        assert!(serialized.contains("wc_proj_demo123456"));
    }

    #[test]
    fn connector_scope_map_matches_capability_risk() {
        assert_eq!(required_scope("task_start"), SCOPE_RUNTIME_READ);
        for capability in ["files_read", "files_search", "task_review"] {
            assert_eq!(required_scope(capability), SCOPE_PROJECT_READ);
        }
        for capability in ["edits_apply", "task_finish"] {
            assert_eq!(required_scope(capability), SCOPE_PROJECT_WRITE);
        }
        for capability in ["checks_run", "commands_run"] {
            assert_eq!(required_scope(capability), SCOPE_JOB_RUN);
        }
    }
}
