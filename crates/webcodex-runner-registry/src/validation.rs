use sha2::{Digest, Sha256};
use webcodex_core::runner_protocol::{
    validate_process_argv, validate_raw_shell_wire_command, validate_script_request,
    ProviderCallSummary, RunnerConfigReloadStatus, RunnerProjectSummary, ShellFileOpRequest,
    ShellProcessArgv, ShellRunRequest, ShellScriptPayload, ToolProvidersStatus,
    PROCESS_CWD_MAX_BYTES, PROCESS_STDIN_MAX_BYTES,
    PROJECT_INVENTORY_SNAPSHOT_MAX_SERIALIZED_BYTES,
    STRUCTURED_EXECUTION_DIRECT_SYNC_TIMEOUT_MAX_SECS,
};

const MAX_CLIENT_ID_LEN: usize = 80;
const MAX_RUNNER_FIELD_LEN: usize = 200;
/// Max length for `agent_instance_id`. A UUID v4 is 36 chars; allow headroom
/// for future formats but bound it so a malicious peer cannot stash huge
/// strings in the registry.
const MAX_AGENT_INSTANCE_ID_LEN: usize = 128;
const MAX_CWD_LEN: usize = 1_024;
const MAX_FILE_PATH_LEN: usize = 2_048;
const MAX_FILE_CONTENT_BYTES: usize = 512 * 1024;
const MAX_STRUCTURED_EDIT_PAYLOAD_BYTES: usize = 2 * 1024 * 1024;
const MAX_ARTIFACT_PAYLOAD_BYTES: usize = 15 * 1024 * 1024;
const MAX_CHECKPOINT_PAYLOAD_BYTES: usize = 15 * 1024 * 1024;
pub(super) const MAX_RUN_STDIN_BYTES: usize = 15 * 1024 * 1024;
const MAX_SYNC_WAIT_SECS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECS: u64 = 24 * 60 * 60;
const MAX_PROVIDER_TEXT_CHARS: usize = 120;
const MAX_PROVIDER_TOOL_NAMES: usize = 64;

pub(super) fn normalize_config_reload(
    status: Option<RunnerConfigReloadStatus>,
) -> Option<RunnerConfigReloadStatus> {
    let mut status = status?;
    const RESULTS: &str = "not_attempted success partial failure unsupported";
    const ERRORS: &str = "config_read_failed config_parse_failed config_validation_failed provider_config_invalid reload_unsupported";
    const FIELDS: &str = "capabilities client_id display_name host_context hostname max_concurrent_jobs owner poll_interval_ms project_registry_dir projects_dir quic server_url token transport websocket_connect_timeout_secs";
    const ERROR_FIELDS: &str = "max_concurrent_jobs shell.max_persistent_shells shell.persistent_shell_idle_timeout_secs acp.max_concurrent_runs acp.permission_timeout_secs mcp.request_timeout_secs";
    const ERROR_REASONS: &str = "out_of_range";
    if status.generation == 0
        || !RESULTS
            .split_whitespace()
            .any(|v| v == status.last_reload_result)
        || status
            .last_reload_error_code
            .as_deref()
            .is_some_and(|code| !ERRORS.split_whitespace().any(|v| v == code))
    {
        return None;
    }
    let diagnostic_valid = match (
        status.last_reload_error_field.as_deref(),
        status.last_reload_error_reason.as_deref(),
    ) {
        (None, None) => true,
        (Some(field), Some(reason)) => {
            ERROR_FIELDS.split_whitespace().any(|value| value == field)
                && ERROR_REASONS
                    .split_whitespace()
                    .any(|value| value == reason)
                && status.last_reload_error_code.as_deref() == Some("config_validation_failed")
                && status.last_reload_result == "failure"
        }
        _ => false,
    };
    if !diagnostic_valid {
        status.last_reload_error_field = None;
        status.last_reload_error_reason = None;
    }
    status
        .restart_required_fields
        .retain(|field| FIELDS.split_whitespace().any(|v| v == field));
    status.restart_required_fields.sort();
    status.restart_required_fields.dedup();
    status.restart_required = !status.restart_required_fields.is_empty();
    Some(status)
}

fn bounded_provider_text(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_PROVIDER_TEXT_CHARS)
        .collect()
}

fn safe_provider_identifier(value: &str) -> Option<String> {
    let value = bounded_provider_text(value);
    (!value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_-.:".contains(character)))
    .then_some(value)
}

fn safe_provider_version(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, ' ' | '.' | '_' | '-' | '+' | '(' | ')')
        }))
    .then(|| bounded_provider_text(value))
}

