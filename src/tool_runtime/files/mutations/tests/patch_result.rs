use super::*;

#[test]
fn apply_patch_matching_mode_capability_rejection_fails_closed() {
    let result = apply_patch_capability_rejection(
        "capability_unavailable: demo lacks apply_patch_matching_mode",
        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH_MATCHING_MODE,
    );

    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["error_kind"], "agent_capability_unavailable");
    assert_eq!(
        result.output["capability"],
        crate::runner_protocol::RUNNER_CAPABILITY_APPLY_PATCH_MATCHING_MODE
    );
    assert_eq!(result.output["recovery_kind"], "retry_same");
}

#[test]
fn apply_patch_success_metadata_is_bound_to_requested_matching_mode() {
    let patch = one_update_patch();
    for (matching_mode, mode, count, unique_match, strict_match) in [
        (
            crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique,
            "exact",
            1,
            true,
            true,
        ),
        (
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
            "trim_end",
            1,
            true,
            false,
        ),
        (
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
            "trim",
            1,
            true,
            false,
        ),
        (
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
            "normalized",
            1,
            true,
            false,
        ),
        (
            crate::apply_patch_shared::ApplyPatchMatchingMode::FirstMatch,
            "exact",
            2,
            false,
            false,
        ),
    ] {
        let payload =
            apply_patch_success_payload(matching_mode, mode, count, unique_match, strict_match)
                .to_string();
        let result = apply_patch_agent_stdout_result(&payload, &patch, true, matching_mode);
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["execution_state"], "completed");
        assert_eq!(result.output["files"][0]["edits"][0]["match_mode"], mode);
        assert_eq!(
            result.output["requested_matching_mode"],
            matching_mode.as_str()
        );
    }
}

#[test]
fn apply_patch_success_metadata_rejects_untrusted_match_summaries_and_sanitizes_them() {
    let patch = one_update_patch();
    let mut cases = Vec::new();

    let mut unique_violation = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        2,
        false,
        false,
    );
    cases.push(unique_violation.take());

    let mut wrong_source = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        1,
        true,
        true,
    );
    wrong_source["files"][0]["edits"][0]["match_source"] = json!("append");
    cases.push(wrong_source.clone());

    let mut wrong_chunk = wrong_source;
    wrong_chunk["files"][0]["edits"][0]["match_source"] = json!("old_lines");
    wrong_chunk["files"][0]["edits"][0]["chunk_index"] = json!(1);
    cases.push(wrong_chunk);

    let mut wrong_path = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        1,
        true,
        true,
    );
    wrong_path["files"][0]["path"] = json!("other.txt");
    cases.push(wrong_path);

    let contradictory_strict = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "trim",
        1,
        true,
        true,
    );
    cases.push(contradictory_strict);

    let wrong_requested_mode = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique,
        "exact",
        1,
        true,
        true,
    );
    cases.push(wrong_requested_mode);

    for payload in cases {
        let result = apply_patch_agent_stdout_result(
            &payload.to_string(),
            &patch,
            true,
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        );
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(result.output["state_changed"].is_null());
        assert!(result.output.get("files").is_none());
        assert!(result.output.get("changed_paths").is_none());
        assert!(!serde_json::to_string(&result.output)
            .unwrap()
            .contains("NEVER_SURVIVE_PATCH_METADATA"));
        assert!(result
            .error
            .as_deref()
            .unwrap()
            .contains("invalid or contradictory patch-plan metadata"));
    }
}

#[test]
fn apply_patch_success_metadata_strips_unknown_fields_without_losing_known_result() {
    let patch = one_update_patch();
    let mut payload = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        1,
        true,
        true,
    );
    payload["future_top_level"] = json!("NEVER_SURVIVE_PATCH_METADATA");
    payload["files"][0]["future_file_field"] = json!("NEVER_SURVIVE_PATCH_METADATA");
    payload["files"][0]["edits"][0]["future_edit_field"] = json!("NEVER_SURVIVE_PATCH_METADATA");

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        true,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_state"], "completed");
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("future_top_level"));
    assert!(!serialized.contains("future_file_field"));
    assert!(!serialized.contains("future_edit_field"));
    assert!(!serialized.contains("NEVER_SURVIVE_PATCH_METADATA"));
}

#[test]
fn apply_patch_success_metadata_accepts_create_delete_rename_and_append_shapes() {
    let patch = crate::apply_patch_shared::parse_codex_patch(
            "*** Begin Patch\n*** Add File: new.txt\n+hello\n*** Delete File: old.txt\n*** Update File: move.txt\n*** Move to: moved.txt\n-old\n+new\n*** Update File: append.txt\n+tail\n*** End Patch",
        )
        .unwrap();
    let payload = json!({
        "dry_run": true,
        "requested_matching_mode": "unique",
        "applied_count": 4,
        "changed": false,
        "state_changed": false,
        "execution_state": "completed",
        "would_change": true,
        "files": [
            {
                "index": 0,
                "kind": "create",
                "path": "new.txt",
                "to_path": null,
                "old_sha256": null,
                "new_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "changed": false,
                "would_change": true,
                "edits": []
            },
            {
                "index": 1,
                "kind": "delete",
                "path": "old.txt",
                "to_path": null,
                "old_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "new_sha256": null,
                "changed": false,
                "would_change": true,
                "edits": []
            },
            {
                "index": 2,
                "kind": "rename",
                "path": "move.txt",
                "to_path": "moved.txt",
                "old_sha256": "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                "new_sha256": "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
                "changed": false,
                "would_change": true,
                "edits": [{
                    "chunk_index": 0,
                    "change_context_present": false,
                    "old_line_count": 1,
                    "new_line_count": 1,
                    "end_of_file": false,
                    "match_mode": "exact",
                    "match_source": "old_lines",
                    "matched_start_line": 1,
                    "candidate_count": 1,
                    "unique_match": true,
                    "strict_match": true
                }]
            },
            {
                "index": 3,
                "kind": "edit",
                "path": "append.txt",
                "to_path": null,
                "old_sha256": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
                "new_sha256": "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                "changed": false,
                "would_change": true,
                "edits": [{
                    "chunk_index": 0,
                    "change_context_present": false,
                    "old_line_count": 0,
                    "new_line_count": 1,
                    "end_of_file": false,
                    "match_mode": null,
                    "match_source": "append",
                    "matched_start_line": 2,
                    "candidate_count": null,
                    "unique_match": true,
                    "strict_match": true
                }]
            }
        ],
        "changed_paths": ["new.txt", "old.txt", "move.txt", "moved.txt", "append.txt"]
    });

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        true,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["files"].as_array().unwrap().len(), 4);
    assert_eq!(result.output["changed_paths"].as_array().unwrap().len(), 5);
}

#[test]
fn apply_patch_missing_current_match_metadata_is_outcome_unknown() {
    let patch = one_update_patch();
    let mut payload = apply_patch_success_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        1,
        true,
        true,
    );
    payload["files"][0]["edits"][0]
        .as_object_mut()
        .expect("current edit object")
        .remove("unique_match");

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        true,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert!(result.output.get("files").is_none());
}
