use super::*;

fn input(args: &[&str]) -> Result<Input, String> {
    super::super::parse(
        &args
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>(),
    )
}
fn profile() -> CloudflareTunnelRuntimeProfile {
    CloudflareTunnelRuntimeProfile {
        profile_id: "quick".into(),
        configuration_id: "00000000-0000-4000-8000-000000000001".into(),
        provider: TunnelProvider::CloudflareQuick,
        host_mode: TunnelHostMode::Embedded,
        autostart: false,
        revision: 7,
        runtime_revision: 4,
        owner_username: "owner".into(),
        runner_client_id: Some("client-one".into()),
        ingress_port: 34567,
        local_target: "http://127.0.0.1:34567".into(),
        local_server_url: "http://127.0.0.1:34568".into(),
        bootstrap_token: Secret::new("private-bootstrap-fixture".into()),
        token_file: None,
        readiness_path: PathBuf::from("/private/readiness.json"),
    }
}

#[test]
fn cloudflare_configuration_is_explicit_and_credential_inputs_are_files() {
    let named = input(&[
        "configure-tunnel",
        "named",
        "--provider",
        "cloudflare_named",
        "--public-origin",
        "https://mcp.example.com",
        "--tunnel-id",
        "tunnel-id",
        "--token-file",
        "private-token",
        "--host",
        "embedded",
        "--autostart",
        "false",
        "--expected-revision",
        "7",
        "--ingress-port",
        "34567",
    ])
    .unwrap();
    assert_eq!(
        named.tunnel_provider,
        Some(ServerTunnelProvider::CloudflareNamed)
    );
    assert_eq!(named.autostart, Some(false));
    assert_eq!(named.expected_revision, Some(7));
    assert_eq!(named.ingress_port, Some(34567));
    for args in [
        vec![
            "configure-tunnel",
            "--provider",
            "cloudflare_quick",
            "--token-file",
            "secret",
        ],
        vec![
            "configure-tunnel",
            "--provider",
            "cloudflare_quick",
            "--public-origin",
            "https://stale.example.com",
        ],
        vec![
            "configure-tunnel",
            "--provider",
            "cloudflare_named",
            "--token",
            "literal-secret",
        ],
        vec![
            "configure-tunnel",
            "--provider",
            "cloudflare_named",
            "--credentials-file",
            "private.json",
            "--token-file",
            "token",
        ],
        vec!["configure-tunnel", "--ingress-port", "0"],
        vec!["configure-tunnel", "--expected-revision", "0"],
        vec!["status", "--provider", "cloudflare_named"],
        vec!["cloudflare-oauth", "quick"],
        vec![
            "cloudflare-status",
            "quick",
            "--redirect-uri",
            "https://callback.example.com",
        ],
    ] {
        assert!(input(&args).is_err(), "{args:?}");
    }
}

#[test]
fn control_requests_bind_start_revision_and_stop_process_generation() {
    let profile = profile();
    let status = json!({"server_instance_id":"server-one","process_generation":42});
    let start = action_request(&profile, "start", Some(7), &status).unwrap();
    assert_eq!(start["expected_revision"], 7);
    assert_eq!(start["server_instance_id"], "server-one");
    let stop = action_request(&profile, "stop", None, &status).unwrap();
    assert_eq!(stop["process_generation"], 42);
    assert_eq!(stop["profile_id"], "quick");
    assert!(action_request(
        &profile,
        "stop",
        None,
        &json!({"server_instance_id":"server-one"})
    )
    .is_err());
    let serialized = serde_json::to_string(&start).unwrap();
    assert!(!serialized.contains("private-bootstrap-fixture"));
    assert!(!serialized.contains("/private"));
}

#[test]
fn oauth_scopes_are_bounded_and_identity_sources_do_not_silently_override() {
    let parsed = input(&[
        "cloudflare-oauth",
        "quick",
        "--redirect-uri",
        "https://callback.example.com",
        "--scopes",
        "[\"mcp\"]",
    ])
    .unwrap();
    assert_eq!(parsed.scopes, Some(vec!["mcp".into()]));
    assert!(input(&[
        "cloudflare-oauth",
        "quick",
        "--redirect-uri",
        "https://callback.example.com",
        "--scopes",
        "[1]"
    ])
    .is_err());
    assert!(one_value(Some("one"), Some("two"), None).is_err());
    assert_eq!(one_value(None, None, Some("saved")).unwrap(), "saved");
}

#[test]
fn oauth_replacement_requires_explicit_command_scoped_opt_in() {
    let status = json!({"server_instance_id":"server-one","process_generation":42});
    let args = [
        "cloudflare-oauth",
        "quick",
        "--redirect-uri",
        "https://callback.example.com",
    ];
    let normal = input(&args).unwrap();
    let request = oauth_request(&normal, "quick", &status).unwrap();
    assert_eq!(request["replace"], false);
    assert_eq!(request["server_instance_id"], "server-one");
    assert_eq!(request["process_generation"], 42);
    let mut replace_args = args.to_vec();
    replace_args.push("--replace");
    let replace = input(&replace_args).unwrap();
    let request = oauth_request(&replace, "quick", &status).unwrap();
    assert_eq!(request["replace"], true);
    assert_eq!(request["redirect_uri"], "https://callback.example.com");
    assert_eq!(request["profile_id"], "quick");
    assert_eq!(request["process_generation"], 42);
    for status in [
        json!({}),
        json!({"server_instance_id":"server-one"}),
        json!({"server_instance_id":"server-one","process_generation":0}),
        json!({"server_instance_id":"server-one","process_generation":-1}),
        json!({"server_instance_id":"server-one","process_generation":"42"}),
        json!({"server_instance_id":"server-one","process_generation":u64::MAX}),
        json!({"process_generation":42}),
    ] {
        assert!(oauth_request(&replace, "quick", &status).is_err());
    }
    replace_args.push("--replace");
    assert!(input(&replace_args).is_err());
    for command in [
        "configure-tunnel",
        "cloudflare-start",
        "cloudflare-status",
        "status",
    ] {
        assert!(input(&[command, "quick", "--replace"]).is_err());
    }
}

