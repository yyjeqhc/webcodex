use super::output::format_error;
use super::*;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
}

fn request(values: &[&str]) -> AdminCliRequest {
    let cmd = parse_admin_cli(&args(values)).unwrap();
    build_admin_request(&cmd).unwrap()
}

#[test]
fn admin_usage_keeps_rest_registration_commands_but_not_create_local() {
    let stdout = usage();
    assert!(!stdout.contains("create-local"));
    assert!(stdout.contains("webcodex tokens create"));
    assert!(stdout.contains("webcodex token register-hash"));
    assert!(stdout.contains("webcodex tokens register-hash"));
    assert!(stdout.contains("webcodex agent-tokens create"));
    assert!(stdout.contains("webcodex agent-token register-hash"));
    assert!(stdout.contains("webcodex agent-tokens register-hash"));
}

#[test]
fn users_create_builds_request_path_and_body() {
    let req = request(&[
        "users",
        "create",
        "--server-url",
        "https://example.test/",
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--display-name",
        "Alice",
        "--role",
        "user",
    ]);
    assert_eq!(req.server_url, "https://example.test");
    assert_eq!(req.path, "/api/users/create");
    assert_eq!(req.body["username"], "alice");
    assert_eq!(req.body["display_name"], "Alice");
    assert_eq!(req.body["role"], "user");
}

#[test]
fn tokens_create_builds_repeated_scopes() {
    let req = request(&[
        "tokens",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--name",
        "chatgpt-action",
        "--scope",
        "runtime:read",
        "--scope",
        "project:write",
    ]);
    assert_eq!(req.path, "/api/tokens/create");
    assert_eq!(req.body["username"], "alice");
    assert_eq!(req.body["name"], "chatgpt-action");
    assert_eq!(req.body["scopes"], json!(["runtime:read", "project:write"]));
}

#[test]
fn agent_tokens_create_defaults_agent_scopes() {
    let req = request(&[
        "agent-tokens",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
    ]);
    assert_eq!(req.path, "/api/agent-tokens/create");
    assert_eq!(req.body["username"], "alice");
    assert_eq!(req.body["client_id"], "alice-laptop");
    assert_eq!(
        req.body["scopes"],
        json!([
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update"
        ])
    );
}

#[test]
fn agent_tokens_create_supports_explicit_scopes() {
    let req = request(&[
        "agent-tokens",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
        "--scope",
        "agent:register",
        "--scope",
        "agent:poll",
    ]);
    assert_eq!(req.body["scopes"], json!(["agent:register", "agent:poll"]));
}

