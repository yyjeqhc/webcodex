use super::*;

#[test]
fn file_write_project_file_creates_parent_dirs_and_reports_effect() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("nested/new.txt");

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "nested/new.txt",
            serde_json::json!({
                "path": "nested/new.txt",
                "content": "line1\nline2\n",
                "overwrite": false,
                "expected_sha256": null,
            }),
        ),
    ));

    assert_eq!(out["created"], true);
    assert_eq!(out["overwritten"], false);
    assert_eq!(out["bytes_written"], 12);
    assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(out["changed"], true);
    assert_eq!(out["state_changed"], true);
    assert_eq!(out["execution_state"], "completed");
    assert!(out.get("warning").is_none());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "line1\nline2\n");
}

#[test]
fn file_write_project_file_rejects_existing_without_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "original").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "new",
                "overwrite": false,
                "expected_sha256": null,
            }),
        ),
    ));

    assert_eq!(out["created"], false);
    assert_eq!(out["overwritten"], false);
    assert_eq!(out["changed"], false);
    assert_eq!(out["state_changed"], false);
    assert_eq!(out["execution_state"], "not_started");
    assert!(out["error"].as_str().unwrap().contains("overwrite"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
}

#[test]
fn file_write_project_file_rejects_string_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "original").unwrap();

    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "new",
                "overwrite": "false",
                "expected_sha256": null,
            }),
        ),
    ));

    assert_eq!(out["created"], false);
    assert_eq!(out["changed"], false);
    assert_eq!(out["execution_state"], "not_started");
    assert_eq!(out["error"], "overwrite must be a boolean");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
}

#[test]
fn file_write_project_file_requires_exact_sha_and_reports_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "original").unwrap();
    let original_sha = sha256_hex_bytes("original".as_bytes());

    let sha_ok = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "v1 replaced",
                "overwrite": true,
                "expected_sha256": original_sha,
            }),
        ),
    ));
    assert_eq!(sha_ok["overwritten"], true);
    assert_eq!(sha_ok["changed"], true);
    assert_eq!(sha_ok["state_changed"], true);
    assert_eq!(sha_ok["execution_state"], "completed");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 replaced");

    let replaced_sha = sha256_hex_bytes("v1 replaced".as_bytes());
    let no_op = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "v1 replaced",
                "overwrite": true,
                "expected_sha256": replaced_sha,
            }),
        ),
    ));
    assert_eq!(no_op["overwritten"], true);
    assert_eq!(no_op["changed"], false);
    assert_eq!(no_op["state_changed"], false);
    assert_eq!(no_op["bytes_written"], 0);
    assert_eq!(no_op["execution_state"], "completed");

    let sha_bad = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "bad",
                "overwrite": true,
                "expected_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            }),
        ),
    ));
    assert_eq!(sha_bad["created"], false);
    assert_eq!(sha_bad["changed"], false);
    assert_eq!(sha_bad["state_changed"], false);
    assert_eq!(sha_bad["execution_state"], "not_started");
    assert_eq!(sha_bad["sha256"], replaced_sha);
    assert!(sha_bad["error"].as_str().unwrap().contains("sha256"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 replaced");
}

#[test]
fn file_write_project_file_rejects_unguarded_legacy_and_invalid_requests() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "v2 content").unwrap();

    let unguarded = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "unguarded",
                "overwrite": true,
                "expected_sha256": null,
            }),
        ),
    ));
    assert_eq!(unguarded["changed"], false);
    assert_eq!(unguarded["execution_state"], "not_started");
    assert!(unguarded["error"]
        .as_str()
        .unwrap()
        .contains("requires expected_sha256"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2 content");

    let legacy_prefix = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "legacy",
                "overwrite": true,
                "expected_sha256": sha256_hex_bytes("v2 content".as_bytes()),
                "expected_content_prefix": "v2 ",
            }),
        ),
    ));
    assert_eq!(legacy_prefix["changed"], false);
    assert!(legacy_prefix["error"]
        .as_str()
        .unwrap()
        .contains("no longer supported"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2 content");

    let missing_overwrite = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "missing.txt",
            serde_json::json!({
                "content": "bad",
                "overwrite": true,
                "expected_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            }),
        ),
    ));
    assert_eq!(missing_overwrite["changed"], false);
    assert!(missing_overwrite["error"]
        .as_str()
        .unwrap()
        .contains("existing file"));
    assert!(!tmp.path().join("missing.txt").exists());

    let nul = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "new.txt",
            serde_json::json!({
                "content": "a\u{0000}b",
                "overwrite": false,
                "expected_sha256": null,
            }),
        ),
    ));
    assert_eq!(nul["created"], false);
    assert_eq!(nul["changed"], false);
    assert!(nul["error"].as_str().unwrap().contains("NUL"));
    assert!(!tmp.path().join("new.txt").exists());

    let missing_content = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "empty.txt",
            serde_json::json!({
                "overwrite": false,
                "expected_sha256": null,
            }),
        ),
    ));
    assert_eq!(missing_content["changed"], false);
    assert!(missing_content["error"]
        .as_str()
        .unwrap()
        .contains("content must be"));
    assert!(!tmp.path().join("empty.txt").exists());
}
