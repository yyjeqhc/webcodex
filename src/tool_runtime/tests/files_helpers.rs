//! Python helper integration tests for tool_runtime file tools.

use super::super::files::*;
use super::support::*;
use serde_json::{json, Value};

#[test]
fn helper_replace_in_file_single_replacement_success() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "world",
        "new": "rust",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], true);
    assert_eq!(out["replacements"], 1);
    assert_eq!(out["before_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(out["after_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "hello rust"
    );
}

#[test]
fn helper_replace_in_file_old_missing_leaves_file_unchanged() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "missing",
        "new": "x",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("not found"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "hello world"
    );
}

#[test]
fn helper_replace_in_file_multiple_without_allow_multiple_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "a",
        "new": "b",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("multiple"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "a a a"
    );
}

#[test]
fn helper_replace_in_file_rejects_expected_multiple_without_allow_multiple() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "hello world").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "world",
        "new": "rust",
        "expected_replacements": 2,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("allow_multiple"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "hello world"
    );
}

#[test]
fn helper_replace_in_file_allow_multiple_exact_count_succeeds() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "a",
        "new": "b",
        "expected_replacements": 3,
        "allow_multiple": true
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], true);
    assert_eq!(out["replacements"], 3);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "b b b"
    );
}

#[test]
fn helper_replace_in_file_allow_multiple_count_mismatch_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "a a a").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "a",
        "new": "b",
        "expected_replacements": 2,
        "allow_multiple": true
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("mismatch"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "a a a"
    );
}

#[test]
fn helper_replace_in_file_rejects_empty_old_and_nul() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.txt"), "x").unwrap();
    let payload = json!({
        "path": "f.txt",
        "old": "",
        "new": "y",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("old"));
    // File unchanged.
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("f.txt")).unwrap(),
        "x"
    );
}

#[test]
fn helper_replace_in_file_rejects_non_utf8_file() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("f.bin"), [0xFF, 0xFE, 0xFD]).unwrap();
    let payload = json!({
        "path": "f.bin",
        "old": "x",
        "new": "y",
        "expected_replacements": 1,
        "allow_multiple": false
    });
    let out = run_helper_locally(REPLACE_IN_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["changed"], false);
    assert!(out["error"].as_str().unwrap().contains("UTF-8"));
}

#[test]
fn helper_save_project_artifact_writes_binary_and_blocks_overwrite() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payload = json!({
        "path": "artifacts/imports/tiny.png",
        "content_base64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0x89, b'P', b'N', b'G']),
        "mime_type": "image/png",
        "overwrite": false,
        "max_bytes": 1024
    });
    let out = run_helper_locally(SAVE_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert_eq!(out["bytes_written"], 4);
    assert_eq!(out["mime_type"], "image/png");
    assert!(out["sha256"].as_str().unwrap().len() == 64);
    assert_eq!(
        std::fs::read(tmp.path().join("artifacts/imports/tiny.png")).unwrap(),
        vec![0x89, b'P', b'N', b'G']
    );

    let out2 = run_helper_locally(SAVE_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert!(out2["error"]
        .as_str()
        .unwrap()
        .contains("overwrite is false"));
}

#[test]
fn helper_read_project_artifact_metadata_counts_zip_without_extracting() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let zip_path = tmp.path().join("sample.zip");
    let status = std::process::Command::new("python3")
            .arg("-c")
            .arg("import zipfile; z=zipfile.ZipFile('sample.zip','w'); z.writestr('a.txt','a'); z.writestr('b.txt','b'); z.close()")
            .current_dir(tmp.path())
            .status()
            .unwrap();
    assert!(status.success());
    assert!(zip_path.exists());
    let payload = json!({"path": "sample.zip", "max_bytes": 1024 * 1024});
    let out = run_helper_locally(READ_PROJECT_ARTIFACT_METADATA_HELPER, &payload, tmp.path());
    assert_eq!(out["mime_type"], "application/zip");
    assert_eq!(out["archive_entries_count"], 2);
    assert!(!tmp.path().join("a.txt").exists());
    assert!(!tmp.path().join("b.txt").exists());
}

#[test]
fn helper_read_project_artifact_reads_small_png_single_chunk_and_matches_metadata() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let png = [
        0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89,
    ];
    std::fs::write(tmp.path().join("tiny.png"), png).unwrap();
    let metadata_payload = json!({"path": "tiny.png", "max_bytes": 1024});
    let metadata = run_helper_locally(
        READ_PROJECT_ARTIFACT_METADATA_HELPER,
        &metadata_payload,
        tmp.path(),
    );
    let payload = json!({"path": "tiny.png", "offset": 0, "length": 1024});
    let out = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert_eq!(out["mime_type"], "image/png");
    assert_eq!(out["file_bytes"], png.len());
    assert_eq!(out["sha256"], metadata["sha256"]);
    assert_eq!(out["offset"], 0);
    assert_eq!(out["bytes_returned"], png.len());
    assert_eq!(out["next_offset"], png.len());
    assert_eq!(out["truncated"], false);
    assert_eq!(
        out["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
    );
}

