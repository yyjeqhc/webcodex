//! apply_text_edits tests for tool_runtime.

use super::super::*;
use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerPollRequest, RunnerResultRequest};
use serde_json::Value;

fn scoped_text_edit(
    mut edit: ApplyTextEditInput,
    start_line: usize,
    end_line: usize,
) -> ApplyTextEditInput {
    edit.line_scope = Some(ApplyTextLineScope {
        start_line,
        end_line,
    });
    edit
}

#[tokio::test]
async fn bulk_exact_invalid_combination_and_missing_revision_fail_before_dispatch() {
    let runtime = runtime_with_agent_project("ate-bulk-preflight");
    let project = agent_test_project_id("ate-bulk-preflight");
    let mut edit = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("OLD"),
        Some("NEW"),
        None,
    );
    edit.expected_match_count = Some(2);
    edit.occurrence = Some(1);
    let change = edit_change("sample.txt", "unused", vec![edit.clone()]);
    let rejected = runtime
        .apply_text_edits(project.clone(), vec![change], None)
        .await;
    assert!(!rejected.success);
    assert_eq!(rejected.output["error_kind"], "invalid_edit");
    assert_eq!(rejected.output["change_index"], 0);
    assert_eq!(rejected.output["edit_index"], 0);
    assert_eq!(rejected.output["execution_state"], "not_started");

    edit.occurrence = None;
    let change = edit_change("sample.txt", "unused", vec![edit]);
    let rejected = runtime.apply_text_edits(project, vec![change], None).await;
    assert!(!rejected.success);
    assert_eq!(rejected.output["error_kind"], "missing_read_revision");
    assert_eq!(rejected.output["edit_index"], 0);
}

#[tokio::test]
async fn bulk_exact_old_runner_is_rejected_without_queueing() {
    let runtime = runtime_with_agent_project("ate-bulk-old-runner");
    register_agent(
        &runtime,
        "ate-bulk-old-runner",
        None,
        RunnerCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-bulk-old-runner");
    let revision = seed_read_revision(&runtime, &project, "sample.txt", &"a".repeat(64)).await;
    let mut edit = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("OLD"),
        Some("NEW"),
        None,
    );
    edit.expected_match_count = Some(2);
    let mut change = edit_change("sample.txt", "unused", vec![edit]);
    change.expected_read_revision = Some(revision);
    let mut stale_change = change.clone();
    stale_change.expected_read_revision = Some(revision + 10_000);
    let stale = runtime
        .apply_text_edits(project.clone(), vec![stale_change], None)
        .await;
    assert!(!stale.success);
    assert_eq!(stale.output["error_kind"], "unknown_read_revision");
    assert_eq!(stale.output["change_index"], 0);
    assert_eq!(stale.output["execution_state"], "not_started");
    let result = runtime.apply_text_edits(project, vec![change], None).await;
    assert!(!result.success);
    assert_eq!(
        result.output["capability"],
        "apply_text_edit_expected_match_count"
    );
    assert_eq!(result.output["change_index"], 0);
    assert_eq!(result.output["edit_index"], 0);
    assert_eq!(result.output["execution_state"], "not_started");
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "ate-bulk-old-runner".to_string(),
            runner_instance_id: "inst".to_string()
        })
        .await
        .unwrap()
        .is_none());
}

#[test]
fn apply_text_edits_occurrence_and_recovery_schemas_are_model_visible() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
    let change_variants = spec.input_schema["properties"]["changes"]["items"]["anyOf"]
        .as_array()
        .expect("apply_text_edits change wire union");
    let canonical = change_variants
        .iter()
        .find(|variant| variant["properties"].get("kind").is_some())
        .expect("canonical apply-file-change wire variant");
    let edit = &canonical["properties"]["edits"]["items"];
    assert_eq!(
        edit["properties"]["kind"]["enum"],
        serde_json::json!([
            "replace_exact",
            "insert_after",
            "insert_before",
            "delete_exact"
        ])
    );
    assert_eq!(edit["properties"]["occurrence"]["type"], "integer");
    assert_eq!(edit["properties"]["occurrence"]["minimum"], 1);
    assert_eq!(edit["properties"]["expected_match_count"]["minimum"], 1);
    assert_eq!(edit["properties"]["expected_match_count"]["maximum"], 1024);
    let line_scope = &edit["properties"]["line_scope"];
    assert_eq!(line_scope["type"], "object");
    assert_eq!(line_scope["additionalProperties"], false);
    assert_eq!(
        line_scope["required"],
        serde_json::json!(["start_line", "end_line"])
    );
    assert_eq!(line_scope["properties"]["start_line"]["minimum"], 1);
    assert_eq!(line_scope["properties"]["end_line"]["minimum"], 1);
    assert!(!edit["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("occurrence")));
    let output_properties = &spec.output_schema["properties"]["output"]["properties"];
    for removed in [
        "conflict_recovery",
        "retry_guidance",
        "expected_read_revision",
        "suggested_call",
        "recovery_action",
    ] {
        assert!(
            output_properties.get(removed).is_none(),
            "legacy recovery field {removed}"
        );
    }
    assert_eq!(output_properties["recovery"]["type"], "object");
    let candidate = &output_properties["candidate_ranges"]["items"];
    assert_eq!(
        candidate["required"],
        serde_json::json!(["start_line", "end_line"])
    );
    assert_eq!(candidate["properties"]["occurrence"]["minimum"], 1);
    let conflict_range = &output_properties["conflicting_edit_ranges"]["items"];
    assert_eq!(
        conflict_range["required"],
        serde_json::json!(["edit_index", "start_line", "end_line"])
    );
    assert_eq!(conflict_range["additionalProperties"], false);
    assert_eq!(output_properties["conflicting_edit_ranges"]["maxItems"], 2);
    assert!(output_properties["change_index"]["anyOf"].is_array());
    assert!(output_properties["edit_index"]["anyOf"].is_array());
    assert!(output_properties["state_changed"]["anyOf"].is_array());
    assert_eq!(
        output_properties["execution_state"]["enum"],
        serde_json::json!(["not_started", "completed", "outcome_unknown"])
    );
    assert_eq!(output_properties["ignored_noop_count"]["type"], "integer");
    assert_eq!(output_properties["planned_count"]["type"], "integer");
    assert_eq!(output_properties["change_summary"]["type"], "object");
    assert_eq!(output_properties["expected_match_count"]["type"], "integer");
    assert_eq!(output_properties["actual_match_count"]["type"], "integer");
    let file_properties = output_properties["files"]["items"]["properties"]
        .as_object()
        .expect("apply_text_edits file summary properties");
    assert!(file_properties.contains_key("read_revision"));
    assert!(!file_properties.contains_key("old_sha256"));
    assert!(!file_properties.contains_key("new_sha256"));
    assert_eq!(
        file_properties["read_revision"]["anyOf"][0]["maximum"],
        9_007_199_254_740_991_u64
    );

    let stale_revision = serde_json::json!({
        "success": false,
        "output": {
            "state_changed": false,
            "error_kind": "stale_file_revision",
            "change_index": 0,
            "kind": "edit",
            "path": "src/lib.rs",
            "recovery": {
                "tool": "read_files",
                "arguments": {"project": "agent:r:p", "items": [{"path": "src/lib.rs"}]}
            }
        },
        "error": "source changed before mutation"
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &stale_revision,
        &spec.output_schema,
    )
    .unwrap_or_else(|error| panic!("compact stale recovery must match output schema: {error}"));

    let scoped_conflict = serde_json::json!({
        "success": false,
        "output": {
            "state_changed": false,
            "error_kind": "occurrence_outside_line_scope",
            "change_index": 0,
            "edit_index": 0,
            "kind": "replace_exact",
            "path": "src/lib.rs",
            "match_count": 2,
            "candidate_ranges": [{"occurrence": 2, "start_line": 50, "end_line": 50}],
            "candidates_truncated": false
        },
        "error": "occurrence and line_scope disagree"
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &scoped_conflict,
        &spec.output_schema,
    )
    .unwrap_or_else(|error| panic!("compact conflict evidence must match output schema: {error}"));

    let openapi = crate::openapi::build_openapi_spec();
    let action = &openapi["paths"]["/api/actions/apply_text_edits"]["post"];
    assert_eq!(action["operationId"], "apply_text_edits");
    let action_change_variants = action["requestBody"]["content"]["application/json"]["schema"]
        ["properties"]["changes"]["items"]["anyOf"]
        .as_array()
        .expect("Action apply_text_edits change wire union");
    let action_canonical = action_change_variants
        .iter()
        .find(|variant| variant["properties"].get("kind").is_some())
        .expect("Action canonical apply-file-change wire variant");
    let action_edit = &action_canonical["properties"]["edits"]["items"];
    assert_eq!(action_edit["properties"]["occurrence"]["type"], "integer");
    assert_eq!(action_edit["properties"]["occurrence"]["minimum"], 1);
    assert!(spec.description.contains("occurrence"));
    assert!(spec.description.contains("line_scope"));
    assert!(spec.description.contains("global source order"));
    assert!(spec.description.contains("expected_read_revision"));
    assert!(spec.description.contains("preflighted transactionally"));
    assert!(spec.description.contains("conflicts fail closed"));
    assert!(
        spec.description.chars().count() <= crate::tool_runtime::MODEL_TOOL_DESCRIPTION_MAX_CHARS
    );
}

#[test]
fn apply_text_edits_replace_exact_large_block() {
    let original = "mod foo {\n    fn old_a() {\n        todo!()\n    }\n    fn old_b() {\n        todo!()\n    }\n}\n";
    let old_block =
        "    fn old_a() {\n        todo!()\n    }\n    fn old_b() {\n        todo!()\n    }";
    let new_block =
        "    fn new_a() -> u32 {\n        1\n    }\n    fn new_b() -> u32 {\n        2\n    }";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some(old_block),
        Some(new_block),
        None,
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/foo.rs", &edits, None, false).unwrap();
    assert!(updated.contains("fn new_a() -> u32 {"));
    assert!(updated.contains("fn new_b() -> u32 {"));
    assert!(!updated.contains("old_a"));
    assert!(!updated.contains("old_b"));
    assert_eq!(out["path"], "src/foo.rs");
    assert_eq!(out["applied_count"], 1);
    assert_eq!(out["changed"], true);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["edits"][0]["kind"], "replace_exact");
    assert_eq!(out["changed_paths"][0], "src/foo.rs");
}

#[test]
fn apply_text_edits_multiple_edits_atomic() {
    let original = "alpha\nbeta\ngamma\ndelta\n";
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("beta"),
            Some("BETA"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("delta"),
            Some("DELTA"),
            None,
        ),
    ];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "alpha\nBETA\ngamma\nDELTA\n");
    assert_eq!(out["applied_count"], 2);
    assert_eq!(out["edits"].as_array().unwrap().len(), 2);
}

