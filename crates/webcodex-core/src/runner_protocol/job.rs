use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::validation_evidence::CargoTestCountEvidenceStatus;

use super::{
    ShellCommandExecutionState, ShellScriptLanguage, PROCESS_ARG_MAX_COUNT, SCRIPT_ARG_MAX_COUNT,
    SCRIPT_MAX_BYTES, SCRIPT_MIN_BYTES,
};

/// Maximum byte length of the single argv value that may follow `cargo test`.
pub const RUST_TEST_FILTER_MAX_BYTES: usize = 200;

/// Maximum byte length of a value-taking Cargo argument (`--features`,
/// `-p`). Matches the `is_canonical` per-argument bound.
pub const CARGO_VALUE_MAX_BYTES: usize = 500;

/// Largest caller-declared Cargo test-count minimum.
pub const CARGO_TEST_MIN_TESTS_MAX: u64 = 1_000_000;

/// Maximum number of project-relative package patterns accepted by the
/// first-class focused `go_test` validation tool.
pub const GO_TEST_PACKAGE_MAX_ITEMS: usize = 8;

/// Maximum byte length of one focused `go_test` package pattern.
pub const GO_TEST_PACKAGE_MAX_BYTES: usize = 256;

/// Valid process-wide Runner Job execution concurrency advertised during
/// registration. This is intentionally independent from inventory retention:
/// a future inventory may retain more queued Jobs than the Runner executes.
pub const RUNNER_JOB_CONCURRENCY_MIN: usize = 1;
pub const RUNNER_JOB_CONCURRENCY_MAX: usize = 64;
/// Maximum retained bytes for one stdout or stderr stream in a runner job
/// snapshot. The server may retain a larger live tail, but reconciliation
/// deliberately converges to this bounded authoritative runner tail.
pub const JOB_SNAPSHOT_STREAM_MAX_BYTES: usize = 64 * 1024;
/// Runner inventory includes every active job or rejects further job starts.
pub const JOB_INVENTORY_MAX_ACTIVE_JOBS: usize = 64;
/// Terminal snapshots retained by one runner process.
pub const JOB_INVENTORY_MAX_TERMINAL_JOBS: usize = 64;
pub const JOB_INVENTORY_MAX_JOBS: usize =
    JOB_INVENTORY_MAX_ACTIVE_JOBS + JOB_INVENTORY_MAX_TERMINAL_JOBS;
/// Leaves headroom below the server's default 2 MiB polling request-body
/// ceiling as well as the shared 8 MiB WebSocket/QUIC frame ceiling for
/// registration, project, policy, and envelope metadata.
pub const JOB_INVENTORY_MAX_SERIALIZED_BYTES: usize = 1024 * 1024;
/// Same-process terminal results remain available for long-running Job recovery
/// while count and payload bounds prevent an unbounded process-lifetime ledger.
pub const JOB_TERMINAL_RETENTION_SECS: i64 = 24 * 60 * 60;

