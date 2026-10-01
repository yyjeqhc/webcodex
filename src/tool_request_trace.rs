//! Gated lifecycle and forensic tracing for model-facing tool invocations.
//!
//! `WEBCODEX_TOOL_REQUEST_TRACE=true|metadata` persists bounded lifecycle and
//! selected operator diagnostics without raw bodies. `full` additionally
//! persists semantic JSON request/argument/result payloads on the Server host.
//! Full payloads are zstd-compressed files under a bounded trace directory; they
//! are deliberately not stored in the canonical runtime database. Compression,
//! filesystem persistence, reconciliation, and pruning run on a bounded dedicated
//! writer thread so diagnostic trace maintenance never blocks tool request workers.
//!
//! Full tracing is an explicit self-hosted operator diagnostic mode. It may
//! contain file contents, command input/output, user messages, or other tool
//! payload data. The trace path never reads WebCodex ingress HTTP Authorization
//! headers; credential-like values that are themselves part of a tool/Runner
//! payload are captured like any other payload field. Trace persistence is
//! fail-open: storage, compression, pruning, correlation failures, or writer
//! saturation never change tool execution correctness; saturated queues drop
//! diagnostic records instead of backpressuring tool execution.
//!
//! `*_tool_handler_returned` means the handler constructed a response and handed
//! it to the HTTP framework. It does **not** prove the client received the body;
//! combine with reverse-proxy `status` / `body_bytes_sent` / `request_time` for
//! that transport boundary.

use crate::client_window::ClientWindow;
use crate::config::ToolRequestTraceMode;
use crate::json_digest::update_sha256_with_json;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::future::Future;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
#[cfg(unix)]
use std::{os::unix::fs::OpenOptionsExt, os::unix::fs::PermissionsExt};
use uuid::Uuid;

mod diagnostics;
mod reader;
pub(crate) use reader::{capture_mode, read_trace};

tokio::task_local! {
    static ACTIVE_TOOL_TRACE_ID: String;
}

// Bounded process-wide counters, not per-trace completeness or durable history.
static TRACE_QUEUE_DROPS: AtomicU64 = AtomicU64::new(0);
static TRACE_BUDGET_DROPS: AtomicU64 = AtomicU64::new(0);
static TRACE_WRITE_FAILURES: AtomicU64 = AtomicU64::new(0);

fn capture_health() -> Value {
    json!({"scope":"server_process_since_start_not_per_trace",
        "queue_drops":TRACE_QUEUE_DROPS.load(Ordering::Relaxed),
        "budget_drops":TRACE_BUDGET_DROPS.load(Ordering::Relaxed),
        "write_failures":TRACE_WRITE_FAILURES.load(Ordering::Relaxed)})
}

const MAX_MODEL_TRACE_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_MODEL_TRACE_COMPRESSED_BYTES: usize = 512 * 1024;
const MAX_TRACE_EVENTS_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TRACE_PAYLOAD_ENTRIES: usize = 4096;
const DEFAULT_TRACE_INDEX_LIMIT: usize = 20;
const MAX_TRACE_INDEX_LIMIT: usize = 64;

#[derive(Debug, Clone)]
pub(crate) struct TraceReadError {
    pub(crate) kind: &'static str,
    pub(crate) message: String,
}

impl TraceReadError {
    fn new(kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/// Whether tool-request lifecycle tracing is enabled.
pub fn tool_request_trace_enabled() -> bool {
    crate::config::tool_request_trace_enabled()
}

fn full_trace_enabled() -> bool {
    crate::config::tool_request_trace_mode() == ToolRequestTraceMode::Full
}

/// Generate a server-side trace id for one inbound handler invocation.
pub fn new_trace_id() -> String {
    Uuid::new_v4().to_string()
}

/// Scope the current async tool dispatch so deeper Server→Runner enqueue code can
/// correlate its durable Runner `request_id` with this model-facing trace.
pub async fn scope_active_trace<F>(trace_id: Option<String>, future: F) -> F::Output
where
    F: Future,
{
    match trace_id {
        Some(trace_id) => ACTIVE_TOOL_TRACE_ID.scope(trace_id, future).await,
        None => future.await,
    }
}

pub(crate) fn current_active_trace_id() -> Option<String> {
    ACTIVE_TOOL_TRACE_ID.try_with(Clone::clone).ok()
}

/// Opaque reference for the currently executing inbound request, but only when
/// the Server is actually retaining full payload traces. This never exposes the
/// native trace root or a filesystem path.
pub(crate) fn current_full_trace_ref() -> Option<String> {
    full_trace_enabled().then(current_active_trace_id).flatten()
}

fn json_sha256_or_empty<T: Serialize + ?Sized>(value: &T) -> [u8; 32] {
    let mut hasher = Sha256::new();
    if update_sha256_with_json(&mut hasher, value).is_err() {
        // Preserve the historical `to_vec(...).unwrap_or_default()` fallback:
        // serialization failure hashes an empty byte sequence, never a partial one.
        hasher = Sha256::new();
    }
    hasher.finalize().into()
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
            let digest = json_sha256_or_empty(a);
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
            let digest = json_sha256_or_empty(o);
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
/// serialization of the response body.
pub fn estimate_json_bytes<T: serde::Serialize + ?Sized>(value: &T) -> Option<usize> {
    if !tool_request_trace_enabled() {
        return None;
    }
    crate::json_measurement::serialized_json_len(value).ok()
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn safe_phase(phase: &str) -> String {
    let safe: String = phase
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    if safe.is_empty() {
        "payload".to_string()
    } else {
        safe
    }
}

fn base_event(trace_id: &str, event: &str) -> Value {
    let build = crate::build_info::current();
    json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "event": event,
        "server_trace_id": trace_id,
        "server_version": build.version,
        "server_git_commit": build.git_commit,
        "server_git_dirty": build.git_dirty,
    })
}

fn merge_event_fields(event: &mut Value, fields: Value) {
    let (Some(event), Value::Object(fields)) = (event.as_object_mut(), fields) else {
        return;
    };
    event.extend(fields);
}

#[cfg(test)]
mod tests;

mod writer;
use writer::*;
mod correlation;
#[cfg(test)]
use correlation::*;
mod store;
use store::*;
mod payload;
use payload::*;
mod lifecycle;
use lifecycle::*;

pub(crate) use correlation::capture_runner_job_update;
pub(crate) use correlation::capture_runner_result;
pub(crate) use correlation::finalize_runner_job_correlation;
pub(crate) use correlation::finalize_runner_result_correlation;
pub(crate) use correlation::record_runner_request_enqueued;
pub(crate) use lifecycle::capture_effective_arguments;
pub(crate) use lifecycle::capture_execution_evidence;
pub(crate) use lifecycle::RequestCompletionTiming;
pub use lifecycle::ToolRequestLifecycle;
pub(crate) use payload::read_full_trace;
#[cfg(test)]
pub(crate) use writer::flush_full_trace_writer;