#[test]
fn apply_text_edits_ignores_empty_insert_noop_without_blocking_other_edits() {
    let original = "alpha\nbeta\n";
    let edits = vec![
        text_edit(
            ApplyTextEditKind::InsertBefore,
            None,
            Some(""),
            Some("anchor-that-does-not-exist"),
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("beta"),
            Some("BETA"),
            None,
        ),
    ];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "alpha\nBETA\n");
    assert_eq!(out["applied_count"], 2);
    assert_eq!(out["ignored_noop_count"], 1);
    assert_eq!(out["edits"].as_array().unwrap().len(), 1);
}

#[test]
fn apply_text_edits_rejects_missing_match() {
    let original = "alpha\nbeta\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("nonexistent"),
        Some("x"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("not found"));
    assert!(err.contains("No files were modified"));
    // Original is untouched (pure function never mutates input).
    assert_eq!(original, "alpha\nbeta\n");
}

#[test]
fn apply_text_edits_rejects_ambiguous_match() {
    let original = "dup\ndup\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("x"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("matched 2 times"));
    assert!(err.contains("ambiguous"));
}

#[test]
fn apply_text_edits_line_scope_fences_all_exact_edit_kinds() {
    let original = "head\ndup\nmid\ndup\ntail\n";
    let cases = [
        (
            scoped_text_edit(
                text_edit(
                    ApplyTextEditKind::ReplaceExact,
                    Some("dup"),
                    Some("SECOND"),
                    None,
                ),
                4,
                4,
            ),
            "head\ndup\nmid\nSECOND\ntail\n",
        ),
        (
            scoped_text_edit(
                text_edit(ApplyTextEditKind::DeleteExact, Some("dup\n"), None, None),
                4,
                4,
            ),
            "head\ndup\nmid\ntail\n",
        ),
        (
            scoped_text_edit(
                text_edit(
                    ApplyTextEditKind::InsertBefore,
                    None,
                    Some("BEFORE\n"),
                    Some("dup\n"),
                ),
                4,
                4,
            ),
            "head\ndup\nmid\nBEFORE\ndup\ntail\n",
        ),
        (
            scoped_text_edit(
                text_edit(
                    ApplyTextEditKind::InsertAfter,
                    None,
                    Some("AFTER\n"),
                    Some("dup\n"),
                ),
                4,
                4,
            ),
            "head\ndup\nmid\ndup\nAFTER\ntail\n",
        ),
    ];
    for (edit, expected) in cases {
        let (updated, _) =
            files::apply_text_edits_to_string(original, "src/x.rs", &[edit], None, false).unwrap();
        assert_eq!(updated, expected);
    }
}

#[test]
fn apply_text_edits_line_scope_never_renumbers_occurrence_and_rejects_invalid_range() {
    let original = "head\ndup\nmid\ndup\ntail\n";
    let mut mismatch = scoped_text_edit(
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("dup"),
            Some("x"),
            None,
        ),
        4,
        4,
    );
    mismatch.occurrence = Some(1);
    let err = files::apply_text_edits_to_string(original, "src/x.rs", &[mismatch], None, false)
        .unwrap_err();
    assert!(err.contains("occurrence 1 is outside line_scope"));

    let reversed = scoped_text_edit(
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("dup"),
            Some("x"),
            None,
        ),
        4,
        3,
    );
    let err = files::apply_text_edits_to_string(original, "src/x.rs", &[reversed], None, false)
        .unwrap_err();
    assert!(err.contains("end_line must be greater than or equal to start_line"));
}

#[test]
fn apply_text_edits_expected_file_sha256_guard() {
    let original = "alpha\nbeta\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("beta"),
        Some("BETA"),
        None,
    )];
    // Wrong sha → rejected before any edit is applied.
    let err = files::apply_text_edits_to_string(
        original,
        "src/x.rs",
        &edits,
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
        false,
    )
    .unwrap_err();
    assert!(err.contains("expected_file_sha256 mismatch"));
    // Correct sha → succeeds.
    let real_sha = files::sha256_hex_bytes(original.as_bytes());
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, Some(&real_sha), false)
            .unwrap();
    assert_eq!(updated, "alpha\nBETA\n");
}

#[test]
fn apply_text_edits_insert_before_after_unique_anchor() {
    let original = "header\nbody\nfooter\n";
    // insert_before unique anchor.
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertBefore,
        None,
        Some("// before body\n"),
        Some("body"),
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "header\n// before body\nbody\nfooter\n");
    assert_eq!(out["edits"][0]["kind"], "insert_before");

    // insert_after unique anchor.
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertAfter,
        None,
        Some("// after body\n"),
        Some("body\n"),
    )];
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "header\nbody\n// after body\nfooter\n");

    // Ambiguous anchor → rejected.
    let dup = "tag\ntag\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::InsertBefore,
        None,
        Some("x"),
        Some("tag"),
    )];
    let err = files::apply_text_edits_to_string(dup, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("matched 2 times"));
}

#[test]
fn apply_text_edits_delete_exact_removes_block() {
    let original = "keep1\ndelete_me\nkeep2\n";
    let edits = vec![text_edit(
        ApplyTextEditKind::DeleteExact,
        Some("delete_me\n"),
        None,
        None,
    )];
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();
    assert_eq!(updated, "keep1\nkeep2\n");
    assert_eq!(out["edits"][0]["kind"], "delete_exact");
}

#[test]
fn apply_text_edits_rejects_overlapping_edits() {
    let original = "abcdefghij\n";
    // Two replace_exact ops whose ranges overlap.
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("cde"),
            Some("X"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("def"),
            Some("Y"),
            None,
        ),
    ];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("overlap"));
}