/// Normalize untrusted provider metadata without making agent traffic fail.
/// Unknown fields are already discarded by serde; unknown enum-like values or
/// unsafe strings drop the entire optional update so tool completion continues.
pub(super) fn normalize_tool_providers(
    status: Option<ToolProvidersStatus>,
) -> Option<ToolProvidersStatus> {
    let mut status = status?;
    status.config_reload = normalize_config_reload(Some(status.config_reload))?;
    if !matches!(
        status.strategy.as_str(),
        "native" | "claude_code" | "claude_code_then_native"
    ) || !matches!(
        status.claude_code.process_state.as_str(),
        "not_started"
            | "starting"
            | "initializing"
            | "discovering"
            | "mapping"
            | "running"
            | "stopped"
    ) {
        return None;
    }
    status.claude_code.version = status
        .claude_code
        .version
        .as_deref()
        .and_then(safe_provider_version);
    let mut names = status
        .claude_code
        .discovered_tool_names
        .iter()
        .filter_map(|name| safe_provider_identifier(name))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names.truncate(MAX_PROVIDER_TOOL_NAMES);
    status.claude_code.discovered_tool_names = names;
    if status.claude_code.capabilities.len() > 2
        || status.claude_code.capabilities.iter().any(|(name, state)| {
            !matches!(name.as_str(), "edit_file" | "search_project_text")
                || !matches!(state.as_str(), "available" | "unmapped" | "schema_mismatch")
        })
    {
        return None;
    }
    status.claude_code.last_error_code = status
        .claude_code
        .last_error_code
        .as_deref()
        .and_then(safe_provider_identifier);
    status.claude_code.last_call = status
        .claude_code
        .last_call
        .and_then(normalize_provider_call);
    Some(status)
}

