//! Runtime tool call wire/data model and behavioral helpers.
//!
//! This module owns the model-visible tool call enum, parsing by runtime tool
//! name, and the project/session accessors used by dispatch guards and audit
//! logging.

#[cfg(feature = "workspace-checkpoints")]
use super::tool_inputs::CheckpointValidationInput;
use super::tool_inputs::{
    default_true, ApplyFileChangeInput, CodingGuidanceProfile, ExecutionPurpose, ExecutionShell,
    GoalLifecycleInput, SessionMode, WorkOnProjectMode,
};
use crate::{lookup_tool_definition, model_visible_tool_names_csv};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use webcodex_core::apply_patch_shared::ApplyPatchMatchingMode;
use webcodex_core::job_observation::{
    ObservationRefRegistry, MAX_JOB_OBSERVATION_TOKEN_LEN, MAX_OBSERVATION_REF_LEN,
};
use webcodex_core::lsp_bridge::{
    CallHierarchyDirection, DEFAULT_CALL_HIERARCHY_DEPTH, DEFAULT_CALL_HIERARCHY_LIMIT,
};
use webcodex_core::plugin::{
    validate_json_value as validate_plugin_json_value,
    validate_provider_id as validate_plugin_provider_id,
    validate_tool_name as validate_plugin_tool_name, PLUGIN_MAX_ARGUMENT_BYTES,
};
use webcodex_core::runner_protocol::{
    normalize_cargo_packages, ShellScriptLanguage, CARGO_PACKAGE_MAX_ITEMS, CARGO_VALUE_MAX_BYTES,
};
use webcodex_core::runtime_contract::{
    validate_project_op_path, DEFAULT_OBSERVE_JOBS_TAIL_LINES,
    GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES,
};
use webcodex_core::workflow_session_contract::{
    strip_tool_call_expectation_metadata, validate_model_facing_assertion_name,
    validate_model_facing_result_expectation, SessionExecutionContext, SessionMessageKind,
    SessionMessagePriority, SessionMessageStatus,
};

pub const TOOL_CALL_TOOL_FIELD: &str = "tool";
pub const TOOL_CALL_PARAMS_FIELD: &str = "params";
pub const TOOL_CALL_WRAPPER_FIELDS: &[&str] = &[TOOL_CALL_TOOL_FIELD, TOOL_CALL_PARAMS_FIELD];

/// Compact model-facing form of one exact Agent continuation tuple.
/// The server stores the mapping. This text is not a credential.
pub const AGENT_CONTINUATION_REF_PATTERN: &str = "^~ac[1-9][0-9]{0,18}$";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PluginToolAction {
    List,
    Check,
    Reload,
    Describe,
    Call,
}

impl PluginToolAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Check => "check",
            Self::Reload => "reload",
            Self::Describe => "describe",
            Self::Call => "call",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PluginToolCall {
    /// Gateway operation. Discovery starts from an exact caller-visible Runner; call uses only an
    /// opaque binding from describe.
    pub action: PluginToolAction,
    /// Exact caller-visible Runner client_id. Required for check/reload/describe and for Runner-scoped
    /// list operations.
    #[schemars(length(min = 1, max = 128))]
    #[serde(default)]
    pub runner: Option<String>,
    /// Logical Plugin provider id on the selected exact Runner.
    #[schemars(length(min = 1, max = 64))]
    #[serde(default)]
    pub plugin: Option<String>,
    /// Logical provider-local Plugin tool name. Provider tools never become outer WebCodex MCP tool
    /// names.
    #[schemars(length(min = 1, max = 128))]
    #[serde(default)]
    pub tool: Option<String>,
    /// Opaque exact Runner/provider/tool/schema binding returned by describe. It is observation
    /// identity, not authority.
    #[schemars(length(min = 1, max = 128))]
    #[schemars(regex(pattern = "^wc_pbind_[A-Za-z0-9_-]{21}[AQgw]$"))]
    #[serde(default)]
    pub binding: Option<String>,
    /// Plugin tool arguments matching the schema observed by describe; encoded payload is bounded to
    /// 65536 bytes.
    #[serde(default)]
    pub arguments: Option<Value>,
}

impl PluginToolCall {
    fn validate(&self) -> Result<(), String> {
        let valid_runner = |runner: &str| {
            !runner.trim().is_empty()
                && runner.len() <= 128
                && !runner.chars().any(char::is_control)
        };
        if self
            .runner
            .as_deref()
            .is_some_and(|runner| !valid_runner(runner))
        {
            return Err("runner must be a bounded non-empty exact Runner client id".to_string());
        }
        if let Some(plugin) = self.plugin.as_deref() {
            validate_plugin_provider_id(plugin)
                .map_err(|_| "plugin must be a valid bounded provider id".to_string())?;
        }
        if let Some(tool) = self.tool.as_deref() {
            validate_plugin_tool_name(tool)
                .map_err(|_| "tool must be a valid bounded provider-local tool name".to_string())?;
        }
        if let Some(binding) = self.binding.as_deref() {
            let Some(random) = binding.strip_prefix("wc_pbind_") else {
                return Err("binding must be a valid opaque Plugin binding".to_string());
            };
            if webcodex_core::compact::decode::<16>(random).is_none() {
                return Err("binding must be a valid opaque Plugin binding".to_string());
            }
        }
        if let Some(arguments) = self.arguments.as_ref() {
            if !arguments.is_object() {
                return Err("arguments must be a JSON object".to_string());
            }
            validate_plugin_json_value(arguments, PLUGIN_MAX_ARGUMENT_BYTES, "Plugin arguments")
                .map_err(|_| "arguments exceed Plugin bounds".to_string())?;
        }

        match self.action {
            PluginToolAction::List => {
                if self.tool.is_some() || self.binding.is_some() || self.arguments.is_some() {
                    return Err("action=list accepts only optional runner and plugin".to_string());
                }
                if self.plugin.is_some() && self.runner.is_none() {
                    return Err("action=list requires runner when plugin is provided".to_string());
                }
            }
            PluginToolAction::Check => {
                if self.runner.is_none()
                    || self.plugin.is_none()
                    || self.tool.is_some()
                    || self.binding.is_some()
                    || self.arguments.is_some()
                {
                    return Err("action=check requires only runner and plugin".to_string());
                }
            }
            PluginToolAction::Reload => {
                if self.runner.is_none()
                    || self.plugin.is_some()
                    || self.tool.is_some()
                    || self.binding.is_some()
                    || self.arguments.is_some()
                {
                    return Err("action=reload requires only runner".to_string());
                }
            }
            PluginToolAction::Describe => {
                if self.runner.is_none()
                    || self.plugin.is_none()
                    || self.tool.is_none()
                    || self.binding.is_some()
                    || self.arguments.is_some()
                {
                    return Err(
                        "action=describe requires only runner, plugin, and tool".to_string()
                    );
                }
            }
            PluginToolAction::Call => {
                if self.binding.is_none()
                    || self.arguments.is_none()
                    || self.runner.is_some()
                    || self.plugin.is_some()
                    || self.tool.is_some()
                {
                    return Err("action=call requires only binding and arguments".to_string());
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SshResourceToolCall {
    /// Managed SSH resource operation. List first to obtain the exact Runner/revision binding required
    /// by register/remove.
    pub action: String,
    /// Exact caller-visible Runner client_id. Required only for list.
    #[schemars(length(min = 1, max = 128))]
    #[serde(default)]
    pub runner: Option<String>,
    /// Opaque exact Runner + registry revision observation returned by list. Required for
    /// register/remove; never grants authority by itself.
    #[schemars(regex(pattern = "^wc_sbind_[A-Za-z0-9_-]{21}[AQgw]$"))]
    #[serde(default)]
    pub binding: Option<String>,
    /// Logical Runner-local SSH resource name. Required for register/remove.
    #[schemars(length(min = 1, max = 80))]
    #[schemars(regex(pattern = "^[A-Za-z0-9_.-]+$"))]
    #[serde(default)]
    pub name: Option<String>,
    /// Single OpenSSH destination argv. Required only for register. It is persisted on the Runner and
    /// never echoed. Options or credential material are not accepted.
    #[schemars(length(min = 1, max = 512))]
    #[serde(default)]
    pub target: Option<String>,
    /// Optional remote default cwd for register.
    #[schemars(length(min = 1, max = 4096))]
    #[serde(default)]
    pub default_cwd: Option<String>,
}

impl SshResourceToolCall {
    pub fn validate(&self) -> Result<(), String> {
        // Only classify the closed action vocabulary before specialized
        // governance. Action-specific identity/value validation remains in the
        // SSH gateway after scope/session/permission checks so an unauthorized
        // caller cannot learn whether a binding, Runner, name, or target is valid.
        if matches!(self.action.as_str(), "list" | "register" | "remove") {
            Ok(())
        } else {
            Err("action must be one of list, register, or remove".to_string())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchResultMode {
    Matches,
    FilesWithMatches,
    Count,
}

impl SearchResultMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Matches => "matches",
            Self::FilesWithMatches => "files_with_matches",
            Self::Count => "count",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchPatternMode {
    Regex,
    Literal,
}

impl SearchPatternMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Regex => "regex",
            Self::Literal => "literal",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReadFilesItem {
    /// Non-empty project-relative file path.
    #[schemars(length(min = 1))]
    #[serde(deserialize_with = "deserialize_non_empty_read_path")]
    pub path: String,
    /// Optional 1-based line offset; normalized by the canonical file-read range rules.
    #[serde(default)]
    pub start_line: Option<usize>,
    /// Optional maximum line count; normalized by the canonical file-read range rules.
    #[serde(default)]
    pub limit: Option<usize>,
    #[schemars(range(min = 1, max = 9007199254740991u64))]
    /// Optional full-file snapshot fence for this exact Project/path. Normal callers should not invent
    /// or manually transfer this value: Runtime places it in parser-ready read_files suggested_call
    /// items when a partial range must continue. If supplied, Runtime rejects the item when that
    /// snapshot is no longer current.
    #[serde(default, deserialize_with = "deserialize_optional_read_revision")]
    pub expected_read_revision: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SearchProjectTextsQuery {
    /// Search pattern. Interpreted as a regular expression by default. For identifiers, source
    /// snippets, paths, and other exact text, prefer pattern_mode=literal; use regex when regex syntax
    /// is intentional.
    #[schemars(length(min = 1))]
    pub pattern: String,
    #[schemars(extend("default" = "regex"))]
    /// Pattern interpretation: regex (default, backward compatible) or literal for exact text. Prefer
    /// literal unless regex syntax is intentional.
    #[serde(default)]
    pub pattern_mode: Option<SearchPatternMode>,
    /// Optional project-relative directory to scope the search (default: project root).
    #[serde(default)]
    pub path: Option<String>,
    /// Maximum records to return: matches in matches mode, files in files_with_matches/count modes.
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional number of context lines before each match (clamped to 80).
    #[serde(default)]
    pub context_before: Option<usize>,
    /// Optional number of context lines after each match (clamped to 80).
    #[serde(default)]
    pub context_after: Option<usize>,
    /// Optional ripgrep include globs. At most 32 entries of 1..256 bytes; negated and protected-path
    /// globs are rejected.
    #[schemars(length(max = 32))]
    #[schemars(inner(length(min = 1, max = 256)))]
    #[serde(default)]
    pub include_globs: Option<Vec<String>>,
    /// Optional additive ripgrep exclude globs. Built-in secret/build excludes always remain active.
    #[schemars(length(max = 32))]
    #[schemars(inner(length(min = 1, max = 256)))]
    #[serde(default)]
    pub exclude_globs: Option<Vec<String>>,
    #[schemars(extend("default" = "matches"))]
    /// Result shape: matches (default), files_with_matches, or count.
    #[serde(default)]
    pub result_mode: Option<SearchResultMode>,
    #[schemars(extend("default" = 30))]
    /// Per-query execution ceiling in seconds. Server clamps the value to 1..120 (default 30).
    /// A search_project_texts batch also shares one Server-owned absolute latency deadline, so the
    /// effective query timeout is the smaller of this ceiling and the remaining batch budget. Queued
    /// queries, bounded retries, and diagnostics do not reserve or reset that outer deadline.
    #[serde(default)]
    pub timeout_secs: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObserveJobsItem {
    /// Existing opaque runtime Job id. Required unless `observation_ref` is supplied.
    /// The empty string is used only as the serde default for an unresolved ref selector and
    /// never reaches canonical Job observation.
    #[schemars(length(min = 1))]
    #[serde(
        default,
        deserialize_with = "deserialize_non_empty_job_id",
        skip_serializing_if = "String::is_empty"
    )]
    pub job_id: String,
    /// Optional opaque Job-bound lifecycle/log-delta token from the latest observation. Return it
    /// unchanged without interpreting its cursor state. It is not execution identity or retry
    /// authority; a stale Server epoch is immediately actionable and conservatively resets the bounded
    /// log projection.
    #[schemars(length(max = 62))]
    #[serde(default, deserialize_with = "deserialize_optional_observation_token")]
    pub after_observation_token: Option<String>,
    /// Compact server-issued continuation selector for one exact prior Job observation state.
    /// Mutually exclusive with a non-empty `job_id`; when supplied, callers must not also supply
    /// `after_observation_token`. Unknown or expired refs fail closed at observation time.
    #[schemars(length(min = 3, max = 22))]
    #[serde(
        default,
        deserialize_with = "deserialize_optional_observation_ref",
        skip_serializing_if = "Option::is_none"
    )]
    pub observation_ref: Option<String>,
}

impl ObserveJobsItem {
    pub fn resolved(job_id: String, after_observation_token: Option<String>) -> Self {
        Self {
            job_id,
            after_observation_token,
            observation_ref: None,
        }
    }
}

/// Which observable changes may end a bounded batch Job wait early.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObserveJobsWakeOn {
    #[default]
    Change,
    Terminal,
    AllTerminal,
}

fn deserialize_non_empty_read_path<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let path = String::deserialize(deserializer)?;
    if path.trim().is_empty() {
        return Err(serde::de::Error::custom("path must not be empty"));
    }
    Ok(path)
}

fn deserialize_optional_read_revision<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let revision = u64::deserialize(deserializer)?;
    if !(1..=9_007_199_254_740_991_u64).contains(&revision) {
        return Err(serde::de::Error::custom(
            "expected_read_revision must be a positive JSON-safe integer",
        ));
    }
    Ok(Some(revision))
}

fn deserialize_non_empty_job_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let job_id = String::deserialize(deserializer)?;
    if job_id.trim().is_empty() {
        return Err(serde::de::Error::custom("job_id must not be empty"));
    }
    Ok(job_id)
}

fn deserialize_optional_observation_ref<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    if let Some(value) = value.as_deref() {
        if !ObservationRefRegistry::is_ref_syntax(value) {
            return Err(serde::de::Error::custom(
                "observation_ref must use compact ~j<decimal> syntax",
            ));
        }
    }
    Ok(value)
}

fn deserialize_optional_observation_token<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let token = Option::<String>::deserialize(deserializer)?;
    if token
        .as_ref()
        .is_some_and(|token| token.len() > MAX_JOB_OBSERVATION_TOKEN_LEN)
    {
        return Err(serde::de::Error::custom(format!(
            "after_observation_token must not exceed {MAX_JOB_OBSERVATION_TOKEN_LEN} bytes"
        )));
    }
    Ok(token)
}

fn deserialize_optional_git_diff_hunks_continuation<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let token = Option::<String>::deserialize(deserializer)?;
    if token
        .as_ref()
        .is_some_and(|token| token.len() > GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES)
    {
        return Err(serde::de::Error::custom(
            "continuation exceeds the git_diff_hunks size bound",
        ));
    }
    Ok(token)
}

fn deserialize_read_files_items<'de, D>(deserializer: D) -> Result<Vec<ReadFilesItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let items = Vec::<ReadFilesItem>::deserialize(deserializer)?;
    if !(1..=8).contains(&items.len()) {
        return Err(serde::de::Error::custom(
            "items must contain between 1 and 8 entries",
        ));
    }
    Ok(items)
}

fn deserialize_observe_jobs_items<'de, D>(deserializer: D) -> Result<Vec<ObserveJobsItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let items = Vec::<ObserveJobsItem>::deserialize(deserializer)?;
    if !(1..=8).contains(&items.len()) {
        return Err(serde::de::Error::custom(
            "items must contain between 1 and 8 entries",
        ));
    }
    let mut selectors = HashSet::with_capacity(items.len());
    for item in &items {
        let has_job_id = !item.job_id.is_empty();
        match (has_job_id, item.observation_ref.as_deref()) {
            (false, None) => {
                return Err(serde::de::Error::custom(
                    "each observe_jobs item must supply either job_id or observation_ref",
                ));
            }
            (true, Some(_)) => {
                return Err(serde::de::Error::custom(
                    "observation_ref and job_id are mutually exclusive in the same item",
                ));
            }
            (false, Some(_)) if item.after_observation_token.is_some() => {
                return Err(serde::de::Error::custom(
                    "after_observation_token must be absent when observation_ref is supplied",
                ));
            }
            _ => {}
        }
        let selector = item
            .observation_ref
            .as_deref()
            .unwrap_or(item.job_id.as_str());
        if !selectors.insert(selector) {
            return Err(serde::de::Error::custom(format!(
                "duplicate selector in observe_jobs items: {selector}"
            )));
        }
    }
    Ok(items)
}

fn observe_jobs_items_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "minItems": 1,
        "maxItems": 8,
        "items": {
            "oneOf": [
                {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "job_id": {
                            "type": "string",
                            "minLength": 1,
                            "description": "Existing opaque runtime Job id."
                        },
                        "after_observation_token": {
                            "anyOf": [
                                {"type": "string", "maxLength": MAX_JOB_OBSERVATION_TOKEN_LEN},
                                {"type": "null"}
                            ],
                            "description": "Optional opaque Job-bound lifecycle/log-delta token from the latest observation. Return it unchanged without interpreting its cursor state. It is not execution identity or retry authority; a stale Server epoch resets the bounded log projection."
                        },
                        "observation_ref": {"type": "null"}
                    },
                    "required": ["job_id"]
                },
                {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "after_observation_token": {"type": "null"},
                        "observation_ref": {
                            "type": "string",
                            "pattern": "^~j[0-9]+$",
                            "minLength": 3,
                            "maxLength": MAX_OBSERVATION_REF_LEN,
                            "description": "Compact server-issued continuation selector for one exact prior Job observation state. Unknown or expired refs fail closed."
                        }
                    },
                    "required": ["observation_ref"]
                }
            ]
        }
    })
}

fn deserialize_observe_jobs_tail_lines<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let tail_lines = usize::deserialize(deserializer)?;
    if tail_lines == 0 {
        return Err(serde::de::Error::custom("tail_lines must be at least 1"));
    }
    Ok(tail_lines)
}

fn deserialize_observe_jobs_wait_secs<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let wait_secs = Option::<u64>::deserialize(deserializer)?;
    if wait_secs == Some(0) {
        return Err(serde::de::Error::custom("wait_secs must be at least 1"));
    }
    Ok(wait_secs)
}

fn default_observe_jobs_tail_lines() -> usize {
    DEFAULT_OBSERVE_JOBS_TAIL_LINES
}

fn default_call_hierarchy_depth() -> usize {
    DEFAULT_CALL_HIERARCHY_DEPTH
}

fn default_call_hierarchy_limit() -> usize {
    DEFAULT_CALL_HIERARCHY_LIMIT
}

fn deserialize_search_project_texts_queries<'de, D>(
    deserializer: D,
) -> Result<Vec<SearchProjectTextsQuery>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let queries = Vec::<SearchProjectTextsQuery>::deserialize(deserializer)?;
    if !(1..=8).contains(&queries.len()) {
        return Err(serde::de::Error::custom(
            "queries must contain between 1 and 8 entries",
        ));
    }
    Ok(queries)
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAiHostFileRef {
    pub download_url: String,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub file_name: Option<String>,
}

/// Adapter-derived provenance for host file references. This is deliberately
/// skipped by serde on ToolCall: caller/model JSON can never grant either trust
/// path, and the two host mechanisms cannot impersonate one another.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, JsonSchema)]
pub enum HostFileImportProvenance {
    #[default]
    Untrusted,
    GptActionOpenAiHost,
    /// Authenticated MCP OAuth client that may import only from OpenAI file hosts.
    AuthenticatedMcpOpenAiHostFile,
    /// Explicitly allowlisted MCP client that may import from arbitrary public HTTPS.
    TrustedMcpHostFile,
}

impl HostFileImportProvenance {
    pub fn is_trusted(self) -> bool {
        !matches!(self, Self::Untrusted)
    }
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserSnapshotModeCall {
    #[default]
    Auto,
    Full,
    Interactive,
}

impl BrowserSnapshotModeCall {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Full => "full",
            Self::Interactive => "interactive",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserObserveToolCall {
    Targets,
    Browsers {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
    },
    Pages {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(range(min = 1, max = 32))]
        #[serde(default)]
        limit: Option<usize>,
    },
    Snapshot {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[serde(default)]
        mode: BrowserSnapshotModeCall,
        #[schemars(range(min = 1, max = 256))]
        #[serde(default)]
        max_nodes: Option<usize>,
        #[schemars(range(min = 1, max = 32))]
        #[serde(default)]
        max_depth: Option<u32>,
    },
    Console {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
    },
    Network {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
    },
    Diagnostics {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[serde(default)]
        include_all_console: bool,
        #[serde(default)]
        include_all_network: bool,
        #[serde(default)]
        since_cursor: Option<u64>,
    },
    Screenshot {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
    },
}

impl BrowserObserveToolCall {
    pub const fn action_name(&self) -> &'static str {
        match self {
            Self::Targets => "targets",
            Self::Browsers { .. } => "browsers",
            Self::Pages { .. } => "pages",
            Self::Snapshot { .. } => "snapshot",
            Self::Console { .. } => "console",
            Self::Network { .. } => "network",
            Self::Diagnostics { .. } => "diagnostics",
            Self::Screenshot { .. } => "screenshot",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserKeyCall {
    Enter,
    Tab,
    Escape,
    Backspace,
    Delete,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
}

impl BrowserKeyCall {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Tab => "tab",
            Self::Escape => "escape",
            Self::Backspace => "backspace",
            Self::Delete => "delete",
            Self::ArrowUp => "arrow_up",
            Self::ArrowDown => "arrow_down",
            Self::ArrowLeft => "arrow_left",
            Self::ArrowRight => "arrow_right",
            Self::Home => "home",
            Self::End => "end",
            Self::PageUp => "page_up",
            Self::PageDown => "page_down",
            Self::Space => "space",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserActToolCall {
    Launch {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
    },
    NewPage {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
    },
    Navigate {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 8192))]
        #[schemars(regex(pattern = "^https?://"))]
        url: String,
    },
    Reload {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
    },
    Click {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
    },
    InputText {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        #[schemars(length(min = 1, max = 4096))]
        text: String,
    },
    SelectOption {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        /// Exact native option value or trimmed visible option text.
        #[schemars(length(min = 1, max = 4096))]
        option: String,
    },
    SetValue {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        /// Exact native form-control value, for example 2027-06 for input[type=month].
        #[schemars(length(min = 1, max = 4096))]
        value: String,
    },
    UploadFile {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        /// Authorized Runner project containing the file to upload.
        #[schemars(length(min = 1, max = 512))]
        project: String,
        /// Project-relative path to one existing regular file.
        #[schemars(length(min = 1, max = 4096))]
        path: String,
    },
    Key {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        key: BrowserKeyCall,
    },
    ClearDiagnostics {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
    },
    ClosePage {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
    },
    CloseBrowser {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
    },
}

impl BrowserActToolCall {
    pub const fn action_name(&self) -> &'static str {
        match self {
            Self::Launch { .. } => "launch",
            Self::NewPage { .. } => "new_page",
            Self::Navigate { .. } => "navigate",
            Self::Reload { .. } => "reload",
            Self::Click { .. } => "click",
            Self::InputText { .. } => "input_text",
            Self::SelectOption { .. } => "select_option",
            Self::SetValue { .. } => "set_value",
            Self::UploadFile { .. } => "upload_file",
            Self::Key { .. } => "key",
            Self::ClearDiagnostics { .. } => "clear_diagnostics",
            Self::ClosePage { .. } => "close_page",
            Self::CloseBrowser { .. } => "close_browser",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComputerObserveToolCall {
    Targets,
    Windows {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },
    Displays {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },
    Applications {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },
    AccessibilityStatus {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
    },
    AccessibilityTree {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(range(min = 0))]
        #[serde(default)]
        max_depth: Option<usize>,
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_nodes: Option<usize>,
    },
    FindElements {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(length(min = 1, max = 256))]
        #[serde(default)]
        role: Option<String>,
        #[schemars(length(min = 1, max = 256))]
        #[serde(default)]
        subrole: Option<String>,
        #[schemars(length(min = 1, max = 256))]
        #[serde(default)]
        label: Option<String>,
        #[serde(default)]
        focused: Option<bool>,
        #[serde(default)]
        enabled: Option<bool>,
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },
    ElementState {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(length(min = 1, max = 128))]
        element_id: String,
    },
    SnapshotWindow {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[serde(default)]
        region: Option<ComputerSnapshotRegion>,
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_width: Option<u32>,
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_height: Option<u32>,
    },
    SnapshotDisplay {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(max = 128))]
        #[schemars(regex(pattern = "^display_[A-Za-z0-9_-]{16}$"))]
        display_id: String,
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_width: Option<u32>,
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_height: Option<u32>,
    },
    ReadClipboard {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
    },
}