#[test]
fn apply_text_edits_crlf_accepts_lf_edits_and_preserves_crlf() {
    let original = "one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n";
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("one\n"),
            Some("ONE\n"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::InsertAfter,
            None,
            Some("AFTER-TWO\n"),
            Some("two\n"),
        ),
        text_edit(ApplyTextEditKind::DeleteExact, Some("three\n"), None, None),
        text_edit(
            ApplyTextEditKind::InsertBefore,
            None,
            Some("BEFORE-FOUR\n"),
            Some("four\n"),
        ),
    ];
    let real_sha = files::sha256_hex_bytes(original.as_bytes());
    let (updated, out) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, Some(&real_sha), false)
            .unwrap();

    assert_eq!(
        updated,
        "ONE\r\ntwo\r\nAFTER-TWO\r\nBEFORE-FOUR\r\nfour\r\nfive\r\n"
    );
    assert!(!updated.replace("\r\n", "").contains('\n'));
    assert_eq!(out["changed"], true);
}

#[test]
fn apply_text_edits_lf_accepts_crlf_edits_and_preserves_lf() {
    let original = "one\ntwo\nthree\nfour\nfive\n";
    let edits = vec![
        text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("one\r\n"),
            Some("ONE\r\n"),
            None,
        ),
        text_edit(
            ApplyTextEditKind::InsertAfter,
            None,
            Some("AFTER-TWO\r\n"),
            Some("two\r\n"),
        ),
        text_edit(
            ApplyTextEditKind::DeleteExact,
            Some("three\r\n"),
            None,
            None,
        ),
        text_edit(
            ApplyTextEditKind::InsertBefore,
            None,
            Some("BEFORE-FOUR\r\n"),
            Some("four\r\n"),
        ),
    ];
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap();

    assert_eq!(updated, "ONE\ntwo\nAFTER-TWO\nBEFORE-FOUR\nfour\nfive\n");
    assert!(!updated.contains('\r'));
}

#[test]
fn apply_text_edits_rejects_mixed_or_bare_cr_line_endings() {
    for original in ["one\r\ntwo\n", "one\rtwo\r"] {
        let edits = vec![text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("one\n"),
            Some("ONE\n"),
            None,
        )];
        let err = files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false)
            .unwrap_err();
        assert!(err.contains("line endings"), "{err}");
        assert!(err.contains("No files were modified"), "{err}");
    }
}

#[test]
fn apply_text_edits_rejects_bare_cr_edit_text_without_file_line_endings() {
    let original = "one";
    let edits = vec![text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("one"),
        Some("ONE\r"),
        None,
    )];
    let err =
        files::apply_text_edits_to_string(original, "src/x.rs", &edits, None, false).unwrap_err();
    assert!(err.contains("bare CR"), "{err}");
    assert!(err.contains("No files were modified"), "{err}");
}

#[test]
fn apply_text_edits_occurrence_selects_exact_second_match_in_test_mirror() {
    let original = "dup\nkeep\ndup\n";
    let mut edit = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("SECOND"),
        None,
    );
    edit.occurrence = Some(2);
    let sha = files::sha256_hex_bytes(original.as_bytes());
    let (updated, _) =
        files::apply_text_edits_to_string(original, "src/x.rs", &[edit], Some(&sha), false)
            .unwrap();
    assert_eq!(updated, "dup\nkeep\nSECOND\n");
}

#[test]
fn apply_text_edits_stale_sha_still_rejects_before_occurrence() {
    let original = "dup\ndup\n";
    let mut edit = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("SECOND"),
        None,
    );
    edit.occurrence = Some(2);
    let error = files::apply_text_edits_to_string(
        original,
        "src/x.rs",
        &[edit],
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        false,
    )
    .unwrap_err();
    assert!(error.contains("expected_file_sha256 mismatch"));
    assert!(!error.contains("occurrence"));
}

fn parsed_apply_text_edits_changes(arguments: Value) -> Vec<ApplyFileChangeInput> {
    let call = ToolCall::from_tool_name("apply_text_edits", arguments).unwrap();
    let ToolCall::ApplyTextEdits { changes, .. } = call else {
        panic!("expected apply_text_edits");
    };
    changes
}

async fn complete_apply_text_edits_success(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: String,
    path: &str,
) {
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false,
                    "applied_count": 1,
                    "changed": true,
                    "would_change": true,
                    "files": [{
                        "index": 0,
                        "kind": "edit",
                        "path": path,
                        "to_path": null,
                        "old_sha256": "a".repeat(64),
                        "new_sha256": "b".repeat(64),
                        "changed": true,
                        "would_change": true,
                        "edits": []
                    }],
                    "changed_paths": [path]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn apply_text_edits_shorthand_unique_replace_uses_canonical_runner_payload() {
    let client_id = "ate-shorthand-unique";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let changes = parsed_apply_text_edits_changes(serde_json::json!({
        "project": &project,
        "changes": [{"path":"src/lib.rs","old_text":"old","new_text":"new"}]
    }));
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move { runtime.apply_text_edits(project, changes, None).await }
    });

    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(request.kind, "file_apply_text_edits");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["changes"][0]["kind"], "edit");
    assert_eq!(payload["changes"][0]["path"], "src/lib.rs");
    assert!(payload["changes"][0].get("old_text").is_none());
    assert!(payload["changes"][0].get("new_text").is_none());
    assert_eq!(payload["changes"][0]["edits"][0]["kind"], "replace_exact");
    assert_eq!(payload["changes"][0]["edits"][0]["old_text"], "old");
    assert_eq!(payload["changes"][0]["edits"][0]["new_text"], "new");
    assert!(payload["changes"][0]["expected_sha256"].is_null());

    complete_apply_text_edits_success(&runtime, client_id, request.request_id, "src/lib.rs").await;
    assert!(task.await.unwrap().success);
}

#[tokio::test]
async fn apply_text_edits_shorthand_read_revision_uses_existing_wire_guard() {
    let client_id = "ate-shorthand-revision";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_occurrence: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let sha = "d".repeat(64);
    let revision = seed_read_revision(&runtime, &project, "src/lib.rs", &sha).await;
    let changes = parsed_apply_text_edits_changes(serde_json::json!({
        "project": &project,
        "changes": [{
            "path":"src/lib.rs",
            "old_text":"dup",
            "new_text":"SECOND",
            "expected_read_revision": revision
        }]
    }));
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move { runtime.apply_text_edits(project, changes, None).await }
    });

    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["changes"][0]["kind"], "edit");
    assert_eq!(payload["changes"][0]["expected_sha256"], sha);
    assert_eq!(payload["changes"][0]["edits"][0]["kind"], "replace_exact");
    assert!(payload["changes"][0]["edits"][0]["occurrence"].is_null());
    assert!(payload["changes"][0]
        .get("expected_read_revision")
        .is_none());

    complete_apply_text_edits_success(&runtime, client_id, request.request_id, "src/lib.rs").await;
    assert!(task.await.unwrap().success);
}

#[tokio::test]
async fn apply_text_edits_shorthand_positional_guards_match_canonical_rejection() {
    let runtime = test_runtime();
    for shorthand in [
        serde_json::json!({"path":"src/lib.rs","old_text":"dup","new_text":"x","occurrence":2}),
        serde_json::json!({"path":"src/lib.rs","old_text":"dup","new_text":"x","line_scope":{"start_line":2,"end_line":2}}),
        serde_json::json!({"path":"src/lib.rs","old_text":"dup","new_text":"x","expected_read_revision":3817291045227_u64,"occurrence":2}),
    ] {
        let parsed = ToolCall::from_tool_name(
            "apply_text_edits",
            serde_json::json!({
                "project": "agent:unused:unused",
                "changes": [shorthand]
            }),
        );
        assert!(
            parsed.is_err(),
            "positional shorthand must stay canonical-only"
        );
    }

    let empty_old_text = parsed_apply_text_edits_changes(serde_json::json!({
        "project": "agent:unused:unused",
        "changes": [{"path":"src/lib.rs","old_text":"","new_text":"x"}]
    }));
    let rejected = runtime
        .apply_text_edits("agent:unused:unused".to_string(), empty_old_text, None)
        .await;
    assert!(!rejected.success);
    assert_eq!(rejected.output["error_kind"], "invalid_edit");
    assert_eq!(rejected.output["state_changed"], false);
    assert!(rejected
        .error
        .as_deref()
        .is_some_and(|error| error.contains("old_text must be non-empty")));
}

