//! Persistent Tunnel profiles retain the existing OpenAI connection identity.
//! Runner machines never import or acquire these credentials.
use crate::native::{bootstrap_token, env_value, service_error};
use crate::service::{Component, Ownership, ServiceAccount, ServiceManager, ServiceSpec};
use crate::storage::{atomic_private_write, ensure_private_directory};
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct TunnelCredentials {
    pub tunnel_id: Secret,
    pub api_key: Secret,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRecord {
    pub profile_id: String,
    pub installed: bool,
    pub started: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRuntimeObservation {
    pub service_status: service::ServiceStatus,
    pub ready: bool,
    pub tunnel_ready: bool,
    pub local_mcp_ready: bool,
}
fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Inspect the saved Tunnel profile and its system service; keep its original Tunnel ID and API credential")
}
fn validate_id(id: &str) -> SetupResultValue<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(diagnostic(
            "tunnel_profile",
            "Tunnel profile identifier is invalid",
        ));
    }
    Ok(())
}
pub fn tunnel_service_spec(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    profile_id: &str,
) -> SetupResultValue<ServiceSpec> {
    validate_id(profile_id)?;
    if !record.request.local_server() {
        return Err(diagnostic(
            "tunnel_local_server",
            "A Tunnel can only be hosted by the Server machine",
        ));
    }
    let id = if cfg!(windows) {
        format!("WebCodexTunnel-{profile_id}")
    } else {
        format!("webcodex-tunnel-{profile_id}")
    };
    let directory = store.root().join("server/tunnels").join(profile_id);
    let env_file = directory.join("webcodex.env");
    let mut args = vec![
        "server".into(),
        "tunnel".into(),
        "--env-file".into(),
        env_file.to_string_lossy().into_owned(),
        "--provider".into(),
        "openai".into(),
        "--json".into(),
    ];
    if cfg!(windows) {
        args.splice(0..0, ["--windows-service".into(), id.clone()]);
    }
    let account = if cfg!(windows) {
        ServiceAccount::WindowsVirtual {
            name: format!("NT SERVICE\\{id}"),
        }
    } else {
        ServiceAccount::SystemUser {
            name: record.request.account.name.clone(),
            group: None,
            expected_identity: record.request.account.identity.clone(),
            home: Some(record.request.account.home.clone()),
        }
    };
    let environment = if cfg!(target_os = "macos") {
        BTreeMap::from([(
            "WEBCODEX_SERVICE_LOG_DIR".into(),
            directory.to_string_lossy().into_owned(),
        )])
    } else {
        BTreeMap::new()
    };
    Ok(ServiceSpec {
        id,
        component: Component::Tunnel,
        program: record.request.binaries.cli.clone(),
        args,
        working_directory: directory,
        account,
        config_identity: format!("{}-tunnel-{profile_id}", record.environment_id),
        env_file: Some(env_file),
        environment,
        linux_socket: None,
    })
}
pub fn tunnel_profiles(store: &EnvironmentStore) -> SetupResultValue<Vec<TunnelRecord>> {
    store
        .read_json("tunnel.json")
        .map(|value| value.unwrap_or_default())
}
impl NativeEnvironment {
    pub async fn configure_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        credentials: Option<&TunnelCredentials>,
    ) -> SetupResultValue<service::ServiceStatus> {
        let lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        self.configure_tunnel_under_lock(store, &lock, profile_id, credentials, true)
            .await
    }
    pub(crate) async fn configure_tunnel_under_lock(
        &self,
        store: &EnvironmentStore,
        _lock: &crate::storage::EnvironmentLock,
        profile_id: &str,
        credentials: Option<&TunnelCredentials>,
        start: bool,
    ) -> SetupResultValue<service::ServiceStatus> {
        let record = store
            .load_environment()?
            .or_else(|| {
                store
                    .load_journal()
                    .ok()
                    .flatten()
                    .map(|journal| journal.environment)
            })
            .ok_or_else(|| {
                diagnostic("not_configured", "Configure the Server environment first")
            })?;
        let spec = tunnel_service_spec(store, &record, profile_id)?;
        ServiceManager::preflight(&spec).map_err(service_error)?;
        let path = spec.env_file.as_ref().unwrap();
        if !path.exists() {
            let credentials = credentials.ok_or_else(|| {
                diagnostic(
                    "tunnel_credentials",
                    "The original Tunnel credentials are required for the first configuration",
                )
            })?;
            let id = credentials.tunnel_id.expose();
            let api = credentials.api_key.expose();
            if !id.strip_prefix("tunnel_").is_some_and(|suffix| {
                suffix.len() == 32
                    && suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            }) || api.is_empty()
                || api.len() > 8192
                || api.contains(['\n', '\r', '\0', '"', '\''])
            {
                return Err(diagnostic(
                    "tunnel_credentials",
                    "Tunnel credentials are invalid",
                ));
            }
            ensure_private_directory(&spec.working_directory)?;
            let server = read_secret(&store.root().join("server/webcodex.env"))?;
            let address = env_value(server.expose(), "WEBCODEX_ADDR").ok_or_else(|| {
                diagnostic(
                    "server_configuration",
                    "Server listening address is unavailable",
                )
            })?;
            let content = Secret::new(format!("WEBCODEX_ADDR={address}\nWEBCODEX_TOKEN={}\nCONTROL_PLANE_TUNNEL_ID={id}\nCONTROL_PLANE_API_KEY={api}\nWEBCODEX_TUNNEL_PROFILE_ID={profile_id}\n", bootstrap_token(store)?.expose()));
            atomic_private_write(path, content.expose().as_bytes())?;
        } else if let Some(credentials) = credentials {
            let saved = read_secret(path)?;
            if env_value(saved.expose(), "CONTROL_PLANE_TUNNEL_ID").as_deref()
                != Some(credentials.tunnel_id.expose())
                || env_value(saved.expose(), "CONTROL_PLANE_API_KEY").as_deref()
                    != Some(credentials.api_key.expose())
            {
                return Err(diagnostic(
                    "tunnel_binding_conflict",
                    "This profile already contains another Tunnel identity or credential",
                ));
            }
        }
        let health = spec.working_directory.join("readiness.json");
        if !health.exists() {
            atomic_private_write(&health, b"{}")?;
        }
        let mut profiles = tunnel_profiles(store)?;
        if !profiles
            .iter()
            .any(|profile| profile.profile_id == profile_id)
        {
            profiles.push(TunnelRecord {
                profile_id: profile_id.into(),
                installed: false,
                started: false,
            });
        }
        store.write_json("tunnel.json", &profiles)?;
        // A saved inactive profile has no enabled boot service. Installation
        // occurs only when the user explicitly starts this profile.
        if !start {
            return ServiceManager::inspect(&spec).map_err(service_error);
        }
        let current = ServiceManager::inspect(&spec).map_err(service_error)?;
        // Reconfiguring an already running profile must not re-enter Install:
        // on Windows that path grants the virtual account access to private
        // state and is deliberately invalid against a live service.
        if tunnel_install_required(&current)? {
            crate::privilege::service_operation_spec(
                store,
                &record,
                spec.clone(),
                ServiceOperation::Install,
                None,
            )
            .await?;
        }
        profiles
            .iter_mut()
            .find(|profile| profile.profile_id == profile_id)
            .unwrap()
            .installed = true;
        store.write_json("tunnel.json", &profiles)?;
        if ServiceManager::inspect(&spec)
            .map_err(service_error)?
            .running
            != Some(true)
        {
            write_tunnel_health(&health, false, false)?;
        }
        let status = crate::privilege::service_operation_spec(
            store,
            &record,
            spec.clone(),
            ServiceOperation::Start,
            None,
        )
        .await?;
        wait_tunnel_readiness(&spec).await?;
        profiles
            .iter_mut()
            .find(|profile| profile.profile_id == profile_id)
            .unwrap()
            .started = status.running == Some(true);
        store.write_json("tunnel.json", &profiles)?;
        Ok(status)
    }
    pub fn tunnel_status(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
    ) -> SetupResultValue<TunnelRuntimeObservation> {
        let record = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "Configure this environment first"))?;
        let spec = tunnel_service_spec(store, &record, profile_id)?;
        let service_status = ServiceManager::inspect(&spec).map_err(service_error)?;
        let (tunnel_ready, local_mcp_ready) = if service_status.ownership == Ownership::Owned
            && service_status.running == Some(true)
        {
            read_health(&spec.working_directory.join("readiness.json")).unwrap_or((false, false))
        } else {
            (false, false)
        };
        Ok(TunnelRuntimeObservation {
            service_status,
            ready: tunnel_ready && local_mcp_ready,
            tunnel_ready,
            local_mcp_ready,
        })
    }
    pub async fn remove_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
    ) -> SetupResultValue<()> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let record = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "Configure this environment first"))?;
        let spec = tunnel_service_spec(store, &record, profile_id)?;
        let status = ServiceManager::inspect(&spec).map_err(service_error)?;
        match status.ownership {
            Ownership::Owned => {
                crate::privilege::service_operation_spec(
                    store,
                    &record,
                    spec.clone(),
                    ServiceOperation::Stop,
                    None,
                )
                .await?;
                let removed = crate::privilege::service_operation_spec(
                    store,
                    &record,
                    spec.clone(),
                    ServiceOperation::Uninstall,
                    None,
                )
                .await?;
                if removed.ownership != Ownership::Absent {
                    return Err(diagnostic(
                        "tunnel_remove_uncertain",
                        "Tunnel service removal has not been confirmed",
                    ));
                }
            }
            Ownership::Absent => {}
            _ => {
                return Err(diagnostic(
                    "tunnel_owner",
                    "The saved Tunnel service owner cannot be verified",
                ))
            }
        }
        // Keep runtime logs for diagnosis. Remove only the exact saved credential
        // after the service is absent, so no live process loses its unique key.
        let env = spec.env_file.as_ref().unwrap();
        if env.exists() {
            std::fs::remove_file(env).map_err(|_| SetupDiagnostic::io())?;
        }
        let health = spec.working_directory.join("readiness.json");
        if health.exists() {
            std::fs::remove_file(health).map_err(|_| SetupDiagnostic::io())?;
        }
        let mut profiles = tunnel_profiles(store)?;
        profiles.retain(|profile| profile.profile_id != profile_id);
        store.write_json("tunnel.json", &profiles)
    }
    pub async fn control_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        operation: ServiceOperation,
    ) -> SetupResultValue<service::ServiceStatus> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let record = store.load_environment()?.ok_or_else(|| {
            diagnostic("not_configured", "Configure the Server environment first")
        })?;
        let spec = tunnel_service_spec(store, &record, profile_id)?;
        let status = ServiceManager::inspect(&spec).map_err(service_error)?;
        if status.ownership != Ownership::Owned {
            return Err(diagnostic(
                "tunnel_owner",
                "This Tunnel service is not owned by the saved profile",
            ));
        }
        if operation == ServiceOperation::Restart
            || operation == ServiceOperation::Start && status.running != Some(true)
        {
            write_tunnel_health(&spec.working_directory.join("readiness.json"), false, false)?;
        }
        let result =
            crate::privilege::service_operation_spec(store, &record, spec.clone(), operation, None)
                .await?;
        if matches!(
            operation,
            ServiceOperation::Start | ServiceOperation::Restart
        ) {
            wait_tunnel_readiness(&spec).await?;
        }
        Ok(result)
    }
}

