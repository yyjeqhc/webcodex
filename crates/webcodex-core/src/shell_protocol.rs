use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

pub const EXTERNAL_SEARCH_REQUEST_PREFIX: &str = "# webcodex:search_project_text:v1";

fn default_shell_true() -> bool {
    true
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn default_timeout_secs() -> u64 {
    120
}

fn default_wait_timeout_secs() -> u64 {
    30
}

fn default_agent_request_kind() -> String {
    "run_shell".to_string()
}

fn default_shell_job_kind() -> String {
    "shell".to_string()
}

/// Default `transport` for `ShellClientView` when deserializing views that
/// predate the transport field (e.g. older snapshots). Polling is the legacy
/// default.
fn default_transport_polling() -> String {
    "polling".to_string()
}

/// Legacy polling wire label for inline project registration. The `v1`/`v2`
/// suffix is a rolling-compatibility projection of project inventory strategy,
/// not the canonical agent protocol generation.
pub const AGENT_PROTOCOL_VERSION_POLLING_V1: &str = "polling-v1";
/// Legacy polling wire label for paged project inventory. Current Servers
/// normalize this label at registration ingress before business logic sees it.
pub const AGENT_PROTOCOL_VERSION_POLLING_V2: &str = "polling-v2";
/// Model/user-authored raw shell command ceiling. Raw shell remains a bounded
/// escape hatch; larger program text belongs in `run_script`, while large
/// literal data belongs in stdin/files/artifacts.
pub const RAW_SHELL_COMMAND_MAX_BYTES: usize = 16_000;

/// Internal Control -> Runner raw-shell command envelope. This is deliberately
/// larger than the authored-command ceiling because an explicit `sh`/`bash`
/// request is transported through the existing POSIX single-quote wrapper.
/// In the worst case every authored byte is a single quote, expanding a
/// 16,000-byte command to about 64 KiB. This transport bound is not a model
/// input allowance.
pub const RAW_SHELL_WIRE_MAX_BYTES: usize = 64 * 1024;

/// Validate the internal raw-shell request envelope accepted by Control and
/// revalidated by the Runner. Model-facing authored commands use the smaller
/// `RAW_SHELL_COMMAND_MAX_BYTES` bound before any explicit-shell wrapper is
/// constructed.
pub fn validate_raw_shell_wire_command(command: &str) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("command cannot be empty".to_string());
    }
    if command.len() > RAW_SHELL_WIRE_MAX_BYTES {
        return Err(format!(
            "command is too long for the Runner wire envelope; maximum is {RAW_SHELL_WIRE_MAX_BYTES} bytes"
        ));
    }
    if command.contains('\0') {
        return Err("command cannot contain NUL bytes".to_string());
    }
    Ok(())
}
pub const VALIDATION_STEP_SPAWN_FAILED_CODE: &str = "validation_step_spawn_failed";
pub const VALIDATION_TOOL_UNAVAILABLE_CODE: &str = "validation_tool_unavailable";
/// The step ran, but the executor lost the ability to reap it — `waitpid`
/// answered with an error rather than a status. Distinct from a spawn
/// failure because the process did start, and distinct from a step failure
/// because nothing is known about how the step would have ended. Without its
/// own code it is indistinguishable from "the check failed", which blames the
/// project for the host's problem.
pub const VALIDATION_STEP_WAIT_FAILED_CODE: &str = "validation_step_wait_failed";

/// Return the canonical code when a validation result describes executor
/// infrastructure rather than a failed project check.
///
/// The runner uses this to avoid naming a failed check, the server protocol
/// validator uses it to accept that shape, and the connector uses it to
/// attribute the failure to the executor. Keeping the set here prevents those
/// three decisions from drifting apart.
pub fn validation_infrastructure_failure_code(error: &str) -> Option<&'static str> {
    match error {
        VALIDATION_STEP_SPAWN_FAILED_CODE => Some(VALIDATION_STEP_SPAWN_FAILED_CODE),
        VALIDATION_TOOL_UNAVAILABLE_CODE => Some(VALIDATION_TOOL_UNAVAILABLE_CODE),
        VALIDATION_STEP_WAIT_FAILED_CODE => Some(VALIDATION_STEP_WAIT_FAILED_CODE),
        _ => None,
    }
}

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

/// Legacy WebSocket wire label for inline project registration. Kept in the
/// shared protocol module so Server and Runner agree on the rolling-compatibility
/// representation without treating the label as canonical transport semantics.
pub const AGENT_PROTOCOL_VERSION_WEBSOCKET_V1: &str = "websocket-v1";
/// Legacy WebSocket wire label for paged project inventory. The `v2` suffix is
/// compatibility metadata rather than a distinct canonical protocol generation.
pub const AGENT_PROTOCOL_VERSION_WEBSOCKET_V2: &str = "websocket-v2";

/// Legacy QUIC wire label for inline project registration. QUIC still uses the
/// shared `AgentEnvelope` grammar; the actual transport identity remains the
/// separate `"quic"` transport state maintained by the Server connection path.
pub const AGENT_PROTOCOL_VERSION_QUIC_V1: &str = "quic-v1";
/// Legacy QUIC wire label for paged project inventory. The `v2` suffix is
/// compatibility metadata rather than a distinct canonical protocol generation.
pub const AGENT_PROTOCOL_VERSION_QUIC_V2: &str = "quic-v2";

/// Raw additive protocol-generation advertisement carried by Runner registration.
///
/// This wire type deliberately permits future/unsupported numeric values so a new
/// peer can be rejected explicitly at Server ingress instead of being truncated or
/// guessed into a supported generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentProtocolGenerationNumber(u16);

impl AgentProtocolGenerationNumber {
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

pub const AGENT_PROTOCOL_GENERATION_LEGACY_V1: AgentProtocolGenerationNumber =
    AgentProtocolGenerationNumber::new(1);
pub const AGENT_PROTOCOL_GENERATION_V2: AgentProtocolGenerationNumber =
    AgentProtocolGenerationNumber::new(2);

/// Canonical compatibility identity for the agent request/response grammar.
///
/// All six historical transport x `v1`/`v2` labels below describe the same
/// compatibility generation. Their suffix difference only projects registration
/// inventory strategy for rolling compatibility with older Servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProtocolCompatibility {
    V1,
    Unsupported,
}

impl Default for AgentProtocolCompatibility {
    fn default() -> Self {
        Self::Unsupported
    }
}

impl AgentProtocolCompatibility {
    pub fn is_supported(self) -> bool {
        matches!(self, Self::V1)
    }

    /// V1 project summaries have an explicit optional Git metadata contract, so
    /// absent Git fields mean "not a Git project" rather than "old peer unknown".
    pub fn reports_project_git_metadata(self) -> bool {
        matches!(self, Self::V1)
    }
}

/// Canonical project inventory synchronization strategy for one registration.
/// This is deliberately independent from transport and protocol compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentProjectInventoryStrategy {
    Inline,
    Paged,
}

impl Default for AgentProjectInventoryStrategy {
    fn default() -> Self {
        Self::Inline
    }
}

/// Server-normalized semantics derived once from the legacy announced wire label.
/// Business logic must consume this representation rather than reinterpreting the
/// raw transport/version string.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProtocolSemantics {
    pub compatibility: AgentProtocolCompatibility,
    pub project_inventory: AgentProjectInventoryStrategy,
}

/// Compatibility ingress adapter for historical `transport-v1/v2` labels.
/// Unknown labels fail closed to an unsupported compatibility identity and do
/// not opt into paged inventory by suffix guessing.
pub fn normalize_agent_protocol_semantics(version: &str) -> AgentProtocolSemantics {
    match version {
        AGENT_PROTOCOL_VERSION_POLLING_V1
        | AGENT_PROTOCOL_VERSION_WEBSOCKET_V1
        | AGENT_PROTOCOL_VERSION_QUIC_V1 => AgentProtocolSemantics {
            compatibility: AgentProtocolCompatibility::V1,
            project_inventory: AgentProjectInventoryStrategy::Inline,
        },
        AGENT_PROTOCOL_VERSION_POLLING_V2
        | AGENT_PROTOCOL_VERSION_WEBSOCKET_V2
        | AGENT_PROTOCOL_VERSION_QUIC_V2 => AgentProtocolSemantics {
            compatibility: AgentProtocolCompatibility::V1,
            project_inventory: AgentProjectInventoryStrategy::Paged,
        },
        _ => AgentProtocolSemantics::default(),
    }
}
pub const AGENT_QUIC_ALPN_V1: &str = "webcodex-runner/1";

pub const SHELL_CLIENT_CAPABILITY_SHELL: &str = "shell";
pub const SHELL_CLIENT_CAPABILITY_FILE_READ: &str = "file_read";
pub const SHELL_CLIENT_CAPABILITY_FILE_WRITE: &str = "file_write";
/// The Runner implements a narrow internal project-artifact export chunk read
/// that seeks and reads only the requested bounded segment. Missing on older
/// Runners is false and must never be inferred from ordinary file_read.
pub const SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ: &str = "artifact_export_chunk_read";
/// The Runner computes artifact-export metadata for files above the whole-payload
/// limit with bounded streaming I/O. This is separate from chunk-read support:
/// older optimized-export Runners may advertise chunk reads while still
/// materializing metadata up to the caller-provided bound.
pub const SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA: &str =
    "artifact_export_streaming_metadata";
/// The Runner implements bounded, project-root-enforced structured file deletion.
/// Missing on older Runners and false; never inferred from file_write, shell,
/// protocol version, transport, or operating system.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE: &str = "structured_file_delete";
/// The Runner understands and enforces the optional 1-based exact occurrence
/// selector in ApplyTextEditInput. Missing on older Runners is false and is
/// never inferred from other file capabilities, protocol, build, transport, or OS.
pub const SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE: &str = "apply_text_edit_occurrence";
pub const SHELL_CLIENT_CAPABILITY_GIT: &str = "git";
pub const SHELL_CLIENT_CAPABILITY_JOBS: &str = "jobs";
pub const SHELL_CLIENT_CAPABILITY_ASYNC_JOBS: &str = "async_jobs";
pub const SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS: &str = "async_shell_jobs";
/// Runner-side one-shot/background SSH shell execution for Workflow Session
/// resources. This is deliberately separate from persistent SSH support: older
/// runners must reject such a Session-bound SSH request rather than silently
/// running it on their local project checkout.
pub const SHELL_CLIENT_CAPABILITY_SSH_SHELL: &str = "ssh_shell";
/// Command-oriented, long-lived shell processes owned by one Workflow Session.
/// Missing on older runners and therefore fails closed.
pub const SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL: &str = "persistent_shell";
/// Long-lived persistent shells opened on a Workflow Session's SSH resource.
/// This is additive to `persistent_shell` and independent of the one-shot
/// `ssh_shell` capability; older runners that predate it must reject the request
/// rather than silently opening a local shell.
pub const SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL: &str = "ssh_persistent_shell";
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV: &str = "structured_validation_argv";
/// The Runner preserves the optional Cargo test-count postcondition in durable
/// validation Job metadata and returns it unchanged through reconciliation.
/// Missing on older Runners is false; Control must not start a validation Job
/// whose assertion could disappear after a Server restart.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_CARGO_TEST_COUNT_ASSERTION: &str =
    "structured_cargo_test_count_assertion";
/// The Runner accepts the canonical machine-readable `go test -json` validation
/// shape. Older implementations may support only the historical fixed `./...`
/// scope; expanded caller-selected packages are fenced separately.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON: &str = "structured_go_test_json";
/// The Runner understands the first-class model-facing `go_test` tool identity
/// and its durable `ShellJobValidationMetadata` contract. This is deliberately
/// separate from Go JSON parsing support: older Runners may advertise
/// `structured_go_test_json` for Connector validation without understanding
/// first-class `validation.tool = "go_test"` metadata.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL: &str = "structured_go_test_tool";
/// The Runner accepts the expanded first-class `go_test` argv shape with
/// caller-selected bounded project-relative package patterns. Older Runners
/// already advertised the JSON/tool capabilities for fixed `./...`, so this
/// must remain a separate additive rolling-upgrade fence.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES: &str = "structured_go_test_packages";
/// General model-facing native process execution with a typed executable and
/// argv. This is deliberately independent from structured Cargo validation:
/// older Runners may support validation argv without accepting arbitrary
/// process argv.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV: &str = "structured_process_argv";
/// Model-facing bounded script content carried as typed protocol data. This is
/// deliberately independent from raw shell and native process argv support:
/// older Runners must fail closed rather than interpreting script text through
/// the legacy command channel.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD: &str = "structured_script_payload";
/// Runner-owned WebCodex-generated POSIX programs execute through an explicit
/// internal runtime instead of the configured interactive shell. Missing on
/// older Runners is false so Control never sends the dedicated request kind to
/// a Runner that could fall through to legacy shell dispatch.
pub const SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT: &str = "internal_posix_script";
/// Durable Job execution for both typed native processes and typed script
/// payloads. This is deliberately independent from the synchronous structured
/// execution and legacy async-shell capabilities: older B1/B2 Runners may
/// advertise those capabilities without understanding typed Job starts.
pub const SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS: &str = "structured_execution_jobs";
/// Explicit authority for durable detached native-process Jobs whose payload tree is
/// handed off to the detached supervisor. Missing on older Runners is false and
/// is never inferred from structured process argv or ordinary durable Job support.
pub const SHELL_CLIENT_CAPABILITY_DETACHED_PROCESS_JOBS: &str = "detached_process_jobs";
/// Explicit capability for agent-side read-only LSP navigation. Missing on
/// older agents and defaults to `false` so the server never dispatches typed
/// LSP requests to agents that cannot handle them.
pub const SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION: &str = "lsp_read_only_navigation";
/// Bounded typed call-hierarchy traversal. Missing on older Runners and false;
/// never inferred from general LSP navigation or protocol version.
pub const SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY: &str = "lsp_call_hierarchy";
/// Linux Landlock ABI v3 inspect-command write sandbox.
pub const SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS: &str = "sandbox_inspect_commands";
pub const SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE: &str = "project_lifecycle";
/// Resolve an absolute canonical project path to an existing registration or
/// atomically persist a new projects.d entry. Missing on older runners and
/// therefore fails closed.
pub const SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION: &str = "project_path_registration";
/// Runner-global read-only operator-installed Skill store discovery/read.
/// Missing on older Runners is false and is never inferred from file_read or
/// project lifecycle support.
pub const SHELL_CLIENT_CAPABILITY_SKILL_STORE_READ: &str = "skill_store_read";
/// Runner-global operator Skill store mutation. This is an independent
/// consequential capability and is never inferred from Skill read support.
pub const SHELL_CLIENT_CAPABILITY_SKILL_STORE_MANAGE: &str = "skill_store_manage";
/// Same-process async job recovery across server restarts and transport
/// reconnects. Missing on older runners and therefore defaults to `false`.
/// Read-only native desktop/window observation. Missing on older Runners and
/// false; never inferred from shell or file capabilities.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE: &str = "computer_observe";
/// Bounded installed-application discovery. Missing on older Runners is false
/// and is never inferred from desktop observation or launch authority.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY: &str =
    "computer_application_discovery";
/// Exact native application launch for a fresh opaque discovery handle. Missing
/// on older Runners is false and is never inferred from discovery or control.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH: &str = "computer_application_launch";
/// Exact full-display discovery and snapshot observation. Missing on older
/// Runners is false and is never inferred from window observation, region
/// snapshots, or platform identity.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE: &str = "computer_display_observe";
/// Snapshot-fenced exact coordinate pointer input. Missing on older Runners is
/// false and is never inferred from control, display observation, or platform.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL: &str = "computer_pointer_control";
/// Native bounded Unicode-text clipboard observation. Missing is false and is never
/// inferred from Computer read/control/platform capabilities.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ: &str = "computer_clipboard_read";
/// Native bounded Unicode-text clipboard replacement. Missing is false and is never
/// inferred from clipboard read or other Computer effect capabilities.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE: &str = "computer_clipboard_write";
/// Bounded surface-relative region/downscale snapshot requests. Missing on older
/// Runners is false and is never inferred from whole-window observation support.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION: &str = "computer_snapshot_region";
/// Native read-only semantic accessibility inspection. Missing on older Runners
/// is false and is never inferred from screenshot/window observation.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE: &str =
    "computer_accessibility_observe";
/// Native read-only normalized state for one exact observed Accessibility element.
/// Missing on older Runners is false and is never inferred from tree observation.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE: &str = "computer_element_state";
/// Native bounded accessibility control. Missing on older Runners is false and
/// is never inferred from either observation capability.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL: &str = "computer_control";
/// Native semantic scroll-to-visible on one exact observed Accessibility element.
/// Missing on older Runners is false and is never inferred from computer_control.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT: &str = "computer_scroll_to_element";
/// Native closed-vocabulary key input to one exact already-focused window surface.
/// Missing on older Runners is false and is never inferred from computer_control.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT: &str = "computer_key_input";
/// Native exact-window activation/raise. Missing on older Runners is false and
/// is never inferred from accessibility control, observation, or platform.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE: &str = "computer_window_activate";
/// Native bounded Accessibility text input. Missing on older Runners is false
/// and is never inferred from accessibility observation or computer control.
pub const SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT: &str = "computer_text_input";
/// Baseline bounded JSON payload size for typed computer requests carried in stdin.
pub const SHELL_COMPUTER_REQUEST_PAYLOAD_MAX_BYTES: usize = 4096;
/// Text input needs a larger wire envelope because valid caller text may expand
/// under JSON escaping while its decoded UTF-8 body remains capped at 2048 bytes.
pub const SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES: usize = 16 * 1024;
/// Clipboard write allows a 16 KiB decoded UTF-8 body; JSON escaping can expand
/// control characters by up to six bytes each, so keep a separate bounded wire envelope.
pub const SHELL_COMPUTER_CLIPBOARD_WRITE_PAYLOAD_MAX_BYTES: usize = 128 * 1024;

pub fn shell_computer_request_payload_max_bytes(kind: &str) -> usize {
    if kind == "computer_input_text" {
        SHELL_COMPUTER_TEXT_INPUT_PAYLOAD_MAX_BYTES
    } else if kind == "computer_write_clipboard" {
        SHELL_COMPUTER_CLIPBOARD_WRITE_PAYLOAD_MAX_BYTES
    } else {
        SHELL_COMPUTER_REQUEST_PAYLOAD_MAX_BYTES
    }
}
pub const SHELL_CLIENT_CAPABILITY_JOB_STATE_RECONCILIATION: &str = "job_state_reconciliation";
pub const SHELL_CLIENT_CAPABILITY_CODING_AGENT_RUNS: &str = "coding_agent_runs";
/// Capabilities guaranteed by every Runner that explicitly advertises protocol
/// generation 2. These are protocol facts shared with the Runner so its legacy
/// bool projection remains compatible with older Servers. Server authority still
/// uses its typed RunnerFeature classification and verifies this list cannot drift.
pub const AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES: &[&str] = &[
    SHELL_CLIENT_CAPABILITY_FILE_READ,
    SHELL_CLIENT_CAPABILITY_FILE_WRITE,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
    SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE,
    SHELL_CLIENT_CAPABILITY_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_CARGO_TEST_COUNT_ASSERTION,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD,
    SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
    SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY,
    SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE,
    SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION,
];

