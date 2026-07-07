use super::support::*;

#[test]
fn pairing_create_parse_defaults() {
    let opts = parse_pairing_create(&args(&[
        "--server-url",
        "https://example.test",
        "--token-file",
        "/tmp/webcodex-token",
        "--username",
        "alice",
        "--client-id",
        "alice-laptop",
    ]))
    .unwrap();
    assert_eq!(opts.ttl_secs, 600);
    assert_eq!(opts.username, "alice");
    assert_eq!(opts.client_id, "alice-laptop");
    assert_eq!(opts.token_file, Some(PathBuf::from("/tmp/webcodex-token")));
}

#[test]
fn pairing_create_missing_env_file_error_includes_server_admin_guidance() {
    let opts = PairingCreateOptions {
        server_url: "https://example.test".to_string(),
        env_file: Some(PathBuf::from("/tmp/webcodex-missing-server-env-file")),
        username: "alice".to_string(),
        client_id: "alice-laptop".to_string(),
        ttl_secs: 600,
        ..PairingCreateOptions::default()
    };
    let err = resolve_pairing_create_token(&opts).unwrap_err();
    assert!(err.contains("failed to read server env file"));
    assert!(err.contains("pairing create is a server/admin-side command"));
    assert!(err.contains("Run it on the server or pass a server/admin token file"));
}

#[test]
fn client_enroll_refuses_overwrite_before_network() {
    let tmp = tempfile::tempdir().unwrap();
    let existing = tmp.path().join("webcodex-user-token");
    std::fs::write(&existing, "old\n").unwrap();
    let opts = ClientEnrollOptions {
        server_url: "http://127.0.0.1:9".to_string(),
        pairing_code: "wc_pair_fake".to_string(),
        client_id: "alice-laptop".to_string(),
        display_name: None,
        transport: TRANSPORT_WEBSOCKET.to_string(),
        output_dir: tmp.path().to_path_buf(),
        agent_config: tmp.path().join("agent.toml"),
        projects_dir: tmp.path().join("projects.d"),
        allowed_roots: vec![tmp.path().to_path_buf()],
        allow_cwd_anywhere: false,
        overwrite: false,
        json: false,
    };
    let err = ensure_enroll_outputs_available(&opts).unwrap_err();
    assert!(err.contains("already exists"));
}

#[tokio::test]
async fn pairing_create_prints_pairing_code_once_without_auth_token() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]);
        assert!(request.starts_with("POST /api/pairing/create "));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer fake-bootstrap"));
        let body = r#"{"success":true,"pairing_code":"wc_pair_copy_once","expires_at":123,"username":"alice","client_id":"alice-laptop"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let opts = PairingCreateOptions {
        server_url: format!("http://{}", addr),
        token: Some("fake-bootstrap".to_string()),
        username: "alice".to_string(),
        client_id: "alice-laptop".to_string(),
        ttl_secs: 600,
        ..PairingCreateOptions::default()
    };
    let output = run_pairing_create(opts).await.unwrap();
    handle.join().unwrap();
    assert_eq!(output.matches("wc_pair_copy_once").count(), 1);
    assert!(!output.contains("fake-bootstrap"));
}

