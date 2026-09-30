//! Canonical Runner protocol result projections.

use super::*;

pub(super) fn summary_result(
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

pub(super) fn exec_result(
    operation: &PersistentShellRequest,
    result: ShellExecResult,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: result.shell_id,
        workflow_session_id: operation.workflow_session_id.clone(),
        runtime_project_id: operation.runtime_project_id.clone(),
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

pub(super) fn shell_error_result(
    operation: &PersistentShellRequest,
    error: ShellError,
) -> PersistentShellResult {
    error_result(
        &operation.shell_id,
        &operation.workflow_session_id,
        &operation.runtime_project_id,
        error.code,
        error.message,
    )
}

pub(super) fn summary_error_result(
    summary: ShellSummary,
    code: &str,
    message: impl Into<String>,
) -> PersistentShellResult {
    let mut result = summary_result(summary, "rejected", false);
    result.error_code = Some(code.to_string());
    result.error = Some(message.into());
    result
}

/// Like `summary_error_result` but fetches the current status from the manager
/// first, so an SSH exec rejection still reports the authoritative running
/// state (and closes a stale shell) the way the local path does.
pub(super) fn summary_error_result_from_status(
    processes: &ProcessManager,
    operation: &PersistentShellRequest,
    code: &str,
    message: impl Into<String>,
) -> PersistentShellResult {
    match processes.status(
        &operation.shell_id,
        &operation.workflow_session_id,
        &operation.runtime_project_id,
    ) {
        Ok(summary) => summary_error_result(summary, code, message),
        Err(error) => shell_error_result(operation, error),
    }
}

pub(super) fn error_result(
    shell_id: &str,
    workflow_session_id: &str,
    runtime_project_id: &str,
    code: &str,
    message: impl Into<String>,
) -> PersistentShellResult {
    PersistentShellResult {
        shell_id: shell_id.to_string(),
        workflow_session_id: workflow_session_id.to_string(),
        runtime_project_id: runtime_project_id.to_string(),
        shell_state: if code == "shell_reset_required" {
            ShellState::Poisoned.as_str().to_string()
        } else {
            "unknown".to_string()
        },
        execution_state: "rejected".to_string(),
        command_started: false,
        command_completed: false,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: 0,
        cwd: None,
        initial_cwd: None,
        shell: None,
        profile: None,
        created_at: None,
        last_activity_at: None,
        busy: false,
        already_closed: false,
        close_reason: None,
        error_code: Some(code.to_string()),
        error: Some(message.into()),
    }
}
