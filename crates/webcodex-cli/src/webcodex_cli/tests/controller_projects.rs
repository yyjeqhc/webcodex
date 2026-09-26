use super::*;
use crate::webcodex_cli::controller::{parse_controller_command, ControllerCommand};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

fn online() -> Value {
    json!({"runners":[{"client_id":"runner-a","status":"online",
        "project_inventory":{"sync_state":"complete"}}]})
}

fn inventory() -> Value {
    json!({"truncated":false,"projects":[{"id":"agent:runner-a:demo",
        "agent_project_id":"demo","client_id":"runner-a","path":"/workspace/demo",
        "revision":"revision-from-server"}],"project_inventory":{"sync_state":"complete"}})
}

// One response per expected request; EOF simulates an uncertain mutation outcome.
async fn server(
    responses: Vec<Option<Value>>,
) -> (ProjectClient, tokio::task::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut requests = Vec::new();
            for response in responses {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                reader.read_line(&mut first).await.unwrap();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).await.unwrap();
                    if line == "\r\n" { break; }
                    if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).await.unwrap();
                requests.push(json!({"route":first.split_whitespace().nth(1),
                    "body":serde_json::from_slice::<Value>(&bytes).unwrap()}));
                if let Some(output) = response {
                    let status = output.get("fixture_http_status").and_then(Value::as_u64).unwrap_or(200);
                    let body = if output.get("success").is_some() { output } else { json!({"success":true,"output":output}) }.to_string();
                    let response = format!("HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    reader.get_mut().write_all(response.as_bytes()).await.unwrap();
                }
            }
            requests
        }).await.unwrap()
    });
    (
        ProjectClient {
            server: format!("http://{address}"),
            client_id: "runner-a".into(),
            token: "fixture-user-token".into(),
            http: ServerHttpOptions {
                no_system_proxy: true,
                ..Default::default()
            },
        },
        task,
    )
}

#[test]
fn project_parser_accepts_optional_user_token_for_all_actions() {
    for args in [
        vec!["list"],
        vec!["register", "/workspace"],
        vec!["remove", "demo"],
    ] {
        for explicit in [false, true] {
            let mut words = vec!["project"];
            words.extend(args.clone());
            words.extend(["--config", "/controller.toml", "--json"]);
            if explicit {
                words.extend(["--user-token-file", "/user-token"]);
            }
            let command =
                parse_controller_command(&words.iter().map(|s| s.to_string()).collect::<Vec<_>>())
                    .unwrap();
            assert!(
                matches!(command, ControllerCommand::Project { user_token_file, json:true, .. }
                if user_token_file == explicit.then(|| PathBuf::from("/user-token")))
            );
        }
    }
}

#[tokio::test]
async fn offline_runner_blocks_all_commands_before_project_requests() {
    for action in [
        ProjectAction::List,
        ProjectAction::Register {
            project: "/missing".into(),
        },
        ProjectAction::Remove {
            target: "demo".into(),
        },
    ] {
        let (client, task) = server(vec![Some(
            json!({"runners":[{"client_id":"runner-a","status":"stale"}]}),
        )])
        .await;
        assert!(client
            .execute(action)
            .await
            .unwrap_err()
            .contains("runner_offline"));
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0]["body"]["tool"], "list_runners");
    }
}

#[tokio::test]
async fn online_list_uses_scoped_api_and_preserves_truncation() {
    let mut projects = inventory();
    projects["truncated"] = json!(true);
    let (client, task) = server(vec![Some(online()), Some(projects), Some(online())]).await;
    let result = client.execute(ProjectAction::List).await.unwrap();
    assert_eq!(result["truncated"], true);
    let requests = task.await.unwrap();
    assert_eq!(requests[1]["body"]["tool"], "list_projects");
    assert_eq!(requests[1]["body"]["params"]["client_id"], "runner-a");
}