fn default_shell_job_kind() -> String {
    "shell".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobStatusRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobLogRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
    #[serde(default)]
    pub tail_lines: Option<usize>,
    #[serde(default)]
    pub since_stdout_line: Option<usize>,
    #[serde(default)]
    pub since_stderr_line: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobStopRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobsListRequest {
    pub client_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobUpdateRequest {
    pub client_id: String,
    /// Active Runner process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    #[serde(rename = "agent_instance_id")]
    pub runner_instance_id: String,
    pub job_id: String,
    #[serde(default)]
    pub request_id: Option<String>,
    /// Runner-owned per-job monotonic sequence. Current reconciliation-capable
    /// runners always send it; older runners omit it and keep legacy behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_seq: Option<u64>,
    pub status: String,
    #[serde(default)]
    pub stdout_chunk: Option<String>,
    #[serde(default)]
    pub stderr_chunk: Option<String>,
    /// Full authoritative tails with absolute line metadata. Reconciliation-
    /// capable runners use this for sequenced updates and post-register replay.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_snapshot: Option<ShellJobLogSnapshot>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    /// Phase-A structured execution lifecycle. It is absent for older Runner
    /// updates and ordinary legacy shell Jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_execution_state: Option<ShellCommandExecutionState>,
    /// Executor-owned bounded progress for an internally submitted validation
    /// plan. Project stdout/stderr never populates this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_progress: Option<ShellJobValidationProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_count_evidence: Option<ShellJobTestCountEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<ShellJobActivity>,
    #[serde(default)]
    pub finished: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunnerJobUpdateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobCodexMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_runtime_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobOpRequest {
    pub op: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub job_id: Option<String>,
    #[serde(default)]
    pub since_stdout_line: Option<usize>,
    #[serde(default)]
    pub since_stderr_line: Option<usize>,
    #[serde(default)]
    pub tail_lines: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ShellJobCodexMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobValidationStep {
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Cache-steering variables applied at spawn (e.g. CARGO_TARGET_DIR so
    /// the shared build cache survives slot resets). Key-allowlisted by
    /// `is_canonical`; omitted from the wire when empty so older Runners keep
    /// parsing unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<(String, String)>,
}

impl ShellJobValidationStep {
    pub fn is_canonical(&self) -> bool {
        if self
            .args
            .iter()
            .any(|arg| arg.contains('\0') || arg.len() > CARGO_VALUE_MAX_BYTES)
        {
            return false;
        }
        const ALLOWED_STEP_ENV_KEYS: &[&str] = &["CARGO_TARGET_DIR"];
        if !self.env.iter().all(|(key, value)| {
            ALLOWED_STEP_ENV_KEYS.contains(&key.as_str())
                && !value.is_empty()
                && !value.contains('\0')
                && value.len() <= 500
        }) {
            return false;
        }
        let args = self.args.iter().map(String::as_str).collect::<Vec<_>>();
        match (self.name.as_str(), self.program.as_str()) {
            ("format", "cargo") => args == ["fmt", "--", "--check"],
            ("check", "cargo") => is_canonical_cargo_check_args(&args),
            ("test", "cargo") => is_canonical_cargo_test_args(&args),
            ("check", "go") => args == ["vet", "./..."],
            ("test", "go") => args == ["test", "./..."] || self.is_structured_go_test_json(),
            ("format", "python") => {
                args == ["-m", "ruff", "format", "--check"] || args == ["-m", "black", "--check"]
            }
            ("check", "python") => args == ["-m", "ruff", "check"] || args == ["-m", "mypy"],
            ("test", "python") => {
                args == ["-m", "pytest"] || args == ["-B", "-m", "unittest", "discover", "-v"]
            }
            (kind, "npm" | "pnpm" | "yarn" | "bun") => {
                args.len() == 3
                    && args[0] == "run"
                    && args[1] == "--silent"
                    && node_script_allowed(kind, args[2])
            }
            _ => false,
        }
    }

    /// True only for the first-class machine-readable Go test shape. Package
    /// patterns are checked by the same bounded normalizer used by the runtime
    /// command builders; validation steps with environment overrides are not
    /// part of this contract.
    pub fn is_structured_go_test_json(&self) -> bool {
        if self.name != "test" || self.program != "go" || !self.env.is_empty() {
            return false;
        }
        let args = self.args.iter().map(String::as_str).collect::<Vec<_>>();
        is_canonical_go_test_json_args(&args)
    }
}

/// Canonical `cargo check` argv: `check` followed by zero or more distinct
/// read-only flags (`--all-targets`, `--all-features`,
/// `--no-default-features`) and `--features <value>` / `-p <value>` pairs.
fn is_canonical_cargo_check_args(args: &[&str]) -> bool {
    args.first() == Some(&"check") && is_canonical_cargo_flags(&args[1..], false)
}

/// Canonical `cargo test` argv: the `test` subcommand, an optional libtest
/// filter (never a Cargo option), then zero or more distinct read-only flags
/// and `--features <value>` / `-p <value>` pairs, optionally including the
/// Cargo test-only `--lib` and `--no-run` selectors.
///
/// The flat argv boundary has inherent information loss: `["test",
/// "--all-features"]` is a legal `cargo test --all-features` whether the
/// caller meant the flag or mis-placed it in the filter field, so it is parsed
/// here as the flag. Rejecting option-like filters is the planner and
/// request-validation contract (`valid_rust_test_filter`), not this function.
fn is_canonical_cargo_test_args(args: &[&str]) -> bool {
    if args.first() != Some(&"test") {
        return false;
    }
    let flags_start = match args.get(1) {
        Some(filter) if valid_rust_test_filter(filter) => 2,
        _ => 1,
    };
    is_canonical_cargo_flags(&args[flags_start..], true)
}

/// Normalize and validate one value-taking Cargo argument (`--features`,
/// `-p`). This is the single shared contract used by the synchronous command
/// builders and the structured long-Job argv builder, so a given request runs
/// identical arguments no matter how long it takes.
///
/// Applies exactly one leading/trailing whitespace trim, then rejects values
/// that are NUL/control-containing, longer than [`CARGO_VALUE_MAX_BYTES`],
/// start with `-` (which would consume the next Cargo option as this option's
/// value), or are empty after trimming. The length bound applies to the
/// normalized value that is written into argv, so a padded input whose
/// trimmed form is within bounds stays accepted. `Ok(None)` means the option
/// is simply omitted. Valid multi-word values such as `"a b"` are preserved.
pub fn normalize_cargo_value(raw: &str) -> Result<Option<String>, &'static str> {
    if raw.contains('\0') {
        return Err("cannot contain NUL bytes");
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(char::is_control) {
        return Err("contains control characters");
    }
    if trimmed.starts_with('-') {
        return Err("must not start with '-'");
    }
    if trimmed.len() > CARGO_VALUE_MAX_BYTES {
        return Err("exceeds 500 bytes");
    }
    Ok(Some(trimmed.to_string()))
}

/// Normalize the optional package scope of the first-class `go_test` tool.
/// Omission preserves the historical `./...` scope; an explicit list must
/// contain one to eight already-normalized project-relative patterns.
pub fn normalize_go_test_packages(
    packages: Option<&[String]>,
) -> Result<Vec<String>, &'static str> {
    let Some(packages) = packages else {
        return Ok(vec!["./...".to_string()]);
    };
    if packages.is_empty() || packages.len() > GO_TEST_PACKAGE_MAX_ITEMS {
        return Err("packages must contain between 1 and 8 items");
    }
    packages
        .iter()
        .map(|package| normalize_go_test_package(package))
        .collect()
}

fn normalize_go_test_package(raw: &str) -> Result<String, &'static str> {
    if raw.is_empty() {
        return Err("package pattern cannot be empty");
    }
    if raw.len() > GO_TEST_PACKAGE_MAX_BYTES {
        return Err("package pattern exceeds 256 bytes");
    }
    if !raw.is_ascii() {
        return Err("package pattern must be ASCII");
    }
    if raw
        .bytes()
        .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err("package pattern cannot contain whitespace or control characters");
    }
    if raw.contains('\\') {
        return Err("package pattern cannot contain backslashes");
    }
    if raw == "." {
        return Ok(raw.to_string());
    }
    let Some(rest) = raw.strip_prefix("./") else {
        return Err("package pattern must be '.' or start with './'");
    };
    if rest.is_empty() {
        return Err("package pattern must name a package path");
    }
    let segments = rest.split('/').collect::<Vec<_>>();
    for (index, segment) in segments.iter().enumerate() {
        if segment.is_empty() {
            return Err("package pattern contains an empty segment");
        }
        if *segment == "." || *segment == ".." {
            return Err("package pattern contains an interior '.' or '..' segment");
        }
        if *segment == "..." {
            if index + 1 != segments.len() {
                return Err("'...' is only allowed as the final complete segment");
            }
            continue;
        }
        if segment.contains("...") {
            return Err("'...' is only allowed as the final complete segment");
        }
        if !segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return Err("package pattern contains invalid characters");
        }
    }
    Ok(raw.to_string())
}

fn is_canonical_go_test_json_args(args: &[&str]) -> bool {
    if args.len() < 3 || args[0] != "test" || args[1] != "-json" {
        return false;
    }
    let packages = args[2..]
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    matches!(
        normalize_go_test_packages(Some(&packages)),
        Ok(normalized) if normalized == packages
    )
}

/// Validate the read-only Cargo flag tail shared by `cargo check` and
/// `cargo test` validation steps. Each single flag and each value-taking flag
/// appears at most once. A value-taking flag's value must already satisfy the
/// shared [`normalize_cargo_value`] contract: non-empty after trimming, not a
/// `-`-prefixed option, NUL/control-free, bounded to `CARGO_VALUE_MAX_BYTES`,
/// and already normalized (no leading/trailing whitespace). `--lib` and
/// `--no-run` are accepted only for `cargo test`.
fn is_canonical_cargo_flags(args: &[&str], cargo_test: bool) -> bool {
    let mut seen = HashSet::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let key = match *arg {
            "--all-targets" | "--all-features" | "--no-default-features" => *arg,
            "--lib" | "--no-run" if cargo_test => *arg,
            "--features" | "-p" => {
                if !seen.insert(*arg) {
                    return false;
                }
                let Some(value) = iter.next() else {
                    return false;
                };
                // The value must already be exactly its normalized form; a
                // whitespace-padded, option-like, control-containing, or
                // over-long value is not a canonical cargo value.
                match normalize_cargo_value(value) {
                    Ok(Some(normalized)) if normalized == *value => continue,
                    _ => return false,
                }
            }
            _ => return false,
        };
        if !seen.insert(key) {
            return false;
        }
    }
    true
}

