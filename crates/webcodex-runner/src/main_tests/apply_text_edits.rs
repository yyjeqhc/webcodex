use super::*;

fn apply_text_edits_request(
    cwd: &Path,
    path: &str,
    mut payload: serde_json::Value,
) -> RunnerRequest {
    if payload.get("changes").is_none() {
        let expected_sha256 = payload
            .get("expected_file_sha256")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                sha256_hex_bytes(&std::fs::read(cwd.join(path)).unwrap_or_default())
            });
        payload = serde_json::json!({
            "dry_run": payload.get("dry_run").cloned().unwrap_or(serde_json::Value::Bool(false)),
            "changes": [{
                "kind": "edit",
                "path": path,
                "expected_sha256": expected_sha256,
                "edits": payload.get("edits").cloned().unwrap_or_else(|| serde_json::json!([]))
            }]
        });
    }
    RunnerRequest {
        login: false,
        shell: None,
        request_id: "req-apply-text-edits".to_string(),
        client_id: "agent-1".to_string(),
        kind: "file_apply_text_edits".to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: Some(path.to_string()),
        content: Some(payload.to_string()),
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 30,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        plugin_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

#[test]
fn bulk_exact_replaces_original_ranges_once_with_compact_review() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("bulk.txt");
    std::fs::write(&file, "αα OLD OLD\r\nOLD\r\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "bulk.txt",
            serde_json::json!({"changes":[{"kind":"edit","path":"bulk.txt","expected_sha256":hash,
            "edits":[{"kind":"replace_exact","old_text":"OLD","new_text":"OLD+OLD","expected_match_count":3},
                {"kind":"replace_exact","old_text":"αα","new_text":"β","occurrence":1}]}]}),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(out["change_summary"]["logical_edits"], 2);
    assert_eq!(out["change_summary"]["changed_files"], 1);
    assert_eq!(out["change_summary"]["resolved_matches"], 4);
    assert_eq!(out["files"][0]["edits"].as_array().unwrap().len(), 2);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "β OLD+OLD OLD+OLD\r\nOLD+OLD\r\n"
    );
}

#[test]
fn bulk_exact_adjacent_matches_do_not_rematch_inserted_text() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("adjacent.txt");
    std::fs::write(&file, "aaaa").unwrap();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "adjacent.txt",
            serde_json::json!({"changes":[{"kind":"edit","path":"adjacent.txt",
            "expected_sha256":sha256_hex_bytes(b"aaaa"),"edits":[
            {"kind":"replace_exact","old_text":"a","new_text":"aa","expected_match_count":4}]}]}),
        ),
    ));
    assert_eq!(out["change_summary"]["resolved_matches"], 4);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "aaaaaaaa");
}

#[test]
fn bulk_exact_mismatch_and_generated_overlap_leave_whole_batch_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let first = tmp.path().join("first.txt");
    let second = tmp.path().join("second.txt");
    let removed = tmp.path().join("removed.txt");
    std::fs::write(&first, "keep").unwrap();
    std::fs::write(&second, "OLD OLD OLD").unwrap();
    std::fs::write(&removed, "retain").unwrap();
    let first_hash = sha256_hex_bytes(&std::fs::read(&first).unwrap());
    let removed_hash = sha256_hex_bytes(&std::fs::read(&removed).unwrap());
    let hash = sha256_hex_bytes(&std::fs::read(&second).unwrap());
    for expected in [2, 4] {
        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "first.txt",
                serde_json::json!({"recovery_metadata_version":1,"changes":[
                {"kind":"create","path":format!("created-{expected}.txt"),"content":"new"},
                {"kind":"rename","path":"first.txt","to_path":format!("renamed-{expected}.txt"),"expected_sha256":first_hash},
                {"kind":"delete","path":"removed.txt","expected_sha256":removed_hash},
                {"kind":"edit","path":"second.txt","expected_sha256":hash,"edits":[
                    {"kind":"replace_exact","old_text":"OLD","new_text":"NEW","expected_match_count":expected}]}]}),
            ),
        ));
        assert_eq!(
            out["conflict_recovery"]["conflict_kind"],
            "match_count_mismatch"
        );
        assert_eq!(out["conflict_recovery"]["actual_match_count"], 3);
        assert_eq!(out["change_index"], 3);
        assert_eq!(out["edit_index"], 0);
        assert_eq!(out["state_changed"], false);
        assert!(!tmp.path().join(format!("created-{expected}.txt")).exists());
        assert!(!tmp.path().join(format!("renamed-{expected}.txt")).exists());
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(&removed).unwrap(), "retain");
    }
    let zero = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "second.txt",
            serde_json::json!({"recovery_metadata_version":1,"changes":[{"kind":"edit","path":"second.txt","expected_sha256":hash,"edits":[
            {"kind":"replace_exact","old_text":"ABSENT","new_text":"NEW","expected_match_count":1}]}]}),
        ),
    ));
    assert_eq!(
        zero["conflict_recovery"]["conflict_kind"],
        "match_count_mismatch"
    );
    assert_eq!(zero["conflict_recovery"]["actual_match_count"], 0);
    assert_eq!(
        zero["conflict_recovery"]["candidate_ranges"],
        serde_json::json!([])
    );
    let overlap = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "second.txt",
            serde_json::json!({"recovery_metadata_version":1,"changes":[{"kind":"edit","path":"second.txt","expected_sha256":hash,"edits":[
            {"kind":"replace_exact","old_text":"OLD","new_text":"NEW","expected_match_count":3},
            {"kind":"replace_exact","old_text":"OLD","new_text":"OTHER","occurrence":3}]}]}),
        ),
    ));
    assert_eq!(
        overlap["conflict_recovery"]["conflict_kind"],
        "overlapping_edits"
    );
    assert_eq!(
        overlap["conflict_recovery"]["conflicting_edit_indices"],
        serde_json::json!([0, 1])
    );
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "OLD OLD OLD");
    assert_eq!(std::fs::read_to_string(&first).unwrap(), "keep");
}

