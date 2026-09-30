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

struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous: std::collections::BTreeMap<String, Option<std::ffi::OsString>>,
}

impl EnvGuard {
    fn new() -> Self {
        Self {
            _lock: TEST_ENV_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
            previous: std::collections::BTreeMap::new(),
        }
    }

    fn set(&mut self, name: &str, value: &str) {
        self.previous
            .entry(name.to_string())
            .or_insert_with(|| std::env::var_os(name));
        std::env::set_var(name, value);
    }

    fn remove(&mut self, name: &str) {
        self.previous
            .entry(name.to_string())
            .or_insert_with(|| std::env::var_os(name));
        std::env::remove_var(name);
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (name, value) in &self.previous {
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
        }
    }
}

#[test]
fn admin_usage_keeps_rest_registration_commands_but_not_create_local() {
    let stdout = usage();
    assert!(!stdout.contains("create-local"));
    assert!(stdout.contains("webcodex tokens create"));
    assert!(stdout.contains("webcodex tokens register-hash"));
    assert!(stdout.contains("webcodex runner-tokens create"));
    assert!(stdout.contains("webcodex runner-tokens register-hash"));
    assert!(!stdout.contains("webcodex agent-tokens create"));
    assert!(!stdout.contains("webcodex agent-tokens register-hash"));
    assert!(!stdout.contains("webcodex token register-hash"));
    assert!(!stdout.contains("webcodex agent-token register-hash"));
}