#[test]
fn agent_tokens_register_hash_builds_hash_registration_request() {
    let req = request(&[
        "agent-token",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--credential",
        "wc_acct_fake",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
        "--name",
        "alice laptop",
        "--hash",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--prefix",
        "wc_agent_aaaaaaa",
        "--scope",
        "agent:register",
        "--scope",
        "agent:poll",
    ]);
    assert_eq!(req.path, "/api/agent-tokens/register_hash");
    assert_eq!(req.token, "wc_acct_fake");
    assert_eq!(req.body["username"], "alice");
    assert_eq!(req.body["client_id"], "alice-laptop");
    assert_eq!(req.body["name"], "alice laptop");
    assert_eq!(
        req.body["token_hash"],
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
    assert_eq!(req.body["token_prefix"], "wc_agent_aaaaaaa");
    assert_eq!(req.body["scopes"], json!(["agent:register", "agent:poll"]));
    assert!(req.body.get("token").is_none());
}

#[test]
fn agent_tokens_register_hash_defaults_agent_scopes_and_prefers_admin_token() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    let req = request(&[
        "agent-tokens",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--admin-token",
        "fake-admin",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
        "--hash",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--prefix",
        "wc_agent_aaaaaaa",
    ]);
    assert_eq!(req.token, "fake-admin");
    assert_eq!(
        req.body["scopes"],
        json!([
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update"
        ])
    );
    std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[test]
fn agent_tokens_register_hash_uses_credential_env_and_default_account_credential() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("CUSTOM_ACCT", "wc_acct_custom");
    let req = request(&[
        "agent-tokens",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--credential-env",
        "CUSTOM_ACCT",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
        "--hash",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--prefix",
        "wc_agent_aaaaaaa",
    ]);
    assert_eq!(req.token, "wc_acct_custom");
    std::env::remove_var("CUSTOM_ACCT");

    std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    let req = request(&[
        "agent-tokens",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
        "--hash",
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "--prefix",
        "wc_agent_bbbbbbb",
    ]);
    assert_eq!(req.token, "wc_acct_default");
    std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[test]
fn list_and_revoke_commands_build_expected_requests() {
    let list = request(&[
        "tokens",
        "list",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
    ]);
    assert_eq!(list.path, "/api/tokens/list");
    assert_eq!(list.body, json!({"username": "alice"}));

    let revoke = request(&[
        "agent-tokens",
        "revoke",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--token-id",
        "tok-1",
    ]);
    assert_eq!(revoke.path, "/api/agent-tokens/revoke");
    assert_eq!(
        revoke.body,
        json!({"username": "alice", "token_id": "tok-1"})
    );
}

#[test]
fn token_file_is_read() {
    let tmp = tempfile::tempdir().unwrap();
    let token_file = tmp.path().join("token");
    std::fs::write(&token_file, "fake-file-token\n").unwrap();
    let cmd = parse_admin_cli(&args(&[
        "users",
        "list",
        "--server-url",
        "https://example.test",
        "--token-file",
        token_file.to_str().unwrap(),
    ]))
    .unwrap();
    let req = build_admin_request(&cmd).unwrap();
    assert_eq!(req.token, "fake-file-token");
}

#[test]
fn env_token_fallback_is_used() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_TOKEN", "fake-env-token");
    let cmd = parse_admin_cli(&args(&[
        "users",
        "list",
        "--server-url",
        "https://example.test",
    ]))
    .unwrap();
    let req = build_admin_request(&cmd).unwrap();
    assert_eq!(req.token, "fake-env-token");
    std::env::remove_var("WEBCODEX_TOKEN");
}

#[test]
fn explicit_admin_token_wins_over_default_account_credential_env() {
    let _guard = TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "fake-account-credential");
    let cmd = parse_admin_cli(&args(&[
        "tokens",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--admin-token",
        "fake-admin",
        "--username",
        "alice",
        "--hash",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--prefix",
        "wc_pat_aaaaaaaa",
    ]))
    .unwrap();
    let req = build_admin_request(&cmd).unwrap();
    assert_eq!(req.token, "fake-admin");
    std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[test]
fn auth_token_is_not_printed_in_error_output() {
    let msg = format_error(
        500,
        "application/json",
        r#"{"error":"bad fake-secret-token"}"#,
        "fake-secret-token",
    );
    assert!(!msg.contains("fake-secret-token"));
    assert!(msg.contains("[redacted]"));
}

#[test]
fn non_json_error_reports_status_and_content_type_without_body() {
    let body = "<html>".repeat(1000);
    let msg = format_error(502, "text/html; charset=utf-8", &body, "fake-admin");
    assert_eq!(
        msg,
        "request failed: HTTP 502 (content-type: text/html; charset=utf-8)"
    );
    assert!(!msg.contains("<html>"));
}

#[tokio::test]
async fn token_create_output_includes_plaintext_once_from_fake_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        let request_lower = request.to_ascii_lowercase();
        assert!(request.starts_with("POST /api/tokens/create "));
        assert!(request_lower.contains("authorization: bearer fake-admin"));
        assert!(request.contains(r#""scopes":["runtime:read"]"#));
        let body = r#"{"success":true,"token":"wc_fake_plaintext_once","token_id":"tok-1"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let cmd = parse_admin_cli(&args(&[
        "tokens",
        "create",
        "--server-url",
        &format!("http://{}", addr),
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--scope",
        "runtime:read",
    ]))
    .unwrap();
    let output = run_admin_command(cmd).await.unwrap();
    assert_eq!(output.matches("wc_fake_plaintext_once").count(), 1);
    handle.join().unwrap();
}