impl ComputerObserveToolCall {
    pub const fn action_name(&self) -> &'static str {
        match self {
            Self::Targets => "targets",
            Self::Windows { .. } => "windows",
            Self::Displays { .. } => "displays",
            Self::Applications { .. } => "applications",
            Self::AccessibilityStatus { .. } => "accessibility_status",
            Self::AccessibilityTree { .. } => "accessibility_tree",
            Self::FindElements { .. } => "find_elements",
            Self::ElementState { .. } => "element_state",
            Self::SnapshotWindow { .. } => "snapshot_window",
            Self::SnapshotDisplay { .. } => "snapshot_display",
            Self::ReadClipboard { .. } => "read_clipboard",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComputerControlToolCall {
    LaunchApplication {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(max = 128))]
        #[schemars(regex(pattern = "^application_[A-Za-z0-9_-]{16}$"))]
        application_id: String,
    },
    ActivateWindow {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
    },
    Press {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(length(min = 1, max = 128))]
        element_id: String,
    },
    Focus {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(length(min = 1, max = 128))]
        element_id: String,
    },
    ScrollToElement {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(length(min = 1, max = 128))]
        element_id: String,
    },
    Key {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        key: String,
        #[schemars(length(max = 4))]
        #[serde(default)]
        modifiers: Option<Vec<String>>,
    },
    InputText {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        #[schemars(length(min = 1, max = 128))]
        element_id: String,
        #[schemars(length(min = 1, max = 2048))]
        text: String,
    },
    PointerMove {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(max = 128))]
        #[schemars(regex(pattern = "^display_[A-Za-z0-9_-]{16}$"))]
        display_id: String,
        #[schemars(range(min = 1, max = 4294967295u64))]
        snapshot_generation: u32,
        #[schemars(range(min = 0, max = 4294967295u64))]
        x: u32,
        #[schemars(range(min = 0, max = 4294967295u64))]
        y: u32,
    },
    PointerClick {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(max = 128))]
        #[schemars(regex(pattern = "^display_[A-Za-z0-9_-]{16}$"))]
        display_id: String,
        #[schemars(range(min = 1, max = 4294967295u64))]
        snapshot_generation: u32,
        #[schemars(range(min = 0, max = 4294967295u64))]
        x: u32,
        #[schemars(range(min = 0, max = 4294967295u64))]
        y: u32,
    },
    WriteClipboard {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(length(min = 1, max = 16384))]
        text: String,
    },
}

impl ComputerControlToolCall {
    pub const fn action_name(&self) -> &'static str {
        match self {
            Self::LaunchApplication { .. } => "launch_application",
            Self::ActivateWindow { .. } => "activate_window",
            Self::Press { .. } => "press",
            Self::Focus { .. } => "focus",
            Self::ScrollToElement { .. } => "scroll_to_element",
            Self::Key { .. } => "key",
            Self::InputText { .. } => "input_text",
            Self::PointerMove { .. } => "pointer_move",
            Self::PointerClick { .. } => "pointer_click",
            Self::WriteClipboard { .. } => "write_clipboard",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ComputerSnapshotRegion {
    #[schemars(range(min = 0, max = 4294967295u64))]
    pub x: u32,
    #[schemars(range(min = 0, max = 4294967295u64))]
    pub y: u32,
    #[schemars(range(min = 1, max = 4294967295u64))]
    pub width: u32,
    #[schemars(range(min = 1, max = 4294967295u64))]
    pub height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GoalStepInputCall {
    /// Stable step id, fixed for the lifetime of this Goal plan.
    #[schemars(regex(pattern = "^[A-Za-z0-9_-]{1,32}$"))]
    pub id: String,
    /// Bounded plan milestone, not a Task instruction or execution selector.
    #[schemars(length(min = 1, max = 120))]
    pub title: String,
}

fn goal_conditions_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array", "maxItems": 8,
        "items": {"type": "string", "minLength": 1, "maxLength": 512}
    })
}

fn goal_step_ids_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array", "maxItems": 32, "uniqueItems": true,
        "items": {"type": "string", "pattern": "^[A-Za-z0-9_-]{1,32}$"}
    })
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AgentWaitEventSelectorCall {
    /// Durable Agent Wait v1 supports only authoritative AgentTask terminal facts.
    pub kind: String,
    /// Exact independently authorized AgentTask source. This reference grants no Task, Project, Goal,
    /// Session, or execution authority.
    #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
    pub task_id: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentWaitModeCall {
    /// Trigger when any registered exact source Task becomes terminal.
    #[default]
    Any,
    /// Trigger only after every registered exact source Task is terminal.
    All,
}

impl AgentWaitModeCall {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::All => "all",
        }
    }
}

fn agent_wait_mode_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "enum": ["any", "all"],
        "default": "any"
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectArtifactAction {
    Metadata,
    Inspect,
    Image,
    Export,
}

impl ProjectArtifactAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Inspect => "inspect",
            Self::Image => "image",
            Self::Export => "export",
        }
    }
}

