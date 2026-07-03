use super::support::*;

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
fn agent_init_writes_valid_toml_and_refuses_overwrite() {
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("agent.toml");
    let opts = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test/",
        "--token",
        "agent_fake_test_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--display-name",
        "Alice Laptop",
        "--allowed-root",
        "/srv/projects",
        "--output",
        output.to_str().unwrap(),
    ]))
    .unwrap();
    let msg = run_agent_init(opts).unwrap();
    assert!(msg.contains("agent.toml"));

    // Refuse overwrite without --overwrite.
    let opts2 = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test/",
        "--token",
        "agent_fake_test_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        output.to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_agent_init(opts2).unwrap_err();
    assert!(err.contains("already exists"));
}

#[test]
fn agent_init_stdout_output_contains_token_only_once() {
    let opts = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token",
        "agent_fake_stdout_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let content = run_agent_init(opts).unwrap();
    assert_eq!(content.matches("agent_fake_stdout_token").count(), 1);
}

#[cfg(unix)]
#[test]
fn agent_init_writes_0600_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let output = tmp.path().join("agent.toml");
    let opts = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token",
        "agent_fake_perms_token",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        output.to_str().unwrap(),
    ]))
    .unwrap();
    run_agent_init(opts).unwrap();
    let mode = std::fs::metadata(&output).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn agent_init_token_file_and_env_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let token_file = tmp.path().join("agent.token");
    std::fs::write(&token_file, "agent_fake_file_token\n").unwrap();
    let opts = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token-file",
        token_file.to_str().unwrap(),
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let content = run_agent_init(opts).unwrap();
    assert!(content.contains("agent_fake_file_token"));

    let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_AGENT_TOKEN", "agent_fake_env_token");
    let opts = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let content = run_agent_init(opts).unwrap();
    assert!(content.contains("agent_fake_env_token"));
    std::env::remove_var("WEBCODEX_AGENT_TOKEN");
}

#[test]
fn agent_init_empty_tokens_are_rejected() {
    let opts = parse_cli_agent_init(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--token",
        "   ",
        "--client-id",
        "alice-laptop",
        "--owner",
        "alice",
        "--allowed-root",
        "/srv/projects",
        "--output",
        "-",
    ]))
    .unwrap();
    let err = run_agent_init(opts).unwrap_err();
    assert!(err.contains("--token cannot be empty"), "{err}");
}

#[test]
fn agent_init_allows_empty_allowed_roots_with_home_default() {
    let _guard = agent_init::TEST_ENV_LOCK.lock().unwrap();
    let home = std::env::var_os("HOME");
    if home.is_some() {
        let opts = parse_cli_agent_init(&args(&[
            "--server-url",
            "https://v4.example.test",
            "--token",
            "agent_fake_home_token",
            "--client-id",
            "alice-laptop",
            "--owner",
            "alice",
            "--output",
            "-",
        ]))
        .unwrap();
        let content = run_agent_init(opts).unwrap();
        let home = std::env::var_os("HOME").unwrap();
        assert!(content.contains(&home.to_string_lossy().to_string()));
    }
}

#[test]
fn setup_single_user_parse_validates_required_fields() {
    let err = parse_setup_single_user(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "fake-bootstrap",
        "--username",
        "yyjeqhc",
        // missing --client-id and --output-dir
    ]))
    .unwrap_err();
    assert!(err.contains("--client-id is required"));
}

#[test]
fn setup_single_user_parse_defaults() {
    let opts = parse_setup_single_user(&args(&[
        "--server-url",
        "https://example.test",
        "--token",
        "fake-bootstrap",
        "--username",
        "yyjeqhc",
        "--client-id",
        "oe",
        "--output-dir",
        "/tmp/webcodex-setup-test",
    ]))
    .unwrap();
    assert_eq!(opts.role, "admin");
    assert_eq!(opts.gpt_token_name, "chatgpt-action");
    assert_eq!(opts.agent_token_name, "oe agent");
}

