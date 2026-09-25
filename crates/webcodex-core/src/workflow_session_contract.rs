//! Stable protocol-neutral contracts shared with the Workflow Session domain.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SESSION_ID_PREFIX: &str = "wc_sess_";

pub const MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS: usize =
    crate::runner_protocol::VALIDATION_ASSERTION_NAME_MAX_CHARS;
pub const TOOL_CALL_RECORDING_SESSION_ID_FIELD: &str = "recording_session_id";
pub const TOOL_CALL_ACK_SESSION_MESSAGE_IDS_FIELD: &str = "ack_session_message_ids";
pub const TOOL_CALL_SESSION_MESSAGE_RESOLUTION_FIELD: &str = "session_message_resolution";
pub const MAX_TOOL_CALL_ACK_MESSAGE_IDS: usize = 8;
pub const TOOL_EXPECTED_FAILURE_FIELD: &str = "expected_failure";
pub const TOOL_EXPECTED_FAILURE_KIND_FIELD: &str = "expected_failure_kind";
pub const TOOL_RESULT_EXPECTATION_FIELD: &str = "result_expectation";
pub const TOOL_ACCEPTED_EXIT_CODES_FIELD: &str = "accepted_exit_codes";
pub const TOOL_ASSERTION_NAME_FIELD: &str = "assertion_name";
pub const TOOL_CALL_EXPECTATION_METADATA_FIELDS: &[&str] = &[
    TOOL_EXPECTED_FAILURE_FIELD,
    TOOL_EXPECTED_FAILURE_KIND_FIELD,
    TOOL_RESULT_EXPECTATION_FIELD,
    TOOL_ACCEPTED_EXIT_CODES_FIELD,
    TOOL_ASSERTION_NAME_FIELD,
];

pub fn is_tool_call_expectation_metadata_field(field: &str) -> bool {
    TOOL_CALL_EXPECTATION_METADATA_FIELDS.contains(&field)
}

/// Public/model-facing recorder metadata support. These classifiers belong to
/// the wrapper contract, not to any concrete ToolCall business payload.
pub fn tool_supports_model_facing_assertion_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "run_process" | "run_script" | "run_shell" | "run_job"
    )
}

pub fn tool_supports_model_facing_result_expectation(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "run_process"
            | "run_script"
            | "run_shell"
            | "session_shell_exec"
            | "cargo_fmt"
            | "cargo_check"
            | "cargo_test"
            | "go_test"
    )
}

pub fn tool_supports_model_facing_accepted_exit_codes(tool_name: &str) -> bool {
    tool_name == "run_process"
}

fn model_facing_assertion_looks_secret_like(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("bearer ")
        || value.contains("wc_pat_")
        || value.contains("wc_oat_")
        || value.contains("wc_ort_")
        || value.contains("wc_agent_")
        || value.contains("wc_acct_")
        || value.contains("wc_pair_")
        || value.contains("wc_csec_")
        || value.contains("client_secret")
}

pub fn validate_model_facing_assertion_name(
    tool_name: &str,
    arguments: &Value,
) -> Result<(), String> {
    if !tool_supports_model_facing_assertion_name(tool_name) {
        return Ok(());
    }
    let Some(value) = arguments
        .as_object()
        .and_then(|object| object.get(TOOL_ASSERTION_NAME_FIELD))
    else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': assertion_name must be a string"
        ));
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': assertion_name must not be empty or whitespace-only"
        ));
    }
    if trimmed.chars().count() > MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': assertion_name exceeds the {MAX_MODEL_VALIDATION_ASSERTION_NAME_CHARS}-character limit"
        ));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': assertion_name must be a single-line human-readable label"
        ));
    }
    if model_facing_assertion_looks_secret_like(trimmed) {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': assertion_name must not contain credential-like material"
        ));
    }
    Ok(())
}

pub fn validate_model_facing_result_expectation(
    tool_name: &str,
    arguments: &Value,
) -> Result<(), String> {
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let result_expectation = object.get(TOOL_RESULT_EXPECTATION_FIELD);
    let accepted_exit_codes = object.get(TOOL_ACCEPTED_EXIT_CODES_FIELD);
    if result_expectation.is_none() && accepted_exit_codes.is_none() {
        return Ok(());
    }
    if !tool_supports_model_facing_result_expectation(tool_name) {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': result expectation is not supported by this tool"
        ));
    }
    if tool_name == "cargo_fmt"
        && result_expectation.is_some()
        && object.get("check").and_then(Value::as_bool) != Some(true)
    {
        return Err(
            "invalid arguments for tool 'cargo_fmt': result_expectation is supported only with check=true; mutating cargo fmt failures cannot be reclassified as expected observations"
                .to_string(),
        );
    }
    if let Some(value) = result_expectation {
        let Some(value) = value.as_str() else {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': result_expectation must be one of success, failure, or observe"
            ));
        };
        if !matches!(value, "success" | "failure" | "observe") {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': result_expectation must be one of success, failure, or observe"
            ));
        }
    }
    if let Some(value) = accepted_exit_codes {
        if !tool_supports_model_facing_accepted_exit_codes(tool_name) {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': accepted_exit_codes is supported only by run_process"
            ));
        }
        let Some(values) = value.as_array() else {
            return Err(
                "invalid arguments for tool 'run_process': accepted_exit_codes must be a non-empty array of integers"
                    .to_string(),
            );
        };
        if values.is_empty()
            || values.len() > 32
            || values.iter().any(|value| value.as_i64().is_none())
        {
            return Err(
                "invalid arguments for tool 'run_process': accepted_exit_codes must contain 1..32 integers"
                    .to_string(),
            );
        }
        if result_expectation
            .and_then(Value::as_str)
            .is_some_and(|value| value != "observe")
        {
            return Err(
                "invalid arguments for tool 'run_process': accepted_exit_codes may be combined only with result_expectation=observe (or with result_expectation omitted)"
                    .to_string(),
            );
        }
    }
    Ok(())
}