fn node_script_allowed(kind: &str, script: &str) -> bool {
    matches!(
        (kind, script),
        ("format", "format:check" | "format-check" | "check:format")
            | ("check", "check" | "typecheck" | "lint")
            | ("test", "test")
    )
}

/// Normalize and validate the single argv value that may follow `cargo test`:
/// a libtest name substring, never a Cargo option. This is the shared contract
/// used by the planner (`safe_rust_filter`), the synchronous command builder,
/// and the structured long-Job argv builder, so a given filter runs identically
/// regardless of runtime path.
///
/// Applies exactly one leading/trailing trim and rejects control bytes,
/// over-long values, and anything that begins with `-` after trimming, so a
/// forged, replayed, or drifted request cannot smuggle an option such as
/// `--manifest-path` through the filter field. `Ok(None)` means no filter.
pub fn normalize_rust_test_filter(raw: &str) -> Result<Option<String>, &'static str> {
    if raw.len() > RUST_TEST_FILTER_MAX_BYTES {
        return Err("exceeds 200 bytes");
    }
    if raw.contains('\0') {
        return Err("cannot contain NUL bytes");
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().any(char::is_control) {
        return Err("contains control characters");
    }
    if trimmed.starts_with('-') {
        return Err("must not start with '-'");
    }
    Ok(Some(trimmed.to_string()))
}

