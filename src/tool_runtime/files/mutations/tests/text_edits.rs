use super::*;

#[test]
fn apply_text_edits_success_sanitizes_duplicate_anchor_advisory_text() {
    let change = ApplyFileChangeInput {
        kind: ApplyFileChangeKind::Edit,
        path: "file.txt".to_string(),
        to_path: None,
        content: None,
        edits: vec![ApplyTextEditInput {
            kind: ApplyTextEditKind::InsertBefore,
            old_text: None,
            new_text: Some("anchor".to_string()),
            anchor_text: Some("anchor".to_string()),
            occurrence: None,
            expected_match_count: None,
            line_scope: None,
        }],
        expected_read_revision: None,
    };
    let payload = |warning: &str, kind: &str| {
        json!({
            "dry_run": true,
            "applied_count": 1,
            "changed": false,
            "would_change": true,
            "files": [{
                "index": 0,
                "kind": "edit",
                "path": "file.txt",
                "to_path": null,
                "old_sha256": "a".repeat(64),
                "new_sha256": "b".repeat(64),
                "changed": false,
                "would_change": true,
                "edits": [{
                    "index": 0,
                    "kind": kind,
                    "old_start_line": 1,
                    "old_end_line": 1,
                    "new_line_count": 1,
                    "warning": warning,
                }]
            }],
            "changed_paths": []
        })
    };
    let canonical = crate::apply_edits_shared::APPLY_TEXT_EDIT_DUPLICATE_ANCHOR_WARNING;
    let result = apply_text_edits_agent_stdout_result(
        &payload(canonical, "insert_before").to_string(),
        1,
        true,
        "agent:test:demo",
        std::slice::from_ref(&change),
    );
    assert!(result.success);
    assert_eq!(result.output["files"][0]["edits"][0]["warning"], canonical);

    for payload in [
        payload("PRIVATE_RUNNER_TEXT", "insert_before"),
        payload(canonical, "replace_exact"),
    ] {
        let result = apply_text_edits_agent_stdout_result(
            &payload.to_string(),
            1,
            true,
            "agent:test:demo",
            std::slice::from_ref(&change),
        );
        assert!(result.success);
        assert!(result.output["files"][0]["edits"][0]
            .get("warning")
            .is_none());
        assert!(!result.output.to_string().contains("PRIVATE_RUNNER_TEXT"));
    }
}

