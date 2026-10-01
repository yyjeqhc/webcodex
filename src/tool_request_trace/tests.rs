use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn payload_files(root: &Path, trace_id: &str) -> Vec<PathBuf> {
    let payload_dir = root.join(trace_id).join("payloads");
    let Ok(entries) = fs::read_dir(payload_dir) else {
        return Vec::new();
    };
    entries.flatten().map(|entry| entry.path()).collect()
}

fn reset_trace_store_accounting() {
    flush_full_trace_writer();
    *trace_io_state().lock().unwrap() = TraceStoreAccounting::default();
}

fn event_line_len(event: &Value) -> u64 {
    serde_json::to_vec(event).unwrap().len() as u64 + 1
}

struct CountingSerialize<'a> {
    calls: &'a AtomicUsize,
    value: Value,
}

impl serde::Serialize for CountingSerialize<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        serde::Serialize::serialize(&self.value, serializer)
    }
}

#[test]
fn job_trace_correlation_requires_request_and_job_to_agree() {
    let request_correlation = TraceCorrelation {
        trace_id: "trace-a".to_string(),
        request_id: "request-a".to_string(),
        job_id: Some("job-a".to_string()),
        runner_kind: "run_process".to_string(),
        created_at: 1,
    };
    let other_correlation = TraceCorrelation {
        trace_id: "trace-b".to_string(),
        request_id: "request-b".to_string(),
        job_id: Some("job-b".to_string()),
        runner_kind: "run_process".to_string(),
        created_at: 1,
    };
    let mut correlations = TraceCorrelations::default();
    correlations
        .requests
        .insert("request-a".to_string(), request_correlation.clone());
    correlations
        .jobs
        .insert("job-b".to_string(), other_correlation.clone());

    assert!(resolve_job_correlation(&correlations, Some("request-a"), "job-b").is_none());
    assert!(resolve_job_correlation(&correlations, Some("missing"), "job-b").is_none());
    assert_eq!(
        resolve_job_correlation(&correlations, None, "job-b"),
        Some(other_correlation)
    );

    correlations
        .jobs
        .insert("job-a".to_string(), request_correlation.clone());
    assert_eq!(
        resolve_job_correlation(&correlations, Some("request-a"), "job-a"),
        Some(request_correlation)
    );
}

