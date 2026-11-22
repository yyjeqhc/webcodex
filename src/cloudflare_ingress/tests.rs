use super::*;
use crate::models::{
    OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthRefreshTokenRecord,
};
use crate::test_support::{seed_user, test_config_oauth2, test_db};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn private_file(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    for parent in path
        .ancestors()
        .skip(1)
        .take_while(|path| path.starts_with(std::env::temp_dir()) && *path != std::env::temp_dir())
    {
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::write(path, bytes).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

struct Fixture {
    _dir: tempfile::TempDir,
    _db_dir: tempfile::TempDir,
    control: Arc<CloudflareControl>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        // macOS temp_dir() may be reached through /var -> /private/var.
        // This fixture must use a physical path; production rejects symlink ancestors.
        let physical_temp_root = dir.path().canonicalize().unwrap();
        let root = physical_temp_root.join("environment");
        let store = EnvironmentStore::open(root.clone()).unwrap();
        let environment = json!({"schema_version":1,"environment_id":"fixture","username":"owner","runner_client_id":"runner","projects":[],"configured":true,
            "request":{"service_scope":"user","mode":{"kind":"user_create","listen":"127.0.0.1:8888"},"server_url":"http://127.0.0.1:8888","project":null,"runner":true,
            "account":{"name":"fixture","identity":"1000","home":physical_temp_root},"binaries":{"cli":"/fixture/cli","server":"/fixture/server","runner":"/fixture/runner"}}});
        store
            .save_environment(&serde_json::from_value(environment).unwrap())
            .unwrap();
        private_file(
            &root.join("server/webcodex.env"),
            b"WEBCODEX_ADDR=127.0.0.1:8888\nWEBCODEX_TOKEN=private-bootstrap\n",
        );
        private_file(
            &root.join("server/cloudflare-ingress.json"),
            br#"{"port":8889}"#,
        );
        private_file(&root.join("tunnel.json"), br#"[{"profile_id":"quick","configuration_id":"00000000-0000-4000-8000-000000000001","provider":{"kind":"cloudflare_quick"},"name":"Quick","host_mode":"embedded","autostart":false,"revision":1,"runtime_revision":1,"installed":false,"started":false}]"#);
        private_file(
            &root.join("server/tunnels/quick/webcodex.env"),
            b"WEBCODEX_TUNNEL_PROFILE_ID=quick\nWEBCODEX_TUNNEL_PROVIDER=cloudflare_quick\n",
        );
        private_file(&root.join("server/tunnels/quick/readiness.json"), b"{}");
        let (_db_dir, db) = test_db();
        seed_user(&db, "owner");
        Self {
            _dir: dir,
            _db_dir,
            control: Arc::new(CloudflareControl {
                root,
                ingress_port: 8889,
                db,
                registry: Arc::new(crate::RunnerRegistry::default()),
                config: test_config_oauth2(Some("private-bootstrap")),
                instance_id: "server-one".into(),
                generation: std::sync::atomic::AtomicI64::new(0),
                attempt: Mutex::new(None),
            }),
        }
    }
}

fn activate_quick_fixture(
    control: &CloudflareControl,
    origin: &str,
) -> (PreparedIngress, PublicIngressEntry) {
    let standalone = control.profile("quick").unwrap().host_mode == TunnelHostMode::Standalone;
    let prepared = control.prepare("quick", 1, standalone, None, None).unwrap();
    let mut guard = control.attempt.lock().unwrap();
    let attempt = guard.as_mut().unwrap();
    let entry = control
        .db
        .finalize_public_ingress_origin(&attempt.entry.as_ref().unwrap().fence(), origin)
        .unwrap()
        .unwrap();
    assert!(control
        .db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap());
    let entry = control
        .db
        .get_public_ingress_entry(&entry.entry_id)
        .unwrap()
        .unwrap();
    attempt.entry = Some(entry.clone());
    attempt.lifecycle = "running";
    attempt.network_ready = true;
    (prepared, entry)
}

fn issue_access_grant_fixture(
    control: &CloudflareControl,
    entry: &PublicIngressEntry,
    client_id: &str,
) -> String {
    let now = chrono::Utc::now().timestamp();
    let code = OAuthAuthorizationCodeRecord {
        id: "current-code".into(),
        code_hash: "current-code-hash".into(),
        client_id: client_id.into(),
        subject_kind: "managed_user".into(),
        subject_id: entry.owner_user_id.clone(),
        user_id: Some(entry.owner_user_id.clone()),
        redirect_uri: "https://current.example/callback".into(),
        scopes: "runtime:read".into(),
        code_challenge: None,
        code_challenge_method: None,
        resource: None,
        shared_key_hash: None,
        created_at: now,
        expires_at: now + 600,
        used_at: None,
        revoked_at: None,
        admin_authority: false,
    };
    assert!(control
        .db
        .insert_public_ingress_oauth_authorization_code(&code, &code.code_hash, &entry.fence())
        .unwrap());
    let access = OAuthAccessTokenRecord {
        id: "current-access".into(),
        token_hash: "current-access-hash".into(),
        client_id: client_id.into(),
        subject_kind: code.subject_kind.clone(),
        subject_id: code.subject_id.clone(),
        user_id: code.user_id.clone(),
        scopes: code.scopes.clone(),
        resource: None,
        shared_key_hash: None,
        created_at: now,
        expires_at: now + 3600,
        revoked_at: None,
        last_used_at: None,
        admin_authority: false,
    };
    let refresh = OAuthRefreshTokenRecord {
        id: "current-refresh".into(),
        token_hash: "current-refresh-hash".into(),
        client_id: client_id.into(),
        subject_kind: code.subject_kind.clone(),
        subject_id: code.subject_id.clone(),
        user_id: code.user_id.clone(),
        scopes: code.scopes.clone(),
        resource: None,
        shared_key_hash: None,
        created_at: now,
        expires_at: now + 7200,
        revoked_at: None,
        last_used_at: None,
        rotated_from_id: None,
        admin_authority: false,
    };
    assert!(control
        .db
        .exchange_oauth_authorization_code_for_tokens(
            Some(&entry.fence()),
            &code.code_hash,
            now,
            &access,
            &refresh
        )
        .unwrap()
        .is_some());
    access.token_hash
}

#[test]
fn metadata_only_edit_keeps_the_verified_attempt_but_runtime_change_hides_it() {
    let control = Fixture::new();
    let control = &control.control;
    activate_quick_fixture(control, "https://fixture.trycloudflare.com");
    let catalog = control.root.join("tunnel.json");
    let mut profiles: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&catalog).unwrap()).unwrap();
    profiles[0]["name"] = json!("Renamed");
    profiles[0]["revision"] = json!(2);
    private_file(&catalog, &serde_json::to_vec(&profiles).unwrap());
    let renamed = control.status("quick").unwrap();
    assert_eq!(renamed.configured_revision, 2);
    assert_eq!(renamed.applied_revision, Some(2));
    assert!(renamed.public_origin.is_some());
    profiles[0]["runtime_revision"] = json!(2);
    private_file(&catalog, &serde_json::to_vec(&profiles).unwrap());
    let changed = control.status("quick").unwrap();
    assert!(changed.public_origin.is_none());
    assert_eq!(
        changed.reason_code.as_deref(),
        Some("cloudflare_revision_conflict")
    );
}

