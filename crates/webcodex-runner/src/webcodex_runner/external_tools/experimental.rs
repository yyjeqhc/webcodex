//! Experimental raw Claude coding tool harness.
//!
//! Three fixed agent-internal request kinds (`claude_list_tools`,
//! `claude_describe_tool`, `claude_tool_call`) expose the live Claude MCP tool
//! surface through the generation-owned `ExternalToolRouter` captured at
//! dispatch time. They are never published as public MCP tools or OpenAPI
//! operations, and only `Read`/`Edit`/`Write`/`Bash` are callable.

use super::{
    command_result, cwd_allowed, path_error, protocol_error, request_error, sanitize_tool_name,
    sha256_hex_bytes, ClaudeCodeMcpProvider, ExternalToolRouter, ProjectMcpClient,
    ProviderCallSummary, ProviderError, RunnerPolicy, ShellAgentShellRequest, WriteState,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// Experimental raw Claude harness bounds (experiment-local; not production API).
pub(super) const MAX_EXPERIMENTAL_TOOLS: usize = 64;
const MAX_EXPERIMENTAL_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_EXPERIMENTAL_RESULT_BYTES: usize = 256 * 1024;
const MAX_EXPERIMENTAL_DESCRIPTION_CHARS: usize = 4_096;
pub(super) const EXPERIMENTAL_KIND_LIST: &str = "claude_list_tools";
pub(super) const EXPERIMENTAL_KIND_DESCRIBE: &str = "claude_describe_tool";
pub(super) const EXPERIMENTAL_KIND_CALL: &str = "claude_tool_call";

/// Fixed experimental call allowlist. list/describe may observe other tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExperimentalClaudeToolKind {
    Read,
    Edit,
    Write,
    Bash,
}

impl ExperimentalClaudeToolKind {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "Read" => Some(Self::Read),
            "Edit" => Some(Self::Edit),
            "Write" => Some(Self::Write),
            "Bash" => Some(Self::Bash),
            _ => None,
        }
    }

    fn is_callable(name: &str) -> bool {
        Self::parse(name).is_some()
    }

    /// Mutating tools may have already executed if the MCP request was written.
    fn post_send_failure_state(self) -> WriteState {
        match self {
            Self::Read => WriteState::NotSubmitted,
            Self::Edit | Self::Write | Self::Bash => WriteState::Uncertain,
        }
    }
}

/// Experimental dispatch succeeded at the MCP transport layer.
/// `tool_succeeded` is false when Claude returned `isError: true`.
pub(super) struct ExperimentalDispatchOutcome {
    value: Value,
    tool_succeeded: bool,
    error_code: Option<&'static str>,
    /// Present for tool-level failures after a tools/call was sent (isError /
    /// hard oversized path via Err).
    write_state: Option<WriteState>,
}

pub(super) fn is_experimental_claude_kind(kind: &str) -> bool {
    matches!(
        kind,
        EXPERIMENTAL_KIND_LIST | EXPERIMENTAL_KIND_DESCRIBE | EXPERIMENTAL_KIND_CALL
    )
}

fn experimental_error_code(code: &str) -> &str {
    match code {
        "claude_tool_not_found"
        | "claude_tool_not_allowed"
        | "claude_schema_unavailable"
        | "claude_arguments_invalid"
        | "claude_mcp_timeout"
        | "claude_mcp_process_exited"
        | "claude_tool_error"
        | "claude_result_too_large"
        | "claude_code_unavailable"
        | "provider_path_rejected"
        | "provider_invalid_request" => code,
        "mcp_request_timeout" => "claude_mcp_timeout",
        "mcp_connection_closed"
        | "claude_code_spawn_failed"
        | "mcp_protocol_error"
        | "mcp_invalid_json"
        | "mcp_message_too_large"
        | "mcp_rpc_error"
        | "mcp_pending_limit" => "claude_mcp_process_exited",
        "claude_tool_failed" => "claude_tool_error",
        "provider_response_too_large" => "claude_result_too_large",
        other => other,
    }
}

