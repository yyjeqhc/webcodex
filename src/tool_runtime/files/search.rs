use super::*;

const SEARCH_PROJECT_TEXT_EXCLUDES: &[&str] = &[
    "--exclude-dir=.git",
    "--exclude-dir=.claude",
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
    "--exclude=runner.toml",
    "--exclude=agent.toml",
    "--exclude=webcodex.env",
    "--exclude=*.pem",
    "--exclude=*.key",
];

use webcodex_core::project_search::{
    project_text_search_line_budget, project_text_search_rg_options,
};

pub(crate) use webcodex_tool_contracts::MAX_SEARCH_CONTEXT_LINES;
pub(crate) const MAX_SEARCH_GLOBS: usize = 32;
pub(crate) const MAX_SEARCH_GLOB_BYTES: usize = 256;
pub(crate) const DEFAULT_SEARCH_TIMEOUT_SECS: u64 = 30;
const MIN_SEARCH_TIMEOUT_SECS: i64 = 1;
const MAX_SEARCH_TIMEOUT_SECS: i64 = 120;

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Default))]
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

    #[cfg(test)]
    pub(crate) fn into_tool_result(self) -> ToolResult {
        let reason_code = self.reason_code();
        let mut output = json!({
            "code": "invalid_search_request",
            "failure_stage": "request_validation",
            "reason_code": reason_code,
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

    fn reason_code(&self) -> &'static str {
        match self.field {
            "pattern" => "invalid_pattern",
            "path" => "invalid_path",
            "include_globs" | "exclude_globs" => "invalid_glob",
            _ => "invalid_search_request",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchOptions {
    pub(crate) pattern: String,
    pub(crate) pattern_mode: SearchPatternMode,
    pub(crate) path: String,
    pub(crate) limit: usize,
    pub(crate) context_before: usize,
    pub(crate) context_after: usize,
    pub(crate) include_globs: Vec<String>,
    pub(crate) exclude_globs: Vec<String>,
    pub(crate) result_mode: SearchResultMode,
    pub(crate) timeout_secs: u64,
}

impl SearchOptions {
    #[cfg(test)]
    pub(crate) fn normalize(request: SearchRequest) -> Result<Self, SearchValidationError> {
        Self::normalize_with_pattern_mode(request, None)
    }

    pub(crate) fn normalize_with_pattern_mode(
        request: SearchRequest,
        pattern_mode: Option<SearchPatternMode>,
    ) -> Result<Self, SearchValidationError> {
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
        let pattern_mode = pattern_mode.unwrap_or(SearchPatternMode::Regex);
        let timeout_secs = request
            .timeout_secs
            .unwrap_or(DEFAULT_SEARCH_TIMEOUT_SECS as i64)
            .clamp(MIN_SEARCH_TIMEOUT_SECS, MAX_SEARCH_TIMEOUT_SECS)
            as u64;
        Ok(Self {
            pattern: request.pattern,
            pattern_mode,
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
        })
    }

    pub(crate) fn requires_ripgrep(&self) -> bool {
        !self.include_globs.is_empty()
            || !self.exclude_globs.is_empty()
            || self.result_mode == SearchResultMode::Count
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

/// Shell preamble that resolves `head_cmd` at runtime on the Runner POSIX shell.
/// Absolute fallbacks are embedded as literals for POSIX `sh`.
pub(super) fn search_head_resolution_shell(absolute_candidates: &[&str]) -> String {
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
/// the Runner command so no single over-long match
/// line, context line, or path can push the output past the Runner transport
/// cap (default 256 KiB) before the Rust layer ever sees it. The command emits
/// at most one probe byte beyond this formal budget; the parser consumes that
/// byte only as proof of truncation and never exposes it. A record cut mid-line
/// is dropped and reports `truncation_reason = "output_bytes"`.
///
/// Kept at 32 KiB so a single over-long record remains well below the Runner
/// transport cap while still leaving useful room for the bounded marker and
/// result metadata. The test harness drains subprocess pipes concurrently, so
/// this production budget does not depend on host-specific pipe capacity.
pub(crate) const SEARCH_OUTPUT_BYTE_BUDGET: usize =
    webcodex_core::project_search::PROJECT_TEXT_SEARCH_OUTPUT_BYTE_BUDGET;

fn search_output_line_budget(options: &SearchOptions) -> usize {
    project_text_search_line_budget(
        options.limit,
        options.result_mode == SearchResultMode::Matches,
        options.context_before,
        options.context_after,
    )
}

fn escape_search_literal_for_regex(pattern: &str) -> String {
    let mut escaped = String::with_capacity(pattern.len());
    for ch in pattern.chars() {
        if matches!(
            ch,
            '\\' | '.' | '^' | '$' | '|' | '?' | '*' | '+' | '(' | ')' | '[' | ']' | '{' | '}'
        ) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn ripgrep_search_command(options: &SearchOptions) -> String {
    let args = project_text_search_rg_options(
        options.result_mode.as_str(),
        options.context_before,
        options.context_after,
        &options.include_globs,
        &options.exclude_globs,
        shell_escape_simple,
    )
    .expect("normalized search result mode")
    .join(" ");
    let pattern = shell_escape_simple(&options.pattern);
    let pattern_mode_arg = match options.pattern_mode {
        SearchPatternMode::Regex => "",
        SearchPatternMode::Literal => "--fixed-strings ",
    };
    let target = shell_escape_simple(&options.path);
    // Deliberately no `--sort path`: a global sort forces ripgrep to scan and
    // buffer the whole search space before emitting anything, so a small
    // `limit` request still waits for a full-repo walk. Without it, matches
    // stream in traversal order and `head -n` closes the pipe as soon as the
    // record budget is satisfied, which SIGPIPEs the backend and stops the
    // work early. Match order is not stable, but the result is bounded and
    // timely, which matters more for this tool.
    format!("rg {args} {pattern_mode_arg}-e {pattern} -- {target} 2>/dev/null")
}

fn grep_search_command(options: &SearchOptions) -> String {
    let pattern_mode_arg = match options.pattern_mode {
        SearchPatternMode::Regex => "-E ",
        SearchPatternMode::Literal => "-F ",
    };
    let mode_args = match options.result_mode {
        SearchResultMode::Matches => format!(
            "-rHnI --null -B {} -A {}",
            options.context_before, options.context_after
        ),
        SearchResultMode::FilesWithMatches => "-rlI".to_string(),
        SearchResultMode::Count => unreachable!("count mode requires ripgrep"),
    };
    format!(
        "grep {mode_args} {pattern_mode_arg}{excludes} -e {pattern} -- {target} 2>/dev/null",
        excludes = search_project_text_exclude_args(),
        pattern = shell_escape_simple(&options.pattern),
        target = shell_escape_simple(&options.path),
    )
}

/// Build one bounded capability-selecting command for every search mode. Basic
/// matches and files-with-matches calls retain grep fallback; requests that need full capabilities
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

/// Search records and failure provenance remain typed through batching and
/// compound inspection. JSON is produced only by the terminal projections.
pub(crate) type SearchOutcome = Result<SearchSuccess, SearchFailure>;

#[derive(Debug)]
pub(crate) struct SearchFailure(SearchFailureData);

#[derive(Debug)]
enum SearchFailureData {
    Validation(SearchValidationError),
    Execution {
        cause: SearchExecutionFailure,
        context: SearchFailureContext,
    },
}

#[derive(Debug)]
struct SearchFailureContext {
    result_mode: SearchResultMode,
    effective_timeout_secs: u64,
    backend: Option<String>,
    exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
enum SearchTimeoutStage {
    BatchDeadline,
    BackendExecution,
    AgentExecution,
    AgentTransport,
}

impl SearchTimeoutStage {
    fn as_str(self) -> &'static str {
        match self {
            Self::BatchDeadline => "batch_deadline",
            Self::BackendExecution => "backend_execution",
            Self::AgentExecution => "agent_execution",
            Self::AgentTransport => "agent_transport",
        }
    }
}

#[derive(Debug)]
enum SearchExecutionFailure {
    RequestDropped,
    Timeout(SearchTimeoutStage),
    BackendIdentityMissing,
    BackendIdentityInvalid,
    BackendStatusUnavailable,
    BackendOutputInconsistent,
    BackendFeatureUnavailable,
    PathNotFound,
    BackendProcessFailed,
    AgentRequestFailed,
    AgentExecutionFailed,
    ProviderProtocolInvalid,
    ProviderExecutionFailed { provider_code: String },
}

struct SearchFailureProjection {
    #[cfg(test)]
    code: &'static str,
    broad_reason: &'static str,
    stage: &'static str,
    detail: &'static str,
    #[cfg(test)]
    message: &'static str,
}

impl SearchExecutionFailure {
    fn projection(&self) -> SearchFailureProjection {
        use SearchExecutionFailure::*;
        let (_code, broad_reason, stage, detail, _message) = match self {
            RequestDropped => ("search_request_dropped", "search_request_dropped", "agent_transport", "search_request_dropped", "search_project_text Runner request was dropped"),
            Timeout(stage) => ("search_timeout", "timeout", stage.as_str(), "timeout", "search_project_text timed out"),
            BackendIdentityMissing => ("search_execution_failed", "search_execution_failed", "backend_protocol", "backend_identity_missing", "search_project_text backend did not emit its identity marker"),
            BackendIdentityInvalid => ("search_execution_failed", "search_execution_failed", "backend_protocol", "backend_identity_invalid", "search_project_text backend did not emit its identity marker"),
            BackendStatusUnavailable => ("search_execution_failed", "search_execution_failed", "backend_protocol", "backend_status_unavailable", "search_project_text backend completion status was unavailable"),
            BackendOutputInconsistent => ("search_execution_failed", "search_execution_failed", "backend_protocol", "backend_output_inconsistent", "search_project_text backend output was inconsistent with its completion status"),
            BackendFeatureUnavailable => ("search_backend_feature_unavailable", "search_backend_feature_unavailable", "backend_selection", "backend_feature_unavailable", "ripgrep is required for the requested search_project_text features; grep fallback supports matches and files_with_matches without globs"),
            PathNotFound => ("search_path_not_found", "not_found", "path_resolution", "not_found", "search_project_text path was not found"),
            BackendProcessFailed => ("search_execution_failed", "search_execution_failed", "backend_execution", "backend_process_failed", "search_project_text backend execution failed"),
            AgentRequestFailed => ("agent_unavailable", "agent_unavailable", "agent_request", "agent_request_failed", "search_project_text Runner request could not be started"),
            AgentExecutionFailed => ("search_execution_failed", "search_execution_failed", "agent_execution", "agent_execution_failed", "search_project_text Runner execution failed"),
            ProviderProtocolInvalid => ("search_execution_failed", "search_execution_failed", "provider", "provider_protocol_invalid", "external search provider returned an invalid failure envelope"),
            ProviderExecutionFailed { .. } => ("external_provider_error", "external_provider_error", "provider", "provider_execution_failed", "external search provider failed"),
        };
        SearchFailureProjection {
            #[cfg(test)]
            code: _code,
            broad_reason,
            stage,
            detail,
            #[cfg(test)]
            message: _message,
        }
    }
}

impl From<SearchValidationError> for SearchFailure {
    fn from(error: SearchValidationError) -> Self {
        Self(SearchFailureData::Validation(error))
    }
}

impl SearchFailure {
    fn execution(
        options: &SearchOptions,
        cause: SearchExecutionFailure,
        backend: Option<&str>,
        exit_code: Option<i32>,
    ) -> Self {
        Self(SearchFailureData::Execution {
            cause,
            context: SearchFailureContext {
                result_mode: options.result_mode,
                effective_timeout_secs: options.timeout_secs,
                backend: backend.map(str::to_owned),
                exit_code,
            },
        })
    }

    pub(crate) fn retryable_request_drop(&self) -> bool {
        matches!(
            &self.0,
            SearchFailureData::Execution {
                cause: SearchExecutionFailure::RequestDropped,
                ..
            }
        )
    }

    #[cfg(test)]
    fn into_tool_result(self, options: &SearchOptions) -> ToolResult {
        let (cause, context) = match self.0 {
            SearchFailureData::Validation(error) => return error.into_tool_result(),
            SearchFailureData::Execution { cause, context } => (cause, context),
        };
        let facts = cause.projection();
        if let SearchExecutionFailure::ProviderExecutionFailed { provider_code } = &cause {
            return ToolResult::err_with_output(
                facts.message,
                json!({
                    "format": "webcodex.external_provider_error.v1",
                    "provider": "claude_code",
                    "capability": "search_project_text",
                    "code": provider_code,
                    "provider_code": provider_code,
                    "failure_stage": facts.stage,
                    "reason_code": facts.detail,
                    "result_mode": context.result_mode.as_str(),
                    "effective_timeout_secs": context.effective_timeout_secs,
                    "message": facts.message,
                    "write_state": "not_submitted",
                    "changed": false,
                    "error": "external_provider_error",
                }),
            );
        }
        let mut output = json!({
            "code": facts.code,
            "failure_stage": facts.stage,
            "reason_code": facts.detail,
            "backend": context.backend,
            "result_mode": context.result_mode.as_str(),
            "pattern_mode": options.pattern_mode.as_str(),
            "effective_timeout_secs": context.effective_timeout_secs,
            "message": facts.message,
        });
        if let Some(exit_code) = context.exit_code {
            output["exit_code"] = json!(exit_code);
        }
        if let SearchExecutionFailure::BackendFeatureUnavailable = cause {
            // Preserve legacy parser-fixture presentation without retaining
            // derived request fields in production failure state.
            let mut requested_features = Vec::new();
            if !options.include_globs.is_empty() {
                requested_features.push("include_globs".to_string());
            }
            if !options.exclude_globs.is_empty() {
                requested_features.push("exclude_globs".to_string());
            }
            if options.result_mode == SearchResultMode::Count {
                requested_features.push(format!("result_mode={}", options.result_mode.as_str()));
            }
            output["requested_features"] = json!(requested_features);
        }
        ToolResult::err_with_output(facts.message, output)
    }

    pub(crate) fn into_batch_item(self, index: usize) -> Value {
        let (reason, mut output) = match self.0 {
            SearchFailureData::Validation(error) => {
                let reason = error.reason_code();
                (
                    reason,
                    json!({
                        "failure_stage": "request_validation",
                        "detail_code": reason,
                    }),
                )
            }
            SearchFailureData::Execution { cause, context } => {
                let facts = cause.projection();
                let mut output = json!({
                    "failure_stage": facts.stage,
                    "detail_code": facts.detail,
                    "result_mode": context.result_mode.as_str(),
                    "effective_timeout_secs": context.effective_timeout_secs,
                });
                if let Some(backend) = context.backend {
                    output["backend"] = json!(backend);
                }
                if let Some(exit_code) = context.exit_code {
                    output["exit_code"] = json!(exit_code);
                }
                if let SearchExecutionFailure::ProviderExecutionFailed { provider_code } = cause {
                    output["provider_code"] = json!(provider_code);
                }
                (facts.broad_reason, output)
            }
        };
        output["error_kind"] = json!("search_project_text_failed");
        output["reason_code"] = json!(reason);
        output["state_changed"] = json!(false);
        json!({
            "index": index,
            "success": false,
            "output": output,
            "error": format!("search_project_text failed: {reason}"),
        })
    }
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

fn search_timeout_failure(
    options: &SearchOptions,
    backend: Option<&str>,
    stage: SearchTimeoutStage,
) -> SearchFailure {
    SearchFailure::execution(
        options,
        SearchExecutionFailure::Timeout(stage),
        backend,
        None,
    )
}

fn is_search_project_text_excluded_path(path: &str) -> bool {
    // Search skips credentials and the bulk trees alike: the first for
    // confidentiality, the second for cost and noise. `.claude` is
    // search-local rather than a global bulk exclusion because it commonly
    // contains ignored nested worktrees/stale checkout copies, while explicit
    // tracked-file discovery must remain governed by Git's index.
    crate::sensitive_paths::is_bulk_skipped_path(path)
        || path
            .split(['/', '\\'])
            .any(|component| component.eq_ignore_ascii_case(".claude"))
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

const SEARCH_READ_HINT_CONTEXT_BEFORE: u64 = 20;
const SEARCH_READ_HINT_LIMIT: u64 = 80;

#[derive(Debug, Serialize)]
struct SearchReadHint {
    path: String,
    start_line: u64,
    limit: u64,
}

fn search_read_hint(path: &str, line: u64) -> SearchReadHint {
    SearchReadHint {
        path: path.to_string(),
        start_line: line.saturating_sub(SEARCH_READ_HINT_CONTEXT_BEFORE).max(1),
        limit: SEARCH_READ_HINT_LIMIT,
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct SearchMatch {
    pub(crate) path: String,
    pub(crate) line: u64,
    preview: String,
    context_before: Vec<SearchContextLine>,
    context_after: Vec<SearchContextLine>,
    read_hint: SearchReadHint,
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

#[derive(Debug, Default, Clone, Copy)]
struct CountParseEvidence {
    data_record_seen: bool,
    parsed_record_count: usize,
    safe_record_seen: bool,
    filtered_record_seen: bool,
    malformed_record_seen: bool,
}

impl CountParseEvidence {
    fn parsed_record_seen(self) -> bool {
        self.parsed_record_count > 0
    }

    fn projection_complete(self) -> bool {
        !self.filtered_record_seen
            && !self.malformed_record_seen
            && (!self.parsed_record_seen() || self.safe_record_seen)
    }
}

#[derive(Debug)]
struct ParsedFileCounts {
    files: Vec<SearchFileCount>,
    returned_match_count: u64,
    limit_truncated: bool,
    bytes_truncated: bool,
    evidence: CountParseEvidence,
}

#[derive(Debug)]
enum SearchResultData {
    Matches(Vec<SearchMatch>),
    FilesWithMatches(Vec<SearchFile>),
    Count {
        files: Vec<SearchFileCount>,
        returned_match_count: u64,
        count_complete: bool,
        evidence: CountParseEvidence,
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
    /// stdout lost its prefix to Runner/Server result retention. Public
    /// identity validation rejects that loss before retained records can be
    /// promoted. The public reason string remains the legacy `transport`
    /// spelling for compatibility; this is not a wire/body/frame ceiling.
    ResultRetention,
    /// The search did not finish within the effective timeout; records
    /// collected before the timeout are still complete and trusted.
    Timeout,
}

impl SearchTruncation {
    fn reason(self) -> &'static str {
        match self {
            SearchTruncation::Limit => "limit",
            SearchTruncation::OutputBytes => "output_bytes",
            SearchTruncation::ResultRetention => "transport",
            SearchTruncation::Timeout => "timeout",
        }
    }
}

#[derive(Debug)]
struct SearchResult {
    backend: String,
    data: SearchResultData,
    truncation_reason: Option<SearchTruncation>,
}

#[derive(Debug)]
pub(crate) struct SearchSuccess {
    options: SearchOptions,
    result: SearchResult,
    exit_code: Option<i32>,
    include_globs_excluded_matches: bool,
}

impl SearchSuccess {
    fn new(options: &SearchOptions, result: SearchResult, exit_code: Option<i32>) -> Self {
        Self {
            options: options.clone(),
            result,
            exit_code,
            include_globs_excluded_matches: false,
        }
    }

    pub(crate) fn matches(&self) -> Option<&[SearchMatch]> {
        match &self.result.data {
            SearchResultData::Matches(matches) => Some(matches),
            _ => None,
        }
    }

    pub(crate) fn is_complete_and_empty(&self) -> bool {
        if self.result.truncation_reason.is_some() {
            return false;
        }
        match &self.result.data {
            SearchResultData::Matches(matches) => matches.is_empty(),
            SearchResultData::FilesWithMatches(files) => files.is_empty(),
            SearchResultData::Count {
                returned_match_count,
                count_complete,
                ..
            } => *count_complete && *returned_match_count == 0,
        }
    }

    pub(crate) fn mark_include_globs_excluded_matches(&mut self) {
        self.include_globs_excluded_matches = true;
    }

    pub(crate) fn into_output(self, project: Option<&str>) -> Value {
        let mut output = search_result_json(project, &self.options, self.result, self.exit_code);
        if self.include_globs_excluded_matches {
            output["zero_match_hint"] = json!("include_globs_excluded_matches");
        }
        output
    }

    #[cfg(test)]
    fn into_tool_result(self, project: &str) -> ToolResult {
        ToolResult::ok(self.into_output(Some(project)))
    }
}

fn search_result_has_records(result: &SearchResult) -> bool {
    match &result.data {
        SearchResultData::Matches(matches) => !matches.is_empty(),
        SearchResultData::FilesWithMatches(files) => !files.is_empty(),
        SearchResultData::Count { files, .. } => !files.is_empty(),
    }
}

#[derive(Debug)]
struct SearchBackendStatus {
    backend: String,
    feature_unavailable: bool,
    path_not_found: bool,
    marker_present: bool,
    marker_invalid: bool,
    payload_start: usize,
}

fn missing_search_backend_status(marker_invalid: bool) -> SearchBackendStatus {
    SearchBackendStatus {
        backend: "grep".to_string(),
        feature_unavailable: false,
        path_not_found: false,
        marker_present: false,
        marker_invalid,
        payload_start: 0,
    }
}

/// Return the first non-empty stdout line and the byte offset immediately after
/// it. Blank transport padding is tolerated, but no non-empty payload may
/// precede the backend marker.
fn first_search_payload_line(stdout: &str) -> Option<(&str, usize)> {
    let mut offset = 0;
    for segment in stdout.split_inclusive('\n') {
        let next_offset = offset + segment.len();
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.trim().is_empty() {
            return Some((line, next_offset));
        }
        offset = next_offset;
    }
    None
}

fn parse_search_backend_status(stdout: &str) -> SearchBackendStatus {
    let Some((line, payload_start)) = first_search_payload_line(stdout) else {
        return missing_search_backend_status(false);
    };
    let value = match serde_json::from_str::<Value>(line) {
        Ok(value) => value,
        Err(_) => return missing_search_backend_status(line.contains("webcodex_search")),
    };
    let Some(marker) = value.get("webcodex_search") else {
        // Bare {"backend": ...} objects and arbitrary JSON payload are not
        // trusted identity evidence.
        return missing_search_backend_status(false);
    };
    let Some(backend) = marker.get("backend").and_then(Value::as_str) else {
        return missing_search_backend_status(true);
    };
    if !matches!(backend, "rg" | "grep" | "native" | "claude_code")
        || marker
            .get("feature_unavailable")
            .is_some_and(|value| !value.is_boolean())
    {
        return missing_search_backend_status(true);
    }
    let path_not_found = match marker.get("path_status") {
        None => false,
        Some(value) if value.as_str() == Some("not_found") => true,
        Some(_) => return missing_search_backend_status(true),
    };
    SearchBackendStatus {
        backend: backend.to_string(),
        feature_unavailable: marker
            .get("feature_unavailable")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        path_not_found,
        marker_present: true,
        marker_invalid: false,
        payload_start,
    }
}

fn safe_external_provider_error_code(value: Option<&str>) -> Option<&str> {
    value.filter(|code| {
        !code.is_empty()
            && code.len() <= 64
            && code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    })
}

fn external_provider_error(stdout: &str, options: &SearchOptions) -> Option<SearchFailure> {
    let value: Value = serde_json::from_str(stdout.trim()).ok()?;
    if value.get("format").and_then(Value::as_str) != Some("webcodex.external_provider_error.v1") {
        return None;
    }
    let provider_code =
        safe_external_provider_error_code(value.get("code").and_then(Value::as_str));
    let cause = if value.get("provider").and_then(Value::as_str) != Some("claude_code")
        || value.get("capability").and_then(Value::as_str) != Some("search_project_text")
        || provider_code.is_none()
    {
        SearchExecutionFailure::ProviderProtocolInvalid
    } else {
        SearchExecutionFailure::ProviderExecutionFailed {
            provider_code: provider_code.expect("validated provider code").to_owned(),
        }
    };
    Some(SearchFailure::execution(options, cause, None, None))
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
    if p.has_root()
        || p.components()
            .any(|component| matches!(component, std::path::Component::Prefix(_)))
        || p.components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return false;
    }
    !is_search_project_text_excluded_path(path)
}

fn normalize_search_record_path(path: &str) -> Option<String> {
    #[cfg(windows)]
    let normalized = path.replace('\\', "/");
    #[cfg(windows)]
    let path = normalized.as_str();
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
/// payload. Low-level parser helpers still tolerate markerless input, but the
/// public search result path rejects it before parsing.
fn search_payload_start(stdout: &str) -> usize {
    let status = parse_search_backend_status(stdout);
    status
        .marker_present
        .then_some(status.payload_start)
        .unwrap_or(0)
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

fn strip_leading_result_retention_truncation_marker(stdout: &str) -> (&str, bool) {
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
            read_hint: search_read_hint(&record.path, record.line),
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

fn parse_file_counts(stdout: &str, limit: usize) -> ParsedFileCounts {
    let (lines, bytes_truncated) = split_complete_search_lines(stdout);
    let mut counts = Vec::<(String, u64)>::new();
    let mut evidence = CountParseEvidence::default();
    for line in lines {
        if serde_json::from_str::<Value>(line).is_ok() {
            continue;
        }
        evidence.data_record_seen = true;
        let parsed = line
            .split_once('\0')
            .or_else(|| line.rsplit_once(':'))
            .and_then(|(path, count)| {
                Some((path, count.trim_end_matches('\r').parse::<u64>().ok()?))
            });
        let Some((path, count)) = parsed else {
            evidence.malformed_record_seen = true;
            continue;
        };
        evidence.parsed_record_count = evidence.parsed_record_count.saturating_add(1);
        if count == 0 {
            evidence.malformed_record_seen = true;
            continue;
        }
        let Some(path) = normalize_search_record_path(path) else {
            evidence.filtered_record_seen = true;
            continue;
        };
        evidence.safe_record_seen = true;
        if let Some((_, existing)) = counts.iter_mut().find(|(existing, _)| existing == &path) {
            *existing = existing.saturating_add(count);
        } else {
            counts.push((path, count));
        }
    }
    let limit_truncated = evidence.parsed_record_count > limit;
    counts.truncate(limit);
    let returned_match_count = counts.iter().map(|(_, count)| *count).sum();
    ParsedFileCounts {
        files: counts
            .into_iter()
            .map(|(path, match_count)| SearchFileCount { path, match_count })
            .collect(),
        returned_match_count,
        limit_truncated,
        bytes_truncated,
        evidence,
    }
}

fn parse_search_result(stdout: &str, options: &SearchOptions, backend: String) -> SearchResult {
    let (stdout, result_retention_truncated) =
        strip_leading_result_retention_truncation_marker(stdout);
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
            let parsed = parse_file_counts(stdout, options.limit);
            let count_complete = !parsed.limit_truncated
                && !parsed.bytes_truncated
                && !result_retention_truncated
                && parsed.evidence.projection_complete();
            (
                SearchResultData::Count {
                    files: parsed.files,
                    returned_match_count: parsed.returned_match_count,
                    count_complete,
                    evidence: parsed.evidence,
                },
                parsed.limit_truncated,
                parsed.bytes_truncated,
            )
        }
    };
    let truncation = if result_retention_truncated {
        Some(SearchTruncation::ResultRetention)
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
        truncation_reason: truncation,
    }
}

#[cfg(test)]
pub(crate) fn search_project_text_output(
    project: &str,
    options: &SearchOptions,
    stdout: &str,
    exit_code: Option<i32>,
    stderr: &str,
) -> ToolResult {
    search_project_text_output_with_agent_error(project, options, stdout, exit_code, stderr, None)
}

#[cfg(test)]
pub(crate) fn search_project_text_output_with_agent_error(
    project: &str,
    options: &SearchOptions,
    stdout: &str,
    exit_code: Option<i32>,
    stderr: &str,
    agent_error: Option<&str>,
) -> ToolResult {
    match search_project_text_outcome(options, stdout, exit_code, stderr, agent_error) {
        Ok(success) => success.into_tool_result(project),
        Err(failure) => failure.into_tool_result(options),
    }
}

pub(crate) fn search_project_text_outcome(
    options: &SearchOptions,
    stdout: &str,
    exit_code: Option<i32>,
    stderr: &str,
    agent_error: Option<&str>,
) -> SearchOutcome {
    let backend_status = parse_search_backend_status(stdout);
    // Identity is validated once at the external response boundary. Missing
    // identity can never be promoted into an empty successful observation.
    if !backend_status.marker_present {
        return Err(SearchFailure::execution(
            options,
            if backend_status.marker_invalid {
                SearchExecutionFailure::BackendIdentityInvalid
            } else {
                SearchExecutionFailure::BackendIdentityMissing
            },
            None,
            exit_code,
        ));
    }
    if backend_status.path_not_found {
        return Err(SearchFailure::execution(
            options,
            SearchExecutionFailure::PathNotFound,
            None,
            None,
        ));
    }
    if backend_status.feature_unavailable {
        return Err(SearchFailure::execution(
            options,
            SearchExecutionFailure::BackendFeatureUnavailable,
            Some("grep"),
            exit_code,
        ));
    }
    if looks_like_search_timeout(exit_code, stderr, agent_error, options.timeout_secs) {
        return search_timeout_outcome_with_records(
            options,
            stdout,
            Some(backend_status.backend.as_str()),
            exit_code,
            SearchTimeoutStage::BackendExecution,
        );
    }
    // 0 = matches, 1 = no matches, 141 = SIGPIPE after the bounded head.
    match exit_code {
        Some(code) if is_search_backend_success_exit(code) => {}
        Some(code) => {
            return Err(SearchFailure::execution(
                options,
                SearchExecutionFailure::BackendProcessFailed,
                Some(backend_status.backend.as_str()),
                Some(code),
            ));
        }
        None => {
            return Err(SearchFailure::execution(
                options,
                SearchExecutionFailure::BackendStatusUnavailable,
                Some(backend_status.backend.as_str()),
                None,
            ));
        }
    }

    let result = parse_search_result(stdout, options, backend_status.backend.clone());
    // Count evidence distinguishes malformed backend records from records
    // removed by privacy filtering. Filtered counts remain incomplete, not zero.
    let has_records = search_result_has_records(&result);
    let status_consistent = match (&result.data, exit_code) {
        (SearchResultData::Count { evidence, .. }, Some(1)) => !evidence.data_record_seen,
        (SearchResultData::Count { evidence, .. }, Some(0 | 141)) => {
            !evidence.malformed_record_seen
                && (evidence.parsed_record_seen() || result.truncation_reason.is_some())
        }
        (_, Some(1)) => !has_records,
        (_, Some(0 | 141)) => has_records,
        _ => true,
    };
    if !status_consistent {
        return Err(SearchFailure::execution(
            options,
            SearchExecutionFailure::BackendOutputInconsistent,
            Some(backend_status.backend.as_str()),
            exit_code,
        ));
    }
    Ok(SearchSuccess::new(options, result, exit_code))
}

/// Serialize a parsed [`SearchResult`] into the public `search_project_text`
/// output object. Shared by the normal success path and the timeout
/// partial-success path so both emit identical field semantics.
fn search_result_json(
    project: Option<&str>,
    options: &SearchOptions,
    result: SearchResult,
    exit_code: Option<i32>,
) -> Value {
    let mut output = json!({
        "path": options.path,
        "backend": result.backend,
        "result_mode": options.result_mode.as_str(),
        "pattern_mode": options.pattern_mode.as_str(),
        "effective_timeout_secs": options.timeout_secs,
        "exit_code": exit_code,
        "context_before": options.context_before,
        "context_after": options.context_after,
    });
    if let Some(project) = project {
        output["project"] = json!(project);
        output["pattern"] = json!(options.pattern);
    }
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
            evidence: _,
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
    output["truncated"] = json!(result.truncation_reason.is_some());
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
fn search_timeout_outcome_with_records(
    options: &SearchOptions,
    stdout: &str,
    backend: Option<&str>,
    exit_code: Option<i32>,
    failure_stage: SearchTimeoutStage,
) -> SearchOutcome {
    let Some(backend_name) = backend else {
        return Err(search_timeout_failure(options, None, failure_stage));
    };
    let mut result = parse_search_result(stdout, options, backend_name.to_string());
    if !search_result_has_records(&result) {
        return Err(search_timeout_failure(options, backend, failure_stage));
    }
    result.truncation_reason = Some(SearchTruncation::Timeout);
    if let SearchResultData::Count { count_complete, .. } = &mut result.data {
        *count_complete = false;
    }
    Ok(SearchSuccess::new(options, result, exit_code))
}

fn empty_search_project_text_outcome(options: &SearchOptions) -> SearchOutcome {
    // Preserve the existing excluded-path observation, including the
    // unterminated synthetic marker's output-bytes truncation semantics.
    let marker = json!({
        "webcodex_search": {
            "backend": "native",
            "feature_unavailable": false,
        }
    })
    .to_string();
    Ok(SearchSuccess::new(
        options,
        parse_search_result(&marker, options, "native".to_string()),
        Some(1),
    ))
}

impl ToolRuntime {
    pub(crate) async fn search_one_resolved_project_text_outcome(
        &self,
        proj: &ProjectConfig,
        mut options: SearchOptions,
        batch_deadline: Option<Instant>,
    ) -> SearchOutcome {
        let mut batch_deadline_limited_timeout = false;
        if let Some(deadline) = batch_deadline {
            let now = Instant::now();
            if now >= deadline {
                return Err(search_timeout_failure(
                    &options,
                    None,
                    SearchTimeoutStage::BatchDeadline,
                ));
            }
            // The Runner protocol expresses command timeouts in whole seconds;
            // the outer `timeout_at` below still enforces the exact subsecond
            // remainder when tests or a nearly exhausted batch have less than
            // one second left.
            let remaining_secs = deadline.duration_since(now).as_secs().max(1);
            batch_deadline_limited_timeout = remaining_secs < options.timeout_secs;
            options.timeout_secs = options.timeout_secs.min(remaining_secs);
        }
        if is_search_project_text_excluded_path(&options.path) {
            return empty_search_project_text_outcome(&options);
        }
        let cmd = search_project_text_command(&options);
        let effective_timeout_secs = options.timeout_secs;
        let (command_timeout, wait_timeout, outer_timeout) =
            search_agent_timeout_budget(effective_timeout_secs);
        let client_id = proj.client_id.clone();
        // External search providers historically interpret `pattern` as regex and
        // older Runners ignore unknown request fields. Encode literal semantics into
        // that established pattern contract so mixed Server/Runner versions cannot
        // silently reinterpret an exact-text request as a regex. The native command
        // above still uses --fixed-strings/-F when the external provider falls back.
        let external_pattern = match options.pattern_mode {
            SearchPatternMode::Regex => options.pattern.clone(),
            SearchPatternMode::Literal => escape_search_literal_for_regex(&options.pattern),
        };
        let payload = json!({
            "pattern": external_pattern,
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
            .runner_registry
            .enqueue_run(
                ShellRunRequest {
                    login: false,
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
            Err(_) => {
                return Err(SearchFailure::execution(
                    &options,
                    SearchExecutionFailure::AgentRequestFailed,
                    None,
                    None,
                ))
            }
        };
        let agent_wait_deadline = Instant::now() + Duration::from_secs(outer_timeout);
        let batch_deadline_wins =
            batch_deadline.is_some_and(|deadline| deadline <= agent_wait_deadline);
        let wait_deadline = batch_deadline.map_or(agent_wait_deadline, |deadline| {
            std::cmp::min(deadline, agent_wait_deadline)
        });
        match tokio::time::timeout_at(wait_deadline, rx).await {
            Ok(Ok(resp)) => {
                let raw_stdout = resp.stdout.unwrap_or_default();
                if let Some(failure) = external_provider_error(&raw_stdout, &options) {
                    return Err(failure);
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
                    let backend_status = parse_search_backend_status(&stdout);
                    let backend = backend_status
                        .marker_present
                        .then_some(backend_status.backend);
                    return search_timeout_outcome_with_records(
                        &options,
                        &stdout,
                        backend.as_deref(),
                        resp.exit_code,
                        if batch_deadline_limited_timeout {
                            SearchTimeoutStage::BatchDeadline
                        } else if backend.is_some() {
                            SearchTimeoutStage::BackendExecution
                        } else {
                            SearchTimeoutStage::AgentExecution
                        },
                    );
                }
                if agent_error.is_some() {
                    let backend_status = parse_search_backend_status(&stdout);
                    return Err(SearchFailure::execution(
                        &options,
                        SearchExecutionFailure::AgentExecutionFailed,
                        backend_status
                            .marker_present
                            .then_some(backend_status.backend.as_str()),
                        resp.exit_code,
                    ));
                }
                search_project_text_outcome(&options, &stdout, resp.exit_code, &stderr, None)
            }
            Ok(Err(_)) => {
                self.runner_registry.cancel_request(&req_id).await;
                // Channel closed without a result: agent disconnect / waiter
                // drop — not a search timeout.
                Err(SearchFailure::execution(
                    &options,
                    SearchExecutionFailure::RequestDropped,
                    None,
                    None,
                ))
            }
            Err(_) => {
                self.runner_registry.cancel_request(&req_id).await;
                // Preserve whether the per-search transport bound or the
                // batch's shared absolute deadline ended the wait.
                Err(search_timeout_failure(
                    &options,
                    None,
                    if batch_deadline_wins {
                        SearchTimeoutStage::BatchDeadline
                    } else {
                        SearchTimeoutStage::AgentTransport
                    },
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excluded_path_retains_legacy_empty_result_truncation() {
        for result_mode in [
            SearchResultMode::Matches,
            SearchResultMode::FilesWithMatches,
            SearchResultMode::Count,
        ] {
            let options = SearchOptions::normalize(SearchRequest {
                pattern: "needle".into(),
                path: Some(".git".into()),
                result_mode: Some(result_mode),
                ..Default::default()
            })
            .unwrap();
            let success = empty_search_project_text_outcome(&options).unwrap();
            assert!(!success.is_complete_and_empty());
            let output = success.into_output(None);
            assert_eq!(output["backend"], "native");
            assert_eq!(output["truncated"], true);
            assert_eq!(output["truncation_reason"], "output_bytes");
            match result_mode {
                SearchResultMode::Matches => assert_eq!(output["matches"], json!([])),
                SearchResultMode::FilesWithMatches => assert_eq!(output["files"], json!([])),
                SearchResultMode::Count => {
                    assert_eq!(output["files"], json!([]));
                    assert_eq!(output["returned_match_count"], 0);
                    assert_eq!(output["count_complete"], false);
                    assert_eq!(output["total_matches"], Value::Null);
                }
            }
        }
    }

    #[test]
    fn provider_error_codes_cannot_impersonate_retryable_transport_failures() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".into(),
            ..Default::default()
        })
        .unwrap();
        for code in [
            "search_request_dropped",
            "search_timeout",
            "invalid_search_request",
        ] {
            let envelope = json!({
                "format": "webcodex.external_provider_error.v1",
                "provider": "claude_code",
                "capability": "search_project_text",
                "code": code,
                "message": "RAW_PROVIDER_BODY /private/never-return.rs",
            })
            .to_string();
            let failure = external_provider_error(&envelope, &options).unwrap();
            assert!(!failure.retryable_request_drop(), "provider code {code}");
            let projected = failure.into_batch_item(2);
            assert_eq!(
                projected["output"]["reason_code"],
                "external_provider_error"
            );
            assert_eq!(projected["output"]["failure_stage"], "provider");
            assert_eq!(
                projected["output"]["detail_code"],
                "provider_execution_failed"
            );
            assert_eq!(projected["output"]["provider_code"], code);
            assert!(!projected.to_string().contains("RAW_PROVIDER_BODY"));
            assert!(!projected.to_string().contains("/private/"));
        }
        assert!(SearchFailure::execution(
            &options,
            SearchExecutionFailure::RequestDropped,
            None,
            None,
        )
        .retryable_request_drop());
    }

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
    fn parse_search_matches_default_output_has_empty_context_arrays() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "main".to_string(),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
        let result = search_project_text_output(
            "demo",
            &options,
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/main.rs:42:fn main() {}\n",
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
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/lib.rs\x001-one\nsrc/lib.rs\x002-two\nsrc/lib.rs\x003:needle\nsrc/lib.rs\x004-four\nsrc/lib.rs\x005-five\n";
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            context_before: Some(2),
            context_after: Some(2),
            ..Default::default()
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
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/a.rs:1:needle one\nsrc/b.rs:2:needle tw";
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
            limit: Some(50),
            ..Default::default()
        })
        .unwrap();
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/a.rs:1:needle one\n";
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
            limit: Some(50),
            ..Default::default()
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
            limit: Some(50),
            result_mode: Some(SearchResultMode::FilesWithMatches),
            ..Default::default()
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
            limit: Some(50),
            result_mode: Some(SearchResultMode::Count),
            ..Default::default()
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
            limit: Some(2),
            ..Default::default()
        })
        .unwrap();
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/a.rs:1:one\nsrc/b.rs:2:two\nsrc/c.rs:3:three\n";
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
            limit: Some(5),
            ..Default::default()
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
            limit: Some(10),
            timeout_secs: Some(0),
            ..Default::default()
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
            limit: Some(10),
            timeout_secs: Some(0),
            ..Default::default()
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
            limit: Some(10),
            result_mode: Some(SearchResultMode::Count),
            timeout_secs: Some(0),
            ..Default::default()
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
            limit: Some(10),
            result_mode: Some(SearchResultMode::FilesWithMatches),
            timeout_secs: Some(0),
            ..Default::default()
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
            limit: Some(5),
            ..Default::default()
        })
        .unwrap();
        let stdout = "{\"webcodex_search\":{\"backend\":\"rg\"}}\nsrc/a.rs:1:needle\n";
        let result = search_project_text_output("demo", &options, stdout, Some(2), "");
        assert!(!result.success);
        assert_eq!(result.output["code"], "search_execution_failed");
    }

    #[test]
    fn search_rejects_absolute_and_temp_paths_in_output() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
        // Absolute path, parent traversal, and a temp-file path must be
        // dropped; only the trusted relative record survives.
        let stdout = concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
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

    fn result_retention_truncation_markers() -> [&'static str; 3] {
        [
            "[output truncated to last 12000 bytes]\n",
            "[output truncated]\n",
            "[...]\n",
        ]
    }

    #[test]
    fn search_project_text_literal_mode_matches_regex_metacharacters_as_text() {
        let root = unique_temp_dir("search-literal-pattern");
        std::fs::write(
            root.join("sample.txt"),
            "RuntimeInfo { value.* }\nRuntimeInfo valueZZ\n",
        )
        .expect("write sample");
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: "RuntimeInfo { value.* }".to_string(),
                limit: Some(10),
                ..Default::default()
            },
            Some(SearchPatternMode::Literal),
        )
        .unwrap();
        let command = search_project_text_command(&options);
        assert!(command.contains("--fixed-strings"));
        assert!(command.contains("grep -rHnI --null -B 0 -A 0 -F"));
        let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
        assert_eq!(exit_code, 0, "stderr: {stderr}");
        let result =
            search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["pattern_mode"], "literal");
        let matches = result.output["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["preview"], "RuntimeInfo { value.* }");
    }

    #[test]
    fn search_result_retention_truncated_stdout_cannot_recover_backend_identity() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
        let stdout = concat!(
            "[output truncated to last 12000 bytes]\n",
            "src/z.rs:1:needle tail\n",
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
        );
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(!result.success, "{:?}", result.output);
        assert_eq!(result.output["failure_stage"], "backend_protocol");
        assert_eq!(result.output["reason_code"], "backend_identity_missing");
        assert!(result.output["backend"].is_null());
        assert!(!serde_json::to_string(&result).unwrap().contains("src/z.rs"));
    }

    #[test]
    fn search_result_retention_marker_forms_cannot_recover_match_identity() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();

        for marker in result_retention_truncation_markers() {
            let stdout = format!(
                "{marker}{{\"webcodex_search\":{{\"backend\":\"rg\"}}}}\nsrc/a.rs:1:needle one\nsrc/b.rs:2:needle two\n"
            );
            let result = search_project_text_output("demo", &options, &stdout, Some(0), "");
            assert!(!result.success, "marker {marker:?}: {:?}", result.output);
            assert_eq!(
                result.output["failure_stage"], "backend_protocol",
                "marker {marker:?}"
            );
            assert_eq!(
                result.output["reason_code"], "backend_identity_missing",
                "marker {marker:?}"
            );
            assert!(result.output["backend"].is_null(), "marker {marker:?}");
        }
    }

    #[test]
    fn search_result_retention_marker_forms_cannot_recover_file_identity() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            result_mode: Some(SearchResultMode::FilesWithMatches),
            ..Default::default()
        })
        .unwrap();

        for marker in result_retention_truncation_markers() {
            let stdout = format!(
                "{marker}{{\"webcodex_search\":{{\"backend\":\"rg\"}}}}\nsrc/a.rs\nsrc/b.rs\n"
            );
            let result = search_project_text_output("demo", &options, &stdout, Some(0), "");
            assert!(!result.success, "marker {marker:?}: {:?}", result.output);
            assert_eq!(
                result.output["failure_stage"], "backend_protocol",
                "marker {marker:?}"
            );
            assert_eq!(
                result.output["reason_code"], "backend_identity_missing",
                "marker {marker:?}"
            );
            assert!(result.output["backend"].is_null(), "marker {marker:?}");
        }
    }

    #[test]
    fn search_result_retention_marker_forms_cannot_recover_count_identity() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            result_mode: Some(SearchResultMode::Count),
            ..Default::default()
        })
        .unwrap();

        for marker in result_retention_truncation_markers() {
            let stdout = format!(
                "{marker}{{\"webcodex_search\":{{\"backend\":\"rg\"}}}}\nsrc/a.rs:2\nsrc/b.rs:3\n"
            );
            let result = search_project_text_output("demo", &options, &stdout, Some(0), "");
            assert!(!result.success, "marker {marker:?}: {:?}", result.output);
            assert_eq!(
                result.output["failure_stage"], "backend_protocol",
                "marker {marker:?}"
            );
            assert_eq!(
                result.output["reason_code"], "backend_identity_missing",
                "marker {marker:?}"
            );
            assert!(result.output["backend"].is_null(), "marker {marker:?}");
        }
    }

    #[test]
    fn search_result_retention_marker_text_in_middle_is_not_prefix_loss() {
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
        let stdout = concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
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
        // The Runner path parses the Runner's stdout with the same function as
        // the local path, so the exact same stdout string must yield identical
        // field semantics in both. This pins that parity for the record fields
        // the task lists: backend, result_mode, matches, count, truncated,
        // truncation_reason, effective_timeout_secs.
        let options = SearchOptions::normalize(SearchRequest {
            pattern: "needle".to_string(),
            limit: Some(2),
            context_before: Some(1),
            context_after: Some(1),
            timeout_secs: Some(5),
            ..Default::default()
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
            limit: Some(10),
            context_before: Some(3),
            context_after: Some(3),
            ..Default::default()
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
            context_before: Some(21),
            context_after: Some(99),
            ..Default::default()
        })
        .unwrap();
        assert_eq!((options.context_before, options.context_after), (21, 80));
    }
}