#[test]
fn bulk_exact_scoped_dry_run_is_bounded_and_does_not_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("bulk.txt");
    let source = (0..40).map(|_| "OLD\n").collect::<String>();
    std::fs::write(&file, &source).unwrap();
    let hash = sha256_hex_bytes(source.as_bytes());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "bulk.txt",
            serde_json::json!({"dry_run":true,"changes":[{"kind":"edit","path":"bulk.txt","expected_sha256":hash,"edits":[
            {"kind":"replace_exact","old_text":"OLD","new_text":"NEW","line_scope":{"start_line":2,"end_line":39},"expected_match_count":38}]}]}),
        ),
    ));
    assert_eq!(out["dry_run"], true);
    assert_eq!(out["changed"], false);
    assert_eq!(out["state_changed"], false);
    assert_eq!(out["applied_count"], 0);
    assert_eq!(out["planned_count"], 1);
    assert_eq!(out["files"][0]["edits"][0]["match_count"], 38);
    assert_eq!(out["files"][0]["edits"][0]["expected_match_count"], 38);
    assert_eq!(
        out["files"][0]["edits"][0]["match_ranges"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(out["files"][0]["edits"][0]["match_ranges_truncated"], true);
    assert_eq!(out["files"][0]["edits"][0]["would_change"], true);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn bulk_exact_stale_sha_and_later_mismatch_preserve_prior_delete() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let old = tmp.path().join("old.txt");
    let target = tmp.path().join("target.txt");
    std::fs::write(&old, "retain").unwrap();
    std::fs::write(&target, "OLD OLD").unwrap();
    let old_hash = sha256_hex_bytes(&std::fs::read(&old).unwrap());
    let target_hash = sha256_hex_bytes(&std::fs::read(&target).unwrap());
    for expected_sha256 in ["0".repeat(64), target_hash] {
        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "old.txt",
                serde_json::json!({"recovery_metadata_version":1,"changes":[
                {"kind":"delete","path":"old.txt","expected_sha256":old_hash},
                {"kind":"edit","path":"target.txt","expected_sha256":expected_sha256,"edits":[
                    {"kind":"replace_exact","old_text":"OLD","new_text":"NEW","expected_match_count":3}]}]}),
            ),
        ));
        assert_eq!(out["changed"], false);
        assert_eq!(out["state_changed"], false);
        assert_eq!(out["change_index"], 1);
        assert!(old.exists());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "OLD OLD");
    }
}

#[test]
fn bulk_exact_evidence_has_transaction_wide_range_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let source = (0..9)
        .map(|index| format!("TOKEN{index} ").repeat(9))
        .collect::<String>();
    let file = tmp.path().join("many.txt");
    std::fs::write(&file, &source).unwrap();
    let edits = (0..9)
        .map(|index| {
            serde_json::json!({
                "kind":"replace_exact","old_text":format!("TOKEN{index}"),
                "new_text":format!("DONE{index}"),"expected_match_count":9
            })
        })
        .collect::<Vec<_>>();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "many.txt",
            serde_json::json!({"dry_run":true,"changes":[{"kind":"edit","path":"many.txt",
            "expected_sha256":sha256_hex_bytes(source.as_bytes()),"edits":edits}]}),
        ),
    ));
    assert_eq!(out["change_summary"]["resolved_matches"], 81);
    let summaries = out["files"][0]["edits"].as_array().unwrap();
    assert_eq!(
        summaries
            .iter()
            .map(|edit| edit["match_ranges"].as_array().unwrap().len())
            .sum::<usize>(),
        64
    );
    assert!(summaries
        .iter()
        .any(|edit| edit["match_ranges_truncated"] == true));
    assert!(serde_json::to_vec(&out).unwrap().len() < 256 * 1024);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), source);
}

#[test]
fn bulk_exact_requires_wire_sha_even_when_match_is_unique() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("unique.txt");
    std::fs::write(&file, "OLD").unwrap();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "unique.txt",
            serde_json::json!({"changes":[{"kind":"edit","path":"unique.txt","edits":[
            {"kind":"replace_exact","old_text":"OLD","new_text":"NEW","expected_match_count":1}]}]}),
        ),
    ));
    assert_eq!(out["error_kind"], "missing_sha256_guard");
    assert_eq!(out["state_changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "OLD");
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "unique.txt",
            serde_json::json!({"changes":[{"kind":"edit","path":"unique.txt",
            "expected_sha256":sha256_hex_bytes(b"OLD"),"edits":[
            {"kind":"replace_exact","old_text":"OLD","new_text":"NEW","expected_match_count":1}]}]}),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "NEW");
}

#[test]
fn bulk_exact_rejects_expanded_file_before_allocating_or_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("growth.txt");
    std::fs::write(&file, "aaaaaa").unwrap();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "growth.txt",
            serde_json::json!({"changes":[{"kind":"edit","path":"growth.txt",
            "expected_sha256":sha256_hex_bytes(b"aaaaaa"),"edits":[
            {"kind":"replace_exact","old_text":"a","new_text":"x".repeat(400 * 1024),"expected_match_count":6}]}]}),
        ),
    ));
    assert_eq!(out["error_kind"], "edit_conflict");
    assert_eq!(out["state_changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "aaaaaa");
}

#[test]
fn bulk_exact_bounds_final_size_after_all_original_source_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("balanced.txt");
    let original = format!("{}{}", "A".repeat(6), "Z".repeat(1_500_000));
    std::fs::write(&file, &original).unwrap();
    let expanded = "X".repeat(300_000);
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "balanced.txt",
            serde_json::json!({"changes":[{"kind":"edit","path":"balanced.txt",
            "expected_sha256":sha256_hex_bytes(original.as_bytes()),"edits":[
            {"kind":"replace_exact","old_text":"A","new_text":expanded,"expected_match_count":6},
            {"kind":"replace_exact","old_text":"Z".repeat(500_000),"new_text":"","expected_match_count":3}
            ]}]}),
        ),
    ));
    assert_eq!(out["changed"], true, "{out}");
    assert_eq!(out["change_summary"]["resolved_matches"], 9);
    assert_eq!(std::fs::metadata(&file).unwrap().len(), 1_800_000);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "X".repeat(1_800_000)
    );
}