fn accounting_snapshot() -> (PathBuf, u64, usize, u64) {
    let accounting = trace_io_state().lock().unwrap();
    let config = accounting.config.as_ref().expect("trace accounting config");
    (
        config.root.clone(),
        accounting.total_bytes,
        accounting.traces.len(),
        accounting.filesystem_scans,
    )
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
fn jsonrpc_id_safe_streaming_digest_matches_buffered_json() {
    for value in [
        json!(["quote=\"", "slash=\\", "Unicode 你好 🦀", {"nested": [1, 2, 3]}]),
        json!({
            "escaped": "line\nnext\tvalue",
            "unicode": "日本語 🌍",
            "nested": {"array": [null, true, 42]}
        }),
    ] {
        let raw = serde_json::to_vec(&value).unwrap();
        let digest = Sha256::digest(&raw);
        let expected = match &value {
            Value::Array(values) => format!(
                "array:len={}:sha256_8={:02x}{:02x}{:02x}{:02x}",
                values.len(),
                digest[0],
                digest[1],
                digest[2],
                digest[3]
            ),
            Value::Object(values) => format!(
                "object:keys={}:sha256_8={:02x}{:02x}{:02x}{:02x}",
                values.len(),
                digest[0],
                digest[1],
                digest[2],
                digest[3]
            ),
            _ => unreachable!(),
        };
        assert_eq!(jsonrpc_id_safe(Some(&value)), expected);
    }
}

#[test]
fn estimate_json_bytes_is_none_when_trace_disabled() {
    let mut env = crate::test_support::TestEnvGuard::new();
    let calls = AtomicUsize::new(0);
    let measured = CountingSerialize {
        calls: &calls,
        value: json!({"escaped": "line\n\"quoted\"", "unicode": "你好"}),
    };

    env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
    assert!(estimate_json_bytes(&measured).is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "true");
    assert_eq!(
        estimate_json_bytes(&measured),
        Some(serde_json::to_vec(&measured.value).unwrap().len())
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
}

#[test]
fn job_terminal_app_tools_suppress_full_payload_capture() {
    for name in [
        "bind_job_terminal_continuation",
        "get_job_terminal_continuation_state",
        "prepare_job_terminal_continuation",
        "finish_job_terminal_continuation",
        "unbind_job_terminal_continuation",
    ] {
        assert!(
            tool_suppresses_payload_capture(Some(name)),
            "{name} must not persist App-private binding/message payloads in full traces"
        );
    }
    assert!(!tool_suppresses_payload_capture(Some(
        "present_job_terminal_continuation"
    )));
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
fn lazy_payload_capture_only_materializes_unsuppressed_full_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();

    env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
    let off_calls = AtomicUsize::new(0);
    let off = ToolRequestLifecycle::new(
        "mcp",
        "trace-lazy-off".into(),
        "none",
        "tools/call",
        Some("write_project_file".into()),
    );
    off.capture_payload_lazy("raw_arguments", || {
        off_calls.fetch_add(1, Ordering::SeqCst);
        json!({"mode": "off"})
    });
    assert_eq!(off_calls.load(Ordering::SeqCst), 0);
    drop(off);

    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "true");
    let metadata_calls = AtomicUsize::new(0);
    let metadata = ToolRequestLifecycle::new(
        "mcp",
        "trace-lazy-metadata".into(),
        "none",
        "tools/call",
        Some("write_project_file".into()),
    );
    metadata.capture_payload_lazy("raw_arguments", || {
        metadata_calls.fetch_add(1, Ordering::SeqCst);
        json!({"mode": "metadata"})
    });
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 0);
    drop(metadata);

    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let full_calls = AtomicUsize::new(0);
    let expected = json!({"mode": "full", "nested": [1, 2, 3]});
    let full = ToolRequestLifecycle::new(
        "mcp",
        "trace-lazy-full".into(),
        "none",
        "tools/call",
        Some("write_project_file".into()),
    );
    full.capture_payload_lazy("raw_arguments", || {
        full_calls.fetch_add(1, Ordering::SeqCst);
        expected.clone()
    });
    assert_eq!(full_calls.load(Ordering::SeqCst), 1);
    drop(full);
    flush_full_trace_writer();
    let payload = payload_files(temp.path(), "trace-lazy-full")
        .into_iter()
        .next()
        .expect("full lazy trace payload");
    let compressed = fs::read(payload).unwrap();
    let raw = zstd::stream::decode_all(compressed.as_slice()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&raw).unwrap(), expected);

    let suppressed_calls = AtomicUsize::new(0);
    let mut suppressed = ToolRequestLifecycle::new(
        "mcp",
        "trace-lazy-suppressed".into(),
        "none",
        "tools/call",
        Some("call_runtime_tool".into()),
    );
    suppressed.set_tool_name(Some("read_tool_trace".into()));
    suppressed.capture_payload_lazy("effective_arguments", || {
        suppressed_calls.fetch_add(1, Ordering::SeqCst);
        json!({"trace_ref": "must-not-materialize"})
    });
    assert_eq!(suppressed_calls.load(Ordering::SeqCst), 0);
    drop(suppressed);
    flush_full_trace_writer();
    assert!(payload_files(temp.path(), "trace-lazy-suppressed").is_empty());
}

