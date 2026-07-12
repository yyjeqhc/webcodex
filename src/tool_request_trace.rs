//! Gated, argument-free lifecycle tracing for tool entry handlers.
//!
//! Enabled only when `WEBCODEX_TOOL_REQUEST_TRACE=true` (default **false**).
//! When disabled, handlers must not serialize response bodies solely to measure
//! size, and this module emits no log lines.
//!
//! Events record operational metadata only (trace id, method, tool name,
//! duration, estimated JSON size, HTTP status, protocol/tool success, category).
//! They never log tool arguments, tool results, file contents, user messages,
//! or credentials.
//!
//! `*_tool_handler_returned` means the handler constructed a response and handed
//! it to the HTTP framework. It does **not** prove the client received the body;
//! combine with nginx `status` / `body_bytes_sent` / `request_time` for that.
//!
//! `*_tool_handler_incomplete_drop` is emitted when the lifecycle guard is
//! dropped without `mark_completed()`. That can mean client disconnect, cancel,
//! panic unwind, or any other early abort — it is **not** a confirmed TCP
//! disconnect.

use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use uuid::Uuid;

/// Whether tool-request lifecycle tracing is enabled.
///
/// Reads `WEBCODEX_TOOL_REQUEST_TRACE` via the shared `env_flag` helper.
/// Default **false** when unset or invalid.
pub fn tool_request_trace_enabled() -> bool {
    crate::config::tool_request_trace_enabled()
}

/// Generate a short server-side trace id for one inbound handler invocation.
pub fn new_trace_id() -> String {
    Uuid::new_v4().to_string()
}

/// Safe JSON-RPC id summary: type + length + short digest. Never the raw string.
pub fn jsonrpc_id_safe(id: Option<&serde_json::Value>) -> String {
    use serde_json::Value;
    match id {
        None => "none".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Bool(v)) => format!("bool:{v}"),
        Some(Value::Number(n)) => format!("number:{n}"),
        Some(Value::String(s)) => {
            let digest = Sha256::digest(s.as_bytes());
            format!(
                "string:len={}:sha256_8={:02x}{:02x}{:02x}{:02x}",
                s.len(),
                digest[0],
                digest[1],
                digest[2],
                digest[3]
            )
        }
        Some(Value::Array(a)) => {
            let raw = serde_json::to_vec(a).unwrap_or_default();
            let digest = Sha256::digest(&raw);
            format!(
                "array:len={}:sha256_8={:02x}{:02x}{:02x}{:02x}",
                a.len(),
                digest[0],
                digest[1],
                digest[2],
                digest[3]
            )
        }
        Some(Value::Object(o)) => {
            let raw = serde_json::to_vec(o).unwrap_or_default();
            let digest = Sha256::digest(&raw);
            format!(
                "object:keys={}:sha256_8={:02x}{:02x}{:02x}{:02x}",
                o.len(),
                digest[0],
                digest[1],
                digest[2],
                digest[3]
            )
        }
    }
}

/// Estimate serialized JSON byte length for diagnostics.
///
/// Returns `None` when tracing is disabled so callers never pay for a size-only
/// `serde_json::to_vec` of the response body.
pub fn estimate_json_bytes(value: &serde_json::Value) -> Option<usize> {
    if !tool_request_trace_enabled() {
        return None;
    }
    serde_json::to_vec(value).ok().map(|b| b.len())
}

/// Lifecycle guard shared by MCP `/mcp` and API `/api/tools/call` handlers.
pub struct ToolRequestLifecycle {
    /// Stable event name prefix, e.g. `"mcp"` or `"api"`.
    prefix: &'static str,
    enabled: bool,
    trace_id: String,
    jsonrpc_id: String,
    method: String,
    tool_name: Option<String>,
    started: Instant,
    completed: AtomicBool,
}

