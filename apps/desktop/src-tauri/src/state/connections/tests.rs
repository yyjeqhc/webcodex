use super::*;
use crate::connections::ConnectionRuntimeSnapshot;
use crate::models::{Enrollment, Exposure, RunnerTopology, RuntimeTopology};
use crate::tunnel_config::TunnelProfileConfigSnapshot;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "webcodex-connection-state-{}",
            uuid::Uuid::new_v4()
        )))
    }
    fn app(&self) -> AppState {
        AppState::new(self.0.clone(), self.0.join("resources")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_connection_observations_survive_desktop_state_reads() {
    let fixture = Fixture::new();
    let app = fixture.app();
    let mut published = app.published.write().unwrap();
    published.persistent_environment = Some("fixture-environment".into());
    published.connections.profiles = vec![TunnelConnectionSnapshot {
        config: TunnelProfileConfigSnapshot {
            id: TunnelProfileId::new().to_string(),
            name: "ChatGPT".into(),
            tunnel_id: Some("fixture".into()),
            credential_present: true,
            enabled: true,
            autostart: true,
            revision: 1,
            source: crate::models::TunnelConfigSource::File,
            host_mode: webcodex_environment::TunnelHostMode::Standalone,
            server_restart_required: false,
        },
        runtime: ConnectionRuntimeSnapshot {
            lifecycle: ConnectionLifecycle::Running,
            health: ConnectionHealth::Healthy,
            ready: true,
            process_started: true,
            process_ready: true,
            tunnel_ready: Some(true),
            local_mcp_ready: Some(true),
            ..Default::default()
        },
    }];
    published.connections.recount();
    let expected = published.connections.clone();
    drop(published);
    for _ in 0..3 {
        assert_eq!(app.get_state().connections, expected);
    }
}

#[tokio::test]
async fn connection_resume_uses_current_profile_preferences_without_enabling_stopped_profiles() {
    let fixture = Fixture::new();
    let app = fixture.app();
    {
        let mut slot = app.core.lock().await;
        let core = slot.as_mut().unwrap();
        core.config.persistent_environment = None;
        core.tunnel_config = TunnelConfig::default();
        core.config.runtime_autostart = Some(false);
        core.config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        core.snapshot.readiness.runtime_ready = true;
        let path = fixture.0.join("tunnels.json");
        for autostart in [false, true] {
            let id = core
                .tunnel_config
                .update_profile(
                    &path,
                    TunnelProfileRequest {
                        id: None,
                        name: format!("Manual-{autostart}"),
                        tunnel_id: format!("fixture-{autostart}"),
                        api_key: Some("fixture-key".into()),
                        autostart,
                        host_mode: webcodex_environment::TunnelHostMode::Standalone,
                        expected_revision: None,
                    },
                )
                .unwrap();
            if autostart {
                core.tunnel_config.set_enabled(&path, id, false).unwrap();
            }
        }
    }
    let state = app.resume_saved_connections().await.unwrap();
    assert!(!state.runtime_autostart);
    assert_eq!(state.connections.profiles.len(), 2);
    assert!(state.connections.profiles.iter().all(|profile| {
        profile.runtime.lifecycle == ConnectionLifecycle::Stopped
            && profile.runtime.last_error.is_none()
    }));
    assert!(state
        .connections
        .profiles
        .iter()
        .any(|profile| !profile.config.enabled && profile.config.autostart));
    assert!(app
        .supervisor
        .lock()
        .await
        .snapshot(ProcessKey::LocalServer)
        .is_none());
    assert!(app
        .supervisor
        .lock()
        .await
        .snapshot(ProcessKey::LocalRunner)
        .is_none());
}

fn service_status(running: bool) -> webcodex_environment::service::ServiceStatus {
    webcodex_environment::service::ServiceStatus {
        id: "webcodex".into(),
        ownership: webcodex_environment::service::Ownership::Owned,
        installed: true,
        enabled: Some(true),
        running: Some(running),
        detail: None,
    }
}

fn environment_profile(
    host_mode: webcodex_environment::TunnelHostMode,
    autostart: bool,
) -> webcodex_environment::TunnelProfileSnapshot {
    webcodex_environment::TunnelProfileSnapshot {
        profile_id: "work".into(),
        name: "Work".into(),
        tunnel_id: "tunnel_work".into(),
        credential_present: true,
        host_mode,
        autostart,
        revision: 3,
        installed: host_mode == webcodex_environment::TunnelHostMode::Standalone,
        started: false,
    }
}

#[test]
fn persistent_projection_preserves_cli_identity_and_surfaces_one_explicit_server_restart() {
    let profile = environment_profile(webcodex_environment::TunnelHostMode::Embedded, true);
    let pending = webcodex_environment::TunnelRuntimeObservation {
        profile_id: "work".into(),
        tunnel_id: "tunnel_work".into(),
        service_status: service_status(true),
        host_mode: webcodex_environment::TunnelHostMode::Embedded,
        autostart: true,
        ready: false,
        tunnel_ready: false,
        local_mcp_ready: false,
        configured_revision: 3,
        applied_revision: Some(2),
        server_restart_required: true,
    };
    let projected = persistent_connection_projection(profile.clone(), Ok(pending));
    assert_eq!(projected.config.id, "work");
    assert_eq!(
        projected.config.source,
        crate::models::TunnelConfigSource::Environment
    );
    assert_eq!(
        projected.config.host_mode,
        webcodex_environment::TunnelHostMode::Embedded
    );
    assert!(projected.config.server_restart_required);
    assert_eq!(projected.runtime.lifecycle, ConnectionLifecycle::Stopped);
    assert!(projected.runtime.last_error.is_none());
    assert!(!projected.runtime.process_started);

    let applied = webcodex_environment::TunnelRuntimeObservation {
        profile_id: "work".into(),
        tunnel_id: "tunnel_work".into(),
        service_status: service_status(true),
        host_mode: webcodex_environment::TunnelHostMode::Embedded,
        autostart: true,
        ready: true,
        tunnel_ready: true,
        local_mcp_ready: true,
        configured_revision: 3,
        applied_revision: Some(3),
        server_restart_required: false,
    };
    let running = persistent_connection_projection(profile, Ok(applied));
    assert!(!running.config.server_restart_required);
    assert_eq!(running.runtime.lifecycle, ConnectionLifecycle::Running);
    assert_eq!(running.runtime.health, ConnectionHealth::Healthy);
    assert!(running.runtime.ready);
}

#[test]
fn persistent_projection_fails_closed_when_runtime_identity_differs_from_catalog() {
    let profile = environment_profile(webcodex_environment::TunnelHostMode::Embedded, true);
    let observation = webcodex_environment::TunnelRuntimeObservation {
        profile_id: "work".into(),
        tunnel_id: "tunnel_other".into(),
        service_status: service_status(true),
        host_mode: webcodex_environment::TunnelHostMode::Embedded,
        autostart: true,
        ready: true,
        tunnel_ready: true,
        local_mcp_ready: true,
        configured_revision: 3,
        applied_revision: Some(3),
        server_restart_required: false,
    };
    let projected = persistent_connection_projection(profile, Ok(observation));
    assert_eq!(projected.runtime.lifecycle, ConnectionLifecycle::Error);
    assert_eq!(
        projected.runtime.last_error,
        Some(ConnectionError::StartFailed)
    );
    assert!(!projected.runtime.ready);
}

#[test]
fn disabled_server_owned_profile_is_not_misreported_as_failed_while_server_runs() {
    let profile = environment_profile(webcodex_environment::TunnelHostMode::Embedded, false);
    let observation = webcodex_environment::TunnelRuntimeObservation {
        profile_id: "work".into(),
        tunnel_id: "tunnel_work".into(),
        service_status: service_status(true),
        host_mode: webcodex_environment::TunnelHostMode::Embedded,
        autostart: false,
        ready: false,
        tunnel_ready: false,
        local_mcp_ready: false,
        configured_revision: 3,
        applied_revision: Some(3),
        server_restart_required: false,
    };
    let projected = persistent_connection_projection(profile, Ok(observation));
    assert!(!projected.config.enabled);
    assert_eq!(projected.runtime.lifecycle, ConnectionLifecycle::Stopped);
    assert!(projected.runtime.last_error.is_none());
    assert_eq!(projected.runtime.tunnel_ready, None);
}
