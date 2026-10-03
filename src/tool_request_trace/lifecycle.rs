//! Lifecycle responsibility of the existing trace subsystem.
use super::*;

/// Record bounded arguments entering the canonical parser, not a proof of
/// successful parsing, authorization or later execution normalization.
pub(crate) fn capture_effective_arguments(tool: &str, arguments: &Value) {
    if !tool_request_trace_enabled() {
        return;
    }
    let Some(trace_id) = current_active_trace_id() else {
        return;
    };
    let Some(diagnostic) = diagnostics::arguments(tool, arguments) else {
        return;
    };
    let mut event = base_event(&trace_id, "tool_trace_diagnostic");
    merge_event_fields(
        &mut event,
        json!({"phase":"kernel_arguments", "tool_name":tool, "diagnostic":diagnostic}),
    );
    enqueue_metadata_event(&trace_id, "kernel_arguments", event);
}

/// Record canonical normalization/effect evidence before model compression.
/// Fields come from the producer's result; missing fields remain unobserved.
pub(crate) fn capture_execution_evidence(tool: &str, output: &Value) {
    if !tool_request_trace_enabled() || !diagnostics::selected(tool) {
        return;
    }
    let Some(trace_id) = current_active_trace_id() else {
        return;
    };
    let mut evidence = serde_json::Map::new();
    for name in [
        "input_normalization",
        "execution_state",
        "command_execution_state",
        "command_started",
        "command_completed",
        "effective_timeout_secs",
        "sync_wait_secs",
        "state_changed",
        "failure_kind",
        "job_id",
        "resolved_project",
    ] {
        if let Some(value) = output.get(name) {
            evidence.insert(name.into(), value.clone());
        }
    }
    let Some(diagnostic) = diagnostics::arguments(tool, &Value::Object(evidence)) else {
        return;
    };
    let mut event = base_event(&trace_id, "tool_trace_diagnostic");
    merge_event_fields(
        &mut event,
        json!({"phase":"execution_evidence", "tool_name":tool, "diagnostic":diagnostic}),
    );
    enqueue_metadata_event(&trace_id, "execution_evidence", event);
}

/// Closed call-site phase/outcome labels only; no arguments or payloads.
/// The existing bounded trace writer never backpressures requests.
pub(crate) fn record_phase_latency(phase: &'static str, started: Instant, outcome: &'static str) {
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    tracing::debug!(target: "webcodex::phase", phase, outcome, elapsed_ms, "runtime phase completed");
    if !tool_request_trace_enabled() {
        return;
    }
    let Some(trace_id) = current_active_trace_id() else {
        return;
    };
    let mut event = base_event(&trace_id, "tool_phase_latency");
    merge_event_fields(
        &mut event,
        json!({"phase": phase, "outcome": outcome, "elapsed_ms": elapsed_ms}),
    );
    enqueue_metadata_event(&trace_id, phase, event);
}

/// Canonical HTTP-adapter completion timing for one request. The absolute
/// handoff timestamp is anchored at the request-observed wall clock and
/// advanced by monotonic elapsed time, preserving sub-second precision without
/// allowing a wall-clock adjustment to create a negative request duration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RequestCompletionTiming {
    pub(crate) request_observed_at_ms: i64,
    pub(crate) response_handed_at_ms: i64,
    pub(crate) elapsed_ms: u64,
}

/// Lifecycle guard shared by MCP `/mcp` and API `/api/tools/call` handlers.
pub struct ToolRequestLifecycle {
    pub(super) prefix: &'static str,
    pub(super) mode: ToolRequestTraceMode,
    pub(super) trace_id: String,
    pub(super) jsonrpc_id: String,
    pub(super) method: String,
    pub(super) tool_name: Option<String>,
    pub(super) client_window: Option<ClientWindow>,
    pub(super) app_call_id: Option<String>,
    pub(super) suppress_payload_capture: bool,
    pub(super) request_observed_at_ms: i64,
    pub(super) started: Instant,
    pub(super) completed: AtomicBool,
}

pub(super) fn tool_suppresses_payload_capture(tool_name: Option<&str>) -> bool {
    matches!(
        tool_name,
        Some(
            "read_tool_trace"
                | "bind_agent_continuation"
                | "recover_agent_continuation_endpoint"
                | "get_agent_continuation_state"
                | "acquire_agent_continuation_wake"
                | "prepare_agent_continuation_wake"
                | "finish_agent_continuation_wake"
                | "unbind_agent_continuation"
                | "bind_job_terminal_continuation"
                | "get_job_terminal_continuation_state"
                | "prepare_job_terminal_continuation"
                | "finish_job_terminal_continuation"
                | "unbind_job_terminal_continuation"
        )
    )
}