fn tunnel_install_required(status: &service::ServiceStatus) -> SetupResultValue<bool> {
    match status.ownership {
        Ownership::Absent => Ok(true),
        Ownership::Owned if status.enabled == Some(true) => Ok(false),
        Ownership::Owned if status.enabled == Some(false) && status.running == Some(false) => Ok(true),
        Ownership::Owned => Err(diagnostic("tunnel_service_state_uncertain", "Tunnel boot state cannot be changed safely while the service is running or its state is unknown")),
        Ownership::Foreign => Err(diagnostic("tunnel_owner", "A different owner already controls this Tunnel service")),
        Ownership::Unknown => Err(diagnostic("tunnel_owner", "Tunnel service ownership cannot be verified")),
    }
}

/// Service heartbeat contains no Tunnel ID, key or authorization header. This
/// precreated file keeps the installer-selected ACL when a virtual account writes.
pub fn write_tunnel_health(
    path: &std::path::Path,
    tunnel_ready: bool,
    local_mcp_ready: bool,
) -> SetupResultValue<()> {
    use std::io::Write;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(windows)]
    crate::runtime_entry::validate_windows_env_acl(path).map_err(|_| SetupDiagnostic::io())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::OpenOptionsExt;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(SetupDiagnostic::io());
        }
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path).map_err(|_| SetupDiagnostic::io())?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| SetupDiagnostic::io())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| SetupDiagnostic::io())?
        .as_millis();
    let value = serde_json::json!({"schema_version":1,"observed_at_ms":now,"service_pid":std::process::id(),"tunnel_ready":tunnel_ready,"local_mcp_ready":local_mcp_ready});
    file.set_len(0)
        .and_then(|_| file.write_all(value.to_string().as_bytes()))
        .and_then(|_| file.sync_all())
        .map_err(|_| SetupDiagnostic::io())
}