/// True when `value` is a valid non-empty libtest filter (never a Cargo
/// option). `is_canonical` uses this to decide whether a flat argv's second
/// element is the filter, but enforcement of "no option-like filter" lives
/// with the planner and request-validation builders, not the flat-argv
/// boundary: `["test", "--all-features"]` is a legal `cargo test
/// --all-features` regardless of how it was constructed.
pub fn valid_rust_test_filter(value: &str) -> bool {
    normalize_rust_test_filter(value).is_ok_and(|normalized| normalized.is_some())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobValidationProgress {
    pub completed: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_step: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_step: Option<String>,
}

/// Runner-authoritative terminal Cargo test-count evidence produced before lossy log retention.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobTestCountEvidence {
    pub tests_detected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests_run_count: Option<u64>,
    pub status: CargoTestCountEvidenceStatus,
}

impl ShellJobTestCountEvidence {
    pub fn is_valid(&self) -> bool {
        if self.status.count_is_proven() {
            self.tests_detected && self.tests_run_count.is_some()
        } else {
            self.tests_run_count.is_none()
        }
    }
}

/// Bounded Runner-owned observation of what an active Job is currently doing.
/// Activity is advisory execution telemetry only: it never replaces canonical
/// Job status, proves completion, or grants retry/continuation authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellJobActivityState {
    Working,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellJobActivityPhase {
    ProcessRunning,
    ValidationFormat,
    ValidationCheck,
    ValidationTest,
    CargoWaitingForBuildLock,
    CargoCompiling,
    CargoChecking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellJobActivitySource {
    RunnerExecution,
    ValidationPlan,
    CargoOutput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellJobActivity {
    pub state: ShellJobActivityState,
    pub phase: ShellJobActivityPhase,
    pub source: ShellJobActivitySource,
}

impl ShellJobActivity {
    /// Closed field-combination validation for protocol/reconciliation callers.
    /// Provenance narrows interpretation but remains observation-only.
    pub fn is_canonical(self) -> bool {
        use ShellJobActivityPhase as Phase;
        use ShellJobActivitySource as Source;
        use ShellJobActivityState as State;

        matches!(
            (self.state, self.phase, self.source),
            (
                State::Working,
                Phase::ProcessRunning,
                Source::RunnerExecution
            ) | (
                State::Working,
                Phase::ValidationFormat,
                Source::ValidationPlan
            ) | (
                State::Working,
                Phase::ValidationCheck,
                Source::ValidationPlan
            ) | (
                State::Working,
                Phase::ValidationTest,
                Source::ValidationPlan
            ) | (
                State::Waiting,
                Phase::CargoWaitingForBuildLock,
                Source::CargoOutput
            ) | (State::Working, Phase::CargoCompiling, Source::CargoOutput)
                | (State::Working, Phase::CargoChecking, Source::CargoOutput)
        )
    }
}

/// Stable structured-validation identity retained for Job handoff, status,
/// terminal projection, and server restart reconciliation. This is internal
/// protocol metadata; it is not a model input and never contains shell text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobValidationMetadata {
    pub tool: String,
    pub kind: String,
    pub steps: Vec<ShellJobValidationStep>,
    pub effective_timeout_secs: u64,
    pub sync_wait_secs: u64,
    pub adapter: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_target_id: Option<String>,
    /// Launch observation in the existing Control Project epoch. Not a source snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fence: Option<crate::validation_source::ValidationSourceFence>,
    /// Effective caller-requested minimum Cargo test count. This is an
    /// observation postcondition, not part of the executable argv.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_tests: Option<u64>,
    /// Exact caller-provided Cargo test execution requirement. `Some(false)`
    /// is materially different from omission: it explicitly accepts a
    /// successful zero-test execution as validation proof when no minimum is
    /// requested. This is bounded policy metadata, never executable text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_tests: Option<bool>,
    /// Exact caller-provided Cargo `--no-run` intent. `Some(true)` means the
    /// validation is compile-only and therefore does not require executed-test
    /// count evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_run: Option<bool>,
}

impl ShellJobValidationMetadata {
    pub fn is_valid(&self) -> bool {
        if self.adapter != self.tool
            || self.steps.len() != 1
            || !self.steps[0].is_canonical()
            || self.effective_timeout_secs < 1
            || self.sync_wait_secs > self.effective_timeout_secs
            || self.validation_target_id.as_deref().is_some_and(|value| {
                let Some(suffix) = value.strip_prefix("target:") else {
                    return true;
                };
                suffix.len() != 24 || !suffix.as_bytes().iter().all(u8::is_ascii_hexdigit)
            })
            || self
                .source_fence
                .as_ref()
                .is_some_and(|fence| !fence.is_valid())
            || self
                .minimum_tests
                .is_some_and(|minimum| !(1..=CARGO_TEST_MIN_TESTS_MAX).contains(&minimum))
        {
            return false;
        }
        if self.minimum_tests.is_some() && self.tool != "cargo_test" {
            return false;
        }
        if (self.require_tests.is_some() || self.no_run.is_some()) && self.tool != "cargo_test" {
            return false;
        }
        if self.no_run == Some(true) && self.minimum_tests.is_some() {
            return false;
        }
        if self.require_tests == Some(true)
            && (self.minimum_tests.is_none() || self.no_run == Some(true))
        {
            return false;
        }
        let step = &self.steps[0];
        match self.tool.as_str() {
            "cargo_fmt" => {
                self.kind == "format" && step.name == "format" && step.program == "cargo"
            }
            "cargo_check" => {
                self.kind == "check" && step.name == "check" && step.program == "cargo"
            }
            "cargo_test" => self.kind == "test" && step.name == "test" && step.program == "cargo",
            "go_test" => self.kind == "test" && step.is_structured_go_test_json(),
            _ => false,
        }
    }
}

pub const VALIDATION_ASSERTION_NAME_MAX_CHARS: usize = 120;

/// Safe bounded metadata for a structured execution Job. Raw executable argv,
/// script bodies, script argv, and stdin are intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobStructuredExecutionMetadata {
    pub execution_source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<ShellScriptLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_bytes: Option<usize>,
    pub arg_count: usize,
    pub stdin_present: bool,
    /// Admission-derived opaque validation identity. It is a proven structured
    /// `target:`, generic body-free `command:`, or model assertion `assertion:` identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_identity: Option<String>,
    /// Safe human-readable correlation label paired with an `assertion:` identity.
    /// It is recovery metadata only and never grants execution or validation authority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assertion_name: Option<String>,
    /// Present only when admission proved exact equivalence to one canonical
    /// structured validation tool. Parser output never populates this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_tool: Option<String>,
}