pub fn strip_tool_call_expectation_metadata(arguments: Value) -> Value {
    let Value::Object(mut object) = arguments else {
        return arguments;
    };
    for &field in TOOL_CALL_EXPECTATION_METADATA_FIELDS {
        object.remove(field);
    }
    Value::Object(object)
}

/// Session ledger compatibility is deliberately local to this domain.
fn is_session_identity_suffix(suffix: &str) -> bool {
    crate::compact::decode::<12>(suffix).is_some()
        || (suffix.len() == 32
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

pub fn is_valid_session_id(value: &str) -> bool {
    value
        .strip_prefix(SESSION_ID_PREFIX)
        .is_some_and(is_session_identity_suffix)
}

pub fn is_valid_session_message_id(value: &str) -> bool {
    value
        .strip_prefix("wc_msg_")
        .is_some_and(is_session_identity_suffix)
}

pub const SESSION_INBOX_ACK_REQUIRED_ATTENTION_REASON: &str = "session_message_requires_ack";
pub const SESSION_INBOX_ACK_REQUIRED_ATTENTION_INSTRUCTION: &str =
    "A Session message requiring acknowledgement is pending. Read session_discussion_summary before continuing.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionShell {
    Sh,
    Bash,
}

impl ExecutionShell {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Bash => "bash",
        }
    }
}

/// Execution intent used for evidence classification. Generic execution may
/// declare it explicitly; structured validators derive it from tool identity.
/// It never grants authority or selects the command that Runtime executes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPurpose {
    Validation,
    Test,
    Build,
    Format,
    Release,
    Diagnostic,
    Operation,
    #[default]
    Other,
}

pub const EXECUTION_PURPOSE_VALUES: &[&str] = &[
    "validation",
    "test",
    "build",
    "format",
    "release",
    "diagnostic",
    "operation",
    "other",
];

impl ExecutionPurpose {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::Test => "test",
            Self::Build => "build",
            Self::Format => "format",
            Self::Release => "release",
            Self::Diagnostic => "diagnostic",
            Self::Operation => "operation",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "validation" => Some(Self::Validation),
            "test" => Some(Self::Test),
            "build" => Some(Self::Build),
            "format" => Some(Self::Format),
            "release" => Some(Self::Release),
            "diagnostic" => Some(Self::Diagnostic),
            "operation" => Some(Self::Operation),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub const fn is_validation_like(self) -> bool {
        matches!(
            self,
            Self::Validation | Self::Test | Self::Build | Self::Format | Self::Release
        )
    }
}

pub fn is_validation_like_execution_purpose(value: &str) -> bool {
    ExecutionPurpose::parse(value).is_some_and(ExecutionPurpose::is_validation_like)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    #[default]
    Normal,
    ReadOnly,
}

impl SessionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ReadOnly => "read_only",
        }
    }
}

/// Durable execution defaults inherited by a closed set of execution tools
/// attached to a project-scoped Workflow Session.
///
/// This intentionally contains no environment, credential, connection, or
/// arbitrary option bag. `resource` is only a named Runner-local SSH resource;
/// it never stores an SSH host, config, key, password, or transport.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionExecutionContext {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_shell: Option<ExecutionShell>,
    /// Optional logical name of a Runner-owned resource on the Runner that owns this Session's
    /// project. It changes supported one-shot/background shell execution and newly opened
    /// `open_session_shell` execution location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource: Option<String>,
}

impl SessionExecutionContext {
    pub fn is_empty(&self) -> bool {
        self.default_cwd.is_none() && self.default_shell.is_none() && self.resource.is_none()
    }

