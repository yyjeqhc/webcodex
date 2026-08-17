use super::*;

fn artifact_upload_temp_paths(
    root: &Path,
    artifact_path: &str,
    upload_id: &str,
) -> (PathBuf, PathBuf) {
    let target = root.join(artifact_path);
    let parent = target.parent().expect("artifact path parent");
    (
        parent.join(format!(".wc-upload-{upload_id}.part")),
        parent.join(format!(".wc-upload-{upload_id}.json")),
    )
}

fn assert_upload_temp_files_exist(root: &Path, artifact_path: &str, upload_id: &str) {
    let (part, sidecar) = artifact_upload_temp_paths(root, artifact_path, upload_id);
    assert!(
        part.exists(),
        "missing upload part file: {}",
        part.display()
    );
    assert!(
        sidecar.exists(),
        "missing upload sidecar file: {}",
        sidecar.display()
    );
    let parent = part.parent().unwrap();
    assert!(
        !directory_contains_name_prefix(parent, ".pd-upload-"),
        "legacy .pd upload temp files must not be created in {}",
        parent.display()
    );
}

fn assert_no_upload_temp_files(root: &Path, artifact_path: &str) {
    let target = root.join(artifact_path);
    let Some(parent) = target.parent() else {
        return;
    };
    assert!(
        !directory_contains_name_prefix(parent, ".wc-upload-"),
        "upload temp files remained in {}",
        parent.display()
    );
    assert!(
        !directory_contains_name_prefix(parent, ".pd-upload-"),
        "legacy .pd upload temp files remained in {}",
        parent.display()
    );
}

#[test]
fn file_save_and_upload_begin_accept_office_mimes() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let cases = [
        (
            "artifacts/imports/report.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (
            "artifacts/imports/deck.pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        (
            "artifacts/imports/book.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
    ];

    for (path, mime) in cases {
        let content_base64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"office-artifact",
        );
        let saved = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_save_project_artifact",
                path,
                serde_json::json!({
                    "path": path,
                    "content_base64": content_base64,
                    "mime_type": mime,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(saved["mime_type"], mime, "{path}");

        let upload_path = path.replacen("artifacts/imports/", "artifacts/uploads/", 1);
        let upload = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                &upload_path,
                serde_json::json!({
                    "path": upload_path,
                    "expected_bytes": 0,
                    "expected_sha256": null,
                    "mime_type": mime,
                    "overwrite": false,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(upload["mime_type"], mime, "{path}");
        let upload_id = upload["upload_id"].as_str().unwrap();
        assert_upload_temp_files_exist(tmp.path(), &upload_path, upload_id);
    }
}

#[test]
fn file_artifact_upload_chunks_finish_and_abort() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/upload.bin";
    let bytes = b"abcdefgh";
    let expected_sha256 = sha256_hex_bytes(bytes);

    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": bytes.len(),
                "expected_sha256": expected_sha256,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    assert!(upload_id.starts_with("wc_upload_"));
    assert_eq!(begin["received_bytes"], 0);
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let first = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..4]);
    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": first,
                "max_chunk_bytes": 4,
            }),
        ),
    ));
    assert_eq!(out["received_bytes"], 4);
    assert_eq!(out["next_offset"], 4);
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let second = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[4..]);
    let out = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 4,
                "content_base64": second,
                "max_chunk_bytes": 4,
            }),
        ),
    ));
    assert_eq!(out["received_bytes"], bytes.len());

    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
            }),
        ),
    ));
    assert_eq!(finish["committed"], true);
    assert_eq!(finish["bytes"], bytes.len());
    assert_eq!(finish["sha256"], sha256_hex_bytes(bytes));
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), bytes);
    assert_no_upload_temp_files(tmp.path(), path);

    let abort_path = "artifacts/imports/abort.bin";
    let begin_abort = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            abort_path,
            serde_json::json!({
                "path": abort_path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let abort_upload_id = begin_abort["upload_id"].as_str().unwrap();
    assert_upload_temp_files_exist(tmp.path(), abort_path, abort_upload_id);
    let abort = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            abort_path,
            serde_json::json!({
                "path": abort_path,
                "upload_id": abort_upload_id,
            }),
        ),
    ));
    assert_eq!(abort["aborted"], true);
    assert!(!tmp.path().join(abort_path).exists());
    assert_no_upload_temp_files(tmp.path(), abort_path);
}

