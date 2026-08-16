use super::helpers::{
    project_relative_agent_cwd, project_relative_cwd, resolve_agent_cwd, resolve_local_cwd,
};
use super::{ExecutionPurpose, ExecutionShell, ToolResult, ToolRuntime};
use crate::projects::ProjectConfig;
use crate::shell_protocol::{
    PersistentShellRequest, PersistentShellResult, ShellJobContext, RAW_SHELL_COMMAND_MAX_BYTES,
};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;
use webcodex_persistent_shell::{
    PersistentShellManager as LocalProcessManager, ShellIdentity, ShellLaunch, ShellLimits,
    ShellState, ShellSummary,
};

const SERVER_MAX_PERSISTENT_SHELLS: usize = 64;
const SERVER_MAX_TERMINAL_SHELLS: usize = 128;
const LOCAL_IDLE_TIMEOUT_SECS: u64 = 30 * 60;
const LOCAL_MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone)]
struct SessionShellRecord {
    shell_id: String,
    workflow_session_id: String,
    runtime_project_id: String,
    executor: String,
    client_id: Option<String>,
    /// Named SSH resource this shell is bound to, if any. `None` for local and
    /// plain agent shells. Bound at open and never re-derived from Session
    /// context, so a later context change cannot redirect an open shell.
    resource: Option<String>,
    shell: String,
    profile: Option<String>,
    initial_cwd: String,
    initial_cwd_frozen: bool,
    cwd: String,
    created_at: i64,
    last_activity_at: i64,
    state: String,
    busy: Arc<AtomicBool>,
    exit_code: Option<i32>,
    close_reason: Option<String>,
}

impl SessionShellRecord {
    fn is_active(&self) -> bool {
        matches!(self.state.as_str(), "opening" | "running")
    }

    /// Build the safe `ShellJobContext` that routes this shell to its bound
    /// Runner resource. Carries only the named resource + ids; never host,
    /// ControlPath, credentials, or command text.
    fn job_context(&self) -> Option<ShellJobContext> {
        self.resource.as_ref().map(|resource| ShellJobContext {
            runtime_project_id: Some(self.runtime_project_id.clone()),
            workflow_session_id: Some(self.workflow_session_id.clone()),
            ssh_resource: Some(resource.clone()),
            project_cwd: None,
            cwd: None,
            purpose: None,
            shell: None,
            command_preview: String::new(),
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: None,
        })
    }
}

#[derive(Debug, Default)]
struct SessionShellRegistryState {
    records: HashMap<String, SessionShellRecord>,
    terminal_order: VecDeque<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionShellRegistry {
    state: Arc<Mutex<SessionShellRegistryState>>,
    local_processes: LocalProcessManager,
}

impl Default for SessionShellRegistry {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(SessionShellRegistryState::default())),
            local_processes: LocalProcessManager::new(ShellLimits {
                max_shells: SERVER_MAX_PERSISTENT_SHELLS,
                idle_timeout: Duration::from_secs(LOCAL_IDLE_TIMEOUT_SECS),
                max_terminal_records: SERVER_MAX_TERMINAL_SHELLS,
            }),
        }
    }
}

impl SessionShellRegistry {
    async fn reserve_open(
        &self,
        workflow_session_id: &str,
        runtime_project_id: &str,
        executor: &str,
        client_id: Option<String>,
        resource: Option<String>,
        shell: &str,
        initial_cwd: &str,
    ) -> Result<String, String> {
        let mut state = self.state.lock().await;
        if state
            .records
            .values()
            .any(|record| record.workflow_session_id == workflow_session_id && record.is_active())
        {
            return Err(
                "persistent_shell_already_open: Workflow Session already has an active persistent shell"
                    .to_string(),
            );
        }
        let active = state
            .records
            .values()
            .filter(|record| record.is_active())
            .count();
        if active >= SERVER_MAX_PERSISTENT_SHELLS {
            return Err(format!(
                "persistent_shell_limit_reached: Server limit is {SERVER_MAX_PERSISTENT_SHELLS}"
            ));
        }
        let now = chrono::Utc::now().timestamp();
        let shell_id = format!("wc_shell_{}", Uuid::new_v4().simple());
        state.records.insert(
            shell_id.clone(),
            SessionShellRecord {
                shell_id: shell_id.clone(),
                workflow_session_id: workflow_session_id.to_string(),
                runtime_project_id: runtime_project_id.to_string(),
                executor: executor.to_string(),
                client_id,
                resource,
                shell: shell.to_string(),
                profile: None,
                initial_cwd: initial_cwd.to_string(),
                initial_cwd_frozen: false,
                cwd: initial_cwd.to_string(),
                created_at: now,
                last_activity_at: now,
                state: "opening".to_string(),
                busy: Arc::new(AtomicBool::new(false)),
                exit_code: None,
                close_reason: None,
            },
        );
        Ok(shell_id)
    }

    async fn record_for_operation(
        &self,
        workflow_session_id: &str,
        runtime_project_id: &str,
        shell_id: &str,
    ) -> Result<SessionShellRecord, String> {
        let state = self.state.lock().await;
        state
            .records
            .get(shell_id)
            .filter(|record| {
                record.workflow_session_id == workflow_session_id
                    && record.runtime_project_id == runtime_project_id
            })
            .cloned()
            .ok_or_else(|| {
                "persistent_shell_not_found: shell does not belong to this Session and project"
                    .to_string()
            })
    }