async fn assert_no_apply_text_edits_runner_request(runtime: &ToolRuntime, client_id: &str) {
    let request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap();
    assert!(
        request.is_none(),
        "unexpected hidden apply_text_edits Runner request"
    );
}

#[tokio::test]
async fn apply_text_edits_translates_strong_read_revisions_to_existing_wire_sha_guards() {
    let runtime = runtime_with_agent_project("ate-revision-translation");
    register_agent(
        &runtime,
        "ate-revision-translation",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_occurrence: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-revision-translation");
    let edit_sha = "a".repeat(64);
    let delete_sha = "b".repeat(64);
    let rename_sha = "c".repeat(64);
    let edit_revision = seed_read_revision(&runtime, &project, "src/lib.rs", &edit_sha).await;
    let delete_revision = seed_read_revision(&runtime, &project, "delete.txt", &delete_sha).await;
    let rename_revision = seed_read_revision(&runtime, &project, "rename.txt", &rename_sha).await;

    let mut positional = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("SECOND"),
        None,
    );
    positional.occurrence = Some(2);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .apply_text_edits(
                    project,
                    vec![
                        ApplyFileChangeInput {
                            kind: ApplyFileChangeKind::Edit,
                            path: "src/lib.rs".to_string(),
                            to_path: None,
                            content: None,
                            edits: vec![positional],
                            expected_read_revision: Some(edit_revision),
                        },
                        ApplyFileChangeInput {
                            kind: ApplyFileChangeKind::Delete,
                            path: "delete.txt".to_string(),
                            to_path: None,
                            content: None,
                            edits: Vec::new(),
                            expected_read_revision: Some(delete_revision),
                        },
                        ApplyFileChangeInput {
                            kind: ApplyFileChangeKind::Rename,
                            path: "rename.txt".to_string(),
                            to_path: Some("moved.txt".to_string()),
                            content: None,
                            edits: Vec::new(),
                            expected_read_revision: Some(rename_revision),
                        },
                    ],
                    None,
                )
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "ate-revision-translation").await;
    assert_eq!(request.kind, "file_apply_text_edits");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["changes"][0]["expected_sha256"], edit_sha);
    assert_eq!(payload["changes"][1]["expected_sha256"], delete_sha);
    assert_eq!(payload["changes"][2]["expected_sha256"], rename_sha);
    assert_eq!(payload["changes"][0]["edits"][0]["occurrence"], 2);
    for change in payload["changes"].as_array().unwrap() {
        assert!(change.get("expected_read_revision").is_none());
    }

    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "ate-revision-translation".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false,
                    "applied_count": 3,
                    "changed": true,
                    "would_change": true,
                    "files": [
                        {
                            "index": 0, "kind": "edit", "path": "src/lib.rs", "to_path": null,
                            "old_sha256": edit_sha, "new_sha256": "d".repeat(64),
                            "changed": true, "would_change": true, "edits": []
                        },
                        {
                            "index": 1, "kind": "delete", "path": "delete.txt", "to_path": null,
                            "old_sha256": delete_sha, "new_sha256": null,
                            "changed": true, "would_change": true, "edits": []
                        },
                        {
                            "index": 2, "kind": "rename", "path": "rename.txt", "to_path": "moved.txt",
                            "old_sha256": rename_sha, "new_sha256": "e".repeat(64),
                            "changed": true, "would_change": true, "edits": []
                        }
                    ],
                    "changed_paths": ["src/lib.rs", "delete.txt", "rename.txt", "moved.txt"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
}

#[tokio::test]
async fn apply_text_edits_success_mints_final_revisions_and_continues_without_reread() {
    let client_id = "ate-final-revisions";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let edit_old = "1".repeat(64);
    let rename_old = "2".repeat(64);
    let delete_old = "3".repeat(64);
    let noop_sha = "4".repeat(64);
    let edit_revision = seed_read_revision(&runtime, &project, "edit.txt", &edit_old).await;
    let rename_revision = seed_read_revision(&runtime, &project, "rename.txt", &rename_old).await;
    let delete_revision = seed_read_revision(&runtime, &project, "delete.txt", &delete_old).await;
    let noop_revision = seed_read_revision(&runtime, &project, "noop.txt", &noop_sha).await;

    let guarded_edit = |path: &str, revision: u64, old_text: &str, new_text: &str| {
        let mut change = edit_change(
            path,
            "unused",
            vec![text_edit(
                ApplyTextEditKind::ReplaceExact,
                Some(old_text),
                Some(new_text),
                None,
            )],
        );
        change.expected_read_revision = Some(revision);
        change
    };
    let changes = vec![
        guarded_edit("edit.txt", edit_revision, "old", "new"),
        ApplyFileChangeInput {
            kind: ApplyFileChangeKind::Create,
            path: "created.txt".to_string(),
            to_path: None,
            content: Some("created\n".to_string()),
            edits: Vec::new(),
            expected_read_revision: None,
        },
        ApplyFileChangeInput {
            kind: ApplyFileChangeKind::Rename,
            path: "rename.txt".to_string(),
            to_path: Some("renamed.txt".to_string()),
            content: None,
            edits: Vec::new(),
            expected_read_revision: Some(rename_revision),
        },
        ApplyFileChangeInput {
            kind: ApplyFileChangeKind::Delete,
            path: "delete.txt".to_string(),
            to_path: None,
            content: None,
            edits: Vec::new(),
            expected_read_revision: Some(delete_revision),
        },
        guarded_edit("noop.txt", noop_revision, "same", "same"),
    ];
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move { runtime.apply_text_edits(project, changes, None).await }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    let edit_new = "5".repeat(64);
    let create_new = "6".repeat(64);
    let rename_new = "7".repeat(64);
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false,
                    "applied_count": 5,
                    "changed": true,
                    "would_change": true,
                    "files": [
                        {"index":0,"kind":"edit","path":"edit.txt","to_path":null,"old_sha256":edit_old,"new_sha256":edit_new,"changed":true,"would_change":true,"edits":[]},
                        {"index":1,"kind":"create","path":"created.txt","to_path":null,"old_sha256":null,"new_sha256":create_new,"changed":true,"would_change":true,"edits":[]},
                        {"index":2,"kind":"rename","path":"rename.txt","to_path":"renamed.txt","old_sha256":rename_old,"new_sha256":rename_new,"changed":true,"would_change":true,"edits":[]},
                        {"index":3,"kind":"delete","path":"delete.txt","to_path":null,"old_sha256":delete_old,"new_sha256":null,"changed":true,"would_change":true,"edits":[]},
                        {"index":4,"kind":"edit","path":"noop.txt","to_path":null,"old_sha256":noop_sha,"new_sha256":noop_sha,"changed":false,"would_change":false,"edits":[]}
                    ],
                    "changed_paths": ["edit.txt", "created.txt", "rename.txt", "renamed.txt", "delete.txt"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let files = result.output["files"].as_array().unwrap();
    assert_eq!(files.len(), 5);
    for file in files {
        assert!(file.get("old_sha256").is_some());
        assert!(file.get("new_sha256").is_some());
        assert!(file.get("read_revision").is_some());
    }
    let edit_final_revision = files[0]["read_revision"].as_u64().unwrap();
    assert!(files[1]["read_revision"].as_u64().is_some());
    let rename_final_revision = files[2]["read_revision"].as_u64().unwrap();
    assert!(files[3]["read_revision"].is_null());
    assert_eq!(files[4]["read_revision"].as_u64(), Some(noop_revision));
    let audit = crate::tool_runtime::tool_audit::session_log_result_for_tool(
        "apply_text_edits",
        &result.output,
    );
    assert_eq!(audit["files"][0]["old_sha256"], edit_old);
    assert_eq!(audit["files"][0]["new_sha256"], edit_new);

    let edit_continuation = guarded_edit("edit.txt", edit_final_revision, "new", "newer");
    let continuation_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .apply_text_edits(project, vec![edit_continuation], None)
                .await
        }
    });
    let continuation_request = wait_for_patch_agent_request(&runtime, client_id).await;
    let continuation_payload: Value =
        serde_json::from_str(continuation_request.content.as_deref().unwrap()).unwrap();
    assert_eq!(
        continuation_payload["changes"][0]["expected_sha256"],
        edit_new
    );
    let edit_newer = "8".repeat(64);
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: continuation_request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false, "applied_count": 1, "changed": true, "would_change": true,
                    "files": [{"index":0,"kind":"edit","path":"edit.txt","to_path":null,"old_sha256":edit_new,"new_sha256":edit_newer,"changed":true,"would_change":true,"edits":[]}],
                    "changed_paths": ["edit.txt"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let continuation_result = continuation_task.await.unwrap();
    assert!(
        continuation_result.success,
        "{:?}",
        continuation_result.error
    );
    let edit_newer_revision = continuation_result.output["files"][0]["read_revision"]
        .as_u64()
        .unwrap();

    let source_rejected = runtime
        .apply_text_edits(
            project.clone(),
            vec![guarded_edit(
                "rename.txt",
                rename_final_revision,
                "old",
                "new",
            )],
            None,
        )
        .await;
    assert!(!source_rejected.success);
    assert_eq!(
        source_rejected.output["error_kind"],
        "read_revision_path_mismatch"
    );
    assert_no_apply_text_edits_runner_request(&runtime, client_id).await;

    let destination_continuation = guarded_edit(
        "renamed.txt",
        rename_final_revision,
        "renamed",
        "renamed-again",
    );
    let destination_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .apply_text_edits(project, vec![destination_continuation], None)
                .await
        }
    });
    let destination_request = wait_for_patch_agent_request(&runtime, client_id).await;
    let destination_payload: Value =
        serde_json::from_str(destination_request.content.as_deref().unwrap()).unwrap();
    assert_eq!(
        destination_payload["changes"][0]["expected_sha256"],
        rename_new
    );
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: destination_request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false, "applied_count": 1, "changed": true, "would_change": true,
                    "files": [{"index":0,"kind":"edit","path":"renamed.txt","to_path":null,"old_sha256":rename_new,"new_sha256":"9".repeat(64),"changed":true,"would_change":true,"edits":[]}],
                    "changed_paths": ["renamed.txt"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    assert!(destination_task.await.unwrap().success);

    let other_client = "ate-final-revisions-other";
    register_agent(
        &runtime,
        other_client,
        None,
        RunnerCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let other_project = agent_test_project_id(other_client);
    let project_rejected = runtime
        .apply_text_edits(
            other_project,
            vec![guarded_edit(
                "edit.txt",
                edit_newer_revision,
                "newer",
                "other",
            )],
            None,
        )
        .await;
    assert!(!project_rejected.success);
    assert_eq!(
        project_rejected.output["error_kind"],
        "read_revision_project_mismatch"
    );
    assert_no_apply_text_edits_runner_request(&runtime, other_client).await;

    runtime
        .runner_registry
        .set_last_seen_for_test(client_id, chrono::Utc::now().timestamp() - 120)
        .await;
    register_agent_with_instance(
        &runtime,
        client_id,
        "inst-replacement",
        None,
        RunnerCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let owner_rejected = runtime
        .apply_text_edits(
            project,
            vec![guarded_edit(
                "edit.txt",
                edit_newer_revision,
                "newer",
                "again",
            )],
            None,
        )
        .await;
    assert!(!owner_rejected.success);
    assert_eq!(
        owner_rejected.output["error_kind"],
        "read_revision_owner_mismatch"
    );
    let replacement_request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst-replacement".to_string(),
        })
        .await
        .unwrap();
    assert!(
        replacement_request.is_none(),
        "owner mismatch must reject before dispatch to replacement Runner"
    );
}

