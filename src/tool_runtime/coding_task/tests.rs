use super::closeout::*;
use super::projection::*;
use super::*;
use crate::projects::ProjectConfig;
use crate::tool_runtime::closeout_facts::CloseoutFacts;
use crate::tool_runtime::closeout_projection::{
    closeout_facts, hygiene_observation, workspace_observation,
};

fn fixture_closeout_facts(output: &Value) -> CloseoutFacts {
    let workspace = output.get("workspace").unwrap_or(&Value::Null);
    let hygiene = output.get("hygiene").unwrap_or(&Value::Null);
    closeout_facts(
        workspace_observation(workspace, true, true),
        Some(hygiene_observation(hygiene, !hygiene.is_null(), true)),
        output.get("jobs").unwrap_or(&Value::Null),
        output.get("validation").unwrap_or(&Value::Null),
        output.get("tool_failures").unwrap_or(&Value::Null),
        output.get("review_evidence").unwrap_or(&Value::Null),
    )
}

fn finish_from_fixture(output: &Value) -> Value {
    let facts = fixture_closeout_facts(output);
    let outputs = output.get("task_outputs").and_then(|value| {
        serde_json::from_value::<webcodex_core::task_outputs::TaskOutputs>(value.clone()).ok()
    });
    let actions = output
        .get("suggested_next_actions")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    finish_decision_output(&facts, output, &actions, outputs.as_ref()).output
}

#[test]
fn finish_actions_prefer_review_changes_when_nested_show_changes_hands_off() {
    let output = json!({
        "workspace": {"clean": false},
        "changes": {
            "show_changes": {
                "diff_review_handoff": {
                    "next_call": {"follow_up_kind": "mechanically_followable", "tool": "read_git_diff_hunks", "arguments": {}}
                }
            }
        },
        "jobs": {"blocking_active_count": 0},
        "validation": {},
        "tool_failures": {},
    });
    let facts = fixture_closeout_facts(&output);
    let actions = finish_suggested_next_actions(
        &facts,
        output
            .pointer("/changes/show_changes/diff_review_handoff/next_call/tool")
            .and_then(Value::as_str)
            == Some("read_git_diff_hunks"),
    );
    assert!(actions.iter().any(|action| {
        action == "continue the review with review_changes when its continuation is available"
    }));
    assert_eq!(
        output["changes"]["show_changes"]["diff_review_handoff"]["next_call"]["tool"],
        "read_git_diff_hunks"
    );
    assert!(!actions
        .iter()
        .any(|action| action == "review workspace changes with read_workspace_changes"));
}

#[test]
fn finish_summary_keeps_non_git_cleanliness_not_applicable() {
    let canonical = json!({
        "workspace": {
            "clean": null,
            "git_available": false,
            "non_git_project": true,
            "counts": {},
        },
        "jobs": {},
        "validation": {},
        "review_evidence": {},
        "tool_failures": {},
        "final_warnings": [],
        "suggested_next_actions": [],
    });
    let decision = finish_from_fixture(&canonical);
    assert!(
        decision["workspace_clean"].is_null(),
        "non-Git cleanliness must stay unknown/N/A: {decision}"
    );
    let warnings = decision["warnings"].as_array().expect("warnings");
    assert!(
        !warnings
            .iter()
            .any(|warning| warning.as_str() == Some("workspace_dirty")),
        "non-Git workspace must not become dirty: {decision}"
    );
    let compact = compact_finish_output(&decision);
    assert!(
        compact["workspace_clean"].is_null(),
        "summary_only must preserve non-Git N/A: {compact}"
    );
}

