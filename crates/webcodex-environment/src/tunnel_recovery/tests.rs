use super::*;
use std::{
    cell::Cell,
    path::{Path, PathBuf},
};
struct FixtureServices {
    running: Cell<bool>,
    foreign: bool,
    competing: bool,
    starts: Cell<usize>,
    health: PathBuf,
    pid_calls: Cell<usize>,
    change_pid: bool,
}
impl RecoveryServices for FixtureServices {
    fn inspect(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<service::ServiceStatus, service::ServiceError> {
        let standalone = spec.component == Component::Tunnel;
        let ownership = if self.foreign {
            Ownership::Foreign
        } else if standalone && !self.competing {
            Ownership::Absent
        } else {
            Ownership::Owned
        };
        Ok(service::ServiceStatus {
            id: spec.id.clone(),
            ownership,
            installed: ownership != Ownership::Absent,
            enabled: Some(true),
            running: Some(self.running.get()),
            detail: None,
        })
    }
    fn start(
        &self,
        spec: &service::ServiceSpec,
    ) -> Result<service::ServiceStatus, service::ServiceError> {
        self.starts.set(self.starts.get() + 1);
        self.running.set(true);
        write_embedded_tunnel_observation(&self.health, 1, TunnelState::Running, true, true, None)
            .unwrap();
        self.inspect(spec)
    }
    fn running_pid(&self, _: &service::ServiceSpec) -> Result<Option<u32>, service::ServiceError> {
        let calls = self.pid_calls.get();
        self.pid_calls.set(calls + 1);
        Ok(self
            .running
            .get()
            .then(|| std::process::id() + u32::from(self.change_pid && calls > 0)))
    }
    fn owner_absent(&self, _: u32) -> bool {
        true
    }
}
fn fixture() -> (
    tempfile::TempDir,
    EnvironmentStore,
    PathBuf,
    FixtureServices,
    String,
) {
    let temp = crate::test_tempdir().unwrap();
    let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
    let directory = store.root().join("server/tunnels/primary");
    crate::storage::ensure_private_directory(&directory).unwrap();
    crate::storage::atomic_private_write(&directory.join("webcodex.env"), b"WEBCODEX_TUNNEL_PROFILE_ID=primary\nCONTROL_PLANE_TUNNEL_ID=tunnel_fixture\nCONTROL_PLANE_API_KEY=fixture-private\nWEBCODEX_TOKEN=fixture-local\n").unwrap();
    let health = directory.join("readiness.json");
    crate::storage::atomic_private_write(&health, b"{}").unwrap();
    let profile = TunnelRecord {
        profile_id: "primary".into(),
        provider: TunnelProvider::Openai,
        configuration_id: None,
        name: "Primary".into(),
        host_mode: TunnelHostMode::Embedded,
        autostart: true,
        revision: 1,
        runtime_revision: 1,
        installed: false,
        started: false,
    };
    store.write_json("tunnel.json", &vec![profile]).unwrap();
    let binary = std::env::current_exe().unwrap();
    let record = EnvironmentRecord {
        schema_version: ENVIRONMENT_SCHEMA,
        environment_id: "fixture-environment".into(),
        request: SetupRequest {
            runner_display_name: None,
            service_scope: ServiceScope::User,
            mode: EnvironmentMode::Create {
                listen: "127.0.0.1:18080".into(),
            },
            server_url: "http://127.0.0.1:18080".into(),
            project: None,
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
        projects: vec![],
        configured: true,
    };
    store.save_environment(&record).unwrap();
    let root = temp.path().join("runs");
    let fence = run_fence::RunFence::acquire(&root, ORIGIN, "tunnel_fixture").unwrap();
    let run = run_fence::observe(&root, ORIGIN, "tunnel_fixture")
        .unwrap()
        .unwrap()
        .run_id;
    drop(fence);
    let services = FixtureServices {
        running: Cell::new(false),
        foreign: false,
        competing: false,
        starts: Cell::new(0),
        health,
        pid_calls: Cell::new(0),
        change_pid: false,
    };
    (temp, store, root, services, run)
}
fn apply(run: &str) -> TunnelRecoveryRequest {
    TunnelRecoveryRequest {
        apply: true,
        accept_uncertain_effects: true,
        expected_environment_id: Some("fixture-environment".into()),
        expected_revision: Some(1),
        expected_run_id: Some(run.into()),
    }
}
fn marker_present(root: &Path) -> bool {
    run_fence::observe(root, ORIGIN, "tunnel_fixture")
        .unwrap()
        .is_some()
}

#[tokio::test]
async fn exact_recovery_archives_starts_existing_owner_and_checks_both_ready() {
    let (_temp, store, root, services, run) = fixture();
    let plan = recover(
        &store,
        "primary",
        TunnelRecoveryRequest::default(),
        &root,
        &services,
    )
    .await
    .unwrap();
    assert_eq!(plan.status, "tunnel_restart_uncertain");
    assert!(marker_present(&root));
    assert_eq!(services.starts.get(), 0);
    let result = recover(&store, "primary", apply(&run), &root, &services)
        .await
        .unwrap();
    assert_eq!(result.status, "ready");
    assert_eq!(services.starts.get(), 1);
    assert!(!marker_present(&root));
    assert!(recover(&store, "primary", apply(&run), &root, &services)
        .await
        .is_err());
    assert_eq!(services.starts.get(), 1);
}
#[tokio::test]
async fn active_foreign_competing_and_stale_authorities_preserve_the_fence() {
    for scenario in 0..7 {
        let (_temp, store, root, mut services, run) = fixture();
        let mut request = apply(&run);
        match scenario {
            0 => services.running.set(true),
            1 => services.foreign = true,
            2 => services.competing = true,
            3 => request.expected_revision = Some(2),
            4 => request.expected_environment_id = Some("other".into()),
            5 => request.expected_run_id = Some(uuid::Uuid::new_v4().to_string()),
            _ => request.accept_uncertain_effects = false,
        }
        assert!(recover(&store, "primary", request, &root, &services)
            .await
            .is_err());
        assert!(marker_present(&root));
        assert_eq!(services.starts.get(), 0);
    }
}
#[tokio::test]
async fn wrong_profile_and_concurrent_environment_owner_never_start() {
    let (_temp, store, root, services, run) = fixture();
    assert!(recover(&store, "wrong", apply(&run), &root, &services)
        .await
        .is_err());
    let _lock = store.lock().unwrap();
    assert!(recover(&store, "primary", apply(&run), &root, &services)
        .await
        .is_err());
    assert!(marker_present(&root));
    assert_eq!(services.starts.get(), 0);
}

#[tokio::test]
async fn changed_pid_during_readiness_verification_cannot_report_success() {
    let (_temp, store, root, mut services, run) = fixture();
    services.change_pid = true;
    let error = recover(&store, "primary", apply(&run), &root, &services)
        .await
        .unwrap_err();
    assert_eq!(error.code, "tunnel_recovery_owner_changed");
    assert_eq!(services.starts.get(), 1);
    assert!(!marker_present(&root)); // Original evidence remains in the archive.
    assert!(!owner_absent(std::process::id()));
}
