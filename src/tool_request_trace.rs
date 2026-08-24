//! Gated lifecycle and forensic tracing for model-facing tool invocations.
//!
//! `WEBCODEX_TOOL_REQUEST_TRACE=true|metadata` preserves the historical
//! metadata-only behavior. `WEBCODEX_TOOL_REQUEST_TRACE=full` additionally
//! persists semantic JSON request/argument/result payloads on the Server host.
//! Full payloads are zstd-compressed files under a bounded trace directory; they
//! are deliberately not stored in the canonical runtime database.
//!
//! Full tracing is an explicit self-hosted operator diagnostic mode. It may
//! contain file contents, command input/output, user messages, or other tool
//! payload data. The trace path never reads WebCodex ingress HTTP Authorization
//! headers; credential-like values that are themselves part of a tool/Runner
//! payload are captured like any other payload field. Trace persistence is
//! fail-open: storage, compression, pruning, or correlation failures never change
//! tool execution correctness.
//!
//! `*_tool_handler_returned` means the handler constructed a response and handed
//! it to the HTTP framework. It does **not** prove the client received the body;
//! combine with reverse-proxy `status` / `body_bytes_sent` / `request_time` for
//! that transport boundary.

use crate::config::ToolRequestTraceMode;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::future::Future;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use uuid::Uuid;

tokio::task_local! {
    static ACTIVE_TOOL_TRACE_ID: String;
}

const TRACE_CORRELATION_TTL_SECS: i64 = 24 * 60 * 60;
const MAX_TRACE_CORRELATIONS: usize = 8_192;

static TRACE_IO_LOCK: Mutex<()> = Mutex::new(());
static TRACE_CORRELATIONS: OnceLock<Mutex<TraceCorrelations>> = OnceLock::new();

#[derive(Debug, Clone)]
struct TraceCorrelation {
    trace_id: String,
    request_id: String,
    job_id: Option<String>,
    created_at: i64,
}

#[derive(Default)]
struct TraceCorrelations {
    requests: HashMap<String, TraceCorrelation>,
    jobs: HashMap<String, TraceCorrelation>,
}

#[derive(Debug)]
struct TraceDirInfo {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
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

fn current_active_trace_id() -> Option<String> {
    ACTIVE_TOOL_TRACE_ID.try_with(Clone::clone).ok()
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
/// serialization of the response body.
pub fn estimate_json_bytes(value: &serde_json::Value) -> Option<usize> {
    if !tool_request_trace_enabled() {
        return None;
    }
    serde_json::to_vec(value).ok().map(|bytes| bytes.len())
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

fn trace_root() -> PathBuf {
    crate::config::tool_request_trace_dir()
}

fn trace_retention() -> Duration {
    Duration::from_secs(crate::config::tool_request_trace_retention_hours().saturating_mul(60 * 60))
}

fn trace_budget() -> u64 {
    crate::config::tool_request_trace_max_total_bytes()
}

fn directory_stats(path: &Path) -> io::Result<(u64, SystemTime)> {
    let metadata = fs::metadata(path)?;
    let mut bytes = if metadata.is_file() {
        metadata.len()
    } else {
        0
    };
    let mut modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let (child_bytes, child_modified) = directory_stats(&entry.path())?;
            bytes = bytes.saturating_add(child_bytes);
            if child_modified > modified {
                modified = child_modified;
            }
        }
    }
    Ok((bytes, modified))
}

fn trace_dirs(root: &Path) -> io::Result<Vec<TraceDirInfo>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        let (bytes, modified) = directory_stats(&path)?;
        dirs.push(TraceDirInfo {
            path,
            bytes,
            modified,
        });
    }
    Ok(dirs)
}

/// Enforce age and total-byte bounds before one new file/event is persisted.
/// The active trace is never deleted out from under its current write; if it
/// alone cannot fit, the new capture is omitted instead.
fn reserve_trace_capacity(root: &Path, trace_id: &str, incoming: u64) -> io::Result<bool> {
    fs::create_dir_all(root)?;
    let retention = trace_retention();
    let now = SystemTime::now();
    for info in trace_dirs(root)? {
        if info.path.file_name().and_then(|name| name.to_str()) == Some(trace_id) {
            continue;
        }
        if now
            .duration_since(info.modified)
            .map(|age| age > retention)
            .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(info.path);
        }
    }

    let mut dirs = trace_dirs(root)?;
    let mut total = dirs
        .iter()
        .fold(0_u64, |sum, info| sum.saturating_add(info.bytes));
    if total.saturating_add(incoming) <= trace_budget() {
        return Ok(true);
    }
    dirs.sort_by_key(|info| info.modified);
    for info in dirs {
        if info.path.file_name().and_then(|name| name.to_str()) == Some(trace_id) {
            continue;
        }
        if total.saturating_add(incoming) <= trace_budget() {
            break;
        }
        fs::remove_dir_all(&info.path)?;
        total = total.saturating_sub(info.bytes);
    }
    Ok(total.saturating_add(incoming) <= trace_budget())
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