#[tokio::test]
async fn client_enroll_posts_without_authorization_and_writes_files() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 16384];
        let n = stream.read(&mut buf).unwrap();
        let request = String::from_utf8_lossy(&buf[..n]).to_string();
        tx.send(request).unwrap();
        let body = r#"{"success":true,"username":"alice","client_id":"alice-laptop","user_token":"pat_fake_plaintext_123456","agent_token":"agent_fake_plaintext_abcdef","user_token_prefix":"wc_pat_fake_pre","agent_token_prefix":"wc_agent_fake_p"}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let tmp = tempfile::tempdir().unwrap();
    let opts = ClientEnrollOptions {
        server_url: format!("http://{}", addr),
        pairing_code: "wc_pair_fake".to_string(),
        client_id: "alice-laptop".to_string(),
        display_name: Some("Alice Laptop".to_string()),
        transport: TRANSPORT_WEBSOCKET.to_string(),
        output_dir: tmp.path().to_path_buf(),
        agent_config: tmp.path().join("agent.toml"),
        projects_dir: tmp.path().join("projects.d"),
        allowed_roots: vec![tmp.path().to_path_buf()],
        allow_cwd_anywhere: false,
        overwrite: false,
        json: true,
    };
    let output = run_client_enroll(opts).await.unwrap();
    handle.join().unwrap();
    let request = rx.recv().unwrap();
    assert!(request.starts_with("POST /api/pairing/enroll "));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    assert!(request.contains(r#""pairing_code":"wc_pair_fake""#));
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("webcodex-user-token"))
            .unwrap()
            .trim(),
        "pat_fake_plaintext_123456"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("webcodex-agent-token"))
            .unwrap()
            .trim(),
        "agent_fake_plaintext_abcdef"
    );
    let agent_config = std::fs::read_to_string(tmp.path().join("agent.toml")).unwrap();
    assert!(agent_config.contains("agent_fake_plaintext_abcdef"));
    assert!(output.contains(
        tmp.path()
            .join("webcodex-user-token")
            .to_string_lossy()
            .as_ref()
    ));
    assert!(output.contains(
        tmp.path()
            .join("webcodex-agent-token")
            .to_string_lossy()
            .as_ref()
    ));
    assert!(output.contains(tmp.path().join("agent.toml").to_string_lossy().as_ref()));
    assert!(output.contains(tmp.path().join("projects.d").to_string_lossy().as_ref()));
    assert!(!output.contains("pat_fake_plaintext_123456"));
    assert!(!output.contains("agent_fake_plaintext_abcdef"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [
            tmp.path().join("webcodex-user-token"),
            tmp.path().join("webcodex-agent-token"),
            tmp.path().join("agent.toml"),
        ] {
            let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }
}

#[test]
fn connect_help_prints_usage() {
    let out = cli_exit(["connect", "--help"]).unwrap();
    assert!(out.contains("Usage: webcodex-cli connect"));
    assert!(out.contains("--key"));
    assert!(out.contains("--open"));
    assert!(out.contains("mutually exclusive"));
}

#[test]
fn connect_key_and_open_are_mutually_exclusive() {
    let err = cli_exit(["connect", "http://127.0.0.1:8080", "--key", "abc", "--open"]).unwrap_err();
    assert!(err.contains("mutually exclusive"), "err was: {err}");
}

#[test]
fn connect_requires_key_or_open() {
    let err = cli_exit(["connect", "http://127.0.0.1:8080"]).unwrap_err();
    assert!(
        err.contains("--key") || err.contains("--open"),
        "err was: {err}"
    );
}

#[test]
fn connect_key_parses_with_default_root() {
    match cli_action(["connect", "http://127.0.0.1:8080", "--key", "abc123"]) {
        CliAction::Connect(opts) => {
            assert_eq!(opts.server_url, "http://127.0.0.1:8080");
            assert_eq!(opts.mode, ConnectMode::SharedKey("abc123".to_string()));
            // Default root is the current working directory.
            assert!(opts.root.is_absolute() || !opts.root.as_os_str().is_empty());
            assert!(!opts.overwrite);
            assert!(!opts.json);
        }
        other => panic!("expected Connect, got {other:?}"),
    }
}

#[test]
fn connect_open_parses() {
    match cli_action([
        "connect",
        "http://127.0.0.1:8080",
        "--open",
        "--root",
        "/tmp/proj",
    ]) {
        CliAction::Connect(opts) => {
            assert_eq!(opts.server_url, "http://127.0.0.1:8080");
            assert_eq!(opts.mode, ConnectMode::Open);
            assert_eq!(opts.root, PathBuf::from("/tmp/proj"));
        }
        other => panic!("expected Connect, got {other:?}"),
    }
}

#[test]
fn connect_explicit_client_id_and_output_dir() {
    match cli_action([
        "connect",
        "https://example.com",
        "--key",
        "k",
        "--root",
        "/tmp/p",
        "--client-id",
        "my-laptop",
        "--output-dir",
        "/tmp/out",
        "--overwrite",
        "--json",
    ]) {
        CliAction::Connect(opts) => {
            assert_eq!(opts.client_id.as_deref(), Some("my-laptop"));
            assert_eq!(
                opts.output_dir.as_deref(),
                Some(std::path::Path::new("/tmp/out"))
            );
            assert!(opts.overwrite);
            assert!(opts.json);
        }
        other => panic!("expected Connect, got {other:?}"),
    }
}

#[test]
fn connect_requires_server_url() {
    let err = cli_exit(["connect", "--key", "abc"]).unwrap_err();
    assert!(err.contains("server URL"), "err was: {err}");
}

#[test]
fn server_up_help_prints_usage() {
    let out = cli_exit(["server", "up", "--help"]).unwrap();
    assert!(out.contains("Usage: webcodex-cli server up"));
    assert!(out.contains("--open"));
    assert!(!out.contains("--foreground"));
}

#[test]
fn server_up_parses_open_mode() {
    match cli_action([
        "server",
        "up",
        "--open",
        "--public-url",
        "https://x.example",
    ]) {
        CliAction::ServerUp(opts) => {
            assert!(opts.open);
            assert_eq!(opts.public_url.as_deref(), Some("https://x.example"));
        }
        other => panic!("expected ServerUp, got {other:?}"),
    }
}

#[test]
fn server_up_defaults_to_closed_mode() {
    match cli_action(["server", "up"]) {
        CliAction::ServerUp(opts) => {
            assert!(!opts.open);
            assert!(opts.public_url.is_none());
        }
        other => panic!("expected ServerUp, got {other:?}"),
    }
}

#[test]
fn server_up_foreground_reports_not_implemented() {
    let err = cli_exit(["server", "up", "--foreground"]).unwrap_err();
    assert!(
        err.contains("--foreground is not implemented yet"),
        "err was: {err}"
    );
    assert!(
        !err.contains("Starting server in foreground"),
        "foreground must not imply a server was started"
    );
}

#[test]
fn server_up_output_hides_full_admin_key() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let out = match cli_action([
        "server",
        "up",
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().join("data").to_str().unwrap(),
    ]) {
        CliAction::ServerUp(opts) => run_server_up(opts).unwrap(),
        other => panic!("expected ServerUp, got {other:?}"),
    };
    let env_content = fs::read_to_string(&env_file).unwrap();
    let token = read_env_file_value(&env_file, "WEBCODEX_TOKEN")
        .unwrap()
        .unwrap();
    assert!(env_content.contains(&token));
    assert!(
        !out.contains(&token),
        "stdout must not contain full admin key"
    );
    assert!(out.contains("admin key:"));
    assert!(out.contains("token prefix:"));
}