#[test]
fn prepare_stop_restart_fences_profile_revision_instance_and_generation() {
    let fixture = Fixture::new();
    let control = fixture.control;
    assert!(matches!(
        control.prepare("quick", 2, false, None, None),
        Err("cloudflare_revision_conflict")
    ));
    assert!(matches!(
        control.prepare("quick", 1, true, None, None),
        Err("cloudflare_owner_conflict")
    ));
    let first = control.prepare("quick", 1, false, None, None).unwrap();
    assert!(matches!(
        control.prepare("quick", 1, false, None, None),
        Err("cloudflare_already_active")
    ));
    assert!(control
        .stop("quick", "server-other", first.process_generation, None)
        .is_err());
    assert!(control
        .stop("quick", "server-one", first.process_generation + 1, None)
        .is_err());
    control
        .stop("quick", "server-one", first.process_generation, None)
        .unwrap();
    assert!(matches!(
        control.prepare("quick", 1, false, Some(0), None),
        Err("cloudflare_stale_attempt")
    ));
    assert!(matches!(
        control.prepare(
            "quick",
            1,
            false,
            Some(first.process_generation),
            Some("replaced")
        ),
        Err("cloudflare_revision_conflict")
    ));
    let second = control.prepare("quick", 1, false, None, None).unwrap();
    assert!(second.process_generation > first.process_generation);
    assert_ne!(second.probe_nonce, first.probe_nonce);
    assert!(control
        .stop("quick", "server-one", first.process_generation, None)
        .is_err());
    assert_eq!(control.status("quick").unwrap().lifecycle, "starting");
    control
        .stop("quick", "server-one", second.process_generation, None)
        .unwrap();
    assert!(control.status("quick").unwrap().public_origin.is_none());
}