impl ToolRequestLifecycle {
    pub fn new(
        prefix: &'static str,
        trace_id: String,
        jsonrpc_id: impl Into<String>,
        method: impl Into<String>,
        tool_name: Option<String>,
    ) -> Self {
        Self {
            prefix,
            enabled: tool_request_trace_enabled(),
            trace_id,
            jsonrpc_id: jsonrpc_id.into(),
            method: method.into(),
            tool_name,
            started: Instant::now(),
            completed: AtomicBool::new(false),
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    #[allow(dead_code)]
    pub fn trace_id(&self) -> &str {
        &self.trace_id
    }

    pub fn set_method(&mut self, method: impl Into<String>) {
        self.method = method.into();
    }

    pub fn set_tool_name(&mut self, tool_name: Option<String>) {
        self.tool_name = tool_name;
    }

    pub fn set_jsonrpc_id(&mut self, jsonrpc_id: impl Into<String>) {
        self.jsonrpc_id = jsonrpc_id.into();
    }

    pub fn duration_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub fn mark_completed(&self) {
        self.completed.store(true, Ordering::SeqCst);
    }

    fn event_name(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.prefix)
    }

    /// Log a lifecycle event.
    ///
    /// `estimated_json_bytes`: `Some(n)` when size is known; `None` when not
    /// computed (logged as `-1`). Never invent `0` for "unknown".
    ///
    /// `protocol_success`: JSON-RPC / HTTP envelope success when applicable.
    /// `tool_success`: tool kernel success when a tool ran; `None` otherwise.
    pub fn log(
        &self,
        suffix: &str,
        http_status: Option<u16>,
        estimated_json_bytes: Option<usize>,
        protocol_success: Option<bool>,
        tool_success: Option<bool>,
        category: &str,
    ) {
        if !self.enabled {
            return;
        }
        let event = self.event_name(suffix);
        tracing::info!(
            event = %event,
            server_trace_id = %self.trace_id,
            jsonrpc_id = %self.jsonrpc_id,
            method = %self.method,
            tool_name = self.tool_name.as_deref().unwrap_or("-"),
            duration_ms = self.duration_ms(),
            estimated_json_bytes = estimated_json_bytes.map(|b| b as i64).unwrap_or(-1),
            http_status = http_status.map(|s| s as i32).unwrap_or(-1),
            protocol_success = protocol_success
                .map(|s| if s { 1_i32 } else { 0_i32 })
                .unwrap_or(-1),
            tool_success = tool_success
                .map(|s| if s { 1_i32 } else { 0_i32 })
                .unwrap_or(-1),
            category = category,
            "{event}"
        );
    }

    pub fn received(&self) {
        self.log("tool_request_received", None, None, None, None, "received");
    }

    pub fn parsed(&self, category: &str) {
        self.log("tool_request_parsed", None, None, None, None, category);
    }

    pub fn dispatch_started(&self) {
        self.log("tool_dispatch_started", None, None, None, None, "started");
    }

    pub fn dispatch_finished(
        &self,
        protocol_success: bool,
        tool_success: Option<bool>,
        category: &str,
    ) {
        self.log(
            "tool_dispatch_finished",
            None,
            None,
            Some(protocol_success),
            tool_success,
            category,
        );
    }

    pub fn dispatch_failed(&self, category: &str) {
        self.log(
            "tool_dispatch_failed",
            None,
            None,
            Some(false),
            Some(false),
            category,
        );
    }

    pub fn response_serialized(
        &self,
        http_status: u16,
        estimated_json_bytes: Option<usize>,
        protocol_success: Option<bool>,
        tool_success: Option<bool>,
        category: &str,
    ) {
        self.log(
            "tool_response_serialized",
            Some(http_status),
            estimated_json_bytes,
            protocol_success,
            tool_success,
            category,
        );
    }

    /// Response constructed and handed to the HTTP framework (not client ACK).
    pub fn handler_returned(
        &self,
        http_status: u16,
        estimated_json_bytes: Option<usize>,
        protocol_success: Option<bool>,
        tool_success: Option<bool>,
        category: &str,
    ) {
        self.log(
            "tool_handler_returned",
            Some(http_status),
            estimated_json_bytes,
            protocol_success,
            tool_success,
            category,
        );
        self.mark_completed();
    }
}

impl Drop for ToolRequestLifecycle {
    fn drop(&mut self) {
        if !self.enabled || self.completed.load(Ordering::SeqCst) {
            return;
        }
        // Neutral name: incomplete Drop is not proof of client disconnect.
        self.log(
            "tool_handler_incomplete_drop",
            None,
            None,
            None,
            None,
            "handler_dropped_before_response",
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jsonrpc_id_safe_never_echoes_raw_string() {
        let secret = "very-secret-request-id-value";
        let summary = jsonrpc_id_safe(Some(&json!(secret)));
        assert!(!summary.contains(secret));
        assert!(summary.starts_with("string:len="));
        assert!(summary.contains("sha256_8="));
    }

    #[test]
    fn estimate_json_bytes_is_none_when_trace_disabled() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_TOOL_REQUEST_TRACE");
        assert!(estimate_json_bytes(&json!({"a": 1})).is_none());
        std::env::set_var("WEBCODEX_TOOL_REQUEST_TRACE", "true");
        let n = estimate_json_bytes(&json!({"a": 1})).expect("size when enabled");
        assert!(n > 0);
        std::env::remove_var("WEBCODEX_TOOL_REQUEST_TRACE");
    }

    #[test]
    fn incomplete_drop_is_safe_when_disabled() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_TOOL_REQUEST_TRACE");
        let guard = ToolRequestLifecycle::new(
            "mcp",
            "trace-test".into(),
            "none",
            "tools/call",
            Some("list_projects".into()),
        );
        assert!(!guard.enabled());
        guard.received();
        drop(guard);
    }

    #[test]
    fn completed_drop_is_silent() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::set_var("WEBCODEX_TOOL_REQUEST_TRACE", "true");
        let guard =
            ToolRequestLifecycle::new("api", "trace-ok".into(), "-", "POST /api/tools/call", None);
        guard.handler_returned(200, Some(12), Some(true), Some(true), "ok");
        drop(guard);
        std::env::remove_var("WEBCODEX_TOOL_REQUEST_TRACE");
    }
}