pub(crate) async fn wait_tunnel_readiness(spec: &ServiceSpec) -> SetupResultValue<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(65);
    let path = spec.working_directory.join("readiness.json");
    loop {
        let status = ServiceManager::inspect(spec).map_err(service_error)?;
        if status.ownership != Ownership::Owned {
            return Err(diagnostic(
                "tunnel_owner",
                "Tunnel service ownership changed while checking readiness",
            ));
        }
        if status.running == Some(true) {
            let value = read_health(&path);
            if value.is_ok_and(|(tunnel, mcp)| tunnel && mcp) {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(diagnostic("tunnel_not_ready", "The Tunnel service is configured but Tunnel and local MCP readiness have not both been verified"));
        }
        tokio::time::sleep_until(
            (tokio::time::Instant::now() + std::time::Duration::from_millis(250)).min(deadline),
        )
        .await;
    }
}
fn read_health(path: &std::path::Path) -> SetupResultValue<(bool, bool)> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_file() || meta.is_symlink() || meta.len() > 4096 {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(windows)]
    crate::runtime_entry::validate_windows_env_acl(path).map_err(|_| SetupDiagnostic::io())?;
    let file = std::fs::File::open(path).map_err(|_| SetupDiagnostic::io())?;
    fs2::FileExt::try_lock_shared(&file).map_err(|_| SetupDiagnostic::io())?;
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| SetupDiagnostic::io())?
        .as_millis();
    let time = value
        .get("observed_at_ms")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u128;
    let fresh = time <= now && now.saturating_sub(time) < 7000;
    Ok((
        fresh
            && value
                .get("tunnel_ready")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        fresh
            && value
                .get("local_mcp_ready")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_owned_tunnel_skips_reinstall_and_ambiguous_state_fails_closed() {
        let mut status = service::ServiceStatus {
            id: "webcodex-tunnel-main".into(),
            ownership: Ownership::Owned,
            installed: true,
            enabled: Some(true),
            running: Some(true),
            detail: None,
        };
        assert_eq!(tunnel_install_required(&status).unwrap(), false);
        status.running = Some(false);
        status.enabled = Some(false);
        assert_eq!(tunnel_install_required(&status).unwrap(), true);
        status.running = Some(true);
        assert_eq!(
            tunnel_install_required(&status).unwrap_err().code,
            "tunnel_service_state_uncertain"
        );
        status.ownership = Ownership::Foreign;
        assert_eq!(
            tunnel_install_required(&status).unwrap_err().code,
            "tunnel_owner"
        );
    }
}
