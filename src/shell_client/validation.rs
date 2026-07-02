use crate::shell_protocol::{ShellAgentProjectSummary, ShellFileOpRequest, ShellRunRequest};
use sha2::{Digest, Sha256};

const MAX_CLIENT_ID_LEN: usize = 80;
const MAX_CLIENT_FIELD_LEN: usize = 200;
/// Max length for `agent_instance_id`. A UUID v4 is 36 chars; allow headroom
/// for future formats but bound it so a malicious peer cannot stash huge
/// strings in the registry.
const MAX_AGENT_INSTANCE_ID_LEN: usize = 128;
pub(super) const MAX_COMMAND_LEN: usize = 8_000;
const MAX_CWD_LEN: usize = 1_024;
const MAX_FILE_PATH_LEN: usize = 2_048;
const MAX_FILE_CONTENT_BYTES: usize = 512 * 1024;
pub(super) const MAX_RUN_STDIN_BYTES: usize = 15 * 1024 * 1024;
const MAX_SYNC_WAIT_SECS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECS: u64 = 24 * 60 * 60;

pub(super) fn validate_id(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > MAX_CLIENT_ID_LEN {
        return Err(format!(
            "{} must be 1..={} characters",
            field, MAX_CLIENT_ID_LEN
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "{} may only contain ASCII letters, digits, '-', '_', and '.'",
            field
        ));
    }
    Ok(())
}

/// Validate `agent_instance_id`. It must be a non-empty, bounded ASCII string.
/// We accept the canonical UUID v4 format (`8-4-4-4-12` hex with dashes) and
/// also any short alphanumeric/dash string so future identity formats keep
/// working, but we reject empty / oversized / control-char values. This is not
/// a secret, so the value itself may appear in logs and `runtime_status`.
pub(super) fn validate_agent_instance_id(value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err("agent_instance_id must not be empty".to_string());
    }
    if value.len() > MAX_AGENT_INSTANCE_ID_LEN {
        return Err(format!(
            "agent_instance_id is too long; maximum is {} characters",
            MAX_AGENT_INSTANCE_ID_LEN
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "agent_instance_id may only contain ASCII letters, digits, '-', and '_'".to_string(),
        );
    }
    Ok(())
}

pub(super) fn validate_optional_field(value: &Option<String>, field: &str) -> Result<(), String> {
    if let Some(value) = value {
        if value.chars().count() > MAX_CLIENT_FIELD_LEN {
            return Err(format!(
                "{} is too long; maximum is {} characters",
                field, MAX_CLIENT_FIELD_LEN
            ));
        }
        if value.contains('\0') {
            return Err(format!("{} cannot contain NUL bytes", field));
        }
    }
    Ok(())
}

