use super::*;
use std::collections::VecDeque;
use std::sync::Mutex;

struct MockGateway {
    expected: Mutex<VecDeque<(&'static str, Result<Value, SshResourceError>)>>,
    calls: Mutex<Vec<Value>>,
}
impl MockGateway {
    fn new(expected: Vec<(&'static str, Result<Value, SshResourceError>)>) -> Self {
        Self {
            expected: Mutex::new(expected.into()),
            calls: Mutex::new(Vec::new()),
        }
    }
    fn actions(&self) -> Vec<String> {
        assert!(
            self.expected.lock().unwrap().is_empty(),
            "expected gateway request was not issued"
        );
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|call| call["action"].as_str().unwrap().into())
            .collect()
    }
}
impl Gateway for MockGateway {
    async fn call(&self, _: &StoredRuntime, arguments: Value) -> Result<Value, SshResourceError> {
        let (action, result) = self
            .expected
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected retry or retarget");
        assert_eq!(arguments["action"], action);
        self.calls.lock().unwrap().push(arguments);
        result
    }
}
fn runtime() -> StoredRuntime {
    StoredRuntime {
        server_url: "http://127.0.0.1:1".into(),
        server_env_file: None,
        runner_config: Some("/private/runner.toml".into()),
        user_token_file: Some("/private/user.token".into()),
        runner_client_id: Some("mini".into()),
        project_id: None,
        runtime_project_id: None,
    }
}
fn resource(name: &str, source: &str) -> Value {
    json!({"name":name,"source":source,"active":true,"pending_restart":false})
}
fn list(resources: Vec<Value>) -> Result<Value, SshResourceError> {
    Ok(json!({"runner":"mini", "binding":"wc_sbind_private-observation", "resources":resources}))
}
fn registered(name: &str) -> Result<Value, SshResourceError> {
    Ok(json!({"resource":name,"persisted":true,"active":true,"restart_required":false}))
}
fn register(name: &str) -> Mutation {
    Mutation::Register {
        name: name.into(),
        target: "private-user@private-host".into(),
        default_cwd: Some("/private/cwd".into()),
    }
}

#[tokio::test]
async fn ssh_list_is_exact_and_public_inventory_has_no_binding_or_target() {
    let gateway = MockGateway::new(vec![(
        "list",
        list(vec![
            resource("managed", "managed"),
            resource("static", "static"),
        ]),
    )]);
    let mut manager = SshResourcesManager::default();
    let snapshot = manager.list(&runtime(), &gateway).await;
    assert!(snapshot.available);
    assert!(snapshot.observation_id.is_some());
    let public = serde_json::to_string(&snapshot).unwrap();
    for forbidden in [
        "wc_sbind_",
        "private",
        "target",
        "hostname",
        "username",
        "default_cwd",
        "config",
        "token",
    ] {
        assert!(!public.contains(forbidden));
    }
    assert_eq!(gateway.actions(), ["list"]);
    assert_eq!(
        gateway.calls.lock().unwrap()[0],
        json!({"action":"list","runner":"mini"})
    );
}

#[tokio::test]
async fn ssh_register_remove_each_consume_observation_then_relist() {
    let gateway = MockGateway::new(vec![
        ("list", list(vec![])),
        ("register", registered("test")),
        ("list", list(vec![resource("test", "managed")])),
        (
            "remove",
            Ok(json!({"resource":"test","persisted":true,"active":false,"restart_required":false})),
        ),
        ("list", list(vec![])),
    ]);
    let mut manager = SshResourcesManager::default();
    let first = manager.list(&runtime(), &gateway).await;
    let added = manager
        .mutate(
            &runtime(),
            first.observation_id.as_deref().unwrap(),
            register("test"),
            &gateway,
        )
        .await;
    assert!(added.success);
    assert_ne!(first.observation_id, added.inventory.observation_id);
    let public = serde_json::to_string(&added).unwrap();
    assert!(!public.contains("private-user") && !public.contains("/private/cwd"));
    let removed = manager
        .mutate(
            &runtime(),
            added.inventory.observation_id.as_deref().unwrap(),
            Mutation::Remove {
                name: "test".into(),
            },
            &gateway,
        )
        .await;
    assert!(removed.success);
    assert!(removed.inventory.resources.is_empty());
    assert_eq!(
        gateway.actions(),
        ["list", "register", "list", "remove", "list"]
    );
    let calls = gateway.calls.lock().unwrap();
    assert_eq!(calls[1]["binding"], "wc_sbind_private-observation");
    assert!(calls[1].get("runner").is_none());
    assert_eq!(calls[1]["target"], "private-user@private-host");
}

#[tokio::test]
async fn ssh_second_mutation_cannot_reuse_previous_observation() {
    let gateway = MockGateway::new(vec![
        ("list", list(vec![])),
        ("register", registered("test")),
        ("list", list(vec![resource("test", "managed")])),
        ("list", list(vec![resource("test", "managed")])),
    ]);
    let mut manager = SshResourcesManager::default();
    let first = manager.list(&runtime(), &gateway).await;
    let id = first.observation_id.unwrap();
    assert!(
        manager
            .mutate(&runtime(), &id, register("test"), &gateway)
            .await
            .success
    );
    let stale = manager
        .mutate(
            &runtime(),
            &id,
            Mutation::Remove {
                name: "test".into(),
            },
            &gateway,
        )
        .await;
    assert_eq!(
        stale.error_kind,
        Some(SshResourceError::SshResourceRegistryStale)
    );
    assert_eq!(gateway.actions(), ["list", "register", "list", "list"]);
}

#[tokio::test]
async fn ssh_static_resource_is_read_only_even_before_server_dispatch() {
    let gateway = MockGateway::new(vec![
        ("list", list(vec![resource("static", "static")])),
        ("list", list(vec![resource("static", "static")])),
    ]);
    let mut manager = SshResourcesManager::default();
    let first = manager.list(&runtime(), &gateway).await;
    let result = manager
        .mutate(
            &runtime(),
            first.observation_id.as_deref().unwrap(),
            Mutation::Remove {
                name: "static".into(),
            },
            &gateway,
        )
        .await;
    assert_eq!(
        result.error_kind,
        Some(SshResourceError::SshResourceStaticReadOnly)
    );
    assert_eq!(gateway.actions(), ["list", "list"]);
}

#[tokio::test]
async fn ssh_unknown_stale_replaced_and_conflict_never_retry_mutation() {
    for error in [
        SshResourceError::SshResourceOutcomeUnknown,
        SshResourceError::RunnerReplaced,
        SshResourceError::SshResourceRegistryStale,
        SshResourceError::SshResourceNameConflict,
        SshResourceError::SshResourceStaticConflict,
        SshResourceError::InsufficientScope,
    ] {
        let gateway = MockGateway::new(vec![
            ("list", list(vec![])),
            ("register", Err(error)),
            ("list", list(vec![])),
        ]);
        let mut manager = SshResourcesManager::default();
        let first = manager.list(&runtime(), &gateway).await;
        let result = manager
            .mutate(
                &runtime(),
                first.observation_id.as_deref().unwrap(),
                register("test"),
                &gateway,
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.error_kind, Some(error));
        assert!(result.inventory.available);
        assert_ne!(first.observation_id, result.inventory.observation_id);
        assert_eq!(gateway.actions(), ["list", "register", "list"]);
    }
}

#[tokio::test]
async fn ssh_connection_or_credential_replacement_invalidates_observation() {
    let gateway = MockGateway::new(vec![("list", list(vec![])), ("list", list(vec![]))]);
    let mut manager = SshResourcesManager::default();
    let first = manager.list(&runtime(), &gateway).await;
    let mut replaced = runtime();
    replaced.user_token_file = Some("/new-enrollment/user.token".into());
    let result = manager
        .mutate(
            &replaced,
            first.observation_id.as_deref().unwrap(),
            register("test"),
            &gateway,
        )
        .await;
    assert_eq!(result.error_kind, Some(SshResourceError::RunnerReplaced));
    assert_eq!(gateway.actions(), ["list", "list"]);
}

#[tokio::test]
async fn ssh_missing_capability_offline_denied_and_invalid_inventory_are_not_empty_success() {
    for response in [
        Err(SshResourceError::SshResourceRegistryUnavailable), // offline/missing managed-SSH capability
        Err(SshResourceError::InsufficientScope),
        Err(SshResourceError::AuthorizationUnavailable),
        Ok(json!({"runner":"other", "binding":"wc_sbind_wrong", "resources":[]})),
        Ok(
            json!({"runner":"mini", "binding":"wc_sbind_private", "resources":[{"name":"leak", "source":"managed","active":true,"pending_restart":false,"target":"private"}]}),
        ),
    ] {
        let gateway = MockGateway::new(vec![("list", response)]);
        let mut manager = SshResourcesManager::default();
        let result = manager.list(&runtime(), &gateway).await;
        assert!(!result.available);
        assert!(result.observation_id.is_none());
        assert!(result.error_kind.is_some());
        assert!(manager.observation.is_none());
        assert!(!serde_json::to_string(&result).unwrap().contains("private"));
    }
}

#[tokio::test]
async fn ssh_pending_restart_is_runner_state_not_local_form_history() {
    let gateway = MockGateway::new(vec![(
        "list",
        list(vec![
            json!({"name":"pending","source":"managed","active":false,"pending_restart":true}),
        ]),
    )]);
    let result = SshResourcesManager::default()
        .list(&runtime(), &gateway)
        .await;
    assert!(result.available);
    assert!(!result.resources[0].active);
    assert!(result.resources[0].pending_restart);
}

#[test]
fn ssh_request_vocabulary_is_closed_and_ssh_alias_is_not_parsed() {
    let base = json!({"expected":{"config_path":"/runner.toml","client_id":"mini","server_url":"http://127.0.0.1:1"},
        "observation_id":"observed", "name":"special", "target":"special", "default_cwd":null});
    assert!(serde_json::from_value::<SshRegisterRequest>(base.clone()).is_ok());
    for (key, value) in [
        ("tool", json!("run_shell")),
        ("runner", json!("other")),
        ("url", json!("https://other.example")),
        ("binding", json!("wc_sbind_any")),
        ("env", json!({})),
    ] {
        let mut input = base.clone();
        input[key] = value;
        assert!(serde_json::from_value::<SshRegisterRequest>(input).is_err());
    }
    let args = Mutation::Register {
        name: "special".into(),
        target: "special".into(),
        default_cwd: None,
    }
    .arguments("binding".into())
    .unwrap();
    assert_eq!(args["target"], "special");
    assert!(args.get("default_cwd").is_none());
    assert!(Mutation::Register {
        name: "special".into(),
        target: "-oIdentityFile=secret".into(),
        default_cwd: None
    }
    .arguments("binding".into())
    .is_err());
}

#[tokio::test]
async fn ssh_http_transport_uses_fixed_gateway_and_native_token_only() {
    use tokio::io::AsyncWriteExt;
    let root = crate::coding_agents::tests::Scratch::new();
    let token_path = root.0.join("user.token");
    tokio::fs::write(&token_path, "fixture-native-token")
        .await
        .unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = tokio::spawn(async move {
        let mut calls = Vec::new();
        for response in [
            Some(json!({"success":true,"output":list(vec![]).unwrap()})),
            None,
            Some(json!({"success":true,"output":list(vec![resource("test","managed")]).unwrap()})),
        ] {
            let (mut stream, _) = tokio::time::timeout(Duration::from_secs(10), listener.accept())
                .await
                .unwrap()
                .unwrap();
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 4096];
            let (header_end, length) = loop {
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() <= 16384);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&bytes[..end])
                        .unwrap()
                        .to_ascii_lowercase();
                    assert!(header.starts_with("post /api/tools/call http/1.1"));
                    assert!(header.contains("authorization: bearer fixture-native-token"));
                    let length: usize = header
                        .lines()
                        .find_map(|line| line.strip_prefix("content-length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let count = stream.read(&mut chunk).await.unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                assert!(bytes.len() <= 16384);
            }
            let body: Value =
                serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
            assert_eq!(body["tool"], "ssh_resource");
            assert!(!body.to_string().contains("fixture-native-token"));
            calls.push(body["params"]["action"].as_str().unwrap().to_owned());
            if let Some(response) = response {
                let body = response.to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}", body.len(), body).as_bytes()).await.unwrap();
            }
            // None simulates a lost response after the Server received the mutation.
        }
        calls
    });
    let mut runtime = runtime();
    runtime.server_url = format!("http://127.0.0.1:{port}/ignored?not_forwarded=1");
    runtime.user_token_file = Some(token_path);
    let mut manager = SshResourcesManager::default();
    let first = manager.list(&runtime, &HttpGateway).await;
    assert!(first.available);
    let result = manager
        .mutate(
            &runtime,
            first.observation_id.as_deref().unwrap(),
            register("test"),
            &HttpGateway,
        )
        .await;
    assert_eq!(
        result.error_kind,
        Some(SshResourceError::SshResourceOutcomeUnknown)
    );
    assert!(result.inventory.available);
    assert_eq!(result.inventory.resources[0].name, "test");
    assert_eq!(server.await.unwrap(), ["list", "register", "list"]);
}
