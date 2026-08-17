use super::*;

#[test]
fn file_write_project_file_creates_parent_dirs_and_reports_hash() {
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
                "expected_content_prefix": null,
            }),
        ),
    ));

    assert_eq!(out["created"], true);
    assert_eq!(out["overwritten"], false);
    assert_eq!(out["bytes_written"], 12);
    assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
    assert!(out["warning"].is_null());
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
                "expected_content_prefix": null,
            }),
        ),
    ));

    assert_eq!(out["created"], false);
    assert_eq!(out["overwritten"], false);
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
                "expected_content_prefix": null,
            }),
        ),
    ));

    assert_eq!(out["created"], false);
    assert_eq!(out["error"], "overwrite must be a boolean");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "original");
}

#[test]
fn file_write_project_file_enforces_sha_and_prefix_guards() {
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
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(sha_ok["overwritten"], true);
    assert!(sha_ok["warning"].is_null());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 replaced");

    let prefix_ok = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "v1 final",
                "overwrite": true,
                "expected_sha256": null,
                "expected_content_prefix": "v1 ",
            }),
        ),
    ));
    assert_eq!(prefix_ok["overwritten"], true);
    assert!(prefix_ok["warning"].is_null());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 final");

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
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(sha_bad["created"], false);
    assert!(sha_bad["error"].as_str().unwrap().contains("sha256"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v1 final");
}

#[test]
fn file_write_project_file_warns_on_unguarded_overwrite_and_rejects_bad_prefix() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let file = tmp.path().join("target.txt");
    std::fs::write(&file, "v2 content").unwrap();

    let prefix_bad = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_write_project_file",
            "target.txt",
            serde_json::json!({
                "content": "bad",
                "overwrite": true,
                "expected_sha256": null,
                "expected_content_prefix": "v1 ",
            }),
        ),
    ));
    assert_eq!(prefix_bad["created"], false);
    assert!(prefix_bad["error"].as_str().unwrap().contains("prefix"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2 content");

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
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(unguarded["overwritten"], true);
    assert!(unguarded["warning"]
        .as_str()
        .unwrap()
        .contains("expected_sha256"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "unguarded");

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
                "expected_content_prefix": null,
            }),
        ),
    ));
    assert_eq!(nul["created"], false);
    assert!(nul["error"].as_str().unwrap().contains("NUL"));
    assert!(!tmp.path().join("new.txt").exists());
}