impl ShellJobStructuredExecutionMetadata {
    pub fn is_valid(&self) -> bool {
        let identity_valid = self.validation_identity.as_deref().is_none_or(|value| {
            let suffix = value
                .strip_prefix("target:")
                .or_else(|| value.strip_prefix("command:"))
                .or_else(|| value.strip_prefix("assertion:"));
            suffix.is_some_and(|suffix| {
                suffix.len() == 24 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        });
        let assertion_identity_source_valid =
            self.validation_identity.as_deref().is_none_or(|value| {
                !value.starts_with("assertion:")
                    || matches!(self.execution_source.as_str(), "run_process" | "run_script")
            });
        let validation_tool_valid = match self.validation_tool.as_deref() {
            None => true,
            Some(tool) => {
                matches!(tool, "cargo_fmt" | "cargo_check" | "cargo_test")
                    && self.validation_identity.as_deref().is_some_and(|identity| {
                        identity.starts_with("target:") || identity.starts_with("assertion:")
                    })
            }
        };
        let assertion_name_valid = self.assertion_name.as_deref().is_none_or(|value| {
            let trimmed = value.trim();
            value == trimmed
                && !trimmed.is_empty()
                && trimmed.chars().count() <= VALIDATION_ASSERTION_NAME_MAX_CHARS
                && !trimmed.chars().any(char::is_control)
                && self
                    .validation_identity
                    .as_deref()
                    .is_some_and(|identity| identity.starts_with("assertion:"))
                && matches!(self.execution_source.as_str(), "run_process" | "run_script")
        });
        if !identity_valid
            || !assertion_identity_source_valid
            || !validation_tool_valid
            || !assertion_name_valid
        {
            return false;
        }
        match self.execution_source.as_str() {
            "run_process" | "run_detached_process" => {
                self.language.is_none()
                    && self.script_bytes.is_none()
                    && self.arg_count <= PROCESS_ARG_MAX_COUNT
            }
            "run_script" => {
                self.language.is_some()
                    && self
                        .script_bytes
                        .is_some_and(|bytes| (SCRIPT_MIN_BYTES..=SCRIPT_MAX_BYTES).contains(&bytes))
                    && self.arg_count <= SCRIPT_ARG_MAX_COUNT
            }
            _ => false,
        }
    }
}

/// Safe server-derived metadata needed to reconstruct a job record after a
/// server restart. This is an internal Runner protocol model, not a public
/// `run_job` input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_session_id: Option<String>,
    /// Named Runner-local SSH resource. It is safe recovery metadata, unlike
    /// an SSH host/configuration/key, which never crosses this protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_resource: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    pub command_preview: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ShellJobValidationMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_execution: Option<ShellJobStructuredExecutionMetadata>,
}

/// One bounded stream tail plus absolute line range. `next_line` is the
/// cursor immediately after the last retained/observed line; reconciliation
/// replaces the server stream with this authoritative range instead of
/// appending it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobStreamSnapshot {
    #[serde(default)]
    pub tail: String,
    #[serde(default = "default_first_retained_line")]
    pub first_retained_line: usize,
    #[serde(default = "default_first_retained_line")]
    pub next_line: usize,
    #[serde(default)]
    pub truncated: bool,
}

