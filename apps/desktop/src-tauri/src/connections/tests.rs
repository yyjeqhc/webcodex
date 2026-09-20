use super::*;
use crate::models::{
    aggregate_readiness, ExposureReadiness, ProjectReadiness, RunnerReadiness, ServerReadiness,
    TunnelConfigSource,
};
use crate::process::ProcessPhase;
use serde_json::json;
use std::io::{Read, Write};
use std::process::Command;

fn config(id: TunnelProfileId) -> TunnelProfileConfigSnapshot {
    TunnelProfileConfigSnapshot {
        id,
        name: id.to_string(),
        tunnel_id: Some(format!("tunnel_{}", id)),
        credential_present: true,
        enabled: true,
        autostart: true,
        revision: 1,
        source: TunnelConfigSource::File,
    }
}
fn snapshot(ids: &[TunnelProfileId]) -> ConnectionsSnapshot {
    ConnectionsSnapshot {
        profiles: ids
            .iter()
            .map(|id| TunnelConnectionSnapshot {
                config: config(*id),
                runtime: Default::default(),
            })
            .collect(),
        ..Default::default()
    }
}
fn root() -> PathBuf {
    std::env::temp_dir()
        .join("webcodex-connection-runtime-tests")
        .join("regular-tunnel-runtime")
}
fn metadata(port: u16) -> Value {
    let directory = root().join(format!("openai-{}", uuid::Uuid::new_v4().simple()));
    json!({"directory":directory, "health_url":format!("http://127.0.0.1:{port}"), "log_file":directory.join("openai-tunnel.log"), "tunnel_client_pid":42, "local_mcp_url":"http://127.0.0.1:62645/mcp"})
}
const FIXTURE_CHILD_ENV: &str = "WEBCODEX_CONNECTION_FIXTURE_CHILD";
const FIXTURE_EVENTS_ENV: &str = "WEBCODEX_FIXTURE_EVENTS";
const FIXTURE_CHILD_TEST: &str =
    "connections::tests::fixture_child_emits_machine_events_and_holds_parent_lease";

fn fixture(events: &[Value]) -> Command {
    let stream = events
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", FIXTURE_CHILD_TEST, "--nocapture"]);
    command.env(FIXTURE_CHILD_ENV, "1");
    command.env(FIXTURE_EVENTS_ENV, stream);
    command
}

