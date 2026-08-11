use super::super::support::*;

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn install_service_generates_expected_unit_without_tokens() {
    let opts = parse_server_install_service(&args(&[
        "--env-file",
        "/etc/webcodex/webcodex.env",
        "--bin",
        "/usr/local/bin/webcodex-server",
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
    assert!(unit.contains("ExecStart=\"/usr/local/bin/webcodex-server\"\n"));
    assert!(unit.contains("WorkingDirectory=/var/lib/webcodex\n"));
    assert!(unit.contains("User=webcodex\n"));
    assert!(unit.contains("Group=webcodex\n"));
    assert!(unit.contains("WantedBy=multi-user.target\n"));
    assert!(!unit.contains("WEBCODEX_TOKEN"));
    assert!(!unit.contains("wc_boot_"));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn user_agent_unit_uses_user_target_without_identity_directives_or_root_workdir() {
    let opts = parse_agent_install_service_with_identity(
        &args(&[
            "--scope",
            "user",
            "--config",
            "/home/alice/.config/webcodex/agent.toml",
            "--service-file",
            "/home/alice/.config/systemd/user/webcodex-runner.service",
            "--bin",
            "/home/alice/.local/bin/webcodex-runner",
            "--working-directory",
            "/home/alice",
            "--dry-run",
        ]),
        false,
    )
    .unwrap();
    let unit = run_agent_install_service(opts).unwrap();
    assert!(unit.contains("WantedBy=default.target\n"));
    assert!(!unit.contains("network-online.target"));
    assert!(unit.contains("WorkingDirectory=/home/alice\n"));
    assert!(!unit.contains("WorkingDirectory=/root\n"));
    assert!(!unit.contains("\nUser="));
    assert!(!unit.contains("\nGroup="));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn explicitly_allowed_root_runner_is_visibly_marked() {
    let opts = parse_agent_install_service_with_identity(
        &args(&[
            "--scope",
            "system",
            "--bin",
            "/opt/webcodex/bin/webcodex-runner",
            "--working-directory",
            "/root",
            "--allow-root-runner",
            "--dry-run",
        ]),
        true,
    )
    .unwrap();
    let unit = run_agent_install_service(opts).unwrap();
    assert!(unit.contains("WARNING: --allow-root-runner was explicitly accepted"));
    assert!(unit.contains("WorkingDirectory=/root\n"));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn install_service_refuses_overwrite_unless_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let service_file = tmp.path().join("webcodex.service");
    std::fs::write(&service_file, "old").unwrap();
    let opts = parse_server_install_service(&args(&[
        "--bin",
        "/usr/local/bin/webcodex-server",
        "--service-file",
        service_file.to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_server_install_service(opts).unwrap_err();
    assert!(err.contains("already exists"));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn install_service_dry_run_and_output_work_without_systemd() {
    let dry = parse_server_install_service(&args(&[
        "--bin",
        "/usr/local/bin/webcodex-server",
        "--dry-run",
    ]))
    .unwrap();
    assert!(run_server_install_service(dry)
        .unwrap()
        .contains("ExecStart=\"/usr/local/bin/webcodex-server\""));

    let out = parse_server_install_service(&args(&[
        "--bin",
        "/usr/local/bin/webcodex-server",
        "--output",
        "-",
        "--json",
    ]))
    .unwrap();
    let json: Value = serde_json::from_str(&run_server_install_service(out).unwrap()).unwrap();
    assert_eq!(json["dry_run"], true);
    assert!(json["unit"].as_str().unwrap().contains("[Service]"));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn agent_install_service_generates_expected_unit_without_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("agent.toml");
    std::fs::write(&config, "token = \"agent_secret_should_not_print\"\n").unwrap();
    let opts = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--config",
        config.to_str().unwrap(),
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
        "--working-directory",
        "/srv/webcodex",
        "--user",
        "webcodex",
        "--group",
        "webcodex",
        "--dry-run",
    ]))
    .unwrap();
    let unit = run_agent_install_service(opts).unwrap();
    assert!(unit.contains("[Unit]\nDescription=WebCodex Runner\n"));
    assert!(unit.contains("After=network-online.target\n"));
    assert!(unit.contains("Wants=network-online.target\n"));
    assert!(unit.contains(&format!(
        "ExecStart=\"/opt/webcodex/bin/webcodex-runner\" \"--config\" \"{}\"\n",
        config.display()
    )));
    assert!(unit.contains("ExecReload=/bin/kill -HUP $MAINPID\n"));
    assert!(unit.contains("Restart=always\n"));
    assert!(unit.contains("RestartSec=5s\n"));
    assert!(unit.contains("StandardOutput=journal\n"));
    assert!(unit.contains("StandardError=journal\n"));
    assert!(unit.contains("Environment=RUST_LOG=info\n"));
    assert!(unit.contains("WorkingDirectory=/srv/webcodex\n"));
    assert!(unit.contains("User=webcodex\n"));
    assert!(unit.contains("Group=webcodex\n"));
    assert!(!unit.contains("agent_secret_should_not_print"));
    assert!(!unit.contains("Authorization"));
    assert!(!unit.contains("token ="));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn agent_install_service_refuses_overwrite_unless_requested() {
    let tmp = tempfile::tempdir().unwrap();
    let service_file = tmp.path().join("webcodex-runner.service");
    std::fs::write(&service_file, "old").unwrap();
    let opts = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--config",
        "/etc/webcodex/agent.toml",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
        "--service-file",
        service_file.to_str().unwrap(),
    ]))
    .unwrap();
    let err = run_agent_install_service(opts).unwrap_err();
    assert!(err.contains("already exists"));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn agent_install_service_dry_run_and_output_work_without_systemd() {
    let dry = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--config",
        "/etc/webcodex/agent.toml",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
        "--dry-run",
    ]))
    .unwrap();
    assert!(run_agent_install_service(dry).unwrap().contains(
        "ExecStart=\"/opt/webcodex/bin/webcodex-runner\" \"--config\" \"/etc/webcodex/agent.toml\""
    ));

    let out = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--config",
        "/etc/webcodex/agent.toml",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
        "--output",
        "-",
        "--json",
    ]))
    .unwrap();
    let json: Value = serde_json::from_str(&run_agent_install_service(out).unwrap()).unwrap();
    assert_eq!(json["dry_run"], true);
    assert!(json["unit"].as_str().unwrap().contains(
        "ExecStart=\"/opt/webcodex/bin/webcodex-runner\" \"--config\" \"/etc/webcodex/agent.toml\""
    ));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn systemd_unit_rendering_quotes_paths_and_rejects_invalid_fields_in_dry_run() {
    let server = parse_server_install_service(&args(&[
        "--env-file",
        "/etc/webcodex/env files/web\"codex\\main.env",
        "--bin",
        "/opt/web codex/bin/webcodex-server%p",
        "--working-directory",
        "/var/lib/web codex\\work",
        "--user",
        "web_codex-1.service",
        "--group",
        "web.group-1",
        "--dry-run",
    ]))
    .unwrap();
    let unit = run_server_install_service(server).unwrap();
    assert!(unit.contains("EnvironmentFile=/etc/webcodex/env\\x20files/web\\x22codex\\x5cmain.env"));
    assert!(unit.contains("ExecStart=\"/opt/web codex/bin/webcodex-server%%p\""));
    assert!(unit.contains("WorkingDirectory=/var/lib/web\\x20codex\\x5cwork"));
    assert!(unit.contains("User=web_codex-1.service"));
    assert!(unit.contains("Group=web.group-1"));

    for (flag, value, field) in [
        ("--user", "bad user", "User"),
        ("--group", "bad/group", "Group"),
        (
            "--bin",
            "/opt/webcodex/bin/server\nInjected=yes",
            "ExecStart",
        ),
        ("--env-file", "/etc/webcodex/a\rb", "EnvironmentFile"),
        (
            "--working-directory",
            "/var/lib/web\0codex",
            "WorkingDirectory",
        ),
    ] {
        let opts = parse_server_install_service(&args(&[
            "--bin",
            "/usr/local/bin/webcodex-server",
            flag,
            value,
            "--dry-run",
        ]))
        .unwrap();
        let error = run_server_install_service(opts).unwrap_err();
        assert!(error.contains(field), "{error}");
    }
}