    /// Validate and normalize persisted execution-context fields.
    ///
    /// Without an SSH resource, `default_cwd` remains project-relative and
    /// follows the existing project-bound validation. With one, it is a remote
    /// path instead and never reaches Runner-local project path validation.
    pub fn validated(mut self) -> Result<Self, String> {
        if let Some(raw_resource) = self.resource.take() {
            let resource = raw_resource.trim();
            if resource.is_empty()
                || resource.len() > 80
                || resource.contains("..")
                || !resource
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            {
                return Err(
                    "execution_context.resource must be a safe named SSH resource".to_string(),
                );
            }
            self.resource = Some(resource.to_string());
        }
        if let Some(raw_cwd) = self.default_cwd.take() {
            let cwd = raw_cwd.trim();
            if self.resource.is_some() {
                if cwd.is_empty() || cwd.len() > 4096 || cwd.chars().any(char::is_control) {
                    return Err(
                        "execution_context.default_cwd must be a bounded remote path without control characters"
                            .to_string(),
                    );
                }
                self.default_cwd = Some(cwd.to_string());
            } else {
                crate::validation_bridge::validate_project_relative_path(cwd)
                    .map_err(|error| format!("execution_context.default_cwd {error}"))?;
                let normalized = cwd
                    .split(['/', '\\'])
                    .filter(|component| !component.is_empty() && *component != ".")
                    .collect::<Vec<_>>()
                    .join("/");
                self.default_cwd = Some(if normalized.is_empty() {
                    ".".to_string()
                } else {
                    normalized
                });
            }
        }
        Ok(self)
    }

    /// Restore valid fields independently so a malformed persisted cwd cannot
    /// bypass the project boundary or erase a valid explicit shell choice.
    pub fn sanitized_for_restore(mut self) -> Self {
        self.resource = self.resource.take().and_then(|raw_resource| {
            Self {
                default_cwd: None,
                default_shell: None,
                resource: Some(raw_resource),
            }
            .validated()
            .ok()
            .and_then(|context| context.resource)
        });
        if let Some(raw_cwd) = self.default_cwd.take() {
            let cwd_only = Self {
                default_cwd: Some(raw_cwd),
                default_shell: None,
                resource: self.resource.clone(),
            };
            self.default_cwd = cwd_only
                .validated()
                .ok()
                .and_then(|context| context.default_cwd);
        }
        self
    }

    /// Audit-safe form for pre-validation request logging. Invalid cwd text is
    /// represented only by booleans and never copied into evidence.
    pub fn audit_summary(&self) -> Value {
        let resource = Self {
            default_cwd: None,
            default_shell: None,
            resource: self.resource.clone(),
        }
        .validated()
        .ok()
        .and_then(|context| context.resource);
        let cwd = Self {
            default_cwd: self.default_cwd.clone(),
            default_shell: None,
            resource: resource.clone(),
        }
        .validated()
        .ok()
        .and_then(|context| context.default_cwd);
        serde_json::json!({
            "default_cwd": cwd,
            "default_cwd_present": self.default_cwd.is_some(),
            "default_cwd_valid": self.default_cwd.is_none() || cwd.is_some(),
            "default_shell": self.default_shell,
            "resource": resource,
            "resource_present": self.resource.is_some(),
            "resource_valid": self.resource.is_none() || resource.is_some(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageKind {
    Note,
    Proposal,
    Question,
    Answer,
    Decision,
    Risk,
    Progress,
    Guidance,
    Todo,
}

impl SessionMessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Proposal => "proposal",
            Self::Question => "question",
            Self::Answer => "answer",
            Self::Decision => "decision",
            Self::Risk => "risk",
            Self::Progress => "progress",
            Self::Guidance => "guidance",
            Self::Todo => "todo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessagePriority {
    Low,
    #[default]
    Normal,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionOutcome {
    AutoApproved,
    Approved,
    Denied,
    Pending,
    HardDenied,
}

impl PermissionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoApproved => "auto_approved",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Pending => "requested",
            Self::HardDenied => "hard_denied",
        }
    }

    pub fn allows_execution(self) -> bool {
        matches!(self, Self::AutoApproved | Self::Approved)
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto_approved" => Some(Self::AutoApproved),
            "approved" => Some(Self::Approved),
            "denied" | "expired" => Some(Self::Denied),
            "requested" | "pending" => Some(Self::Pending),
            "hard_denied" => Some(Self::HardDenied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionDecision {
    pub required: bool,
    pub policy: String,
    pub request_id: String,
    pub status: String,
    pub reason: String,
    pub risk: String,
    pub tool_name: String,
    pub project: Option<String>,
}

impl PermissionDecision {
    pub fn outcome(&self) -> Option<PermissionOutcome> {
        PermissionOutcome::parse(&self.status)
    }

    pub fn allows_execution(&self) -> bool {
        self.outcome()
            .map(PermissionOutcome::allows_execution)
            .unwrap_or(false)
    }
}

pub fn is_safe_job_id(job_id: &str) -> bool {
    if job_id.is_empty() || job_id.len() > 80 || job_id.contains("..") {
        return false;
    }
    job_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}