#[test]
fn oauth_handoff_is_once_and_replacement_revokes_previous_client() {
    let fixture = Fixture::new();
    let control = fixture.control;
    let prepared = control.prepare("quick", 1, false, None, None).unwrap();
    let entry = control
        .db
        .get_public_ingress_entry_for_profile("quick")
        .unwrap()
        .unwrap();
    let entry = control
        .db
        .finalize_public_ingress_origin(&entry.fence(), "https://fixture.trycloudflare.com")
        .unwrap()
        .unwrap();
    assert!(control
        .db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap());
    assert!(!control.status("quick").unwrap().oauth_configured);
    let first = control
        .provision_oauth(
            "quick",
            prepared.process_generation,
            "https://client.example/callback",
            vec![],
            false,
        )
        .unwrap();
    assert!(first["client_secret"].as_str().is_some());
    assert!(control.status("quick").unwrap().oauth_configured);
    let repeated = control
        .provision_oauth(
            "quick",
            prepared.process_generation,
            "https://client.example/callback",
            vec![],
            false,
        )
        .unwrap();
    assert!(repeated["client_secret"].is_null());
    assert_eq!(repeated["client_id"], first["client_id"]);
    assert!(control
        .provision_oauth(
            "quick",
            prepared.process_generation,
            "https://other.example/callback",
            vec![],
            false
        )
        .is_err());
    let replacement = control
        .provision_oauth(
            "quick",
            prepared.process_generation,
            "https://other.example/callback",
            vec![],
            true,
        )
        .unwrap();
    assert_ne!(replacement["client_id"], first["client_id"]);
    assert!(control.status("quick").unwrap().oauth_configured);
    assert_eq!(
        control
            .db
            .list_manual_oauth_clients_for_public_ingress(&entry.entry_id)
            .unwrap()
            .len(),
        1
    );
    assert!(control
        .db
        .get_oauth_client_by_client_id(first["client_id"].as_str().unwrap())
        .unwrap()
        .is_none());
    control
        .db
        .revoke_oauth_client_by_client_id(
            replacement["client_id"].as_str().unwrap(),
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
    assert!(!control.status("quick").unwrap().oauth_configured);
    assert!(control
        .provision_oauth(
            "quick",
            prepared.process_generation,
            "https://other.example/callback",
            vec!["admin".into()],
            true
        )
        .is_err());
}

async fn assert_stale_oauth_configuration_preserves_current_grants(replace_profile: bool) {
    use salvo::test::{ResponseExt, TestClient};

    let fixture = Fixture::new();
    let control = fixture.control;
    let profiles_path = control.root.join("tunnel.json");
    let mut profiles: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&profiles_path).unwrap()).unwrap();
    profiles[0]["host_mode"] = json!("standalone");
    profiles[0]["autostart"] = json!(true);
    private_file(&profiles_path, &serde_json::to_vec(&profiles).unwrap());

    let (first, first_entry) = activate_quick_fixture(&control, "https://first.trycloudflare.com");
    let stale_request = json!({
        "action": "configure_oauth",
        "profile_id": first.profile_id,
        "server_instance_id": first.server_instance_id,
        "process_generation": first.process_generation,
        "redirect_uri": "https://stale.example/callback",
        "replace": true
    });
    let entry = if replace_profile {
        first_entry
    } else {
        control
            .stop(
                "quick",
                &control.instance_id,
                first.process_generation,
                None,
            )
            .unwrap();
        activate_quick_fixture(&control, "https://second.trycloudflare.com").1
    };
    let current = control
        .provision_oauth(
            "quick",
            entry.process_generation,
            "https://current.example/callback",
            vec![],
            false,
        )
        .unwrap();
    let current_id = current["client_id"].as_str().unwrap();
    let access_hash = issue_access_grant_fixture(&control, &entry, current_id);
    assert!(control
        .db
        .oauth_access_token_matches_public_ingress(&access_hash, Some(&entry.fence()))
        .unwrap());

    if replace_profile {
        // Standalone removal can finish before its old Server lease expires.
        profiles[0]["configuration_id"] = json!("00000000-0000-4000-8000-000000000002");
        profiles[0]["revision"] = json!(2);
        profiles[0]["runtime_revision"] = json!(2);
        private_file(&profiles_path, &serde_json::to_vec(&profiles).unwrap());
    }

    let service = Service::new(
        Router::with_path("api/connections/cloudflare")
            .hoop(affix_state::inject(Some(control.clone())))
            .hoop(affix_state::inject(control.config.clone()))
            .hoop(affix_state::inject(control.db.clone()))
            .hoop(crate::AuthMiddleware)
            .post(cloudflare_control_handler),
    );
    let mut response = TestClient::post("http://127.0.0.1:8888/api/connections/cloudflare")
        .bearer_auth("private-bootstrap")
        .json(&stale_request)
        .send(&service)
        .await;

    let client_retained = control
        .db
        .get_oauth_client_by_client_id(current_id)
        .unwrap()
        .is_some();
    let grant_retained = control
        .db
        .oauth_access_token_matches_public_ingress(&access_hash, Some(&entry.fence()))
        .unwrap();
    assert_eq!(
        (client_retained, grant_retained),
        (true, true),
        "stale replacement must not revoke the current target's client or grants"
    );
    assert_eq!(response.status_code, Some(StatusCode::CONFLICT));
    assert_eq!(
        response.take_json::<serde_json::Value>().await.unwrap()["error"],
        "cloudflare_stale_attempt"
    );
    assert_eq!(
        control
            .db
            .get_public_ingress_entry(&entry.entry_id)
            .unwrap(),
        Some(entry)
    );
}