    async fn begin_exec(
        &self,
        workflow_session_id: &str,
        runtime_project_id: &str,
        shell_id: &str,
    ) -> Result<SessionShellRecord, String> {
        let mut state = self.state.lock().await;
        let record = state
            .records
            .get_mut(shell_id)
            .filter(|record| {
                record.workflow_session_id == workflow_session_id
                    && record.runtime_project_id == runtime_project_id
            })
            .ok_or_else(|| {
                "persistent_shell_not_found: shell does not belong to this Session and project"
                    .to_string()
            })?;
        if record.state == "opening" {
            return Err("shell_busy: persistent shell is still opening".to_string());
        }
        if !record.is_active() {
            return Err(format!(
                "persistent_shell_stale: persistent shell is {}",
                record.state
            ));
        }
        if record
            .busy
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("shell_busy: persistent shell is executing a command".to_string());
        }
        Ok(record.clone())
    }

    async fn apply_result(&self, result: &PersistentShellResult) {
        let mut state = self.state.lock().await;
        let Some(record) = state.records.get_mut(&result.shell_id) else {
            return;
        };
        if record.workflow_session_id != result.workflow_session_id
            || record.runtime_project_id != result.runtime_project_id
        {
            return;
        }
        let next_state = if result.shell_state == "unknown" && result.error_code.is_some() {
            "lost".to_string()
        } else {
            result.shell_state.clone()
        };
        // Results can arrive out of order across concurrent exec/close
        // requests. A terminal shell id is never reusable, so no late result
        // may change the authoritative terminal state in the Server registry.
        if !record.is_active() {
            record.busy.store(false, Ordering::SeqCst);
            return;
        }
        record.state = next_state;
        record.busy.store(result.busy, Ordering::SeqCst);
        record.exit_code = result.exit_code;
        if let Some(cwd) = &result.cwd {
            record.cwd = cwd.clone();
        }
        if !record.initial_cwd_frozen {
            if let Some(initial_cwd) = result.initial_cwd.as_deref().filter(|cwd| !cwd.is_empty()) {
                record.initial_cwd = initial_cwd.to_string();
                record.initial_cwd_frozen = true;
            }
        }
        if let Some(shell) = &result.shell {
            record.shell = shell.clone();
        }
        if result.profile.is_some() {
            record.profile = result.profile.clone();
        }
        if let Some(created_at) = result.created_at {
            record.created_at = created_at;
        }
        if let Some(last_activity_at) = result.last_activity_at {
            record.last_activity_at = last_activity_at;
        } else {
            record.last_activity_at = chrono::Utc::now().timestamp();
        }
        record.close_reason = result.close_reason.clone();
        if !record.is_active() {
            let shell_id = record.shell_id.clone();
            if !state.terminal_order.iter().any(|id| id == &shell_id) {
                state.terminal_order.push_back(shell_id);
            }
            prune_terminal_records(&mut state);
        } else {
            state
                .terminal_order
                .retain(|shell_id| shell_id != &result.shell_id);
        }
    }

    async fn mark_lost(&self, shell_id: &str, reason: &str) {
        let mut state = self.state.lock().await;
        if let Some(record) = state.records.get_mut(shell_id) {
            record.state = "lost".to_string();
            record.busy.store(false, Ordering::SeqCst);
            record.close_reason = Some(reason.to_string());
            record.last_activity_at = chrono::Utc::now().timestamp();
            if !state.terminal_order.iter().any(|id| id == shell_id) {
                state.terminal_order.push_back(shell_id.to_string());
            }
            prune_terminal_records(&mut state);
        }
    }

    async fn active_for_session(&self, session_id: &str) -> Vec<SessionShellRecord> {
        self.state
            .lock()
            .await
            .records
            .values()
            .filter(|record| record.workflow_session_id == session_id && record.is_active())
            .cloned()
            .collect()
    }
}

struct SessionShellExecGuard {
    busy: Arc<AtomicBool>,
}

impl Drop for SessionShellExecGuard {
    fn drop(&mut self) {
        self.busy.store(false, Ordering::SeqCst);
    }
}

fn prune_terminal_records(state: &mut SessionShellRegistryState) {
    while state.terminal_order.len() > SERVER_MAX_TERMINAL_SHELLS {
        let Some(shell_id) = state.terminal_order.pop_front() else {
            break;
        };
        if state
            .records
            .get(&shell_id)
            .is_some_and(|record| !record.is_active())
        {
            state.records.remove(&shell_id);
        }
    }
}

impl ToolRuntime {
    pub(crate) async fn dispatch_session_shell_tool(
        &self,
        call: super::ToolCall,
        ssh_resource: Option<&str>,
    ) -> ToolResult {
        let session_id = call
            .session_id()
            .expect("persistent shell tools require an explicit Session");
        if self
            .sessions
            .lifecycle_state(session_id)
            .is_some_and(|lifecycle| !lifecycle.allows_mutation())
        {
            return shell_tool_error(
                "persistent_shell_session_inactive",
                "Workflow Session is not active",
                None,
            );
        }
        match call {
            super::ToolCall::OpenSessionShell {
                project,
                session_id,
                cwd,
                shell,
            } => {
                self.open_session_shell(project, session_id, cwd, shell, ssh_resource)
                    .await
            }
            super::ToolCall::SessionShellExec {
                project,
                session_id,
                shell_id,
                command,
                timeout_secs,
                purpose,
            } => {
                self.session_shell_exec(
                    project,
                    session_id,
                    shell_id,
                    command,
                    timeout_secs,
                    purpose,
                )
                .await
            }
            super::ToolCall::SessionShellStatus {
                project,
                session_id,
                shell_id,
            } => {
                self.session_shell_status(project, session_id, shell_id)
                    .await
            }
            super::ToolCall::CloseSessionShell {
                project,
                session_id,
                shell_id,
            } => {
                self.close_session_shell(project, session_id, shell_id)
                    .await
            }
            _ => unreachable!("non-persistent-shell tool routed to persistent-shell dispatcher"),
        }
    }