#[test]
fn file_apply_text_edits_applies_multi_file_transaction() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
    std::fs::write(tmp.path().join("c.txt"), "gamma\n").unwrap();
    let hash = |path: &str| sha256_hex_bytes(&std::fs::read(tmp.path().join(path)).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "a.txt",
            serde_json::json!({
                "changes": [
                    {
                        "kind": "edit",
                        "path": "a.txt",
                        "expected_sha256": hash("a.txt"),
                        "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                    },
                    {"kind": "create", "path": "nested/new.txt", "content": "new\n"},
                    {"kind": "delete", "path": "b.txt", "expected_sha256": hash("b.txt")},
                    {"kind": "rename", "path": "c.txt", "to_path": "moved/c.txt", "expected_sha256": hash("c.txt")}
                ]
            }),
        ),
    ));

    assert_eq!(out["changed"], true);
    assert_eq!(out["applied_count"], 4);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
        "ALPHA\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("nested/new.txt")).unwrap(),
        "new\n"
    );
    assert!(!tmp.path().join("b.txt").exists());
    assert!(!tmp.path().join("c.txt").exists());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("moved/c.txt")).unwrap(),
        "gamma\n"
    );
    assert_eq!(out["files"].as_array().unwrap().len(), 4);
}

#[test]
fn file_apply_text_edits_allows_public_dotenv_template_but_rejects_env() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join(".env.example"), "KEY=fake\n").unwrap();
    std::fs::write(tmp.path().join(".env"), "KEY=secret\n").unwrap();

    let allowed = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            ".env.example",
            serde_json::json!({
                "edits": [{"kind": "replace_exact", "old_text": "fake", "new_text": "sample"}]
            }),
        ),
    ));
    assert_eq!(allowed["changed"], true, "{allowed}");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".env.example")).unwrap(),
        "KEY=sample\n"
    );

    let denied = handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            ".env",
            serde_json::json!({
                "edits": [{"kind": "replace_exact", "old_text": "secret", "new_text": "changed"}]
            }),
        ),
    );
    assert_eq!(denied.exit_code, None, "{denied:?}");
    assert!(
        denied
            .error
            .as_deref()
            .is_some_and(|error| error.contains("sensitive")),
        "{denied:?}"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join(".env")).unwrap(),
        "KEY=secret\n"
    );
}

#[test]
fn file_apply_text_edits_unique_local_edit_without_sha_uses_current_content() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "target\nunrelated=old\n").unwrap();

    // Simulate an unrelated same-file change after the model's historical read.
    std::fs::write(&file, "target\nunrelated=current\n").unwrap();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "changes": [{
                    "kind": "edit",
                    "path": "target.txt",
                    "edits": [{"kind":"replace_exact","old_text":"target","new_text":"TARGET"}]
                }]
            }),
        ),
    ));

    assert_eq!(out["changed"], true);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "TARGET\nunrelated=current\n"
    );
}

#[test]
fn file_apply_text_edits_local_edit_without_sha_rejects_changed_or_ambiguous_target() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");

    std::fs::write(&file, "target=current\nunrelated=current\n").unwrap();
    let missing = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [{
                    "kind": "edit",
                    "path": "target.txt",
                    "edits": [{"kind":"replace_exact","old_text":"target=old","new_text":"target=MODEL"}]
                }]
            }),
        ),
    ));
    assert_eq!(missing["error_kind"], "edit_conflict");
    assert_eq!(
        missing["conflict_recovery"]["conflict_kind"],
        "match_not_found"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "target=current\nunrelated=current\n"
    );

    std::fs::write(&file, "target\nother\ntarget\n").unwrap();
    let ambiguous = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [{
                    "kind": "edit",
                    "path": "target.txt",
                    "edits": [{"kind":"replace_exact","old_text":"target","new_text":"TARGET"}]
                }]
            }),
        ),
    ));
    assert_eq!(ambiguous["error_kind"], "edit_conflict");
    assert_eq!(
        ambiguous["conflict_recovery"]["conflict_kind"],
        "multiple_matches"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "target\nother\ntarget\n"
    );
}

#[test]
fn file_apply_text_edits_delete_and_rename_still_require_wire_sha_guard() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("delete.txt"), "delete me\n").unwrap();
    std::fs::write(tmp.path().join("rename.txt"), "rename me\n").unwrap();

    for changes in [
        serde_json::json!([{"kind":"delete","path":"delete.txt"}]),
        serde_json::json!([{"kind":"rename","path":"rename.txt","to_path":"renamed.txt"}]),
    ] {
        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "delete.txt",
                serde_json::json!({"changes": changes}),
            ),
        ));
        assert_eq!(out["error_kind"], "missing_sha256_guard");
        assert_eq!(out["state_changed"], false);
    }
    assert!(tmp.path().join("delete.txt").exists());
    assert!(tmp.path().join("rename.txt").exists());
    assert!(!tmp.path().join("renamed.txt").exists());
}