fn default_first_retained_line() -> usize {
    1
}

impl Default for ShellJobStreamSnapshot {
    fn default() -> Self {
        Self {
            tail: String::new(),
            first_retained_line: 1,
            next_line: 1,
            truncated: false,
        }
    }
}

/// Authoritative bounded log view attached to a sequenced replay update.
/// This closes the register/ack race where executor state advances after the
/// register inventory was serialized but before the new sink becomes usable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobLogSnapshot {
    pub stdout: ShellJobStreamSnapshot,
    pub stderr: ShellJobStreamSnapshot,
}

/// Runner-authoritative same-process job state used only during registration
/// reconciliation. Raw command, stdin, environment, tokens, and Runner config
/// are intentionally absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobSnapshot {
    pub job_id: String,
    pub request_id: String,
    pub status: String,
    pub update_seq: u64,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Phase-A lifecycle for typed structured execution Jobs. Older snapshots
    /// and legacy shell Jobs omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_execution_state: Option<ShellCommandExecutionState>,
    pub context: ShellJobContext,
    #[serde(default)]
    pub stdout: ShellJobStreamSnapshot,
    #[serde(default)]
    pub stderr: ShellJobStreamSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_progress: Option<ShellJobValidationProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_count_evidence: Option<ShellJobTestCountEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<ShellJobActivity>,
}

