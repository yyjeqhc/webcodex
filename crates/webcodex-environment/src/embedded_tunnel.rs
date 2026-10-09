//! Explicit, per-profile bindings for a Server-owned Tunnel. No credential lookup
//! in process environment, no global environment mutation, and no service adoption.
use crate::native::service_error;
use crate::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct EmbeddedTunnelProfile {
    pub profile_id: String,
    pub autostart: bool,
    pub runtime_revision: u64,
    pub credentials: TunnelCredentials,
    pub local_token: Secret,
    pub proxy: Option<Secret>,
    pub readiness_path: PathBuf,
}

fn invalid() -> SetupDiagnostic {
    SetupDiagnostic::new("tunnel_profile_configuration", "Embedded Tunnel configuration is incomplete or ambiguous", "Verify each private profile file independently; inherited process credentials are never used")
}

/// Load only explicitly embedded records from an operator-selected Environment.
/// Standalone profiles and existing installations are not migrated implicitly.
pub fn embedded_tunnel_profiles(root: &Path) -> SetupResultValue<Vec<EmbeddedTunnelProfile>> {
    let store = EnvironmentStore::open(root.to_path_buf())?;
    let profiles = tunnel_profiles(&store)?;
    crate::tunnel::validate_catalog(&profiles)?;
    let mut identities = BTreeSet::new();
    let mut output = Vec::new();
    for profile in profiles {
        crate::cloudflare_tunnel::ensure_cloudflare_profile_available(&store, &profile.profile_id)?;
        if profile.provider != TunnelProvider::Openai {
            continue;
        }
        let directory = root.join("server/tunnels").join(&profile.profile_id);
        let binding_path = directory.join("webcodex.env");
        let binding_present = match std::fs::symlink_metadata(&binding_path) {
            Ok(_) => true,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => return Err(invalid()),
        };
        // Old standalone catalog records could predate per-profile bindings and
        // remain irrelevant to Server startup. Any binding that does exist is
        // authoritative, however, so its Tunnel identity participates in the
        // same duplicate fence as every Server-owned profile.
        if profile.host_mode != TunnelHostMode::Embedded && !binding_present {
            continue;
        }
        let binding = crate::tunnel::tunnel_profile_binding(&store, &profile.profile_id)?;
        if !identities.insert(binding.tunnel_id.expose().to_owned()) {
            return Err(invalid());
        }
        if profile.host_mode != TunnelHostMode::Embedded {
            continue;
        }
        let runtime_revision = profile.effective_runtime_revision();
        output.push(EmbeddedTunnelProfile {
            profile_id: profile.profile_id,
            autostart: profile.autostart,
            runtime_revision,
            credentials: TunnelCredentials {
                tunnel_id: binding.tunnel_id,
                api_key: binding.api_key,
            },
            local_token: binding.local_token,
            proxy: binding.proxy,
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
        crate::tunnel::validate_catalog(&profiles)?;
        // Validate every private binding and Tunnel identity before an ownership
        // transfer. A pre-existing ambiguity must never become Server-owned.
        let _ = tunnel_profile_snapshots(store)?;
        let profile_index = profiles
            .iter()
            .position(|profile| profile.profile_id == profile_id)
            .ok_or_else(invalid)?;
        if profiles[profile_index].provider != TunnelProvider::Openai {
            crate::cloudflare_tunnel::ensure_cloudflare_profile_available(store, profile_id)?;
        }
        if profiles[profile_index].host_mode == mode {
            return Ok(());
        }
        if mode == TunnelHostMode::Embedded
            && profiles
                .iter()
                .filter(|profile| profile.host_mode == TunnelHostMode::Embedded)
                .count()
                >= 16
        {
            return Err(SetupDiagnostic::new(
                "tunnel_profile_capacity",
                "At most 16 Server-owned Tunnel profiles are supported",
                "Keep this profile standalone or remove another Server-owned profile first",
            ));
        }
        let profile = &mut profiles[profile_index];
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
            crate::tunnel::ensure_server_tunnel_environment(store)?;
        }
        let previous_runtime_revision = profile.effective_runtime_revision();
        profile.host_mode = mode;
        if mode == TunnelHostMode::Standalone {
            // A standalone profile returns to the historical managed-service
            // lifecycle; only Server-owned profiles have selectable autostart.
            profile.autostart = true;
        }
        profile.revision = crate::tunnel::next_revision(profile.revision)?;
        profile.runtime_revision = crate::tunnel::next_revision(previous_runtime_revision)?;
        profile.installed = false;
        profile.started = false;
        crate::tunnel::validate_catalog(&profiles)?;
        store.write_json("tunnel.json", &profiles)
    }
}

#[cfg(test)]
mod tests;