#[tokio::test]
async fn rejected_control_never_echoes_response_credentials_or_follows_redirect() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let size = socket.read(&mut request).await.unwrap();
        let request = String::from_utf8_lossy(&request[..size]);
        assert!(request.contains("POST /api/connections/cloudflare"));
        assert!(request
            .to_ascii_lowercase()
            .contains("authorization: bearer private-bootstrap-fixture"));
        let body = "private-response-credential";
        socket.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://{address}/credential-leak\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
    });
    let mut profile = profile();
    profile.local_server_url = format!("http://{address}");
    let error = control(&profile, json!({"action":"status","profile_id":"quick"}))
        .await
        .unwrap_err();
    assert!(error.contains("rejected"));
    assert!(!error.contains("private-response-credential"));
    assert!(!error.contains("private-bootstrap-fixture"));
    server.await.unwrap();
}

#[tokio::test]
async fn control_restart_hint_requires_exact_bounded_unapplied_ingress_response() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let restart = json!({
        "error": "cloudflare_ingress_not_applied",
        "next_action": "restart_server",
    })
    .to_string();
    for (status, body, expected) in [
        ("503 Service Unavailable", restart.clone(), INGRESS_RESTART_REQUIRED),
        ("503 Service Unavailable", json!({"error":"cloudflare_ingress_not_applied","next_action":"restart_server","message":"private-response-credential"}).to_string(), CONTROL_ACTION_REJECTED),
        ("502 Bad Gateway", restart.clone(), CONTROL_ACTION_REJECTED),
        ("503 Service Unavailable", json!({"error":"cloudflare_ingress_not_applied"}).to_string(), CONTROL_ACTION_REJECTED),
        ("503 Service Unavailable", json!({"error":"other","next_action":"restart_server"}).to_string(), CONTROL_ACTION_REJECTED),
        ("503 Service Unavailable", json!({"error":"cloudflare_ingress_not_applied","next_action":"other"}).to_string(), CONTROL_ACTION_REJECTED),
        ("503 Service Unavailable", "private-response-credential".into(), CONTROL_ACTION_REJECTED),
        ("503 Service Unavailable", format!("{restart}{}", " ".repeat(64 * 1024)), CONTROL_ACTION_REJECTED),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            socket.read(&mut request).await.unwrap();
            socket.write_all(format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            ).as_bytes()).await.unwrap();
        });
        let mut profile = profile();
        profile.local_server_url = format!("http://{address}");
        let error = control(&profile, json!({"action":"status","profile_id":"quick"}))
            .await
            .unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.contains("private-response-credential"));
        assert!(!error.contains("private-bootstrap-fixture"));
        server.await.unwrap();
    }
}

#[cfg(unix)]
#[test]
fn named_credentials_require_private_file_and_parse_failures_do_not_echo_token() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("named.json");
    std::fs::write(
        &path,
        r#"{"tunnel_id":"id-one","token":"private-token-fixture"}"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        read_named_credentials(&path).unwrap().tunnel_id.as_deref(),
        Some("id-one")
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read_named_credentials(&path).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&path, r#"{"extra":"private-token-fixture"}"#).unwrap();
    let error = read_named_credentials(&path).err().unwrap();
    assert!(!error.contains("private-token-fixture"));
}

#[test]
fn successful_control_projects_only_typed_safe_status_and_explicit_oauth_issuance() {
    let profile = profile();
    let status = json!({"profile_id":"quick","server_instance_id":"server-one","process_generation":42,"lifecycle":"running","public_origin":"https://example.trycloudflare.com","oauth_configured":true,"observed_authorization":false,"configured_revision":7,"applied_revision":4,"local_target":"http://127.0.0.1:34567","reason_code":null,"bootstrap_token":"must-not-be-projected","client_secret":"must-not-be-projected"});
    let projected = project_response(&profile, "status", status.clone()).unwrap();
    let encoded = projected.to_string();
    assert!(!encoded.contains("must-not-be-projected"));
    let mut wrong = status;
    wrong["profile_id"] = "other".into();
    assert!(project_response(&profile, "status", wrong).is_err());
    let issued = project_response(&profile,"configure_oauth",json!({"client_id":"client-one","client_secret":"shown-once","already_configured":false,"bootstrap_token":"never-show"})).unwrap();
    assert_eq!(issued["client_secret"], "shown-once");
    assert!(!issued.to_string().contains("never-show"));
    let repeated = project_response(
        &profile,
        "configure_oauth",
        json!({"client_id":"client-one","client_secret":null,"already_configured":true}),
    )
    .unwrap();
    assert!(repeated["client_secret"].is_null());
    assert_eq!(repeated["already_configured"], true);
}