#[test]
fn file_apply_text_edits_hash_conflict_keeps_every_file_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
    let a_hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("a.txt")).unwrap());
    let b_hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("b.txt")).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "a.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [
                    {
                        "kind": "edit",
                        "path": "a.txt",
                        "expected_sha256": a_hash,
                        "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                    },
                    {
                        "kind": "delete",
                        "path": "b.txt",
                        "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    }
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "sha256_conflict");
    assert_eq!(out["change_index"], 1);
    assert_eq!(out["state_changed"], false);
    assert_eq!(out["conflict_recovery"]["conflict_kind"], "sha256_mismatch");
    assert_eq!(out["conflict_recovery"]["direct_retry_safe"], false);
    assert_eq!(out["conflict_recovery"]["reread_required"], true);
    assert_eq!(
        out["conflict_recovery"]["occurrence_selector_supported"],
        false
    );
    assert_eq!(
        out["conflict_recovery"]["expected_sha256"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(out["conflict_recovery"]["current_sha256"], b_hash);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("a.txt")).unwrap(),
        "alpha\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("b.txt")).unwrap(),
        "beta\n"
    );
}

#[test]
fn file_apply_text_edits_rejects_resolved_path_aliases() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::create_dir(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/a.txt"), "alpha\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("src/a.txt")).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "src/a.txt",
            serde_json::json!({
                "changes": [
                    {
                        "kind": "edit",
                        "path": "src/a.txt",
                        "expected_sha256": hash,
                        "edits": [{"kind": "replace_exact", "old_text": "alpha", "new_text": "ALPHA"}]
                    },
                    {
                        "kind": "delete",
                        "path": "src//a.txt",
                        "expected_sha256": hash
                    }
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "path_overlap");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("src/a.txt")).unwrap(),
        "alpha\n"
    );
}

#[test]
fn file_apply_text_edits_replace_exact_writes_atomically() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "old\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                ]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["changed_paths"][0], "target.txt");
    assert!(out["files"][0]["edits"][0].get("match_ranges").is_none());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
}

#[test]
fn file_apply_text_edits_ignores_empty_insert_noop_and_applies_remaining_edit() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "old\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "insert_before", "anchor_text": "missing", "new_text": ""},
                    {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                ]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(out["ignored_noop_count"], 1);
    assert_eq!(out["change_summary"]["logical_edits"], 2);
    assert_eq!(out["files"][0]["edits"].as_array().unwrap().len(), 1);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
}

#[test]
fn file_apply_text_edits_empty_insert_noop_still_validates_anchor_text() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "old\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "insert_before", "anchor_text": "bad\u{0}anchor", "new_text": ""},
                    {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                ]
            }),
        ),
    ));
    let msg = out["error"].as_str().unwrap();
    assert!(msg.contains("NUL"), "{msg}");
    assert!(msg.contains("No files were modified"), "{msg}");
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old\n");
}

#[test]
fn file_apply_text_edits_dry_run_does_not_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "old\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "dry_run": true,
                "edits": [
                    {"kind": "replace_exact", "old_text": "old", "new_text": "new"}
                ]
            }),
        ),
    ));
    assert_eq!(out["dry_run"], true);
    assert_eq!(out["changed"], false);
    assert_eq!(out["would_change"], true);
    assert_eq!(out["changed_paths"][0], "target.txt");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "old\n");
}

#[test]
fn file_apply_text_edits_rejects_missing_match_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "missing", "new_text": "x"}
                ]
            }),
        ),
    ));
    let msg = out["error"].as_str().unwrap();
    assert!(msg.contains("match text was not found"));
    assert!(msg.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
}

#[test]
fn file_apply_text_edits_rejects_ambiguous_match_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "dup-dup\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "dup", "new_text": "x"}
                ]
            }),
        ),
    ));
    let msg = out["error"].as_str().unwrap();
    assert!(msg.contains("matched 2 times"));
    assert!(msg.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "dup-dup\n");
}

#[test]
fn file_apply_text_edits_expected_file_sha256_mismatch_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "expected_file_sha256": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdead",
                "edits": [
                    {"kind": "replace_exact", "old_text": "alpha", "new_text": "beta"}
                ]
            }),
        ),
    ));
    let err = out["error"].as_str().unwrap();
    assert_eq!(out["error_kind"], "sha256_conflict");
    assert_eq!(out["state_changed"], false);
    assert!(err.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
}

#[test]
fn file_apply_text_edits_duplicate_anchor_advisory_boundaries() {
    let cases = [
        ("insert_before", "prefix\nanchor\n", true),
        ("insert_after", "anchor\nsuffix\n", true),
        ("insert_before", "anchor\n", true),
        ("insert_after", "anchor\n", true),
        ("insert_before", "prefix\nanchor\nsuffix\n", false),
        ("insert_after", "prefix\nanchor\nsuffix\n", false),
        ("insert_before", "anchor\nsuffix\n", false),
        ("insert_after", "prefix\nanchor\n", false),
        ("insert_before", "anchor \n", false),
        ("insert_after", " anchor\n", false),
        ("insert_before", "anchor", false),
        ("insert_after", "anchor", false),
        ("insert_before", "", false),
        ("insert_after", "", false),
    ];
    for dry_run in [true, false] {
        for crlf in [false, true] {
            for (kind, new_text, warned) in cases {
                let tmp = tempfile::tempdir().unwrap();
                let policy = project_policy(tmp.path());
                let file = tmp.path().join("target.txt");
                let source = "head\nanchor\ntail\n";
                let original = if crlf {
                    source.replace('\n', "\r\n")
                } else {
                    source.to_string()
                };
                std::fs::write(&file, &original).unwrap();
                // Anchor and insertion deliberately use different newline forms.
                let out = line_edit_json(handle_file_request(
                    &policy,
                    &apply_text_edits_request(
                        tmp.path(),
                        "target.txt",
                        serde_json::json!({
                            "dry_run": dry_run,
                            "edits": [{"kind": kind, "anchor_text": "anchor\r\n", "new_text": new_text}]
                        }),
                    ),
                ));
                assert_eq!(out["execution_state"], "completed", "{out}");
                assert_eq!(out["changed"], !dry_run && !new_text.is_empty());
                assert_eq!(out["state_changed"], !dry_run && !new_text.is_empty());
                assert_eq!(out["would_change"], !new_text.is_empty());
                let edits = out["files"][0]["edits"].as_array().unwrap();
                assert_eq!(edits.len(), usize::from(!new_text.is_empty()));
                if let Some(edit) = edits.first() {
                    assert_eq!(
                        edit.get("warning").is_some(),
                        warned,
                        "{kind} {new_text:?}: {out}"
                    );
                    if warned {
                        assert!(edit["warning"]
                            .as_str()
                            .unwrap()
                            .contains("original anchor remains"));
                    }
                }
                let expected = if kind == "insert_before" {
                    format!("head\n{new_text}anchor\ntail\n")
                } else {
                    format!("head\nanchor\n{new_text}tail\n")
                };
                let expected = if dry_run {
                    original
                } else if crlf {
                    expected.replace('\n', "\r\n")
                } else {
                    expected
                };
                assert_eq!(std::fs::read_to_string(&file).unwrap(), expected);
            }
        }
    }
}

