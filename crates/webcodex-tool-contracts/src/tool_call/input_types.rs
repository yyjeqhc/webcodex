#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WebcodexResourceKind {
    Project,
    File,
    Goal,
    Artifact,
}

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
    /// Exact Project required by projectBound on describe; calls recheck write authority and root, not a sandbox.
    #[schemars(length(min = 1, max = 512))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Plugin tool arguments matching the schema observed by describe; encoded payload is bounded to
    /// 65536 bytes.
    #[serde(default)]
    pub arguments: Option<Value>,
}

#[cfg(test)]
#[path = "plugin_project_tests.rs"]
mod plugin_project_tests;

impl PluginToolCall {
    fn validate(&self) -> Result<(), String> {
        if let Some(project) = self.project.as_deref() {
            if self.action != PluginToolAction::Describe
                || project.trim().is_empty()
                || project.len() > 512
                || project.chars().any(char::is_control)
            {
                return Err(
                    "project must be an exact bounded Project on action=describe".to_string(),
                );
            }
        }
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

/// App-only keyset history read in the current authenticated Host Window.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkResultCollaborationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub before_message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: Option<usize>,
}

/// App-only inspection of a pinned Work Result file snapshot.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkResultFilesRequest {
    #[serde(default)]
    pub snapshot_id: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub path: Option<String>,
    /// Omit for inventory or diff; content/PDF reads require an advertised path and snapshot.
    #[serde(default)]
    pub view: Option<WorkResultFileView>,
    /// Byte position in the immutable final blob, independent of inventory offset.
    #[serde(default)]
    pub byte_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WorkResultFileView {
    Content,
    /// PDF bytes: at most 128 KiB per page and 20 MiB per file.
    Pdf,
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

/// Canonical search context ceiling shared by Runtime normalization and output schemas.
pub const MAX_SEARCH_CONTEXT_LINES: usize = 80;

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

/// Transient terminal readiness condition for one exact Job set.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JobReadinessMode {
    Any,
    All,
}

/// Explicit Host wait bound, independent of generic execution handoff slices.
pub const MAX_JOB_READINESS_WAIT_SECS: u64 = 45;

/// Which observable changes may end a bounded batch Job wait early.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObserveJobsWakeOn {
    #[default]
    Change,
    MeaningfulChange,
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
    crate::read_revision::deserialize(deserializer).map(Some)
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
            "continuation exceeds the read_git_diff_hunks size bound",
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
    /// Discover only tabs explicitly shared in the local Chrome extension.
    Discover { #[schemars(length(min = 1, max = 128))] client_id: String },
    /// Resolve one exact native window owned by this Browser. Requires Browser
    /// and Computer read authority; ambiguous windows are not guessed.
    Surface {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
    },
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
            Self::Surface { .. } => "surface",
            Self::Discover { .. } => "discover",
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
pub enum BrowserBatchOperation {
    Click {
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
    },
    InputText {
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        #[schemars(length(min = 1, max = 4096))]
        text: String,
    },
    SelectOption {
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        /// Exact native option value or trimmed visible option text.
        #[schemars(length(min = 1, max = 4096))]
        option: String,
    },
    SetValue {
        #[schemars(length(min = 1, max = 128))]
        #[schemars(regex(pattern = "^element_[A-Za-z0-9_-]{16,64}$"))]
        element_id: String,
        /// Exact native form-control value, for example 2027-06 for input[type=month].
        #[schemars(length(min = 1, max = 4096))]
        value: String,
    },
    UploadFile {
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
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrowserLaunchModeCall { Ephemeral, Managed }
impl BrowserLaunchModeCall {
    pub const fn as_str(self) -> &'static str {
        match self { Self::Ephemeral => "ephemeral", Self::Managed => "managed" }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrowserActToolCall {
    /// Attach an exact live extension offer. Closing this Browser only detaches.
    Attach {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(regex(pattern = "^attachment_[A-Za-z0-9_-]{16,64}$"))]
        attachment_id: String,
    },
    Batch {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        #[schemars(regex(pattern = "^browser_[A-Za-z0-9_-]{16,64}$"))]
        browser_id: String,
        #[schemars(regex(pattern = "^page_[A-Za-z0-9_-]{16,64}$"))]
        page_id: String,
        #[schemars(length(min = 1, max = 32))]
        operations: Vec<BrowserBatchOperation>,
    },

    Launch {
        #[schemars(length(min = 1, max = 128))]
        client_id: String,
        /// Default is the existing headless, temporary Browser. Managed launches
        /// a visible Browser with a private persistent WebCodex-owned profile.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mode: Option<BrowserLaunchModeCall>,
        /// Required only for managed mode. A name, never a profile filesystem path.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(length(min = 1, max = 48))]
        #[schemars(regex(pattern = "^[a-z0-9][a-z0-9_-]{0,47}$"))]
        profile: Option<String>,
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
            Self::Attach { .. } => "attach",
            Self::NewPage { .. } => "new_page",
            Self::Navigate { .. } => "navigate",
            Self::Reload { .. } => "reload",
            Self::Click { .. } => "click",
            Self::InputText { .. } => "input_text",
            Self::SelectOption { .. } => "select_option",
            Self::SetValue { .. } => "set_value",
            Self::UploadFile { .. } => "upload_file",
            Self::Batch { .. } => "batch",
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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GitReviewScopeInput {
    /// Net HEAD-to-worktree changes, including staged/untracked contents; not an index-only patch.
    Workspace,
    /// Review one exact committed range, resolved once to a single merge-base.
    Committed {
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        base_commit: String,
        #[schemars(length(min = 40, max = 40))]
        #[schemars(regex(pattern = "^[0-9A-Fa-f]{40}$"))]
        head_commit: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectUnregisterInput {
    /// Exact canonical Project id, not a fuzzy name or alias.
    #[schemars(length(min = 1, max = 512))]
    pub project: String,
    /// Exact sha256 registration revision from list_projects full output.
    #[schemars(length(min = 71, max = 71))]
    #[schemars(regex(pattern = "^sha256:[0-9A-Fa-f]{64}$"))]
    pub expected_revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatusFilter {
    Any,
    Online,
    Offline,
    Stale,
}