#[tokio::test]
async fn apply_text_edits_mixed_batch_rejects_mismatched_strong_revision_before_dispatch() {
    let runtime = runtime_with_agent_project("ate-revision-mixed");
    register_agent(
        &runtime,
        "ate-revision-mixed",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-revision-mixed");
    let revision = seed_read_revision(&runtime, &project, "guarded.txt", &"d".repeat(64)).await;

    let result = runtime
        .apply_text_edits(
            project,
            vec![
                edit_change(
                    "src/lib.rs",
                    &"a".repeat(64),
                    vec![text_edit(
                        ApplyTextEditKind::ReplaceExact,
                        Some("target"),
                        Some("replacement"),
                        None,
                    )],
                ),
                ApplyFileChangeInput {
                    kind: ApplyFileChangeKind::Delete,
                    path: "other.txt".to_string(),
                    to_path: None,
                    content: None,
                    edits: Vec::new(),
                    expected_read_revision: Some(revision),
                },
            ],
            None,
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "read_revision_path_mismatch");
    assert_eq!(result.output["state_changed"], false);
    assert!(result.output.get("expected_read_revision").is_none());
    assert_eq!(result.output["recovery"]["tool"], "read_files");
    assert_eq!(
        result.output["recovery"]["arguments"]["items"][0]["path"],
        "other.txt"
    );
    assert_no_apply_text_edits_runner_request(&runtime, "ate-revision-mixed").await;
}

#[tokio::test]
async fn apply_text_edits_ambiguous_unguarded_requires_read_before_positional_retry() {
    let runtime = runtime_with_agent_project("ate-recovery");
    register_agent(
        &runtime,
        "ate-recovery",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-recovery");

    let first = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .apply_text_edits(
                    project,
                    vec![edit_change(
                        "src/lib.rs",
                        &"a".repeat(64),
                        vec![text_edit(
                            ApplyTextEditKind::ReplaceExact,
                            Some("dup"),
                            Some("SECOND"),
                            None,
                        )],
                    )],
                    None,
                )
                .await
        }
    });
    let first_request = wait_for_patch_agent_request(&runtime, "ate-recovery").await;
    let first_payload: Value =
        serde_json::from_str(first_request.content.as_deref().unwrap()).unwrap();
    assert_eq!(first_payload["recovery_metadata_version"], 1);
    assert!(first_payload["changes"][0]["expected_sha256"].is_null());
    assert!(first_payload["changes"][0]["edits"][0]["occurrence"].is_null());
    runtime.runner_registry.complete(RunnerResultRequest {
        client_id: "ate-recovery".to_string(),
        runner_instance_id: "inst".to_string(),
        request_id: first_request.request_id,
        exit_code: Some(0),
        stdout: Some(serde_json::json!({
            "changed": false,
            "error_kind": "edit_conflict",
            "state_changed": false,
            "change_index": 0,
            "edit_index": 0,
            "kind": "replace_exact",
            "path": "src/lib.rs",
            "conflict_recovery": {
                "schema_version": 1,
                "conflict_kind": "multiple_matches",
                "match_count": 2,
                "occurrence_selector_supported": true,
                "direct_retry_safe": true,
                "reread_required": false,
                "candidate_ranges": [
                    {"occurrence":1,"start_line":1,"end_line":1},
                    {"occurrence":2,"start_line":3,"end_line":3}
                ],
                "candidates_truncated": false,
                "recovery_action": "select_occurrence_or_refine_match"
            },
            "error": "legacy Runner recovery text mentioning expected_sha256 must not escape model projection"
        }).to_string()),
        stderr: Some(String::new()),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: Some(1),
        error: None,
    }).await.unwrap();
    let conflict = first.await.unwrap();
    assert!(!conflict.success);
    assert_eq!(conflict.output["error_kind"], "multiple_matches");
    assert_eq!(conflict.output["match_count"], 2);
    assert_eq!(conflict.output["recovery"]["tool"], "read_files");
    let candidates = conflict.output["candidate_ranges"].as_array().unwrap();
    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.get("occurrence").is_none()));
    for removed in [
        "conflict_recovery",
        "retry_guidance",
        "direct_retry_safe",
        "positional_retry_requires_read_revision",
    ] {
        assert!(
            conflict.output.get(removed).is_none(),
            "legacy field {removed}"
        );
    }
    let conflict_error = conflict
        .error
        .as_deref()
        .expect("model-facing conflict error");
    assert!(
        !conflict_error.contains("expected_sha256"),
        "{conflict_error}"
    );
    assert!(
        !conflict_error.contains("expected_read_revision"),
        "{conflict_error}"
    );
    assert_no_apply_text_edits_runner_request(&runtime, "ate-recovery").await;

    let mut edit = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("dup"),
        Some("SECOND"),
        None,
    );
    edit.occurrence = Some(2);
    let retry = runtime
        .apply_text_edits(
            project.clone(),
            vec![edit_change("src/lib.rs", &"a".repeat(64), vec![edit])],
            None,
        )
        .await;
    assert!(!retry.success);
    assert_eq!(retry.output["state_changed"], false);
    assert_eq!(retry.output["execution_state"], "not_started");
    assert_eq!(retry.output["error_kind"], "missing_read_revision");
    assert_eq!(retry.output["change_index"], 0);
    assert_eq!(retry.output["edit_index"], 0);
    assert_eq!(retry.output["path"], "src/lib.rs");
    assert_eq!(retry.output["recovery"]["tool"], "read_files");
    assert_eq!(retry.output["recovery"]["arguments"]["project"], project);
    assert_eq!(
        retry.output["recovery"]["arguments"]["items"],
        serde_json::json!([{"path":"src/lib.rs"}])
    );
    assert!(retry
        .error
        .as_deref()
        .is_some_and(|error| error.contains("expected_read_revision is required")));
    assert_no_apply_text_edits_runner_request(&runtime, "ate-recovery").await;
}