#[test]
fn file_apply_text_edits_duplicate_anchor_advisory_tracks_sorted_edit_indices() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "first\nanchor\nbody\nanchor\nbody\n").unwrap();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind":"insert_after","anchor_text":"anchor\nbody\n","new_text":"anchor\r\nbody\r\n","occurrence":2,"line_scope":{"start_line":4,"end_line":5}},
                    {"kind":"replace_exact","old_text":"first\n","new_text":"first\nfirst\n"}
                ]
            }),
        ),
    ));
    let edits = out["files"][0]["edits"].as_array().unwrap();
    assert_eq!(edits[0]["index"], 1);
    assert!(edits[0].get("warning").is_none());
    assert_eq!(edits[1]["index"], 0);
    assert!(edits[1]["warning"].is_string());
    assert_eq!(
        std::fs::read_to_string(file).unwrap(),
        "first\nfirst\nanchor\nbody\nanchor\nbody\nanchor\nbody\n"
    );
}

#[test]
fn file_apply_text_edits_insert_before_after_and_delete_exact() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\nbeta\ngamma\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "insert_after", "anchor_text": "alpha\n", "new_text": "ALPHA-AFTER\n"},
                    {"kind": "delete_exact", "old_text": "beta\n"},
                    {"kind": "insert_before", "anchor_text": "gamma\n", "new_text": "GAMMA-BEFORE\n"}
                ]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(out["applied_count"], 1);
    assert_eq!(out["files"][0]["edits"].as_array().unwrap().len(), 3);
    assert_eq!(out["changed_paths"][0], "target.txt");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "alpha\nALPHA-AFTER\nGAMMA-BEFORE\ngamma\n"
    );
}

#[test]
fn file_apply_text_edits_crlf_accepts_lf_edits_and_preserves_crlf() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, b"one\r\ntwo\r\nthree\r\nfour\r\nfive\r\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "one\n", "new_text": "ONE\n"},
                    {"kind": "insert_after", "anchor_text": "two\n", "new_text": "AFTER-TWO\n"},
                    {"kind": "delete_exact", "old_text": "three\n"},
                    {"kind": "insert_before", "anchor_text": "four\n", "new_text": "BEFORE-FOUR\n"}
                ]
            }),
        ),
    ));

    assert_eq!(out["changed"], true);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"ONE\r\ntwo\r\nAFTER-TWO\r\nBEFORE-FOUR\r\nfour\r\nfive\r\n"
    );
}

#[test]
fn file_apply_text_edits_lf_accepts_crlf_edits_and_preserves_lf() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, b"one\ntwo\nthree\nfour\nfive\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "one\r\n", "new_text": "ONE\r\n"},
                    {"kind": "insert_after", "anchor_text": "two\r\n", "new_text": "AFTER-TWO\r\n"},
                    {"kind": "delete_exact", "old_text": "three\r\n"},
                    {"kind": "insert_before", "anchor_text": "four\r\n", "new_text": "BEFORE-FOUR\r\n"}
                ]
            }),
        ),
    ));

    assert_eq!(out["changed"], true);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"ONE\ntwo\nAFTER-TWO\nBEFORE-FOUR\nfour\nfive\n"
    );
}

#[test]
fn file_apply_text_edits_mixed_line_endings_abort_entire_batch() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let first = tmp.path().join("first.txt");
    let mixed = tmp.path().join("mixed.txt");
    std::fs::write(&first, b"alpha\r\n").unwrap();
    std::fs::write(&mixed, b"beta\r\ngamma\n").unwrap();
    let hash = |path: &Path| sha256_hex_bytes(&std::fs::read(path).unwrap());

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "first.txt",
            serde_json::json!({
                "changes": [
                    {
                        "kind": "edit",
                        "path": "first.txt",
                        "expected_sha256": hash(&first),
                        "edits": [{"kind": "replace_exact", "old_text": "alpha\n", "new_text": "ALPHA\n"}]
                    },
                    {
                        "kind": "edit",
                        "path": "mixed.txt",
                        "expected_sha256": hash(&mixed),
                        "edits": [{"kind": "replace_exact", "old_text": "beta\n", "new_text": "BETA\n"}]
                    }
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "edit_conflict");
    assert_eq!(out["change_index"], 1);
    assert!(out["error"].as_str().unwrap().contains("mixed LF and CRLF"));
    assert_eq!(std::fs::read(&first).unwrap(), b"alpha\r\n");
    assert_eq!(std::fs::read(&mixed).unwrap(), b"beta\r\ngamma\n");
}

#[test]
fn file_apply_text_edits_line_ending_normalization_is_not_fuzzy_matching() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, b"alpha beta\r\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "alpha  beta\n", "new_text": "x\n"}
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "edit_conflict");
    assert!(out["error"]
        .as_str()
        .unwrap()
        .contains("match text was not found"));
    assert_eq!(std::fs::read(&file).unwrap(), b"alpha beta\r\n");
}