pub const SHELL_CLIENT_CAPABILITY_NAMES: &[&str] = &[
    SHELL_CLIENT_CAPABILITY_SHELL,
    SHELL_CLIENT_CAPABILITY_FILE_READ,
    SHELL_CLIENT_CAPABILITY_FILE_WRITE,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_CHUNK_READ,
    SHELL_CLIENT_CAPABILITY_ARTIFACT_EXPORT_STREAMING_METADATA,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_FILE_DELETE,
    SHELL_CLIENT_CAPABILITY_APPLY_TEXT_EDIT_OCCURRENCE,
    SHELL_CLIENT_CAPABILITY_GIT,
    SHELL_CLIENT_CAPABILITY_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_JOBS,
    SHELL_CLIENT_CAPABILITY_ASYNC_SHELL_JOBS,
    SHELL_CLIENT_CAPABILITY_SSH_SHELL,
    SHELL_CLIENT_CAPABILITY_PERSISTENT_SHELL,
    SHELL_CLIENT_CAPABILITY_SSH_PERSISTENT_SHELL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_VALIDATION_ARGV,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_CARGO_TEST_COUNT_ASSERTION,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_JSON,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_PROCESS_ARGV,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_SCRIPT_PAYLOAD,
    SHELL_CLIENT_CAPABILITY_INTERNAL_POSIX_SCRIPT,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_EXECUTION_JOBS,
    SHELL_CLIENT_CAPABILITY_DETACHED_PROCESS_JOBS,
    SHELL_CLIENT_CAPABILITY_LSP_READ_ONLY_NAVIGATION,
    SHELL_CLIENT_CAPABILITY_LSP_CALL_HIERARCHY,
    SHELL_CLIENT_CAPABILITY_SANDBOX_INSPECT_COMMANDS,
    SHELL_CLIENT_CAPABILITY_PROJECT_LIFECYCLE,
    SHELL_CLIENT_CAPABILITY_PROJECT_PATH_REGISTRATION,
    SHELL_CLIENT_CAPABILITY_SKILL_STORE_READ,
    SHELL_CLIENT_CAPABILITY_SKILL_STORE_MANAGE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY,
    SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH,
    SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SNAPSHOT_REGION,
    SHELL_CLIENT_CAPABILITY_COMPUTER_ACCESSIBILITY_OBSERVE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_ELEMENT_STATE,
    SHELL_CLIENT_CAPABILITY_JOB_STATE_RECONCILIATION,
    SHELL_CLIENT_CAPABILITY_CODING_AGENT_RUNS,
    SHELL_CLIENT_CAPABILITY_COMPUTER_CONTROL,
    SHELL_CLIENT_CAPABILITY_COMPUTER_SCROLL_TO_ELEMENT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_KEY_INPUT,
    SHELL_CLIENT_CAPABILITY_COMPUTER_WINDOW_ACTIVATE,
    SHELL_CLIENT_CAPABILITY_COMPUTER_TEXT_INPUT,
];

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
/// Same-process terminal results remain available long enough for ordinary
/// reconnect backoff without becoming an unbounded process-lifetime ledger.
pub const JOB_TERMINAL_RETENTION_SECS: i64 = 15 * 60;

/// Legacy registration/poll refreshes remain inline only while they fit this
/// bounded batch. This is a wire-compatibility threshold, not a Runner project
/// cardinality limit: larger inventories use [`ShellProjectInventoryPage`].
pub const PROJECT_INVENTORY_INLINE_MAX_SUMMARIES: usize = 64;
/// Maximum summaries in one project-inventory page. Cardinality is bounded per
/// request rather than across the lifetime of a Runner.
pub const PROJECT_INVENTORY_PAGE_MAX_SUMMARIES: usize = 64;
/// Maximum serialized JSON bytes for one inventory page.
pub const PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES: usize = 256 * 1024;
/// Maximum staged serialized inventory bytes for one in-progress snapshot. A
/// Runner that exceeds this remains live; only project inventory sync degrades.
pub const PROJECT_INVENTORY_SNAPSHOT_MAX_SERIALIZED_BYTES: usize = 16 * 1024 * 1024;
/// Opaque generation identifiers are deliberately small and never carry paths.
pub const PROJECT_INVENTORY_GENERATION_MAX_BYTES: usize = 96;
/// Incomplete server-side staging is discarded after this many seconds without
/// publishing a partial snapshot.
pub const PROJECT_INVENTORY_STAGING_TTL_SECS: i64 = 120;
/// Bound concurrent temporary inventory staging across one Server process.
pub const PROJECT_INVENTORY_MAX_CONCURRENT_SYNCS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellClientCapabilities {
    #[serde(default = "default_shell_true")]
    pub shell: bool,
    #[serde(default)]
    pub file_read: bool,
    #[serde(default)]
    pub file_write: bool,
    /// Internal bounded export-segment read that does not recompute whole-file
    /// MIME/SHA metadata. Missing on older Runners is false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub artifact_export_chunk_read: bool,
    /// Whole-file export metadata (size/SHA/MIME) is computed without loading
    /// the complete artifact into memory. Missing on older Runners is false and
    /// is never inferred from `artifact_export_chunk_read`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub artifact_export_streaming_metadata: bool,
    /// Bounded structured file deletion with Runner-authoritative project-root
    /// containment and file-only semantics. Missing on older Runners is false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub structured_file_delete: bool,
    /// Correct enforcement of ApplyTextEditInput.occurrence. Missing on older
    /// Runners is false and is never inferred from another capability.
    #[serde(default, skip_serializing_if = "is_false")]
    pub apply_text_edit_occurrence: bool,
    #[serde(default)]
    pub git: bool,
    #[serde(default)]
    pub jobs: bool,
    #[serde(default)]
    pub async_jobs: bool,
    #[serde(default)]
    pub async_shell_jobs: bool,
    /// The Runner can execute one-shot/background shell work through a Workflow
    /// Session's configured SSH resource. Missing on older runners fails closed.
    #[serde(default)]
    pub ssh_shell: bool,
    /// The Runner supports explicit Workflow Session persistent shells on its
    /// own host. This does not imply SSH or PTY support.
    #[serde(default)]
    pub persistent_shell: bool,
    /// The Runner can open a long-lived persistent shell on a Workflow Session's
    /// SSH resource. Missing on older runners fails closed; this is independent
    /// of `ssh_shell` and is never inferred from another capability combination.
    #[serde(default)]
    pub ssh_persistent_shell: bool,
    /// Validation plans use a fixed executable plus argv, never shell text.
    /// Missing on older agents and therefore fail-closed.
    #[serde(default)]
    pub structured_validation_argv: bool,
    /// Durable round-trip support for Cargo test-count postconditions.
    #[serde(default, skip_serializing_if = "is_false")]
    pub structured_cargo_test_count_assertion: bool,
    /// Machine-readable canonical `go test -json` validation. Older Runners may
    /// support only the historical fixed `./...` scope; focused package argv is
    /// an independent additive capability.
    #[serde(default, skip_serializing_if = "is_false")]
    pub structured_go_test_json: bool,
    /// First-class `go_test` tool plus its durable validation metadata identity.
    /// Missing on older Runners and false; never inferred from Go JSON parsing,
    /// generic structured validation, protocol version, or executable presence.
    #[serde(default, skip_serializing_if = "is_false")]
    pub structured_go_test_tool: bool,
    /// Expanded first-class `go_test` package argv beyond the historical fixed
    /// `./...` shape. Missing on older Runners and false; never inferred from
    /// the existing Go JSON or first-class tool capabilities.
    #[serde(default, skip_serializing_if = "is_false")]
    pub structured_go_test_packages: bool,
    /// General native executable + argv requests. Missing on older agents and
    /// therefore false; the Server must fail closed without a shell fallback.
    #[serde(default)]
    pub structured_process_argv: bool,
    /// Bounded typed script payloads executed from Runner-owned temporary
    /// files. Missing on older agents and therefore false; this is never
    /// inferred from shell, validation argv, or process argv support.
    #[serde(default)]
    pub structured_script_payload: bool,
    /// Dedicated server-generated POSIX script request kind. Missing on older
    /// Runners is false and is never inferred from raw shell or typed public
    /// script support.
    #[serde(default, skip_serializing_if = "is_false")]
    pub internal_posix_script: bool,
    /// Typed process and typed script requests can execute as durable Jobs.
    /// Missing on older agents and therefore false; it is never inferred from
    /// any synchronous structured-execution or async-shell capability.
    #[serde(default)]
    pub structured_execution_jobs: bool,
    /// Durable detached native-process ownership handoff. This is an additive
    /// authority fence and is never implied by structured_process_argv or
    /// structured_execution_jobs.
    #[serde(default, skip_serializing_if = "is_false")]
    pub detached_process_jobs: bool,
    /// Read-only semantic navigation via constrained Runner language-server
    /// profiles. Defaults to false for wire compatibility with older agents.
    #[serde(default)]
    pub lsp_read_only_navigation: bool,
    /// The Runner implements the bounded typed call-hierarchy operation.
    #[serde(default)]
    pub lsp_call_hierarchy: bool,
    /// The runner can fail-closed enforce the Linux Landlock ABI v3 write
    /// sandbox used by inspect commands.
    #[serde(default)]
    pub sandbox_inspect_commands: bool,
    /// Structured project enable/disable/unregister requests. Missing on older
    /// runners and therefore fail-closed.
    #[serde(default)]
    pub project_lifecycle: bool,
    /// The Runner can resolve an absolute canonical project path or
    /// atomically register it. Missing on older runners and therefore
    /// fail-closed.
    #[serde(default)]
    pub project_path_registration: bool,
    /// Read-only operator-installed Skill store support. Missing on older
    /// Runners is false and never follows from generic file_read.
    #[serde(default, skip_serializing_if = "is_false")]
    pub skill_store_read: bool,
    /// Operator Skill store mutation support. Missing on older Runners is
    /// false and never follows from skill_store_read or file_write.
    #[serde(default, skip_serializing_if = "is_false")]
    pub skill_store_manage: bool,
    /// Native read-only desktop/window observation. Missing on older Runners
    /// and therefore fail-closed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_observe: bool,
    /// The Runner can return a bounded process-local installed-application list.
    /// Missing on older Runners is false and never follows from observation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_application_discovery: bool,
    /// The Runner can submit an exact native launch for a fresh application_id.
    /// Missing on older Runners is false and never follows from discovery/control.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_application_launch: bool,
    /// The Runner can discover exact native displays and snapshot one fresh
    /// opaque display handle. Missing is false and never follows from window observation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_display_observe: bool,
    /// The Runner implements snapshot-fenced exact coordinate pointer input.
    /// Missing on older Runners is false and never follows from other Computer capabilities.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_pointer_control: bool,
    /// The Runner supports bounded native Unicode-text clipboard observation.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_clipboard_read: bool,
    /// The Runner supports bounded native Unicode-text clipboard replacement.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_clipboard_write: bool,
    /// The Runner supports bounded region/max-output snapshot transforms while
    /// preserving the existing whole-window snapshot wire for older Runners.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_snapshot_region: bool,
    /// Native read-only semantic accessibility inspection. Missing on older
    /// Runners is false; future computer control requires a distinct capability.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_accessibility_observe: bool,
    /// The Runner can revalidate one exact element and return normalized read-only
    /// affordances without exposing its true value. Missing on older Runners is false.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_element_state: bool,
    /// The runner retains bounded active and recent terminal job snapshots and
    /// Native bounded accessibility control. Missing on older Runners is false
    /// and never follows from desktop or accessibility observation authority.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_control: bool,
    /// The Runner can semantically scroll one exact observed Accessibility element
    /// into view. Missing on older Runners is false and never follows from control.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_scroll_to_element: bool,
    /// The Runner can post one closed navigation/action key to one exact already-focused
    /// native window. Missing on older Runners is false and never follows from control.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_key_input: bool,
    /// The Runner can activate/raise one exact previously observed native window.
    /// Missing on older Runners is false and never follows from computer_control.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_window_activate: bool,
    /// The Runner implements bounded native Accessibility text input. Missing on
    /// older Runners is false and never follows from computer_control.
    #[serde(default, skip_serializing_if = "is_false")]
    pub computer_text_input: bool,
    /// submits a complete active inventory at register/re-register time.
    #[serde(default, skip_serializing_if = "is_false")]
    pub job_state_reconciliation: bool,
    /// Runner-owned ACP coding-agent execution with closed typed Run lifecycle.
    /// Missing on older Runners is false and is never inferred from shell/MCP.
    #[serde(default, skip_serializing_if = "is_false")]
    pub coding_agent_runs: bool,
    /// Registration-only compatibility envelope for explicit protocol generation.
    ///
    /// This is deliberately nested under the historically additive capabilities
    /// object so the current -> latest-stable compatibility contract keeps the
    /// top-level registration shape unchanged. Stable v0.3.8 itself also ignores
    /// unknown top-level struct fields, so nesting is a conservative compatibility
    /// policy rather than a parser requirement. It is not a RunnerFeature and
    /// Server ingress removes it before retaining the legacy capability projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_protocol_generation: Option<AgentProtocolGenerationNumber>,
}

/// Bounded, non-secret status for the agent's active configuration generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfigReloadStatus {
    pub generation: u64,
    pub last_reload_result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_reload_error_code: Option<String>,
    pub restart_required: bool,
    #[serde(default)]
    pub restart_required_fields: Vec<String>,
}

impl Default for AgentConfigReloadStatus {
    fn default() -> Self {
        Self {
            generation: 1,
            last_reload_result: "not_attempted".to_string(),
            last_reload_error_code: None,
            restart_required: false,
            restart_required_fields: Vec::new(),
        }
    }
}

impl Default for ShellClientCapabilities {
    fn default() -> Self {
        Self {
            shell: true,
            file_read: false,
            file_write: false,
            artifact_export_chunk_read: false,
            artifact_export_streaming_metadata: false,
            structured_file_delete: false,
            apply_text_edit_occurrence: false,
            git: false,
            jobs: false,
            async_jobs: false,
            async_shell_jobs: false,
            ssh_shell: false,
            persistent_shell: false,
            ssh_persistent_shell: false,
            structured_validation_argv: false,
            structured_cargo_test_count_assertion: false,
            structured_go_test_json: false,
            structured_go_test_tool: false,
            structured_go_test_packages: false,
            structured_process_argv: false,
            structured_script_payload: false,
            internal_posix_script: false,
            structured_execution_jobs: false,
            detached_process_jobs: false,
            lsp_read_only_navigation: false,
            lsp_call_hierarchy: false,
            sandbox_inspect_commands: false,
            project_lifecycle: false,
            project_path_registration: false,
            skill_store_read: false,
            skill_store_manage: false,
            computer_observe: false,
            computer_application_discovery: false,
            computer_application_launch: false,
            computer_snapshot_region: false,
            computer_display_observe: false,
            computer_pointer_control: false,
            computer_clipboard_read: false,
            computer_clipboard_write: false,
            computer_accessibility_observe: false,
            computer_element_state: false,
            computer_control: false,
            computer_scroll_to_element: false,
            computer_key_input: false,
            computer_window_activate: false,
            computer_text_input: false,
            job_state_reconciliation: false,
            coding_agent_runs: false,
            agent_protocol_generation: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentProjectSummary {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub path: String,
    #[serde(default = "default_shell_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
    #[serde(default)]
    pub disabled: bool,
    /// Stable SHA-256 revision of the persisted projects.d TOML content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub git_head: Option<String>,
    #[serde(default)]
    pub git_dirty: Option<bool>,
    pub updated_at: i64,
    /// Project-bound shell profile name (`project.shell_profile`). Non-secret:
    /// just a profile name. `None` means the project did not override the
    /// profile, so the agent falls back to `shell.default_profile`. Carried so
    /// `listProjects` / `runtime_status` can show which profile a project uses
    /// without exposing env values or init_script contents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_profile: Option<String>,
}

/// One bounded page of a complete Runner project snapshot. Pages are strictly
/// ordered starting at zero. `complete=true` is valid only for the final page;
/// the Server publishes the staged snapshot atomically at that point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellProjectInventoryPage {
    pub generation: String,
    /// Monotonic within one `agent_instance_id`. The Server keeps a high-water
    /// mark so arbitrarily old generations cannot be replayed after bounded
    /// generation metadata has been cleaned up.
    pub snapshot_sequence: u64,
    pub page_index: u32,
    pub total_reported: usize,
    #[serde(default)]
    pub complete: bool,
    #[serde(default)]
    pub projects: Vec<ShellAgentProjectSummary>,
}

/// Compact, path-free synchronization status projected by a new Server. Its
/// presence is also the rolling-upgrade negotiation signal: an old Server omits
/// the field, so a new Runner never sends page envelopes it cannot decode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellProjectInventoryStatus {
    pub sync_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_reported: Option<usize>,
    pub total_synced: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sync_at: Option<i64>,
    pub max_summaries_per_page: usize,
    pub max_serialized_bytes_per_page: usize,
}

impl ShellProjectInventoryStatus {
    pub fn pending(total_synced: usize) -> Self {
        Self {
            sync_state: "pending".to_string(),
            generation: None,
            total_reported: None,
            total_synced,
            last_error_code: None,
            last_sync_at: None,
            max_summaries_per_page: PROJECT_INVENTORY_PAGE_MAX_SUMMARIES,
            max_serialized_bytes_per_page: PROJECT_INVENTORY_PAGE_MAX_SERIALIZED_BYTES,
        }
    }
}

/// Sanitized summary of one configured shell profile. Exposes ONLY safe
/// metadata: whether an init_script is set (boolean, never the body), the
/// number of env keys (never the values), the resolved program, and the arg
/// count. Used by `ShellProfilesSummary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellProfileSummaryEntry {
    pub name: String,
    pub has_init_script: bool,
    pub env_keys_count: usize,
    pub program: String,
    pub args_count: usize,
    /// Shell dialect for this profile derived from the resolved program
    /// basename: `sh`, `bash`, or `custom`. Older agents omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialect: Option<String>,
}