#[test]
fn full_mode_lifecycle_persists_only_hashed_client_window_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let raw_window = "chatgpt-window-opaque-secret";
    let params = json!({
        "name": "get_runtime_status",
        "arguments": {},
        "_meta": {"openai/session": raw_window}
    });
    let resolved = crate::client_window::stateless_mcp_window(&params);
    let window = resolved.identity.expect("valid OpenAI session window");
    let expected_key = window.key().to_string();
    let trace_ids = ["window-trace-a", "window-trace-b"];

    for trace_id in trace_ids {
        let mut guard = ToolRequestLifecycle::new(
            "mcp",
            trace_id.into(),
            "none",
            "tools/call",
            Some("get_runtime_status".into()),
        );
        guard.set_client_window(Some(&window));
        guard.parsed("ok");
        guard.mark_completed();
    }

    let absent_trace_id = "window-trace-absent";
    let absent = ToolRequestLifecycle::new(
        "mcp",
        absent_trace_id.into(),
        "none",
        "tools/call",
        Some("get_runtime_status".into()),
    );
    absent.parsed("ok");
    absent.mark_completed();
    flush_full_trace_writer();

    for trace_id in trace_ids {
        let events = fs::read_to_string(temp.path().join(trace_id).join("events.jsonl")).unwrap();
        assert!(!events.contains(raw_window));
        let event: Value = serde_json::from_str(events.lines().next().unwrap()).unwrap();
        assert_eq!(event["client_window_key"], expected_key);
        assert_eq!(event["client_window_source"], "openai-session");
        assert_eq!(event["server_trace_id"], trace_id);
    }

    let absent_events =
        fs::read_to_string(temp.path().join(absent_trace_id).join("events.jsonl")).unwrap();
    let absent_event: Value = serde_json::from_str(absent_events.lines().next().unwrap()).unwrap();
    assert!(absent_event["client_window_key"].is_null());
    assert!(absent_event["client_window_source"].is_null());
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
        "ack_session_message_ids": ["wc_msg_abcd-efgh_ijklmn"],
        "content": "large-body-".repeat(100_000),
    });
    guard.capture_payload("raw_arguments", &payload);
    drop(guard);
    flush_full_trace_writer();
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
fn full_mode_trace_reader_lists_then_reads_verified_payload_without_native_paths() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let trace_id = Uuid::new_v4().to_string();
    let payload = json!({
        "private_diagnostic": "visible only through the side channel",
        "nested": [1, 2, 3]
    });
    assert!(persist_payload(&trace_id, "runner_result", &payload)
        .unwrap()
        .is_some());

    let listing = read_full_trace(&trace_id, None, None, None).unwrap();
    assert_eq!(listing["payload_count"], 1);
    assert_eq!(listing["returned_count"], 1);
    assert_eq!(listing["payloads"][0]["payload_index"], 0);
    assert_eq!(listing["payloads"][0]["phase"], "runner_result");
    assert_eq!(listing["payloads"][0]["payload_available"], true);
    let serialized_listing = serde_json::to_string(&listing).unwrap();
    assert!(!serialized_listing.contains("payload_path"));
    assert!(!serialized_listing.contains(temp.path().to_string_lossy().as_ref()));
    assert!(!serialized_listing.contains("private_diagnostic"));

    let selected = read_full_trace(&trace_id, None, None, Some(0)).unwrap();
    assert_eq!(selected["payload_index"], 0);
    assert_eq!(selected["phase"], "runner_result");
    assert_eq!(selected["payload_available"], true);
    assert_eq!(selected["payload"], payload);
}

#[test]
fn full_mode_trace_reader_rejects_unsafe_refs_and_never_returns_oversize_payload() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let unsafe_ref = read_full_trace("../etc/passwd", None, None, None).unwrap_err();
    assert_eq!(unsafe_ref.kind, "invalid_trace_ref");
    let canonical = Uuid::new_v4().to_string();
    let uppercase = canonical.to_ascii_uppercase();
    let uppercase_error = read_full_trace(&uppercase, None, None, None).unwrap_err();
    assert_eq!(uppercase_error.kind, "invalid_trace_ref");

    let large_payload = json!({"body": "x".repeat(MAX_MODEL_TRACE_PAYLOAD_BYTES + 1024)});
    assert!(
        persist_payload(&canonical, "final_response", &large_payload)
            .unwrap()
            .is_some()
    );
    let selected = read_full_trace(&canonical, None, None, Some(0)).unwrap();
    assert_eq!(selected["payload_available"], false);
    assert_eq!(selected["reason"], "payload_exceeds_model_read_limit");
    assert_eq!(selected["max_payload_bytes"], MAX_MODEL_TRACE_PAYLOAD_BYTES);
    assert!(selected.get("payload").is_none());
}

#[test]
fn read_tool_trace_lifecycle_never_recursively_captures_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let trace_id = Uuid::new_v4().to_string();
    let mut guard = ToolRequestLifecycle::new(
        "mcp",
        trace_id.clone(),
        "none",
        "tools/call",
        Some("call_runtime_tool".into()),
    );
    // Adaptive routing begins at the gateway and later identifies the real
    // target. The setter must suppress every subsequent forensic payload.
    guard.set_tool_name(Some("read_tool_trace".into()));
    guard.capture_payload(
        "effective_arguments",
        &json!({"trace_ref": Uuid::new_v4().to_string()}),
    );
    guard.capture_payload("final_response", &json!({"payload": "PRIVATE_RAW_TRACE"}));
    drop(guard);
    flush_full_trace_writer();
    assert!(payload_files(temp.path(), &trace_id).is_empty());
}