fn experimental_write_state_label(write_state: WriteState) -> &'static str {
    match write_state {
        WriteState::NotSubmitted => "not_submitted",
        WriteState::Uncertain => "uncertain",
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

impl ExternalToolRouter {
    pub(super) fn handle_experimental(
        &self,
        policy: &RunnerPolicy,
        request: &ShellAgentShellRequest,
        shutdown: Option<&AtomicBool>,
    ) -> super::CommandResult {
        let started = Instant::now();
        match self.claude.experimental_dispatch(policy, request, shutdown) {
            Ok(outcome) => command_result(outcome.value.to_string(), started),
            Err(error) => {
                // Cover preflight path/payload errors that exit before record_call.
                self.claude.record_error(&error);
                let (write_state, changed) = match error.write_state {
                    WriteState::NotSubmitted => ("not_submitted", Value::Bool(false)),
                    WriteState::Uncertain => ("uncertain", Value::Null),
                };
                let code = experimental_error_code(error.code);
                command_result(
                    json!({
                        "experimental": true,
                        "error": code,
                        "code": code,
                        "message": code,
                        "write_state": write_state,
                        "changed": changed,
                    })
                    .to_string(),
                    started,
                )
            }
        }
    }
}

impl ClaudeCodeMcpProvider {
    fn record_experimental_failure(
        &self,
        kind: &str,
        error: &ProviderError,
        started: Option<Instant>,
    ) {
        self.record_error(error);
        self.record_call(
            ProviderCallSummary {
                capability: kind.to_string(),
                selected_provider: "claude_code".to_string(),
                fallback_used: false,
                result: "failure".to_string(),
                write_state: Some(experimental_write_state_label(error.write_state).to_string()),
                duration_ms: started.map_or(0, elapsed_ms),
                error_code: Some(experimental_error_code(error.code).to_string()),
            },
            false,
        );
    }

    /// Experimental: list/describe/call raw Claude MCP tools for one project root.
    fn experimental_dispatch(
        &self,
        policy: &RunnerPolicy,
        request: &ShellAgentShellRequest,
        shutdown: Option<&AtomicBool>,
    ) -> Result<ExperimentalDispatchOutcome, ProviderError> {
        let root = request.cwd.as_deref().ok_or_else(path_error)?;
        let root = Path::new(root).canonicalize().map_err(|_| path_error())?;
        cwd_allowed(policy, &root).map_err(|_| path_error())?;
        let timeout_secs = request
            .timeout_secs
            .max(1)
            .min(policy.max_timeout_secs)
            .min(self.config.timeout_secs.max(1));
        let deadline = Instant::now() + Duration::from_secs(timeout_secs);
        let payload = match request
            .content
            .as_deref()
            .filter(|raw| !raw.trim().is_empty())
            .map(|raw| serde_json::from_str::<Value>(raw).map_err(|_| request_error()))
            .transpose()
        {
            Ok(payload) => payload.unwrap_or_else(|| json!({})),
            Err(error) => {
                self.record_experimental_failure(&request.kind, &error, None);
                return Err(error);
            }
        };
        let tool_name = match request.kind.as_str() {
            EXPERIMENTAL_KIND_DESCRIBE | EXPERIMENTAL_KIND_CALL => {
                match payload.get("tool_name").and_then(Value::as_str) {
                    Some(tool_name) => Some(tool_name),
                    None => {
                        let error = request_error();
                        self.record_experimental_failure(&request.kind, &error, None);
                        return Err(error);
                    }
                }
            }
            _ => None,
        };
        let process_reused = self
            .projects
            .lock()
            .unwrap()
            .get(&root)
            .is_some_and(|client| client.connection.is_alive());
        let client = match self.project_client_with_shutdown(&root, deadline, shutdown) {
            Ok(client) => client,
            Err(error) => {
                self.record_experimental_failure(&request.kind, &error, None);
                return Err(error);
            }
        };
        let started = Instant::now();
        let outcome = match request.kind.as_str() {
            EXPERIMENTAL_KIND_LIST => Ok(ExperimentalDispatchOutcome {
                value: client.experimental_list_tools(process_reused),
                tool_succeeded: true,
                error_code: None,
                write_state: None,
            }),
            EXPERIMENTAL_KIND_DESCRIBE => client
                .experimental_describe_tool(
                    tool_name.expect("describe preflight tool name"),
                    process_reused,
                )
                .map(|value| ExperimentalDispatchOutcome {
                    value,
                    tool_succeeded: true,
                    error_code: None,
                    write_state: None,
                }),
            EXPERIMENTAL_KIND_CALL => {
                let arguments = payload
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                client.experimental_tool_call(
                    tool_name.expect("call preflight tool name"),
                    arguments,
                    process_reused,
                    deadline,
                    shutdown,
                )
            }
            _ => Err(request_error()),
        };
        if !client.connection.is_alive() {
            self.projects.lock().unwrap().remove(&root);
        }
        match &outcome {
            Ok(success) => self.record_call(
                ProviderCallSummary {
                    capability: request.kind.clone(),
                    selected_provider: "claude_code".to_string(),
                    fallback_used: false,
                    result: if success.tool_succeeded {
                        "success".to_string()
                    } else {
                        "failure".to_string()
                    },
                    write_state: success
                        .write_state
                        .map(|state| experimental_write_state_label(state).to_string()),
                    duration_ms: elapsed_ms(started),
                    error_code: success
                        .error_code
                        .map(|code| experimental_error_code(code).to_string()),
                },
                // Only clear last_error_code when the tool itself succeeded.
                success.tool_succeeded,
            ),
            Err(error) => self.record_experimental_failure(&request.kind, error, Some(started)),
        }
        outcome
    }
}

pub(super) struct DiscoveredTool {
    pub(super) fields: BTreeSet<String>,
    description: String,
    /// Full schema when within the experimental size bound; `None` when oversized.
    input_schema: Option<Value>,
    schema_hash: String,
    schema_available: bool,
    schema_truncated: bool,
}

/// Build one bounded discovery entry from a raw tools/list item.
/// Returns `None` for entries without a sanitizable name.
pub(super) fn discovered_tool_entry(tool: &Value) -> Option<(String, DiscoveredTool)> {
    let name = tool.get("name")?.as_str()?.to_string();
    sanitize_tool_name(&name)?;
    let input_schema = tool
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(|| json!({"type": "object"}));
    // Hash the original full schema even when we drop the body for size.
    let schema_hash = schema_hash_hex(&input_schema);
    let schema_bytes = serde_json::to_vec(&input_schema).unwrap_or_default();
    let schema_truncated = schema_bytes.len() > MAX_EXPERIMENTAL_SCHEMA_BYTES;
    let schema_available = !schema_truncated;
    let fields = if schema_available {
        input_schema
            .pointer("/properties")
            .and_then(Value::as_object)
            .into_iter()
            .flat_map(|properties| properties.keys().cloned())
            .collect()
    } else {
        BTreeSet::new()
    };
    let description = tool
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .chars()
        .take(MAX_EXPERIMENTAL_DESCRIPTION_CHARS)
        .collect::<String>();
    Some((
        name,
        DiscoveredTool {
            fields,
            description,
            input_schema: schema_available.then_some(input_schema),
            schema_hash,
            schema_available,
            schema_truncated,
        },
    ))
}

impl ProjectMcpClient {
    pub(super) fn experimental_list_tools(&self, process_reused: bool) -> Value {
        let mut tools = self
            .tools
            .iter()
            .filter_map(|(name, tool)| {
                let name = sanitize_tool_name(name)?;
                Some(json!({
                    "name": name,
                    "schema_hash": tool.schema_hash,
                    "schema_available": tool.schema_available,
                    "callable": ExperimentalClaudeToolKind::is_callable(&name),
                }))
            })
            .collect::<Vec<_>>();
        tools.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        json!({
            "experimental": true,
            "claude_version": self.version,
            "process_reused": process_reused,
            "tools": tools,
            "truncated": self.tools_truncated,
        })
    }

    fn experimental_describe_tool(
        &self,
        tool_name: &str,
        process_reused: bool,
    ) -> Result<Value, ProviderError> {
        let tool = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ProviderError::new("claude_tool_not_found"))?;
        let mut description = tool.description.clone();
        let mut truncated = tool.schema_truncated;
        let input_schema = if let Some(schema) = tool.input_schema.as_ref() {
            schema.clone()
        } else {
            truncated = true;
            json!({
                "type": "object",
                "truncated": true,
                "note": "schema exceeded experimental describe bound",
            })
        };
        if description.chars().count() > MAX_EXPERIMENTAL_DESCRIPTION_CHARS {
            description = description
                .chars()
                .take(MAX_EXPERIMENTAL_DESCRIPTION_CHARS)
                .collect();
            truncated = true;
        }
        Ok(json!({
            "experimental": true,
            "tool_name": tool_name,
            "claude_version": self.version,
            "schema_hash": tool.schema_hash,
            "description": description,
            "input_schema": input_schema,
            "callable": ExperimentalClaudeToolKind::is_callable(tool_name),
            "process_reused": process_reused,
            "truncated": truncated,
        }))
    }

    fn experimental_tool_call(
        &self,
        tool_name: &str,
        arguments: Value,
        process_reused: bool,
        deadline: Instant,
        shutdown: Option<&AtomicBool>,
    ) -> Result<ExperimentalDispatchOutcome, ProviderError> {
        let tool = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ProviderError::new("claude_tool_not_found"))?;
        // Schema must be available before allowlist/validation so oversized schemas
        // surface as claude_schema_unavailable (tool exists) rather than not_found.
        let schema = tool
            .input_schema
            .as_ref()
            .ok_or_else(|| ProviderError::new("claude_schema_unavailable"))?;
        let kind = ExperimentalClaudeToolKind::parse(tool_name)
            .ok_or_else(|| ProviderError::new("claude_tool_not_allowed"))?;
        validate_against_schema(schema, &arguments)
            .map_err(|_| ProviderError::new("claude_arguments_invalid"))?;
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            // Preflight: request never written to Claude stdin.
            return Err(ProviderError::new("mcp_request_timeout"));
        }
        // tools/call has not been written yet; post-send failure_state applies only after send.
        let failure_state = kind.post_send_failure_state();
        let started = Instant::now();
        let result = self.connection.request_with_shutdown(
            "tools/call",
            json!({"name": tool_name, "arguments": arguments}),
            timeout,
            failure_state,
            shutdown,
        )?;
        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut result_value = result;
        let mut result_truncated = false;
        let encoded = serde_json::to_vec(&result_value).map_err(|_| protocol_error())?;
        if encoded.len() > MAX_EXPERIMENTAL_RESULT_BYTES {
            result_truncated = true;
            result_value = json!({
                "truncated": true,
                "note": "claude tool result exceeded experimental bound",
                "original_bytes": encoded.len(),
                "isError": is_error,
            });
            if encoded.len() > MAX_EXPERIMENTAL_RESULT_BYTES * 2 {
                // Hard bound after a completed tools/call: preserve post-send write-state.
                // Do not auto-retry mutating tools.
                return Err(ProviderError::new("claude_result_too_large").with_state(failure_state));
            }
        }
        let tool_status = if is_error { "failure" } else { "success" };
        let mut value = json!({
            "experimental": true,
            "tool_name": tool_name,
            "claude_version": self.version,
            "schema_hash": tool.schema_hash,
            "duration_ms": elapsed_ms(started),
            "process_reused": process_reused,
            "tool_status": tool_status,
            "is_error": is_error,
            "result": result_value,
            "result_truncated": result_truncated,
        });
        // Tool-level isError still completed tools/call; apply class write-state.
        let outcome_write_state = if is_error {
            let (label, changed) = match failure_state {
                WriteState::NotSubmitted => ("not_submitted", Value::Bool(false)),
                WriteState::Uncertain => ("uncertain", Value::Null),
            };
            value["write_state"] = json!(label);
            value["changed"] = changed;
            Some(failure_state)
        } else {
            None
        };
        Ok(ExperimentalDispatchOutcome {
            value,
            tool_succeeded: !is_error,
            error_code: is_error.then_some("claude_tool_error"),
            write_state: outcome_write_state,
        })
    }
}

