use super::*;
use crate::connection_id::TunnelProfileId;
use crate::connections::{
    ConnectionError, ConnectionHealth, ConnectionLifecycle, ConnectionsSnapshot,
    TunnelConnectionSnapshot,
};
use crate::tunnel_config::TunnelProfileRequest;
use serde::Deserialize;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAction {
    Start,
    Stop,
    Restart,
    Delete,
}

impl AppState {
    pub async fn resume_saved_connections(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RegularTunnelStart, true)
            .await?;
        let result = async {
            if core.configuration_issue.is_some() || super::environment::migration_in_progress() {
                return Err(DesktopError::new(
                    "configuration_unavailable",
                    "Saved configuration is unavailable or being migrated",
                    "Resolve the configuration issue before reconnecting.",
                ));
            }
            core.autostart_connections(&cancellation).await?;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn save_tunnel_profile(
        &self,
        request: TunnelProfileRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelConfigUpdate, false)
            .await?;
        let result = async {
            cancellation.check()?;
            if core.config.persistent_environment.is_some() {
                core.save_persistent_tunnel_profile(request, &cancellation)
                    .await?;
                return core.get_state().await;
            }

            let previous = core.tunnel_config.clone();
            let id = core
                .mutate_tunnel_config(move |config, path| config.update_profile(path, request))
                .await?;
            core.project_connections();
            core.publish_snapshot();
            let active =
                process_is_active(core.process_snapshot(ProcessKey::RegularTunnel(id)).await);
            if active && !core.tunnel_config.same_launch_as(&previous, id) {
                core.stop_connection_process(id)
                    .await
                    .map_err(tunnel_apply_error)?;
            }
            let enabled = core
                .tunnel_config
                .profiles()
                .iter()
                .any(|profile| profile.id == id.to_string() && profile.enabled);
            if enabled && core.snapshot.readiness.runtime_ready {
                core.start_connection_process(id, &cancellation)
                    .await
                    .map_err(tunnel_apply_error)?;
            }
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn tunnel_profile_action(
        &self,
        id: String,
        action: ConnectionAction,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelConfigUpdate, false)
            .await?;
        let result = async {
            cancellation.check()?;
            if core.config.persistent_environment.is_some() {
                core.persistent_tunnel_profile_action(&id, action, &cancellation)
                    .await?;
                return core.get_state().await;
            }

            let id = TunnelProfileId::try_from(id).map_err(|_| connection_missing())?;
            if !core
                .tunnel_config
                .profiles()
                .iter()
                .any(|profile| profile.id == id.to_string())
            {
                return Err(connection_missing());
            }
            match action {
                ConnectionAction::Start | ConnectionAction::Restart => {
                    core.mutate_tunnel_config(move |config, path| {
                        config.set_enabled(path, id, true)
                    })
                    .await?;
                    if matches!(action, ConnectionAction::Restart) {
                        core.stop_connection_process(id).await?;
                    }
                    core.start_connection_process(id, &cancellation).await?;
                }
                ConnectionAction::Stop => {
                    // Persist the user's stop intent before touching the owned process.
                    core.mutate_tunnel_config(move |config, path| {
                        config.set_enabled(path, id, false)
                    })
                    .await?;
                    core.stop_connection_process(id).await?;
                }
                ConnectionAction::Delete => {
                    // A failed stop must retain both identity and secret for recovery.
                    core.stop_connection_process(id).await?;
                    core.mutate_tunnel_config(move |config, path| config.remove(path, id))
                        .await?;
                    core.connections.remove(id);
                }
            }
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

impl DesktopCore {
    pub(super) async fn mutate_tunnel_config<T: Send + 'static>(
        &mut self,
        change: impl FnOnce(&mut TunnelConfig, &Path) -> DesktopResult<T> + Send + 'static,
    ) -> DesktopResult<T> {
        let mut config = self.tunnel_config.clone();
        let path = self.data_dir.join("secrets/tunnel-config.json");
        let (config, result) = tokio::task::spawn_blocking(move || {
            let result = change(&mut config, &path)?;
            Ok::<_, DesktopError>((config, result))
        })
        .await
        .map_err(|_| {
            DesktopError::new(
                "tunnel_config_unavailable",
                "Connection settings could not be saved",
                "Refresh Desktop before retrying.",
            )
        })??;
        self.tunnel_config = config;
        Ok(result)
    }

    async fn save_persistent_tunnel_profile(
        &mut self,
        request: TunnelProfileRequest,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        cancellation.check()?;
        let store = super::environment::store()?;
        self.tunnel_config
            .ensure_persistent_catalog_compatible(&store)?;
        let profiles = webcodex_environment::tunnel_profile_snapshots(&store)
            .map_err(super::environment::desktop_error)?;
        let profile_id = request
            .id
            .clone()
            .unwrap_or_else(|| TunnelProfileId::new().to_string());
        let existing = profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id);
        if request.id.is_some() && existing.is_none() {
            return Err(connection_missing());
        }
        let tunnel_id = request.tunnel_id.trim();
        let api_key = request
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let credentials = match existing {
            Some(profile) => {
                if profile.tunnel_id != tunnel_id {
                    return Err(DesktopError::new(
                        "tunnel_binding_conflict",
                        "The persistent Tunnel already owns another identity",
                        "Keep its Tunnel ID, or use an explicit credential rotation workflow.",
                    ));
                }
                api_key.map(|api_key| webcodex_environment::TunnelCredentials {
                    tunnel_id: webcodex_environment::Secret::new(tunnel_id.to_owned()),
                    api_key: webcodex_environment::Secret::new(api_key.to_owned()),
                })
            }
            None => Some(webcodex_environment::TunnelCredentials {
                tunnel_id: webcodex_environment::Secret::new(tunnel_id.to_owned()),
                api_key: webcodex_environment::Secret::new(
                    api_key
                        .ok_or_else(|| {
                            DesktopError::new(
                                "tunnel_credentials",
                                "A Tunnel API key is required for a new connection",
                                "Enter the protected credential issued for this Tunnel.",
                            )
                        })?
                        .to_owned(),
                ),
            }),
        };
        let native = webcodex_environment::NativeEnvironment::new()
            .map_err(super::environment::desktop_error)?;
        native
            .configure_tunnel_profile(
                &store,
                &profile_id,
                Some(&request.name),
                request.host_mode,
                Some(request.autostart),
                request.expected_revision,
                credentials.as_ref(),
                request.host_mode == webcodex_environment::TunnelHostMode::Standalone,
            )
            .await
            .map_err(super::environment::desktop_error)?;
        cancellation.check()
    }

    async fn persistent_tunnel_profile_action(
        &mut self,
        id: &str,
        action: ConnectionAction,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        cancellation.check()?;
        let store = super::environment::store()?;
        self.tunnel_config
            .ensure_persistent_catalog_compatible(&store)?;
        let profile = webcodex_environment::tunnel_profile_snapshots(&store)
            .map_err(super::environment::desktop_error)?
            .into_iter()
            .find(|profile| profile.profile_id == id)
            .ok_or_else(connection_missing)?;
        let native = webcodex_environment::NativeEnvironment::new()
            .map_err(super::environment::desktop_error)?;
        if action == ConnectionAction::Delete {
            native
                .remove_tunnel(&store, id)
                .await
                .map_err(super::environment::desktop_error)?;
            return cancellation.check();
        }
        if profile.host_mode == webcodex_environment::TunnelHostMode::Embedded {
            return Err(DesktopError::new(
                "tunnel_server_owned",
                "This connection runs with the WebCodex Server",
                "Use the explicit Server lifecycle control. Desktop will never restart a working Server implicitly.",
            ));
        }
        let observation = native
            .tunnel_status(&store, id)
            .map_err(super::environment::desktop_error)?;
        let ownership = observation.service_status.ownership;
        match action {
            ConnectionAction::Start => {
                native
                    .configure_tunnel_profile(
                        &store,
                        id,
                        None,
                        webcodex_environment::TunnelHostMode::Standalone,
                        None,
                        Some(profile.revision),
                        None,
                        true,
                    )
                    .await
                    .map_err(super::environment::desktop_error)?;
            }
            ConnectionAction::Restart => match ownership {
                webcodex_environment::service::Ownership::Owned => {
                    native
                        .control_tunnel(&store, id, webcodex_environment::ServiceOperation::Restart)
                        .await
                        .map_err(super::environment::desktop_error)?;
                }
                webcodex_environment::service::Ownership::Absent => {
                    native
                        .configure_tunnel_profile(
                            &store,
                            id,
                            None,
                            webcodex_environment::TunnelHostMode::Standalone,
                            None,
                            Some(profile.revision),
                            None,
                            true,
                        )
                        .await
                        .map_err(super::environment::desktop_error)?;
                }
                webcodex_environment::service::Ownership::Foreign
                | webcodex_environment::service::Ownership::Unknown => {
                    return Err(tunnel_owner_unverified());
                }
            },
            ConnectionAction::Stop => match ownership {
                webcodex_environment::service::Ownership::Owned => {
                    native
                        .control_tunnel(&store, id, webcodex_environment::ServiceOperation::Stop)
                        .await
                        .map_err(super::environment::desktop_error)?;
                }
                webcodex_environment::service::Ownership::Absent => {}
                webcodex_environment::service::Ownership::Foreign
                | webcodex_environment::service::Ownership::Unknown => {
                    return Err(tunnel_owner_unverified());
                }
            },
            ConnectionAction::Delete => unreachable!("handled above"),
        }
        cancellation.check()
    }

    pub(super) fn project_connections(&mut self) {
        if self.config.persistent_environment.is_some() {
            self.project_persistent_connections();
            return;
        }
        self.snapshot.connections = ConnectionsSnapshot {
            profiles: self
                .tunnel_config
                .profiles()
                .into_iter()
                .map(|config| TunnelConnectionSnapshot {
                    config,
                    runtime: Default::default(),
                })
                .collect(),
            config_error: self.tunnel_config.is_invalid(),
            ..Default::default()
        };
        self.connections.project(
            &mut self.snapshot.connections,
            self.snapshot.readiness.runtime_ready,
        );
    }

    fn project_persistent_connections(&mut self) {
        let projected = (|| -> DesktopResult<ConnectionsSnapshot> {
            let store = super::environment::store()?;
            self.tunnel_config
                .ensure_persistent_catalog_compatible(&store)?;
            let profiles = webcodex_environment::tunnel_profile_snapshots(&store)
                .map_err(super::environment::desktop_error)?;
            let native = webcodex_environment::NativeEnvironment::new()
                .map_err(super::environment::desktop_error)?;
            let mut snapshot = ConnectionsSnapshot::default();
            for profile in profiles {
                let observation = native.tunnel_status(&store, &profile.profile_id);
                snapshot
                    .profiles
                    .push(persistent_connection_projection(profile, observation));
            }
            snapshot.recount();
            Ok(snapshot)
        })();
        self.snapshot.connections = projected.unwrap_or_else(|_| ConnectionsSnapshot {
            config_error: true,
            ..Default::default()
        });
    }

    pub(super) async fn start_connection_process(
        &mut self,
        id: TunnelProfileId,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        if self.config.persistent_environment.is_some() {
            cancellation.check()?;
            let store = super::environment::store()?;
            self.tunnel_config
                .ensure_persistent_catalog_compatible(&store)?;
            let profile_id = id.to_string();
            let profile = webcodex_environment::tunnel_profile_snapshots(&store)
                .map_err(super::environment::desktop_error)?
                .into_iter()
                .find(|profile| profile.profile_id == profile_id)
                .ok_or_else(connection_missing)?;
            if profile.host_mode == webcodex_environment::TunnelHostMode::Embedded {
                return Err(server_owned_connection());
            }
            webcodex_environment::NativeEnvironment::new()
                .map_err(super::environment::desktop_error)?
                .configure_tunnel_profile(
                    &store,
                    &profile_id,
                    None,
                    webcodex_environment::TunnelHostMode::Standalone,
                    None,
                    Some(profile.revision),
                    None,
                    true,
                )
                .await
                .map_err(super::environment::desktop_error)?;
            return cancellation.check();
        }
        let result = self.spawn_connection(id, cancellation).await;
        if result.is_err() {
            self.connections.fail_start(id);
        }
        result
    }

    async fn spawn_connection(
        &mut self,
        id: TunnelProfileId,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        cancellation.check()?;
        if process_is_active(self.process_snapshot(ProcessKey::RegularTunnel(id)).await) {
            return Ok(());
        }
        if self.snapshot.quick_share.is_some()
            || !self.config.topology.as_ref().is_some_and(|t| {
                t.experience == Experience::Full && matches!(t.server, ServerTopology::Local)
            })
        {
            return Err(DesktopError::new(
                "unsupported_topology",
                "Connections require the shared local full runtime",
                "Start the local Server and Runner before starting a connection.",
            ));
        }
        if !self.snapshot.readiness.runtime_ready {
            return Err(DesktopError::new(
                "runtime_not_ready",
                "The local runtime is not ready",
                "Restore Server and Runner readiness before starting the connection.",
            ));
        }
        let runtime = self
            .config
            .runtime
            .as_ref()
            .ok_or_else(connection_missing)?;
        let env_file = runtime
            .server_env_file
            .clone()
            .filter(|path| path.is_file())
            .ok_or_else(|| {
                DesktopError::new(
                    "server_unavailable",
                    "Local Server configuration is unavailable",
                    "Run Local Setup again.",
                )
            })?;
        let expected_root = env_file
            .parent()
            .ok_or_else(connection_missing)?
            .join("regular-tunnel-runtime");
        let local_mcp_url = format!("{}/mcp", runtime.server_url.trim_end_matches('/'));
        self.adapter.ensure_binaries(cancellation).await?;
        let proxy = effective_tunnel_proxy(&self.config.tunnel_proxy)?;
        let auto_proxy_used =
            self.config.tunnel_proxy.mode == TunnelProxyMode::Auto && proxy.url.is_some();
        let mut command = self
            .adapter
            .regular_tunnel_command(&env_file, proxy.url.as_deref())?;
        // Use the credential belonging to this exact Desktop-managed Server file,
        // not a bootstrap credential inherited from the shell that launched Desktop.
        command.env_remove("WEBCODEX_TOKEN");
        self.tunnel_config
            .apply_profile_to_command(id, &mut command)?;
        let key = ProcessKey::RegularTunnel(id);
        let mut supervisor = self.supervisor.lock().await;
        cancellation.check()?;
        let events = supervisor
            .spawn_owned(key, command, true)
            .await?
            .ok_or_else(connection_missing)?;
        let process = supervisor.snapshot(key).ok_or_else(connection_missing)?;
        self.connections.start(
            id,
            process,
            events,
            expected_root,
            local_mcp_url,
            auto_proxy_used,
            self.supervisor.clone(),
            self.activity.clone(),
        );
        Ok(())
    }

    pub(super) async fn stop_connection_process(
        &mut self,
        id: TunnelProfileId,
    ) -> DesktopResult<()> {
        if self.config.persistent_environment.is_some() {
            let store = super::environment::store()?;
            self.tunnel_config
                .ensure_persistent_catalog_compatible(&store)?;
            let profile_id = id.to_string();
            let profile = webcodex_environment::tunnel_profile_snapshots(&store)
                .map_err(super::environment::desktop_error)?
                .into_iter()
                .find(|profile| profile.profile_id == profile_id)
                .ok_or_else(connection_missing)?;
            if profile.host_mode == webcodex_environment::TunnelHostMode::Embedded {
                return Err(server_owned_connection());
            }
            let native = webcodex_environment::NativeEnvironment::new()
                .map_err(super::environment::desktop_error)?;
            let observation = native
                .tunnel_status(&store, &profile_id)
                .map_err(super::environment::desktop_error)?;
            match observation.service_status.ownership {
                webcodex_environment::service::Ownership::Owned => {
                    native
                        .control_tunnel(
                            &store,
                            &profile_id,
                            webcodex_environment::ServiceOperation::Stop,
                        )
                        .await
                        .map_err(super::environment::desktop_error)?;
                }
                webcodex_environment::service::Ownership::Absent => {}
                webcodex_environment::service::Ownership::Foreign
                | webcodex_environment::service::Ownership::Unknown => {
                    return Err(tunnel_owner_unverified());
                }
            }
            return Ok(());
        }
        self.connections.stopping(id);
        let result = self
            .supervisor
            .lock()
            .await
            .stop_checked(ProcessKey::RegularTunnel(id))
            .await;
        self.connections.stopped(id, result.is_ok());
        result
    }

    pub(super) async fn stop_all_connection_processes(&mut self) -> DesktopResult<()> {
        self.connections.cancel_all();
        let keys = self.supervisor.lock().await.keys();
        let mut failure = None;
        for key in keys {
            if let ProcessKey::RegularTunnel(id) = key {
                if let Err(error) = self.stop_connection_process(id).await {
                    failure.get_or_insert(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    pub(super) async fn autostart_connections(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        if self.config.persistent_environment.is_some()
            || !self.snapshot.readiness.runtime_ready
            || self.snapshot.quick_share.is_some()
            || !self.config.topology.as_ref().is_some_and(|topology| {
                topology.experience == Experience::Full
                    && matches!(topology.server, ServerTopology::Local)
            })
        {
            return cancellation.check();
        }
        for profile in self.tunnel_config.profiles() {
            cancellation.check()?;
            if profile.enabled && profile.autostart {
                // A profile failure is local to that connection, not a setup failure.
                if let Ok(id) = TunnelProfileId::try_from(profile.id) {
                    let _ = self.start_connection_process(id, cancellation).await;
                }
            }
        }
        cancellation.check()
    }

    /// Legacy onboarding addresses only the reserved default profile.
    pub async fn start_regular_tunnel(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if self.config.persistent_environment.is_none() {
            self.mutate_tunnel_config(|config, path| config.enable_default_onboarding(path))
                .await?;
        }
        self.start_connection_process(TunnelProfileId::DEFAULT, cancellation)
            .await?;
        self.get_state().await
    }
    pub async fn stop_regular_tunnel(
        &mut self,
        _cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if self.config.persistent_environment.is_none() {
            self.mutate_tunnel_config(|config, path| {
                config.set_enabled(path, TunnelProfileId::DEFAULT, false)
            })
            .await?;
        }
        self.stop_connection_process(TunnelProfileId::DEFAULT)
            .await?;
        self.get_state().await
    }
}

fn persistent_connection_projection(
    profile: webcodex_environment::TunnelProfileSnapshot,
    observation: webcodex_environment::SetupResultValue<
        webcodex_environment::TunnelRuntimeObservation,
    >,
) -> TunnelConnectionSnapshot {
    let server_restart_required = observation
        .as_ref()
        .is_ok_and(|status| status.server_restart_required);
    let enabled = match profile.host_mode {
        webcodex_environment::TunnelHostMode::Embedded => profile.autostart,
        webcodex_environment::TunnelHostMode::Standalone => observation
            .as_ref()
            .is_ok_and(|status| status.service_status.running == Some(true)),
    };
    let runtime = persistent_runtime_projection(&profile, observation);
    TunnelConnectionSnapshot {
        config: crate::tunnel_config::TunnelProfileConfigSnapshot {
            id: profile.profile_id,
            name: profile.name,
            tunnel_id: Some(profile.tunnel_id),
            credential_present: profile.credential_present,
            enabled,
            autostart: profile.autostart,
            revision: profile.revision,
            source: crate::models::TunnelConfigSource::Environment,
            host_mode: profile.host_mode,
            server_restart_required,
        },
        runtime,
    }
}

fn persistent_runtime_projection(
    profile: &webcodex_environment::TunnelProfileSnapshot,
    observation: webcodex_environment::SetupResultValue<
        webcodex_environment::TunnelRuntimeObservation,
    >,
) -> crate::connections::ConnectionRuntimeSnapshot {
    let Ok(status) = observation else {
        return crate::connections::ConnectionRuntimeSnapshot {
            lifecycle: ConnectionLifecycle::Error,
            last_error: Some(ConnectionError::StartFailed),
            ..Default::default()
        };
    };
    if status.profile_id != profile.profile_id
        || status.tunnel_id != profile.tunnel_id
        || status.host_mode != profile.host_mode
        || status.autostart != profile.autostart
    {
        return crate::connections::ConnectionRuntimeSnapshot {
            lifecycle: ConnectionLifecycle::Error,
            last_error: Some(ConnectionError::StartFailed),
            ..Default::default()
        };
    }
    if matches!(
        status.service_status.ownership,
        webcodex_environment::service::Ownership::Foreign
            | webcodex_environment::service::Ownership::Unknown
    ) {
        return crate::connections::ConnectionRuntimeSnapshot {
            lifecycle: ConnectionLifecycle::Error,
            last_error: Some(ConnectionError::StartFailed),
            ..Default::default()
        };
    }
    let owner_running = status.service_status.running == Some(true);
    let selected_at_startup =
        profile.host_mode == webcodex_environment::TunnelHostMode::Standalone || profile.autostart;
    let process_started = owner_running && selected_at_startup && !status.server_restart_required;
    let ready = process_started && status.ready;
    let lifecycle = if process_started {
        ConnectionLifecycle::Running
    } else {
        ConnectionLifecycle::Stopped
    };
    let last_error = if process_started && !ready {
        Some(if !status.local_mcp_ready {
            ConnectionError::LocalMcpUnavailable
        } else {
            ConnectionError::TunnelUnavailable
        })
    } else {
        None
    };
    crate::connections::ConnectionRuntimeSnapshot {
        lifecycle,
        pid: None,
        health: if ready {
            ConnectionHealth::Healthy
        } else if process_started {
            ConnectionHealth::Degraded
        } else {
            ConnectionHealth::Unknown
        },
        last_error,
        ready,
        process_started,
        process_ready: ready,
        tunnel_ready: process_started.then_some(status.tunnel_ready),
        local_mcp_ready: process_started.then_some(status.local_mcp_ready),
        ..Default::default()
    }
}

fn tunnel_owner_unverified() -> DesktopError {
    DesktopError::new(
        "tunnel_owner",
        "The separate Tunnel service owner cannot be verified",
        "Inspect the actual service owner before starting, stopping, or restarting it.",
    )
}

fn server_owned_connection() -> DesktopError {
    DesktopError::new(
        "tunnel_server_owned",
        "This connection runs with the WebCodex Server",
        "Use the explicit Server lifecycle control. Desktop will never restart a working Server implicitly.",
    )
}

fn connection_missing() -> DesktopError {
    DesktopError::new(
        "tunnel_profile_missing",
        "This connection is unavailable",
        "Refresh Connections before retrying.",
    )
}
