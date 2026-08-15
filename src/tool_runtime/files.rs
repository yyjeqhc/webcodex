use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;
use tokio::time::Instant;
use webcodex_workspace::file_read_range::{self, EffectiveRange, FileReadRange, ReadFileReason};

#[cfg(test)]
use super::helpers::run_command_sync;
use super::helpers::{
    looks_like_command_timeout, run_command_sync_bounded, shell_escape_simple, shell_join_paths,
    validate_limited_cleanup_paths, validate_project_relative_path, LocalRunFailure,
};
use super::project_resolution::ResolvedProject;
use super::shell::{agent_command_lifecycle, dispatch_uncertainty_lifecycle};
use super::tool_inputs::{
    ApplyFileChangeInput, ApplyFileChangeKind, ApplyTextEditInput, ApplyTextEditKind,
};
use super::tool_result::ToolResult;
use super::{SearchResultMode, ToolRuntime};
use crate::artifact_policy::{
    has_safe_octet_stream_artifact_extension, octet_stream_safe_extension_error,
    ooxml_extension_for_mime, MAX_MCP_IMAGE_BYTES,
};
use crate::auth::AuthContext;
use crate::project_overview::{
    effective_project_overview_limit, effective_project_overview_max_depth,
    normalize_project_overview_path,
};
use crate::projects::ProjectConfig;
use crate::shell_protocol::{
    ShellCommandExecutionState, ShellFileOpRequest, ShellRunRequest, ShellRunResponse,
    EXTERNAL_SEARCH_REQUEST_PREFIX,
};

#[cfg(test)]
pub(crate) fn read_file_content_result(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    read_file_content_result_with_options(content, start_line, limit, false)
}

#[cfg(test)]
pub(crate) fn read_file_content_result_with_options(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
    with_line_numbers: bool,
) -> ToolResult {
    // The local path previously kept the entire file body in `content` and then
    // sliced it. It now feeds the bytes through the shared streaming range
    // reader so only the selected window is retained, and the model output is
    // built by the shared normalizer shared with the agent path.
    let range = EffectiveRange::new(start_line, limit);
    match file_read_range::read_range_from(content.as_bytes(), range) {
        Ok(result) => build_read_file_success(&result, with_line_numbers, None),
        Err(error) => read_file_failure(error.reason, None),
    }
}

#[cfg(test)]
pub(crate) fn read_file_agent_stdout_result(
    stdout: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    read_file_agent_stdout_result_with_options(stdout, start_line, limit, false)
}

pub(crate) fn read_file_agent_stdout_result_with_options(
    stdout: String,
    start_line: Option<usize>,
    limit: Option<usize>,
    with_line_numbers: bool,
) -> ToolResult {
    // The runner stdout is untrusted input. The shared v1 envelope is accepted,
    // but every formal field is strictly validated and the model output is
    // reconstructed from those fields alone — no envelope extras, padding, or
    // absolute paths are passed through.
    match parse_agent_file_read_range(&stdout, start_line, limit) {
        Ok(result) => build_read_file_success(&result, with_line_numbers, None),
        Err(reason) => read_file_failure(reason, None),
    }
}

pub(crate) fn effective_read_file_range(
    start_line: Option<usize>,
    limit: Option<usize>,
) -> (usize, usize, usize) {
    let range = EffectiveRange::new(start_line, limit);
    (range.start_line, range.limit, range.end_line())
}

/// Build the unified `read_file` success [`ToolResult`] from a shared range
/// result, enforcing the final serialized-output hard limit after JSON
/// escaping and (optional) line numbering. The model output is reconstructed
/// from the canonical fields only; no agent envelope extras survive.
///
/// `path` is the project-relative input path to attach for model navigation.
fn build_read_file_success(
    result: &FileReadRange,
    with_line_numbers: bool,
    path: Option<&str>,
) -> ToolResult {
    let mut output =
        webcodex_workspace::file_read_normalize::success_output(result, with_line_numbers);
    if let Some(path) = path {
        output["path"] = json!(path);
    }
    // Re-check the hard serialized limit after numbering and JSON escaping.
    // The content budget already bounds the raw range, but a heavily escaped or
    // numbered payload could still grow; fail closed rather than truncate.
    if !webcodex_workspace::file_read_normalize::serialized_fits(&output) {
        return read_file_failure(ReadFileReason::RangeTooLarge, path);
    }
    ToolResult::ok(output)
}

/// Stable, schema-backed failure output for `read_file`. Carries only the
/// project-relative input path and a stable reason code — never absolute paths,
/// raw OS error text, or runner stdout/stderr.
fn read_file_failure(reason: ReadFileReason, path: Option<&str>) -> ToolResult {
    let output = json!({
        "error_kind": "read_file_failed",
        "reason_code": reason.as_str(),
        "path": path.unwrap_or(""),
        "state_changed": false,
    });
    ToolResult::err_with_output(format!("read_file failed: {}", reason.as_str()), output)
}

fn validate_read_file_path(path: &str) -> Option<ToolResult> {
    if validate_project_relative_path(path).is_err() {
        return Some(read_file_failure(ReadFileReason::InvalidPath, Some(path)));
    }
    if crate::sensitive_paths::is_secret_path(path) {
        return Some(read_file_failure(ReadFileReason::SensitivePath, Some(path)));
    }
    None
}

fn io_error_reason(error: &std::io::Error) -> ReadFileReason {
    match error.kind() {
        std::io::ErrorKind::NotFound => ReadFileReason::NotFound,
        std::io::ErrorKind::PermissionDenied => ReadFileReason::PermissionDenied,
        _ => ReadFileReason::IoError,
    }
}

/// Map a non-zero agent execution response to a stable reason code. Current
/// Runners emit `read_file failed: <reason_code>`; only a narrow set of legacy
/// path-free phrases is retained for rolling upgrades. Unrecognized text fails
/// closed as `io_error` and is never returned to the model.
fn map_agent_read_error(resp: &ShellRunResponse) -> ReadFileReason {
    for text in [resp.error.as_deref(), resp.stderr.as_deref()]
        .into_iter()
        .flatten()
    {
        for line in text.lines() {
            if let Some(code) = line.trim().strip_prefix("read_file failed: ") {
                if let Some(reason) = ReadFileReason::from_code(code.trim()) {
                    return reason;
                }
            }
        }
    }

    let text = resp
        .error
        .as_deref()
        .or(resp.stderr.as_deref())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    match text.as_str() {
        "file_read target not found" | "file_read target is unavailable" => {
            ReadFileReason::NotFound
        }
        "file_read permission denied" | "permission denied" => ReadFileReason::PermissionDenied,
        "file is not valid utf-8" | "file is not valid utf8" => ReadFileReason::InvalidUtf8,
        "range output too large" => ReadFileReason::RangeTooLarge,
        "file_read path escapes project root" | "invalid line range for file_read" => {
            ReadFileReason::InvalidPath
        }
        "file_read target is not a file" | "not a regular file" => ReadFileReason::NotFile,
        "file_read io error" | "file_read project root is unavailable" => ReadFileReason::IoError,
        _ if text.starts_with("range output too large:") => ReadFileReason::RangeTooLarge,
        _ => ReadFileReason::IoError,
    }
}

/// Strictly validate an agent `webcodex.file_read_range.v1` stdout envelope and
/// return a shared [`FileReadRange`] reconstructed from its formal fields alone.
/// Returns a stable [`ReadFileReason`] for any malformed, mistyped, inconsistent,
/// or oversized response so the caller can fail closed without leaking runner
/// internals.
fn parse_agent_file_read_range(
    stdout: &str,
    request_start_line: Option<usize>,
    request_limit: Option<usize>,
) -> Result<FileReadRange, ReadFileReason> {
    let effective = EffectiveRange::new(request_start_line, request_limit);
    let trimmed = stdout.trim();
    let value = serde_json::from_str::<Value>(trimmed)
        .map_err(|_| ReadFileReason::MalformedAgentResponse)?;
    if value.get("format").and_then(|f| f.as_str()) != Some("webcodex.file_read_range.v1") {
        return Err(ReadFileReason::MalformedAgentResponse);
    }

    let content = value
        .get("content")
        .and_then(|c| c.as_str())
        .ok_or(ReadFileReason::MalformedAgentResponse)?
        .to_string();
    let sha256 = value
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|s| file_read_range::is_valid_sha256_hex(s))
        .ok_or(ReadFileReason::MalformedAgentResponse)?
        .to_string();
    let total_lines = value
        .get("total_lines")
        .and_then(Value::as_u64)
        .filter(|t| *t <= usize::MAX as u64)
        .map(|t| t as usize)
        .ok_or(ReadFileReason::MalformedAgentResponse)?;
    let resp_start_line = value
        .get("start_line")
        .and_then(Value::as_u64)
        .filter(|l| *l >= 1 && *l <= usize::MAX as u64)
        .map(|l| l as usize)
        .ok_or(ReadFileReason::MalformedAgentResponse)?;
    let resp_limit = value
        .get("limit")
        .and_then(Value::as_u64)
        .filter(|l| *l >= 1 && *l <= usize::MAX as u64)
        .map(|l| l as usize)
        .ok_or(ReadFileReason::MalformedAgentResponse)?;

    // The response must match the effective request range the server sent.
    if resp_start_line != effective.start_line || resp_limit != effective.limit {
        return Err(ReadFileReason::MalformedAgentResponse);
    }

    // Compute the expected returned line count from the range and total, then
    // verify the content's segment count matches exactly. `split('\n')`
    // preserves empty segments so a single blank line is one segment, not zero.
    let expected_returned = if effective.start_line > total_lines {
        0
    } else {
        total_lines
            .saturating_sub(effective.start_line)
            .saturating_add(1)
            .min(effective.limit)
    };
    let content_segments = content.split('\n').count();
    let content_returned = if content.is_empty() {
        // Disambiguate: empty content is 0 segments for an empty file or
        // overflow, but 1 segment for a single selected blank line.
        if expected_returned == 1 {
            1
        } else {
            0
        }
    } else {
        content_segments
    };
    if content_returned != expected_returned {
        return Err(ReadFileReason::MalformedAgentResponse);
    }

    // Reject oversized agent content before it can reach the model output.
    if content.len() > file_read_range::MAX_RANGE_CONTENT_BYTES {
        return Err(ReadFileReason::RangeTooLarge);
    }

    let end_line = if content_returned > 0 {
        Some(effective.start_line + content_returned - 1)
    } else {
        None
    };
    let has_more = end_line.is_some_and(|end| end < total_lines);
    let next_start_line = if has_more {
        end_line.map(|e| e + 1)
    } else {
        None
    };

    Ok(FileReadRange {
        content,
        sha256,
        total_lines,
        start_line: effective.start_line,
        limit: effective.limit,
        returned_lines: content_returned,
        end_line,
        has_more,
        next_start_line,
    })
}

/// Parse the stdout of a best-effort agent `file_read` for an instruction
/// candidate. Only the canonical `webcodex.file_read_range.v1` JSON envelope
/// is accepted. Empty content is a successfully observed absent rule body;
/// malformed or obsolete output is conservatively unavailable.
fn parse_instruction_agent_stdout(
    stdout: String,
) -> Result<Option<(String, usize, Option<String>)>, ()> {
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if value.get("format").and_then(|format| format.as_str())
                == Some("webcodex.file_read_range.v1")
            {
                let content = value
                    .get("content")
                    .and_then(|c| c.as_str())
                    .ok_or(())?
                    .to_string();
                let total_lines = value
                    .get("total_lines")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;
                if content_is_empty_instruction(&content) {
                    return Ok(None);
                }
                let full_sha256 = value
                    .get("sha256")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Ok(Some((content, total_lines, full_sha256)));
            }
        }
    }
    Err(())
}

/// True when an instruction body carries no meaningful content (empty or
/// whitespace-only). Empty instruction files are skipped so a later candidate
/// can win.
fn content_is_empty_instruction(content: &str) -> bool {
    content.trim().is_empty()
}

enum InstructionCandidateRead {
    Found(super::project_instructions::LoadedInstructionCandidate),
    Missing,
    Unavailable,
}

fn instruction_candidate_missing(error: &Option<String>, stderr: &Option<String>) -> bool {
    error
        .iter()
        .chain(stderr.iter())
        .map(|value| value.to_ascii_lowercase())
        .any(|value| {
            value.contains("no such file")
                || value.contains("not found")
                || value.contains("not_found")
                || value.contains("cannot find the file")
        })
}

// =============================================================================
// Phase A read-only console helpers
// =============================================================================

/// Build the project-relative path for a single entry returned by an agent
/// `file_list` op. `rel_path` is the project-relative directory the caller
/// requested (`"."` for the project root); `name` is the bare entry name.
pub(crate) fn relative_entry_path(rel_path: &str, name: &str) -> String {
    let trimmed = rel_path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        name.to_string()
    } else {
        format!("{}/{}", trimmed, name)
    }
}

/// Parse agent `file_list` stdout (one entry per line, dirs suffixed with
/// `/`) into bounded project-relative entries with a file/dir kind. Returns
/// the entries and whether the source exceeded `max_entries`.
pub(crate) fn parse_file_list_entries(
    stdout: &str,
    rel_path: &str,
    max_entries: usize,
) -> (Vec<Value>, bool) {
    let mut all: Vec<Value> = Vec::new();
    for line in stdout.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let (name, is_dir) = if let Some(stripped) = line.strip_suffix('/') {
            (stripped.to_string(), true)
        } else {
            (line.to_string(), false)
        };
        if name.is_empty() {
            continue;
        }
        all.push(json!({
            "path": relative_entry_path(rel_path, &name),
            "kind": if is_dir { "dir" } else { "file" },
        }));
    }
    all.sort_by(|a, b| {
        a["path"]
            .as_str()
            .unwrap_or("")
            .cmp(b["path"].as_str().unwrap_or(""))
    });
    let truncated = all.len() > max_entries;
    all.truncate(max_entries);
    (all, truncated)
}

const SEARCH_PROJECT_TEXT_EXCLUDES: &[&str] = &[
    "--exclude-dir=.git",
    "--exclude-dir=target",
    "--exclude-dir=node_modules",
    "--exclude-dir=.venv",
    "--exclude-dir=venv",
    "--exclude-dir=__pycache__",
    "--exclude-dir=.pytest_cache",
    "--exclude-dir=.mypy_cache",
    "--exclude-dir=.ruff_cache",
    "--exclude-dir=.tox",
    "--exclude-dir=site-packages",
    "--exclude-dir=.ipynb_checkpoints",
    "--exclude-dir=dist",
    "--exclude-dir=build",
    "--exclude-dir=coverage",
    "--exclude-dir=.next",
    "--exclude-dir=secrets",
    "--exclude-dir=tokens",
    "--exclude=.env",
    "--exclude=.env.*",
    "--exclude=agent.toml",
    "--exclude=webcodex.env",
    "--exclude=*.pem",
    "--exclude=*.key",
];

const SEARCH_PROJECT_TEXT_RG_EXCLUDE_GLOBS: &[&str] = &[
    "!.git/**",
    "!**/.git/**",
    "!target/**",
    "!**/target/**",
    "!node_modules/**",
    "!**/node_modules/**",
    "!**/.venv/**",
    "!**/venv/**",
    "!**/__pycache__/**",
    "!**/.pytest_cache/**",
    "!**/.mypy_cache/**",
    "!**/.ruff_cache/**",
    "!**/.tox/**",
    "!**/site-packages/**",
    "!**/.ipynb_checkpoints/**",
    "!**/dist/**",
    "!**/build/**",
    "!**/coverage/**",
    "!**/.next/**",
    "!secrets/**",
    "!**/secrets/**",
    "!tokens/**",
    "!**/tokens/**",
    "!.env",
    "!**/.env",
    "!.env.*",
    "!**/.env.*",
    "!agent.toml",
    "!**/agent.toml",
    "!webcodex.env",
    "!**/webcodex.env",
    "!*.pem",
    "!**/*.pem",
    "!*.key",
    "!**/*.key",
];

pub(crate) const MAX_SEARCH_CONTEXT_LINES: usize = 20;
pub(crate) const MAX_SEARCH_GLOBS: usize = 32;
pub(crate) const MAX_SEARCH_GLOB_BYTES: usize = 256;
const DEFAULT_SEARCH_TIMEOUT_SECS: u64 = 30;
const MIN_SEARCH_TIMEOUT_SECS: i64 = 1;
const MAX_SEARCH_TIMEOUT_SECS: i64 = 120;

#[derive(Debug, Clone)]
pub(crate) struct SearchRequest {
    pub(crate) pattern: String,
    pub(crate) path: Option<String>,
    pub(crate) limit: Option<usize>,
    pub(crate) context_before: Option<usize>,
    pub(crate) context_after: Option<usize>,
    pub(crate) include_globs: Option<Vec<String>>,
    pub(crate) exclude_globs: Option<Vec<String>>,
    pub(crate) result_mode: Option<SearchResultMode>,
    pub(crate) timeout_secs: Option<i64>,
}

/// Stable structured validation failure for `search_project_text` inputs.
/// Messages must not echo raw pattern/glob values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchValidationError {
    pub field: &'static str,
    pub message: String,
    pub index: Option<usize>,
    pub reason: Option<&'static str>,
}

impl SearchValidationError {
    fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
            index: None,
            reason: None,
        }
    }

    fn with_index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    fn with_reason(mut self, reason: &'static str) -> Self {
        self.reason = Some(reason);
        self
    }

    pub(crate) fn into_tool_result(self) -> ToolResult {
        let mut output = json!({
            "code": "invalid_search_request",
            "field": self.field,
            "message": self.message,
        });
        if let Some(index) = self.index {
            output["index"] = json!(index);
        }
        if let Some(reason) = self.reason {
            output["reason"] = json!(reason);
        }
        ToolResult::err_with_output(self.message, output)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchOptions {
    pub(crate) pattern: String,
    pub(crate) path: String,
    pub(crate) limit: usize,
    pub(crate) context_before: usize,
    pub(crate) context_after: usize,
    pub(crate) include_globs: Vec<String>,
    pub(crate) exclude_globs: Vec<String>,
    pub(crate) result_mode: SearchResultMode,
    pub(crate) timeout_secs: u64,
    requested_features: Vec<String>,
}

impl SearchOptions {
    pub(crate) fn normalize(request: SearchRequest) -> Result<Self, SearchValidationError> {
        if request.pattern.trim().is_empty() {
            return Err(
                SearchValidationError::new("pattern", "pattern cannot be empty")
                    .with_reason("empty"),
            );
        }
        if request.pattern.contains('\0') {
            return Err(
                SearchValidationError::new("pattern", "pattern cannot contain NUL bytes")
                    .with_reason("nul_byte"),
            );
        }
        let path = request
            .path
            .map(|path| path.trim().to_string())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".to_string());
        if let Err(message) = validate_project_relative_path(&path) {
            return Err(SearchValidationError::new("path", message).with_reason("invalid_path"));
        }

        let include_globs = validate_search_globs(
            "include_globs",
            request.include_globs.unwrap_or_default(),
            true,
        )?;
        let exclude_globs = validate_search_globs(
            "exclude_globs",
            request.exclude_globs.unwrap_or_default(),
            false,
        )?;
        let result_mode = request.result_mode.unwrap_or(SearchResultMode::Matches);
        let timeout_secs = request
            .timeout_secs
            .unwrap_or(DEFAULT_SEARCH_TIMEOUT_SECS as i64)
            .clamp(MIN_SEARCH_TIMEOUT_SECS, MAX_SEARCH_TIMEOUT_SECS)
            as u64;
        // Capability features that require ripgrep. Empty glob arrays and
        // timeout_secs are not rg-only capabilities (timeout is runner-owned).
        let mut requested_features = Vec::new();
        if !include_globs.is_empty() {
            requested_features.push("include_globs".to_string());
        }
        if !exclude_globs.is_empty() {
            requested_features.push("exclude_globs".to_string());
        }
        if result_mode != SearchResultMode::Matches {
            requested_features.push(format!("result_mode={}", result_mode.as_str()));
        }

        Ok(Self {
            pattern: request.pattern,
            path,
            limit: request.limit.unwrap_or(50).clamp(1, 200),
            context_before: request
                .context_before
                .unwrap_or(0)
                .min(MAX_SEARCH_CONTEXT_LINES),
            context_after: request
                .context_after
                .unwrap_or(0)
                .min(MAX_SEARCH_CONTEXT_LINES),
            include_globs,
            exclude_globs,
            result_mode,
            timeout_secs,
            requested_features,
        })
    }

    pub(crate) fn requires_ripgrep(&self) -> bool {
        !self.include_globs.is_empty()
            || !self.exclude_globs.is_empty()
            || self.result_mode != SearchResultMode::Matches
    }
}

/// Agent-path timeout layering for `search_project_text`.
///
/// Returns `(command_timeout_secs, wait_timeout_secs, outer_timeout_secs)`.
/// Shell-client validation caps `wait_timeout_secs` at 120, so at the max
/// search budget wait may equal command; the outer tokio wait always stays
/// strictly above the command timeout so agent-reported timeouts can surface.
pub(crate) fn search_agent_timeout_budget(effective_timeout_secs: u64) -> (u64, u64, u64) {
    const MAX_SYNC_WAIT_SECS: u64 = 120;
    let command_timeout = effective_timeout_secs.max(1);
    let wait_timeout = command_timeout.saturating_add(2).min(MAX_SYNC_WAIT_SECS);
    let outer_timeout = command_timeout
        .saturating_add(4)
        .max(wait_timeout.saturating_add(2));
    (command_timeout, wait_timeout, outer_timeout)
}

impl SearchResultMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Matches => "matches",
            Self::FilesWithMatches => "files_with_matches",
            Self::Count => "count",
        }
    }
}

