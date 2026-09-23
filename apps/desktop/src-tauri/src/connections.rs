//! Independent, generation-fenced observers for Desktop-owned tunnel processes.
//! Desired configuration lives in tunnel_config; this module never owns credentials.
use crate::activity::{ActivityEventKind, ActivityLevel, ActivityLog};
use crate::connection_id::TunnelProfileId;
use crate::operation::CancellationSignal;
use crate::process::{MachineEventReceiver, ProcessKey, ProcessSnapshot, ProcessSupervisor};
use crate::tunnel_config::TunnelProfileConfigSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::Instant;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);
const HEALTH_STALE_AFTER: Duration = Duration::from_secs(12);

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionLifecycle {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionHealth {
    #[default]
    Unknown,
    Healthy,
    Degraded,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionError {
    StartFailed,
    StartupTimeout,
    ProcessExited,
    ProtocolInvalid,
    HealthStale,
    TunnelUnavailable,
    LocalMcpUnavailable,
    StopFailed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionLogEntry {
    pub timestamp_ms: u64,
    pub event: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionRuntimeSnapshot {
    pub lifecycle: ConnectionLifecycle,
    pub pid: Option<u32>,
    pub health: ConnectionHealth,
    pub last_error: Option<ConnectionError>,
    pub ready: bool,
    pub process_started: bool,
    pub process_ready: bool,
    pub tunnel_ready: Option<bool>,
    pub local_mcp_ready: Option<bool>,
    pub failure_stage: Option<String>,
    pub reason_code: Option<String>,
    pub runtime_directory: Option<PathBuf>,
    pub health_url: Option<String>,
    pub log_file: Option<PathBuf>,
    pub tunnel_client_pid: Option<u32>,
    pub local_mcp_url: Option<String>,
    pub logs: VecDeque<ConnectionLogEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelConnectionSnapshot {
    #[serde(flatten)]
    pub config: TunnelProfileConfigSnapshot,
    #[serde(flatten)]
    pub runtime: ConnectionRuntimeSnapshot,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionsSnapshot {
    pub profiles: Vec<TunnelConnectionSnapshot>,
    pub running: usize,
    pub needs_attention: usize,
    pub config_error: bool,
}
impl ConnectionsSnapshot {
    pub fn recount(&mut self) {
        self.running = self.profiles.iter().filter(|p| p.runtime.ready).count();
        self.needs_attention = self
            .profiles
            .iter()
            .filter(|p| p.runtime.last_error.is_some())
            .count();
    }
    pub fn any_active(&self) -> bool {
        self.profiles.iter().any(|p| {
            matches!(
                p.runtime.lifecycle,
                ConnectionLifecycle::Starting
                    | ConnectionLifecycle::Running
                    | ConnectionLifecycle::Stopping
            )
        })
    }
}

struct Entry {
    generation: u64,
    cancel: CancellationSignal,
    state: ConnectionRuntimeSnapshot,
}
#[derive(Clone, Default)]
pub(crate) struct ConnectionRuntimes(Arc<Mutex<HashMap<TunnelProfileId, Entry>>>);

impl ConnectionRuntimes {
    pub fn project(&self, snapshot: &mut ConnectionsSnapshot, runtime_ready: bool) {
        let entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        for profile in &mut snapshot.profiles {
            profile.runtime = entries
                .get(&profile.config.id)
                .map(|e| e.state.clone())
                .unwrap_or_default();
            if !runtime_ready && profile.runtime.lifecycle == ConnectionLifecycle::Running {
                profile.runtime.ready = false;
                profile.runtime.health = ConnectionHealth::Degraded;
                profile.runtime.last_error = Some(ConnectionError::LocalMcpUnavailable);
            }
        }
        snapshot.recount();
    }

    pub fn start(
        &self,
        id: TunnelProfileId,
        process: ProcessSnapshot,
        events: MachineEventReceiver,
        expected_runtime_root: PathBuf,
        local_mcp_url: String,
        supervisor: Arc<tokio::sync::Mutex<ProcessSupervisor>>,
        activity: ActivityLog,
    ) {
        let cancel = CancellationSignal::new();
        let mut state = ConnectionRuntimeSnapshot {
            lifecycle: ConnectionLifecycle::Starting,
            pid: process.pid,
            process_started: true,
            ..Default::default()
        };
        log(&mut state, "starting");
        let generation = process.generation;
        if let Some(old) = self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(
            id,
            Entry {
                generation,
                cancel: cancel.clone(),
                state,
            },
        ) {
            old.cancel.cancel();
        }
        let registry = self.clone();
        tokio::spawn(async move {
            registry
                .monitor(
                    id,
                    generation,
                    cancel,
                    events,
                    expected_runtime_root,
                    local_mcp_url,
                    supervisor,
                    activity,
                )
                .await;
        });
    }

    pub fn fail_start(&self, id: TunnelProfileId) {
        let mut state = ConnectionRuntimeSnapshot {
            lifecycle: ConnectionLifecycle::Error,
            last_error: Some(ConnectionError::StartFailed),
            ..Default::default()
        };
        log(&mut state, "start_failed");
        if let Some(old) = self.0.lock().unwrap_or_else(|e| e.into_inner()).insert(
            id,
            Entry {
                generation: 0,
                cancel: CancellationSignal::new(),
                state,
            },
        ) {
            old.cancel.cancel();
        }
    }

    pub fn stopping(&self, id: TunnelProfileId) {
        if let Some(entry) = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            entry.cancel.cancel();
            entry.state.lifecycle = ConnectionLifecycle::Stopping;
            entry.state.ready = false;
            log(&mut entry.state, "stopping");
        }
    }
    pub fn stopped(&self, id: TunnelProfileId, success: bool) {
        if let Some(entry) = self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&id)
        {
            entry.cancel.cancel();
            entry.state.lifecycle = if success {
                ConnectionLifecycle::Stopped
            } else {
                ConnectionLifecycle::Error
            };
            entry.state.ready = false;
            entry.state.health = ConnectionHealth::Unknown;
            entry.state.last_error = if success {
                None
            } else {
                Some(ConnectionError::StopFailed)
            };
            if success {
                entry.state.pid = None;
                entry.state.tunnel_client_pid = None;
            }
            log(
                &mut entry.state,
                if success { "stopped" } else { "stop_failed" },
            );
        }
    }
    pub fn remove(&self, id: TunnelProfileId) {
        if let Some(entry) = self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(&id) {
            entry.cancel.cancel();
        }
    }
    pub fn cancel_all(&self) {
        for entry in self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values_mut()
        {
            entry.cancel.cancel();
            entry.state.ready = false;
            entry.state.lifecycle = ConnectionLifecycle::Stopping;
        }
    }
    fn update(
        &self,
        id: TunnelProfileId,
        generation: u64,
        f: impl FnOnce(&mut ConnectionRuntimeSnapshot),
    ) -> bool {
        let mut entries = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = entries
            .get_mut(&id)
            .filter(|entry| entry.generation == generation && !entry.cancel.is_cancelled())
        else {
            return false;
        };
        f(&mut entry.state);
        true
    }

    async fn monitor(
        &self,
        id: TunnelProfileId,
        generation: u64,
        cancel: CancellationSignal,
        mut events: MachineEventReceiver,
        expected_root: PathBuf,
        local_mcp_url: String,
        supervisor: Arc<tokio::sync::Mutex<ProcessSupervisor>>,
        activity: ActivityLog,
    ) {
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let mut ready_seen = false;
        let mut last_health = None;
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let failure = loop {
            let event = tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tick.tick() => {
                    if !ready_seen && Instant::now() >= deadline { break ConnectionError::StartupTimeout; }
                    if ready_seen && last_health.is_none_or(|at| Instant::now().duration_since(at) > HEALTH_STALE_AFTER) {
                        self.update(id, generation, |state| {
                            if state.last_error != Some(ConnectionError::HealthStale) { log(state, "health_stale"); }
                            state.ready = false;
                            state.health = ConnectionHealth::Degraded;
                            state.last_error = Some(ConnectionError::HealthStale);
                            state.failure_stage = Some("tunnel_health".into());
                            state.reason_code = Some("tunnel_health_stale".into());
                        });
                    }
                    continue;
                },
                event = events.recv() => event,
            };
            let Some(event) = event else {
                break ConnectionError::ProcessExited;
            };
            match event.get("event").and_then(Value::as_str) {
                Some("ready") if !ready_seen => {
                    if event["schema_version"] != 1 || event["provider"] != "openai" {
                        break ConnectionError::ProtocolInvalid;
                    }
                    let Some(metadata) =
                        runtime_metadata(&event["runtime"], &expected_root, &local_mcp_url)
                    else {
                        break ConnectionError::ProtocolInvalid;
                    };
                    if !self.update(id, generation, |state| {
                        state.lifecycle = ConnectionLifecycle::Running;
                        state.runtime_directory = Some(metadata.directory);
                        state.health_url = Some(metadata.health_url);
                        state.log_file = Some(metadata.log_file);
                        state.tunnel_client_pid = Some(metadata.tunnel_client_pid);
                        state.local_mcp_url = Some(metadata.local_mcp_url);
                        state.process_ready = true;
                        state.failure_stage = None;
                        state.reason_code = None;
                        log(state, "process_ready");
                    }) {
                        return;
                    }
                    ready_seen = true;
                    last_health = Some(Instant::now());
                }
                Some("health") if ready_seen => {
                    let (Some(tunnel), Some(local)) = (
                        event["tunnel_ready"].as_bool(),
                        event["local_mcp_ready"].as_bool(),
                    ) else {
                        break ConnectionError::ProtocolInvalid;
                    };
                    if event["schema_version"] != 1 {
                        break ConnectionError::ProtocolInvalid;
                    }
                    last_health = Some(Instant::now());
                    let error = if !local {
                        Some(ConnectionError::LocalMcpUnavailable)
                    } else if !tunnel {
                        Some(ConnectionError::TunnelUnavailable)
                    } else {
                        None
                    };
                    if !self.update(id, generation, |state| {
                        if state.ready != (tunnel && local) || state.last_error != error {
                            log(
                                state,
                                if tunnel && local {
                                    "healthy"
                                } else if !local {
                                    "local_mcp_unavailable"
                                } else {
                                    "tunnel_unavailable"
                                },
                            );
                        }
                        state.ready = tunnel && local;
                        state.tunnel_ready = Some(tunnel);
                        state.local_mcp_ready = Some(local);
                        state.health = if state.ready {
                            ConnectionHealth::Healthy
                        } else {
                            ConnectionHealth::Degraded
                        };
                        state.last_error = error;
                        if state.ready {
                            state.failure_stage = None;
                            state.reason_code = None;
                        } else if !local {
                            state.failure_stage = Some("local_mcp".into());
                            state.reason_code = Some("local_mcp_unavailable".into());
                        } else {
                            state.failure_stage = Some("tunnel_health".into());
                            state.reason_code = Some("tunnel_unavailable".into());
                        }
                    }) {
                        return;
                    }
                }
                Some("failure") => {
                    let Some((failure_stage, reason_code)) = safe_failure_evidence(&event) else {
                        break ConnectionError::ProtocolInvalid;
                    };
                    let failure = if reason_code == "local_mcp_unavailable" {
                        ConnectionError::LocalMcpUnavailable
                    } else {
                        ConnectionError::TunnelUnavailable
                    };
                    if !self.update(id, generation, |state| {
                        state.failure_stage = Some(failure_stage.to_string());
                        state.reason_code = Some(reason_code.to_string());
                        log(state, "typed_failure");
                    }) {
                        return;
                    }
                    break failure;
                }
                Some("error" | "failed" | "machine_event_overflow" | "stopped" | "exited") => {
                    break ConnectionError::ProcessExited
                }
                _ => {} // Unknown progress may not reset the absolute startup deadline.
            }
        };
        if self.update(id, generation, |state| {
            state.lifecycle = ConnectionLifecycle::Error;
            state.ready = false;
            state.health = ConnectionHealth::Degraded;
            state.last_error = Some(failure);
            if state.reason_code.is_none() {
                let (stage, reason) = connection_error_evidence(failure);
                state.failure_stage = Some(stage.into());
                state.reason_code = Some(reason.into());
            }
            log(state, "connection_failed");
        }) {
            activity.push_for_profile(Some(id), ActivityEventKind::ProcessObservationFailed, "regular_tunnel", ActivityLevel::Error, "Connection needs attention; other connections and the shared runtime are unchanged");
            let mut processes = supervisor.lock().await;
            processes
                .stop_generation(ProcessKey::RegularTunnel(id), generation)
                .await;
            let cleaned = processes
                .snapshot(ProcessKey::RegularTunnel(id))
                .is_none_or(|process| process.generation != generation);
            drop(processes);
            self.update(id, generation, |state| {
                if cleaned {
                    state.pid = None;
                    state.tunnel_client_pid = None;
                } else {
                    state.last_error = Some(ConnectionError::StopFailed);
                    log(state, "stop_failed");
                }
            });
        }
    }
}

fn safe_failure_evidence(event: &Value) -> Option<(&str, &str)> {
    if event["schema_version"] != 1 || event["provider"] != "openai" {
        return None;
    }
    let stage = event["failure_stage"].as_str()?;
    let reason = event["reason_code"].as_str()?;
    let valid = matches!(
        (stage, reason),
        (
            "tunnel_client_verification",
            "tunnel_client_verification_failed"
        ) | ("tunnel_doctor", "tunnel_doctor_failed")
            | ("tunnel_control_plane", "tunnel_control_plane_unreachable")
            | ("tunnel_control_plane", "tunnel_control_plane_probe_failed")
            | ("tunnel_daemon_start", "tunnel_daemon_start_failed")
            | ("tunnel_daemon_readiness", "tunnel_daemon_not_ready")
            | ("local_mcp", "local_mcp_unavailable")
            | ("tunnel_startup", "tunnel_startup_failed")
    );
    valid.then_some((stage, reason))
}

fn connection_error_evidence(error: ConnectionError) -> (&'static str, &'static str) {
    match error {
        ConnectionError::StartupTimeout => ("tunnel_startup", "tunnel_startup_timeout"),
        ConnectionError::ProcessExited => ("tunnel_process", "tunnel_process_exited"),
        ConnectionError::ProtocolInvalid => ("machine_protocol", "tunnel_protocol_invalid"),
        ConnectionError::HealthStale => ("tunnel_health", "tunnel_health_stale"),
        ConnectionError::TunnelUnavailable => ("tunnel_health", "tunnel_unavailable"),
        ConnectionError::LocalMcpUnavailable => ("local_mcp", "local_mcp_unavailable"),
        ConnectionError::StartFailed => ("tunnel_process", "tunnel_process_start_failed"),
        ConnectionError::StopFailed => ("tunnel_process", "tunnel_process_stop_failed"),
    }
}

fn log(state: &mut ConnectionRuntimeSnapshot, event: &'static str) {
    state.logs.push_back(ConnectionLogEntry {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64,
        event: event.into(),
    });
    while state.logs.len() > 40 {
        state.logs.pop_front();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RuntimeMetadata {
    directory: PathBuf,
    health_url: String,
    log_file: PathBuf,
    tunnel_client_pid: u32,
    local_mcp_url: String,
}
fn runtime_metadata(
    value: &Value,
    expected_root: &Path,
    local_mcp_url: &str,
) -> Option<RuntimeMetadata> {
    let metadata: RuntimeMetadata = serde_json::from_value(value.clone()).ok()?;
    if metadata.directory.parent()? != expected_root
        || !metadata.directory.is_absolute()
        || metadata.log_file != metadata.directory.join("openai-tunnel.log")
        || metadata.local_mcp_url != local_mcp_url
        || metadata.tunnel_client_pid == 0
    {
        return None;
    }
    let name = metadata
        .directory
        .file_name()?
        .to_str()?
        .strip_prefix("openai-")?;
    if name.len() != 32 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let url = url::Url::parse(&metadata.health_url).ok()?;
    if url.scheme() != "http"
        || !matches!(url.host_str()?, "127.0.0.1" | "localhost" | "[::1]" | "::1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return None;
    }
    Some(metadata)
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn safe_failure_evidence_accepts_known_codes_and_ignores_new_optional_fields() {
        let event = json!({
            "event": "failure",
            "schema_version": 1,
            "provider": "openai",
            "failure_stage": "tunnel_control_plane",
            "reason_code": "tunnel_control_plane_probe_failed",
            "future_optional": {"ignored": true}
        });
        assert_eq!(
            safe_failure_evidence(&event),
            Some(("tunnel_control_plane", "tunnel_control_plane_probe_failed"))
        );
    }

    #[test]
    fn safe_failure_evidence_fails_closed_on_malformed_or_unknown_codes() {
        for event in [
            json!({"event":"failure","schema_version":1,"provider":"openai"}),
            json!({"event":"failure","schema_version":2,"provider":"openai","failure_stage":"tunnel_doctor","reason_code":"tunnel_doctor_failed"}),
            json!({"event":"failure","schema_version":1,"provider":"openai","failure_stage":"tunnel_doctor","reason_code":"private_runtime_key_rejected"}),
        ] {
            assert_eq!(safe_failure_evidence(&event), None);
        }
    }
}

#[cfg(test)]
mod tests;
