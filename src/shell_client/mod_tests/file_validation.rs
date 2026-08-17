use super::*;

#[test]
fn validate_file_request_rejects_invalid_read_requests() {
    let cases: Vec<(&str, fn(&mut ShellFileOpRequest), &str)> = vec![
        (
            "only start_line",
            |req| req.start_line = Some(10),
            "end_line is required when start_line is set for op=read",
        ),
        (
            "only end_line",
            |req| req.end_line = Some(20),
            "start_line is required when end_line is set for op=read",
        ),
        (
            "inverted line range",
            |req| {
                req.start_line = Some(20);
                req.end_line = Some(10);
            },
            "invalid line range",
        ),
        (
            "zero start_line",
            |req| {
                req.start_line = Some(0);
                req.end_line = Some(10);
            },
            "invalid line range",
        ),
        (
            "old_text field on read",
            |req| req.old_text = Some("old".to_string()),
            "old_text is not supported for any file op",
        ),
        (
            "pattern field on read",
            |req| req.pattern = Some("needle".to_string()),
            "pattern is not supported for any file op",
        ),
        (
            "line field on read",
            |req| req.line = Some(10),
            "line is not supported for any file op",
        ),
        (
            "expected_prefix on read",
            |req| req.expected_prefix = Some("pub fn".to_string()),
            "expected_prefix is only allowed for op=write",
        ),
    ];

    for (case, mutate, expected) in cases {
        let mut req = file_request("read");
        mutate(&mut req);
        let err = validate_file_request(&req).unwrap_err();
        assert_eq!(err, expected, "case: {case}");
    }
}

#[test]
fn validate_file_request_allows_structured_edit_payload_ops() {
    for op in ["write_project_file", "apply_text_edits"] {
        let mut req = file_request(op);
        req.content = Some(r#"{"path":"src/lib.rs"}"#.to_string());

        validate_file_request(&req).unwrap();
    }
}

#[test]
fn validate_file_request_rejects_structured_edit_extra_fields() {
    let mut req = file_request("write_project_file");
    req.content = Some(r#"{"path":"src/lib.rs"}"#.to_string());
    req.expected_sha256 =
        Some("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string());

    let err = validate_file_request(&req).unwrap_err();
    assert!(err.contains("expected_sha256 is only allowed"), "{err}");
}