    async fn open_session_shell(
        &self,
        project: String,
        session_id: String,
        cwd: Option<String>,
        shell: Option<ExecutionShell>,
        ssh_resource: Option<&str>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let runtime_project_id = resolved.resolved_id;
        let project_config = resolved.config;
        self.reconcile_active_shell_before_open(&session_id, &runtime_project_id)
            .await;
        let shell_name = shell.map(|shell| shell.as_str()).unwrap_or("sh");
        // An SSH persistent shell still runs through the agent Runner (the
        // Runner opens the remote shell on its host). It is only valid for an
        // agent-backed project with a named SSH resource.
        if let Some(resource) = ssh_resource {
            if !project_config.is_agent() {
                return shell_tool_error(
                    "ssh_resource_requires_agent_project",
                    "SSH resources require a project owned by a connected Runner",
                    None,
                );
            }
            let client_id = match project_config.agent_client_id() {
                Ok(client_id) => client_id.to_string(),
                Err(error) => return ToolResult::err(error),
            };
            // Remote cwd: explicit open cwd > Session default_cwd (already
            // validated remote-path-shaped) > Runner resource default_cwd /
            // remote login default (filled by the Runner). It is NOT constrained
            // to the local project root.
            let effective_cwd = match resolve_remote_cwd(cwd.as_deref()) {
                Ok(cwd) => cwd,
                Err(error) => return shell_tool_error("persistent_shell_cwd_invalid", error, None),
            };
            let shell_id = match self
                .session_shells
                .reserve_open(
                    &session_id,
                    &runtime_project_id,
                    "ssh",
                    Some(client_id.clone()),
                    Some(resource.to_string()),
                    shell_name,
                    effective_cwd.as_deref().unwrap_or(""),
                )
                .await
            {
                Ok(shell_id) => shell_id,
                Err(error) => return shell_tool_error_from_message(error, None),
            };
            let job_context = ShellJobContext {
                runtime_project_id: Some(runtime_project_id.clone()),
                workflow_session_id: Some(session_id.clone()),
                ssh_resource: Some(resource.to_string()),
                project_cwd: None,
                cwd: effective_cwd.clone(),
                purpose: None,
                shell: None,
                command_preview: String::new(),
                validation_steps: Vec::new(),
                validation: None,
                structured_execution: None,
            };
            let request = PersistentShellRequest {
                action: "open".to_string(),
                shell_id: shell_id.clone(),
                workflow_session_id: session_id.clone(),
                runtime_project_id: runtime_project_id.clone(),
                cwd: effective_cwd,
                shell: shell.map(|shell| shell.as_str().to_string()),
                command: None,
                timeout_secs: None,
                purpose: None,
            };
            let result = match self
                .run_agent_persistent_shell(&client_id, request, Some(job_context), 35)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    if let Ok(record) = self
                        .session_shells
                        .record_for_operation(&session_id, &runtime_project_id, &shell_id)
                        .await
                    {
                        self.close_or_mark_lost(&record, "runner_open_result_lost", &error)
                            .await;
                    }
                    return shell_tool_lost_error_from_message(error, Some(&shell_id));
                }
            };
            self.session_shells.apply_result(&result).await;
            if let Some(error) = self
                .close_opened_shell_if_session_inactive(&session_id, &runtime_project_id, &shell_id)
                .await
            {
                return error;
            }
            return persistent_result_to_tool(
                result,
                &project_config,
                &runtime_project_id,
                &session_id,
                "open",
                "ssh",
                ssh_resource,
                None,
            );
        }
        if project_config.is_agent() {
            let client_id = match project_config.agent_client_id() {
                Ok(client_id) => client_id.to_string(),
                Err(error) => return ToolResult::err(error),
            };
            let effective_cwd = match resolve_agent_cwd(&project_config, cwd.as_deref()) {
                Ok(cwd) => cwd,
                Err(error) => return shell_tool_error("persistent_shell_cwd_invalid", error, None),
            };
            let shell_id = match self
                .session_shells
                .reserve_open(
                    &session_id,
                    &runtime_project_id,
                    "agent",
                    Some(client_id.clone()),
                    None,
                    shell_name,
                    &effective_cwd,
                )
                .await
            {
                Ok(shell_id) => shell_id,
                Err(error) => return shell_tool_error_from_message(error, None),
            };
            let request = PersistentShellRequest {
                action: "open".to_string(),
                shell_id: shell_id.clone(),
                workflow_session_id: session_id.clone(),
                runtime_project_id: runtime_project_id.clone(),
                cwd: Some(effective_cwd),
                shell: shell.map(|shell| shell.as_str().to_string()),
                command: None,
                timeout_secs: None,
                purpose: None,
            };
            let result = match self
                .run_agent_persistent_shell(&client_id, request, None, 35)
                .await
            {
                Ok(result) => result,
                Err(error) => {
                    if let Ok(record) = self
                        .session_shells
                        .record_for_operation(&session_id, &runtime_project_id, &shell_id)
                        .await
                    {
                        self.close_or_mark_lost(&record, "runner_open_result_lost", &error)
                            .await;
                    }
                    return shell_tool_lost_error_from_message(error, Some(&shell_id));
                }
            };
            self.session_shells.apply_result(&result).await;
            if let Some(error) = self
                .close_opened_shell_if_session_inactive(&session_id, &runtime_project_id, &shell_id)
                .await
            {
                return error;
            }
            persistent_result_to_tool(
                result,
                &project_config,
                &runtime_project_id,
                &session_id,
                "open",
                "agent",
                None,
                None,
            )
        } else {
            let cwd = match resolve_local_cwd(&project_config, cwd.as_deref()) {
                Ok(cwd) => cwd,
                Err(error) => return shell_tool_error("persistent_shell_cwd_invalid", error, None),
            };
            let cwd_text = cwd.to_string_lossy().to_string();
            let shell_id = match self
                .session_shells
                .reserve_open(
                    &session_id,
                    &runtime_project_id,
                    "local",
                    None,
                    None,
                    shell_name,
                    &cwd_text,
                )
                .await
            {
                Ok(shell_id) => shell_id,
                Err(error) => return shell_tool_error_from_message(error, None),
            };
            let launch = local_launch(&shell_id, &session_id, &runtime_project_id, shell_name, cwd);
            let local_processes = self.session_shells.local_processes.clone();
            match tokio::task::spawn_blocking(move || local_processes.open(launch)).await {
                Ok(Ok(summary)) => {
                    let result = local_summary_result(summary, "opened", false);
                    self.session_shells.apply_result(&result).await;
                    if let Some(error) = self
                        .close_opened_shell_if_session_inactive(
                            &session_id,
                            &runtime_project_id,
                            &shell_id,
                        )
                        .await
                    {
                        return error;
                    }
                    persistent_result_to_tool(
                        result,
                        &project_config,
                        &runtime_project_id,
                        &session_id,
                        "open",
                        "local",
                        None,
                        None,
                    )
                }
                Ok(Err(error)) => {
                    self.session_shells.mark_lost(&shell_id, error.code).await;
                    shell_tool_error_with_state(
                        error.code,
                        error.message,
                        Some(&shell_id),
                        "lost",
                        "lost",
                    )
                }
                Err(error) => {
                    self.session_shells
                        .mark_lost(&shell_id, "persistent_shell_runtime_error")
                        .await;
                    shell_tool_error_with_state(
                        "persistent_shell_runtime_error",
                        format!("persistent shell open worker failed: {error}"),
                        Some(&shell_id),
                        "lost",
                        "lost",
                    )
                }
            }
        }
    }

    async fn session_shell_exec(
        &self,
        project: String,
        session_id: String,
        shell_id: String,
        command: String,
        timeout_secs: Option<u64>,
        purpose: Option<ExecutionPurpose>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let runtime_project_id = resolved.resolved_id;
        let project_config = resolved.config;
        if command.len() > RAW_SHELL_COMMAND_MAX_BYTES {
            return shell_tool_error(
                "persistent_shell_invalid_command",
                format!("command exceeds the {RAW_SHELL_COMMAND_MAX_BYTES}-byte Server limit"),
                Some(&shell_id),
            );
        }
        let record = match self
            .session_shells
            .begin_exec(&session_id, &runtime_project_id, &shell_id)
            .await
        {
            Ok(record) => record,
            Err(error) => return shell_tool_error_from_message(error, Some(&shell_id)),
        };
        let _exec_guard = SessionShellExecGuard {
            busy: Arc::clone(&record.busy),
        };
        let timeout_secs = timeout_secs.unwrap_or(60);
        if !(1..=3_600).contains(&timeout_secs) {
            return shell_tool_error(
                "persistent_shell_invalid_timeout",
                "timeout_secs must be between 1 and 3600",
                Some(&shell_id),
            );
        }
        if record.executor == "local"
            && !local_cwd_within_project(&project_config, Path::new(&record.cwd))
        {
            if let Ok(result) = self
                .close_record(&record, "persistent_shell_cwd_boundary_changed")
                .await
            {
                self.session_shells.apply_result(&result).await;
            } else {
                self.session_shells
                    .mark_lost(&shell_id, "persistent_shell_cwd_boundary_changed")
                    .await;
            }
            return shell_tool_error(
                "shell_reset_required",
                "persistent shell cwd is no longer inside the current local project boundary",
                Some(&shell_id),
            );
        }
        let result = if record.executor == "agent" || record.executor == "ssh" {
            let client_id = record.client_id.as_deref().unwrap_or_default();
            let request = PersistentShellRequest {
                action: "exec".to_string(),
                shell_id: shell_id.clone(),
                workflow_session_id: session_id.clone(),
                runtime_project_id: runtime_project_id.clone(),
                cwd: None,
                shell: None,
                command: Some(command),
                timeout_secs: Some(timeout_secs),
                purpose: purpose.map(|purpose| purpose.as_str().to_string()),
            };
            self.run_agent_persistent_shell(
                client_id,
                request,
                record.job_context(),
                timeout_secs + 5,
            )
            .await
        } else {
            let local_processes = self.session_shells.local_processes.clone();
            let local_shell_id = shell_id.clone();
            let local_session_id = session_id.clone();
            let local_project_id = runtime_project_id.clone();
            match tokio::task::spawn_blocking(move || {
                local_processes.exec(
                    &local_shell_id,
                    &local_session_id,
                    &local_project_id,
                    &command,
                    Duration::from_secs(timeout_secs),
                )
            })
            .await
            {
                Ok(Ok(shell_result)) => {
                    let escaped = shell_result.shell_state == ShellState::Running
                        && !local_cwd_within_project(&project_config, &shell_result.cwd);
                    let mut result =
                        local_exec_result(&session_id, &runtime_project_id, shell_result);
                    if escaped {
                        let terminal = self
                            .close_record(&record, "persistent_shell_cwd_boundary_changed")
                            .await;
                        if let Ok(closed) = terminal {
                            result.shell_state = closed.shell_state;
                            result.close_reason = closed.close_reason;
                        } else {
                            result.shell_state = "poisoned".to_string();
                        }
                        result.error_code = Some("shell_reset_required".to_string());
                        result.error = Some(
                            "persistent shell moved outside the current local project boundary and was closed"
                                .to_string(),
                        );
                    }
                    Ok(result)
                }
                Ok(Err(error)) => Err(format!("{}: {}", error.code, error.message)),
                Err(error) => Err(format!(
                    "persistent_shell_runtime_error: persistent shell exec worker failed: {error}"
                )),
            }
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                if record.executor == "local" {
                    let local_processes = self.session_shells.local_processes.clone();
                    let local_shell_id = shell_id.clone();
                    let local_session_id = session_id.clone();
                    let local_project_id = runtime_project_id.clone();
                    if let Ok(Ok(summary)) = tokio::task::spawn_blocking(move || {
                        local_processes.status(
                            &local_shell_id,
                            &local_session_id,
                            &local_project_id,
                        )
                    })
                    .await
                    {
                        let mut result = local_summary_result(summary, "rejected", false);
                        let (code, detail) = split_shell_error(&error);
                        result.error_code = Some(code.to_string());
                        result.error = Some(detail.to_string());
                        result
                    } else {
                        self.session_shells.mark_lost(&shell_id, &error).await;
                        return shell_tool_lost_error_from_message(error, Some(&shell_id));
                    }
                } else {
                    self.close_or_mark_lost(&record, "runner_exec_result_lost", &error)
                        .await;
                    return shell_tool_lost_error_from_message(error, Some(&shell_id));
                }
            }
        };
        self.session_shells.apply_result(&result).await;
        persistent_result_to_tool(
            result,
            &project_config,
            &runtime_project_id,
            &session_id,
            "exec",
            record.executor.as_str(),
            record.resource.as_deref(),
            purpose,
        )
    }

    async fn session_shell_status(
        &self,
        project: String,
        session_id: String,
        shell_id: String,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let runtime_project_id = resolved.resolved_id;
        let project_config = resolved.config;
        let record = match self
            .session_shells
            .record_for_operation(&session_id, &runtime_project_id, &shell_id)
            .await
        {
            Ok(record) => record,
            Err(error) => return shell_tool_error_from_message(error, Some(&shell_id)),
        };
        if !record.is_active() {
            return persistent_record_to_tool(record, &project_config, "status");
        }
        let result = if record.executor == "agent" || record.executor == "ssh" {
            self.run_agent_persistent_shell(
                record.client_id.as_deref().unwrap_or_default(),
                PersistentShellRequest {
                    action: "status".to_string(),
                    shell_id: shell_id.clone(),
                    workflow_session_id: session_id.clone(),
                    runtime_project_id: runtime_project_id.clone(),
                    cwd: None,
                    shell: None,
                    command: None,
                    timeout_secs: None,
                    purpose: None,
                },
                record.job_context(),
                5,
            )
            .await
        } else {
            let local_processes = self.session_shells.local_processes.clone();
            let local_shell_id = shell_id.clone();
            let local_session_id = session_id.clone();
            let local_project_id = runtime_project_id.clone();
            match tokio::task::spawn_blocking(move || {
                local_processes.status(&local_shell_id, &local_session_id, &local_project_id)
            })
            .await
            {
                Ok(Ok(summary)) => {
                    let execution_state = if summary.busy { "executing" } else { "idle" };
                    Ok(local_summary_result(summary, execution_state, false))
                }
                Ok(Err(error)) => Err(format!("{}: {}", error.code, error.message)),
                Err(error) => Err(format!(
                    "persistent_shell_runtime_error: persistent shell status worker failed: {error}"
                )),
            }
        };
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                self.close_or_mark_lost(&record, "runner_status_result_lost", &error)
                    .await;
                return shell_tool_lost_error_from_message(error, Some(&shell_id));
            }
        };
        self.session_shells.apply_result(&result).await;
        persistent_result_to_tool(
            result,
            &project_config,
            &runtime_project_id,
            &session_id,
            "status",
            record.executor.as_str(),
            record.resource.as_deref(),
            None,
        )
    }

    async fn close_session_shell(
        &self,
        project: String,
        session_id: String,
        shell_id: String,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let runtime_project_id = resolved.resolved_id;
        let project_config = resolved.config;
        let record = match self
            .session_shells
            .record_for_operation(&session_id, &runtime_project_id, &shell_id)
            .await
        {
            Ok(record) => record,
            Err(error) => return shell_tool_error_from_message(error, Some(&shell_id)),
        };
        if !record.is_active() {
            return persistent_record_to_tool(record, &project_config, "close");
        }
        let result = self.close_record(&record, "explicit_close").await;
        match result {
            Ok(result) => {
                self.session_shells.apply_result(&result).await;
                persistent_result_to_tool(
                    result,
                    &project_config,
                    &runtime_project_id,
                    &session_id,
                    "close",
                    record.executor.as_str(),
                    record.resource.as_deref(),
                    None,
                )
            }
            Err(error) => {
                self.session_shells.mark_lost(&shell_id, &error).await;
                shell_tool_lost_error_from_message(error, Some(&shell_id))
            }
        }
    }

    async fn close_record(
        &self,
        record: &SessionShellRecord,
        reason: &str,
    ) -> Result<PersistentShellResult, String> {
        if record.executor == "agent" || record.executor == "ssh" {
            self.run_agent_persistent_shell(
                record.client_id.as_deref().unwrap_or_default(),
                PersistentShellRequest {
                    action: "close".to_string(),
                    shell_id: record.shell_id.clone(),
                    workflow_session_id: record.workflow_session_id.clone(),
                    runtime_project_id: record.runtime_project_id.clone(),
                    cwd: None,
                    shell: None,
                    command: None,
                    timeout_secs: None,
                    purpose: Some(reason.to_string()),
                },
                record.job_context(),
                10,
            )
            .await
        } else {
            let local_processes = self.session_shells.local_processes.clone();
            let shell_id = record.shell_id.clone();
            let session_id = record.workflow_session_id.clone();
            let project_id = record.runtime_project_id.clone();
            let reason = reason.to_string();
            match tokio::task::spawn_blocking(move || {
                local_processes.close(&shell_id, &session_id, &project_id, &reason)
            })
            .await
            {
                Ok(Ok(result)) => Ok(local_summary_result(
                    result.summary,
                    "closed",
                    result.already_closed,
                )),
                Ok(Err(error)) => Err(format!("{}: {}", error.code, error.message)),
                Err(error) => Err(format!(
                    "persistent_shell_runtime_error: persistent shell close worker failed: {error}"
                )),
            }
        }
    }

    async fn reconcile_active_shell_before_open(&self, session_id: &str, runtime_project_id: &str) {
        let records = self.session_shells.active_for_session(session_id).await;
        for record in records
            .into_iter()
            .filter(|record| record.runtime_project_id == runtime_project_id)
        {
            let result = if record.executor == "agent" || record.executor == "ssh" {
                self.run_agent_persistent_shell(
                    record.client_id.as_deref().unwrap_or_default(),
                    PersistentShellRequest {
                        action: "status".to_string(),
                        shell_id: record.shell_id.clone(),
                        workflow_session_id: record.workflow_session_id.clone(),
                        runtime_project_id: record.runtime_project_id.clone(),
                        cwd: None,
                        shell: None,
                        command: None,
                        timeout_secs: None,
                        purpose: None,
                    },
                    record.job_context(),
                    5,
                )
                .await
            } else {
                let local_processes = self.session_shells.local_processes.clone();
                let shell_id = record.shell_id.clone();
                let workflow_session_id = record.workflow_session_id.clone();
                let runtime_project_id = record.runtime_project_id.clone();
                match tokio::task::spawn_blocking(move || {
                    local_processes.status(&shell_id, &workflow_session_id, &runtime_project_id)
                })
                .await
                {
                    Ok(Ok(summary)) => {
                        let execution_state = if summary.busy { "executing" } else { "idle" };
                        Ok(local_summary_result(summary, execution_state, false))
                    }
                    Ok(Err(error)) => Err(format!("{}: {}", error.code, error.message)),
                    Err(error) => Err(format!(
                        "persistent_shell_runtime_error: persistent shell status worker failed: {error}"
                    )),
                }
            };
            match result {
                Ok(result) => self.session_shells.apply_result(&result).await,
                Err(error) => {
                    self.session_shells
                        .mark_lost(&record.shell_id, &error)
                        .await
                }
            }
        }
    }

    async fn close_opened_shell_if_session_inactive(
        &self,
        session_id: &str,
        runtime_project_id: &str,
        shell_id: &str,
    ) -> Option<ToolResult> {
        if self
            .sessions
            .lifecycle_state(session_id)
            .is_some_and(|lifecycle| lifecycle.allows_mutation())
        {
            return None;
        }
        if let Ok(record) = self
            .session_shells
            .record_for_operation(session_id, runtime_project_id, shell_id)
            .await
        {
            match self
                .close_record(&record, "workflow_session_closed_during_open")
                .await
            {
                Ok(result) => {
                    self.session_shells.apply_result(&result).await;
                    self.record_session_close_shell_result(session_id, &result);
                }
                Err(error) => {
                    self.session_shells.mark_lost(shell_id, &error).await;
                    self.sessions
                        .record_session_close_persistent_shell_evidence(
                            session_id,
                            shell_id,
                            "lost",
                            "lost",
                            Some(shell_error_code(&error)),
                            false,
                        );
                }
            }
        }
        Some(shell_tool_error(
            "persistent_shell_session_inactive",
            "Workflow Session closed while the persistent shell was opening; the shell was released",
            Some(shell_id),
        ))
    }

    async fn close_or_mark_lost(
        &self,
        record: &SessionShellRecord,
        reason: &str,
        original_error: &str,
    ) {
        match self.close_record(record, reason).await {
            Ok(result) => {
                let close_failed = result.error_code.is_some();
                self.session_shells.apply_result(&result).await;
                if close_failed {
                    self.session_shells
                        .mark_lost(&record.shell_id, original_error)
                        .await;
                }
            }
            Err(_) => {
                self.session_shells
                    .mark_lost(&record.shell_id, original_error)
                    .await
            }
        }
    }

    pub(crate) async fn close_persistent_shells_for_session(&self, session_id: &str) -> usize {
        let records = self.session_shells.active_for_session(session_id).await;
        let mut closed = 0usize;
        for record in records {
            match self.close_record(&record, "workflow_session_closed").await {
                Ok(result) => {
                    self.session_shells.apply_result(&result).await;
                    self.record_session_close_shell_result(session_id, &result);
                    if result.error_code.is_none() {
                        closed = closed.saturating_add(1);
                    }
                }
                Err(error) => {
                    self.session_shells
                        .mark_lost(&record.shell_id, &error)
                        .await;
                    self.sessions
                        .record_session_close_persistent_shell_evidence(
                            session_id,
                            &record.shell_id,
                            "lost",
                            "lost",
                            Some(shell_error_code(&error)),
                            false,
                        );
                }
            }
        }
        closed
    }

    fn record_session_close_shell_result(&self, session_id: &str, result: &PersistentShellResult) {
        let (shell_state, execution_state) = normalized_result_states(result);
        self.sessions
            .record_session_close_persistent_shell_evidence(
                session_id,
                &result.shell_id,
                shell_state,
                execution_state,
                result.error_code.as_deref(),
                result.already_closed,
            );
    }

    async fn run_agent_persistent_shell(
        &self,
        client_id: &str,
        request: PersistentShellRequest,
        job_context: Option<ShellJobContext>,
        wait_secs: u64,
    ) -> Result<PersistentShellResult, String> {
        let (request_id, receiver) = self
            .shell_clients
            .enqueue_persistent_shell(
                client_id.to_string(),
                request,
                job_context,
                "tool_runtime".to_string(),
            )
            .await?;
        match tokio::time::timeout(Duration::from_secs(wait_secs), receiver).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                Err("persistent_shell_result_lost: Runner result waiter was dropped".to_string())
            }
            Err(_) => {
                let dispatched = self.shell_clients.cancel_request(&request_id).await;
                Err(if dispatched {
                    "persistent_shell_runner_unavailable: result timed out after dispatch; shell state is lost"
                        .to_string()
                } else {
                    "persistent_shell_runner_unavailable: Runner did not accept the request"
                        .to_string()
                })
            }
        }
    }
}