#[tokio::test]
async fn apply_text_edits_without_occurrence_unique_match_queues_and_succeeds() {
    let runtime = runtime_with_agent_project("ate-no-occurrence-unique");
    register_agent(
        &runtime,
        "ate-no-occurrence-unique",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-no-occurrence-unique");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .apply_text_edits(
                    project,
                    vec![edit_change(
                        "src/lib.rs",
                        &"a".repeat(64),
                        vec![text_edit(
                            ApplyTextEditKind::ReplaceExact,
                            Some("foo"),
                            Some("bar"),
                            None,
                        )],
                    )],
                    None,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "ate-no-occurrence-unique").await;
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert!(payload["changes"][0]["edits"][0]["occurrence"].is_null());
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "ate-no-occurrence-unique".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false, "applied_count": 1, "changed": true,
                    "would_change": true,
                    "files": [{
                        "index": 0, "kind": "edit", "path": "src/lib.rs", "to_path": null,
                        "old_sha256": "a".repeat(64), "new_sha256": "b".repeat(64),
                        "changed": true, "would_change": true, "edits": []
                    }],
                    "changed_paths": ["src/lib.rs"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    assert!(task.await.unwrap().success);
}

#[tokio::test]
async fn apply_text_edits_without_occurrence_ambiguous_match_fails_closed() {
    let runtime = runtime_with_agent_project("ate-no-occurrence-ambiguous");
    register_agent(
        &runtime,
        "ate-no-occurrence-ambiguous",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-no-occurrence-ambiguous");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .apply_text_edits(
                    project,
                    vec![edit_change(
                        "src/lib.rs",
                        &"a".repeat(64),
                        vec![text_edit(
                            ApplyTextEditKind::ReplaceExact,
                            Some("dup"),
                            Some("x"),
                            None,
                        )],
                    )],
                    None,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "ate-no-occurrence-ambiguous").await;
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "ate-no-occurrence-ambiguous".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "changed": false,
                    "error_kind": "edit_conflict",
                    "state_changed": false,
                    "error": "Rejected transactional file batch: exact match matched 2 times; ambiguous. No files were modified."
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert!(result.output.get("conflict_recovery").is_none());
}

#[tokio::test]
async fn apply_text_edits_overlap_projects_bounded_resolved_ranges_without_bodies() {
    let client_id = "ate-overlap-ranges";
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .apply_text_edits(
                    project,
                    vec![edit_change(
                        "src/lib.rs",
                        &"a".repeat(64),
                        vec![
                            text_edit(
                                ApplyTextEditKind::ReplaceExact,
                                Some("abc"),
                                Some("SECRET_A"),
                                None,
                            ),
                            text_edit(
                                ApplyTextEditKind::ReplaceExact,
                                Some("cde"),
                                Some("SECRET_B"),
                                None,
                            ),
                        ],
                    )],
                    None,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, client_id).await;
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "changed": false,
                    "state_changed": false,
                    "execution_state": "not_started",
                    "error_kind": "edit_conflict",
                    "change_index": 0,
                    "edit_index": 1,
                    "kind": "replace_exact",
                    "path": "src/lib.rs",
                    "conflict_recovery": {
                        "schema_version": 1,
                        "conflict_kind": "overlapping_edits",
                        "occurrence_selector_supported": false,
                        "direct_retry_safe": true,
                        "reread_required": false,
                        "conflicting_edit_indices": [0, 1],
                        "conflicting_edit_ranges": [
                            {"edit_index":0,"start_line":492,"end_line":492,"source":"SECRET_SOURCE"},
                            {"edit_index":1,"start_line":492,"end_line":493,"replacement":"SECRET_REPLACEMENT"}
                        ],
                        "recovery_action": "refine_edit_batch"
                    },
                    "error": "Runner private overlap detail SECRET_RAW"
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["error_kind"], "overlapping_edits");
    assert_eq!(
        result.output["conflicting_edit_indices"],
        serde_json::json!([0, 1])
    );
    assert_eq!(
        result.output["conflicting_edit_ranges"],
        serde_json::json!([
            {"edit_index":0,"start_line":492,"end_line":492},
            {"edit_index":1,"start_line":492,"end_line":493}
        ])
    );
    let serialized = serde_json::to_string(&result).unwrap();
    for secret in [
        "SECRET_A",
        "SECRET_B",
        "SECRET_SOURCE",
        "SECRET_REPLACEMENT",
        "SECRET_RAW",
    ] {
        assert!(
            !serialized.contains(secret),
            "leaked {secret}: {serialized}"
        );
    }
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("planned exact edit ranges overlap")));
    let output_schema = crate::tool_runtime::registry::output_schema_for_tool("apply_text_edits");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &output_schema,
    )
    .unwrap_or_else(|error| panic!("overlap range projection must match output schema: {error}"));
}

#[tokio::test]
async fn apply_text_edits_server_preflight_reports_exact_failed_edit() {
    let runtime = test_runtime();
    let result = runtime
        .apply_text_edits(
            "agent:unused:unused".to_string(),
            vec![
                edit_change(
                    "src/first.rs",
                    &"a".repeat(64),
                    vec![text_edit(
                        ApplyTextEditKind::ReplaceExact,
                        Some("first"),
                        Some("FIRST"),
                        None,
                    )],
                ),
                edit_change(
                    "src/second.rs",
                    &"b".repeat(64),
                    vec![text_edit(
                        ApplyTextEditKind::ReplaceExact,
                        None,
                        Some("SECOND"),
                        None,
                    )],
                ),
            ],
            None,
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "invalid_edit");
    assert_eq!(result.output["change_index"], 1);
    assert_eq!(result.output["edit_index"], 0);
    assert_eq!(result.output["kind"], "replace_exact");
    assert_eq!(result.output["path"], "src/second.rs");
    assert!(result.output.get("retry_guidance").is_none());
    let output_schema = crate::tool_runtime::registry::output_schema_for_tool("apply_text_edits");
    let serialized = serde_json::to_value(&result).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serialized,
        &output_schema,
    )
    .unwrap_or_else(|error| panic!("structured preflight must match output schema: {error}"));
    assert!(result.output.get("conflict_recovery").is_none());
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("No files were modified"));
}

#[tokio::test]
async fn apply_text_edits_empty_batch_proves_preflight_no_effect_without_fake_indices() {
    let runtime = test_runtime();
    let result = runtime
        .apply_text_edits("agent:unused:unused".to_string(), Vec::new(), None)
        .await;
    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "empty_batch");
    for field in ["change_index", "edit_index", "kind", "path"] {
        assert!(
            result.output.get(field).is_none(),
            "{field} must remain absent"
        );
    }
    assert!(result.output.get("retry_guidance").is_none());
}