#[test]
fn apply_text_edits_success_metadata_rejects_invalid_file_authority() {
    let change = ApplyFileChangeInput {
        kind: ApplyFileChangeKind::Edit,
        path: "file.txt".to_string(),
        to_path: None,
        content: None,
        edits: Vec::new(),
        expected_read_revision: None,
    };
    let valid_payload = || {
        json!({
            "dry_run": false,
            "applied_count": 1,
            "changed": true,
            "would_change": true,
            "files": [{
                "index": 0,
                "kind": "edit",
                "path": "file.txt",
                "to_path": null,
                "old_sha256": "a".repeat(64),
                "new_sha256": "b".repeat(64),
                "changed": true,
                "would_change": true,
                "edits": []
            }],
            "changed_paths": ["file.txt"]
        })
    };

    let mut invalid = Vec::new();
    let mut wrong_index = valid_payload();
    wrong_index["files"][0]["index"] = json!(1);
    invalid.push(wrong_index);
    let mut wrong_path = valid_payload();
    wrong_path["files"][0]["path"] = json!("retargeted.txt");
    invalid.push(wrong_path);
    let mut wrong_to_path = valid_payload();
    wrong_to_path["files"][0]["to_path"] = json!("retargeted.txt");
    invalid.push(wrong_to_path);
    let mut wrong_kind = valid_payload();
    wrong_kind["files"][0]["kind"] = json!("rename");
    invalid.push(wrong_kind);
    let mut missing_files = valid_payload();
    missing_files.as_object_mut().unwrap().remove("files");
    invalid.push(missing_files);
    let mut duplicate_file = valid_payload();
    let duplicate = duplicate_file["files"][0].clone();
    duplicate_file["files"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    invalid.push(duplicate_file);
    let mut malformed_new_sha = valid_payload();
    malformed_new_sha["files"][0]["new_sha256"] = json!("ABC");
    invalid.push(malformed_new_sha);
    let mut contradictory_changed = valid_payload();
    contradictory_changed["files"][0]["changed"] = json!(false);
    invalid.push(contradictory_changed);
    let mut contradictory_would_change = valid_payload();
    contradictory_would_change["files"][0]["would_change"] = json!(false);
    invalid.push(contradictory_would_change);

    for payload in invalid {
        let result = apply_text_edits_agent_stdout_result(
            &payload.to_string(),
            1,
            false,
            "agent:test:demo",
            std::slice::from_ref(&change),
        );
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(result.output["state_changed"].is_null());
        assert!(result.output.get("files").is_none());
        assert!(result.output.get("read_revision").is_none());
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .contains("invalid or contradictory file-result metadata"));
    }

    let create = ApplyFileChangeInput {
        kind: ApplyFileChangeKind::Create,
        path: "created.txt".to_string(),
        to_path: None,
        content: Some("created".to_string()),
        edits: Vec::new(),
        expected_read_revision: None,
    };
    let create_with_old_sha = json!({
        "dry_run": false, "applied_count": 1, "changed": true, "would_change": true,
        "files": [{"index":0,"kind":"create","path":"created.txt","to_path":null,"old_sha256":"a".repeat(64),"new_sha256":"b".repeat(64),"changed":true,"would_change":true,"edits":[]}],
        "changed_paths": ["created.txt"]
    });
    let create_noop = json!({
        "dry_run": false, "applied_count": 1, "changed": false, "would_change": false,
        "files": [{"index":0,"kind":"create","path":"created.txt","to_path":null,"old_sha256":null,"new_sha256":"b".repeat(64),"changed":false,"would_change":false,"edits":[]}],
        "changed_paths": []
    });
    let create_noop_result = apply_text_edits_agent_stdout_result(
        &create_noop.to_string(),
        1,
        false,
        "agent:test:demo",
        std::slice::from_ref(&create),
    );
    assert!(!create_noop_result.success);
    assert_eq!(
        create_noop_result.output["execution_state"],
        "outcome_unknown"
    );

    let create_result = apply_text_edits_agent_stdout_result(
        &create_with_old_sha.to_string(),
        1,
        false,
        "agent:test:demo",
        &[create],
    );
    assert!(!create_result.success);
    assert_eq!(create_result.output["execution_state"], "outcome_unknown");

    let delete = ApplyFileChangeInput {
        kind: ApplyFileChangeKind::Delete,
        path: "deleted.txt".to_string(),
        to_path: None,
        content: None,
        edits: Vec::new(),
        expected_read_revision: Some(1),
    };
    let delete_with_new_sha = json!({
        "dry_run": false, "applied_count": 1, "changed": true, "would_change": true,
        "files": [{"index":0,"kind":"delete","path":"deleted.txt","to_path":null,"old_sha256":"a".repeat(64),"new_sha256":"b".repeat(64),"changed":true,"would_change":true,"edits":[]}],
        "changed_paths": ["deleted.txt"]
    });
    let delete_result = apply_text_edits_agent_stdout_result(
        &delete_with_new_sha.to_string(),
        1,
        false,
        "agent:test:demo",
        &[delete],
    );
    assert!(!delete_result.success);
    assert_eq!(delete_result.output["execution_state"], "outcome_unknown");
}

#[test]
fn apply_text_edits_path_policy_recovery_omits_untrusted_path_metadata() {
    let result = compact_apply_text_edits_path_policy_rejection(
        2,
        "edit",
        "/private/secret.txt",
        "path must be project-relative".to_string(),
    );

    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "policy_rejected");
    assert_eq!(result.output["change_index"], 2);
    assert_eq!(result.output["kind"], "edit");
    assert!(result.output.get("path").is_none());
    assert!(result.output.get("error").is_none());
    assert!(!serde_json::to_string(&result.output)
        .unwrap()
        .contains("/private/secret.txt"));
}

#[test]
fn incomplete_apply_text_edits_rollback_is_not_reported_as_no_write() {
    let result = apply_text_edits_agent_stdout_result(
        r#"{"changed":true,"state_changed":false,"rollback_complete":false,"retry_guidance":"retry directly","conflict_recovery":{"schema_version":1,"conflict_kind":"multiple_matches","occurrence_selector_supported":true,"direct_retry_safe":true,"reread_required":false,"recovery_action":"select_occurrence_or_refine_match"},"error":"rollback failed"}"#,
        1,
        false,
        "agent:test:demo",
        &[],
    );

    assert!(!result.success);
    assert_eq!(result.output["rollback_complete"], false);
    assert!(result.output.get("conflict_recovery").is_none());
    assert!(result.output.get("retry_guidance").is_none());
    assert!(result.output.get("error").is_none());
    assert!(result.output["state_changed"].is_null());
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    let error = result.error.unwrap();
    assert!(error.contains("outcome is unknown"));
    assert!(!error.contains("No files were modified"));
}