fn normalize_provider_call(mut call: ProviderCallSummary) -> Option<ProviderCallSummary> {
    if !matches!(
        call.capability.as_str(),
        "edit_file" | "search_project_text"
    ) || !matches!(call.selected_provider.as_str(), "claude_code" | "native")
        || !matches!(call.result.as_str(), "success" | "failure")
        || !call
            .write_state
            .as_deref()
            .is_none_or(|state| matches!(state, "not_submitted" | "confirmed" | "uncertain"))
    {
        return None;
    }
    if (call.capability == "search_project_text" && call.write_state.is_some())
        || (call.capability == "edit_file" && call.write_state.is_none())
        || (call.fallback_used && call.selected_provider != "native")
    {
        return None;
    }
    call.duration_ms = call.duration_ms.min(24 * 60 * 60 * 1000);
    call.error_code = call
        .error_code
        .as_deref()
        .and_then(safe_provider_identifier);
    Some(call)
}

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
pub(super) fn validate_runner_instance_id(value: &str) -> Result<(), String> {
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
        if value.chars().count() > MAX_RUNNER_FIELD_LEN {
            return Err(format!(
                "{} is too long; maximum is {} characters",
                field, MAX_RUNNER_FIELD_LEN
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
        | "project_overview"
        | "delete_project_files"
        | "write_project_file"
        | "apply_text_edits"
        | "apply_patch"
        | "save_project_artifact"
        | "read_project_artifact_metadata"
        | "read_project_artifact"
        | "read_project_artifact_export_chunk"
        | "artifact_upload_begin"
        | "artifact_upload_chunk"
        | "artifact_upload_finish"
        | "artifact_upload_abort"
        | "checkpoint_create"
        | "checkpoint_restore"
        | "skill_list_packages"
        | "skill_read_file" => {}
        _ => {
            return Err(
                "op must be one of read, write, list, project_overview, write_project_file, apply_text_edits, apply_patch, save_project_artifact, read_project_artifact_metadata, read_project_artifact, read_project_artifact_export_chunk, artifact_upload_begin, artifact_upload_chunk, artifact_upload_finish, artifact_upload_abort, checkpoint_create, checkpoint_restore, skill_list_packages, skill_read_file"
                    .to_string(),
            )
        }
    }
    let structured_edit_payload = matches!(body.op.as_str(), "write_project_file" | "apply_patch");
    let structured_delete_payload = body.op == "delete_project_files";
    let artifact_payload = matches!(
        body.op.as_str(),
        "save_project_artifact"
            | "read_project_artifact_metadata"
            | "read_project_artifact"
            | "read_project_artifact_export_chunk"
            | "artifact_upload_begin"
            | "artifact_upload_chunk"
            | "artifact_upload_finish"
            | "artifact_upload_abort"
    );
    let checkpoint_payload = matches!(body.op.as_str(), "checkpoint_create" | "checkpoint_restore");
    let skill_payload = matches!(body.op.as_str(), "skill_list_packages" | "skill_read_file");

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
    if body.expected_sha256.is_some() && body.op != "write" {
        return Err("expected_sha256 is only allowed for op=write".to_string());
    }
    if let Some(prefix) = &body.expected_prefix {
        if body.op != "write" {
            return Err("expected_prefix is only allowed for op=write".to_string());
        }
        if prefix.contains('\0') {
            return Err("expected_prefix cannot contain NUL bytes".to_string());
        }
    }
    if body.create_dirs && body.op != "write" {
        return Err("create_dirs is only allowed for op=write".to_string());
    }

    if let Some(content) = &body.content {
        let max_content_bytes = if artifact_payload {
            MAX_ARTIFACT_PAYLOAD_BYTES
        } else if checkpoint_payload {
            MAX_CHECKPOINT_PAYLOAD_BYTES
        } else if structured_edit_payload {
            MAX_STRUCTURED_EDIT_PAYLOAD_BYTES
        } else {
            MAX_FILE_CONTENT_BYTES
        };
        if content.len() > max_content_bytes {
            return Err(format!(
                "content is too large; maximum is {} bytes",
                max_content_bytes
            ));
        }
        if body.op != "write"
            && body.op != "project_overview"
            && body.op != "apply_text_edits"
            && !structured_edit_payload
            && !structured_delete_payload
            && !artifact_payload
            && !checkpoint_payload
            && !skill_payload
        {
            return Err(
                "content is only allowed for op=write, project_overview options, delete_project_files, apply_text_edits, structured edit tools, artifact tools, checkpoint tools, or skill runtime file ops"
                    .to_string(),
            );
        }
    }
    if body.old_text.is_some() {
        return Err("old_text is not supported for any file op".to_string());
    }
    if body.pattern.is_some() {
        return Err("pattern is not supported for any file op".to_string());
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
                return Err("line is not supported for any file op".to_string());
            }
        }
        "skill_read_file" => {
            if body.content.is_none() || body.start_line.is_none() || body.end_line.is_none() {
                return Err("skill_read_file requires content/start_line/end_line".to_string());
            }
            if body.start_line == Some(0)
                || body
                    .end_line
                    .zip(body.start_line)
                    .is_some_and(|(end, start)| end < start)
                || body.old_text.is_some()
                || body.pattern.is_some()
                || body.expected_sha256.is_some()
                || body.expected_prefix.is_some()
                || body.line.is_some()
                || body.create_dirs
            {
                return Err(
                    "skill_read_file only accepts path/cwd/content/max_bytes/start_line/end_line"
                        .to_string(),
                );
            }
        }
        "skill_list_packages" => {
            if body.content.is_none()
                || body.old_text.is_some()
                || body.pattern.is_some()
                || body.expected_sha256.is_some()
                || body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
                || body.max_bytes.is_some()
                || body.create_dirs
            {
                return Err("skill_list_packages only accepts path/cwd/content".to_string());
            }
        }
        "write_project_file" | "apply_patch" => {
            if body.content.is_none() {
                return Err(format!("content is required for op={}", body.op));
            }
            if body.old_text.is_some()
                || body.pattern.is_some()
                || body.expected_sha256.is_some()
                || body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
                || body.max_bytes.is_some()
                || body.create_dirs
            {
                return Err(format!("{} only accepts path/content", body.op));
            }
        }
        "save_project_artifact"
        | "read_project_artifact_metadata"
        | "read_project_artifact"
        | "artifact_upload_begin"
        | "artifact_upload_chunk"
        | "artifact_upload_finish"
        | "artifact_upload_abort" => {
            if body.content.is_none() {
                return Err(format!("content is required for op={}", body.op));
            }
            if body.old_text.is_some()
                || body.pattern.is_some()
                || body.expected_sha256.is_some()
                || body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
                || body.max_bytes.is_some()
                || body.create_dirs
            {
                return Err(format!("{} only accepts path/content", body.op));
            }
        }
        "checkpoint_create" | "checkpoint_restore" => {
            if body.content.is_none() {
                return Err(format!("content is required for op={}", body.op));
            }
            if body.old_text.is_some()
                || body.pattern.is_some()
                || body.expected_sha256.is_some()
                || body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
                || body.max_bytes.is_some()
                || body.create_dirs
            {
                return Err("checkpoint ops only accept path/cwd/content".to_string());
            }
        }
        _ => {
            if body.expected_prefix.is_some()
                || body.start_line.is_some()
                || body.end_line.is_some()
                || body.line.is_some()
                || body.old_text.is_some()
                || body.pattern.is_some()
            {
                return Err(
                    "line/range/anchor edit fields are not supported for this file op".to_string(),
                );
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
    validate_raw_shell_wire_command(&body.command)?;
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

pub(super) fn validate_process_request(
    client_id: &str,
    cwd: Option<&str>,
    process: &ShellProcessArgv,
    stdin: Option<&str>,
    timeout_secs: u64,
    wait_timeout_secs: u64,
) -> Result<(), String> {
    validate_id(client_id, "client_id")?;
    validate_process_argv(process)?;
    if let Some(stdin) = stdin {
        if stdin.len() > PROCESS_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {PROCESS_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > PROCESS_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {PROCESS_CWD_MAX_BYTES} bytes"
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if timeout_secs == 0 || timeout_secs > STRUCTURED_EXECUTION_DIRECT_SYNC_TIMEOUT_MAX_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {STRUCTURED_EXECUTION_DIRECT_SYNC_TIMEOUT_MAX_SECS}"
        ));
    }
    if wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {MAX_SYNC_WAIT_SECS} for synchronous run_process"
        ));
    }
    Ok(())
}

