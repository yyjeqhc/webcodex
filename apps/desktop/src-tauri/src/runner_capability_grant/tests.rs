use super::*;

#[tokio::test]
async fn authorization_status_uses_native_user_token_and_never_contacts_ssh_or_operator() {
    use tokio::io::AsyncWriteExt;
    let dir = crate::coding_agents::tests::Scratch::new();
    let path = dir.0.join("user.token");
    std::fs::write(&path, "fixture-legacy-user-token").unwrap();
    for body in [
        r#"{"coding_agents":false,"ssh_resources":true}"#,
        r#"{"coding_agents":true,"ssh_resources":false}"#,
        r#"{"coding_agents":true,"ssh_resources":true,"token":"private-response-sentinel"}"#,
        r#"{"coding_agents":true}"#,
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut runtime = runtime(listener.local_addr().unwrap().port());
        // This operator path intentionally does not exist; passive observation must not read it.
        runtime.user_token_file = Some(path.clone());
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            while !bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                assert!(bytes.len() < 16384);
            }
            let request = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
            assert!(request.starts_with("post /api/pairing/runner-capabilities/status http/1.1"));
            assert!(request.contains("authorization: bearer fixture-legacy-user-token"));
            assert!(!request.contains("/api/tools/call"));
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",body.len(),body).as_bytes()).await.unwrap();
        });
        let result = observe(&runtime).await;
        server.await.unwrap();
        if body.contains("private-response-sentinel") || !body.contains("ssh_resources") {
            let error = result.err().unwrap();
            assert!(!error.message.contains("private-response-sentinel"));
        } else {
            let result = result.unwrap();
            assert_eq!(result.coding_agents, body.contains("coding_agents\":true"));
            assert!(result.can_authorize);
            let value = serde_json::to_string(&result).unwrap();
            assert!(!value.contains("fixture-legacy-user-token"));
            assert!(!value.contains("server.env"));
        }
    }
}

fn runtime(port: u16) -> StoredRuntime {
    StoredRuntime {
        server_url: format!("http://127.0.0.1:{port}"),
        server_env_file: Some("/private/server.env".into()),
        runner_config: Some("/private/runner.toml".into()),
        user_token_file: None,
        runner_client_id: Some("mini".into()),
        project_id: None,
        runtime_project_id: None,
    }
}

#[test]
fn operator_grant_is_local_only_and_never_accepts_shell_environment_syntax() {
    assert!(can_authorize(&runtime(12345)));
    for url in [
        "https://remote.example:443",
        "http://localhost:12345",
        "http://127.0.0.1:12345/path",
        "http://user:secret@127.0.0.1:12345",
        "http://127.0.0.1:12345/?redirect=remote",
    ] {
        let mut candidate = runtime(12345);
        candidate.server_url = url.into();
        assert!(!can_authorize(&candidate));
    }
    let mut missing = runtime(12345);
    missing.server_env_file = None;
    assert!(!can_authorize(&missing));
    for input in [
        "WEBCODEX_TOKEN=fixture-token",
        "export WEBCODEX_TOKEN='fixture-token'",
        "WEBCODEX_TOKEN=\"fixture-token\"\r\nOTHER=ignored",
    ] {
        assert_eq!(operator_token(input).unwrap(), "fixture-token");
    }
    for input in [
        "",
        "WEBCODEX_TOKEN=",
        "WEBCODEX_TOKEN=$(command)",
        "WEBCODEX_TOKEN=${PRIVATE}",
        "WEBCODEX_TOKEN=secret # comment",
        "WEBCODEX_TOKEN=one\nWEBCODEX_TOKEN=two",
        "WEBCODEX_TOKEN='missing-quote",
    ] {
        let error = operator_token(input).unwrap_err();
        assert!(!error.message.contains(input) || input.is_empty());
    }
}

#[tokio::test]
async fn operator_grant_http_keeps_credentials_native_and_does_not_rewrite_files() {
    use tokio::io::AsyncWriteExt;
    let dir = crate::coding_agents::tests::Scratch::new();
    let path = dir.0.join("user.token");
    tokio::fs::write(&path, "fixture-user-token\n")
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 4096];
        let (end, length) = loop {
            let count = stream.read(&mut buffer).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            assert!(bytes.len() < 16384);
            if let Some(end) = bytes.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                let header = std::str::from_utf8(&bytes[..end])
                    .unwrap()
                    .to_ascii_lowercase();
                assert!(header.starts_with("post /api/pairing/runner-capabilities http/1.1"));
                assert!(header.contains("authorization: bearer fixture-operator-token"));
                let length: usize = header
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                break (end + 4, length);
            }
        };
        while bytes.len() < end + length {
            let count = stream.read(&mut buffer).await.unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        let body: serde_json::Value = serde_json::from_slice(&bytes[end..end + length]).unwrap();
        assert_eq!(body.as_object().unwrap().len(), 2);
        assert_eq!(body["client_id"], "mini");
        assert_eq!(
            body["user_token_hash"],
            format!("{:x}", Sha256::digest(b"fixture-user-token"))
        );
        assert!(!body.to_string().contains("fixture-user-token"));
        assert!(!body.to_string().contains("fixture-operator-token"));
        let response =
            r#"{"success":true,"changed":true,"applied_scopes":["ssh:local","coding_agent:run"]}"#;
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}", response.len(), response).as_bytes()).await.unwrap();
    });
    let mut runtime = runtime(port);
    runtime.user_token_file = Some(path.clone());
    grant(&runtime, "fixture-operator-token".into())
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(
        tokio::fs::read_to_string(path).await.unwrap(),
        "fixture-user-token\n"
    );
}

#[tokio::test]
async fn operator_grant_does_not_retry_unknown_http_outcome() {
    let dir = crate::coding_agents::tests::Scratch::new();
    let path = dir.0.join("user.token");
    std::fs::write(&path, "fixture-user-token").unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut runtime = runtime(listener.local_addr().unwrap().port());
    runtime.user_token_file = Some(path);
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 4096];
        assert!(stream.read(&mut buffer).await.unwrap() > 0);
        drop(stream);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(250), listener.accept())
                .await
                .is_err()
        );
    });
    assert!(grant(&runtime, "fixture-operator-token".into())
        .await
        .is_err());
    server.await.unwrap();
}
