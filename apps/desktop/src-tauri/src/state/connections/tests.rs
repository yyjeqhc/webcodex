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
            id: TunnelProfileId::new(),
            name: "ChatGPT".into(),
            tunnel_id: Some("fixture".into()),
            credential_present: true,
            enabled: true,
            autostart: true,
            revision: 1,
            source: crate::models::TunnelConfigSource::File,
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
