//! Stable model-facing runtime contract constants shared by execution and tool schemas.

pub const MAX_UNIFIED_DIFF_BYTES: usize = 256 * 1024;
pub const MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES: usize = 8 * 1024;
pub const DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES: usize = 64 * 1024;
pub const MIN_READ_FILES_RESULT_BYTES: usize = 8 * 1024;
pub const DEFAULT_READ_FILES_RESULT_BYTES: usize = 64 * 1024;
/// Stable model-facing ceiling for explicitly requested broad/deep batch inspection.
pub const MODEL_INSPECTION_MAX_RESULT_BYTES: usize = 512 * 1024;
pub const FILE_READ_MAX_SERIALIZED_OUTPUT_BYTES: usize = 256 * 1024;
pub const FILE_READ_DEFAULT_LIMIT: usize = 2000;
pub const FILE_READ_MAX_LIMIT: usize = 2000;
/// Raw producer-page budget for `git_diff_hunks`, independent of the final
/// serialized model-facing result ceiling.
pub const MIN_GIT_DIFF_HUNKS_PAGE_BYTES: usize = 16 * 1024;
/// Keep producer stdout comfortably below the ordinary 256 KiB per-stream
/// Runner result-retention default, leaving headroom for framing and metadata.
pub const MAX_GIT_DIFF_HUNKS_PAGE_BYTES: usize = 192 * 1024;
/// Broad review should use the full safe producer page by default. Smaller pages
/// only add Runner round-trips and repeat the same Git source scan; callers can
/// still request a tighter page explicitly when useful.
pub const DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES: usize = MAX_GIT_DIFF_HUNKS_PAGE_BYTES;
pub const GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES: usize = 192;
pub const DEFAULT_OBSERVE_JOBS_TAIL_LINES: usize = 40;
/// Maximum explicit bounded wait for observing already-accepted Job work.
/// This is intentionally separate from execution timeouts and initial
/// synchronous handoff grace budgets.
pub const MAX_JOB_OBSERVATION_WAIT_SECS: u64 = 100;
/// Model-facing continuation wait kept below common MCP Host call deadlines.
/// Runtime still accepts waits up to MAX_JOB_OBSERVATION_WAIT_SECS.
pub const MODEL_JOB_CONTINUATION_WAIT_SECS: u64 = 55;
/// Keep initial structured-execution handoff grace under the same Host-safe
/// model-facing wait budget. This does not shorten the execution timeout; work
/// past this grace continues as the same durable Job.
pub const STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS: u64 = MODEL_JOB_CONTINUATION_WAIT_SECS;

pub const MAX_SKILL_LIST_LIMIT: usize = 64;
pub const MAX_SKILL_QUERY_CHARS: usize = 200;
pub const MAX_SKILL_RESOURCE_PATH_CHARS: usize = 512;
pub const MAX_SKILL_READ_LINES: usize = 400;

pub const CHECKPOINT_KIND_VALUES: &[&str] = &[
    "snapshot",
    "baseline",
    "before_refactor",
    "after_refactor",
    "last_known_good",
    "rollback_candidate",
];
pub const CHECKPOINT_VALIDATION_STATUS_VALUES: &[&str] =
    &["unknown", "not_run", "passed", "failed"];

pub const RECOVERY_KIND_VALUES: [&str; 7] = [
    "fix_input",
    "retry_same",
    "reobserve",
    "reconcile",
    "wait",
    "user_action",
    "none",
];

/// Closed model-facing vocabulary for continuing successful or partial
/// observations. This is deliberately separate from failure recovery,
/// authorization, retry/idempotency, execution identity, and resource ownership.
pub const CONTINUATION_KIND_VALUES: [&str; 5] =
    ["page", "batch", "observe", "checkpoint", "refine"];
pub const CONTINUATION_CARRIER_VALUES: [&str; 6] = [
    "position",
    "index",
    "opaque_token",
    "observation_token",
    "revision",
    "none",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationKind {
    Page,
    Batch,
    Observe,
    Checkpoint,
    Refine,
}

impl ContinuationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Batch => "batch",
            Self::Observe => "observe",
            Self::Checkpoint => "checkpoint",
            Self::Refine => "refine",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationCarrier {
    Position,
    Index,
    OpaqueToken,
    ObservationToken,
    Revision,
    None,
}

impl ContinuationCarrier {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Index => "index",
            Self::OpaqueToken => "opaque_token",
            Self::ObservationToken => "observation_token",
            Self::Revision => "revision",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct ContinuationSemantics {
    pub kind: ContinuationKind,
    pub carrier: ContinuationCarrier,
}

impl ContinuationSemantics {
    pub const fn new(kind: ContinuationKind, carrier: ContinuationCarrier) -> Self {
        Self { kind, carrier }
    }

    pub fn to_value(self) -> serde_json::Value {
        serde_json::to_value(self).expect("ContinuationSemantics serialization is infallible")
    }
}

pub const BUILTIN_CODING_WORKFLOW_CONTRACT: &str = "webcodex.coding_workflow";
pub const BUILTIN_CODING_WORKFLOW_VERSION: u64 = 21;
pub const BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS: usize = 8;

/// Validate a Runner project path without applying host-local filesystem semantics.
/// The Server may route to an agent on another OS, so both POSIX and Windows
/// absolute-path shapes are accepted; the Runner remains authoritative for
/// existence, policy, and canonicalization.
pub fn validate_project_op_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path must not contain NUL".to_string());
    }
    let bytes = path.as_bytes();
    let posix_absolute = path.starts_with('/');
    let windows_drive_absolute = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let windows_unc_or_verbatim_absolute = path.starts_with("\\\\");
    if !(posix_absolute || windows_drive_absolute || windows_unc_or_verbatim_absolute) {
        return Err("path must be an absolute path".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_project_op_path;

    #[test]
    fn project_op_path_accepts_cross_platform_absolute_shapes_and_rejects_invalid() {
        for valid in [
            "/root/git/repo",
            r"C:\repo",
            "c:/repo",
            r"\\?\C:\repo",
            r"\\server\share\repo",
        ] {
            assert!(validate_project_op_path(valid).is_ok(), "{valid:?}");
        }
        for invalid in ["", "relative/path", r"C:repo", r"\repo", "nul\0path"] {
            assert!(validate_project_op_path(invalid).is_err(), "{invalid:?}");
        }
    }
}
