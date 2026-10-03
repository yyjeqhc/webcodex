use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn addition_store(url: &str) -> (tempfile::TempDir, EnvironmentStore, PathBuf) {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let project = temp.path().join("new-project");
    let authorized = temp.path().join("authorized");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&authorized).unwrap();
    let project = project.canonicalize().unwrap();
    let authorized = authorized.canonicalize().unwrap();
    atomic_private_write(&store.root().join("webcodex-user-token"), b"wc_pat_alice").unwrap();
    let mut environment = record(url.into(), Some(project.clone()), EnvironmentMode::Join);
    environment.request.account.identity = "fixture-user-id".into();
    environment.username = Some("alice".into());
    environment.runner_client_id = Some("alice-client".into());
    store.save_environment(&environment).unwrap();
    let config = format!(
        "server_url = {url:?}\nclient_id = \"alice-client\"\n[policy]\nallowed_roots = [{:?}]\n",
        authorized.to_string_lossy()
    );
    atomic_private_write(&store.root().join("runner.toml"), config.as_bytes()).unwrap();
    (temp, store, project)
}

#[tokio::test]
async fn failed_config_check_does_not_turn_saved_roots_into_live_authority() {
    let (url, requests, server) = fixture(4, |request| match request.path.as_str() {
        "/api/runtime-console/projects" => (200, vec![], json!({"projects":[]})),
        "/api/tools/call" => (
            200,
            vec![],
            json!({"success":false,"output":{"execution_state":"not_started"}}),
        ),
        "/api/projects/resolve-or-register" => (
            200,
            vec![],
            json!({"success":false,"output":{"execution_state":"not_started"}}),
        ),
        other => panic!("unexpected addition route: {other}"),
    });
    let (_temp, store, project) = addition_store(&url);
    let mut backend = NativeEnvironment::new().unwrap();
    for _ in 0..2 {
        assert_eq!(
            backend
                .add_project(&store, &project)
                .await
                .unwrap_err()
                .code,
            "runner_operation_rejected"
        );
    }
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.body["tool"] == "runner_config_check")
            .count(),
        2,
        "retry must reconcile the unapplied configuration, not trust the edited disk file"
    );
    assert!(
        !requests
            .iter()
            .any(|request| request.path == "/api/projects/resolve-or-register"),
        "registration must not start while the required config check still fails"
    );
}

#[tokio::test]
async fn failed_check_can_resume_one_confirmed_reload_before_registration() {
    let checks = AtomicUsize::new(0);
    let (url, requests, server) = fixture(6, move |request| match request.path.as_str() {
        "/api/runtime-console/projects" => (200, vec![], json!({"projects":[]})),
        "/api/tools/call" if request.body["tool"] == "runner_config_check" => {
            if checks.fetch_add(1, Ordering::Relaxed) == 0 {
                (
                    200,
                    vec![],
                    json!({"success":false,"output":{"execution_state":"not_started"}}),
                )
            } else {
                (
                    200,
                    vec![],
                    json!({"success":true,"output":{"valid":true,"current_generation":7,"restart_required":false}}),
                )
            }
        }
        "/api/tools/call" if request.body["tool"] == "runner_config_reload" => {
            assert_eq!(request.body["params"]["expected_generation"], 7);
            (
                200,
                vec![],
                json!({"success":true,"output":{"valid":true,"current_generation":8,"restart_required":false}}),
            )
        }
        "/api/projects/resolve-or-register" => (
            200,
            vec![],
            json!({"success":false,"output":{"execution_state":"not_started"}}),
        ),
        other => panic!("unexpected addition route: {other}"),
    });
    let (_temp, store, project) = addition_store(&url);
    let mut backend = NativeEnvironment::new().unwrap();
    for _ in 0..2 {
        assert_eq!(
            backend
                .add_project(&store, &project)
                .await
                .unwrap_err()
                .code,
            "runner_operation_rejected"
        );
    }
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.body["tool"] == "runner_config_reload")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/api/projects/resolve-or-register")
            .count(),
        1
    );
    let pending: ProjectAddition = store.read_json("add-project.json").unwrap().unwrap();
    assert!(pending.config_change.is_none());
    assert!(!pending.dispatched);
}