fn nullable_stdin_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "anyOf": [
            {"type": "string", "maxLength": 65536},
            {"type": "null"}
        ]
    })
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "tool",
    content = "params",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ToolCall {
    /// List registered tool runtime tools.
    ListTools {
        /// Optional tool_manifest category filter such as artifact, edit, session, git, or runtime.
        #[serde(default)]
        category: Option<String>,
        /// Optional loose feature filter such as artifact_upload, upload, read, edit, session, git, or
        /// validation.
        #[serde(default)]
        features: Option<String>,
        /// When true, omit full input/output schemas and return compact tool summaries.
        #[serde(default)]
        summary_only: bool,
        /// Maximum returned tools for focused discovery; capped at 256.
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Create a bounded task tracking session and return an explicit opaque
    /// session id. Later callers should pass that id explicitly (for example as
    /// REST `recording_session_id` wrapper metadata, tool-specific
    /// `session_id`, or MCP `_session_id`) or bind it as current separately.
    StartSession {
        #[serde(default)]
        project: Option<String>,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        mode: SessionMode,
        #[serde(default)]
        deny_write_tools: bool,
        #[serde(default)]
        deny_shell_tools: bool,
        #[serde(default)]
        execution_context: Option<SessionExecutionContext>,
    },

    /// Start a normal coding workflow with practical defaults, or continue one
    /// by `session_id`. This is the canonical coding entry and returns only a
    /// compact startup projection.
    /// `session_id` is explicit business input for the exact Workflow Session
    /// to continue; omission always creates a fresh Session.
    WorkOnProject {
        /// Existing Runtime Project selector. Prefer a Server-issued project_ref from prior bootstrap or
        /// discovery; canonical agent:<client_id>:<project_id>, client_id:project_id, and existing human
        /// convenience forms remain accepted. Use project + instruction; do not combine with client_id/path.
        #[schemars(length(min = 1))]
        #[serde(default)]
        project: String,
        /// Runner client_id for the path form. Use client_id + path + instruction together; do not combine
        /// with project.
        #[schemars(length(min = 1))]
        #[serde(default)]
        client_id: Option<String>,
        /// Runner-owned absolute directory path for the path form. In checkout mode it is the working
        /// checkout to resolve/register; in worktree mode it is the source Git checkout. Use with client_id
        /// + instruction; do not combine with project. Runner filesystem authority remains authoritative.
        #[schemars(length(min = 1))]
        #[serde(default)]
        path: Option<String>,
        #[schemars(extend("default" = "checkout"))]
        #[schemars(with = "Option<WorkOnProjectMode>")]
        /// Optional bootstrap mode. Omitted or checkout preserves existing behavior exactly. worktree is
        /// supported only with client_id + path and asks the Runner to create/recover an isolated managed
        /// detached worktree, register it as an ordinary Project, then start the Workflow Session.
        #[serde(default)]
        mode: Option<String>,
        /// Optional Git ref only for mode=worktree. The Runner resolves it inside the source repository to
        /// an exact commit SHA before creating the detached worktree. On a fresh bootstrap omission means
        /// the source checkout's current HEAD; on exact session resume omission keeps the registered
        /// managed worktree's stored exact base. The Server never interprets this ref.
        #[schemars(length(min = 1, max = 1024))]
        #[serde(default)]
        base_ref: Option<String>,
        /// Required current user instruction. On a new task it becomes the root task title; when session_id
        /// is provided it is appended to the existing Workflow Session ledger and never overwrites the root
        /// title.
        #[schemars(length(min = 1, max = 4000))]
        instruction: String,
        /// Model guidance only: direct (default), host_code_mode for Host-native orchestration,
        /// or feature-gated code_mode for WebCodex nested orchestration. No tool admission,
        /// authority, effects, or Session state changes; explicit resume may choose again.
        /// Request
        /// `context_request=["webcodex.workflow"]` when the current model context needs that guidance.
        #[serde(default)]
        guidance_profile: CodingGuidanceProfile,
        #[schemars(extend("default" = true))]
        /// Whether startup should include a small bounded Skills/Plugins selection catalog. Defaults to
        /// true. Set false only when the caller's current model context already retains the relevant
        /// extension metadata. False skips the startup Skill/Plugin discovery observations. The catalog
        /// grants no authority, never loads Skill bodies, never creates Plugin bindings, and never
        /// substitutes for plugin_tool describe before invocation.
        #[serde(default = "default_true")]
        include_extension_catalog: bool,
        /// Optional explicit Workflow Session to continue exactly. It must be active and accessible and
        /// remains bound to its exact final Project; in worktree mode the Runner re-observes that
        /// registered managed Project and its source provenance instead of creating a second worktree.
        /// Failure never guesses or creates a replacement Session. Supplying session_id does not prove this
        /// model context still retains project instructions, workflow guidance, or extension metadata. A
        /// fresh model context should request missing static guidance through context_request. This business
        /// input is distinct from wrapper recording_session_id.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Return deterministic finish context for an explicit task session:
    /// changes, workspace hygiene, session/handoff summaries, and bounded
    /// validation-like ledger events. Never calls an LLM.
    FinishCodingTask {
        /// Required runtime project id. Use the same project used to start the task.
        project: String,
        /// Required explicit wc_sess_* business Session id for the current coding task, obtained from its
        /// compatible Session bootstrap.
        session_id: String,
        /// When true, return the minimal decision-complete closeout only: workspace cleanliness/conflicts,
        /// hygiene state, bounded Job counts, final validation state/counts, tool-failure actionability
        /// counts, canonical task_outcome, evidence_integrity, warnings, and suggested_next_actions. Omits
        /// project/session identity, permissions, review/work/change/handoff provenance, facts/evidence
        /// history/informational notes, command text, stdout/stderr, event history, tails, excerpts, and
        /// detailed validation history.
        #[serde(default)]
        summary_only: bool,
        /// Include bounded diff hunks in show_changes. Defaults to true.
        #[serde(default)]
        include_diff: Option<bool>,
        /// Defaults to true. When include_handoff=true, controls whether the nested handoff summary
        /// includes its workspace block; the top-level finish workspace/show_changes check remains
        /// unchanged.
        #[serde(default)]
        include_workspace: Option<bool>,
        /// Include workspace_hygiene_check output. Defaults to true.
        #[serde(default)]
        include_hygiene: Option<bool>,
        /// Include session_handoff_summary output. Defaults to true.
        #[serde(default)]
        include_handoff: Option<bool>,
        /// Include deterministic validation-like session ledger event summary when available. Defaults to
        /// true; minimal diagnostics require bounded tails or safe result metadata.
        #[serde(default)]
        include_validation_summary: Option<bool>,
    },

    /// Explicitly present one persistent card for the current client Window.
    /// Project authority is re-checked on every refresh; Workflow Session linkage is optional evidence.
    PresentWorkResult {
        /// Required exact runtime Project input. It is independently resolved and authorized on every call.
        #[schemars(length(min = 1, max = 512))]
        project: String,
        /// Optional exact project-scoped Workflow Session association for compatibility.
        /// Omit it when the Window has not created or resumed a Workflow Session.
        #[serde(default)]
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: Option<String>,
    },

    /// App-only read of the same Window card projection. Optional Session identity
    /// is association evidence only and is deliberately excluded from generic recording.
    WorkResultState {
        project: String,
        #[serde(default)]
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: Option<String>,
    },

    /// Work Result App-only collaboration write. The App fixes the business kind
    /// to guidance + requires_ack and supplies one bounded replay key so uncertain
    /// Host delivery can be retried without duplicating the retained message.
    WorkResultSendMessage {
        project: String,
        /// Optional exact work context explicitly linked to the current Window; never the recipient.
        #[serde(default)]
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: Option<String>,
        #[schemars(length(min = 1, max = 8000))]
        message: String,
        #[schemars(length(min = 1, max = 128))]
        delivery_key: String,
    },

    /// Work Result App-only lazy read from one opaque frozen final-changes snapshot.
    /// Business session identity is deliberately excluded from generic Session
    /// recording so user expansion clicks cannot become Session work events.
    ChangesFileDiff {
        project: String,
        session_id: String,
        snapshot_id: String,
        path: String,
    },

    /// Return a bounded structured summary of recorded session ledger data for
    /// an explicit session id.
    SessionSummary {
        /// Required explicit wc_sess_* Workflow Session id from a compatible Session bootstrap.
        session_id: String,
        /// Maximum recent events to return, capped by the runtime.
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Replace the complete execution defaults of one known active Workflow
    /// Session after resolving and authorizing its exact project. The
    /// in-memory context/event commit is atomic; ledger persistence is queued.
    UpdateSessionContext {
        /// Required complete runtime project id or unambiguous project input. The caller must be authorized
        /// for the resolved project, and it must exactly match the Session project.
        #[schemars(length(min = 1))]
        project: String,
        /// Required explicit active, project-scoped Workflow Session id. Unknown ids fail without creating
        /// a Session.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Complete replacement execution context. `{}` clears all defaults. The context cannot store
        /// environment variables, credentials, SSH host/configuration, keys, passwords, connections, or
        /// arbitrary options.
        execution_context: SessionExecutionContext,
    },

    /// Explicitly close a workflow session (`Active → Closed`). Requires an
    /// explicit `session_id`. Idempotent when already closed. Does not archive
    /// or evict the Session.
    CloseSession {
        /// Required explicit wc_sess_* id to close. Unknown ids fail without creating a Session. Idempotent
        /// when already closed. finish_coding_task does not close.
        session_id: String,
    },

    /// Read bounded structured validation evidence already present in an
    /// explicit project-scoped session ledger. Never executes validation,
    /// shell commands, Runner requests, or project file reads.
    ValidationSummary {
        /// Required complete runtime project id from list_projects. Must match the project scoped to
        /// session_id.
        #[schemars(length(min = 1))]
        project: String,
        /// Required explicit wc_sess_* business Session id.
        #[schemars(length(min = 1))]
        session_id: String,
        #[schemars(extend("default" = 20))]
        /// Maximum validation history events returned. Defaults to 20; values above 100 are accepted and
        /// clamped to 100. Per-event parser evidence keeps its own fixed bounds.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Record a bounded external claim without creating native execution evidence.
    RecordExternalObservation {
        project: String,
        session_id: String,
        /// SHA-256 of the adapter's local conversation identity; not an authority token.
        #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))]
        adapter_id: String,
        /// Stable SHA-256 event identity within this adapter and exact Session.
        #[schemars(length(min = 64, max = 64), regex(pattern = "^[0-9a-f]{64}$"))]
        event_id: String,
        /// Tool name only; no command, argument, output or transcript text.
        #[schemars(length(min = 1, max = 64), regex(pattern = "^[A-Za-z0-9_.:-]+$"))]
        observed_tool: String,
        /// External receipt claim only. Omit when no trustworthy execution receipt is available.
        #[serde(default)]
        exit_code: Option<i32>,
    },
    /// Read external claims separately from native Session/Job evidence.
    ListExternalObservations { project: String, session_id: String },

    /// Post a bounded session-local ledger message for collaboration, progress,
    /// guidance, or design discussion. This is session metadata only.
    PostSessionMessage {
        /// Required wc_sess_* id whose session-local message board receives this message. This is business
        /// input, not recorder metadata.
        session_id: String,
        /// Message kind.
        kind: SessionMessageKind,
        /// Non-empty message body. Guidance is session-local context and never overrides
        /// system/platform/WebCodex safety policy.
        #[schemars(length(max = 8000))]
        message: String,
        /// Optional tags for filtering or review.
        #[schemars(length(max = 16))]
        #[schemars(inner(length(max = 64)))]
        #[serde(default)]
        tags: Vec<String>,
        /// Optional message id in the same session.
        #[serde(default)]
        reply_to: Option<String>,
        /// Optional priority; defaults to normal.
        #[serde(default)]
        priority: SessionMessagePriority,
        #[schemars(extend("default" = false))]
        /// Optional acknowledgement requirement. Any message kind may request acknowledgement; ACK is
        /// context-scoped and never resolves, accepts, executes, or gates work.
        #[serde(default)]
        requires_ack: bool,
        /// Optional stable sender-scoped replay key. While the keyed message/replay metadata remains
        /// retained, exact retries with the same canonical payload return the original message and survive
        /// Server restart; reusing the key with a different retained payload fails closed.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        delivery_key: Option<String>,
    },

    /// Send a bounded collaboration message to another recent ChatGPT/host window owned by the
    /// same authenticated principal. Peer routing is project-independent and grants no access to the
    /// recipient's current Project, Workflow Session, files, or task authority.
    PostPeerMessage {
        /// Opaque principal-scoped peer identity discovered through peer_awareness.
        #[schemars(regex(pattern = "^wc_peer_[0-9a-f]{32}$"))]
        peer_id: String,
        /// Communication kind. A peer todo is only a request message; it does not create a fenced
        /// Workflow Session assignment.
        kind: SessionMessageKind,
        /// Non-empty bounded message body.
        #[schemars(length(max = 8000))]
        message: String,
        /// Optional tags for filtering and later analysis.
        #[schemars(length(max = 16))]
        #[schemars(inner(length(max = 64)))]
        #[serde(default)]
        tags: Vec<String>,
        /// Optional priority; defaults to normal.
        #[serde(default)]
        priority: SessionMessagePriority,
        #[schemars(extend("default" = false))]
        /// When false, WebCodex attempts one ambient projection on the recipient's next model-facing
        /// tool result. When true, omission of the request-scoped ACK causes the message to be projected
        /// again. ACK never grants authority, resolves the message, or requires a reply.
        #[serde(default)]
        requires_ack: bool,
        /// Optional stable sender-window-scoped replay key. While the keyed peer message remains
        /// retained, exact retries return the original message and survive Server restart; reusing the key
        /// with a different retained payload fails closed.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        delivery_key: Option<String>,
    },

    /// List session-local ledger messages in stable newest-first order.
    ListSessionMessages {
        /// Required wc_sess_* id whose session-local message board is listed.
        session_id: String,
        /// Optional kind filter.
        #[serde(default)]
        kind: Option<SessionMessageKind>,
        /// Optional status filter.
        #[serde(default)]
        status: Option<SessionMessageStatus>,
        /// Optional exact wc_msg_* filter. Combined with kind/status/reply_to using deterministic AND
        /// semantics; returns exact 0/1 when this filter is supplied.
        #[serde(default)]
        message_id: Option<String>,
        /// Optional exact reply_to wc_msg_* filter, useful for finding replies to one todo. Combined with
        /// all other filters using AND semantics.
        #[serde(default)]
        reply_to: Option<String>,
        /// Maximum messages to return. Defaults to 50; values above 100 are accepted and clamped to 100.
        /// Results are newest-first by created_at.
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Read one exact open todo plus every retained direct reply under one
    /// Session-store snapshot and return an opaque assignment fence.
    GetSessionAssignment {
        /// Required coordinator/business Workflow Session containing the exact todo.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Required exact open todo id. No implicit or recent-message inference is used.
        #[schemars(regex(pattern = "^wc_msg_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        message_id: String,
    },

    /// Observe only message-state changes after an opaque Session-bound durable
    /// cursor. Without a token this establishes a current baseline and returns
    /// no history. Optional waiting is one bounded wait, never a subscription.
    ObserveSessionMessages {
        /// Required explicit Workflow Session whose message-state delta is observed.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Optional opaque Session-bound durable observation token returned by an earlier
        /// observe_session_messages call.
        #[schemars(length(max = 192))]
        #[serde(default, deserialize_with = "deserialize_optional_observation_token")]
        after_observation_token: Option<String>,
        /// Optional one-shot bounded wait in seconds. Positive values above 60 are accepted and clamped to
        /// 60. Allowed only with after_observation_token; never creates a subscription or stream.
        #[schemars(range(min = 1))]
        #[serde(default)]
        wait_secs: Option<u64>,
        /// Maximum retained current-state message changes returned. Defaults to 50; values above 100 are
        /// accepted and clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Mark a session-local message resolved. Idempotent for already resolved
    /// messages.
    ResolveSessionMessage {
        /// Required wc_sess_* id containing the message.
        session_id: String,
        /// wc_msg_* id returned by post_session_message.
        message_id: String,
        /// Optional resolution note.
        #[schemars(length(max = 8000))]
        #[serde(default)]
        resolution: Option<String>,
    },

    /// Atomically answer and resolve one exact open todo. A bounded caller key
    /// makes uncertain-result retries return the original completion.
    CompleteSessionMessage {
        /// Required coordinator/business wc_sess_* id containing the exact open todo.
        session_id: String,
        /// Exact open todo wc_msg_* id to answer and resolve atomically.
        message_id: String,
        /// Bounded answer body stored once as a kind=answer message replying to the todo.
        #[schemars(length(min = 1, max = 8000))]
        answer: String,
        /// Caller-generated idempotency key for this exact completion. Same key and same answer returns the
        /// original result; conflicting reuse fails closed.
        #[schemars(length(min = 1, max = 128))]
        completion_key: String,
        /// Required semantic snapshot fence returned by get_session_assignment for this exact Session/todo.
        /// Pass it unchanged; assignment-local semantic changes fail closed before completion.
        #[schemars(length(min = 27, max = 27))]
        #[schemars(regex(pattern = "^wsa2_[A-Za-z0-9_-]{22}$"))]
        expected_assignment_fence: String,
        /// Optional tags on the created answer.
        #[schemars(length(max = 16))]
        #[schemars(inner(length(max = 64)))]
        #[serde(default)]
        tags: Vec<String>,
        /// Optional answer priority; defaults to normal.
        #[serde(default)]
        priority: SessionMessagePriority,
        /// Kernel-injected trusted provenance. Never accepted from public JSON.
        #[serde(skip)]
        trusted_recording_session_id: Option<String>,
    },

    /// Return a bounded structured aggregate of session-local ledger discussion.
    SessionDiscussionSummary {
        /// Required wc_sess_* id whose message board should be summarized.
        session_id: String,
        /// Maximum recent progress/decision messages to return. Defaults to 50; values above 100 are
        /// accepted and clamped to 100.
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Return a bounded structured handoff summary for an explicit session id:
    /// session ledger info, message-board state, recent progress/decisions,
    /// open todos/risks/questions/guidance, recent failed tool calls, and
    /// optional workspace, checkpoint, and ledger-derived validation metadata.
    /// Read-only; never calls an LLM or generates natural-language summaries.
    /// Model/API exposure is derived from the canonical ToolDefinition surface.
    SessionHandoffSummary {
        /// Required explicit wc_sess_* business Session id to summarize.
        session_id: String,
        /// Optional runtime project id. Omission uses the exact authorized Session Project.
        #[serde(default)]
        project: Option<String>,
        /// Include a bounded workspace (git status) summary. Defaults to true when the Session
        /// has an authorized Project or an explicit project is supplied.
        #[serde(default)]
        include_workspace: Option<bool>,
        /// Include bounded checkpoint candidates, especially the latest last_known_good. Defaults to true.
        /// Only effective with an authorized Project and the workspace-checkpoints build feature enabled;
        /// otherwise accepted and ignored.
        #[serde(default)]
        include_checkpoints: Option<bool>,
        /// Include ledger-derived validation summary. Defaults to true. Minimal diagnostics require bounded
        /// tails or safe result metadata; parser.available remains false when session ledger events lack
        /// those fields.
        #[serde(default)]
        include_validation: Option<bool>,
        /// Include detailed ledger and closeout evidence. Defaults to false: the response
        /// contains only identity and the deterministic handoff_brief (at most 8 KiB).
        #[serde(default)]
        diagnostic: bool,
        /// Maximum items per bounded section. Defaults to 20; values above 100 are accepted and clamped to
        /// 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Adapter/API-only exact recovery read. Uses the canonical handoff projection
    /// without exposing the business Session through generic recorder semantics.
    SessionHandoffState {
        /// Required exact runtime Project; must match the authorized Session Project.
        project: String,
        /// Required exact business Workflow Session id.
        session_id: String,
    },

    /// Create a bounded last-known-good workspace checkpoint outside the
    /// project worktree.
    #[cfg(feature = "workspace-checkpoints")]
    WorkspaceCheckpointCreate {
        project: String,
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        include_untracked: Option<bool>,
        #[serde(default)]
        kind: Option<String>,
        #[serde(default)]
        labels: Vec<String>,
        #[serde(default)]
        validation: Option<CheckpointValidationInput>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// List checkpoint metadata for a project without returning diffs.
    #[cfg(feature = "workspace-checkpoints")]
    WorkspaceCheckpointList {
        project: String,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Show bounded checkpoint metadata and file lists without full diff
    /// content.
    #[cfg(feature = "workspace-checkpoints")]
    WorkspaceCheckpointShow {
        project: String,
        checkpoint_id: String,
        #[serde(default)]
        include_diff_stat: Option<bool>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Restore a workspace checkpoint after explicit confirmation.
    #[cfg(feature = "workspace-checkpoints")]
    WorkspaceCheckpointRestore {
        project: String,
        checkpoint_id: String,
        confirm: bool,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Delete a persisted checkpoint file after explicit confirmation.
    #[cfg(feature = "workspace-checkpoints")]
    WorkspaceCheckpointDelete {
        project: String,
        checkpoint_id: String,
        confirm: bool,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Execute one bounded read-only JavaScript orchestration cell. Project and
    /// Workflow Session are mandatory outer authority targets; nested calls may
    /// not select either target.
    #[cfg(feature = "experimental-code-mode")]
    CodeModeExec {
        /// Required Project target. Nested JavaScript tool calls cannot select or override Project authority.
        project: String,
        /// Required exact Workflow Session. Nested JavaScript tool calls remain bound to this Session and record canonical evidence there.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Bounded JavaScript orchestration source. tools.<name>(args) returns a Promise for admitted read-only tools; use direct primitives for simple one-step observations, Promise.all only for independent observations, and sequential adaptive follow-ups inside the cell. Filter and synthesize raw child results before text(value); emit distilled evidence, not raw-result dumps, before reaching the outer-output limit. Project/Session are outer-bound. No shell, filesystem, network, Node, Deno, WebAssembly, mutation, validation, Jobs, plugins, or MCP are exposed.
        #[schemars(length(max = 65536))]
        source: String,
        /// Optional wall-clock budget in milliseconds. Defaults to 5000 and is server-clamped to 1..30000.
        #[schemars(range(min = 0))]
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Execute one experimental E2a effectful orchestration cell. The outer
    /// envelope is consequential, while every admitted nested child still enters
    /// ordinary canonical ToolRuntime authority and evidence paths.
    #[cfg(feature = "experimental-code-mode")]
    CodeModeExecEffectful {
        /// Required Project target. Nested JavaScript tool calls cannot select or override Project authority.
        project: String,
        /// Required exact Workflow Session. Every nested child remains a canonical ToolRuntime invocation in this same Session.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Experimental E2a JavaScript orchestration source. Admitted tools are the E1 read-only set plus cargo_check and cargo_test. Structured validators may hand off the same execution as ordinary Jobs; no mutation, shell, generic process, Job observation, plugins/MCP, or recursive Code Mode is exposed.
        #[schemars(length(max = 65536))]
        source: String,
        /// Optional orchestration/frontend decision deadline in milliseconds. Defaults to 5000 and is server-clamped to 1..30000. The response may follow after a short bounded drain of already-started canonical child calls needed to report truthful consequential outcomes.
        #[schemars(range(min = 0))]
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Execute one experimental E2c bounded coding cell. The outer envelope
    /// requires ProjectWrite and JobRun authority; children remain canonical.
    #[cfg(feature = "experimental-code-mode")]
    CodeModeExecMutating {
        /// Required Project target. Nested JavaScript tool calls cannot select or override Project authority.
        project: String,
        /// Required exact Workflow Session. Every nested child remains a canonical ToolRuntime invocation in this same Session.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Experimental E2c source: E1 reads, at most one canonical apply_text_edits attempt, then cargo_check/cargo_test only after a successful known edit (including no-op). Use read_revision for guarded edits. Inspect source_state independently of execution success. Return Job handoffs to the outer workflow, never wait inside JS. No shell/process, nested Job observation, alternate writes, gateways, recursion or automatic whole-program retry.
        #[schemars(length(max = 65536))]
        source: String,
        /// Optional frontend decision deadline in milliseconds. Defaults to 5000, clamped to 1..30000. A short bounded drain preserves already-dispatched mutation/validation truth and exact Job continuations; timeout is not rollback or retry authority.
        #[schemars(range(min = 0))]
        #[serde(default)]
        timeout_ms: Option<u64>,
    },

    /// Execute one native process directly from a structured executable and
    /// argv. No shell parser, environment mutation, PTY, or durable handoff is
    /// part of this synchronous v1 contract.
    RunProcess {
        /// Configured project id.
        project: String,
        /// Executable name or path, resolved through the Runner execution environment. Native executables
        /// use literal argv. Windows .cmd/.bat shims use Runner-owned cmd.exe conversion with AutoRun and
        /// delayed expansion disabled; no model shell string. Batch paths/arguments reject quotes, %, !, ^,
        /// control characters and trailing backslashes before spawn; use a native runtime for these values.
        /// Batch command lines are limited to 8000 UTF-16 units and require a local drive cwd; UNC cwd is
        /// rejected before spawn.
        #[schemars(length(min = 1, max = 1024))]
        executable: String,
        /// Ordered argv values passed literally to the child process. Defaults to an empty array. Runtime
        /// validation allows at most 16,000 UTF-8 bytes across executable and argv boundaries.
        #[schemars(length(max = 256))]
        #[schemars(inner(length(max = 8192)))]
        #[serde(default)]
        args: Vec<String>,
        /// Optional bounded UTF-8 stdin payload passed through a pipe; null and omission both mean no stdin
        /// payload.
        #[schemars(schema_with = "nullable_stdin_schema")]
        #[serde(default)]
        stdin: Option<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Total process execution lifetime in seconds (minimum 1, default 60). Values above 604800
        /// (7 days) are accepted and clamped to 604800. Short work may return synchronously; longer work keeps
        /// the same execution and returns job_id when durable structured execution is available.
        #[schemars(extend("default" = 60))]
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. The field remains accepted for
        /// compatibility but is hidden from model discovery; Server transport policy and timeout_secs bound
        /// the effective wait. It never extends command lifetime or reruns work.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
        /// Project-relative working directory. Omit, empty string, or '.' for the project root. Named
        /// Session SSH resources are unsupported for run_process.
        #[schemars(length(max = 1024))]
        #[serde(default)]
        cwd: Option<String>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Admit one native executable + argv as an explicitly detached durable Job.
    /// The detached supervisor owns the accepted payload tree; ordinary
    /// RunProcess remains unchanged.
    RunDetachedProcess {
        /// Configured project id.
        project: String,
        /// Required bounded caller-chosen key for this detached initiation. While the logical Job is active
        /// or retained for 86400 seconds (24 hours) after terminal completion, reusing the same key resolves
        /// to that Job and cannot redispatch its payload. After retained history expires the key may identify a new
        /// execution, so never reuse an expired key as a retry token. After Server restart an existing
        /// retained Job is returned for recovery rather than guessing that a resent body matches.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
        /// Executable name or path, resolved through the Runner execution environment. Native executables
        /// use literal argv. Windows .cmd/.bat shims use Runner-owned cmd.exe conversion with AutoRun and
        /// delayed expansion disabled; no model shell string. Batch paths/arguments reject quotes, %, !, ^,
        /// control characters and trailing backslashes before spawn; use a native runtime for these values.
        /// Batch command lines are limited to 8000 UTF-16 units and require a local drive cwd; UNC cwd is
        /// rejected before spawn.
        #[schemars(length(min = 1, max = 1024))]
        executable: String,
        /// Ordered argv values passed literally to the child process. Defaults to an empty array. Runtime
        /// validation allows at most 16,000 UTF-8 bytes across executable and argv boundaries.
        #[schemars(length(max = 256))]
        #[schemars(inner(length(max = 8192)))]
        #[serde(default)]
        args: Vec<String>,
        /// Optional bounded UTF-8 stdin payload passed through a pipe; null and omission both mean no stdin
        /// payload.
        #[schemars(schema_with = "nullable_stdin_schema")]
        #[serde(default)]
        stdin: Option<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = 60))]
        /// Total detached process execution lifetime in seconds (minimum 1, default 60). Values above
        /// 604800 (7 days) are accepted and clamped to 604800. Admission returns the stable Job identity
        /// without waiting for terminal completion.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Project-relative working directory. Omit, empty string, or '.' for the project root. Named
        /// Session SSH resources are unsupported for run_detached_process.
        #[schemars(length(max = 1024))]
        #[serde(default)]
        cwd: Option<String>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },
    CodingAgentStart {
        /// Exact registered Project id. It resolves the Runner and fixes ACP session cwd to that Project
        /// root; cwd is not a filesystem sandbox.
        #[schemars(length(min = 1))]
        project: String,
        /// Logical Runner-advertised ACP provider id, for example codex. Executable, argv, environment,
        /// credentials, and provider instance ids are Runner-owned and cannot be supplied here.
        #[schemars(length(min = 1, max = 64))]
        provider_id: String,
        /// Required caller-chosen replay key for this autonomous Run initiation. Reuse the same key for the
        /// same intent after an uncertain start; do not mint a replacement key to retry an uncertain
        /// prompt.
        #[schemars(length(min = 1, max = 256))]
        idempotency_key: String,
        /// Bounded coding-agent instruction/prompt. It is sent to the delegated ACP agent but excluded from
        /// durable Run recovery records, Workflow lifecycle evidence, audit summaries, and generic
        /// telemetry bodies.
        #[schemars(length(min = 1, max = 65536))]
        instruction: String,
        /// Optional explicit run-level ACP config overrides. Omission or {} sends no caller-requested
        /// set_config_option calls; Runner-owned forced_config policy may still apply its own values.
        /// Every caller key/value must be live-advertised and operator-allowed before prompt dispatch.
        #[serde(default)]
        config: Option<BTreeMap<String, webcodex_core::coding_agent::CodingAgentConfigValue>>,
        #[schemars(extend("default" = 300))]
        /// Total Run budget. Timeout requests cancellation; it is not a retry signal and may become
        /// lost/outcome_unknown if terminal correlation is unavailable.
        #[schemars(range(min = 1, max = 3600))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Kernel-injected recorder provenance. Public wrapper metadata is stripped by adapters before
        /// concrete parsing and can never populate this business payload.
        #[serde(skip)]
        recording_session_id: Option<String>,
    },
    CodingAgentObserve {
        /// Opaque CodingAgentRun id returned by coding_agent_start. Knowing the id alone grants no
        /// authority.
        #[schemars(regex(pattern = "^wc_agent_run_[A-Za-z0-9_.-]+$"))]
        run_id: String,
        /// Opaque exact-Run-bound observation token returned by the previous observation. A Server restart
        /// may reset it while preserving the Run.
        #[schemars(length(max = 192))]
        #[serde(default)]
        after_observation_token: Option<String>,
        #[schemars(extend("default" = 0))]
        /// One bounded wait for retained Run changes; values above the supported wait ceiling are accepted
        /// and clamped. Not a subscription or stream.
        #[schemars(range(min = 0))]
        #[serde(default)]
        wait_secs: Option<u64>,
    },
    CodingAgentCancel {
        /// Opaque CodingAgentRun id to cancel. Cancellation never starts or retries work and must be
        /// followed by observation for authoritative terminal state.
        #[schemars(regex(pattern = "^wc_agent_run_[A-Za-z0-9_.-]+$"))]
        run_id: String,
    },

    /// Execute bounded script content transported as typed data and written to
    /// a Runner-owned temporary file. The selected language is explicit and
    /// never inherited from Session default_shell.
    RunScript {
        /// Configured project id.
        project: String,
        /// Required semantic script language: sh, bash, PowerShell, Python, JavaScript, or TypeScript.
        /// Python uses a Runner-resolved interpreter and a temporary .py file. JavaScript uses Runner-resolved Node.js with fixed .mjs ESM
        /// semantics. TypeScript uses Runner-resolved Node.js native erasable type stripping from a fixed
        /// .mts ESM file and requires Node.js 22.6.0 or newer. The Runner owns any runtime compatibility
        /// flags; callers cannot provide a runtime path or runtime flags. Session default_shell never
        /// overrides this field.
        language: ShellScriptLanguage,
        /// Bounded UTF-8 script content transported as typed data. It is written to a Runner-owned
        /// temporary file and never placed in a shell command string.
        #[schemars(length(min = 1, max = 524288))]
        script: String,
        /// Ordered script arguments passed as independent argv values. Defaults to an empty array. At most
        /// 256 entries, each at most 8192 UTF-8 bytes, with at most 16000 bytes total including one
        /// boundary byte per value; values are never interpolated into the script body.
        #[schemars(length(max = 256))]
        #[schemars(inner(length(max = 8192)))]
        #[serde(default)]
        args: Vec<String>,
        /// Optional bounded UTF-8 stdin payload piped independently to the script process; null and
        /// omission both mean no stdin payload.
        #[schemars(schema_with = "nullable_stdin_schema")]
        #[serde(default)]
        stdin: Option<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = 60))]
        /// Total script execution lifetime in seconds (minimum 1, default 60). Values above 604800
        /// (7 days) are accepted and clamped to 604800. Short work may return synchronously; longer work keeps
        /// the same execution and returns job_id when durable structured execution is available.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. The field remains accepted for
        /// compatibility but is hidden from model discovery; Server transport policy and timeout_secs bound
        /// the effective wait. It never extends command lifetime or reruns work.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
        /// Project-relative working directory. Omit, empty string, or '.' for the project root. Named
        /// Session SSH resources are unsupported for run_script.
        #[schemars(length(max = 1024))]
        #[serde(default)]
        cwd: Option<String>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Execute a shell command in a project directory (sync, short-lived).
    RunShell {
        /// Configured project id.
        project: String,
        /// Shell command to run. At most 65536 UTF-8 bytes; use run_script for substantially larger
        /// typed program text and stdin/files/artifacts for large data.
        #[schemars(length(max = 65536))]
        command: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = 60))]
        /// Total lifetime seconds (default 60, min 1); values above the shared structured-execution
        /// ceiling of 3600 seconds are accepted and clamped to 3600; named SSH keeps the direct ceiling.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution durable Job handoff grace. Hidden from model discovery;
        /// Server transport policy and timeout_secs bound the effective wait. It controls return only, not
        /// when the command is killed; named SSH remains unsupported.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
        /// Working directory contract: without a Session SSH resource, omit, empty string, or '.' selects
        /// the project root and any other value is project-relative. With a named Session SSH resource, cwd
        /// is a remote path checked by the remote shell instead of the Runner project-root policy.
        #[serde(default)]
        cwd: Option<String>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
        /// Optional explicit command language: sh or bash. When omitted, local run_shell uses sh, a
        /// Runner-backed run_shell uses that Runner's configured shell, and a named Session SSH resource
        /// uses the remote login shell. The response always records the actual selection.
        #[serde(default)]
        shell: Option<ExecutionShell>,
        /// Bash login mode. `true` requires `shell="bash"` and executes exactly
        /// `bash -lc <command>` using the Runner-resolved Bash program.
        #[serde(default)]
        login: bool,
    },

    /// Open one explicit command-oriented persistent shell for this Workflow
    /// Session. It is not shared with run_shell/run_job and is not a Job.
    OpenSessionShell {
        /// Exact Workflow Session project id.
        project: String,
        /// Explicit active Workflow Session id. Current-session fallback is not used.
        session_id: String,
        /// Optional initial cwd. Without a named Session SSH resource it is project-relative; with one it
        /// is a remote path. Omission uses the Session default, then the project or SSH-resource default.
        #[serde(default)]
        cwd: Option<String>,
        /// Optional Unix local-shell override: sh or bash. Omit on Windows so the Runner uses its
        /// configured PowerShell program/profile.
        #[serde(default)]
        shell: Option<ExecutionShell>,
    },

    /// Execute one framed command in an already-open persistent shell.
    SessionShellExec {
        /// Exact Workflow Session project id.
        project: String,
        /// Explicit active Workflow Session id.
        session_id: String,
        /// Opaque id returned by open_session_shell.
        shell_id: String,
        /// One command evaluated by the existing long-lived shell. At most 65536 UTF-8 bytes.
        #[schemars(length(max = 65536))]
        command: String,
        #[schemars(extend("default" = 60))]
        /// Command timeout in seconds (minimum 1, default 60). Values above 3600 are accepted and clamped
        /// to 3600. Timeout recovery requires verified framing resynchronization; otherwise the shell is
        /// poisoned and terminated before reuse.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Read Runner-authoritative lifecycle state for a persistent shell.
    SessionShellStatus {
        /// Exact Workflow Session project id.
        project: String,
        /// Explicit active Workflow Session id.
        session_id: String,
        /// Opaque id returned by open_session_shell.
        shell_id: String,
    },

    /// Close a persistent shell and its complete process group. Idempotent for
    /// an already-closed shell retained by the current Server process.
    CloseSessionShell {
        /// Exact Workflow Session project id.
        project: String,
        /// Explicit active Workflow Session id.
        session_id: String,
        /// Opaque id returned by open_session_shell.
        shell_id: String,
    },

    /// Apply one bounded Codex *** Begin Patch payload transactionally through the owning Runner.
    ApplyPatch {
        /// Runner-registered project id.
        project: String,
        /// Codex apply_patch DSL using *** Begin Patch with Add File, Update File, Delete File, optional
        /// Move to, @@ context, and optional *** End of File markers.
        #[schemars(length(min = 1, max = 262144))]
        patch: String,
        #[schemars(extend("default" = false))]
        /// If true, fully parse and preflight the patch without writing any file.
        #[serde(default)]
        dry_run: Option<bool>,
        #[schemars(extend("default" = "unique"))]
        /// Positioning policy. unique (default) tries Exact, TrimEnd, Trim, then Normalized and requires
        /// exactly one final mutation target at the selected tier; a repeated @@ anchor is allowed when
        /// old_lines still resolves to one target, while anchored pure additions require a unique anchor.
        /// exact_unique additionally requires Exact and unique at every textual positioning decision and is
        /// intended for an explicit stale-context/concurrency fence after reading exact current source.
        /// first_match is only for explicitly requested permissive compatibility and deterministically
        /// selects the first eligible candidate in the highest-priority tier.
        #[serde(default)]
        matching_mode: Option<ApplyPatchMatchingMode>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Apply one bounded raw standard unified diff after an internal safety/applicability preflight.
    ApplyUnifiedDiff {
        /// Runner-registered project id.
        project: String,
        /// Raw standard unified diff only. Do not include shell heredocs or Codex apply_patch wrapper
        /// syntax such as *** Begin Patch / *** Update File / *** End Patch. The first non-empty line
        /// should be diff --git ..., --- ..., or another git-apply-compatible unified diff header.
        #[schemars(length(max = 262144))]
        diff: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = true))]
        /// Optional fail-safe sensitive-path policy. Defaults to true; when true, any sensitive-path
        /// warning blocks mutation before git apply --check is dispatched.
        #[serde(default)]
        deny_sensitive_paths: Option<bool>,
    },

    /// Delete project-relative files only (not directories).
    DeleteProjectFiles {
        /// Runner-registered project id.
        project: String,
        /// Project-relative file paths to delete.
        paths: Vec<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Restore tracked paths with `git restore -- <paths>`.
    GitRestorePaths {
        /// Runner-registered project id.
        project: String,
        /// Project-relative tracked paths to restore.
        paths: Vec<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Discard selected untracked files with `git clean -f -- <paths>`.
    DiscardUntracked {
        /// Runner-registered project id.
        project: String,
        /// Project-relative untracked paths to remove.
        paths: Vec<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Commit exactly requested changed paths behind an exact HEAD fence.
    GitCommitPaths {
        /// Runner-registered project id.
        project: String,
        /// Exact current 40-hex HEAD fence; normally copy show_changes.head.commit immediately before
        /// committing.
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        expected_head: String,
        /// Exact project-relative file paths to commit. Directories, project root, sensitive paths, and
        /// unchanged paths are rejected.
        #[schemars(length(min = 1, max = 32))]
        #[schemars(inner(length(min = 1, max = 512)))]
        paths: Vec<String>,
        /// Commit message. Bounded and never persisted in model-facing audit previews.
        #[schemars(length(min = 1, max = 1000))]
        message: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Run `git status` on a project.
    GitStatus {
        /// Configured project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Return bounded structured recent git commit history.
    GitLog {
        /// Runner-registered project id.
        project: String,
        /// Optional exact 40-hex commit snapshot fence. Omit on the first page so Runtime resolves the
        /// current HEAD; parser-ready suggested_call carries the observed commit on later pages.
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        #[serde(default)]
        head_commit: Option<String>,
        /// Maximum commits to return (default 20, clamped to 1..100).
        #[serde(default)]
        limit: Option<usize>,
        /// Number of recent commits to skip (default 0, clamped to 0..10000).
        #[serde(default)]
        skip: Option<usize>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Return bounded structured hunks from `git diff`.
    GitDiffHunks {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative paths to scope diff.
        #[serde(default)]
        paths: Option<Vec<String>>,
        /// Maximum hunks to return (clamped).
        #[serde(default)]
        max_hunks: Option<usize>,
        /// Maximum lines per hunk (clamped).
        #[serde(default)]
        max_hunk_lines: Option<usize>,
        #[schemars(extend("default" = 196608))]
        /// Raw producer page budget in bytes, independent of the final serialized model result. Defaults to
        /// the shared safe producer maximum (192 KiB). Any recognized nonnegative integer is accepted and
        /// runtime-clamped to the fixed 16..192 KiB producer bounds so ordinary Runner result retention
        /// retains framing headroom.
        #[schemars(range(min = 0))]
        #[serde(default)]
        max_page_bytes: Option<usize>,
        /// Use staged diff via git diff --cached.
        #[serde(default)]
        cached: Option<bool>,
        /// Optional exact 40-hex Git commit object id; requires head_commit and committed-range mode.
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        #[serde(default)]
        base_commit: Option<String>,
        /// Optional exact 40-hex Git commit object id reviewed from the single merge-base; requires
        /// base_commit.
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        #[serde(default)]
        head_commit: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_optional_git_diff_hunks_continuation"
        )]
        /// Compact opaque runtime continuation returned by git_diff_hunks. Copy it verbatim only through
        /// the returned parser-ready suggested_call; do not interpret it. It may identify either a
        /// later-record page cursor or the next complete-line fragment of one exact hunk; token type is
        /// opaque and scope/fence-bound. Repeat its exact original effective scope/paging inputs unchanged
        /// (base_commit/head_commit for committed mode, cached/worktree mode, paths, max_hunks,
        /// max_hunk_lines, and max_page_bytes). Later-record and hunk-fragment continuations remain
        /// distinct identities.
        #[schemars(length(max = 192))]
        continuation: Option<String>,
    },

    /// Return a deterministic bounded review map for an exact committed range.
    GitReviewSummary {
        /// Runner-registered project id.
        project: String,
        /// Exact 40-hex Git commit object id used to compute the merge-base.
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        base_commit: String,
        /// Exact 40-hex Git commit object id reviewed from merge-base to head.
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        head_commit: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Run `cargo fmt` in a Runner-registered Rust project.
    CargoFmt {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative working directory.
        #[serde(default)]
        cwd: Option<String>,
        /// When true, perform pure read-only `cargo fmt -- --check` validation. Omit or use false during
        /// coding to ensure formatting: WebCodex first checks, then runs mutating `cargo fmt` only when a
        /// stable rustfmt diff is proven.
        #[serde(default)]
        check: Option<bool>,
        #[schemars(extend("default" = 120))]
        /// For check=false ensure-format, this is the shared synchronous budget for precheck plus any
        /// required mutation; minimum 1, default 120, values above 120 clamp to 120. With check=true,
        /// values above the 3600-second read-only validation budget clamp to 3600 and a long check may
        /// return job_id.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. With check=true the field remains
        /// accepted but is hidden from model discovery; Server transport policy and timeout_secs bound the
        /// effective wait. With check=false it is accepted for caller-shape compatibility but ignored;
        /// ensure-format remains synchronous and timeout_secs remains the full precheck-plus-mutation budget.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
    },

    /// Run `cargo check` in a Runner-registered Rust project.
    CargoCheck {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative working directory.
        #[serde(default)]
        cwd: Option<String>,
        /// Include --all-targets (default true).
        #[serde(default)]
        all_targets: Option<bool>,
        /// Include --all-features.
        #[serde(default)]
        all_features: Option<bool>,
        /// Include --no-default-features.
        #[serde(default)]
        no_default_features: Option<bool>,
        /// Feature list passed to --features.
        #[serde(default)]
        features: Option<String>,
        /// Legacy single workspace package passed to `-p`. Mutually exclusive
        /// with `packages`; internally canonicalized to the same package set.
        #[serde(default)]
        package: Option<String>,
        /// Workspace packages passed as repeated `-p` selectors in one Cargo
        /// invocation. Mutually exclusive with `package`; order and duplicates
        /// are canonicalized because package selection is set-like.
        #[schemars(length(min = 1, max = CARGO_PACKAGE_MAX_ITEMS))]
        #[schemars(inner(length(min = 1, max = CARGO_VALUE_MAX_BYTES)))]
        #[serde(default)]
        packages: Option<Vec<String>>,
        #[schemars(extend("default" = 600))]
        /// Total validation runtime budget in seconds (minimum 1). Values above 3600 are accepted and
        /// clamped to 3600. Short validation returns immediately; longer validation keeps the same
        /// execution and returns job_id for observation. Defaults vary per tool.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. Hidden from model discovery;
        /// Server transport policy and the effective timeout_secs bound the wait. The submitted validation
        /// may still be queued; this never extends timeout_secs, retries, or starts a second validation.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
    },

    /// Run `cargo test` in a Runner-registered Rust project.
    CargoTest {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative working directory.
        #[serde(default)]
        cwd: Option<String>,
        /// Optional Rust test substring passed as `cargo test FILTER`. This is not a CLI-argument field: do
        /// not include `--exact`, `--nocapture`, or other Cargo/libtest flags. Omit it to run the selected
        /// Cargo test target normally; if zero tests run, broaden or remove the filter, or use the test's
        /// full qualified name.
        #[serde(default)]
        filter: Option<String>,
        /// When true, include Cargo's --lib target selector. Omission and false have the same ordinary
        /// target-selection semantics.
        #[serde(default)]
        lib: Option<bool>,
        /// Include --all-targets.
        #[serde(default)]
        all_targets: Option<bool>,
        /// Include --all-features.
        #[serde(default)]
        all_features: Option<bool>,
        /// Include --no-default-features.
        #[serde(default)]
        no_default_features: Option<bool>,
        /// Feature list passed to --features.
        #[serde(default)]
        features: Option<String>,
        /// Package passed to -p.
        #[serde(default)]
        package: Option<String>,
        /// When true, compile tests with --no-run. A successful compile-only validation does not require
        /// executed-test-count proof.
        #[serde(default)]
        no_run: Option<bool>,
        /// Controls executed-test proof explicitly. Omission keeps the normal requirement for non-zero
        /// executed-test evidence; false is an explicit opt-out that allows zero tests to count as proof
        /// when no min_tests minimum is requested; true requires proof that at least one test executed.
        #[serde(default)]
        require_tests: Option<bool>,
        /// Require proof that at least this many tests executed. Combined with require_tests using the
        /// stricter minimum.
        #[schemars(range(min = 1, max = 1000000))]
        #[serde(default)]
        min_tests: Option<u64>,
        #[schemars(extend("default" = 1800))]
        /// Total validation runtime budget in seconds (minimum 1). Values above 3600 are accepted and
        /// clamped to 3600. Short validation returns immediately; longer validation keeps the same
        /// execution and returns job_id for observation. Defaults vary per tool.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. Hidden from model discovery;
        /// Server transport policy and the effective timeout_secs bound the wait. The submitted validation
        /// may still be queued; this never extends timeout_secs, retries, or starts a second validation.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
    },

    /// Run canonical structured `go test -json` validation with an optional
    /// bounded project-relative package scope.
    GoTest {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative working directory.
        #[serde(default)]
        cwd: Option<String>,
        /// Optional 1..8 project-relative Go package patterns: '.', './path', './...', or './path/...'.
        #[schemars(length(min = 1, max = 8))]
        #[schemars(inner(length(min = 1, max = 256)))]
        #[serde(default)]
        packages: Option<Vec<String>>,
        #[schemars(extend("default" = 1800))]
        /// Total validation runtime budget in seconds (minimum 1). Values above 3600 are accepted and
        /// clamped to 3600. Short validation returns immediately; longer validation keeps the same
        /// execution and returns job_id for observation. Defaults vary per tool.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. Hidden from model discovery;
        /// Server transport policy and the effective timeout_secs bound the wait. The submitted validation
        /// may still be queued; this never extends timeout_secs, retries, or starts a second validation.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
    },

    /// Read up to eight UTF-8 files or file ranges under one bounded call.
    ReadFiles {
        /// Configured project id.
        project: String,
        /// One to eight project-relative UTF-8 file ranges, returned in request order.
        #[schemars(length(min = 1, max = 8))]
        #[serde(deserialize_with = "deserialize_read_files_items")]
        items: Vec<ReadFilesItem>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// When true, every successful item returns numbered text instead of plain text.
        #[serde(default)]
        with_line_numbers: Option<bool>,
        #[schemars(extend("default" = 65536))]
        /// Optional primary model-facing batch projection budget in bytes. Defaults to 64 KiB. Any
        /// recognized nonnegative integer is accepted and runtime-clamped to the fixed 8..512 KiB
        /// inspection bounds; raise the effective budget only for explicit broad/deep reads. If the current
        /// budget cannot return any part of the first remaining item, the result supplies a bounded
        /// increase_result_budget suggested call; otherwise batch continuation reuses the current effective
        /// budget. Independently bounded Session/continuity protocol overlays are preserved outside this
        /// budget.
        #[schemars(range(min = 0))]
        #[serde(default)]
        max_result_bytes: Option<usize>,
    },

    /// Load one Skill definition by unique exact Unicode case-folded name.
    SkillLoad {
        /// Required authorized runtime Project id.
        #[schemars(length(min = 1))]
        project: String,
        /// Exact Skill name to load. Matching uses Unicode case folding; substring and fuzzy matching are
        /// not used.
        #[schemars(length(min = 1, max = 96))]
        name: String,
        /// Optional explicit Workflow Session for this tool call. No implicit current-Session fallback is
        /// used.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Execute one trusted Runner Skill script through the dedicated Runner Skill execution contract.
    /// The Runner resolves the trusted package again at execution under the supplied revision/digest
    /// fences, preserves package-relative script identity while keeping the requested Project cwd, and
    /// receives only caller-provided script arguments; source bytes and Runner-native package paths are
    /// never model inputs.
    RunSkillResource {
        /// Configured project id.
        project: String,
        /// Opaque Runner Skill identity returned by skill_load or skill_list.
        #[schemars(regex(pattern = "^wc_skill_[A-Za-z0-9_-]{21}[AQgw]$"))]
        skill_id: String,
        /// Skill-package-relative script path under scripts/. Absolute paths and traversal are rejected.
        #[schemars(length(min = 9, max = 512))]
        #[schemars(regex(pattern = "^scripts/.+$"))]
        path: String,
        /// Required SKILL.md definition digest fence. For configured live Skills this fences the definition
        /// only; script resource bytes are read live at execution. Managed installed Skills additionally
        /// use expected_package_revision to fence the immutable package.
        #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
        expected_definition_revision: String,
        /// Required for operator-installed Skills and forbidden for configured live Skills. Pins the
        /// immutable installed package revision.
        #[schemars(regex(pattern = "^wc_skillpkg_[A-Za-z0-9_-]{43}$"))]
        #[serde(default)]
        expected_package_revision: Option<String>,
        /// Ordered literal script arguments. WebCodex selects the interpreter from the trusted Skill
        /// resource extension and preserves the selected package/script execution identity while keeping
        /// the requested Project cwd. The Skill script body and Runner-native package path are never model
        /// arguments.
        #[schemars(length(max = 256))]
        #[schemars(inner(length(max = 8192)))]
        #[serde(default)]
        args: Vec<String>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = 60))]
        /// Total process runtime budget in seconds (minimum 1, default 60). Values above 3600 are accepted
        /// and clamped to 3600. Short work returns synchronously; longer work keeps the same execution and
        /// returns job_id when durable structured execution is available.
        #[schemars(range(min = 1))]
        #[serde(default)]
        timeout_secs: Option<u64>,
        /// Legacy caller override for same-execution Job handoff grace. The field remains accepted for
        /// compatibility but is hidden from model discovery; Server transport policy and timeout_secs bound
        /// the effective wait. It never extends command lifetime or reruns work.
        #[schemars(skip)]
        #[serde(default)]
        sync_wait_secs: Option<u64>,
        /// Project-relative working directory. Omit, empty string, or '.' for the project root. Skill
        /// package resolution remains Runner-owned and does not change the requested business cwd.
        #[schemars(length(max = 1024))]
        #[serde(default)]
        cwd: Option<String>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
    },

    /// Fresh bounded discovery of project-scoped Agent Skills. This tool is
    /// model-hidden globally and exposed only by capable Stateless MCP Full
    /// Operator surfaces.
    SkillList {
        /// Required authorized runtime Project id.
        #[schemars(length(min = 1))]
        project: String,
        /// Optional bounded case-insensitive substring filter over Skill name and description only.
        #[schemars(length(max = 200))]
        #[serde(default)]
        query: Option<String>,
        #[schemars(range(min = 0))]
        #[serde(default)]
        offset: Option<usize>,
        #[schemars(range(min = 1, max = 64))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional catalog revision guard. If current discovery differs, fail instead of continuing an old offset.
        #[schemars(regex(pattern = "^wc_skillcat_[A-Za-z0-9_-]{43}$"))]
        #[serde(default)]
        expected_catalog_revision: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read one bounded UTF-8 text resource from a selected Skill package.
    SkillReadFile {
        #[schemars(length(min = 1))]
        project: String,
        #[schemars(regex(pattern = "^wc_skill_[A-Za-z0-9_-]{21}[AQgw]$"))]
        skill_id: String,
        #[schemars(length(min = 1, max = 512))]
        #[serde(default)]
        path: Option<String>,
        #[schemars(range(min = 1))]
        #[serde(default)]
        start_line: Option<usize>,
        #[schemars(range(min = 1, max = 400))]
        #[serde(default)]
        limit: Option<usize>,
        #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
        #[serde(default)]
        expected_definition_revision: Option<String>,
        #[schemars(regex(pattern = "^wc_skillpkg_[A-Za-z0-9_-]{43}$"))]
        #[serde(default)]
        expected_package_revision: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    SkillVersions {
        #[schemars(length(min = 1))]
        project: String,
        #[schemars(length(min = 1, max = 96))]
        #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
        skill_key: String,
        #[schemars(range(min = 0))]
        #[serde(default)]
        offset: Option<usize>,
        #[schemars(range(min = 1, max = 64))]
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        session_id: Option<String>,
    },

    SkillInstall {
        #[schemars(length(min = 1))]
        project: String,
        #[schemars(length(min = 1, max = 96))]
        #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
        skill_key: String,
        #[schemars(length(min = 1, max = 1024))]
        artifact_path: String,
        #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
        expected_artifact_sha256: String,
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
        #[schemars(extend("default" = false))]
        #[serde(default)]
        activate: Option<bool>,
        #[schemars(regex(pattern = "^wc_skillstate_[A-Za-z0-9_-]{43}$"))]
        #[serde(default)]
        expected_state_revision: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    SkillActivate {
        #[schemars(length(min = 1))]
        project: String,
        #[schemars(length(min = 1, max = 96))]
        #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
        skill_key: String,
        #[schemars(regex(pattern = "^wc_skillpkg_[A-Za-z0-9_-]{43}$"))]
        package_revision: String,
        #[schemars(regex(pattern = "^wc_skillstate_[A-Za-z0-9_-]{43}$"))]
        expected_state_revision: String,
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    SkillRemoveRevision {
        #[schemars(length(min = 1))]
        project: String,
        #[schemars(length(min = 1, max = 96))]
        #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
        skill_key: String,
        #[schemars(regex(pattern = "^wc_skillpkg_[A-Za-z0-9_-]{43}$"))]
        package_revision: String,
        #[schemars(regex(pattern = "^wc_skillstate_[A-Za-z0-9_-]{43}$"))]
        expected_state_revision: String,
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Atomically admit one new durable Goal with one exact Workflow Session correlation.
    /// This is Runtime/Store workflow composition only; it does not establish any Host carrier.
    PrepareGoalWorkflow {
        /// Exact Workflow Session independently re-authorized before durable admission.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Fixed durable completion intent; the Server does not evaluate natural-language conditions.
        /// At most 8 conditions, each additionally bounded to 512 UTF-8 bytes.
        #[serde(default)]
        #[schemars(schema_with = "goal_conditions_schema")]
        completion_conditions: Vec<String>,
        /// Fixed bounded plan. Stable ids are unique; all steps start pending.
        #[serde(default)]
        #[schemars(length(max = 32))]
        steps: Vec<GoalStepInputCall>,
        /// Bounded human-readable Goal title.
        #[schemars(length(min = 1, max = 200))]
        title: String,
        /// Bounded authoritative high-level objective/instruction.
        #[schemars(length(min = 1, max = 8192))]
        objective: String,
        /// Optional exact owned durable Agent used only as Goal attention-routing identity.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        controller_agent_id: Option<String>,
        /// Caller-generated composition key. Exact replay returns the same admitted Goal;
        /// changed reuse fails closed.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Create explicit high-level durable intent/control state without execution authority.
    CreateGoal {
        /// Fixed durable completion intent; the Server does not evaluate natural-language conditions.
        /// At most 8 conditions, each additionally bounded to 512 UTF-8 bytes.
        #[serde(default)]
        #[schemars(schema_with = "goal_conditions_schema")]
        completion_conditions: Vec<String>,
        /// Fixed bounded plan. Stable ids are unique; all steps start pending. Use checkpoint_goal
        /// for atomic progress at recovery-worthy milestones, not after every tool call.
        #[serde(default)]
        #[schemars(length(max = 32))]
        steps: Vec<GoalStepInputCall>,
        /// Bounded human-readable Goal title.
        #[schemars(length(min = 1, max = 200))]
        title: String,
        /// Bounded authoritative high-level objective/instruction. The Server additionally enforces an
        /// 8192-byte UTF-8 bound.
        #[schemars(length(min = 1, max = 8192))]
        objective: String,
        /// Optional exact durable Agent that receives future Goal attention. This routing identity is
        /// independently authorized and grants no Goal, Task, Project, Session, Runner, or execution authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        controller_agent_id: Option<String>,
        /// Caller-generated Goal creation key. Exact retry returns the same Goal; changed reuse fails
        /// closed.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Read one exact caller-owned durable Goal.
    GetGoal {
        /// Canonical durable Goal id. It is exact identity only and is never a bearer credential or
        /// execution selector.
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
    },

    /// Build the bounded read-only Goal Plan projection for one explicit App presentation.
    PresentGoalPlan {
        /// Canonical durable Goal id. It is exact identity only and is never a bearer credential or
        /// execution selector.
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
    },

    /// App-only effectful synchronization. The Server recomputes activity,
    /// current Window relation, Session/Goal/Project authority and epoch dedup,
    /// may commit one durable stall Attention/Wake, then returns the final bounded
    /// Goal Plan projection. No caller timing or authority selectors are accepted.
    GoalPlanSync {
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
    },

    /// Atomically checkpoint one Goal's mechanical plan and recovery summary.
    CheckpointGoal {
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
        /// Exact current Goal revision; stale writes fail closed.
        #[schemars(range(min = 1))]
        expected_revision: i64,
        /// Pending or in-progress steps to complete together. Duplicate/unknown ids fail closed.
        #[serde(default)]
        #[schemars(schema_with = "goal_step_ids_schema")]
        completed_step_ids: Vec<String>,
        /// Optional next/current step. Complete any other in-progress step in the same batch;
        /// a completed step can never be selected as current.
        #[serde(default)]
        #[schemars(regex(pattern = "^[A-Za-z0-9_-]{1,32}$"))]
        current_step_id: Option<String>,
        /// Required bounded recovery point; additionally limited to 2048 UTF-8 bytes.
        #[schemars(length(min = 1, max = 2048))]
        summary: String,
        /// Exact keyed replay does not mutate; changed reuse conflicts.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// List caller-visible durable Goals with an optional authoritative lifecycle filter.
    ListGoals {
        /// Closed authoritative Goal lifecycle. Execution/presentation states such as implementing,
        /// blocked, or waiting_validation are not Goal lifecycle values.
        #[serde(default)]
        lifecycle: Option<GoalLifecycleInput>,
        #[schemars(extend("default" = 0))]
        /// Bounded SQLite-compatible page offset.
        #[schemars(range(min = 0, max = 9223372036854775807i64))]
        #[serde(default)]
        offset: Option<usize>,
        #[schemars(extend("default" = 50))]
        /// Maximum number of caller-visible Goal summaries returned.
        #[schemars(range(min = 1, max = 100))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// CAS-update bounded Goal metadata or its closed lifecycle.
    UpdateGoal {
        /// Canonical durable Goal id. It is exact identity only and is never a bearer credential or
        /// execution selector.
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
        /// Exact observed Goal revision. Stale mutation fails closed and returns the current revision only.
        #[schemars(range(min = 1))]
        expected_revision: i64,
        /// Optional replacement Goal title.
        #[schemars(length(min = 1, max = 200))]
        #[serde(default)]
        title: Option<String>,
        /// Optional replacement objective. The Server additionally enforces an 8192-byte UTF-8 bound.
        #[schemars(length(min = 1, max = 8192))]
        #[serde(default)]
        objective: Option<String>,
        /// Optional replacement durable Goal controller Agent. Omission preserves the current routing identity;
        /// setting it re-authorizes the exact Agent and never inherits authority from prior routing.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        controller_agent_id: Option<String>,
        /// Closed authoritative Goal lifecycle. Execution/presentation states such as implementing,
        /// blocked, or waiting_validation are not Goal lifecycle values.
        #[serde(default)]
        lifecycle: Option<GoalLifecycleInput>,
        /// Optional bounded terminal reason; valid only with an explicit completed or cancelled transition.
        #[schemars(length(min = 1, max = 4096))]
        #[serde(default)]
        terminal_reason: Option<String>,
        /// Caller-generated Goal update key. Exact retry replays; changed reuse fails closed.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Explicitly correlate an owned Goal with an independently authorized AgentTask.
    AssociateGoalAgentTask {
        /// Canonical durable Goal id. It is exact identity only and is never a bearer credential or
        /// execution selector.
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
        /// Exact durable AgentTask id. Association is correlation only and never grants TaskAttempt,
        /// Project, Runner, or execution authority.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Caller-generated Goal-to-AgentTask association key. Exact retry replays; changed reuse fails
        /// closed.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Explicitly correlate an owned Goal with an independently authorized Workflow Session.
    AssociateGoalWorkflowSession {
        /// Canonical durable Goal id. It is exact identity only and is never a bearer credential or
        /// execution selector.
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: String,
        /// Exact Workflow Session id. The target Session is independently re-authorized before association;
        /// the correlation never grants Session or Project authority.
        #[schemars(regex(pattern = "^wc_sess_([A-Za-z0-9_-]{16}|[0-9a-f]{32})$"))]
        session_id: String,
        /// Caller-generated Goal-to-Workflow-Session association key. Exact retry replays; changed reuse
        /// fails closed.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Create one explicit durable one-shot interest in future AgentTask terminal facts.
    WaitForAgentEvents {
        /// Exact caller-owned durable Agent that will resume when this one-shot Wait triggers. Agent
        /// identity grants no source-domain authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Exact current Agent Endpoint used only as the Host presentation/carrier selector at Wait
        /// creation time; it is not persisted as Wait execution ownership.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        /// Exact current Endpoint controller generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        /// Closed bounded rendezvous mode. Omission is exactly equivalent to `any`; `all` triggers only
        /// after every registered exact source Task is terminal.
        #[serde(default)]
        #[schemars(schema_with = "agent_wait_mode_schema")]
        mode: AgentWaitModeCall,
        /// Optional exact Goal correlation context. When present, the Goal must be active, owned by the
        /// same principal, have `agent_id` as its explicit controller, and every selected source Task must
        /// already be explicitly correlated to this exact Goal and still be non-terminal. This reference
        /// grants no Goal, Task, Project, Session, or execution authority.
        #[serde(default)]
        #[schemars(regex(pattern = "^wc_goal_[A-Za-z0-9_-]{16}$"))]
        goal_id: Option<String>,
        /// One to eight exact AgentTask terminal selectors. Matching facts are recorded durably; under
        /// `any` the first match triggers, while under `all` only the final required match triggers.
        #[schemars(length(min = 1, max = 8))]
        events: Vec<AgentWaitEventSelectorCall>,
        /// Caller-generated Wait creation key. Exact replay returns the same Wait; changed reuse conflicts.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Read one exact caller-owned durable AgentWait.
    ReadAgentWait {
        /// Exact caller-owned durable AgentWait id. Identity alone grants no authority over its source
        /// Tasks.
        #[schemars(regex(pattern = "^wc_agent_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
    },

    /// Cancel one exact AgentWait before the durable Host-dispatch fence.
    CancelAgentWait {
        /// Exact caller-owned durable AgentWait to cancel before Host dispatch preparation.
        #[schemars(regex(pattern = "^wc_agent_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
        /// Caller-generated cancellation key. Exact retry replays; changed reuse conflicts.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// App-only exact read of one caller-owned AgentWait.
    AgentWaitState { wait_id: String },

    /// Create explicit durable Agent work independent from communication messages and execution backends.
    CreateAgentTask {
        /// Bounded AgentTask title.
        #[schemars(length(min = 1, max = 200))]
        title: String,
        /// Bounded execution instruction owned by the AgentTask. Conversation Message bodies are not copied
        /// implicitly; the Server enforces an 8192-byte UTF-8 bound.
        #[schemars(length(min = 1, max = 8192))]
        instruction: String,
        /// Optional explicit current assignee. Omit to create an unassigned Task; an unassigned Task cannot
        /// start an Attempt.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        assignee_agent_id: Option<String>,
        /// Optional authorized Conversation correlation only. Conversation participation does not grant
        /// AgentTask execution authority.
        #[schemars(regex(pattern = "^wc_conv_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        source_conversation_id: Option<String>,
        /// Optional exact Message correlation inside source_conversation_id. The Message does not become or
        /// control the Task.
        #[schemars(regex(pattern = "^wc_cmsg_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        source_message_id: Option<String>,
        /// Optional intended Project correlation only. AgentTask authorization never grants Project,
        /// Runner, filesystem, Job, or CodingAgent authority.
        #[schemars(length(min = 1, max = 256))]
        #[serde(default)]
        referenced_project_id: Option<String>,
        /// Caller-generated AgentTask creation key. Exact retry returns the same Task; changed reuse
        /// conflicts.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// List durable AgentTasks owned by the current communication principal.
    ListAgentTasks {
        /// Optional assignee filter within Tasks visible to the current owner principal.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        assignee_agent_id: Option<String>,
        #[schemars(extend("default" = 0))]
        #[schemars(range(min = 0))]
        #[serde(default)]
        offset: Option<usize>,
        #[schemars(extend("default" = 50))]
        /// Maximum AgentTasks returned. Values above 100 are accepted and clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Read one exact owned durable AgentTask and latest Attempt metadata.
    ReadAgentTask {
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
    },

    /// Explicitly assign or reassign an AgentTask to an owned durable Agent.
    AssignAgentTask {
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Explicit current durable Agent assignee. Agent identity does not grant Project or executor
        /// authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        assignee_agent_id: String,
    },

    /// Atomically create one fenced leased Attempt for the current assignee.
    StartAgentTaskAttempt {
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Explicit current durable Agent assignee. Agent identity does not grant Project or executor
        /// authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        assignee_agent_id: String,
        /// Caller-generated Attempt-start key. Exact retry returns the same attempt_id, attempt_fence,
        /// and attempt_ref, even if that Attempt later becomes stale.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Select the concrete Agent Endpoint continuation backend for one exact live Attempt.
    StartAgentTaskEndpointContinuation {
        /// Server-issued ~ta selector for one exact task, attempt, assignee, fence, and generation.
        /// Not a credential. Omit it when passing the explicit tuple.
        #[schemars(regex(pattern = "^~ta[1-9][0-9]{0,18}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt_ref: Option<String>,
        /// Canonical durable AgentTask id. Required with the explicit tuple. Omit it when using attempt_ref.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task_id: Option<String>,
        /// Exact durable AgentTaskAttempt id. Required with the explicit tuple. Omit it with attempt_ref.
        #[schemars(regex(pattern = "^wc_agent_task_attempt_[A-Za-z0-9_-]{16}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt_id: Option<String>,
        /// Explicit current assignee. Required with the explicit tuple. Agent identity grants no executor
        /// authority. Omit it when using attempt_ref.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assignee_agent_id: Option<String>,
        /// Opaque freshness fence from start_agent_task_attempt. Required with the explicit tuple. Not a
        /// bearer credential. Omit it when using attempt_ref.
        #[schemars(regex(pattern = "^wc_agent_task_fence_[A-Za-z0-9_-]{21}[AQgw]$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt_fence: Option<String>,
        /// Exact Attempt-local controller generation. Required with the explicit tuple. Stale generations
        /// fail closed. Omit it when using attempt_ref.
        #[schemars(range(min = 1))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        attempt_controller_generation: Option<i64>,
    },

    /// Explicitly dispatch the exact latest fenced AgentTaskAttempt to one durable CodingAgentRun.
    StartAgentTaskCodingRun {
        /// Exact registered Project id. It must equal AgentTask.referenced_project_id, which remains
        /// correlation only; this execution call independently re-authorizes Project write and
        /// CodingAgentRun authority.
        #[schemars(length(min = 1))]
        project: String,
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Exact durable AgentTaskAttempt id.
        #[schemars(regex(pattern = "^wc_agent_task_attempt_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
        /// Explicit current durable Agent assignee. Agent identity does not grant Project or executor
        /// authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        assignee_agent_id: String,
        /// Opaque exact-Attempt freshness fence returned by start_agent_task_attempt. It is not a bearer
        /// credential or idempotency key.
        #[schemars(regex(pattern = "^wc_agent_task_fence_[A-Za-z0-9_-]{21}[AQgw]$"))]
        attempt_fence: String,
        /// Exact current Attempt-local controller generation. Carrier replacement increments it without
        /// creating a new Attempt.
        #[schemars(range(min = 1))]
        attempt_controller_generation: i64,
        /// Logical Runner-advertised CodingAgent provider. The Server binds its exact provider instance;
        /// callers cannot supply provider_instance_id.
        #[schemars(length(min = 1, max = 64))]
        provider_id: String,
        /// Optional run-level CodingAgent config. It participates in the immutable Attempt binding intent;
        /// changing it after binding conflicts rather than creating a second Run.
        #[serde(default)]
        config: Option<BTreeMap<String, webcodex_core::coding_agent::CodingAgentConfigValue>>,
        #[schemars(extend("default" = 300))]
        /// Total CodingAgentRun budget. It participates in immutable binding intent.
        #[schemars(range(min = 1, max = 3600))]
        #[serde(default)]
        timeout_secs: Option<u64>,
    },

    /// Reconcile the exact durable CodingAgentRun already bound to one AgentTaskAttempt.
    ReconcileAgentTaskCodingRun {
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Exact durable AgentTaskAttempt whose already-bound CodingAgentRun must be reconciled. No old
        /// attempt fence is required because this operation can only consume authoritative backend truth.
        #[schemars(regex(pattern = "^wc_agent_task_attempt_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
    },

    /// Renew only the exact latest unexpired fenced AgentTaskAttempt.
    HeartbeatAgentTaskAttempt {
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Exact durable AgentTaskAttempt id.
        #[schemars(regex(pattern = "^wc_agent_task_attempt_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
        /// Explicit current durable Agent assignee. Agent identity does not grant Project or executor
        /// authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        assignee_agent_id: String,
        /// Opaque exact-Attempt freshness fence returned by start_agent_task_attempt. It is not a bearer
        /// credential or idempotency key.
        #[schemars(regex(pattern = "^wc_agent_task_fence_[A-Za-z0-9_-]{21}[AQgw]$"))]
        attempt_fence: String,
        /// Exact current Attempt-local controller generation. Carrier replacement increments it without
        /// creating a new Attempt.
        #[schemars(range(min = 1))]
        attempt_controller_generation: i64,
        /// Optional exact consumed A4b agent_task_attempt Wake proving this model-turn lineage. It grants
        /// no Task, Project, Runner, Goal, Session, or Endpoint authority and must be paired with
        /// active_turn_consume_token.
        #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        active_turn_wake_id: Option<String>,
        /// Optional opaque consume token for active_turn_wake_id. The Server verifies its durable hash
        /// together with normal Task authority and exact Attempt fences; it is never a standalone
        /// credential.
        #[schemars(regex(pattern = "^wc_wake_consume_[A-Za-z0-9_-]{21}[AQgw]$"))]
        #[serde(default)]
        active_turn_consume_token: Option<String>,
    },

    /// Commit exact fenced terminal AgentTaskAttempt truth with independent keyed replay.
    CompleteAgentTaskAttempt {
        /// Canonical durable AgentTask id. It is not a credential or Connector Task id.
        #[schemars(regex(pattern = "^wc_agent_task_[A-Za-z0-9_-]{16}$"))]
        task_id: String,
        /// Exact durable AgentTaskAttempt id.
        #[schemars(regex(pattern = "^wc_agent_task_attempt_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
        /// Explicit current durable Agent assignee. Agent identity does not grant Project or executor
        /// authority.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        assignee_agent_id: String,
        /// Opaque exact-Attempt freshness fence returned by start_agent_task_attempt. It is not a bearer
        /// credential or idempotency key.
        #[schemars(regex(pattern = "^wc_agent_task_fence_[A-Za-z0-9_-]{21}[AQgw]$"))]
        attempt_fence: String,
        /// Exact current Attempt-local controller generation. Carrier replacement increments it without
        /// creating a new Attempt.
        #[schemars(range(min = 1))]
        attempt_controller_generation: i64,
        /// Terminal AgentTask outcome. In A3, failed completion is terminal; only lease expiry before
        /// completion permits a later Attempt.
        outcome: String,
        /// Optional bounded terminal result metadata; full Conversation bodies or execution logs do not
        /// belong here.
        #[schemars(length(min = 1, max = 4096))]
        #[serde(default)]
        terminal_result: Option<String>,
        /// Optional bounded terminal reason metadata.
        #[schemars(length(min = 1, max = 4096))]
        #[serde(default)]
        terminal_reason: Option<String>,
        /// Caller-generated terminal completion key. Same key and same intent replay exactly; changed reuse
        /// conflicts. attempt_fence is never used as this key.
        #[schemars(length(min = 1, max = 128))]
        completion_key: String,
    },

    /// Create a durable Server-owned Agent identity and mutable self-description card.
    CreateAgentIdentity {
        /// Mutable non-unique Agent handle. It is self-description metadata, not canonical identity or
        /// authority.
        #[schemars(length(min = 1, max = 64))]
        #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
        handle: String,
        /// Mutable non-unique Agent display name.
        #[schemars(length(min = 1, max = 128))]
        display_name: String,
        #[schemars(extend("default" = ""))]
        /// Optional Agent Card description, bounded by 2048 UTF-8 bytes server-side.
        #[schemars(length(max = 2048))]
        #[serde(default)]
        description: Option<String>,
        /// Bounded self-description labels. Labels never grant authority.
        #[schemars(length(max = 16))]
        #[schemars(inner(length(min = 1, max = 64)))]
        #[serde(default)]
        specialty_labels: Vec<String>,
        /// Caller-generated operation key. Exact replay under the same communication principal returns the
        /// original durable resource; reuse with changed input is rejected.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// List Agent identities owned by the current communication principal.
    ListAgentIdentities {
        /// Optional exact canonical Agent id owned by the current communication principal.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        agent_id: Option<String>,
        #[schemars(extend("default" = 0))]
        /// Zero-based bounded page offset.
        #[schemars(range(min = 0))]
        #[serde(default)]
        offset: Option<usize>,
        #[schemars(extend("default" = 50))]
        /// Maximum records to return. Values above 100 are accepted and clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// CAS-update mutable Agent Card metadata without changing canonical identity.
    UpdateAgentIdentity {
        /// Canonical durable Agent id to update.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Exact profile revision fence. A stale value is rejected without mutation.
        #[schemars(range(min = 1))]
        expected_profile_revision: i64,
        #[schemars(length(min = 1, max = 64))]
        #[schemars(regex(pattern = "^[A-Za-z0-9._-]+$"))]
        #[serde(default)]
        handle: Option<String>,
        /// Replacement display name.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        display_name: Option<String>,
        /// Replacement description, bounded by 2048 UTF-8 bytes server-side.
        #[schemars(length(max = 2048))]
        #[serde(default)]
        description: Option<String>,
        /// Bounded self-description labels. Labels never grant authority.
        #[schemars(length(max = 16))]
        #[schemars(inner(length(min = 1, max = 64)))]
        #[serde(default)]
        specialty_labels: Option<Vec<String>>,
    },

    /// Rotate the server-local continuation Endpoint/controller generation for a durable Agent.
    RotateAgentContinuationEndpoint {
        /// Canonical durable Agent id owned by the current communication principal.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Server-local continuation adapter label, for example ChatGPT. This is recorded metadata only: it
        /// does not initiate, configure, or authorize an external connection.
        #[schemars(length(min = 1, max = 64))]
        host: String,
        /// Optional opaque host-local attachment label copied into server-local Endpoint metadata. It is
        /// not durable Agent identity, remote-host authority, or a connection selector.
        #[schemars(length(max = 128))]
        #[serde(default)]
        client_attachment_id: Option<String>,
        /// Caller-generated operation key. Exact replay under the same communication principal returns the
        /// original durable resource; reuse with changed input is rejected.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Compatibility name for server-local continuation Endpoint rotation.
    /// Attach a current Host/Client Endpoint to a durable Agent.
    AttachAgentEndpoint {
        /// Canonical durable Agent id owned by the current communication principal.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Server-local continuation adapter label, for example ChatGPT. This is recorded metadata only: it
        /// does not initiate, configure, or authorize an external connection.
        #[schemars(length(min = 1, max = 64))]
        host: String,
        /// Optional opaque host-local attachment label copied into server-local Endpoint metadata. It is
        /// not durable Agent identity, remote-host authority, or a connection selector.
        #[schemars(length(max = 128))]
        #[serde(default)]
        client_attachment_id: Option<String>,
        /// Caller-generated operation key. Exact replay under the same communication principal returns the
        /// original durable resource; reuse with changed input is rejected.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Present one exact Agent/Endpoint continuation controller card. Never infers a target.
    /// Pass agent_continuation_ref, or the explicit tuple. Do not pass both.
    PresentAgentContinuation {
        /// Server-issued ~ac selector for one exact Agent, Endpoint, and generation. Not a credential.
        /// Omit it when passing the explicit tuple.
        #[schemars(regex(pattern = "^~ac[1-9][0-9]{0,18}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_continuation_ref: Option<String>,
        /// Exact durable Agent id. Required with endpoint_id and expected_controller_generation when
        /// agent_continuation_ref is omitted. No current or recent Agent fallback.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        /// Exact Agent Endpoint id. Required with the explicit tuple. Omit it when using
        /// agent_continuation_ref.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        endpoint_id: Option<String>,
        /// Exact Server-assigned Endpoint generation. Required with the explicit tuple. Stale
        /// generations fail closed and the selector never follows a newer generation.
        #[schemars(range(min = 1))]
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_controller_generation: Option<i64>,
    },

    /// App-only bind of one live Host View to an exact freshly attached Endpoint generation.
    AgentContinuationBind {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only same-Window recovery for one exact naturally expired Endpoint.
    AgentContinuationRecoverEndpoint {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only exact Host heartbeat plus bounded authoritative state refresh.
    AgentContinuationState {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only pre-fence acquire through the durable Wake claim state machine.
    AgentContinuationWakeAcquire {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only crossing of the existing durable dispatch fence immediately before ui/message.
    AgentContinuationWakePrepare {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
        #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
        wake_id: String,
        #[schemars(regex(pattern = "^wc_wake_attempt_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
    },

    /// App-only record of Host dispatch acceptance or conservative post-fence uncertainty.
    AgentContinuationWakeFinish {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
        #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
        wake_id: String,
        #[schemars(regex(pattern = "^wc_wake_attempt_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
        outcome: String,
    },

    /// App-only best-effort withdrawal of one exact process-local Host View binding.
    AgentContinuationUnbind {
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// Detach an Endpoint while preserving the durable Agent.
    DetachAgentEndpoint {
        /// Canonical Agent Endpoint id attached by the current communication principal.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
    },

    /// Create a durable Conversation with the current Human principal and Agents.
    CreateConversation {
        /// Optional mutable room title.
        #[schemars(length(max = 200))]
        #[serde(default)]
        title: Option<String>,
        /// Owned Agent participants to add with the current Human principal.
        #[schemars(length(min = 1, max = 16))]
        #[schemars(inner(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$")))]
        agent_ids: Vec<String>,
        /// Caller-generated operation key. Exact replay under the same communication principal returns the
        /// original durable resource; reuse with changed input is rejected.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// List Conversations for a Human view or an explicitly attached Agent view.
    ListConversations {
        /// Optional Agent view. Must be paired with exact Endpoint fencing; omit all three fields for the
        /// current Human principal.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        agent_id: Option<String>,
        /// Active Endpoint proving the optional Agent view.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        endpoint_id: Option<String>,
        /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        #[serde(default)]
        expected_controller_generation: Option<i64>,
        #[schemars(extend("default" = 0))]
        /// Zero-based bounded page offset.
        #[schemars(range(min = 0))]
        #[serde(default)]
        offset: Option<usize>,
        #[schemars(extend("default" = 50))]
        /// Maximum records to return. Values above 100 are accepted and clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Read an ordered append-only Conversation transcript page.
    ReadConversation {
        /// Canonical Conversation id.
        #[schemars(regex(pattern = "^wc_conv_[A-Za-z0-9_-]{16}$"))]
        conversation_id: String,
        /// Optional Agent view. Must be paired with exact Endpoint fencing; omit all three fields for the
        /// current Human principal.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        agent_id: Option<String>,
        /// Active Endpoint proving the optional Agent view.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        endpoint_id: Option<String>,
        /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        #[serde(default)]
        expected_controller_generation: Option<i64>,
        #[schemars(extend("default" = 0))]
        /// Return append-only transcript messages with seq greater than this cursor.
        #[schemars(range(min = 0))]
        #[serde(default)]
        after_seq: Option<i64>,
        #[schemars(extend("default" = 50))]
        /// Maximum records to return. Values above 100 are accepted and clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Atomically append a Message and recipient-specific Agent deliveries.
    PostConversationMessage {
        /// Canonical Conversation id.
        #[schemars(regex(pattern = "^wc_conv_[A-Za-z0-9_-]{16}$"))]
        conversation_id: String,
        /// Append-only message body, bounded by 4096 UTF-8 bytes server-side.
        #[schemars(length(min = 1, max = 4096))]
        body: String,
        /// Agent author provenance. Omit for the current Human principal; Agent authors require
        /// endpoint_id.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        author_agent_id: Option<String>,
        /// Active Endpoint proving an Agent-authored message. Omit for Human authors.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        endpoint_id: Option<String>,
        /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        #[serde(default)]
        expected_controller_generation: Option<i64>,
        /// Optional explicit Agent Inbox recipients. Omit to deliver to every Agent participant except the
        /// author; an explicit empty array posts to the transcript/room without Agent deliveries.
        #[schemars(length(min = 0, max = 16))]
        #[schemars(inner(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$")))]
        #[serde(default)]
        recipient_agent_ids: Option<Vec<String>>,
        /// Optional parent Message in the same Conversation.
        #[schemars(regex(pattern = "^wc_cmsg_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        reply_to: Option<String>,
        /// Caller-generated operation key. Exact replay under the same communication principal returns the
        /// original durable resource; reuse with changed input is rejected.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        idempotency_key: Option<String>,
        /// Exact durable Wake that already crossed a Host dispatch or explicit-activation fence and
        /// provides stable resumed-turn reply replay identity. Use with reply_operation_index instead of
        /// idempotency_key.
        #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        wake_reply_id: Option<String>,
        /// Stable per-send index within one Wake. Reuse the same index only for an exact uncertain retry;
        /// use a different index for each intentional additional Message.
        #[schemars(range(min = 0, max = 31))]
        #[serde(default)]
        reply_operation_index: Option<i64>,
    },

    /// List queued deliveries for an Agent proven by an active Endpoint.
    ListAgentInbox {
        /// Canonical recipient Agent id.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Active Endpoint proving access to this Agent Inbox.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        #[schemars(extend("default" = 0))]
        /// Return queued deliveries with durable delivery_order greater than this cursor.
        #[schemars(range(min = 0))]
        #[serde(default)]
        after_delivery_order: Option<i64>,
        #[schemars(extend("default" = 50))]
        /// Maximum records to return. Values above 100 are accepted and clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Mark exact recipient-specific Agent deliveries consumed.
    ConsumeAgentDeliveries {
        /// Canonical recipient Agent id.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Active Endpoint proving access to this Agent Inbox.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        /// Deliveries to mark consumed. Repeating already-consumed ids is a safe desired-state retry.
        #[schemars(length(min = 1, max = 100))]
        #[schemars(inner(regex(pattern = "^wc_delivery_[A-Za-z0-9_-]{16}$")))]
        delivery_ids: Vec<String>,
    },

    /// Verify one exact Agent/Endpoint activation and return bounded current
    /// Conversation, Inbox, Wake, Host-binding, and reply-replay context.
    BootstrapAgentConversation {
        /// Exact durable Agent this active turn acts for.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Exact current Host Endpoint carrying this activation.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        /// Exact Server-assigned current Endpoint generation. Stale generations fail closed.
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        /// Optional explicit current Conversation. When omitted, an exact Wake may select its latest
        /// Conversation; no hidden Host selection is inferred.
        #[schemars(regex(pattern = "^wc_conv_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        conversation_id: Option<String>,
        /// Optional exact Wake identity from a continuation envelope or explicit pending-work activation.
        #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
        #[serde(default)]
        wake_id: Option<String>,
        /// Caller-generated key used only to accept/replay an eligible pending Inbox-style Wake through
        /// explicit activation into this already-active model turn. OMIT this field for agent_task_attempt,
        /// attention_event, and agent_wait_events continuations already dispatched by an Endpoint carrier;
        /// bootstrap those exact Wakes directly instead of converting them to explicit activation.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        activation_idempotency_key: Option<String>,
    },

    /// Consume one exact durable Agent Wake continuation without consuming Inbox deliveries.
    ConsumeAgentWake {
        /// Exact target Agent named by the durable Wake Intent.
        #[schemars(regex(pattern = "^wc_dagent_[A-Za-z0-9_-]{16}$"))]
        agent_id: String,
        /// Exact current Endpoint bound to this continuation. Host-adapter continuations require it to
        /// remain wake-capable; explicit_activation continuations do not.
        #[schemars(regex(pattern = "^wc_endpoint_[A-Za-z0-9_-]{16}$"))]
        endpoint_id: String,
        /// Server-assigned Endpoint generation carried by this exact continuation. Stale generations fail
        /// closed.
        #[schemars(range(min = 1))]
        expected_controller_generation: i64,
        /// Exact durable Wake Intent to consume. This is not a caller-generated retry key.
        #[schemars(regex(pattern = "^wc_wake_[A-Za-z0-9_-]{16}$"))]
        wake_id: String,
        /// Opaque exact-continuation token delivered by the Host adapter. It is bound to wake_id, target
        /// Agent, Endpoint, and generation; never substitute a new token or retry key.
        #[schemars(regex(pattern = "^wc_wake_consume_[A-Za-z0-9_-]{21}[AQgw]$"))]
        consume_token: String,
    },

    /// Search/list explicit durable project Memory. Model-hidden globally and
    /// exposed only when Stateless MCP 2026 admits the Memory protocol capability.
    MemorySearch {
        project: String,
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        expected_catalog_revision: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read one explicit durable project Memory body.
    MemoryRead {
        project: String,
        memory_key: String,
        #[serde(default)]
        expected_revision: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Create or CAS-update explicit durable project Memory guidance.
    MemorySet {
        project: String,
        memory_key: String,
        summary: String,
        #[serde(default)]
        body: Option<String>,
        #[serde(default)]
        priority: Option<String>,
        #[serde(default)]
        bootstrap: Option<bool>,
        #[serde(default)]
        tags: Option<Vec<String>>,
        #[serde(default)]
        expected_revision: Option<String>,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// CAS-delete explicit durable project Memory guidance.
    MemoryDelete {
        project: String,
        memory_key: String,
        expected_revision: String,
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Admin-only paginated inventory of durable project Memory scopes.
    MemoryScopeList {
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Admin-only explicit purge of one non-current durable Memory scope.
    MemoryScopePurge {
        memory_scope_id: String,
        expected_catalog_revision: String,
        #[serde(default)]
        confirm: bool,
    },

    /// Start an async background job (long-running commands, codex CLI, etc.).
    RunJob {
        /// Configured project id.
        project: String,
        /// Shell command to run asynchronously. At most 65536 UTF-8 bytes; use run_script for
        /// substantially larger typed program text and stdin/files/artifacts for large data.
        #[schemars(length(max = 65536))]
        command: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Maximum runtime in seconds.
        #[serde(default)]
        timeout_secs: Option<i64>,
        /// Working directory contract: without a Session SSH resource, omit, empty string, or '.' selects
        /// the project root and any other value is project-relative. With a named Session SSH resource, cwd
        /// is a remote path checked by the remote shell instead of the Runner project-root policy.
        #[serde(default)]
        cwd: Option<String>,
        /// Optional caller-declared evidence intent. Set it only when the execution has a real
        /// validation/test/build/format/release/diagnostic/operation classification; it never changes the
        /// command, authorization, or execution authority.
        #[serde(default)]
        purpose: Option<ExecutionPurpose>,
        /// Optional explicit command language: sh or bash. When omitted, local run_job preserves its
        /// existing bash contract, a Runner-backed run_job uses that Runner's configured shell, and a named
        /// Session SSH resource uses the remote login shell. The response always records the actual
        /// selection.
        #[serde(default)]
        shell: Option<ExecutionShell>,
    },

    /// Stop a bounded runtime job after explicit confirmation.
    StopJob {
        /// Configured project id that must match the job project.
        project: String,
        /// Existing runtime Job id to stop.
        job_id: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Must be true to stop or no-op an already-finished job; false returns confirmation_required.
        #[serde(default)]
        confirm: bool,
    },

    /// Observe up to eight existing Jobs using one shared bounded wait. Each
    /// item reuses the canonical single-Job observation-token and projection
    /// path; item failures are isolated and no Job is launched or modified.
    ObserveJobs {
        /// Existing Jobs to observe in input order. Each item supplies either a raw job_id
        /// (optionally with after_observation_token) or one compact observation_ref from a prior
        /// successful response. Selectors are mutually exclusive and duplicate resolved Jobs are rejected.
        #[schemars(schema_with = "observe_jobs_items_schema")]
        #[serde(deserialize_with = "deserialize_observe_jobs_items")]
        items: Vec<ObserveJobsItem>,
        #[serde(
            default = "default_observe_jobs_tail_lines",
            deserialize_with = "deserialize_observe_jobs_tail_lines"
        )]
        #[schemars(extend("default" = 40))]
        /// Global per-stream bound. Values above 200 are accepted and clamped to 200. First observations
        /// return a current tail; cursor-aware follow-ups return at most this many new or reset-recovery
        /// lines.
        #[schemars(range(min = 1))]
        tail_lines: usize,
        /// Optional one shared bounded wait (a maximum), never a minimum sleep or multiplied by item count.
        /// Omission or any item without a token returns an immediate observation/baseline. Values above 100
        /// seconds are clamped to 100. With tokens, wake_on selects early wake behavior; updates never
        /// extend the deadline. Runtime accepts explicit waits up to 100 seconds; MCP transport may clamp
        /// that wait further using the effective Server-side Host timing profile. Use terminal waits
        /// when further useful progress depends on terminal outcome; otherwise defer observation while
        /// independent work continues.
        #[schemars(range(min = 1))]
        #[serde(default, deserialize_with = "deserialize_observe_jobs_wait_secs")]
        wait_secs: Option<u64>,
        #[schemars(extend("default" = "change"))]
        /// Bounded-wait wake policy. change (default) returns on any observable change. terminal waits for
        /// any Job to be terminal; use it for one Job or when any terminal result unblocks progress.
        /// all_terminal waits for every Job in a predetermined set needed before progress. Both coalesce
        /// non-terminal log/progress/activity updates and return immediately on any item error or at the
        /// shared deadline. Deadline returns timeout even when changed=true; deltas remain relative to the
        /// caller's original tokens. No token means immediate baseline; no wait_secs means immediate
        /// observation.
        #[serde(default)]
        wake_on: ObserveJobsWakeOn,
        /// Opt-in projection for proven successful structured validation Jobs. Removes routine
        /// passed-test/progress lines only; preserves diagnostics, lifecycle, counts and log boundaries.
        /// A returned suggested_call expands retained logs from the original cursor, not the advanced
        /// observation token. Failures, unknown results and ordinary commands keep their full projection.
        #[serde(default)]
        #[schemars(extend("default" = false))]
        summary_only: bool,
    },

    /// Arm one caller-owned durable one-shot terminal attention for an exact
    /// existing Job. This never starts, retries, stops, or replaces execution.
    WaitForJobTerminal {
        /// Exact existing public Job identity. The Job is independently re-authorized; this value never
        /// starts, retries, stops, or replaces execution.
        #[schemars(length(min = 1, max = 128))]
        job_id: String,
        /// Caller-generated bounded operation key. Exact replay for the same Job returns the same durable
        /// terminal wait; changed reuse is rejected. It is registration identity only, never Job or retry
        /// identity.
        #[schemars(length(min = 1, max = 128))]
        idempotency_key: String,
    },

    /// Present one exact caller-owned Job terminal continuation App card.
    PresentJobTerminalContinuation {
        #[schemars(regex(pattern = "^wc_job_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
    },

    /// App-only bind of one live Host View to one exact Job terminal wait.
    JobTerminalContinuationBind {
        #[schemars(regex(pattern = "^wc_job_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only sparse state read for one exact current Job terminal binding.
    JobTerminalContinuationState {
        #[schemars(regex(pattern = "^wc_job_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only crossing of the existing durable Job terminal delivery fence.
    JobTerminalContinuationPrepare {
        #[schemars(regex(pattern = "^wc_job_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// App-only record of Host ui/message acceptance or conservative uncertainty.
    JobTerminalContinuationFinish {
        #[schemars(regex(pattern = "^wc_job_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
        #[schemars(regex(pattern = "^wc_job_delivery_[A-Za-z0-9_-]{16}$"))]
        attempt_id: String,
        outcome: String,
    },

    /// App-only best-effort withdrawal of one exact process-local Job View binding.
    JobTerminalContinuationUnbind {
        #[schemars(regex(pattern = "^wc_job_wait_[A-Za-z0-9_-]{16}$"))]
        wait_id: String,
        #[schemars(regex(pattern = "^wc_host_binding_[A-Za-z0-9_-]{21}[AQgw]$"))]
        binding_id: String,
    },

    /// List files in a Runner-registered project directory (bounded, read-only).
    /// Returns project-relative paths plus a file/dir kind. Routed to the
    /// owning registered Runner via the `file_list` op; the server never reads
    /// the Runner project path directly.
    ListProjectFiles {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative directory to list (default: project root).
        #[serde(default)]
        path: Option<String>,
        #[schemars(extend("default" = 200))]
        /// Maximum number of entries to return; runtime clamps to 1..500 (default 200).
        #[serde(default)]
        limit: Option<usize>,
        #[schemars(extend("default" = 0))]
        /// Zero-based entry offset for deterministic paging; use next_offset from the previous page.
        #[schemars(range(min = 0))]
        #[serde(default)]
        offset: Option<usize>,
    },

    /// List the project's tracked files from the Git index, with glob
    /// filtering and automatic directory rollup. Unlike `ListProjectFiles`
    /// (one directory, filesystem order) this answers "what is in this
    /// project" in a single bounded call and never descends into ignored
    /// directories such as `.venv` or `target`.
    ListProjectTrackedFiles {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative directory scope (default: project root). Rollup depth is counted
        /// inside this scope.
        #[serde(default)]
        path: Option<String>,
        /// Optional path globs; an entry matches if it matches any of them. Supports * (not crossing /), **
        /// (crossing /), and ?. A pattern without / also matches the basename, so *.py works at any depth.
        #[schemars(length(max = 20))]
        #[schemars(inner(length(min = 1, max = 256)))]
        #[serde(default)]
        globs: Option<Vec<String>>,
        /// Optional directory rollup depth, clamped to 1..16. Omit to list every file when the result fits
        /// limit, and otherwise roll up automatically to the deepest depth that does fit.
        #[serde(default)]
        depth: Option<usize>,
        #[schemars(extend("default" = 200))]
        /// Maximum entries to return; clamped to 1..1000 (default 200).
        #[serde(default)]
        limit: Option<usize>,
        /// Entry offset for paging; use the next_offset value from the previous page.
        #[serde(default)]
        offset: Option<usize>,
    },

    /// Return a deterministic, bounded, metadata-only overview of an
    /// Runner-registered project. The owning Runner scans directory entries;
    /// file contents are never read and no LLM is used.
    ProjectOverview {
        /// Full Runner runtime project id (legacy agent:<client_id>:<project_id> identity).
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional project-relative directory scope (default: project root).
        #[serde(default)]
        path: Option<String>,
        #[schemars(extend("default" = 2))]
        /// Bounded scan depth; defaults to 2 and is clamped by the runtime to 1..4.
        #[serde(default)]
        max_depth: Option<usize>,
        #[schemars(extend("default" = 200))]
        /// Bounded scanned-entry limit; defaults to 200 and is clamped by the runtime to 20..500.
        #[serde(default)]
        limit: Option<usize>,
    },

    /// Run up to eight independent bounded project-text searches under one
    /// project authorization and outer Session event.
    SearchProjectTexts {
        /// Runner-registered project id.
        project: String,
        /// One to eight independent bounded text-search queries, returned in request order.
        #[schemars(length(min = 1, max = 8))]
        #[serde(deserialize_with = "deserialize_search_project_texts_queries")]
        queries: Vec<SearchProjectTextsQuery>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = 65536))]
        /// Optional primary model-facing batch projection budget in bytes. Defaults to 64 KiB. Any
        /// recognized nonnegative integer is accepted and runtime-clamped to the fixed 8..512 KiB
        /// inspection bounds. Whole-query batch follow-up is returned as one parser-ready suggested_call
        /// when that complete suffix call fits the bounded model result; otherwise no raw cursor or
        /// oversized fake call is exposed. If the first remaining query cannot fit, Runtime may raise this
        /// budget, while hard-cap zero progress requires narrowing that query's limit/context/path.
        /// Independently bounded Session/continuity overlays remain outside this budget.
        #[schemars(range(min = 0))]
        #[serde(default)]
        max_result_bytes: Option<usize>,
    },

    /// Search for text and immediately read bounded source ranges around the
    /// returned matches in one model-visible round trip.
    SearchAndRead {
        /// Runner-registered project id.
        project: String,
        /// One bounded search query. Exactly one of `query` or `queries` is required.
        /// Runtime forces match mode and zero search context because source context
        /// is returned by the read phase.
        #[serde(default)]
        query: Option<SearchProjectTextsQuery>,
        /// Batch of 1..8 predetermined independent queries. Exactly one of `query`
        /// or `queries` is required; all queries share the global `max_reads` budget.
        #[schemars(length(max = 8))]
        #[serde(default)]
        queries: Option<Vec<SearchProjectTextsQuery>>,
        /// Optional explicit wc_sess_* Workflow Session id.
        #[serde(default)]
        session_id: Option<String>,
        #[schemars(extend("default" = 40))]
        /// Source lines to read before each match; clamped to 0..100.
        #[serde(default)]
        read_before: Option<usize>,
        #[schemars(extend("default" = 40))]
        /// Source lines to read after each match; clamped to 0..100.
        #[serde(default)]
        read_after: Option<usize>,
        #[schemars(extend("default" = 8))]
        /// Global maximum match-derived read requests across all queries; clamped to 1..8.
        #[serde(default)]
        max_reads: Option<usize>,
        /// When true, successful source reads return numbered text.
        #[serde(default)]
        with_line_numbers: Option<bool>,
    },

    /// Read-only model-facing git worktree summary for a project. Reports
    /// branch/head, parsed status counts/files, diff stat, warnings, suggested
    /// next actions, and optional bounded diff hunks. Routed to the owning
    /// agent.
    ShowChanges {
        /// Runner-registered project id.
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Include bounded diff hunks (default false).
        #[serde(default)]
        include_diff: Option<bool>,
        /// Maximum hunks to return when include_diff=true (clamped).
        #[serde(default)]
        max_hunks: Option<usize>,
        /// Maximum lines per hunk when include_diff=true (clamped).
        #[serde(default)]
        max_hunk_lines: Option<usize>,
        /// Optional recent session-event diagnostic projection (clamped). Omit or use 0 for the compact
        /// default, which keeps review signals and changed paths without event history.
        #[serde(default)]
        session_event_limit: Option<usize>,
    },

    /// List bounded runtime Job summaries for caller-visible Runner work.
    /// Never returns stdout/stderr bodies — only metadata (job_id, kind,
    /// status, project, timestamps, exit_code).
    ListJobs {
        /// Maximum number of job summaries to return after all filters. Values above 100 are accepted and
        /// clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional exact status filter (e.g. running, completed, failed).
        #[serde(default)]
        status: Option<String>,
        /// Optional exact full runtime Project id; filters only already caller-visible Jobs.
        #[schemars(length(min = 1, max = 512))]
        #[serde(default)]
        project: Option<String>,
        /// Optional exact Workflow Session id; filters only already caller-visible Jobs.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read bounded, sanitized persisted activity for the current Host Window.
    /// Window identity is accepted only from the adapter's ToolCallContext.
    CurrentWindowActivity {
        /// Maximum returned events, clamped to 1..50 (default 20).
        #[serde(default)]
        limit: Option<usize>,
        /// Include support/diagnostic events in the event list (default false).
        #[serde(default)]
        include_nonmeaningful: bool,
    },

    /// Return bounded stdout/stderr tails for a job. Defaults to a bounded tail
    /// so the console never reads full logs by default. When `after_observation_token`
    /// and `wait_secs` are both supplied, this is a single bounded wait (up to
    /// `wait_secs`, 1..=60) until the current opaque Job observation token
    /// differs or the Job becomes terminal; it is never a subscription or streaming
    /// connection.
    JobTail {
        job_id: String,
        #[serde(default)]
        tail_lines: Option<usize>,
        #[serde(default)]
        after_observation_token: Option<String>,
        #[serde(default)]
        wait_secs: Option<u64>,
    },

    /// Write a UTF-8 file in a project via the owning Runner. Creates new files
    /// and, with `overwrite=true` plus the current `expected_read_revision`,
    /// replaces existing ones without a stale read clobbering concurrent work.
    /// The server never reads the Runner filesystem directly; the write runs as
    /// a native agent file operation.
    WriteProjectFile {
        /// Runner-registered project id.
        project: String,
        /// Project-relative file path.
        path: String,
        /// UTF-8 file content (no NUL).
        content: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Allow intentional replacement of an existing file (default false); true requires
        /// expected_read_revision.
        #[serde(default)]
        overwrite: Option<bool>,
        /// Current read_revision returned by read_files for this exact Project/path snapshot. Required with
        /// overwrite=true; omit for new-file creation. ToolRuntime resolves it to the Runner wire SHA
        /// guard.
        #[schemars(range(min = 1, max = 9007199254740991u64))]
        #[serde(default)]
        expected_read_revision: Option<u64>,
    },

    /// Write a binary artifact in a project via the owning Runner. The payload is
    /// base64-encoded and decoded by the Runner's native artifact file-op path.
    SaveProjectArtifact {
        /// Runner-registered project id.
        project: String,
        /// Project-relative output path.
        path: String,
        /// Base64-encoded binary content.
        content_base64: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional MIME type.
        #[serde(default)]
        mime_type: Option<String>,
        /// Allow overwriting an existing file (default false).
        #[serde(default)]
        overwrite: Option<bool>,
    },

    /// Import host-provided ChatGPT conversation attachments without routing
    /// temporary OpenAI download URLs or raw attachment bytes to the model or Runner.
    ImportConversationFilesToProject {
        /// Runner-registered project id.
        project: String,
        #[schemars(length(min = 1, max = 10))]
        #[serde(rename = "openaiFileIdRefs")]
        openai_file_id_refs: Vec<OpenAiHostFileRef>,
        /// Optional project-relative output directory; defaults to artifacts/imports.
        #[serde(default)]
        output_dir: Option<String>,
        /// Optional per-file output filenames, in attachment order.
        #[serde(default)]
        targets: Option<Vec<String>>,
        /// Allow overwriting existing files (default false).
        #[serde(default)]
        overwrite: Option<bool>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Internal host-file provenance set only by a trusted protocol
        /// adapter. Never deserialized from model/caller arguments and never
        /// serialized back out.
        #[serde(skip)]
        host_file_import_provenance: HostFileImportProvenance,
    },

    /// Stream one exact source artifact snapshot directly from one Project to
    /// another through Control, without Host attachments or model-facing base64.
    TransferProjectArtifact {
        /// Exact or resolvable source Runtime Project.
        source_project: String,
        /// Project-relative source artifact path.
        source_path: String,
        /// Exact or resolvable destination Runtime Project.
        destination_project: String,
        /// Project-relative destination artifact path.
        destination_path: String,
        /// Allow replacing an existing destination artifact (default false).
        #[serde(default)]
        overwrite: Option<bool>,
    },

    /// Preferred unified read-side facade for Project artifacts. Physical
    /// dispatch remains action-specific: Runner-backed metadata/inspection and
    /// MCP presentation/authority for native images and complete export.
    ProjectArtifact {
        /// Runner-registered project id.
        project: String,
        /// Project-relative artifact path.
        path: String,
        /// metadata, inspect, image, or export.
        action: ProjectArtifactAction,
        /// Optional compatible wc_sess_* Workflow Session id.
        #[serde(default)]
        session_id: Option<String>,
        /// metadata only; missing => exists=false.
        #[serde(default)]
        allow_missing: Option<bool>,
        /// inspect only; byte offset (default 0).
        #[serde(default)]
        offset: Option<usize>,
        /// inspect only; bytes (default 32768, max 65536).
        #[schemars(range(min = 1, max = 65536))]
        #[serde(default)]
        length: Option<usize>,
        /// inspect only; 64-char lowercase SHA-256 fence.
        #[schemars(length(min = 64, max = 64))]
        #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
        #[serde(default)]
        expected_sha256: Option<String>,
    },

    /// Read bounded metadata for a binary project artifact. Zip files are
    /// counted but never extracted.
    ReadProjectArtifactMetadata {
        /// Runner-registered project id.
        project: String,
        /// Project-relative artifact path.
        path: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// When true, a missing artifact returns exists=false instead of an error.
        #[serde(default)]
        allow_missing: Option<bool>,
    },

    /// Read one bounded binary content segment for a project artifact. Returns
    /// base64 for the requested chunk plus full-file sha256 and MIME metadata.
    /// MCP callers may request one complete, size-limited PNG/JPEG/WebP for
    /// native image content framing; that transport-only option is deliberately
    /// not part of the generic REST/GPT Actions schema.
    ReadProjectArtifact {
        /// Runner-registered project id.
        project: String,
        /// Project-relative artifact path.
        path: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional encoding; only base64 is supported (default base64).
        #[serde(default)]
        encoding: Option<String>,
        /// Optional byte offset to start reading from; defaults to 0.
        #[serde(default)]
        offset: Option<usize>,
        /// Optional chunk length in bytes; defaults to 32768 and cannot exceed 65536.
        #[schemars(range(min = 1, max = 65536))]
        #[serde(default)]
        length: Option<usize>,
        /// Optional exact full-file snapshot fence. Normally do not invent or manually transfer it: Runtime
        /// carries the observed sha256 in parser-ready ranged-read continuation. If current content no
        /// longer matches, the read fails closed before changed bytes are returned.
        #[schemars(length(min = 64, max = 64))]
        #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
        #[serde(default)]
        expected_sha256: Option<String>,
        #[schemars(skip)]
        #[serde(default)]
        as_image: Option<bool>,
    },

    /// Begin a chunked binary artifact upload bounded to 256 MiB. The agent
    /// creates a project-local temporary upload file and returns an opaque id.
    ArtifactUploadBegin {
        /// Runner-registered project id.
        project: String,
        /// Project-relative output path.
        path: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
        /// Optional final byte count guard; cannot exceed 268435456 bytes (256 MiB).
        #[serde(default)]
        expected_bytes: Option<usize>,
        /// Optional final sha256 guard.
        #[serde(default)]
        expected_sha256: Option<String>,
        /// Optional MIME type.
        #[serde(default)]
        mime_type: Option<String>,
        /// Allow overwriting an existing file at finish (default false).
        #[serde(default)]
        overwrite: Option<bool>,
    },

    /// Append one base64-encoded chunk, at most 1 MiB decoded, to an upload.
    ArtifactUploadChunk {
        /// Runner-registered project id.
        project: String,
        /// Required project-relative path; must exactly match the path used in artifact_upload_begin to
        /// bind upload_id to the target.
        path: String,
        /// Opaque wc_upload_* id from artifact_upload_begin.
        upload_id: String,
        /// Expected current upload byte offset.
        offset: usize,
        /// Base64-encoded chunk; decoded chunk max is 1048576 bytes (1 MiB).
        content_base64: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Verify and atomically commit a bounded artifact upload.
    ArtifactUploadFinish {
        /// Runner-registered project id.
        project: String,
        /// Required project-relative path; must exactly match the path used in artifact_upload_begin to
        /// bind upload_id to the target.
        path: String,
        /// Opaque wc_upload_* id from artifact_upload_begin.
        upload_id: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Abort a bounded artifact upload and remove its temporary files.
    ArtifactUploadAbort {
        /// Runner-registered project id.
        project: String,
        /// Required project-relative path; must exactly match the path used in artifact_upload_begin to
        /// bind upload_id to the target.
        path: String,
        /// Opaque wc_upload_* id from artifact_upload_begin.
        upload_id: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Apply a bounded transactional batch of edit/create/delete/rename file
    /// changes via the owning Runner. Whole-file and positional changes carry
    /// a model-facing read revision; globally unique local exact edits may omit
    /// it. Every change is preflighted before the first mutation. `dry_run` computes the full plan without writing. Model/API
    /// exposure is derived from the canonical ToolDefinition surface.
    ApplyTextEdits {
        /// Runner-registered project id.
        project: String,
        /// Transactional list of 1..16 file changes. For multiple independent edits to the same file, use ONE
        /// change with multiple entries in edits. Never repeat a source or destination path in changes.
        /// All edits in one file change resolve against the same original source snapshot.
        #[schemars(length(min = 1, max = 16))]
        changes: Vec<ApplyFileChangeInput>,
        /// If true, compute the plan without writing.
        #[serde(default)]
        dry_run: Option<bool>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read-only workspace hygiene inspection. Detects pollution risks before
    /// deployment smoke, model handoff, or real development: dirty worktree,
    /// untracked temporary/smoke/anchor files, cache directories, secret-like
    /// path names, and large untracked files. Never cleans, deletes, restores,
    /// or modifies the project. Never reads file contents, env values, tokens,
    /// or stdout/stderr bodies. Suspicious secret files are identified by
    /// path/name only. Model/API exposure is derived from the canonical
    /// ToolDefinition surface.
    WorkspaceHygieneCheck {
        /// Runtime project id.
        project: String,
        /// Maximum findings to return. Defaults to 50; values above 200 are accepted and clamped to 200.
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_findings: Option<usize>,
        /// Also report tracked suspicious path names (default false). When false, only untracked entries
        /// and the dirty-worktree summary are reported. Never reads file contents.
        #[serde(default)]
        include_tracked: Option<bool>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// List all Runner-registered runtime Projects.

    /// Probe Runner-side language-server availability without starting it.
    LspStatus {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Hierarchical document symbols for a project-relative supported source file.
    DocumentSymbols {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Project-relative UTF-8 path to a supported source file.
        path: String,
        #[schemars(extend("default" = 100))]
        /// Maximum symbol nodes to return (default 100, clamped to 1..500).
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Latest bounded language-server diagnostics for a project-relative supported source file.
    DocumentDiagnostics {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Project-relative UTF-8 path to a supported source file.
        path: String,
        #[schemars(extend("default" = 100))]
        /// Maximum normalized diagnostics to return (default 100, clamped to 1..200).
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Hover information at a 1-based Unicode scalar position.
    Hover {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Project-relative UTF-8 path to a supported source file.
        path: String,
        /// 1-based line number.
        #[schemars(range(min = 1))]
        line: usize,
        /// 1-based Unicode scalar column (end-of-line caret allowed at length+1).
        #[schemars(range(min = 1))]
        column: usize,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Bounded workspace symbols matching a non-empty query.
    WorkspaceSymbols {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Non-empty workspace symbol query after trimming (1..200 characters).
        #[schemars(length(min = 1, max = 200))]
        query: String,
        #[schemars(extend("default" = 50))]
        /// Maximum workspace symbols to return (default 50, clamped to 1..200).
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Goto definition at a 1-based Unicode scalar position.
    GotoDefinition {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Project-relative UTF-8 path to a supported source file.
        path: String,
        /// 1-based line number.
        #[schemars(range(min = 1))]
        line: usize,
        /// 1-based Unicode scalar column (end-of-line caret allowed at length+1).
        #[schemars(range(min = 1))]
        column: usize,
        #[schemars(extend("default" = 20))]
        /// Maximum locations to return (default 20, clamped to 1..100).
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Find references at a 1-based Unicode scalar position.
    FindReferences {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Project-relative UTF-8 path to a supported source file.
        path: String,
        /// 1-based line number.
        #[schemars(range(min = 1))]
        line: usize,
        /// 1-based Unicode scalar column (end-of-line caret allowed at length+1).
        #[schemars(range(min = 1))]
        column: usize,
        #[schemars(extend("default" = true))]
        /// Include the declaration in results (default true).
        #[serde(default = "default_true")]
        include_declaration: bool,
        #[schemars(extend("default" = 50))]
        /// Maximum locations to return (default 50, clamped to 1..200).
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Bounded incoming/outgoing semantic call hierarchy at a source position.
    CallHierarchy {
        /// Full Runner runtime project id (legacy wire form agent:<client_id>:<project_id>).
        project: String,
        /// Project-relative UTF-8 path to a supported source file.
        path: String,
        /// 1-based line number.
        #[schemars(range(min = 1))]
        line: usize,
        /// 1-based Unicode scalar column (end-of-line caret allowed at length+1).
        #[schemars(range(min = 1))]
        column: usize,
        #[schemars(extend("default" = "both"))]
        /// Call direction: incoming, outgoing, or both (default both).
        #[serde(default)]
        direction: CallHierarchyDirection,
        #[schemars(extend("default" = 1))]
        /// Breadth-first traversal depth (default 1, maximum 2).
        #[schemars(range(min = 1, max = 2))]
        #[serde(default = "default_call_hierarchy_depth")]
        depth: usize,
        #[schemars(extend("default" = 50))]
        /// Global flattened edge result ceiling (default 50). Positive values above 100 are accepted and
        /// clamped to 100.
        #[schemars(range(min = 1))]
        #[serde(default = "default_call_hierarchy_limit")]
        limit: usize,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[serde(default)]
        session_id: Option<String>,
    },

    /// Read-only Browser observation gateway with closed typed actions.
    BrowserObserve(BrowserObserveToolCall),

    /// Effectful Browser gateway with action-sensitive authority resolved before dispatch.
    BrowserAct(BrowserActToolCall),

    /// Read-only Computer observation gateway. The closed action enum preserves exact per-action semantics.
    ComputerObserve(ComputerObserveToolCall),

    /// Effectful Computer control gateway. Exact action authority/capability is resolved before dispatch.
    ComputerControl(ComputerControlToolCall),

    /// Capture one exact window snapshot and persist it directly as a create-only project artifact.
    ComputerSaveSnapshot {
        /// Target project that will receive the create-only snapshot artifact.
        #[schemars(length(min = 1))]
        project: String,
        /// Project-relative artifact path. The first version is create-only and never overwrites an
        /// existing file.
        #[schemars(length(min = 1, max = 4096))]
        path: String,
        /// Exact Runner client_id whose desktop is observed.
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        /// Opaque process-local surface_id returned by computer_observe(action=windows).
        #[schemars(length(min = 1, max = 128))]
        surface_id: String,
        /// Optional rectangle in the revalidated surface coordinate space. It must fit fully inside the
        /// exact surface.
        #[serde(default)]
        region: Option<ComputerSnapshotRegion>,
        /// Optional upper bound on encoded output width. Values above 4096 are clamped to 4096. Never
        /// upscales.
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_width: Option<u32>,
        /// Optional upper bound on encoded output height. Values above 4096 are clamped to 4096. Never
        /// upscales.
        #[schemars(range(min = 1))]
        #[serde(default)]
        max_height: Option<u32>,
        /// Optional explicit wc_sess_* Workflow Session id from a prior compatible bootstrap. When
        /// provided, this tool call is recorded in that exact Session ledger; omission leaves the call
        /// unlinked to Workflow Session state.
        #[schemars(length(min = 1))]
        #[serde(default)]
        session_id: Option<String>,
    },

    ListProjects {
        /// Exact Runner client_id. Filters only caller-visible Projects on that Runner.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        client_id: Option<String>,
        /// Exact full runtime Project id (agent:<client_id>:<project_id>).
        #[schemars(length(min = 1, max = 512))]
        #[serde(default)]
        project: Option<String>,
        /// Bounded deterministic case-insensitive text filter over already-visible Project metadata.
        #[schemars(length(min = 1, max = 200))]
        #[serde(default)]
        query: Option<String>,
        /// Maximum Projects returned after all filters. Values above 100 are accepted and clamped to 100.
        /// Omit to preserve the legacy full visible registry result.
        #[schemars(range(min = 1))]
        #[serde(default)]
        limit: Option<usize>,
        /// Return a compact workspace-selection projection without paths, revisions, or broad smoke
        /// metadata.
        #[serde(default)]
        summary_only: bool,
    },

    /// Register an existing directory as a WebCodex project on a selected
    /// agent. The Runner validates the path against its own policy, writes a
    /// project registration record `<project_registry_dir>/<id>.toml` atomically, and refreshes its local
    /// Project list. The Server refreshes its cached Project summaries for
    /// that Runner so `list_projects` sees the new Project immediately. This is
    /// a mutating Runner-side operation constrained by Runner policy; the Server
    /// never writes Project config files on the Runner host directly.
    RegisterProject {
        /// Registered Runner client_id.
        client_id: String,
        /// Project id (ASCII letters, digits, '-', '_'; no slash).
        id: String,
        /// Human-readable project name, bounded to 120 UTF-8 bytes server-side.
        name: String,
        /// Existing absolute directory path on the Runner. Git is not required.
        path: String,
        /// Optional project description, bounded to 500 UTF-8 bytes server-side.
        #[serde(default)]
        description: Option<String>,
        /// Allow patch operations on this project (default true).
        #[serde(default = "default_true")]
        allow_patch: bool,
        /// Overwrite an existing project config file (default false).
        #[serde(default)]
        overwrite: bool,
    },

    /// Unregister one exact Runner project registration using a revision
    /// observed from `list_projects`. This removes only WebCodex registration
    /// state; it never deletes the project directory, Git worktree, or branch.
    /// The target deliberately bypasses generic project pre-resolution so a
    /// terminal `already_unregistered` Runner outcome remains representable.
    UnregisterProject {
        /// Exact full runtime project id returned by list_projects (agent:<client_id>:<project_id>).
        project: String,
        /// Exact sha256 revision returned by the same list_projects observation; stale revisions fail
        /// closed.
        expected_revision: String,
    },

    /// Create a new directory on the selected Runner, or explicitly adopt an
    /// already-existing empty directory, and register it as a WebCodex project.
    /// The Runner validates the path against its own policy, creates or adopts
    /// the directory (and may add requested template files / git init), writes
    /// a project registration record `<project_registry_dir>/<id>.toml` atomically, and refreshes its local
    /// Project list. The Server refreshes its cached Project summaries so
    /// `list_projects` sees the new Project immediately. This is a mutating
    /// Runner-side operation constrained by Runner policy; the Server never
    /// creates directories or writes Project config files on the Runner host
    /// directly.
    CreateProject {
        /// Registered Runner client_id.
        client_id: String,
        /// Project id (ASCII letters, digits, '-', '_'; no slash).
        id: String,
        /// Human-readable project name, bounded to 120 UTF-8 bytes server-side.
        name: String,
        /// Absolute directory path to create and register on the Runner. If it already exists, it must be
        /// empty and adopt_existing_empty must be true.
        path: String,
        /// Optional project registration description, bounded to 500 UTF-8 bytes server-side. The 'empty'
        /// template never creates project files from this metadata; the 'basic' template also includes it
        /// in generated README.md content.
        #[serde(default)]
        description: Option<String>,
        /// Allow patch operations on this project (default true).
        #[serde(default = "default_true")]
        allow_patch: bool,
        /// Template: 'empty' (default; generates no project files) or 'basic' (generates README.md and
        /// .gitignore). git_init is a separate explicit side effect.
        #[serde(default)]
        template: Option<String>,
        /// Initialize git in the new directory (default false).
        #[serde(default)]
        git_init: bool,
        /// Adopt an already-existing empty target directory instead of requiring create_project to create
        /// it (default false). Non-empty directories are always rejected.
        #[serde(default)]
        adopt_existing_empty: bool,
        /// Overwrite an existing project config file (default false).
        #[serde(default)]
        overwrite: bool,
    },

    /// List connected Runners.
    ListRunners {
        /// Exact single Runner client_id. Mutually exclusive with client_ids.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        client_id: Option<String>,
        /// Bounded exact Runner client_ids. Mutually exclusive with client_id; duplicates are rejected.
        #[schemars(length(min = 1, max = 8))]
        #[schemars(inner(length(min = 1, max = 128)))]
        #[schemars(extend("uniqueItems" = true))]
        #[serde(default)]
        client_ids: Option<Vec<String>>,
        /// When false, omit Project bodies while retaining each Runner project count. Defaults to true for
        /// compatibility.
        #[serde(default)]
        include_projects: Option<bool>,
        /// Return compact Runner identity, health, build, project-count, and shared Job-concurrency facts.
        #[serde(default)]
        summary_only: bool,
    },

    /// Validate the candidate at one exact Runner process's startup-bound config
    /// path without mutating active configuration or instantiating providers.
    RunnerConfigCheck {
        /// Exact caller-visible Runner client_id whose startup-bound runner.toml candidate is checked. No
        /// filesystem path is accepted.
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
    },

    /// Activate the disk candidate on one exact Runner process with an optimistic
    /// active-generation fence. The Runner never accepts a filesystem path here.
    RunnerConfigReload {
        /// Exact caller-visible Runner client_id whose startup-bound runner.toml candidate is activated. No
        /// filesystem path is accepted.
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        /// Optimistic active-config generation fence previously observed from runner_config_check,
        /// runtime_status, or list_runners. A mismatch is rejected before candidate validation or mutation.
        #[schemars(range(min = 1))]
        expected_generation: u64,
    },

    /// Stable gateway for Runner-owned native Tool Plugins. The static
    /// ToolDefinition is a worst-case discovery contract; execution policy is
    /// classified from `action` before scope/session/permission governance.
    PluginTool(PluginToolCall),

    /// Stable gateway for Runner-local managed SSH resources. The static
    /// ToolDefinition is a worst-case discovery contract; exact execution
    /// policy is classified from `action` before specialized governance.
    SshResource(SshResourceToolCall),

    /// Return a structured runtime health/observability summary.
    ///
    /// This is a read-only observability tool: it never exposes tokens,
    /// secrets, full env, or stdout/stderr. It returns service metadata,
    /// Project config status, Runner summaries, and Job counts.
    RuntimeStatus {
        /// True selects sparse health, Project/Job counts and protocol/build/source alignment without
        /// inventories. Canonical/API default is false (full diagnostics); MCP defaults omission to true.
        #[serde(default)]
        compact: bool,
        /// Alias for compact=true. Returns the same compact runtime observability shape. Defaults to false.
        #[serde(default)]
        summary_only: bool,
        /// Exact caller-visible Runner client_id. Focused source alignment evaluates only this Runner; omit
        /// for fleet-wide status.
        #[schemars(length(min = 1, max = 128))]
        #[serde(default)]
        client_id: Option<String>,
    },

    /// Admin-only bounded read of one Server-hosted full tool-request trace.
    ReadToolTrace {
        trace_ref: String,
        #[serde(default)]
        offset: Option<usize>,
        #[serde(default)]
        limit: Option<usize>,
        #[serde(default)]
        payload_index: Option<usize>,
    },

    /// Return a compact, bounded tool manifest with categories, risk summary,
    /// recommended flows, and optional intent-shaped tool views. Intent views
    /// only filter and rank discovery output; they do not change tool behavior,
    /// policy, permissions, execution, or finish verdict semantics. Intended as
    /// a lightweight alternative to `list_tools` for long-running tasks where
    /// full catalog schemas cause ResponseTooLargeError. Read-only runtime
    /// introspection; list/filter mode stays schema-free, while exact tool_name
    /// mode exposes only that tool's input schema. Never exposes tokens, secrets,
    /// internal paths, or output schemas.
    ToolManifest {
        #[schemars(length(min = 1, max = 128))]
        /// Optional exact model-visible runtime tool name for one-tool contract discovery.
        #[serde(default)]
        tool_name: Option<String>,
        /// Optional category filter (e.g. session, edit, git, checkpoint, runtime, job, validation).
        /// Distinct from intent.
        #[serde(default)]
        category: Option<String>,
        /// Optional task intent view such as coding, audit, exploration,
        /// release, or discovery. Distinct from `category`. Discovery filtering
        /// only; does not change tool behavior or finish verdict semantics.
        #[serde(default)]
        intent: Option<String>,
        /// Include recommended_flows in the output. Omission defaults to false for exact tool_name lookup
        /// and true for category, intent, or broad discovery.
        #[serde(default = "default_true")]
        include_recommended_flows: bool,
        /// Request aggregate risk_summary where the selected projection exposes it (default true).
        /// Unfiltered/full discovery can return the aggregate; sparse filtered discovery omits it and
        /// carries per-tool risk only when needed for selection. This flag does not change authority,
        /// permission, or tool behavior.
        #[serde(default = "default_true")]
        include_risk_summary: bool,
    },
}

fn validate_coding_project_source_shape(tool_name: &str, arguments: &Value) -> Result<(), String> {
    let Some(arguments) = arguments.as_object() else {
        return Ok(());
    };
    let project = arguments.contains_key("project");
    let client_id = arguments.contains_key("client_id");
    let path = arguments.contains_key("path");
    for field in ["project", "client_id", "path"] {
        if arguments
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(format!(
                "invalid arguments for tool '{tool_name}': {field} must not be empty"
            ));
        }
    }
    if let Some(path) = arguments.get("path").and_then(Value::as_str) {
        if let Err(error) = validate_project_op_path(path) {
            return Err(format!("invalid arguments for tool '{tool_name}': {error}"));
        }
    }
    if project {
        let mut conflicts = Vec::new();
        if client_id {
            conflicts.push("client_id");
        }
        if path {
            conflicts.push("path");
        }
        return if conflicts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "invalid arguments for tool '{tool_name}': conflicting fields project and {}",
                conflicts.join(", ")
            ))
        };
    }
    if path {
        return if client_id {
            Ok(())
        } else {
            Err(format!(
                "invalid arguments for tool '{tool_name}': missing client_id required with path"
            ))
        };
    }
    if client_id {
        return Err(format!(
            "invalid arguments for tool '{tool_name}': missing path required with client_id"
        ));
    }
    Err(format!(
        "invalid arguments for tool '{tool_name}': missing project source; expected project or client_id + path"
    ))
}

fn validate_project_artifact_arguments(name: &str, arguments: &Value) -> Result<(), String> {
    if name != "project_artifact" {
        return Ok(());
    }
    let Some(object) = arguments.as_object() else {
        return Ok(()); // serde reports the canonical object-shape error below.
    };
    let action = object
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "invalid arguments for tool 'project_artifact': action is required".to_string()
        })?;
    let action_fields: &[&str] = match action {
        "metadata" => &["allow_missing"],
        "inspect" => &["offset", "length", "expected_sha256"],
        "image" | "export" => &[],
        _ => {
            return Err(format!(
                "invalid arguments for tool 'project_artifact': unsupported action '{action}'; expected metadata, inspect, image, or export"
            ))
        }
    };
    let common = ["project", "path", "action", "session_id"];
    let invalid = object
        .keys()
        .map(String::as_str)
        .filter(|key| !common.contains(key) && !action_fields.contains(key))
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "invalid arguments for tool 'project_artifact': action={action} does not accept field(s) {}",
            invalid.join(", ")
        ))
    }
}

fn validate_read_project_artifact_expected_sha256(
    name: &str,
    arguments: &Value,
) -> Result<(), String> {
    if !matches!(name, "read_project_artifact" | "project_artifact") {
        return Ok(());
    }
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let Some(value) = object.get("expected_sha256") else {
        return Ok(());
    };
    let valid = value.as_str().is_some_and(|value| {
        value.len() == 64
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "invalid arguments for tool '{name}': expected_sha256 must be exactly 64 lowercase hexadecimal characters"
        ))
    }
}

fn validate_structured_validation_sync_wait(name: &str, arguments: &Value) -> Result<(), String> {
    if !matches!(name, "cargo_fmt" | "cargo_check" | "cargo_test" | "go_test") {
        return Ok(());
    }
    let Some(object) = arguments.as_object() else {
        return Ok(());
    };
    let Some(sync_wait_value) = object.get("sync_wait_secs") else {
        return Ok(());
    };
    if sync_wait_value.is_null() {
        return Ok(());
    }
    let Some(sync_wait_secs) = sync_wait_value.as_u64() else {
        return Ok(()); // serde reports the canonical type error below.
    };
    if sync_wait_secs == 0 {
        return Err(format!(
            "invalid arguments for tool '{name}': sync_wait_secs must be at least 1"
        ));
    }
    Ok(())
}

fn canonicalize_cargo_check_packages(name: &str, arguments: &mut Value) -> Result<(), String> {
    if name != "cargo_check" {
        return Ok(());
    }
    let Some(object) = arguments.as_object_mut() else {
        return Ok(());
    };
    let package_present = object.get("package").is_some_and(|value| !value.is_null());
    let packages_present = object.get("packages").is_some_and(|value| !value.is_null());
    if package_present && packages_present {
        return Err(
            "invalid arguments for tool 'cargo_check': package and packages are mutually exclusive"
                .to_string(),
        );
    }

    let package = object.get("package").and_then(Value::as_str);
    let packages = match object.get("packages") {
        Some(Value::Array(values)) => values
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.into_iter().map(str::to_string).collect::<Vec<_>>()),
        Some(Value::Null) | None => None,
        Some(_) => return Ok(()), // serde reports the canonical type error.
    };
    if package_present && package.is_none() || packages_present && packages.is_none() {
        return Ok(()); // serde reports the canonical item/type error.
    }
    let normalized = normalize_cargo_packages(package, packages.as_deref())
        .map_err(|reason| format!("invalid arguments for tool 'cargo_check': {reason}"))?;
    object.remove("package");
    object.remove("packages");
    if let Some(packages) = normalized {
        object.insert("packages".to_string(), serde_json::json!(packages));
    }
    Ok(())
}

/// Only explicitly documented, lossless model-input spellings belong here.
/// Business ToolCall variants and Runner payloads retain `args` alone.
fn canonicalize_process_argv_alias(name: &str, arguments: &mut Value) -> Result<bool, String> {
    if !matches!(name, "run_process" | "run_detached_process") {
        return Ok(false);
    }
    let Some(object) = arguments.as_object_mut() else {
        return Ok(false);
    };
    let Some(alias) = object.remove("argv") else {
        return Ok(false);
    };
    if let Some(canonical) = object.get("args") {
        if canonical != &alias {
            return Err("ambiguous compatibility alias: args and argv differ".to_string());
        }
    } else {
        object.insert("args".to_string(), alias);
    }
    Ok(true)
}

fn validate_run_shell_login(name: &str, arguments: &Value) -> Result<(), String> {
    if name == "run_shell"
        && arguments.get("login").and_then(Value::as_bool) == Some(true)
        && arguments.get("shell").and_then(Value::as_str) != Some("bash")
    {
        return Err("run_shell login=true requires shell=bash".to_string());
    }
    Ok(())
}

impl ToolCall {
    pub fn from_tool_name(name: &str, arguments: Value) -> Result<Self, String> {
        Self::from_tool_name_with_normalization(name, arguments).map(|(call, _)| call)
    }

    /// Returns a stable code only when a documented compatibility alias was used.
    pub fn from_tool_name_with_normalization(
        name: &str,
        arguments: Value,
    ) -> Result<(Self, Option<&'static str>), String> {
        validate_model_facing_assertion_name(name, &arguments)?;
        validate_model_facing_result_expectation(name, &arguments)?;
        if name == "create_project"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("managed_temporary_project"))
        {
            return Err(
                "invalid arguments for tool 'create_project': field 'managed_temporary_project' is no longer supported; use ordinary explicit project creation"
                    .to_string(),
            );
        }
        if name == "create_project"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("allow_existing_empty"))
        {
            return Err(
                "invalid arguments for tool 'create_project': field 'allow_existing_empty' is no longer supported; use 'adopt_existing_empty' to explicitly adopt an existing empty directory"
                    .to_string(),
            );
        }
        // Reject unknown tool names up front with a helpful message that lists
        // every accepted tool and points the caller at canonical discovery. This
        // avoids leaking a raw serde "unknown variant" error and gives model/API
        // callers an actionable discovery hint.
        let definition = lookup_tool_definition(name).ok_or_else(|| {
            format!(
                "unknown tool '{}'. Available tools: {}. Call tool_manifest with \
                 an exact tool_name (or use its category/intent views) to discover \
                 accepted model-visible tool names.",
                name,
                model_visible_tool_names_csv()
            )
        })?;
        validate_structured_validation_sync_wait(name, &arguments)?;
        validate_project_artifact_arguments(name, &arguments)?;
        validate_read_project_artifact_expected_sha256(name, &arguments)?;
        if name == "apply_patch"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("strict_matching"))
        {
            return Err(
                "invalid arguments for tool 'apply_patch': field 'strict_matching' is no longer supported; use matching_mode='exact_unique' for former strict_matching=true, matching_mode='first_match' for former strict_matching=false, or omit matching_mode for the current unique default"
                    .to_string(),
            );
        }
        let mut arguments = strip_tool_call_expectation_metadata(arguments);
        validate_run_shell_login(name, &arguments)?;
        let normalization =
            canonicalize_process_argv_alias(name, &mut arguments)?.then_some("argv_to_args");
        canonicalize_cargo_check_packages(name, &mut arguments)?;
        if name == "tool_manifest" {
            if let Some(object) = arguments.as_object_mut() {
                if !object.contains_key("include_recommended_flows") {
                    let exact_lookup = object.contains_key("tool_name");
                    object.insert(
                        "include_recommended_flows".to_string(),
                        Value::Bool(!exact_lookup),
                    );
                }
            }
        }
        if name == "cargo_test" {
            if let Some(object) = arguments.as_object_mut() {
                if object.get("lib").and_then(Value::as_bool) == Some(false) {
                    object.remove("lib");
                }
            }
        }
        if name == "cargo_fmt" {
            if let Some(object) = arguments.as_object_mut() {
                // Positive sync_wait_secs is a recognized caller-shape hint in
                // ensure-format mode, but that mode is intentionally synchronous.
                // Canonicalize the inert hint away before concrete ToolCall serde
                // so execution/audit truth has one representation: omission.
                if object.get("check").and_then(Value::as_bool) != Some(true) {
                    object.remove("sync_wait_secs");
                }
            }
        }
        if name == "read_project_artifact"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("max_bytes"))
        {
            return Err(
                "invalid arguments for tool 'read_project_artifact': field 'max_bytes' is no longer supported; use 'length'"
                    .to_string(),
            );
        }
        if name == "write_project_file"
            && arguments
                .as_object()
                .is_some_and(|object| object.contains_key("expected_content_prefix"))
        {
            return Err(
                "invalid arguments for tool 'write_project_file': field 'expected_content_prefix' is no longer supported; use expected_read_revision"
                    .to_string(),
            );
        }
        if name == "work_on_project" {
            validate_coding_project_source_shape(name, &arguments)?;
        }
        let mut wrapped = serde_json::Map::new();
        wrapped.insert(
            TOOL_CALL_TOOL_FIELD.to_string(),
            Value::String(name.to_string()),
        );
        if definition.requires_artifact_upload_path_binding() {
            let missing_path = arguments
                .as_object()
                .and_then(|obj| obj.get("path"))
                .and_then(Value::as_str)
                .map(str::is_empty)
                .unwrap_or(true);
            if missing_path {
                return Err(format!(
                    "invalid arguments for tool '{}': path is required and must match the path \
                     used by artifact_upload_begin to bind upload_id to the requested target path",
                    name
                ));
            }
        }
        if !definition.uses_unit_arguments() {
            // Non-unit tools always carry a `params` object so variants whose
            // fields are all optional (e.g. `list_jobs`) still deserialize when
            // a caller passes `null` arguments. A null argument is normalized
            // to an empty object; required-field validation still fires for
            // tools that need fields.
            let params = if arguments.is_null() {
                Value::Object(serde_json::Map::new())
            } else {
                arguments
            };
            wrapped.insert(TOOL_CALL_PARAMS_FIELD.to_string(), params);
        }
        let call: Self = serde_json::from_value(Value::Object(wrapped))
            .map_err(|e| format!("invalid arguments for tool '{}': {}", name, e))?;
        if let Self::PluginTool(plugin) = &call {
            plugin
                .validate()
                .map_err(|error| format!("invalid arguments for tool '{}': {}", name, error))?;
        }
        if let Self::SshResource(ssh_resource) = &call {
            ssh_resource
                .validate()
                .map_err(|error| format!("invalid arguments for tool '{}': {}", name, error))?;
        }
        Ok((call, normalization))
    }

    /// Raw command text for shell-like calls. Consumed only by the workspace
    /// activity recorder, which truncates it to a bounded preview and honors
    /// the operator's preview config switch — it is never logged verbatim.
    pub fn command_text(&self) -> Option<&str> {
        match self {
            Self::RunShell { command, .. }
            | Self::SessionShellExec { command, .. }
            | Self::RunJob { command, .. } => Some(command),
            _ => None,
        }
    }

    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::ListTools { .. } => "list_tools",
            Self::StartSession { .. } => "start_session",
            Self::WorkOnProject { .. } => "work_on_project",
            Self::FinishCodingTask { .. } => "finish_coding_task",
            Self::PresentWorkResult { .. } => "present_work_result",
            Self::WorkResultState { .. } => "work_result_state",
            Self::WorkResultSendMessage { .. } => "work_result_send_message",
            Self::ChangesFileDiff { .. } => "changes_file_diff",
            Self::SessionSummary { .. } => "session_summary",
            Self::UpdateSessionContext { .. } => "update_session_context",
            Self::CloseSession { .. } => "close_session",
            Self::ValidationSummary { .. } => "validation_summary",
            Self::RecordExternalObservation { .. } => "record_external_observation",
            Self::ListExternalObservations { .. } => "list_external_observations",
            Self::PostSessionMessage { .. } => "post_session_message",
            Self::PostPeerMessage { .. } => "post_peer_message",
            Self::ListSessionMessages { .. } => "list_session_messages",
            Self::GetSessionAssignment { .. } => "get_session_assignment",
            Self::ObserveSessionMessages { .. } => "observe_session_messages",
            Self::ResolveSessionMessage { .. } => "resolve_session_message",
            Self::CompleteSessionMessage { .. } => "complete_session_message",
            Self::SessionDiscussionSummary { .. } => "session_discussion_summary",
            Self::SessionHandoffSummary { .. } => "session_handoff_summary",
            Self::SessionHandoffState { .. } => "session_handoff_state",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate { .. } => "workspace_checkpoint_create",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointList { .. } => "workspace_checkpoint_list",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointShow { .. } => "workspace_checkpoint_show",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointRestore { .. } => "workspace_checkpoint_restore",
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointDelete { .. } => "workspace_checkpoint_delete",
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec { .. } => "code_mode_exec",
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExecEffectful { .. } => "code_mode_exec_effectful",
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExecMutating { .. } => "code_mode_exec_mutating",
            Self::RunProcess { .. } => "run_process",
            Self::RunDetachedProcess { .. } => "run_detached_process",
            Self::CodingAgentStart { .. } => "coding_agent_start",
            Self::CodingAgentObserve { .. } => "coding_agent_observe",
            Self::CodingAgentCancel { .. } => "coding_agent_cancel",
            Self::RunScript { .. } => "run_script",
            Self::RunShell { .. } => "run_shell",
            Self::OpenSessionShell { .. } => "open_session_shell",
            Self::SessionShellExec { .. } => "session_shell_exec",
            Self::SessionShellStatus { .. } => "session_shell_status",
            Self::CloseSessionShell { .. } => "close_session_shell",
            Self::ApplyPatch { .. } => "apply_patch",
            Self::ApplyUnifiedDiff { .. } => "apply_unified_diff",
            Self::DeleteProjectFiles { .. } => "delete_project_files",
            Self::GitRestorePaths { .. } => "git_restore_paths",
            Self::DiscardUntracked { .. } => "discard_untracked",
            Self::GitCommitPaths { .. } => "git_commit_paths",
            Self::GitStatus { .. } => "git_status",
            Self::GitDiffHunks { .. } => "git_diff_hunks",
            Self::GitReviewSummary { .. } => "git_review_summary",
            Self::GitLog { .. } => "git_log",
            Self::CargoFmt { .. } => "cargo_fmt",
            Self::CargoCheck { .. } => "cargo_check",
            Self::CargoTest { .. } => "cargo_test",
            Self::GoTest { .. } => "go_test",
            Self::ReadFiles { .. } => "read_files",
            Self::SkillLoad { .. } => "skill_load",
            Self::RunSkillResource { .. } => "run_skill_resource",
            Self::SkillList { .. } => "skill_list",
            Self::SkillReadFile { .. } => "skill_read_file",
            Self::SkillVersions { .. } => "skill_versions",
            Self::SkillInstall { .. } => "skill_install",
            Self::SkillActivate { .. } => "skill_activate",
            Self::SkillRemoveRevision { .. } => "skill_remove_revision",
            Self::PrepareGoalWorkflow { .. } => "prepare_goal_workflow",
            Self::CreateGoal { .. } => "create_goal",
            Self::GetGoal { .. } => "get_goal",
            Self::PresentGoalPlan { .. } => "present_goal_plan",
            Self::GoalPlanSync { .. } => "goal_plan_sync",
            Self::CheckpointGoal { .. } => "checkpoint_goal",
            Self::ListGoals { .. } => "list_goals",
            Self::UpdateGoal { .. } => "update_goal",
            Self::AssociateGoalAgentTask { .. } => "associate_goal_agent_task",
            Self::AssociateGoalWorkflowSession { .. } => "associate_goal_workflow_session",
            Self::WaitForAgentEvents { .. } => "wait_for_agent_events",
            Self::ReadAgentWait { .. } => "read_agent_wait",
            Self::CancelAgentWait { .. } => "cancel_agent_wait",
            Self::AgentWaitState { .. } => "agent_wait_state",
            Self::CreateAgentTask { .. } => "create_agent_task",
            Self::ListAgentTasks { .. } => "list_agent_tasks",
            Self::ReadAgentTask { .. } => "read_agent_task",
            Self::AssignAgentTask { .. } => "assign_agent_task",
            Self::StartAgentTaskAttempt { .. } => "start_agent_task_attempt",
            Self::StartAgentTaskEndpointContinuation { .. } => {
                "start_agent_task_endpoint_continuation"
            }
            Self::StartAgentTaskCodingRun { .. } => "start_agent_task_coding_run",
            Self::ReconcileAgentTaskCodingRun { .. } => "reconcile_agent_task_coding_run",
            Self::HeartbeatAgentTaskAttempt { .. } => "heartbeat_agent_task_attempt",
            Self::CompleteAgentTaskAttempt { .. } => "complete_agent_task_attempt",
            Self::CreateAgentIdentity { .. } => "create_agent_identity",
            Self::ListAgentIdentities { .. } => "list_agent_identities",
            Self::UpdateAgentIdentity { .. } => "update_agent_identity",
            Self::RotateAgentContinuationEndpoint { .. } => "rotate_agent_continuation_endpoint",
            Self::AttachAgentEndpoint { .. } => "attach_agent_endpoint",
            Self::PresentAgentContinuation { .. } => "present_agent_continuation",
            Self::AgentContinuationBind { .. } => "agent_continuation_bind",
            Self::AgentContinuationRecoverEndpoint { .. } => "agent_continuation_recover_endpoint",
            Self::AgentContinuationState { .. } => "agent_continuation_state",
            Self::AgentContinuationWakeAcquire { .. } => "agent_continuation_wake_acquire",
            Self::AgentContinuationWakePrepare { .. } => "agent_continuation_wake_prepare",
            Self::AgentContinuationWakeFinish { .. } => "agent_continuation_wake_finish",
            Self::AgentContinuationUnbind { .. } => "agent_continuation_unbind",
            Self::DetachAgentEndpoint { .. } => "detach_agent_endpoint",
            Self::CreateConversation { .. } => "create_conversation",
            Self::ListConversations { .. } => "list_conversations",
            Self::ReadConversation { .. } => "read_conversation",
            Self::PostConversationMessage { .. } => "post_conversation_message",
            Self::ListAgentInbox { .. } => "list_agent_inbox",
            Self::ConsumeAgentDeliveries { .. } => "consume_agent_deliveries",
            Self::BootstrapAgentConversation { .. } => "bootstrap_agent_conversation",
            Self::ConsumeAgentWake { .. } => "consume_agent_wake",
            Self::MemorySearch { .. } => "memory_search",
            Self::MemoryRead { .. } => "memory_read",
            Self::MemorySet { .. } => "memory_set",
            Self::MemoryDelete { .. } => "memory_delete",
            Self::MemoryScopeList { .. } => "memory_scope_list",
            Self::MemoryScopePurge { .. } => "memory_scope_purge",
            Self::RunJob { .. } => "run_job",
            Self::StopJob { .. } => "stop_job",
            Self::ObserveJobs { .. } => "observe_jobs",
            Self::WaitForJobTerminal { .. } => "wait_for_job_terminal",
            Self::PresentJobTerminalContinuation { .. } => "present_job_terminal_continuation",
            Self::JobTerminalContinuationBind { .. } => "job_terminal_continuation_bind",
            Self::JobTerminalContinuationState { .. } => "job_terminal_continuation_state",
            Self::JobTerminalContinuationPrepare { .. } => "job_terminal_continuation_prepare",
            Self::JobTerminalContinuationFinish { .. } => "job_terminal_continuation_finish",
            Self::JobTerminalContinuationUnbind { .. } => "job_terminal_continuation_unbind",
            Self::ListProjectFiles { .. } => "list_project_files",
            Self::ListProjectTrackedFiles { .. } => "list_project_tracked_files",
            Self::ProjectOverview { .. } => "project_overview",
            Self::SearchProjectTexts { .. } => "search_project_texts",
            Self::SearchAndRead { .. } => "search_and_read",
            Self::ShowChanges { .. } => "show_changes",
            Self::WorkspaceHygieneCheck { .. } => "workspace_hygiene_check",
            Self::ListJobs { .. } => "list_jobs",
            Self::CurrentWindowActivity { .. } => "current_window_activity",
            Self::JobTail { .. } => "job_tail",
            Self::WriteProjectFile { .. } => "write_project_file",
            Self::SaveProjectArtifact { .. } => "save_project_artifact",
            Self::ImportConversationFilesToProject { .. } => "import_conversation_files_to_project",
            Self::TransferProjectArtifact { .. } => "transfer_project_artifact",
            Self::ProjectArtifact { .. } => "project_artifact",
            Self::ReadProjectArtifactMetadata { .. } => "read_project_artifact_metadata",
            Self::ReadProjectArtifact { .. } => "read_project_artifact",
            Self::ArtifactUploadBegin { .. } => "artifact_upload_begin",
            Self::ArtifactUploadChunk { .. } => "artifact_upload_chunk",
            Self::ArtifactUploadFinish { .. } => "artifact_upload_finish",
            Self::ArtifactUploadAbort { .. } => "artifact_upload_abort",
            Self::ApplyTextEdits { .. } => "apply_text_edits",
            Self::LspStatus { .. } => "lsp_status",
            Self::DocumentSymbols { .. } => "document_symbols",
            Self::DocumentDiagnostics { .. } => "document_diagnostics",
            Self::Hover { .. } => "hover",
            Self::WorkspaceSymbols { .. } => "workspace_symbols",
            Self::GotoDefinition { .. } => "goto_definition",
            Self::FindReferences { .. } => "find_references",
            Self::CallHierarchy { .. } => "call_hierarchy",
            Self::BrowserObserve(..) => "browser_observe",
            Self::BrowserAct(..) => "browser_act",
            Self::ComputerObserve(..) => "computer_observe",
            Self::ComputerControl(..) => "computer_control",
            Self::ComputerSaveSnapshot { .. } => "computer_save_snapshot",
            Self::ListProjects { .. } => "list_projects",
            Self::RegisterProject { .. } => "register_project",
            Self::UnregisterProject { .. } => "unregister_project",
            Self::CreateProject { .. } => "create_project",
            Self::ListRunners { .. } => "list_runners",
            Self::RunnerConfigCheck { .. } => "runner_config_check",
            Self::RunnerConfigReload { .. } => "runner_config_reload",
            Self::PluginTool(_) => "plugin_tool",
            Self::SshResource(_) => "ssh_resource",
            Self::RuntimeStatus { .. } => "runtime_status",
            Self::ReadToolTrace { .. } => "read_tool_trace",
            Self::ToolManifest { .. } => "tool_manifest",
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        match self {
            // External-observation ingress/read carries an exact business Session
            // that is independently re-authorized by the runtime method. Keep it
            // out of this generic recorder projection so adapter traffic cannot
            // become native Session evidence or consume the bounded event tail.
            Self::RecordExternalObservation { .. } | Self::ListExternalObservations { .. } => None,
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec { session_id, .. }
            | Self::CodeModeExecEffectful { session_id, .. }
            | Self::CodeModeExecMutating { session_id, .. } => Some(session_id.as_str()),
            Self::RunProcess { session_id, .. }
            | Self::RunDetachedProcess { session_id, .. }
            | Self::RunScript { session_id, .. }
            | Self::RunShell { session_id, .. }
            | Self::ApplyPatch { session_id, .. }
            | Self::ApplyUnifiedDiff { session_id, .. }
            | Self::DeleteProjectFiles { session_id, .. }
            | Self::GitRestorePaths { session_id, .. }
            | Self::DiscardUntracked { session_id, .. }
            | Self::GitCommitPaths { session_id, .. }
            | Self::GitStatus { session_id, .. }
            | Self::GitDiffHunks { session_id, .. }
            | Self::GitReviewSummary { session_id, .. }
            | Self::GitLog { session_id, .. }
            | Self::CargoFmt { session_id, .. }
            | Self::CargoCheck { session_id, .. }
            | Self::CargoTest { session_id, .. }
            | Self::GoTest { session_id, .. }
            | Self::ReadFiles { session_id, .. }
            | Self::SkillLoad { session_id, .. }
            | Self::RunSkillResource { session_id, .. }
            | Self::SkillList { session_id, .. }
            | Self::SkillReadFile { session_id, .. }
            | Self::SkillVersions { session_id, .. }
            | Self::SkillInstall { session_id, .. }
            | Self::SkillActivate { session_id, .. }
            | Self::SkillRemoveRevision { session_id, .. }
            | Self::MemorySearch { session_id, .. }
            | Self::MemoryRead { session_id, .. }
            | Self::MemorySet { session_id, .. }
            | Self::MemoryDelete { session_id, .. }
            | Self::RunJob { session_id, .. }
            | Self::StopJob { session_id, .. }
            | Self::ListProjectFiles { session_id, .. }
            | Self::ListProjectTrackedFiles { session_id, .. }
            | Self::ProjectOverview { session_id, .. }
            | Self::SearchProjectTexts { session_id, .. }
            | Self::SearchAndRead { session_id, .. }
            | Self::ShowChanges { session_id, .. }
            | Self::WriteProjectFile { session_id, .. }
            | Self::SaveProjectArtifact { session_id, .. }
            | Self::ComputerSaveSnapshot { session_id, .. }
            | Self::ProjectArtifact { session_id, .. }
            | Self::ReadProjectArtifactMetadata { session_id, .. }
            | Self::ReadProjectArtifact { session_id, .. }
            | Self::ArtifactUploadBegin { session_id, .. }
            | Self::ArtifactUploadChunk { session_id, .. }
            | Self::ArtifactUploadFinish { session_id, .. }
            | Self::ArtifactUploadAbort { session_id, .. }
            | Self::ApplyTextEdits { session_id, .. }
            | Self::WorkspaceHygieneCheck { session_id, .. }
            | Self::LspStatus { session_id, .. }
            | Self::DocumentSymbols { session_id, .. }
            | Self::DocumentDiagnostics { session_id, .. }
            | Self::Hover { session_id, .. }
            | Self::WorkspaceSymbols { session_id, .. }
            | Self::GotoDefinition { session_id, .. }
            | Self::FindReferences { session_id, .. } => session_id.as_deref(),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate { session_id, .. }
            | Self::WorkspaceCheckpointList { session_id, .. }
            | Self::WorkspaceCheckpointShow { session_id, .. }
            | Self::WorkspaceCheckpointRestore { session_id, .. }
            | Self::WorkspaceCheckpointDelete { session_id, .. } => session_id.as_deref(),
            Self::SessionHandoffSummary { session_id, .. } => Some(session_id.as_str()),
            // Window-card presentation/refresh never becomes generic Session recorder
            // evidence. An optional Session selector is association evidence only.
            Self::PresentWorkResult { .. }
            | Self::WorkResultState { .. }
            | Self::WorkResultSendMessage { .. }
            | Self::ChangesFileDiff { .. }
            | Self::SessionHandoffState { .. } => None,
            Self::ImportConversationFilesToProject { session_id, .. } => session_id.as_deref(),
            Self::CallHierarchy { session_id, .. } => session_id.as_deref(),
            Self::WorkOnProject { session_id, .. } => session_id.as_deref(),
            Self::OpenSessionShell { session_id, .. }
            | Self::SessionShellExec { session_id, .. }
            | Self::SessionShellStatus { session_id, .. }
            | Self::CloseSessionShell { session_id, .. } => Some(session_id.as_str()),
            _ => None,
        }
    }

    /// Attach explicit generic recorder provenance to a CodingAgentStart only.
    /// This never makes the recorder a business Session or Run authority.
    pub fn with_coding_agent_recording_session_id(
        mut self,
        recorder_session_id: Option<String>,
    ) -> Self {
        if let (
            Self::CodingAgentStart {
                recording_session_id,
                ..
            },
            Some(recorder_session_id),
        ) = (&mut self, recorder_session_id)
        {
            if recording_session_id.is_none() {
                *recording_session_id = Some(recorder_session_id);
            }
        }
        self
    }

    /// Apply project-matched Session defaults without overwriting explicit
    /// per-call arguments.
    pub fn with_session_execution_context(
        mut self,
        execution_context: &SessionExecutionContext,
    ) -> Self {
        match &mut self {
            Self::RunProcess { cwd, .. }
            | Self::RunDetachedProcess { cwd, .. }
            | Self::RunScript { cwd, .. }
            | Self::RunSkillResource { cwd, .. }
                if cwd.is_none() =>
            {
                *cwd = execution_context.default_cwd.clone();
            }
            Self::RunShell { cwd, shell, .. }
            | Self::RunJob { cwd, shell, .. }
            | Self::OpenSessionShell { cwd, shell, .. } => {
                if cwd.is_none() {
                    *cwd = execution_context.default_cwd.clone();
                }
                if shell.is_none() {
                    *shell = execution_context.default_shell;
                }
            }
            _ => {}
        }
        self
    }

    pub fn project(&self) -> Option<&str> {
        match self {
            Self::RecordExternalObservation { project, .. }
            | Self::ListExternalObservations { project, .. } => Some(project),
            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec { project, .. }
            | Self::CodeModeExecEffectful { project, .. }
            | Self::CodeModeExecMutating { project, .. } => Some(project.as_str()),
            Self::RunProcess { project, .. }
            | Self::RunDetachedProcess { project, .. }
            | Self::CodingAgentStart { project, .. }
            | Self::StartAgentTaskCodingRun { project, .. }
            | Self::RunScript { project, .. }
            | Self::RunShell { project, .. }
            | Self::OpenSessionShell { project, .. }
            | Self::SessionShellExec { project, .. }
            | Self::SessionShellStatus { project, .. }
            | Self::CloseSessionShell { project, .. }
            | Self::ApplyPatch { project, .. }
            | Self::ApplyUnifiedDiff { project, .. }
            | Self::DeleteProjectFiles { project, .. }
            | Self::GitRestorePaths { project, .. }
            | Self::DiscardUntracked { project, .. }
            | Self::GitCommitPaths { project, .. }
            | Self::GitStatus { project, .. }
            | Self::GitDiffHunks { project, .. }
            | Self::GitReviewSummary { project, .. }
            | Self::GitLog { project, .. }
            | Self::CargoFmt { project, .. }
            | Self::CargoCheck { project, .. }
            | Self::CargoTest { project, .. }
            | Self::GoTest { project, .. }
            | Self::ReadFiles { project, .. }
            | Self::SkillLoad { project, .. }
            | Self::RunSkillResource { project, .. }
            | Self::SkillList { project, .. }
            | Self::SkillReadFile { project, .. }
            | Self::SkillVersions { project, .. }
            | Self::SkillInstall { project, .. }
            | Self::SkillActivate { project, .. }
            | Self::SkillRemoveRevision { project, .. }
            | Self::MemorySearch { project, .. }
            | Self::MemoryRead { project, .. }
            | Self::MemorySet { project, .. }
            | Self::MemoryDelete { project, .. }
            | Self::RunJob { project, .. }
            | Self::StopJob { project, .. }
            | Self::ListProjectFiles { project, .. }
            | Self::ListProjectTrackedFiles { project, .. }
            | Self::ProjectOverview { project, .. }
            | Self::SearchProjectTexts { project, .. }
            | Self::SearchAndRead { project, .. }
            | Self::ShowChanges { project, .. }
            | Self::WriteProjectFile { project, .. }
            | Self::SaveProjectArtifact { project, .. }
            | Self::ComputerSaveSnapshot { project, .. }
            | Self::ImportConversationFilesToProject { project, .. }
            | Self::ProjectArtifact { project, .. }
            | Self::ReadProjectArtifactMetadata { project, .. }
            | Self::ReadProjectArtifact { project, .. }
            | Self::ArtifactUploadBegin { project, .. }
            | Self::ArtifactUploadChunk { project, .. }
            | Self::ArtifactUploadFinish { project, .. }
            | Self::ArtifactUploadAbort { project, .. }
            | Self::ApplyTextEdits { project, .. }
            | Self::WorkspaceHygieneCheck { project, .. }
            | Self::LspStatus { project, .. }
            | Self::DocumentSymbols { project, .. }
            | Self::DocumentDiagnostics { project, .. }
            | Self::Hover { project, .. }
            | Self::WorkspaceSymbols { project, .. }
            | Self::GotoDefinition { project, .. }
            | Self::FindReferences { project, .. } => Some(project.as_str()),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate { project, .. }
            | Self::WorkspaceCheckpointList { project, .. }
            | Self::WorkspaceCheckpointShow { project, .. }
            | Self::WorkspaceCheckpointRestore { project, .. }
            | Self::WorkspaceCheckpointDelete { project, .. } => Some(project.as_str()),
            Self::CallHierarchy { project, .. } => Some(project.as_str()),
            Self::WorkOnProject { project, .. } if !project.trim().is_empty() => {
                Some(project.as_str())
            }
            Self::FinishCodingTask { project, .. }
            | Self::PresentWorkResult { project, .. }
            | Self::WorkResultState { project, .. }
            | Self::WorkResultSendMessage { project, .. }
            | Self::ChangesFileDiff { project, .. } => Some(project.as_str()),
            Self::UpdateSessionContext { project, .. }
            | Self::ValidationSummary { project, .. } => Some(project.as_str()),
            Self::SessionHandoffSummary { project, .. } => project.as_deref(),
            Self::SessionHandoffState { project, .. } => Some(project.as_str()),
            _ => None,
        }
    }
}