#[test]
fn server_init_parse_defaults() {
    let opts = parse_server_init(&args(&[])).unwrap();
    assert_eq!(opts.listen, "127.0.0.1:8080");
    if is_effective_root() {
        assert_eq!(opts.data_dir, PathBuf::from("/var/lib/webcodex"));
        assert_eq!(opts.env_file, PathBuf::from("/etc/webcodex/webcodex.env"));
    } else {
        assert!(opts.data_dir.ends_with(".local/share/webcodex"));
        assert!(opts.env_file.ends_with(".config/webcodex/webcodex.env"));
    }
    assert!(!opts.overwrite);
    assert!(!opts.json);
}

#[test]
fn server_init_writes_env_file_and_0600_permissions() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("etc/webcodex.env");
    let data_dir = tmp.path().join("data");
    let opts = parse_server_init(&args(&[
        "--listen",
        "127.0.0.1:9090",
        "--data-dir",
        data_dir.to_str().unwrap(),
        "--env-file",
        env_file.to_str().unwrap(),
        "--public-url",
        "https://example.test/",
    ]))
    .unwrap();
    let output = run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    assert!(content.contains("WEBCODEX_ADDR=127.0.0.1:9090\n"));
    assert!(content.contains(&format!("WEBCODEX_DATA={}\n", data_dir.display())));
    assert!(content.contains("WEBCODEX_TOKEN=wc_boot_"));
    assert!(content.contains("WEBCODEX_PUBLIC_URL=https://example.test\n"));
    let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
    assert!(!output.contains(&token));
    assert!(output.contains("token prefix:"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&env_file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn server_init_refuses_overwrite_unless_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    std::fs::write(&env_file, "WEBCODEX_TOKEN=old\n").unwrap();
    let mut opts = parse_server_init(&args(&[
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_server_init(opts.clone()).unwrap_err();
    assert!(err.contains("already exists"));
    opts.overwrite = true;
    run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    assert!(content.contains("WEBCODEX_ADDR="));
    assert!(!content.contains("WEBCODEX_TOKEN=old"));
}

#[test]
fn server_init_json_output_does_not_include_full_token() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let opts = parse_server_init(&args(&[
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().to_str().unwrap(),
        "--json",
    ]))
    .unwrap();
    let output = run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
    assert!(!output.contains(&token));
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["wrote_env_file"], true);
    assert!(json["token_prefix"]
        .as_str()
        .unwrap()
        .starts_with("wc_boot"));
    assert!(json.get("token").is_none());
}

#[test]
fn server_init_output_stdout_explicitly_prints_env_contents_with_token() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let opts = parse_server_init(&args(&[
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().to_str().unwrap(),
        "--output",
        "-",
    ]))
    .unwrap();
    let output = run_server_init(opts).unwrap();
    let content = std::fs::read_to_string(&env_file).unwrap();
    let token = parse_env_content_value(&content, "WEBCODEX_TOKEN").unwrap();
    assert_eq!(output, content);
    assert!(output.contains(&format!("WEBCODEX_TOKEN={}", token)));
    assert!(server_init_usage().contains("including the full WEBCODEX_TOKEN"));
}

#[test]
fn install_service_generates_expected_unit_without_tokens() {
    let opts = parse_server_install_service(&args(&[
        "--env-file",
        "/etc/webcodex/webcodex.env",
        "--bin",
        "/usr/local/bin/webcodex",
        "--working-directory",
        "/var/lib/webcodex",
        "--user",
        "webcodex",
        "--group",
        "webcodex",
        "--dry-run",
    ]))
    .unwrap();
    let unit = run_server_install_service(opts).unwrap();
    assert!(unit.contains("[Unit]\nDescription=WebCodex Runtime\n"));
    assert!(unit.contains("EnvironmentFile=/etc/webcodex/webcodex.env\n"));
    assert!(unit.contains("ExecStart=/usr/local/bin/webcodex\n"));
    assert!(unit.contains("WorkingDirectory=/var/lib/webcodex\n"));
    assert!(unit.contains("User=webcodex\n"));
    assert!(unit.contains("Group=webcodex\n"));
    assert!(!unit.contains("WEBCODEX_TOKEN"));
    assert!(!unit.contains("wc_boot_"));
}

#[test]
fn install_service_refuses_overwrite_unless_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let service_file = tmp.path().join("webcodex.service");
    std::fs::write(&service_file, "old").unwrap();
    let opts = parse_server_install_service(&args(&[
        "--bin",
        "/usr/local/bin/webcodex",
        "--service-file",
        service_file.to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_server_install_service(opts).unwrap_err();
    assert!(err.contains("already exists"));
}

#[test]
fn install_service_dry_run_and_output_work_without_systemd() {
    let dry =
        parse_server_install_service(&args(&["--bin", "/usr/local/bin/webcodex", "--dry-run"]))
            .unwrap();
    assert!(run_server_install_service(dry)
        .unwrap()
        .contains("ExecStart=/usr/local/bin/webcodex"));

    let out = parse_server_install_service(&args(&[
        "--bin",
        "/usr/local/bin/webcodex",
        "--output",
        "-",
        "--json",
    ]))
    .unwrap();
    let json: Value = serde_json::from_str(&run_server_install_service(out).unwrap()).unwrap();
    assert_eq!(json["dry_run"], true);
    assert!(json["unit"].as_str().unwrap().contains("[Service]"));
}

#[test]
fn agent_install_service_generates_expected_unit_without_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("agent.toml");
    std::fs::write(&config, "token = \"agent_secret_should_not_print\"\n").unwrap();
    let opts = parse_agent_install_service(&args(&[
        "--config",
        config.to_str().unwrap(),
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
        "--working-directory",
        "/root",
        "--user",
        "webcodex",
        "--group",
        "webcodex",
        "--dry-run",
    ]))
    .unwrap();
    let unit = run_agent_install_service(opts).unwrap();
    assert!(unit.contains("[Unit]\nDescription=WebCodex Agent\n"));
    assert!(unit.contains(&format!(
        "ExecStart=/opt/webcodex/bin/webcodex-agent --config {}\n",
        config.display()
    )));
    assert!(unit.contains("Restart=on-failure\n"));
    assert!(unit.contains("RestartSec=3\n"));
    assert!(unit.contains("WorkingDirectory=/root\n"));
    assert!(unit.contains("User=webcodex\n"));
    assert!(unit.contains("Group=webcodex\n"));
    assert!(!unit.contains("agent_secret_should_not_print"));
    assert!(!unit.contains("Authorization"));
    assert!(!unit.contains("token ="));
}

#[test]
fn agent_install_service_refuses_overwrite_unless_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let service_file = tmp.path().join("webcodex-agent.service");
    std::fs::write(&service_file, "old").unwrap();
    let opts = parse_agent_install_service(&args(&[
        "--config",
        "/etc/webcodex/agent.toml",
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
        "--service-file",
        service_file.to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_agent_install_service(opts).unwrap_err();
    assert!(err.contains("already exists"));
}

#[test]
fn agent_install_service_dry_run_and_output_work_without_systemd() {
    let dry = parse_agent_install_service(&args(&[
        "--config",
        "/etc/webcodex/agent.toml",
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
        "--dry-run",
    ]))
    .unwrap();
    assert!(run_agent_install_service(dry)
        .unwrap()
        .contains("ExecStart=/opt/webcodex/bin/webcodex-agent --config /etc/webcodex/agent.toml"));

    let out = parse_agent_install_service(&args(&[
        "--config",
        "/etc/webcodex/agent.toml",
        "--bin",
        "/opt/webcodex/bin/webcodex-agent",
        "--output",
        "-",
        "--json",
    ]))
    .unwrap();
    let json: Value = serde_json::from_str(&run_agent_install_service(out).unwrap()).unwrap();
    assert_eq!(json["dry_run"], true);
    assert!(json["unit"]
        .as_str()
        .unwrap()
        .contains("ExecStart=/opt/webcodex/bin/webcodex-agent --config /etc/webcodex/agent.toml"));
}

#[test]
fn agent_status_parses_agent_toml_without_printing_token_and_systemd_unknown() {
    let _guard = admin_cli::TEST_ENV_LOCK.lock().unwrap();
    let old_path = std::env::var_os("PATH");
    std::env::set_var("PATH", "");
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("agent.toml");
    let secret = "agent_status_secret_1234567890";
    std::fs::write(
        &config,
        format!(
            r#"
server_url = "https://example.test"
token = "{secret}"
client_id = "alice-laptop"
owner = "alice"
display_name = "Alice Laptop"
transport = "websocket"
projects_dir = "/etc/webcodex/projects.d"

[policy]
allowed_roots = ["/srv/projects"]
"#
        ),
    )
    .unwrap();
    let opts =
        parse_agent_status(&args(&["--config", config.to_str().unwrap(), "--json"])).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = rt.block_on(run_agent_status(opts)).unwrap();
    if let Some(path) = old_path {
        std::env::set_var("PATH", path);
    } else {
        std::env::remove_var("PATH");
    }
    assert!(!output.contains(secret));
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["service"]["active"], "unknown");
    assert_eq!(json["service"]["enabled"], "unknown");
    assert_eq!(json["config"]["client_id"], "alice-laptop");
    assert_eq!(json["config"]["owner"], "alice");
    assert_eq!(json["config"]["allowed_roots"]["count"], 1);
    assert!(json.get("token").is_none());
    assert!(json["config"].get("token").is_none());
}

#[tokio::test]
async fn agent_status_detects_current_client_online_and_agent_boundary() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        for i in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]).to_string();
            tx.send(request.clone()).unwrap();
            if i == 0 {
                let body = r#"{"success":true,"output":{"agents":{"clients":[{"client_id":"alice-laptop","connected":true,"status":"online"}]}}}"#;
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            } else {
                let body = r#"{"error":"forbidden"}"#;
                write!(
                    stream,
                    "HTTP/1.1 403 Forbidden\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
            }
        }
    });
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("agent.toml");
    std::fs::write(
        &config,
        r#"
server_url = "http://127.0.0.1:1"
token = "agent_config_secret_abcdef"
client_id = "alice-laptop"
owner = "alice"
transport = "websocket"
"#,
    )
    .unwrap();
    let user_token_file = tmp.path().join("webcodex-user-token");
    let agent_token_file = tmp.path().join("webcodex-agent-token");
    std::fs::write(&user_token_file, "pat_online_secret_1234567890\n").unwrap();
    std::fs::write(&agent_token_file, "agent_boundary_secret_1234567890\n").unwrap();
    let opts = parse_agent_status(&args(&[
        "--config",
        config.to_str().unwrap(),
        "--server-url",
        &format!("http://{}", addr),
        "--user-token-file",
        user_token_file.to_str().unwrap(),
        "--agent-token-file",
        agent_token_file.to_str().unwrap(),
    ]))
    .unwrap();
    let output = run_agent_status(opts).await.unwrap();
    handle.join().unwrap();
    let first_request = rx.recv().unwrap();
    let second_request = rx.recv().unwrap();
    assert!(first_request
        .to_ascii_lowercase()
        .contains("authorization: bearer pat_online_secret_1234567890"));
    assert!(second_request
        .to_ascii_lowercase()
        .contains("authorization: bearer agent_boundary_secret_1234567890"));
    for secret in [
        "agent_config_secret_abcdef",
        "pat_online_secret_1234567890",
        "agent_boundary_secret_1234567890",
    ] {
        assert!(!output.contains(secret));
    }
    assert!(output.contains("client online:        yes"));
    assert!(output.contains("agent token boundary: PASS"));
}