#[test]
fn helper_read_project_artifact_reads_multiple_chunks() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let bytes = b"abcdefghijkl";
    std::fs::write(tmp.path().join("data.bin"), bytes).unwrap();

    let first_payload = json!({"path": "data.bin", "offset": 0, "length": 5});
    let first = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &first_payload, tmp.path());
    assert_eq!(first["file_bytes"], bytes.len());
    assert_eq!(first["offset"], 0);
    assert_eq!(first["bytes_returned"], 5);
    assert_eq!(first["next_offset"], 5);
    assert_eq!(first["truncated"], true);
    assert_eq!(
        first["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[..5])
    );

    let second_payload = json!({"path": "data.bin", "offset": 5, "length": 20});
    let second = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &second_payload, tmp.path());
    assert_eq!(second["sha256"], first["sha256"]);
    assert_eq!(second["offset"], 5);
    assert_eq!(second["bytes_returned"], bytes.len() - 5);
    assert_eq!(second["next_offset"], bytes.len());
    assert_eq!(second["truncated"], false);
    assert_eq!(
        second["content_base64"],
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes[5..])
    );
}

#[test]
fn helper_read_project_artifact_offset_at_eof_returns_empty_chunk() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("data.bin"), b"abc").unwrap();
    let payload = json!({"path": "data.bin", "offset": 3, "length": 10});
    let out = run_helper_locally(READ_PROJECT_ARTIFACT_HELPER, &payload, tmp.path());
    assert_eq!(out["file_bytes"], 3);
    assert_eq!(out["offset"], 3);
    assert_eq!(out["bytes_returned"], 0);
    assert_eq!(out["content_base64"], "");
    assert_eq!(out["next_offset"], 3);
    assert_eq!(out["truncated"], false);
}

#[test]
fn helper_read_project_artifact_rejects_invalid_offset_and_length() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("data.bin"), b"abc").unwrap();
    let bad_offset = run_helper_locally(
        READ_PROJECT_ARTIFACT_HELPER,
        &json!({"path": "data.bin", "offset": -1, "length": 1}),
        tmp.path(),
    );
    assert!(bad_offset["error"].as_str().unwrap().contains("offset"));
    let bad_length = run_helper_locally(
        READ_PROJECT_ARTIFACT_HELPER,
        &json!({"path": "data.bin", "offset": 0, "length": 0}),
        tmp.path(),
    );
    assert!(bad_length["error"].as_str().unwrap().contains("length"));
}

#[test]
fn helper_write_project_file_creates_new_file() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "line1\nline2\n",
        "overwrite": false,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], true);
    assert_eq!(out["overwritten"], false);
    assert_eq!(out["bytes_written"], 12);
    assert_eq!(out["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(out["warning"], Value::Null);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "line1\nline2\n"
    );
}

#[test]
fn helper_write_project_file_existing_without_overwrite_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "new",
        "overwrite": false,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("overwrite"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "original"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_matching_sha256_succeeds() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let sha = sha256_hex("original");
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "replaced",
        "overwrite": true,
        "expected_sha256": sha,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["overwritten"], true);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "replaced"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_mismatched_sha256_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "replaced",
        "overwrite": true,
        "expected_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("sha256"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "original"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_matching_prefix_succeeds() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "v1 content").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "v1 replaced",
        "overwrite": true,
        "expected_sha256": null,
        "expected_content_prefix": "v1 "
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["overwritten"], true);
    assert_eq!(out["warning"], Value::Null);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "v1 replaced"
    );
}

#[test]
fn helper_write_project_file_overwrite_with_mismatched_prefix_fails() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "v2 content").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "x",
        "overwrite": true,
        "expected_sha256": null,
        "expected_content_prefix": "v1 "
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("prefix"));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "v2 content"
    );
}

#[test]
fn helper_write_project_file_overwrite_without_guards_warns() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("EDIT_PROBE.txt"), "original").unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "replaced",
        "overwrite": true,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["overwritten"], true);
    assert!(
        out["warning"].as_str().unwrap().contains("expected_sha256"),
        "should warn about missing guard: {:?}",
        out["warning"]
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("EDIT_PROBE.txt")).unwrap(),
        "replaced"
    );
}

#[test]
fn helper_write_project_file_rejects_nul_content() {
    if !python3_available() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let payload = json!({
        "path": "EDIT_PROBE.txt",
        "content": "a\u{0000}b",
        "overwrite": false,
        "expected_sha256": null,
        "expected_content_prefix": null
    });
    let out = run_helper_locally(WRITE_PROJECT_FILE_HELPER, &payload, tmp.path());
    assert_eq!(out["created"], false);
    assert!(out["error"].as_str().unwrap().contains("NUL"));
    assert!(!tmp.path().join("EDIT_PROBE.txt").exists());
}