#[tokio::test]
async fn oauth_configuration_rejects_stale_generation_without_revoking_current_grants() {
    assert_stale_oauth_configuration_preserves_current_grants(false).await;
}

#[tokio::test]
async fn oauth_configuration_rejects_recreated_profile_before_old_lease_expires() {
    assert_stale_oauth_configuration_preserves_current_grants(true).await;
}

async fn probe_response(body: Vec<u8>, prepared: &PreparedIngress) -> Result<bool, &'static str> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0u8; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).await.unwrap();
        let _ = stream.write_all(&body).await;
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let result = forwarding_matches(&client, &format!("http://{addr}"), prepared).await;
    task.await.unwrap();
    result
}

#[tokio::test]
async fn probe_requires_exact_server_profile_generation_and_nonce_and_bounds_body() {
    let prepared = PreparedIngress {
        profile_id: "quick".into(),
        server_instance_id: "server".into(),
        process_generation: 3,
        local_target: String::new(),
        probe_nonce: "random-nonce".into(),
    };
    assert_eq!(
        probe_response(serde_json::to_vec(&prepared).unwrap(), &prepared).await,
        Ok(true)
    );
    for field in [
        "profile_id",
        "server_instance_id",
        "process_generation",
        "probe_nonce",
    ] {
        let mut value = serde_json::to_value(&prepared).unwrap();
        value[field] = if field == "process_generation" {
            json!(2)
        } else {
            json!("stale")
        };
        assert_eq!(
            probe_response(serde_json::to_vec(&value).unwrap(), &prepared).await,
            Err("cloudflare_forwarding_mismatch")
        );
    }
    assert_eq!(
        probe_response(vec![b'x'; 4097], &prepared).await,
        Err("cloudflare_forwarding_mismatch")
    );
}