#[test]
fn agent_continuation_app_lifecycle_never_captures_host_binding_or_resume_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    for tool_name in [
        "bind_agent_continuation",
        "recover_agent_continuation_endpoint",
        "get_agent_continuation_state",
        "acquire_agent_continuation_wake",
        "prepare_agent_continuation_wake",
        "finish_agent_continuation_wake",
        "unbind_agent_continuation",
    ] {
        let trace_id = Uuid::new_v4().to_string();
        let mut guard = ToolRequestLifecycle::new(
            "mcp",
            trace_id.clone(),
            "none",
            "tools/call",
            Some(tool_name.to_string()),
        );
        guard.set_app_call_id(Some("wc_app_call_0123456789abcdef_1".to_string()));
        guard.parsed("ok");
        guard.capture_payload(
            "raw_request",
            &json!({"binding_id": "wc_host_binding_PRIVATE", "consume_token": "PRIVATE"}),
        );
        guard.capture_payload(
                "final_response",
                &json!({"structuredContent": {"success": true, "output": {
                    "app_protocol": {"automatic_message": "consume_token=wc_wake_consume_PRIVATE_RESUME_ENVELOPE"}
                }}}),
            );
        drop(guard);
        flush_full_trace_writer();
        assert!(
            payload_files(temp.path(), &trace_id).is_empty(),
            "{tool_name} must suppress forensic payload capture"
        );
        let events = fs::read_to_string(temp.path().join(&trace_id).join("events.jsonl"))
            .expect("continuation trace metadata");
        assert!(events.contains("wc_app_call_0123456789abcdef_1"));
        assert!(events.contains("mcp_tool_request_parsed"));
        assert!(!events.contains("wc_host_binding_PRIVATE"));
        assert!(!events.contains("PRIVATE_RESUME_ENVELOPE"));
    }
}

#[test]
fn full_mode_capture_does_not_wait_for_trace_io_lock() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let io_guard = trace_io_state().lock().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let capture = thread::spawn(move || {
        let _trace_env = crate::test_support::TestToolRequestTraceEnvReaderGuard::new();
        let guard = ToolRequestLifecycle::new(
            "mcp",
            "trace-background-writer".into(),
            "none",
            "tools/call",
            Some("read_files".into()),
        );
        guard.capture_payload("raw_arguments", &json!({"path": "README.md"}));
        let _ = done_tx.send(());
    });
    let returned_without_io = done_rx.recv_timeout(Duration::from_millis(250)).is_ok();
    drop(io_guard);
    capture.join().unwrap();
    assert!(
        returned_without_io,
        "full trace capture must enqueue without waiting for trace-store I/O"
    );
    flush_full_trace_writer();
    assert_eq!(
        payload_files(temp.path(), "trace-background-writer").len(),
        1
    );
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
        Some("read_files".into()),
    );
    guard.capture_payload("raw_arguments", &json!({"path": "README.md"}));
    drop(guard);
    flush_full_trace_writer();
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
    drop(guard);
    flush_full_trace_writer();
    assert!(payload_files(temp.path(), "trace-budget").is_empty());
}

#[test]
fn full_mode_accounting_tracks_writes_without_rescanning_hot_path() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let trace_id = "trace-accounting-hot-path";
    assert!(persist_metadata_event(trace_id, json!({"event": "first"})).unwrap());
    let scans_after_first = accounting_snapshot().3;
    assert_eq!(scans_after_first, 1);

    assert!(persist_payload(
        trace_id,
        "raw_arguments",
        &json!({"content": "payload".repeat(512)})
    )
    .unwrap()
    .is_some());
    assert!(persist_metadata_event(trace_id, json!({"event": "last"})).unwrap());

    let (_, cached_total, trace_count, scans_after_writes) = accounting_snapshot();
    let actual_total = directory_stats(&temp.path().join(trace_id)).unwrap().0;
    assert_eq!(trace_count, 1);
    assert_eq!(cached_total, actual_total);
    assert_eq!(scans_after_writes, scans_after_first);
}