/// Resolve the remote cwd for an SSH persistent shell. Unlike the local/agent
/// paths it is NOT constrained to the project root: it is a remote path that
/// only needs the existing bounded remote-path shape validation. `None` lets
/// the Runner fall back to the resource default_cwd / remote login default.
fn resolve_remote_cwd(cwd: Option<&str>) -> Result<Option<String>, String> {
    let Some(cwd) = cwd.map(str::trim).filter(|cwd| !cwd.is_empty()) else {
        return Ok(None);
    };
    if cwd.len() > 4096 || cwd.chars().any(char::is_control) {
        return Err("cwd must be a bounded remote path without control characters".to_string());
    }
    Ok(Some(cwd.to_string()))
}

fn local_cwd_within_project(project: &ProjectConfig, cwd: &Path) -> bool {
    matches!(
        (project.root().canonicalize(), cwd.canonicalize()),
        (Ok(root), Ok(cwd)) if cwd == root || cwd.starts_with(&root)
    )
}

fn local_launch(
    shell_id: &str,
    session_id: &str,
    runtime_project_id: &str,
    shell: &str,
    cwd: PathBuf,
) -> ShellLaunch {
    let env = std::env::vars()
        .filter(|(key, _)| {
            !matches!(
                key.as_str(),
                "WEBCODEX_TOKEN" | "WEBCODEX_AGENT_TOKEN" | "WEBCODEX_USER_TOKEN" | "AUTHORIZATION"
            )
        })
        .collect();
    ShellLaunch {
        identity: ShellIdentity {
            shell_id: shell_id.to_string(),
            workflow_session_id: session_id.to_string(),
            runtime_project_id: runtime_project_id.to_string(),
            executor: "local".to_string(),
            client_id: None,
        },
        dialect: shell.to_string(),
        profile: None,
        program: shell.to_string(),
        args: if shell == "bash" {
            vec!["--noprofile".to_string(), "--norc".to_string()]
        } else {
            Vec::new()
        },
        initial_cwd: cwd,
        env,
        initialization: None,
        max_output_bytes: LOCAL_MAX_OUTPUT_BYTES,
    }
}