#[test]
fn file_apply_text_edits_rejects_bare_cr_replacement_without_file_line_endings() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, b"one").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "one", "new_text": "ONE\r"}
                ]
            }),
        ),
    ));

    assert_eq!(out["error_kind"], "edit_conflict");
    assert!(out["error"].as_str().unwrap().contains("bare CR"));
    assert_eq!(std::fs::read(&file).unwrap(), b"one");
}

#[test]
fn file_apply_text_edits_structured_multiple_match_recovery_is_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(
        &file,
        (1..=10)
            .map(|n| format!("dup-{n}\ndup\n"))
            .collect::<String>(),
    )
    .unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"dup\n","new_text":"x\n"}]}]
            }),
        ),
    ));
    assert_eq!(out["error_kind"], "edit_conflict");
    let recovery = &out["conflict_recovery"];
    assert_eq!(recovery["schema_version"], 1);
    assert_eq!(recovery["conflict_kind"], "multiple_matches");
    assert_eq!(recovery["match_count"], 10);
    assert_eq!(recovery["occurrence_selector_supported"], true);
    assert_eq!(recovery["direct_retry_safe"], true);
    assert_eq!(recovery["reread_required"], false);
    assert_eq!(recovery["candidate_ranges"].as_array().unwrap().len(), 8);
    assert_eq!(recovery["candidate_ranges"][0]["occurrence"], 1);
    assert_eq!(recovery["candidate_ranges"][0]["start_line"], 2);
    assert_eq!(recovery["candidate_ranges"][7]["occurrence"], 8);
    assert_eq!(recovery["candidates_truncated"], true);
    let error = out["error"].as_str().unwrap();
    assert!(error.contains("choose an advertised occurrence"));
    assert!(error.contains("still-valid snapshot guard"));
    assert!(!error.contains("read this file again"));
    let serialized = serde_json::to_string(&out).unwrap();
    assert!(!serialized.contains("x\\n"));
    assert_eq!(
        std::fs::read_to_string(&file)
            .unwrap()
            .matches("dup\n")
            .count(),
        10
    );
}

#[test]
fn file_apply_text_edits_structured_not_found_disables_selector() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "alpha\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"missing","new_text":"SECRET_NEW"}]}]
            }),
        ),
    ));
    let recovery = &out["conflict_recovery"];
    assert_eq!(recovery["conflict_kind"], "match_not_found");
    assert_eq!(recovery["match_count"], 0);
    assert_eq!(recovery["occurrence_selector_supported"], false);
    assert_eq!(recovery["direct_retry_safe"], false);
    assert_eq!(recovery["reread_required"], true);
    assert_eq!(recovery["recovery_action"], "reread_or_refine_match");
    let retry_guidance = out["retry_guidance"].as_str().unwrap();
    assert!(retry_guidance.contains("reread or refine the exact target"));
    assert!(retry_guidance.contains("bounded deterministic transformation"));
    assert!(!retry_guidance.contains("prefer apply_patch"));
    assert_eq!(recovery["candidate_ranges"].as_array().unwrap().len(), 0);
    assert!(!serde_json::to_string(recovery)
        .unwrap()
        .contains("SECRET_NEW"));
}

#[test]
fn file_apply_text_edits_structured_overlap_is_atomic_and_body_free() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "abcdef abcdef\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[
                        {"kind":"replace_exact","old_text":"abc","new_text":"SECRET_A","occurrence":2},
                        {"kind":"replace_exact","old_text":"cde","new_text":"SECRET_B","occurrence":2}
                    ]}]
            }),
        ),
    ));
    let recovery = &out["conflict_recovery"];
    assert_eq!(recovery["conflict_kind"], "overlapping_edits");
    assert_eq!(
        recovery["conflicting_edit_indices"],
        serde_json::json!([0, 1])
    );
    assert_eq!(
        recovery["conflicting_edit_ranges"],
        serde_json::json!([
            {"edit_index": 0, "start_line": 1, "end_line": 1},
            {"edit_index": 1, "start_line": 1, "end_line": 1}
        ])
    );
    assert_eq!(recovery["recovery_action"], "refine_edit_batch");
    assert_eq!(recovery["direct_retry_safe"], true);
    assert_eq!(recovery["reread_required"], false);
    let serialized = serde_json::to_string(recovery).unwrap();
    assert!(!serialized.contains("SECRET_A"));
    assert!(!serialized.contains("SECRET_B"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "abcdef abcdef\n");
}

#[test]
fn file_apply_text_edits_occurrence_selects_second_exact_match() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "dup\nkeep\ndup\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"SECOND","occurrence":2}]}]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "dup\nkeep\nSECOND\n"
    );
}

#[test]
fn file_apply_text_edits_occurrence_out_of_range_is_actionable_and_atomic() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "dup\ndup\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"delete_exact","old_text":"dup","occurrence":3}]}]
            }),
        ),
    ));
    assert_eq!(out["error_kind"], "edit_conflict");
    assert_eq!(
        out["conflict_recovery"]["conflict_kind"],
        "occurrence_out_of_range"
    );
    assert_eq!(out["conflict_recovery"]["match_count"], 2);
    assert_eq!(out["conflict_recovery"]["requested_occurrence"], 3);
    assert_eq!(out["conflict_recovery"]["direct_retry_safe"], true);
    assert_eq!(out["conflict_recovery"]["reread_required"], false);
    assert!(out["error"]
        .as_str()
        .unwrap()
        .contains("choose a valid advertised occurrence"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "dup\ndup\n");
}