#[test]
fn fixture_child_emits_machine_events_and_holds_parent_lease() {
    if std::env::var_os(FIXTURE_CHILD_ENV).is_none() {
        return;
    }

    let events = std::env::var(FIXTURE_EVENTS_ENV).unwrap();
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(events.as_bytes()).unwrap();
    stdout.write_all(b"\n").unwrap();
    stdout.flush().unwrap();
    drop(stdout);

    let mut stdin = std::io::stdin().lock();
    let mut sink = Vec::new();
    stdin.read_to_end(&mut sink).unwrap();
}
async fn start(
    registry: &ConnectionRuntimes,
    supervisor: &Arc<tokio::sync::Mutex<ProcessSupervisor>>,
    id: TunnelProfileId,
    port: u16,
) -> ProcessSnapshot {
    let mut supervisor_guard = supervisor.lock().await;
    let key = ProcessKey::RegularTunnel(id);
    let command = fixture(&[
        json!({"event":"ready", "schema_version":1, "provider":"openai", "runtime":metadata(port)}),
        json!({"event":"health", "schema_version":1, "tunnel_ready":true, "local_mcp_ready":true}),
    ]);
    let events = supervisor_guard
        .spawn_owned(key, command, true)
        .await
        .unwrap()
        .unwrap();
    let process = supervisor_guard.snapshot(key).unwrap();
    registry.start(
        id,
        process.clone(),
        events,
        root(),
        "http://127.0.0.1:62645/mcp".into(),
        supervisor.clone(),
        ActivityLog::default(),
    );
    process
}
async fn wait_running(
    registry: &ConnectionRuntimes,
    ids: &[TunnelProfileId],
) -> ConnectionsSnapshot {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let mut snapshot = snapshot(ids);
        registry.project(&mut snapshot, true);
        if snapshot.running == ids.len() {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "profiles did not become ready: {snapshot:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn concurrent_observers_are_independent_and_stale_generation_cannot_kill_replacement() {
    let registry = ConnectionRuntimes::default();
    let supervisor = Arc::new(tokio::sync::Mutex::new(ProcessSupervisor::new(
        ActivityLog::default(),
    )));
    let a = TunnelProfileId::new();
    let b = TunnelProfileId::new();
    let first_a = start(&registry, &supervisor, a, 51001).await;
    let first_b = start(&registry, &supervisor, b, 51002).await;
    let running = wait_running(&registry, &[a, b]).await;
    assert_ne!(
        running.profiles[0].runtime.health_url,
        running.profiles[1].runtime.health_url
    );
    assert_ne!(
        running.profiles[0].runtime.runtime_directory,
        running.profiles[1].runtime.runtime_directory
    );
    assert_ne!(
        running.profiles[0].runtime.log_file,
        running.profiles[1].runtime.log_file
    );
    assert_eq!(
        running.profiles[0].runtime.local_mcp_url,
        running.profiles[1].runtime.local_mcp_url
    );
    registry.stopping(a);
    supervisor
        .lock()
        .await
        .stop_checked(ProcessKey::RegularTunnel(a))
        .await
        .unwrap();
    registry.stopped(a, true);
    assert_eq!(
        supervisor
            .lock()
            .await
            .snapshot(ProcessKey::RegularTunnel(b))
            .unwrap()
            .pid,
        first_b.pid
    );
    let second_a = start(&registry, &supervisor, a, 51003).await;
    wait_running(&registry, &[a, b]).await;
    assert_ne!(first_a.generation, second_a.generation);
    assert!(!registry.update(a, first_a.generation, |s| s.lifecycle =
        ConnectionLifecycle::Error));
    supervisor
        .lock()
        .await
        .stop_generation(ProcessKey::RegularTunnel(a), first_a.generation)
        .await;
    assert_eq!(
        supervisor
            .lock()
            .await
            .snapshot(ProcessKey::RegularTunnel(a))
            .unwrap()
            .pid,
        second_a.pid
    );
    registry.stopping(b);
    supervisor
        .lock()
        .await
        .stop_checked(ProcessKey::RegularTunnel(b))
        .await
        .unwrap();
    registry.remove(b);
    assert_eq!(
        supervisor
            .lock()
            .await
            .snapshot(ProcessKey::RegularTunnel(a))
            .unwrap()
            .phase,
        ProcessPhase::Running
    );
    registry.cancel_all();
    supervisor.lock().await.stop_all().await;
    assert!(supervisor.lock().await.keys().is_empty());
}

#[tokio::test]
async fn malformed_or_secret_bearing_child_failure_stays_local_and_safe() {
    let registry = ConnectionRuntimes::default();
    let activity = ActivityLog::default();
    let supervisor = Arc::new(tokio::sync::Mutex::new(ProcessSupervisor::new(
        activity.clone(),
    )));
    let a = TunnelProfileId::new();
    let b = TunnelProfileId::new();
    let first_a = start(&registry, &supervisor, a, 51011).await;
    wait_running(&registry, &[a]).await;
    let key = ProcessKey::RegularTunnel(b);
    let mut guard = supervisor.lock().await;
    let events = guard.spawn_owned(key, fixture(&[json!({"event":"error","message":"private-fixture-api-key","api_key":"private-fixture-api-key"})]), true).await.unwrap().unwrap();
    let process = guard.snapshot(key).unwrap();
    drop(guard);
    registry.start(
        b,
        process,
        events,
        root(),
        "http://127.0.0.1:62645/mcp".into(),
        supervisor.clone(),
        activity.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    let projected = loop {
        let mut s = snapshot(&[a, b]);
        registry.project(&mut s, true);
        if s.needs_attention == 1 {
            break s;
        }
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(projected.running, 1);
    assert_eq!(
        supervisor
            .lock()
            .await
            .snapshot(ProcessKey::RegularTunnel(a))
            .unwrap()
            .pid,
        first_a.pid
    );
    assert!(!serde_json::to_string(&projected)
        .unwrap()
        .contains("private-fixture-api-key"));
    assert!(!serde_json::to_string(&activity.snapshot())
        .unwrap()
        .contains("private-fixture-api-key"));
    let readiness = aggregate_readiness(
        ServerReadiness::Ready,
        RunnerReadiness::Ready,
        ExposureReadiness::LocalReady,
        ProjectReadiness::Ready,
    );
    assert!(readiness.runtime_ready);
    assert_eq!(
        readiness.summary_kind,
        crate::models::ReadinessSummaryKind::RuntimeReadyLocalOnly
    );
    registry.cancel_all();
    supervisor.lock().await.stop_all().await;
}

#[test]
fn metadata_rejects_remote_health_credentials_traversal_or_another_local_server() {
    let valid = metadata(51234);
    assert!(runtime_metadata(&valid, &root(), "http://127.0.0.1:62645/mcp").is_some());
    for url in [
        "https://example.test",
        "http://user:secret@127.0.0.1:80",
        "http://127.0.0.1:80/?key=secret",
        "http://127.0.0.1:80/elsewhere",
        "http://127.0.0.1",
    ] {
        let mut invalid = valid.clone();
        invalid["health_url"] = json!(url);
        assert!(runtime_metadata(&invalid, &root(), "http://127.0.0.1:62645/mcp").is_none());
    }
    for (key, value) in [
        ("directory", json!(root().join("../escaped"))),
        ("log_file", json!("/tmp/unrelated.log")),
        ("local_mcp_url", json!("http://127.0.0.1:62646/mcp")),
        ("api_key", json!("fixture-secret")),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(runtime_metadata(&invalid, &root(), "http://127.0.0.1:62645/mcp").is_none());
    }
}

#[test]
fn runtime_outage_degrades_only_projection_and_recovery_does_not_spawn_a_new_process() {
    let registry = ConnectionRuntimes::default();
    let id = TunnelProfileId::new();
    registry.0.lock().unwrap().insert(
        id,
        Entry {
            generation: 1,
            cancel: CancellationSignal::new(),
            state: ConnectionRuntimeSnapshot {
                lifecycle: ConnectionLifecycle::Running,
                ready: true,
                health: ConnectionHealth::Healthy,
                pid: Some(42),
                ..Default::default()
            },
        },
    );
    let mut view = snapshot(&[id]);
    registry.project(&mut view, false);
    assert_eq!(view.running, 0);
    assert_eq!(
        view.profiles[0].runtime.last_error,
        Some(ConnectionError::LocalMcpUnavailable)
    );
    registry.project(&mut view, true);
    assert_eq!(view.running, 1);
    assert_eq!(view.profiles[0].runtime.pid, Some(42));
    assert!(!registry.update(id, 0, |s| s.pid = Some(999)));
}