#[tokio::test(start_paused = true)]
async fn embedded_quick_forwarding_waits_for_resolution_without_extending_other_modes() {
    let named = TunnelProvider::CloudflareNamed {
        public_origin: "https://named.example".into(),
        tunnel_id: "fixture".into(),
    };
    for (provider, host_mode, expected, elapsed) in [
        (
            TunnelProvider::CloudflareQuick,
            TunnelHostMode::Embedded,
            Ok(()),
            75,
        ),
        (
            named.clone(),
            TunnelHostMode::Embedded,
            Err("cloudflare_forwarding_timeout"),
            60,
        ),
        (
            TunnelProvider::CloudflareQuick,
            TunnelHostMode::Standalone,
            Err("cloudflare_forwarding_timeout"),
            60,
        ),
        (
            named,
            TunnelHostMode::Standalone,
            Err("cloudflare_forwarding_timeout"),
            60,
        ),
    ] {
        let started = tokio::time::Instant::now();
        let ready_at = started + Duration::from_secs(75);
        let result = wait_for_forwarding(forwarding_timeout(&provider, &host_mode), || {
            std::future::ready(if tokio::time::Instant::now() >= ready_at {
                Ok(true)
            } else {
                Err("cloudflare_probe_unavailable")
            })
        })
        .await;
        assert_eq!(result, expected);
        assert_eq!(started.elapsed(), Duration::from_secs(elapsed));
    }
}

#[tokio::test(start_paused = true)]
async fn quick_forwarding_deadline_cancels_a_pending_probe() {
    struct ProbeDrop<'a>(&'a std::cell::Cell<bool>);
    impl Drop for ProbeDrop<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = std::cell::Cell::new(false);
    let started = tokio::time::Instant::now();
    assert_eq!(
        wait_for_forwarding(
            forwarding_timeout(&TunnelProvider::CloudflareQuick, &TunnelHostMode::Embedded),
            || async {
                let _drop = ProbeDrop(&dropped);
                std::future::pending::<Result<bool, &'static str>>().await
            }
        )
        .await,
        Err("cloudflare_forwarding_timeout")
    );
    assert_eq!(started.elapsed(), Duration::from_secs(120));
    assert!(dropped.get());
}

#[tokio::test(start_paused = true)]
async fn quick_forwarding_never_retries_an_identity_mismatch() {
    let started = tokio::time::Instant::now();
    let mut calls = 0;
    assert_eq!(
        wait_for_forwarding(
            forwarding_timeout(&TunnelProvider::CloudflareQuick, &TunnelHostMode::Embedded),
            || {
                calls += 1;
                std::future::ready(Err("cloudflare_forwarding_mismatch"))
            }
        )
        .await,
        Err("cloudflare_forwarding_mismatch")
    );
    assert_eq!(calls, 1);
    assert_eq!(started.elapsed(), Duration::ZERO);
}

