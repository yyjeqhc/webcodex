use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct HttpRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Value,
}

fn fixture(
    expected: usize,
    reply: impl Fn(&HttpRequest) -> (u16, Vec<(&'static str, &'static str)>, Value) + Send + 'static,
) -> (
    String,
    Arc<Mutex<Vec<HttpRequest>>>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(Vec::<HttpRequest>::new()));
    let captured = requests.clone();
    let handle = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut accepted = 0usize;
        while accepted < expected {
            let (mut stream, _) = match listener.accept() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        let paths: Vec<_> = captured
                            .lock()
                            .unwrap()
                            .iter()
                            .map(|request| request.path.clone())
                            .collect();
                        panic!(
                            "HTTP fixture timed out after {accepted}/{expected} requests; captured paths: {paths:?}"
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => panic!("HTTP fixture accept failed: {error}"),
            };
            let request = read_request(&mut stream);
            let (status, headers, body) = reply(&request);
            captured.lock().unwrap().push(request);
            accepted += 1;
            let body = body.to_string();
            let mut response = format!("HTTP/1.1 {status} Fixture\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n", body.len());
            for (name, value) in headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            stream.write_all(response.as_bytes()).unwrap();
            stream.write_all(body.as_bytes()).unwrap();
        }
    });
    (url, requests, handle)
}

fn read_request(stream: &mut TcpStream) -> HttpRequest {
    // macOS accepts sockets with the listener's nonblocking flag; Linux does
    // not. Request parsing uses bounded blocking reads on either platform.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let header_end = loop {
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "HTTP client closed before sending headers");
        bytes.extend_from_slice(&chunk[..read]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
        assert!(bytes.len() < 64 * 1024);
    };
    let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
    let mut lines = headers.split("\r\n");
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let mut length = 0;
    let mut authorization = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap();
            }
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_owned());
            }
        }
    }
    while bytes.len() - header_end < length {
        let mut chunk = [0u8; 1024];
        let read = stream.read(&mut chunk).unwrap();
        assert!(read > 0, "HTTP client closed before sending the body");
        bytes.extend_from_slice(&chunk[..read]);
    }
    let body = if length == 0 {
        Value::Null
    } else {
        serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap()
    };
    HttpRequest {
        method,
        path,
        authorization,
        body,
    }
}

#[cfg(unix)]
#[test]
fn http_fixture_clears_inherited_nonblocking_mode_before_reading() {
    use std::os::fd::AsRawFd;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut stream, _) = listener.accept().unwrap();
    // Force macOS's accepted-socket state on Linux CI as well.
    stream.set_nonblocking(true).unwrap();
    client
        .write_all(b"GET /fixture HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let request = read_request(&mut stream);
    assert_eq!(request.method, "GET");
    assert_eq!(request.path, "/fixture");
    let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) };
    assert_ne!(flags, -1);
    assert_eq!(flags & libc::O_NONBLOCK, 0);
    assert!(stream.read_timeout().unwrap().is_some());
}

fn record(
    server_url: String,
    project: Option<std::path::PathBuf>,
    mode: EnvironmentMode,
) -> EnvironmentRecord {
    let binary = std::env::current_exe().unwrap();
    EnvironmentRecord {
        schema_version: ENVIRONMENT_SCHEMA,
        environment_id: "fixture-environment".into(),
        request: SetupRequest {
            service_scope: service::ServiceScope::System,
            mode,
            server_url,
            project,
            runner: None,
            account: current_account().unwrap(),
            binaries: RuntimeBinaries {
                cli: binary.clone(),
                server: binary.clone(),
                runner: binary,
            },
        },
        username: None,
        runner_client_id: None,
        projects: Vec::new(),
        configured: false,
    }
}