#[test]
fn full_mode_accounting_rebuilds_when_trace_root_changes() {
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        first_root.path().to_string_lossy().as_ref(),
    );
    reset_trace_store_accounting();
    assert!(persist_metadata_event("trace-first-root", json!({"event": "first"})).unwrap());
    let first_scans = accounting_snapshot().3;

    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        second_root.path().to_string_lossy().as_ref(),
    );
    assert!(persist_metadata_event("trace-second-root", json!({"event": "second"})).unwrap());
    let (root, _, trace_count, scans) = accounting_snapshot();
    assert_eq!(root, second_root.path());
    assert_eq!(trace_count, 1);
    assert_eq!(scans, first_scans + 1);
    assert!(second_root
        .path()
        .join("trace-second-root/events.jsonl")
        .exists());
}

#[test]
fn full_mode_accounting_rebuilds_when_same_root_config_changes() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_RETENTION_HOURS", "2");
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();
    assert!(persist_metadata_event("trace-config", json!({"event": "first"})).unwrap());
    let first_scans = accounting_snapshot().3;

    env.set("WEBCODEX_TOOL_REQUEST_TRACE_RETENTION_HOURS", "3");
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "16777216");
    assert!(persist_metadata_event("trace-config", json!({"event": "second"})).unwrap());
    let accounting = trace_io_state().lock().unwrap();
    assert_eq!(accounting.filesystem_scans, first_scans + 1);
    assert_eq!(
        accounting.config.as_ref().unwrap().retention,
        Duration::from_secs(3 * 60 * 60)
    );
    assert_eq!(accounting.config.as_ref().unwrap().budget, 16_777_216);
}

#[test]
fn full_mode_due_maintenance_reconciles_external_owned_drift() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();
    assert!(persist_metadata_event("trace-known", json!({"event": "first"})).unwrap());
    let first_scans = accounting_snapshot().3;

    let external = temp.path().join("trace-external-owned");
    create_private_trace_dir(&external).unwrap();
    ensure_trace_owner_marker(&external).unwrap();
    fs::write(external.join("events.jsonl"), vec![b'x'; 311]).unwrap();
    {
        let mut accounting = trace_io_state().lock().unwrap();
        accounting.last_reconcile = Some(Instant::now() - TRACE_STORE_RECONCILE_INTERVAL);
    }

    assert!(persist_metadata_event("trace-known", json!({"event": "second"})).unwrap());
    let (_, cached_total, trace_count, scans) = accounting_snapshot();
    assert_eq!(scans, first_scans + 1);
    assert_eq!(trace_count, 2);
    assert_eq!(cached_total, directory_stats(temp.path()).unwrap().0);
}

#[test]
fn full_mode_invalidated_accounting_rebuilds_on_next_write() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();
    let trace_id = "trace-rebuild-invalidated";
    assert!(persist_metadata_event(trace_id, json!({"event": "first"})).unwrap());
    let first_scans = accounting_snapshot().3;
    trace_io_state().lock().unwrap().invalidate();

    assert!(persist_metadata_event(trace_id, json!({"event": "second"})).unwrap());
    let (_, cached_total, _, scans) = accounting_snapshot();
    assert_eq!(scans, first_scans + 1);
    assert_eq!(
        cached_total,
        directory_stats(&temp.path().join(trace_id)).unwrap().0
    );
}

#[test]
fn full_mode_budget_eviction_updates_cached_total_without_rescan() {
    let temp = tempfile::tempdir().unwrap();
    let event = json!({"event": "budget", "padding": "x".repeat(128)});
    let line_len = event_line_len(&event);
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES",
        (line_len * 2).to_string(),
    );
    reset_trace_store_accounting();

    assert!(persist_metadata_event("trace-oldest", event.clone()).unwrap());
    assert!(persist_metadata_event("trace-newer", event.clone()).unwrap());
    let scans_before_eviction = accounting_snapshot().3;
    {
        let mut accounting = trace_io_state().lock().unwrap();
        accounting.traces.get_mut("trace-oldest").unwrap().modified = SystemTime::UNIX_EPOCH;
        accounting.traces.get_mut("trace-newer").unwrap().modified =
            SystemTime::UNIX_EPOCH + Duration::from_secs(1);
    }

    assert!(persist_metadata_event("trace-current", event).unwrap());
    let (_, cached_total, trace_count, scans_after_eviction) = accounting_snapshot();
    assert!(!temp.path().join("trace-oldest").exists());
    assert!(temp.path().join("trace-newer").exists());
    assert!(temp.path().join("trace-current").exists());
    assert_eq!(trace_count, 2);
    assert_eq!(cached_total, line_len * 2);
    assert_eq!(cached_total, directory_stats(temp.path()).unwrap().0);
    assert_eq!(scans_after_eviction, scans_before_eviction);
}