#[test]
fn server_up_json_hides_full_admin_key() {
    let tmp = tempfile::tempdir().unwrap();
    let env_file = tmp.path().join("webcodex.env");
    let out = match cli_action([
        "server",
        "up",
        "--json",
        "--env-file",
        env_file.to_str().unwrap(),
        "--data-dir",
        tmp.path().join("data").to_str().unwrap(),
    ]) {
        CliAction::ServerUp(opts) => run_server_up(opts).unwrap(),
        other => panic!("expected ServerUp, got {other:?}"),
    };
    let token = read_env_file_value(&env_file, "WEBCODEX_TOKEN")
        .unwrap()
        .unwrap();
    assert!(
        !out.contains(&token),
        "json must not contain full admin key"
    );
    let value: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["token_generated"], true);
    assert!(value["token_prefix"]
        .as_str()
        .unwrap()
        .starts_with("wc_boot"));
    assert!(value.get("token").is_none());
}

#[test]
fn connect_output_uses_agent_registration_quick_start_model() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("repo");
    fs::create_dir_all(&root).unwrap();
    let output_dir = tmp.path().join("client");
    let out = match cli_action([
        "connect",
        "http://127.0.0.1:8080",
        "--key",
        "abc123",
        "--root",
        root.to_str().unwrap(),
        "--output-dir",
        output_dir.to_str().unwrap(),
    ]) {
        CliAction::Connect(opts) => run_connect(opts).unwrap(),
        other => panic!("expected Connect, got {other:?}"),
    };
    assert!(out.contains("The project should appear after the agent registers"));
    assert!(
        output_dir.join("projects.d").join("repo.toml").exists(),
        "connect must write the agent-side projects.d entry"
    );
    assert!(
        !output_dir.join("projects.toml").exists(),
        "connect must not write legacy server-side projects.toml"
    );
    assert!(!out.contains("PROJECTS_CONFIG"));
    assert!(!out.contains("merge projects.toml"));
    assert!(!out.contains("use the runtime API"));
    assert!(!out.contains("Register the project on the server"));
}