fn validate_search_globs(
    field: &'static str,
    globs: Vec<String>,
    reject_protected: bool,
) -> Result<Vec<String>, SearchValidationError> {
    if globs.len() > MAX_SEARCH_GLOBS {
        return Err(SearchValidationError::new(
            field,
            format!("{field} may contain at most {MAX_SEARCH_GLOBS} entries"),
        )
        .with_reason("too_many"));
    }
    for (index, glob) in globs.iter().enumerate() {
        if glob.is_empty() {
            return Err(SearchValidationError::new(
                field,
                format!("{field} entry cannot be empty"),
            )
            .with_index(index)
            .with_reason("empty"));
        }
        if glob.len() > MAX_SEARCH_GLOB_BYTES {
            return Err(SearchValidationError::new(
                field,
                format!("{field} entry must be at most {MAX_SEARCH_GLOB_BYTES} bytes"),
            )
            .with_index(index)
            .with_reason("too_long"));
        }
        if glob.chars().any(char::is_control) {
            return Err(SearchValidationError::new(
                field,
                format!("{field} entry cannot contain control characters"),
            )
            .with_index(index)
            .with_reason("control_char"));
        }
        if glob.starts_with('!') {
            return Err(SearchValidationError::new(
                field,
                format!("{field} entry cannot start with '!'"),
            )
            .with_index(index)
            .with_reason("negated"));
        }
        if reject_protected && include_glob_explicitly_targets_protected_path(glob) {
            return Err(SearchValidationError::new(
                field,
                format!("{field} entry cannot explicitly include a protected path"),
            )
            .with_index(index)
            .with_reason("protected_path"));
        }
    }
    Ok(globs)
}

fn include_glob_explicitly_targets_protected_path(glob: &str) -> bool {
    crate::sensitive_paths::glob_targets_protected_path(glob)
}

fn search_project_text_exclude_args() -> String {
    SEARCH_PROJECT_TEXT_EXCLUDES.join(" ")
}

fn search_project_text_rg_glob_args(options: &SearchOptions) -> String {
    let mut globs = options
        .include_globs
        .iter()
        .map(|glob| glob.to_string())
        .chain(options.exclude_globs.iter().map(|glob| format!("!{glob}")))
        .collect::<Vec<_>>();
    globs.extend(
        SEARCH_PROJECT_TEXT_RG_EXCLUDE_GLOBS
            .iter()
            .map(|glob| (*glob).to_string()),
    );
    globs
        .iter()
        .map(|glob| format!("--glob {}", shell_escape_simple(glob)))
        .collect::<Vec<_>>()
        .join(" ")
}

fn search_project_text_marker_command(backend: &str, feature_unavailable: bool) -> String {
    let marker = json!({
        "webcodex_search": {
            "backend": backend,
            "feature_unavailable": feature_unavailable,
        }
    })
    .to_string();
    format!("printf '%s\\n' {}", shell_escape_simple(&marker))
}

/// Default absolute `head` candidates when `command -v head` is unavailable.
pub(crate) const DEFAULT_SEARCH_HEAD_ABSOLUTE_CANDIDATES: &[&str] = &["/usr/bin/head", "/bin/head"];