pub(super) fn validate_script_enqueue_request(
    client_id: &str,
    cwd: Option<&str>,
    script: &ShellScriptPayload,
    stdin: Option<&str>,
    timeout_secs: u64,
    wait_timeout_secs: u64,
) -> Result<(), String> {
    validate_id(client_id, "client_id")?;
    validate_script_request(script, stdin, cwd, timeout_secs)?;
    if timeout_secs > STRUCTURED_EXECUTION_DIRECT_SYNC_TIMEOUT_MAX_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {STRUCTURED_EXECUTION_DIRECT_SYNC_TIMEOUT_MAX_SECS}"
        ));
    }
    if wait_timeout_secs > MAX_SYNC_WAIT_SECS {
        return Err(format!(
            "wait_timeout_secs must be <= {MAX_SYNC_WAIT_SECS} for synchronous run_script"
        ));
    }
    Ok(())
}

pub(super) fn trim_string(value: Option<String>) -> Option<String> {
    value
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(super) fn validate_project_summary(project: &RunnerProjectSummary) -> Result<(), &'static str> {
    if project.id.is_empty()
        || project.id.len() > 64
        || !project
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("project_summary_invalid_id");
    }
    if project.path.is_empty() || project.path.len() > 4096 || project.path.contains('\0') {
        return Err("project_summary_invalid_path");
    }
    if project
        .name
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > 120 || value.contains('\0'))
    {
        return Err("project_summary_invalid_name");
    }
    if project
        .description
        .as_deref()
        .is_some_and(|value| value.len() > 500 || value.contains('\0'))
    {
        return Err("project_summary_invalid_description");
    }
    if project
        .kind
        .as_deref()
        .is_some_and(|value| value.len() > 120 || value.contains('\0'))
        || project
            .shell_profile
            .as_deref()
            .is_some_and(|value| value.len() > 120 || value.contains('\0'))
        || project
            .git_branch
            .as_deref()
            .is_some_and(|value| value.len() > 512 || value.contains('\0'))
        || project
            .git_head
            .as_deref()
            .is_some_and(|value| value.len() > 128 || value.contains('\0'))
    {
        return Err("project_summary_invalid_metadata");
    }
    if project.hooks.len() > 64
        || project
            .hooks
            .iter()
            .any(|hook| hook.is_empty() || hook.len() > 120 || hook.contains('\0'))
    {
        return Err("project_summary_invalid_hooks");
    }
    if project.revision.as_deref().is_some_and(|revision| {
        let Some(hex) = revision.strip_prefix("sha256:") else {
            return true;
        };
        hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit())
    }) {
        return Err("project_summary_invalid_revision");
    }
    Ok(())
}

pub(super) fn validate_project_summary_batch(
    projects: &[RunnerProjectSummary],
) -> Result<usize, &'static str> {
    for project in projects {
        validate_project_summary(project)?;
    }
    let serialized_bytes = serde_json::to_vec(projects)
        .map_err(|_| "project_inventory_serialization_failed")?
        .len();
    if serialized_bytes > PROJECT_INVENTORY_SNAPSHOT_MAX_SERIALIZED_BYTES {
        return Err("project_inventory_snapshot_too_large");
    }
    let unique = projects
        .iter()
        .map(|project| &project.id)
        .collect::<std::collections::HashSet<_>>();
    if unique.len() != projects.len() {
        return Err("project_inventory_duplicate_project_id");
    }
    Ok(serialized_bytes)
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

