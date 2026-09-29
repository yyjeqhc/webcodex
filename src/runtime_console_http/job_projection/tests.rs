use super::*;
use serde_json::{json, Value};

fn job() -> ShellJobInfo {
    serde_json::from_value(json!({
        "job_id": "job-a", "client_id": "runner-a", "kind": "shell",
        "status": "running", "created_at": 42, "started_at": 43,
        "command_preview": "PRIVATE_COMMAND", "cwd": "/private/path",
        "error": "PRIVATE_ERROR", "request_id": "PRIVATE_REQUEST",
        "activity": {"state":"waiting", "phase":"cargo_waiting_for_build_lock", "source":"cargo_output"}
    })).unwrap()
}

#[test]
fn session_job_projection_is_allowlisted_and_keeps_typed_activity_wire_values() {
    let value = serde_json::to_value(session_job(&job()).unwrap()).unwrap();
    assert_eq!(
        value,
        json!({
            "job_id":"job-a", "kind":"shell", "status":"running", "terminal":false,
            "created_at":42, "started_at":43,
            "activity_state":"waiting", "activity_phase":"cargo_waiting_for_build_lock"
        })
    );
    assert!(!value.to_string().contains("PRIVATE"));
    assert!(!value.to_string().contains("/private"));
    let mut absent = job();
    absent.activity = None;
    let absent = serde_json::to_value(session_job(&absent).unwrap()).unwrap();
    assert!(absent.get("activity_state").is_none());
    assert!(absent.get("activity_phase").is_none());
}

#[test]
fn session_job_projection_preserves_bounds_omissions_and_unknown_lifecycle() {
    let mut source = job();
    source.kind = "  shell  ".to_string();
    source.job_id = "x".repeat(200);
    for (status, terminal) in [
        ("running", false),
        ("queued", false),
        ("completed", true),
        ("failed", true),
        ("stopped", true),
        ("unknown-future", false),
    ] {
        source.status = status.to_string();
        let projected = session_job(&source).unwrap();
        assert_eq!(projected.terminal, terminal, "{status}");
        assert_eq!(projected.job_id.len(), 160);
        assert_eq!(projected.kind, "shell");
        let value: Value = serde_json::to_value(projected).unwrap();
        assert!(value.get("ended_at").is_none());
    }
    source.kind = " \n ".to_string();
    assert!(session_job(&source).is_none());
}