#[test]
fn file_artifact_upload_finish_detects_ooxml_mime_from_file() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/streamed.docx";
    let bytes = fake_ooxml_zip(
        "word/document.xml",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
        false,
    );
    let expected_sha256 = sha256_hex_bytes(&bytes);

    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": bytes.len(),
                "expected_sha256": expected_sha256,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": bytes.len(),
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let content_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
    let chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": content_base64,
                "max_chunk_bytes": bytes.len(),
            }),
        ),
    ));
    assert_eq!(chunk["received_bytes"], bytes.len());

    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id}),
        ),
    ));
    assert_eq!(finish["committed"], true);
    assert_eq!(finish["bytes"], bytes.len());
    assert_eq!(finish["sha256"], sha256_hex_bytes(&bytes));
    assert_eq!(
        finish["mime_type"],
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
    );
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), bytes);
    assert_no_upload_temp_files(tmp.path(), path);
}

#[test]
fn file_artifact_upload_begin_rejects_validation_and_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());

    let sensitive = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            ".env",
            serde_json::json!({
                "path": ".env",
                "expected_bytes": 1,
                "expected_sha256": null,
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    assert!(sensitive["error"]
        .as_str()
        .unwrap()
        .contains("sensitive artifact path"));

    let bad_hash_path = "artifacts/imports/bad-hash.txt";
    let bad_hash = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            bad_hash_path,
            serde_json::json!({
                "path": bad_hash_path,
                "expected_bytes": 1,
                "expected_sha256": "not-a-sha",
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    assert!(bad_hash["error"]
        .as_str()
        .unwrap()
        .contains("expected_sha256 must be"));

    let too_large_path = "artifacts/imports/too-large.txt";
    let too_large = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            too_large_path,
            serde_json::json!({
                "path": too_large_path,
                "expected_bytes": 5,
                "expected_sha256": null,
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 4,
            }),
        ),
    ));
    assert_eq!(too_large["error"], "expected_bytes exceeds max_bytes");

    let unsafe_octet_path = "artifacts/imports/raw.bin";
    let unsafe_octet = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            unsafe_octet_path,
            serde_json::json!({
                "path": unsafe_octet_path,
                "expected_bytes": 1,
                "expected_sha256": null,
                "mime_type": "application/octet-stream",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let unsafe_octet_error = unsafe_octet["error"].as_str().unwrap();
    assert!(unsafe_octet_error.contains(".artifact"));
    assert!(unsafe_octet_error.contains(".txt"));
    assert!(unsafe_octet_error.contains("artifacts/smoke/<name>.artifact"));
    assert_eq!(unsafe_octet["failure_kind"], "policy_rejected");

    let existing_path = "artifacts/imports/existing.txt";
    std::fs::create_dir_all(tmp.path().join("artifacts/imports")).unwrap();
    std::fs::write(tmp.path().join(existing_path), b"old").unwrap();
    let existing = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            existing_path,
            serde_json::json!({
                "path": existing_path,
                "expected_bytes": 3,
                "expected_sha256": null,
                "mime_type": "text/plain",
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    assert_eq!(existing["error"], "file exists and overwrite is false");
    assert_eq!(
        std::fs::read(tmp.path().join(existing_path)).unwrap(),
        b"old"
    );

    #[cfg(unix)]
    {
        let symlink_path = "artifacts/imports/link.txt";
        let victim = tmp.path().join("victim.txt");
        std::fs::write(&victim, b"victim").unwrap();
        std::os::unix::fs::symlink(&victim, tmp.path().join(symlink_path)).unwrap();
        let symlink = line_edit_json(handle_file_request(
            &policy,
            &json_file_op_request(
                tmp.path(),
                "file_artifact_upload_begin",
                symlink_path,
                serde_json::json!({
                    "path": symlink_path,
                    "expected_bytes": 3,
                    "expected_sha256": null,
                    "mime_type": "text/plain",
                    "overwrite": true,
                    "max_bytes": 1024,
                }),
            ),
        ));
        assert_eq!(
            symlink["error"],
            "refusing to overwrite symlink artifact path"
        );
        assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
    }
}

#[test]
fn file_artifact_upload_chunk_rejects_validation_and_keeps_final_absent() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/chunk.bin";
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024 * 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();

    let invalid_id = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": "bad",
                "offset": 0,
                "content_base64": "YQ==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert!(invalid_id["error"]
        .as_str()
        .unwrap()
        .contains("upload_id must start"));

    let invalid_base64 = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "not valid base64!",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert!(invalid_base64["error"]
        .as_str()
        .unwrap()
        .contains("invalid base64"));

    let empty = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert!(empty["error"]
        .as_str()
        .unwrap()
        .contains("decoded chunk must contain at least 1 byte"));

    let too_large = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        vec![b'x'; 64 * 1024 + 1],
    );
    let too_large = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": too_large,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(too_large["error"], "decoded chunk too large");

    let first = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abc"),
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(first["received_bytes"], 3);
    assert!(!tmp.path().join(path).exists());

    let wrong_offset = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": "ZA==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(
        wrong_offset["error"],
        "offset does not match received_bytes"
    );
    assert_eq!(wrong_offset["received_bytes"], 3);
    assert_eq!(wrong_offset["next_offset"], 3);

    let other_path = "artifacts/imports/other.bin";
    let mismatch = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            other_path,
            serde_json::json!({
                "path": other_path,
                "upload_id": upload_id.clone(),
                "offset": 3,
                "content_base64": "ZA==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(
        mismatch["error"],
        "upload_id does not belong to requested path"
    );
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
}

#[test]
fn file_artifact_upload_finish_validation_failures_keep_retry_state() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/retry.bin";

    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": 4,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let first = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abc");
    let chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": first,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(chunk["received_bytes"], 3);

    let failed_finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(
        failed_finish["error"],
        "uploaded byte count does not match expected_bytes"
    );
    assert_eq!(failed_finish["committed"], false);
    assert!(!tmp.path().join(path).exists());
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let retry_chunk = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 3,
                "content_base64": "ZA==",
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    assert_eq!(retry_chunk["received_bytes"], 4);
    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(finish["committed"], true);
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"abcd");
    assert_no_upload_temp_files(tmp.path(), path);

    let sha_path = "artifacts/imports/bad-sha.bin";
    let bad_sha = "0000000000000000000000000000000000000000000000000000000000000000";
    let begin_sha = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            sha_path,
            serde_json::json!({
                "path": sha_path,
                "expected_bytes": null,
                "expected_sha256": bad_sha,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let sha_upload_id = begin_sha["upload_id"].as_str().unwrap().to_string();
    let data = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"abcd");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            sha_path,
            serde_json::json!({
                "path": sha_path,
                "upload_id": sha_upload_id.clone(),
                "offset": 0,
                "content_base64": data,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    let sha_failed = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            sha_path,
            serde_json::json!({"path": sha_path, "upload_id": sha_upload_id.clone()}),
        ),
    ));
    assert_eq!(
        sha_failed["error"],
        "uploaded sha256 does not match expected_sha256"
    );
    assert_eq!(sha_failed["committed"], false);
    assert!(!tmp.path().join(sha_path).exists());
    assert_upload_temp_files_exist(tmp.path(), sha_path, &sha_upload_id);
}

