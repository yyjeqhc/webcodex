use super::*;

fn apply_text_edits_request(
    cwd: &Path,
    path: &str,
    mut payload: serde_json::Value,
) -> ShellAgentShellRequest {
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
    ShellAgentShellRequest {
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
        sandbox: None,
        job_context: None,
        persistent_shell: None,
    }
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
fn file_apply_text_edits_hash_conflict_keeps_every_file_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(tmp.path().join("b.txt"), "beta\n").unwrap();
    let a_hash = sha256_hex_bytes(&std::fs::read(tmp.path().join("a.txt")).unwrap());

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
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "new\n");
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
    assert!(err.contains("expected_sha256 does not match"));
    assert!(err.contains("No files were modified"));
    assert_eq!(out["changed"], false);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "alpha\n");
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
