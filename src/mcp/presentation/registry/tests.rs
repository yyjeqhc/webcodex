use super::*;
use serde_json::json;
use std::collections::BTreeSet;

#[test]
fn presentation_registry_has_one_owner_for_each_existing_tool_and_no_fallback() {
    let expected = BTreeSet::from([
        "list_jobs",
        "observe_jobs",
        "cargo_check",
        "cargo_test",
        "go_test",
        "read_validation_summary",
        "read_workspace_changes",
        "read_git_review_summary",
    ]);
    let mut actual = BTreeSet::new();
    for renderer in RENDERERS {
        assert!(!renderer.tools.is_empty());
        for tool in renderer.tools {
            assert!(actual.insert(*tool), "duplicate renderer for {tool}");
            assert!(std::ptr::eq(for_tool(tool).unwrap(), renderer));
        }
    }
    assert_eq!(actual, expected);
    for unknown in [
        "",
        "cargo_test ",
        "CARGO_TEST",
        "get_git_status",
        "run_process",
        "project_validate",
        "plugin_tool",
        "present_work_result",
    ] {
        assert!(
            for_tool(unknown).is_none(),
            "unexpected display admission: {unknown}"
        );
    }
}

#[test]
fn presentation_registry_preserves_canonical_failure_and_unrelated_metadata() {
    let canonical = json!({
        "success": false, "error": "PRIVATE canonical error",
        "output": {"execution_state":"outcome_unknown", "job_id":"job-exact", "passed":false,
            "direct_retry_safe":false, "stdout_tail":"PRIVATE stdout", "permission":{"denied":true}}
    });
    let mut framed = json!({"structuredContent":canonical,"content":[{"type":"text","text":"original"}], "isError":true,
        "_meta":{"keep":{"value":42},"ui":{"resourceUri":"existing"}}});
    let before = framed.clone();
    super::super::attach_result_app_presentation("cargo_test", &mut framed);
    assert_eq!(framed["structuredContent"], before["structuredContent"]);
    assert_eq!(framed["content"], before["content"]);
    assert_eq!(framed["isError"], before["isError"]);
    assert_eq!(framed["_meta"]["keep"], before["_meta"]["keep"]);
    assert_eq!(framed["_meta"]["ui"], before["_meta"]["ui"]);
    let projection = &framed["_meta"][super::super::MCP_PRESENTATION_META_KEY];
    assert_eq!(projection["execution_state"], "outcome_unknown");
    assert_eq!(projection["job_id"], "job-exact");
    assert_eq!(projection["passed"], false);
    assert!(!projection.to_string().contains("PRIVATE"));
    assert!(projection.get("permission").is_none());
    assert!(projection.get("direct_retry_safe").is_none());
}

#[test]
fn presentation_registry_unknown_or_malformed_inputs_are_unchanged() {
    for mut framed in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"structuredContent":null}),
        json!({"structuredContent":{"output":[]}}),
        json!({"structuredContent":{"output":{}},"_meta":"not an object"}),
    ] {
        let before = framed.clone();
        super::super::attach_result_app_presentation("cargo_test", &mut framed);
        assert_eq!(framed, before);
    }
    let mut framed =
        json!({"structuredContent":{"output":{}},"_meta":{"webcodex/presentation":{"keep":true}}});
    let before = framed.clone();
    super::super::attach_result_app_presentation("plugin_tool", &mut framed);
    assert_eq!(framed, before);
}

#[test]
fn presentation_registry_validation_has_shared_item_budget_and_unicode_text_bound() {
    let diagnostic =
        json!({"severity":"error", "message":"界".repeat(300), "code":"E1", "raw":"PRIVATE"});
    let output = json!({"diagnostics":{"available":true,
        "diagnostics":vec![diagnostic;6],
        "failed_test_details":vec![json!({"name":"failed", "failure_kind":"assertion"});6]}});
    let before = output.clone();
    let projection = (for_tool("cargo_check").unwrap().project)("cargo_check", &output).unwrap();
    assert_eq!(output, before);
    let diagnostics = &projection["diagnostics"];
    assert_eq!(diagnostics["items"].as_array().unwrap().len(), 6);
    assert_eq!(diagnostics["failed_tests"].as_array().unwrap().len(), 2);
    assert_eq!(diagnostics["presentation_items_truncated"], true);
    assert_eq!(
        diagnostics["items"][0]["message"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        256
    );
    assert!(!projection.to_string().contains("PRIVATE"));
}

#[test]
fn presentation_registry_summary_keeps_current_truth_and_recent_chronological_subset() {
    let events: Vec<_> = (0..12)
        .map(|index| json!({"tool_name":format!("test-{index}"),"success":true,"stdout":"PRIVATE"}))
        .collect();
    let output = json!({"validation":{"status":"passed","latest_status":"passed","successes":12,
        "current_evidence":{"status":"stale","evidence_after_latest_content_change":false},
        "events":events,"unresolved_failures":{"count":2,"events":["PRIVATE"]}}});
    let projection =
        (for_tool("read_validation_summary").unwrap().project)("read_validation_summary", &output)
            .unwrap();
    let summary = &projection["validation"];
    assert_eq!(summary["current_evidence"]["status"], "stale");
    assert_eq!(
        summary["current_evidence"]["evidence_after_latest_content_change"],
        false
    );
    assert_eq!(summary["unresolved_failures"], json!({"count":2}));
    assert_eq!(summary["events"].as_array().unwrap().len(), 8);
    assert_eq!(summary["events"][0]["tool_name"], "test-4");
    assert_eq!(summary["events"][7]["tool_name"], "test-11");
    assert_eq!(summary["events_truncated"], true);
    assert!(!projection.to_string().contains("PRIVATE"));
}