fn local_summary_result(
    summary: ShellSummary,
    execution_state: &str,
    already_closed: bool,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: summary.identity.shell_id,
        workflow_session_id: summary.identity.workflow_session_id,
        runtime_project_id: summary.identity.runtime_project_id,
        shell_state: summary.state.as_str().to_string(),
        execution_state: execution_state.to_string(),
        command_started: false,
        command_completed: false,
        exit_code: summary.exit_code,
        stdout: String::new(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: 0,
        cwd: Some(summary.cwd.to_string_lossy().to_string()),
        initial_cwd: Some(summary.initial_cwd.to_string_lossy().to_string()),
        shell: Some(summary.dialect),
        profile: summary.profile,
        created_at: Some(summary.created_at),
        last_activity_at: Some(summary.last_activity_at),
        busy: summary.busy,
        already_closed,
        close_reason: summary.close_reason,
        error_code: None,
        error: None,
    }
}

fn local_exec_result(
    session_id: &str,
    runtime_project_id: &str,
    result: webcodex_persistent_shell::ShellExecResult,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: result.shell_id,
        workflow_session_id: session_id.to_string(),
        runtime_project_id: runtime_project_id.to_string(),
        shell_state: result.shell_state.as_str().to_string(),
        execution_state: result.execution_state,
        command_started: result.command_started,
        command_completed: result.command_completed,
        exit_code: result.exit_code,
        stdout: result.stdout,
        stderr: result.stderr,
        stdout_truncated: result.stdout_truncated,
        stderr_truncated: result.stderr_truncated,
        duration_ms: result.duration_ms,
        cwd: Some(result.cwd.to_string_lossy().to_string()),
        initial_cwd: None,
        shell: None,
        profile: None,
        created_at: None,
        last_activity_at: None,
        busy: false,
        already_closed: false,
        close_reason: None,
        error_code: result.error_code,
        error: result.error,
    }
}