#[cfg(test)]
mod provider_status_tests {
    use super::*;
    use std::collections::BTreeMap;
    use webcodex_core::runner_protocol::ClaudeCodeProviderStatus;

    fn provider_status() -> ToolProvidersStatus {
        ToolProvidersStatus {
            strategy: "claude_code_then_native".to_string(),
            claude_code: ClaudeCodeProviderStatus {
                enabled: true,
                version: Some("2.1.217".to_string()),
                available: true,
                process_state: "running".to_string(),
                discovered_tool_names: (0..100).map(|index| format!("Tool_{index}")).collect(),
                capabilities: BTreeMap::from([
                    ("edit_file".to_string(), "available".to_string()),
                    ("search_project_text".to_string(), "unmapped".to_string()),
                ]),
                last_error_code: None,
                last_call: Some(ProviderCallSummary {
                    capability: "edit_file".to_string(),
                    selected_provider: "claude_code".to_string(),
                    fallback_used: false,
                    result: "success".to_string(),
                    write_state: Some("confirmed".to_string()),
                    duration_ms: u64::MAX,
                    error_code: None,
                }),
            },
            config_reload: Default::default(),
        }
    }

    #[test]
    fn provider_status_is_bounded_and_rejects_path_like_version() {
        let mut status = provider_status();
        status.claude_code.version = Some("/tmp/private/project".to_string());
        status
            .claude_code
            .discovered_tool_names
            .push("/tmp/private/Edit".to_string());
        let status = normalize_tool_providers(Some(status)).unwrap();
        assert_eq!(status.claude_code.version, None);
        assert_eq!(status.claude_code.discovered_tool_names.len(), 64);
        assert!(status
            .claude_code
            .discovered_tool_names
            .iter()
            .all(|name| name.chars().count() <= MAX_PROVIDER_TEXT_CHARS));
        assert_eq!(
            status.claude_code.last_call.as_ref().unwrap().duration_ms,
            24 * 60 * 60 * 1000
        );
        let serialized = serde_json::to_string(&status).unwrap();
        for forbidden in ["/tmp/private", "stderr", "environment", "token", "cookie"] {
            assert!(!serialized.contains(forbidden));
        }
    }

    #[test]
    fn unknown_provider_state_is_ignored_without_error() {
        let mut status = provider_status();
        status.claude_code.process_state = "raw stderr follows".to_string();
        assert!(normalize_tool_providers(Some(status)).is_none());
    }

    #[test]
    fn config_reload_status_is_whitelisted_sorted_and_bounded() {
        let status = normalize_config_reload(Some(RunnerConfigReloadStatus {
            generation: 3,
            last_reload_result: "partial".to_string(),
            last_reload_error_code: None,
            last_reload_error_field: Some("not.safe".to_string()),
            last_reload_error_reason: Some("raw_detail".to_string()),
            restart_required: false,
            restart_required_fields: vec![
                "transport".to_string(),
                "/private/project".to_string(),
                "token".to_string(),
                "project_registry_dir".to_string(),
                "temporary_projects_root".to_string(),
                "transport".to_string(),
            ],
        }))
        .unwrap();
        assert!(status.restart_required);
        assert_eq!(
            status.restart_required_fields,
            ["project_registry_dir", "token", "transport"]
        );
        assert!(status.last_reload_error_field.is_none());
        assert!(status.last_reload_error_reason.is_none());

        let diagnostic = normalize_config_reload(Some(RunnerConfigReloadStatus {
            generation: 4,
            last_reload_result: "failure".to_string(),
            last_reload_error_code: Some("config_validation_failed".to_string()),
            last_reload_error_field: Some("max_concurrent_jobs".to_string()),
            last_reload_error_reason: Some("out_of_range".to_string()),
            restart_required: false,
            restart_required_fields: vec![],
        }))
        .unwrap();
        assert_eq!(
            diagnostic.last_reload_error_field.as_deref(),
            Some("max_concurrent_jobs")
        );
        assert_eq!(
            diagnostic.last_reload_error_reason.as_deref(),
            Some("out_of_range")
        );
        assert!(normalize_config_reload(Some(RunnerConfigReloadStatus {
            last_reload_result: "raw error follows".to_string(),
            ..RunnerConfigReloadStatus::default()
        }))
        .is_none());
    }
}
