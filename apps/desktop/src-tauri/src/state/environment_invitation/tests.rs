use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use webcodex_environment::service::ServiceScope;
use webcodex_environment::{
    EnvironmentMode, EnvironmentRecord, LocalAccount, RuntimeBinaries, SetupJournal, SetupRequest,
    ENVIRONMENT_SCHEMA,
};

const ID: &str = "invitation-fixture";
const CODE: &str = "wc_pair_fixture_only";

struct Fixture {
    _temp: tempfile::TempDir,
    app: Arc<AppState>,
    store: EnvironmentStore,
}

impl Fixture {
    fn new(url: String, local: bool) -> Self {
        let temp = tempfile::Builder::new()
            .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
            .unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let app = Arc::new(
            AppState::new(temp.path().join("desktop"), temp.path().join("resources")).unwrap(),
        );
        store
            .save_environment(&record(store.root(), url, local))
            .unwrap();
        private_file(
            &store.root().join("server/webcodex.env"),
            b"WEBCODEX_TOKEN=wc_boot_test_fixture\n",
        );
        private_file(
            &store.root().join("server/server.db"),
            b"database must remain unchanged",
        );
        private_file(
            &store.root().join("runner.toml"),
            b"runner identity must remain unchanged",
        );
        private_file(
            &store.root().join("webcodex-user-token"),
            b"wc_pat_test_fixture",
        );
        drop(store.lock().unwrap());
        {
            let mut published = app.published.write().unwrap();
            published.persistent_environment = Some(ID.into());
            published.readiness = aggregate_readiness(
                ServerReadiness::Ready,
                RunnerReadiness::Ready,
                ExposureReadiness::LocalReady,
                ProjectReadiness::Ready,
            );
        }
        Self {
            _temp: temp,
            app,
            store,
        }
    }

    async fn unchanged(&self, before: &(Value, Value, BTreeMap<String, Vec<u8>>)) {
        assert_eq!(
            serde_json::to_value(&*self.app.published.read().unwrap()).unwrap(),
            before.0
        );
        let core = self.app.core.lock().await;
        assert_eq!(
            serde_json::to_value(&core.as_ref().unwrap().config).unwrap(),
            before.1
        );
        assert_eq!(files(self.store.root()), before.2);
        let mut supervisor = self.app.supervisor.lock().await;
        assert!(supervisor.snapshot(ProcessKey::LocalServer).is_none());
        assert!(supervisor.snapshot(ProcessKey::LocalRunner).is_none());
    }

    async fn baseline(&self) -> (Value, Value, BTreeMap<String, Vec<u8>>) {
        let core = self.app.core.lock().await;
        (
            serde_json::to_value(&*self.app.published.read().unwrap()).unwrap(),
            serde_json::to_value(&core.as_ref().unwrap().config).unwrap(),
            files(self.store.root()),
        )
    }
}

fn record(root: &Path, url: String, local: bool) -> EnvironmentRecord {
    let binary = std::env::current_exe().unwrap();
    EnvironmentRecord {
        schema_version: ENVIRONMENT_SCHEMA,
        environment_id: ID.into(),
        request: SetupRequest {
            service_scope: ServiceScope::System,
            mode: if local {
                EnvironmentMode::Create {
                    listen: "127.0.0.1:8787".into(),
                }
            } else {
                EnvironmentMode::Join
            },
            server_url: url,
            project: None,
            runner: Some(false),
            account: LocalAccount {
                name: "fixture".into(),
                identity: "fixture-user-identity".into(),
                home: root.to_owned(),
            },
            binaries: RuntimeBinaries {
                cli: binary.clone(),
                server: binary.clone(),
                runner: binary,
            },
        },
        username: Some("fixture-user".into()),
        runner_client_id: Some("fixture-runner-identity".into()),
        projects: Vec::new(),
        configured: true,
    }
}

#[cfg(windows)]
fn copy_windows_private_security(source: &Path, target: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };

    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut owner = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            source.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    assert_eq!(status, 0, "fixture security source must be readable");
    assert!(!owner.is_null() && !dacl.is_null() && !descriptor.is_null());
    let status = unsafe {
        SetNamedSecurityInfoW(
            target.as_mut_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            owner,
            std::ptr::null_mut(),
            dacl,
            std::ptr::null_mut(),
        )
    };
    unsafe { LocalFree(descriptor) };
    assert_eq!(status, 0, "fixture path must inherit private test security");
}