#[test]
fn file_apply_text_edits_sha_conflict_precedes_occurrence_selection() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "dup\ndup\n").unwrap();
    let current_sha256 = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version": 1,
                "changes": [{"kind":"edit","path":"target.txt","expected_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"SECOND","occurrence":2,"line_scope":{"start_line":2,"end_line":2}}]}]
            }),
        ),
    ));
    assert_eq!(out["error_kind"], "sha256_conflict");
    assert_eq!(out["conflict_recovery"]["conflict_kind"], "sha256_mismatch");
    assert_eq!(out["conflict_recovery"]["recovery_action"], "reread_file");
    assert_eq!(out["conflict_recovery"]["direct_retry_safe"], false);
    assert_eq!(out["conflict_recovery"]["reread_required"], true);
    assert_eq!(
        out["conflict_recovery"]["occurrence_selector_supported"],
        false
    );
    assert_eq!(
        out["conflict_recovery"]["expected_sha256"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(out["conflict_recovery"]["current_sha256"], current_sha256);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "dup\ndup\n");
}

#[test]
fn file_apply_text_edits_multiline_and_crlf_candidates_use_source_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, b"head\r\na\r\nb\r\nmid\r\na\r\nb\r\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let conflict = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"a\nb\n","new_text":"X\n"}]}]
            }),
        ),
    ));
    assert_eq!(
        conflict["conflict_recovery"]["candidate_ranges"][0]["start_line"],
        2
    );
    assert_eq!(
        conflict["conflict_recovery"]["candidate_ranges"][0]["end_line"],
        3
    );
    assert_eq!(
        conflict["conflict_recovery"]["candidate_ranges"][1]["start_line"],
        5
    );
    let success = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"a\nb\n","new_text":"X\n","occurrence":2}]}]
            }),
        ),
    ));
    assert_eq!(success["changed"], true);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"head\r\na\r\nb\r\nmid\r\nX\r\n"
    );
}

#[test]
fn file_apply_text_edits_old_server_payload_keeps_legacy_conflict_shape() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "dup\ndup\n").unwrap();
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"x"}]
            }),
        ),
    ));
    assert_eq!(out["error_kind"], "edit_conflict");
    assert!(out.get("conflict_recovery").is_none());
}

#[test]
fn file_apply_text_edits_new_payload_is_legacy_deserializable_and_fail_closed() {
    #[derive(serde::Deserialize)]
    struct LegacyEdit {
        kind: crate::apply_edits_shared::ApplyTextEditKind,
        #[serde(default)]
        old_text: Option<String>,
        #[serde(default)]
        new_text: Option<String>,
        #[serde(default)]
        anchor_text: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct LegacyChange {
        kind: crate::apply_edits_shared::ApplyFileChangeKind,
        path: String,
        #[serde(default)]
        to_path: Option<String>,
        #[serde(default)]
        content: Option<String>,
        #[serde(default)]
        edits: Vec<LegacyEdit>,
        #[serde(default)]
        expected_sha256: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct LegacyPayload {
        changes: Vec<LegacyChange>,
        #[serde(default)]
        dry_run: Option<bool>,
    }
    let value = serde_json::json!({
        "recovery_metadata_version":1,
        "changes":[{"kind":"edit","path":"target.txt","expected_sha256":"a".repeat(64),
            "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"x","occurrence":2,"line_scope":{"start_line":2,"end_line":2}}]}],
        "dry_run":false
    });
    let legacy: LegacyPayload = serde_json::from_value(value).unwrap();
    assert_eq!(legacy.changes.len(), 1);
    let change = &legacy.changes[0];
    assert_eq!(
        change.kind,
        crate::apply_edits_shared::ApplyFileChangeKind::Edit
    );
    assert_eq!(change.path, "target.txt");
    assert_eq!(
        change.expected_sha256.as_deref(),
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
    );
    assert!(change.to_path.is_none() && change.content.is_none());
    assert_eq!(legacy.dry_run, Some(false));
    let edit = &change.edits[0];
    assert_eq!(
        edit.kind,
        crate::apply_edits_shared::ApplyTextEditKind::ReplaceExact
    );
    assert_eq!(edit.new_text.as_deref(), Some("x"));
    assert!(edit.anchor_text.is_none());
    let needle = edit.old_text.as_deref().unwrap();
    assert_eq!(
        "dup\ndup\n".matches(needle).count(),
        2,
        "pre-feature Runner sees no occurrence and its unique-only semantics still fail closed"
    );
}

#[test]
fn file_apply_text_edits_rejects_overlapping_edits() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "abcdef\n").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "edits": [
                    {"kind": "replace_exact", "old_text": "abc", "new_text": "ABC"},
                    {"kind": "replace_exact", "old_text": "cde", "new_text": "CDE"}
                ]
            }),
        ),
    ));
    let err = out["error"].as_str().unwrap();
    assert!(err.contains("edits overlap"));
    assert!(err.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "abcdef\n");
}

