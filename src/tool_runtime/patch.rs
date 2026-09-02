use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use super::helpers::{bounded_tail, decode_git_quoted_path};
use super::shell::{agent_command_lifecycle, dispatch_uncertainty_lifecycle};
use super::tool_result::ToolResult;
use super::ToolRuntime;
use crate::shell_protocol::{ShellCommandExecutionState, ShellRunRequest, ShellRunResponse};

pub(crate) const MAX_UNIFIED_DIFF_BYTES: usize = 256 * 1024;
const MAX_UNIFIED_DIFF_AFFECTED_FILES: usize = 128;
const MAX_UNIFIED_DIFF_WARNINGS: usize = 32;
const UNIFIED_DIFF_STDERR_MAX_CHARS: usize = 4096;
const UNIFIED_DIFF_COMMAND_TIMEOUT_SECS: u64 = 60;
const UNIFIED_DIFF_WAIT_TIMEOUT_SECS: u64 = 62;
const UNIFIED_DIFF_SERVER_WAIT_SECS: u64 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnifiedDiffAnalysis {
    pub(crate) affected_files: Vec<String>,
    pub(crate) affected_files_truncated: bool,
    pub(crate) warnings: Vec<String>,
    pub(crate) warnings_truncated: bool,
    pub(crate) has_sensitive_paths: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnifiedDiffInputError {
    pub(crate) error_kind: &'static str,
    pub(crate) recovery_action: &'static str,
    pub(crate) expected_format: Option<&'static str>,
    pub(crate) message: String,
}

fn unsupported_diff_format(message: impl Into<String>) -> UnifiedDiffInputError {
    UnifiedDiffInputError {
        error_kind: "unsupported_diff_format",
        recovery_action: "regenerate_unified_diff",
        expected_format: Some("unified_diff"),
        message: message.into(),
    }
}

fn invalid_unified_diff(
    message: impl Into<String>,
    recovery_action: &'static str,
) -> UnifiedDiffInputError {
    UnifiedDiffInputError {
        error_kind: "invalid_unified_diff",
        recovery_action,
        expected_format: Some("unified_diff"),
        message: message.into(),
    }
}

pub(crate) fn looks_like_codex_apply_patch_wrapper(diff: &str) -> bool {
    diff.lines().any(|line| {
        let line = line.trim_end();
        line == "*** Begin Patch"
            || line == "*** End Patch"
            || line.starts_with("*** Update File:")
            || line.starts_with("*** Add File:")
            || line.starts_with("*** Delete File:")
    })
}

fn take_git_path_token(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();
    if input.is_empty() {
        return None;
    }
    if input.starts_with('"') {
        let bytes = input.as_bytes();
        let mut index = 1usize;
        while index < bytes.len() {
            match bytes[index] {
                b'\\' => index = index.saturating_add(2),
                b'"' => {
                    let token = &input[..=index];
                    return Some((token, input[index + 1..].trim_start()));
                }
                _ => index += 1,
            }
        }
        return None;
    }
    match input.find(char::is_whitespace) {
        Some(index) => Some((&input[..index], input[index..].trim_start())),
        None => Some((input, "")),
    }
}

fn is_cross_platform_absolute_path(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with('\\') || Path::new(path).is_absolute() {
        return true;
    }
    let bytes = path.as_bytes();
    bytes.first().is_some_and(u8::is_ascii_alphabetic)
        && bytes.get(1) == Some(&b':')
        && bytes
            .get(2)
            .is_some_and(|byte| matches!(*byte, b'\\' | b'/'))
}

fn normalize_decoded_diff_path(
    decoded: &str,
    strip_git_prefix: bool,
) -> Result<Option<String>, UnifiedDiffInputError> {
    if decoded == "/dev/null" {
        return Ok(None);
    }
    let path = if strip_git_prefix {
        decoded
            .strip_prefix("a/")
            .or_else(|| decoded.strip_prefix("b/"))
            .unwrap_or(decoded)
    } else {
        decoded
    };
    if path.is_empty() || path == "." {
        return Err(invalid_unified_diff(
            "Unified diff path cannot be empty or '.'",
            "fix_diff_paths",
        ));
    }
    if path.contains('\0') {
        return Err(invalid_unified_diff(
            "Unified diff path cannot contain NUL bytes",
            "fix_diff_paths",
        ));
    }
    if is_cross_platform_absolute_path(path) {
        return Err(invalid_unified_diff(
            format!("Unified diff path must be project-relative: {path}"),
            "fix_diff_paths",
        ));
    }
    if path.split(['/', '\\']).any(|component| component == "..") {
        return Err(invalid_unified_diff(
            format!("Unified diff path cannot contain parent traversal: {path}"),
            "fix_diff_paths",
        ));
    }
    Ok(Some(path.to_string()))
}

fn normalize_diff_path(raw: &str) -> Result<Option<String>, UnifiedDiffInputError> {
    let decoded = decode_git_quoted_path(raw).ok_or_else(|| {
        invalid_unified_diff(
            "Unified diff contains an invalid or unsupported quoted path",
            "regenerate_unified_diff",
        )
    })?;
    normalize_decoded_diff_path(&decoded, true)
}

fn normalize_extended_header_path(raw: &str) -> Result<Option<String>, UnifiedDiffInputError> {
    let raw = raw.strip_suffix('\r').unwrap_or(raw);
    let decoded = if raw.starts_with('"') {
        decode_git_quoted_path(raw).ok_or_else(|| {
            invalid_unified_diff(
                "Unified diff contains an invalid or unsupported quoted extended-header path",
                "regenerate_unified_diff",
            )
        })?
    } else {
        raw.to_string()
    };
    normalize_decoded_diff_path(&decoded, false)
}

fn sensitive_path_warning(path: &str) -> Option<String> {
    let mut sensitive = None;
    for component in path.split(['/', '\\']) {
        let lower = component.to_ascii_lowercase();
        let is_sensitive = matches!(
            lower.as_str(),
            "runner.toml"
                | "agent.toml"
                | "webcodex.env"
                | "secret.pem"
                | "id_rsa"
                | "project-registry"
                | "projects.d"
                | ".git"
                | "target"
                | "node_modules"
        ) || lower == ".env"
            || lower.starts_with(".env.");
        if is_sensitive {
            sensitive = Some(format!(
                "unified diff touches sensitive path component '{component}': {path}"
            ));
            break;
        }
    }
    sensitive
}

pub(crate) fn sensitive_path_warnings(path: &str) -> Vec<String> {
    sensitive_path_warning(path).into_iter().collect()
}

fn insert_normalized_path(
    paths: &mut BTreeSet<String>,
    raw: &str,
) -> Result<(), UnifiedDiffInputError> {
    if let Some(path) = normalize_diff_path(raw)? {
        paths.insert(path);
    }
    Ok(())
}

fn insert_extended_header_path(
    paths: &mut BTreeSet<String>,
    raw: &str,
) -> Result<(), UnifiedDiffInputError> {
    if raw.is_empty() {
        return Err(invalid_unified_diff(
            "Unified diff extended header path cannot be empty",
            "fix_diff_paths",
        ));
    }
    if let Some(path) = normalize_extended_header_path(raw)? {
        paths.insert(path);
    }
    Ok(())
}

fn parse_diff_git_paths(
    line: &str,
    paths: &mut BTreeSet<String>,
) -> Result<(), UnifiedDiffInputError> {
    let rest = line.strip_prefix("diff --git ").ok_or_else(|| {
        invalid_unified_diff("Invalid diff --git header", "regenerate_unified_diff")
    })?;
    let (left, rest) = take_git_path_token(rest).ok_or_else(|| {
        invalid_unified_diff("Invalid diff --git source path", "regenerate_unified_diff")
    })?;
    let (right, trailing) = take_git_path_token(rest).ok_or_else(|| {
        invalid_unified_diff(
            "Invalid diff --git destination path",
            "regenerate_unified_diff",
        )
    })?;
    if !trailing.trim().is_empty() {
        return Err(invalid_unified_diff(
            "Invalid trailing data in diff --git header",
            "regenerate_unified_diff",
        ));
    }
    insert_normalized_path(paths, left)?;
    insert_normalized_path(paths, right)?;
    Ok(())
}

fn diff_file_header_path(raw: &str) -> &str {
    raw.split_once('\t').map_or(raw, |(path, _timestamp)| path)
}

fn parse_extended_header_path(
    line: &str,
    paths: &mut BTreeSet<String>,
) -> Result<bool, UnifiedDiffInputError> {
    for prefix in ["rename from ", "rename to ", "copy from ", "copy to "] {
        if let Some(raw) = line.strip_prefix(prefix) {
            insert_extended_header_path(paths, raw)?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_hunk_range_count(token: &str, prefix: char) -> Option<usize> {
    let range = token.strip_prefix(prefix)?;
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start, count.parse::<usize>().ok()?),
        None => (range, 1),
    };
    start.parse::<usize>().ok()?;
    Some(count)
}

fn parse_unified_hunk_counts(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@ ")?;
    let (ranges, _context) = rest.split_once(" @@")?;
    let mut tokens = ranges.split_whitespace();
    let old_count = parse_hunk_range_count(tokens.next()?, '-')?;
    let new_count = parse_hunk_range_count(tokens.next()?, '+')?;
    if tokens.next().is_some() {
        return None;
    }
    Some((old_count, new_count))
}

fn consume_unified_hunk_line(
    line: &str,
    old_remaining: &mut usize,
    new_remaining: &mut usize,
) -> Result<(), UnifiedDiffInputError> {
    if line.starts_with('\\') {
        return Ok(());
    }
    match line.as_bytes().first().copied() {
        Some(b' ') if *old_remaining > 0 && *new_remaining > 0 => {
            *old_remaining -= 1;
            *new_remaining -= 1;
        }
        Some(b'-') if *old_remaining > 0 => *old_remaining -= 1,
        Some(b'+') if *new_remaining > 0 => *new_remaining -= 1,
        _ => {
            return Err(invalid_unified_diff(
                "Unified diff hunk body does not match its declared line counts",
                "regenerate_unified_diff",
            ))
        }
    }
    Ok(())
}

pub(crate) fn analyze_unified_diff(
    diff: &str,
) -> Result<UnifiedDiffAnalysis, UnifiedDiffInputError> {
    if diff.is_empty() {
        return Err(invalid_unified_diff(
            "Unified diff cannot be empty",
            "regenerate_unified_diff",
        ));
    }
    if diff.contains('\0') {
        return Err(invalid_unified_diff(
            "Unified diff cannot contain NUL bytes",
            "regenerate_unified_diff",
        ));
    }
    if diff.len() > MAX_UNIFIED_DIFF_BYTES {
        return Err(UnifiedDiffInputError {
            error_kind: "diff_too_large",
            recovery_action: "split_unified_diff",
            expected_format: Some("unified_diff"),
            message: format!(
                "Unified diff is {} bytes; maximum is {} bytes",
                diff.len(),
                MAX_UNIFIED_DIFF_BYTES
            ),
        });
    }
    if looks_like_codex_apply_patch_wrapper(diff) {
        return Err(unsupported_diff_format(
            "Codex apply_patch wrapper syntax is not accepted. Regenerate a raw standard unified diff without *** Begin Patch / *** Update File markers.",
        ));
    }

    let lines: Vec<&str> = diff.lines().collect();
    let mut paths = BTreeSet::new();
    let mut hunk_remaining: Option<(usize, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some((old_remaining, new_remaining)) = hunk_remaining.as_mut() {
            consume_unified_hunk_line(line, old_remaining, new_remaining)?;
            if *old_remaining == 0 && *new_remaining == 0 {
                hunk_remaining = None;
            }
            continue;
        }
        if line.starts_with("diff --git ") {
            parse_diff_git_paths(line, &mut paths)?;
            continue;
        }
        if parse_extended_header_path(line, &mut paths)? {
            continue;
        }
        if line.starts_with("@@ ") {
            let counts = parse_unified_hunk_counts(line).ok_or_else(|| {
                invalid_unified_diff(
                    "Unified diff contains an invalid hunk header",
                    "regenerate_unified_diff",
                )
            })?;
            if counts != (0, 0) {
                hunk_remaining = Some(counts);
            }
            continue;
        }
        if let Some(old_path) = line.strip_prefix("--- ") {
            if let Some(new_path) = lines
                .get(index + 1)
                .and_then(|next| next.strip_prefix("+++ "))
            {
                insert_normalized_path(&mut paths, diff_file_header_path(old_path))?;
                insert_normalized_path(&mut paths, diff_file_header_path(new_path))?;
            }
        }
    }
    if hunk_remaining.is_some() {
        return Err(invalid_unified_diff(
            "Unified diff ended before a hunk's declared line counts were satisfied",
            "regenerate_unified_diff",
        ));
    }
    if paths.is_empty() {
        return Err(invalid_unified_diff(
            "Unified diff does not declare any project file paths",
            "regenerate_unified_diff",
        ));
    }

    let all_paths: Vec<String> = paths.into_iter().collect();
    let mut all_warnings = all_paths
        .iter()
        .filter_map(|path| sensitive_path_warning(path))
        .collect::<Vec<_>>();
    all_warnings.sort();
    all_warnings.dedup();
    let has_sensitive_paths = !all_warnings.is_empty();
    let affected_files_truncated = all_paths.len() > MAX_UNIFIED_DIFF_AFFECTED_FILES;
    let warnings_truncated = all_warnings.len() > MAX_UNIFIED_DIFF_WARNINGS;

    Ok(UnifiedDiffAnalysis {
        affected_files: all_paths
            .into_iter()
            .take(MAX_UNIFIED_DIFF_AFFECTED_FILES)
            .collect(),
        affected_files_truncated,
        warnings: all_warnings
            .into_iter()
            .take(MAX_UNIFIED_DIFF_WARNINGS)
            .collect(),
        warnings_truncated,
        has_sensitive_paths,
    })
}

fn bounded_stderr(stderr: Option<String>) -> (Value, bool) {
    match stderr {
        Some(stderr) if !stderr.is_empty() => {
            let (tail, truncated) = bounded_tail(&stderr, UNIFIED_DIFF_STDERR_MAX_CHARS);
            (Value::String(tail), truncated)
        }
        _ => (Value::Null, false),
    }
}

#[allow(clippy::too_many_arguments)]
fn unified_diff_output(
    analysis: Option<&UnifiedDiffAnalysis>,
    applied: Option<bool>,
    can_apply: Option<bool>,
    policy_blocked: bool,
    state_changed: Option<bool>,
    execution_state: &'static str,
    stderr: Option<String>,
    error_kind: Option<&'static str>,
    expected_format: Option<&'static str>,
    recovery_action: Option<&'static str>,
) -> Value {
    let (stderr, stderr_truncated) = bounded_stderr(stderr);
    json!({
        "applied": applied,
        "can_apply": can_apply,
        "policy_blocked": policy_blocked,
        "state_changed": state_changed,
        "execution_state": execution_state,
        "affected_files": analysis.map(|value| value.affected_files.clone()).unwrap_or_default(),
        "affected_files_truncated": analysis.is_some_and(|value| value.affected_files_truncated),
        "warnings": analysis.map(|value| value.warnings.clone()).unwrap_or_default(),
        "warnings_truncated": analysis.is_some_and(|value| value.warnings_truncated),
        "stderr": stderr,
        "stderr_truncated": stderr_truncated,
        "error_kind": error_kind,
        "expected_format": expected_format,
        "recovery_action": recovery_action,
    })
}

fn input_rejection(error: UnifiedDiffInputError) -> ToolResult {
    ToolResult::err_with_output(
        error.message,
        unified_diff_output(
            None,
            Some(false),
            None,
            false,
            Some(false),
            "not_started",
            None,
            Some(error.error_kind),
            error.expected_format,
            Some(error.recovery_action),
        ),
    )
}

fn pre_apply_rejection(
    message: impl Into<String>,
    analysis: &UnifiedDiffAnalysis,
    error_kind: &'static str,
    recovery_action: &'static str,
) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        unified_diff_output(
            Some(analysis),
            Some(false),
            None,
            false,
            Some(false),
            "not_started",
            None,
            Some(error_kind),
            None,
            Some(recovery_action),
        ),
    )
}