#[test]
fn admin_parser_rejects_retired_agent_tokens_group_with_migration_guidance() {
    assert!(!is_admin_group("agent-tokens"));
    for args in [
        vec!["agent-tokens".to_string()],
        vec!["agent-tokens".to_string(), "list".to_string()],
    ] {
        let error = parse_admin_cli(&args).unwrap_err();
        assert!(error.contains("agent-tokens was removed"), "{error}");
        assert!(error.contains("runner-tokens"), "{error}");
    }
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
fn runner_tokens_create_defaults_runner_scopes() {
    let req = request(&[
        "runner-tokens",
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
fn runner_tokens_create_supports_explicit_scopes() {
    let req = request(&[
        "runner-tokens",
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
fn runner_tokens_register_hash_builds_hash_registration_request() {
    let req = request(&[
        "runner-tokens",
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
fn runner_tokens_register_hash_defaults_runner_scopes_and_prefers_explicit_token() {
    let mut env = EnvGuard::new();
    env.set("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    let req = request(&[
        "runner-tokens",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--token",
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
    env.remove("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[test]
fn runner_tokens_register_hash_uses_credential_env_and_default_account_credential() {
    let mut env = EnvGuard::new();
    env.set("CUSTOM_ACCT", "wc_acct_custom");
    let req = request(&[
        "runner-tokens",
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
    env.remove("CUSTOM_ACCT");

    env.set("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    let req = request(&[
        "runner-tokens",
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
    env.remove("WEBCODEX_ACCOUNT_CREDENTIAL");
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
        "runner-tokens",
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
    let mut env = EnvGuard::new();
    env.set("WEBCODEX_TOKEN", "fake-env-token");
    let cmd = parse_admin_cli(&args(&[
        "users",
        "list",
        "--server-url",
        "https://example.test",
    ]))
    .unwrap();
    let req = build_admin_request(&cmd).unwrap();
    assert_eq!(req.token, "fake-env-token");
    env.remove("WEBCODEX_TOKEN");
}

#[test]
fn explicit_token_wins_over_default_account_credential_env() {
    let mut env = EnvGuard::new();
    env.set("WEBCODEX_ACCOUNT_CREDENTIAL", "fake-account-credential");
    let cmd = parse_admin_cli(&args(&[
        "tokens",
        "register-hash",
        "--server-url",
        "https://example.test",
        "--token",
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
    env.remove("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[test]
fn removed_admin_namespace_and_flag_aliases_are_rejected() {
    let cases = [
        args(&[
            "token",
            "list",
            "--server-url",
            "https://example.test",
            "--username",
            "alice",
        ]),
        args(&[
            "agent-token",
            "list",
            "--server-url",
            "https://example.test",
            "--username",
            "alice",
        ]),
        args(&[
            "tokens",
            "list",
            "--server",
            "https://example.test",
            "--username",
            "alice",
        ]),
        args(&[
            "tokens",
            "list",
            "--server-url",
            "https://example.test",
            "--admin-token",
            "fake-admin",
            "--username",
            "alice",
        ]),
        args(&[
            "tokens",
            "list",
            "--server-url",
            "https://example.test",
            "--admin-token-env",
            "ADMIN_TOKEN",
            "--username",
            "alice",
        ]),
        args(&[
            "tokens",
            "register-hash",
            "--server-url",
            "https://example.test",
            "--user",
            "alice",
        ]),
        args(&[
            "tokens",
            "register-hash",
            "--server-url",
            "https://example.test",
            "--username",
            "alice",
            "--token-hash",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ]),
        args(&[
            "tokens",
            "register-hash",
            "--server-url",
            "https://example.test",
            "--username",
            "alice",
            "--token-prefix",
            "wc_pat_aaaaaaa",
        ]),
    ];

    for case in cases {
        let error = parse_admin_cli(&case).expect_err("removed alias must fail closed");
        assert!(error.contains("unknown"), "{error}");
    }
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

#[test]
fn admin_proxy_controls_parse_and_validate() {
    let req = request(&[
        "users",
        "list",
        "--server-url",
        "http://server.invalid",
        "--proxy",
        "http://proxy.example:80",
        "--token",
        "fake-admin",
    ]);
    assert_eq!(
        req.server_http.proxy.as_deref(),
        Some("http://proxy.example:80")
    );
    assert!(!req.server_http.no_system_proxy);

    let error = parse_admin_cli(&args(&[
        "users",
        "list",
        "--server-url",
        "http://server.invalid",
        "--proxy",
        "http://127.0.0.1:7890",
        "--no-system-proxy",
        "--token",
        "fake-admin",
    ]))
    .unwrap_err();
    assert!(error.contains("mutually exclusive"));
}

#[tokio::test]
async fn admin_request_routes_through_explicit_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let proxy_addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("POST http://server.invalid:9/api/users/list HTTP/1.1"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer fake-admin"));
        let body = r#"{"success":true,"users":[]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });

    let proxy = format!("http://{proxy_addr}");
    let cmd = parse_admin_cli(&args(&[
        "users",
        "list",
        "--server-url",
        "http://server.invalid:9",
        "--proxy",
        &proxy,
        "--token",
        "fake-admin",
    ]))
    .unwrap();
    let output = run_admin_command(cmd).await.unwrap();
    assert!(output.contains("\"users\": []"));
    handle.join().unwrap();
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
        "--no-system-proxy",
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

// ---------------------------------------------------------------------------
// `webcodex oauth ...` — Server-side OAuth client administration.
//
// These are the commands an operator uses to register the redirect URI of
// another MCP client, and to widen a client's allowed scopes. Both are
// operator-authority operations, so the tests pin the request path, the body
// field names and the failure modes the Server contract depends on.
// ---------------------------------------------------------------------------

/// Read one complete HTTP request. A single `read` can return only the headers
/// when the body arrives in a later segment, and these tests assert on the
/// body, so accumulate until `content-length` is satisfied.
fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let mut raw = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&raw).to_string();
                if let Some(headers_end) = text.find("\r\n\r\n") {
                    let content_length = text
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if name.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    if raw.len() >= headers_end + 4 + content_length {
                        break;
                    }
                }
            }
        }
    }
    String::from_utf8_lossy(&raw).to_string()
}

/// Fake Server that answers the given `(status, body)` responses in order and
/// returns every request it saw, so a test can assert on the wire format.
fn spawn_fake_server(
    responses: Vec<(&'static str, String)>,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut seen = String::new();
        for (status, body) in responses {
            let (mut stream, _) = listener.accept().unwrap();
            seen.push_str(&read_http_request(&mut stream));
            seen.push('\n');
            write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
        seen
    });
    (format!("http://{addr}"), handle)
}

#[test]
fn oauth_is_an_admin_group_and_is_documented() {
    assert!(is_admin_group("oauth"));
    let stdout = usage();
    for command in [
        "webcodex oauth list",
        "webcodex oauth add-redirect-uri",
        "webcodex oauth remove-redirect-uri",
        "webcodex oauth update-scopes",
    ] {
        assert!(stdout.contains(command), "{command} missing from usage");
    }
}

#[test]
fn oauth_list_builds_the_client_list_request() {
    let req = request(&[
        "oauth",
        "list",
        "--server-url",
        "https://example.test/",
        "--token",
        "fake-admin",
    ]);
    assert_eq!(req.server_url, "https://example.test");
    assert_eq!(req.path, "/api/oauth/clients/list");
    assert_eq!(req.body, json!({}));
}

#[test]
fn oauth_redirect_uri_add_and_remove_target_distinct_endpoints() {
    let add = request(&[
        "oauth",
        "add-redirect-uri",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--redirect-uri",
        "https://chatgpt.com/connector_platform_oauth_redirect",
    ]);
    assert_eq!(add.path, "/api/oauth/clients/add_redirect_uri");
    assert_eq!(add.body["client_id"], "chatgpt");
    assert_eq!(
        add.body["redirect_uri"],
        "https://chatgpt.com/connector_platform_oauth_redirect"
    );

    let remove = request(&[
        "oauth",
        "remove-redirect-uri",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--redirect-uri",
        "https://chatgpt.com/connector_platform_oauth_redirect",
    ]);
    assert_eq!(remove.path, "/api/oauth/clients/remove_redirect_uri");
    // The two commands differ only by endpoint; a shared body keeps the
    // Server's add/remove handlers symmetric.
    assert_eq!(remove.body, add.body);
}

#[test]
fn oauth_redirect_uri_requires_both_client_and_uri() {
    let missing_uri = parse_admin_cli(&args(&[
        "oauth",
        "add-redirect-uri",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
    ]))
    .unwrap_err();
    assert!(missing_uri.contains("--redirect-uri"), "{missing_uri}");

    let missing_client = parse_admin_cli(&args(&[
        "oauth",
        "remove-redirect-uri",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--redirect-uri",
        "https://a.test/cb",
    ]))
    .unwrap_err();
    assert!(missing_client.contains("--client-id"), "{missing_client}");
}

#[test]
fn oauth_update_scopes_accepts_repeated_and_comma_separated_scopes() {
    let repeated = request(&[
        "oauth",
        "update-scopes",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--scope",
        "runtime:read",
        "--scope",
        "project:write",
    ]);
    assert_eq!(repeated.path, "/api/oauth/clients/update_scopes");
    assert_eq!(repeated.body["client_id"], "chatgpt");
    assert_eq!(
        repeated.body["allowed_scopes"],
        json!(["runtime:read", "project:write"])
    );

    // `--scopes` tolerates surrounding whitespace and empty entries so a
    // hand-written comma list cannot smuggle an empty scope into the client.
    let comma = request(&[
        "oauth",
        "update-scopes",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--scopes",
        " runtime:read , project:write ,, ",
    ]);
    assert_eq!(
        comma.body["allowed_scopes"],
        json!(["runtime:read", "project:write"])
    );
}

#[test]
fn oauth_update_scopes_requires_exactly_one_scope_source() {
    let none = parse_admin_cli(&args(&[
        "oauth",
        "update-scopes",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
    ]))
    .unwrap_err();
    assert!(
        none.contains("--scope SCOPE, --scopes A,B, or --all-scopes"),
        "{none}"
    );

    let both = parse_admin_cli(&args(&[
        "oauth",
        "update-scopes",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--all-scopes",
        "--scope",
        "runtime:read",
    ]))
    .unwrap_err();
    assert!(
        both.contains("only one of --all-scopes or --scope/--scopes"),
        "{both}"
    );
}

#[tokio::test]
async fn oauth_all_scopes_expands_against_the_live_discovery_document() {
    // The Server owns the grantable scope registry. `--all-scopes` resolves
    // against its discovery document so the CLI never ships a copy that drifts.
    let (server_url, handle) = spawn_fake_server(vec![
        (
            "200 OK",
            r#"{"scopes_supported":["runtime:read","offline_access","project:write","runtime:read"]}"#
                .to_string(),
        ),
        ("200 OK", r#"{"success":true,"changed":true}"#.to_string()),
    ]);

    let cmd = parse_admin_cli(&args(&[
        "oauth",
        "update-scopes",
        "--server-url",
        &server_url,
        "--no-system-proxy",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--all-scopes",
    ]))
    .unwrap();
    let output = run_admin_command(cmd).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    assert_eq!(parsed["changed"], json!(true));

    let seen = handle.join().unwrap();
    assert!(
        seen.contains("GET /.well-known/oauth-authorization-server "),
        "{seen}"
    );
    assert!(
        seen.contains("POST /api/oauth/clients/update_scopes "),
        "{seen}"
    );
    // `offline_access` is a protocol scope, not a grantable permission, so it
    // must be dropped; duplicates collapse and the result is sorted.
    assert!(
        seen.contains(r#""allowed_scopes":["project:write","runtime:read"]"#),
        "{seen}"
    );
    assert!(!seen.contains("offline_access"), "{seen}");
}

#[tokio::test]
async fn oauth_all_scopes_fails_closed_when_the_discovery_document_is_unusable() {
    let cases = [
        ("404 Not Found", r#"{"error":"not_found"}"#, "HTTP 404"),
        (
            "200 OK",
            r#"{"issuer":"https://example.test"}"#,
            "omitted scopes_supported",
        ),
        (
            "200 OK",
            r#"{"scopes_supported":["offline_access"]}"#,
            "no grantable scopes",
        ),
    ];
    for (status, body, expected) in cases {
        let (server_url, handle) = spawn_fake_server(vec![(status, body.to_string())]);
        let cmd = parse_admin_cli(&args(&[
            "oauth",
            "update-scopes",
            "--server-url",
            &server_url,
            "--no-system-proxy",
            "--token",
            "fake-admin",
            "--client-id",
            "chatgpt",
            "--all-scopes",
        ]))
        .unwrap();
        // Never degrade to "update the client with an empty scope set".
        let error = run_admin_command(cmd).await.unwrap_err();
        assert!(error.contains(expected), "expected {expected}, got {error}");
        handle.join().unwrap();
    }
}

#[test]
fn oauth_rejects_unknown_flags() {
    let unknown = parse_admin_cli(&args(&[
        "oauth",
        "list",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--scope",
        "runtime:read",
    ]))
    .unwrap_err();
    assert!(unknown.contains("unknown oauth list flag"), "{unknown}");

    let unknown_scopes = parse_admin_cli(&args(&[
        "oauth",
        "update-scopes",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt",
        "--all",
    ]))
    .unwrap_err();
    assert!(
        unknown_scopes.contains("unknown oauth update-scopes flag"),
        "{unknown_scopes}"
    );
}

#[test]
fn oauth_commands_require_a_first_party_token_not_an_account_credential() {
    // OAuth client-management routes are FirstPartyOnly on the Server:
    // Bootstrap and personal API tokens are accepted, AccountCredential is not.
    let mut env = EnvGuard::new();
    env.remove("WEBCODEX_TOKEN");
    env.set("WEBCODEX_ACCOUNT_CREDENTIAL", "fake-account-credential");

    let cmd = parse_admin_cli(&args(&[
        "oauth",
        "list",
        "--server-url",
        "https://example.test",
        "--no-system-proxy",
    ]))
    .unwrap();
    let error = build_admin_request(&cmd).unwrap_err();
    assert!(error.contains("WEBCODEX_TOKEN"), "{error}");

    // An explicit first-party token must win even when the unrelated account
    // credential environment is present.
    let req = request(&[
        "oauth",
        "list",
        "--server-url",
        "https://example.test",
        "--no-system-proxy",
        "--token",
        "fake-first-party-token",
    ]);
    assert_eq!(req.token, "fake-first-party-token");
}
#[test]
fn oauth_create_builds_the_client_create_request() {
    let req = request(&[
        "oauth",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--name",
        "ChatGPT MCP",
        "--redirect-uri",
        "https://chatgpt.com/connector_platform_oauth_redirect",
        "--redirect-uri",
        "https://example.test/callback",
        "--scope",
        "runtime:read",
        "--scopes",
        "project:write,job:run",
    ]);
    assert_eq!(req.path, "/api/oauth/clients/create");
    assert_eq!(req.body["name"], "ChatGPT MCP");
    assert_eq!(
        req.body["redirect_uris"],
        json!([
            "https://chatgpt.com/connector_platform_oauth_redirect",
            "https://example.test/callback"
        ])
    );
    assert_eq!(
        req.body["allowed_scopes"],
        json!(["runtime:read", "project:write", "job:run"])
    );
}

#[test]
fn oauth_create_requires_name_redirect_uri_and_exactly_one_scope_source() {
    let missing_name = parse_admin_cli(&args(&[
        "oauth",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--redirect-uri",
        "https://a.test/cb",
        "--all-scopes",
    ]))
    .unwrap_err();
    assert!(missing_name.contains("--name"), "{missing_name}");

    let missing_uri = parse_admin_cli(&args(&[
        "oauth",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--name",
        "n",
        "--all-scopes",
    ]))
    .unwrap_err();
    assert!(missing_uri.contains("--redirect-uri"), "{missing_uri}");

    let both = parse_admin_cli(&args(&[
        "oauth",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--name",
        "n",
        "--redirect-uri",
        "https://a.test/cb",
        "--all-scopes",
        "--scope",
        "runtime:read",
    ]))
    .unwrap_err();
    assert!(both.contains("only one of --all-scopes"), "{both}");

    let none = parse_admin_cli(&args(&[
        "oauth",
        "create",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--name",
        "n",
        "--redirect-uri",
        "https://a.test/cb",
    ]))
    .unwrap_err();
    assert!(none.contains("--scope SCOPE"), "{none}");
}

#[test]
fn oauth_show_targets_the_client_list_then_renders_a_panel() {
    let cmd = parse_admin_cli(&args(&[
        "oauth",
        "show",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--client-id",
        "chatgpt-client",
        "--secret-file",
        "/tmp/secret",
        "--pat-file",
        "/tmp/pat",
    ]))
    .unwrap();
    let req = build_admin_request(&cmd).unwrap();
    assert_eq!(req.path, "/api/oauth/clients/list");
    assert!(matches!(
        cmd,
        AdminCliCommand::OAuthShow(_, OAuthShowArgs { .. })
    ));

    let list = json!({
        "success": true,
        "clients": [{
            "client_id": "chatgpt-client",
            "allowed_scopes": ["runtime:read", "project:write"],
            "redirect_uris": ["https://chatgpt.com/connector_platform_oauth_redirect"],
        }]
    });
    // Without local secret/pat files the panel still renders, marking the
    // two create-only / operator-only lines as not provided.
    let panel = super::commands::render_oauth_connection_panel(
        "https://example.test",
        &OAuthShowArgs {
            client_id: "chatgpt-client".into(),
            secret_file: None,
            pat_file: None,
        },
        &list,
    )
    .unwrap();
    assert!(panel.contains("MCP URL                  https://example.test/mcp"));
    assert!(panel.contains("Client ID                chatgpt-client"));
    assert!(panel.contains("<not provided; pass --secret-file>"));
    assert!(panel.contains("Authorization Endpoint   https://example.test/oauth/authorize"));
    assert!(panel.contains("1) https://chatgpt.com/connector_platform_oauth_redirect"));
    assert!(!panel.contains("wc_boot_"));
}

#[test]
fn oauth_show_rejects_an_unknown_client() {
    let list = json!({"success": true, "clients": []});
    let error = super::commands::render_oauth_connection_panel(
        "https://example.test",
        &OAuthShowArgs {
            client_id: "missing".into(),
            secret_file: None,
            pat_file: None,
        },
        &list,
    )
    .unwrap_err();
    assert!(error.contains("not found"), "{error}");
}