fn append_event_locked(root: &Path, trace_id: &str, event: &Value) -> io::Result<bool> {
    let mut line = serde_json::to_vec(event).map_err(io::Error::other)?;
    line.push(b'\n');
    if !reserve_trace_capacity(root, trace_id, line.len() as u64)? {
        return Ok(false);
    }
    let trace_dir = root.join(trace_id);
    fs::create_dir_all(&trace_dir)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_dir.join("events.jsonl"))?;
    file.write_all(&line)?;
    Ok(true)
}

fn persist_metadata_event(trace_id: &str, event: Value) -> io::Result<bool> {
    let _lock = TRACE_IO_LOCK
        .lock()
        .map_err(|_| io::Error::other("tool trace I/O lock poisoned"))?;
    append_event_locked(&trace_root(), trace_id, &event)
}

fn persist_payload(
    trace_id: &str,
    phase: &str,
    value: &Value,
) -> io::Result<Option<(usize, usize, String, String)>> {
    let raw = serde_json::to_vec(value).map_err(io::Error::other)?;
    let digest = sha256_hex(&raw);
    let compressed = zstd::stream::encode_all(&raw[..], 3)?;
    let file_name = format!("{}-{}.json.zst", Uuid::new_v4(), safe_phase(phase));
    let relative_path = format!("payloads/{file_name}");
    let mut event = base_event(trace_id, "tool_trace_payload_captured");
    merge_event_fields(
        &mut event,
        json!({
            "phase": phase,
            "payload_bytes": raw.len(),
            "compressed_bytes": compressed.len(),
            "payload_sha256": digest,
            "payload_path": relative_path,
            "encoding": "json+zstd",
        }),
    );
    let mut event_line = serde_json::to_vec(&event).map_err(io::Error::other)?;
    event_line.push(b'\n');
    let incoming = (compressed.len() + event_line.len()) as u64;

    let _lock = TRACE_IO_LOCK
        .lock()
        .map_err(|_| io::Error::other("tool trace I/O lock poisoned"))?;
    let root = trace_root();
    if !reserve_trace_capacity(&root, trace_id, incoming)? {
        return Ok(None);
    }
    let trace_dir = root.join(trace_id);
    let payload_dir = trace_dir.join("payloads");
    fs::create_dir_all(&payload_dir)?;
    let final_path = payload_dir.join(&file_name);
    let temp_path = payload_dir.join(format!(".{file_name}.tmp"));
    fs::write(&temp_path, &compressed)?;
    if let Err(error) = fs::rename(&temp_path, &final_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }
    if let Err(error) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_dir.join("events.jsonl"))
        .and_then(|mut file| file.write_all(&event_line))
    {
        let _ = fs::remove_file(&final_path);
        return Err(error);
    }
    Ok(Some((raw.len(), compressed.len(), digest, relative_path)))
}

fn capture_payload_for_trace(trace_id: &str, phase: &str, value: &Value) {
    if !full_trace_enabled() {
        return;
    }
    match persist_payload(trace_id, phase, value) {
        Ok(Some((payload_bytes, compressed_bytes, digest, path))) => tracing::info!(
            event = "tool_trace_payload_persisted",
            server_trace_id = %trace_id,
            phase = phase,
            payload_bytes = payload_bytes as u64,
            compressed_bytes = compressed_bytes as u64,
            payload_sha256 = %digest,
            payload_path = %path,
            "tool_trace_payload_persisted"
        ),
        Ok(None) => tracing::warn!(
            event = "tool_trace_capture_omitted",
            server_trace_id = %trace_id,
            phase = phase,
            reason = "trace_disk_budget_exceeded",
            "tool_trace_capture_omitted"
        ),
        Err(error) => tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %trace_id,
            phase = phase,
            error = %error,
            "tool_trace_capture_failed"
        ),
    }
}

fn correlations() -> &'static Mutex<TraceCorrelations> {
    TRACE_CORRELATIONS.get_or_init(|| Mutex::new(TraceCorrelations::default()))
}

