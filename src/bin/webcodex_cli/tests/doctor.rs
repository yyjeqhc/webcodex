use super::support::*;

#[test]
fn doctor_revision_check_passes_when_commits_match() {
    let local = build_metadata(Some("81f322d5b580"));
    let remote = build_metadata(Some("81f322d"));
    let check = doctor_revision_check(&local, Some(&remote));
    assert_eq!(check.status, "PASS");
    assert_eq!(check.name, "cli/server revision");
    assert!(check
        .detail
        .contains("local CLI and server runtime commit match"));
}

#[test]
fn doctor_revision_check_warns_when_commits_differ() {
    let local = build_metadata(Some("81f322d5b580"));
    let remote = build_metadata(Some("fd156ba92fc7"));
    let check = doctor_revision_check(&local, Some(&remote));
    assert_eq!(check.status, "WARN");
    assert!(check.detail.contains("local CLI commit 81f322d5b580"));
    assert!(check.detail.contains("server runtime commit fd156ba92fc7"));
    assert!(check
        .detail
        .contains("deploy/update one side before debugging old behavior"));
}

#[test]
fn doctor_revision_check_warns_when_server_build_missing() {
    let local = build_metadata(Some("81f322d5b580"));
    let remote = build_metadata(None);
    let check = doctor_revision_check(&local, Some(&remote));
    assert_eq!(check.status, "WARN");
    assert!(check
        .detail
        .contains("server runtime did not report build.git_commit"));
    assert!(check
        .detail
        .contains("server may be older than build metadata support"));
}

#[test]
fn doctor_revision_check_skips_without_runtime_status_credentials() {
    let local = build_metadata(Some("81f322d5b580"));
    let check = doctor_revision_check(&local, None);
    assert_eq!(check.status, "WARN");
    assert_eq!(check.name, "cli/server revision");
    assert!(check.detail.contains("not checked; pass --server-url"));
    assert!(check.detail.contains("--user-token-file or --token-file"));
}

#[test]
fn doctor_parse_quic_flags() {
    let opts = parse_doctor(&args(&[
        "--quic",
        "--server-only",
        "--quic-server-addr",
        "v4.example.test:8443",
        "--quic-server-name",
        "v4.example.test",
        "--quic-alpn",
        "webcodex-agent/1",
        "--quic-timeout-secs",
        "7",
        "--quic-client-id",
        "alice-laptop",
    ]))
    .unwrap();
    assert!(opts.quic);
    assert!(opts.quic_server_only);
    assert!(!opts.quic_agent_e2e);
    assert_eq!(
        opts.quic_server_addr.as_deref(),
        Some("v4.example.test:8443")
    );
    assert_eq!(opts.quic_server_name.as_deref(), Some("v4.example.test"));
    assert_eq!(opts.quic_alpn, "webcodex-agent/1");
    assert_eq!(opts.quic_timeout_secs, 7);
    assert_eq!(opts.quic_client_id.as_deref(), Some("alice-laptop"));
}

#[test]
fn doctor_parse_accepts_quic_and_auto_transport_flags() {
    let opts = parse_client_enroll(&args(&[
        "--server-url",
        "https://v4.example.test",
        "--pairing-code",
        "abc123",
        "--client-id",
        "alice-laptop",
        "--transport",
        agent_init::TRANSPORT_AUTO,
    ]))
    .unwrap();
    assert_eq!(opts.transport, agent_init::TRANSPORT_AUTO);
}

#[test]
fn doctor_runtime_quic_checks_fail_when_disabled_or_listener_failed() {
    let disabled = json!({
        "quic": {
            "enabled": false,
            "listen": "0.0.0.0:8443",
            "alpn": "webcodex-agent/1",
            "listener_started": false,
            "last_error": null
        }
    });
    let (checks, should_continue) = doctor_runtime_quic_checks(&disabled);
    assert!(!should_continue);
    assert!(checks
        .iter()
        .any(|c| c.status == "FAIL" && c.detail.contains("server reports QUIC disabled")));

    let listener_failed = json!({
        "quic": {
            "enabled": true,
            "listen": "0.0.0.0:8443",
            "alpn": "webcodex-agent/1",
            "listener_started": false,
            "last_error": "WEBCODEX_QUIC_KEY path does not exist: /etc/secret/privkey.pem"
        }
    });
    let (checks, should_continue) = doctor_runtime_quic_checks(&listener_failed);
    assert!(!should_continue);
    let detail = checks
        .iter()
        .find(|c| c.name == "quic listener started")
        .unwrap()
        .detail
        .clone();
    assert!(detail.contains("listener not started"));
    assert!(detail.contains("WEBCODEX_QUIC_KEY path does not exist"));
    assert!(!detail.contains("/etc/secret"));
    assert!(!detail.contains("privkey.pem"));
}

#[test]
fn doctor_runtime_quic_checks_warn_for_older_server() {
    let (checks, should_continue) = doctor_runtime_quic_checks(&json!({}));
    assert!(should_continue);
    assert_eq!(checks[0].status, "WARN");
    assert!(checks[0]
        .detail
        .contains("not exposed by this server version"));
}

#[test]
fn doctor_runtime_quic_checks_pass_when_listener_started() {
    let value = json!({
        "quic": {
            "enabled": true,
            "listen": "0.0.0.0:8443",
            "alpn": "webcodex-agent/1",
            "listener_started": true,
            "last_error": null
        }
    });
    let (checks, should_continue) = doctor_runtime_quic_checks(&value);
    assert!(should_continue);
    assert!(checks.iter().any(|c| c.name == "quic runtime config"
        && c.detail.contains("enabled=true listen=0.0.0.0:8443")
        && c.detail.contains("listener_started=true")));
}

