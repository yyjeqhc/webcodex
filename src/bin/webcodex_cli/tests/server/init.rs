use super::super::support::*;

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
