use super::output::{err_cmd, ok_cmd, CommandResult};
use crate::shell_protocol::ShellAgentShellRequest;
use crate::workspace_checkpoint::{create_workspace_checkpoint, restore_workspace_checkpoint};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

pub(crate) fn is_checkpoint_request_kind(kind: &str) -> bool {
    matches!(kind, "file_checkpoint_create" | "file_checkpoint_restore")
}

pub(crate) fn handle_checkpoint_file_request(
    request: &ShellAgentShellRequest,
    resolved: &Path,
    start: Instant,
) -> CommandResult {
    let payload = match parse_payload(request) {
        Ok(payload) => payload,
        Err(err) => return ok_cmd(start, checkpoint_error("invalid_checkpoint_payload", err)),
    };
    let output = match request.kind.as_str() {
        "file_checkpoint_create" => {
            let include_untracked = payload
                .get("include_untracked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            create_workspace_checkpoint(resolved, include_untracked)
        }
        "file_checkpoint_restore" => {
            let Some(checkpoint) = payload.get("checkpoint") else {
                return ok_cmd(
                    start,
                    checkpoint_error("invalid_checkpoint_payload", "checkpoint is required"),
                );
            };
            restore_workspace_checkpoint(resolved, checkpoint)
        }
        _ => {
            return err_cmd(
                start,
                format!("unknown checkpoint request kind: {}", request.kind),
            )
        }
    };
    ok_cmd(start, output)
}

fn parse_payload(request: &ShellAgentShellRequest) -> Result<Value, String> {
    let content = request
        .content
        .as_deref()
        .ok_or_else(|| "checkpoint request missing JSON payload".to_string())?;
    serde_json::from_str(content).map_err(|err| format!("invalid JSON payload: {err}"))
}

fn checkpoint_error(kind: &str, message: impl Into<String>) -> Value {
    json!({
        "error_kind": kind,
        "error": message.into(),
    })
}