/// Resolve a bounded-output `head` binary for search commands.
/// Prefers the first executable named `head` on `path_env`, then absolute
/// candidates. Returns `None` when nothing is usable (caller must fail closed).
///
/// Runtime shell commands re-implement the same policy (agent PATH may differ
/// from the server). This helper is the testable mirror of that policy.
#[cfg(test)]
pub(crate) fn resolve_search_head_command(
    path_env: Option<&str>,
    absolute_candidates: &[&str],
) -> Option<String> {
    if let Some(path_env) = path_env {
        for dir in std::env::split_paths(path_env) {
            let candidate = dir.join("head");
            if is_executable_file(&candidate) {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    for candidate in absolute_candidates {
        let path = Path::new(candidate);
        if path.is_absolute() && is_executable_file(path) {
            return Some((*candidate).to_string());
        }
    }
    None
}

#[cfg(test)]
fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Shell preamble that resolves `head_cmd` at runtime (agent/local sh).
/// Absolute fallbacks are embedded as literals for POSIX `sh`.
fn search_head_resolution_shell(absolute_candidates: &[&str]) -> String {
    let mut script = String::from(
        r#"head_cmd=
if command -v head >/dev/null 2>&1; then
  head_cmd=$(command -v head)
fi
"#,
    );
    for candidate in absolute_candidates {
        // Only absolute paths are considered as fallbacks.
        if !candidate.starts_with('/') {
            continue;
        }
        let escaped = shell_escape_simple(candidate);
        script.push_str(&format!(
            r#"if [ -z "$head_cmd" ] && [ -x {escaped} ]; then
  head_cmd={escaped}
fi
"#
        ));
    }
    script.push_str(
        r#"[ -n "$head_cmd" ] || exit 2
"#,
    );
    script
}

/// Select a safe absolute temp directory for search status files.
///
/// Uses physical paths (`pwd -P` / `cd && pwd -P`) so a TMPDIR symlink that
/// resolves into the project worktree cannot bypass the worktree ban.
/// Relative, unreadable, unwritable, or worktree-scoped TMPDIR falls back to `/tmp`.
fn search_status_tmpdir_shell() -> &'static str {
    r#"tmp_base=/tmp
if [ -n "${TMPDIR:-}" ]; then
  case "$TMPDIR" in
    /*)
      # Physical cwd (follows Command::current_dir; not inherited $PWD).
      pwd_phys=$(pwd -P 2>/dev/null || true)
      # Physical TMPDIR via cd (POSIX; no realpath/readlink -f).
      tmp_phys=$(cd "$TMPDIR" 2>/dev/null && pwd -P 2>/dev/null || true)
      if [ -n "$pwd_phys" ] && [ -n "$tmp_phys" ]; then
        case "$tmp_phys" in
          "$pwd_phys"|"$pwd_phys"/*)
            tmp_base=/tmp
            ;;
          *)
            tmp_base=$tmp_phys
            ;;
        esac
      else
        # Missing, unenterable, or unresolvable TMPDIR.
        tmp_base=/tmp
      fi
      ;;
    *)
      # Relative TMPDIR is never used for status files.
      tmp_base=/tmp
      ;;
  esac
fi
if [ ! -d "$tmp_base" ] || [ ! -w "$tmp_base" ]; then
  tmp_base=/tmp
fi
"#
}

/// Wrap a backend search invocation so POSIX `sh` preserves the backend exit
/// status even when output is bounded. Two `head` stages bound the work and
/// bytes: `head -n` closes the pipe once the record budget is satisfied (which
/// SIGPIPEs the backend and stops the scan early), and `head -c` emits at most
/// the formal byte budget plus one probe byte. That probe lets the Rust parser
/// prove the byte cap fired even when the formal boundary lands exactly after a
/// newline; the parser never exposes the probe byte. The backend status is
/// captured via a side-channel file so the pipeline cannot mask exit >= 2.
/// SIGPIPE (141) may occur when a bounder closes early on a large successful
/// result set and is treated as success by the Rust layer. Head's own non-zero
/// exit is never treated as success.
///
/// Status files live only under a safe absolute temp dir (never the project
/// worktree) and are cleaned up via trap + explicit removal. Uses pure shell
/// noclobber file creation instead of `mktemp` so the command still works when
/// PATH is restricted to a tool sandbox without coreutils.
///
/// Caller must ensure `head_cmd` is set (see `search_head_resolution_shell`).
fn wrap_search_project_text_backend_command(
    backend: &str,
    backend_cmd: &str,
    head_lines: usize,
    head_bytes: usize,
) -> String {
    let head_probe_bytes = head_bytes.saturating_add(1);
    let marker = search_project_text_marker_command(backend, false);
    format!(
        r#"{marker}
{tmpdir}
status_file=
cleanup_search_status() {{
  if [ -n "${{status_file:-}}" ]; then
    /bin/rm -f "$status_file" 2>/dev/null || /usr/bin/rm -f "$status_file" 2>/dev/null || rm -f "$status_file" 2>/dev/null || true
    status_file=
  fi
}}
# EXIT covers normal completion; signal traps clean then exit so the shell does not resume.
trap 'cleanup_search_status' EXIT
trap 'cleanup_search_status; exit 143' HUP INT TERM
i=0
while [ "$i" -lt 100 ]; do
  candidate="$tmp_base/webcodex-search-$$-$i"
  if (umask 077; set -C; : > "$candidate") 2>/dev/null; then
    status_file=$candidate
    break
  fi
  i=$((i + 1))
done
[ -n "$status_file" ] || exit 2
{{
  {backend_cmd}
  echo $? > "$status_file"
}} | "$head_cmd" -n {head_lines} | "$head_cmd" -c {head_probe_bytes}
head_status=$?
status=2
# read is a shell builtin so this works even when PATH lacks coreutils.
if [ -f "$status_file" ]; then
  read -r status < "$status_file" || status=2
fi
cleanup_search_status
case "$status" in
  ''|*[!0-9]*) status=2 ;;
esac
case "$head_status" in
  ''|*[!0-9]*) head_status=2 ;;
esac
if [ "$head_status" -ne 0 ]; then
  status=2
fi
exit "$status""#,
        marker = marker,
        tmpdir = search_status_tmpdir_shell(),
        backend_cmd = backend_cmd,
        head_lines = head_lines,
        head_probe_bytes = head_probe_bytes,
    )
}

/// Formal cap on search output bytes, applied by a second `head -c` stage in
/// the command (shared by local and agent paths) so no single over-long match
/// line, context line, or path can push the output past the Runner transport
/// cap (default 256 KiB) before the Rust layer ever sees it. The command emits
/// at most one probe byte beyond this formal budget; the parser consumes that
/// byte only as proof of truncation and never exposes it. A record cut mid-line
/// is dropped and reports `truncation_reason = "output_bytes"`.
///
/// Kept at 32 KiB, not larger: the local path executes the command through
/// [`run_command_sync`](crate::tool_runtime::helpers::run_command_sync), whose
/// polling loop does not drain stdout while waiting. Output over the ~64 KiB
/// Linux pipe buffer would block the producer until the hard timeout. 32 KiB
/// plus the backend marker stays comfortably under that buffer while still
/// bounding any single over-long record well below the transport cap.
pub(crate) const SEARCH_OUTPUT_BYTE_BUDGET: usize = 32 * 1024;

fn search_output_line_budget(options: &SearchOptions) -> usize {
    let result_budget = options.limit.saturating_add(1);
    if options.result_mode != SearchResultMode::Matches {
        return result_budget;
    }
    if options.context_before == 0 && options.context_after == 0 {
        return result_budget;
    }
    let context_budget = options
        .context_before
        .saturating_add(options.context_after)
        .saturating_add(2);
    result_budget
        .saturating_mul(context_budget)
        .saturating_add(1)
}

fn ripgrep_search_command(options: &SearchOptions) -> String {
    let globs = search_project_text_rg_glob_args(options);
    let pattern = shell_escape_simple(&options.pattern);
    let target = shell_escape_simple(&options.path);
    let mode_args = match options.result_mode {
        SearchResultMode::Matches => format!(
            "--with-filename --null --line-number --no-heading -B {} -A {}",
            options.context_before, options.context_after
        ),
        SearchResultMode::FilesWithMatches => "--files-with-matches".to_string(),
        SearchResultMode::Count => "--count --null".to_string(),
    };
    // Deliberately no `--sort path`: a global sort forces ripgrep to scan and
    // buffer the whole search space before emitting anything, so a small
    // `limit` request still waits for a full-repo walk. Without it, matches
    // stream in traversal order and `head -n` closes the pipe as soon as the
    // record budget is satisfied, which SIGPIPEs the backend and stops the
    // work early. Match order is not stable, but the result is bounded and
    // timely, which matters more for this tool.
    format!("rg {mode_args} --color never --hidden {globs} -e {pattern} -- {target} 2>/dev/null")
}

fn grep_search_command(options: &SearchOptions) -> String {
    format!(
        "grep -rnI --null {excludes} -B {before} -A {after} -e {pattern} -- {target} 2>/dev/null",
        excludes = search_project_text_exclude_args(),
        before = options.context_before,
        after = options.context_after,
        pattern = shell_escape_simple(&options.pattern),
        target = shell_escape_simple(&options.path),
    )
}

/// Build one bounded capability-selecting command for every search mode. Basic
/// matches calls retain grep fallback; requests that need full capabilities
/// emit a machine-readable marker when ripgrep is unavailable.
/// Wall-clock budget for one tracked-file listing. `git ls-files` reads the
/// index and does no tree walk, so a project that needs longer than this is
/// reporting a sick repository, not a big one.
const LIST_TRACKED_TIMEOUT_SECS: u64 = 20;

/// Transport cap on raw `git ls-files -z` output. Roughly 25k paths at typical
/// lengths; beyond it the listing reports `list_truncated` rather than
/// pretending the index ended there.
const LIST_TRACKED_MAX_BYTES: usize = 1024 * 1024;

/// Structured failure shaped like the other file-tool errors, so a caller can
/// branch on `code` instead of matching prose.
fn list_tracked_error(code: &str, message: String) -> ToolResult {
    ToolResult::err_with_output(message.clone(), json!({ "code": code, "message": message }))
}

/// First non-empty line of command stderr, bounded — enough to diagnose,
/// short enough not to spend model context on a stack of Git noise.
fn first_line(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .chars()
        .take(200)
        .collect()
}

/// Build the tracked-file listing command.
///
/// `git rev-parse` guards the pipeline so "not a repository" arrives as a
/// distinct exit code rather than as an empty listing that looks like an empty
/// project. Exit contract: 2 = no usable `head`, 3 = not a Git repository.
pub(crate) fn list_tracked_files_command(scope: &str) -> String {
    list_tracked_files_command_with_head_fallbacks(scope, DEFAULT_SEARCH_HEAD_ABSOLUTE_CANDIDATES)
}

pub(crate) fn list_tracked_files_command_with_head_fallbacks(
    scope: &str,
    absolute_head_candidates: &[&str],
) -> String {
    let head_setup = search_head_resolution_shell(absolute_head_candidates);
    // `:(literal)` disables pathspec globbing, so a directory named `*` or
    // `[a]` scopes to itself instead of matching siblings.
    let pathspec = if scope.is_empty() {
        String::new()
    } else {
        format!(
            " -- {}",
            shell_escape_simple(&format!(":(literal){}", scope.trim_end_matches('/')))
        )
    };
    format!(
        r#"{head_setup}if git rev-parse --git-dir >/dev/null 2>&1; then
  git ls-files -z --cached{pathspec} | "$head_cmd" -c {LIST_TRACKED_MAX_BYTES}
else
  exit 3
fi"#
    )
}

pub(crate) fn search_project_text_command(options: &SearchOptions) -> String {
    search_project_text_command_with_head_fallbacks(
        options,
        DEFAULT_SEARCH_HEAD_ABSOLUTE_CANDIDATES,
    )
}

/// Like [`search_project_text_command`], but absolute `head` fallbacks are
/// injectable so tests can simulate environments without a system head.
pub(crate) fn search_project_text_command_with_head_fallbacks(
    options: &SearchOptions,
    absolute_head_candidates: &[&str],
) -> String {
    let head = search_output_line_budget(options);
    let head_setup = search_head_resolution_shell(absolute_head_candidates);
    let head_bytes = SEARCH_OUTPUT_BYTE_BUDGET;
    let rg = wrap_search_project_text_backend_command(
        "rg",
        &ripgrep_search_command(options),
        head,
        head_bytes,
    );
    let fallback = if options.requires_ripgrep() {
        search_project_text_marker_command("grep", true)
    } else {
        wrap_search_project_text_backend_command(
            "grep",
            &grep_search_command(options),
            head,
            head_bytes,
        )
    };
    format!("{head_setup}if command -v rg >/dev/null 2>&1; then\n{rg}\nelse\n{fallback}\nfi")
}

fn search_request_dropped_tool_result(options: &SearchOptions) -> ToolResult {
    let message = "search_project_text agent request was dropped";
    ToolResult::err_with_output(
        message,
        json!({
            "code": "search_request_dropped",
            "result_mode": options.result_mode.as_str(),
            "effective_timeout_secs": options.timeout_secs,
            "message": message,
        }),
    )
}

/// Backend exit codes treated as successful search completion.
/// 0 = matches found, 1 = no matches, 141 = SIGPIPE after head truncated output.
fn is_search_backend_success_exit(code: i32) -> bool {
    matches!(code, 0 | 1 | 141)
}

fn looks_like_search_timeout(
    exit_code: Option<i32>,
    stderr: &str,
    agent_error: Option<&str>,
    timeout_secs: u64,
) -> bool {
    if looks_like_command_timeout(exit_code, stderr, timeout_secs) {
        return true;
    }
    let needle = format!("command timed out after {timeout_secs} seconds");
    let stderr_l = stderr.to_ascii_lowercase();
    if exit_code == Some(-1) && stderr_l.contains(&needle) {
        return true;
    }
    agent_error.is_some_and(|error| {
        let error_l = error.to_ascii_lowercase();
        error_l == "command timed out" || error_l.contains(&needle)
    })
}

fn search_timeout_tool_result(options: &SearchOptions, backend: Option<&str>) -> ToolResult {
    let message = format!(
        "search_project_text timed out after {} seconds",
        options.timeout_secs
    );
    let mut output = json!({
        "code": "search_timeout",
        "result_mode": options.result_mode.as_str(),
        "effective_timeout_secs": options.timeout_secs,
        "message": message,
    });
    if let Some(backend) = backend {
        output["backend"] = json!(backend);
    }
    ToolResult::err_with_output(message, output)
}

fn is_search_project_text_excluded_path(path: &str) -> bool {
    // Search skips credentials and the bulk trees alike: the first for
    // confidentiality, the second for cost and noise.
    crate::sensitive_paths::is_bulk_skipped_path(path)
}

#[derive(Debug, Clone)]
struct SearchLineRecord {
    path: String,
    line: u64,
    text: String,
    is_match: bool,
}

#[derive(Debug, Serialize)]
struct SearchContextLine {
    line: u64,
    text: String,
}

#[derive(Debug, Serialize)]
struct SearchMatch {
    path: String,
    line: u64,
    preview: String,
    context_before: Vec<SearchContextLine>,
    context_after: Vec<SearchContextLine>,
}

#[derive(Debug, Serialize)]
struct SearchFile {
    path: String,
}

#[derive(Debug, Serialize)]
struct SearchFileCount {
    path: String,
    match_count: u64,
}

#[derive(Debug)]
enum SearchResultData {
    Matches(Vec<SearchMatch>),
    FilesWithMatches(Vec<SearchFile>),
    Count {
        files: Vec<SearchFileCount>,
        returned_match_count: u64,
        count_complete: bool,
    },
}

/// Why a search result is incomplete. Distinct from "backend execution
/// failed": every variant below still returns complete, trusted records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchTruncation {
    /// The caller's `limit` was reached (the backend may have been stopped
    /// early by `head -n` closing the pipe).
    Limit,
    /// The `head -c` byte budget cut the stream, possibly mid-record; the
    /// parser drops the partial tail so only complete records are returned.
    OutputBytes,
    /// stdout was transport-truncated (the Runner keeps a tail of the output);
    /// the kept tail is honored but the prefix marks it incomplete.
    Transport,
    /// The search did not finish within the effective timeout; records
    /// collected before the timeout are still complete and trusted.
    Timeout,
}

impl SearchTruncation {
    fn reason(self) -> &'static str {
        match self {
            SearchTruncation::Limit => "limit",
            SearchTruncation::OutputBytes => "output_bytes",
            SearchTruncation::Transport => "transport",
            SearchTruncation::Timeout => "timeout",
        }
    }
}

#[derive(Debug)]
struct SearchResult {
    backend: String,
    data: SearchResultData,
    truncated: bool,
    truncation_reason: Option<SearchTruncation>,
}

#[derive(Debug)]
struct SearchBackendStatus {
    backend: String,
    feature_unavailable: bool,
}

fn parse_search_backend_status(stdout: &str) -> SearchBackendStatus {
    stdout
        .lines()
        .find_map(|line| {
            let value = serde_json::from_str::<Value>(line).ok()?;
            let marker = value.get("webcodex_search").unwrap_or(&value);
            let backend = marker.get("backend").and_then(Value::as_str)?;
            if !matches!(backend, "rg" | "grep" | "native" | "claude_code") {
                return None;
            }
            Some(SearchBackendStatus {
                backend: backend.to_string(),
                feature_unavailable: marker
                    .get("feature_unavailable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .unwrap_or_else(|| SearchBackendStatus {
            backend: "grep".to_string(),
            feature_unavailable: false,
        })
}

fn external_provider_error_result(stdout: &str) -> Option<ToolResult> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    if value.get("format").and_then(Value::as_str) != Some("webcodex.external_provider_error.v1") {
        return None;
    }
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("external tool provider failed")
        .to_string();
    Some(ToolResult {
        success: false,
        output: value,
        error: Some(message),
    })
}

/// A path parsed out of search stdout is only trusted when it is a plain
/// project-relative path: not absolute, no parent traversal, and not a
/// sensitive/bulk-skipped path. Anything else (a broken or hostile backend
/// emitting an absolute path or temp-file path) is dropped rather than
/// surfaced to the model.
fn is_trusted_search_record_path(path: &str) -> bool {
    let path = path.strip_prefix("./").unwrap_or(path);
    if path.is_empty() {
        return false;
    }
    let p = Path::new(path);
    if p.is_absolute()
        || p.components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return false;
    }
    !is_search_project_text_excluded_path(path)
}

fn normalize_search_record_path(path: &str) -> Option<String> {
    let path = path.strip_prefix("./").unwrap_or(path);
    if !is_trusted_search_record_path(path) {
        return None;
    }
    Some(path.to_string())
}

fn parse_search_line_record(line: &str) -> Option<SearchLineRecord> {
    let (path, line_no, separator, text) = if let Some((path, rest)) = line.split_once('\0') {
        let digits_end = rest
            .char_indices()
            .take_while(|(_, ch)| ch.is_ascii_digit())
            .map(|(index, ch)| index + ch.len_utf8())
            .last()?;
        let separator = rest[digits_end..].chars().next()?;
        if separator != ':' && separator != '-' {
            return None;
        }
        let text_start = digits_end + separator.len_utf8();
        (path, &rest[..digits_end], separator, &rest[text_start..])
    } else {
        let mut parts = line.splitn(3, ':');
        (parts.next()?, parts.next()?, ':', parts.next()?)
    };
    let path = normalize_search_record_path(path)?;
    Some(SearchLineRecord {
        path,
        line: line_no.parse().ok()?,
        text: text.to_string(),
        is_match: separator == ':',
    })
}

/// Return the byte offset immediately after the leading trusted backend marker,
/// when present. The command always emits this marker outside the bounded search
/// payload. Parser-only tests and legacy/transport-truncated responses may omit
/// it, in which case the whole stdout string is treated as payload.
fn search_payload_start(stdout: &str) -> usize {
    let Some(newline) = stdout.find('\n') else {
        return 0;
    };
    let line = &stdout[..newline];
    let Some(()) = serde_json::from_str::<Value>(line).ok().and_then(|value| {
        let marker = value.get("webcodex_search").unwrap_or(&value);
        let backend = marker.get("backend").and_then(Value::as_str)?;
        matches!(backend, "rg" | "grep" | "native" | "claude_code").then_some(())
    }) else {
        return 0;
    };
    newline + 1
}

/// Enforce the formal search payload budget in Rust and retain the shell's
/// single probe byte only long enough to prove that `head -c` hit the cap. This
/// remains correct when the formal byte boundary lands exactly after a newline,
/// where an unterminated-tail heuristic alone cannot distinguish truncation
/// from a naturally complete stream.
fn bounded_search_stdout(stdout: &str) -> (&str, bool) {
    let payload_start = search_payload_start(stdout);
    let payload = &stdout[payload_start..];
    if payload.len() <= SEARCH_OUTPUT_BYTE_BUDGET {
        return (stdout, false);
    }
    let mut payload_end = SEARCH_OUTPUT_BYTE_BUDGET;
    while payload_end > 0 && !payload.is_char_boundary(payload_end) {
        payload_end -= 1;
    }
    (&stdout[..payload_start + payload_end], true)
}

/// Split stdout into its complete newline-terminated lines plus a flag that is
/// true when either the explicit byte-cap probe proved truncation or the raw
/// output ended mid-record. The partial final segment is never a complete
/// record and is dropped. The unterminated-tail rule also guards timeout
/// truncation on the local path.
fn split_complete_search_lines(stdout: &str) -> (Vec<&str>, bool) {
    let (stdout, byte_cap_hit) = bounded_search_stdout(stdout);
    let unterminated = !stdout.is_empty() && !stdout.ends_with('\n');
    let cut = if unterminated {
        stdout.rfind('\n').map(|index| index + 1).unwrap_or(0)
    } else {
        stdout.len()
    };
    (
        stdout[..cut].lines().collect(),
        byte_cap_hit || unterminated,
    )
}

fn parse_search_line_records(stdout: &str) -> (Vec<SearchLineRecord>, bool) {
    let (lines, bytes_truncated) = split_complete_search_lines(stdout);
    let records = lines
        .iter()
        .filter_map(|line| parse_search_line_record(line))
        .collect();
    (records, bytes_truncated)
}

fn strip_leading_transport_truncation_marker(stdout: &str) -> (&str, bool) {
    for marker in ["[output truncated]\n", "[...]\n"] {
        if let Some(rest) = stdout.strip_prefix(marker) {
            return (rest, true);
        }
    }

    let Some(rest) = stdout.strip_prefix("[output truncated to last ") else {
        return (stdout, false);
    };
    let Some(newline) = rest.find('\n') else {
        return (stdout, false);
    };
    let marker_tail = &rest[..newline];
    let Some(byte_count) = marker_tail.strip_suffix(" bytes]") else {
        return (stdout, false);
    };
    if byte_count.is_empty() || !byte_count.bytes().all(|byte| byte.is_ascii_digit()) {
        return (stdout, false);
    }

    (&rest[newline + 1..], true)
}

fn search_matches_from_records(
    records: &[SearchLineRecord],
    options: &SearchOptions,
) -> (Vec<SearchMatch>, bool) {
    let mut matches = Vec::new();
    let mut truncated = false;
    for (index, record) in records.iter().enumerate() {
        if !record.is_match {
            continue;
        }
        if matches.len() >= options.limit {
            truncated = true;
            break;
        }
        let before_floor = record.line.saturating_sub(options.context_before as u64);
        let after_ceiling = record.line.saturating_add(options.context_after as u64);
        let context_before = records
            .iter()
            .take(index)
            .filter(|candidate| {
                candidate.path == record.path
                    && candidate.line >= before_floor
                    && candidate.line < record.line
            })
            .map(|candidate| SearchContextLine {
                line: candidate.line,
                text: candidate.text.clone(),
            })
            .collect::<Vec<_>>();
        let context_after = records
            .iter()
            .skip(index + 1)
            .filter(|candidate| {
                candidate.path == record.path
                    && candidate.line > record.line
                    && candidate.line <= after_ceiling
            })
            .map(|candidate| SearchContextLine {
                line: candidate.line,
                text: candidate.text.clone(),
            })
            .collect::<Vec<_>>();
        matches.push(SearchMatch {
            path: record.path.clone(),
            line: record.line,
            preview: record.text.clone(),
            context_before,
            context_after,
        });
    }
    (matches, truncated)
}

fn parse_file_paths(stdout: &str, limit: usize) -> (Vec<SearchFile>, bool, bool) {
    let (lines, bytes_truncated) = split_complete_search_lines(stdout);
    let mut paths = Vec::<String>::new();
    for line in lines {
        if serde_json::from_str::<Value>(line).is_ok() {
            continue;
        }
        let path = line.trim_end_matches('\r');
        let Some(path) = normalize_search_record_path(path) else {
            continue;
        };
        if paths.iter().any(|existing| existing == &path) {
            continue;
        }
        paths.push(path);
    }
    let limit_truncated = paths.len() > limit;
    paths.truncate(limit);
    (
        paths.into_iter().map(|path| SearchFile { path }).collect(),
        limit_truncated,
        bytes_truncated,
    )
}

fn parse_file_counts(stdout: &str, limit: usize) -> (Vec<SearchFileCount>, u64, bool, bool) {
    let (lines, bytes_truncated) = split_complete_search_lines(stdout);
    let mut counts = Vec::<(String, u64)>::new();
    for line in lines {
        if serde_json::from_str::<Value>(line).is_ok() {
            continue;
        }
        let parsed = line
            .split_once('\0')
            .or_else(|| line.rsplit_once(':'))
            .and_then(|(path, count)| {
                Some((path, count.trim_end_matches('\r').parse::<u64>().ok()?))
            });
        let Some((path, count)) = parsed else {
            continue;
        };
        let Some(path) = normalize_search_record_path(path) else {
            continue;
        };
        if let Some((_, existing)) = counts.iter_mut().find(|(existing, _)| existing == &path) {
            *existing = existing.saturating_add(count);
        } else {
            counts.push((path, count));
        }
    }
    let limit_truncated = counts.len() > limit;
    counts.truncate(limit);
    let returned_match_count = counts.iter().map(|(_, count)| *count).sum();
    (
        counts
            .into_iter()
            .map(|(path, match_count)| SearchFileCount { path, match_count })
            .collect(),
        returned_match_count,
        limit_truncated,
        bytes_truncated,
    )
}

fn parse_search_result(stdout: &str, options: &SearchOptions, backend: String) -> SearchResult {
    let (stdout, transport_truncated) = strip_leading_transport_truncation_marker(stdout);
    let (data, limit_truncated, bytes_truncated) = match options.result_mode {
        SearchResultMode::Matches => {
            let (records, bytes_truncated) = parse_search_line_records(stdout);
            let (matches, truncated) = search_matches_from_records(&records, options);
            (
                SearchResultData::Matches(matches),
                truncated,
                bytes_truncated,
            )
        }
        SearchResultMode::FilesWithMatches => {
            let (files, limit_truncated, bytes_truncated) = parse_file_paths(stdout, options.limit);
            (
                SearchResultData::FilesWithMatches(files),
                limit_truncated,
                bytes_truncated,
            )
        }
        SearchResultMode::Count => {
            let (files, returned_match_count, limit_truncated, bytes_truncated) =
                parse_file_counts(stdout, options.limit);
            (
                SearchResultData::Count {
                    files,
                    returned_match_count,
                    count_complete: !limit_truncated && !bytes_truncated && !transport_truncated,
                },
                limit_truncated,
                bytes_truncated,
            )
        }
    };
    let truncation = if transport_truncated {
        Some(SearchTruncation::Transport)
    } else if limit_truncated {
        Some(SearchTruncation::Limit)
    } else if bytes_truncated {
        Some(SearchTruncation::OutputBytes)
    } else {
        None
    };
    SearchResult {
        backend,
        data,
        truncated: truncation.is_some(),
        truncation_reason: truncation,
    }
}

pub(crate) fn search_project_text_output(
    project: &str,
    options: &SearchOptions,
    stdout: &str,
    exit_code: Option<i32>,
    stderr: &str,
) -> ToolResult {
    search_project_text_output_with_agent_error(project, options, stdout, exit_code, stderr, None)
}

pub(crate) fn search_project_text_output_with_agent_error(
    project: &str,
    options: &SearchOptions,
    stdout: &str,
    exit_code: Option<i32>,
    stderr: &str,
    agent_error: Option<&str>,
) -> ToolResult {
    let backend_status = parse_search_backend_status(stdout);
    if backend_status.feature_unavailable {
        let message = "ripgrep is required for the requested search_project_text features; grep fallback supports only basic matches requests";
        return ToolResult::err_with_output(
            message,
            json!({
                "code": "search_backend_feature_unavailable",
                "backend": "grep",
                "requested_features": options.requested_features,
                "message": message,
                "result_mode": options.result_mode.as_str(),
                "effective_timeout_secs": options.timeout_secs,
            }),
        );
    }
    if looks_like_search_timeout(exit_code, stderr, agent_error, options.timeout_secs) {
        // Timeout after a timeout can still have collected complete, trusted
        // records: return them as a partial success rather than discarding
        // them. Only when nothing complete was collected do we fall back to
        // the structured `search_timeout` failure.
        return search_timeout_tool_result_with_records(
            project,
            options,
            stdout,
            Some(backend_status.backend.as_str()),
            exit_code,
        );
    }
    // 0 = matches, 1 = no matches (success empty), 141 = SIGPIPE after head bound.
    // exit >= 2 (other) is a real backend execution failure.
    if exit_code.is_some_and(|code| !is_search_backend_success_exit(code)) {
        let message = "search_project_text backend execution failed";
        return ToolResult::err_with_output(
            message,
            json!({
                "code": "search_execution_failed",
                "backend": backend_status.backend,
                "result_mode": options.result_mode.as_str(),
                "effective_timeout_secs": options.timeout_secs,
                "message": message,
            }),
        );
    }

    let result = parse_search_result(stdout, options, backend_status.backend);
    ToolResult::ok(search_result_json(project, options, result, exit_code))
}

/// Serialize a parsed [`SearchResult`] into the public `search_project_text`
/// output object. Shared by the normal success path and the timeout
/// partial-success path so both emit identical field semantics.
fn search_result_json(
    project: &str,
    options: &SearchOptions,
    result: SearchResult,
    exit_code: Option<i32>,
) -> Value {
    let mut output = json!({
        "project": project,
        "pattern": options.pattern,
        "path": options.path,
        "backend": result.backend,
        "result_mode": options.result_mode.as_str(),
        "effective_timeout_secs": options.timeout_secs,
        "exit_code": exit_code,
        "context_before": options.context_before,
        "context_after": options.context_after,
    });
    match result.data {
        SearchResultData::Matches(matches) => {
            output["count"] = json!(matches.len());
            output["matches"] = json!(matches);
        }
        SearchResultData::FilesWithMatches(files) => {
            output["returned_file_count"] = json!(files.len());
            output["files"] = json!(files);
        }
        SearchResultData::Count {
            files,
            returned_match_count,
            count_complete,
        } => {
            output["returned_file_count"] = json!(files.len());
            output["returned_match_count"] = json!(returned_match_count);
            output["count_complete"] = json!(count_complete);
            output["total_matches"] = if count_complete {
                json!(returned_match_count)
            } else {
                Value::Null
            };
            output["files"] = json!(files);
        }
    }
    output["truncated"] = json!(result.truncated);
    output["truncation_reason"] = result
        .truncation_reason
        .map_or(Value::Null, |reason| json!(reason.reason()));
    output
}

/// A search timed out but may have collected complete records before the
/// backend was stopped. Return those records as a partial success
/// (`success = true`, `truncated = true`, `truncation_reason = "timeout"`).
/// Only complete records are trusted; a mid-record tail is dropped. When
/// nothing complete was collected, fall back to the structured
/// `search_timeout` failure. Count mode never presents a partial count as a
/// complete total (`count_complete` stays false, `total_matches` stays null).
fn search_timeout_tool_result_with_records(
    project: &str,
    options: &SearchOptions,
    stdout: &str,
    backend: Option<&str>,
    exit_code: Option<i32>,
) -> ToolResult {
    let backend_name = backend.unwrap_or("grep").to_string();
    let mut result = parse_search_result(stdout, options, backend_name);
    let has_records = match &result.data {
        SearchResultData::Matches(matches) => !matches.is_empty(),
        SearchResultData::FilesWithMatches(files) => !files.is_empty(),
        SearchResultData::Count { files, .. } => !files.is_empty(),
    };
    if !has_records {
        return search_timeout_tool_result(options, backend);
    }
    // The scan did not complete, so the result is truncated by timeout no
    // matter what the parsed state suggested.
    result.truncated = true;
    result.truncation_reason = Some(SearchTruncation::Timeout);
    let mut output = search_result_json(project, options, result, exit_code);
    if options.result_mode == SearchResultMode::Count {
        output["count_complete"] = json!(false);
        output["total_matches"] = Value::Null;
    }
    ToolResult::ok(output)
}

fn empty_search_project_text_output(project: &str, options: &SearchOptions) -> ToolResult {
    let marker = json!({
        "webcodex_search": {
            "backend": "native",
            "feature_unavailable": false,
        }
    })
    .to_string();
    search_project_text_output(project, options, &marker, None, "")
}

/// Maximum accepted size for `write_project_file` `content`.
pub(crate) const MAX_WRITE_CONTENT_BYTES: usize = 256 * 1024; // 256 KiB

// Edit limits and the sensitive-path guard are shared with the agent binary.
pub(crate) use crate::apply_edits_shared::{
    is_sensitive_edit_path, MAX_APPLY_FILE_CHANGES, MAX_APPLY_TEXT_EDITS,
    MAX_APPLY_TEXT_EDIT_FIELD_BYTES,
};

/// Maximum serialized batch payload sent to the owning agent. Host-only (the
/// agent enforces a per-file cap instead), so it stays local.
pub(crate) const MAX_APPLY_FILE_CHANGES_BYTES: usize = 1024 * 1024;

fn recoverable_write_rejection(reason: impl AsRef<str>) -> String {
    format!(
        "Rejected before write: {}.\nNo files were modified.\nRetry guidance: read the file again to refresh line numbers/context, then retry with updated guards.",
        reason.as_ref()
    )
}

/// Maximum decoded size for one binary project artifact imported through GPT
/// Actions/runtime tools. Keep bounded because artifact content travels to the
/// owning agent as base64 in a JSON file-op payload.
pub(crate) const MAX_PROJECT_ARTIFACT_BYTES: usize = 10 * 1024 * 1024; // 10 MiB

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectArtifactExportSnapshot {
    pub(crate) path: String,
    pub(crate) bytes: usize,
    pub(crate) sha256: String,
    pub(crate) mime_type: String,
    pub(crate) name: String,
}

/// Default returned segment size for `read_project_artifact`. This tool returns
/// base64 content in the JSON response, so keep chunks small for GPT Actions.
pub(crate) const DEFAULT_READ_PROJECT_ARTIFACT_LENGTH: usize = 32 * 1024; // 32 KiB

/// Maximum returned segment size for `read_project_artifact`.
pub(crate) const MAX_READ_PROJECT_ARTIFACT_LENGTH: usize = 64 * 1024; // 64 KiB

/// Maximum decoded size accepted for one `artifact_upload_chunk` request.
pub(crate) const MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES: usize = 64 * 1024; // 64 KiB

/// Hard cap for a base64-encoded artifact payload plus JSON overhead.
pub(crate) const MAX_PROJECT_ARTIFACT_BASE64_BYTES: usize = 14 * 1024 * 1024; // ~10 MiB decoded

/// Hard cap for a base64-encoded chunk plus JSON overhead.
pub(crate) const MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BASE64_BYTES: usize = 96 * 1024;

fn sniff_supported_mcp_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if data.starts_with(b"\xff\xd8") {
        Some("image/jpeg")
    } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn validate_mcp_image_artifact_output(output: &Value) -> Result<(), String> {
    let mime_type = output
        .get("mime_type")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "MCP image read requires a detected image/png, image/jpeg, or image/webp MIME type"
                .to_string()
        })?;
    if !matches!(mime_type, "image/png" | "image/jpeg" | "image/webp") {
        return Err(format!(
            "unsupported MCP image MIME type '{mime_type}'; supported types are image/png, image/jpeg, and image/webp"
        ));
    }
    let encoded = output
        .get("content_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP image read did not return complete base64 content".to_string())?;
    let decoded = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| format!("MCP image read returned invalid base64: {error}"))?;
    if decoded.len() > MAX_MCP_IMAGE_BYTES {
        return Err(format!(
            "MCP image is too large; maximum is {} bytes",
            MAX_MCP_IMAGE_BYTES
        ));
    }
    let detected = sniff_supported_mcp_image_mime(&decoded).ok_or_else(|| {
        "artifact content is not a supported PNG, JPEG, or WebP image".to_string()
    })?;
    if detected != mime_type {
        return Err(format!(
            "artifact MIME mismatch: runner reported '{mime_type}' but content is '{detected}'"
        ));
    }
    let file_bytes = output
        .get("file_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "MCP image read did not report file_bytes".to_string())?;
    let bytes_returned = output
        .get("bytes_returned")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "MCP image read did not report bytes_returned".to_string())?;
    let complete = output.get("offset").and_then(Value::as_u64) == Some(0)
        && output.get("truncated").and_then(Value::as_bool) == Some(false)
        && output.get("eof").and_then(Value::as_bool) == Some(true)
        && file_bytes == decoded.len()
        && bytes_returned == decoded.len();
    if !complete {
        return Err("MCP image read returned an incomplete artifact".to_string());
    }
    let reported_sha256 = output
        .get("sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP image read did not report sha256".to_string())?;
    if sha256_hex_bytes(&decoded) != reported_sha256 {
        return Err("MCP image read returned content that does not match its sha256".to_string());
    }
    Ok(())
}

/// Validate a project-relative file path for the structured edit tools
/// (`write_project_file`, `apply_text_edits`). Unlike the patch preflight
/// path validator, this HARD-rejects sensitive path components (the task spec
/// for these tools says "拒绝敏感路径", not "warn"). Absolute paths, `..`
/// traversal, empty paths, NUL bytes, and sensitive components are all rejected
/// so the helper never touches secrets, version control, or build output.
pub(crate) fn validate_edit_file_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_edit_path(path) {
        return Err(format!(
            "refusing sensitive path '{}': touches agent.toml, webcodex.env, \
             .env, projects.d, .git, target, or node_modules",
            path
        ));
    }
    Ok(())
}

/// Validate a project-relative binary artifact path. This is stricter than
/// source edit validation: in addition to build/VCS dirs it rejects secrets,
/// token paths, and private-key filenames.
pub(crate) fn validate_artifact_file_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path cannot contain NUL bytes".to_string());
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return Err("path must be project-relative".to_string());
    }
    if p.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("path cannot contain parent traversal".to_string());
    }
    if is_sensitive_artifact_path(path) {
        return Err(format!("refusing sensitive artifact path '{}'", path));
    }
    Ok(())
}

pub(crate) fn is_sensitive_artifact_path(path: &str) -> bool {
    // Artifacts previously missed `*.key` and `agent.toml`; they now share the
    // same policy as edits.
    crate::sensitive_paths::is_bulk_skipped_path(path)
}

fn validate_artifact_mime(mime_type: Option<&str>) -> Result<Option<String>, String> {
    let Some(mime) = mime_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if ooxml_extension_for_mime(mime).is_some() {
        return Ok(Some(mime.to_string()));
    }
    match mime {
        "image/png"
        | "image/jpeg"
        | "image/webp"
        | "application/pdf"
        | "application/zip"
        | "text/plain"
        | "text/csv"
        | "application/json" => Ok(Some(mime.to_string())),
        "application/octet-stream" => Ok(Some(mime.to_string())),
        _ => Err(format!("unsupported mime_type '{}'; allowed artifact MIME types are image/png, image/jpeg, image/webp, application/pdf, application/zip, application/vnd.openxmlformats-officedocument.wordprocessingml.document, application/vnd.openxmlformats-officedocument.presentationml.presentation, application/vnd.openxmlformats-officedocument.spreadsheetml.sheet, text/plain, text/csv, application/json", mime)),
    }
}

fn validate_artifact_mime_for_path(
    path: &str,
    mime_type: Option<&str>,
) -> Result<Option<String>, String> {
    let mime_type = validate_artifact_mime(mime_type)?;
    if matches!(mime_type.as_deref(), Some("application/octet-stream"))
        && !has_safe_octet_stream_artifact_extension(path)
    {
        return Err(octet_stream_safe_extension_error());
    }
    if let Some(mime) = mime_type.as_deref() {
        if let Some(required_extension) = ooxml_extension_for_mime(mime) {
            if !path.to_ascii_lowercase().ends_with(required_extension) {
                return Err(format!(
                    "OOXML MIME type '{mime}' requires a matching {required_extension} artifact path"
                ));
            }
        }
    }
    Ok(mime_type)
}

fn artifact_policy_rejected_result(path: &str, message: String) -> ToolResult {
    ToolResult::err_with_output(
        message.clone(),
        json!({
            "path": path,
            "error": message,
            "failure_kind": "policy_rejected",
            "error_kind": "policy_rejected",
        }),
    )
}

fn validate_artifact_upload_id(upload_id: &str) -> Result<(), String> {
    if !upload_id.starts_with("wc_upload_") {
        return Err("upload_id must start with wc_upload_".to_string());
    }
    if upload_id.len() > 96 {
        return Err("upload_id too long".to_string());
    }
    if !upload_id
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("upload_id contains unsupported characters".to_string());
    }
    Ok(())
}

/// True if `s` is a lowercase 64-character hex string (a sha256 digest).
pub(crate) fn is_hex_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

pub(crate) fn validate_project_artifact_export_snapshot(
    path: &str,
    output: &Value,
) -> Result<ProjectArtifactExportSnapshot, String> {
    validate_artifact_file_path(path)?;
    if output.get("path").and_then(Value::as_str) != Some(path) {
        return Err("artifact metadata path does not match the requested export path".to_string());
    }
    let bytes = output
        .get("bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| "artifact metadata did not report a valid byte count".to_string())?;
    if bytes > MAX_PROJECT_ARTIFACT_BYTES {
        return Err(format!(
            "artifact is too large to export; maximum is {} bytes",
            MAX_PROJECT_ARTIFACT_BYTES
        ));
    }
    let sha256 = output
        .get("sha256")
        .and_then(Value::as_str)
        .filter(|value| is_hex_sha256(value))
        .ok_or_else(|| "artifact metadata did not report a valid sha256".to_string())?
        .to_string();
    let reported_mime = output
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "artifact export requires a detected or inferred MIME type".to_string())?;
    let mime_type = validate_artifact_mime_for_path(path, Some(reported_mime))?
        .ok_or_else(|| "artifact export requires a validated MIME type".to_string())?;
    let name = Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty() && *value != "." && *value != "..")
        .ok_or_else(|| "artifact export path does not have a safe basename".to_string())?;
    if name.len() > 255
        || name
            .chars()
            .any(|ch| ch.is_control() || ch == '/' || ch == '\\')
    {
        return Err("artifact export basename is not safe for MCP presentation".to_string());
    }
    Ok(ProjectArtifactExportSnapshot {
        path: path.to_string(),
        bytes,
        sha256,
        mime_type,
        name: name.to_string(),
    })
}

fn validate_apply_text_edit(
    change_index: usize,
    edit_index: usize,
    edit: &ApplyTextEditInput,
) -> Result<(), String> {
    let validate_field = |label: &str, value: &Option<String>| -> Result<(), String> {
        if let Some(value) = value {
            if value.contains('\0') {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): {label} cannot contain NUL bytes",
                    edit.kind.as_str()
                ));
            }
            if value.len() > MAX_APPLY_TEXT_EDIT_FIELD_BYTES {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): {label} exceeds {} bytes",
                    edit.kind.as_str(),
                    MAX_APPLY_TEXT_EDIT_FIELD_BYTES
                ));
            }
        }
        Ok(())
    };
    validate_field("old_text", &edit.old_text)?;
    validate_field("new_text", &edit.new_text)?;
    validate_field("anchor_text", &edit.anchor_text)?;
    match edit.kind {
        ApplyTextEditKind::ReplaceExact => {
            if edit
                .old_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_exact): old_text must be non-empty"
                ));
            }
            if edit.anchor_text.is_some() {
                return Err(format!(
                    "change {change_index} edit {edit_index} (replace_exact): anchor_text is not allowed"
                ));
            }
        }
        ApplyTextEditKind::DeleteExact => {
            if edit
                .old_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} (delete_exact): old_text must be non-empty"
                ));
            }
            if edit.new_text.is_some() || edit.anchor_text.is_some() {
                return Err(format!(
                    "change {change_index} edit {edit_index} (delete_exact): new_text and anchor_text are not allowed"
                ));
            }
        }
        ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
            if edit
                .anchor_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): anchor_text must be non-empty",
                    edit.kind.as_str()
                ));
            }
            if edit
                .new_text
                .as_deref()
                .filter(|value| !value.is_empty())
                .is_none()
            {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): new_text must be non-empty",
                    edit.kind.as_str()
                ));
            }
            if edit.old_text.is_some() {
                return Err(format!(
                    "change {change_index} edit {edit_index} ({}): old_text is not allowed",
                    edit.kind.as_str()
                ));
            }
        }
    }
    Ok(())
}

fn validate_apply_file_change(index: usize, change: &ApplyFileChangeInput) -> Result<(), String> {
    let expected_hash = || -> Result<(), String> {
        match change.expected_sha256.as_deref() {
            Some(hash) if is_hex_sha256(hash) => Ok(()),
            _ => Err(format!(
                "change {index} ({}): expected_sha256 is required and must be 64 lowercase hexadecimal characters",
                change.kind.as_str()
            )),
        }
    };
    match change.kind {
        ApplyFileChangeKind::Edit => {
            expected_hash()?;
            if change.to_path.is_some() || change.content.is_some() {
                return Err(format!(
                    "change {index} (edit): to_path and content are not allowed"
                ));
            }
            if change.edits.is_empty() || change.edits.len() > MAX_APPLY_TEXT_EDITS {
                return Err(format!(
                    "change {index} (edit): edits must contain 1..={MAX_APPLY_TEXT_EDITS} entries"
                ));
            }
            for (edit_index, edit) in change.edits.iter().enumerate() {
                validate_apply_text_edit(index, edit_index, edit)?;
            }
        }
        ApplyFileChangeKind::Create => {
            if change.to_path.is_some()
                || change.expected_sha256.is_some()
                || !change.edits.is_empty()
            {
                return Err(format!(
                    "change {index} (create): to_path, expected_sha256, and edits are not allowed"
                ));
            }
            let content = change
                .content
                .as_deref()
                .ok_or_else(|| format!("change {index} (create): content is required"))?;
            if content.contains('\0') {
                return Err(format!(
                    "change {index} (create): content cannot contain NUL bytes"
                ));
            }
        }
        ApplyFileChangeKind::Delete => {
            expected_hash()?;
            if change.to_path.is_some() || change.content.is_some() || !change.edits.is_empty() {
                return Err(format!(
                    "change {index} (delete): to_path, content, and edits are not allowed"
                ));
            }
        }
        ApplyFileChangeKind::Rename => {
            expected_hash()?;
            let to_path = change
                .to_path
                .as_deref()
                .ok_or_else(|| format!("change {index} (rename): to_path is required"))?;
            if to_path == change.path {
                return Err(format!(
                    "change {index} (rename): path and to_path must differ"
                ));
            }
            if change.content.is_some() || !change.edits.is_empty() {
                return Err(format!(
                    "change {index} (rename): content and edits are not allowed"
                ));
            }
        }
    }
    Ok(())
}

fn apply_text_edits_agent_stdout_result(stdout: &str) -> ToolResult {
    let stdout = stdout.trim();
    let obj: Value = match serde_json::from_str(stdout) {
        Ok(value) => value,
        Err(error) => {
            return ToolResult::err(format!(
                "agent apply_text_edits returned invalid JSON: {} (got: {})",
                error,
                &stdout[..stdout.len().min(200)]
            ))
        }
    };
    if let Some(error) = obj.get("error").and_then(Value::as_str) {
        let uncertain = obj.get("rollback_complete").and_then(Value::as_bool) == Some(false)
            || obj.get("changed").and_then(Value::as_bool) == Some(true);
        let message = if uncertain {
            format!(
                "Edit outcome is uncertain: {error}. Inspect the affected files before issuing another write."
            )
        } else {
            recoverable_write_rejection(error)
        };
        return ToolResult::err_with_output(message, obj);
    }
    ToolResult::ok(obj)
}

pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Pure, allocation-only computation of an `apply_text_edits` plan against
/// `original` UTF-8 content. Performs every semantic validation (unique
/// match, no overlap, whole-file sha guard) and returns the new content plus
/// a structured summary. Never touches the filesystem — the runtime/agent
/// layer decides whether to write. Used directly by unit tests; the agent
/// handler mirrors these exact semantics for the production write path.
#[cfg(test)]
pub(crate) fn apply_text_edits_to_string(
    original: &str,
    path: &str,
    edits: &[ApplyTextEditInput],
    expected_file_sha256: Option<&str>,
    dry_run: bool,
) -> Result<(String, Value), String> {
    if edits.is_empty() {
        return Err("edits must contain at least one edit".to_string());
    }
    if edits.len() > MAX_APPLY_TEXT_EDITS {
        return Err(format!(
            "too many edits; maximum is {}",
            MAX_APPLY_TEXT_EDITS
        ));
    }
    let old_sha256 = sha256_hex_bytes(original.as_bytes());
    if let Some(expected) = expected_file_sha256 {
        if old_sha256 != expected {
            return Err(recoverable_write_rejection("expected_file_sha256 mismatch"));
        }
    }

    // Resolve each edit to a (start, end, replacement, index) op against the
    // original content. start/end are byte offsets; inserts are zero-width.
    let mut ops: Vec<(usize, usize, String, usize)> = Vec::with_capacity(edits.len());
    for (index, edit) in edits.iter().enumerate() {
        let kind = edit.kind;
        let (needle, replacement): (&str, String) = match kind {
            ApplyTextEditKind::ReplaceExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "old_text must be non-empty"))?;
                let new = edit.new_text.clone().unwrap_or_default();
                (old, new)
            }
            ApplyTextEditKind::DeleteExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "old_text must be non-empty"))?;
                (old, String::new())
            }
            ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
                let anchor = edit
                    .anchor_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        edit_field_error(index, kind, "anchor_text must be non-empty")
                    })?;
                let new = edit
                    .new_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "new_text must be non-empty"))?;
                (anchor, new.to_string())
            }
        };
        if needle.contains('\0') {
            return Err(edit_field_error(
                index,
                kind,
                "match text cannot contain NUL bytes",
            ));
        }
        if replacement.contains('\0') {
            return Err(edit_field_error(
                index,
                kind,
                "replacement text cannot contain NUL bytes",
            ));
        }
        let matches = original.matches(needle).count();
        if matches == 0 {
            return Err(edit_match_error(index, kind, "match text was not found"));
        }
        if matches > 1 {
            return Err(edit_match_error(
                index,
                kind,
                &format!(
                    "match text matched {} times; refusing ambiguous edit",
                    matches
                ),
            ));
        }
        let start = original.find(needle).expect("unique match already counted");
        let end = start + needle.len();
        let (range_start, range_end) = match kind {
            ApplyTextEditKind::InsertBefore => (start, start),
            ApplyTextEditKind::InsertAfter => (end, end),
            _ => (start, end),
        };
        ops.push((range_start, range_end, replacement, index));
    }

    // Stable sort by (start, end, original index) so the slice build is
    // deterministic and ties (e.g. multiple inserts at one point) keep caller
    // order.
    ops.sort_by_key(|&(s, e, _, i)| (s, e, i));

    // Reject overlapping edits: a later op must not start before an earlier
    // op ends. Zero-width ops (inserts) never trigger this because their
    // start == end.
    for w in ops.windows(2) {
        let (_, e1, _, _) = w[0];
        let (s2, _, _, _) = w[1];
        if s2 < e1 {
            return Err(recoverable_write_rejection(
                "edits overlap; refusing ambiguous atomic edit batch",
            ));
        }
    }

    // Build the new content by slicing the original at op boundaries.
    let mut new_content = String::with_capacity(original.len() + 64);
    let mut cursor = 0usize;
    let mut edit_summaries: Vec<Value> = Vec::with_capacity(ops.len());
    for &(start, end, ref replacement, index) in &ops {
        new_content.push_str(&original[cursor..start]);
        new_content.push_str(replacement);
        cursor = end;
        let edit = &edits[index];
        let old_start_line = 1 + original[..start].matches('\n').count();
        let mut old_end_line = 1 + original[..end].matches('\n').count();
        if end > start && end <= original.len() && original.as_bytes()[end - 1] == b'\n' {
            old_end_line = old_end_line.saturating_sub(1).max(old_start_line);
        }
        if end == start {
            old_end_line = old_start_line;
        }
        let new_line_count = if replacement.is_empty() {
            0
        } else {
            replacement.lines().count()
        };
        edit_summaries.push(json!({
            "index": index,
            "kind": edit.kind.as_str(),
            "old_start_line": old_start_line,
            "old_end_line": old_end_line,
            "new_line_count": new_line_count,
        }));
    }
    new_content.push_str(&original[cursor..]);

    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let changed = new_content != original;
    let output = json!({
        "path": path,
        "dry_run": dry_run,
        "applied_count": edits.len(),
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "changed": changed,
        "would_change": changed,
        "edits": edit_summaries,
        "changed_paths": [path],
    });
    Ok((new_content, output))
}

#[cfg(test)]
fn edit_field_error(index: usize, kind: ApplyTextEditKind, msg: &str) -> String {
    format!(
        "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with corrected edit fields.",
        index,
        kind.as_str(),
        msg
    )
}

#[cfg(test)]
fn edit_match_error(index: usize, kind: ApplyTextEditKind, msg: &str) -> String {
    format!(
        "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact match text.",
        index,
        kind.as_str(),
        msg
    )
}

impl ToolRuntime {
    pub(crate) async fn delete_project_files(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };

        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(client_id) => client_id.to_string(),
                Err(error) => return ToolResult::err(error),
            };
            // Pre-check is a non-authoritative optimization only: the
            // authoritative capability decision happens under the registry lock
            // at enqueue time, so a client that re-registers without
            // structured_file_delete between here and the enqueue falls back to
            // the legacy shell path instead of receiving an unknown file op.
            let supports_structured_delete = self
                .shell_clients
                .get_client_capabilities(&client_id)
                .await
                .map(|caps| caps.structured_file_delete)
                .unwrap_or(false);
            if supports_structured_delete {
                return self
                    .delete_project_files_structured_agent(&proj, client_id, paths, project, 30)
                    .await;
            }
            return self.delete_project_files_legacy_shell(project, paths).await;
        }

        self.delete_project_files_local(&proj, paths)
    }

    async fn delete_project_files_legacy_shell(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let command = format!("rm -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            let stdout_present = result
                .output
                .get("stdout_tail")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            let stderr_present = result
                .output
                .get("stderr_tail")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            ToolResult::ok(json!({
                "ok": true,
                "deleted_paths": paths,
                "missing_paths": [],
                "refused_paths": [],
                "stdout_present": stdout_present,
                "stderr_present": stderr_present,
            }))
        } else {
            result
        }
    }

    /// Bounded structured-delete failure result. Projects the shared
    /// `ShellCommandExecutionState` to the two authoritative effect states for
    /// a mutation: `not_started` (registry evidence proves the request was
    /// never dispatched) and `outcome_unknown` (dispatched, or dispatch cannot
    /// be proven false — the Runner may already have deleted files). No
    /// shell-command facts (`command_started` / `command_completed` /
    /// `command_ok`) are emitted: a structured file operation has no command
    /// lifecycle, and emitting command fields merely for symmetry would lie.
    fn delete_project_files_lifecycle_failure(
        message: impl Into<String>,
        state: ShellCommandExecutionState,
    ) -> ToolResult {
        let (execution_state, failure_kind) = match state {
            ShellCommandExecutionState::NotStarted => ("not_started", "not_started"),
            _ => ("outcome_unknown", "outcome_unknown"),
        };
        ToolResult::err_with_output(
            message.into(),
            json!({
                "execution_state": execution_state,
                "failure_kind": failure_kind,
                "tool_failure": true,
            }),
        )
    }

    /// Structured-delete effect-boundary failure message: the request may
    /// already have executed, so the model must inspect workspace state rather
    /// than blindly retrying.
    fn delete_project_files_outcome_unknown_message() -> String {
        "agent delete_project_files outcome is unknown; the request may already have deleted files. Inspect current workspace state before deciding whether to retry."
            .to_string()
    }

    /// Structured-delete not-started failure message: registry evidence proved
    /// the request was never handed to the Runner, so nothing was executed.
    fn delete_project_files_not_started_message() -> String {
        "agent delete_project_files was not dispatched; the structured delete did not start and the Runner executed no deletion from this request."
            .to_string()
    }

    pub(crate) async fn delete_project_files_structured_agent(
        &self,
        proj: &ProjectConfig,
        client_id: String,
        paths: Vec<String>,
        project: String,
        wait_timeout_secs: u64,
    ) -> ToolResult {
        let payload = match serde_json::to_string(&json!({"paths": paths})) {
            Ok(payload) => payload,
            Err(_) => return ToolResult::err("failed to encode delete_project_files request"),
        };
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_structured_file_delete(
                ShellFileOpRequest {
                    op: "delete_project_files".to_string(),
                    client_id,
                    path: ".".to_string(),
                    cwd: Some(proj.path.clone()),
                    content: Some(payload),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout_secs,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(request) => request,
            Err(error) if error.starts_with("capability_unavailable:") => {
                // The client re-registered without structured_file_delete
                // between the pre-check and this authoritative enqueue; the
                // registry queued nothing and no mutation can have happened.
                // The legacy shell path is the rolling-upgrade fallback every
                // Runner generation supports. This is the ONLY case that may
                // fall back after a structured attempt.
                return self.delete_project_files_legacy_shell(project, paths).await;
            }
            Err(_) => return ToolResult::err("agent delete_project_files is unavailable"),
        };
        let response = tokio::time::timeout(Duration::from_secs(wait_timeout_secs + 2), rx).await;
        let response = match response {
            Ok(Ok(response)) => {
                // Authoritative effect-boundary evidence: only a definite
                // terminal success enters the success path; every other
                // response classifies as not_started (registry-proven
                // undispatch, e.g. runner replacement before poll) or
                // outcome_unknown (dispatched, or dispatch cannot be proven
                // false — the Runner may already have deleted files).
                let state = agent_command_lifecycle(&response, wait_timeout_secs);
                match state {
                    ShellCommandExecutionState::Completed
                        if response.error.is_none() && response.exit_code == Some(0) =>
                    {
                        response
                    }
                    ShellCommandExecutionState::NotStarted => {
                        return Self::delete_project_files_lifecycle_failure(
                            Self::delete_project_files_not_started_message(),
                            state,
                        )
                    }
                    _ => {
                        return Self::delete_project_files_lifecycle_failure(
                            Self::delete_project_files_outcome_unknown_message(),
                            state,
                        )
                    }
                }
            }
            Ok(Err(_)) => {
                // Waiter channel closed. Atomically remove the pending request
                // and classify from recovered dispatch truth; a missing record
                // cannot prove undispatch, so only explicit `Some(false)` is
                // not_started.
                let dispatch = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                let state = dispatch_uncertainty_lifecycle(dispatch);
                if state == ShellCommandExecutionState::NotStarted {
                    return Self::delete_project_files_lifecycle_failure(
                        Self::delete_project_files_not_started_message(),
                        state,
                    );
                }
                return Self::delete_project_files_lifecycle_failure(
                    "agent delete_project_files waiter was dropped and dispatch cannot be proven false; the request may already have deleted files. Inspect current workspace state before deciding whether to retry."
                        .to_string(),
                    state,
                );
            }
            Err(_) => {
                // Wait timeout: cancel with dispatch truth instead of erasing
                // it. A timed-out mutation that may have dispatched must never
                // be presented as definitely not started.
                let dispatch = self
                    .shell_clients
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                let state = dispatch_uncertainty_lifecycle(dispatch);
                if state == ShellCommandExecutionState::NotStarted {
                    return Self::delete_project_files_lifecycle_failure(
                        format!(
                            "timed out waiting {wait_timeout_secs} seconds for agent delete_project_files before dispatch; the structured delete did not start and the Runner executed no deletion from this request."
                        ),
                        state,
                    );
                }
                return Self::delete_project_files_lifecycle_failure(
                    format!(
                        "timed out waiting {wait_timeout_secs} seconds for agent delete_project_files; the request may already have deleted files. Inspect current workspace state before deciding whether to retry."
                    ),
                    state,
                );
            }
        };
        let output: Value =
            match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                Ok(output) => output,
                Err(_) => {
                    return Self::delete_project_files_lifecycle_failure(
                        Self::delete_project_files_outcome_unknown_message(),
                        ShellCommandExecutionState::OutcomeUnknown,
                    )
                }
            };
        let expected = json!(paths);
        if output.get("deleted_paths") != Some(&expected)
            || output.get("missing_paths") != Some(&json!([]))
            || output.get("refused_paths") != Some(&json!([]))
        {
            return Self::delete_project_files_lifecycle_failure(
                Self::delete_project_files_outcome_unknown_message(),
                ShellCommandExecutionState::OutcomeUnknown,
            );
        }
        ToolResult::ok(json!({
            "ok": true,
            "deleted_paths": paths,
            "missing_paths": [],
            "refused_paths": [],
            "stdout_present": false,
            "stderr_present": false,
        }))
    }

    fn delete_project_files_local(&self, proj: &ProjectConfig, paths: Vec<String>) -> ToolResult {
        let canonical_root = match proj.root().canonicalize() {
            Ok(root) => root,
            Err(_) => return ToolResult::err("project root is unavailable"),
        };
        for path in &paths {
            let target = canonical_root.join(path);
            match std::fs::symlink_metadata(&target) {
                Ok(metadata) => {
                    if metadata.file_type().is_dir() {
                        return ToolResult::err("delete_project_files refuses directory targets");
                    }
                    let containment = if metadata.file_type().is_symlink() {
                        target
                            .parent()
                            .and_then(|parent| parent.canonicalize().ok())
                    } else {
                        target.canonicalize().ok()
                    };
                    if !containment.as_ref().is_some_and(|candidate| {
                        webcodex_agent_config::paths::path_is_within(candidate, &canonical_root)
                    }) {
                        return ToolResult::err(
                            "delete_project_files target is outside the project",
                        );
                    }
                    match std::fs::remove_file(&target) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(_) => return ToolResult::err("delete_project_files failed"),
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    let mut ancestor = target.parent();
                    let mut contained = false;
                    while let Some(candidate) = ancestor {
                        match candidate.canonicalize() {
                            Ok(candidate) => {
                                contained = webcodex_agent_config::paths::path_is_within(
                                    &candidate,
                                    &canonical_root,
                                );
                                break;
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                                ancestor = candidate.parent();
                            }
                            Err(_) => {
                                return ToolResult::err(
                                    "delete_project_files parent is unavailable",
                                )
                            }
                        }
                    }
                    if !contained {
                        return ToolResult::err(
                            "delete_project_files target is outside the project",
                        );
                    }
                }
                Err(_) => return ToolResult::err("delete_project_files failed"),
            }
        }
        ToolResult::ok(json!({
            "ok": true,
            "deleted_paths": paths,
            "missing_paths": [],
            "refused_paths": [],
            "stdout_present": false,
            "stderr_present": false,
        }))
    }

    // -------------------------------------------------------------------------
    // Phase 4: native agent JSON file ops
    // -------------------------------------------------------------------------
    //
    // Structured edits and project artifact tools run through the owning agent.
    // The server never reads or writes the agent project filesystem directly.
    // Arguments travel as JSON in a native agent file-op payload; the agent
    // performs validation and returns one JSON object on stdout.

    pub(crate) async fn run_agent_json_file_op(
        &self,
        client_id: String,
        cwd: String,
        path: String,
        op: &str,
        payload: Value,
        tool_name: &str,
    ) -> Result<Value, String> {
        let serialized = serde_json::to_string(&payload)
            .map_err(|e| format!("failed to serialize file-op payload: {}", e))?;
        let wait_timeout = 60_u64;
        let (request_id, rx) = self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: op.to_string(),
                    client_id,
                    path: path.clone(),
                    cwd: Some(cwd),
                    content: Some(serialized),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "tool_runtime".to_string(),
            )
            .await?;
        let resp = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return Err(format!("agent {} request was dropped", tool_name));
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return Err(format!("timed out waiting for agent {}", tool_name));
            }
        };
        if let Some(e) = resp.error {
            return Err(e);
        }
        if resp.exit_code != Some(0) {
            return Err(resp.stderr.unwrap_or_else(|| {
                format!("agent {} failed with code {:?}", tool_name, resp.exit_code)
            }));
        }
        let stdout = resp.stdout.unwrap_or_default();
        let stdout = stdout.trim();
        serde_json::from_str(stdout).map_err(|e| {
            format!(
                "agent {} returned invalid JSON: {} (got: {})",
                tool_name,
                e,
                &stdout[..stdout.len().min(200)]
            )
        })
    }

    /// Internal-only optimized segment transport for MCP artifact export. This
    /// is not a ToolCall and has no model schema. Project ownership and Runner
    /// access are re-resolved for the authenticated caller before each enqueue;
    /// the registry then atomically fences file_read plus the additive optimized
    /// capability with request admission. `Ok(None)` means only that the current
    /// Runner predates the optimized request and the caller may use the existing
    /// public read_project_artifact compatibility path.
    pub(crate) async fn read_project_artifact_export_chunk_internal(
        &self,
        project: &str,
        path: &str,
        expected_file_bytes: usize,
        offset: usize,
        length: usize,
        auth: Option<&AuthContext>,
    ) -> Result<Option<Value>, String> {
        if let Err(error) = validate_artifact_file_path(path) {
            return Err(error);
        }
        if expected_file_bytes > MAX_PROJECT_ARTIFACT_BYTES {
            return Err(format!(
                "artifact is too large to export; maximum is {} bytes",
                MAX_PROJECT_ARTIFACT_BYTES
            ));
        }
        if length == 0 || length > MAX_READ_PROJECT_ARTIFACT_LENGTH {
            return Err(format!(
                "artifact export chunk length must be between 1 and {} bytes",
                MAX_READ_PROJECT_ARTIFACT_LENGTH
            ));
        }
        offset
            .checked_add(length)
            .ok_or_else(|| "artifact export offset + length overflow".to_string())?;
        let resolved = self
            .resolve_project_for_auth(project, auth)
            .await
            .map_err(|error| error.to_message())?;
        if !resolved.is_agent() {
            return Err("artifact export chunks require an agent-registered project".to_string());
        }
        let client_id = resolved.agent_client_id()?.to_string();
        let payload = json!({
            "path": path,
            "expected_file_bytes": expected_file_bytes,
            "offset": offset,
            "length": length,
        });
        let serialized = serde_json::to_string(&payload).map_err(|error| {
            format!("failed to serialize artifact export chunk payload: {error}")
        })?;
        let wait_timeout = 60_u64;
        let request = ShellFileOpRequest {
            op: "read_project_artifact_export_chunk".to_string(),
            client_id,
            path: path.to_string(),
            cwd: Some(resolved.path.clone()),
            content: Some(serialized),
            max_bytes: None,
            old_text: None,
            pattern: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            line: None,
            create_dirs: false,
            wait_timeout_secs: wait_timeout,
        };
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_artifact_export_chunk(request, "mcp_artifact_export".to_string(), auth)
            .await
        {
            Ok(request) => request,
            Err(error)
                if error.contains(
                    crate::shell_protocol::SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ,
                ) =>
            {
                return Ok(None)
            }
            Err(error) => return Err(error),
        };
        let response = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(response)) => response,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return Err("agent artifact export chunk request was dropped".to_string());
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return Err("timed out waiting for agent artifact export chunk".to_string());
            }
        };
        if let Some(error) = response.error {
            return Err(error);
        }
        if response.exit_code != Some(0) {
            return Err(response.stderr.unwrap_or_else(|| {
                format!(
                    "agent artifact export chunk failed with code {:?}",
                    response.exit_code
                )
            }));
        }
        let stdout = response.stdout.unwrap_or_default();
        let stdout = stdout.trim();
        let output = serde_json::from_str(stdout).map_err(|error| {
            format!(
                "agent artifact export chunk returned invalid JSON: {error} (got: {})",
                &stdout[..stdout.len().min(200)]
            )
        })?;
        Ok(Some(output))
    }

    pub(crate) async fn write_project_file(
        &self,
        project: String,
        path: String,
        content: String,
        overwrite: Option<bool>,
        expected_sha256: Option<String>,
        expected_content_prefix: Option<String>,
    ) -> ToolResult {
        // ---- Input validation (before project resolution) ----
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if content.contains('\0') {
            return ToolResult::err("content cannot contain NUL bytes");
        }
        if content.len() > MAX_WRITE_CONTENT_BYTES {
            return ToolResult::err(format!(
                "content too large; maximum is {} bytes",
                MAX_WRITE_CONTENT_BYTES
            ));
        }
        if let Some(hash) = expected_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_sha256 must be a lowercase 64-char hex sha256 digest",
                );
            }
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "write_project_file requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path.clone(),
            "content": content,
            "overwrite": overwrite.unwrap_or(false),
            "expected_sha256": expected_sha256,
            "expected_content_prefix": expected_content_prefix,
        });
        let obj = match self
            .run_agent_json_file_op(
                client_id,
                proj.path.clone(),
                path.clone(),
                "write_project_file",
                payload,
                "write_project_file",
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn save_project_artifact(
        &self,
        project: String,
        path: String,
        content_base64: String,
        mime_type: Option<String>,
        overwrite: Option<bool>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        if content_base64.len() > MAX_PROJECT_ARTIFACT_BASE64_BYTES {
            return ToolResult::err(format!(
                "content_base64 too large; maximum encoded size is {} bytes",
                MAX_PROJECT_ARTIFACT_BASE64_BYTES
            ));
        }
        let mime_type = match validate_artifact_mime_for_path(&path, mime_type.as_deref()) {
            Ok(v) => v,
            Err(e) => return artifact_policy_rejected_result(&path, e),
        };
        let decoded = match general_purpose::STANDARD.decode(content_base64.as_bytes()) {
            Ok(bytes) => bytes,
            Err(e) => return ToolResult::err(format!("invalid base64: {}", e)),
        };
        if decoded.len() > MAX_PROJECT_ARTIFACT_BYTES {
            return ToolResult::err(format!(
                "decoded artifact too large; maximum is {} bytes",
                MAX_PROJECT_ARTIFACT_BYTES
            ));
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err("save_project_artifact requires an agent-registered project");
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path.clone(),
            "content_base64": content_base64,
            "mime_type": mime_type,
            "overwrite": overwrite.unwrap_or(false),
            "max_bytes": MAX_PROJECT_ARTIFACT_BYTES,
        });
        let obj = match self
            .run_agent_json_file_op(
                client_id,
                proj.path.clone(),
                path.clone(),
                "save_project_artifact",
                payload,
                "save_project_artifact",
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn read_project_artifact_metadata(
        &self,
        project: String,
        path: String,
        allow_missing: Option<bool>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "read_project_artifact_metadata requires an agent-registered project",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let payload = json!({
            "path": path.clone(),
            "max_bytes": MAX_PROJECT_ARTIFACT_BYTES,
            "allow_missing": allow_missing.unwrap_or(false),
        });
        let obj = match self
            .run_agent_json_file_op(
                client_id,
                proj.path.clone(),
                path.clone(),
                "read_project_artifact_metadata",
                payload,
                "read_project_artifact_metadata",
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn export_project_artifact_metadata_resolved(
        &self,
        resolved: &ResolvedProject,
        path: String,
    ) -> ToolResult {
        if let Err(error) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, error);
        }
        if !resolved.config.is_agent() {
            return ToolResult::err("export_project_artifact requires an agent-registered project");
        }
        let client_id = match resolved.config.agent_client_id() {
            Ok(client_id) => client_id.to_string(),
            Err(error) => return ToolResult::err(error),
        };
        let payload = json!({
            "path": path.clone(),
            "max_bytes": MAX_PROJECT_ARTIFACT_BYTES,
            "allow_missing": false,
        });
        let output = match self
            .run_agent_json_file_op(
                client_id,
                resolved.config.path.clone(),
                path.clone(),
                "read_project_artifact_metadata",
                payload,
                "export_project_artifact",
            )
            .await
        {
            Ok(output) => output,
            Err(error) => return ToolResult::err(error),
        };
        if let Some(error) = output
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output,
                error: Some(error),
            };
        }
        let snapshot = match validate_project_artifact_export_snapshot(&path, &output) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({
                        "project": resolved.resolved_id,
                        "path": path,
                        "error_kind": "invalid_artifact_export_metadata",
                    }),
                )
            }
        };
        ToolResult::ok(json!({
            "project": resolved.resolved_id,
            "path": snapshot.path,
            "bytes": snapshot.bytes,
            "sha256": snapshot.sha256,
            "mime_type": snapshot.mime_type,
            "name": snapshot.name,
        }))
    }

    pub(crate) async fn read_project_artifact(
        &self,
        project: String,
        path: String,
        encoding: Option<String>,
        offset: Option<usize>,
        length: Option<usize>,
        max_bytes: Option<usize>,
        as_image: Option<bool>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        let encoding = encoding.unwrap_or_else(|| "base64".to_string());
        if encoding != "base64" {
            return ToolResult::err("unsupported encoding; only 'base64' is currently supported");
        }
        let as_image = as_image.unwrap_or(false);
        if as_image && (offset.is_some() || length.is_some() || max_bytes.is_some()) {
            return ToolResult::err(
                "as_image cannot be combined with offset, length, or max_bytes; the MCP image path always reads one complete bounded image",
            );
        }
        let offset = offset.unwrap_or(0);
        let mut length = if as_image {
            MAX_MCP_IMAGE_BYTES
        } else {
            length.unwrap_or_else(|| max_bytes.unwrap_or(DEFAULT_READ_PROJECT_ARTIFACT_LENGTH))
        };
        if let Some(max_bytes) = max_bytes {
            if max_bytes == 0 {
                return ToolResult::err("max_bytes must be at least 1");
            }
            length = length.min(max_bytes);
        }
        if length == 0 {
            return ToolResult::err("length must be at least 1");
        }
        if !as_image && length > MAX_READ_PROJECT_ARTIFACT_LENGTH {
            return ToolResult::err(format!(
                "length too large; maximum is {} bytes",
                MAX_READ_PROJECT_ARTIFACT_LENGTH
            ));
        }
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err("read_project_artifact requires an agent-registered project");
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let mut payload = json!({
            "path": path.clone(),
            "offset": offset,
            "length": length,
            "max_file_bytes": if as_image {
                MAX_MCP_IMAGE_BYTES
            } else {
                MAX_PROJECT_ARTIFACT_BYTES
            },
        });
        if as_image {
            payload["mcp_image"] = json!(true);
        }
        let obj = match self
            .run_agent_json_file_op(
                client_id,
                proj.path.clone(),
                path.clone(),
                "read_project_artifact",
                payload,
                "read_project_artifact",
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        if as_image {
            if let Err(error) = validate_mcp_image_artifact_output(&obj) {
                return ToolResult::err_with_output(
                    error,
                    json!({
                        "path": path,
                        "error_kind": "invalid_mcp_image_artifact",
                    }),
                );
            }
        }
        ToolResult::ok(obj)
    }

    async fn run_project_artifact_write_file_op(
        &self,
        project: String,
        path: String,
        payload: Value,
        op: &str,
        tool_name: &str,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(format!("{tool_name} requires an agent-registered project"));
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let obj = match self
            .run_agent_json_file_op(client_id, proj.path.clone(), path, op, payload, tool_name)
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(e),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(err),
            };
        }
        ToolResult::ok(obj)
    }

    pub(crate) async fn artifact_upload_begin(
        &self,
        project: String,
        path: String,
        expected_bytes: Option<usize>,
        expected_sha256: Option<String>,
        mime_type: Option<String>,
        overwrite: Option<bool>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        if let Some(bytes) = expected_bytes {
            if bytes > MAX_PROJECT_ARTIFACT_BYTES {
                return ToolResult::err(format!(
                    "expected_bytes too large; maximum is {} bytes",
                    MAX_PROJECT_ARTIFACT_BYTES
                ));
            }
        }
        if let Some(hash) = expected_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_sha256 must be a lowercase 64-char hex sha256 digest".to_string(),
                );
            }
        }
        let mime_type = match validate_artifact_mime_for_path(&path, mime_type.as_deref()) {
            Ok(v) => v,
            Err(e) => return artifact_policy_rejected_result(&path, e),
        };
        let payload = json!({
            "path": path.clone(),
            "expected_bytes": expected_bytes,
            "expected_sha256": expected_sha256,
            "mime_type": mime_type,
            "overwrite": overwrite.unwrap_or(false),
            "max_bytes": MAX_PROJECT_ARTIFACT_BYTES,
        });
        self.run_project_artifact_write_file_op(
            project,
            path,
            payload,
            "artifact_upload_begin",
            "artifact_upload_begin",
        )
        .await
    }

    pub(crate) async fn artifact_upload_chunk(
        &self,
        project: String,
        path: String,
        upload_id: String,
        offset: usize,
        content_base64: String,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        if let Err(e) = validate_artifact_upload_id(&upload_id) {
            return ToolResult::err(e);
        }
        if content_base64.len() > MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BASE64_BYTES {
            return ToolResult::err(format!(
                "content_base64 chunk too large; maximum encoded size is {} bytes",
                MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BASE64_BYTES
            ));
        }
        let decoded = match general_purpose::STANDARD.decode(content_base64.as_bytes()) {
            Ok(bytes) => bytes,
            Err(e) => return ToolResult::err(format!("invalid base64: {}", e)),
        };
        if decoded.is_empty() {
            return ToolResult::err("decoded chunk must contain at least 1 byte");
        }
        if decoded.len() > MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES {
            return ToolResult::err(format!(
                "decoded chunk too large; maximum is {} bytes",
                MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES
            ));
        }
        let payload = json!({
            "path": path.clone(),
            "upload_id": upload_id,
            "offset": offset,
            "content_base64": content_base64,
            "max_chunk_bytes": MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES,
        });
        self.run_project_artifact_write_file_op(
            project,
            path,
            payload,
            "artifact_upload_chunk",
            "artifact_upload_chunk",
        )
        .await
    }

    pub(crate) async fn artifact_upload_finish(
        &self,
        project: String,
        path: String,
        upload_id: String,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        if let Err(e) = validate_artifact_upload_id(&upload_id) {
            return ToolResult::err(e);
        }
        let payload = json!({
            "path": path.clone(),
            "upload_id": upload_id,
        });
        self.run_project_artifact_write_file_op(
            project,
            path,
            payload,
            "artifact_upload_finish",
            "artifact_upload_finish",
        )
        .await
    }

    pub(crate) async fn artifact_upload_abort(
        &self,
        project: String,
        path: String,
        upload_id: String,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        if let Err(e) = validate_artifact_upload_id(&upload_id) {
            return ToolResult::err(e);
        }
        let payload = json!({
            "path": path.clone(),
            "upload_id": upload_id,
        });
        self.run_project_artifact_write_file_op(
            project,
            path,
            payload,
            "artifact_upload_abort",
            "artifact_upload_abort",
        )
        .await
    }

    pub(crate) async fn apply_text_edits(
        &self,
        project: String,
        changes: Vec<ApplyFileChangeInput>,
        dry_run: Option<bool>,
    ) -> ToolResult {
        if changes.is_empty() {
            return ToolResult::err("changes must contain at least one file change");
        }
        if changes.len() > MAX_APPLY_FILE_CHANGES {
            return ToolResult::err(format!(
                "too many file changes; maximum is {}",
                MAX_APPLY_FILE_CHANGES
            ));
        }
        let mut touched_paths = HashSet::new();
        for (change_index, change) in changes.iter().enumerate() {
            if let Err(error) = validate_edit_file_path(&change.path) {
                return super::permissions::edit_path_policy_rejected_result(&change.path, error);
            }
            if !touched_paths.insert(change.path.as_str()) {
                return ToolResult::err(format!(
                    "change {change_index} reuses path '{}'; each source/destination path may appear only once",
                    change.path
                ));
            }
            if let Some(to_path) = change.to_path.as_deref() {
                if let Err(error) = validate_edit_file_path(to_path) {
                    return super::permissions::edit_path_policy_rejected_result(to_path, error);
                }
                if !touched_paths.insert(to_path) {
                    return ToolResult::err(format!(
                        "change {change_index} reuses destination path '{to_path}'; each source/destination path may appear only once"
                    ));
                }
            }
            if let Err(message) = validate_apply_file_change(change_index, change) {
                return ToolResult::err(message);
            }
        }

        let payload = json!({
            "changes": changes,
            "dry_run": dry_run.unwrap_or(false),
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(serialized) if serialized.len() <= MAX_APPLY_FILE_CHANGES_BYTES => serialized,
            Ok(_) => {
                return ToolResult::err(format!(
                    "serialized file changes exceed {} bytes",
                    MAX_APPLY_FILE_CHANGES_BYTES
                ))
            }
            Err(error) => {
                return ToolResult::err(format!(
                    "failed to serialize file changes payload: {error}"
                ))
            }
        };

        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "apply_text_edits requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let wait_timeout = 60_u64;
        let routing_path = changes
            .first()
            .map(|change| change.path.clone())
            .expect("non-empty changes validated above");
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "apply_text_edits".to_string(),
                    client_id,
                    path: routing_path,
                    cwd: Some(proj.path.clone()),
                    content: Some(serialized),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(r) => r,
            Err(e) => return ToolResult::err(recoverable_write_rejection(e)),
        };
        let resp = match tokio::time::timeout(Duration::from_secs(wait_timeout + 4), rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("agent apply_text_edits request was dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("timed out waiting for agent apply_text_edits");
            }
        };
        if let Some(e) = resp.error {
            return ToolResult::err(recoverable_write_rejection(e));
        }
        if resp.exit_code != Some(0) {
            return ToolResult::err(recoverable_write_rejection(resp.stderr.unwrap_or_else(
                || {
                    format!(
                        "agent apply_text_edits failed with code {:?}",
                        resp.exit_code
                    )
                },
            )));
        }
        apply_text_edits_agent_stdout_result(&resp.stdout.unwrap_or_default())
    }

    pub(crate) async fn read_file(
        &self,
        project: String,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        let with_line_numbers = with_line_numbers.unwrap_or(false);
        // Bound the request to the project before it can reach an executor.
        // The agent branch below forwards `path` to a remote host that scopes
        // file ops to `allowed_roots` — which is broader than the project — so
        // the project boundary has to be enforced here, as `list_project_files`
        // and `project_overview` already do.
        if let Some(failure) = validate_read_file_path(&path) {
            return failure;
        }
        // Every other surface already refuses credentials: search excludes
        // them, artifacts and edits reject them. Reading was the one way left
        // to get a `.env` or a private key back verbatim. Only the narrow
        // secret policy applies here — reading `.git/HEAD` or a file under
        // `target/` by explicit path stays allowed.
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        self.read_one_validated_project_file(
            &proj,
            path,
            start_line,
            limit,
            with_line_numbers,
            None,
        )
        .await
    }

    pub(crate) async fn read_one_resolved_project_file(
        &self,
        project: &ProjectConfig,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        with_line_numbers: bool,
        deadline: Instant,
    ) -> ToolResult {
        if Instant::now() >= deadline {
            return read_file_failure(ReadFileReason::Timeout, Some(&path));
        }
        if let Some(failure) = validate_read_file_path(&path) {
            return failure;
        }
        self.read_one_validated_project_file(
            project,
            path,
            start_line,
            limit,
            with_line_numbers,
            Some(deadline),
        )
        .await
    }

    async fn read_one_validated_project_file(
        &self,
        proj: &ProjectConfig,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        with_line_numbers: bool,
        deadline: Option<Instant>,
    ) -> ToolResult {
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let wait_timeout = 30;
            let (eff_start, _eff_limit, eff_end) = effective_read_file_range(start_line, limit);
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "read".to_string(),
                        client_id,
                        path: path.clone(),
                        cwd: Some(proj.path.clone()),
                        content: None,
                        // The shared scanner bounds raw content. The Runner
                        // preflights the complete v1 envelope against this cap,
                        // and ToolRuntime separately checks the final model
                        // output after numbering and JSON escaping.
                        max_bytes: Some(512 * 1024),
                        old_text: None,
                        pattern: None,
                        expected_sha256: None,
                        expected_prefix: None,
                        start_line: Some(eff_start),
                        end_line: Some(eff_end),
                        line: None,
                        create_dirs: false,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(_) => return read_file_failure(ReadFileReason::AgentUnavailable, Some(&path)),
            };
            let response = match deadline {
                Some(deadline) => tokio::time::timeout_at(deadline, rx).await,
                None => tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await,
            };
            return match response {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    let mut result = read_file_agent_stdout_result_with_options(
                        resp.stdout.unwrap_or_default(),
                        start_line,
                        limit,
                        with_line_numbers,
                    );
                    if result.success {
                        result.output["path"] = json!(path);
                        result
                    } else {
                        // The agent response failed validation; surface a stable
                        // structured error using the project-relative path.
                        result.output["path"] = json!(path);
                        result
                    }
                }
                // Map agent execution failures to stable reason codes. The
                // raw `resp.error`/`resp.stderr` (which may carry the runner's
                // absolute path) is never forwarded to the model.
                Ok(Ok(resp)) => {
                    let reason = map_agent_read_error(&resp);
                    read_file_failure(reason, Some(&path))
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    read_file_failure(ReadFileReason::AgentUnavailable, Some(&path))
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    read_file_failure(ReadFileReason::Timeout, Some(&path))
                }
            };
        }
        // Local branch: stream the file through the shared range reader instead
        // of `std::fs::read_to_string`. The canonicalize/starts_with checks
        // stay as the project-boundary guard; failures map to stable reason
        // codes rather than OS error text.
        let file_path = proj.root().join(&path);
        let canonical_root = match proj.root().canonicalize() {
            Ok(p) => p,
            Err(error) => {
                return read_file_failure(io_error_reason(&error), Some(&path));
            }
        };
        let canonical = match file_path.canonicalize() {
            Ok(p) => p,
            Err(error) => {
                return read_file_failure(io_error_reason(&error), Some(&path));
            }
        };
        if !canonical.starts_with(&canonical_root) {
            return read_file_failure(ReadFileReason::InvalidPath, Some(&path));
        }
        if !canonical.is_file() {
            return read_file_failure(ReadFileReason::NotFile, Some(&path));
        }
        let range = EffectiveRange::new(start_line, limit);
        match file_read_range::read_range(&canonical, range) {
            Ok(result) => build_read_file_success(&result, with_line_numbers, Some(&path)),
            Err(error) => read_file_failure(error.reason, Some(&path)),
        }
    }

    // -------------------------------------------------------------------------
    // Project instructions auto-load (best-effort, session-start guidance)
    // -------------------------------------------------------------------------

    /// Best-effort load of project-local instruction files
    /// (`project_instructions::INSTRUCTION_CANDIDATE_PATHS`) for a resolved
    /// project. Candidates are tried in fixed order; the first candidate that
    /// reads successfully wins, bounding agent round-trips. Any read failure
    /// (agent not connected, file missing, timeout, decode error) is swallowed
    /// and the next candidate is tried. Returns an empty (`loaded=false`)
    /// snapshot when no candidate could be read.
    ///
    /// This never records session events (the session does not exist yet) and
    /// never fails `start_session`.
    pub(crate) async fn load_project_instructions(
        &self,
        config: &ProjectConfig,
    ) -> super::project_instructions::ProjectInstructionsSnapshot {
        use super::project_instructions::{
            ProjectInstructionsSnapshot, INSTRUCTION_CANDIDATE_PATHS,
        };
        let mut scan_complete = true;
        for candidate in INSTRUCTION_CANDIDATE_PATHS {
            match self.read_instruction_candidate(config, candidate).await {
                InstructionCandidateRead::Found(candidate) => {
                    return ProjectInstructionsSnapshot::from_candidates(
                        vec![candidate],
                        scan_complete,
                    );
                }
                InstructionCandidateRead::Missing => {}
                InstructionCandidateRead::Unavailable => scan_complete = false,
            }
        }
        if scan_complete {
            ProjectInstructionsSnapshot::empty()
        } else {
            ProjectInstructionsSnapshot::unavailable()
        }
    }

    /// Observe every fixed project-instruction candidate for model-facing
    /// coding startup. Requests run concurrently under the existing per-read
    /// deadline, while the resulting sources retain the fixed candidate order.
    /// This is what lets a Workflow Session detect additions, removals,
    /// content changes, and truncation changes across continuations.
    pub(crate) async fn load_coding_project_instructions(
        &self,
        config: &ProjectConfig,
    ) -> super::project_instructions::ProjectInstructionsSnapshot {
        use super::project_instructions::{
            ProjectInstructionsSnapshot, INSTRUCTION_CANDIDATE_PATHS,
        };
        let reads = INSTRUCTION_CANDIDATE_PATHS
            .iter()
            .map(|candidate| self.read_instruction_candidate(config, candidate));
        let mut found = Vec::new();
        let mut scan_complete = true;
        for result in futures_util::future::join_all(reads).await {
            match result {
                InstructionCandidateRead::Found(candidate) => found.push(candidate),
                InstructionCandidateRead::Missing => {}
                InstructionCandidateRead::Unavailable => scan_complete = false,
            }
        }
        if found.is_empty() && scan_complete {
            ProjectInstructionsSnapshot::empty()
        } else if found.is_empty() {
            ProjectInstructionsSnapshot::unavailable()
        } else {
            ProjectInstructionsSnapshot::from_candidates(found, scan_complete)
        }
    }

    /// Read a single instruction candidate from a resolved project. Returns
    /// `(content, total_lines)` on success or `None` on any failure.
    ///
    /// Reads are routed to the owning agent via the `file_read` op with a short
    /// best-effort timeout.
    async fn read_instruction_candidate(
        &self,
        config: &ProjectConfig,
        path: &str,
    ) -> InstructionCandidateRead {
        use super::project_instructions::{LoadedInstructionCandidate, MAX_LINES_PER_FILE};
        // Request one extra line so canonical envelope total/selection metadata
        // reliably signals truncation beyond the per-file cap.
        let read_limit = MAX_LINES_PER_FILE + 1;
        const WAIT_TIMEOUT: u64 = 6;

        let client_id = match config.agent_client_id() {
            Ok(client_id) => client_id,
            Err(_) => return InstructionCandidateRead::Unavailable,
        };
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "read".to_string(),
                    client_id: client_id.to_string(),
                    path: path.to_string(),
                    cwd: Some(config.path.clone()),
                    content: None,
                    max_bytes: Some(512 * 1024),
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: Some(1),
                    end_line: Some(read_limit),
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: WAIT_TIMEOUT,
                },
                "project_instructions".to_string(),
            )
            .await
        {
            Ok(enqueued) => enqueued,
            Err(_) => return InstructionCandidateRead::Unavailable,
        };
        match tokio::time::timeout(Duration::from_secs(WAIT_TIMEOUT + 2), rx).await {
            Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                match parse_instruction_agent_stdout(resp.stdout.unwrap_or_default()) {
                    Ok(Some((content, total_lines, full_sha256))) => {
                        InstructionCandidateRead::Found(LoadedInstructionCandidate {
                            path: path.to_string(),
                            content,
                            total_lines,
                            full_sha256,
                        })
                    }
                    Ok(None) => InstructionCandidateRead::Missing,
                    Err(()) => InstructionCandidateRead::Unavailable,
                }
            }
            Ok(Ok(resp)) => {
                if instruction_candidate_missing(&resp.error, &resp.stderr) {
                    InstructionCandidateRead::Missing
                } else {
                    InstructionCandidateRead::Unavailable
                }
            }
            _ => {
                self.shell_clients.cancel_request(&request_id).await;
                InstructionCandidateRead::Unavailable
            }
        }
    }

    /// `list_project_files`: bounded, project-relative file listing routed to
    /// the owning registered agent via the `file_list` op. The server never
    /// reads the agent project path directly. Returns `path` + `kind`
    /// (file/dir); size/mtime are not exposed by the current file op protocol.
    pub(crate) async fn list_project_files(
        &self,
        project: String,
        path: Option<String>,
        limit: Option<usize>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        let rel_path = path
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| ".".to_string());
        if let Err(e) = validate_project_relative_path(&rel_path) {
            return ToolResult::err(e);
        }
        let max_entries = limit.unwrap_or(200).clamp(1, 500);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let wait_timeout = 30;
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_file_op(
                    ShellFileOpRequest {
                        op: "list".to_string(),
                        client_id,
                        path: rel_path.clone(),
                        cwd: Some(proj.path.clone()),
                        content: None,
                        max_bytes: None,
                        old_text: None,
                        pattern: None,
                        expected_sha256: None,
                        expected_prefix: None,
                        start_line: None,
                        end_line: None,
                        line: None,
                        create_dirs: false,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    let stdout = resp.stdout.unwrap_or_default();
                    let (entries, truncated) =
                        parse_file_list_entries(&stdout, &rel_path, max_entries);
                    ToolResult::ok(json!({
                        "project": project,
                        "path": rel_path,
                        "entries": entries,
                        "truncated": truncated,
                    }))
                }
                Ok(Ok(resp)) => ToolResult::err(
                    resp.error
                        .or(resp.stderr)
                        .unwrap_or_else(|| "agent list_project_files failed".to_string()),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("agent list_project_files waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("timed out waiting for agent list_project_files")
                }
            };
        }
        // Local-executor parity path (the runtime surface is agent-first; this
        // branch mirrors read_file/git_status for structural consistency).
        let root = proj.root();
        let dir = if rel_path == "." {
            root.to_path_buf()
        } else {
            root.join(&rel_path)
        };
        let canonical_root = match root.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Project root does not exist: {}", e)),
        };
        let canonical_dir = match dir.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Path does not exist: {}", e)),
        };
        if !canonical_dir.starts_with(&canonical_root) {
            return ToolResult::err("Path is outside project directory");
        }
        let (entries, truncated) = match std::fs::read_dir(&canonical_dir) {
            Ok(rd) => {
                let mut all = Vec::new();
                for entry in rd.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    all.push(json!({
                        "path": relative_entry_path(&rel_path, &name),
                        "kind": if is_dir { "dir" } else { "file" },
                    }));
                }
                all.sort_by(|a, b| {
                    a["path"]
                        .as_str()
                        .unwrap_or("")
                        .cmp(b["path"].as_str().unwrap_or(""))
                });
                let truncated = all.len() > max_entries;
                all.truncate(max_entries);
                (all, truncated)
            }
            Err(e) => return ToolResult::err(format!("Failed to list directory: {}", e)),
        };
        ToolResult::ok(json!({
            "project": project,
            "path": rel_path,
            "entries": entries,
            "truncated": truncated,
        }))
    }

    /// `list_project_tracked_files`: enumerate what the project actually
    /// contains, using the Git index as the definition of "contains".
    ///
    /// Deliberately not `read_dir` (which `list_project_files` uses): a
    /// filesystem walk of a real project descends into `.venv`, `target`, and
    /// datasets, and reports tool state such as `.opencode/` as project
    /// content. `git ls-files` is the same definition `files_search`, project
    /// detection, and workspace provenance already use, so all four agree on
    /// what belongs to the project.
    ///
    /// The agent runs one deterministic command; scope, globs, rollup, and
    /// pagination are decided server-side in [`super::file_listing`].
    pub(crate) async fn list_project_tracked_files(
        &self,
        project: String,
        path: Option<String>,
        globs: Option<Vec<String>>,
        depth: Option<usize>,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> ToolResult {
        let scope_input = path.as_deref().unwrap_or("").trim().to_string();
        if !scope_input.is_empty() && scope_input != "." {
            if let Err(error) = validate_project_relative_path(&scope_input) {
                return list_tracked_error("invalid_path", error);
            }
        }
        let globs = globs.unwrap_or_default();
        if globs.len() > 20 {
            return list_tracked_error(
                "invalid_globs",
                "at most 20 globs are accepted".to_string(),
            );
        }
        if let Some(bad) = globs
            .iter()
            .find(|glob| glob.is_empty() || glob.len() > 256 || glob.contains('\0'))
        {
            return list_tracked_error(
                "invalid_globs",
                format!("glob must be 1..=256 bytes without NUL: {bad:?}"),
            );
        }
        let scope = super::file_listing::normalize_scope(Some(&scope_input));
        let limit = limit.unwrap_or(200).clamp(1, 1000);
        let offset = offset.unwrap_or(0);
        let depth = depth.map(|depth| depth.clamp(1, 16));

        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };
        let command = list_tracked_files_command(&scope);
        let (raw, exit_code, stderr) = if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(client_id) => client_id.to_string(),
                Err(error) => return ToolResult::err(error),
            };
            let (request_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command,
                        stdin: None,
                        timeout_secs: LIST_TRACKED_TIMEOUT_SECS,
                        wait_timeout_secs: LIST_TRACKED_TIMEOUT_SECS + 5,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(pending) => pending,
                Err(error) => return ToolResult::err(error),
            };
            match tokio::time::timeout(Duration::from_secs(LIST_TRACKED_TIMEOUT_SECS + 10), rx)
                .await
            {
                Ok(Ok(response)) => (
                    response.stdout.unwrap_or_default(),
                    response.exit_code,
                    response.stderr.unwrap_or_default(),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    return list_tracked_error(
                        "list_request_dropped",
                        "the agent request was dropped before a listing arrived".to_string(),
                    );
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    return list_tracked_error(
                        "list_timeout",
                        "timed out waiting for the project file listing".to_string(),
                    );
                }
            }
        } else {
            match run_command_sync_bounded(command, proj.root(), LIST_TRACKED_TIMEOUT_SECS).await {
                Ok((exit_code, stdout, stderr, _)) => (stdout, Some(exit_code), stderr),
                Err(LocalRunFailure::HardTimeout { bound_secs: _ }) => {
                    return list_tracked_error(
                        "list_timeout",
                        "timed out waiting for the project file listing".to_string(),
                    )
                }
                Err(LocalRunFailure::Join(error)) => {
                    return ToolResult::err(format!("task join error: {error}"))
                }
            }
        };

        // Exit codes are the command's own contract (see
        // `list_tracked_files_command`): 2 = no usable `head`, 3 = not a Git
        // repository, 141 = SIGPIPE once `head` closed a large stream early.
        match exit_code {
            Some(3) => return list_tracked_error(
                "not_a_git_repository",
                "this project is not a Git repository, so its tracked-file index cannot be read"
                    .to_string(),
            ),
            Some(2) => {
                return list_tracked_error(
                    "list_unavailable",
                    "the agent host has no usable head command to bound the listing".to_string(),
                )
            }
            Some(0) | Some(141) | None => {}
            Some(code) => {
                return list_tracked_error(
                    "list_failed",
                    format!(
                        "listing command failed with exit {code}: {}",
                        first_line(&stderr)
                    ),
                )
            }
        }

        let (paths, list_truncated) = super::file_listing::parse_nul_separated(&raw);
        let listing =
            super::file_listing::build_listing(&paths, &scope, &globs, depth, limit, offset);
        ToolResult::ok(listing.to_json(&project, &scope, list_truncated))
    }

    /// `project_overview`: deterministic, bounded project metadata routed to
    /// the owning agent. The server validates inputs and parses the structured
    /// response but never reads the agent host's project path.
    pub(crate) async fn project_overview(
        &self,
        project: String,
        path: Option<String>,
        max_depth: Option<usize>,
        limit: Option<usize>,
    ) -> ToolResult {
        let rel_path = match normalize_project_overview_path(path.as_deref().unwrap_or("")) {
            Ok(path) => path,
            Err(error) => return ToolResult::err(error),
        };
        let max_depth = effective_project_overview_max_depth(max_depth);
        let limit = effective_project_overview_limit(limit);
        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };
        if !proj.is_agent() {
            return ToolResult::err("project_overview requires a full agent runtime project id");
        }
        let client_id = match proj.agent_client_id() {
            Ok(client_id) => client_id.to_string(),
            Err(error) => return ToolResult::err(error),
        };
        let wait_timeout = 30;
        let agent_path = if rel_path.is_empty() {
            ".".to_string()
        } else {
            rel_path.clone()
        };
        let (request_id, receiver) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "project_overview".to_string(),
                    client_id,
                    path: agent_path,
                    cwd: Some(proj.path.clone()),
                    content: Some(json!({"max_depth": max_depth, "limit": limit}).to_string()),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: wait_timeout,
                },
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(request) => request,
            Err(error) => return ToolResult::err(error),
        };
        match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), receiver).await {
            Ok(Ok(response)) if response.exit_code == Some(0) && response.error.is_none() => {
                let mut output: Value =
                    match serde_json::from_str(response.stdout.as_deref().unwrap_or_default()) {
                        Ok(Value::Object(output)) => Value::Object(output),
                        Ok(_) => {
                            return ToolResult::err(
                                "agent project_overview returned a non-object payload",
                            )
                        }
                        Err(error) => {
                            return ToolResult::err(format!(
                                "agent project_overview returned invalid JSON: {error}"
                            ))
                        }
                    };
                if output["path"] != rel_path
                    || output["scan"]["max_depth"] != max_depth
                    || output["scan"]["limit"] != limit
                {
                    return ToolResult::err(
                        "agent project_overview response did not match the requested bounds",
                    );
                }
                output["project"] = json!(project);
                ToolResult::ok(output)
            }
            Ok(Ok(response)) => ToolResult::err(
                response
                    .error
                    .or(response.stderr)
                    .unwrap_or_else(|| "agent project_overview failed".to_string()),
            ),
            Ok(Err(_)) => {
                self.shell_clients.cancel_request(&request_id).await;
                ToolResult::err("agent project_overview waiter was dropped")
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                ToolResult::err("timed out waiting for agent project_overview")
            }
        }
    }

    /// `search_project_text`: bounded rg-first text search with grep fallback.
    /// Excludes sensitive/build paths by default. Each match carries a
    /// project-relative path, 1-based line number, preview line, and bounded
    /// context arrays.
    pub(crate) async fn search_project_text(
        &self,
        project: String,
        pattern: String,
        path: Option<String>,
        limit: Option<usize>,
        context_before: Option<usize>,
        context_after: Option<usize>,
        include_globs: Option<Vec<String>>,
        exclude_globs: Option<Vec<String>>,
        result_mode: Option<SearchResultMode>,
        timeout_secs: Option<i64>,
    ) -> ToolResult {
        let request = SearchRequest {
            pattern,
            path,
            limit,
            context_before,
            context_after,
            include_globs,
            exclude_globs,
            result_mode,
            timeout_secs,
        };
        // Preserve the single-query validation-before-resolution ordering.
        let options = match SearchOptions::normalize(request) {
            Ok(options) => options,
            Err(error) => return error.into_tool_result(),
        };
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        self.search_one_resolved_project_text(&proj, &project, options, None)
            .await
    }

    pub(crate) async fn search_project_text_resolved(
        &self,
        resolved: &ResolvedProject,
        output_project: &str,
        request: SearchRequest,
    ) -> ToolResult {
        let options = match SearchOptions::normalize(request) {
            Ok(options) => options,
            Err(error) => return error.into_tool_result(),
        };
        self.search_one_resolved_project_text(&resolved.config, output_project, options, None)
            .await
    }

    pub(crate) async fn search_one_resolved_project_text(
        &self,
        proj: &ProjectConfig,
        output_project: &str,
        mut options: SearchOptions,
        batch_deadline: Option<Instant>,
    ) -> ToolResult {
        if let Some(deadline) = batch_deadline {
            let now = Instant::now();
            if now >= deadline {
                return search_timeout_tool_result(&options, None);
            }
            // The Runner protocol expresses command timeouts in whole seconds;
            // the outer `timeout_at` below still enforces the exact subsecond
            // remainder when tests or a nearly exhausted batch have less than
            // one second left.
            let remaining_secs = deadline.duration_since(now).as_secs().max(1);
            options.timeout_secs = options.timeout_secs.min(remaining_secs);
        }
        if is_search_project_text_excluded_path(&options.path) {
            return empty_search_project_text_output(output_project, &options);
        }
        let cmd = search_project_text_command(&options);
        let effective_timeout_secs = options.timeout_secs;
        let (command_timeout, wait_timeout, outer_timeout) =
            search_agent_timeout_budget(effective_timeout_secs);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => return ToolResult::err(e),
            };
            let payload = json!({
                "pattern": options.pattern,
                "path": options.path,
                "limit": options.limit,
                "context_before": options.context_before,
                "context_after": options.context_after,
                "include_globs": options.include_globs,
                "exclude_globs": options.exclude_globs,
                "result_mode": options.result_mode.as_str(),
                "timeout_secs": command_timeout,
            });
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: format!("{EXTERNAL_SEARCH_REQUEST_PREFIX}\n{cmd}"),
                        stdin: Some(payload.to_string()),
                        timeout_secs: command_timeout,
                        wait_timeout_secs: wait_timeout,
                    },
                    "tool_runtime".to_string(),
                )
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::err(e),
            };
            let wait_deadline = Instant::now() + Duration::from_secs(outer_timeout);
            let wait_deadline = batch_deadline.map_or(wait_deadline, |deadline| {
                std::cmp::min(deadline, wait_deadline)
            });
            return match tokio::time::timeout_at(wait_deadline, rx).await {
                Ok(Ok(resp)) => {
                    let raw_stdout = resp.stdout.unwrap_or_default();
                    if let Some(result) = external_provider_error_result(&raw_stdout) {
                        return result;
                    }
                    let stdout = raw_stdout;
                    let stderr = resp.stderr.unwrap_or_default();
                    let agent_error = resp.error.as_deref();
                    if looks_like_search_timeout(
                        resp.exit_code,
                        &stderr,
                        agent_error,
                        options.timeout_secs,
                    ) {
                        let backend = if stdout.contains("webcodex_search") {
                            Some(parse_search_backend_status(&stdout).backend)
                        } else {
                            None
                        };
                        return search_timeout_tool_result_with_records(
                            output_project,
                            &options,
                            &stdout,
                            backend.as_deref(),
                            resp.exit_code,
                        );
                    }
                    if agent_error.is_some() {
                        let message = "search_project_text agent execution failed";
                        return ToolResult::err_with_output(
                            message,
                            json!({
                                "code": "search_execution_failed",
                                "result_mode": options.result_mode.as_str(),
                                "effective_timeout_secs": options.timeout_secs,
                                "message": message,
                            }),
                        );
                    }
                    search_project_text_output(
                        output_project,
                        &options,
                        &stdout,
                        resp.exit_code,
                        &stderr,
                    )
                }
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    // Channel closed without a result: agent disconnect / waiter
                    // drop — not a search timeout.
                    search_request_dropped_tool_result(&options)
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&req_id).await;
                    // Outer wait timed out before the agent reported; backend is unknown.
                    search_timeout_tool_result(&options, None)
                }
            };
        }
        let root = proj.root();
        let local = run_command_sync_bounded(cmd, root, effective_timeout_secs);
        let local = match batch_deadline {
            Some(deadline) => match tokio::time::timeout_at(deadline, local).await {
                Ok(result) => result,
                Err(_) => return search_timeout_tool_result(&options, Some("local_hard_bound")),
            },
            None => local.await,
        };
        match local {
            Ok((exit_code, stdout, stderr, _)) => search_project_text_output(
                output_project,
                &options,
                &stdout,
                Some(exit_code),
                &stderr,
            ),
            // Outer hard bound (command timeout + grace) fired: treat as a
            // search timeout so the MCP request still returns a structured error
            // instead of parking forever on a wedged output drain.
            Err(LocalRunFailure::HardTimeout { bound_secs: _ }) => {
                search_timeout_tool_result(&options, Some("local_hard_bound"))
            }
            Err(LocalRunFailure::Join(e)) => ToolResult::err(format!("task join error: {}", e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(name: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "webcodex-{}-{}-{}",
            name,
            std::process::id(),
            stamp
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn effective_read_file_range_defaults_and_clamps() {
        assert_eq!(effective_read_file_range(None, None), (1, 2000, 2000));
        assert_eq!(effective_read_file_range(Some(0), Some(0)), (1, 1, 1));
        assert_eq!(
            effective_read_file_range(Some(7), Some(5000)),
            (7, 2000, 2006)
        );
    }

    #[test]
    fn read_file_default_behavior_has_no_line_number_fields() {
        let result = read_file_content_result("one\ntwo\nthree".to_string(), Some(2), Some(1));

        assert!(result.success);
        assert_eq!(result.output["text"], "two");
        assert_eq!(result.output["format"], "plain");
        assert_eq!(result.output["total_lines"], 3);
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 1);
        assert_eq!(
            result.output["sha256"],
            sha256_hex_bytes(b"one\ntwo\nthree")
        );
        assert_eq!(result.output["returned_lines"], 1);
        assert_eq!(result.output["end_line"], 2);
        assert!(result.output["has_more"].as_bool().unwrap());
        assert_eq!(result.output["next_start_line"], 3);
        assert!(result.output.get("content").is_none());
        assert!(result.output.get("lines").is_none());
    }

    #[test]
    fn incomplete_apply_text_edits_rollback_is_not_reported_as_no_write() {
        let result = apply_text_edits_agent_stdout_result(
            r#"{"changed":true,"rollback_complete":false,"error":"rollback failed"}"#,
        );

        assert!(!result.success);
        assert_eq!(result.output["rollback_complete"], false);
        let error = result.error.unwrap();
        assert!(error.contains("uncertain"));
        assert!(!error.contains("No files were modified"));
    }

    #[test]
    fn read_file_agent_stdout_json_is_returned_without_reslicing() {
        let result = read_file_agent_stdout_result(
            serde_json::json!({
                "format": "webcodex.file_read_range.v1",
                "content": "line-560\nline-561",
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "total_lines": 7348,
                "start_line": 560,
                "limit": 2,
            })
            .to_string(),
            Some(560),
            Some(2),
        );

        assert!(result.success);
        assert_eq!(
            result.output["sha256"],
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(result.output["text"], "line-560\nline-561");
        assert_eq!(result.output["total_lines"], 7348);
        assert_eq!(result.output["start_line"], 560);
        assert_eq!(result.output["limit"], 2);
        // Continuation cursor metadata is always present.
        assert_eq!(result.output["returned_lines"], 2);
        assert_eq!(result.output["end_line"], 561);
        assert!(result.output["has_more"].as_bool().unwrap());
        assert_eq!(result.output["next_start_line"], 562);
        // No envelope extras are passed through.
        assert!(result.output.get("content").is_none());
    }

    #[test]
    fn read_file_agent_stdout_json_without_canonical_envelope_is_rejected() {
        let result = read_file_agent_stdout_result(
            serde_json::json!({
                "content": "file-json-content",
                "total_lines": 7348,
                "start_line": 560,
                "limit": 2,
            })
            .to_string(),
            Some(560),
            Some(2),
        );

        assert!(!result.success);
        // Malformed envelope surfaces a stable reason code, never raw stdout.
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
        assert!(result.error.unwrap().contains("malformed_agent_response"));
    }

    #[test]
    fn read_file_agent_stdout_plain_text_is_rejected() {
        let result =
            read_file_agent_stdout_result("one\ntwo\nthree\n".to_string(), Some(2), Some(1));

        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
    }

    #[test]
    fn read_file_with_line_numbers_returns_one_numbered_representation() {
        let result = read_file_content_result_with_options(
            "alpha\nbeta\ngamma".to_string(),
            None,
            None,
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["text"], "1 | alpha\n2 | beta\n3 | gamma");
        assert_eq!(result.output["format"], "numbered");
        assert!(result.output.get("content").is_none());
        assert!(result.output.get("numbered_text").is_none());
    }

    #[test]
    fn read_file_start_line_limit_with_line_numbers_uses_effective_range() {
        let result = read_file_content_result_with_options(
            "one\ntwo\nthree\nfour".to_string(),
            Some(2),
            Some(2),
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["text"], "2 | two\n3 | three");
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 2);
        assert_eq!(result.output["format"], "numbered");
    }

    #[test]
    fn read_file_with_line_numbers_handles_eof_and_short_files() {
        let result =
            read_file_content_result_with_options("one\ntwo".to_string(), Some(5), Some(3), true);

        assert!(result.success);
        assert_eq!(result.output["text"], "");
        assert_eq!(result.output["total_lines"], 2);
        assert_eq!(result.output["start_line"], 5);
        assert_eq!(result.output["limit"], 3);
        assert_eq!(result.output["format"], "numbered");
    }

    #[test]
    fn read_file_agent_stdout_json_with_line_numbers_preserves_empty_lines() {
        let result = read_file_agent_stdout_result_with_options(
            serde_json::json!({
                "format": "webcodex.file_read_range.v1",
                "content": "\nsecond",
                "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "total_lines": 3,
                "start_line": 1,
                "limit": 2,
            })
            .to_string(),
            Some(1),
            Some(2),
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["text"], "1 | \n2 | second");
        assert_eq!(result.output["format"], "numbered");
        // Numbering does not change range cursor metadata.
        assert_eq!(result.output["returned_lines"], 2);
        assert_eq!(result.output["end_line"], 2);
        assert!(result.output["has_more"].as_bool().unwrap());
        assert_eq!(result.output["next_start_line"], 3);
    }

    #[test]
    fn parse_search_matches_default_output_has_empty_context_arrays() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "main".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let result = search_project_text_output(
            "demo",
            &options,
            "src/main.rs:42:fn main() {}\n",
            Some(0),
            "",
        );
        let matches = result.output["matches"].as_array().unwrap();

        assert_eq!(result.output["truncated"], false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/main.rs");
        assert_eq!(matches[0]["line"], 42);
        assert_eq!(matches[0]["preview"], "fn main() {}");
        assert_eq!(matches[0]["context_before"], json!([]));
        assert_eq!(matches[0]["context_after"], json!([]));
    }

    #[test]
    fn parse_search_context_matches_returns_context_line_numbers() {
        let stdout = "src/lib.rs\x001-one\nsrc/lib.rs\x002-two\nsrc/lib.rs\x003:needle\nsrc/lib.rs\x004-four\nsrc/lib.rs\x005-five\n";
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: Some(2),
            context_after: Some(2),
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        let matches = result.output["matches"].as_array().unwrap();

        assert_eq!(result.output["truncated"], false);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/lib.rs");
        assert_eq!(matches[0]["line"], 3);
        assert_eq!(matches[0]["preview"], "needle");
        assert_eq!(
            matches[0]["context_before"],
            json!([
                {"line": 1, "text": "one"},
                {"line": 2, "text": "two"},
            ])
        );
        assert_eq!(
            matches[0]["context_after"],
            json!([
                {"line": 4, "text": "four"},
                {"line": 5, "text": "five"},
            ])
        );
    }

    #[test]
    fn search_matches_byte_budget_drops_partial_tail_record() {
        // A `head -c` cut stops mid-record: the last line has no trailing
        // newline. Only complete records are returned and the partial tail is
        // reported as an output_bytes truncation, never surfaced as a record.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = "src/a.rs:1:needle one\nsrc/b.rs:2:needle tw";
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        let matches = result.output["matches"].as_array().unwrap();

        assert!(result.success);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/a.rs");
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "output_bytes");
    }

    #[test]
    fn search_complete_output_under_budget_is_not_truncated() {
        // A naturally complete stream below the formal byte budget is not
        // truncated merely because its final record ends with a newline.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(50),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = "src/a.rs:1:needle one\n";
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(result.success);
        assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
        assert_eq!(result.output["truncated"], false);
        assert_eq!(result.output["truncation_reason"], Value::Null);
    }

    #[test]
    fn search_byte_probe_detects_exact_newline_boundary_truncation() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(50),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let prefix = "src/a.rs:1:";
        let text_len = SEARCH_OUTPUT_BYTE_BUDGET - prefix.len() - 1;
        let stdout = format!(
            "{{\"webcodex_search\":{{\"backend\":\"rg\"}}}}\n{prefix}{}\nX",
            "x".repeat(text_len)
        );
        let result = search_project_text_output("demo", &options, &stdout, Some(141), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "output_bytes");
    }

    #[test]
    fn search_files_with_matches_reports_output_byte_truncation() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(50),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(SearchResultMode::FilesWithMatches),
            timeout_secs: None,
        })
        .unwrap();
        let suffix = ".rs\n";
        let path = format!(
            "src/{}{}",
            "a".repeat(SEARCH_OUTPUT_BYTE_BUDGET - "src/".len() - suffix.len()),
            suffix
        );
        let stdout = format!("{{\"webcodex_search\":{{\"backend\":\"rg\"}}}}\n{path}X");
        let result = search_project_text_output("demo", &options, &stdout, Some(141), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["returned_file_count"], 1);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "output_bytes");
    }

    #[test]
    fn search_count_reports_output_byte_truncation_and_incomplete_total() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(50),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(SearchResultMode::Count),
            timeout_secs: None,
        })
        .unwrap();
        let count_suffix = "\x002\n";
        let path = format!(
            "src/{}",
            "a".repeat(SEARCH_OUTPUT_BYTE_BUDGET - "src/".len() - count_suffix.len())
        );
        let stdout =
            format!("{{\"webcodex_search\":{{\"backend\":\"rg\"}}}}\n{path}{count_suffix}X");
        let result = search_project_text_output("demo", &options, &stdout, Some(141), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["returned_file_count"], 1);
        assert_eq!(result.output["returned_match_count"], 2);
        assert_eq!(result.output["count_complete"], false);
        assert_eq!(result.output["total_matches"], Value::Null);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "output_bytes");
    }

    #[test]
    fn search_limit_stops_early_and_is_a_success_not_failure() {
        // `head -n` closes the pipe once the record budget is met, which
        // SIGPIPEs the backend (141). That is an intentional early stop, not a
        // backend failure, and the collected records are returned with
        // truncation_reason = "limit".
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(2),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = "src/a.rs:1:one\nsrc/b.rs:2:two\nsrc/c.rs:3:three\n";
        let result = search_project_text_output("demo", &options, stdout, Some(141), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["matches"].as_array().unwrap().len(), 2);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "limit");
    }

    #[test]
    fn search_normal_no_matches_is_success_empty() {
        // Exit 1 (no matches) is a successful empty result, never an error.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(5),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\n";
        let result = search_project_text_output("demo", &options, stdout, Some(1), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["matches"], json!([]));
        assert_eq!(result.output["count"], 0);
        assert_eq!(result.output["truncated"], false);
        assert_eq!(result.output["truncation_reason"], Value::Null);
    }

    #[test]
    fn search_timeout_with_complete_records_returns_partial_success() {
        // The backend timed out but had already emitted complete records. They
        // are returned as a partial success, not discarded.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: Some(0),
        })
        .unwrap();
        let stdout = concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
            "src/a.rs:1:needle one\n",
            "src/b.rs:2:needle tw",
        );
        let result = search_project_text_output_with_agent_error(
            "demo",
            &options,
            stdout,
            Some(-1),
            "Command timed out after 1 seconds",
            Some("command timed out"),
        );

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "timeout");
        assert_eq!(result.output["backend"], "rg");
        assert_eq!(result.output["effective_timeout_secs"], 1);
        let matches = result.output["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/a.rs");
        // The partial tail (src/b.rs record without trailing newline) is never
        // surfaced.
        assert!(
            matches
                .iter()
                .all(|m| m["path"].as_str() != Some("src/b.rs")),
            "partial tail record must not be returned"
        );
    }

    #[test]
    fn search_timeout_without_complete_records_returns_structured_failure() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: Some(0),
        })
        .unwrap();
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\n";
        let result = search_project_text_output(
            "demo",
            &options,
            stdout,
            Some(-1),
            "Command timed out after 1 seconds",
        );

        assert!(!result.success);
        assert_eq!(result.output["code"], "search_timeout");
        assert_eq!(result.output["effective_timeout_secs"], 1);
        // The structured failure carries no matches array at all.
        assert!(result.output.get("matches").is_none());
    }

    #[test]
    fn search_count_timeout_never_claims_complete_total() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(SearchResultMode::Count),
            timeout_secs: Some(0),
        })
        .unwrap();
        // Count mode: one complete file record followed by a partial tail.
        let stdout = concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
            "src/a.rs:2\n",
            "src/partial.rs:1",
        );
        let result = search_project_text_output(
            "demo",
            &options,
            stdout,
            Some(-1),
            "Command timed out after 1 seconds",
        );

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "timeout");
        assert_eq!(result.output["count_complete"], false);
        assert_eq!(result.output["total_matches"], Value::Null);
        assert_eq!(result.output["returned_file_count"], 1);
        assert_eq!(result.output["returned_match_count"], 2);
    }

    #[test]
    fn search_files_with_matches_timeout_returns_complete_files_truncated() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(SearchResultMode::FilesWithMatches),
            timeout_secs: Some(0),
        })
        .unwrap();
        let stdout = concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
            "src/a.rs\n",
            "src/b.rs",
        );
        let result = search_project_text_output(
            "demo",
            &options,
            stdout,
            Some(-1),
            "Command timed out after 1 seconds",
        );

        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "timeout");
        let files = result.output["files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["path"], "src/a.rs");
        assert_eq!(result.output["returned_file_count"], 1);
    }

    #[test]
    fn search_backend_real_failure_not_masked_by_early_stop() {
        // A backend exit >= 2 is a real failure even when the pipeline produced
        // some output before it died; it is never reported as an early-stop
        // partial success.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(5),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = "src/a.rs:1:needle\n";
        let result = search_project_text_output("demo", &options, stdout, Some(2), "");
        assert!(!result.success);
        assert_eq!(result.output["code"], "search_execution_failed");
    }

    #[test]
    fn search_rejects_absolute_and_temp_paths_in_output() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        // Absolute path, parent traversal, and a temp-file path must be
        // dropped; only the trusted relative record survives.
        let stdout = concat!(
            "/tmp/webcodex-x:1:secret\n",
            "src/../../etc/passwd:1:secret\n",
            "src/a.rs:1:needle\n",
        );
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(result.success, "{:?}", result.error);
        let matches = result.output["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/a.rs");
    }

    fn transport_truncation_markers() -> [&'static str; 3] {
        [
            "[output truncated to last 12000 bytes]\n",
            "[output truncated]\n",
            "[...]\n",
        ]
    }

    #[test]
    fn search_transport_truncated_stdout_is_not_mistaken_for_complete() {
        // The Runner keeps a tail of oversized output prefixed with a marker;
        // that marker proves the output was truncated and must be reported as
        // transport truncation, not a complete result.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = concat!(
            "[output truncated to last 12000 bytes]\n",
            "src/z.rs:1:needle tail\n",
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
        );
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["truncated"], true);
        assert_eq!(result.output["truncation_reason"], "transport");
        let matches = result.output["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "src/z.rs");
    }

    #[test]
    fn search_transport_marker_forms_preserve_complete_matches() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();

        for marker in transport_truncation_markers() {
            let stdout = format!("{marker}src/a.rs:1:needle one\nsrc/b.rs:2:needle two\n");
            let result = search_project_text_output("demo", &options, &stdout, Some(0), "");
            assert!(result.success, "marker {marker:?}: {:?}", result.error);
            assert_eq!(result.output["truncated"], true, "marker {marker:?}");
            assert_eq!(
                result.output["truncation_reason"], "transport",
                "marker {marker:?}"
            );
            let paths = result.output["matches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["path"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(paths, ["src/a.rs", "src/b.rs"], "marker {marker:?}");
        }
    }

    #[test]
    fn search_transport_marker_forms_never_become_file_paths() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(SearchResultMode::FilesWithMatches),
            timeout_secs: None,
        })
        .unwrap();

        for marker in transport_truncation_markers() {
            let stdout = format!("{marker}src/a.rs\nsrc/b.rs\n");
            let result = search_project_text_output("demo", &options, &stdout, Some(0), "");
            assert!(result.success, "marker {marker:?}: {:?}", result.error);
            assert_eq!(result.output["truncated"], true, "marker {marker:?}");
            assert_eq!(
                result.output["truncation_reason"], "transport",
                "marker {marker:?}"
            );
            let paths = result.output["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["path"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(paths, ["src/a.rs", "src/b.rs"], "marker {marker:?}");
            assert!(
                !paths.contains(&"[output truncated]"),
                "Phase F long marker must not become a fake path"
            );
        }
    }

    #[test]
    fn search_transport_marker_forms_make_counts_incomplete() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: Some(SearchResultMode::Count),
            timeout_secs: None,
        })
        .unwrap();

        for marker in transport_truncation_markers() {
            let stdout = format!("{marker}src/a.rs:2\nsrc/b.rs:3\n");
            let result = search_project_text_output("demo", &options, &stdout, Some(0), "");
            assert!(result.success, "marker {marker:?}: {:?}", result.error);
            assert_eq!(result.output["truncated"], true, "marker {marker:?}");
            assert_eq!(
                result.output["truncation_reason"], "transport",
                "marker {marker:?}"
            );
            assert_eq!(result.output["returned_file_count"], 2, "marker {marker:?}");
            assert_eq!(
                result.output["returned_match_count"], 5,
                "marker {marker:?}"
            );
            assert_eq!(result.output["count_complete"], false, "marker {marker:?}");
            assert_eq!(
                result.output["total_matches"],
                Value::Null,
                "marker {marker:?}"
            );
        }
    }

    #[test]
    fn search_transport_marker_text_in_middle_is_not_transport_truncation() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: None,
            context_after: None,
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let stdout = concat!(
            "src/a.rs:1:needle one\n",
            "[output truncated]\n",
            "src/b.rs:2:needle two\n",
        );
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["truncated"], false);
        assert_eq!(result.output["truncation_reason"], Value::Null);
        assert_eq!(result.output["matches"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn search_local_and_agent_parse_same_stdout_identically() {
        // The agent path parses the runner's stdout with the same function as
        // the local path, so the exact same stdout string must yield identical
        // field semantics in both. This pins that parity for the record fields
        // the task lists: backend, result_mode, matches, count, truncated,
        // truncation_reason, effective_timeout_secs.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(2),
            context_before: Some(1),
            context_after: Some(1),
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: Some(5),
        })
        .unwrap();
        let stdout = concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
            "src/a.rs\01-one\n",
            "src/a.rs\02:needle\n",
            "src/a.rs\03-three\n",
        );
        let local = search_project_text_output("demo", &options, stdout, Some(0), "");
        let agent = search_project_text_output_with_agent_error(
            "demo",
            &options,
            stdout,
            Some(0),
            "",
            None,
        );
        for field in [
            "backend",
            "result_mode",
            "effective_timeout_secs",
            "count",
            "truncated",
            "truncation_reason",
            "matches",
            "exit_code",
        ] {
            assert_eq!(local.output[field], agent.output[field], "field {field}");
        }
        assert!(local.success == agent.success);
    }

    #[test]
    fn search_context_command_bounds_file_start_and_end() {
        let root = unique_temp_dir("search-context");
        std::fs::write(
            root.join("sample.txt"),
            "needle-start\nmiddle\nneedle-end\n",
        )
        .expect("write sample");
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: Some(10),
            context_before: Some(3),
            context_after: Some(3),
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        let cmd = search_project_text_command(&options);
        let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);

        assert_eq!(exit_code, 0, "stderr: {stderr}");
        let result =
            search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
        let matches = result.output["matches"].as_array().unwrap();
        assert_eq!(result.output["truncated"], false);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0]["line"], 1);
        assert_eq!(matches[0]["context_before"], json!([]));
        assert_eq!(
            matches[0]["context_after"],
            json!([
                {"line": 2, "text": "middle"},
                {"line": 3, "text": "needle-end"},
            ])
        );
        assert_eq!(matches[1]["line"], 3);
        assert_eq!(
            matches[1]["context_before"],
            json!([
                {"line": 1, "text": "needle-start"},
                {"line": 2, "text": "middle"},
            ])
        );
        assert_eq!(matches[1]["context_after"], json!([]));
    }

    #[test]
    fn effective_search_context_clamps_values() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            path: None,
            limit: None,
            context_before: Some(21),
            context_after: Some(99),
            include_globs: None,
            exclude_globs: None,
            result_mode: None,
            timeout_secs: None,
        })
        .unwrap();
        assert_eq!((options.context_before, options.context_after), (20, 20));
    }

    // -------------------------------------------------------------------------
    // Bounded source reads: local/agent parity, strict validation, budgets.
    // -------------------------------------------------------------------------

    /// Build a local result from full file content and a request range.
    fn local_read(
        content: &str,
        start: Option<usize>,
        limit: Option<usize>,
        numbered: bool,
    ) -> Value {
        read_file_content_result_with_options(content.to_string(), start, limit, numbered).output
    }

    /// Build an agent result from a synthesized v1 envelope derived from the
    /// same full content and range, so local and agent outputs can be compared
    /// field-by-field.
    fn agent_read(
        content: &str,
        start: Option<usize>,
        limit: Option<usize>,
        numbered: bool,
    ) -> Value {
        use webcodex_workspace::file_read_range::{self, EffectiveRange};
        let range = EffectiveRange::new(start, limit);
        let result = file_read_range::read_range_from(content.as_bytes(), range).unwrap();
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": result.content,
            "sha256": result.sha256,
            "total_lines": result.total_lines,
            "start_line": result.start_line,
            "limit": result.limit,
            "padding": "x".repeat(8192),
        })
        .to_string();
        read_file_agent_stdout_result_with_options(envelope, start, limit, numbered).output
    }

    fn assert_parity(content: &str, start: Option<usize>, limit: Option<usize>, numbered: bool) {
        let local = local_read(content, start, limit, numbered);
        let agent = agent_read(content, start, limit, numbered);
        for field in [
            "text",
            "format",
            "sha256",
            "start_line",
            "limit",
            "total_lines",
            "returned_lines",
            "end_line",
            "has_more",
            "next_start_line",
        ] {
            assert_eq!(
                local.get(field),
                agent.get(field),
                "parity mismatch on {field} for content={content:?} start={start:?} limit={limit:?} numbered={numbered}"
            );
        }
        // Agent envelope padding never reaches the model output.
        assert!(agent.get("padding").is_none());
        assert!(agent.get("content").is_none());
    }

    #[test]
    fn read_file_local_agent_parity_across_ranges() {
        assert_parity("one\ntwo\nthree\nfour\nfive", Some(2), Some(2), false);
        assert_parity("one\ntwo\nthree\nfour\nfive", Some(2), Some(2), true);
        assert_parity("one\ntwo\nthree", Some(1), Some(100), false);
        assert_parity("a\n\nb\nc", Some(2), Some(2), false);
        assert_parity("\nsecond\nthird", Some(1), Some(2), true);
        assert_parity("only", None, None, false);
    }

    #[test]
    fn read_file_parity_empty_file_and_overflow() {
        assert_parity("", Some(1), Some(5), false);
        assert_parity("a\nb\nc", Some(10), Some(5), false);
    }

    #[test]
    fn read_file_parity_no_trailing_newline() {
        assert_parity("one\ntwo", Some(2), Some(5), false);
        assert_parity("one\ntwo", Some(2), Some(5), true);
    }

    #[test]
    fn read_file_parity_clamps() {
        assert_parity("x\ny\nz", Some(0), Some(0), false);
        assert_parity("x\ny\nz", Some(1), Some(5000), false);
    }

    #[test]
    fn read_file_agent_rejects_bad_sha256() {
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": "x",
            "sha256": "too-short",
            "total_lines": 1,
            "start_line": 1,
            "limit": 2000,
        })
        .to_string();
        let result = read_file_agent_stdout_result_with_options(envelope, None, None, false);
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
    }

    #[test]
    fn read_file_agent_rejects_wrong_start_line() {
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": "x\ny",
            "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "total_lines": 2,
            "start_line": 5,
            "limit": 2000,
        })
        .to_string();
        let result = read_file_agent_stdout_result_with_options(envelope, None, None, false);
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
    }

    #[test]
    fn read_file_agent_rejects_inconsistent_content_lines() {
        // content has 2 segments but total_lines/start/limit imply 3 returned.
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": "a\nb",
            "sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            "total_lines": 4,
            "start_line": 1,
            "limit": 3,
        })
        .to_string();
        let result = read_file_agent_stdout_result_with_options(envelope, Some(1), Some(3), false);
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
    }

    #[test]
    fn read_file_agent_rejects_wrong_field_types() {
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": 7,
            "sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            "total_lines": "1",
            "start_line": 1,
            "limit": 2000,
        })
        .to_string();
        let result = read_file_agent_stdout_result_with_options(envelope, None, None, false);
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
    }

    #[test]
    fn read_file_agent_rejects_malformed_json() {
        let result = read_file_agent_stdout_result_with_options(
            "{ not valid json ".to_string(),
            None,
            None,
            false,
        );
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "malformed_agent_response");
    }

    #[test]
    fn read_file_agent_rejects_oversized_formal_content() {
        let big = "x".repeat(webcodex_workspace::file_read_range::MAX_RANGE_CONTENT_BYTES + 1);
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": big,
            "sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            "total_lines": 1,
            "start_line": 1,
            "limit": 2000,
        })
        .to_string();
        let result = read_file_agent_stdout_result_with_options(envelope, None, None, false);
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "range_too_large");
    }

    #[test]
    fn read_file_agent_strips_huge_padding() {
        let envelope = serde_json::json!({
            "format": "webcodex.file_read_range.v1",
            "content": "hello",
            "sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "total_lines": 1,
            "start_line": 1,
            "limit": 2000,
            "runner_secret": "S".repeat(64_000),
            "canonical_root": "/abs/leak/path",
            "cwd": "/abs/cwd/leak",
        })
        .to_string();
        let result = read_file_agent_stdout_result_with_options(envelope, None, None, false);
        assert!(result.success, "{:?}", result.error);
        // No envelope extras, absolute paths, or secrets leak.
        assert!(result.output.get("runner_secret").is_none());
        assert!(result.output.get("canonical_root").is_none());
        assert!(result.output.get("cwd").is_none());
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert!(!serialized.contains("leak"));
        assert!(!serialized.contains("runner_secret"));
        assert!(
            serialized.len() < webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
        );
    }

    #[test]
    fn read_file_error_no_absolute_path_or_os_text() {
        let result = read_file_failure(
            webcodex_workspace::file_read_range::ReadFileReason::NotFound,
            Some("README.md"),
        );
        assert!(!result.success);
        assert_eq!(result.output["reason_code"], "not_found");
        assert_eq!(result.output["path"], "README.md");
        assert_eq!(result.output["state_changed"], false);
        let serialized = serde_json::to_string(&result.output).unwrap();
        assert!(!serialized.contains("/"));
        assert!(!serialized.contains("os error"));
    }

    #[test]
    fn read_file_large_file_small_range_small_output() {
        let line = "x".repeat(64);
        let body = (0..200_000)
            .map(|_| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let out = local_read(&body, Some(100_000), Some(3), false);
        assert_eq!(out["returned_lines"], 3);
        assert!(out["has_more"].as_bool().unwrap());
        assert_eq!(out["next_start_line"], 100_003);
        let text = out["text"].as_str().unwrap();
        assert!(text.len() < 256);
        let serialized = serde_json::to_string(&out).unwrap();
        assert!(
            serialized.len() < webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES
        );
    }

    #[test]
    fn read_file_range_exceeding_budget_fails_with_reason_code() {
        let line = "x".repeat(1024);
        let body = (0..512)
            .map(|_| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let out = local_read(&body, Some(1), Some(2000), false);
        assert_eq!(out["reason_code"], "range_too_large");
    }

    #[test]
    fn read_file_local_io_errors_keep_stable_reason_codes() {
        assert_eq!(
            io_error_reason(&std::io::Error::from(std::io::ErrorKind::NotFound)),
            ReadFileReason::NotFound
        );
        assert_eq!(
            io_error_reason(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            ReadFileReason::PermissionDenied
        );
        assert_eq!(
            io_error_reason(&std::io::Error::from(std::io::ErrorKind::Other)),
            ReadFileReason::IoError
        );
    }

    fn agent_error_response(message: &str) -> ShellRunResponse {
        ShellRunResponse {
            success: false,
            request_id: "req-read".to_string(),
            client_id: "agent".to_string(),
            cwd: None,
            command_preview: String::new(),
            exit_code: None,
            stdout: None,
            stderr: None,
            duration_ms: Some(1),
            error: Some(message.to_string()),
            request_dispatched: Some(true),
            command_execution_state: None,
        }
    }

    #[test]
    fn read_file_agent_error_mapping_prefers_formal_reason_codes() {
        for reason in [
            ReadFileReason::InvalidPath,
            ReadFileReason::SensitivePath,
            ReadFileReason::NotFound,
            ReadFileReason::NotFile,
            ReadFileReason::PermissionDenied,
            ReadFileReason::InvalidUtf8,
            ReadFileReason::RangeTooLarge,
            ReadFileReason::AgentUnavailable,
            ReadFileReason::Timeout,
            ReadFileReason::MalformedAgentResponse,
            ReadFileReason::IoError,
        ] {
            let response = agent_error_response(&format!("read_file failed: {}", reason.as_str()));
            assert_eq!(map_agent_read_error(&response), reason);
        }
        assert_eq!(
            map_agent_read_error(&agent_error_response("range output too large")),
            ReadFileReason::RangeTooLarge
        );
        assert_eq!(
            map_agent_read_error(&agent_error_response("file_read target is not a file")),
            ReadFileReason::NotFile
        );
        assert_eq!(
            map_agent_read_error(&agent_error_response("invalid unrelated runner detail")),
            ReadFileReason::IoError
        );
    }

    #[test]
    fn read_file_payload_reserves_final_result_and_session_telemetry_budget() {
        fn result_for(content_len: usize) -> ToolResult {
            let range = FileReadRange {
                content: "\0".repeat(content_len),
                sha256: "a".repeat(64),
                total_lines: 1,
                start_line: 1,
                limit: 1,
                returned_lines: 1,
                end_line: Some(1),
                has_more: false,
                next_start_line: None,
            };
            build_read_file_success(&range, false, Some("src/lib.rs"))
        }

        let mut accepted = 0usize;
        let mut rejected = file_read_range::MAX_RANGE_CONTENT_BYTES.saturating_add(1);
        while accepted.saturating_add(1) < rejected {
            let candidate = accepted + (rejected - accepted) / 2;
            if result_for(candidate).success {
                accepted = candidate;
            } else {
                rejected = candidate;
            }
        }
        let rejected_result = result_for(rejected);
        assert!(!rejected_result.success);
        assert_eq!(rejected_result.output["reason_code"], "range_too_large");

        let mut result = result_for(accepted);
        assert!(result.success);
        result.output["session_recorded"] = json!(true);
        result.output["session_id"] = json!(format!("wc_sess_{}", "s".repeat(64)));
        result.output["session_event_id"] = json!(format!("evt_{}", "e".repeat(64)));
        result.output["session_hint"] = json!({
            "has_open_messages": true,
            "open_counts": {
                "guidance": u64::MAX,
                "question": u64::MAX,
                "todo": u64::MAX,
                "risk": u64::MAX
            },
            "highest_priority": "high",
            "suggested_next_tool": "session_discussion_summary"
        });
        let serialized = serde_json::to_vec(&result).unwrap();
        assert!(
            serialized.len() <= file_read_range::MAX_SERIALIZED_OUTPUT_BYTES,
            "final serialized result {} exceeds hard limit",
            serialized.len()
        );
    }

    #[test]
    fn read_file_json_escaping_stays_under_hard_limit() {
        // A range that fits the content budget but is full of quote/backslash
        // characters must not escape past the serialized hard limit.
        let line = "\\\"\\\n".repeat(8);
        let body = line.repeat(2000);
        let out = local_read(&body, Some(1), Some(2), true);
        if out.get("reason_code").and_then(|v| v.as_str()) == Some("range_too_large") {
            return;
        }
        let serialized = serde_json::to_string(&out).unwrap();
        assert!(
            serialized.len() <= webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES,
            "serialized output {} exceeds hard limit",
            serialized.len()
        );
    }
}
