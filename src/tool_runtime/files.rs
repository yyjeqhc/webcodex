use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

#[cfg(test)]
use super::helpers::run_command_sync;
use super::helpers::{
    looks_like_command_timeout, run_command_sync_bounded, shell_escape_simple, shell_join_paths,
    validate_limited_cleanup_paths, validate_project_relative_path, LocalRunFailure,
};
use super::tool_inputs::{ApplyTextEditInput, ApplyTextEditKind};
use super::tool_result::ToolResult;
use super::{SearchResultMode, ToolRuntime};
use crate::artifact_policy::{
    has_safe_octet_stream_artifact_extension, octet_stream_safe_extension_error,
};
use crate::project_overview::{
    effective_project_overview_limit, effective_project_overview_max_depth,
    normalize_project_overview_path,
};
use crate::projects::ProjectConfig;
use crate::shell_protocol::{ShellFileOpRequest, ShellRunRequest};

#[cfg(test)]
pub(crate) fn read_file_content_result(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
) -> ToolResult {
    read_file_content_result_with_options(content, start_line, limit, false)
}

pub(crate) fn read_file_content_result_with_options(
    content: String,
    start_line: Option<usize>,
    limit: Option<usize>,
    with_line_numbers: bool,
) -> ToolResult {
    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let eff_start = start_line.unwrap_or(1).max(1);
    let eff_limit = limit.unwrap_or(2000).clamp(1, 2000);
    if eff_start > total_lines {
        let mut output = json!({
            "content": "",
            "total_lines": total_lines,
            "start_line": eff_start,
            "limit": eff_limit,
        });
        if with_line_numbers {
            add_line_number_fields(&mut output, eff_start, &Vec::<&str>::new());
        }
        return ToolResult::ok(output);
    }
    let start_idx = eff_start - 1;
    let end_idx = (start_idx + eff_limit).min(total_lines);
    let selected_lines = &all_lines[start_idx..end_idx];
    let slice = selected_lines.join("\n");
    let mut output = json!({
        "content": slice,
        "total_lines": total_lines,
        "start_line": eff_start,
        "limit": eff_limit,
    });
    if with_line_numbers {
        add_line_number_fields(&mut output, eff_start, selected_lines);
    }
    ToolResult::ok(output)
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
    let trimmed = stdout.trim();
    if let Ok(mut value) = serde_json::from_str::<Value>(trimmed) {
        if value.get("format").and_then(|format| format.as_str())
            == Some("webcodex.file_read_range.v1")
        {
            if with_line_numbers {
                add_agent_read_file_line_number_fields(&mut value, start_line, limit);
            }
            return ToolResult::ok(value);
        }
    }
    read_file_content_result_with_options(stdout, start_line, limit, with_line_numbers)
}

pub(crate) fn effective_read_file_range(
    start_line: Option<usize>,
    limit: Option<usize>,
) -> (usize, usize, usize) {
    let eff_start = start_line.unwrap_or(1).max(1);
    let eff_limit = limit.unwrap_or(2000).clamp(1, 2000);
    let eff_end = eff_start.saturating_add(eff_limit).saturating_sub(1);
    (eff_start, eff_limit, eff_end)
}

/// Parse the stdout of a best-effort agent `file_read` for an instruction
/// candidate. Recognizes the `webcodex.file_read_range.v1` JSON envelope
/// (which carries the true `total_lines` of the file) and falls back to
/// treating stdout as raw text (where the returned line count is a lower
/// bound on the true total). Returns `None` for empty/unusable output so the
/// caller skips to the next candidate.
fn parse_instruction_agent_stdout(stdout: String) -> Option<(String, usize)> {
    let trimmed = stdout.trim();
    if !trimmed.is_empty() {
        if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
            if value.get("format").and_then(|format| format.as_str())
                == Some("webcodex.file_read_range.v1")
            {
                let content = value.get("content").and_then(|c| c.as_str())?.to_string();
                let total_lines = value
                    .get("total_lines")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0) as usize;
                if content_is_empty_instruction(&content) {
                    return None;
                }
                return Some((content, total_lines));
            }
        }
    }
    if content_is_empty_instruction(&stdout) {
        return None;
    }
    let total_lines = stdout.lines().count();
    Some((stdout, total_lines))
}

/// True when an instruction body carries no meaningful content (empty or
/// whitespace-only). Empty instruction files are skipped so a later candidate
/// can win.
fn content_is_empty_instruction(content: &str) -> bool {
    content.trim().is_empty()
}

fn add_line_number_fields<T: AsRef<str>>(output: &mut Value, start_line: usize, texts: &[T]) {
    let lines: Vec<Value> = texts
        .iter()
        .enumerate()
        .map(|(idx, text)| {
            json!({
                "line": start_line.saturating_add(idx),
                "text": text.as_ref(),
            })
        })
        .collect();
    let numbered_text = texts
        .iter()
        .enumerate()
        .map(|(idx, text)| format!("{} | {}", start_line.saturating_add(idx), text.as_ref()))
        .collect::<Vec<_>>()
        .join("\n");
    if let Some(obj) = output.as_object_mut() {
        obj.insert("numbered_text".to_string(), Value::String(numbered_text));
        obj.insert("lines".to_string(), Value::Array(lines));
    }
}

fn add_agent_read_file_line_number_fields(
    output: &mut Value,
    request_start_line: Option<usize>,
    request_limit: Option<usize>,
) {
    let content = output
        .get("content")
        .and_then(|content| content.as_str())
        .unwrap_or("");
    let start_line = output
        .get("start_line")
        .and_then(|line| line.as_u64())
        .and_then(|line| usize::try_from(line).ok())
        .or(request_start_line)
        .unwrap_or(1)
        .max(1);
    let limit = output
        .get("limit")
        .and_then(|limit| limit.as_u64())
        .and_then(|limit| usize::try_from(limit).ok())
        .or(request_limit)
        .unwrap_or(2000)
        .clamp(1, 2000);
    let selected_count = output
        .get("total_lines")
        .and_then(|total| total.as_u64())
        .and_then(|total| usize::try_from(total).ok())
        .map(|total_lines| {
            if start_line > total_lines {
                0
            } else {
                total_lines
                    .saturating_sub(start_line)
                    .saturating_add(1)
                    .min(limit)
            }
        });
    let texts = line_texts_from_content(content, selected_count);
    add_line_number_fields(output, start_line, &texts);
}