/// Sanitized summary of an agent's prepared-shell-profile configuration.
/// Reported by the agent at registration (carried inside `AgentPolicySummary`)
/// and exposed in `runtime_status` / `listAgents` / `listProjects` so users can
/// see which profiles are configured and which one a project resolves to.
///
/// This summary NEVER includes: init_script bodies, env values, tokens,
/// Authorization headers, full agent.toml, the full env snapshot, or stderr
/// tails. `prepared_cache_count` reflects the number of prepared snapshots at
/// the last registration (snapshots are prepared lazily on first use, so this
/// is typically 0 right after agent start; it is not a live counter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellProfilesSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_profile: Option<String>,
    pub configured_count: usize,
    pub prepared_cache_count: usize,
    pub profiles: Vec<ShellProfileSummaryEntry>,
    /// Dialect of the default execution path when no explicit `shell` is
    /// selected: `sh`, `bash`, or `custom`. Reported by the runner from its
    /// actual configuration; the server never guesses. Older agents omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_dialect: Option<String>,
    /// Dialects an explicit `shell=` selection can resolve to on this runner
    /// (always includes `sh` and `bash`; configured custom profiles add
    /// `custom`). Older agents omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_dialects: Option<Vec<String>>,
}

/// Bounded, non-secret snapshot of the agent's experimental tool providers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolProvidersStatus {
    pub strategy: String,
    pub claude_code: ClaudeCodeProviderStatus,
    #[serde(default)]
    pub config_reload: AgentConfigReloadStatus,
}

/// Bounded evidence for the most recent allowlisted external-provider route.
/// This is metadata only: it never carries tool arguments, file contents,
/// project paths, stderr, or raw RPC payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCallSummary {
    pub capability: String,
    pub selected_provider: String,
    pub fallback_used: bool,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_state: Option<String>,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaudeCodeProviderStatus {
    pub enabled: bool,
    pub version: Option<String>,
    pub available: bool,
    pub process_state: String,
    pub discovered_tool_names: Vec<String>,
    pub capabilities: BTreeMap<String, String>,
    pub last_error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_call: Option<ProviderCallSummary>,
}

/// Sanitized agent policy summary. Carried in the registration payload and
/// exposed in `runtime_status` / `listAgents`. Contains ONLY non-secret
/// fields: it never includes the agent token, shell env values, init_script
/// contents, or full agent.toml contents. `allowed_roots` is intentionally
/// exposed as a path-policy summary. `shell_profiles` carries the sanitized
/// prepared-shell-profile configuration summary (profile names, default
/// profile, counts) so observability can show which profile a project uses;
/// it never carries env values or init_script bodies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPolicySummary {
    #[serde(default = "default_shell_true")]
    pub allow_raw_shell: bool,
    #[serde(default)]
    pub allow_cwd_anywhere: bool,
    #[serde(default)]
    pub allowed_roots: Vec<PathBuf>,
    #[serde(default = "default_policy_max_timeout_secs")]
    pub max_timeout_secs: u64,
    #[serde(default = "default_policy_max_output_bytes")]
    pub max_output_bytes: usize,
    /// Sanitized prepared-shell-profile summary. `None` for older agents that
    /// did not report one. Never carries env values or init_script bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_profiles: Option<ShellProfilesSummary>,
    /// Read-only provider state captured when the agent registers/reconnects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_providers: Option<ToolProvidersStatus>,
    /// Bounded, non-secret inventory of exact Runner-owned MCP provider
    /// instances captured at registration. This is liveness/capability
    /// metadata only; executable paths, argv, environment, PIDs, and stderr
    /// are never projected to the Server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_gateway_providers: Option<Vec<crate::mcp_gateway::McpGatewayProvider>>,
}

impl Default for AgentPolicySummary {
    fn default() -> Self {
        Self {
            allow_raw_shell: true,
            allow_cwd_anywhere: false,
            allowed_roots: Vec::new(),
            max_timeout_secs: default_policy_max_timeout_secs(),
            max_output_bytes: default_policy_max_output_bytes(),
            shell_profiles: None,
            tool_providers: None,
            mcp_gateway_providers: None,
        }
    }
}

fn default_policy_max_timeout_secs() -> u64 {
    3600
}

fn default_policy_max_output_bytes() -> usize {
    256 * 1024
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientRegisterRequest {
    pub client_id: String,
    /// Stable per-process identity for the registering agent. Generated once
    /// by `webcodex-runner` at startup and reused for the whole process
    /// lifetime (including WebSocket reconnects). The server treats this as
    /// the active agent lease identity: a second agent process with the same
    /// `client_id` but a different `agent_instance_id` is rejected while the
    /// first is online, and a stale/replaced instance can no longer poll or
    /// submit results. It is not a secret.
    pub agent_instance_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub capabilities: Option<ShellClientCapabilities>,
    /// Optional bounded planning context declared by the Runner configuration.
    /// This is descriptive metadata only: it never grants authority or proves
    /// current host/service/network state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_context: Option<AgentHostContext>,
    #[serde(default)]
    pub projects: Option<Vec<ShellAgentProjectSummary>>,
    /// Protocol identity announced by the agent during registration. The wire
    /// field remains optional at deserialization so every transport can return
    /// the same explicit registration-validation error for an omitted field;
    /// successful registration requires a non-empty value.
    #[serde(default)]
    pub agent_protocol_version: Option<String>,
    /// Sanitized agent policy summary. Older agents that omit this field
    /// register with `None`; `runtime_status` / `listAgents` then expose
    /// `null` for the policy so older/minimal payloads stay compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<AgentPolicySummary>,
    /// Unix timestamp when the agent process started. Reported by the runner
    /// itself so `runner_process` observations come from the process, not
    /// from server-side inference. Older agents omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_started_at: Option<i64>,
    /// Runner build metadata (crate version and optional git commit). Used
    /// for mixed-version diagnostics; never carries paths or environment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<AgentBuildInfo>,
    /// Effective static Job execution concurrency configured for this Runner
    /// process. Older Runners omit it, so the Server must preserve `None`
    /// rather than infer a value from capabilities or observed Jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_concurrency_limit: Option<usize>,
    /// Complete active plus bounded recent-terminal inventory for the current
    /// runner process. Required when `job_state_reconciliation` is declared;
    /// absent for older runners.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_inventory: Option<ShellJobInventory>,
    /// Bounded non-secret startup-owned ACP provider inventory. Executable,
    /// argv, environment, credentials, PID, and private ACP session ids are
    /// never projected to the Server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent_providers: Option<Vec<crate::coding_agent::CodingAgentProvider>>,
    /// Complete active plus bounded recent-terminal CodingAgentRun inventory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent_inventory: Option<crate::coding_agent::CodingAgentRunInventory>,
}

/// Non-secret runner build identity for mixed-version diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentBuildInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    /// Whether the build workspace contained source changes when build metadata
    /// was captured. `None` means exact source alignment is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_dirty: Option<bool>,
}

pub const AGENT_HOST_CONTEXT_ROLE_MAX_BYTES: usize = 64;
pub const AGENT_HOST_CONTEXT_TEXT_MAX_BYTES: usize = 512;
pub const AGENT_HOST_CONTEXT_TOTAL_MAX_BYTES: usize = 1_536;

/// Small, closed planning context attached to one Runner registration.
///
/// These values are human-authored hints for choosing between already-valid
/// execution paths. They are not capabilities, policy, connection state, or
/// authorization. `source = runner_config` is added by model-facing
/// projections rather than accepted from Runner configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentHostContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub architecture: Option<String>,
}

impl AgentHostContext {
    /// Validate and canonicalize untrusted/configured host guidance. Errors name
    /// only the invalid field and bound; they never echo configured values.
    pub fn normalized(mut self) -> Result<Self, String> {
        self.role = normalize_host_role(self.role)?;
        self.runtime = normalize_host_context_text("runtime", self.runtime)?;
        self.service = normalize_host_context_text("service", self.service)?;
        self.network = normalize_host_context_text("network", self.network)?;
        self.architecture = normalize_host_context_text("architecture", self.architecture)?;

        let fields = [
            self.role.as_deref(),
            self.runtime.as_deref(),
            self.service.as_deref(),
            self.network.as_deref(),
            self.architecture.as_deref(),
        ];
        if fields.iter().all(|value| value.is_none()) {
            return Err("host_context must contain at least one field".to_string());
        }
        let total = fields
            .iter()
            .flatten()
            .map(|value| value.len())
            .sum::<usize>();
        if total > AGENT_HOST_CONTEXT_TOTAL_MAX_BYTES {
            return Err(format!(
                "host_context content must be at most {AGENT_HOST_CONTEXT_TOTAL_MAX_BYTES} UTF-8 bytes total"
            ));
        }
        Ok(self)
    }
}

fn normalize_host_role(value: Option<String>) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.len() > AGENT_HOST_CONTEXT_ROLE_MAX_BYTES {
        return Err(format!(
            "host_context.role must be 1..={AGENT_HOST_CONTEXT_ROLE_MAX_BYTES} UTF-8 bytes"
        ));
    }
    if !value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return Err(
            "host_context.role may only contain lowercase ASCII letters, digits, '_' and '-'"
                .to_string(),
        );
    }
    Ok(Some(value.to_string()))
}

fn normalize_host_context_text(
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.len() > AGENT_HOST_CONTEXT_TEXT_MAX_BYTES {
        return Err(format!(
            "host_context.{field} must be 1..={AGENT_HOST_CONTEXT_TEXT_MAX_BYTES} UTF-8 bytes"
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(format!(
            "host_context.{field} must not contain control characters"
        ));
    }
    Ok(Some(value.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientView {
    pub client_id: String,
    /// Active agent process identity (UUID) for this client. Empty for views
    /// that predate the instance id field. Not a secret.
    #[serde(default)]
    pub agent_instance_id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub hostname: Option<String>,
    pub status: String,
    /// Bounded descriptive planning context from Runner configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_context: Option<AgentHostContext>,
    pub connected: bool,
    pub last_seen: i64,
    pub capabilities: ShellClientCapabilities,
    /// Bounded sanitized startup-owned ACP provider inventory. Logical ids are
    /// model-visible planning metadata; executable/argv/env/PID/private ACP ids
    /// never enter this view.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent_providers: Option<Vec<crate::coding_agent::CodingAgentProvider>>,
    pub pending_requests: usize,
    #[serde(default)]
    pub projects: Vec<ShellAgentProjectSummary>,
    /// Project inventory synchronization health. Missing means this Server
    /// predates paged inventory synchronization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_inventory: Option<ShellProjectInventoryStatus>,
    /// Agent-announced protocol identity preserved for diagnostics. Successful
    /// registration always supplies this value; there is no omission fallback.
    pub agent_protocol_version: String,
    /// Canonical Server-normalized semantics for business decisions. The raw
    /// announced label above remains diagnostics-only after registration ingress.
    #[serde(skip)]
    pub agent_protocol_semantics: AgentProtocolSemantics,
    /// Transport the agent is currently connected over: `"polling"`,
    /// `"websocket"`, or `"quic"`. Defaults to `"polling"` for older
    /// agents/views.
    #[serde(default = "default_transport_polling")]
    pub transport: String,
    /// Sanitized agent policy summary reported at registration. `None`
    /// (serialized as `null`/omitted) for older agents that did not report a
    /// policy. Never contains token/env/init_script.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<AgentPolicySummary>,
    /// When this client_id first registered its current agent instance.
    #[serde(default)]
    pub registered_at: i64,
    /// When the current transport connection was established (register or
    /// reconnect). `0` for views that predate connection lifecycle tracking.
    #[serde(default)]
    pub connected_at: i64,
    /// When the server observed the last transport disconnect for the current
    /// instance, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disconnected_at: Option<i64>,
    /// Runner-reported process start timestamp, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_started_at: Option<i64>,
    /// Runner-reported build identity, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<AgentBuildInfo>,
    /// Runner-reported effective static Job execution concurrency. `None` for
    /// older Runners; the Server never infers this value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_concurrency_limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellClientRegisterResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ShellClientView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunRequest {
    pub client_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    #[serde(default = "default_wait_timeout_secs")]
    pub wait_timeout_secs: u64,
}

/// Structured native process representation carried end-to-end. This is the
/// execution source of truth for `run_process`; `ShellAgentShellRequest.command`
/// stays empty and is never populated by quoting or JSON-encoding this value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellProcessArgv {
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Explicit script language selected by the model-facing `run_script`
/// contract. The Runner owns the mapping from this semantic language to a
/// concrete interpreter; no executable path or custom shell grammar is
/// accepted from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShellScriptLanguage {
    Sh,
    Bash,
    Powershell,
}

impl ShellScriptLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Bash => "bash",
            Self::Powershell => "powershell",
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Sh | Self::Bash => ".sh",
            Self::Powershell => ".ps1",
        }
    }
}

/// Structured script representation carried end-to-end. This is the
/// execution source of truth for `run_script`;
/// `ShellAgentShellRequest.command` stays empty and is never populated by
/// quoting or JSON-encoding this value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellScriptPayload {
    pub language: ShellScriptLanguage,
    pub script: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Conservative cross-platform process-input limits.
///
/// Windows ultimately represents argv in a CreateProcess command line even
/// when callers use `Command::args`. Limiting the raw UTF-8 executable + args
/// plus one boundary byte per value to 16,000 bytes leaves room below the
/// 32,767 UTF-16-unit platform ceiling even when quoting doubles every byte.
/// The limit still permits a structured invocation larger than the legacy
/// 8,000-byte shell-command channel. Long scripts/text belong to `run_script`.
pub const PROCESS_EXECUTABLE_MAX_BYTES: usize = 1_024;
pub const PROCESS_ARG_MAX_COUNT: usize = 256;
pub const PROCESS_ARG_MAX_BYTES: usize = 8_192;
pub const PROCESS_ARGV_MAX_BYTES: usize = 16_000;
pub const PROCESS_STDIN_MAX_BYTES: usize = 64 * 1_024;
pub const PROCESS_CWD_MAX_BYTES: usize = 1_024;
pub const DETACHED_IDEMPOTENCY_KEY_MAX_BYTES: usize = 128;
pub const STRUCTURED_EXECUTION_TIMEOUT_MIN_SECS: u64 = 1;
pub const STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS: u64 = 3_600;
pub const STRUCTURED_EXECUTION_TIMEOUT_DEFAULT_SECS: u64 = 60;
/// Compatibility ceiling for the pre-Phase-C direct synchronous Runner wire.
/// Durable typed Jobs use `STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS` instead.
pub const STRUCTURED_EXECUTION_LEGACY_SYNC_TIMEOUT_MAX_SECS: u64 = 120;
pub const PROCESS_TIMEOUT_MAX_SECS: u64 = STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS;

pub const SCRIPT_MIN_BYTES: usize = 1;
pub const SCRIPT_MAX_BYTES: usize = 512 * 1024;
pub const SCRIPT_ARG_MAX_COUNT: usize = 256;
pub const SCRIPT_ARG_MAX_BYTES: usize = 8_192;
pub const SCRIPT_ARGV_MAX_BYTES: usize = 16_000;
pub const SCRIPT_STDIN_MAX_BYTES: usize = 64 * 1024;
pub const SCRIPT_CWD_MAX_BYTES: usize = 1_024;
pub const SCRIPT_TIMEOUT_MAX_SECS: u64 = STRUCTURED_EXECUTION_TIMEOUT_MAX_SECS;

/// Validate the transport-neutral executable/argv payload. Both Server and
/// Runner call this so a stale or malicious peer cannot bypass either side.
pub fn validate_process_argv(process: &ShellProcessArgv) -> Result<(), String> {
    if process.executable.trim().is_empty() {
        return Err("executable must not be empty".to_string());
    }
    if process.executable.len() > PROCESS_EXECUTABLE_MAX_BYTES {
        return Err(format!(
            "executable is too long; maximum is {PROCESS_EXECUTABLE_MAX_BYTES} bytes"
        ));
    }
    if process.executable.contains('\0') {
        return Err("executable cannot contain NUL bytes".to_string());
    }
    if process.args.len() > PROCESS_ARG_MAX_COUNT {
        return Err(format!(
            "args may contain at most {PROCESS_ARG_MAX_COUNT} entries"
        ));
    }
    let mut total = process.executable.len();
    for (index, arg) in process.args.iter().enumerate() {
        if arg.len() > PROCESS_ARG_MAX_BYTES {
            return Err(format!(
                "args[{index}] is too long; maximum is {PROCESS_ARG_MAX_BYTES} bytes"
            ));
        }
        if arg.contains('\0') {
            return Err(format!("args[{index}] cannot contain NUL bytes"));
        }
        total = total.saturating_add(1).saturating_add(arg.len());
    }
    if total > PROCESS_ARGV_MAX_BYTES {
        return Err(format!(
            "executable and args are too large; maximum total is {PROCESS_ARGV_MAX_BYTES} bytes"
        ));
    }
    if process_uses_shell_command_mode(process) {
        return Err(
            "run_process does not accept shell command modes; use run_shell for shell syntax"
                .to_string(),
        );
    }
    Ok(())
}

/// Validate the complete transport-neutral script request. Both Server and
/// Runner call this exact helper so bounds and rejection ordering cannot drift.
/// Whitespace-only scripts are valid; only a zero-byte script is rejected.
pub fn validate_script_request(
    payload: &ShellScriptPayload,
    stdin: Option<&str>,
    cwd: Option<&str>,
    timeout_secs: u64,
) -> Result<(), String> {
    if payload.script.len() < SCRIPT_MIN_BYTES {
        return Err("script must contain at least 1 UTF-8 byte".to_string());
    }
    if payload.script.len() > SCRIPT_MAX_BYTES {
        return Err(format!(
            "script is too large; maximum is {SCRIPT_MAX_BYTES} bytes"
        ));
    }
    if payload.script.contains('\0') {
        return Err("script cannot contain NUL bytes".to_string());
    }
    if payload.args.len() > SCRIPT_ARG_MAX_COUNT {
        return Err(format!(
            "args may contain at most {SCRIPT_ARG_MAX_COUNT} entries"
        ));
    }
    let mut total = 0usize;
    for (index, arg) in payload.args.iter().enumerate() {
        if arg.len() > SCRIPT_ARG_MAX_BYTES {
            return Err(format!(
                "args[{index}] is too long; maximum is {SCRIPT_ARG_MAX_BYTES} bytes"
            ));
        }
        if arg.contains('\0') {
            return Err(format!("args[{index}] cannot contain NUL bytes"));
        }
        total = total.saturating_add(1).saturating_add(arg.len());
    }
    if total > SCRIPT_ARGV_MAX_BYTES {
        return Err(format!(
            "script args are too large; maximum total is {SCRIPT_ARGV_MAX_BYTES} bytes"
        ));
    }
    if let Some(stdin) = stdin {
        if stdin.len() > SCRIPT_STDIN_MAX_BYTES {
            return Err(format!(
                "stdin is too large; maximum is {SCRIPT_STDIN_MAX_BYTES} bytes"
            ));
        }
        if stdin.contains('\0') {
            return Err("stdin cannot contain NUL bytes".to_string());
        }
    }
    if let Some(cwd) = cwd {
        if cwd.len() > SCRIPT_CWD_MAX_BYTES {
            return Err(format!(
                "cwd is too long; maximum is {SCRIPT_CWD_MAX_BYTES} bytes"
            ));
        }
        if cwd.contains('\0') {
            return Err("cwd cannot contain NUL bytes".to_string());
        }
    }
    if timeout_secs == 0 || timeout_secs > SCRIPT_TIMEOUT_MAX_SECS {
        return Err(format!(
            "timeout_secs must be between 1 and {SCRIPT_TIMEOUT_MAX_SECS}"
        ));
    }
    Ok(())
}

/// Reject the shell-parser forms that would turn `run_process` back into a
/// shell-text transport. Batch files remain a Runner-managed Windows launch
/// mode and do not pass model-authored `/C` text through this check.
pub fn process_uses_shell_command_mode(process: &ShellProcessArgv) -> bool {
    let basename = process
        .executable
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(process.executable.as_str())
        .to_ascii_lowercase();
    match basename.as_str() {
        "sh" | "sh.exe" | "bash" | "bash.exe" => process
            .args
            .iter()
            .take_while(|arg| *arg != "--")
            .any(|arg| {
                let lower = arg.to_ascii_lowercase();
                lower == "-c"
                    || (lower.starts_with('-')
                        && !lower.starts_with("--")
                        && lower[1..].contains('c'))
            }),
        "powershell" | "powershell.exe" | "pwsh" | "pwsh.exe" => process
            .args
            .iter()
            .any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "-command" | "-c")),
        "cmd" | "cmd.exe" => process
            .args
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("/c")),
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellRunResponse {
    pub success: bool,
    pub request_id: String,
    pub client_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub command_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Server-owned dispatch evidence for synchronous requests. `Some(false)`
    /// proves the request never left the queue; `Some(true)` does not by
    /// itself prove that a command process was spawned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_dispatched: Option<bool>,
    /// Runner-owned command lifecycle evidence. Absent for non-command
    /// requests and legacy Runner results.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_execution_state: Option<ShellCommandExecutionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentPollRequest {
    pub client_id: String,
    /// Active agent process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    pub agent_instance_id: String,
    #[serde(default)]
    pub projects: Option<Vec<ShellAgentProjectSummary>>,
}