#[test]
fn full_mode_active_trace_is_not_evicted_for_its_own_write() {
    let temp = tempfile::tempdir().unwrap();
    let event = json!({"event": "active", "padding": "x".repeat(64)});
    let line_len = event_line_len(&event);
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES",
        line_len.to_string(),
    );
    reset_trace_store_accounting();

    assert!(persist_metadata_event("trace-active", event.clone()).unwrap());
    assert!(!persist_metadata_event("trace-active", event).unwrap());
    let events = fs::read_to_string(temp.path().join("trace-active/events.jsonl")).unwrap();
    assert_eq!(events.lines().count(), 1);
    assert_eq!(accounting_snapshot().1, line_len);
}

#[test]
fn full_mode_cached_trace_losing_owner_marker_is_never_evicted() {
    let temp = tempfile::tempdir().unwrap();
    let event = json!({"event": "ownership", "padding": "x".repeat(64)});
    let line_len = event_line_len(&event);
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES",
        line_len.to_string(),
    );
    reset_trace_store_accounting();

    assert!(persist_metadata_event("trace-owned", event.clone()).unwrap());
    let owned_dir = temp.path().join("trace-owned");
    fs::remove_file(owned_dir.join(TRACE_OWNER_MARKER)).unwrap();

    assert!(!persist_metadata_event("trace-current", event).unwrap());
    assert!(owned_dir.join("events.jsonl").exists());
    assert!(!temp.path().join("trace-current/events.jsonl").exists());
    let accounting = trace_io_state().lock().unwrap();
    assert_eq!(accounting.total_bytes, line_len);
    assert!(!accounting.initialized);
    assert!(!accounting.traces["trace-owned"].evictable);
}

#[test]
fn full_mode_retention_prunes_owned_non_active_cached_trace() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_RETENTION_HOURS", "1");
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();
    assert!(persist_metadata_event("trace-expired", json!({"event": "old"})).unwrap());

    let config = trace_store_config();
    let mut accounting = trace_io_state().lock().unwrap();
    accounting.traces.get_mut("trace-expired").unwrap().modified = SystemTime::UNIX_EPOCH;
    prune_expired_traces(&mut accounting, &config, "trace-active").unwrap();
    assert_eq!(accounting.total_bytes, 0);
    assert!(!accounting.traces.contains_key("trace-expired"));
    drop(accounting);
    assert!(!temp.path().join("trace-expired").exists());
}

#[test]
fn full_mode_fresh_accounting_rebuild_counts_existing_owned_trace() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("trace-existing");
    create_private_trace_dir(&existing).unwrap();
    ensure_trace_owner_marker(&existing).unwrap();
    fs::write(existing.join("events.jsonl"), vec![b'x'; 257]).unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();

    let event = json!({"event": "after-restart"});
    let line_len = event_line_len(&event);
    assert!(persist_metadata_event("trace-new", event).unwrap());
    let (_, cached_total, trace_count, scans) = accounting_snapshot();
    assert_eq!(scans, 1);
    assert_eq!(trace_count, 2);
    assert_eq!(cached_total, 257 + line_len);
    assert_eq!(cached_total, directory_stats(temp.path()).unwrap().0);
}

#[test]
fn full_mode_pruning_never_deletes_unowned_sibling_directories() {
    let temp = tempfile::tempdir().unwrap();
    let unrelated = temp.path().join(Uuid::new_v4().to_string());
    fs::create_dir_all(&unrelated).unwrap();
    fs::write(unrelated.join("keep.bin"), vec![b'x'; 16 * 1024]).unwrap();

    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8192");
    let trace_id = new_trace_id();
    let guard = ToolRequestLifecycle::new(
        "mcp",
        trace_id.clone(),
        "none",
        "tools/call",
        Some("list_tools".into()),
    );
    guard.capture_payload("raw_arguments", &json!({"probe": true}));
    drop(guard);
    flush_full_trace_writer();

    assert!(unrelated.join("keep.bin").exists());
    assert_eq!(payload_files(temp.path(), &trace_id).len(), 1);
}