#[tokio::test]
async fn apply_text_edits_duplicate_anchor_advisory_survives_runner_projection() {
    for dry_run in [true, false] {
        let runtime = runtime_with_agent_project("ate-advisory");
        register_agent(
            &runtime,
            "ate-advisory",
            None,
            RunnerCapabilities {
                file_write: true,
                apply_text_edit_local_guard_without_sha: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id("ate-advisory");
        let revision =
            seed_read_revision(&runtime, &project, "EDIT_PROBE.txt", &"a".repeat(64)).await;
        let mut change = edit_change(
            "EDIT_PROBE.txt",
            "unused",
            vec![text_edit(
                ApplyTextEditKind::InsertBefore,
                None,
                Some("anchor"),
                Some("anchor"),
            )],
        );
        change.expected_read_revision = Some(revision);
        let runtime_for_task = runtime.clone();
        let task = tokio::spawn(async move {
            runtime_for_task
                .apply_text_edits(project, vec![change], Some(dry_run))
                .await
        });
        let req = wait_for_patch_agent_request(&runtime, "ate-advisory").await;
        let warning = "Inserted text already contains the full anchor at the insertion boundary; the original anchor remains.";
        runtime
            .runner_registry
            .complete(RunnerResultRequest {
                client_id: "ate-advisory".to_string(),
                runner_instance_id: "inst".to_string(),
                request_id: req.request_id,
                exit_code: Some(0),
                stdout: Some(
                    serde_json::json!({
                        "dry_run": dry_run, "applied_count": 1,
                        "changed": !dry_run, "state_changed": !dry_run,
                        "execution_state": "completed", "would_change": true,
                        "files": [{
                            "index": 0, "kind": "edit", "path": "EDIT_PROBE.txt", "to_path": null,
                            "old_sha256": "a".repeat(64), "new_sha256": "b".repeat(64),
                            "changed": !dry_run, "would_change": true,
                            "edits": [{"index": 0, "kind": "insert_before", "old_start_line": 1,
                                "old_end_line": 1, "new_line_count": 1, "warning": warning}]
                        }],
                        "changed_paths": if dry_run { vec![] } else { vec!["EDIT_PROBE.txt"] }
                    })
                    .to_string(),
                ),
                stderr: Some(String::new()),
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["files"][0]["edits"][0]["warning"], warning);
        assert_eq!(result.output["changed"], !dry_run);
        assert_eq!(result.output["state_changed"], !dry_run);
        assert_eq!(result.output["execution_state"], "completed");
    }
}

#[tokio::test]
async fn apply_text_edits_dry_run_does_not_write() {
    let runtime = runtime_with_agent_project("ate-dry");
    register_agent(
        &runtime,
        "ate-dry",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-dry");
    let before_revision =
        seed_read_revision(&runtime, &project, "EDIT_PROBE.txt", &"a".repeat(64)).await;
    let mut dry_run_change = edit_change(
        "EDIT_PROBE.txt",
        "unused",
        vec![text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("old"),
            Some("new"),
            None,
        )],
    );
    dry_run_change.expected_read_revision = Some(before_revision);

    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .apply_text_edits(project_for_task, vec![dry_run_change], Some(true))
            .await
    });

    let req = wait_for_patch_agent_request(&runtime, "ate-dry").await;
    assert_eq!(req.kind, "file_apply_text_edits");
    // The payload carries dry_run and the edits.
    let payload: Value = serde_json::from_str(req.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["changes"][0]["kind"], "edit");
    assert_eq!(payload["changes"][0]["edits"][0]["kind"], "replace_exact");
    assert!(payload["changes"][0]["edits"][0]
        .get("line_scope")
        .is_none());

    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "ate-dry".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": true,
                    "applied_count": 1,
                    "changed": false,
                    "would_change": true,
                    "files": [{
                        "index": 0, "kind": "edit", "path": "EDIT_PROBE.txt", "to_path": null,
                        "old_sha256": "a".repeat(64), "new_sha256": "b".repeat(64),
                        "changed": false, "would_change": true, "edits": []
                    }],
                    "changed_paths": ["EDIT_PROBE.txt"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["dry_run"], true);
    assert_eq!(result.output["would_change"], true);
    assert_eq!(result.output["changed"], false);
    assert!(result.output["files"][0]["read_revision"].is_null());
    assert!(result.output["files"][0].get("old_sha256").is_some());
    assert!(result.output["files"][0].get("new_sha256").is_some());
    let next_revision =
        seed_read_revision(&runtime, &project, "after-dry-run.txt", &"c".repeat(64)).await;
    assert_eq!(
        next_revision,
        before_revision + 1,
        "dry-run must not mutate the read revision registry"
    );
}

#[tokio::test]
async fn apply_text_edits_scoped_request_fails_closed_before_enqueue_without_capability() {
    let runtime = runtime_with_agent_project("ate-scope-off");
    register_agent(
        &runtime,
        "ate-scope-off",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            apply_text_edit_occurrence: true,
            apply_text_edit_line_scope: false,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-scope-off");
    let revision = seed_read_revision(&runtime, &project, "src/lib.rs", &"a".repeat(64)).await;
    let mut change = edit_change(
        "src/lib.rs",
        &"a".repeat(64),
        vec![scoped_text_edit(
            text_edit(
                ApplyTextEditKind::ReplaceExact,
                Some("dup"),
                Some("x"),
                None,
            ),
            40,
            60,
        )],
    );
    change.expected_read_revision = Some(revision);
    let result = runtime.apply_text_edits(project, vec![change], None).await;
    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert_eq!(result.output["capability"], "apply_text_edit_line_scope");
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("No files were modified"));
    assert!(runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: "ate-scope-off".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn apply_text_edits_read_only_session_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let session = runtime.sessions.start_session_with_guards(
        Some("demo".to_string()),
        Some("read only".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );

    let result = runtime
        .dispatch(ToolCall::ApplyTextEdits {
            project: "demo".to_string(),
            changes: vec![edit_change(
                "should-not-exist.txt",
                &"a".repeat(64),
                vec![text_edit(
                    ApplyTextEditKind::ReplaceExact,
                    Some("old"),
                    Some("new"),
                    None,
                )],
            )],
            dry_run: None,
            session_id: Some(session.session_id.clone()),
        })
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "session_guard_denied");
    assert_eq!(result.output["guard"], "deny_write_tools");
    assert_eq!(result.output["mode"], "read_only");
    assert!(!tmp.path().join("should-not-exist.txt").exists());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.failed, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "apply_text_edits");
    assert_eq!(event.status.as_deref(), Some("failed"));
    assert_eq!(event.error_kind.as_deref(), Some("session_guard_denied"));
}

#[tokio::test]
async fn apply_text_edits_session_event_summary() {
    let runtime = runtime_with_agent_project("ate-sess");
    register_agent(
        &runtime,
        "ate-sess",
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-sess");
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("apply_text_edits session".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );

    let bootstrap = auth_context(None, true);
    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let session_id = session.session_id.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .dispatch_with_auth(
                ToolCall::ApplyTextEdits {
                    project: project_for_task,
                    changes: vec![edit_change(
                        "src/lib.rs",
                        &"a".repeat(64),
                        vec![text_edit(
                            ApplyTextEditKind::ReplaceExact,
                            Some("SECRET_OLD_BLOCK"),
                            Some("SECRET_NEW_BLOCK"),
                            None,
                        )],
                    )],
                    dry_run: None,
                    session_id: Some(session_id),
                },
                Some(&bootstrap),
            )
            .await
    });

    let req = wait_for_patch_agent_request(&runtime, "ate-sess").await;
    assert_eq!(req.kind, "file_apply_text_edits");
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "ate-sess".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false,
                    "applied_count": 1,
                    "changed": true,
                    "would_change": true,
                    "files": [{
                        "index": 0, "kind": "edit", "path": "src/lib.rs", "to_path": null,
                        "old_sha256": "a".repeat(64), "new_sha256": "b".repeat(64),
                        "changed": true, "would_change": true, "edits": []
                    }],
                    "changed_paths": ["src/lib.rs"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["changed_paths"][0], "src/lib.rs");
    assert!(result.output["files"][0].get("old_sha256").is_none());
    assert!(result.output["files"][0].get("new_sha256").is_none());
    assert!(result.output["files"][0]["read_revision"]
        .as_u64()
        .is_some());
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &crate::tool_runtime::registry::output_schema_for_tool("apply_text_edits"),
    )
    .unwrap();

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.succeeded, 1);
    assert_eq!(summary.counts.write_like, 1);
    let event = finished_event(&summary, "apply_text_edits");
    assert_eq!(event.status.as_deref(), Some("succeeded"));
    // changed_paths recorded from the input path.
    assert!(event.changed_paths.iter().any(|p| p == "src/lib.rs"));
    // input_summary lives on the tool_call_started event; it must NOT leak
    // old_text/new_text contents.
    let started = summary
        .events
        .iter()
        .rev()
        .find(|e| e.kind == "tool_call_started" && e.tool_name == "apply_text_edits")
        .expect("started event for apply_text_edits");
    let input_summary = started
        .input_summary
        .as_ref()
        .expect("input_summary present on started event");
    let summary_str = serde_json::to_string(input_summary).unwrap();
    assert!(summary_str.contains("change_count"));
    assert!(summary_str.contains("src/lib.rs"));
    assert!(
        !summary_str.contains("SECRET_OLD_BLOCK"),
        "input_summary must not leak old_text content: {}",
        summary_str
    );
    assert!(
        !summary_str.contains("SECRET_NEW_BLOCK"),
        "input_summary must not leak new_text content: {}",
        summary_str
    );
    assert_eq!(input_summary["expected_read_revision_count"], 0);
}