fn persistent_record_to_tool(
    record: SessionShellRecord,
    project: &ProjectConfig,
    action: &str,
) -> ToolResult {
    let result = PersistentShellResult {
        shell_id: record.shell_id,
        workflow_session_id: record.workflow_session_id.clone(),
        runtime_project_id: record.runtime_project_id.clone(),
        shell_state: record.state,
        execution_state: action.to_string(),
        command_started: false,
        command_completed: false,
        exit_code: record.exit_code,
        stdout: String::new(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: 0,
        cwd: Some(record.cwd),
        initial_cwd: Some(record.initial_cwd),
        shell: Some(record.shell),
        profile: record.profile,
        created_at: Some(record.created_at),
        last_activity_at: Some(record.last_activity_at),
        busy: record.busy.load(Ordering::SeqCst),
        already_closed: action == "close",
        close_reason: record.close_reason,
        error_code: None,
        error: None,
    };
    persistent_result_to_tool(
        result,
        project,
        &record.runtime_project_id,
        &record.workflow_session_id,
        action,
        record.executor.as_str(),
        record.resource.as_deref(),
        None,
    )
}

fn persistent_result_to_tool(
    mut result: PersistentShellResult,
    project: &ProjectConfig,
    runtime_project_id: &str,
    session_id: &str,
    action: &str,
    executor: &str,
    resource: Option<&str>,
    purpose: Option<ExecutionPurpose>,
) -> ToolResult {
    normalize_persistent_result_state(&mut result);
    let cwd = relative_cwd(project, result.cwd.as_deref());
    let initial_cwd = relative_cwd(project, result.initial_cwd.as_deref());
    let command_ok = result.command_completed && result.exit_code == Some(0);
    let output = json!({
        "shell_id": result.shell_id,
        "project": runtime_project_id,
        "session_id": session_id,
        "executor": executor,
        "resource": resource,
        "shell": result.shell,
        "profile": result.profile,
        "initial_cwd": initial_cwd,
        "cwd": cwd,
        "created_at": result.created_at,
        "last_activity_at": result.last_activity_at,
        "shell_state": result.shell_state,
        "execution_state": result.execution_state,
        "command_started": result.command_started,
        "command_completed": result.command_completed,
        "command_ok": command_ok,
        "exit_code": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "stdout_truncated": result.stdout_truncated,
        "stderr_truncated": result.stderr_truncated,
        "duration_ms": result.duration_ms,
        "busy": result.busy,
        "already_closed": result.already_closed,
        "close_reason": result.close_reason,
        "purpose": purpose.map(|purpose| purpose.as_str()),
        "error_code": result.error_code,
        "tool_failure": result.error_code.is_some(),
    });
    if let Some(error) = result.error {
        return ToolResult::err_with_output(error, output);
    }
    if action == "exec" && !command_ok {
        return ToolResult::err_with_output(
            format!(
                "persistent shell command exited with {}",
                result
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown status".to_string())
            ),
            output,
        );
    }
    ToolResult::ok(output)
}

fn relative_cwd(project: &ProjectConfig, cwd: Option<&str>) -> Option<String> {
    let cwd = cwd?;
    if project.is_agent() {
        project_relative_agent_cwd(project, cwd).ok()
    } else {
        project_relative_cwd(project, Path::new(cwd)).ok()
    }
}

fn shell_tool_error_from_message(message: String, shell_id: Option<&str>) -> ToolResult {
    let (code, detail) = split_shell_error(&message);
    shell_tool_error(code, detail, shell_id)
}

fn shell_tool_lost_error_from_message(message: String, shell_id: Option<&str>) -> ToolResult {
    let (code, detail) = split_shell_error(&message);
    shell_tool_error_with_state(code, detail, shell_id, "lost", "lost")
}

fn split_shell_error(message: &str) -> (&str, &str) {
    let Some((code, detail)) = message.split_once(':') else {
        return ("persistent_shell_runtime_error", message);
    };
    let code = code.trim();
    if code.is_empty()
        || code.len() > 80
        || !code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        ("persistent_shell_runtime_error", message)
    } else {
        (code, detail.trim())
    }
}

