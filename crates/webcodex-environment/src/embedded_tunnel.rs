//! Explicit, per-profile bindings for a Server-owned Tunnel. No credential lookup
//! in process environment, no global environment mutation, and no service adoption.
use crate::native::{env_value, service_error};
use crate::storage::{atomic_private_write, read_private};
use crate::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct EmbeddedTunnelProfile {
    pub profile_id: String,
    pub credentials: TunnelCredentials,
    pub local_token: Secret,
    pub proxy: Option<Secret>,
    pub readiness_path: PathBuf,
}

fn invalid() -> SetupDiagnostic {
    SetupDiagnostic::new("tunnel_profile_configuration", "Embedded Tunnel configuration is incomplete or ambiguous", "Verify each private profile file independently; inherited process credentials are never used")
}

fn unique_value(content: &str, key: &str) -> SetupResultValue<Option<String>> {
    if content
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| name.trim() == key)
        .count()
        > 1
    {
        return Err(invalid());
    }
    Ok(env_value(content, key))
}

/// Load only explicitly embedded records from an operator-selected Environment.
/// Standalone profiles and existing installations are not migrated implicitly.
pub fn embedded_tunnel_profiles(root: &Path) -> SetupResultValue<Vec<EmbeddedTunnelProfile>> {
    let store = EnvironmentStore::open(root.to_path_buf())?;
    let profiles = tunnel_profiles(&store)?;
    if profiles.len() > 64 {
        return Err(invalid());
    }
    let mut names = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut output = Vec::new();
    for profile in profiles {
        crate::tunnel::validate_id(&profile.profile_id)?;
        if !names.insert(profile.profile_id.clone()) {
            return Err(invalid());
        }
        if profile.host_mode != TunnelHostMode::Embedded {
            continue;
        }
        if output.len() == 16 || profile.installed || profile.started {
            return Err(invalid());
        }
        let directory = root.join("server/tunnels").join(&profile.profile_id);
        let bytes = read_private(&directory.join("webcodex.env"))?;
        if bytes.len() > 32768 {
            return Err(invalid());
        }
        let content = std::str::from_utf8(&bytes).map_err(|_| invalid())?;
        let required = |key| {
            unique_value(content, key)?
                .filter(|value| !value.is_empty())
                .ok_or_else(invalid)
        };
        if required("WEBCODEX_TUNNEL_PROFILE_ID")? != profile.profile_id {
            return Err(invalid());
        }
        let tunnel_id = required("CONTROL_PLANE_TUNNEL_ID")?;
        if !identities.insert(tunnel_id.clone()) {
            return Err(invalid());
        }
        output.push(EmbeddedTunnelProfile {
            profile_id: profile.profile_id,
            credentials: TunnelCredentials {
                tunnel_id: Secret::new(tunnel_id),
                api_key: Secret::new(required("CONTROL_PLANE_API_KEY")?),
            },
            local_token: Secret::new(required("WEBCODEX_TOKEN")?),
            proxy: unique_value(content, "WEBCODEX_TUNNEL_PROXY")?.map(Secret::new),
            readiness_path: directory.join("readiness.json"),
        });
    }
    Ok(output)
}

impl NativeEnvironment {
    /// Explicit host-mode change. Lifecycle handoff requires the previous owner to
    /// be stopped and the standalone boot service to be uninstalled. This never
    /// clears a Tunnel run marker or silently restarts the Server.
    pub fn set_tunnel_host(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        mode: TunnelHostMode,
    ) -> SetupResultValue<()> {
        crate::tunnel::validate_id(profile_id)?;
        let _lock = store.lock()?;
        ensure_upgrade_idle_under_lock(store)?;
        let record = store.load_environment()?.ok_or_else(invalid)?;
        if !record.request.local_server() {
            return Err(invalid());
        }
        // System Windows credential ACL handoff needs its own installer operation.
        // Never broaden another service account's credential permissions here.
        if cfg!(windows)
            && record.request.service_scope.is_system()
            && mode == TunnelHostMode::Embedded
        {
            return Err(SetupDiagnostic::new(
                "tunnel_host_unsupported",
                "Embedded Tunnel setup currently requires a user-owned Server on Windows",
                "Keep the existing standalone managed service",
            ));
        }
        let mut profiles = tunnel_profiles(store)?;
        let profile = profiles
            .iter_mut()
            .find(|p| p.profile_id == profile_id)
            .ok_or_else(invalid)?;
        if profile.host_mode == mode {
            return Ok(());
        }
        if profile.host_mode == TunnelHostMode::Embedded {
            let server = service::ServiceManager::inspect(&service_spec(
                store,
                &record,
                service::Component::Server,
            )?)
            .map_err(service_error)?;
            if server.ownership != service::Ownership::Owned || server.running != Some(false) {
                return Err(SetupDiagnostic::new(
                    "tunnel_host_busy",
                    "Stop the owning Server before changing an embedded Tunnel host",
                    "Observe a clean Server stop; never clear an uncertain run marker",
                ));
            }
        }
        let standalone =
            service::ServiceManager::inspect(&tunnel_service_spec(store, &record, profile_id)?)
                .map_err(service_error)?;
        if standalone.ownership != service::Ownership::Absent {
            return Err(SetupDiagnostic::new("tunnel_host_busy", "Uninstall the standalone Tunnel service before changing its host", "Stop it cleanly and uninstall only the exact named Tunnel service; credentials are retained"));
        }
        if mode == TunnelHostMode::Embedded {
            let path = store.root().join("server/webcodex.env");
            let source = read_secret(&path)?;
            let root = store.root().to_str().ok_or_else(invalid)?;
            if root.contains(['\n', '\r', '"']) {
                return Err(invalid());
            }
            let existing = unique_value(source.expose(), "WEBCODEX_TUNNEL_ENVIRONMENT")?;
            if existing.as_deref().is_some_and(|value| value != root) {
                return Err(invalid());
            }
            if existing.is_none() {
                let content = format!(
                    "{}\nWEBCODEX_TUNNEL_ENVIRONMENT=\"{root}\"\n",
                    source.expose()
                );
                atomic_private_write(&path, content.as_bytes())?;
            }
        }
        profile.host_mode = mode;
        profile.installed = false;
        profile.started = false;
        store.write_json("tunnel.json", &profiles)
    }
}

#[cfg(test)]
mod tests;