#[cfg(unix)]
#[test]
fn full_mode_trace_storage_is_private_on_unix() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    let trace_id = new_trace_id();
    let guard = ToolRequestLifecycle::new(
        "mcp",
        trace_id.clone(),
        "none",
        "tools/call",
        Some("write_project_file".into()),
    );
    guard.capture_payload("raw_arguments", &json!({"content": "private"}));
    drop(guard);
    flush_full_trace_writer();

    let trace_dir = temp.path().join(&trace_id);
    let payload_dir = trace_dir.join("payloads");
    let payload = payload_files(temp.path(), &trace_id)
        .into_iter()
        .next()
        .expect("full trace payload");
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&trace_dir), 0o700);
    assert_eq!(mode(&payload_dir), 0o700);
    assert_eq!(mode(&trace_dir.join("events.jsonl")), 0o600);
    assert_eq!(mode(&payload), 0o600);
}

#[tokio::test]
async fn window_correlation_without_tracing_does_not_retain_runner_requests() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "off");
    let guard = ToolRequestLifecycle::new(
        "mcp",
        new_trace_id(),
        "none",
        "tools/call",
        Some("read_files".into()),
    );
    let runtime = crate::tool_runtime::ToolRuntime::new(
        std::sync::Arc::new(crate::RunnerRegistry::default()),
        std::sync::Arc::new(crate::tool_runtime::RuntimeInfo::default()),
    );
    let registry = runtime.window_activity_registry();
    let window = crate::client_window::ClientWindow::for_test("trace-disabled-window");
    let trace_id = guard.correlation_trace_id();
    let _active = registry.start(&window, &trace_id, "tools/call", None);
    assert!(guard.active_trace_id().is_none());
    scope_active_trace(Some(trace_id), async {
        let current = current_active_trace_id().unwrap();
        registry.update(&current, Some("read_files"), Some("agent:r:p"));
        assert!(current_full_trace_ref().is_none());
        record_runner_request_enqueued(
            &json!({}),
            "window-without-tracing",
            "r",
            "read_files",
            None,
            None,
            None,
            None,
            None,
        );
    })
    .await;
    assert_eq!(
        registry.list_for_window(window.key(), None)[0]
            .project
            .as_deref(),
        Some("agent:r:p")
    );
    assert!(!correlations()
        .lock()
        .unwrap()
        .requests
        .contains_key("window-without-tracing"));
}

