//! Stable model-facing runtime contract constants shared by execution and tool schemas.

pub const MAX_UNIFIED_DIFF_BYTES: usize = 256 * 1024;
pub const MIN_SEARCH_PROJECT_TEXTS_RESULT_BYTES: usize = 8 * 1024;
pub const DEFAULT_SEARCH_PROJECT_TEXTS_RESULT_BYTES: usize = 64 * 1024;
pub const MIN_READ_FILES_RESULT_BYTES: usize = 8 * 1024;
pub const DEFAULT_READ_FILES_RESULT_BYTES: usize = 64 * 1024;
pub const FILE_READ_MAX_SERIALIZED_OUTPUT_BYTES: usize = 256 * 1024;
pub const FILE_READ_DEFAULT_LIMIT: usize = 2000;
pub const FILE_READ_MAX_LIMIT: usize = 2000;
pub const GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES: usize = 512;
pub const STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS: u64 = 60;

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
pub const RECOVERY_TOOL_VALUES: [&str; 7] = [
    "list_jobs",
    "computer_find_elements",
    "computer_list_windows",
    "computer_list_applications",
    "computer_list_displays",
    "computer_snapshot_display",
    "read_project_artifact_metadata",
];

pub const BUILTIN_CODING_WORKFLOW_CONTRACT: &str = "webcodex.coding_workflow";
pub const BUILTIN_CODING_WORKFLOW_VERSION: u64 = 5;
pub const BUILTIN_CODING_WORKFLOW_MAX_GUIDANCE_ITEMS: usize = 8;