#[test]
fn bulk_success_metadata_rejects_unbounded_or_unexpected_ranges() {
    let change = ApplyFileChangeInput {
        kind: ApplyFileChangeKind::Edit,
        path: "bulk.txt".to_string(),
        to_path: None,
        content: None,
        edits: vec![ApplyTextEditInput {
            kind: ApplyTextEditKind::ReplaceExact,
            old_text: Some("OLD".to_string()),
            new_text: Some("NEW".to_string()),
            anchor_text: None,
            occurrence: None,
            expected_match_count: Some(2),
            line_scope: None,
        }],
        expected_read_revision: Some(1),
    };
    let mut payload = json!({
        "dry_run":true,"applied_count":0,"planned_count":1,"changed":false,"would_change":true,
        "files":[{"index":0,"kind":"edit","path":"bulk.txt","to_path":null,
            "old_sha256":"a".repeat(64),"new_sha256":"b".repeat(64),"changed":false,"would_change":true,
            "edits":[{"index":0,"kind":"replace_exact","old_start_line":1,"old_end_line":1,
                "new_line_count":1,"would_change":true,"match_count":2,"expected_match_count":2,
                "match_ranges":[{"occurrence":1,"start_line":1,"end_line":1},{"occurrence":2,"start_line":2,"end_line":2}],
                "match_ranges_truncated":false}]}],
        "changed_paths":["bulk.txt"],
        "change_summary":{"requested_changes":1,"changed_files":0,"logical_edits":1,"resolved_matches":2,"warnings":0}
    });
    let evaluate = |value: &Value| {
        apply_text_edits_agent_stdout_result(
            &value.to_string(),
            1,
            true,
            "project",
            &[change.clone()],
        )
    };
    assert!(evaluate(&payload).success);
    payload["files"][0]["edits"][0]["match_ranges"][0]["old_text"] = json!("SECRET");
    let rejected = evaluate(&payload);
    assert!(!rejected.success);
    assert_eq!(rejected.output["execution_state"], "outcome_unknown");
    assert!(!serde_json::to_string(&rejected).unwrap().contains("SECRET"));
    payload["files"][0]["edits"] = json!([]);
    payload["change_summary"]["logical_edits"] = json!(0);
    payload["change_summary"]["resolved_matches"] = json!(0);
    assert!(!evaluate(&payload).success);
}

#[test]
fn bulk_count_mismatch_projects_bounded_request_bound_evidence() {
    let change = ApplyFileChangeInput {
        kind: ApplyFileChangeKind::Edit,
        path: "bulk.txt".to_string(),
        to_path: None,
        content: None,
        edits: vec![ApplyTextEditInput {
            kind: ApplyTextEditKind::ReplaceExact,
            old_text: Some("PRIVATE_OLD".to_string()),
            new_text: Some("PRIVATE_NEW".to_string()),
            anchor_text: None,
            occurrence: None,
            expected_match_count: Some(2),
            line_scope: Some(crate::apply_edits_shared::ApplyTextLineScope {
                start_line: 3,
                end_line: 4,
            }),
        }],
        expected_read_revision: Some(1),
    };
    let failure = json!({
        "changed":false,"state_changed":false,"execution_state":"not_started",
        "error_kind":"edit_conflict","change_index":0,"edit_index":0,
        "kind":"replace_exact","path":"bulk.txt","error":"No files were modified",
        "conflict_recovery":{"conflict_kind":"match_count_mismatch",
            "expected_match_count":99,"actual_match_count":0,
            "line_scope":{"start_line":1,"end_line":99},
            "candidate_ranges":[],"candidates_truncated":false}
    });
    let result =
        apply_text_edits_agent_stdout_result(&failure.to_string(), 1, false, "project", &[change]);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "match_count_mismatch");
    assert_eq!(result.output["expected_match_count"], 2);
    assert_eq!(result.output["actual_match_count"], 0);
    assert_eq!(
        result.output["line_scope"],
        json!({"start_line":3,"end_line":4})
    );
    assert_eq!(result.output["candidate_ranges"], json!([]));
    assert_eq!(result.output["direct_retry_safe"], false);
    assert_eq!(result.output["reread_required"], true);
    assert_eq!(result.output["execution_state"], "not_started");
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("PRIVATE_OLD"));
}