/// Register-time inventory. Terminal records are deliberately partial history,
/// while `active_complete=true` guarantees every locally active/queued job is
/// present so omission can safely reconcile a server record to `lost`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellJobInventory {
    #[serde(default)]
    pub active_complete: bool,
    #[serde(default)]
    pub jobs: Vec<ShellJobSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerShellJobResult {
    #[serde(default)]
    pub cwd: Option<String>,
    pub command_preview: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<RunnerShellJobResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobInfo {
    pub job_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub client_id: String,
    #[serde(default = "default_shell_job_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Named Runner-local SSH resource used by this job, when any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_resource: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    pub command_preview: String,
    pub status: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_execution_state: Option<ShellCommandExecutionState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_execution: Option<ShellJobStructuredExecutionMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<ShellJobCodexMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RunnerJobResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_progress: Option<ShellJobValidationProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_count_evidence: Option<ShellJobTestCountEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub activity: Option<ShellJobActivity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<ShellJobValidationMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_state: Option<String>,
    #[serde(default)]
    pub recovered_after_server_restart: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_reason_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_update_seq: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_retained_from_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_retained_from_line: Option<usize>,
    #[serde(default)]
    pub stdout_log_truncated: bool,
    #[serde(default)]
    pub stderr_log_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellJobOpResponse {
    pub success: bool,
    pub op: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stdout_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stderr_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobStatusResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elapsed_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<RunnerJobResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobLogResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stdout_line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_stderr_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobStopResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerJobsListResponse {
    pub success: bool,
    pub client_id: String,
    pub jobs: Vec<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