#[cfg(target_os = "linux")]
fn verify_systemd_unit(unit: &str, name: &str) {
    let available = std::process::Command::new("systemd-analyze")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !available {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join(name);
    std::fs::write(&path, unit).unwrap();
    let output = std::process::Command::new("systemd-analyze")
        .arg("verify")
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "systemd-analyze verify failed for {name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(target_os = "linux")]
fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn generated_server_and_agent_units_pass_systemd_analyze_verify() {
    let tmp = tempfile::tempdir().unwrap();
    let server_bin = tmp.path().join("webcodex-server");
    let runner_bin = tmp.path().join("webcodex-runner");
    make_executable(&server_bin);
    make_executable(&runner_bin);

    let server = parse_server_install_service(&args(&[
        "--bin",
        server_bin.to_str().unwrap(),
        "--env-file",
        "/etc/webcodex/webcodex.env",
        "--working-directory",
        "/var/lib/webcodex",
        "--dry-run",
    ]))
    .unwrap();
    let server_unit = run_server_install_service(server).unwrap();
    assert!(server_unit.contains("EnvironmentFile=/etc/webcodex/webcodex.env\n"));
    assert!(server_unit.contains("WorkingDirectory=/var/lib/webcodex\n"));
    assert!(server_unit.contains("webcodex-server"));
    verify_systemd_unit(&server_unit, "webcodex-default.service");

    let config = tmp.path().join("agent.toml");
    std::fs::write(&config, "server_url = \"http://127.0.0.1\"\n").unwrap();
    let agent = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--bin",
        runner_bin.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--working-directory",
        "/var/lib/webcodex",
        "--dry-run",
    ]))
    .unwrap();
    let agent_unit = run_agent_install_service(agent).unwrap();
    assert!(agent_unit.contains("webcodex-runner\" \"--config\""));
    assert!(agent_unit.contains("WorkingDirectory=/var/lib/webcodex\n"));
    verify_systemd_unit(&agent_unit, "webcodex-runner-default.service");
}