#[test]
fn output_guidance_preserves_unproven_validation_and_rejects_invalid_receipts() {
    let mut canonical = json!({
        "workspace": {"clean": null, "git_available": false, "counts": {}},
        "jobs": {}, "review_evidence": {}, "tool_failures": {},
        "final_warnings": [], "suggested_next_actions": [],
        "validation": {
            "status": "passed", "latest_status": "passed",
            "successes": 1, "failures": 0,
            "current_evidence": {
                "status": "unproven", "reason": "validation_source_unproven",
                "events_total": 1, "successes": 0, "failures": 0,
                "unresolved_failure_count": 0, "evidence_gap_event_count": 0
            }
        }
    });
    let original = finish_from_fixture(&canonical);
    assert_eq!(original["task_outcome"]["status"], "warn");
    assert!(original["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(
            |action| action.as_str() == Some(super::super::handoff::UNPROVEN_SOURCE_REVIEW_ACTION)
        ));
    canonical["task_outputs"] = json!({
        "items": [{"path": "report.csv", "status": "verified", "file_bytes": 42,
            "sha256": "a".repeat(64), "mime_type": "text/csv"}],
        "verified_count": 1, "missing_count": 0, "unavailable_count": 0, "observed_at": 1
    });
    let adapted = finish_from_fixture(&canonical);
    assert_eq!(adapted["task_outcome"], original["task_outcome"]);
    assert_eq!(adapted["validation"], original["validation"]);
    let actions = adapted["suggested_next_actions"].as_array().unwrap();
    assert_eq!(
        actions.len(),
        original["suggested_next_actions"].as_array().unwrap().len()
    );
    assert!(actions
        .iter()
        .any(|action| action
            .as_str()
            .is_some_and(|text| text.contains("input/output stability")
                && text.contains("not content/counts"))));
    assert!(!actions.iter().any(
        |action| action.as_str() == Some(super::super::handoff::UNPROVEN_SOURCE_REVIEW_ACTION)
    ));
    canonical["task_outputs"]["verified_count"] = json!(2);
    let invalid = finish_from_fixture(&canonical);
    assert_eq!(
        invalid["suggested_next_actions"],
        original["suggested_next_actions"]
    );
    assert_eq!(invalid["task_outcome"], original["task_outcome"]);
}

fn resolved_agent(client_id: &str) -> ResolvedProject {
    ResolvedProject {
        input: "demo".to_string(),
        resolved_id: format!("agent:{client_id}:demo"),
        config: ProjectConfig {
            path: "/tmp/demo".to_string(),
            client_id: client_id.to_string(),
            allow_patch: true,
        },
        root_fingerprint: None,
        knowledge_association: None,
    }
}

#[test]
fn missing_target_runner_is_unavailable_even_when_a_peer_is_online() {
    let runtime_status = json!({
        "runners": {
            "clients": [{"client_id": "peer", "status": "online"}]
        }
    });
    assert_eq!(
        owning_runner_available(&resolved_agent("target"), &runtime_status, false),
        Some(false)
    );
}

#[test]
fn target_runner_online_is_available_even_when_a_peer_is_stale() {
    let runtime_status = json!({
        "runners": {
            "clients": [
                {"client_id": "peer", "status": "stale"},
                {"client_id": "target", "status": "online"}
            ]
        }
    });
    assert_eq!(
        owning_runner_available(&resolved_agent("target"), &runtime_status, false),
        Some(true)
    );
}
#[test]
fn runner_health_failure_stays_unknown_and_peer_does_not_mask_offline_target() {
    let status = json!({"runners":{"clients":[
        {"client_id":"target","status":"stale"},
        {"client_id":"peer","status":"online"}
    ]}});
    assert_eq!(
        owning_runner_available(&resolved_agent("target"), &status, false),
        Some(false)
    );
    assert_eq!(
        owning_runner_available(&resolved_agent("target"), &status, true),
        None
    );
    assert_eq!(
        startup_agent_check(&json!({}), None),
        ("warn", Some("agent_health_unknown"))
    );
}

#[test]
fn finish_decision_uses_typed_facts_not_serialized_workspace_or_receipts() {
    let captured = json!({"workspace":{"clean":false,"counts":{"conflicted":1}},"validation":{"status":"passed"}});
    let facts = fixture_closeout_facts(&captured);
    let forged_display = json!({"workspace":{"clean":true,"counts":{"conflicted":0}},
        "validation":{"status":"passed"}, "task_outputs":{"missing_count":99}});
    let result = finish_decision_output(&facts, &forged_display, &[], None);
    assert!(result.blocking);
    assert_eq!(result.output["workspace_conflicts"], 1);
    assert_eq!(result.output["task_outcome"]["blocking"], true);
    assert!(result.output.get("task_outputs").is_none());
}

#[test]
fn typed_missing_output_blocks_the_seal_and_the_public_projection_together() {
    let canonical = json!({"workspace":{"clean":true,"counts":{"conflicted":0}},"validation":{"status":"passed"}});
    let facts = fixture_closeout_facts(&canonical);
    let outputs: webcodex_core::task_outputs::TaskOutputs = serde_json::from_value(json!({
        "items":[{"path":"report.csv","status":"missing"}],"verified_count":0,
        "missing_count":1,"unavailable_count":0,"observed_at":1
    }))
    .unwrap();
    assert!(outputs.valid());
    let result = finish_decision_output(&facts, &canonical, &[], Some(&outputs));
    assert!(result.blocking);
    assert_eq!(result.output["task_outcome"]["blocking"], true);
    assert!(result.output["hard_blockers"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "task_outputs_unverified"));
}