#[test]
fn user_plans_keep_all_components_on_the_same_manager_without_system_entrypoints() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record(
        "http://127.0.0.1:18880".into(),
        None,
        EnvironmentMode::Create {
            listen: "127.0.0.1:18880".into(),
        },
    );
    saved.request.runner = Some(true);
    saved.runner_client_id = Some("user-runner".into());
    saved.request.service_scope = service::ServiceScope::User;
    // Pure plans: no platform manager or local credentials are touched.
    for spec in [
        service_spec(&store, &saved, Component::Server).unwrap(),
        service_spec(&store, &saved, Component::Runner).unwrap(),
        crate::tunnel_service_spec(&store, &saved, "default").unwrap(),
    ] {
        assert_eq!(spec.scope, service::ServiceScope::User);
        assert!(spec.linux_socket.is_none());
        assert!(matches!(
            spec.account,
            ServiceAccount::SystemUser { group: None, .. }
        ));
        for arg in ["--windows-service", "--computer-session-dir"] {
            assert!(!spec.args.iter().any(|value| value == arg));
        }
        #[cfg(windows)]
        assert!(spec.environment.is_empty());
    }
    assert!(!store.root().join("runner.toml").exists());
    assert!(!store.root().join("webcodex-user-token").exists());
}

#[tokio::test]
async fn projectless_runner_keeps_bounded_default_policy_and_an_empty_registry() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record("https://server.example".into(), None, EnvironmentMode::Join);
    saved.request.runner = Some(true);
    saved.runner_client_id = Some("existing-runner".into());
    saved.username = Some("alice".into());
    store
        .write_json(
            "enrollment-recovery.json",
            &Enrollment {
                server_url: saved.request.server_url.clone(),
                client_id: "existing-runner".into(),
                username: "alice".into(),
                user_token: "wc_user_fixture".into(),
                runner_token: "wc_agent_fixture".into(),
            },
        )
        .unwrap();
    NativeEnvironment::new()
        .unwrap()
        .configure_runner(&store, &mut saved)
        .await
        .unwrap();
    let config: toml::Value =
        toml::from_str(&std::fs::read_to_string(store.root().join("runner.toml")).unwrap())
            .unwrap();
    assert_eq!(config["client_id"].as_str(), Some("existing-runner"));
    assert_eq!(
        config["policy"]["allow_cwd_anywhere"].as_bool(),
        Some(false)
    );
    assert_eq!(
        config["policy"]["allowed_roots"].as_array().unwrap().len(),
        1
    );
    assert!(saved.projects.is_empty());
    assert_eq!(
        std::fs::read_dir(store.root().join("project-registry"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn projectless_runner_still_rejects_root_and_project_without_runner() {
    let mut saved = record("https://server.example".into(), None, EnvironmentMode::Join);
    saved.request.runner = Some(true);
    saved.request.account.identity = "0".into();
    assert_eq!(
        validate_request(&saved.request).unwrap_err().code,
        "project_user_required"
    );
    saved.request.runner = Some(false);
    saved.request.project = Some(std::env::current_dir().unwrap());
    assert_eq!(
        validate_request(&saved.request).unwrap_err().code,
        "runner_required"
    );
}

#[tokio::test]
async fn credential_repair_rejects_another_server_user_without_changing_saved_state() {
    let (url, requests, server) = fixture(1, |_| {
        (
            200,
            vec![],
            json!({"service":"webcodex","authenticated_user":"bob"}),
        )
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record(url, None, EnvironmentMode::Join);
    saved.username = Some("alice".into());
    saved.runner_client_id = Some("existing-runner".into());
    saved.configured = true;
    store.save_environment(&saved).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_old").unwrap();
    atomic_private_write(&store.root().join("runner.toml"), b"existing runner config").unwrap();

    let error = NativeEnvironment::new()
        .unwrap()
        .repair_user_credential(
            &store,
            &saved.environment_id,
            &Secret::new("wc_pat_new".into()),
        )
        .await
        .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "user_identity_conflict");
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_old"
    );
    assert_eq!(
        std::fs::read(store.root().join("runner.toml")).unwrap(),
        b"existing runner config"
    );
    assert_eq!(
        store
            .load_environment()
            .unwrap()
            .unwrap()
            .runner_client_id
            .as_deref(),
        Some("existing-runner")
    );
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].path, "/api/runtime-console/overview");
    assert_eq!(
        requests[0].authorization.as_deref(),
        Some("Bearer wc_pat_new")
    );
}

#[tokio::test]
async fn credential_repair_updates_recovery_before_active_token_without_changing_runner() {
    let (url, requests, server) = fixture(1, |_| {
        (
            200,
            vec![],
            json!({"service":"webcodex","authenticated_user":"alice"}),
        )
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record(url.clone(), None, EnvironmentMode::Join);
    saved.username = Some("alice".into());
    saved.runner_client_id = Some("existing-runner".into());
    saved.configured = true;
    store.save_environment(&saved).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_old").unwrap();
    atomic_private_write(&store.root().join("runner.toml"), b"existing runner config").unwrap();
    store
        .write_json(
            "enrollment-recovery.json",
            &Enrollment {
                server_url: url,
                client_id: "existing-runner".into(),
                username: "alice".into(),
                user_token: "wc_pat_old".into(),
                runner_token: "wc_agent_existing".into(),
            },
        )
        .unwrap();
    let before = serde_json::to_value(store.load_environment().unwrap().unwrap()).unwrap();

    NativeEnvironment::new()
        .unwrap()
        .repair_user_credential(
            &store,
            &saved.environment_id,
            &Secret::new("wc_pat_new".into()),
        )
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_new"
    );
    let recovery: Enrollment = store
        .read_json("enrollment-recovery.json")
        .unwrap()
        .unwrap();
    assert_eq!(recovery.user_token, "wc_pat_new");
    assert_eq!(recovery.runner_token, "wc_agent_existing");
    assert_eq!(
        std::fs::read(store.root().join("runner.toml")).unwrap(),
        b"existing runner config"
    );
    assert_eq!(
        serde_json::to_value(store.load_environment().unwrap().unwrap()).unwrap(),
        before
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn credential_repair_rejects_changed_environment_and_missing_frozen_user_before_network() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record("http://127.0.0.1:1".into(), None, EnvironmentMode::Join);
    saved.configured = true;
    store.save_environment(&saved).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_old").unwrap();
    let native = NativeEnvironment::new().unwrap();
    assert_eq!(
        native
            .repair_user_credential(
                &store,
                "another-environment",
                &Secret::new("wc_pat_new".into())
            )
            .await
            .unwrap_err()
            .code,
        "environment_changed"
    );
    assert_eq!(
        native
            .repair_user_credential(
                &store,
                &saved.environment_id,
                &Secret::new("wc_pat_new".into())
            )
            .await
            .unwrap_err()
            .code,
        "server_identity_contract"
    );
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_old"
    );
}

#[tokio::test]
async fn credential_repair_requires_server_to_attest_user_identity() {
    let (url, _, server) = fixture(1, |_| (200, vec![], json!({"service":"webcodex"})));
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record(url, None, EnvironmentMode::Join);
    saved.username = Some("alice".into());
    saved.configured = true;
    store.save_environment(&saved).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_old").unwrap();

    let error = NativeEnvironment::new()
        .unwrap()
        .repair_user_credential(
            &store,
            &saved.environment_id,
            &Secret::new("wc_pat_new".into()),
        )
        .await
        .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "server_identity_contract");
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_old"
    );
}

