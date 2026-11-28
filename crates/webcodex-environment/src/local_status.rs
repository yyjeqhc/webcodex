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

fn inspect_probes(probes: Vec<ServiceProbe>) -> Vec<ServiceProbeResult> {
    inspect_probes_with(probes, |requests| {
        ServiceManager::inspect_many(requests)
            .into_iter()
            .map(|result| result.map_err(crate::native::service_error))
            .collect()
    })
}

/// Collect one native snapshot, coalescing identical saved owners (including
/// embedded Tunnels). Results retain input order and each probe's failure.
fn inspect_probes_with(
    probes: Vec<ServiceProbe>,
    observe: impl FnOnce(&[(&crate::service::ServiceSpec, bool)]) -> Vec<ServiceProbeResult>,
) -> Vec<ServiceProbeResult> {
    let mut unique: Vec<(crate::service::ServiceSpec, bool)> = Vec::new();
    let indices: Vec<_> = probes
        .iter()
        .map(|probe| {
            let spec = probe.spec.as_ref().map_err(Clone::clone)?;
            let index = if let Some(index) = unique.iter().position(|(owner, _)| owner == spec) {
                unique[index].1 |= probe.with_pid;
                index
            } else {
                unique.push((spec.clone(), probe.with_pid));
                unique.len() - 1
            };
            Ok(index)
        })
        .collect();
    let requests: Vec<_> = unique
        .iter()
        .map(|(spec, with_pid)| (spec, *with_pid))
        .collect();
    let observed = if requests.is_empty() {
        Vec::new()
    } else {
        observe(&requests)
    };
    indices
        .into_iter()
        .zip(probes)
        .map(|(index, probe)| {
            let index = index?;
            observed
                .get(index)
                .cloned()
                .unwrap_or_else(|| {
                    Err(SetupDiagnostic::new(
                        "local_observation_unavailable",
                        "Native service status could not be observed",
                        "Inspect the service owner before retrying",
                    ))
                })
                .map(|(status, pid)| (status, probe.with_pid.then_some(pid).flatten()))
        })
        .collect()
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

    let observed = inspect_probes(probes);
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

    fn probe_spec(identity: &str) -> crate::service::ServiceSpec {
        crate::service::ServiceSpec {
            scope: ServiceScope::User,
            id: "observation-fixture".into(),
            component: Component::Server,
            program: std::env::temp_dir().join("observation-fixture"),
            args: vec!["--config".into(), "fixture.toml".into()],
            working_directory: std::env::temp_dir(),
            account: crate::service::ServiceAccount::SystemUser {
                name: "fixture".into(),
                group: None,
                expected_identity: "fixture-identity".into(),
                home: None,
            },
            config_identity: identity.into(),
            env_file: None,
            environment: Default::default(),
            linux_socket: None,
        }
    }

    #[test]
    fn one_snapshot_coalesces_identical_owners_but_keeps_pid_intent_and_failures() {
        let owner = probe_spec("first");
        let other = probe_spec("second");
        let plans = vec![
            ServiceProbe {
                spec: Ok(owner.clone()),
                with_pid: false,
            },
            ServiceProbe {
                spec: Err(SetupDiagnostic::new(
                    "invalid_plan",
                    "Invalid fixture",
                    "Inspect",
                )),
                with_pid: true,
            },
            ServiceProbe {
                spec: Ok(other),
                with_pid: true,
            },
            ServiceProbe {
                spec: Ok(owner),
                with_pid: true,
            },
        ];
        let observed = inspect_probes_with(plans, |requests| {
            // Same task name with a different saved identity is never merged.
            assert_eq!(requests.len(), 2);
            assert_eq!(requests[0].0.config_identity, "first");
            assert_eq!(requests[1].0.config_identity, "second");
            assert!(requests[0].1 && requests[1].1);
            vec![
                Ok((
                    ServiceStatus {
                        id: "observation-fixture".into(),
                        ownership: Ownership::Owned,
                        installed: true,
                        enabled: Some(true),
                        running: Some(true),
                        detail: None,
                    },
                    Some(42),
                )),
                Err(SetupDiagnostic::new(
                    "owner_unknown",
                    "Unknown fixture",
                    "Inspect",
                )),
            ]
        });
        assert_eq!(observed.len(), 4);
        assert_eq!(observed[0].as_ref().unwrap().1, None);
        assert_eq!(observed[1].as_ref().unwrap_err().code, "invalid_plan");
        assert_eq!(observed[2].as_ref().unwrap_err().code, "owner_unknown");
        assert_eq!(observed[3].as_ref().unwrap().1, Some(42));
    }

    #[test]
    fn missing_batch_results_never_become_successful_observations() {
        let observed = inspect_probes_with(
            vec![ServiceProbe {
                spec: Ok(probe_spec("missing")),
                with_pid: true,
            }],
            |_| Vec::new(),
        );
        assert_eq!(
            observed[0].as_ref().unwrap_err().code,
            "local_observation_unavailable"
        );
    }

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
        let results = inspect_probes(plans);
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