/// Polling transport wrapper. Existing `ShellAgentPollRequest` JSON remains
/// valid; current agents may attach changed-only sanitized runtime metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentPollPayload {
    #[serde(flatten)]
    pub request: ShellAgentPollRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_providers: Option<ToolProvidersStatus>,
    /// Optional bounded project inventory page. Old Servers ignore this field;
    /// new Runners send it only after registration negotiated support.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_inventory_page: Option<ShellProjectInventoryPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobStatusRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobLogRequest {
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
pub struct ShellClientJobStopRequest {
    #[serde(default)]
    pub client_id: Option<String>,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobsListRequest {
    pub client_id: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentShellRequest {
    pub request_id: String,
    pub client_id: String,
    #[serde(default = "default_agent_request_kind")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub create_dirs: bool,
    pub command: String,
    /// Typed native process payload. Present only for `kind = "run_process"`
    /// or `kind = "start_process_job"`; defaults to `None` for backward
    /// compatibility with older envelopes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<ShellProcessArgv>,
    /// Typed bounded script payload. Present for `kind = "run_script"`,
    /// `kind = "start_script_job"`, or the server-generated
    /// `kind = "run_internal_posix_script"`; defaults to `None` for backward
    /// compatibility with older envelopes. The raw body never enters `command`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<ShellScriptPayload>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
    pub timeout_secs: u64,
    pub requested_by: String,
    pub created_at: i64,
    /// Typed validation bridge payload. Present only for `kind = "validation"`.
    /// Defaults to `None` so older request bodies continue to deserialize.
    /// Never carries arbitrary shell commands — only declarative adapter
    /// requests with project-relative paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<crate::validation_bridge::ValidationBridgeRequest>,
    /// Typed read-only LSP navigation payload. Present only for `kind = "lsp"`.
    /// Defaults to `None` so older request bodies continue to deserialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsp: Option<crate::lsp_bridge::AgentLspPayload>,
    /// Optional kernel sandbox mode (`"inspect"`). Agents fail closed for
    /// unsupported, partial, or unknown modes and never run them unconfined.
    /// Absent on the wire when unset so older agents continue to deserialize.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<String>,
    /// Server-derived safe execution metadata. Async jobs retain it for
    /// same-process recovery; Session-bound SSH shell requests use only the
    /// workflow session/resource fields. It never contains raw command text,
    /// stdin, environment, SSH host/configuration, or credentials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_context: Option<ShellJobContext>,
    /// Explicit persistent-shell lifecycle request. It is independent from
    /// `job_id`/Job state and absent on all legacy request kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_shell: Option<PersistentShellRequest>,
    /// Closed, typed Runner-owned MCP gateway operation. This is deliberately
    /// separate from shell/file fields and never carries arbitrary JSON-RPC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_gateway: Option<crate::mcp_gateway::McpGatewayRequest>,
    /// Closed typed ACP CodingAgentRun operation. Raw ACP JSON-RPC stays local
    /// to the Runner and is never accepted through this transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<crate::coding_agent::CodingAgentRequest>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentPollResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<ShellAgentShellRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Current inventory status/acknowledgement. Missing on old Servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_inventory: Option<ShellProjectInventoryStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentResultRequest {
    pub client_id: String,
    /// Active agent process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    pub agent_instance_id: String,
    pub request_id: String,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: Option<String>,
    #[serde(default)]
    pub stderr: Option<String>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Narrow Runner-owned lifecycle evidence for one synchronous shell command.
/// This is transport metadata, not a general execution framework: the Runner
/// sets it from its actual process-spawn/collection path, while non-command
/// request results omit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellCommandExecutionState {
    NotStarted,
    OutcomeUnknown,
    TimedOut,
    Completed,
}

/// Transport payload for synchronous Runner results. Flattening preserves the
/// existing HTTP/WebSocket/QUIC JSON shape and adds only optional shell-command
/// lifecycle evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentResultPayload {
    #[serde(flatten)]
    pub result: ShellAgentResultRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_execution_state: Option<ShellCommandExecutionState>,
    /// Typed MCP gateway result. Present only for a request whose
    /// `mcp_gateway` field was present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_gateway: Option<crate::mcp_gateway::McpGatewayResponse>,
    /// Typed CodingAgentRun result. Present only for a request whose
    /// `coding_agent` field was present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<crate::coding_agent::CodingAgentResponse>,
}

impl From<ShellAgentResultRequest> for ShellAgentResultPayload {
    fn from(result: ShellAgentResultRequest) -> Self {
        Self {
            result,
            command_execution_state: None,
            mcp_gateway: None,
            coding_agent: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentResultResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Server-to-Runner request for one explicit persistent-shell lifecycle
/// operation. The Runner revalidates the project, policy, cwd, and profile on
/// every request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistentShellRequest {
    pub action: String,
    pub shell_id: String,
    pub workflow_session_id: String,
    pub runtime_project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
}

/// Runner-authoritative result for a persistent-shell lifecycle operation.
/// Output is bounded on the Runner before this value crosses the protocol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistentShellResult {
    pub shell_id: String,
    pub workflow_session_id: String,
    pub runtime_project_id: String,
    pub shell_state: String,
    pub execution_state: String,
    pub command_started: bool,
    pub command_completed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub stdout: String,
    #[serde(default)]
    pub stderr: String,
    #[serde(default)]
    pub stdout_truncated: bool,
    #[serde(default)]
    pub stderr_truncated: bool,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<i64>,
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub already_closed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub close_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentPersistentShellResultRequest {
    pub client_id: String,
    pub agent_instance_id: String,
    pub request_id: String,
    pub result: PersistentShellResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentPersistentShellResultResponse {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellAgentJobUpdateRequest {
    pub client_id: String,
    /// Active agent process identity. Must match the instance that currently
    /// holds the lease for `client_id`; a stale/replaced instance is rejected.
    pub agent_instance_id: String,
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
    #[serde(default)]
    pub stdout_tail: Option<String>,
    #[serde(default)]
    pub stderr_tail: Option<String>,
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
    #[serde(default)]
    pub finished: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ShellAgentJobUpdateResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFileOpRequest {
    pub op: String,
    pub client_id: String,
    pub path: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
    // Retired edit fields remain on the operator-facing request only so the
    // Server can reject old payloads explicitly instead of silently ignoring
    // unknown JSON fields. They are never forwarded to the Runner.
    #[serde(default)]
    pub old_text: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub expected_prefix: Option<String>,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub end_line: Option<usize>,
    #[serde(default)]
    pub line: Option<usize>,
    #[serde(default)]
    pub create_dirs: bool,
    #[serde(default = "default_wait_timeout_secs")]
    pub wait_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellFileOpResponse {
    pub success: bool,
    pub op: String,
    pub request_id: String,
    pub client_id: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
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
    /// `is_canonical`; omitted from the wire when empty so older agents keep
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
/// and `--features <value>` / `-p <value>` pairs, optionally `--no-run`.
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
/// and already normalized (no leading/trailing whitespace). `--no-run` is
/// accepted only for `cargo test`.
fn is_canonical_cargo_flags(args: &[&str], allow_no_run: bool) -> bool {
    let mut seen = HashSet::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let key = match *arg {
            "--all-targets" | "--all-features" | "--no-default-features" => *arg,
            "--no-run" if allow_no_run => "--no-run",
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
    /// Effective caller-requested minimum Cargo test count. This is an
    /// observation postcondition, not part of the executable argv.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_tests: Option<u64>,
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
                .minimum_tests
                .is_some_and(|minimum| !(1..=CARGO_TEST_MIN_TESTS_MAX).contains(&minimum))
        {
            return false;
        }
        if self.minimum_tests.is_some() && self.tool != "cargo_test" {
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
/// server restart. This is an internal agent protocol model, not a public
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
/// reconciliation. Raw command, stdin, environment, tokens, and agent config
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
pub struct ShellAgentShellJobResult {
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
pub struct ShellAgentJobResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell: Option<ShellAgentShellJobResult>,
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
    pub result: Option<ShellAgentJobResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_progress: Option<ShellJobValidationProgress>,
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
pub struct ShellClientJobStatusResponse {
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
    pub result: Option<ShellAgentJobResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellClientJobLogResponse {
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
pub struct ShellClientJobStopResponse {
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
pub struct ShellClientJobsListResponse {
    pub success: bool,
    pub client_id: String,
    pub jobs: Vec<ShellJobInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ============================================================================
// Transport-neutral agent message envelope
// ============================================================================
//
// A single message format used by the WebSocket agent transport (and future
// QUIC transport). It wraps the existing polling protocol payloads so the
// server and agent never duplicate business logic: register/request/result/
// job_update reuse the same structs as the HTTP polling endpoints.
//
// Wire format is JSON with an internal `type` tag:
//
//   {"type":"register","client_id":"...","projects":[...]}
//   {"type":"registered","success":true,"client":{...}}
//   {"type":"request","request_id":"...","client_id":"...","kind":"run_shell",...}
//   {"type":"result","client_id":"...","request_id":"...","exit_code":0,...}
//   {"type":"job_update","client_id":"...","job_id":"...","status":"running",...}
//   {"type":"ping","ts":1700000000}
//   {"type":"pong","ts":1700000000}
//   {"type":"goodbye","reason":"shutdown"}
//   {"type":"error","code":"bad_request","message":"..."}
//
// The envelope is transport-neutral: it carries no WebSocket-specific fields
// and could be framed over QUIC streams unchanged.

/// One agent transport message. Used by both the server WebSocket handler and
/// the `webcodex-runner` WebSocket client mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEnvelope {
    /// Agent -> server registration application envelope. WebSocket sends this
    /// after its authenticated HTTP handshake. QUIC keeps authentication in its
    /// transport-specific first-register codec and enters the shared envelope
    /// lifecycle only after transport authentication succeeds.
    Register {
        #[serde(flatten)]
        payload: ShellClientRegisterRequest,
    },
    /// Server -> agent. Acknowledgement of `Register`.
    Registered {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        client: Option<ShellClientView>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Server -> agent. A pending shell/file/job request pushed to the agent.
    /// Same payload as the `request` field of the polling response.
    Request {
        #[serde(flatten)]
        request: ShellAgentShellRequest,
    },
    /// Agent -> server. Result of a synchronous shell/file request. Same
    /// payload as `POST /api/shell/agent/result`.
    Result {
        #[serde(flatten)]
        payload: ShellAgentResultPayload,
    },
    /// Agent -> server. Incremental or final update for an async job. Same
    /// payload as `POST /api/shell/agent/job_update`.
    JobUpdate {
        #[serde(flatten)]
        payload: ShellAgentJobUpdateRequest,
    },
    /// Agent -> server result for a persistent-shell lifecycle request.
    PersistentShellResult {
        #[serde(flatten)]
        payload: ShellAgentPersistentShellResultRequest,
    },
    /// Either direction. Liveness keepalive.
    Ping { ts: i64 },
    /// Agent -> server changed-only sanitized runtime metadata. It reuses the
    /// active transport and never requires an acknowledgement round trip.
    RuntimeMetadata { tool_providers: ToolProvidersStatus },
    /// Agent -> Server bounded page of one project-inventory snapshot. New
    /// Runners send this only after the Registered view proved support.
    ProjectInventoryPage {
        #[serde(flatten)]
        page: ShellProjectInventoryPage,
    },
    /// Server -> Agent status acknowledgement for a project inventory page.
    ProjectInventoryStatus { status: ShellProjectInventoryStatus },
    /// Either direction. Reply to `Ping`.
    Pong { ts: i64 },
    /// Agent -> server. Best-effort graceful shutdown notice. Older agents do
    /// not send this frame; transports still reconcile on observed disconnect.
    Goodbye {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Server -> agent. Fatal protocol error; the agent should reconnect.
    Error { code: String, message: String },
}

impl AgentEnvelope {
    /// Short discriminator string for a variant, e.g. `"register"`. Useful
    /// for logging and tests.
    pub fn kind(&self) -> &'static str {
        match self {
            AgentEnvelope::Register { .. } => "register",
            AgentEnvelope::Registered { .. } => "registered",
            AgentEnvelope::Request { .. } => "request",
            AgentEnvelope::Result { .. } => "result",
            AgentEnvelope::JobUpdate { .. } => "job_update",
            AgentEnvelope::PersistentShellResult { .. } => "persistent_shell_result",
            AgentEnvelope::Ping { .. } => "ping",
            AgentEnvelope::RuntimeMetadata { .. } => "runtime_metadata",
            AgentEnvelope::ProjectInventoryPage { .. } => "project_inventory_page",
            AgentEnvelope::ProjectInventoryStatus { .. } => "project_inventory_status",
            AgentEnvelope::Pong { .. } => "pong",
            AgentEnvelope::Goodbye { .. } => "goodbye",
            AgentEnvelope::Error { .. } => "error",
        }
    }

    /// Encode the envelope as a JSON string.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Decode an envelope from a JSON byte slice.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

/// QUIC-v1 transport registration wire. Authentication remains transport-owned
/// while retaining the byte-compatible first-frame JSON shape expected by
/// rolling-old Servers and Runners: `type=register`, flattened registration
/// payload fields, and an optional `auth_token`.
///
/// Deliberately does not implement `Debug`: the credential must not become
/// printable through routine transport diagnostics.
#[derive(Serialize, Deserialize)]
pub struct QuicRegisterFrame {
    #[serde(rename = "type")]
    frame_type: QuicRegisterFrameType,
    #[serde(flatten)]
    payload: ShellClientRegisterRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth_token: Option<String>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
enum QuicRegisterFrameType {
    #[serde(rename = "register")]
    Register,
}

impl QuicRegisterFrame {
    pub fn new(payload: ShellClientRegisterRequest, auth_token: Option<String>) -> Self {
        Self {
            frame_type: QuicRegisterFrameType::Register,
            payload,
            auth_token,
        }
    }

    pub fn payload_mut(&mut self) -> &mut ShellClientRegisterRequest {
        &mut self.payload
    }

    pub fn into_parts(self) -> (ShellClientRegisterRequest, Option<String>) {
        (self.payload, self.auth_token)
    }
}

// ============================================================================
// QUIC length-prefixed frame codec
// ============================================================================
//
// The custom QUIC agent transport frames each [`AgentEnvelope`] as:
//
//   u32_be length (big-endian)
//   JSON bytes
//
// Length-prefixing (rather than newline-delimited JSON) avoids boundary
// problems when a payload contains embedded newlines. The codec lives in this
// shared module so the server (`agent_quic.rs`) and the `webcodex-runner`
// binary (which inlines this file) use byte-identical framing.
//
// This is a custom QUIC *stream* transport, NOT HTTP/3. It is transport-
// neutral framing over a single QUIC bidirectional stream.

/// Maximum frame body size. Matches the WebSocket `WS_MAX_MESSAGE_SIZE` head
/// room and the registry output cap; bounds memory per peer.
pub const QUIC_FRAME_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Errors produced by the QUIC frame codec.
#[derive(Debug)]
pub enum QuicFrameError {
    /// Underlying I/O error reading/writing the stream.
    Io(std::io::Error),
    /// JSON encode/decode failure.
    Json(serde_json::Error),
    /// Announced frame length exceeds `QUIC_FRAME_MAX_BYTES`. `len` is the
    /// announced (attacker-controlled) length; rejected before allocation.
    Oversized { len: usize, max: usize },
    /// The peer closed the stream cleanly before any frame was read.
    EmptyStream,
    /// A frame header announced a length but the body was short / invalid.
    Malformed(&'static str),
}

impl std::fmt::Display for QuicFrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuicFrameError::Io(e) => write!(f, "quic frame io error: {}", e),
            QuicFrameError::Json(e) => write!(f, "quic frame json error: {}", e),
            QuicFrameError::Oversized { len, max } => write!(
                f,
                "quic frame oversized: announced {} bytes, max {}",
                len, max
            ),
            QuicFrameError::EmptyStream => write!(f, "quic stream closed before any frame"),
            QuicFrameError::Malformed(msg) => write!(f, "quic frame malformed: {}", msg),
        }
    }
}

impl std::error::Error for QuicFrameError {}

fn encode_quic_json<T: Serialize>(value: &T) -> Result<Vec<u8>, QuicFrameError> {
    let json = serde_json::to_vec(value).map_err(QuicFrameError::Json)?;
    // u32 cap is far above QUIC_FRAME_MAX_BYTES, but guard anyway so a
    // pathological payload can never overflow the length prefix.
    if json.len() > QUIC_FRAME_MAX_BYTES {
        return Err(QuicFrameError::Oversized {
            len: json.len(),
            max: QUIC_FRAME_MAX_BYTES,
        });
    }
    let len = u32::try_from(json.len()).expect("checked against MAX");
    let mut out = Vec::with_capacity(4 + json.len());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&json);
    Ok(out)
}

/// Encode an envelope as a length-prefixed frame: `u32_be(len) || json`.
pub fn encode_quic_frame(env: &AgentEnvelope) -> Result<Vec<u8>, QuicFrameError> {
    encode_quic_json(env)
}

/// Encode the QUIC-v1 transport-owned register frame without routing its
/// credential through [`AgentEnvelope`].
pub fn encode_quic_register_frame(frame: &QuicRegisterFrame) -> Result<Vec<u8>, QuicFrameError> {
    encode_quic_json(frame)
}

/// Write a single length-prefixed frame to an async sink.
pub async fn write_quic_frame<W>(w: &mut W, env: &AgentEnvelope) -> Result<(), QuicFrameError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let buf = encode_quic_frame(env)?;
    w.write_all(&buf).await.map_err(QuicFrameError::Io)?;
    Ok(())
}

/// Write the transport-owned QUIC-v1 registration frame.
pub async fn write_quic_register_frame<W>(
    w: &mut W,
    frame: &QuicRegisterFrame,
) -> Result<(), QuicFrameError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let buf = encode_quic_register_frame(frame)?;
    w.write_all(&buf).await.map_err(QuicFrameError::Io)?;
    Ok(())
}

async fn read_quic_frame_body<R>(r: &mut R) -> Result<Vec<u8>, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut len_buf = [0u8; 4];
    match r.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(QuicFrameError::EmptyStream);
        }
        Err(e) => return Err(QuicFrameError::Io(e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > QUIC_FRAME_MAX_BYTES {
        // Reject before allocating. `len` is peer-controlled.
        return Err(QuicFrameError::Oversized {
            len,
            max: QUIC_FRAME_MAX_BYTES,
        });
    }
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            QuicFrameError::Malformed("announced frame length but stream ended early")
        } else {
            QuicFrameError::Io(e)
        }
    })?;
    Ok(buf)
}

/// Read a single length-prefixed shared application envelope.
pub async fn read_quic_frame<R>(r: &mut R) -> Result<AgentEnvelope, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let buf = read_quic_frame_body(r).await?;
    AgentEnvelope::from_slice(&buf).map_err(QuicFrameError::Json)
}

/// Read the transport-owned first QUIC-v1 registration frame. A structurally
/// valid registration with a missing token decodes successfully so the Server
/// can preserve the existing external `unauthorized` behavior.
pub async fn read_quic_register_frame<R>(r: &mut R) -> Result<QuicRegisterFrame, QuicFrameError>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let buf = read_quic_frame_body(r).await?;
    serde_json::from_slice(&buf).map_err(QuicFrameError::Json)
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    #[test]
    fn legacy_agent_protocol_labels_normalize_into_one_protocol_generation() {
        for version in [
            AGENT_PROTOCOL_VERSION_POLLING_V1,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
            AGENT_PROTOCOL_VERSION_QUIC_V1,
        ] {
            let semantics = normalize_agent_protocol_semantics(version);
            assert_eq!(semantics.compatibility, AgentProtocolCompatibility::V1);
            assert!(semantics.compatibility.reports_project_git_metadata());
            assert_eq!(
                semantics.project_inventory,
                AgentProjectInventoryStrategy::Inline
            );
        }
        for version in [
            AGENT_PROTOCOL_VERSION_POLLING_V2,
            AGENT_PROTOCOL_VERSION_WEBSOCKET_V2,
            AGENT_PROTOCOL_VERSION_QUIC_V2,
        ] {
            let semantics = normalize_agent_protocol_semantics(version);
            assert_eq!(semantics.compatibility, AgentProtocolCompatibility::V1);
            assert!(semantics.compatibility.reports_project_git_metadata());
            assert_eq!(
                semantics.project_inventory,
                AgentProjectInventoryStrategy::Paged
            );
        }

        for version in [
            "future-v2",
            "websocket-next",
            "quic-next",
            "polling-v3",
            "totally-random",
        ] {
            let unknown = normalize_agent_protocol_semantics(version);
            assert_eq!(
                unknown.compatibility,
                AgentProtocolCompatibility::Unsupported,
                "{version}"
            );
            assert!(!unknown.compatibility.reports_project_git_metadata());
            assert_eq!(
                unknown.project_inventory,
                AgentProjectInventoryStrategy::Inline,
                "unknown labels must not opt into paged inventory: {version}"
            );
        }
    }

    fn sample_process_request() -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: "req-process-1".to_string(),
            client_id: "ws-1".to_string(),
            kind: "run_process".to_string(),
            job_id: None,
            cwd: Some("sub dir".to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: Some(ShellProcessArgv {
                executable: "argv-helper".to_string(),
                args: vec![
                    "literal space".to_string(),
                    "\"quote\"".to_string(),
                    "$(not-shell)".to_string(),
                ],
            }),
            script: None,
            stdin: Some("bounded input".to_string()),
            timeout_secs: 60,
            requested_by: "tester".to_string(),
            created_at: 123,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            mcp_gateway: None,
            coding_agent: None,
            persistent_shell: None,
        }
    }

    fn sample_process_job_request() -> ShellAgentShellRequest {
        let mut request = sample_process_request();
        request.kind = "start_process_job".to_string();
        request.job_id = Some("job-process-1".to_string());
        request.job_context = Some(ShellJobContext {
            runtime_project_id: None,
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some("sub dir".to_string()),
            cwd: Some("sub dir".to_string()),
            purpose: Some("test".to_string()),
            shell: Some("direct_argv".to_string()),
            command_preview: "argv-helper literal space …".to_string(),
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: Some(ShellJobStructuredExecutionMetadata {
                execution_source: "run_process".to_string(),
                language: None,
                script_bytes: None,
                arg_count: 3,
                stdin_present: true,
                validation_identity: None,
                validation_tool: None,
                assertion_name: None,
            }),
        });
        request
    }

    fn sample_script_request() -> ShellAgentShellRequest {
        ShellAgentShellRequest {
            request_id: "req-script-1".to_string(),
            client_id: "ws-1".to_string(),
            kind: "run_script".to_string(),
            job_id: None,
            cwd: Some("sub dir".to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: String::new(),
            process: None,
            script: Some(ShellScriptPayload {
                language: ShellScriptLanguage::Bash,
                script: "printf '%s\\n' \"$1\"".to_string(),
                args: vec!["two words".to_string(), "$(literal)".to_string()],
            }),
            stdin: Some("bounded input".to_string()),
            timeout_secs: 60,
            requested_by: "tester".to_string(),
            created_at: 123,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            mcp_gateway: None,
            coding_agent: None,
            persistent_shell: None,
        }
    }

    fn sample_script_job_request() -> ShellAgentShellRequest {
        let mut request = sample_script_request();
        request.kind = "start_script_job".to_string();
        request.job_id = Some("job-script-1".to_string());
        request.job_context = Some(ShellJobContext {
            runtime_project_id: None,
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some("sub dir".to_string()),
            cwd: Some("sub dir".to_string()),
            purpose: Some("test".to_string()),
            shell: Some("bash".to_string()),
            command_preview: "bash script (18 bytes, 2 args)".to_string(),
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: Some(ShellJobStructuredExecutionMetadata {
                execution_source: "run_script".to_string(),
                language: Some(ShellScriptLanguage::Bash),
                script_bytes: Some(18),
                arg_count: 2,
                stdin_present: true,
                validation_identity: None,
                validation_tool: None,
                assertion_name: None,
            }),
        });
        request
    }

    #[test]
    fn agent_host_context_is_closed_normalized_and_bounded() {
        let context = AgentHostContext {
            role: Some(" server_host ".to_string()),
            runtime: Some(" Prefer the local Runner for host operations. ".to_string()),
            service: None,
            network: None,
            architecture: None,
        }
        .normalized()
        .unwrap();
        assert_eq!(context.role.as_deref(), Some("server_host"));
        assert_eq!(
            context.runtime.as_deref(),
            Some("Prefer the local Runner for host operations.")
        );

        let unknown = serde_json::from_value::<AgentHostContext>(serde_json::json!({
            "role": "server_host",
            "arbitrary": "not an extension point"
        }));
        assert!(unknown.is_err());
        assert!(AgentHostContext {
            role: Some("Server Host".to_string()),
            ..Default::default()
        }
        .normalized()
        .unwrap_err()
        .contains("host_context.role"));
        assert!(AgentHostContext {
            runtime: Some("x".repeat(AGENT_HOST_CONTEXT_TEXT_MAX_BYTES + 1)),
            ..Default::default()
        }
        .normalized()
        .unwrap_err()
        .contains("host_context.runtime"));
        assert!(AgentHostContext::default()
            .normalized()
            .unwrap_err()
            .contains("at least one field"));
    }

    fn sample_register() -> ShellClientRegisterRequest {
        ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            client_id: "ws-1".to_string(),
            agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
            display_name: Some("WS Agent".to_string()),
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                file_write: false,
                artifact_export_chunk_read: false,
                artifact_export_streaming_metadata: false,
                structured_file_delete: false,
                apply_text_edit_occurrence: false,
                git: false,
                jobs: true,
                async_jobs: true,
                async_shell_jobs: true,
                ssh_shell: true,
                persistent_shell: true,
                ssh_persistent_shell: true,
                structured_validation_argv: true,
                structured_cargo_test_count_assertion: true,
                structured_go_test_json: true,
                structured_go_test_tool: true,
                structured_go_test_packages: true,
                structured_process_argv: true,
                structured_script_payload: true,
                internal_posix_script: true,
                structured_execution_jobs: true,
                detached_process_jobs: true,
                lsp_read_only_navigation: false,
                lsp_call_hierarchy: false,
                sandbox_inspect_commands: false,
                project_lifecycle: false,
                project_path_registration: false,
                skill_store_read: false,
                skill_store_manage: false,
                computer_observe: false,
                computer_application_discovery: false,
                computer_application_launch: false,
                computer_display_observe: false,
                computer_pointer_control: false,
                computer_clipboard_read: false,
                computer_clipboard_write: false,
                computer_snapshot_region: false,
                computer_accessibility_observe: false,
                computer_element_state: false,
                computer_control: false,
                computer_scroll_to_element: false,
                computer_key_input: false,
                computer_window_activate: false,
                computer_text_input: false,
                job_state_reconciliation: false,
                coding_agent_runs: false,
                agent_protocol_generation: None,
            }),
            projects: None,
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1.to_string()),
            policy: None,
            job_concurrency_limit: Some(4),
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
        }
    }

    fn sample_tool_providers() -> ToolProvidersStatus {
        ToolProvidersStatus {
            strategy: "claude_code_then_native".to_string(),
            claude_code: ClaudeCodeProviderStatus {
                enabled: true,
                version: Some("2.1.217".to_string()),
                available: true,
                process_state: "running".to_string(),
                discovered_tool_names: vec!["Edit".to_string()],
                capabilities: BTreeMap::from([
                    ("edit_file".to_string(), "available".to_string()),
                    ("search_project_text".to_string(), "unmapped".to_string()),
                ]),
                last_error_code: None,
                last_call: None,
            },
            config_reload: AgentConfigReloadStatus::default(),
        }
    }

    #[test]
    fn register_envelope_round_trips_with_type_tag() {
        let env = AgentEnvelope::Register {
            payload: sample_register(),
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"register""#), "json was: {json}");
        assert!(json.contains(r#""client_id":"ws-1""#));
        assert!(!json.contains(r#""auth_token""#), "json was: {json}");
        let back = AgentEnvelope::from_slice(json.as_bytes()).unwrap();
        match back {
            AgentEnvelope::Register { payload, .. } => {
                assert_eq!(payload.client_id, "ws-1");
                assert_eq!(
                    payload.agent_protocol_version.as_deref(),
                    Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1),
                );
                assert_eq!(payload.job_concurrency_limit, Some(4));
                let caps = payload.capabilities.expect("capabilities");
                assert!(caps.shell);
                assert!(!caps.file_write);
                assert!(caps.persistent_shell);
            }
            other => panic!("expected register, got {:?}", other.kind()),
        }
    }

    #[test]
    fn additive_protocol_generation_is_ignored_by_frozen_pre_c4_envelope_shape() {
        #[allow(dead_code)]
        #[derive(Deserialize)]
        struct LatestPreC4RegisterRequest {
            client_id: String,
            agent_instance_id: String,
            #[serde(default)]
            display_name: Option<String>,
            #[serde(default)]
            owner: Option<String>,
            #[serde(default)]
            hostname: Option<String>,
            #[serde(default)]
            capabilities: Option<serde_json::Value>,
            #[serde(default)]
            host_context: Option<AgentHostContext>,
            #[serde(default)]
            projects: Option<Vec<ShellAgentProjectSummary>>,
            #[serde(default)]
            agent_protocol_version: Option<String>,
            #[serde(default)]
            policy: Option<AgentPolicySummary>,
            #[serde(default)]
            process_started_at: Option<i64>,
            #[serde(default)]
            build: Option<AgentBuildInfo>,
            #[serde(default)]
            job_concurrency_limit: Option<usize>,
            #[serde(default)]
            job_inventory: Option<ShellJobInventory>,
            #[serde(default)]
            coding_agent_providers: Option<Vec<crate::coding_agent::CodingAgentProvider>>,
            #[serde(default)]
            coding_agent_inventory: Option<crate::coding_agent::CodingAgentRunInventory>,
        }

        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum LatestPreC4Envelope {
            Register {
                #[serde(flatten)]
                payload: LatestPreC4RegisterRequest,
                #[serde(default)]
                auth_token: Option<String>,
            },
        }

        let mut payload = sample_register();
        payload
            .capabilities
            .as_mut()
            .unwrap()
            .agent_protocol_generation = Some(AGENT_PROTOCOL_GENERATION_V2);
        let json = AgentEnvelope::Register { payload }.to_json().unwrap();
        assert!(json.contains(r#""agent_protocol_generation":2"#));
        match serde_json::from_str::<LatestPreC4Envelope>(&json).unwrap() {
            LatestPreC4Envelope::Register {
                payload,
                auth_token,
            } => {
                assert_eq!(payload.client_id, "ws-1");
                assert_eq!(
                    payload.agent_protocol_version.as_deref(),
                    Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1)
                );
                assert_eq!(payload.capabilities.unwrap()["file_read"], true);
                assert!(auth_token.is_none());
            }
        }
    }

    #[test]
    fn raw_protocol_generation_number_preserves_future_wire_values_for_ingress_rejection() {
        let mut payload = sample_register();
        payload
            .capabilities
            .as_mut()
            .unwrap()
            .agent_protocol_generation = Some(AgentProtocolGenerationNumber::new(u16::MAX));
        let json = serde_json::to_string(&payload).unwrap();
        let decoded: ShellClientRegisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            decoded
                .capabilities
                .unwrap()
                .agent_protocol_generation
                .unwrap()
                .get(),
            u16::MAX
        );
    }

    #[test]
    fn legacy_capabilities_default_persistent_shell_to_false() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"shell":true,"jobs":true}"#).unwrap();
        assert!(capabilities.shell);
        assert!(capabilities.jobs);
        assert!(!capabilities.persistent_shell);
        // SSH persistent shells are never implied by ssh_shell or persistent_shell;
        // a legacy/older runner that omits the field fails closed.
        assert!(!capabilities.ssh_shell);
        assert!(!capabilities.ssh_persistent_shell);
        assert!(!capabilities.structured_execution_jobs);
        assert!(!capabilities.project_path_registration);
        assert!(!capabilities.structured_file_delete);
        assert!(!capabilities.artifact_export_streaming_metadata);
        assert!(!capabilities.computer_observe);
        assert!(!capabilities.computer_application_discovery);
        assert!(!capabilities.computer_application_launch);
        assert!(!capabilities.computer_accessibility_observe);
        assert!(!capabilities.computer_element_state);
        assert!(!capabilities.computer_control);
        assert!(!capabilities.computer_key_input);
        assert!(!capabilities.computer_window_activate);
        assert!(!ShellClientCapabilities::default().ssh_persistent_shell);
        assert!(!capabilities.computer_text_input);
        assert!(!ShellClientCapabilities::default().project_path_registration);
        assert!(!ShellClientCapabilities::default().computer_observe);
        assert!(!ShellClientCapabilities::default().computer_application_discovery);
        assert!(!ShellClientCapabilities::default().computer_application_launch);
        assert!(!ShellClientCapabilities::default().computer_accessibility_observe);
        assert!(!ShellClientCapabilities::default().computer_element_state);
        assert!(!ShellClientCapabilities::default().computer_control);
        assert!(!ShellClientCapabilities::default().computer_key_input);
        assert!(!ShellClientCapabilities::default().computer_window_activate);
        assert!(!ShellClientCapabilities::default().computer_text_input);
        assert!(!ShellClientCapabilities::default().artifact_export_streaming_metadata);
    }

    #[test]
    fn artifact_export_streaming_metadata_capability_deserializes_only_when_present() {
        let missing: ShellClientCapabilities =
            serde_json::from_str(r#"{"artifact_export_chunk_read":true}"#).unwrap();
        assert!(missing.artifact_export_chunk_read);
        assert!(!missing.artifact_export_streaming_metadata);
        let present: ShellClientCapabilities = serde_json::from_str(
            r#"{"artifact_export_chunk_read":true,"artifact_export_streaming_metadata":true}"#,
        )
        .unwrap();
        assert!(present.artifact_export_streaming_metadata);
    }

    #[test]
    fn structured_file_delete_capability_deserializes_only_when_present() {
        let missing: ShellClientCapabilities = serde_json::from_str(r#"{"shell":true}"#).unwrap();
        assert!(!missing.structured_file_delete);
        let present: ShellClientCapabilities =
            serde_json::from_str(r#"{"structured_file_delete":true}"#).unwrap();
        assert!(present.structured_file_delete);
    }

    #[test]
    fn project_path_registration_capability_deserializes_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"project_path_registration":true}"#).unwrap();
        assert!(capabilities.project_path_registration);
    }

    #[test]
    fn computer_observe_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_observe":true}"#).unwrap();
        assert!(capabilities.computer_observe);
        assert!(!capabilities.file_read);
    }

    #[test]
    fn computer_application_capabilities_are_additive_and_default_false() {
        let legacy: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_observe":true,"computer_control":true}"#).unwrap();
        assert!(legacy.computer_observe);
        assert!(legacy.computer_control);
        assert!(!legacy.computer_application_discovery);
        assert!(!legacy.computer_application_launch);
        assert!(!legacy.computer_display_observe);
        assert!(!legacy.computer_pointer_control);
        assert!(!legacy.computer_clipboard_read);
        assert!(!legacy.computer_clipboard_write);

        let discovery: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_application_discovery":true}"#).unwrap();
        assert!(discovery.computer_application_discovery);
        assert!(!discovery.computer_application_launch);
        assert!(!discovery.computer_observe);
        assert!(!discovery.computer_control);
        assert!(!discovery.computer_display_observe);
        assert!(!discovery.computer_pointer_control);

        let launch: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_application_launch":true}"#).unwrap();
        assert!(launch.computer_application_launch);
        assert!(!launch.computer_application_discovery);
        assert!(!launch.computer_observe);
        assert!(!launch.computer_control);
        assert!(!launch.computer_display_observe);
        assert!(!launch.computer_pointer_control);

        let display: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_display_observe":true}"#).unwrap();
        assert!(display.computer_display_observe);
        assert!(!display.computer_observe);
        assert!(!display.computer_snapshot_region);
        assert!(!display.computer_application_discovery);
        assert!(!display.computer_application_launch);
        assert!(!display.computer_pointer_control);

        let pointer: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_pointer_control":true}"#).unwrap();
        assert!(pointer.computer_pointer_control);
        assert!(!pointer.computer_control);
        assert!(!pointer.computer_display_observe);
        assert!(!pointer.computer_observe);
        assert!(!pointer.computer_clipboard_read);
        assert!(!pointer.computer_clipboard_write);

        let clipboard_read: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_clipboard_read":true}"#).unwrap();
        assert!(clipboard_read.computer_clipboard_read);
        assert!(!clipboard_read.computer_clipboard_write);
        assert!(!clipboard_read.computer_control);
        assert!(!clipboard_read.computer_observe);
        assert!(!clipboard_read.computer_pointer_control);

        let clipboard_write: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_clipboard_write":true}"#).unwrap();
        assert!(clipboard_write.computer_clipboard_write);
        assert!(!clipboard_write.computer_clipboard_read);
        assert!(!clipboard_write.computer_control);
        assert!(!clipboard_write.computer_display_observe);

        assert!(SHELL_CLIENT_CAPABILITY_NAMES
            .contains(&SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_DISCOVERY));
        assert!(SHELL_CLIENT_CAPABILITY_NAMES
            .contains(&SHELL_CLIENT_CAPABILITY_COMPUTER_APPLICATION_LAUNCH));
        assert!(SHELL_CLIENT_CAPABILITY_NAMES
            .contains(&SHELL_CLIENT_CAPABILITY_COMPUTER_DISPLAY_OBSERVE));
        assert!(SHELL_CLIENT_CAPABILITY_NAMES
            .contains(&SHELL_CLIENT_CAPABILITY_COMPUTER_POINTER_CONTROL));
        assert!(SHELL_CLIENT_CAPABILITY_NAMES
            .contains(&SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_READ));
        assert!(SHELL_CLIENT_CAPABILITY_NAMES
            .contains(&SHELL_CLIENT_CAPABILITY_COMPUTER_CLIPBOARD_WRITE));
    }

    #[test]
    fn computer_snapshot_region_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_observe":true}"#).unwrap();
        assert!(capabilities.computer_observe);
        assert!(!capabilities.computer_snapshot_region);

        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_snapshot_region":true}"#).unwrap();
        assert!(capabilities.computer_snapshot_region);
        assert!(!capabilities.computer_observe);
        assert!(!capabilities.computer_accessibility_observe);
    }

    #[test]
    fn computer_accessibility_observe_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_accessibility_observe":true}"#).unwrap();
        assert!(capabilities.computer_accessibility_observe);
        assert!(!capabilities.computer_observe);
    }

    #[test]
    fn computer_element_state_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_accessibility_observe":true}"#).unwrap();
        assert!(capabilities.computer_accessibility_observe);
        assert!(!capabilities.computer_element_state);

        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_element_state":true}"#).unwrap();
        assert!(capabilities.computer_element_state);
        assert!(!capabilities.computer_accessibility_observe);
        assert!(!capabilities.computer_control);
    }

    #[test]
    fn computer_control_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_control":true}"#).unwrap();
        assert!(capabilities.computer_control);
        assert!(!capabilities.computer_observe);
        assert!(!capabilities.computer_accessibility_observe);
    }

    #[test]
    fn computer_scroll_to_element_capability_is_distinct_from_control() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_control":true}"#).unwrap();
        assert!(capabilities.computer_control);
        assert!(!capabilities.computer_scroll_to_element);

        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_scroll_to_element":true}"#).unwrap();
        assert!(capabilities.computer_scroll_to_element);
        assert!(!capabilities.computer_control);
        assert!(!capabilities.computer_observe);
    }