#[cfg(target_os = "linux")]
#[test]
fn special_supported_paths_pass_systemd_analyze_verify() {
    let tmp = tempfile::tempdir().unwrap();
    let server_bin = tmp.path().join("webcodex server%p");
    let runner_bin = tmp.path().join("webcodex runner%p");
    make_executable(&server_bin);
    make_executable(&runner_bin);

    let working = tmp.path().join("work space\"slash\\percent%p");
    std::fs::create_dir(&working).unwrap();
    let env_file = tmp.path().join("env space\"slash\\percent%p.env");
    std::fs::write(&env_file, "WEBCODEX_LISTEN=127.0.0.1:0\n").unwrap();
    let config = tmp.path().join("config space\"slash\\percent%p.toml");
    std::fs::write(&config, "server_url = \"http://127.0.0.1\"\n").unwrap();

    let server = parse_server_install_service(&args(&[
        "--bin",
        server_bin.to_str().unwrap(),
        "--env-file",
        env_file.to_str().unwrap(),
        "--working-directory",
        working.to_str().unwrap(),
        "--dry-run",
    ]))
    .unwrap();
    let server_unit = run_server_install_service(server).unwrap();
    assert!(server_unit.contains("\\x20"));
    assert!(server_unit.contains("\\x22"));
    assert!(server_unit.contains("\\x5c"));
    assert!(server_unit.contains("%%p"));
    verify_systemd_unit(&server_unit, "webcodex-special.service");

    let agent = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--bin",
        runner_bin.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--working-directory",
        working.to_str().unwrap(),
        "--dry-run",
    ]))
    .unwrap();
    let agent_unit = run_agent_install_service(agent).unwrap();
    assert!(agent_unit.contains("\\\"slash\\\\percent%%p.toml"));
    verify_systemd_unit(&agent_unit, "webcodex-runner-special.service");
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn executable_program_rejects_quote_and_backslash_in_dry_run() {
    for path in [
        "/opt/webcodex/web\"codex-server",
        "/opt/webcodex/web\\codex-server",
    ] {
        let opts = parse_server_install_service(&args(&["--bin", path, "--dry-run"])).unwrap();
        let error = run_server_install_service(opts).unwrap_err();
        assert!(error.contains("executable path"), "{error}");
    }
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn agent_output_mode_rejects_invalid_unit_fields() {
    let opts = parse_agent_install_service(&args(&[
        "--scope",
        "system",
        "--user",
        "webcodex",
        "--working-directory",
        "/srv/webcodex",
        "--config",
        "/etc/webcodex/agent.toml\nEnvironment=BAD=1",
        "--bin",
        "/opt/webcodex/bin/webcodex-runner",
        "--output",
        "-",
    ]))
    .unwrap();
    let error = run_agent_install_service(opts).unwrap_err();
    assert!(error.contains("ExecStart --config"));
    assert!(!error.contains("Environment=BAD=1"));
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn agent_status_parses_agent_toml_without_printing_token_and_systemd_unknown() {
    let _guard = env_test_guard();
    let _env = EnvGuard::new().set_os("PATH", OsString::new());
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
    let opts = parse_agent_status(&args(&[
        "--scope",
        "system",
        "--config",
        config.to_str().unwrap(),
        "--json",
    ]))
    .unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = rt.block_on(run_agent_status(opts)).unwrap();
    assert!(!output.contains(secret));
    let json: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(json["service"]["unit"], "webcodex-runner.service");
    assert!(json["service"].get("legacy_unit").is_none());
    assert_eq!(json["service"]["active"], "unknown");
    assert_eq!(json["service"]["enabled"], "unknown");
    assert_eq!(json["config"]["client_id"], "alice-laptop");
    assert_eq!(json["config"]["owner"], "alice");
    assert_eq!(json["config"]["allowed_roots"]["count"], 1);
    assert!(json.get("token").is_none());
    assert!(json["config"].get("token").is_none());
}

/// Unix-only: systemd service unit semantics with Unix absolute-path
/// rules. On Windows the systemd service feature fails closed.
#[cfg(unix)]
#[test]
fn agent_status_rejects_agent_token_in_user_runtime_token_file_without_leaking_it() {
    let _guard = env_test_guard();
    let _env = EnvGuard::new().set_os("PATH", OsString::new());
    let tmp = tempfile::tempdir().unwrap();
    let config = tmp.path().join("agent.toml");
    std::fs::write(
        &config,
        "server_url = \"https://example.test\"\nclient_id = \"alice\"\n",
    )
    .unwrap();
    let token_file = tmp.path().join("webcodex-user-token");
    let secret = "wc_agent_do_not_echo_status_0123456789";
    std::fs::write(&token_file, format!("{secret}\n")).unwrap();
    let opts = parse_agent_status(&args(&[
        "--scope",
        "system",
        "--config",
        config.to_str().unwrap(),
        "--user-token-file",
        token_file.to_str().unwrap(),
    ]))
    .unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let error = runtime.block_on(run_agent_status(opts)).unwrap_err();
    assert!(error.contains("Agent transport token"), "{error}");
    assert!(error.contains("webcodex-user-token"), "{error}");
    assert!(!error.contains(secret));
}

#[cfg(unix)]
#[test]
fn hosted_profile_status_uses_xdg_config_and_never_invokes_systemctl() {
    use std::os::unix::fs::PermissionsExt;

    let _guard = env_test_guard();
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let config_home = tmp.path().join("config");
    let state_home = tmp.path().join("state");
    let profile_config = config_home.join("webcodex/clients/hosted/agent.toml");
    let profile_state = state_home.join("webcodex/clients/hosted");
    let fake_bin = tmp.path().join("bin");
    let systemctl_called = tmp.path().join("systemctl-called");
    std::fs::create_dir_all(profile_config.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&profile_state).unwrap();
    std::fs::create_dir_all(&fake_bin).unwrap();
    std::fs::write(
        &profile_config,
        format!(
            "server_url = \"\"\ntoken = \"hosted-shared-key\"\nclient_id = \"hosted\"\nprojects_dir = {:?}\n",
            profile_config.parent().unwrap().join("projects.d")
        ),
    )
    .unwrap();
    std::fs::write(
        crate::webcodex_cli::local_runner_profile_marker(&profile_state),
        "profile = \"hosted\"\n",
    )
    .unwrap();
    let fake_systemctl = fake_bin.join("systemctl");
    std::fs::write(
        &fake_systemctl,
        format!("#!/bin/sh\n: > {:?}\n", systemctl_called),
    )
    .unwrap();
    std::fs::set_permissions(&fake_systemctl, std::fs::Permissions::from_mode(0o755)).unwrap();

    let _env = EnvGuard::new()
        .set_os("HOME", home.into_os_string())
        .set_os("XDG_CONFIG_HOME", config_home.into_os_string())
        .set_os("XDG_STATE_HOME", state_home.into_os_string())
        .set_os("PATH", fake_bin.into_os_string());
    let opts = parse_agent_status_with_identity(&args(&["--profile", "hosted"]), true).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let output = runtime.block_on(run_agent_status(opts));

    let output = output.unwrap();
    assert!(output.contains("runner mode:          hosted local process"));
    assert!(!systemctl_called.exists());
}

/// Unix-only: systemd service status semantics. On Windows the systemd
/// service feature fails closed.
#[cfg(unix)]
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
    let agent_token_file = tmp.path().join("webcodex-runner-token");
    std::fs::write(&user_token_file, "pat_online_secret_1234567890\n").unwrap();
    std::fs::write(&agent_token_file, "agent_boundary_secret_1234567890\n").unwrap();
    let opts = parse_agent_status(&args(&[
        "--scope",
        "system",
        "--config",
        config.to_str().unwrap(),
        "--server-url",
        &format!("http://{}", addr),
        "--no-system-proxy",
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
