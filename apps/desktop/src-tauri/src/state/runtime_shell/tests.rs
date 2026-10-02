use super::*;
use serde_json::json;
use std::io::{Read, Write};

fn runner_build() -> webcodex_core::desktop_runtime_contract::MachineBuildInfo {
    let mut build = webcodex_build_info::machine_build_info("webcodex-runner");
    build.git_commit = Some("runner-source".into());
    build.git_dirty = Some(true);
    build
}

fn runner_details() -> Value {
    json!({
        "client_id": "selected-runner", "connected": true,
        "version": runner_build().version,
        "build_git_commit": "runner-source", "build_git_dirty": true,
        "jobs_running": 2, "jobs_queued": 1
    })
}

#[tokio::test]
async fn selection_observes_saved_runner_identity_and_jobs_instead_of_fleet_overview() {
    // Existing Desktop state may identify the Runner through its exact Project
    // identity; both forms must observe the same saved Runner.
    for (legacy_identity, active_jobs) in [(false, 0), (false, 3), (true, 0), (true, 3)] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut response = runner_details();
        if active_jobs == 0 {
            response["jobs_running"] = json!(0);
            response["jobs_queued"] = json!(0);
        }
        let server = tokio::task::spawn_blocking(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "request did not arrive"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept failed: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let (headers, body) = loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&buffer[..count]);
                assert!(request.len() <= 16_384);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8(request[..end].to_vec()).unwrap();
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break (headers, request[end + 4..end + 4 + length].to_vec());
                    }
                }
            };
            let is_runner = headers.starts_with("POST /api/runtime-console/runner HTTP/1.1\r\n")
                && serde_json::from_slice::<Value>(&body).unwrap()["client_id"]
                    == "selected-runner";
            let response = if is_runner {
                response
            } else {
                json!({"service":"webcodex", "version":"server-version", "active_jobs":99,
                "build_git_commit":"server-source", "runners":[response]})
            };
            let body = serde_json::to_vec(&response).unwrap();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
            is_runner
        });
        let data = tempfile::tempdir().unwrap();
        let token = data.path().join("user-token");
        std::fs::write(&token, "fixture-user-token").unwrap();
        let runtime = StoredRuntime {
            server_url: format!("http://{address}"),
            runner_client_id: (!legacy_identity).then(|| "selected-runner".into()),
            user_token_file: Some(token),
            runner_config: None,
            server_env_file: None,
            project_id: Some("project".into()),
            runtime_project_id: Some("agent:selected-runner:project".into()),
        };
        let config = StoredDesktopConfig {
            runtime: Some(runtime),
            ..Default::default()
        };
        let observed = observe_runtime_runner(&config).await.unwrap();
        assert!(server.await.unwrap());
        assert_eq!(observed_active_jobs(&observed), Some(active_jobs));
        verify_selected_runner(&observed, "selected-runner", &runner_build()).unwrap();
    }
}

#[test]
fn fleet_overview_cannot_prove_the_selected_runners_jobs_or_identity() {
    let overview = json!({
        "service": "webcodex", "version": runner_build().version,
        "build_git_commit": "server-source", "build_git_dirty": false,
        "active_jobs": 99, "runners": [runner_details()]
    });
    assert_eq!(observed_active_jobs(&overview), None);
    assert_eq!(
        verify_selected_runner(&overview, "selected-runner", &runner_build())
            .unwrap_err()
            .code,
        "selected_runtime_not_active"
    );
}

#[tokio::test]
async fn runner_observation_requires_saved_identity_before_querying() {
    let mut config = StoredDesktopConfig::default();
    assert_eq!(
        observe_runtime_runner(&config).await.unwrap_err().code,
        "runtime_identity_unavailable"
    );
    config.runtime = Some(StoredRuntime {
        server_url: "http://127.0.0.1:9".into(),
        runner_client_id: None,
        user_token_file: None,
        runner_config: None,
        server_env_file: None,
        project_id: None,
        runtime_project_id: None,
    });
    // A request would fail authentication before contacting the endpoint.
    // Missing identity must instead fail before entering the query path.
    assert_eq!(
        observe_runtime_runner(&config).await.unwrap_err().code,
        "runtime_identity_unavailable"
    );
}

#[test]
fn selected_runner_must_be_connected_and_match_every_build_identity_field() {
    for (field, value) in [
        ("connected", json!(false)),
        ("client_id", json!("other-runner")),
        ("version", json!("other-version")),
        ("build_git_commit", json!("other-source")),
        ("build_git_dirty", json!(false)),
        ("client_id", Value::Null),
        ("connected", Value::Null),
        ("version", Value::Null),
        ("build_git_commit", Value::Null),
        ("build_git_dirty", Value::Null),
    ] {
        let mut observed = runner_details();
        observed[field] = value;
        assert_eq!(
            verify_selected_runner(&observed, "selected-runner", &runner_build())
                .unwrap_err()
                .code,
            "selected_runtime_not_active",
            "{field}"
        );
    }
    let mut disconnected = runner_details();
    disconnected["connected"] = json!(false);
    assert_eq!(observed_active_jobs(&disconnected), None);
}

#[test]
fn a_connection_change_invalidates_a_pending_candidate_without_a_binary_revision_change() {
    let initial = StoredDesktopConfig::default();
    let mut changed = initial.clone();
    changed.topology = Some(RuntimeTopology {
        experience: Experience::Full,
        server: ServerTopology::Local,
        runner: RunnerTopology::Local,
        exposure: Exposure::None,
        enrollment: Enrollment::ManagedPairing,
    });
    assert_ne!(selection_context(&initial), selection_context(&changed));
}

#[test]
fn an_update_check_does_not_invalidate_runtime_selection_authority() {
    let initial = StoredDesktopConfig::default();
    let mut changed = initial.clone();
    changed.update_cache.last_check_at_ms = Some(42);
    assert_eq!(selection_context(&initial), selection_context(&changed));
}
