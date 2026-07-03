use super::super::support::*;

#[test]
fn users_create_builds_admin_request_via_admin_cli() {
    // webcodex-cli users create ... reuses admin_cli parsing.
    let action = cli_action(args(&[
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
    ]));
    match action {
        CliAction::Admin(AdminCliCommand::UsersCreate(opts, user)) => {
            let req = build_admin_request(&AdminCliCommand::UsersCreate(opts, user)).unwrap();
            assert_eq!(req.server_url, "https://example.test");
            assert_eq!(req.path, "/api/users/create");
            assert_eq!(req.body["username"], "alice");
            assert_eq!(req.body["role"], "user");
        }
        other => panic!("expected Admin, got {other:?}"),
    }
}

#[test]
fn user_create_issue_credential_sets_request_field() {
    let action = cli_action(args(&[
        "user",
        "create",
        "--server",
        "https://example.test/",
        "--admin-token",
        "fake-admin",
        "--username",
        "alice",
        "--issue-credential",
    ]));
    match action {
        CliAction::Admin(AdminCliCommand::UsersCreate(opts, user)) => {
            let req = build_admin_request(&AdminCliCommand::UsersCreate(opts, user)).unwrap();
            assert_eq!(req.path, "/api/users/create");
            assert_eq!(req.token, "fake-admin");
            assert_eq!(req.body["issue_credential"], true);
        }
        other => panic!("expected UsersCreate, got {other:?}"),
    }
}

#[test]
fn token_generate_api_prints_token_hash_and_prefix() {
    let action = cli_action(args(&["token", "generate", "--kind", "api"]));
    match action {
        CliAction::TokenGenerate(opts) => {
            let out = render_token_generate(opts);
            assert!(out.contains("Token:\nwc_pat_"));
            assert!(out.contains("\nHash:\nsha256:"));
            assert!(out.contains("\nPrefix:\nwc_pat_"));
        }
        other => panic!("expected TokenGenerate, got {other:?}"),
    }
}

#[test]
fn token_generate_agent_prints_token_hash_and_prefix() {
    let action = cli_action(args(&["token", "generate", "--kind", "agent"]));
    match action {
        CliAction::TokenGenerate(opts) => {
            let out = render_token_generate(opts);
            assert!(out.contains("Token:\nwc_agent_"));
            assert!(out.contains("\nHash:\nsha256:"));
            assert!(out.contains("\nPrefix:\nwc_agent_"));
        }
        other => panic!("expected TokenGenerate, got {other:?}"),
    }
}

#[test]
fn credential_resolution_priority_is_explicit_then_env_name_then_default_env() {
    let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    std::env::set_var("CUSTOM_ACCT", "wc_acct_custom");
    assert_eq!(
        resolve_account_credential(&Some("wc_acct_explicit".to_string()), &None).unwrap(),
        "wc_acct_explicit"
    );
    assert_eq!(
        resolve_account_credential(&None, &Some("CUSTOM_ACCT".to_string())).unwrap(),
        "wc_acct_custom"
    );
    assert_eq!(
        resolve_account_credential(&None, &None).unwrap(),
        "wc_acct_default"
    );
    std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
    std::env::remove_var("CUSTOM_ACCT");
}

#[test]
fn token_register_hash_builds_hash_registration_request() {
    let action = cli_action(args(&[
        "token",
        "register-hash",
        "--server",
        "https://example.test",
        "--user",
        "alice",
        "--credential",
        "wc_acct_fake",
        "--name",
        "gpt-action",
        "--hash",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--prefix",
        "wc_pat_aaaaaaaa",
        "--scopes",
        "runtime:read,project:read",
    ]));
    match action {
        CliAction::Admin(AdminCliCommand::TokensRegisterHash(opts, t)) => {
            let req = build_admin_request(&AdminCliCommand::TokensRegisterHash(opts, t)).unwrap();
            assert_eq!(req.path, "/api/tokens/register_hash");
            assert_eq!(req.token, "wc_acct_fake");
            assert_eq!(req.body["username"], "alice");
            assert_eq!(req.body["name"], "gpt-action");
            assert_eq!(req.body["token_prefix"], "wc_pat_aaaaaaaa");
            assert_eq!(req.body["scopes"], json!(["runtime:read", "project:read"]));
        }
        other => panic!("expected TokensRegisterHash, got {other:?}"),
    }
}

#[test]
fn agent_token_register_hash_builds_hash_registration_request() {
    let action = cli_action(args(&[
        "agent-token",
        "register-hash",
        "--server",
        "https://example.test",
        "--user",
        "alice",
        "--credential",
        "wc_acct_fake",
        "--client-id",
        "alice-laptop",
        "--name",
        "alice laptop",
        "--hash",
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--prefix",
        "wc_agent_aaaaaaa",
        "--scopes",
        "agent:register,agent:poll",
    ]));
    match action {
        CliAction::Admin(AdminCliCommand::AgentTokensRegisterHash(opts, t)) => {
            let req =
                build_admin_request(&AdminCliCommand::AgentTokensRegisterHash(opts, t)).unwrap();
            assert_eq!(req.path, "/api/agent-tokens/register_hash");
            assert_eq!(req.token, "wc_acct_fake");
            assert_eq!(req.body["username"], "alice");
            assert_eq!(req.body["client_id"], "alice-laptop");
            assert_eq!(req.body["name"], "alice laptop");
            assert_eq!(req.body["token_prefix"], "wc_agent_aaaaaaa");
            assert_eq!(req.body["scopes"], json!(["agent:register", "agent:poll"]));
            assert!(req.body.get("token").is_none());
        }
        other => panic!("expected AgentTokensRegisterHash, got {other:?}"),
    }
}