fn prune_correlations(correlations: &mut TraceCorrelations) {
    let cutoff = now_ts().saturating_sub(TRACE_CORRELATION_TTL_SECS);
    correlations
        .requests
        .retain(|_, correlation| correlation.created_at >= cutoff);
    correlations
        .jobs
        .retain(|_, correlation| correlation.created_at >= cutoff);
    while correlations.requests.len() > MAX_TRACE_CORRELATIONS {
        let Some(oldest) = correlations
            .requests
            .iter()
            .min_by_key(|(_, correlation)| correlation.created_at)
            .map(|(request_id, _)| request_id.clone())
        else {
            break;
        };
        correlations.requests.remove(&oldest);
    }
    while correlations.jobs.len() > MAX_TRACE_CORRELATIONS {
        let Some(oldest) = correlations
            .jobs
            .iter()
            .min_by_key(|(_, correlation)| correlation.created_at)
            .map(|(job_id, _)| job_id.clone())
        else {
            break;
        };
        correlations.jobs.remove(&oldest);
    }
}

/// Record the exact Server→Runner request identity selected for the current
/// model-facing tool call. This keeps only a bounded in-memory correlation index;
/// full request bodies remain in the file-backed trace store.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_runner_request_enqueued<T: Serialize>(
    request_payload: &T,
    request_id: &str,
    client_id: &str,
    kind: &str,
    job_id: Option<&str>,
    agent_instance_id: Option<&str>,
    runner_transport: Option<&str>,
    runner_version: Option<&str>,
    runner_git_commit: Option<&str>,
) {
    let Some(trace_id) = current_active_trace_id() else {
        return;
    };
    if full_trace_enabled() {
        match serde_json::to_value(request_payload) {
            Ok(value) => capture_payload_for_trace(&trace_id, "runner_request", &value),
            Err(error) => tracing::warn!(
                event = "tool_trace_capture_failed",
                server_trace_id = %trace_id,
                phase = "runner_request",
                error = %error,
                "tool_trace_capture_failed"
            ),
        }
    }
    tracing::info!(
        event = "tool_runner_request_enqueued",
        server_trace_id = %trace_id,
        runner_request_id = request_id,
        runner_client_id = client_id,
        runner_request_kind = kind,
        runner_job_id = job_id.unwrap_or("-"),
        runner_agent_instance_id = agent_instance_id.unwrap_or("-"),
        runner_transport = runner_transport.unwrap_or("-"),
        runner_version = runner_version.unwrap_or("-"),
        runner_git_commit = runner_git_commit.unwrap_or("-"),
        "tool_runner_request_enqueued"
    );

    if let Ok(mut correlations) = correlations().lock() {
        prune_correlations(&mut correlations);
        let correlation = TraceCorrelation {
            trace_id: trace_id.clone(),
            request_id: request_id.to_string(),
            job_id: job_id.map(str::to_string),
            created_at: now_ts(),
        };
        correlations
            .requests
            .insert(request_id.to_string(), correlation.clone());
        if let Some(job_id) = job_id {
            correlations.jobs.insert(job_id.to_string(), correlation);
        }
    }

    if full_trace_enabled() {
        let mut event = base_event(&trace_id, "tool_runner_request_enqueued");
        merge_event_fields(
            &mut event,
            json!({
                "runner_request_id": request_id,
                "runner_client_id": client_id,
                "runner_request_kind": kind,
                "runner_job_id": job_id,
                "runner_agent_instance_id": agent_instance_id,
                "runner_transport": runner_transport,
                "runner_version": runner_version,
                "runner_git_commit": runner_git_commit,
            }),
        );
        if let Err(error) = persist_metadata_event(&trace_id, event) {
            tracing::warn!(
                event = "tool_trace_capture_failed",
                server_trace_id = %trace_id,
                phase = "runner_request_enqueued",
                error = %error,
                "tool_trace_capture_failed"
            );
        }
    }
}

fn lookup_request_correlation(request_id: &str) -> Option<TraceCorrelation> {
    let mut correlations = correlations().lock().ok()?;
    prune_correlations(&mut correlations);
    correlations.requests.get(request_id).cloned()
}

fn lookup_job_correlation(request_id: Option<&str>, job_id: &str) -> Option<TraceCorrelation> {
    let mut correlations = correlations().lock().ok()?;
    prune_correlations(&mut correlations);
    request_id
        .and_then(|request_id| correlations.requests.get(request_id).cloned())
        .or_else(|| correlations.jobs.get(job_id).cloned())
}

fn remove_correlation(correlation: &TraceCorrelation) {
    if let Ok(mut correlations) = correlations().lock() {
        correlations.requests.remove(&correlation.request_id);
        if let Some(job_id) = correlation.job_id.as_deref() {
            correlations.jobs.remove(job_id);
        }
    }
}

