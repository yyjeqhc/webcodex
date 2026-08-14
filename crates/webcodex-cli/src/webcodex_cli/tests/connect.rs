use super::support::*;

fn parsed(args: &[&str]) -> ConnectOptions {
    match cli_action(args.iter().copied()) {
        CliAction::Connect(options) => options,
        other => panic!("expected connect action, got {other:?}"),
    }
}

fn parsed_disconnect(args: &[&str]) -> DisconnectOptions {
    match cli_action(args.iter().copied()) {
        CliAction::Disconnect(options) => options,
        other => panic!("expected disconnect action, got {other:?}"),
    }
}

#[test]
fn connect_parses_explicit_key_project_and_overrides() {
    let options = parsed(&[
        "connect",
        "https://example.test",
        "--key",
        "shared-key",
        "--project",
        "/tmp/project",
        "--profile",
        "workstation",
        "--client-id",
        "laptop-a1",
        "--project-id",
        "demo",
    ]);
    assert_eq!(options.server_url, "https://example.test");
    assert_eq!(options.key.as_deref(), Some("shared-key"));
    assert_eq!(options.project, PathBuf::from("/tmp/project"));
    assert_eq!(options.profile.as_deref(), Some("workstation"));
    assert_eq!(options.client_id.as_deref(), Some("laptop-a1"));
    assert_eq!(options.project_id.as_deref(), Some("demo"));
}

#[test]
fn connect_defaults_project_and_allows_automatic_key() {
    let options = parsed(&["connect", "https://example.test"]);
    assert_eq!(options.project, PathBuf::from("."));
    assert!(options.key.is_none());
    assert!(options.key_file.is_none());
}

#[test]
fn connect_rejects_conflicting_key_sources_and_unsafe_profile() {
    for (args, needle) in [
        (
            vec![
                "connect",
                "https://example.test",
                "--key",
                "one",
                "--key-file",
                "/tmp/key",
            ],
            "mutually exclusive",
        ),
        (
            vec!["connect", "https://example.test", "--profile", "../escape"],
            "--profile must be",
        ),
    ] {
        match cli_action(args) {
            CliAction::Exit {
                code: 2, stderr, ..
            } => assert!(stderr.contains(needle), "{stderr}"),
            other => panic!("expected parse error, got {other:?}"),
        }
    }
}

#[test]
fn connect_help_is_a_top_level_quick_start() {
    let help = cli_exit(["connect", "--help"]).unwrap();
    assert!(help.contains("Usage: webcodex connect <SERVER_URL>"));
    assert!(help.contains("--key-file"));
    assert!(help.contains("--project PATH"));
    let top = cli_exit(["--help"]).unwrap();
    assert!(top.contains("connect"));
    assert!(top.contains("hosted Server"));
    assert!(!top.contains("__hosted-log-writer"));
}

#[test]
fn disconnect_parses_defaults_profile_and_help() {
    let defaults = parsed_disconnect(&["disconnect"]);
    assert_eq!(defaults.project, PathBuf::from("."));
    assert!(defaults.profile.is_none());

    let explicit = parsed_disconnect(&[
        "disconnect",
        "--project",
        "/tmp/project",
        "--profile",
        "workstation",
    ]);
    assert_eq!(explicit.project, PathBuf::from("/tmp/project"));
    assert_eq!(explicit.profile.as_deref(), Some("workstation"));

    let help = cli_exit(["disconnect", "--help"]).unwrap();
    assert!(help.contains("Usage: webcodex disconnect [OPTIONS]"));
    assert!(help.contains("never removed or modified"));
    let top = cli_exit(["--help"]).unwrap();
    assert!(top.contains("disconnect"));
}

#[test]
fn disconnect_rejects_unsafe_profile_and_positional_guessing() {
    for (args, needle) in [
        (
            vec!["disconnect", "--profile", "../escape"],
            "--profile must be",
        ),
        (vec!["disconnect", "repo"], "unexpected disconnect argument"),
    ] {
        match cli_action(args) {
            CliAction::Exit {
                code: 2, stderr, ..
            } => assert!(stderr.contains(needle), "{stderr}"),
            other => panic!("expected parse error, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn connect_rejects_invalid_url_and_missing_project_before_network_or_writes() {
    let tmp = tempfile::tempdir().unwrap();
    let base = ConnectOptions {
        server_url: "ssh://example.test".to_string(),
        server_http: ServerHttpOptions::default(),
        key: Some("shared-key".to_string()),
        key_file: None,
        project: tmp.path().join("missing"),
        profile: None,
        client_id: None,
        project_id: None,
        config_base: Some(tmp.path().join("config")),
        state_base: Some(tmp.path().join("state")),
        runner_bin: None,
        wait_timeout_ms: 100,
    };
    let error = run_connect(base.clone()).await.unwrap_err();
    assert!(error.contains("http or https"), "{error}");
    let error = run_connect(ConnectOptions {
        server_url: "https://example.test".to_string(),
        ..base
    })
    .await
    .unwrap_err();
    assert!(error.contains("does not exist"), "{error}");
    assert!(!tmp.path().join("config").exists());
}