fn assert_apply_text_edits_outcome_unknown(result: &ToolResult) {
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert!(result.output["state_changed"].is_null());
    assert_eq!(result.output["error_kind"], "outcome_unknown");
    assert_eq!(result.output["failure_kind"], "outcome_unknown");
    assert!(result.output.get("recovery_action").is_none());
    assert_eq!(result.output["recovery_kind"], "reobserve");
    assert!(result.output.get("conflict_recovery").is_none());
    let error = result.error.as_deref().expect("model-facing uncertainty");
    assert!(error.contains("outcome is unknown"), "{error}");
    assert!(error.contains("Inspect current workspace state"), "{error}");
    assert!(!error.contains("No files were modified"), "{error}");

    let schema = crate::tool_runtime::registry::output_schema_for_tool("apply_text_edits");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &schema,
    )
    .unwrap_or_else(|schema_error| {
        panic!("apply_text_edits outcome_unknown result must match output schema: {schema_error}")
    });
}

async fn apply_text_edits_effect_runtime(client_id: &str) -> (ToolRuntime, String) {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_local_guard_without_sha: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    (runtime, project)
}

fn one_effect_change() -> Vec<ApplyFileChangeInput> {
    vec![edit_change(
        "src/lib.rs",
        &"a".repeat(64),
        vec![text_edit(
            ApplyTextEditKind::ReplaceExact,
            Some("old"),
            Some("new"),
            None,
        )],
    )]
}

#[tokio::test]
async fn apply_text_edits_dropped_waiter_after_dispatch_is_outcome_unknown() {
    let (runtime, project) = apply_text_edits_effect_runtime("ate-drop").await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .apply_text_edits(project, one_effect_change(), None)
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "ate-drop").await;
    assert_eq!(request.kind, "file_apply_text_edits");
    assert_eq!(
        runtime
            .runner_registry
            .cancel_request_dispatch_state(&request.request_id)
            .await,
        Some(true)
    );

    assert_apply_text_edits_outcome_unknown(&task.await.unwrap());
}

#[tokio::test]
async fn apply_text_edits_malformed_success_payload_is_outcome_unknown() {
    let (runtime, project) = apply_text_edits_effect_runtime("ate-malformed").await;
    let before_revision =
        seed_read_revision(&runtime, &project, "before-malformed.txt", &"a".repeat(64)).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .apply_text_edits(project, one_effect_change(), None)
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "ate-malformed").await;
    complete_patch_agent_request(&runtime, "ate-malformed", &request.request_id, 0, "{}", "").await;

    assert_apply_text_edits_outcome_unknown(&task.await.unwrap());
    let after_revision =
        seed_read_revision(&runtime, &project, "after-malformed.txt", &"b".repeat(64)).await;
    assert_eq!(
        after_revision,
        before_revision + 1,
        "invalid Runner success metadata must not mint a read revision"
    );
}

#[tokio::test]
async fn apply_text_edits_complete_rollback_is_known_completed_no_effect() {
    let (runtime, project) = apply_text_edits_effect_runtime("ate-rollback").await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .apply_text_edits(project, one_effect_change(), None)
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "ate-rollback").await;
    complete_patch_agent_request(
        &runtime,
        "ate-rollback",
        &request.request_id,
        0,
        r#"{"changed":false,"state_changed":false,"rollback_complete":true,"error_kind":"transaction_failed","error":"Transactional file batch failed and was rolled back: simulated failure"}"#,
        "",
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["changed"], false);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["rollback_complete"], true);
    assert_ne!(result.output["error_kind"], "outcome_unknown");
    assert!(result.output.get("recovery_kind").is_none());
    assert!(result
        .error
        .as_deref()
        .is_some_and(|error| error.contains("was rolled back")));
}

#[tokio::test]
async fn apply_text_edits_host_structural_schema_accepts_but_runtime_rejects_unguarded_position() {
    let client = "ate-structural-schema";
    let runtime = runtime_with_agent_project(client);
    register_agent(
        &runtime,
        client,
        None,
        RunnerCapabilities {
            file_write: true,
            apply_text_edit_occurrence: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client);
    let specs = registered_tool_specs();
    let schema = &spec_named(&specs, "apply_text_edits").input_schema;
    for selector in [
        serde_json::json!({"occurrence":2}),
        serde_json::json!({"line_scope":{"start_line":2,"end_line":2}}),
    ] {
        let mut edit =
            serde_json::json!({"kind":"replace_exact","old_text":"dup","new_text":"changed"});
        edit.as_object_mut()
            .unwrap()
            .extend(selector.as_object().unwrap().clone());
        let arguments = serde_json::json!({"project":project,"changes":[
            {"kind":"create","path":"must-not-exist.txt","content":"no partial writes"},
            {"kind":"edit","path":"src/lib.rs","edits":[edit]}
        ]});
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&arguments, schema)
            .unwrap();
        let changes = parsed_apply_text_edits_changes(arguments);
        let rejected = runtime
            .apply_text_edits(project.clone(), changes, None)
            .await;
        assert!(!rejected.success);
        assert_eq!(rejected.output["state_changed"], false);
        assert!(rejected
            .error
            .as_deref()
            .unwrap()
            .contains("expected_read_revision"));
        assert_no_apply_text_edits_runner_request(&runtime, client).await;
    }
}

#[tokio::test]
async fn apply_text_edits_path_overlap_identifies_first_and_current_changes() {
    let edit = |path: &str| serde_json::json!({"path":path,"old_text":"A","new_text":"B"});
    let rename = |path: &str, to_path: &str| {
        serde_json::json!({
            "kind":"rename","path":path,"to_path":to_path,"expected_read_revision":1
        })
    };
    for (changes, indices, path) in [
        // Nonadjacent source/source, source/destination, destination/source,
        // destination/destination and same-change source/destination conflicts.
        (
            vec![edit("a.rs"), edit("b.rs"), edit("a.rs")],
            [0, 2],
            "a.rs",
        ),
        (vec![edit("a.rs"), rename("b.rs", "a.rs")], [0, 1], "a.rs"),
        (vec![rename("a.rs", "b.rs"), edit("b.rs")], [0, 1], "b.rs"),
        (
            vec![rename("a.rs", "c.rs"), rename("b.rs", "c.rs")],
            [0, 1],
            "c.rs",
        ),
        (vec![rename("a.rs", "a.rs")], [0, 0], "a.rs"),
        // Sequential replacements are rejected, never automatically coalesced.
        (
            vec![
                edit("a.rs"),
                serde_json::json!({"path":"a.rs","old_text":"B","new_text":"C"}),
            ],
            [0, 1],
            "a.rs",
        ),
    ] {
        let runtime = test_runtime();
        let result = runtime
            .apply_text_edits(
                "agent:unused:unused".to_string(),
                parsed_apply_text_edits_changes(
                    serde_json::json!({"project":"agent:unused:unused","changes":changes}),
                ),
                None,
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "path_overlap");
        assert_eq!(
            result.output["path_conflict_change_indices"],
            serde_json::json!(indices)
        );
        assert_eq!(result.output["change_index"], indices[1]);
        assert_eq!(result.output["path"], path);
        assert_eq!(result.output["execution_state"], "not_started");
        assert_eq!(result.output["state_changed"], false);
        for absent in [
            "recovery",
            "retry_guidance",
            "recovery_action",
            "old_text",
            "new_text",
        ] {
            assert!(result.output.get(absent).is_none(), "unexpected {absent}");
        }
        crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
            &serde_json::to_value(&result).unwrap(),
            &crate::tool_runtime::registry::output_schema_for_tool("apply_text_edits"),
        )
        .unwrap();
    }
}
