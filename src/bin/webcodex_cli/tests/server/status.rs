use super::super::support::*;

#[test]
fn build_revision_compare_matches_same_commit() {
    assert_eq!(
        compare_build_commits(Some("81f322d5b580"), Some("81f322d5b580")),
        RevisionComparison::Match
    );
}

#[test]
fn build_revision_compare_matches_prefix_commit() {
    assert_eq!(
        compare_build_commits(Some("81f322d5b580"), Some("81f322d")),
        RevisionComparison::Match
    );
    assert_eq!(
        compare_build_commits(Some("81f322d"), Some("81f322d5b580")),
        RevisionComparison::Match
    );
}

#[test]
fn build_revision_compare_reports_mismatch() {
    assert_eq!(
        compare_build_commits(Some("81f322d5b580"), Some("fd156ba92fc7")),
        RevisionComparison::Mismatch {
            local: "81f322d5b580".to_string(),
            remote: "fd156ba92fc7".to_string(),
        }
    );
}

#[test]
fn build_revision_compare_reports_unknown() {
    assert!(matches!(
        compare_build_commits(Some("81f322d5b580"), Some("unknown")),
        RevisionComparison::Unknown { reason } if reason.contains("server runtime did not report")
    ));
    assert!(matches!(
        compare_build_commits(Some(""), Some("81f322d5b580")),
        RevisionComparison::Unknown { reason } if reason.contains("local CLI did not report")
    ));
}

#[test]
fn server_status_includes_remote_build_metadata() {
    let output = json!({
        "version": "0.1.0",
        "build": {
            "git_commit": "81f322d5b580",
            "git_dirty": false,
            "built_at": "1782739890"
        }
    });
    let build = runtime_build_metadata(Some(&output));
    let rendered = render_build_metadata_block("Server build", &build);
    assert!(rendered.contains("Server build:"));
    assert!(rendered.contains("version:    0.1.0"));
    assert!(rendered.contains("commit:     81f322d5b580"));
    assert!(rendered.contains("dirty:      false"));
    assert!(rendered.contains("built_at:   1782739890"));
}

#[test]
fn server_status_reports_revision_match() {
    let local = build_metadata(Some("81f322d5b580"));
    let remote = build_metadata(Some("81f322d"));
    let comparison =
        compare_build_commits(local.git_commit.as_deref(), remote.git_commit.as_deref());
    assert_eq!(comparison, RevisionComparison::Match);
    assert!(server_status_revision_check(&comparison).starts_with("ok:"));
}

#[test]
fn server_status_reports_revision_mismatch() {
    let comparison = compare_build_commits(Some("81f322d5b580"), Some("fd156ba92fc7"));
    let detail = server_status_revision_check(&comparison);
    assert!(detail.starts_with("warning:"));
    assert!(detail.contains("local CLI commit 81f322d5b580"));
    assert!(detail.contains("server runtime commit fd156ba92fc7"));
    assert!(detail.contains("deploy/update one side before debugging old behavior"));
}

#[test]
fn server_status_handles_missing_remote_build_metadata() {
    let output = json!({"version":"0.1.0"});
    let remote = runtime_build_metadata(Some(&output));
    let comparison = compare_build_commits(Some("81f322d5b580"), remote.git_commit.as_deref());
    let detail = server_status_revision_check(&comparison);
    assert!(detail.starts_with("unknown:"));
    assert!(detail.contains("server runtime did not report build.git_commit"));
    assert!(detail.contains("server may be older than build metadata support"));
}