#[tokio::test]
async fn online_register_uses_operator_endpoint_without_writing_registry() {
    let directory = tempfile::tempdir().unwrap();
    let (client, task) = server(vec![
        Some(online()),
        Some(json!({"id":"agent:runner-a:demo"})),
    ])
    .await;
    client
        .execute(ProjectAction::Register {
            project: directory.path().to_path_buf(),
        })
        .await
        .unwrap();
    let requests = task.await.unwrap();
    assert_eq!(requests[1]["route"], "/api/projects/resolve-or-register");
    assert_eq!(
        requests[1]["body"]["path"],
        json!(directory.path().canonicalize().unwrap())
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn remove_uses_exact_server_identity_and_revision() {
    let (client, task) = server(vec![
        Some(online()),
        Some(inventory()),
        Some(online()),
        Some(json!({"project":"agent:runner-a:demo","outcome":"unregistered"})),
    ])
    .await;
    client
        .execute(ProjectAction::Remove {
            target: "demo".into(),
        })
        .await
        .unwrap();
    let requests = task.await.unwrap();
    assert_eq!(
        requests[3]["body"],
        json!({"tool":"unregister_project","params":{
        "project":"agent:runner-a:demo","expected_revision":"revision-from-server"}})
    );
}

#[tokio::test]
async fn full_project_id_uses_exact_server_filter() {
    let (client, task) = server(vec![
        Some(online()),
        Some(inventory()),
        Some(online()),
        Some(json!({"project":"agent:runner-a:demo","outcome":"unregistered"})),
    ])
    .await;
    client
        .execute(ProjectAction::Remove {
            target: "agent:runner-a:demo".into(),
        })
        .await
        .unwrap();
    let requests = task.await.unwrap();
    assert_eq!(
        requests[1]["body"]["params"],
        json!({
        "client_id":"runner-a","limit":100,"project":"agent:runner-a:demo"})
    );
}

#[tokio::test]
async fn missing_revision_does_not_dispatch_unregister() {
    let mut projects = inventory();
    projects["projects"][0]
        .as_object_mut()
        .unwrap()
        .remove("revision");
    let (client, task) = server(vec![Some(online()), Some(projects), Some(online())]).await;
    assert!(client
        .execute(ProjectAction::Remove {
            target: "demo".into()
        })
        .await
        .unwrap_err()
        .contains("omitted revision"));
    assert_eq!(task.await.unwrap().len(), 3);
}

#[test]
fn controller_help_describes_online_projects_and_optional_credentials() {
    let help = crate::webcodex_cli::controller_usage();
    assert!(help.contains("--user-token-file PATH"));
    assert!(help.contains("configured Runner online"));
}

#[tokio::test]
async fn lost_mutation_response_is_unknown_and_not_retried() {
    let (client, task) = server(vec![
        Some(online()),
        Some(inventory()),
        Some(online()),
        None,
    ])
    .await;
    assert!(client
        .execute(ProjectAction::Remove {
            target: "demo".into()
        })
        .await
        .unwrap_err()
        .contains("outcome_unknown"));
    assert_eq!(task.await.unwrap().len(), 4);
}

#[tokio::test]
async fn another_runner_inventory_is_rejected() {
    let mut output = inventory();
    output["projects"][0]["client_id"] = json!("other");
    let (client, task) = server(vec![Some(online()), Some(output)]).await;
    assert!(client
        .execute(ProjectAction::List)
        .await
        .unwrap_err()
        .contains("another Runner"));
    task.await.unwrap();
}

#[test]
fn incomplete_and_ambiguous_inventory_cannot_delete() {
    let mut output = inventory();
    output["truncated"] = json!(true);
    assert!(select_project(&output, "demo")
        .unwrap_err()
        .contains("incomplete"));
    output["truncated"] = json!(false);
    output["project_inventory"]["sync_state"] = json!("pending");
    assert!(select_project(&output, "demo")
        .unwrap_err()
        .contains("incomplete"));
    output["project_inventory"]["sync_state"] = json!("complete");
    let duplicate = output["projects"][0].clone();
    output["projects"].as_array_mut().unwrap().push(duplicate);
    assert!(select_project(&output, "demo")
        .unwrap_err()
        .contains("ambiguous"));
}

fn connection(base: &Path, user: &str, client: &str) -> connections::ConnectionPaths {
    let slug = connections::canonical_server_url("https://example.test")
        .unwrap()
        .slug;
    let paths = connections::ConnectionPaths::new(base.join(slug).join(user));
    std::fs::create_dir_all(&paths.dir).unwrap();
    std::fs::write(
        &paths.descriptor,
        format!("server_url = \"https://example.test\"\nusername = \"{user}\"\n"),
    )
    .unwrap();
    std::fs::write(
        &paths.runner_config,
        format!("server_url = \"https://example.test\"\nclient_id = \"{client}\"\n"),
    )
    .unwrap();
    paths
}

#[test]
fn default_token_is_bound_to_server_runner_and_exact_connection() {
    let temp = tempfile::tempdir().unwrap();
    let alice = connection(temp.path(), "alice", "runner-a");
    let bob = connection(temp.path(), "bob", "runner-b");
    let view = runner_view(&alice.runner_config).unwrap();
    let bases = vec![temp.path().to_path_buf()];
    assert_eq!(
        select_default_token(Path::new("/custom/runner.toml"), &view, &bases).unwrap(),
        alice.user_token
    );
    connection(temp.path(), "bob", "runner-a");
    assert!(select_default_token(Path::new("/custom/runner.toml"), &view, &bases).is_err());
    assert_eq!(
        select_default_token(&bob.runner_config.canonicalize().unwrap(), &view, &bases).unwrap(),
        bob.user_token
    );
    let other = RunnerConfigView {
        server_url: "https://other.test".into(),
        client_id: "runner-a".into(),
    };
    assert!(select_default_token(&alice.runner_config, &other, &bases).is_err());
}

#[tokio::test]
async fn authentication_and_visibility_failures_are_not_reported_as_offline() {
    for (response, expected) in [
        (
            json!({"fixture_http_status":401,"success":false,"error":"unauthorized"}),
            "project_access_denied",
        ),
        (
            json!({"fixture_http_status":403,"success":false,"error":"forbidden"}),
            "project_access_denied",
        ),
        (
            json!({"fixture_http_status":404,"success":false,"error":"not found"}),
            "project_api_unavailable",
        ),
        (json!({"runners":[]}), "runner_not_visible"),
    ] {
        let (client, task) = server(vec![Some(response)]).await;
        assert!(client
            .execute(ProjectAction::List)
            .await
            .unwrap_err()
            .contains(expected));
        assert_eq!(task.await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn online_empty_inventory_is_success_but_disconnect_is_an_error() {
    let empty = json!({"projects":[],"truncated":false});
    let (client, task) = server(vec![Some(online()), Some(empty.clone()), Some(online())]).await;
    assert_eq!(
        client.execute(ProjectAction::List).await.unwrap()["projects"],
        json!([])
    );
    task.await.unwrap();
    let (client, task) = server(vec![
        Some(online()),
        Some(empty),
        Some(json!({"runners":[{
        "client_id":"runner-a","status":"stale"}]})),
    ])
    .await;
    assert!(client
        .execute(ProjectAction::List)
        .await
        .unwrap_err()
        .contains("runner_offline"));
    task.await.unwrap();
}

#[tokio::test]
async fn revision_conflict_preserves_canonical_error_and_does_not_retry() {
    let error = json!({"success":false,"error":"revision_conflict","output":{
        "error_kind":"revision_conflict","state_changed":false}});
    let (client, task) = server(vec![
        Some(online()),
        Some(inventory()),
        Some(online()),
        Some(error.clone()),
    ])
    .await;
    let actual = client
        .execute(ProjectAction::Remove {
            target: "demo".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(
        serde_json::from_str::<Value>(&format_error(actual, true)).unwrap(),
        error
    );
    assert_eq!(task.await.unwrap().len(), 4);
}

#[tokio::test]
async fn registration_policy_denial_does_not_extend_allowed_roots() {
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("runner.toml");
    let original = "[policy]\nallowed_roots = []\n";
    std::fs::write(&config, original).unwrap();
    let (client, task) = server(vec![
        Some(online()),
        Some(json!({"success":false,
        "error":"path_outside_allowed_roots","output":{"state_changed":false}})),
    ])
    .await;
    assert!(client
        .execute(ProjectAction::Register {
            project: directory.path().to_path_buf()
        })
        .await
        .unwrap_err()
        .contains("path_outside_allowed_roots"));
    assert_eq!(std::fs::read_to_string(config).unwrap(), original);
    assert_eq!(task.await.unwrap().len(), 2);
}

#[tokio::test]
async fn explicit_invalid_credentials_never_fall_back_to_default() {
    let directory = tempfile::tempdir().unwrap();
    let paths = connection(directory.path(), "alice", "runner-a");
    std::fs::write(&paths.user_token, "valid-default-fixture").unwrap();
    let config: ControllerConfig = toml::from_str(&format!(
        "[server]\nmode = 'remote'\nurl = 'https://example.test'\n[runner]\nconfig = {:?}\n",
        paths.runner_config.to_str().unwrap()
    ))
    .unwrap();
    let explicit = directory.path().join("explicit-token");
    let error = run(&config, ProjectAction::List, Some(&explicit), true)
        .await
        .unwrap_err();
    assert!(serde_json::from_str::<Value>(&error).unwrap()["error"]
        .as_str()
        .unwrap()
        .contains("failed to read"));
    std::fs::write(&explicit, "").unwrap();
    assert!(run(&config, ProjectAction::List, Some(&explicit), false)
        .await
        .unwrap_err()
        .contains("empty"));
    std::fs::write(&explicit, "wc_agent_transport-fixture").unwrap();
    assert!(run(&config, ProjectAction::List, Some(&explicit), false)
        .await
        .unwrap_err()
        .contains("Runner transport token"));
    assert_eq!(
        default_token_file(
            &paths.runner_config,
            &runner_view(&paths.runner_config).unwrap()
        )
        .unwrap(),
        paths.user_token
    );
}
