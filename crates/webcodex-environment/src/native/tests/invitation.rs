use super::*;

const ID: &str = "fixture-environment";
const CODE: &str = "wc_pair_invitation_fixture";

fn saved(store: &EnvironmentStore, url: String, local: bool) {
    let mut saved = record(
        url,
        None,
        if local {
            EnvironmentMode::Create {
                listen: "127.0.0.1:8787".into(),
            }
        } else {
            EnvironmentMode::Join
        },
    );
    saved.username = Some("fixture-user".into());
    saved.configured = true;
    store.save_environment(&saved).unwrap();
}

#[tokio::test]
async fn stale_target_fails_before_server_credentials_or_http() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    saved(
        &store,
        format!("http://{}", listener.local_addr().unwrap()),
        true,
    );
    // No Server credential file exists: a stale target must be diagnosed before
    // credential loading, rather than fail with an I/O/credential error.
    let before = std::fs::read(store.root().join("environment.json")).unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        NativeEnvironment::new()
            .unwrap()
            .invite_for_environment(&store, "stale-environment"),
    )
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, "environment_changed");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        std::fs::read(store.root().join("environment.json")).unwrap(),
        before
    );
    assert!(!store.root().join("server").exists());
    assert!(!store.root().join("setup.json").exists());
    // The failure releases the existing setup fence.
    drop(store.lock().unwrap());
}

#[tokio::test]
async fn matching_target_and_cli_invite_share_authorized_ten_minute_issuance_under_lock() {
    for target_bound in [false, true] {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let observed_store = store.clone();
        let (url, requests, server) = fixture(1, move |request| {
            // Observe the lock from the HTTP side while the call is outstanding.
            match observed_store.lock() {
                Err(error) => assert_eq!(error.code, "setup_busy"),
                Ok(_) => panic!("invitation released its setup lock during the request"),
            }
            assert_eq!(request.method, "POST");
            assert_eq!(request.path, "/api/pairing/create");
            assert_eq!(
                request.authorization.as_deref(),
                Some("Bearer wc_boot_invitation_fixture")
            );
            assert_eq!(
                request.body,
                json!({"username": "fixture-user", "ttl_secs": 600})
            );
            (200, vec![], json!({"pairing_code": CODE}))
        });
        saved(&store, url, true);
        ensure_private_directory(&store.root().join("server")).unwrap();
        atomic_private_write(
            &store.root().join("server/webcodex.env"),
            b"WEBCODEX_TOKEN=wc_boot_invitation_fixture\n",
        )
        .unwrap();
        let before = std::fs::read(store.root().join("environment.json")).unwrap();
        let native = NativeEnvironment::new().unwrap();
        let code = if target_bound {
            native.invite_for_environment(&store, ID).await
        } else {
            native.invite(&store).await
        }
        .unwrap();
        assert_eq!(code.expose(), CODE);
        server.join().unwrap();
        assert_eq!(requests.lock().unwrap().len(), 1);
        assert_eq!(
            std::fs::read(store.root().join("environment.json")).unwrap(),
            before
        );
        assert!(!store.root().join("setup.json").exists());
        drop(store.lock().unwrap());
    }
}

#[tokio::test]
async fn target_bound_invitation_keeps_existing_busy_and_local_server_authority_checks() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    saved(&store, "http://127.0.0.1:8787".into(), false);
    let native = NativeEnvironment::new().unwrap();
    let lock = store.lock().unwrap();
    assert_eq!(
        native
            .invite_for_environment(&store, ID)
            .await
            .unwrap_err()
            .code,
        "setup_busy"
    );
    drop(lock);
    assert_eq!(
        native
            .invite_for_environment(&store, ID)
            .await
            .unwrap_err()
            .code,
        "server_admin_required"
    );
    assert_eq!(
        native.invite(&store).await.unwrap_err().code,
        "server_admin_required"
    );
    assert!(!store.root().join("server").exists());
}

#[tokio::test]
async fn matching_target_keeps_server_authorization_failure_without_state_changes() {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let (url, requests, server) = fixture(1, |_| (403, vec![], json!({"pairing_code": CODE})));
    saved(&store, url, true);
    ensure_private_directory(&store.root().join("server")).unwrap();
    atomic_private_write(
        &store.root().join("server/webcodex.env"),
        b"WEBCODEX_TOKEN=wc_boot_invitation_fixture\n",
    )
    .unwrap();
    let before = std::fs::read(store.root().join("environment.json")).unwrap();
    let error = NativeEnvironment::new()
        .unwrap()
        .invite_for_environment(&store, ID)
        .await
        .unwrap_err();
    assert_eq!(error.code, "permission_denied");
    assert!(!format!("{error:?}").contains(CODE));
    server.join().unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(store.root().join("environment.json")).unwrap(),
        before
    );
    assert!(!store.root().join("setup.json").exists());
    drop(store.lock().unwrap());
}
