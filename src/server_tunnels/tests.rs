use super::*;
use webcodex_environment::{EnvironmentStore, Secret, TunnelCredentials};

fn fixture() -> (tempfile::TempDir, EmbeddedTunnelProfile, TunnelStatus) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("environment");
    let store = EnvironmentStore::open(root).unwrap();
    let path = store.root().join("readiness.json");
    create_health_file(&path);
    let profile = EmbeddedTunnelProfile {
        profile_id: "primary".into(),
        autostart: true,
        runtime_revision: 7,
        credentials: TunnelCredentials {
            tunnel_id: Secret::new("tunnel_0123456789abcdef0123456789abcdef".into()),
            api_key: Secret::new("fixture-control".into()),
        },
        local_token: Secret::new("fixture-local".into()),
        proxy: None,
        readiness_path: path,
    };
    let status = TunnelStatus(Arc::new(Mutex::new(vec![ProfileStatus {
        profile_id: profile.profile_id.clone(),
        host_mode: "embedded",
        state: TunnelState::Running,
        tunnel_ready: true,
        local_mcp_ready: true,
        shutdown_outcome: None,
        diagnostic: None,
    }])));
    (temp, profile, status)
}

fn create_health_file(path: &Path) {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    // On Windows the file inherits the EnvironmentStore directory's private ACL.
    options.open(path).unwrap();
}

#[test]
fn reader_contention_preserves_live_status_and_publication_recovers() {
    let (_temp, profile, status) = fixture();
    persist_status(&profile, &status, 0).unwrap();
    let previous = std::fs::read(&profile.readiness_path).unwrap();
    let reader = std::fs::File::open(&profile.readiness_path).unwrap();
    reader.try_lock_shared().unwrap();
    // A separately opened handle exercises the same lock conflict as a Desktop
    // or CLI reader, deterministically and without timing or external services.
    assert!(persist_status(&profile, &status, 0).is_err());
    for _ in 0..4 {
        persist_heartbeat(&profile, &status, 0);
        let state = &status.snapshot()[0];
        assert_eq!(state.state, TunnelState::Running);
        assert!(state.tunnel_ready && state.local_mcp_ready);
        assert_eq!(state.shutdown_outcome, None);
        assert_eq!(
            state.diagnostic.as_deref(),
            Some("tunnel_health_unavailable")
        );
        assert_eq!(std::fs::read(&profile.readiness_path).unwrap(), previous);
    }

    // Live observations continue changing while publication is unavailable.
    status.update(0, |state| state.local_mcp_ready = false);
    persist_heartbeat(&profile, &status, 0);
    assert!(!status.snapshot()[0].local_mcp_ready);
    reader.unlock().unwrap();
    persist_heartbeat(&profile, &status, 0);
    let recovered: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&profile.readiness_path).unwrap()).unwrap();
    assert_eq!(recovered["state"], "running");
    assert_eq!(recovered["service_pid"], std::process::id());
    assert_eq!(recovered["profile_revision"], 7);
    assert_eq!(recovered["tunnel_ready"], true);
    assert_eq!(recovered["local_mcp_ready"], false);
    assert!(recovered["diagnostic"].is_null());
    assert!(status.snapshot()[0].diagnostic.is_none());
}

#[test]
fn missing_heartbeat_file_preserves_live_status_without_recreating_it() {
    let (_temp, profile, status) = fixture();
    std::fs::remove_file(&profile.readiness_path).unwrap();
    persist_heartbeat(&profile, &status, 0);
    assert_eq!(status.snapshot()[0].state, TunnelState::Running);
    assert_eq!(
        status.snapshot()[0].diagnostic.as_deref(),
        Some("tunnel_health_unavailable")
    );
    assert!(!profile.readiness_path.exists());
    // Only repair of the precreated private file allows publication to recover.
    create_health_file(&profile.readiness_path);
    persist_heartbeat(&profile, &status, 0);
    assert!(status.snapshot()[0].diagnostic.is_none());

    // A real terminal failure still owns the final diagnostic and lifecycle.
    status.update(0, |state| {
        state.state = TunnelState::Failed;
        state.tunnel_ready = false;
        state.local_mcp_ready = false;
        state.shutdown_outcome = Some("uncertain");
        state.diagnostic = Some("tunnel_restart_uncertain".into());
    });
    persist_status(&profile, &status, 0).unwrap();
    let terminal: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&profile.readiness_path).unwrap()).unwrap();
    assert_eq!(terminal["state"], "failed");
    assert_eq!(terminal["diagnostic"], "tunnel_restart_uncertain");
    assert_eq!(terminal["tunnel_ready"], false);
    assert_eq!(terminal["local_mcp_ready"], false);
}

#[tokio::test]
async fn unavailable_initial_status_still_blocks_profile_launch() {
    let (_temp, profile, status) = fixture();
    std::fs::remove_file(&profile.readiness_path).unwrap();
    status.update(0, |state| {
        state.state = TunnelState::Starting;
        state.tunnel_ready = false;
        state.local_mcp_ready = false;
    });
    let (_stop, rx) = watch::channel(false);
    run_profile(
        0,
        profile,
        "http://127.0.0.1:9/mcp".into(),
        status.clone(),
        rx,
    )
    .await;
    let state = &status.snapshot()[0];
    assert_eq!(state.state, TunnelState::Failed);
    assert_eq!(
        state.diagnostic.as_deref(),
        Some("tunnel_health_unavailable")
    );
    assert!(!state.tunnel_ready && !state.local_mcp_ready);
    assert_eq!(state.shutdown_outcome, None);
}