#[test]
fn file_apply_text_edits_line_scope_filters_candidates_and_occurrence_stays_global() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    let original = "head\ndup\nmid\ndup\ntail\n";
    std::fs::write(&file, original).unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());

    let ambiguity = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"x","line_scope":{"start_line":1,"end_line":5}}]}]
            }),
        ),
    ));
    let recovery = &ambiguity["conflict_recovery"];
    assert_eq!(recovery["conflict_kind"], "multiple_matches");
    assert_eq!(recovery["match_count"], 2);
    assert_eq!(recovery["line_scope_match_count"], 2);
    assert_eq!(recovery["candidate_ranges"][0]["occurrence"], 1);
    assert_eq!(recovery["candidate_ranges"][1]["occurrence"], 2);
    assert_eq!(
        recovery["recovery_action"],
        "narrow_line_scope_or_select_occurrence"
    );
    assert_eq!(recovery["direct_retry_safe"], true);
    assert_eq!(recovery["reread_required"], false);

    let no_match = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"x","line_scope":{"start_line":5,"end_line":5}}]}]
            }),
        ),
    ));
    let recovery = &no_match["conflict_recovery"];
    assert_eq!(recovery["conflict_kind"], "match_not_found");
    assert_eq!(recovery["match_count"], 2);
    assert_eq!(recovery["line_scope_match_count"], 0);
    assert_eq!(recovery["candidate_ranges"], serde_json::json!([]));
    assert_eq!(recovery["direct_retry_safe"], true);
    assert_eq!(recovery["reread_required"], false);

    let mismatch = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"x","occurrence":1,"line_scope":{"start_line":4,"end_line":4}}]}]
            }),
        ),
    ));
    let recovery = &mismatch["conflict_recovery"];
    assert_eq!(recovery["conflict_kind"], "occurrence_outside_line_scope");
    assert_eq!(recovery["requested_occurrence"], 1);
    assert_eq!(
        recovery["line_scope"],
        serde_json::json!({"start_line":4,"end_line":4})
    );
    assert_eq!(recovery["candidate_ranges"][0]["occurrence"], 2);
    assert_eq!(
        recovery["recovery_action"],
        "align_occurrence_with_line_scope"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);

    let success = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"dup","new_text":"SECOND","line_scope":{"start_line":4,"end_line":4}}]}]
            }),
        ),
    ));
    assert_eq!(success["changed"], true);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "head\ndup\nmid\nSECOND\ntail\n"
    );
}

#[test]
fn file_apply_text_edits_multiline_crlf_scope_requires_full_containment() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    let original = b"head\r\na\r\nb\r\nmid\r\na\r\nb\r\ntail\r\n";
    std::fs::write(&file, original).unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());

    for line_scope in [
        serde_json::json!({"start_line":5,"end_line":5}),
        serde_json::json!({"start_line":4,"end_line":5}),
    ] {
        let out = line_edit_json(handle_file_request(
            &policy,
            &apply_text_edits_request(
                tmp.path(),
                "target.txt",
                serde_json::json!({
                    "recovery_metadata_version":1,
                    "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                        "edits":[{"kind":"replace_exact","old_text":"a\nb\n","new_text":"X\n","line_scope":line_scope}]}]
                }),
            ),
        ));
        assert_eq!(out["conflict_recovery"]["conflict_kind"], "match_not_found");
        assert_eq!(out["conflict_recovery"]["line_scope_match_count"], 0);
        assert_eq!(std::fs::read(&file).unwrap(), original);
    }

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,
                    "edits":[{"kind":"replace_exact","old_text":"a\nb\n","new_text":"X\n","line_scope":{"start_line":5,"end_line":6}}]}]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"head\r\na\r\nb\r\nmid\r\nX\r\ntail\r\n"
    );
}

#[test]
fn file_apply_text_edits_line_scope_supports_all_exact_edit_kinds() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "replace\ndelete\nbefore\nafter\n").unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,"edits":[
                    {"kind":"replace_exact","old_text":"replace\n","new_text":"REPLACE\n","line_scope":{"start_line":1,"end_line":1}},
                    {"kind":"delete_exact","old_text":"delete\n","line_scope":{"start_line":2,"end_line":2}},
                    {"kind":"insert_before","anchor_text":"before\n","new_text":"BEFORE+\n","line_scope":{"start_line":3,"end_line":3}},
                    {"kind":"insert_after","anchor_text":"after\n","new_text":"AFTER+\n","line_scope":{"start_line":4,"end_line":4}}
                ]}]
            }),
        ),
    ));
    assert_eq!(out["changed"], true);
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "REPLACE\nBEFORE+\nbefore\nafter\nAFTER+\n"
    );
}

#[test]
fn file_apply_text_edits_scoped_failures_and_overlap_remain_transactional() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    let original = "abcdef\nsecond\n";
    std::fs::write(&file, original).unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());

    let failed = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,"edits":[
                    {"kind":"replace_exact","old_text":"second","new_text":"SECOND","line_scope":{"start_line":2,"end_line":2}},
                    {"kind":"replace_exact","old_text":"missing","new_text":"x","line_scope":{"start_line":2,"end_line":2}}
                ]}]
            }),
        ),
    ));
    assert_eq!(failed["error_kind"], "edit_conflict");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);

    let overlap = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "recovery_metadata_version":1,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,"edits":[
                    {"kind":"replace_exact","old_text":"abc","new_text":"ABC","line_scope":{"start_line":1,"end_line":1}},
                    {"kind":"replace_exact","old_text":"cde","new_text":"CDE","line_scope":{"start_line":1,"end_line":1}}
                ]}]
            }),
        ),
    ));
    assert_eq!(
        overlap["conflict_recovery"]["conflict_kind"],
        "overlapping_edits"
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);

    let reversed = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,"edits":[
                    {"kind":"replace_exact","old_text":"second","new_text":"SECOND","line_scope":{"start_line":2,"end_line":1}}
                ]}]
            }),
        ),
    ));
    assert_eq!(reversed["error_kind"], "edit_conflict");
    assert!(reversed["error"].as_str().unwrap().contains("end_line"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}

#[test]
fn file_apply_text_edits_scoped_dry_run_uses_same_resolution_without_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    let original = "dup\ndup\n";
    std::fs::write(&file, original).unwrap();
    let hash = sha256_hex_bytes(&std::fs::read(&file).unwrap());
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_text_edits_request(
            tmp.path(),
            "target.txt",
            serde_json::json!({
                "dry_run":true,
                "changes":[{"kind":"edit","path":"target.txt","expected_sha256":hash,"edits":[
                    {"kind":"replace_exact","old_text":"dup","new_text":"SECOND","line_scope":{"start_line":2,"end_line":2}}
                ]}]
            }),
        ),
    ));
    assert_eq!(out["dry_run"], true);
    assert_eq!(out["changed"], false);
    assert_eq!(out["would_change"], true);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), original);
}