// These fixtures enter the final commit stage after the forwarding/Runner proofs.
// They exercise the real setup.lock and admission fences, not a simulated TLS probe.
async fn assert_admission_waiting<F: std::future::Future>(mut future: std::pin::Pin<&mut F>) {
    std::future::poll_fn(|context| {
        assert!(future.as_mut().poll(context).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
}

fn assert_admission_closed(control: &CloudflareControl) {
    let entry = control
        .db
        .get_public_ingress_entry_for_profile("quick")
        .unwrap()
        .unwrap();
    assert!(!entry.admission_open);
    let status = control.status("quick").unwrap();
    assert!(status.public_origin.is_none());
    assert!(!status.registration_available);
    assert!(
        !control
            .attempt
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .network_ready
    );
}

#[tokio::test(start_paused = true)]
async fn embedded_admission_waits_for_a_short_configuration_lock() {
    let fixture = Fixture::new();
    let control = &fixture.control;
    let prepared = control.prepare("quick", 1, false, None, None).unwrap();
    let profile = control.profile("quick").unwrap();
    let store = EnvironmentStore::open(control.root.clone()).unwrap();
    let lock = store.lock().unwrap();
    let started = tokio::time::Instant::now();
    let mut admission = Box::pin(control.admit_forwarded_ingress(
        &prepared,
        1,
        "https://fixture.trycloudflare.com",
        &profile,
    ));
    assert_admission_waiting(admission.as_mut()).await;
    assert_admission_closed(control);
    tokio::time::advance(Duration::from_secs(2)).await;
    drop(lock);
    let entry = admission.await.unwrap();
    assert_eq!(started.elapsed(), Duration::from_secs(2));
    assert!(entry.active && entry.admission_open);
    assert_eq!(entry.process_generation, prepared.process_generation);
    assert_eq!(control.status("quick").unwrap().lifecycle, "running");
    assert_eq!(
        control
            .db
            .get_public_ingress_entry(&entry.entry_id)
            .unwrap(),
        Some(entry)
    );
    assert!(
        store.lock().is_ok(),
        "the final commit must release its lock"
    );
}

#[tokio::test(start_paused = true)]
async fn configuration_lock_deadline_keeps_admission_closed_and_standalone_immediate() {
    for (mode, standalone, expected_elapsed) in [
        ("embedded", false, Duration::from_secs(5)),
        ("standalone", true, Duration::ZERO),
    ] {
        let fixture = Fixture::new();
        let control = &fixture.control;
        let catalog = control.root.join("tunnel.json");
        let mut profiles: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&catalog).unwrap()).unwrap();
        profiles[0]["host_mode"] = json!(mode);
        private_file(&catalog, &serde_json::to_vec(&profiles).unwrap());
        let prepared = control.prepare("quick", 1, standalone, None, None).unwrap();
        let profile = control.profile("quick").unwrap();
        let store = EnvironmentStore::open(control.root.clone()).unwrap();
        let lock = store.lock().unwrap();
        let started = tokio::time::Instant::now();
        let result = control
            .admit_forwarded_ingress(&prepared, 1, "https://fixture.trycloudflare.com", &profile)
            .await;
        assert!(matches!(result, Err("cloudflare_configuration_busy")));
        assert_eq!(started.elapsed(), expected_elapsed, "{mode}");
        assert_admission_closed(control);
        drop(lock);
        assert!(store.lock().is_ok());
    }
}

#[tokio::test(start_paused = true)]
async fn configuration_lock_storage_error_is_immediate_and_not_busy() {
    let fixture = Fixture::new();
    let control = &fixture.control;
    let prepared = control.prepare("quick", 1, false, None, None).unwrap();
    let profile = control.profile("quick").unwrap();
    std::fs::create_dir(control.root.join("setup.lock")).unwrap();
    let started = tokio::time::Instant::now();
    let result = control
        .admit_forwarded_ingress(&prepared, 1, "https://fixture.trycloudflare.com", &profile)
        .await;
    assert!(matches!(result, Err("environment_unavailable")));
    assert_eq!(started.elapsed(), Duration::ZERO);
    assert_admission_closed(control);
}

#[tokio::test(start_paused = true)]
async fn admission_revalidates_profile_after_configuration_lock_wait() {
    for (field, value) in [
        ("revision", json!(2)),
        (
            "configuration_id",
            json!("00000000-0000-4000-8000-000000000002"),
        ),
    ] {
        let fixture = Fixture::new();
        let control = &fixture.control;
        let prepared = control.prepare("quick", 1, false, None, None).unwrap();
        let profile = control.profile("quick").unwrap();
        let store = EnvironmentStore::open(control.root.clone()).unwrap();
        let lock = store.lock().unwrap();
        let mut admission = Box::pin(control.admit_forwarded_ingress(
            &prepared,
            1,
            "https://fixture.trycloudflare.com",
            &profile,
        ));
        assert_admission_waiting(admission.as_mut()).await;
        let catalog = control.root.join("tunnel.json");
        let mut profiles: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&catalog).unwrap()).unwrap();
        profiles[0][field] = value;
        private_file(&catalog, &serde_json::to_vec(&profiles).unwrap());
        drop(lock);
        assert!(matches!(
            admission.await,
            Err("cloudflare_revision_conflict")
        ));
        assert_admission_closed(control);
    }
}