fn line_texts_from_content(content: &str, selected_count: Option<usize>) -> Vec<String> {
    match selected_count {
        Some(0) => Vec::new(),
        Some(count) => {
            let mut texts: Vec<String> = content.split('\n').map(str::to_string).collect();
            texts.resize(count, String::new());
            texts.truncate(count);
            texts
        }
        None if content.is_empty() => Vec::new(),
        None => content.lines().map(str::to_string).collect(),
    }
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
    const PROTECTED_GLOBS: &[&str] = &[
        ".env",
        "**/.env",
        ".env.*",
        "**/.env.*",
        "agent.toml",
        "**/agent.toml",
        "webcodex.env",
        "**/webcodex.env",
        "*.pem",
        "**/*.pem",
        "*.key",
        "**/*.key",
    ];
    let normalized = glob.strip_prefix("./").unwrap_or(glob);
    if PROTECTED_GLOBS.contains(&normalized) {
        return true;
    }
    normalized.split('/').any(|component| {
        !component
            .chars()
            .any(|ch| matches!(ch, '*' | '?' | '[' | ']'))
            && (matches!(
                component,
                ".git"
                    | "target"
                    | "node_modules"
                    | "secrets"
                    | "tokens"
                    | "agent.toml"
                    | "webcodex.env"
                    | ".env"
            ) || component.starts_with(".env.")
                || component.ends_with(".pem")
                || component.ends_with(".key"))
    })
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
#[cfg_attr(not(test), allow(dead_code))]
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

#[cfg_attr(not(test), allow(dead_code))]
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
/// status even when output is bounded by `head`. Status files live only under
/// a safe absolute temp dir (never the project worktree) and are cleaned up via
/// trap + explicit removal.
///
/// Uses pure shell noclobber file creation instead of `mktemp` so the command
/// still works when PATH is restricted to a tool sandbox without coreutils.
///
/// Caller must ensure `head_cmd` is set (see `search_head_resolution_shell`).
fn wrap_search_project_text_backend_command(
    backend: &str,
    backend_cmd: &str,
    head_lines: usize,
) -> String {
    let marker = search_project_text_marker_command(backend, false);
    // Capture backend status via a side-channel file so `head` cannot mask
    // exit >= 2. SIGPIPE (141) may occur when head closes early on large
    // successful result sets and is treated as success by the Rust layer.
    // Head's own non-zero exit is never treated as success.
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
}} | "$head_cmd" -n {head_lines}
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
{marker}
exit "$status""#,
        marker = marker,
        tmpdir = search_status_tmpdir_shell(),
        backend_cmd = backend_cmd,
        head_lines = head_lines,
    )
}

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
    format!(
        "rg {mode_args} --color never --hidden --no-ignore --sort path {globs} -e {pattern} -- {target} 2>/dev/null"
    )
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
    let rg = wrap_search_project_text_backend_command("rg", &ripgrep_search_command(options), head);
    let fallback = if options.requires_ripgrep() {
        search_project_text_marker_command("grep", true)
    } else {
        wrap_search_project_text_backend_command("grep", &grep_search_command(options), head)
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
    Path::new(path).components().any(|component| {
        let Some(component) = component.as_os_str().to_str() else {
            return false;
        };
        matches!(
            component,
            ".git"
                | "target"
                | "node_modules"
                | "secrets"
                | "tokens"
                | "agent.toml"
                | "webcodex.env"
                | ".env"
        ) || component.starts_with(".env.")
            || component.ends_with(".pem")
            || component.ends_with(".key")
    })
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

#[derive(Debug)]
struct SearchResult {
    backend: String,
    data: SearchResultData,
    truncated: bool,
    truncation_reason: Option<&'static str>,
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
            if !matches!(backend, "rg" | "grep" | "native") {
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
    let path = path.strip_prefix("./").unwrap_or(path).to_string();
    if is_search_project_text_excluded_path(&path) {
        return None;
    }
    Some(SearchLineRecord {
        path,
        line: line_no.parse().ok()?,
        text: text.to_string(),
        is_match: separator == ':',
    })
}

fn parse_search_line_records(stdout: &str) -> Vec<SearchLineRecord> {
    stdout
        .lines()
        .filter_map(parse_search_line_record)
        .collect()
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

fn parse_file_paths(stdout: &str, limit: usize) -> (Vec<SearchFile>, bool) {
    let mut paths = Vec::<String>::new();
    for line in stdout.lines() {
        if line.starts_with("[output truncated to last ") {
            continue;
        }
        if serde_json::from_str::<Value>(line).is_ok() {
            continue;
        }
        let path = line.trim_end_matches('\r');
        let path = path.strip_prefix("./").unwrap_or(path);
        if path.is_empty()
            || is_search_project_text_excluded_path(path)
            || paths.iter().any(|existing| existing == path)
        {
            continue;
        }
        paths.push(path.to_string());
    }
    let truncated = paths.len() > limit;
    paths.truncate(limit);
    (
        paths.into_iter().map(|path| SearchFile { path }).collect(),
        truncated,
    )
}

fn parse_file_counts(stdout: &str, limit: usize) -> (Vec<SearchFileCount>, u64, bool) {
    let mut counts = Vec::<(String, u64)>::new();
    for line in stdout.lines() {
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
        let path = path.strip_prefix("./").unwrap_or(path);
        if is_search_project_text_excluded_path(path) {
            continue;
        }
        if let Some((_, existing)) = counts.iter_mut().find(|(existing, _)| existing == path) {
            *existing = existing.saturating_add(count);
        } else {
            counts.push((path.to_string(), count));
        }
    }
    let truncated = counts.len() > limit;
    counts.truncate(limit);
    let returned_match_count = counts.iter().map(|(_, count)| *count).sum();
    (
        counts
            .into_iter()
            .map(|(path, match_count)| SearchFileCount { path, match_count })
            .collect(),
        returned_match_count,
        truncated,
    )
}

fn search_stdout_was_transport_truncated(stdout: &str) -> bool {
    stdout.starts_with("[output truncated to last ")
}

fn parse_search_result(stdout: &str, options: &SearchOptions, backend: String) -> SearchResult {
    let transport_truncated = search_stdout_was_transport_truncated(stdout);
    let (data, limit_truncated) = match options.result_mode {
        SearchResultMode::Matches => {
            let records = parse_search_line_records(stdout);
            let (matches, truncated) = search_matches_from_records(&records, options);
            (SearchResultData::Matches(matches), truncated)
        }
        SearchResultMode::FilesWithMatches => {
            let (files, truncated) = parse_file_paths(stdout, options.limit);
            (SearchResultData::FilesWithMatches(files), truncated)
        }
        SearchResultMode::Count => {
            let (files, returned_match_count, truncated) = parse_file_counts(stdout, options.limit);
            (
                SearchResultData::Count {
                    files,
                    returned_match_count,
                    count_complete: !truncated && !transport_truncated,
                },
                truncated,
            )
        }
    };
    let truncated = limit_truncated || transport_truncated;
    SearchResult {
        backend,
        data,
        truncated,
        truncation_reason: if transport_truncated {
            Some("transport")
        } else if limit_truncated {
            Some("limit")
        } else {
            None
        },
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
        return search_timeout_tool_result(options, Some(backend_status.backend.as_str()));
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
    output["truncation_reason"] = result.truncation_reason.map_or(Value::Null, Value::from);
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

/// Maximum accepted size for a single `replace_in_file` `old`/`new` field.
/// Generous for text edits while bounding memory and the agent stdin payload.
pub(crate) const MAX_REPLACE_FIELD_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum accepted size for `write_project_file` `content`.
pub(crate) const MAX_WRITE_CONTENT_BYTES: usize = 256 * 1024; // 256 KiB

/// Maximum accepted size for line-edit expected prefix guards. Keep this well
/// below the file-op payload budget so oversized optimistic-concurrency guards
/// fail locally before any agent request is enqueued.
pub(crate) const MAX_EXPECTED_PREFIX_BYTES: usize = 64 * 1024; // 64 KiB

/// Maximum number of edits accepted by a single `apply_text_edits` call.
pub(crate) const MAX_APPLY_TEXT_EDITS: usize = 20;

/// Maximum byte size of a single `old_text`/`new_text`/`anchor_text` field in
/// an `apply_text_edits` edit.
pub(crate) const MAX_APPLY_TEXT_EDIT_FIELD_BYTES: usize = 512 * 1024; // 512 KiB

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

/// Validate a project-relative file path for the Phase 4 structured edit
/// tools (`replace_in_file`, `write_project_file`). Unlike the patch preflight
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
    for comp in path.to_lowercase().split('/') {
        if matches!(
            comp,
            ".git" | "target" | "node_modules" | "secrets" | "tokens"
        ) {
            return true;
        }
        if comp == ".env" || comp.starts_with(".env") || comp.ends_with(".pem") {
            return true;
        }
    }
    false
}

fn validate_artifact_mime(mime_type: Option<&str>) -> Result<Option<String>, String> {
    let Some(mime) = mime_type.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
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
        _ => Err(format!("unsupported mime_type '{}'; allowed first-pass artifact MIME types are image/png, image/jpeg, image/webp, application/pdf, application/zip, text/plain, text/csv, application/json", mime)),
    }
}

fn validate_artifact_mime_for_path(
    path: &str,
    mime_type: Option<&str>,
) -> Result<Option<String>, String> {
    let mime_type = validate_artifact_mime(mime_type)?;
    if matches!(mime_type.as_deref(), Some("application/octet-stream")) {
        if !has_safe_octet_stream_artifact_extension(path) {
            return Err(octet_stream_safe_extension_error());
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

/// True if `path` contains a sensitive component for the structured edit
/// tools. Matching is component-wise (split on `/`) so legitimate filenames
/// that merely contain a sensitive substring (e.g. `targeting.md`) are NOT
/// rejected. A component is sensitive if it equals one of the guarded names or
/// starts with `.env` / `agent.toml` / `webcodex.env` (catching backups
/// like `.env.local` or `agent.toml.bak`).
pub(crate) fn is_sensitive_edit_path(path: &str) -> bool {
    for comp in path.to_lowercase().split('/') {
        if matches!(
            comp,
            ".git"
                | "target"
                | "node_modules"
                | "projects.d"
                | "agent.toml"
                | "webcodex.env"
                | ".env"
        ) {
            return true;
        }
        if comp.starts_with(".env")
            || comp.starts_with("agent.toml")
            || comp.starts_with("webcodex.env")
        {
            return true;
        }
    }
    false
}

/// True if `s` is a lowercase 64-character hex string (a sha256 digest).
pub(crate) fn is_hex_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineEditOperation {
    Replace,
    Insert,
    Delete,
}

#[cfg(test)]
fn normalize_line_edit_text(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{}\n", text)
    }
}

#[cfg(test)]
fn line_edit_text_line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

/// Apply a structured line edit to UTF-8 `content` and return the new content
/// plus the JSON payload shared by the runtime and tests. `new_sha256` is the
/// sha256 digest of the entire file after the operation. Range tools hash the
/// original replaced/deleted text; insert hashes the anchor line (or empty EOF
/// anchor).
#[cfg(test)]
pub(crate) fn apply_line_edit_content(
    content: &str,
    path: &str,
    op: LineEditOperation,
    start_line: Option<usize>,
    end_line: Option<usize>,
    line: Option<usize>,
    text: &str,
    expected_sha256: Option<&str>,
    expected_prefix: Option<&str>,
) -> Result<(String, Value), String> {
    let lines: Vec<&str> = if content.is_empty() {
        Vec::new()
    } else {
        content.split_inclusive('\n').collect()
    };
    let total_lines = lines.len();
    let (old_text, new_text, new_content, old_line_count, new_line_count, range) = match op {
        LineEditOperation::Replace | LineEditOperation::Delete => {
            let start = start_line.ok_or_else(|| "invalid line range".to_string())?;
            let end = end_line.ok_or_else(|| "invalid line range".to_string())?;
            if start == 0 || end < start || end > total_lines {
                return Err("invalid line range".to_string());
            }
            let old = lines[start - 1..end].concat();
            let replacement = if op == LineEditOperation::Delete {
                String::new()
            } else {
                normalize_line_edit_text(text)
            };
            let mut next = String::new();
            next.push_str(&lines[..start - 1].concat());
            next.push_str(&replacement);
            next.push_str(&lines[end..].concat());
            let inserted_lines = line_edit_text_line_count(&replacement);
            (
                old,
                replacement,
                next,
                end - start + 1,
                inserted_lines,
                Some((start, end)),
            )
        }
        LineEditOperation::Insert => {
            let at = line.ok_or_else(|| "line out of range".to_string())?;
            if at == 0 || at > total_lines + 1 {
                return Err("line out of range".to_string());
            }
            let anchor = if at <= total_lines {
                lines[at - 1].to_string()
            } else {
                String::new()
            };
            let insertion = normalize_line_edit_text(text);
            let mut next = String::new();
            next.push_str(&lines[..at - 1].concat());
            next.push_str(&insertion);
            next.push_str(&lines[at - 1..].concat());
            let inserted_lines = line_edit_text_line_count(&insertion);
            let anchor_count = if at <= total_lines { 1 } else { 0 };
            (anchor, insertion, next, anchor_count, inserted_lines, None)
        }
    };
    let old_sha256 = sha256_hex_bytes(old_text.as_bytes());
    if let Some(expected) = expected_sha256 {
        if old_sha256 != expected {
            let label = match op {
                LineEditOperation::Insert => "expected_anchor_sha256 mismatch",
                _ => "expected_old_sha256 mismatch",
            };
            return Err(recoverable_write_rejection(label));
        }
    }
    if let Some(prefix) = expected_prefix {
        if !old_text.starts_with(prefix) {
            let label = match op {
                LineEditOperation::Insert => "expected_anchor_prefix mismatch",
                _ => "expected_old_prefix mismatch",
            };
            return Err(recoverable_write_rejection(label));
        }
    }
    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let mut output = json!({
        "path": path,
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "old_line_count": old_line_count,
        "new_line_count": new_line_count,
        "bytes_written": new_content.len(),
        "changed": new_content != content,
    });
    if let Some((start, end)) = range {
        output["start_line"] = json!(start);
        output["end_line"] = json!(end);
    } else if let Some(at) = line {
        output["line"] = json!(at);
    }
    let _ = new_text;
    Ok((new_content, output))
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

fn validate_anchor_edit_common(path: &str, text: &str) -> Result<(), String> {
    validate_edit_file_path(path)?;
    if text.contains('\0') {
        return Err("text cannot contain NUL bytes".to_string());
    }
    if text.len() > MAX_WRITE_CONTENT_BYTES {
        return Err(format!(
            "text too large; maximum is {} bytes",
            MAX_WRITE_CONTENT_BYTES
        ));
    }
    Ok(())
}

fn parse_anchor_edit_stdout(op: &str, stdout: Option<String>) -> Result<Value, String> {
    let stdout = stdout.unwrap_or_default();
    let stdout = stdout.trim();
    if stdout.is_empty() {
        return Err(format!(
            "agent anchor edit returned empty stdout for {op}; connected agent may not support this file op or transport dispatch may have routed it incorrectly"
        ));
    }
    serde_json::from_str(stdout).map_err(|e| {
        format!(
            "agent anchor edit returned invalid JSON: {} (got: {})",
            e,
            &stdout[..stdout.len().min(200)]
        )
    })
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
        let command = format!("rm -f -- {}", shell_join_paths(&paths));
        let result = self.run_shell(project, command, Some(30), None).await;
        if result.success {
            let stdout_present = result
                .output
                .get("stdout")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            let stderr_present = result
                .output
                .get("stderr")
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

    pub(crate) async fn replace_in_file(
        &self,
        project: String,
        path: String,
        old: String,
        new: String,
        expected_replacements: Option<i64>,
        allow_multiple: Option<bool>,
    ) -> ToolResult {
        // ---- Input validation (before project resolution) ----
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if old.is_empty() {
            return ToolResult::err("old must be non-empty");
        }
        if old.contains('\0') || new.contains('\0') {
            return ToolResult::err("old and new cannot contain NUL bytes");
        }
        if old.len() > MAX_REPLACE_FIELD_BYTES || new.len() > MAX_REPLACE_FIELD_BYTES {
            return ToolResult::err(format!(
                "old/new too large; maximum is {} bytes each",
                MAX_REPLACE_FIELD_BYTES
            ));
        }
        let expected = expected_replacements.unwrap_or(1);
        if expected < 1 {
            return ToolResult::err("expected_replacements must be >= 1");
        }
        let allow_multi = allow_multiple.unwrap_or(false);
        if !allow_multi && expected != 1 {
            return ToolResult::err("expected_replacements must be 1 when allow_multiple is false");
        }

        // ---- Project resolution (agent-registered only) ----
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "replace_in_file requires an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };

        let payload = json!({
            "path": path.clone(),
            "old": old,
            "new": new,
            "expected_replacements": expected,
            "allow_multiple": allow_multi,
        });
        let obj = match self
            .run_agent_json_file_op(
                client_id,
                proj.path.clone(),
                path.clone(),
                "replace_in_file",
                payload,
                "replace_in_file",
            )
            .await
        {
            Ok(v) => v,
            Err(e) => return ToolResult::err(recoverable_write_rejection(e)),
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(recoverable_write_rejection(err)),
            };
        }
        ToolResult::ok(obj)
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

    pub(crate) async fn read_project_artifact(
        &self,
        project: String,
        path: String,
        encoding: Option<String>,
        offset: Option<usize>,
        length: Option<usize>,
        max_bytes: Option<usize>,
    ) -> ToolResult {
        if let Err(e) = validate_artifact_file_path(&path) {
            return artifact_policy_rejected_result(&path, e);
        }
        let encoding = encoding.unwrap_or_else(|| "base64".to_string());
        if encoding != "base64" {
            return ToolResult::err("unsupported encoding; only 'base64' is currently supported");
        }
        let offset = offset.unwrap_or(0);
        let mut length =
            length.unwrap_or_else(|| max_bytes.unwrap_or(DEFAULT_READ_PROJECT_ARTIFACT_LENGTH));
        if let Some(max_bytes) = max_bytes {
            if max_bytes == 0 {
                return ToolResult::err("max_bytes must be at least 1");
            }
            length = length.min(max_bytes);
        }
        if length == 0 {
            return ToolResult::err("length must be at least 1");
        }
        if length > MAX_READ_PROJECT_ARTIFACT_LENGTH {
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
        let payload = json!({
            "path": path.clone(),
            "offset": offset,
            "length": length,
            "max_file_bytes": MAX_PROJECT_ARTIFACT_BYTES,
        });
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

    fn validate_line_edit_common(
        path: &str,
        text: &str,
        expected_sha256: Option<&str>,
        expected_prefix: Option<&str>,
    ) -> Result<(), String> {
        validate_edit_file_path(path)?;
        if text.contains('\0') {
            return Err("text cannot contain NUL bytes".to_string());
        }
        if text.len() > MAX_WRITE_CONTENT_BYTES {
            return Err(format!(
                "text too large; maximum is {} bytes",
                MAX_WRITE_CONTENT_BYTES
            ));
        }
        if let Some(hash) = expected_sha256 {
            if !is_hex_sha256(hash) {
                return Err(
                    "expected sha256 must be a lowercase 64-char hex sha256 digest".to_string(),
                );
            }
        }
        if let Some(prefix) = expected_prefix {
            if prefix.contains('\0') {
                return Err("expected prefix cannot contain NUL bytes".to_string());
            }
            if prefix.len() > MAX_EXPECTED_PREFIX_BYTES {
                return Err(format!(
                    "expected prefix too large; maximum is {} bytes",
                    MAX_EXPECTED_PREFIX_BYTES
                ));
            }
        }
        Ok(())
    }

    async fn run_line_edit(
        &self,
        project: String,
        path: String,
        op: &str,
        content: Option<String>,
        start_line: Option<usize>,
        end_line: Option<usize>,
        line: Option<usize>,
        expected_sha256: Option<String>,
        expected_prefix: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "line edit tools require an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: op.to_string(),
                    client_id,
                    path: path.clone(),
                    cwd: Some(proj.path.clone()),
                    content,
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256,
                    expected_prefix,
                    start_line,
                    end_line,
                    line,
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
                return ToolResult::err("agent line edit request was dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("timed out waiting for agent line edit");
            }
        };
        if let Some(e) = resp.error {
            return ToolResult::err(recoverable_write_rejection(e));
        }
        if resp.exit_code != Some(0) {
            return ToolResult::err(recoverable_write_rejection(resp.stderr.unwrap_or_else(
                || format!("agent line edit failed with code {:?}", resp.exit_code),
            )));
        }
        let stdout = resp.stdout.unwrap_or_default();
        let stdout = stdout.trim();
        let obj: Value = match serde_json::from_str(stdout) {
            Ok(v) => v,
            Err(e) => {
                return ToolResult::err(format!(
                    "agent line edit returned invalid JSON: {} (got: {})",
                    e,
                    &stdout[..stdout.len().min(200)]
                ))
            }
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(recoverable_write_rejection(err)),
            };
        }
        if obj.get("path").is_none() {
            let mut obj = obj;
            obj["path"] = json!(path);
            return ToolResult::ok(obj);
        }
        ToolResult::ok(obj)
    }

    async fn run_anchor_edit(
        &self,
        project: String,
        path: String,
        op: &str,
        old_text: Option<String>,
        pattern: Option<String>,
        content: String,
        expected_sha256: Option<String>,
    ) -> ToolResult {
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if !proj.is_agent() {
            return ToolResult::err(
                "anchor edit tools require an agent-registered project; \
                 server-configured projects are not supported",
            );
        }
        let client_id = match proj.agent_client_id() {
            Ok(id) => id.to_string(),
            Err(e) => return ToolResult::err(e),
        };
        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: op.to_string(),
                    client_id,
                    path: path.clone(),
                    cwd: Some(proj.path.clone()),
                    content: Some(content),
                    max_bytes: None,
                    old_text,
                    pattern,
                    expected_sha256,
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
                return ToolResult::err("agent anchor edit request was dropped");
            }
            Err(_) => {
                self.shell_clients.cancel_request(&request_id).await;
                return ToolResult::err("timed out waiting for agent anchor edit");
            }
        };
        if let Some(e) = resp.error {
            return ToolResult::err(recoverable_write_rejection(e));
        }
        if resp.exit_code != Some(0) {
            return ToolResult::err(recoverable_write_rejection(resp.stderr.unwrap_or_else(
                || format!("agent anchor edit failed with code {:?}", resp.exit_code),
            )));
        }
        let obj = match parse_anchor_edit_stdout(op, resp.stdout) {
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

    pub(crate) async fn replace_exact_block(
        &self,
        project: String,
        path: String,
        old_text: String,
        new_text: String,
        expected_old_sha256: Option<String>,
    ) -> ToolResult {
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if let Err(e) = validate_anchor_edit_common(&path, &new_text) {
            return ToolResult::err(e);
        }
        if old_text.is_empty() {
            return ToolResult::err("old_text must be non-empty");
        }
        if old_text.contains('\0') {
            return ToolResult::err("old_text cannot contain NUL bytes");
        }
        if old_text.len() > MAX_REPLACE_FIELD_BYTES {
            return ToolResult::err(format!(
                "old_text too large; maximum is {} bytes",
                MAX_REPLACE_FIELD_BYTES
            ));
        }
        if let Some(hash) = expected_old_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_old_sha256 must be a lowercase 64-char hex sha256 digest",
                );
            }
        }
        self.run_anchor_edit(
            project,
            path,
            "replace_exact_block",
            Some(old_text),
            None,
            new_text,
            expected_old_sha256,
        )
        .await
    }

    pub(crate) async fn insert_around_pattern(
        &self,
        project: String,
        path: String,
        pattern: String,
        text: String,
        op: &str,
    ) -> ToolResult {
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if let Err(e) = validate_anchor_edit_common(&path, &text) {
            return ToolResult::err(e);
        }
        if pattern.is_empty() {
            return ToolResult::err("pattern must be non-empty literal pattern");
        }
        if text.is_empty() {
            return ToolResult::err("Rejected before write: inserted text must not be empty.\nNo files were modified.\nRetry guidance: provide the exact text to insert, including any intended newlines.");
        }
        if pattern.contains('\0') {
            return ToolResult::err("pattern cannot contain NUL bytes");
        }
        if pattern.len() > MAX_REPLACE_FIELD_BYTES {
            return ToolResult::err(format!(
                "pattern too large; maximum is {} bytes",
                MAX_REPLACE_FIELD_BYTES
            ));
        }
        self.run_anchor_edit(project, path, op, None, Some(pattern), text, None)
            .await
    }
    pub(crate) async fn replace_line_range(
        &self,
        project: String,
        path: String,
        start_line: usize,
        end_line: usize,
        new_text: String,
        expected_old_sha256: Option<String>,
        expected_old_prefix: Option<String>,
    ) -> ToolResult {
        if start_line == 0 || end_line < start_line {
            return ToolResult::err("invalid line range");
        }
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if let Err(e) = Self::validate_line_edit_common(
            &path,
            &new_text,
            expected_old_sha256.as_deref(),
            expected_old_prefix.as_deref(),
        ) {
            return ToolResult::err(e);
        }
        self.run_line_edit(
            project,
            path,
            "replace_line_range",
            Some(new_text),
            Some(start_line),
            Some(end_line),
            None,
            expected_old_sha256,
            expected_old_prefix,
        )
        .await
    }

    pub(crate) async fn insert_at_line(
        &self,
        project: String,
        path: String,
        line: usize,
        text: String,
        expected_anchor_sha256: Option<String>,
        expected_anchor_prefix: Option<String>,
    ) -> ToolResult {
        if line == 0 {
            return ToolResult::err("line out of range");
        }
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if let Err(e) = Self::validate_line_edit_common(
            &path,
            &text,
            expected_anchor_sha256.as_deref(),
            expected_anchor_prefix.as_deref(),
        ) {
            return ToolResult::err(e);
        }
        self.run_line_edit(
            project,
            path,
            "insert_at_line",
            Some(text),
            None,
            None,
            Some(line),
            expected_anchor_sha256,
            expected_anchor_prefix,
        )
        .await
    }

    pub(crate) async fn delete_line_range(
        &self,
        project: String,
        path: String,
        start_line: usize,
        end_line: usize,
        expected_old_sha256: Option<String>,
        expected_old_prefix: Option<String>,
    ) -> ToolResult {
        if start_line == 0 || end_line < start_line {
            return ToolResult::err("invalid line range");
        }
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if let Err(e) = Self::validate_line_edit_common(
            &path,
            "",
            expected_old_sha256.as_deref(),
            expected_old_prefix.as_deref(),
        ) {
            return ToolResult::err(e);
        }
        self.run_line_edit(
            project,
            path,
            "delete_line_range",
            None,
            Some(start_line),
            Some(end_line),
            None,
            expected_old_sha256,
            expected_old_prefix,
        )
        .await
    }

    /// Apply a bounded batch of atomic text edits to a single UTF-8 file via
    /// the owning agent. All input validation (path safety, edit count, field
    /// sizes, sha format, field presence per kind) happens server-side before
    /// any agent request is enqueued. The edits, dry_run flag, and optional
    /// whole-file sha guard travel to the agent as a JSON payload in the file
    /// op `content` field; the agent reads the file, enforces unique-match /
    /// no-overlap semantics, and writes atomically (temp + rename) only when
    /// every edit validates.
    pub(crate) async fn apply_text_edits(
        &self,
        project: String,
        path: String,
        edits: Vec<ApplyTextEditInput>,
        dry_run: Option<bool>,
        expected_file_sha256: Option<String>,
    ) -> ToolResult {
        if let Err(e) = validate_edit_file_path(&path) {
            return super::permissions::edit_path_policy_rejected_result(&path, e);
        }
        if edits.is_empty() {
            return ToolResult::err("edits must contain at least one edit");
        }
        if edits.len() > MAX_APPLY_TEXT_EDITS {
            return ToolResult::err(format!(
                "too many edits; maximum is {}",
                MAX_APPLY_TEXT_EDITS
            ));
        }
        for (index, edit) in edits.iter().enumerate() {
            let kind = edit.kind;
            let index_str = index;
            // Bound + NUL check for a single optional field; returns an error
            // message on failure.
            let validate_field = |label: &str, value: &Option<String>| -> Option<String> {
                if let Some(v) = value {
                    if v.contains('\0') {
                        return Some(format!(
                            "edit {} ({}): {} cannot contain NUL bytes",
                            index_str,
                            kind.as_str(),
                            label
                        ));
                    }
                    if v.len() > MAX_APPLY_TEXT_EDIT_FIELD_BYTES {
                        return Some(format!(
                            "edit {} ({}): {} too large; maximum is {} bytes",
                            index_str,
                            kind.as_str(),
                            label,
                            MAX_APPLY_TEXT_EDIT_FIELD_BYTES
                        ));
                    }
                }
                None
            };
            match kind {
                ApplyTextEditKind::ReplaceExact => {
                    if let Some(msg) = validate_field("old_text", &edit.old_text) {
                        return ToolResult::err(msg);
                    }
                    if let Some(msg) = validate_field("new_text", &edit.new_text) {
                        return ToolResult::err(msg);
                    }
                    if edit.old_text.as_deref().filter(|v| !v.is_empty()).is_none() {
                        return ToolResult::err(format!(
                            "edit {} (replace_exact): old_text must be non-empty",
                            index
                        ));
                    }
                }
                ApplyTextEditKind::DeleteExact => {
                    if let Some(msg) = validate_field("old_text", &edit.old_text) {
                        return ToolResult::err(msg);
                    }
                    if edit.old_text.as_deref().filter(|v| !v.is_empty()).is_none() {
                        return ToolResult::err(format!(
                            "edit {} (delete_exact): old_text must be non-empty",
                            index
                        ));
                    }
                }
                ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
                    if let Some(msg) = validate_field("anchor_text", &edit.anchor_text) {
                        return ToolResult::err(msg);
                    }
                    if let Some(msg) = validate_field("new_text", &edit.new_text) {
                        return ToolResult::err(msg);
                    }
                    if edit
                        .anchor_text
                        .as_deref()
                        .filter(|v| !v.is_empty())
                        .is_none()
                    {
                        return ToolResult::err(format!(
                            "edit {} ({}): anchor_text must be non-empty",
                            index,
                            kind.as_str()
                        ));
                    }
                    if edit.new_text.as_deref().filter(|v| !v.is_empty()).is_none() {
                        return ToolResult::err(format!(
                            "edit {} ({}): new_text must be non-empty",
                            index,
                            kind.as_str()
                        ));
                    }
                }
            }
        }
        if let Some(hash) = expected_file_sha256.as_deref() {
            if !is_hex_sha256(hash) {
                return ToolResult::err(
                    "expected_file_sha256 must be a lowercase 64-char hex sha256 digest",
                );
            }
        }

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

        // Serialize the full edit payload into the file-op `content` field so
        // no shell-protocol field additions are needed. The agent handler for
        // `file_apply_text_edits` deserializes this one field.
        let payload = json!({
            "edits": edits,
            "dry_run": dry_run.unwrap_or(false),
            "expected_file_sha256": expected_file_sha256,
        });
        let serialized = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => return ToolResult::err(format!("failed to serialize edits payload: {}", e)),
        };

        let wait_timeout = 60_u64;
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "apply_text_edits".to_string(),
                    client_id,
                    path: path.clone(),
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
        let stdout = resp.stdout.unwrap_or_default();
        let stdout = stdout.trim();
        let obj: Value = match serde_json::from_str(stdout) {
            Ok(v) => v,
            Err(e) => {
                return ToolResult::err(format!(
                    "agent apply_text_edits returned invalid JSON: {} (got: {})",
                    e,
                    &stdout[..stdout.len().min(200)]
                ))
            }
        };
        if let Some(err) = obj
            .get("error")
            .and_then(|e| e.as_str())
            .map(str::to_string)
        {
            return ToolResult {
                success: false,
                output: obj,
                error: Some(recoverable_write_rejection(err)),
            };
        }
        let mut obj = obj;
        if obj.get("path").is_none() {
            obj["path"] = json!(path);
        }
        ToolResult::ok(obj)
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
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
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
                Err(e) => return ToolResult::err(e),
            };
            return match tokio::time::timeout(Duration::from_secs(wait_timeout + 2), rx).await {
                Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                    read_file_agent_stdout_result_with_options(
                        resp.stdout.unwrap_or_default(),
                        start_line,
                        limit,
                        with_line_numbers,
                    )
                }
                Ok(Ok(resp)) => ToolResult::err(
                    resp.error
                        .or(resp.stderr)
                        .unwrap_or_else(|| "agent read_file failed".to_string()),
                ),
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("agent read_file waiter was dropped")
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    ToolResult::err("timed out waiting for agent read_file")
                }
            };
        }
        let file_path = proj.root().join(&path);
        let canonical_root = match proj.root().canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Project root does not exist: {}", e)),
        };
        let canonical = match file_path.canonicalize() {
            Ok(p) => p,
            Err(e) => return ToolResult::err(format!("Path does not exist: {}", e)),
        };
        if !canonical.starts_with(&canonical_root) {
            return ToolResult::err("Path is outside project directory");
        }
        if !canonical.is_file() {
            return ToolResult::err("Path is not a file");
        }
        let content = match std::fs::read_to_string(&canonical) {
            Ok(c) => c,
            Err(e) => return ToolResult::err(format!("Failed to read file: {}", e)),
        };
        read_file_content_result_with_options(content, start_line, limit, with_line_numbers)
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
        for candidate in INSTRUCTION_CANDIDATE_PATHS {
            if let Some((content, total_lines)) =
                self.read_instruction_candidate(config, candidate).await
            {
                return ProjectInstructionsSnapshot::from_single_file(
                    candidate,
                    content,
                    total_lines,
                );
            }
        }
        ProjectInstructionsSnapshot::empty()
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
    ) -> Option<(String, usize)> {
        use super::project_instructions::MAX_LINES_PER_FILE;
        // Request one extra line so a returned line count strictly greater
        // than the per-file cap reliably signals line truncation regardless of
        // the agent response format (JSON sentinel vs plain-text fallback).
        let read_limit = MAX_LINES_PER_FILE + 1;
        const WAIT_TIMEOUT: u64 = 6;

        let client_id = config.agent_client_id().ok()?;
        let (request_id, rx) = self
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
            .ok()?;
        match tokio::time::timeout(Duration::from_secs(WAIT_TIMEOUT + 2), rx).await {
            Ok(Ok(resp)) if resp.exit_code == Some(0) && resp.error.is_none() => {
                parse_instruction_agent_stdout(resp.stdout.unwrap_or_default())
            }
            _ => {
                self.shell_clients.cancel_request(&request_id).await;
                None
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
        let options = match SearchOptions::normalize(SearchRequest {
            pattern,
            path,
            limit,
            context_before,
            context_after,
            include_globs,
            exclude_globs,
            result_mode,
            timeout_secs,
        }) {
            Ok(options) => options,
            Err(error) => return error.into_tool_result(),
        };
        let proj = match self.resolve_project(&project).await {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };
        if is_search_project_text_excluded_path(&options.path) {
            return empty_search_project_text_output(&project, &options);
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
            let (req_id, rx) = match self
                .shell_clients
                .enqueue_run(
                    ShellRunRequest {
                        client_id,
                        cwd: Some(proj.path.clone()),
                        command: cmd,
                        stdin: None,
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
            return match tokio::time::timeout(Duration::from_secs(outer_timeout), rx).await {
                Ok(Ok(resp)) => {
                    let stdout = resp.stdout.unwrap_or_default();
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
                        return search_timeout_tool_result(&options, backend.as_deref());
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
                    search_project_text_output(&project, &options, &stdout, resp.exit_code, &stderr)
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
        match run_command_sync_bounded(cmd, root, effective_timeout_secs).await {
            Ok((exit_code, stdout, stderr, _)) => {
                search_project_text_output(&project, &options, &stdout, Some(exit_code), &stderr)
            }
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
        assert_eq!(result.output["content"], "two");
        assert_eq!(result.output["total_lines"], 3);
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 1);
        assert!(result.output.get("numbered_text").is_none());
        assert!(result.output.get("lines").is_none());
    }

    #[test]
    fn read_file_agent_stdout_json_is_returned_without_reslicing() {
        let result = read_file_agent_stdout_result(
            serde_json::json!({
                "format": "webcodex.file_read_range.v1",
                "content": "line-560\nline-561",
                "total_lines": 7348,
                "start_line": 560,
                "limit": 2,
            })
            .to_string(),
            Some(1),
            Some(1),
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "line-560\nline-561");
        assert_eq!(result.output["total_lines"], 7348);
        assert_eq!(result.output["start_line"], 560);
        assert_eq!(result.output["limit"], 2);
    }

    #[test]
    fn read_file_agent_stdout_json_without_sentinel_uses_legacy_fallback() {
        let result = read_file_agent_stdout_result(
            serde_json::json!({
                "content": "file-json-content",
                "total_lines": 7348,
                "start_line": 560,
                "limit": 2,
            })
            .to_string(),
            Some(1),
            Some(1),
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "{\"content\":\"file-json-content\",\"limit\":2,\"start_line\":560,\"total_lines\":7348}");
        assert_eq!(result.output["total_lines"], 1);
        assert_eq!(result.output["start_line"], 1);
        assert_eq!(result.output["limit"], 1);
    }

    #[test]
    fn read_file_agent_stdout_plain_text_keeps_legacy_fallback() {
        let result =
            read_file_agent_stdout_result("one\ntwo\nthree\n".to_string(), Some(2), Some(1));

        assert!(result.success);
        assert_eq!(result.output["content"], "two");
        assert_eq!(result.output["total_lines"], 3);
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 1);
        assert!(result.output.get("numbered_text").is_none());
        assert!(result.output.get("lines").is_none());
    }

    #[test]
    fn read_file_with_line_numbers_returns_numbered_text_and_lines() {
        let result = read_file_content_result_with_options(
            "alpha\nbeta\ngamma".to_string(),
            None,
            None,
            true,
        );

        assert!(result.success);
        assert_eq!(result.output["content"], "alpha\nbeta\ngamma");
        assert_eq!(
            result.output["numbered_text"],
            "1 | alpha\n2 | beta\n3 | gamma"
        );
        assert_eq!(
            result.output["lines"],
            json!([
                {"line": 1, "text": "alpha"},
                {"line": 2, "text": "beta"},
                {"line": 3, "text": "gamma"},
            ])
        );
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
        assert_eq!(result.output["content"], "two\nthree");
        assert_eq!(result.output["start_line"], 2);
        assert_eq!(result.output["limit"], 2);
        assert_eq!(result.output["numbered_text"], "2 | two\n3 | three");
        assert_eq!(
            result.output["lines"],
            json!([
                {"line": 2, "text": "two"},
                {"line": 3, "text": "three"},
            ])
        );
    }

    #[test]
    fn read_file_with_line_numbers_handles_eof_and_short_files() {
        let result =
            read_file_content_result_with_options("one\ntwo".to_string(), Some(5), Some(3), true);

        assert!(result.success);
        assert_eq!(result.output["content"], "");
        assert_eq!(result.output["total_lines"], 2);
        assert_eq!(result.output["start_line"], 5);
        assert_eq!(result.output["limit"], 3);
        assert_eq!(result.output["numbered_text"], "");
        assert_eq!(result.output["lines"], json!([]));
    }

    #[test]
    fn read_file_agent_stdout_json_with_line_numbers_preserves_empty_lines() {
        let result = read_file_agent_stdout_result_with_options(
            serde_json::json!({
                "format": "webcodex.file_read_range.v1",
                "content": "\nsecond",
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
        assert_eq!(result.output["content"], "\nsecond");
        assert_eq!(result.output["numbered_text"], "1 | \n2 | second");
        assert_eq!(
            result.output["lines"],
            json!([
                {"line": 1, "text": ""},
                {"line": 2, "text": "second"},
            ])
        );
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
        let stdout = "src/lib.rs\01-one\nsrc/lib.rs\02-two\nsrc/lib.rs\03:needle\nsrc/lib.rs\04-four\nsrc/lib.rs\05-five\n";
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

    #[test]
    fn parse_anchor_edit_stdout_rejects_empty_stdout_with_dispatch_hint() {
        let err = parse_anchor_edit_stdout("replace_exact_block", Some(String::new()))
            .expect_err("empty stdout should be rejected before JSON parsing");
        assert!(err.contains("empty stdout"), "{err}");
        assert!(err.contains("replace_exact_block"), "{err}");
        assert!(err.contains("transport dispatch"), "{err}");
    }
}
