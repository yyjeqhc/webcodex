use super::super::support::*;

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
