//! Experimental transport-neutral JavaScript orchestration for WebCodex.
//!
//! The default build contains only host/runtime contracts. The V8 implementation
//! is available exclusively through the `v8-runtime` feature.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;
pub const MAX_TIMEOUT_MS: u64 = 30_000;
pub const MAX_TOOL_CALLS: usize = 32;
pub const MAX_CONCURRENT_TOOL_CALLS: usize = 8;
/// Default process-local cap for simultaneously active V8 cells. E1 runs V8 on
/// the Server, so bound isolate/thread fanout independently from nested tool-call
/// concurrency. Resource-rich dogfood hosts may raise this through the bounded
/// server-process environment override without changing the safe default.
pub const DEFAULT_MAX_CONCURRENT_EXECUTIONS: usize = 2;
pub const MAX_CONFIGURED_CONCURRENT_EXECUTIONS: usize = 64;
pub const MAX_CONCURRENT_EXECUTIONS_ENV: &str = "WEBPI_CODE_MODE_MAX_CONCURRENT_EXECUTIONS";

pub fn normalized_max_concurrent_executions(raw: Option<&str>) -> usize {
    raw.and_then(|value| value.trim().parse::<usize>().ok())
        .map(|value| value.clamp(1, MAX_CONFIGURED_CONCURRENT_EXECUTIONS))
        .unwrap_or(DEFAULT_MAX_CONCURRENT_EXECUTIONS)
}
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_OUTPUT_ITEMS: usize = 256;

pub type CodeModeHostFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Host callback boundary for one nested Code Mode tool call.
///
/// Implementations are responsible for applying their normal tool authority,
/// permission, routing, and evidence semantics. The JavaScript runtime itself
/// owns no filesystem, network, Project, Session, or tool authority.
pub trait CodeModeHost: Send + Sync {
    fn invoke_tool(
        &self,
        request: CodeModeToolRequest,
    ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>>;

    /// Monotonic frontend lifecycle fence. Effect-aware hosts use this to reject
    /// nested calls that have not crossed their canonical dispatch boundary when
    /// the frontend can no longer make decisions. Read-only/test hosts may no-op.
    fn stop_accepting_calls(&self) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeModeTerminationMode {
    /// Preserve E1 behavior: the frontend deadline is the return boundary.
    ReturnAtFrontendDeadline,
    /// Close nested-call admission, terminate the frontend, and drain host calls
    /// already started by the runtime for at most `max_drain_ms`. Any remaining
    /// host work is cancelled after that bound; consequential hosts must retain
    /// conservative outcome-unknown truth for already-dispatched effects.
    DrainStartedChildren { max_drain_ms: u64 },
}

impl CodeModeTerminationMode {
    pub const fn drain_timeout_ms(self) -> Option<u64> {
        match self {
            Self::ReturnAtFrontendDeadline => None,
            Self::DrainStartedChildren { max_drain_ms } => Some(max_drain_ms),
        }
    }

    pub const fn drains_started_children(self) -> bool {
        self.drain_timeout_ms().is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeToolRequest {
    /// Stable 1-based attempted-call ordinal assigned by the Code Mode frontend
    /// before host admission. It is diagnostic identity only and grants no
    /// authority, retry right, or durable execution identity.
    pub ordinal: usize,
    pub tool_name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeModeToolResponse {
    pub success: bool,
    pub output: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModeHostError {
    failure_kind: String,
    message: String,
}

impl CodeModeHostError {
    pub fn new(message: impl Into<String>) -> Self {
        Self::with_kind("host_failure", message)
    }

    pub fn with_kind(failure_kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            failure_kind: failure_kind.into(),
            message: message.into(),
        }
    }

    pub fn failure_kind(&self) -> &str {
        &self.failure_kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for CodeModeHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CodeModeHostError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeChildFailure {
    pub ordinal: usize,
    pub tool: String,
    pub failure_kind: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeLimit {
    pub kind: String,
    pub allowed: usize,
    pub current: usize,
    pub attempted: usize,
}

#[derive(Debug, Clone)]
pub struct CodeModeExecuteRequest {
    pub source: String,
    pub allowed_tools: Vec<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeStats {
    pub tool_calls: usize,
    pub max_in_flight: usize,
    pub duration_ms: u64,
    pub returned_bytes: usize,
    /// Diagnostic-only wait for the process-wide V8 execution slot. This is
    /// intentionally omitted from the model-facing four-field stats projection.
    #[serde(skip)]
    pub slot_wait_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeModeErrorKind {
    InvalidRequest,
    Runtime,
    ChildCallFailed,
    Timeout,
    ToolCallBudgetExceeded,
    OutputLimitExceeded,
}

impl CodeModeErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Runtime => "runtime_error",
            Self::ChildCallFailed => "child_call_failed",
            Self::Timeout => "timeout",
            Self::ToolCallBudgetExceeded => "tool_call_budget_exceeded",
            Self::OutputLimitExceeded => "output_limit_exceeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeModeError {
    pub kind: CodeModeErrorKind,
    pub message: String,
    pub stats: CodeModeStats,
    pub child_failure: Option<CodeModeChildFailure>,
    pub limit: Option<CodeModeLimit>,
}

impl std::fmt::Display for CodeModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CodeModeError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeModeExecution {
    pub content: Vec<String>,
    pub stats: CodeModeStats,
}

pub fn normalized_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    timeout_ms
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(1, MAX_TIMEOUT_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_concurrency_override_is_bounded_and_defaults_safely() {
        assert_eq!(normalized_max_concurrent_executions(None), 2);
        assert_eq!(normalized_max_concurrent_executions(Some("")), 2);
        assert_eq!(normalized_max_concurrent_executions(Some("invalid")), 2);
        assert_eq!(normalized_max_concurrent_executions(Some("0")), 1);
        assert_eq!(normalized_max_concurrent_executions(Some("16")), 16);
        assert_eq!(normalized_max_concurrent_executions(Some("999")), 64);
    }
}

#[cfg(feature = "v8-runtime")]
mod runtime;

#[cfg(feature = "v8-runtime")]
pub use runtime::{execute, execute_with_termination_mode};
