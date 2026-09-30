use super::*;

#[test]
fn apply_patch_failure_match_diagnostic_is_validated_before_projection() {
    let patch = one_update_patch();
    let valid = json!({
        "changed": false,
        "state_changed": false,
        "execution_state": "not_started",
        "error_kind": "context_mismatch",
        "change_index": 0,
        "path": "file.txt",
        "error": "Rejected Codex patch before write: context mismatch. No files were modified.",
        "future_body_field": "NEVER_SURVIVE_PATCH_FAILURE",
        "match_diagnostic": {
            "chunk_index": 0,
            "match_source": "old_lines",
            "search_start_line": 3,
            "expected_line_count": 1,
            "available_line_count": 8,
            "closest_start_line": 5,
            "closest_exact_line_matches": 0,
            "closest_trim_end_line_matches": 0,
            "closest_trim_line_matches": 0,
            "first_exact_mismatch_offset": 1
        }
    });
    let result = apply_patch_agent_stdout_result(
        &valid.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert_eq!(result.output["match_diagnostic"]["closest_start_line"], 5);
    let recovery_action = result.output["recovery"]["action"].as_str().unwrap();
    assert_eq!(recovery_action, "read_files");
    assert!(crate::tool_runtime::tool_definition::is_adaptive_runtime_direct_tool(recovery_action));
    assert_eq!(result.output["recovery"]["reason"], "context_mismatch");
    assert_eq!(result.output["recovery"]["items"][0]["path"], "file.txt");
    assert_eq!(result.output["recovery"]["items"][0]["start_line"], 1);
    assert_eq!(result.output["recovery"]["items"][0]["limit"], 10);
    assert_eq!(result.output["recovery"]["change_index"], 0);
    assert_eq!(result.output["recovery"]["chunk_index"], 0);
    assert!(result.output.get("future_body_field").is_none());
    assert!(!serde_json::to_string(&result.output)
        .unwrap()
        .contains("NEVER_SURVIVE_PATCH_FAILURE"));

    let mut cases = Vec::new();
    let mut unexpected_field = valid.clone();
    unexpected_field["match_diagnostic"]["unexpected_field"] = json!("must-not-survive");
    cases.push(unexpected_field);
    let mut wrong_change = valid.clone();
    wrong_change["change_index"] = json!(1);
    cases.push(wrong_change);
    let mut wrong_path = valid.clone();
    wrong_path["path"] = json!("other.txt");
    cases.push(wrong_path);
    let mut wrong_chunk = valid.clone();
    wrong_chunk["match_diagnostic"]["chunk_index"] = json!(1);
    cases.push(wrong_chunk);
    let mut wrong_count = valid.clone();
    wrong_count["match_diagnostic"]["expected_line_count"] = json!(2);
    cases.push(wrong_count);
    let mut impossible_order = valid;
    impossible_order["match_diagnostic"]["closest_exact_line_matches"] = json!(1);
    cases.push(impossible_order);
    let mut out_of_range_candidate = context_mismatch_payload(1, 3, 8, Some(11));
    out_of_range_candidate["recovery"] = json!({
        "action": "read_file",
        "path": "other.txt",
        "start_line": 999999,
        "limit": 999999
    });
    cases.push(out_of_range_candidate);

    for invalid in cases {
        let result = apply_patch_agent_stdout_result(
            &invalid.to_string(),
            &patch,
            false,
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        );
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "not_started");
        assert_eq!(result.output["state_changed"], false);
        assert!(result.output.get("match_diagnostic").is_none());
        assert!(result.output.get("recovery").is_none());
        assert!(!serde_json::to_string(&result.output)
            .unwrap()
            .contains("must-not-survive"));
    }
}

