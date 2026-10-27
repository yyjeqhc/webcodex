use super::*;
use crate::tool_request_trace::{self, ToolRequestLifecycle};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct Log(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for Log {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn audit_lock_wait_extends_handler_finish_preserves_handoff_and_exact_trace_at_info() {
    let root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        root.path().join("trace").to_str().unwrap(),
    );
    let db = Arc::new(crate::Database::open(&root.path().join("state.db")).unwrap());
    let ids = [
        tool_request_trace::new_trace_id(),
        tool_request_trace::new_trace_id(),
    ];
    let log = Log(Arc::new(Mutex::new(Vec::new())));
    let held = db.conn_for_tests();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let threads: Vec<_> = ids
        .iter()
        .map(|id| {
            let (id, db, log, ready, done) = (
                id.clone(),
                db.clone(),
                log.clone(),
                ready_tx.clone(),
                done_tx.clone(),
            );
            std::thread::spawn(move || {
                let _env = crate::test_support::TestToolRequestTraceEnvReaderGuard::new();
                let subscriber = tracing_subscriber::fmt()
                    .with_max_level(tracing::Level::INFO)
                    .with_ansi(false)
                    .with_writer(move || log.clone())
                    .finish();
                tracing::subscriber::with_default(subscriber, || {
                    let guard = ToolRequestLifecycle::new(
                        "mcp",
                        id.clone(),
                        "1",
                        "tools/call",
                        Some("read_files".into()),
                    );
                    let mut depot = Depot::new();
                    depot.inject(db);
                    let audit = ActionAudit::start(&Request::new(), &depot, "/mcp", "toolsCall")
                        .with_window(None, Some(&id));
                    guard.dispatch_finished(true, Some(true), "ok");
                    guard.response_serialized(200, None, Some(true), Some(true), "ok");
                    let handed = guard.handler_returned(200, None, Some(true), Some(true), "ok");
                    ready.send(handed).unwrap();
                    // No active Tokio task-local: audit must carry its own identity.
                    let recorded = audit.record_with_completion(
                        ActionAuditRecord::new("read_files", true, StatusCode::OK),
                        audit.capture_record_timing(),
                        handed,
                        None,
                        false,
                        true,
                    );
                    drop(guard);
                    done.send(recorded).unwrap();
                });
            })
        })
        .collect();
    for _ in 0..2 {
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    }
    // A channel deadline proves the audit cannot finish while the real SQLite
    // mutex is held. It also crosses the fixed slow-log threshold intentionally.
    assert!(matches!(
        done_rx.recv_timeout(Duration::from_millis(150)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    drop(held);
    for _ in 0..2 {
        assert!(done_rx.recv_timeout(Duration::from_secs(5)).unwrap());
    }
    for thread in threads {
        thread.join().unwrap();
    }
    tool_request_trace::flush_full_trace_writer();
    for id in &ids {
        let text = std::fs::read_to_string(root.path().join("trace").join(id).join("events.jsonl"))
            .unwrap();
        let events: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(events.iter().all(|event| event["server_trace_id"] == *id));
        let find = |name: &str| events.iter().find(|event| event["event"] == name).unwrap();
        let handed = find("mcp_tool_handler_returned")["duration_ms"]
            .as_u64()
            .unwrap();
        let finished = find("mcp_tool_handler_finished")["duration_ms"]
            .as_u64()
            .unwrap();
        assert!(finished >= handed + 100);
        let audit = events
            .iter()
            .find(|event| event["phase"] == "audit_finalize")
            .unwrap();
        assert_eq!(audit["outcome"], "completed");
        assert!(audit["elapsed_ms"].as_f64().unwrap() >= 100.0);
        let slow = find("tool_store_connection_slow");
        assert_eq!(slow["domain"], "audit");
        assert!(slow["wait_ms"].as_f64().unwrap() >= 100.0);
    }
    let logs = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
    for id in &ids {
        assert!(logs
            .lines()
            .any(|line| line.contains("store_connection_slow") && line.contains(id)));
    }
    assert!(logs.contains("audit_finalize"));
    assert!(!logs.contains("SELECT") && !logs.contains("INSERT"));
}

#[test]
fn audit_failure_preserves_tool_success_and_records_failed_finalize() {
    let root = tempfile::tempdir().unwrap();
    let mut env = crate::test_support::TestEnvGuard::new();
    env.set("WEBCODEX_TOOL_REQUEST_TRACE", "metadata");
    env.set(
        "WEBCODEX_TOOL_REQUEST_TRACE_DIR",
        root.path().join("trace").to_str().unwrap(),
    );
    let db = Arc::new(crate::Database::open(&root.path().join("state.db")).unwrap());
    db.conn_for_tests().execute_batch("CREATE TRIGGER reject_audit BEFORE INSERT ON action_events BEGIN SELECT RAISE(FAIL, 'fixture rejection'); END;").unwrap();
    let id = tool_request_trace::new_trace_id();
    let guard = ToolRequestLifecycle::new(
        "mcp",
        id.clone(),
        "1",
        "tools/call",
        Some("read_files".into()),
    );
    let mut depot = Depot::new();
    depot.inject(db);
    let audit = ActionAudit::start(&Request::new(), &depot, "/mcp", "toolsCall")
        .with_window(None, Some(&id));
    let handed = guard.handler_returned(200, None, Some(true), Some(true), "ok");
    assert!(!audit.record_with_completion(
        ActionAuditRecord::new("read_files", true, StatusCode::OK),
        audit.capture_record_timing(),
        handed,
        None,
        false,
        true
    ));
    drop(guard);
    tool_request_trace::flush_full_trace_writer();
    let text =
        std::fs::read_to_string(root.path().join("trace").join(id).join("events.jsonl")).unwrap();
    let events: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(events
        .iter()
        .any(|event| event["phase"] == "audit_finalize" && event["outcome"] == "failed"));
    assert!(events.iter().any(
        |event| event["event"] == "mcp_tool_handler_returned" && event["tool_success"] == true
    ));
    assert!(events
        .iter()
        .any(|event| event["event"] == "mcp_tool_handler_finished"));
}