#[test]
fn token_prefix_never_exposes_full_token() {
    let p = token_prefix("wc_abcdef0123456789");
    assert!(p.ends_with('…'));
    assert!(!p.contains("0123456789"));
    assert_eq!(p, "wc_abcde…");
}

/// Fake server: respond to a sequence of (path -> response body) entries.
/// Captures the inbound Authorization header so tests can assert the
/// bootstrap token is present but never echoed in our output.
#[tokio::test]
async fn setup_single_user_runs_expected_calls_and_writes_0600_files() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let bootstrap = "fake-bootstrap-token-xyz".to_string();
    let bootstrap_for_thread = bootstrap.clone();
    let handle = thread::spawn(move || {
        let mut remaining = 3u32;
        while remaining > 0 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            let auth_ok = request
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {}", bootstrap_for_thread));
            assert!(auth_ok, "bootstrap token must be sent as bearer");
            let path = request.lines().next().unwrap_or("");
            let body = if path.contains("/api/users/create") {
                r#"{"success":true,"user":{"username":"yyjeqhc"}}"#
            } else if path.contains("/api/tokens/create") {
                r#"{"success":true,"token":"wc_user_fake_plaintext_12345","token_id":"ut-1"}"#
            } else if path.contains("/api/agent-tokens/create") {
                r#"{"success":true,"token":"agent_fake_plaintext_67890","token_id":"at-1"}"#
            } else {
                r#"{"error":"unexpected path"}"#
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            remaining -= 1;
        }
    });

    let tmp = tempfile::tempdir().unwrap();
    let opts = SetupSingleUserOptions {
        server_url: format!("http://{}", addr),
        token: Some(bootstrap.clone()),
        token_file: None,
        username: "yyjeqhc".to_string(),
        client_id: "oe".to_string(),
        display_name: None,
        role: "admin".to_string(),
        gpt_token_name: "chatgpt-action".to_string(),
        agent_token_name: "oe agent".to_string(),
        output_dir: tmp.path().to_path_buf(),
        force_create_tokens: false,
        json: false,
    };
    let summary = run_setup_single_user(opts).await.unwrap();
    handle.join().unwrap();

    // Summary must NOT contain full tokens or the bootstrap token.
    assert!(!summary.contains("wc_user_fake_plaintext_12345"));
    assert!(!summary.contains("agent_fake_plaintext_67890"));
    assert!(!summary.contains(&bootstrap));
    // Prefixes are present.
    assert!(summary.contains("wc_user_"));
    assert!(summary.contains("wc_agent"));
    assert!(summary.contains("yyjeqhc"));

    // Files written with 0600 and contain the full one-time tokens.
    let user_token = std::fs::read_to_string(tmp.path().join("webcodex-user-token")).unwrap();
    assert_eq!(user_token.trim(), "wc_user_fake_plaintext_12345");
    let agent_token = std::fs::read_to_string(tmp.path().join("webcodex-agent-token")).unwrap();
    assert_eq!(agent_token.trim(), "agent_fake_plaintext_67890");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let m = std::fs::metadata(tmp.path().join("webcodex-user-token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(m, 0o600);
        let m = std::fs::metadata(tmp.path().join("webcodex-agent-token"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(m, 0o600);
    }
}

#[tokio::test]
async fn setup_single_user_handles_user_already_exists() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let bootstrap = "fake-bootstrap-ae".to_string();
    let bootstrap_for_thread = bootstrap.clone();
    let handle = thread::spawn(move || {
        let mut remaining = 3u32;
        while remaining > 0 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 16384];
            let n = stream.read(&mut buf).unwrap();
            let request = String::from_utf8_lossy(&buf[..n]);
            let _ = request
                .to_ascii_lowercase()
                .contains(&format!("authorization: bearer {}", bootstrap_for_thread));
            let path = request.lines().next().unwrap_or("");
            let (status, body) = if path.contains("/api/users/create") {
                ("409 Conflict", r#"{"error":"user already exists"}"#)
            } else if path.contains("/api/tokens/create") {
                (
                    "200 OK",
                    r#"{"success":true,"token":"wc_user_ae_fake_token","token_id":"ut-1"}"#,
                )
            } else {
                (
                    "200 OK",
                    r#"{"success":true,"token":"agent_ae_fake_token","token_id":"at-1"}"#,
                )
            };
            write!(
                stream,
                "HTTP/1.1 {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                status,
                body.len(),
                body
            )
            .unwrap();
            remaining -= 1;
        }
    });
    let tmp = tempfile::tempdir().unwrap();
    let opts = SetupSingleUserOptions {
        server_url: format!("http://{}", addr),
        token: Some(bootstrap.clone()),
        token_file: None,
        username: "yyjeqhc".to_string(),
        client_id: "oe".to_string(),
        display_name: None,
        role: "admin".to_string(),
        gpt_token_name: "chatgpt-action".to_string(),
        agent_token_name: "oe agent".to_string(),
        output_dir: tmp.path().to_path_buf(),
        force_create_tokens: false,
        json: true,
    };
    let summary = run_setup_single_user(opts).await.unwrap();
    handle.join().unwrap();
    assert!(summary.contains("\"user_already_existed\": true"));
    assert!(!summary.contains(&bootstrap));
    assert!(!summary.contains("wc_user_ae_fake_token"));
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

// ------------------------------------------------------------------
// connect + server up quick-start CLI tests
// ------------------------------------------------------------------
