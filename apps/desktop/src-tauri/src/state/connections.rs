use super::*;
use crate::connection_id::TunnelProfileId;
use crate::connections::{
    ConnectionError, ConnectionHealth, ConnectionLifecycle, ConnectionsSnapshot,
    TunnelConnectionSnapshot,
};
use crate::tunnel_config::TunnelProfileRequest;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionAction {
    Start,
    Stop,
    Restart,
    Delete,
}

impl AppState {
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
                if let Some(id) = request.id {
                    let saved = super::environment::store()?;
                    let already_managed = webcodex_environment::tunnel_profiles(&saved)
                        .map_err(super::environment::desktop_error)?
                        .iter().any(|profile| profile.profile_id == id.to_string());
                    if already_managed {
                        let original = core.tunnel_config.credentials_for(id)?;
                        let changed_id = original.tunnel_id.expose() != request.tunnel_id.trim();
                        let changed_key = request.api_key.as_deref().filter(|key| !key.trim().is_empty())
                            .is_some_and(|key| original.api_key.expose() != key.trim());
                        if changed_id || changed_key {
                            return Err(DesktopError::new("tunnel_binding_conflict",
                                "The persistent Tunnel already owns another credential binding",
                                "Keep its Tunnel ID and credential, or use an explicit credential rotation workflow."));
                        }
                    }
                }
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
                .any(|p| p.id == id && p.enabled);
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
        id: TunnelProfileId,
        action: ConnectionAction,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelConfigUpdate, false)
            .await?;
        let result = async {
            cancellation.check()?;
            if !core.tunnel_config.profiles().iter().any(|p| p.id == id) {
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
                    if core.config.persistent_environment.is_some() {
                        let store = super::environment::store()?;
                        let native = webcodex_environment::NativeEnvironment::new()
                            .map_err(super::environment::desktop_error)?;
                        native
                            .remove_tunnel(&store, &id.to_string())
                            .await
                            .map_err(super::environment::desktop_error)?;
                        core.mutate_tunnel_config(move |config, path| config.remove(path, id))
                            .await?;
                        core.connections.remove(id);
                        return core.get_state().await;
                    }
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

    pub(super) fn project_connections(&mut self) {
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
        if self.config.persistent_environment.is_some() {
            let store = super::environment::store();
            let native = webcodex_environment::NativeEnvironment::new();
            for profile in &mut self.snapshot.connections.profiles {
                let status =
                    store
                        .as_ref()
                        .ok()
                        .zip(native.as_ref().ok())
                        .and_then(|(store, native)| {
                            native
                                .tunnel_status(store, &profile.config.id.to_string())
                                .ok()
                        });
                let runtime = &mut profile.runtime;
                runtime.pid = None;
                runtime.tunnel_client_pid = None;
                runtime.process_started = status
                    .as_ref()
                    .is_some_and(|status| status.service_status.running == Some(true));
                runtime.process_ready = status.as_ref().is_some_and(|status| status.ready);
                runtime.ready = runtime.process_ready;
                runtime.tunnel_ready = status.as_ref().map(|status| status.tunnel_ready);
                runtime.local_mcp_ready = status.as_ref().map(|status| status.local_mcp_ready);
                runtime.lifecycle = match status.as_ref() {
                    Some(status) if status.service_status.running == Some(true) => {
                        ConnectionLifecycle::Running
                    }
                    Some(status) if status.service_status.running == Some(false) => {
                        ConnectionLifecycle::Stopped
                    }
                    _ => ConnectionLifecycle::Error,
                };
                runtime.health = if runtime.ready {
                    ConnectionHealth::Healthy
                } else if runtime.process_started {
                    ConnectionHealth::Degraded
                } else {
                    ConnectionHealth::Unknown
                };
                runtime.last_error = if runtime.process_started && !runtime.ready {
                    Some(if runtime.local_mcp_ready == Some(false) {
                        ConnectionError::LocalMcpUnavailable
                    } else {
                        ConnectionError::TunnelUnavailable
                    })
                } else if runtime.lifecycle == ConnectionLifecycle::Error {
                    Some(ConnectionError::StartFailed)
                } else {
                    None
                };
            }
            self.snapshot.connections.recount();
        }
    }

    pub(super) async fn start_connection_process(
        &mut self,
        id: TunnelProfileId,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        if self.config.persistent_environment.is_some() {
            cancellation.check()?;
            let store = super::environment::store()?;
            let credentials = self.tunnel_config.credentials_for(id)?;
            let native = webcodex_environment::NativeEnvironment::new()
                .map_err(super::environment::desktop_error)?;
            native
                .configure_tunnel(&store, &id.to_string(), Some(&credentials))
                .await
                .map_err(super::environment::desktop_error)?;
            return Ok(());
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
            let profile = webcodex_environment::tunnel_profiles(&store)
                .map_err(super::environment::desktop_error)?
                .into_iter()
                .find(|profile| profile.profile_id == id.to_string());
            if !profile.is_some_and(|profile| profile.installed) {
                return Ok(());
            }
            let native = webcodex_environment::NativeEnvironment::new()
                .map_err(super::environment::desktop_error)?;
            native
                .control_tunnel(
                    &store,
                    &id.to_string(),
                    webcodex_environment::ServiceOperation::Stop,
                )
                .await
                .map_err(super::environment::desktop_error)?;
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
        if self.config.persistent_environment.is_some() {
            return cancellation.check();
        }
        for profile in self.tunnel_config.profiles() {
            cancellation.check()?;
            if profile.enabled && profile.autostart {
                // A profile failure is local to that connection, not a setup failure.
                let _ = self
                    .start_connection_process(profile.id, cancellation)
                    .await;
            }
        }
        cancellation.check()
    }

    /// Legacy onboarding addresses only the reserved default profile.
    pub async fn start_regular_tunnel(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.mutate_tunnel_config(|config, path| config.enable_default_onboarding(path))
            .await?;
        self.start_connection_process(TunnelProfileId::DEFAULT, cancellation)
            .await?;
        self.get_state().await
    }
    pub async fn stop_regular_tunnel(
        &mut self,
        _cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.mutate_tunnel_config(|config, path| {
            config.set_enabled(path, TunnelProfileId::DEFAULT, false)
        })
        .await?;
        self.stop_connection_process(TunnelProfileId::DEFAULT)
            .await?;
        self.get_state().await
    }
}

fn connection_missing() -> DesktopError {
    DesktopError::new(
        "tunnel_profile_missing",
        "This connection is unavailable",
        "Refresh Connections before retrying.",
    )
}