impl ToolRequestLifecycle {
    pub fn new(
        prefix: &'static str,
        trace_id: String,
        jsonrpc_id: impl Into<String>,
        method: impl Into<String>,
        tool_name: Option<String>,
    ) -> Self {
        let suppress_payload_capture = tool_suppresses_payload_capture(tool_name.as_deref());
        let request_observed_at_ms = chrono::Utc::now().timestamp_millis();
        Self {
            prefix,
            mode: crate::config::tool_request_trace_mode(),
            trace_id,
            jsonrpc_id: jsonrpc_id.into(),
            method: method.into(),
            tool_name,
            client_window: None,
            app_call_id: None,
            suppress_payload_capture,
            request_observed_at_ms,
            started: Instant::now(),
            completed: AtomicBool::new(false),
        }
    }

    pub fn enabled(&self) -> bool {
        self.mode != ToolRequestTraceMode::Off
    }

    pub fn full_enabled(&self) -> bool {
        self.mode == ToolRequestTraceMode::Full
    }

    pub fn active_trace_id(&self) -> Option<String> {
        self.enabled().then(|| self.trace_id.clone())
    }

    /// Stable safe request-correlation id even when full request tracing is
    /// disabled. Window activity may use this UUID without enabling or
    /// persisting trace payloads.
    pub fn correlation_trace_id(&self) -> String {
        self.trace_id.clone()
    }

    pub fn capture_payload(&self, phase: &str, value: &Value) {
        if phase == "final_response" && self.enabled() && !self.suppress_payload_capture {
            if let Some(summary) = self
                .tool_name
                .as_deref()
                .and_then(|tool| diagnostics::result(tool, value))
            {
                self.capture_diagnostic("response_summary", summary);
            }
        }
        if self.full_enabled() && !self.suppress_payload_capture {
            capture_payload_for_trace(&self.trace_id, phase, value);
        }
    }

    /// Observe bounded supplied arguments before envelope parsing. This does not
    /// invoke the full-payload lazy closure or infer successful normalization.
    pub(crate) fn capture_request_diagnostic(&self, entry: &str, arguments: &Value) {
        if !self.enabled() {
            return;
        }
        if let Some(summary) = diagnostics::request(entry, arguments) {
            self.capture_diagnostic("supplied_arguments", summary);
        }
    }

    pub(super) fn capture_diagnostic(&self, phase: &str, diagnostic: Value) {
        let mut event = base_event(&self.trace_id, "tool_trace_diagnostic");
        merge_event_fields(
            &mut event,
            json!({"phase": phase, "diagnostic": diagnostic,
            "tool_name": self.tool_name, "client_window_key": self.client_window.as_ref().map(ClientWindow::key)}),
        );
        enqueue_metadata_event(&self.trace_id, phase, event);
    }

    /// The MCP adapter supplies only its validated selection receipt. Never
    /// inspect raw headers, infer the caller brand, or change the policy here.
    pub(crate) fn capture_mcp_host_policy(
        &self,
        selection: &crate::mcp_host::McpHostPolicySelection,
    ) {
        if !self.enabled() {
            return;
        }
        let mut event = base_event(&self.trace_id, "mcp_request_policy_selected");
        merge_event_fields(
            &mut event,
            json!({
                "selection": selection, "method": self.method,
                "tool_name": self.tool_name,
                "client_window_key": self.client_window.as_ref().map(ClientWindow::key),
                "duration_ms": self.duration_ms(),
            }),
        );
        enqueue_metadata_event(&self.trace_id, "request_policy", event);
    }

    pub fn capture_payload_lazy<F>(&self, phase: &str, build: F)
    where
        F: FnOnce() -> Value,
    {
        if self.full_enabled() && !self.suppress_payload_capture {
            capture_owned_payload_for_trace(&self.trace_id, phase, build());
        }
    }

    pub fn set_method(&mut self, method: impl Into<String>) {
        self.method = method.into();
    }

    pub fn set_tool_name(&mut self, tool_name: Option<String>) {
        self.suppress_payload_capture = tool_suppresses_payload_capture(tool_name.as_deref());
        self.tool_name = tool_name;
    }

    pub fn set_jsonrpc_id(&mut self, jsonrpc_id: impl Into<String>) {
        self.jsonrpc_id = jsonrpc_id.into();
    }