fn shell_tool_error(code: &str, message: impl Into<String>, shell_id: Option<&str>) -> ToolResult {
    let shell_state = if code == "shell_reset_required" {
        "poisoned"
    } else {
        "unknown"
    };
    shell_tool_error_with_state(code, message, shell_id, shell_state, "rejected")
}

fn shell_tool_error_with_state(
    code: &str,
    message: impl Into<String>,
    shell_id: Option<&str>,
    shell_state: &str,
    execution_state: &str,
) -> ToolResult {
    let message = message.into();
    ToolResult::err_with_output(
        format!("{code}: {message}"),
        json!({
            "error_code": code,
            "shell_id": shell_id,
            "command_started": false,
            "command_completed": false,
            "shell_state": shell_state,
            "execution_state": execution_state,
            "tool_failure": true,
        }),
    )
}

fn normalize_persistent_result_state(result: &mut PersistentShellResult) {
    if result.shell_state == "unknown" && result.error_code.is_some() {
        result.shell_state = "lost".to_string();
        result.execution_state = "lost".to_string();
    }
}

fn normalized_result_states(result: &PersistentShellResult) -> (&str, &str) {
    if result.shell_state == "unknown" && result.error_code.is_some() {
        ("lost", "lost")
    } else {
        (&result.shell_state, &result.execution_state)
    }
}

fn shell_error_code(message: &str) -> &str {
    split_shell_error(message).0
}