/// Capture the raw typed Runner result at the Server-side correlation boundary.
pub(crate) fn capture_runner_result<T: Serialize>(request_id: &str, payload: &T) {
    let Some(correlation) = lookup_request_correlation(request_id) else {
        return;
    };
    match serde_json::to_value(payload) {
        Ok(value) => capture_payload_for_trace(&correlation.trace_id, "runner_result", &value),
        Err(error) => tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %correlation.trace_id,
            phase = "runner_result",
            error = %error,
            "tool_trace_capture_failed"
        ),
    }
    if correlation.job_id.is_none() {
        remove_correlation(&correlation);
    }
}

/// Capture Runner Job updates, including terminal materialization, into the trace
/// that originally admitted the Job. Updates lacking request_id can still use
/// the bounded job_id correlation created at enqueue.
pub(crate) fn capture_runner_job_update<T: Serialize>(
    request_id: Option<&str>,
    job_id: &str,
    finished: bool,
    payload: &T,
) {
    let Some(correlation) = lookup_job_correlation(request_id, job_id) else {
        return;
    };
    match serde_json::to_value(payload) {
        Ok(value) => capture_payload_for_trace(&correlation.trace_id, "runner_job_update", &value),
        Err(error) => tracing::warn!(
            event = "tool_trace_capture_failed",
            server_trace_id = %correlation.trace_id,
            phase = "runner_job_update",
            error = %error,
            "tool_trace_capture_failed"
        ),
    }
    if finished {
        remove_correlation(&correlation);
    }
}

/// Lifecycle guard shared by MCP `/mcp` and API `/api/tools/call` handlers.
pub struct ToolRequestLifecycle {
    prefix: &'static str,
    mode: ToolRequestTraceMode,
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
            mode: crate::config::tool_request_trace_mode(),
            trace_id,
            jsonrpc_id: jsonrpc_id.into(),
            method: method.into(),
            tool_name,
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

    pub fn capture_payload(&self, phase: &str, value: &Value) {
        if self.full_enabled() {
            capture_payload_for_trace(&self.trace_id, phase, value);
        }
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

    pub fn log(
        &self,
        suffix: &str,
        http_status: Option<u16>,
        estimated_json_bytes: Option<usize>,
        protocol_success: Option<bool>,
        tool_success: Option<bool>,
        category: &str,
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
        if self.full_enabled() {
            let mut stored = base_event(&self.trace_id, &event);
            merge_event_fields(
                &mut stored,
                json!({
                    "trace_mode": mode,
                    "jsonrpc_id": self.jsonrpc_id.as_str(),
                    "method": self.method.as_str(),
                    "tool_name": self.tool_name.as_deref(),
                    "duration_ms": self.duration_ms(),
                    "estimated_json_bytes": estimated_json_bytes,
                    "http_status": http_status,
                    "protocol_success": protocol_success,
                    "tool_success": tool_success,
                    "category": category,
                }),
            );
            if let Err(error) = persist_metadata_event(&self.trace_id, stored) {
                tracing::warn!(
                    event = "tool_trace_capture_failed",
                    server_trace_id = %self.trace_id,
                    phase = %event,
                    error = %error,
                    "tool_trace_capture_failed"
                );
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn payload_files(root: &Path, trace_id: &str) -> Vec<PathBuf> {
        let payload_dir = root.join(trace_id).join("payloads");
        let Ok(entries) = fs::read_dir(payload_dir) else {
            return Vec::new();
        };
        entries.flatten().map(|entry| entry.path()).collect()
    }

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
        let mut env = crate::test_support::TestEnvGuard::new();
        env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
        assert!(estimate_json_bytes(&json!({"a": 1})).is_none());
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "true");
        assert!(estimate_json_bytes(&json!({"a": 1})).is_some());
        env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
    }

    #[test]
    fn metadata_mode_never_creates_raw_payload_store() {
        let temp = tempfile::tempdir().unwrap();
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "true");
        env.set(
            "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
            temp.path().to_string_lossy().as_ref(),
        );
        let guard = ToolRequestLifecycle::new(
            "mcp",
            "trace-metadata".into(),
            "none",
            "tools/call",
            Some("write_project_file".into()),
        );
        guard.capture_payload("raw_arguments", &json!({"content": "private"}));
        assert!(!temp.path().join("trace-metadata").exists());
    }

    #[test]
    fn full_mode_persists_complete_compressed_payload() {
        let temp = tempfile::tempdir().unwrap();
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
        env.set(
            "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
            temp.path().to_string_lossy().as_ref(),
        );
        env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
        let guard = ToolRequestLifecycle::new(
            "mcp",
            "trace-full".into(),
            "none",
            "tools/call",
            Some("write_project_file".into()),
        );
        let payload = json!({
            "ack_session_context_revision": 42,
            "content": "large-body-".repeat(100_000),
        });
        guard.capture_payload("raw_arguments", &payload);
        let files = payload_files(temp.path(), "trace-full");
        assert_eq!(files.len(), 1);
        let compressed = fs::read(&files[0]).unwrap();
        let raw = zstd::stream::decode_all(&compressed[..]).unwrap();
        assert_eq!(serde_json::from_slice::<Value>(&raw).unwrap(), payload);
        let events = fs::read_to_string(temp.path().join("trace-full/events.jsonl")).unwrap();
        assert!(events.contains("raw_arguments"));
        assert!(events.contains("payload_sha256"));
    }

    #[test]
    fn full_mode_disk_failure_is_fail_open() {
        let temp = tempfile::tempdir().unwrap();
        let not_a_directory = temp.path().join("trace-file");
        fs::write(&not_a_directory, b"occupied").unwrap();
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
        env.set(
            "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
            not_a_directory.to_string_lossy().as_ref(),
        );
        let guard = ToolRequestLifecycle::new(
            "api",
            "trace-fail-open".into(),
            "-",
            "POST /api/tools/call",
            Some("read_file".into()),
        );
        guard.capture_payload("raw_arguments", &json!({"path": "README.md"}));
        assert_eq!(fs::read(&not_a_directory).unwrap(), b"occupied");
    }

    #[test]
    fn full_mode_omits_payload_that_cannot_fit_disk_budget() {
        let temp = tempfile::tempdir().unwrap();
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
        env.set(
            "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
            temp.path().to_string_lossy().as_ref(),
        );
        env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "1");
        let guard = ToolRequestLifecycle::new(
            "mcp",
            "trace-budget".into(),
            "none",
            "tools/call",
            Some("write_project_file".into()),
        );
        guard.capture_payload("raw_arguments", &json!({"content": "must-not-truncate"}));
        assert!(payload_files(temp.path(), "trace-budget").is_empty());
    }

