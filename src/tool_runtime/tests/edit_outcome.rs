use super::*;
use serde_json::json;

fn receipt(changed: bool) -> Value {
    json!({"dry_run":false,"applied_count":2,"planned_count":2,
        "changed":changed,"would_change":changed,"state_changed":changed,
        "execution_state":"completed"})
}

#[test]
fn confirmed_actual_edits_distinguish_change_noop_and_invalid_receipts() {
    for changed in [false, true] {
        let output = receipt(changed);
        let facts = EditOutcomeFacts::from_output(&output);
        assert_eq!(
            facts.confirmed_runner_change("edit_project_files", 2, false),
            Some(changed)
        );
        assert!(can_compact_edit_success(&ToolResult::ok(output), 2, false));
    }
    for output in [Value::Null, json!([]), json!({})] {
        assert_eq!(
            EditOutcomeFacts::from_output(&output).confirmed_runner_change(
                "edit_project_files",
                2,
                false
            ),
            None
        );
        assert!(!can_compact_edit_success(&ToolResult::ok(output), 2, false));
    }
}

#[test]
fn partial_and_malformed_effect_fields_never_prove_completion() {
    for key in ["dry_run", "applied_count", "changed", "would_change"] {
        for value in [
            None,
            Some(Value::Null),
            Some(json!("true")),
            Some(json!([])),
        ] {
            let mut output = receipt(true);
            match value {
                Some(value) => {
                    output[key] = value;
                }
                None => {
                    output.as_object_mut().unwrap().remove(key);
                }
            }
            assert_eq!(
                EditOutcomeFacts::from_output(&output).confirmed_runner_change(
                    "edit_project_files",
                    2,
                    false
                ),
                None,
                "{key}: {output}"
            );
            assert!(!can_compact_edit_success(&ToolResult::ok(output), 2, false));
        }
    }
    for (key, bad) in [
        ("applied_count", json!(1)),
        ("applied_count", json!(-1)),
        ("planned_count", json!(1)),
        ("planned_count", Value::Null),
        ("planned_count", json!("2")),
        ("would_change", json!(false)),
    ] {
        let mut output = receipt(true);
        output[key] = bad;
        assert_eq!(
            EditOutcomeFacts::from_output(&output).confirmed_runner_change(
                "edit_project_files",
                2,
                false
            ),
            None,
            "{output}"
        );
    }
    let mut older = receipt(true);
    older.as_object_mut().unwrap().remove("planned_count");
    assert!(can_compact_edit_success(&ToolResult::ok(older), 2, false));
}

#[test]
fn both_established_text_edit_dry_run_receipts_are_supported_but_never_compacted() {
    for output in [
        json!({"dry_run":true,"applied_count":2,"changed":false,"would_change":true}),
        json!({"dry_run":true,"applied_count":0,"planned_count":2,"changed":false,"would_change":true}),
    ] {
        assert_eq!(
            EditOutcomeFacts::from_output(&output).confirmed_runner_change(
                "edit_project_files",
                2,
                true
            ),
            Some(false)
        );
        for requested in [false, true] {
            assert!(!can_compact_edit_success(
                &ToolResult::ok(output.clone()),
                2,
                requested
            ));
        }
    }
    let contradictory = json!({"dry_run":true,"applied_count":0,"planned_count":2,"changed":true,"would_change":true});
    assert_eq!(
        EditOutcomeFacts::from_output(&contradictory).confirmed_runner_change(
            "edit_project_files",
            2,
            true
        ),
        None
    );
}

#[test]
fn patch_receipts_do_not_inherit_text_edit_planning_conventions() {
    let output = json!({"dry_run":true,"applied_count":2,"planned_count":99,"changed":false,"would_change":true});
    let facts = EditOutcomeFacts::from_output(&output);
    assert_eq!(
        facts.confirmed_runner_change("apply_patch", 2, true),
        Some(false)
    );
    assert_eq!(
        facts.confirmed_runner_change("edit_project_files", 2, true),
        None
    );
}

#[test]
fn rollback_and_effect_uncertainty_take_precedence_over_no_effect_markers() {
    for output in [
        json!({}),
        json!({"rollback_complete":false,"changed":false}),
        json!({"rollback_complete":true,"changed":true}),
        json!({"changed":false,"state_changed":true}),
        json!({"changed":"false","state_changed":null}),
    ] {
        assert!(
            !EditOutcomeFacts::from_output(&output).proves_no_effect_failure(),
            "{output}"
        );
    }
    for output in [
        json!({"rollback_complete":true}),
        json!({"changed":false}),
        json!({"state_changed":false}),
    ] {
        assert!(
            EditOutcomeFacts::from_output(&output).proves_no_effect_failure(),
            "{output}"
        );
    }
}

#[test]
fn projection_requires_its_own_canonical_lifecycle_and_effect_proof() {
    for (key, value) in [
        ("execution_state", json!("outcome_unknown")),
        ("execution_state", json!("pending")),
        ("execution_state", Value::Null),
        ("state_changed", json!(false)),
        ("state_changed", Value::Null),
    ] {
        let mut output = receipt(true);
        output[key] = value;
        assert!(!can_compact_edit_success(&ToolResult::ok(output), 2, false));
    }
    assert!(!can_compact_edit_success(
        &ToolResult::err_with_output("failure", receipt(true)),
        2,
        false
    ));
    assert!(!can_compact_edit_success(
        &ToolResult::ok(receipt(true)),
        2,
        true
    ));
    assert!(!can_compact_edit_success(
        &ToolResult::ok(receipt(true)),
        1,
        false
    ));
}