#[tokio::test(start_paused = true)]
async fn admission_wait_rejects_a_replaced_attempt() {
    let fixture = Fixture::new();
    let control = &fixture.control;
    let prepared = control.prepare("quick", 1, false, None, None).unwrap();
    let profile = control.profile("quick").unwrap();
    let store = EnvironmentStore::open(control.root.clone()).unwrap();
    let lock = store.lock().unwrap();
    let mut admission = Box::pin(control.admit_forwarded_ingress(
        &prepared,
        1,
        "https://fixture.trycloudflare.com",
        &profile,
    ));
    assert_admission_waiting(admission.as_mut()).await;
    control
        .stop("quick", "server-one", prepared.process_generation, None)
        .unwrap();
    let replacement = control.prepare("quick", 1, false, None, None).unwrap();
    assert!(replacement.process_generation > prepared.process_generation);
    drop(lock);
    assert!(matches!(admission.await, Err("cloudflare_stale_attempt")));
    assert_admission_closed(control);
    assert_eq!(
        control.status("quick").unwrap().process_generation,
        replacement.process_generation
    );
}

#[tokio::test(start_paused = true)]
async fn cancelled_admission_wait_never_commits_in_the_background() {
    let fixture = Fixture::new();
    let control = &fixture.control;
    let prepared = control.prepare("quick", 1, false, None, None).unwrap();
    let profile = control.profile("quick").unwrap();
    let store = EnvironmentStore::open(control.root.clone()).unwrap();
    let lock = store.lock().unwrap();
    let mut admission = Box::pin(control.admit_forwarded_ingress(
        &prepared,
        1,
        "https://fixture.trycloudflare.com",
        &profile,
    ));
    assert_admission_waiting(admission.as_mut()).await;
    drop(admission);
    drop(lock);
    tokio::time::advance(Duration::from_secs(6)).await;
    assert_admission_closed(control);
    assert!(store.lock().is_ok());
}

#[tokio::test]
async fn missing_runner_identity_requires_explicit_repair() {
    let fixture = Fixture::new();
    let mut profile = fixture.control.profile("quick").unwrap();
    profile.runner_client_id = None;
    assert_eq!(
        fixture.control.verify_owner(&profile).await,
        Err("cloudflare_identity_repair_required")
    );
}

#[tokio::test]
async fn incorrect_or_missing_runner_owner_fails_without_rebinding_it() {
    for owner in [None, Some("other-owner")] {
        let fixture = Fixture::new();
        let registration = serde_json::from_value(json!({"client_id":"runner","agent_instance_id":"instance", "agent_protocol_generation":2,"owner":owner,"capabilities":{"shell":true}})).unwrap();
        let registration = crate::test_support::current_runner_registration(registration);
        let access = webcodex_runner_registry::RunnerAccess {
            global_visibility: false,
            owner_bypass: false,
            username: owner.map(str::to_owned),
            group: None,
        };
        fixture
            .control
            .registry
            .register_with_auth(registration, owner.map(|_| &access))
            .await
            .unwrap();
        assert_eq!(
            fixture
                .control
                .verify_owner(&fixture.control.profile("quick").unwrap())
                .await,
            Err("cloudflare_owner_repair_required")
        );
        assert_eq!(
            fixture.control.registry.list_runners_for_auth(None).await[0]
                .owner
                .as_deref(),
            owner
        );
    }
}

