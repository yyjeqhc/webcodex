use super::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn legacy_coding_only_connection_can_observe_confirm_and_grant_without_ssh_or_project() {
    let dir = crate::coding_agents::tests::Scratch::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let env = dir.0.join("server.env");
    let user = dir.0.join("user.token");
    std::fs::write(
        &env,
        format!("WEBCODEX_ADDR={addr}\nWEBCODEX_TOKEN=fixture-operator\n"),
    )
    .unwrap();
    std::fs::write(&user, "fixture-legacy-user").unwrap();
    let runtime = StoredRuntime {
        server_url: format!("http://{addr}"),
        server_env_file: Some(env.clone()),
        runner_config: Some(dir.0.join("runner.toml")),
        user_token_file: Some(user.clone()),
        runner_client_id: Some("mini".into()),
        project_id: None,
        runtime_project_id: None,
    };
    let expected = crate::webcodex::settings::target(&runtime).unwrap();
    let app = AppState::new(dir.0.clone(), dir.0.join("resources")).unwrap();
    app.core.lock().await.as_mut().unwrap().config.runtime = Some(runtime);
    let server = tokio::spawn(async move {
        for (path, credential, body) in [
            (
                "/api/pairing/runner-capabilities/status",
                "fixture-legacy-user",
                r#"{"coding_agents":false,"ssh_resources":false}"#,
            ),
            (
                "/api/pairing/runner-capabilities",
                "fixture-operator",
                r#"{"success":true,"changed":true,"applied_scopes":["ssh:local","coding_agent:run"]}"#,
            ),
            (
                "/api/pairing/runner-capabilities/status",
                "fixture-legacy-user",
                r#"{"coding_agents":true,"ssh_resources":true}"#,
            ),
        ] {
            let (mut stream, _) =
                tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
                    .await
                    .unwrap()
                    .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let (end, size) = loop {
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                assert!(bytes.len() < 16384);
                if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&bytes[..end])
                        .unwrap()
                        .to_ascii_lowercase();
                    assert!(
                        header.starts_with(&format!("post {path} http/1.1")),
                        "wrong endpoint: no SSH inventory is allowed"
                    );
                    assert!(header.contains(&format!("authorization: bearer {credential}")));
                    let size: usize = header
                        .lines()
                        .find_map(|x| x.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    break (end + 4, size);
                }
            };
            while bytes.len() < end + size {
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            assert!(!std::str::from_utf8(&bytes[end..])
                .unwrap()
                .contains("fixture-"));
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(150), listener.accept())
                .await
                .is_err(),
            "unexpected SSH request or grant retry"
        );
    });
    let mut wrong = expected.clone();
    wrong.client_id = "replaced".into();
    assert!(app.runner_capability_authorization(wrong).await.is_err());
    assert!(app
        .authorize_runner_capabilities(crate::runner_capability_grant::GrantRequest {
            expected: expected.clone(),
            confirmed: false
        })
        .await
        .is_err());
    let before = app
        .runner_capability_authorization(expected.clone())
        .await
        .unwrap();
    assert!(before.can_authorize);
    assert!(!before.coding_agents);
    let after = app
        .authorize_runner_capabilities(crate::runner_capability_grant::GrantRequest {
            expected,
            confirmed: true,
        })
        .await
        .unwrap();
    assert!(after.coding_agents && after.ssh_resources);
    assert!(!serde_json::to_string(&after).unwrap().contains("fixture-"));
    assert_eq!(
        std::fs::read_to_string(user).unwrap(),
        "fixture-legacy-user"
    );
    assert_eq!(
        std::fs::read_to_string(env).unwrap(),
        format!("WEBCODEX_ADDR={addr}\nWEBCODEX_TOKEN=fixture-operator\n")
    );
    assert!(app.get_state().current_operation.is_none());
    server.await.unwrap();
}