pub(super) fn validate_file_request(body: &ShellFileOpRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    match body.op.as_str() {
        "read"
        | "write"
        | "list"
        | "replace_line_range"
        | "insert_at_line"
        | "delete_line_range"
        | "replace_exact_block"
        | "insert_before_pattern"
        | "insert_after_pattern"
        | "apply_text_edits" => {}
        _ => {
            return Err(
                "op must be one of read, write, list, replace_line_range, insert_at_line, delete_line_range, replace_exact_block, insert_before_pattern, insert_after_pattern, apply_text_edits"
                    .to_string(),
            )
        }
    }
    let line_edit = matches!(
        body.op.as_str(),
        "replace_line_range" | "insert_at_line" | "delete_line_range"
    );
    let replace_exact_block = body.op == "replace_exact_block";
    let insert_pattern = matches!(
        body.op.as_str(),
        "insert_before_pattern" | "insert_after_pattern"
    );
    let anchor_edit = replace_exact_block || insert_pattern;

    let path = body.path.trim();
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if body.path.len() > MAX_FILE_PATH_LEN {
        return Err(format!(
            "path is too long; maximum is {} bytes",
            MAX_FILE_PATH_LEN
        ));
    }
    if body.path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }

    validate_sha256(&body.expected_sha256)?;
    if body.expected_sha256.is_some() && body.op != "write" && !line_edit && !replace_exact_block {
        return Err(
            "expected_sha256 is only allowed for op=write, replace_exact_block, or line edit ops"
                .to_string(),
        );
    }
    if let Some(prefix) = &body.expected_prefix {
        if !line_edit {
            return Err("expected_prefix is only allowed for line edit ops".to_string());
        }
        if prefix.contains('\0') {
            return Err("expected_prefix cannot contain NUL bytes".to_string());
        }
    }
    if body.create_dirs && body.op != "write" {
        return Err("create_dirs is only allowed for op=write".to_string());
    }

    if let Some(content) = &body.content {
        if content.len() > MAX_FILE_CONTENT_BYTES {
            return Err(format!(
                "content is too large; maximum is {} bytes",
                MAX_FILE_CONTENT_BYTES
            ));
        }
        if body.op != "write"
            && body.op != "replace_line_range"
            && body.op != "insert_at_line"
            && body.op != "apply_text_edits"
            && !anchor_edit
        {
            return Err(
                "content is only allowed for op=write, line edit insert/replace, apply_text_edits, or anchor edit tools"
                    .to_string(),
            );
        }
    }
    if let Some(old_text) = &body.old_text {
        if !replace_exact_block {
            return Err("old_text is only allowed for op=replace_exact_block".to_string());
        }
        if old_text.contains('\0') {
            return Err("old_text cannot contain NUL bytes".to_string());
        }
    }
    if let Some(pattern) = &body.pattern {
        if !insert_pattern {
            return Err("pattern is only allowed for insert pattern ops".to_string());
        }
        if pattern.contains('\0') {
            return Err("pattern cannot contain NUL bytes".to_string());
        }
    }

    if body.op == "write" && body.content.is_none() {
        return Err("content is required for op=write".to_string());
    }

    match body.op.as_str() {
        "read" => {
            match (body.start_line, body.end_line) {
                (Some(start), Some(end)) => {
                    if start == 0 || end < start {
                        return Err("invalid line range".to_string());
                    }
                }
                (Some(_), None) => {
                    return Err(
                        "end_line is required when start_line is set for op=read".to_string()
                    );
                }
                (None, Some(_)) => {
                    return Err(
                        "start_line is required when end_line is set for op=read".to_string()
                    );
                }
                (None, None) => {}
            }
            if body.line.is_some() {
                return Err("line is only allowed for op=insert_at_line".to_string());
            }
        }
        "replace_line_range" => {
            let start = body
                .start_line
                .ok_or_else(|| "start_line is required for op=replace_line_range".to_string())?;
            let end = body
                .end_line
                .ok_or_else(|| "end_line is required for op=replace_line_range".to_string())?;
            if start == 0 || end < start {
                return Err("invalid line range".to_string());
            }
            if body.line.is_some() {
                return Err("line is only allowed for op=insert_at_line".to_string());
            }
            if body.content.is_none() {
                return Err("content is required for op=replace_line_range".to_string());
            }
        }
        "delete_line_range" => {
            let start = body
                .start_line
                .ok_or_else(|| "start_line is required for op=delete_line_range".to_string())?;
            let end = body
                .end_line
                .ok_or_else(|| "end_line is required for op=delete_line_range".to_string())?;
            if start == 0 || end < start {
                return Err("invalid line range".to_string());
            }
            if body.line.is_some() || body.content.is_some() {
                return Err("delete_line_range only accepts start_line/end_line guards".to_string());
            }
        }
        "insert_at_line" => {
            let line = body
                .line
                .ok_or_else(|| "line is required for op=insert_at_line".to_string())?;
            if line == 0 {
                return Err("line out of range".to_string());
            }
            if body.start_line.is_some() || body.end_line.is_some() {
                return Err(
                    "start_line/end_line are only allowed for range line edit ops".to_string(),
                );
            }
            if body.content.is_none() {
                return Err("content is required for op=insert_at_line".to_string());
            }
        }
        "replace_exact_block" => {
            if body.old_text.as_deref().unwrap_or_default().is_empty() {
                return Err("old_text is required for op=replace_exact_block".to_string());
            }
            if body.content.is_none() {
                return Err("content is required for op=replace_exact_block".to_string());
            }
            if body.pattern.is_some()
                || body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
            {
                return Err(
                    "replace_exact_block only accepts old_text/content/expected_sha256 guards"
                        .to_string(),
                );
            }
        }
        "insert_before_pattern" | "insert_after_pattern" => {
            if body.pattern.as_deref().unwrap_or_default().is_empty() {
                return Err("pattern is required for insert pattern ops".to_string());
            }
            if body.content.as_deref().unwrap_or_default().is_empty() {
                return Err("content is required for insert pattern ops".to_string());
            }
            if body.old_text.is_some()
                || body.expected_sha256.is_some()
                || body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
            {
                return Err("insert pattern ops only accept pattern/content".to_string());
            }
        }
        _ => {
            if body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
            {
                return Err("line edit fields are only allowed for line edit ops".to_string());
            }
            if body.old_text.is_some() || body.pattern.is_some() {
                return Err("anchor edit fields are only allowed for anchor edit ops".to_string());
            }
        }
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for shellFileOp",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

pub(super) fn validate_run_request(body: &ShellRunRequest) -> Result<(), String> {
    validate_id(&body.client_id, "client_id")?;
    let command = body.command.trim();
    if command.is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if body.command.len() > MAX_COMMAND_LEN {
        return Err(format!(
            "command is too long; maximum is {} bytes",
            MAX_COMMAND_LEN
        ));
    }
    if body.command.contains('\0') {
        return Err("command cannot contain NUL bytes".to_string());
    }
    if let Some(stdin) = &body.stdin {
        if stdin.len() > MAX_RUN_STDIN_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {} bytes",
                MAX_RUN_STDIN_BYTES
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = &body.cwd {
        if cwd.len() > MAX_CWD_LEN {
            return Err(format!("cwd is too long; maximum is {} bytes", MAX_CWD_LEN));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if body.timeout_secs == 0 || body.timeout_secs > MAX_COMMAND_TIMEOUT_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {}",
            MAX_COMMAND_TIMEOUT_SECS
        ));
    }
    if body.wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {} for synchronous runShell",
            MAX_SYNC_WAIT_SECS
        ));
    }
    Ok(())
}

pub(super) fn trim_string(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(super) fn normalize_project_summaries(
    projects: Option<Vec<ShellAgentProjectSummary>>,
) -> Vec<ShellAgentProjectSummary> {
    let mut projects = projects.unwrap_or_default();
    projects.sort_by(|a, b| a.id.cmp(&b.id));
    projects.dedup_by(|a, b| a.id == b.id);
    projects
}

pub(super) fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn validate_sha256(value: &Option<String>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() != 64 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected_sha256 must be 64 hex characters".to_string());
    }
    Ok(())
}