#[test]
fn apply_patch_context_recovery_uses_deterministic_bounded_read_windows() {
    let patch = update_patch_with_old_line_count(5);
    let result = apply_patch_agent_stdout_result(
        &context_mismatch_payload(5, 120, 50, Some(130)).to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("apply_patch");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &schema,
    )
    .unwrap_or_else(|error| panic!("apply_patch recovery must match output schema: {error}"));
    assert_eq!(result.output["recovery"]["items"][0]["start_line"], 122);
    assert_eq!(result.output["recovery"]["items"][0]["limit"], 21);

    let near_start = apply_patch_agent_stdout_result(
        &context_mismatch_payload(5, 1, 20, Some(1)).to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert_eq!(near_start.output["recovery"]["items"][0]["start_line"], 1);
    assert_eq!(near_start.output["recovery"]["items"][0]["limit"], 20);

    let eof_partial = apply_patch_agent_stdout_result(
        &context_mismatch_payload(5, 11, 2, Some(12)).to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    let recovery = &eof_partial.output["recovery"]["items"][0];
    assert_eq!(recovery["start_line"], 4);
    assert_eq!(recovery["limit"], 9);
    assert_eq!(
        recovery["start_line"].as_u64().unwrap() + recovery["limit"].as_u64().unwrap() - 1,
        12
    );

    let large_patch = update_patch_with_old_line_count(100);
    let large = apply_patch_agent_stdout_result(
        &context_mismatch_payload(100, 1, 300, Some(100)).to_string(),
        &large_patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert_eq!(
        large.output["recovery"]["items"][0]["limit"],
        crate::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES
    );

    let mut distant_mismatch_payload = context_mismatch_payload(100, 1, 300, Some(100));
    distant_mismatch_payload["match_diagnostic"]["closest_exact_line_matches"] = json!(99);
    distant_mismatch_payload["match_diagnostic"]["closest_trim_end_line_matches"] = json!(99);
    distant_mismatch_payload["match_diagnostic"]["closest_trim_line_matches"] = json!(99);
    distant_mismatch_payload["match_diagnostic"]["first_exact_mismatch_offset"] = json!(90);
    let distant_mismatch = apply_patch_agent_stdout_result(
        &distant_mismatch_payload.to_string(),
        &large_patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    let recovery = &distant_mismatch.output["recovery"]["items"][0];
    assert_eq!(recovery["start_line"], 134);
    assert_eq!(
        recovery["limit"],
        crate::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES
    );
    let mismatch_line = 100 + 90 - 1;
    let recovery_start = recovery["start_line"].as_u64().unwrap() as usize;
    let recovery_end = recovery_start + recovery["limit"].as_u64().unwrap() as usize - 1;
    assert!((recovery_start..=recovery_end).contains(&mismatch_line));
}

#[test]
fn apply_patch_context_recovery_does_not_invent_candidate_or_leak_bodies() {
    let patch = update_patch_with_old_line_count(3);
    let no_candidate = apply_patch_agent_stdout_result(
        &context_mismatch_payload(3, 5, 0, None).to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(no_candidate.output.get("match_diagnostic").is_some());
    assert!(no_candidate.output.get("recovery").is_none());

    let private_patch = crate::apply_patch_shared::parse_codex_patch(
        "*** Begin Patch\n*** Update File: file.txt\n-PATCH_PRIVATE_TOKEN\n+new\n*** End Patch",
    )
    .unwrap();
    let mut payload = context_mismatch_payload(1, 1, 3, Some(2));
    payload["future_body_field"] = json!("SOURCE_PRIVATE_TOKEN");
    payload["recovery"] = json!({
        "action": "read_file",
        "reason": "context_mismatch",
        "path": "SOURCE_PRIVATE_TOKEN",
        "start_line": 1,
        "limit": 999999,
        "change_index": 0,
        "chunk_index": 0
    });
    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &private_patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("SOURCE_PRIVATE_TOKEN"));
    assert!(!serialized.contains("PATCH_PRIVATE_TOKEN"));
    assert_eq!(result.output["recovery"]["items"][0]["path"], "file.txt");
    assert!(
        result.output["recovery"]["items"][0]["limit"]
            .as_u64()
            .unwrap()
            <= 64
    );
}

#[test]
fn apply_patch_context_recovery_is_suppressed_for_outcome_unknown() {
    let patch = one_update_patch();
    let mut payload = context_mismatch_payload(1, 1, 3, Some(2));
    payload["changed"] = json!(true);
    payload["recovery"] = json!({"action": "read_file", "path": "other.txt"});

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(
        result.output["recovery_action"],
        "inspect_workspace_before_retry"
    );
    assert!(result.output.get("match_diagnostic").is_none());
    assert!(result.output.get("recovery").is_none());
    let schema = crate::tool_runtime::registry::output_schema_for_tool("apply_patch");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &schema,
    )
    .unwrap_or_else(|error| {
        panic!("apply_patch outcome_unknown must match output schema: {error}")
    });
}

#[test]
fn apply_patch_exact_unique_fuzzy_rejection_gets_validated_bounded_reread() {
    let patch = one_update_patch();
    let result = apply_patch_agent_stdout_result(
        &matching_rejection_payload(
            crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique,
            "trim",
            &[20],
            1,
            100,
        )
        .to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique,
    );

    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(
        result.output["match_rejection_diagnostic"]["classification"],
        "unique_fuzzy_candidate"
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["match_mode"],
        "trim"
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["match_source"],
        "old_lines"
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["matched_start_line"],
        20
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["candidate_count"],
        1
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["candidate_start_lines"],
        json!([20])
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["expected_line_count"],
        1
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["matching_mode_satisfied"],
        false
    );
    assert_eq!(result.output["recovery"]["action"], "read_files");
    assert_eq!(
        result.output["recovery"]["reason"],
        "matching_mode_rejected_unique_fuzzy"
    );
    assert_eq!(result.output["recovery"]["items"][0]["path"], "file.txt");
    assert_eq!(result.output["recovery"]["items"][0]["start_line"], 12);
    assert_eq!(result.output["recovery"]["items"][0]["limit"], 17);
    assert_eq!(
        result.output["recovery_action"],
        "reread_and_regenerate_exact_unique_patch"
    );
    assert!(result.output["retry_guidance"]
        .as_str()
        .unwrap()
        .contains("matching_mode=exact_unique"));
    for raw_runner_field in [
        "chunk_index",
        "match_mode",
        "match_source",
        "matched_start_line",
        "candidate_count",
        "matching_mode_satisfied",
    ] {
        assert!(result.output.get(raw_runner_field).is_none());
    }

    let schema = crate::tool_runtime::registry::output_schema_for_tool("apply_patch");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &schema,
    )
    .unwrap_or_else(|error| panic!("matching recovery must match output schema: {error}"));
}

#[test]
fn apply_patch_unique_ambiguous_rejection_returns_equal_candidate_windows() {
    let patch = one_update_patch();
    let result = apply_patch_agent_stdout_result(
        &matching_rejection_payload(
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
            "exact",
            &[20, 60],
            2,
            100,
        )
        .to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );

    assert!(!result.success);
    assert_eq!(
        result.output["match_rejection_diagnostic"]["classification"],
        "ambiguous_candidate"
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["candidate_count"],
        2
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["candidate_start_lines"],
        json!([20, 60])
    );
    assert!(result.output["match_rejection_diagnostic"]["matched_start_line"].is_null());
    assert_eq!(
        result.output["recovery"]["items"].as_array().unwrap().len(),
        2
    );
    assert_eq!(result.output["recovery"]["items"][0]["path"], "file.txt");
    assert_eq!(result.output["recovery"]["items"][1]["path"], "file.txt");
    assert_eq!(
        result.output["recovery_action"],
        "read_equal_candidates_and_refine_context"
    );
    assert!(result.output["retry_guidance"]
        .as_str()
        .unwrap()
        .contains("equal candidate"));
    assert!(!result.output["retry_guidance"]
        .as_str()
        .unwrap()
        .contains("preferred"));
}

#[test]
fn apply_patch_unique_large_ambiguity_returns_four_truncated_equal_windows() {
    let patch = one_update_patch();
    let result = apply_patch_agent_stdout_result(
        &matching_rejection_payload(
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
            "exact",
            &[10, 20, 30, 40],
            5,
            100,
        )
        .to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );

    assert!(!result.success);
    let diagnostic = &result.output["match_rejection_diagnostic"];
    assert_eq!(diagnostic["classification"], "ambiguous_candidate");
    assert_eq!(diagnostic["candidate_count"], 5);
    assert_eq!(diagnostic["candidate_start_lines"], json!([10, 20, 30, 40]));
    assert_eq!(diagnostic["candidate_positions_truncated"], true);
    assert_eq!(
        result.output["recovery"]["items"].as_array().unwrap().len(),
        4
    );
    assert!(diagnostic.get("winner").is_none());
    assert!(diagnostic.get("preferred").is_none());
    assert!(result.output["recovery"].get("winner").is_none());
    assert!(result.output["recovery"].get("preferred").is_none());
}

#[test]
fn apply_patch_unique_eof_rejection_cannot_expand_effective_search_range() {
    let patch = crate::apply_patch_shared::parse_codex_patch(
        "*** Begin Patch\n*** Update File: file.txt\n-old\n+new\n*** End of File\n*** End Patch",
    )
    .unwrap();
    let mut payload = matching_rejection_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        &[1, 3],
        2,
        3,
    );
    payload["search_start_line"] = json!(1);

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert!(result.output.get("match_rejection_diagnostic").is_none());
    assert!(result.output.get("recovery").is_none());
}

#[test]
fn apply_patch_unique_ambiguous_context_fact_is_valid_for_pure_addition() {
    let patch = crate::apply_patch_shared::parse_codex_patch(
        "*** Begin Patch\n*** Update File: file.txt\n@@ ctx\n+new\n*** End Patch",
    )
    .unwrap();
    let mut payload = matching_rejection_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        &[1, 3],
        2,
        4,
    );
    payload["match_source"] = json!("change_context");

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert_eq!(
        result.output["match_rejection_diagnostic"]["classification"],
        "ambiguous_candidate"
    );
    assert_eq!(
        result.output["match_rejection_diagnostic"]["match_source"],
        "change_context"
    );
    assert!(result.output["match_rejection_diagnostic"]["matched_start_line"].is_null());
    assert_eq!(
        result.output["recovery"]["items"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn apply_patch_unique_rejects_context_only_ambiguity_for_replacement_chunk() {
    let patch = crate::apply_patch_shared::parse_codex_patch(
        "*** Begin Patch\n*** Update File: file.txt\n@@ ctx\n-old\n+new\n*** End Patch",
    )
    .unwrap();
    let mut payload = matching_rejection_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        &[1, 3],
        2,
        4,
    );
    payload["match_source"] = json!("change_context");

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert!(result.output.get("match_rejection_diagnostic").is_none());
    assert!(result.output.get("recovery").is_none());
    assert!(result.output.get("path").is_none());
    assert!(result.output.get("change_index").is_none());
}

#[test]
fn apply_patch_matching_recovery_suppresses_spoofed_or_contradictory_metadata() {
    let patch = one_update_patch();
    let mut cases = Vec::new();

    let base = || {
        matching_rejection_payload(
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
            "exact",
            &[20, 60],
            2,
            100,
        )
    };
    let mut wrong_path = base();
    wrong_path["path"] = json!("other.txt");
    cases.push(wrong_path);
    let mut wrong_change = base();
    wrong_change["change_index"] = json!(1);
    cases.push(wrong_change);
    let mut wrong_chunk = base();
    wrong_chunk["chunk_index"] = json!(1);
    cases.push(wrong_chunk);
    let mut wrong_source = base();
    wrong_source["match_source"] = json!("change_context");
    cases.push(wrong_source);
    let mut equal_positions = base();
    equal_positions["candidate_start_lines"] = json!([20, 20]);
    cases.push(equal_positions);
    let mut out_of_range = base();
    out_of_range["candidate_start_lines"] = json!([20, 101]);
    cases.push(out_of_range);
    let mut bad_truncation = matching_rejection_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        &[10, 20, 30, 40],
        5,
        100,
    );
    bad_truncation["candidate_positions_truncated"] = json!(false);
    cases.push(bad_truncation);
    let mut wrong_mode = base();
    wrong_mode["requested_matching_mode"] = json!("exact_unique");
    cases.push(wrong_mode);

    for payload in cases {
        let result = apply_patch_agent_stdout_result(
            &payload.to_string(),
            &patch,
            false,
            crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        );
        assert!(!result.success);
        assert!(result.output.get("match_rejection_diagnostic").is_none());
        assert!(result.output.get("recovery").is_none());
        assert!(result.output.get("path").is_none());
        assert!(result.output.get("change_index").is_none());
    }
}

#[test]
fn apply_patch_matching_recovery_is_suppressed_for_outcome_unknown() {
    let patch = one_update_patch();
    let mut payload = matching_rejection_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        &[20, 60],
        2,
        100,
    );
    payload["changed"] = json!(true);
    payload["state_changed"] = json!(true);

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(
        result.output["recovery_action"],
        "inspect_workspace_before_retry"
    );
    assert!(result.output.get("match_rejection_diagnostic").is_none());
    assert!(result.output.get("recovery").is_none());
}