#[tokio::test]
async fn runner_payload_serialization_is_full_trace_only() {
    let temp = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    let metadata_trace_id = format!("trace-runner-metadata-{}", Uuid::new_v4());
    let metadata_request_id = format!("request-metadata-{}", Uuid::new_v4());
    let metadata_job_id = format!("job-metadata-{}", Uuid::new_v4());

    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "true");
    let metadata_guard = ToolRequestLifecycle::new(
        "mcp",
        metadata_trace_id,
        "none",
        "tools/call",
        Some("run_process".into()),
    );
    scope_active_trace(metadata_guard.active_trace_id(), async {
        record_runner_request_enqueued(
            &json!({"kind": "run_process"}),
            &metadata_request_id,
            "runner-metadata",
            "run_process",
            Some(&metadata_job_id),
            None,
            None,
            None,
            None,
        );
    })
    .await;
    assert!(lookup_request_correlation(&metadata_request_id).is_some());
    assert!(lookup_job_correlation(Some(&metadata_request_id), &metadata_job_id).is_some());

    let metadata_calls = AtomicUsize::new(0);
    let metadata_payload = CountingSerialize {
        calls: &metadata_calls,
        value: json!({"exit_code": 0, "stdout": "metadata"}),
    };
    capture_runner_result(&metadata_request_id, &metadata_payload);
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 0);
    capture_runner_job_update(
        Some(&metadata_request_id),
        &metadata_job_id,
        &metadata_payload,
    );
    assert_eq!(metadata_calls.load(Ordering::SeqCst), 0);
    finalize_runner_job_correlation(Some(&metadata_request_id), &metadata_job_id);
    drop(metadata_guard);

    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "full");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        temp.path().to_string_lossy().as_ref(),
    );
    env.set("WEBCODEX_TOOL_REQUEST_TRACE_MAX_TOTAL_BYTES", "8388608");
    reset_trace_store_accounting();
    let full_trace_id = format!("trace-runner-full-{}", Uuid::new_v4());
    let full_request_id = format!("request-full-{}", Uuid::new_v4());
    let full_job_id = format!("job-full-{}", Uuid::new_v4());
    let full_guard = ToolRequestLifecycle::new(
        "mcp",
        full_trace_id.clone(),
        "none",
        "tools/call",
        Some("run_process".into()),
    );
    scope_active_trace(full_guard.active_trace_id(), async {
        record_runner_request_enqueued(
            &json!({"kind": "run_process"}),
            &full_request_id,
            "runner-full",
            "run_process",
            Some(&full_job_id),
            None,
            None,
            None,
            None,
        );
    })
    .await;

    let full_calls = AtomicUsize::new(0);
    let full_payload = CountingSerialize {
        calls: &full_calls,
        value: json!({"exit_code": 0, "stdout": "full"}),
    };
    capture_runner_result(&full_request_id, &full_payload);
    assert_eq!(full_calls.load(Ordering::SeqCst), 1);
    capture_runner_job_update(Some(&full_request_id), &full_job_id, &full_payload);
    assert_eq!(full_calls.load(Ordering::SeqCst), 2);
    finalize_runner_job_correlation(Some(&full_request_id), &full_job_id);
    drop(full_guard);
    flush_full_trace_writer();

    let events = fs::read_to_string(temp.path().join(&full_trace_id).join("events.jsonl")).unwrap();
    for phase in ["runner_result", "runner_job_update"] {
        let relative = events
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .find(|event| {
                event["event"] == "tool_trace_payload_captured" && event["phase"] == phase
            })
            .and_then(|event| event["payload_path"].as_str().map(str::to_string))
            .unwrap_or_else(|| panic!("missing trace payload phase {phase}"));
        let compressed = fs::read(temp.path().join(&full_trace_id).join(relative)).unwrap();
        let raw = zstd::stream::decode_all(compressed.as_slice()).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&raw).unwrap(),
            json!({"exit_code": 0, "stdout": "full"})
        );
    }
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
    drop(guard);
    flush_full_trace_writer();
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
fn ssh_resource_runner_result_trace_never_persists_result_bodies() {
    let target = "17724@w10";
    let payload = json!({
        "result": {
            "exit_code": 0,
            "stdout": format!("{{\"action\":\"register\",\"target\":\"{target}\"}}"),
            "stderr": target,
            "error": target
        },
        "command_execution_state": "completed"
    });
    let sanitized = runner_result_trace_payload("ssh_resource", payload.clone());
    let serialized = serde_json::to_string(&sanitized).unwrap();
    assert!(!serialized.contains(target));
    assert_eq!(sanitized["kind"], "ssh_resource");
    assert_eq!(sanitized["stdout_present"], true);
    assert_eq!(sanitized["stderr_present"], true);
    assert_eq!(sanitized["error_present"], true);
    assert_eq!(sanitized["exit_code"], 0);

    assert_eq!(
        runner_result_trace_payload("run_process", payload.clone()),
        payload
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

#[test]
fn completion_timing_preserves_subsecond_monotonic_precision() {
    let mut env = crate::test_support::TestEnvGuard::new();
    env.remove("WEBCODEX_TOOL_REQUEST_TRACE");
    let guard = ToolRequestLifecycle::new(
        "mcp",
        "trace-precise".into(),
        "-",
        "tools/call",
        Some("read_files".into()),
    );
    let observed_at_ms = guard.request_observed_at_ms();
    std::thread::sleep(std::time::Duration::from_millis(12));
    let timing = guard.handler_returned(200, None, Some(true), Some(true), "ok");
    assert_eq!(timing.request_observed_at_ms, observed_at_ms);
    assert!(
        timing.elapsed_ms > 0,
        "sub-second work must not quantize to zero"
    );
    assert!(
        timing.elapsed_ms < 1_000,
        "test request unexpectedly exceeded one second"
    );
    assert_eq!(
        timing.response_handed_at_ms - timing.request_observed_at_ms,
        i64::try_from(timing.elapsed_ms).unwrap()
    );
}