#[tokio::test]
async fn lost_reload_is_reconciled_without_replaying_it() {
    let checks = AtomicUsize::new(0);
    let (url, requests, server) = fixture(6, move |request| match request.path.as_str() {
        "/api/runtime-console/projects" => (200, vec![], json!({"projects":[]})),
        "/api/tools/call" if request.body["tool"] == "runner_config_check" => {
            let generation = 7 + checks.fetch_add(1, Ordering::Relaxed);
            (
                200,
                vec![],
                json!({"success":true,"output":{"valid":true,"current_generation":generation,"restart_required":false}}),
            )
        }
        "/api/tools/call" if request.body["tool"] == "runner_config_reload" => {
            (503, vec![], json!({"error":"lost reply"}))
        }
        "/api/projects/resolve-or-register" => (
            200,
            vec![],
            json!({"success":false,"output":{"execution_state":"not_started"}}),
        ),
        other => panic!("unexpected addition route: {other}"),
    });
    let (_temp, store, project) = addition_store(&url);
    let mut backend = NativeEnvironment::new().unwrap();
    assert!(backend.add_project(&store, &project).await.is_err());
    assert_eq!(
        backend
            .add_project(&store, &project)
            .await
            .unwrap_err()
            .code,
        "runner_operation_rejected"
    );
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.body["tool"] == "runner_config_reload")
            .count(),
        1
    );
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.path == "/api/projects/resolve-or-register")
            .count(),
        1
    );
    let pending: ProjectAddition = store.read_json("add-project.json").unwrap().unwrap();
    assert!(pending.config_change.is_none());
}

#[tokio::test]
async fn unknown_reload_with_unchanged_generation_does_not_register_or_retry() {
    let (url, requests, server) = fixture(5, |request| match request.path.as_str() {
        "/api/runtime-console/projects" => (200, vec![], json!({"projects":[]})),
        "/api/tools/call" if request.body["tool"] == "runner_config_check" => (
            200,
            vec![],
            json!({"success":true,"output":{"valid":true,"current_generation":7,"restart_required":false}}),
        ),
        "/api/tools/call" if request.body["tool"] == "runner_config_reload" => {
            (503, vec![], json!({"error":"unknown"}))
        }
        other => panic!("unexpected addition route: {other}"),
    });
    let (_temp, store, project) = addition_store(&url);
    let mut backend = NativeEnvironment::new().unwrap();
    assert!(backend.add_project(&store, &project).await.is_err());
    assert_eq!(
        backend
            .add_project(&store, &project)
            .await
            .unwrap_err()
            .code,
        "config_reconcile_required"
    );
    server.join().unwrap();
    let requests = requests.lock().unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.body["tool"] == "runner_config_reload")
            .count(),
        1
    );
    assert!(!requests
        .iter()
        .any(|request| request.path == "/api/projects/resolve-or-register"));
}

#[tokio::test]
async fn changed_candidate_is_not_reloaded_or_overwritten_on_retry() {
    let (url, requests, server) = fixture(3, |request| match request.path.as_str() {
        "/api/runtime-console/projects" => (200, vec![], json!({"projects":[]})),
        "/api/tools/call" => (
            200,
            vec![],
            json!({"success":false,"output":{"execution_state":"not_started"}}),
        ),
        other => panic!("unexpected addition route: {other}"),
    });
    let (_temp, store, project) = addition_store(&url);
    let mut backend = NativeEnvironment::new().unwrap();
    assert!(backend.add_project(&store, &project).await.is_err());
    let config_path = store.root().join("runner.toml");
    let edited = format!(
        "{}\n# independent operator edit\n",
        read_secret(&config_path).unwrap().expose()
    );
    atomic_private_write(&config_path, edited.as_bytes()).unwrap();
    assert_eq!(
        backend
            .add_project(&store, &project)
            .await
            .unwrap_err()
            .code,
        "config_concurrent_change"
    );
    server.join().unwrap();
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), edited);
    assert!(!requests
        .lock()
        .unwrap()
        .iter()
        .any(|request| request.body["tool"] == "runner_config_reload"));
}
