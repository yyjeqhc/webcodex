use super::closeout::*;
use super::projection::*;
use super::*;
use crate::projects::ProjectConfig;

#[test]
fn finish_actions_prefer_review_changes_when_nested_show_changes_hands_off() {
    let output = json!({
        "workspace": {"clean": false},
        "changes": {
            "show_changes": {
                "diff_review_handoff": {
                    "next_call": {"follow_up_kind": "mechanically_followable", "tool": "git_diff_hunks", "arguments": {}}
                }
            }
        },
        "jobs": {"blocking_active_count": 0},
        "validation": {},
        "tool_failures": {},
    });
    let actions = finish_suggested_next_actions(&output);
    assert!(actions.iter().any(|action| {
        action == "continue the review with review_changes when its continuation is available"
    }));
    assert_eq!(
        output["changes"]["show_changes"]["diff_review_handoff"]["next_call"]["tool"],
        "git_diff_hunks"
    );
    assert!(!actions
        .iter()
        .any(|action| action == "review workspace changes with show_changes"));
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
    let decision = finish_decision_output(&canonical);
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