#[handler]
async fn effective_public_configuration(depot: &mut Depot, res: &mut Response) {
    let config = crate::auth::get_config(depot).unwrap();
    res.render(Json(json!({"issuer":config.oauth2.issuer,"pkce":config.oauth2.require_pkce,"bridge":config.oauth2.shared_key_bridge_enabled,"private_dcr_owner_present":config.oauth2.dcr_owner_user_id.is_some()})));
}

#[tokio::test]
async fn public_snapshot_enforces_pkce_and_origin_without_changing_private_config() {
    use salvo::test::{ResponseExt, TestClient};
    let mut fixture = Fixture::new();
    let mut config = (*fixture.control.config).clone();
    config.oauth2.require_pkce = false;
    config.oauth2.issuer = Some("https://private.example".into());
    config.oauth2.dcr_owner_user_id = Some("private-owner".into());
    config.oauth2.shared_key_bridge_enabled = true;
    Arc::get_mut(&mut fixture.control).unwrap().config = Arc::new(config);
    fixture
        .control
        .prepare("quick", 1, false, None, None)
        .unwrap();
    let entry = fixture
        .control
        .db
        .get_public_ingress_entry_for_profile("quick")
        .unwrap()
        .unwrap();
    let entry = fixture
        .control
        .db
        .finalize_public_ingress_origin(&entry.fence(), "https://fixture.trycloudflare.com")
        .unwrap()
        .unwrap();
    fixture
        .control
        .db
        .set_public_ingress_admission(&entry.fence(), true)
        .unwrap();
    let service = Service::new(
        Router::new()
            .hoop(affix_state::inject(fixture.control.clone()))
            .hoop(PublicSnapshot)
            .get(effective_public_configuration),
    );
    let mut response = TestClient::get("https://fixture.trycloudflare.com/")
        .send(&service)
        .await;
    assert_eq!(response.status_code, Some(StatusCode::OK));
    let body: serde_json::Value = response.take_json().await.unwrap();
    assert_eq!(
        body,
        json!({"issuer":"https://fixture.trycloudflare.com","pkce":true,"bridge":false,"private_dcr_owner_present":false})
    );
    assert!(!fixture.control.config.oauth2.require_pkce);
    assert_eq!(
        fixture.control.config.oauth2.issuer.as_deref(),
        Some("https://private.example")
    );
}

#[tokio::test]
async fn configured_listener_port_conflict_is_explicit_and_never_reallocated() {
    let mut fixture = Fixture::new();
    let occupied = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = occupied.local_addr().unwrap().port();
    Arc::get_mut(&mut fixture.control).unwrap().ingress_port = port;
    private_file(
        &fixture.control.root.join("server/cloudflare-ingress.json"),
        serde_json::to_string(&json!({"port":port}))
            .unwrap()
            .as_bytes(),
    );
    let (_, active_entry) =
        activate_quick_fixture(&fixture.control, "https://active.trycloudflare.com");
    let result = CloudflareOwner::from_root(
        fixture.control.root.clone(),
        fixture.control.config.clone(),
        fixture.control.db.clone(),
        fixture.control.registry.clone(),
        Arc::new(crate::tool_runtime::ToolRuntime::new_for_tests()),
        Arc::new(crate::oauth_http::AuthorizeSessionStore::new()),
        Arc::new(crate::server_shutdown::ShutdownCoordinator::default()),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(
        fixture
            .control
            .db
            .get_public_ingress_entry(&active_entry.entry_id)
            .unwrap(),
        Some(active_entry),
        "failed listener takeover must not retire the active owner's admission or grants"
    );
    assert_eq!(
        webcodex_environment::cloudflare_ingress_port(
            &EnvironmentStore::open(fixture.control.root.clone()).unwrap()
        )
        .unwrap(),
        Some(port)
    );
}

mod registration;
