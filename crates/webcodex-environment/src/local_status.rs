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
    for component in [
        record.request.local_server().then_some(Component::Server),
        record.request.local_runner().then_some(Component::Runner),
    ]
    .into_iter()
    .flatten()
    {
        let observed = service_spec(store, record, component)
            .and_then(|spec| ServiceManager::inspect(&spec).map_err(crate::native::service_error));
        result
            .components
            .push(observation(component.as_str(), None, observed));
    }
    if record.request.local_server() {
        match crate::tunnel_profiles(store) {
            Ok(profiles) => {
                result.profiles_truncated = profiles.len() > 16;
                for profile in profiles.into_iter().take(16) {
                    let spec = if profile.host_mode == TunnelHostMode::Embedded {
                        service_spec(store, record, Component::Server)
                    } else {
                        crate::tunnel_service_spec(store, record, &profile.profile_id)
                    };
                    let health_path = store
                        .root()
                        .join("server/tunnels")
                        .join(&profile.profile_id)
                        .join("readiness.json");
                    let state = spec.as_ref().map_err(Clone::clone).and_then(|spec| {
                        ServiceManager::inspect(spec).map_err(crate::native::service_error)
                    });
                    let mut row = observation("tunnel", Some(profile.profile_id.clone()), state);
                    row.host_mode = Some(profile.host_mode);
                    let owner_pid = spec
                        .as_ref()
                        .ok()
                        .and_then(|spec| ServiceManager::running_pid(spec).ok().flatten());
                    let health = crate::tunnel::read_tunnel_health(&health_path).ok();
                    let health_status = crate::tunnel::health_status(
                        health.as_ref(),
                        (profile.host_mode == TunnelHostMode::Embedded)
                            .then(|| profile.effective_runtime_revision()),
                        owner_pid,
                    );
                    row.observed_tunnel_state = health.as_ref().and_then(|h| h.state);
                    row.tunnel_ready = Some(
                        health_status == "current"
                            && health.as_ref().is_some_and(|h| h.tunnel_ready),
                    );
                    row.local_mcp_ready = Some(
                        health_status == "current"
                            && health.as_ref().is_some_and(|h| h.local_mcp_ready),
                    );
                    if let Some(code) = health.as_ref().and_then(|h| h.diagnostic.as_deref()) {
                        row.diagnostic = Some(SetupDiagnostic::new(code, "Saved Tunnel owner diagnostic", "Run environment tunnel-diagnose PROFILE --environment-dir PATH; do not clear an uncertain restart fence"));
                    } else if health_status != "current" {
                        row.diagnostic = Some(SetupDiagnostic::new(health_status, "Tunnel readiness is not verified", "Inspect the exact owner PID and profile revision using tunnel-diagnose"));
                    }
                    result.components.push(row);
                }
            }
            Err(error) => result
                .components
                .push(observation("tunnel", None, Err(error))),
        }
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