struct UnifiedDiffCommandFailure {
    message: String,
    execution_state: ShellCommandExecutionState,
}

impl ToolRuntime {
    async fn run_unified_diff_command(
        &self,
        client_id: String,
        cwd: String,
        command: &'static str,
        diff: String,
    ) -> Result<ShellRunResponse, UnifiedDiffCommandFailure> {
        let (request_id, receiver) = self
            .shell_clients
            .enqueue_run(
                ShellRunRequest {
                    client_id,
                    cwd: Some(cwd),
                    command: command.to_string(),
                    stdin: Some(diff),
                    timeout_secs: UNIFIED_DIFF_COMMAND_TIMEOUT_SECS,
                    wait_timeout_secs: UNIFIED_DIFF_WAIT_TIMEOUT_SECS,
                },
                "tool_runtime".to_string(),
            )
            .await
            .map_err(|error| UnifiedDiffCommandFailure {
                message: error,
                execution_state: ShellCommandExecutionState::NotStarted,
            })?;

        match tokio::time::timeout(Duration::from_secs(UNIFIED_DIFF_SERVER_WAIT_SECS), receiver)
            .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                let dispatch = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                Err(UnifiedDiffCommandFailure {
                    message:
                        "Runner response channel closed before a trustworthy result was received"
                            .to_string(),
                    execution_state: dispatch_uncertainty_lifecycle(dispatch),
                })
            }
            Err(_) => {
                let dispatch = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                Err(UnifiedDiffCommandFailure {
                    message: "Timed out waiting for the Runner command result".to_string(),
                    execution_state: dispatch_uncertainty_lifecycle(dispatch),
                })
            }
        }
    }

    pub(crate) async fn apply_unified_diff(
        &self,
        project: String,
        diff: String,
        deny_sensitive_paths: Option<bool>,
    ) -> ToolResult {
        let analysis = match analyze_unified_diff(&diff) {
            Ok(analysis) => analysis,
            Err(error) => return input_rejection(error),
        };

        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => {
                return pre_apply_rejection(error, &analysis, "project_unavailable", "fix_project")
            }
        };
        if !proj.is_agent() {
            return pre_apply_rejection(
                "apply_unified_diff requires an agent-registered project; server-configured projects are not supported",
                &analysis,
                "project_not_agent_registered",
                "fix_project",
            );
        }
        if !proj.allow_patch() {
            return pre_apply_rejection(
                "Unified diff mutation is not allowed for this project",
                &analysis,
                "patch_not_allowed",
                "user_action",
            );
        }
        if deny_sensitive_paths.unwrap_or(true) && analysis.has_sensitive_paths {
            return ToolResult::ok(unified_diff_output(
                Some(&analysis),
                Some(false),
                Some(false),
                true,
                Some(false),
                "not_started",
                None,
                Some("policy_blocked"),
                None,
                Some("review_sensitive_paths"),
            ));
        }

        let client_id = match proj.agent_client_id() {
            Ok(client_id) => client_id.to_string(),
            Err(error) => {
                return pre_apply_rejection(error, &analysis, "project_unavailable", "fix_project")
            }
        };

        let check_response = match self
            .run_unified_diff_command(
                client_id.clone(),
                proj.path.clone(),
                "git apply --check -",
                diff.clone(),
            )
            .await
        {
            Ok(response) => response,
            Err(failure) => {
                return ToolResult::err_with_output(
                    format!(
                        "Unified diff preflight did not produce a trustworthy result: {}. No apply command was dispatched; retrying the same request is safe after the Runner is healthy.",
                        failure.message
                    ),
                    unified_diff_output(
                        Some(&analysis),
                        Some(false),
                        None,
                        false,
                        Some(false),
                        "not_started",
                        None,
                        Some("preflight_failed"),
                        None,
                        Some("retry_same"),
                    ),
                )
            }
        };
        let check_state =
            agent_command_lifecycle(&check_response, UNIFIED_DIFF_COMMAND_TIMEOUT_SECS);
        if check_state != ShellCommandExecutionState::Completed || check_response.error.is_some() {
            return ToolResult::err_with_output(
                "Unified diff preflight did not complete reliably. No apply command was dispatched; retrying the same request is safe after the Runner is healthy.",
                unified_diff_output(
                    Some(&analysis),
                    Some(false),
                    None,
                    false,
                    Some(false),
                    "not_started",
                    check_response.stderr,
                    Some("preflight_failed"),
                    None,
                    Some("retry_same"),
                ),
            );
        }
        if check_response.exit_code != Some(0) {
            return ToolResult::ok(unified_diff_output(
                Some(&analysis),
                Some(false),
                Some(false),
                false,
                Some(false),
                "completed",
                check_response.stderr,
                Some("not_applicable"),
                None,
                Some("regenerate_unified_diff"),
            ));
        }

        let apply_response = match self
            .run_unified_diff_command(
                client_id,
                proj.path.clone(),
                "git apply -",
                diff,
            )
            .await
        {
            Ok(response) => response,
            Err(failure) if failure.execution_state == ShellCommandExecutionState::NotStarted => {
                return ToolResult::err_with_output(
                    "Unified diff apply was not dispatched; no mutation from this apply request started. Retrying the same request is safe after the Runner is healthy.",
                    unified_diff_output(
                        Some(&analysis),
                        Some(false),
                        Some(true),
                        false,
                        Some(false),
                        "not_started",
                        None,
                        Some("apply_not_started"),
                        None,
                        Some("retry_same"),
                    ),
                )
            }
            Err(failure) => {
                return ToolResult::err_with_output(
                    format!(
                        "Unified diff apply outcome is unknown: {}. The apply request may already have changed the worktree. Inspect current workspace state before deciding whether to retry.",
                        failure.message
                    ),
                    unified_diff_output(
                        Some(&analysis),
                        None,
                        Some(true),
                        false,
                        None,
                        "outcome_unknown",
                        None,
                        Some("outcome_unknown"),
                        None,
                        Some("inspect_workspace_before_retry"),
                    ),
                )
            }
        };
        let apply_state =
            agent_command_lifecycle(&apply_response, UNIFIED_DIFF_COMMAND_TIMEOUT_SECS);
        if apply_state == ShellCommandExecutionState::NotStarted {
            return ToolResult::err_with_output(
                "Unified diff apply was not started by the Runner; no mutation from this apply request occurred. Retrying the same request is safe.",
                unified_diff_output(
                    Some(&analysis),
                    Some(false),
                    Some(true),
                    false,
                    Some(false),
                    "not_started",
                    apply_response.stderr,
                    Some("apply_not_started"),
                    None,
                    Some("retry_same"),
                ),
            );
        }
        if apply_state != ShellCommandExecutionState::Completed || apply_response.error.is_some() {
            return ToolResult::err_with_output(
                "Unified diff apply outcome is unknown after dispatch. The worktree may already have changed; inspect current workspace state before deciding whether to retry.",
                unified_diff_output(
                    Some(&analysis),
                    None,
                    Some(true),
                    false,
                    None,
                    "outcome_unknown",
                    apply_response.stderr,
                    Some("outcome_unknown"),
                    None,
                    Some("inspect_workspace_before_retry"),
                ),
            );
        }
        if apply_response.exit_code != Some(0) {
            return ToolResult::err_with_output(
                "git apply completed with a non-zero exit after a successful preflight. The requested diff was not confirmed applied; inspect current workspace state before retrying because post-dispatch mutation state is not inferred.",
                unified_diff_output(
                    Some(&analysis),
                    Some(false),
                    Some(true),
                    false,
                    None,
                    "completed",
                    apply_response.stderr,
                    Some("apply_failed"),
                    None,
                    Some("inspect_workspace_before_retry"),
                ),
            );
        }

        ToolResult::ok(unified_diff_output(
            Some(&analysis),
            Some(true),
            Some(true),
            false,
            Some(true),
            "completed",
            apply_response.stderr,
            None,
            None,
            None,
        ))
    }
}
