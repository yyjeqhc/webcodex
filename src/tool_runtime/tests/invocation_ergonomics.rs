use super::*;
use serde_json::json;

fn record(args: Value, output: Value) -> Value {
    serde_json::to_value(
        ModelErgonomicsTimer::start_with_arguments("work_on_project", &args)
            .unwrap()
            .finish_after(Duration::ZERO)
            .record_for_tool_result(&ToolResult::ok(output))
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn envelope_facts_are_bounded_and_never_copy_private_values() {
    let private = "PRIVATE /secret/path session-id message-id reply resolution revision token";
    let r = record(
        json!({"_wc": {
            "record": private, "ack": [private, private], "ack_ref": private,
            "reply": {"reply_to": private, "message": private},
            "resolve": {"message_id": private, "resolution": private},
            "context": ["project.instructions", "webcodex.workflow", "workflow.resume", "jobs.attention", "skills.catalog", "plugins.catalog", "memory.bootstrap", private],
            "control": {"before": {"goal_progress": {"goal_id": private, "summary": private}}, "after_success": {"todo_completion": {"message_id": private, "answer": private}}}
        }}),
        json!({}),
    );
    let f = &r["invocation"];
    for key in [
        "record_present",
        "ack_present",
        "ack_ref_present",
        "reply_present",
        "resolve_present",
        "context_present",
        "control_present",
    ] {
        assert_eq!(f[key], true, "{key}");
    }
    assert_eq!(f["ack_count"], 2);
    assert_eq!(f["context_requested_count"], 8);
    assert_eq!(f["context_unknown_count"], 1);
    assert!(f["context_known"]
        .as_object()
        .unwrap()
        .values()
        .all(|v| v == true));
    assert_eq!(f["control_before"], "goal_progress");
    assert_eq!(f["control_after_success"], "todo_completion");
    assert!(!r.to_string().contains(private));
    let r = record(
        json!({"_wc": {"ack": vec![private; 2048], "context": vec![private; 2048], "control": {"before": {private: private}}}}),
        json!({}),
    );
    assert_eq!(r["invocation"]["ack_count"], 1024);
    assert_eq!(r["invocation"]["context_requested_count"], 1024);
    assert_eq!(r["invocation"]["context_unknown_count"], 1024);
    assert!(r["invocation"]["control_before"].is_null());
    assert!(!r.to_string().contains(private));
}

#[test]
fn empty_envelope_inputs_remain_distinct_from_omission() {
    let omitted = record(json!({}), json!({}));
    let explicit = record(json!({"_wc": {"ack": [], "context": []}}), json!({}));
    for field in ["ack_present", "context_present"] {
        assert_eq!(omitted["invocation"][field], false);
        assert_eq!(explicit["invocation"][field], true);
    }
    assert!(omitted["bootstrap"]["instructions_available"].is_null());
    assert!(omitted["bootstrap"]["semantic_available"].is_null());
}

#[test]
fn all_control_kinds_are_closed_and_payload_free() {
    for (phase, field, kinds) in [
        (
            "before",
            "control_before",
            vec![
                "goal_progress",
                "wake_consume",
                "attempt_heartbeat",
                "session_context_update",
            ],
        ),
        (
            "after_success",
            "control_after_success",
            vec!["goal_completion", "session_close", "todo_completion"],
        ),
    ] {
        for kind in kinds {
            let r = record(
                json!({"_wc": {"control": {phase: {kind: {"secret": "PRIVATE"}}}}}),
                json!({}),
            );
            assert_eq!(r["invocation"][field], kind);
            assert!(!r.to_string().contains("PRIVATE"));
        }
    }
}

#[test]
fn bootstrap_uses_final_material_and_closed_observation_facts() {
    let r = record(
        json!({"_wc": {"context": ["project.instructions", "webcodex.workflow"]}}),
        json!({
            "instructions": {"status": "loaded", "content_included": false, "sources": [{"path": "/PRIVATE"}]},
            "workspace": {"git": {"status": "conflicted"}, "branch": "PRIVATE", "head": "PRIVATE"},
            "semantic_navigation": {"supported": true, "available": null, "status": "probe_timeout", "provider": "PRIVATE"},
            "extensions": {"skills": {"status": "available", "returned_count": 3, "truncated": false}, "plugins": {"status": "unavailable", "returned_count": 0, "truncated": false}},
            "context_projection": {"materials": [
                {"key": "project.instructions", "status": "available", "projection": {"content_included": true, "truncated": false, "sources": [{"content": "PRIVATE", "fingerprint": "PRIVATE"}]}},
                {"key": "webcodex.workflow", "status": "available", "projection": {"text": "PRIVATE"}}
            ]}
        }),
    );
    let f = &r["bootstrap"];
    for field in [
        "instructions_available",
        "instructions_content_included",
        "workflow_available",
        "skills_available",
        "semantic_supported",
    ] {
        assert_eq!(f[field], true, "{field}");
    }
    assert_eq!(f["instructions_truncated"], false);
    assert_eq!(f["instruction_source_count"], 1);
    assert_eq!(f["instruction_observation_status"], "loaded");
    assert_eq!(f["workspace_status"], "conflicted");
    assert_eq!(f["semantic_status"], "probe_timeout");
    assert!(f["semantic_available"].is_null());
    assert_eq!(f["skills_returned_count"], 3);
    assert_eq!(f["skills_truncated"], false);
    assert_eq!(f["plugins_available"], false);
    assert!(!r.to_string().contains("PRIVATE"));
    let r = record(
        json!({}),
        json!({"context_projection": {"materials": [{"key": "project.instructions", "status": "unavailable", "projection": {"content_included": true, "truncated": true}}]}}),
    );
    assert_eq!(r["bootstrap"]["instructions_available"], false);
    assert_eq!(r["bootstrap"]["instructions_truncated"], true);
}

#[test]
fn instruction_read_projects_basename_booleans_only() {
    let r = ModelErgonomicsTimer::start_with_arguments(
        "read_files",
        &json!({"items": [{"path": "/PRIVATE/AGENTS.md"}, {"path": "PRIVATE/CLAUDE.md"}]}),
    )
    .unwrap()
    .finish_after(Duration::ZERO)
    .record_for_tool_result(&ToolResult::ok(json!({})))
    .unwrap();
    let r = serde_json::to_value(r).unwrap();
    assert_eq!(
        r["instruction_read"],
        json!({"agents_md": true, "claude_md": true})
    );
    assert!(!r.to_string().contains("PRIVATE"));
}