#[tokio::test]
async fn server_status_parses_env_token_posts_and_does_not_print_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 16384];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        tx.send(request.clone()).unwrap();
        let body = r#"{"success":true,"output":{"service":"webcodex","auth_enabled":true,"configured_public_url":"https://example.test","tools":{"count":12},"agents":{"online_count":2}}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let token = "secret-status-token";
    std::fs::write(&env_file, format!("WEBCODEX_TOKEN={}\n", token)).unwrap();
    let opts = parse_server_status(&args(&[
        "--url",
        &format!("http://{}", addr),
        "--env-file",
        env_file.to_str().unwrap(),
    ]))
    .unwrap();
    let output = run_server_status(opts).await.unwrap();
    handle.join().unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/runtime/status "));
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer secret-status-token"));
    assert!(!output.contains(token));
    assert!(output.contains("HTTP reachable:        yes"));
    assert!(output.contains("auth_enabled:          true"));
    assert!(output.contains("configured_public_url: https://example.test"));
    assert!(output.contains("tools.count:           12"));
    assert!(output.contains("agents.online_count:   2"));
}

#[tokio::test]
async fn server_status_token_file_takes_priority_over_env_file() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 16384];
        let n = stream.read(&mut buf).unwrap();
        tx.send(String::from_utf8_lossy(&buf[..n]).to_string())
            .unwrap();
        let body = r#"{"success":true,"output":{"auth_enabled":true,"configured_public_url":null,"tools":{"count":0},"agents":{"online_count":0}}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let token_file = tmp.path().join("token");
    std::fs::write(&env_file, "WEBCODEX_TOKEN=env-token\n").unwrap();
    std::fs::write(&token_file, "file-token\n").unwrap();
    let opts = parse_server_status(&args(&[
        "--url",
        &format!("http://{}", addr),
        "--env-file",
        env_file.to_str().unwrap(),
        "--token-file",
        token_file.to_str().unwrap(),
        "--json",
    ]))
    .unwrap();
    let output = run_server_status(opts).await.unwrap();
    handle.join().unwrap();
    let request = rx.recv().unwrap();
    assert!(request
        .to_ascii_lowercase()
        .contains("authorization: bearer file-token"));
    assert!(!request
        .to_ascii_lowercase()
        .contains("authorization: bearer env-token"));
    assert!(!output.contains("file-token"));
    assert!(!output.contains("env-token"));
}

#[tokio::test]
async fn server_status_connection_failure_reports_unreachable_without_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let token = "connection-failure-token";
    std::fs::write(&env_file, format!("WEBCODEX_TOKEN={}\n", token)).unwrap();
    let opts = parse_server_status(&args(&[
        "--url",
        &format!("http://{}", addr),
        "--env-file",
        env_file.to_str().unwrap(),
    ]))
    .unwrap();
    let output = run_server_status(opts).await.unwrap();
    assert!(output.contains("HTTP reachable:        no"));
    assert!(output.contains("HTTP error:"));
    assert!(!output.contains(token));
}

#[tokio::test]
async fn server_status_non_json_error_reports_status_and_content_type_only() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 4096];
        let _ = stream.read(&mut buf).unwrap();
        let body = "secret body should not be printed";
        write!(
            stream,
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/html; charset=utf-8\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let opts = parse_server_status(&args(&["--url", &format!("http://{}", addr)])).unwrap();
    let output = run_server_status(opts).await.unwrap();
    handle.join().unwrap();
    assert!(output.contains("HTTP reachable:        no"));
    assert!(output.contains("HTTP status:           502"));
    assert!(output.contains("HTTP content-type:     text/html; charset=utf-8"));
    assert!(!output.contains("secret body"));
}

#[test]
fn non_json_error_reports_status_and_content_type_only() {
    let body = "<html>".repeat(500);
    let msg = format_error_body(502, "text/html; charset=utf-8", &body);
    assert_eq!(
        msg,
        "request failed: HTTP 502 (content-type: text/html; charset=utf-8)"
    );
    assert!(!msg.contains("<html>"));
}

#[test]
fn token_not_printed_in_json_error() {
    // Simulate a server error body that echoes the token; the formatter
    // must surface the error text but must never have received the bearer
    // token to echo in the first place. We assert the helper does not add
    // any token of its own.
    let msg = format_error_body(500, "application/json", r#"{"error":"bad request"}"#);
    assert!(msg.contains("HTTP 500"));
    assert!(msg.contains("bad request"));
    assert!(!msg.contains("fake-secret"));
}
