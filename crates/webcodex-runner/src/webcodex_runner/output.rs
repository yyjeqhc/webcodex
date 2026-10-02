use std::time::Instant;
use webcodex_core::runner_protocol::ShellCommandExecutionState;

#[derive(Debug)]
pub(crate) struct CommandResult {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: Option<String>,
    pub(crate) stderr: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) error: Option<String>,
}

#[derive(Debug)]
pub(crate) struct ShellCommandResult {
    pub(crate) result: CommandResult,
    pub(crate) execution_state: ShellCommandExecutionState,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

impl ShellCommandResult {
    pub(crate) fn not_started(result: CommandResult) -> Self {
        Self {
            result,
            execution_state: ShellCommandExecutionState::NotStarted,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    pub(crate) fn outcome_unknown(result: CommandResult) -> Self {
        Self {
            result,
            execution_state: ShellCommandExecutionState::OutcomeUnknown,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    pub(crate) fn timed_out(result: CommandResult) -> Self {
        Self {
            result,
            execution_state: ShellCommandExecutionState::TimedOut,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    pub(crate) fn completed(result: CommandResult) -> Self {
        Self {
            result,
            execution_state: ShellCommandExecutionState::Completed,
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }

    pub(crate) fn with_stream_truncation(
        mut self,
        stdout_truncated: bool,
        stderr_truncated: bool,
    ) -> Self {
        self.stdout_truncated = stdout_truncated;
        self.stderr_truncated = stderr_truncated;
        self
    }
}

pub(crate) fn line_edit_stdout(value: serde_json::Value, start: Instant) -> CommandResult {
    CommandResult {
        exit_code: Some(0),
        stdout: Some(serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string())),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

/// Build a success `CommandResult` with JSON output in stdout.
pub(crate) fn ok_cmd(start: Instant, result: serde_json::Value) -> CommandResult {
    CommandResult {
        exit_code: Some(0),
        stdout: Some(serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}

/// Build an error `CommandResult`.
pub(crate) fn err_cmd(start: Instant, msg: String) -> CommandResult {
    CommandResult {
        exit_code: None,
        stdout: None,
        stderr: None,
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: Some(msg),
    }
}
