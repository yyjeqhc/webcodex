//! apply_text_edits tests for tool_runtime.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellClientCapabilities,
};
use serde_json::Value;

#[test]
fn apply_text_edits_occurrence_and_recovery_schemas_are_model_visible() {
    let specs = registered_tool_specs();
    let spec = spec_named(&specs, "apply_text_edits");
    let edit_variants = spec.input_schema["properties"]["changes"]["items"]["oneOf"][0]
        ["properties"]["edits"]["items"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(edit_variants.len(), 4);
    for variant in edit_variants {
        assert_eq!(variant["properties"]["occurrence"]["type"], "integer");
        assert_eq!(variant["properties"]["occurrence"]["minimum"], 1);
        assert!(!variant["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("occurrence")));
    }
    let output = &spec.output_schema["properties"]["output"]["properties"]["conflict_recovery"];
    assert_eq!(output["properties"]["schema_version"]["const"], 1);
    assert_eq!(output["properties"]["candidate_ranges"]["maxItems"], 8);
    assert_eq!(output["properties"]["direct_retry_safe"]["type"], "boolean");
    assert_eq!(output["properties"]["reread_required"]["type"], "boolean");
    assert_eq!(
        output["properties"]["expected_sha256"]["pattern"],
        "^[a-f0-9]{64}$"
    );
    assert_eq!(
        output["properties"]["current_sha256"]["pattern"],
        "^[a-f0-9]{64}$"
    );
    let output_properties = &spec.output_schema["properties"]["output"]["properties"];
    assert!(output_properties["change_index"]["anyOf"].is_array());
    assert!(output_properties["edit_index"]["anyOf"].is_array());
    assert_eq!(output_properties["state_changed"]["type"], "boolean");
    assert_eq!(output_properties["retry_guidance"]["type"], "string");
    assert!(output["properties"]["conflict_kind"]["enum"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("multiple_matches")));

    let sha_conflict = serde_json::json!({
        "success": false,
        "output": {
            "state_changed": false,
            "error_kind": "sha256_conflict",
            "change_index": 0,
            "kind": "edit",
            "path": "src/lib.rs",
            "retry_guidance": "reread the file",
            "conflict_recovery": {
                "schema_version": 1,
                "conflict_kind": "sha256_mismatch",
                "occurrence_selector_supported": false,
                "direct_retry_safe": false,
                "reread_required": true,
                "expected_sha256": "a".repeat(64),
                "current_sha256": "b".repeat(64),
                "recovery_action": "reread_file"
            }
        },
        "error": "sha256 mismatch"
    });
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &sha_conflict,
        &spec.output_schema,
    )
    .unwrap_or_else(|error| panic!("sha conflict recovery must match output schema: {error}"));

    let openapi = crate::openapi::build_openapi_spec();
    let occurrence = &openapi["components"]["schemas"]["ToolCallRequest"]["properties"]["changes"]
        ["items"]["properties"]["edits"]["items"]["properties"]["occurrence"];
    assert_eq!(occurrence["type"], "integer");
    assert_eq!(occurrence["minimum"], 1);
    assert!(spec.description.contains("occurrence"));
    assert!(spec.description.chars().count() <= 300);
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

async fn assert_no_apply_text_edits_runner_request(runtime: &ToolRuntime, client_id: &str) {
    let request = runtime
        .shell_clients
        .poll(ShellAgentPollRequest {
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap();
    assert!(
        request.is_none(),
        "unexpected hidden apply_text_edits Runner request"
    );
}

#[tokio::test]
async fn apply_text_edits_conflict_then_same_sha_occurrence_retry_needs_no_hidden_read() {
    let runtime = runtime_with_agent_project("ate-recovery");
    register_agent(
        &runtime,
        "ate-recovery",
        None,
        ShellClientCapabilities {
            file_write: true,
            apply_text_edit_occurrence: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-recovery");
    let sha = "a".repeat(64);

    let first = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let sha = sha.clone();
        async move {
            runtime
                .apply_text_edits(
                    project,
                    vec![edit_change(
                        "src/lib.rs",
                        &sha,
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
    assert!(first_payload["changes"][0]["edits"][0]["occurrence"].is_null());
    runtime.shell_clients.complete(ShellAgentResultRequest {
        client_id: "ate-recovery".to_string(),
        agent_instance_id: "inst".to_string(),
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
            "error": "Rejected transactional file batch: exact match is ambiguous. No files were modified. Retry guidance: choose an advertised occurrence or refine the exact match; reuse the same expected_sha256 unless you reread or observe a changed file."
        }).to_string()),
        stderr: Some(String::new()),
        duration_ms: Some(1),
        error: None,
    }).await.unwrap();
    let conflict = first.await.unwrap();
    assert!(!conflict.success);
    assert_eq!(conflict.output["conflict_recovery"]["match_count"], 2);
    assert_eq!(
        conflict.output["conflict_recovery"]["direct_retry_safe"],
        true
    );
    assert_eq!(
        conflict.output["conflict_recovery"]["reread_required"],
        false
    );
    let conflict_error = conflict
        .error
        .as_deref()
        .expect("model-facing conflict error");
    assert!(conflict_error.contains("choose an advertised occurrence"));
    assert!(conflict_error.contains("refine the exact match"));
    assert!(conflict_error.contains("same expected_sha256"));
    assert!(!conflict_error.contains("read the file again"));
    assert!(!conflict_error.contains("read this file again"));
    let output_schema = crate::tool_runtime::registry::output_schema_for_tool("apply_text_edits");
    let serialized_conflict = serde_json::to_value(&conflict).unwrap();
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serialized_conflict,
        &output_schema,
    )
    .unwrap_or_else(|error| panic!("structured conflict must match output schema: {error}"));
    assert_no_apply_text_edits_runner_request(&runtime, "ate-recovery").await;

    let second = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let sha = sha.clone();
        async move {
            let mut edit = text_edit(
                ApplyTextEditKind::ReplaceExact,
                Some("dup"),
                Some("SECOND"),
                None,
            );
            edit.occurrence = Some(2);
            runtime
                .apply_text_edits(
                    project,
                    vec![edit_change("src/lib.rs", &sha, vec![edit])],
                    None,
                )
                .await
        }
    });
    let second_request = wait_for_patch_agent_request(&runtime, "ate-recovery").await;
    let second_payload: Value =
        serde_json::from_str(second_request.content.as_deref().unwrap()).unwrap();
    assert_eq!(second_payload["changes"][0]["expected_sha256"], sha);
    assert_eq!(second_payload["changes"][0]["edits"][0]["occurrence"], 2);
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-recovery".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: second_request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false, "applied_count": 1, "changed": true,
                    "would_change": true, "files": [], "changed_paths": ["src/lib.rs"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let success = second.await.unwrap();
    assert!(success.success, "{:?}", success.error);
    assert_no_apply_text_edits_runner_request(&runtime, "ate-recovery").await;
}

#[tokio::test]
async fn apply_text_edits_legacy_runner_unique_match_occurrence_out_of_range_queues_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    let path = tmp.path().join("src/lib.rs");
    std::fs::write(&path, "foo\n").unwrap();
    let sha = files::sha256_hex_bytes(&std::fs::read(&path).unwrap());
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "ate-legacy-occurrence", "agent-proj", tmp.path())
            .await;
    let mut edit = text_edit(
        ApplyTextEditKind::ReplaceExact,
        Some("foo"),
        Some("bar"),
        None,
    );
    edit.occurrence = Some(2);

    let result = runtime
        .apply_text_edits(
            project,
            vec![edit_change("src/lib.rs", &sha, vec![edit])],
            None,
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "agent_capability_unavailable");
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert_eq!(result.output["capability"], "apply_text_edit_occurrence");
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("upgrade the Runner"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "foo\n");
    assert_no_apply_text_edits_runner_request(&runtime, "ate-legacy-occurrence").await;
}

#[tokio::test]
async fn apply_text_edits_legacy_runner_no_occurrence_still_queues_and_succeeds() {
    let runtime = runtime_with_agent_project("ate-legacy-unique");
    register_agent(
        &runtime,
        "ate-legacy-unique",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-legacy-unique");
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
    let request = wait_for_patch_agent_request(&runtime, "ate-legacy-unique").await;
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert!(payload["changes"][0]["edits"][0]["occurrence"].is_null());
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-legacy-unique".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                serde_json::json!({
                    "dry_run": false, "applied_count": 1, "changed": true,
                    "would_change": true, "files": [], "changed_paths": ["src/lib.rs"]
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    assert!(task.await.unwrap().success);
}

#[tokio::test]
async fn apply_text_edits_legacy_runner_ambiguous_no_occurrence_keeps_legacy_fail_closed() {
    let runtime = runtime_with_agent_project("ate-legacy-ambiguous");
    register_agent(
        &runtime,
        "ate-legacy-ambiguous",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-legacy-ambiguous");
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
    let request = wait_for_patch_agent_request(&runtime, "ate-legacy-ambiguous").await;
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-legacy-ambiguous".to_string(),
            agent_instance_id: "inst".to_string(),
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
    assert!(result.output["retry_guidance"]
        .as_str()
        .unwrap()
        .contains("retry the whole batch"));
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
    assert!(result.output["retry_guidance"].is_string());
}

#[tokio::test]
async fn apply_text_edits_dry_run_does_not_write() {
    let runtime = runtime_with_agent_project("ate-dry");
    register_agent(
        &runtime,
        "ate-dry",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("ate-dry");

    let runtime_for_task = runtime.clone();
    let project_for_task = project.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .apply_text_edits(
                project_for_task,
                vec![edit_change(
                    "EDIT_PROBE.txt",
                    &"a".repeat(64),
                    vec![text_edit(
                        ApplyTextEditKind::ReplaceExact,
                        Some("old"),
                        Some("new"),
                        None,
                    )],
                )],
                Some(true),
            )
            .await
    });

    let req = wait_for_patch_agent_request(&runtime, "ate-dry").await;
    assert_eq!(req.kind, "file_apply_text_edits");
    // The payload carries dry_run and the edits.
    let payload: Value = serde_json::from_str(req.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["changes"][0]["kind"], "edit");
    assert_eq!(payload["changes"][0]["edits"][0]["kind"], "replace_exact");

    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-dry".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"dry_run\":true,\"applied_count\":1,\"changed\":false,\
                     \"would_change\":true,\"files\":[],\"changed_paths\":[\"EDIT_PROBE.txt\"]}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
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
        ShellClientCapabilities {
            file_write: true,
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
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "ate-sess".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(
                "{\"dry_run\":false,\"applied_count\":1,\"changed\":true,\
                     \"would_change\":true,\"files\":[{\"index\":0,\"kind\":\"edit\",\"path\":\"src/lib.rs\"}],\
                     \"changed_paths\":[\"src/lib.rs\"]}"
                    .to_string(),
            ),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["changed"], true);
    assert_eq!(result.output["changed_paths"][0], "src/lib.rs");

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
    assert_eq!(input_summary["expected_sha256_count"], 1);
}