#[test]
fn apply_patch_matching_recovery_never_leaks_source_or_patch_bodies() {
    let patch = crate::apply_patch_shared::parse_codex_patch(
        "*** Begin Patch\n*** Update File: file.txt\n-PATCH_PRIVATE_TOKEN\n+new\n*** End Patch",
    )
    .unwrap();
    let mut payload = matching_rejection_payload(
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
        "exact",
        &[4, 8],
        2,
        20,
    );
    payload["error"] = json!("SOURCE_PRIVATE_TOKEN");
    payload["future_body_field"] = json!("SOURCE_PRIVATE_TOKEN");
    payload["recovery"] = json!({
        "action": "read_files",
        "items": [{"path": "SOURCE_PRIVATE_TOKEN", "start_line": 1, "limit": 999999}]
    });
    payload["match_rejection_diagnostic"] = json!({"source": "SOURCE_PRIVATE_TOKEN"});
    payload["recovery_kind"] = json!("reobserve");
    payload["recovery_tool"] = json!("list_jobs");

    let result = apply_patch_agent_stdout_result(
        &payload.to_string(),
        &patch,
        false,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique,
    );
    let serialized = serde_json::to_string(&result).unwrap();
    assert!(!serialized.contains("SOURCE_PRIVATE_TOKEN"));
    assert!(!serialized.contains("PATCH_PRIVATE_TOKEN"));
    assert!(result.output.get("recovery_kind").is_none());
    assert!(result.output.get("recovery_tool").is_none());
    assert_eq!(result.output["recovery"]["items"][0]["path"], "file.txt");
}