#[test]
fn doctor_parse_quic_modes_are_mutually_exclusive() {
    let err = parse_doctor(&args(&["--quic", "--server-only", "--agent-e2e"])).unwrap_err();
    assert!(err.contains("mutually exclusive"));
}

#[test]
fn doctor_quic_options_fall_back_to_agent_config() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("agent.toml");
    std::fs::write(
        &config,
        r#"
server_url = "https://v4.example.test"
token = "redacted"
client_id = "alice-laptop"
transport = "quic"

[quic]
server_addr = "v4.example.test:8443"
server_name = "v4.example.test"
alpn = "webcodex-agent/1"
connect_timeout_secs = 12
"#,
    )
    .unwrap();
    let opts = parse_doctor(&args(&[
        "--quic",
        "--agent-config",
        config.to_str().unwrap(),
    ]))
    .unwrap();
    let resolved = resolve_doctor_quic_options(&opts).unwrap();
    assert_eq!(resolved.server_addr, "v4.example.test:8443");
    assert_eq!(resolved.server_name, "v4.example.test");
    assert_eq!(resolved.alpn, "webcodex-agent/1");
    assert_eq!(resolved.client_id.as_deref(), Some("alice-laptop"));
}

#[tokio::test]
async fn doctor_does_not_print_token_or_html_body() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buf = [0u8; 8192];
        let _ = stream.read(&mut buf).unwrap();
        let body = "<html>secret-token-in-body</html>";
        write!(
            stream,
            "HTTP/1.1 502 Bad Gateway\r\ncontent-type: text/html\r\ncontent-length: {}\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let tmp = tempfile::tempdir().unwrap();
    let token_file = tmp.path().join("user-token");
    std::fs::write(&token_file, "secret-doctor-token\n").unwrap();
    let opts = DoctorOptions {
        server_url: Some(format!("http://{}", addr)),
        user_token_file: Some(token_file),
        ..DoctorOptions::default()
    };
    let (output, has_fail) = run_doctor(opts).await.unwrap();
    handle.join().unwrap();
    assert!(has_fail);
    assert!(!output.contains("secret-doctor-token"));
    assert!(!output.contains("secret-token-in-body"));
    assert!(output.contains("non-JSON response"));
}

#[test]
fn doctor_local_agent_config_detects_configured_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let project_dir = tmp.path().join("rust-proj");
    std::fs::create_dir_all(&project_dir).unwrap();
    write_doctor_project(&projects_dir, "rust-proj", &project_dir, Some("rust"));
    let cfg_path = write_doctor_agent_config(tmp.path(), &projects_dir, Some("rust"));
    let checks = run_local_agent_doctor(&cfg_path);
    let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"agent config"), "{names:?}");
    assert!(names.contains(&"shell profiles"), "{names:?}");
    assert!(names.contains(&"projects_dir"), "{names:?}");
    // The configured profile resolves successfully.
    let profile_check = checks
        .iter()
        .find(|c| c.name == "project 'rust-proj' shell_profile")
        .expect("shell_profile check present");
    assert_eq!(profile_check.status, "PASS", "{:?}", profile_check);
    assert!(profile_check.detail.contains("resolved='rust'"));
    assert!(profile_check.detail.contains("has_init_script=true"));
    assert!(profile_check.detail.contains("env_keys_count=2"));
    // Sanitization: never print the init_script body or env value.
    assert!(!profile_check
        .detail
        .contains("DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY"));
    let all_rendered = format!(
        "{}",
        checks
            .iter()
            .map(|c| c.detail.as_str())
            .collect::<Vec<_>>()
            .join("|")
    );
    assert!(
        !all_rendered.contains("DO_NOT_LEAK_THIS_ENV_VALUE"),
        "{all_rendered}"
    );
}

#[test]
fn doctor_local_agent_config_detects_missing_shell_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let projects_dir = tmp.path().join("projects.d");
    let project_dir = tmp.path().join("bad-proj");
    std::fs::create_dir_all(&project_dir).unwrap();
    // Project asks for a profile that is not configured, and there is no
    // default_profile to fall back to.
    write_doctor_project(&projects_dir, "bad-proj", &project_dir, Some("nope"));
    let cfg_path = write_doctor_agent_config(tmp.path(), &projects_dir, None);
    let checks = run_local_agent_doctor(&cfg_path);
    let profile_check = checks
        .iter()
        .find(|c| c.name == "project 'bad-proj' shell_profile")
        .expect("shell_profile check present");
    assert_eq!(profile_check.status, "FAIL", "{:?}", profile_check);
    assert!(profile_check.detail.contains("not in shell.profiles"));
}

#[test]
fn doctor_parse_accepts_agent_config_and_project_flags() {
    let opts = parse_doctor(&args(&[
        "--agent-config",
        "/tmp/agent.toml",
        "--project",
        "agent:oe:webcodex",
        "--strict",
    ]))
    .unwrap();
    assert_eq!(
        opts.agent_config.as_deref(),
        Some(Path::new("/tmp/agent.toml"))
    );
    assert_eq!(opts.project.as_deref(), Some("agent:oe:webcodex"));
    assert!(opts.strict);
}

#[test]
fn shell_profiles_doc_exists_and_index_links_it() {
    // The shell-profiles user doc must exist and be linked from INDEX.md.
    let doc = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/SHELL_PROFILES.md");
    assert!(doc.is_file(), "docs/SHELL_PROFILES.md must exist");
    let index =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/INDEX.md"))
            .unwrap();
    assert!(
        index.contains("SHELL_PROFILES.md"),
        "INDEX.md must link SHELL_PROFILES.md"
    );
}
