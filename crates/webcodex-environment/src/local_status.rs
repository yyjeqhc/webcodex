//! Read-only native observations for one saved environment. No credential
//! issuance, pairing, service creation or inferred adoption occurs during status.
use crate::service::{Component, Ownership, ServiceManager, ServiceScope, ServiceStatus};
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentObservation {
    pub component: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_mode: Option<TunnelHostMode>,
    pub profile: Option<String>,
    pub service: Option<ServiceStatus>,
    pub diagnostic: Option<SetupDiagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_tunnel_state: Option<TunnelState>,
    pub tunnel_ready: Option<bool>,
    pub local_mcp_ready: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalEnvironmentStatus {
    pub scope: ServiceScope,
    pub lifecycle: String,
    /// None means unavailable/not applicable, not disabled. Never changed here.
    pub linger_enabled: Option<bool>,
    pub components: Vec<ComponentObservation>,
    pub profiles_truncated: bool,
    /// Saved file state only; the separate authenticated field proves API use.
    pub user_credential_file: String,
    /// None for joined remote Servers: their external exposure is not probed.
    pub local_connection_ready: Option<bool>,
    pub client_connection: String,
}

struct ServiceProbe {
    spec: SetupResultValue<crate::service::ServiceSpec>,
    with_pid: bool,
}

type ServiceProbeResult = SetupResultValue<(ServiceStatus, Option<u32>)>;

fn inspect_probe(probe: ServiceProbe) -> ServiceProbeResult {
    let spec = probe.spec?;
    if probe.with_pid {
        ServiceManager::inspect_with_pid(&spec).map_err(crate::native::service_error)
    } else {
        ServiceManager::inspect(&spec)
            .map(|status| (status, None))
            .map_err(crate::native::service_error)
    }
}

/// Task Scheduler inspection starts a PowerShell process. Keep independent
/// observations concurrent but bounded; the source order remains stable, and a
/// failing query cannot turn an unknown owner into a successful observation.
fn inspect_probes(probes: Vec<ServiceProbe>, windows_user: bool) -> Vec<ServiceProbeResult> {
    if !windows_user {
        return probes.into_iter().map(inspect_probe).collect();
    }

    const MAX_CONCURRENT_PROBES: usize = 3;
    let mut observations = Vec::with_capacity(probes.len());
    let mut remaining = probes.into_iter();
    loop {
        let group: Vec<_> = remaining.by_ref().take(MAX_CONCURRENT_PROBES).collect();
        if group.is_empty() {
            break;
        }
        std::thread::scope(|scope| {
            let tasks: Vec<_> = group
                .into_iter()
                .map(|probe| scope.spawn(move || inspect_probe(probe)))
                .collect();
            for task in tasks {
                observations.push(task.join().unwrap_or_else(|_| {
                    Err(SetupDiagnostic::new(
                        "local_observation_unavailable",
                        "Native service status could not be observed",
                        "Inspect the service owner before retrying",
                    ))
                }));
            }
        });
    }
    observations
}

pub(crate) fn collect(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    api_ready: bool,
    runner_online: Option<bool>,
) -> LocalEnvironmentStatus {
    let mut result = LocalEnvironmentStatus {
        scope: record.request.service_scope,
        lifecycle: record.request.service_scope.lifecycle().into(),
        linger_enabled: linger(record),
        components: Vec::new(),
        profiles_truncated: false,
        user_credential_file: match crate::read_secret(&store.root().join("webcodex-user-token")) {
            Ok(value) if !value.expose().is_empty() => "configured",
            _ if !store.root().join("webcodex-user-token").exists() => "missing",
            _ => "unreadable_or_empty",
        }
        .into(),
        local_connection_ready: None,
        client_connection: "not_observed".into(),
    };

    let tunnel_profiles = record
        .request
        .local_server()
        .then(|| crate::tunnel_profiles(store));
    let embedded_owner = tunnel_profiles
        .as_ref()
        .and_then(|profiles| profiles.as_ref().ok())
        .is_some_and(|profiles| {
            profiles
                .iter()
                .take(16)
                .any(|profile| profile.host_mode == TunnelHostMode::Embedded)
        });

    let mut probes = Vec::new();
    let server_index = record.request.local_server().then(|| {
        let index = probes.len();
        probes.push(ServiceProbe {
            spec: service_spec(store, record, Component::Server),
            with_pid: embedded_owner,
        });
        index
    });
    let runner_index = record.request.local_runner().then(|| {
        let index = probes.len();
        probes.push(ServiceProbe {
            spec: service_spec(store, record, Component::Runner),
            with_pid: false,
        });
        index
    });

    // Embedded Tunnel profiles share the exact Server owner. Its status and
    // verified PID can be observed once and reused for each readiness marker.
    let mut tunnel_indices = Vec::new();
    if let Some(Ok(profiles)) = &tunnel_profiles {
        for profile in profiles.iter().take(16) {
            if profile.host_mode == TunnelHostMode::Embedded {
                tunnel_indices.push(server_index.expect("embedded Tunnel requires local Server"));
            } else {
                let index = probes.len();
                probes.push(ServiceProbe {
                    spec: crate::tunnel_service_spec(store, record, &profile.profile_id),
                    with_pid: true,
                });
                tunnel_indices.push(index);
            }
        }
    }

    let observed = inspect_probes(
        probes,
        cfg!(windows) && record.request.service_scope == crate::service::ServiceScope::User,
    );
    if let Some(index) = server_index {
        result.components.push(observation(
            Component::Server.as_str(),
            None,
            observed[index].clone().map(|(status, _)| status),
        ));
    }
    if let Some(index) = runner_index {
        result.components.push(observation(
            Component::Runner.as_str(),
            None,
            observed[index].clone().map(|(status, _)| status),
        ));
    }

    match tunnel_profiles {
        Some(Ok(profiles)) => {
            result.profiles_truncated = profiles.len() > 16;
            for (profile, index) in profiles.into_iter().take(16).zip(tunnel_indices) {
                let sampled = &observed[index];
                let mut row = observation(
                    "tunnel",
                    Some(profile.profile_id.clone()),
                    sampled.clone().map(|(status, _)| status),
                );
                row.host_mode = Some(profile.host_mode);
                let owner_pid = sampled.as_ref().ok().and_then(|(_, pid)| *pid);
                let health_path = store
                    .root()
                    .join("server/tunnels")
                    .join(&profile.profile_id)
                    .join("readiness.json");
                let health = crate::tunnel::read_tunnel_health(&health_path).ok();
                let health_status = crate::tunnel::health_status(
                    health.as_ref(),
                    (profile.host_mode == TunnelHostMode::Embedded)
                        .then(|| profile.effective_runtime_revision()),
                    owner_pid,
                );
                row.observed_tunnel_state = health.as_ref().and_then(|h| h.state);
                row.tunnel_ready = Some(
                    health_status == "current" && health.as_ref().is_some_and(|h| h.tunnel_ready),
                );
                row.local_mcp_ready = Some(
                    health_status == "current"
                        && health.as_ref().is_some_and(|h| h.local_mcp_ready),
                );
                if let Some(code) = health.as_ref().and_then(|h| h.diagnostic.as_deref()) {
                    row.diagnostic = Some(SetupDiagnostic::new(
                        code,
                        "Saved Tunnel owner diagnostic",
                        "Run environment tunnel-diagnose PROFILE --environment-dir PATH; do not clear an uncertain restart fence",
                    ));
                } else if health_status != "current" {
                    row.diagnostic = Some(SetupDiagnostic::new(
                        health_status,
                        "Tunnel readiness is not verified",
                        "Inspect the exact owner PID and profile revision using tunnel-diagnose",
                    ));
                }
                result.components.push(row);
            }
        }
        Some(Err(error)) => {
            result
                .components
                .push(observation("tunnel", None, Err(error)));
        }
        None => {}
    }
    result.local_connection_ready = record.request.local_server().then(|| {
        connection_ready(
            &result.components,
            api_ready,
            record.request.local_runner(),
            runner_online,
        )
    });
    result
}
fn observation(
    component: &str,
    profile: Option<String>,
    result: SetupResultValue<ServiceStatus>,
) -> ComponentObservation {
    let (service, diagnostic) = match result {
        Ok(value) => (Some(value), None),
        Err(error) => (None, Some(error)),
    };
    ComponentObservation {
        component: component.into(),
        host_mode: None,
        profile,
        service,
        diagnostic,
        observed_tunnel_state: None,
        tunnel_ready: None,
        local_mcp_ready: None,
    }
}
fn running_owned(state: &ServiceStatus) -> bool {
    state.ownership == Ownership::Owned && state.running == Some(true)
}
fn connection_ready(
    components: &[ComponentObservation],
    api_ready: bool,
    wants_runner: bool,
    runner_online: Option<bool>,
) -> bool {
    let has_running = |name| {
        components
            .iter()
            .any(|row| row.component == name && row.service.as_ref().is_some_and(running_owned))
    };
    api_ready
        && has_running("server")
        && (!wants_runner || runner_online == Some(true) && has_running("runner"))
        && components
            .iter()
            .filter(|row| row.component != "tunnel")
            .all(|row| row.service.as_ref().is_some_and(running_owned))
        && components.iter().any(|row| {
            row.component == "tunnel"
                && row.service.as_ref().is_some_and(running_owned)
                && row.tunnel_ready == Some(true)
                && row.local_mcp_ready == Some(true)
        })
}
fn linger(record: &EnvironmentRecord) -> Option<bool> {
    #[cfg(target_os = "linux")]
    if record.request.service_scope == ServiceScope::User {
        use crate::process::CommandOutputExt;
        let output = std::process::Command::new("/usr/bin/loginctl")
            .args([
                "show-user",
                &record.request.account.identity,
                "--property=Linger",
                "--value",
            ])
            .output_with_limits(std::time::Duration::from_secs(5), 256)
            .ok()?;
        if output.status.success() {
            return match std::str::from_utf8(&output.stdout).ok()?.trim() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            };
        }
    }
    let _ = record;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_service_probes_preserve_order_and_fail_closed() {
        let codes = [
            "probe_first",
            "probe_second",
            "probe_third",
            "probe_fourth",
            "probe_fifth",
        ];
        let plans = codes
            .iter()
            .map(|code| ServiceProbe {
                spec: Err(SetupDiagnostic::new(
                    code,
                    "Synthetic service failure",
                    "Inspect owner",
                )),
                with_pid: true,
            })
            .collect();
        let results = inspect_probes(plans, true);
        assert_eq!(results.len(), codes.len());
        for (actual, code) in results.into_iter().zip(codes) {
            assert_eq!(actual.err().expect("probe must fail").code, code);
        }
    }

    #[test]
    fn readiness_requires_owned_services_fresh_transport_and_runtime_authentication() {
        let service = ServiceStatus {
            id: "fixture".into(),
            ownership: Ownership::Owned,
            installed: true,
            enabled: Some(true),
            running: Some(true),
            detail: None,
        };
        let server = observation("server", None, Ok(service.clone()));
        let runner = observation("runner", None, Ok(service.clone()));
        let mut tunnel = observation("tunnel", Some("default".into()), Ok(service));
        tunnel.tunnel_ready = Some(true);
        tunnel.local_mcp_ready = Some(true);
        let mut rows = vec![server, runner, tunnel];
        assert!(!connection_ready(&rows[2..], true, false, None));
        assert!(!connection_ready(
            &[rows[0].clone(), rows[2].clone()],
            true,
            true,
            Some(true)
        ));
        assert!(connection_ready(&rows, true, true, Some(true)));
        assert!(!connection_ready(&rows, false, true, Some(true)));
        assert!(!connection_ready(&rows, true, true, None));
        rows[2].local_mcp_ready = None;
        assert!(!connection_ready(&rows, true, true, Some(true)));
        rows[2].local_mcp_ready = Some(true);
        rows[0].service.as_mut().unwrap().ownership = Ownership::Foreign;
        assert!(!connection_ready(&rows, true, true, Some(true)));
        assert!(!connection_ready(&[], true, false, None));
    }
}