#[tokio::test]
async fn viewer_credential_repair_replaces_only_user_token_without_enrollment() {
    let (url, requests, server) = fixture(1, |_| {
        (
            200,
            vec![],
            json!({"service":"webcodex","authenticated_user":"alice"}),
        )
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record(url, None, EnvironmentMode::Join);
    saved.username = Some("alice".into());
    saved.configured = true;
    store.save_environment(&saved).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_expired").unwrap();

    NativeEnvironment::new()
        .unwrap()
        .repair_user_credential(
            &store,
            &saved.environment_id,
            &Secret::new("wc_pat_valid".into()),
        )
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_valid"
    );
    assert!(!store.root().join("enrollment-recovery.json").exists());
    assert!(!store.root().join("runner.toml").exists());
    assert!(store
        .load_environment()
        .unwrap()
        .unwrap()
        .runner_client_id
        .is_none());
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn service_control_rejects_changed_environment_before_native_service_access() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut saved = record("http://127.0.0.1:1".into(), None, EnvironmentMode::Join);
    saved.configured = true;
    store.save_environment(&saved).unwrap();
    let error = NativeEnvironment::new()
        .unwrap()
        .control_service_for_environment(
            &store,
            Some("another-environment"),
            Component::Runner,
            ServiceOperation::Start,
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "environment_changed");
}

#[tokio::test]
async fn viewer_configure_uses_only_read_only_overview_and_keeps_runner_identity_absent() {
    let (url, requests, server) = fixture(4, |request| match request.path.as_str() {
        "/runtime" => (
            200,
            vec![("x-webcodex-console-assets", "embedded")],
            json!({}),
        ),
        "/api/runtime-console/overview" => (
            200,
            vec![],
            json!({"authenticated_user":"alice","service":"webcodex","version":"fixture", "projects_available":true, "projects_truncated":false,
                "runners":[{"client_id":"B","connected":true,"status":"online","computer_session_availability":true}, {"client_id":"C","connected":true,"status":"stale","computer_session_availability":true}],
                "projects":[{"id":"agent:B:repo","client_id":"B","connected":true,"path":"/remote/only/repository"}, {"id":"agent:C:other","client_id":"C","connected":false,"path":"C:\\projects\\other"}],
                "unexpected_secret":"must-not-project"}),
        ),
        other => panic!("unexpected viewer route: {other}"),
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let request = record(url, None, EnvironmentMode::Join).request;
    let mut setup = EnvironmentSetup::new(NativeEnvironment::new().unwrap());
    let result = setup
        .configure(
            &store,
            request,
            &SetupSecrets {
                user_token: Some(Secret::new("wc_pat_viewer".into())),
                ..Default::default()
            },
            |_| {},
        )
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(result.environment.username.as_deref(), Some("alice"));
    assert_eq!(result.environment.runner_client_id, None);
    assert_eq!(result.observation.runner_online, None);
    let fleet = result.observation.fleet.as_ref().unwrap();
    assert_eq!(fleet.runners.len(), 2);
    assert_eq!(fleet.projects.len(), 2);
    assert_eq!(fleet.runners[1].computer_session_availability, Some(false));
    assert_eq!(
        fleet.projects[0].path.as_deref(),
        Some("/remote/only/repository")
    );
    assert!(!serde_json::to_string(fleet)
        .unwrap()
        .contains("must-not-project"));
    assert!(result.observation.authenticated && result.observation.server_reachable);
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/runtime")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/api/runtime-console/overview")
            .count(),
        3
    );
    assert!(requests
        .iter()
        .filter(|request| request.method == "POST")
        .all(
            |request| request.authorization.as_deref() == Some("Bearer wc_pat_viewer")
                && request.body == json!({})
        ));
    assert!(!store.root().join("runner.toml").exists());
}

#[tokio::test]
async fn older_server_viewer_works_but_runner_transition_requires_user_identity() {
    let (url, requests, server) = fixture(5, |request| match request.path.as_str() {
        "/runtime" => (
            200,
            vec![("x-webcodex-console-assets", "embedded")],
            json!({}),
        ),
        "/api/runtime-console/overview" => {
            (200, vec![], json!({"service":"webcodex","version":"older"}))
        }
        other => panic!("unexpected old-server viewer route: {other}"),
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let request = record(url, None, EnvironmentMode::Join).request;
    let mut setup = EnvironmentSetup::new(NativeEnvironment::new().unwrap());
    let result = setup
        .configure(
            &store,
            request,
            &SetupSecrets {
                user_token: Some(Secret::new("wc_pat_old_viewer".into())),
                ..Default::default()
            },
            |_| {},
        )
        .await
        .unwrap();
    assert_eq!(result.environment.username, None);
    assert_eq!(result.environment.runner_client_id, None);
    assert!(result.observation.authenticated);

    // Preflight normally rejects root/service accounts before a Runner
    // transition can contact the Server. Use a stable non-root identity only on
    // this synthetic transition record so the test reaches the older-Server
    // identity contract without changing the already-configured viewer state.
    let mut transition = result.environment;
    transition.request.project = Some(temp.path().canonicalize().unwrap());
    transition.request.account.identity = "fixture-user-id".into();
    let secrets = SetupSecrets {
        pairing_code: Some(Secret::new("wc_pair_unused".into())),
        ..Default::default()
    };
    let error = EnvironmentBackend::apply(
        &mut setup.backend,
        &store,
        &mut transition,
        SetupStep::Preflight,
        &secrets,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "server_identity_contract");
    server.join().unwrap();
    assert!(transition.runner_client_id.is_none());
    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests.last().unwrap().path,
        "/api/runtime-console/overview"
    );
}

#[tokio::test]
async fn doctor_reports_viewer_auth_and_protocol_without_requiring_local_runner() {
    let (url, requests, server) = fixture(2, |_| {
        (
            200,
            vec![],
            json!({"service":"webcodex","version":"fixture","authenticated_user":"alice"}),
        )
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let mut environment = record(url, None, EnvironmentMode::Join);
    environment.configured = true;
    store.save_environment(&environment).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_viewer").unwrap();
    let report = NativeEnvironment::new()
        .unwrap()
        .doctor(&store)
        .await
        .unwrap();
    server.join().unwrap();
    assert!(report.observation.authenticated);
    assert_eq!(report.observation.runner_online, None);
    let codes: Vec<_> = report
        .observation
        .diagnostics
        .iter()
        .map(|value| value.code.as_str())
        .collect();
    assert!(codes.contains(&"server_protocol"));
    assert!(codes.contains(&"server_user_verified"));
    assert!(!codes
        .iter()
        .any(|code| code.starts_with("service_runner") || code == &"runner_offline"));
    assert!(requests
        .lock()
        .unwrap()
        .iter()
        .all(|request| request.path == "/api/runtime-console/overview"));
}

#[test]
fn service_diagnostic_reports_manager_state_without_runtime_output() {
    let status = service::ServiceStatus {
        id: "webcodex-runner-fixture".into(),
        ownership: Ownership::Owned,
        installed: true,
        enabled: Some(true),
        running: Some(false),
        detail: Some("private runtime output must not be copied".into()),
    };
    let diagnostic = service_diagnostic("runner", &status);
    assert_eq!(diagnostic.code, "service_runner_owned_stopped");
    assert!(diagnostic.message.contains("boot=enabled"));
    assert!(!format!("{diagnostic:?}").contains("private runtime output"));
    #[cfg(target_os = "linux")]
    assert!(diagnostic.recovery.contains("journalctl"));
}

#[test]
fn loopback_server_urls_use_the_direct_client() {
    let native = NativeEnvironment::new().unwrap();
    for url in [
        "http://127.0.0.1:8080",
        "http://127.42.0.7:8080",
        "http://[::1]:8080",
        "http://LOCALHOST:8080",
        "http://localhost.:8080",
    ] {
        assert!(server_url_is_loopback(url), "expected loopback: {url}");
        assert!(std::ptr::eq(
            native.client_for_server(url),
            &native.direct_client
        ));
    }
    for url in [
        "https://example.com",
        "http://192.168.1.10:8080",
        "not a server url",
    ] {
        assert!(!server_url_is_loopback(url), "expected non-loopback: {url}");
        assert!(std::ptr::eq(native.client_for_server(url), &native.client));
    }
}

#[tokio::test]
async fn loopback_server_reachability_uses_the_direct_client() {
    let (url, requests, server) = fixture(1, |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/runtime");
        (
            200,
            vec![("x-webcodex-console-assets", "embedded")],
            json!({"service":"webcodex"}),
        )
    });
    let environment = record(url, None, EnvironmentMode::Join);
    NativeEnvironment::new()
        .unwrap()
        .reachable(&environment)
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_generic_http_200_does_not_satisfy_server_reachability() {
    let (url, _, server) = fixture(1, |_| (200, vec![], json!({"service":"other"})));
    let environment = record(url, None, EnvironmentMode::Join);
    assert_eq!(
        NativeEnvironment::new()
            .unwrap()
            .reachable(&environment)
            .await
            .unwrap_err()
            .code,
        "server_unreachable"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn uncertain_user_auth_does_not_reregister_saved_credential() {
    let (url, requests, server) = fixture(1, |request| match request.path.as_str() {
        "/api/runtime-console/overview" => (503, vec![], json!({"error":"unavailable"})),
        other => panic!("unexpected mutating retry: {other}"),
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    ensure_private_directory(&store.root().join("server")).unwrap();
    atomic_private_write(
        &store.root().join("server/webcodex.env"),
        b"WEBCODEX_TOKEN=wc_boot_fixture\n",
    )
    .unwrap();
    atomic_private_write(
        &store.root().join("webcodex-user-token"),
        b"wc_pat_existing",
    )
    .unwrap();
    let mut environment = record(
        url,
        None,
        EnvironmentMode::Create {
            listen: "127.0.0.1:8080".into(),
        },
    );
    let error = NativeEnvironment::new()
        .unwrap()
        .create_user(&store, &mut environment)
        .await
        .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "server_request_failed");
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_existing"
    );
    assert_eq!(requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn first_user_creation_registers_only_hash_after_auth_rejection() {
    let overview_count = std::sync::atomic::AtomicUsize::new(0);
    let (url, requests, server) = fixture(5, move |request| match request.path.as_str() {
        "/api/users/create" => (
            200,
            vec![],
            json!({"success":true,"user":{"username":"fixture"}}),
        ),
        "/api/runtime-console/overview"
            if overview_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2 =>
        {
            (401, vec![], json!({"error":"unauthorized"}))
        }
        "/api/runtime-console/overview" => (
            200,
            vec![],
            json!({"authenticated_user":"fixture","service":"webcodex"}),
        ),
        "/api/tokens/register_hash" => (
            200,
            vec![],
            json!({"success":true,"token":{"token_prefix":"wc_pat_invalid_f"}}),
        ),
        other => panic!("unexpected user provisioning route: {other}"),
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    ensure_private_directory(&store.root().join("server")).unwrap();
    atomic_private_write(
        &store.root().join("server/webcodex.env"),
        b"WEBCODEX_TOKEN=wc_boot_fixture\n",
    )
    .unwrap();
    atomic_private_write(
        &store.root().join("webcodex-user-token"),
        b"wc_pat_invalid_fixture",
    )
    .unwrap();
    let mut environment = record(
        url,
        None,
        EnvironmentMode::Create {
            listen: "127.0.0.1:8080".into(),
        },
    );
    environment.username = Some("fixture".into());
    NativeEnvironment::new()
        .unwrap()
        .create_user(&store, &mut environment)
        .await
        .unwrap();
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "/api/runtime-console/overview",
            "/api/users/create",
            "/api/runtime-console/overview",
            "/api/tokens/register_hash",
            "/api/runtime-console/overview"
        ]
    );
    let registration = &requests[3];
    assert_eq!(
        registration.authorization.as_deref(),
        Some("Bearer wc_boot_fixture")
    );
    assert_eq!(registration.body["username"], "fixture");
    assert_eq!(
        registration.body["token_hash"],
        format!("sha256:{:x}", Sha256::digest(b"wc_pat_invalid_fixture"))
    );
    assert_eq!(registration.body["token_prefix"], "wc_pat_invalid_f");
    assert!(!registration.body.to_string().contains("wc_boot_fixture"));
}

#[tokio::test]
async fn pairing_conflict_preserves_viewer_identity_and_saves_recovery_credentials() {
    let (url, requests, server) = fixture(1, |_| {
        (
            200,
            vec![],
            json!({"success":true,"client_id":"alice-client","username":"bob","user_token":"wc_pat_bob","agent_token":"wc_agent_bob"}),
        )
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_alice").unwrap();
    let mut environment = record(url, Some(temp.path().to_path_buf()), EnvironmentMode::Join);
    environment.username = Some("alice".into());
    environment.runner_client_id = Some("alice-client".into());
    let secrets = SetupSecrets {
        pairing_code: Some(Secret::new("wc_pair_once".into())),
        ..Default::default()
    };
    let mut backend = NativeEnvironment::new().unwrap();
    let error = EnvironmentBackend::apply(
        &mut backend,
        &store,
        &mut environment,
        SetupStep::RunnerEnrollment,
        &secrets,
    )
    .await
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error.code, "pairing_user_conflict");
    assert_eq!(environment.username.as_deref(), Some("alice"));
    assert_eq!(
        read_secret(&store.root().join("webcodex-user-token"))
            .unwrap()
            .expose(),
        "wc_pat_alice"
    );
    let recovery: Enrollment = store
        .read_json("enrollment-recovery.json")
        .unwrap()
        .unwrap();
    assert_eq!(recovery.username, "bob");
    let requests = requests.lock().unwrap();
    assert_eq!(requests[0].path, "/api/pairing/enroll");
    assert_eq!(requests[0].authorization, None);
    assert_eq!(
        requests[0].body,
        json!({"pairing_code":"wc_pair_once","client_id":"alice-client","transport":"auto"})
    );
}

#[tokio::test]
async fn uncertain_project_removal_is_not_dispatched_again() {
    let (url, requests, server) = fixture(5, |request| match request.path.as_str() {
        "/api/tools/call" if request.body["tool"] == "list_projects" => (
            200,
            vec![],
            json!({"success":true,"output":{"projects":[{"id":"agent:alice-client:project-a","revision":"revision-a"}]}}),
        ),
        "/api/tools/call" if request.body["tool"] == "unregister_project" => {
            (503, vec![], json!({"error":"uncertain"}))
        }
        "/api/runtime-console/overview" => (
            200,
            vec![],
            json!({"authenticated_user":"alice","service":"webcodex"}),
        ),
        "/api/runtime-console/runner" => (
            200,
            vec![],
            json!({"connected":true,"projects":[{"id":"agent:alice-client:project-a"}]}),
        ),
        other => panic!("unexpected removal route: {other}"),
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_alice").unwrap();
    let mut environment = record(url, Some(temp.path().to_path_buf()), EnvironmentMode::Join);
    environment.username = Some("alice".into());
    environment.runner_client_id = Some("alice-client".into());
    environment.projects.push(ProjectRecord {
        id: "project-a".into(),
        path: temp.path().to_path_buf(),
    });
    store.save_environment(&environment).unwrap();
    let mut backend = NativeEnvironment::new().unwrap();
    assert_eq!(
        backend
            .remove_project(&store, "project-a", Some("revision-a"))
            .await
            .unwrap_err()
            .code,
        "server_request_failed"
    );
    assert_eq!(
        backend
            .remove_project(&store, "project-a", Some("revision-a"))
            .await
            .unwrap_err()
            .code,
        "project_reconcile_required"
    );
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.body["tool"] == "unregister_project")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .find(|request| request.body["tool"] == "unregister_project")
            .unwrap()
            .body["params"]["expected_revision"],
        "revision-a"
    );
    let pending: ProjectRemoval = store.read_json("remove-project.json").unwrap().unwrap();
    assert!(pending.dispatched && !pending.confirmed);
    assert_eq!(store.load_environment().unwrap().unwrap().projects.len(), 1);
}

#[tokio::test]
async fn uncertain_project_addition_is_not_dispatched_again() {
    let (url, requests, server) = fixture(6, |request| match request.path.as_str() {
        "/api/runtime-console/projects" => (200, vec![], json!({"projects":[]})),
        "/api/tools/call" if request.body["tool"] == "check_runner_config" => (
            200,
            vec![],
            json!({"success":true,"output":{"valid":true,"restart_required":false,"current_generation":7}}),
        ),
        "/api/tools/call" if request.body["tool"] == "reload_runner_config" => {
            (200, vec![], json!({"success":true,"output":{}}))
        }
        "/api/projects/resolve-or-register" => (503, vec![], json!({"error":"uncertain"})),
        other => panic!("unexpected addition route: {other}"),
    });
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_alice").unwrap();
    let mut environment = record(
        url.clone(),
        Some(temp.path().to_path_buf()),
        EnvironmentMode::Join,
    );
    environment.request.account.identity = "fixture-user-id".into();
    environment.username = Some("alice".into());
    environment.runner_client_id = Some("alice-client".into());
    store.save_environment(&environment).unwrap();
    let config = format!(
        "server_url = {url:?}\nclient_id = \"alice-client\"\n[policy]\nallowed_roots = []\n"
    );
    atomic_private_write(&store.root().join("runner.toml"), config.as_bytes()).unwrap();
    let mut backend = NativeEnvironment::new().unwrap();
    assert_eq!(
        backend
            .add_project(&store, temp.path())
            .await
            .unwrap_err()
            .code,
        "project_reconcile_required"
    );
    assert_eq!(
        backend
            .add_project(&store, temp.path())
            .await
            .unwrap_err()
            .code,
        "project_reconcile_required"
    );
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/api/projects/resolve-or-register")
            .count(),
        1
    );
    let pending: ProjectAddition = store.read_json("add-project.json").unwrap().unwrap();
    assert!(pending.dispatched);
    assert!(store
        .load_environment()
        .unwrap()
        .unwrap()
        .projects
        .is_empty());
}

#[test]
fn new_setup_cannot_reuse_migration_only_listen_permission() {
    let mut request = record(
        "http://127.0.0.1:8080".into(),
        None,
        EnvironmentMode::Create {
            listen: "0.0.0.0:8080".into(),
        },
    )
    .request;
    assert_eq!(
        validate_request(&request).unwrap_err().code,
        "listen_address"
    );
    assert!(validate_request_with_preserved_listen(&request, true).is_ok());
    request.mode = EnvironmentMode::Create {
        listen: "0.0.0.0:0".into(),
    };
    assert!(validate_request_with_preserved_listen(&request, true).is_err());
}