fn private_file(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    #[cfg(windows)]
    {
        let root = path
            .ancestors()
            .find(|candidate| candidate.join("environment.json").is_file())
            .expect("fixture environment root");
        let reference = root.join("environment.json");
        if path.parent() != Some(root) {
            copy_windows_private_security(&reference, path.parent().unwrap());
        }
        copy_windows_private_security(&reference, path);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path.parent().unwrap(),
            std::fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    std::fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

struct HttpRequest {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Value,
}

fn read_request(stream: &mut TcpStream) -> HttpRequest {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut bytes = Vec::new();
    let end = loop {
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).unwrap();
        assert!(count > 0, "fixture received incomplete request");
        bytes.extend_from_slice(&chunk[..count]);
        assert!(bytes.len() <= 64 * 1024);
        if let Some(index) = bytes.windows(4).position(|item| item == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
    let mut lines = headers.split("\r\n");
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().to_owned();
    let path = first.next().unwrap().to_owned();
    let mut length = 0;
    let mut authorization = None;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse::<usize>().unwrap();
            }
            if name.eq_ignore_ascii_case("authorization") {
                authorization = Some(value.trim().to_owned());
            }
        }
    }
    assert!(length <= 64 * 1024);
    while bytes.len() < end + length {
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).unwrap();
        assert!(count > 0);
        bytes.extend_from_slice(&chunk[..count]);
    }
    HttpRequest {
        method,
        path,
        authorization,
        body: serde_json::from_slice(&bytes[end..end + length]).unwrap(),
    }
}

// A single bounded local HTTP exchange. The gate makes late completion deterministic.
fn server(
    status: u16,
    body: Value,
) -> (
    String,
    tokio::sync::oneshot::Receiver<HttpRequest>,
    mpsc::Sender<()>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (request_tx, request_rx) = tokio::sync::oneshot::channel();
    let (gate_tx, gate_rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "invitation HTTP request timed out"
                    );
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("fixture accept failed: {error}"),
            }
        };
        assert!(request_tx.send(read_request(&mut stream)).is_ok());
        gate_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let body = body.to_string();
        write!(stream, "HTTP/1.1 {status} Fixture\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}", body.len()).unwrap();
    });
    (url, request_rx, gate_tx, handle)
}

fn request(id: &str) -> InvitationRequest {
    InvitationRequest {
        environment_id: id.into(),
    }
}
fn spawn_invite(fixture: &Fixture) -> tokio::task::JoinHandle<DesktopResult<InvitationResponse>> {
    let app = fixture.app.clone();
    let store = fixture.store.clone();
    tokio::spawn(async move {
        app.create_environment_invitation_in(&store, request(ID))
            .await
    })
}
fn error_code(result: DesktopResult<InvitationResponse>) -> String {
    match result {
        Err(error) => error.code,
        Ok(_) => panic!("invitation unexpectedly succeeded"),
    }
}

#[test]
fn ipc_request_accepts_only_the_expected_environment_identity() {
    let valid: InvitationRequest = serde_json::from_value(json!({"environmentId": ID})).unwrap();
    assert_eq!(valid.environment_id, ID);
    for extra in ["serverUrl", "url", "ttlSecs", "username", "pairingCode"] {
        let mut payload = json!({"environmentId": ID});
        payload[extra] = json!("untrusted");
        assert!(serde_json::from_value::<InvitationRequest>(payload).is_err());
    }
}

#[tokio::test]
async fn invitation_is_a_ten_minute_authorized_response_without_persistent_secret_or_runtime_changes(
) {
    let (url, captured, gate, thread) = server(200, json!({"pairing_code": CODE}));
    let fixture = Fixture::new(url, true);
    let baseline = fixture.baseline().await;
    let task = spawn_invite(&fixture);
    let http = captured.await.unwrap();
    assert_eq!(http.method, "POST");
    assert_eq!(http.path, "/api/pairing/create");
    assert_eq!(
        http.authorization.as_deref(),
        Some("Bearer wc_boot_test_fixture")
    );
    assert_eq!(
        http.body,
        json!({"username": "fixture-user", "ttl_secs": 600})
    );
    gate.send(()).unwrap();
    let response = task.await.unwrap().unwrap();
    thread.join().unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({"environmentId": ID, "pairingCode": CODE})
    );
    fixture.unchanged(&baseline).await;
    assert!(!serde_json::to_string(&fixture.app.get_state())
        .unwrap()
        .contains(CODE));
    assert!(!serde_json::to_string(&fixture.app.activity.snapshot())
        .unwrap()
        .contains(CODE));
    assert!(fixture.app.operations.current().is_none());
}

#[tokio::test]
async fn target_identity_must_match_both_desktop_and_saved_environment() {
    let fixture = Fixture::new("http://127.0.0.1:8787".into(), true);
    let baseline = fixture.baseline().await;
    assert_eq!(
        error_code(
            fixture
                .app
                .create_environment_invitation_in(&fixture.store, request("other"))
                .await
        ),
        "environment_changed"
    );
    fixture.unchanged(&baseline).await;
    let mut saved = fixture.store.load_environment().unwrap().unwrap();
    saved.environment_id = "replacement".into();
    fixture.store.save_environment(&saved).unwrap();
    let baseline = fixture.baseline().await;
    assert_eq!(
        error_code(
            fixture
                .app
                .create_environment_invitation_in(&fixture.store, request(ID))
                .await
        ),
        "environment_changed"
    );
    fixture.unchanged(&baseline).await;
}