    /// Attach an adapter-resolved client window for diagnostic correlation only.
    /// The lifecycle deliberately accepts `ClientWindow` rather than a raw host
    /// identifier so tracing cannot become another opaque-identity parser.
    pub(crate) fn set_client_window(&mut self, window: Option<&ClientWindow>) {
        self.client_window = window.cloned();
    }

    /// Attach one bounded App-generated correlation id for diagnostics only.
    /// The MCP adapter validates and strips this field before ToolRuntime parsing;
    /// it is never authority and never contains Agent, Endpoint, Wake, or binding ids.
    pub(crate) fn set_app_call_id(&mut self, app_call_id: Option<String>) {
        self.app_call_id = app_call_id;
    }

    pub fn duration_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub(crate) fn request_observed_at_ms(&self) -> i64 {
        self.request_observed_at_ms
    }

    pub(super) fn completion_timing(&self) -> RequestCompletionTiming {
        let elapsed_ms = self.duration_ms();
        let elapsed_i64 = i64::try_from(elapsed_ms).unwrap_or(i64::MAX);
        RequestCompletionTiming {
            request_observed_at_ms: self.request_observed_at_ms,
            response_handed_at_ms: self.request_observed_at_ms.saturating_add(elapsed_i64),
            elapsed_ms,
        }
    }

    pub fn mark_completed(&self) {
        self.completed.store(true, Ordering::SeqCst);
    }

    pub(super) fn event_name(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.prefix)
    }

    pub fn log(
        &self,
        suffix: &str,
        http_status: Option<u16>,
        estimated_json_bytes: Option<usize>,
        protocol_success: Option<bool>,
        tool_success: Option<bool>,
        category: &str,
    ) {
        self.log_with_duration(
            suffix,
            http_status,
            estimated_json_bytes,
            protocol_success,
            tool_success,
            category,
            self.duration_ms(),
        );
    }

    pub(super) fn log_with_duration(
        &self,
        suffix: &str,
        http_status: Option<u16>,
        estimated_json_bytes: Option<usize>,
        protocol_success: Option<bool>,
        tool_success: Option<bool>,
        category: &str,
        duration_ms: u64,
    ) {
        if !self.enabled() {
            return;
        }
        let event = self.event_name(suffix);
        let mode = match self.mode {
            ToolRequestTraceMode::Off => "off",
            ToolRequestTraceMode::Metadata => "metadata",
            ToolRequestTraceMode::Full => "full",
        };
        tracing::info!(
            event = %event,
            server_trace_id = %self.trace_id,
            trace_mode = mode,
            jsonrpc_id = %self.jsonrpc_id,
            method = %self.method,
            tool_name = self.tool_name.as_deref().unwrap_or("-"),
            client_window_key = self.client_window.as_ref().map(ClientWindow::key).unwrap_or("-"),
            client_window_source = self.client_window.as_ref().map(ClientWindow::source).unwrap_or("-"),
            app_call_id = self.app_call_id.as_deref().unwrap_or("-"),
            duration_ms,
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
        // Metadata persists the same bounded lifecycle index, without raw payloads.
        {
            let mut stored = base_event(&self.trace_id, &event);
            merge_event_fields(
                &mut stored,
                json!({
                    "trace_mode": mode,
                    "jsonrpc_id": self.jsonrpc_id.as_str(),
                    "method": self.method.as_str(),
                    "tool_name": self.tool_name.as_deref(),
                    "client_window_key": self.client_window.as_ref().map(ClientWindow::key),
                    "client_window_source": self.client_window.as_ref().map(ClientWindow::source),
                    "app_call_id": self.app_call_id.as_deref(),
                    "duration_ms": duration_ms,
                    "estimated_json_bytes": estimated_json_bytes,
                    "http_status": http_status,
                    "protocol_success": protocol_success,
                    "tool_success": tool_success,
                    "category": category,
                }),
            );
            enqueue_metadata_event(&self.trace_id, &event, stored);
        }
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
    ) -> RequestCompletionTiming {
        let timing = self.completion_timing();
        self.log_with_duration(
            "tool_handler_returned",
            Some(http_status),
            estimated_json_bytes,
            protocol_success,
            tool_success,
            category,
            timing.elapsed_ms,
        );
        self.mark_completed();
        timing
    }
}

impl Drop for ToolRequestLifecycle {
    fn drop(&mut self) {
        if !self.enabled() || self.completed.load(Ordering::SeqCst) {
            return;
        }
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
