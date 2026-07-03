use std::time::Instant;

#[derive(Debug)]
pub(crate) struct CommandResult {
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: Option<String>,
    pub(crate) stderr: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) error: Option<String>,
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