    #[test]
    fn computer_key_input_capability_is_distinct_from_control() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_control":true}"#).unwrap();
        assert!(capabilities.computer_control);
        assert!(!capabilities.computer_key_input);

        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_key_input":true}"#).unwrap();
        assert!(capabilities.computer_key_input);
        assert!(!capabilities.computer_control);
        assert!(!capabilities.computer_observe);
    }

    #[test]
    fn computer_window_activate_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_control":true}"#).unwrap();
        assert!(capabilities.computer_control);
        assert!(!capabilities.computer_window_activate);

        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_window_activate":true}"#).unwrap();
        assert!(capabilities.computer_window_activate);
        assert!(!capabilities.computer_control);
        assert!(!capabilities.computer_observe);
    }

    #[test]
    fn computer_text_input_capability_deserializes_only_when_present() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_control":true}"#).unwrap();
        assert!(capabilities.computer_control);
        assert!(!capabilities.computer_text_input);

        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"computer_text_input":true}"#).unwrap();
        assert!(capabilities.computer_text_input);
        assert!(!capabilities.computer_control);
        assert!(!capabilities.computer_accessibility_observe);
    }

    fn reconciliation_inventory() -> ShellJobInventory {
        ShellJobInventory {
            active_complete: true,
            jobs: vec![ShellJobSnapshot {
                job_id: "job-original".to_string(),
                request_id: "request-original".to_string(),
                status: "running".to_string(),
                update_seq: 9,
                created_at: 1_700_000_000,
                started_at: Some(1_700_000_001),
                ended_at: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                command_execution_state: None,
                context: ShellJobContext {
                    runtime_project_id: Some("agent:oe:demo".to_string()),
                    workflow_session_id: Some("wc_sess_reconcile".to_string()),
                    ssh_resource: None,
                    project_cwd: Some("/srv/demo".to_string()),
                    cwd: Some("/srv/demo".to_string()),
                    purpose: Some("test".to_string()),
                    shell: Some("bash".to_string()),
                    command_preview: "cargo test focused".to_string(),
                    validation_steps: Vec::new(),
                    validation: None,
                    structured_execution: None,
                },
                stdout: ShellJobStreamSnapshot {
                    tail: "one\n".to_string(),
                    first_retained_line: 1,
                    next_line: 2,
                    truncated: false,
                },
                stderr: ShellJobStreamSnapshot::default(),
                validation_progress: None,
            }],
        }
    }

    #[tokio::test]
    async fn job_reconciliation_inventory_round_trips_across_all_register_transports() {
        let inventory = reconciliation_inventory();
        let mut polling = sample_register();
        polling.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_POLLING_V1.to_string());
        polling
            .capabilities
            .as_mut()
            .unwrap()
            .job_state_reconciliation = true;
        polling.job_inventory = Some(inventory.clone());
        let polling_json = serde_json::to_vec(&polling).unwrap();
        let polling_back: ShellClientRegisterRequest =
            serde_json::from_slice(&polling_json).unwrap();
        assert_eq!(polling_back.job_inventory.as_ref(), Some(&inventory));
        assert_eq!(polling_back.job_concurrency_limit, Some(4));

        let mut websocket = polling.clone();
        websocket.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_WEBSOCKET_V1.to_string());
        let websocket_json = AgentEnvelope::Register { payload: websocket }
            .to_json()
            .unwrap();
        let websocket_back = AgentEnvelope::from_slice(websocket_json.as_bytes()).unwrap();
        match websocket_back {
            AgentEnvelope::Register { payload, .. } => {
                assert_eq!(payload.job_inventory.as_ref(), Some(&inventory));
                assert_eq!(payload.job_concurrency_limit, Some(4));
            }
            other => panic!("expected websocket register, got {:?}", other.kind()),
        }

        let mut quic = polling;
        quic.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string());
        let quic_frame = encode_quic_register_frame(&QuicRegisterFrame::new(
            quic,
            Some("test-only-token".to_string()),
        ))
        .unwrap();
        let mut quic_reader = quic_frame.as_slice();
        let (payload, auth_token) = read_quic_register_frame(&mut quic_reader)
            .await
            .unwrap()
            .into_parts();
        assert_eq!(payload.job_inventory.as_ref(), Some(&inventory));
        assert_eq!(payload.job_concurrency_limit, Some(4));
        assert_eq!(auth_token.as_deref(), Some("test-only-token"));
    }

    #[test]
    fn request_envelope_flattens_shell_request_fields() {
        let request = ShellAgentShellRequest {
            request_id: "req-1".to_string(),
            client_id: "ws-1".to_string(),
            kind: "run_shell".to_string(),
            job_id: None,
            cwd: Some("/tmp".to_string()),
            path: None,
            content: None,
            max_bytes: None,
            expected_sha256: None,
            expected_prefix: None,
            start_line: None,
            end_line: None,
            create_dirs: false,
            command: "echo hi".to_string(),
            process: None,
            script: None,
            stdin: Some("input".to_string()),
            timeout_secs: 10,
            requested_by: "tester".to_string(),
            created_at: 123,
            validation: None,
            lsp: None,
            sandbox: None,
            job_context: None,
            mcp_gateway: None,
            coding_agent: None,
            persistent_shell: None,
        };
        let env = AgentEnvelope::Request { request };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"request""#));
        assert!(json.contains(r#""request_id":"req-1""#));
        assert!(json.contains(r#""kind":"run_shell""#));
        assert!(json.contains(r#""command":"echo hi""#));
        assert!(json.contains(r#""stdin":"input""#));
        let back = AgentEnvelope::from_slice(json.as_bytes()).unwrap();
        match back {
            AgentEnvelope::Request { request } => {
                assert_eq!(request.request_id, "req-1");
                assert_eq!(request.command, "echo hi");
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }
    }

    #[tokio::test]
    async fn structured_process_request_round_trips_polling_websocket_and_quic() {
        let request = sample_process_request();
        let polling: ShellAgentPollResponse = serde_json::from_value(
            serde_json::to_value(ShellAgentPollResponse {
                success: true,
                request: Some(request.clone()),
                error: None,
                project_inventory: None,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(polling.request.as_ref().unwrap().process, request.process);
        assert_eq!(polling.request.as_ref().unwrap().command, "");
        assert_eq!(polling.request.as_ref().unwrap().kind, "run_process");

        let websocket_json = AgentEnvelope::Request {
            request: request.clone(),
        }
        .to_json()
        .unwrap();
        match AgentEnvelope::from_slice(websocket_json.as_bytes()).unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.process, request.process);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "run_process");
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }

        let frame = encode_quic_frame(&AgentEnvelope::Request {
            request: request.clone(),
        })
        .unwrap();
        let mut reader = frame.as_slice();
        match read_quic_frame(&mut reader).await.unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.process, request.process);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "run_process");
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }
    }

    #[tokio::test]
    async fn structured_process_job_request_round_trips_polling_websocket_and_quic() {
        let request = sample_process_job_request();
        let polling: ShellAgentPollResponse = serde_json::from_value(
            serde_json::to_value(ShellAgentPollResponse {
                success: true,
                request: Some(request.clone()),
                error: None,
                project_inventory: None,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(polling.request.as_ref().unwrap().process, request.process);
        assert_eq!(polling.request.as_ref().unwrap().command, "");
        assert_eq!(polling.request.as_ref().unwrap().kind, "start_process_job");
        assert!(polling.request.as_ref().unwrap().script.is_none());

        let websocket_json = AgentEnvelope::Request {
            request: request.clone(),
        }
        .to_json()
        .unwrap();
        match AgentEnvelope::from_slice(websocket_json.as_bytes()).unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.process, request.process);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "start_process_job");
                assert!(decoded.script.is_none());
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }

        let frame = encode_quic_frame(&AgentEnvelope::Request {
            request: request.clone(),
        })
        .unwrap();
        let mut reader = frame.as_slice();
        match read_quic_frame(&mut reader).await.unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.process, request.process);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "start_process_job");
                assert!(decoded.script.is_none());
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }
    }

    #[tokio::test]
    async fn structured_script_request_round_trips_polling_websocket_and_quic() {
        let request = sample_script_request();
        let polling: ShellAgentPollResponse = serde_json::from_value(
            serde_json::to_value(ShellAgentPollResponse {
                success: true,
                request: Some(request.clone()),
                error: None,
                project_inventory: None,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(polling.request.as_ref().unwrap().script, request.script);
        assert_eq!(polling.request.as_ref().unwrap().command, "");
        assert_eq!(polling.request.as_ref().unwrap().kind, "run_script");
        assert!(polling.request.as_ref().unwrap().process.is_none());

        let websocket_json = AgentEnvelope::Request {
            request: request.clone(),
        }
        .to_json()
        .unwrap();
        assert!(!websocket_json.contains(r#""command":"printf"#));
        match AgentEnvelope::from_slice(websocket_json.as_bytes()).unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.script, request.script);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "run_script");
                assert!(decoded.process.is_none());
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }

        let frame = encode_quic_frame(&AgentEnvelope::Request {
            request: request.clone(),
        })
        .unwrap();
        let mut reader = frame.as_slice();
        match read_quic_frame(&mut reader).await.unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.script, request.script);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "run_script");
                assert!(decoded.process.is_none());
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }
    }

    #[tokio::test]
    async fn structured_script_job_request_round_trips_polling_websocket_and_quic() {
        let request = sample_script_job_request();
        let polling: ShellAgentPollResponse = serde_json::from_value(
            serde_json::to_value(ShellAgentPollResponse {
                success: true,
                request: Some(request.clone()),
                error: None,
                project_inventory: None,
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(polling.request.as_ref().unwrap().script, request.script);
        assert_eq!(polling.request.as_ref().unwrap().command, "");
        assert_eq!(polling.request.as_ref().unwrap().kind, "start_script_job");
        assert!(polling.request.as_ref().unwrap().process.is_none());

        let websocket_json = AgentEnvelope::Request {
            request: request.clone(),
        }
        .to_json()
        .unwrap();
        assert!(!websocket_json.contains(r#""command":"printf"#));
        match AgentEnvelope::from_slice(websocket_json.as_bytes()).unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.script, request.script);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "start_script_job");
                assert!(decoded.process.is_none());
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }

        let frame = encode_quic_frame(&AgentEnvelope::Request {
            request: request.clone(),
        })
        .unwrap();
        let mut reader = frame.as_slice();
        match read_quic_frame(&mut reader).await.unwrap() {
            AgentEnvelope::Request { request: decoded } => {
                assert_eq!(decoded.script, request.script);
                assert_eq!(decoded.command, "");
                assert_eq!(decoded.kind, "start_script_job");
                assert!(decoded.process.is_none());
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }
    }

    #[test]
    fn b2_capabilities_do_not_imply_structured_execution_jobs() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(
                r#"{"shell":true,"async_jobs":true,"structured_validation_argv":true,"structured_process_argv":true,"structured_script_payload":true}"#,
            )
            .unwrap();
        assert!(capabilities.structured_validation_argv);
        assert!(!capabilities.structured_go_test_json);
        assert!(capabilities.structured_process_argv);
        assert!(capabilities.structured_script_payload);
        assert!(capabilities.async_jobs);
        assert!(!capabilities.structured_execution_jobs);
        assert!(!ShellClientCapabilities::default().structured_script_payload);
        assert!(!ShellClientCapabilities::default().structured_execution_jobs);
    }

    #[test]
    fn legacy_capabilities_do_not_imply_structured_process_argv() {
        let capabilities: ShellClientCapabilities =
            serde_json::from_str(r#"{"shell":true,"structured_validation_argv":true}"#).unwrap();
        assert!(capabilities.structured_validation_argv);
        assert!(!capabilities.structured_go_test_json);
        assert!(!capabilities.structured_process_argv);
        assert!(!capabilities.structured_script_payload);
        assert!(!capabilities.structured_execution_jobs);
    }

    #[test]
    fn process_argv_validation_is_bounded_and_rejects_shell_command_modes() {
        let valid = ShellProcessArgv {
            executable: "tool".to_string(),
            args: vec!["a".repeat(8_000), "b".repeat(7_994)],
        };
        assert!(validate_process_argv(&valid).is_ok());

        let oversized = ShellProcessArgv {
            executable: "tool".to_string(),
            args: vec!["a".repeat(8_000), "b".repeat(7_995)],
        };
        assert!(validate_process_argv(&oversized)
            .unwrap_err()
            .contains("maximum total"));

        for invalid in [
            ShellProcessArgv {
                executable: "x".repeat(PROCESS_EXECUTABLE_MAX_BYTES + 1),
                args: Vec::new(),
            },
            ShellProcessArgv {
                executable: "bad\0program".to_string(),
                args: Vec::new(),
            },
            ShellProcessArgv {
                executable: "tool".to_string(),
                args: vec![String::new(); PROCESS_ARG_MAX_COUNT + 1],
            },
            ShellProcessArgv {
                executable: "tool".to_string(),
                args: vec!["x".repeat(PROCESS_ARG_MAX_BYTES + 1)],
            },
            ShellProcessArgv {
                executable: "tool".to_string(),
                args: vec!["bad\0arg".to_string()],
            },
        ] {
            assert!(validate_process_argv(&invalid).is_err());
        }

        for (executable, args) in [
            ("sh", vec!["-c".to_string(), "echo unsafe".to_string()]),
            (
                "bash.exe",
                vec!["-lc".to_string(), "echo unsafe".to_string()],
            ),
            (
                "powershell.exe",
                vec![
                    "-NoProfile".to_string(),
                    "-Command".to_string(),
                    "echo unsafe".to_string(),
                ],
            ),
            (
                "cmd.exe",
                vec![
                    "/D".to_string(),
                    "/C".to_string(),
                    "echo unsafe".to_string(),
                ],
            ),
        ] {
            assert!(validate_process_argv(&ShellProcessArgv {
                executable: executable.to_string(),
                args,
            })
            .unwrap_err()
            .contains("shell command mode"));
        }
    }

    #[test]
    fn script_request_validation_is_bounded_and_allows_whitespace_only_content() {
        let valid = ShellScriptPayload {
            language: ShellScriptLanguage::Sh,
            script: " \n".to_string(),
            args: vec!["a".repeat(8_000), "b".repeat(7_998)],
        };
        assert!(validate_script_request(
            &valid,
            Some(&"i".repeat(SCRIPT_STDIN_MAX_BYTES)),
            Some("."),
            SCRIPT_TIMEOUT_MAX_SECS,
        )
        .is_ok());

        let invalid_payloads = [
            ShellScriptPayload {
                language: ShellScriptLanguage::Sh,
                script: String::new(),
                args: Vec::new(),
            },
            ShellScriptPayload {
                language: ShellScriptLanguage::Bash,
                script: "x".repeat(SCRIPT_MAX_BYTES + 1),
                args: Vec::new(),
            },
            ShellScriptPayload {
                language: ShellScriptLanguage::Powershell,
                script: "bad\0script".to_string(),
                args: Vec::new(),
            },
            ShellScriptPayload {
                language: ShellScriptLanguage::Sh,
                script: "true".to_string(),
                args: vec![String::new(); SCRIPT_ARG_MAX_COUNT + 1],
            },
            ShellScriptPayload {
                language: ShellScriptLanguage::Sh,
                script: "true".to_string(),
                args: vec!["x".repeat(SCRIPT_ARG_MAX_BYTES + 1)],
            },
            ShellScriptPayload {
                language: ShellScriptLanguage::Sh,
                script: "true".to_string(),
                args: vec!["bad\0arg".to_string()],
            },
            ShellScriptPayload {
                language: ShellScriptLanguage::Sh,
                script: "true".to_string(),
                args: vec!["a".repeat(8_000), "b".repeat(8_000)],
            },
        ];
        for payload in invalid_payloads {
            assert!(validate_script_request(&payload, None, None, 60).is_err());
        }
        assert!(validate_script_request(
            &valid,
            Some(&"i".repeat(SCRIPT_STDIN_MAX_BYTES + 1)),
            None,
            60,
        )
        .is_err());
        assert!(validate_script_request(&valid, Some("bad\0stdin"), None, 60).is_err());
        assert!(validate_script_request(
            &valid,
            None,
            Some(&"c".repeat(SCRIPT_CWD_MAX_BYTES + 1)),
            60,
        )
        .is_err());
        assert!(validate_script_request(&valid, None, Some("bad\0cwd"), 60).is_err());
        assert!(validate_script_request(&valid, None, None, 0).is_err());
        assert!(validate_script_request(&valid, None, None, SCRIPT_TIMEOUT_MAX_SECS + 1).is_err());
    }

    #[test]
    fn cmd_is_not_a_script_language() {
        let error = serde_json::from_value::<ShellScriptLanguage>(serde_json::json!("cmd"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown variant"), "{error}");
    }

    #[test]
    fn persistent_shell_request_and_result_envelopes_round_trip() {
        let request: ShellAgentShellRequest = serde_json::from_value(serde_json::json!({
            "request_id": "req-shell-1",
            "client_id": "ws-1",
            "kind": "persistent_shell",
            "command": "",
            "timeout_secs": 35,
            "requested_by": "tester",
            "created_at": 123,
            "persistent_shell": {
                "action": "exec",
                "shell_id": "wc_shell_1234",
                "workflow_session_id": "wc_sess_1234",
                "runtime_project_id": "agent:ws-1:demo",
                "command": "printf ready",
                "timeout_secs": 30,
                "purpose": "test"
            }
        }))
        .unwrap();
        let request_json = AgentEnvelope::Request { request }.to_json().unwrap();
        match AgentEnvelope::from_slice(request_json.as_bytes()).unwrap() {
            AgentEnvelope::Request { request } => {
                let operation = request.persistent_shell.unwrap();
                assert_eq!(operation.action, "exec");
                assert_eq!(operation.shell_id, "wc_shell_1234");
                assert_eq!(operation.command.as_deref(), Some("printf ready"));
            }
            other => panic!("expected request, got {:?}", other.kind()),
        }

        let result = PersistentShellResult {
            shell_id: "wc_shell_1234".to_string(),
            workflow_session_id: "wc_sess_1234".to_string(),
            runtime_project_id: "agent:ws-1:demo".to_string(),
            shell_state: "running".to_string(),
            execution_state: "completed".to_string(),
            command_started: true,
            command_completed: true,
            exit_code: Some(0),
            stdout: "ready".to_string(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: 4,
            cwd: Some("/srv/demo".to_string()),
            initial_cwd: None,
            shell: None,
            profile: None,
            created_at: None,
            last_activity_at: Some(124),
            busy: false,
            already_closed: false,
            close_reason: None,
            error_code: None,
            error: None,
        };
        let result_json = AgentEnvelope::PersistentShellResult {
            payload: ShellAgentPersistentShellResultRequest {
                client_id: "ws-1".to_string(),
                agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                request_id: "req-shell-1".to_string(),
                result: result.clone(),
            },
        }
        .to_json()
        .unwrap();
        assert!(result_json.contains(r#""type":"persistent_shell_result""#));
        match AgentEnvelope::from_slice(result_json.as_bytes()).unwrap() {
            AgentEnvelope::PersistentShellResult { payload } => {
                assert_eq!(payload.request_id, "req-shell-1");
                assert_eq!(payload.result, result);
            }
            other => panic!("expected persistent shell result, got {:?}", other.kind()),
        }
    }

    #[test]
    fn result_and_job_update_envelopes_round_trip() {
        let result_env = AgentEnvelope::Result {
            payload: ShellAgentResultPayload {
                result: ShellAgentResultRequest {
                    client_id: "ws-1".to_string(),
                    agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                    request_id: "req-1".to_string(),
                    exit_code: Some(0),
                    stdout: Some("hi".to_string()),
                    stderr: None,
                    duration_ms: Some(5),
                    error: None,
                },
                command_execution_state: Some(ShellCommandExecutionState::Completed),
                mcp_gateway: None,
                coding_agent: None,
            },
        };
        let json = result_env.to_json().unwrap();
        assert!(json.contains(r#""type":"result""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.exit_code, Some(0));
                assert_eq!(
                    payload.command_execution_state,
                    Some(ShellCommandExecutionState::Completed)
                );
            }
            other => panic!("expected result, got {:?}", other.kind()),
        }

        let job_env = AgentEnvelope::JobUpdate {
            payload: ShellAgentJobUpdateRequest {
                client_id: "ws-1".to_string(),
                agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
                job_id: "job-1".to_string(),
                request_id: Some("req-1".to_string()),
                update_seq: None,
                status: "running".to_string(),
                stdout_chunk: Some("out".to_string()),
                stderr_chunk: None,
                stdout_tail: None,
                stderr_tail: None,
                log_snapshot: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                command_execution_state: None,
                validation_progress: None,
                finished: false,
            },
        };
        let json = job_env.to_json().unwrap();
        assert!(json.contains(r#""type":"job_update""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::JobUpdate { payload } => assert_eq!(payload.job_id, "job-1"),
            other => panic!("expected job_update, got {:?}", other.kind()),
        }
    }

    #[test]
    fn legacy_job_update_and_snapshot_default_structured_lifecycle_to_absent() {
        let update: ShellAgentJobUpdateRequest = serde_json::from_value(serde_json::json!({
            "client_id": "ws-1",
            "agent_instance_id": "11111111-1111-1111-1111-111111111111",
            "job_id": "job-legacy",
            "status": "completed",
            "exit_code": 0,
            "finished": true
        }))
        .unwrap();
        assert_eq!(update.command_execution_state, None);

        let inventory = reconciliation_inventory();
        let encoded = serde_json::to_value(&inventory).unwrap();
        assert!(encoded["jobs"][0].get("command_execution_state").is_none());
        let decoded: ShellJobInventory = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded.jobs[0].command_execution_state, None);
    }

    #[test]
    fn ping_pong_error_envelopes_round_trip() {
        let ping = AgentEnvelope::Ping { ts: 1700000000 };
        let json = ping.to_json().unwrap();
        assert_eq!(json, r#"{"type":"ping","ts":1700000000}"#);
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Ping { ts } => assert_eq!(ts, 1700000000),
            other => panic!("expected ping, got {:?}", other.kind()),
        }

        let err = AgentEnvelope::Error {
            code: "bad_request".to_string(),
            message: "nope".to_string(),
        };
        let json = err.to_json().unwrap();
        assert!(json.contains(r#""type":"error""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Error { code, message } => {
                assert_eq!(code, "bad_request");
                assert_eq!(message, "nope");
            }
            other => panic!("expected error, got {:?}", other.kind()),
        }
    }

    #[test]
    fn runtime_metadata_and_legacy_poll_payloads_round_trip() {
        let env = AgentEnvelope::RuntimeMetadata {
            tool_providers: sample_tool_providers(),
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"runtime_metadata""#));
        assert!(json.contains(r#""last_error_code":null"#));
        assert!(matches!(
            AgentEnvelope::from_slice(json.as_bytes()).unwrap(),
            AgentEnvelope::RuntimeMetadata { .. }
        ));

        let legacy = r#"{"client_id":"oe","agent_instance_id":"inst","projects":null}"#;
        let payload: ShellAgentPollPayload = serde_json::from_str(legacy).unwrap();
        assert_eq!(payload.request.client_id, "oe");
        assert!(payload.tool_providers.is_none());
    }

    #[test]
    fn goodbye_envelope_round_trips_and_reason_is_optional() {
        let env = AgentEnvelope::Goodbye {
            reason: Some("shutdown".to_string()),
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"goodbye""#));
        assert!(json.contains(r#""reason":"shutdown""#));
        match AgentEnvelope::from_slice(json.as_bytes()).unwrap() {
            AgentEnvelope::Goodbye { reason } => assert_eq!(reason.as_deref(), Some("shutdown")),
            other => panic!("expected goodbye, got {:?}", other.kind()),
        }

        let env = AgentEnvelope::Goodbye { reason: None };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"goodbye""#));
        assert!(!json.contains(r#""reason""#));
        assert!(matches!(
            AgentEnvelope::from_slice(json.as_bytes()).unwrap(),
            AgentEnvelope::Goodbye { reason: None }
        ));
    }

    #[test]
    fn invalid_envelope_type_is_rejected() {
        let json = r#"{"type":"not_a_real_variant"}"#;
        assert!(AgentEnvelope::from_slice(json.as_bytes()).is_err());
    }

    #[test]
    fn registered_envelope_omits_none_fields() {
        let env = AgentEnvelope::Registered {
            success: true,
            client: None,
            error: None,
        };
        let json = env.to_json().unwrap();
        assert!(json.contains(r#""type":"registered""#));
        assert!(json.contains(r#""success":true"#));
        // client/error are skip_serializing_if None.
        assert!(!json.contains(r#""client""#));
        assert!(!json.contains(r#""error""#));
    }

    #[test]
    fn register_request_round_trips_agent_instance_id() {
        let req = sample_register();
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""agent_instance_id":"11111111-1111-1111-1111-111111111111""#));
        let back: ShellClientRegisterRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "11111111-1111-1111-1111-111111111111"
        );
    }

    #[test]
    fn older_register_request_without_job_concurrency_limit_deserializes_as_unknown() {
        let json = r#"{
            "client_id": "legacy-runner",
            "agent_instance_id": "legacy-instance",
            "capabilities": {"shell": true}
        }"#;
        let request: ShellClientRegisterRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.job_concurrency_limit, None);
    }

    #[test]
    fn register_request_without_agent_instance_id_is_rejected() {
        // An old agent that omits agent_instance_id must be rejected at
        // deserialization: the field is now required for correctness.
        let json = r#"{
            "client_id": "oe",
            "capabilities": {"shell": true}
        }"#;
        let err = serde_json::from_str::<ShellClientRegisterRequest>(json);
        assert!(err.is_err(), "missing agent_instance_id must be rejected");
    }

    #[test]
    fn poll_result_job_update_round_trip_agent_instance_id() {
        let poll = ShellAgentPollRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
            projects: None,
        };
        let json = serde_json::to_string(&poll).unwrap();
        let back: ShellAgentPollRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "22222222-2222-2222-2222-222222222222"
        );

        let result = ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
            request_id: "req-1".to_string(),
            exit_code: Some(0),
            stdout: None,
            stderr: None,
            duration_ms: None,
            error: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ShellAgentResultRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "22222222-2222-2222-2222-222222222222"
        );

        let job = ShellAgentJobUpdateRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "22222222-2222-2222-2222-222222222222".to_string(),
            job_id: "job-1".to_string(),
            request_id: None,
            update_seq: None,
            status: "running".to_string(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: None,
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        };
        let json = serde_json::to_string(&job).unwrap();
        let back: ShellAgentJobUpdateRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(
            back.agent_instance_id,
            "22222222-2222-2222-2222-222222222222"
        );
    }

    #[test]
    fn poll_result_job_update_without_agent_instance_id_are_rejected() {
        assert!(serde_json::from_str::<ShellAgentPollRequest>(r#"{"client_id":"oe"}"#).is_err());
        assert!(serde_json::from_str::<ShellAgentResultRequest>(
            r#"{"client_id":"oe","request_id":"r1"}"#
        )
        .is_err());
        assert!(serde_json::from_str::<ShellAgentJobUpdateRequest>(
            r#"{"client_id":"oe","job_id":"j1","status":"running"}"#
        )
        .is_err());
    }

    #[test]
    fn quic_frame_encode_prefixes_u32_be_length() {
        let env = AgentEnvelope::Ping { ts: 42 };
        let frame = encode_quic_frame(&env).unwrap();
        // First 4 bytes are the big-endian JSON length.
        let len = u32::from_be_bytes(frame[0..4].try_into().unwrap()) as usize;
        assert_eq!(len, frame.len() - 4);
        // The body is valid JSON containing the ping.
        let body = &frame[4..];
        assert!(std::str::from_utf8(body)
            .unwrap()
            .contains(r#""type":"ping""#));
    }

    #[tokio::test]
    async fn quic_frame_round_trips_through_read_write() {
        use tokio::io::AsyncReadExt;
        let env = AgentEnvelope::Pong { ts: 99 };
        let mut buf: Vec<u8> = Vec::new();
        write_quic_frame(&mut buf, &env).await.unwrap();
        // Drain the written bytes through a slice reader.
        let mut reader: &[u8] = &buf;
        let back = read_quic_frame(&mut reader).await.unwrap();
        assert!(matches!(back, AgentEnvelope::Pong { ts: 99 }));
        // The stream is fully consumed.
        let mut tail = Vec::new();
        let n = reader.read_to_end(&mut tail).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn quic_frame_rejects_oversized_announced_length() {
        // Craft a header announcing a length far above the cap, followed by a
        // tiny body. The codec must reject *before* allocating/reading the
        // announced body.
        let huge = (QUIC_FRAME_MAX_BYTES as u32 + 1).to_be_bytes();
        let mut bad: Vec<u8> = Vec::new();
        bad.extend_from_slice(&huge);
        bad.extend_from_slice(b"{}");
        let mut reader: &[u8] = &bad;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        match err {
            QuicFrameError::Oversized { len, max } => {
                assert_eq!(len, QUIC_FRAME_MAX_BYTES + 1);
                assert_eq!(max, QUIC_FRAME_MAX_BYTES);
            }
            other => panic!("expected Oversized, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn quic_frame_rejects_malformed_body_and_short_stream() {
        // Announced length says 5 bytes but the body is invalid JSON.
        let mut bad: Vec<u8> = Vec::new();
        bad.extend_from_slice(&5u32.to_be_bytes());
        bad.extend_from_slice(b"notjs");
        let mut reader: &[u8] = &bad;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, QuicFrameError::Json(_)), "got {err:?}");

        // Announced length says 10 bytes but the stream ends after 2.
        let mut short: Vec<u8> = Vec::new();
        short.extend_from_slice(&10u32.to_be_bytes());
        short.extend_from_slice(b"ab");
        let mut reader: &[u8] = &short;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, QuicFrameError::Malformed(_)), "got {err:?}");

        // Empty stream -> EmptyStream, not Malformed.
        let empty: Vec<u8> = Vec::new();
        let mut reader: &[u8] = &empty;
        let err = read_quic_frame(&mut reader).await.unwrap_err();
        assert!(matches!(err, QuicFrameError::EmptyStream), "got {err:?}");
    }

    #[tokio::test]
    async fn quic_register_codec_preserves_legacy_wire_shape_and_round_trips() {
        let payload = ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            client_id: "q-1".to_string(),
            agent_instance_id: "11111111-1111-1111-1111-111111111111".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: None,
            host_context: None,
            projects: None,
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
            policy: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
        };
        let frame = QuicRegisterFrame::new(payload, Some("wc_agent_secret".to_string()));
        let encoded = encode_quic_register_frame(&frame).unwrap();
        let json = std::str::from_utf8(&encoded[4..]).unwrap();
        assert!(json.contains(r#""type":"register""#), "json was: {json}");
        assert!(json.contains(r#""client_id":"q-1""#), "json was: {json}");
        assert!(
            json.contains(r#""auth_token":"wc_agent_secret""#),
            "json was: {json}"
        );

        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum LegacyQuicRegisterEnvelope {
            Register {
                #[serde(flatten)]
                payload: ShellClientRegisterRequest,
                #[serde(default)]
                auth_token: Option<String>,
            },
        }
        match serde_json::from_str::<LegacyQuicRegisterEnvelope>(json).unwrap() {
            LegacyQuicRegisterEnvelope::Register {
                payload,
                auth_token,
            } => {
                assert_eq!(payload.client_id, "q-1");
                assert_eq!(auth_token.as_deref(), Some("wc_agent_secret"));
            }
        }

        let legacy_json = format!(
            r#"{{"type":"register","client_id":"q-legacy","agent_instance_id":"11111111-1111-1111-1111-111111111112","agent_protocol_version":"quic-v1","auth_token":"legacy-secret"}}"#
        );
        let mut legacy_wire = Vec::new();
        legacy_wire.extend_from_slice(&(legacy_json.len() as u32).to_be_bytes());
        legacy_wire.extend_from_slice(legacy_json.as_bytes());
        let (legacy_payload, legacy_token) = read_quic_register_frame(&mut legacy_wire.as_slice())
            .await
            .unwrap()
            .into_parts();
        assert_eq!(legacy_payload.client_id, "q-legacy");
        assert_eq!(legacy_token.as_deref(), Some("legacy-secret"));
    }
}

#[cfg(test)]
mod filter_canonical_tests {
    use super::*;

    fn cargo_test(filter: &str) -> ShellJobValidationStep {
        ShellJobValidationStep {
            name: "test".to_string(),
            program: "cargo".to_string(),
            args: vec!["test".to_string(), filter.to_string()],
            env: Vec::new(),
        }
    }

    #[test]
    fn step_env_allowlist_gates_canonicality() {
        let mut step = cargo_test("tool_runtime");
        assert!(step.is_canonical());
        step.env
            .push(("CARGO_TARGET_DIR".to_string(), "/state/cache".to_string()));
        assert!(
            step.is_canonical(),
            "allowlisted env key must stay canonical"
        );
        step.env.push(("PATH".to_string(), "/tmp/evil".to_string()));
        assert!(
            !step.is_canonical(),
            "non-allowlisted env keys must break canonicality"
        );
    }

    #[test]
    fn canonical_go_test_accepts_legacy_and_bounded_json_packages() {
        let step = |args: &[&str]| ShellJobValidationStep {
            name: "test".to_string(),
            program: "go".to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            env: Vec::new(),
        };
        assert!(step(&["test", "./..."]).is_canonical());
        assert!(step(&["test", "-json", "./..."]).is_canonical());
        assert!(step(&["test", "-json", "./pkg"]).is_canonical());
        assert!(step(&["test", "-json", ".", "./pkg", "./internal/..."]).is_canonical());
        assert!(!step(&["test", "-json", "-run", "TestOne", "./..."]).is_canonical());
        assert!(!step(&["test", "-v", "./..."]).is_canonical());
        assert!(!step(&["run", "./..."]).is_canonical());
    }

    fn validation_step(name: &str, program: &str, args: &[&str]) -> ShellJobValidationStep {
        ShellJobValidationStep {
            name: name.to_string(),
            program: program.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            env: Vec::new(),
        }
    }

    fn validation_metadata(
        tool: &str,
        kind: &str,
        step: ShellJobValidationStep,
    ) -> ShellJobValidationMetadata {
        ShellJobValidationMetadata {
            tool: tool.to_string(),
            kind: kind.to_string(),
            steps: vec![step],
            effective_timeout_secs: 1800,
            sync_wait_secs: 10,
            adapter: tool.to_string(),
            validation_target_id: None,
            minimum_tests: None,
        }
    }

    #[test]
    fn structured_validation_metadata_binds_tool_kind_and_canonical_step() {
        for metadata in [
            validation_metadata(
                "cargo_fmt",
                "format",
                validation_step("format", "cargo", &["fmt", "--", "--check"]),
            ),
            validation_metadata(
                "cargo_check",
                "check",
                validation_step("check", "cargo", &["check", "--all-targets"]),
            ),
            validation_metadata(
                "cargo_test",
                "test",
                validation_step("test", "cargo", &["test", "tool_runtime"]),
            ),
        ] {
            assert!(metadata.is_valid(), "expected valid metadata: {metadata:?}");
        }

        let metadata = validation_metadata(
            "go_test",
            "test",
            validation_step("test", "go", &["test", "-json", "./..."]),
        );
        assert!(metadata.is_valid());

        let mut wrong_adapter = metadata.clone();
        wrong_adapter.adapter = "cargo_test".to_string();
        assert!(!wrong_adapter.is_valid());

        let mut wrong_kind = metadata.clone();
        wrong_kind.kind = "check".to_string();
        assert!(!wrong_kind.is_valid());

        let mut cargo_assertion = validation_metadata(
            "cargo_test",
            "test",
            validation_step("test", "cargo", &["test", "tool_runtime"]),
        );
        cargo_assertion.minimum_tests = Some(6);
        assert!(cargo_assertion.is_valid());
        cargo_assertion.minimum_tests = Some(0);
        assert!(!cargo_assertion.is_valid());
        cargo_assertion.minimum_tests = Some(CARGO_TEST_MIN_TESTS_MAX + 1);
        assert!(!cargo_assertion.is_valid());

        let mut go_assertion = metadata.clone();
        go_assertion.minimum_tests = Some(1);
        assert!(
            !go_assertion.is_valid(),
            "Cargo-specific assertion metadata must not cross validation adapters"
        );

        let plain_go = validation_step("test", "go", &["test", "./..."]);
        assert!(plain_go.is_canonical());
        assert!(!validation_metadata("go_test", "test", plain_go).is_valid());

        let cargo_cross_product = validation_step("test", "cargo", &["test", "tool_runtime"]);
        assert!(cargo_cross_product.is_canonical());
        assert!(!validation_metadata("go_test", "test", cargo_cross_product).is_valid());

        let pytest_cross_product = validation_step("test", "python", &["-m", "pytest"]);
        assert!(pytest_cross_product.is_canonical());
        assert!(!validation_metadata("go_test", "test", pytest_cross_product).is_valid());

        let npm_cross_product = validation_step("test", "npm", &["run", "--silent", "test"]);
        assert!(npm_cross_product.is_canonical());
        assert!(!validation_metadata("go_test", "test", npm_cross_product).is_valid());

        let mut env_injected = metadata;
        env_injected.steps[0]
            .env
            .push(("CARGO_TARGET_DIR".to_string(), "/tmp/cache".to_string()));
        assert!(
            !env_injected.steps[0].is_canonical(),
            "structured go_test rejects per-step environment overrides"
        );
        assert!(!env_injected.is_valid());
    }

    #[test]
    fn canonical_cargo_check_argv_accepts_read_only_flags_only() {
        let step = |args: Vec<&str>| ShellJobValidationStep {
            name: "check".to_string(),
            program: "cargo".to_string(),
            args: args.into_iter().map(str::to_string).collect(),
            env: Vec::new(),
        };
        let accepted = [
            vec!["check"],
            vec!["check", "--all-targets"],
            vec!["check", "--all-features"],
            vec!["check", "--no-default-features"],
            vec!["check", "--all-targets", "--all-features"],
            vec!["check", "--features", "serde"],
            vec!["check", "--features", "a b"],
            vec!["check", "-p", "my-crate"],
            vec![
                "check",
                "--all-targets",
                "-p",
                "my-crate",
                "--features",
                "x",
            ],
        ];
        for args in accepted {
            assert!(
                step(args.clone()).is_canonical(),
                "expected canonical for {args:?}"
            );
        }
        let rejected = [
            vec!["check", "--all-targets", "--all-targets"],
            vec!["check", "--no-run"],
            vec!["check", "--features"],
            vec!["check", "--features", ""],
            vec!["check", "--features", "--no-run"],
            vec!["check", "-p", "--all-features"],
            vec!["check", "--features", " "],
            vec!["check", "--features", "  serde"],
            vec!["check", "--features", "serde  "],
            vec!["check", "--features", "line\nbreak"],
            vec!["check", "-p", "tab\tvalue"],
            vec!["check", "--manifest-path", "/tmp/Cargo.toml"],
            vec!["check", "--locked"],
            vec!["check", "--", "--all-targets"],
        ];
        let over_long_value = "a".repeat(CARGO_VALUE_MAX_BYTES + 1);
        let rejected_over_long = vec!["check", "--features", over_long_value.as_str()];
        assert!(
            !step(rejected_over_long.clone()).is_canonical(),
            "expected non-canonical for {rejected_over_long:?}"
        );
        // A feature list at exactly the max length remains canonical.
        let max_len_feature = "a".repeat(CARGO_VALUE_MAX_BYTES);
        assert!(
            step(vec!["check", "--features", max_len_feature.as_str()]).is_canonical(),
            "max-length feature value must stay canonical"
        );
        for args in rejected {
            assert!(
                !step(args.clone()).is_canonical(),
                "expected non-canonical for {args:?}"
            );
        }
    }

    #[test]
    fn canonical_cargo_test_argv_accepts_filter_and_read_only_flags() {
        let step = |args: Vec<&str>| ShellJobValidationStep {
            name: "test".to_string(),
            program: "cargo".to_string(),
            args: args.into_iter().map(str::to_string).collect(),
            env: Vec::new(),
        };
        let accepted = [
            vec!["test"],
            vec!["test", "focused"],
            vec!["test", "--all-targets"],
            vec!["test", "--no-run"],
            vec!["test", "focused", "--all-features"],
            vec!["test", "--features", "serde", "--no-run"],
            vec!["test", "-p", "my-crate", "--no-default-features"],
        ];
        for args in accepted {
            assert!(
                step(args.clone()).is_canonical(),
                "expected canonical for {args:?}"
            );
        }
        let rejected = [
            vec!["test", "--no-run", "--no-run"],
            vec!["test", "--no-run", "--all-targets", "--all-targets"],
            vec!["test", "--no-default-features", "--no-default-features"],
            vec!["test", "--all-features", "--all-features"],
            vec!["test", "--features", "--no-run"],
            vec!["test", "-p", "--all-features"],
            vec!["test", "--features", ""],
            vec!["test", "--features", "  serde"],
            vec!["test", "-p", "crate  "],
            vec!["test", "--features", "nul\0byte"],
            vec!["test", "--features", "col\tumn"],
            vec!["test", "--manifest-path", "/tmp/Cargo.toml"],
            vec!["test", "--", "--all-targets"],
        ];
        for args in rejected {
            assert!(
                !step(args.clone()).is_canonical(),
                "expected non-canonical for {args:?}"
            );
        }
    }

    #[test]
    fn cargo_test_filter_arm_is_the_fail_closed_boundary() {
        // The reported failure and its variants: a Cargo option or control
        // bytes smuggled in as a "filter". The Agent-facing canonical contract
        // must reject them independently of the planner so an old server,
        // forged request, or protocol drift cannot redirect the manifest.
        let too_long = "a".repeat(RUST_TEST_FILTER_MAX_BYTES + 1);
        // `--all-features` and `--no-default-features` are in the Cargo flag
        // allowlist, so a flat `["test", "--all-features"]` argv is a legal
        // `cargo test --all-features` and parses as the flag (flat-argv
        // information loss). The rejection of option-like filters belongs to
        // `valid_rust_test_filter` / the planner, not this flat-argv boundary.
        // Every other option is still rejected at the canonical boundary.
        let rejected = [
            "--manifest-path=/tmp/outside/Cargo.toml",
            "--manifest-path",
            "--target-dir=/tmp/outside-target",
            "--package=another-package",
            "--workspace",
            "--all",
            "--doc",
            "--test=another-target",
            "--bench=another-target",
            "--features=unexpected",
            "-Zunstable-options",
            "-h",
            "--help",
            "-",
            "--",
            " --",
            " --help",
            "line\nbreak",
            "line\rbreak",
            "col\tumn",
            "nul\0byte",
            "",
            too_long.as_str(),
        ];
        for filter in rejected {
            assert!(
                !cargo_test(filter).is_canonical(),
                "expected non-canonical for {filter:?}"
            );
        }
        // The option-like values that are allowlisted Cargo flags are rejected
        // as *filters* by the shared contract even though the flat argv parses
        // as the flag.
        for filter in ["--all-features", "--no-default-features"] {
            assert!(
                !valid_rust_test_filter(filter),
                "expected non-canonical filter for {filter:?}"
            );
            assert!(
                cargo_test(filter).is_canonical(),
                "flat argv {filter:?} must parse as the legal Cargo flag"
            );
        }

        // No-filter and valid name substrings (including the max length) remain
        // canonical.
        let no_filter = ShellJobValidationStep {
            name: "test".to_string(),
            program: "cargo".to_string(),
            args: vec!["test".to_string()],
            env: Vec::new(),
        };
        assert!(no_filter.is_canonical());
        let max_len = "a".repeat(RUST_TEST_FILTER_MAX_BYTES);
        for filter in [
            "module::nested::test_name",
            "测试::筛选",
            "name; $(sub)",
            max_len.as_str(),
        ] {
            assert!(
                cargo_test(filter).is_canonical(),
                "expected canonical for {filter:?}"
            );
        }
    }

    #[test]
    fn cargo_value_contract_normalizes_exactly_once_and_fails_closed() {
        // The shared normalization contract used by both the synchronous
        // command builders and the structured Job argv builder.
        assert_eq!(
            normalize_cargo_value("serde").unwrap(),
            Some("serde".to_string())
        );
        // Multi-word feature lists remain legal.
        assert_eq!(
            normalize_cargo_value("a b").unwrap(),
            Some("a b".to_string())
        );
        assert_eq!(
            normalize_cargo_value("  a  b  ").unwrap(),
            Some("a  b".to_string())
        );
        // Exactly one leading/trailing trim, applied consistently.
        assert_eq!(
            normalize_cargo_value("  serde  ").unwrap(),
            Some("serde".to_string())
        );
        // Blank input means "option omitted".
        assert_eq!(normalize_cargo_value("").unwrap(), None);
        assert_eq!(normalize_cargo_value("   ").unwrap(), None);

        // Option-like values must never be consumed as an option's value.
        assert!(normalize_cargo_value("--no-run").is_err());
        assert!(normalize_cargo_value("--all-features").is_err());
        assert!(normalize_cargo_value("-p").is_err());
        // Control bytes and NUL are rejected.
        assert!(normalize_cargo_value("line\nbreak").is_err());
        assert!(normalize_cargo_value("tab\tvalue").is_err());
        assert!(normalize_cargo_value("nul\0byte").is_err());
        // Over-long values are rejected; the max length is accepted.
        let over_long = "a".repeat(CARGO_VALUE_MAX_BYTES + 1);
        assert!(normalize_cargo_value(&over_long).is_err());
        let max_len_value = "a".repeat(CARGO_VALUE_MAX_BYTES);
        assert_eq!(
            normalize_cargo_value(&max_len_value).unwrap(),
            Some(max_len_value.clone())
        );
    }

    #[test]
    fn go_test_package_scope_is_narrow_bounded_and_canonical() {
        assert_eq!(
            normalize_go_test_packages(None).unwrap(),
            vec!["./...".to_string()]
        );
        for package in [".", "./...", "./pkg", "./pkg/...", "./a_b-C.d/sub"] {
            assert_eq!(
                normalize_go_test_packages(Some(&[package.to_string()])).unwrap(),
                vec![package.to_string()]
            );
        }

        let eight = (0..GO_TEST_PACKAGE_MAX_ITEMS)
            .map(|index| format!("./pkg{index}"))
            .collect::<Vec<_>>();
        assert_eq!(normalize_go_test_packages(Some(&eight)).unwrap(), eight);
        assert!(normalize_go_test_packages(Some(&[])).is_err());
        let nine = (0..=GO_TEST_PACKAGE_MAX_ITEMS)
            .map(|index| format!("./pkg{index}"))
            .collect::<Vec<_>>();
        assert!(normalize_go_test_packages(Some(&nine)).is_err());

        let max = format!("./{}", "a".repeat(GO_TEST_PACKAGE_MAX_BYTES - 2));
        assert!(normalize_go_test_packages(Some(&[max])).is_ok());
        let over = format!("./{}", "a".repeat(GO_TEST_PACKAGE_MAX_BYTES - 1));
        assert!(normalize_go_test_packages(Some(&[over])).is_err());

        for invalid in [
            "../...",
            "/abs",
            "pkg",
            "./",
            "./foo/../bar",
            "./foo/./bar",
            "./foo//bar",
            "./foo\\bar",
            "./foo bar",
            "./foo\tbar",
            "./foo\nbar",
            "./foo\0bar",
            "./foo;bar",
            "./foo$bar",
            "./foo/.../bar",
            "./foo...",
            "./....",
            "--exec=/tmp/x",
        ] {
            assert!(
                normalize_go_test_packages(Some(&[invalid.to_string()])).is_err(),
                "expected invalid package pattern {invalid:?}"
            );
        }

        let focused = ShellJobValidationStep {
            name: "test".to_string(),
            program: "go".to_string(),
            args: vec![
                "test".to_string(),
                "-json".to_string(),
                "./internal/control".to_string(),
                "./internal/node".to_string(),
            ],
            env: Vec::new(),
        };
        assert!(focused.is_structured_go_test_json());
        assert!(focused.is_canonical());

        for args in [
            vec!["test", "-json", "./pkg", "-exec"],
            vec!["test", "-json", "./pkg;go", "./other"],
            vec!["test", "-json", "../..."],
        ] {
            let step = ShellJobValidationStep {
                name: "test".to_string(),
                program: "go".to_string(),
                args: args.into_iter().map(str::to_string).collect(),
                env: Vec::new(),
            };
            assert!(!step.is_structured_go_test_json());
            assert!(!step.is_canonical());
        }
    }

    #[test]
    fn flat_argv_with_option_like_value_is_rejected_by_normalization() {
        // The reported failure and its variants. A value-taking flag must never
        // consume the next Cargo option as its value, whether on `check` or
        // `test`, and regardless of runtime path.
        let check = |args: Vec<&str>| ShellJobValidationStep {
            name: "check".to_string(),
            program: "cargo".to_string(),
            args: args.into_iter().map(str::to_string).collect(),
            env: Vec::new(),
        };
        for args in [
            vec!["check", "--features", "--no-run"],
            vec!["check", "-p", "--all-features"],
            vec!["check", "--features", "-p"],
        ] {
            assert!(
                !check(args.clone()).is_canonical(),
                "expected non-canonical for {args:?}"
            );
        }
        let test = |args: Vec<&str>| ShellJobValidationStep {
            name: "test".to_string(),
            program: "cargo".to_string(),
            args: args.into_iter().map(str::to_string).collect(),
            env: Vec::new(),
        };
        for args in [
            vec!["test", "--features", "--no-run"],
            vec!["test", "-p", "--all-features"],
        ] {
            assert!(
                !test(args.clone()).is_canonical(),
                "expected non-canonical for {args:?}"
            );
        }
    }

    #[test]
    fn filter_boundary_is_flat_argv_information_loss_not_acceptance() {
        // `["test", "--all-features"]` is a legal `cargo test --all-features`
        // and must parse as the flag. It is NOT a legal filter: the planner
        // rejects option-like filters before argv is built. `is_canonical`
        // cannot classify the same flat argv as both legal and illegal.
        let step = cargo_test("--all-features");
        assert!(step.is_canonical(), "flat argv parses as the Cargo flag");
        assert!(
            !valid_rust_test_filter("--all-features"),
            "option-like values must be rejected as filters"
        );
        // --all-features as a filter value must equally never survive as a
        // value-taking argument's value.
        assert!(normalize_cargo_value("--all-features").is_err());
        // The still-forbidden options remain rejected at the canonical
        // boundary when they appear as value-taking values or extra flags.
        for args in [
            vec!["test", "--manifest-path", "/tmp/Cargo.toml"],
            vec!["test", "--target-dir", "/tmp/outside-target"],
            vec!["test", "-Z", "unstable-options"],
        ] {
            assert!(
                !cargo_test_argv(args.clone()).is_canonical(),
                "expected non-canonical for {args:?}"
            );
        }
    }

    fn cargo_test_argv(args: Vec<&str>) -> ShellJobValidationStep {
        ShellJobValidationStep {
            name: "test".to_string(),
            program: "cargo".to_string(),
            args: args.into_iter().map(str::to_string).collect(),
            env: Vec::new(),
        }
    }
}