#[tokio::test]
async fn remote_environment_cannot_gain_server_authority_from_desktop_topology() {
    let fixture = Fixture::new("http://127.0.0.1:8787".into(), false);
    fixture.app.published.write().unwrap().topology = Some(RuntimeTopology {
        experience: Experience::Full,
        server: ServerTopology::Local,
        runner: RunnerTopology::Local,
        exposure: Exposure::None,
        enrollment: Enrollment::ManagedPairing,
    });
    let baseline = fixture.baseline().await;
    assert_eq!(
        error_code(
            fixture
                .app
                .create_environment_invitation_in(&fixture.store, request(ID))
                .await
        ),
        "server_admin_required"
    );
    fixture.unchanged(&baseline).await;
}

#[tokio::test]
async fn server_authorization_failure_preserves_configuration_readiness_services_and_database() {
    for (status, expected) in [
        (401, "authentication_required"),
        (403, "permission_denied"),
        (503, "server_request_failed"),
    ] {
        let (url, captured, gate, thread) =
            server(status, json!({"pairing_code": CODE, "error": "forbidden"}));
        let fixture = Fixture::new(url, true);
        let baseline = fixture.baseline().await;
        let task = spawn_invite(&fixture);
        let _http = captured.await.unwrap();
        gate.send(()).unwrap();
        assert_eq!(error_code(task.await.unwrap()), expected);
        thread.join().unwrap();
        fixture.unchanged(&baseline).await;
        assert!(!serde_json::to_string(&fixture.app.activity.snapshot())
            .unwrap()
            .contains(CODE));
    }
}

#[tokio::test]
async fn duplicate_operation_is_busy_and_late_response_is_discarded_after_target_changes() {
    for replace_saved_record in [false, true] {
        let (url, captured, gate, thread) = server(200, json!({"pairing_code": CODE}));
        let fixture = Fixture::new(url, true);
        let task = spawn_invite(&fixture);
        let _http = captured.await.unwrap();
        assert_eq!(
            error_code(
                fixture
                    .app
                    .create_environment_invitation_in(&fixture.store, request(ID))
                    .await
            ),
            "desktop_operation_busy"
        );
        if replace_saved_record {
            let mut saved = fixture.store.load_environment().unwrap().unwrap();
            saved.environment_id = "replacement".into();
            fixture.store.save_environment(&saved).unwrap();
        } else {
            fixture
                .app
                .published
                .write()
                .unwrap()
                .persistent_environment = Some("replacement".into());
        }
        let baseline = fixture.baseline().await;
        gate.send(()).unwrap();
        assert_eq!(error_code(task.await.unwrap()), "environment_changed");
        thread.join().unwrap();
        fixture.unchanged(&baseline).await;
        assert!(!serde_json::to_string(&fixture.app.activity.snapshot())
            .unwrap()
            .contains(CODE));
        assert!(fixture.app.operations.current().is_none());
    }
}

#[test]
fn setup_projection_preserves_saved_and_pending_roles_scope_and_optional_project_without_secrets() {
    let fixture = Fixture::new("https://server.example".into(), true);
    let saved = setup_snapshot_in(&fixture.store, Some(ID))
        .unwrap()
        .unwrap();
    assert_eq!(saved.mode, "create");
    assert!(!saved.runner);
    assert!(saved.project_path.is_none());
    assert_eq!(saved.service_scope, ServiceScope::System);
    assert!(saved.configured);
    assert!(setup_snapshot_in(&fixture.store, Some("other"))
        .unwrap()
        .is_none());
    let mut pending = fixture.store.load_environment().unwrap().unwrap();
    pending.configured = false;
    pending.request.mode = EnvironmentMode::Join;
    pending.request.runner = Some(true);
    pending.request.project = Some(fixture.store.root().join("project"));
    pending.request.service_scope = ServiceScope::User;
    fixture
        .store
        .save_journal(&SetupJournal {
            schema_version: ENVIRONMENT_SCHEMA,
            operation_id: "pending-operation".into(),
            environment: pending,
            steps: BTreeMap::new(),
            last_diagnostic: None,
        })
        .unwrap();
    let projection = setup_snapshot_in(&fixture.store, Some(ID))
        .unwrap()
        .unwrap();
    assert_eq!(projection.mode, "join");
    assert!(projection.runner);
    assert!(!projection.configured);
    assert_eq!(projection.service_scope, ServiceScope::User);
    assert_eq!(
        projection.project_path,
        Some(
            fixture
                .store
                .root()
                .join("project")
                .to_string_lossy()
                .into_owned()
        )
    );
    let serialized = serde_json::to_value(&projection).unwrap();
    assert_eq!(serialized.as_object().unwrap().len(), 7);
    let text = serialized.to_string();
    for excluded in [
        "wc_boot_test_fixture",
        "wc_pat_test_fixture",
        "fixture-user-identity",
        "fixture-runner-identity",
        "pending-operation",
        CODE,
    ] {
        assert!(!text.contains(excluded));
    }
    let mut journal = fixture.store.load_journal().unwrap().unwrap();
    journal.environment.configured = true;
    fixture.store.save_journal(&journal).unwrap();
    assert_eq!(
        setup_snapshot_in(&fixture.store, Some(ID))
            .unwrap()
            .unwrap()
            .mode,
        "create"
    );
}