    #[tokio::test]
    async fn runner_correlation_survives_original_dispatch_scope() {
        let temp = tempfile::tempdir().unwrap();
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
        env.set(
            "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
            temp.path().to_string_lossy().as_ref(),
        );
        let guard = ToolRequestLifecycle::new(
            "mcp",
            "trace-runner".into(),
            "none",
            "tools/call",
            Some("run_process".into()),
        );
        scope_active_trace(guard.active_trace_id(), async {
            record_runner_request_enqueued(
                &json!({"kind": "run_process", "argv": ["cargo", "test"]}),
                "request-1",
                "runner-1",
                "run_process",
                None,
                Some("instance-1"),
                Some("quic"),
                Some("0.3.8"),
                Some("abc123"),
            );
        })
        .await;
        capture_runner_result("request-1", &json!({"exit_code": 0, "stdout": "ok"}));
        let events = fs::read_to_string(temp.path().join("trace-runner/events.jsonl")).unwrap();
        assert!(events.contains("tool_runner_request_enqueued"));
        let events = events
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        let read_phase = |phase: &str| {
            let relative = events
                .iter()
                .find(|event| {
                    event["event"] == "tool_trace_payload_captured" && event["phase"] == phase
                })
                .and_then(|event| event["payload_path"].as_str())
                .unwrap_or_else(|| panic!("missing trace payload phase {phase}"));
            let compressed = fs::read(temp.path().join("trace-runner").join(relative)).unwrap();
            let raw = zstd::stream::decode_all(compressed.as_slice()).unwrap();
            serde_json::from_slice::<Value>(&raw).unwrap()
        };
        assert_eq!(
            read_phase("runner_request"),
            json!({"kind": "run_process", "argv": ["cargo", "test"]})
        );
        assert_eq!(
            read_phase("runner_result"),
            json!({"exit_code": 0, "stdout": "ok"})
        );
    }

    #[test]
    fn incomplete_drop_is_safe_when_disabled() {
        let mut env = crate::test_support::TestEnvGuard::new();
        env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
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
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_TOOL_REQUEST_TRACE", "true");
        let guard =
            ToolRequestLifecycle::new("api", "trace-ok".into(), "-", "POST /api/tools/call", None);
        guard.handler_returned(200, Some(12), Some(true), Some(true), "ok");
        drop(guard);
        env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
    }
}