#[test]
fn tokens_and_agent_tokens_commands_parse_to_admin() {
    let action = cli_action(args(&[
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
    ]));
    assert!(matches!(
        action,
        CliAction::Admin(AdminCliCommand::TokensCreate(_, _))
    ));

    let action = cli_action(args(&[
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
    ]));
    match action {
        CliAction::Admin(AdminCliCommand::AgentTokensCreate(_, t)) => {
            // Default agent scopes applied.
            assert_eq!(
                t.scopes,
                SETUP_AGENT_SCOPES
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            );
        }
        other => panic!("expected AgentTokensCreate, got {other:?}"),
    }

    let list = cli_action(args(&[
        "agent-tokens",
        "list",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
    ]));
    assert!(matches!(
        list,
        CliAction::Admin(AdminCliCommand::AgentTokensList(_, _))
    ));

    let revoke = cli_action(args(&[
        "tokens",
        "revoke",
        "--server-url",
        "https://example.test",
        "--token",
        "fake-admin",
        "--username",
        "alice",
        "--token-id",
        "tok-1",
    ]));
    assert!(matches!(
        revoke,
        CliAction::Admin(AdminCliCommand::TokensRevoke(_, _))
    ));
}

#[tokio::test]
async fn token_create_local_does_not_send_plaintext_token_to_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("POST /api/tokens/register_hash "));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer wc_acct_fake"));
        assert!(request.contains(r#""token_hash":"sha256:"#));
        assert!(request.contains(r#""token_prefix":"wc_pat_"#));
        assert!(!request.contains(r#""token":"wc_pat_"#));
        let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_pat_fake"}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let out = run_token_create_local(TokenCreateLocalOptions {
        server_url: format!("http://{}", addr),
        username: "alice".to_string(),
        credential: Some("wc_acct_fake".to_string()),
        credential_env: None,
        name: Some("gpt-action".to_string()),
        scopes: SETUP_GPT_SCOPES.iter().map(|s| s.to_string()).collect(),
    })
    .await
    .unwrap();
    assert_eq!(out.matches("wc_pat_").count(), 1);
    handle.join().unwrap();
}

#[tokio::test]
async fn agent_token_create_local_does_not_send_plaintext_token_to_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("POST /api/agent-tokens/register_hash "));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer wc_acct_fake"));
        assert!(request.contains(r#""token_hash":"sha256:"#));
        assert!(request.contains(r#""token_prefix":"wc_agent_"#));
        assert!(request.contains(r#""client_id":"alice-laptop""#));
        assert!(!request.contains(r#""token":"wc_agent_"#));
        let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_agent_fake","allowed_client_id":"alice-laptop"}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let out = run_agent_token_create_local(AgentTokenCreateLocalOptions {
        admin: AdminOptions {
            server_url: format!("http://{}", addr),
            credential: Some("wc_acct_fake".to_string()),
            ..AdminOptions::default()
        },
        username: "alice".to_string(),
        client_id: "alice-laptop".to_string(),
        name: Some("alice laptop".to_string()),
        scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
    })
    .await
    .unwrap();
    assert!(out.contains("Agent token created locally and registered with server."));
    assert!(out.contains("Client ID:\nalice-laptop"));
    assert_eq!(out.matches("wc_agent_").count(), 1);
    handle.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn agent_token_create_local_prefers_admin_token_over_default_account_credential() {
    let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer fake-admin"));
        let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_agent_fake","allowed_client_id":"alice-laptop"}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let out = run_agent_token_create_local(AgentTokenCreateLocalOptions {
        admin: AdminOptions {
            server_url: format!("http://{}", addr),
            token: Some("fake-admin".to_string()),
            ..AdminOptions::default()
        },
        username: "alice".to_string(),
        client_id: "alice-laptop".to_string(),
        name: None,
        scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
    })
    .await
    .unwrap();
    assert_eq!(out.matches("wc_agent_").count(), 1);
    handle.join().unwrap();
    std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[tokio::test(flavor = "current_thread")]
async fn agent_token_create_local_uses_default_account_credential() {
    let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_ACCOUNT_CREDENTIAL", "wc_acct_default");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer wc_acct_default"));
        let body = r#"{"success":true,"token":{"id":"tok-1","token_prefix":"wc_agent_fake","allowed_client_id":"alice-laptop"}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let out = run_agent_token_create_local(AgentTokenCreateLocalOptions {
        admin: AdminOptions {
            server_url: format!("http://{}", addr),
            ..AdminOptions::default()
        },
        username: "alice".to_string(),
        client_id: "alice-laptop".to_string(),
        name: None,
        scopes: SETUP_AGENT_SCOPES.iter().map(|s| s.to_string()).collect(),
    })
    .await
    .unwrap();
    assert_eq!(out.matches("wc_agent_").count(), 1);
    handle.join().unwrap();
    std::env::remove_var("WEBCODEX_ACCOUNT_CREDENTIAL");
}

#[test]
fn token_prefix_never_exposes_full_token() {
    let p = token_prefix("wc_abcdef0123456789");
    assert!(p.ends_with('…'));
    assert!(!p.contains("0123456789"));
    assert_eq!(p, "wc_abcde…");
}