pub(super) fn schema_hash_hex(schema: &Value) -> String {
    let canonical = canonicalize_json(schema);
    sha256_hex_bytes(canonical.as_bytes())
}

fn canonicalize_json(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().cloned().collect::<Vec<_>>();
            keys.sort();
            let parts = keys
                .into_iter()
                .map(|key| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(&key).unwrap_or_else(|_| "\"\"".into()),
                        canonicalize_json(&map[&key])
                    )
                })
                .collect::<Vec<_>>();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(items) => {
            let parts = items.iter().map(canonicalize_json).collect::<Vec<_>>();
            format!("[{}]", parts.join(","))
        }
        other => serde_json::to_string(other).unwrap_or_else(|_| "null".into()),
    }
}

/// Minimal JSON Schema subset for Claude harness tools (not a full engine).
pub(super) fn validate_against_schema(schema: &Value, value: &Value) -> Result<(), ()> {
    validate_schema_node(schema, value)
}

fn validate_schema_node(schema: &Value, value: &Value) -> Result<(), ()> {
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|item| item == value) {
            return Err(());
        }
    }
    let type_name = schema.get("type").and_then(Value::as_str);
    match type_name {
        Some("object") => {
            let object = value.as_object().ok_or(())?;
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for key in required {
                    let key = key.as_str().ok_or(())?;
                    if !object.contains_key(key) {
                        return Err(());
                    }
                }
            }
            let properties = schema
                .get("properties")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let additional = schema
                .get("additionalProperties")
                .cloned()
                .unwrap_or(Value::Bool(true));
            for (key, item) in object {
                if let Some(property_schema) = properties.get(key) {
                    validate_schema_node(property_schema, item)?;
                } else {
                    match &additional {
                        Value::Bool(false) => return Err(()),
                        Value::Bool(true) | Value::Null => {}
                        other => validate_schema_node(other, item)?,
                    }
                }
            }
            Ok(())
        }
        Some("array") => {
            let items = value.as_array().ok_or(())?;
            if let Some(item_schema) = schema.get("items") {
                for item in items {
                    validate_schema_node(item_schema, item)?;
                }
            }
            Ok(())
        }
        Some("string") => value.is_string().then_some(()).ok_or(()),
        Some("integer") => (value.as_i64().is_some() || value.as_u64().is_some())
            .then_some(())
            .ok_or(()),
        Some("number") => value.as_f64().is_some().then_some(()).ok_or(()),
        Some("boolean") => value.is_boolean().then_some(()).ok_or(()),
        Some("null") => value.is_null().then_some(()).ok_or(()),
        _ => {
            for branch_key in ["oneOf", "anyOf"] {
                if let Some(branches) = schema.get(branch_key).and_then(Value::as_array) {
                    return if branches
                        .iter()
                        .any(|branch| validate_schema_node(branch, value).is_ok())
                    {
                        Ok(())
                    } else {
                        Err(())
                    };
                }
            }
            Ok(())
        }
    }
}