#[test]
fn file_artifact_upload_finish_refuses_late_target_when_overwrite_false() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/race.bin";
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": false,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"new");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": chunk,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    std::fs::write(tmp.path().join(path), b"old").unwrap();
    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(finish["error"], "file exists and overwrite is false");
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"old");
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
}

#[test]
fn file_artifact_upload_finish_refuses_late_symlink_even_with_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/symlink-race.bin";
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": true,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"new");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": chunk,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));
    let victim = tmp.path().join("victim-race.bin");
    std::fs::write(&victim, b"victim").unwrap();
    std::os::unix::fs::symlink(&victim, tmp.path().join(path)).unwrap();
    let finish = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_finish",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(
        finish["error"],
        "refusing to overwrite symlink artifact path"
    );
    assert_eq!(std::fs::read(&victim).unwrap(), b"victim");
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);
}

#[test]
fn file_artifact_upload_abort_rejects_wrong_ids_and_cleans_only_temp() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let path = "artifacts/imports/abort-target.bin";
    std::fs::create_dir_all(tmp.path().join("artifacts/imports")).unwrap();
    std::fs::write(tmp.path().join(path), b"final").unwrap();
    let begin = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_begin",
            path,
            serde_json::json!({
                "path": path,
                "expected_bytes": null,
                "expected_sha256": null,
                "mime_type": null,
                "overwrite": true,
                "max_bytes": 1024,
            }),
        ),
    ));
    let upload_id = begin["upload_id"].as_str().unwrap().to_string();
    let chunk = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"temp");
    let _ = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_chunk",
            path,
            serde_json::json!({
                "path": path,
                "upload_id": upload_id.clone(),
                "offset": 0,
                "content_base64": chunk,
                "max_chunk_bytes": 64 * 1024,
            }),
        ),
    ));

    let missing = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            serde_json::json!({"path": path, "upload_id": "wc_upload_missing"}),
        ),
    ));
    assert!(missing["error"]
        .as_str()
        .unwrap()
        .contains("upload not found"));
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"final");
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let other_path = "artifacts/imports/abort-other.bin";
    let mismatch = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            other_path,
            serde_json::json!({"path": other_path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(
        mismatch["error"],
        "upload_id does not belong to requested path"
    );
    assert_upload_temp_files_exist(tmp.path(), path, &upload_id);

    let abort = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_artifact_upload_abort",
            path,
            serde_json::json!({"path": path, "upload_id": upload_id.clone()}),
        ),
    ));
    assert_eq!(abort["aborted"], true);
    assert_eq!(abort["received_bytes"], 4);
    assert_eq!(std::fs::read(tmp.path().join(path)).unwrap(), b"final");
    assert_no_upload_temp_files(tmp.path(), path);
}
