pub struct DesktopCore {
    data_dir: PathBuf,
    config_path: PathBuf,
    config: StoredDesktopConfig,
    configuration_issue: Option<String>,
    runtime_candidate: Option<crate::runtime_selection::RuntimeCandidate>,
    runtime_candidate_context: Option<String>,
    runtime_selected_probe: Option<crate::runtime_selection::RuntimeCandidate>,
    runtime_last_switch: Option<crate::runtime_selection::RuntimeSwitchResult>,
    tunnel_config: TunnelConfig,
    mcp_providers: crate::mcp_providers::McpProviderStore,
    mcp_applied_revision: Option<u64>,
    coding_agents: crate::coding_agents::CodingAgentStore,
    coding_agents_applied_revision: Option<u64>,
    connections: ConnectionRuntimes,
    snapshot: DesktopStateSnapshot,
    inventory_persistence_pending: bool,
    adapter: WebCodexAdapter,
    supervisor: SharedSupervisor,
    activity: ActivityLog,
    published: Arc<RwLock<DesktopStateSnapshot>>,
}

impl DesktopCore {
    fn new(data_dir: PathBuf, resource_dir: PathBuf) -> DesktopResult<Self> {
        let activity = ActivityLog::default();
        let config_path = data_dir.join("desktop-state.json");
        // Keep Diagnostics usable if migration fails; admission below prevents
        // replacing the operator's files with a default configuration.
        let (mut config, configuration_issue) = match load_config(&config_path, &activity) {
            Ok(config) => (config, None),
            Err(error) => (StoredDesktopConfig::default(), Some(error.code)),
        };
        if configuration_issue.is_none() {
            environment::adopt_saved_environment(&mut config);
            if config.persistent_environment.is_none() && environment::migration_in_progress() {
                config.runtime_autostart = Some(false);
            }
        }
        let tunnel_config = TunnelConfig::load(
            &data_dir.join("secrets").join("tunnel-config.json"),
            config.preferred_connection == Some(RegularConnectionPreference::OpenAiTunnel),
        );
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.configuration_issue = configuration_issue.clone();
        snapshot.topology = config.topology.clone();
        snapshot.project = project_snapshot(&config);
        if config.topology.is_some() && config.runtime_autostart == Some(false) {
            snapshot.readiness = aggregate_readiness(
                ServerReadiness::Stopped,
                RunnerReadiness::Stopped,
                ExposureReadiness::Disabled,
                if config.project.is_some() {
                    ProjectReadiness::Configured
                } else {
                    ProjectReadiness::None
                },
            );
        }
        apply_openai_tunnel_configuration(&mut snapshot, &tunnel_config);
        snapshot.regular_tunnel_available = true;
        snapshot.powershell_runtime = crate::platform::powershell_runtime_snapshot();
        apply_config_projection(&mut snapshot, &config);
        snapshot.connections = crate::connections::ConnectionsSnapshot {
            profiles: tunnel_config
                .profiles()
                .into_iter()
                .map(|config| crate::connections::TunnelConnectionSnapshot {
                    config,
                    runtime: Default::default(),
                })
                .collect(),
            config_error: tunnel_config.is_invalid(),
            ..Default::default()
        };
        let mcp_providers = crate::mcp_providers::McpProviderStore::load(&data_dir);
        snapshot.mcp_providers = mcp_providers.snapshot(None);
        let coding_agents = crate::coding_agents::CodingAgentStore::load(&data_dir);
        snapshot.coding_agents = coding_agents.snapshot(None);
        let published = Arc::new(RwLock::new(snapshot.clone()));
        let supervisor = Arc::new(Mutex::new(ProcessSupervisor::new(activity.clone())));
        let runtime_directory = std::env::current_exe()
            .ok()
            .and_then(|executable| {
                webcodex_environment::installed_desktop_runtime_directory(&executable)
            })
            .unwrap_or_else(|| resource_dir.join("webcodex-runtime"));
        let mut adapter = WebCodexAdapter::new(Some(runtime_directory));
        adapter.set_runtime_source(config.runtime_binary_source.clone());
        adapter.set_runtime_approval(config.runtime_binary_fingerprint.clone());
        Ok(Self {
            data_dir,
            config_path,
            config,
            configuration_issue,
            runtime_candidate: None,
            runtime_candidate_context: None,
            runtime_selected_probe: None,
            runtime_last_switch: None,
            tunnel_config,
            mcp_providers,
            mcp_applied_revision: None,
            coding_agents,
            coding_agents_applied_revision: None,
            connections: ConnectionRuntimes::default(),
            snapshot,
            inventory_persistence_pending: false,
            adapter,
            supervisor,
            activity,
            published,
        })
    }

    pub async fn get_state(&mut self) -> DesktopResult<DesktopStateSnapshot> {
        Ok(self.publish_snapshot())
    }

    fn chatgpt_activity_probe(&self) -> Option<ChatGptActivityProbe> {
        if !self.snapshot.readiness.runtime_ready {
            return None;
        }
        let identity = identity_from_config(&self.config)?;
        let webcodex = self.adapter.binaries().ok()?.webcodex.clone();
        Some(ChatGptActivityProbe { identity, webcodex })
    }

    fn apply_chatgpt_activity_observation(
        &mut self,
        expected_identity: &ProjectRuntimeIdentity,
        last_meaningful_activity_at_ms: Option<i64>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if !self.snapshot.readiness.runtime_ready
            || identity_from_config(&self.config).as_ref() != Some(expected_identity)
        {
            return Ok(self.publish_snapshot());
        }
        let last_meaningful_activity_at_ms = last_meaningful_activity_at_ms.max(
            self.snapshot
                .chatgpt_activity
                .as_ref()
                .and_then(|activity| activity.last_meaningful_activity_at_ms),
        );
        self.snapshot.chatgpt_activity = Some(ChatGptActivitySnapshot {
            observed: last_meaningful_activity_at_ms.is_some(),
            last_meaningful_activity_at_ms,
        });
        Ok(self.publish_snapshot())
    }

    fn stage_project_scope(&mut self, project: ProjectSelection) {
        self.snapshot.project = Some(project);
        // ChatGPT activity is evidence for one exact runtime Project. Never carry
        // an observation from the previously displayed Project into a new setup
        // or Quick Share scope; the new Project must earn its own observation.
        self.snapshot.chatgpt_activity = None;
    }

    pub async fn refresh_runtime_status(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        if self.config.persistent_environment.is_some() {
            return self.refresh_environment_status(cancellation).await;
        }
        if self.snapshot.quick_share.is_some() {
            self.snapshot.chatgpt_activity = None;
            let active = self
                .process_snapshot(ProcessKey::QuickShare)
                .await
                .is_some_and(|process| {
                    matches!(
                        process.phase,
                        ProcessPhase::Starting | ProcessPhase::Running
                    )
                });
            if !active {
                self.snapshot.readiness = aggregate_readiness(
                    ServerReadiness::Stopped,
                    RunnerReadiness::Stopped,
                    ExposureReadiness::Error,
                    ProjectReadiness::Configured,
                );
                self.snapshot.readiness.summary_kind = ReadinessSummaryKind::QuickShareStopped;
                self.snapshot.readiness.next_action_kind =
                    Some(ReadinessNextActionKind::RestartQuickShare);
                self.snapshot.readiness.summary = "Quick Share stopped".to_string();
                self.snapshot.readiness.next_action = Some("Start Quick Share again.".to_string());
            }
            return self.get_state().await;
        }

        let Some(identity) = runner_identity_from_config(&self.config) else {
            self.snapshot.chatgpt_activity = None;
            self.snapshot.topology = self.config.topology.clone();
            self.snapshot.project = project_snapshot(&self.config);
            return self.get_state().await;
        };
        self.adapter.ensure_binaries(cancellation).await.ok();
        cancellation.check()?;
        if let Ok(binaries) = self.adapter.binaries() {
            self.snapshot.binaries = Some(binaries.info());
        }
        let server = match self
            .adapter
            .server_status(
                Some(&identity.server_url),
                self.config
                    .runtime
                    .as_ref()
                    .and_then(|runtime| runtime.server_env_file.as_deref()),
                Some(&identity.user_token_file),
                cancellation,
            )
            .await
        {
            Ok(status) if status.http_reachable => ServerReadiness::Ready,
            Ok(_) => ServerReadiness::Error,
            Err(_) => ServerReadiness::Unknown,
        };
        cancellation.check()?;
        // A saved user stop remains stopped across refresh, while a reachable
        // external service is still observed normally.
        if !runtime_autostart(&self.config) && server != ServerReadiness::Ready {
            self.snapshot.chatgpt_activity = None;
            self.snapshot.readiness = aggregate_readiness(
                ServerReadiness::Stopped,
                RunnerReadiness::Stopped,
                ExposureReadiness::Disabled,
                ProjectReadiness::Configured,
            );
            return self.get_state().await;
        }
        let runner = match self.adapter.runner_ready(&identity, cancellation).await {
            Ok(true) => RunnerReadiness::Ready,
            Ok(false) => RunnerReadiness::Connecting,
            Err(_) => RunnerReadiness::Unknown,
        };
        cancellation.check()?;
        let project_identity = identity_from_config(&self.config);
        let project = if let Some(identity) = project_identity.as_ref() {
            match self.adapter.project_ready(identity, cancellation).await {
                Ok(true) => ProjectReadiness::Ready,
                Ok(false) => ProjectReadiness::ReloadRequired,
                Err(_) => ProjectReadiness::Unknown,
            }
        } else if self.config.project.is_some() {
            ProjectReadiness::Configured
        } else {
            ProjectReadiness::None
        };
        cancellation.check()?;
        self.snapshot.chatgpt_activity =
            if server == ServerReadiness::Ready && project == ProjectReadiness::Ready {
                match self
                    .adapter
                    .chatgpt_activity(
                        project_identity.as_ref().expect("ready project"),
                        cancellation,
                    )
                    .await
                {
                    Ok(last_meaningful_activity_at_ms) => Some(ChatGptActivitySnapshot {
                        observed: last_meaningful_activity_at_ms.is_some(),
                        last_meaningful_activity_at_ms,
                    }),
                    Err(_) => None,
                }
            } else {
                None
            };
        cancellation.check()?;
        // Exposure failures belong to Connections; the shared runtime retains its
        // independent Server/Runner/project readiness.
        let exposure = exposure_readiness(self.config.topology.as_ref());
        self.snapshot.readiness = aggregate_readiness(server, runner, exposure, project);
        self.snapshot.topology = self.config.topology.clone();
        self.snapshot.project = project_snapshot(&self.config);
        self.get_state().await
    }

    fn publish_snapshot(&mut self) -> DesktopStateSnapshot {
        self.snapshot.mcp_providers = self.mcp_providers.snapshot(self.mcp_applied_revision);
        self.snapshot.coding_agents = self
            .coding_agents
            .snapshot(self.coding_agents_applied_revision);
        self.project_connections();
        self.snapshot.current_operation = None;
        self.snapshot.activity_sequence = self.activity.latest_sequence();
        apply_openai_tunnel_configuration(&mut self.snapshot, &self.tunnel_config);
        self.snapshot.regular_tunnel_available = true;
        self.snapshot.powershell_runtime = crate::platform::powershell_runtime_snapshot();
        apply_config_projection(&mut self.snapshot, &self.config);
        let snapshot = self.snapshot.clone();
        *self
            .published
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.clone();
        snapshot
    }

    fn terminalize_failed_start(&mut self, cancelled: bool) {
        let server = if self.snapshot.readiness.server == ServerReadiness::Starting {
            if cancelled {
                ServerReadiness::Stopped
            } else {
                ServerReadiness::Error
            }
        } else {
            self.snapshot.readiness.server.clone()
        };
        let runner = if self.snapshot.readiness.runner == RunnerReadiness::Connecting {
            if cancelled {
                RunnerReadiness::Stopped
            } else {
                RunnerReadiness::Error
            }
        } else {
            self.snapshot.readiness.runner.clone()
        };
        let exposure = if self.snapshot.readiness.exposure == ExposureReadiness::Starting {
            if cancelled {
                ExposureReadiness::Disabled
            } else {
                ExposureReadiness::Error
            }
        } else {
            self.snapshot.readiness.exposure.clone()
        };
        self.snapshot.readiness = aggregate_readiness(
            server,
            runner,
            exposure,
            self.snapshot.readiness.project.clone(),
        );
    }

    fn reconcile_after_operation_failure(
        &mut self,
        kind: DesktopOperationKind,
        baseline: &ProcessBaseline,
        cleanup: ProcessCleanup,
        cancelled: bool,
    ) {
        let observed_binaries = self.snapshot.binaries.clone();
        match kind {
            DesktopOperationKind::RuntimeResume => {
                // Resume is a desired-state replay of an already committed setup.
                // If any step fails or is cancelled, newly owned processes have
                // already been reclaimed above; restore the last published view
                // rather than leaving a synthetic "starting" state behind.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::QuickShareStart => {
                // Quick Share is intentionally ephemeral. Any failed start has
                // already stopped (or will have cleanup stop) the newly owned
                // foreground process, so return the public topology to the
                // last committed runtime instead of leaving "starting" behind.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::RegularTunnelStart if cancelled => {
                // User cancellation is not a tunnel failure. Restore the last
                // observed full-runtime state after the exact owned tunnel is
                // reclaimed rather than publishing a synthetic tunnel error.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::RuntimeRefresh if cancelled => {
                // A cancelled observation must not partially overwrite the
                // last published control-plane state.
                self.snapshot = baseline.snapshot.clone();
            }
            DesktopOperationKind::LocalSetup => {
                let server = if cleanup.local_server {
                    ServerReadiness::Stopped
                } else if self.snapshot.readiness.server == ServerReadiness::Starting {
                    baseline.snapshot.readiness.server.clone()
                } else {
                    self.snapshot.readiness.server.clone()
                };
                let runner = if cleanup.local_runner {
                    RunnerReadiness::Stopped
                } else if self.snapshot.readiness.runner == RunnerReadiness::Connecting {
                    baseline.snapshot.readiness.runner.clone()
                } else {
                    self.snapshot.readiness.runner.clone()
                };
                let project = if cleanup.local_server || cleanup.local_runner {
                    self.snapshot
                        .project
                        .as_ref()
                        .map(|_| ProjectReadiness::Configured)
                        .unwrap_or(ProjectReadiness::None)
                } else {
                    self.snapshot.readiness.project.clone()
                };
                self.snapshot.readiness = aggregate_readiness(
                    server,
                    runner,
                    self.snapshot.readiness.exposure.clone(),
                    project,
                );
            }
            DesktopOperationKind::RemoteSetup => {
                let runner = if cleanup.local_runner {
                    RunnerReadiness::Stopped
                } else if self.snapshot.readiness.runner == RunnerReadiness::Connecting {
                    baseline.snapshot.readiness.runner.clone()
                } else {
                    self.snapshot.readiness.runner.clone()
                };
                let project = if cleanup.local_runner {
                    self.snapshot
                        .project
                        .as_ref()
                        .map(|_| ProjectReadiness::Configured)
                        .unwrap_or(ProjectReadiness::None)
                } else {
                    self.snapshot.readiness.project.clone()
                };
                self.snapshot.readiness = aggregate_readiness(
                    self.snapshot.readiness.server.clone(),
                    runner,
                    self.snapshot.readiness.exposure.clone(),
                    project,
                );
            }
            _ => {}
        }
        if observed_binaries.is_some() {
            self.snapshot.binaries = observed_binaries;
        }
    }

    pub async fn resume_saved_runtime(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        self.preflight_providers()?;
        if self.config.persistent_environment.is_some() {
            return self.resume_environment(cancellation).await;
        }
        if environment::migration_in_progress() {
            return Err(DesktopError::new("migration_required",
                "A previous owner handoff is unfinished",
                "Open environment setup to resume its exact saved migration; do not start another Runner."));
        }
        let Some(topology) = self.config.topology.clone() else {
            return self.get_state().await;
        };
        if topology.experience != Experience::Full {
            return self.get_state().await;
        }
        let identity = runner_identity_from_config(&self.config).ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "The saved Runtime identity is incomplete",
                "Restore the saved Runner configuration and credentials.",
            )
        })?;
        let runtime = self.config.runtime.clone().expect("validated runtime");
        self.adapter.ensure_binaries(cancellation).await?;
        crate::runtime_selection::verify_resolved_files(self.adapter.binaries()?).await?;
        let deadline = Deadline::after(SERVER_READY_TIMEOUT);
        let reachable = self
            .adapter
            .server_status_until(
                Some(&identity.server_url),
                runtime.server_env_file.as_deref(),
                Some(&identity.user_token_file),
                cancellation,
                deadline,
            )
            .await
            .is_ok_and(|status| status.http_reachable);
        let mut server_started = false;
        if !reachable
            && matches!(topology.server, ServerTopology::Local)
            && !process_is_active(self.process_snapshot(ProcessKey::LocalServer).await)
        {
            let env = runtime
                .server_env_file
                .as_deref()
                .filter(|path| path.is_file())
                .ok_or_else(|| {
                    desktop_state_unavailable("The saved Server configuration is unavailable")
                })?;
            let command = self.adapter.local_server_command(env)?;
            self.spawn_owned(ProcessKey::LocalServer, command, false, cancellation)
                .await?;
            server_started = true;
        }
        self.wait_for_server(
            &identity.server_url,
            runtime.server_env_file.as_deref(),
            Some(&identity.user_token_file),
            cancellation,
            deadline,
            server_started,
        )
        .await?;
        let deadline = Deadline::after(RUNNER_READY_TIMEOUT);
        let observation = self
            .adapter
            .observe_runner_connection(
                &identity,
                stored_runner_client_id(&self.config).as_deref(),
                cancellation,
            )
            .await?;
        let runner_started = !observation.online
            && !process_is_active(self.process_snapshot(ProcessKey::LocalRunner).await);
        if runner_started {
            self.spawn_configured_runner(&identity, cancellation)
                .await?;
        }
        self.wait_for_runner(&identity, cancellation, deadline, runner_started)
            .await?;
        if let Ok(overview) =
            crate::workspace::query(&runtime, crate::workspace::WorkspaceRequest::Overview {}).await
        {
            self.reconcile_inventory(&overview).await;
        }
        cancellation.check()?;
        self.config.runtime_autostart = Some(true);
        self.save_config().await?;
        // Observe only: never activate a saved display Project during recovery.
        self.refresh_runtime_status(cancellation).await?;
        self.autostart_connections(cancellation).await?;
        self.get_state().await
    }

    pub async fn update_tunnel_proxy(
        &mut self,
        mode: TunnelProxyMode,
        custom_url: Option<&str>,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let custom_url = match mode {
            TunnelProxyMode::Custom => Some(validate_tunnel_proxy_url(custom_url.unwrap_or(""))?),
            TunnelProxyMode::Auto | TunnelProxyMode::Direct => None,
        };
        self.config.tunnel_proxy = TunnelProxyConfig { mode, custom_url };
        self.save_config().await?;
        self.get_state().await
    }

    pub async fn activate_local_project(
        &mut self,
        project_path: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let local_full = self.config.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(topology.server, ServerTopology::Local)
        });
        if !local_full || self.snapshot.quick_share.is_some() {
            return Err(DesktopError::new(
                "unsupported_topology",
                "Projects can only be activated in place on a local Full Runtime",
                "Use the full runtime setup flow before activating another project.",
            ));
        }
        if !self.snapshot.readiness.runtime_ready {
            return Err(DesktopError::new(
                "runtime_not_ready",
                "The local Full Runtime is not ready for an in-place project activation",
                "Restore the local runtime, then choose the project again.",
            ));
        }

        let project = self.adapter.inspect_project(project_path).await?;
        cancellation.check()?;
        let identity = runner_identity_from_config(&self.config).ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "The saved local Runner identity is incomplete",
                "Restore or reconfigure the local runtime before activating another project.",
            )
        })?;
        let runner_client_id = stored_runner_client_id(&self.config).ok_or_else(|| {
            DesktopError::new(
                "runner_offline",
                "The saved local Runner client identity is unavailable",
                "Restore or reconfigure the local runtime before activating another project.",
            )
        })?;
        let runner = self
            .adapter
            .observe_runner_connection(&identity, Some(&runner_client_id), cancellation)
            .await?;
        cancellation.check()?;
        if !runner.online {
            return Err(DesktopError::new(
                "runner_offline",
                "The current local Runner is not online",
                "Restore the local runtime, then choose the project again.",
            ));
        }

        let identity = self
            .adapter
            .activate_project(&identity, &runner_client_id, &project, cancellation)
            .await?;
        self.wait_for_project(&identity, cancellation).await?;
        cancellation.check()?;

        let previous_config = self.config.clone();
        let previous_snapshot = self.snapshot.clone();
        let mut committed_project = project;
        committed_project.runtime_project_id = Some(identity.runtime_project_id.clone());
        let runtime = self.config.runtime.as_mut().ok_or_else(|| {
            DesktopError::new(
                "runtime_not_ready",
                "The local runtime disappeared during project activation",
                "Restore the local runtime, then choose the project again.",
            )
        })?;
        runtime.project_id = Some(identity.project_id);
        runtime.runtime_project_id = Some(identity.runtime_project_id);
        self.config.project = Some(committed_project.clone());
        if let Err(error) = self.save_config().await {
            self.config = previous_config;
            self.snapshot = previous_snapshot;
            self.publish_snapshot();
            return Err(error);
        }

        self.activity.push(
            ActivityEventKind::ProjectActivated,
            "desktop",
            ActivityLevel::Info,
            std::path::Path::new(&committed_project.path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
        );
        self.snapshot.project = Some(committed_project);
        self.snapshot.chatgpt_activity = None;
        self.snapshot.readiness = aggregate_readiness(
            self.snapshot.readiness.server.clone(),
            self.snapshot.readiness.runner.clone(),
            self.snapshot.readiness.exposure.clone(),
            ProjectReadiness::Ready,
        );
        self.get_state().await
    }

    #[cfg(test)]
    pub async fn configure_local_setup(
        &mut self,
        project_path: Option<&str>,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        self.preflight_providers()?;
        let project = match project_path.map(str::trim).filter(|path| !path.is_empty()) {
            Some(path) => Some(self.adapter.inspect_project(path).await?),
            None => None,
        };
        cancellation.check()?;
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        crate::runtime_selection::verify_resolved_files(&binaries).await?;
        self.activity.push(
            ActivityEventKind::LocalSetupPreparing,
            "desktop",
            ActivityLevel::Info,
            "Preparing WebCodex on this computer",
        );
        self.snapshot.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        if let Some(project) = project.clone() {
            self.stage_project_scope(project);
        } else {
            self.snapshot.project = None;
            self.snapshot.chatgpt_activity = None;
        }
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Stopped,
            ExposureReadiness::Disabled,
            if project.is_some() {
                ProjectReadiness::Configured
            } else {
                ProjectReadiness::None
            },
        );
        self.publish_snapshot();

        let (env_file, data_dir) = local_runtime_paths(&self.data_dir);
        let local_dir = env_file
            .parent()
            .expect("Desktop local runtime env file always has a parent")
            .to_path_buf();
        tokio::fs::create_dir_all(&local_dir).await.map_err(|_| {
            DesktopError::new(
                "desktop_state_unavailable",
                "Desktop could not create its local runtime directory",
                "Check local app-data permissions and retry.",
            )
        })?;
        cancellation.check()?;

        let mut server_url = if env_file.is_file() {
            ensure_desktop_server_defaults(&env_file)?;
            desktop_server_url_from_env(&env_file)?
        } else {
            let listen = reserve_loopback_address()?;
            let status = self
                .adapter
                .init_local_server(&listen, &data_dir, &env_file, cancellation)
                .await?;
            ensure_desktop_server_defaults(&env_file)?;
            status.probe_url
        };
        let mut server_deadline = Deadline::after(SERVER_READY_TIMEOUT);
        let server_owned = process_is_active(self.process_snapshot(ProcessKey::LocalServer).await);
        let mut stale_loopback_conflict = false;
        let running = if !server_owned {
            match loopback_socket_from_server_url(&server_url) {
                Some(address) => match TcpListener::bind(address) {
                    Ok(listener) => {
                        drop(listener);
                        false
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::AddrInUse | io::ErrorKind::PermissionDenied
                        ) =>
                    {
                        stale_loopback_conflict = true;
                        self.adapter
                            .server_status_until(
                                Some(&server_url),
                                Some(&env_file),
                                None,
                                cancellation,
                                Deadline::after(STALE_LOOPBACK_PROBE_TIMEOUT),
                            )
                            .await
                            .is_ok_and(|status| status.http_reachable)
                    }
                    Err(_) => self
                        .adapter
                        .server_status_until(
                            Some(&server_url),
                            Some(&env_file),
                            None,
                            cancellation,
                            server_deadline,
                        )
                        .await
                        .is_ok_and(|status| status.http_reachable),
                },
                None => self
                    .adapter
                    .server_status_until(
                        Some(&server_url),
                        Some(&env_file),
                        None,
                        cancellation,
                        server_deadline,
                    )
                    .await
                    .is_ok_and(|status| status.http_reachable),
            }
        } else {
            self.adapter
                .server_status_until(
                    Some(&server_url),
                    Some(&env_file),
                    None,
                    cancellation,
                    server_deadline,
                )
                .await
                .is_ok_and(|status| status.http_reachable)
        };
        cancellation.check()?;
        if !running && !server_owned && stale_loopback_conflict {
            if let Some(recovered_url) =
                recover_stale_desktop_loopback_address(&env_file, &server_url)?
            {
                server_url = recovered_url;
                server_deadline = Deadline::after(SERVER_READY_TIMEOUT);
                self.activity.push(
                    ActivityEventKind::StateRecovered,
                    "desktop",
                    ActivityLevel::Warning,
                    "Recovered the local Server from an unavailable saved loopback address",
                );
            }
        }
        let reusable_identity = runner_identity_from_config(&self.config)
            .filter(|identity| same_server(&identity.server_url, &server_url));
        let saved_runner_client_id = stored_runner_client_id(&self.config);
        cancellation.check()?;
        let server_started = if !running {
            if server_deadline.is_elapsed() {
                return Err(readiness_timeout_error(
                    "server_unreachable",
                    "WebCodex Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            let command = self.adapter.local_server_command(&env_file)?;
            self.spawn_owned(ProcessKey::LocalServer, command, false, cancellation)
                .await?;
            true
        } else {
            false
        };
        self.wait_for_server(
            &server_url,
            Some(&env_file),
            None,
            cancellation,
            server_deadline,
            server_started,
        )
        .await?;
        self.snapshot.readiness.server = ServerReadiness::Ready;
        self.publish_snapshot();

        let reusable_observation = match reusable_identity.as_ref() {
            Some(identity) => self
                .adapter
                .observe_runner_connection(
                    identity,
                    saved_runner_client_id.as_deref(),
                    cancellation,
                )
                .await
                .ok(),
            None => None,
        };
        let (runner_identity, identity_replaced, runner_client_id) =
            match (reusable_identity, reusable_observation) {
                (Some(identity), Some(observation)) => (identity, false, observation.client_id),
                _ => {
                    // Fresh local enrollment is Runner-scoped. A default Project is
                    // optional: without one, the Runner keeps the normal empty
                    // allowed_roots configuration whose effective policy defaults to
                    // the user's home directory. Model-driven path resolution can
                    // register concrete Projects later.
                    let connections_dir = local_enrollment_directory(&self.data_dir, &self.config);
                    let pairing_code = self
                        .adapter
                        .create_local_pairing(&server_url, &env_file, cancellation)
                        .await?;
                    let identity = match project.as_ref() {
                        Some(project) => {
                            self.adapter
                                .login_with_pairing(
                                    &server_url,
                                    &pairing_code,
                                    &connections_dir,
                                    project,
                                    cancellation,
                                )
                                .await?
                                .runner
                        }
                        None => {
                            self.adapter
                                .login_runner_with_pairing(
                                    &server_url,
                                    &pairing_code,
                                    &connections_dir,
                                    cancellation,
                                )
                                .await?
                        }
                    };
                    drop(pairing_code);
                    cancellation.check()?;
                    let observation = self
                        .adapter
                        .observe_runner_connection(&identity, None, cancellation)
                        .await?;
                    (identity, true, observation.client_id)
                }
            };

        let replacing_owned_runner = identity_replaced
            && process_is_active(self.process_snapshot(ProcessKey::LocalRunner).await);
        let runner_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
        let activation: DesktopResult<(bool, Option<ProjectRuntimeIdentity>)> = async {
            if replacing_owned_runner {
                // A Desktop-owned Runner can only serve the exact config it was
                // started with. Replace that owned process transactionally while
                // keeping the local Server alive; never broad-kill unrelated Runners.
                self.stop_process_until(ProcessKey::LocalRunner, runner_deadline)
                    .await;
                if runner_deadline.is_elapsed() {
                    return Err(readiness_timeout_error(
                        "runner_offline",
                        "Desktop could not stop its previous Runner before refreshing local runtime configuration",
                        "Retry local runtime setup. Desktop will only replace the Runner it owns.",
                    ));
                }
                cancellation.check()?;
            }

            let runner_ready = if identity_replaced {
                // A fresh Desktop enrollment may reuse the same client_id while
                // changing the Runner token or project registry. An older Runner
                // with that client_id is not proof that this exact config is active.
                false
            } else {
                self.adapter
                    .runner_ready_until(&runner_identity, cancellation, runner_deadline)
                    .await
                    .unwrap_or(false)
            };
            cancellation.check()?;
            let mut runner_started = if !runner_ready {
                if runner_deadline.is_elapsed() {
                    return Err(readiness_timeout_error(
                        "runner_offline",
                        "Runner did not become connected",
                        "Retry local runtime setup to restart Desktop's Runner.",
                    ));
                }
                self.snapshot.readiness.runner = RunnerReadiness::Connecting;
                self.publish_snapshot();
                self.spawn_configured_runner(&runner_identity, cancellation)
                    .await?;
                true
            } else {
                false
            };
            self.wait_for_runner(
                &runner_identity,
                cancellation,
                runner_deadline,
                runner_started,
            )
            .await?;
            self.snapshot.readiness.runner = RunnerReadiness::Ready;
            self.publish_snapshot();

            let Some(project) = project.as_ref() else {
                cancellation.check()?;
                return Ok((runner_started, None));
            };
            let project_identity = match self
                .adapter
                .activate_project(&runner_identity, &runner_client_id, project, cancellation)
                .await
            {
                Ok(identity) => identity,
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "project_activation_capability_unavailable"
                            | "project_activation_restart_required"
                    ) =>
                {
                    if !can_refresh_legacy_runner(
                        self.process_snapshot(ProcessKey::LocalRunner).await,
                    ) {
                        return Err(DesktopError::new(
                            "project_activation_legacy_runner",
                            "This Runner needs to be refreshed before the selected project can be activated",
                            "Refresh the Runner, then retry the project.",
                        ));
                    }
                    let legacy_identity = self
                        .adapter
                        .legacy_register_project(
                            &runner_identity,
                            &runner_client_id,
                            project,
                            cancellation,
                        )
                        .await?;
                    let legacy_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
                    self.stop_process_until(ProcessKey::LocalRunner, legacy_deadline)
                        .await;
                    if legacy_deadline.is_elapsed() {
                        return Err(readiness_timeout_error(
                            "runner_offline",
                            "Desktop could not refresh its legacy Runner",
                            "Retry local runtime setup after checking Runner diagnostics.",
                        ));
                    }
                    self.spawn_configured_runner(&legacy_identity, cancellation)
                        .await?;
                    self.wait_for_runner(&legacy_identity, cancellation, legacy_deadline, true)
                        .await?;
                    runner_started = true;
                    legacy_identity
                }
                Err(error) => return Err(error),
            };
            self.wait_for_project(&project_identity, cancellation).await?;
            cancellation.check()?;
            Ok((runner_started, Some(project_identity)))
        }
        .await;
        let (runner_started, project_identity) = match activation {
            Ok(result) => result,
            Err(error) => {
                if replacing_owned_runner {
                    self.stop_process_until(
                        ProcessKey::LocalRunner,
                        Deadline::after(READINESS_CLEANUP_SLACK),
                    )
                    .await;
                    self.snapshot.topology = self.config.topology.clone();
                    self.snapshot.project = project_snapshot(&self.config);
                    self.snapshot.readiness = aggregate_readiness(
                        ServerReadiness::Ready,
                        RunnerReadiness::Stopped,
                        exposure_readiness(self.config.topology.as_ref()),
                        self.config
                            .project
                            .as_ref()
                            .map(|_| ProjectReadiness::Configured)
                            .unwrap_or(ProjectReadiness::None),
                    );
                    self.publish_snapshot();
                }
                return Err(error);
            }
        };

        let previous_config = self.config.clone();
        let committed_project = match (project.clone(), project_identity.as_ref()) {
            (Some(mut project), Some(identity)) => {
                project.runtime_project_id = Some(identity.runtime_project_id.clone());
                Some(project)
            }
            (None, None) => None,
            _ => {
                return Err(DesktopError::new(
                    "desktop_runtime_contract_invalid",
                    "Local runtime project state is inconsistent",
                    "Open Diagnostics and retry local runtime setup.",
                ))
            }
        };
        self.config.topology = self.snapshot.topology.clone();
        self.config.project = committed_project.clone();
        self.config.runtime_autostart = Some(true);
        self.config.runtime = Some(StoredRuntime {
            server_url: runner_identity.server_url.clone(),
            server_env_file: Some(env_file.clone()),
            runner_config: Some(runner_identity.runner_config.clone()),
            user_token_file: Some(runner_identity.user_token_file.clone()),
            runner_client_id: Some(runner_client_id.clone()),
            project_id: project_identity
                .as_ref()
                .map(|identity| identity.project_id.clone()),
            runtime_project_id: project_identity
                .as_ref()
                .map(|identity| identity.runtime_project_id.clone()),
        });
        if let Err(error) = self.save_config().await {
            self.config = previous_config;
            self.snapshot.topology = self.config.topology.clone();
            self.snapshot.project = project_snapshot(&self.config);
            if runner_started || replacing_owned_runner {
                self.stop_process_until(
                    ProcessKey::LocalRunner,
                    Deadline::after(READINESS_CLEANUP_SLACK),
                )
                .await;
            }
            if server_started {
                self.stop_process_until(
                    ProcessKey::LocalServer,
                    Deadline::after(READINESS_CLEANUP_SLACK),
                )
                .await;
            }
            self.snapshot.readiness = aggregate_readiness(
                if server_started {
                    ServerReadiness::Stopped
                } else {
                    ServerReadiness::Ready
                },
                if runner_started || replacing_owned_runner {
                    RunnerReadiness::Stopped
                } else {
                    RunnerReadiness::Ready
                },
                ExposureReadiness::Disabled,
                self.config
                    .project
                    .as_ref()
                    .map(|_| ProjectReadiness::Configured)
                    .unwrap_or(ProjectReadiness::None),
            );
            self.publish_snapshot();
            return Err(error);
        }
        self.snapshot.project = committed_project;
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::LocalReady,
            if project_identity.is_some() {
                ProjectReadiness::Ready
            } else {
                ProjectReadiness::None
            },
        );
        self.activity.push(
            ActivityEventKind::LocalRuntimeReady,
            "desktop",
            ActivityLevel::Info,
            "Local WebCodex runtime is ready",
        );
        self.autostart_connections(cancellation).await?;
        self.get_state().await
    }
    pub async fn configure_remote_setup(
        &mut self,
        server_url: &str,
        pairing_code: &str,
        project_path: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        self.preflight_providers()?;
        let server_url = crate::webcodex::validate_server_url(server_url)?;
        let project = self.adapter.inspect_project(project_path).await?;
        cancellation.check()?;
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        crate::runtime_selection::verify_resolved_files(&binaries).await?;
        let exposure = if server_url.starts_with("https://") {
            Exposure::ExistingHttps {
                url: server_url.clone(),
            }
        } else {
            Exposure::None
        };
        let topology = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: server_url.clone(),
            },
            runner: RunnerTopology::Local,
            exposure,
            enrollment: Enrollment::ManagedPairing,
        };
        self.snapshot.topology = Some(topology.clone());
        self.stage_project_scope(project.clone());
        self.config.topology = Some(topology.clone());
        self.config.runtime_autostart = Some(true);
        self.config.preferred_connection = Some(RegularConnectionPreference::NoChatGpt);
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            if server_url.starts_with("https://") {
                ExposureReadiness::Starting
            } else {
                ExposureReadiness::Degraded
            },
            ProjectReadiness::Configured,
        );
        self.activity.push(
            ActivityEventKind::RemoteConnecting,
            "desktop",
            ActivityLevel::Info,
            "Connecting this computer to the existing WebCodex Server",
        );
        self.publish_snapshot();

        let reusable_identity = runner_identity_from_config(&self.config)
            .filter(|identity| same_server(&identity.server_url, &server_url));
        let saved_runner_client_id = stored_runner_client_id(&self.config);
        let reusable_observation = match reusable_identity.as_ref() {
            Some(identity) => self
                .adapter
                .observe_runner_connection(
                    identity,
                    saved_runner_client_id.as_deref(),
                    cancellation,
                )
                .await
                .ok(),
            None => None,
        };
        let (identity, identity_replaced, runner_client_id) = match (
            reusable_identity,
            reusable_observation,
        ) {
            (Some(identity), Some(observation)) => (identity, false, observation.client_id),
            _ => {
                if !pairing_code.starts_with("wc_pair_") {
                    return Err(DesktopError::new(
                            "pairing_code_invalid",
                            "The saved Runner identity is not reusable and no new WebCodex pairing code was provided",
                            "Refresh this Runner connection with a new wc_pair_… code.",
                        ));
                }
                let project_identity = self
                    .adapter
                    .login_with_pairing(
                        &server_url,
                        pairing_code,
                        &self.data_dir.join("connections"),
                        &project,
                        cancellation,
                    )
                    .await?;
                let observation = self
                    .adapter
                    .observe_runner_connection(&project_identity, None, cancellation)
                    .await?;
                // A remote pairing code is one-shot. Publish the newly
                // validated connection identity before Runner/project
                // activation so a later readiness failure can retry this
                // same credential instead of forcing another pairing.
                self.config.topology = Some(topology.clone());
                self.store_identity(
                    &project,
                    &project_identity,
                    None,
                    Some(observation.client_id.clone()),
                )
                .await?;
                cancellation.check()?;
                (project_identity.runner, true, observation.client_id)
            }
        };

        let server_deadline = Deadline::after(SERVER_READY_TIMEOUT);
        let server_status = match self
            .adapter
            .server_status_until(
                Some(&server_url),
                None,
                Some(&identity.user_token_file),
                cancellation,
                server_deadline,
            )
            .await
        {
            Ok(status) => status,
            Err(error) => {
                cancellation.check()?;
                if server_deadline.is_elapsed() {
                    return Err(readiness_timeout_error(
                        "server_unreachable",
                        "The existing WebCodex Server did not respond before the readiness deadline",
                        "Check the Server URL and network path, then retry.",
                    ));
                }
                return Err(error);
            }
        };
        cancellation.check()?;
        if !server_status.http_reachable {
            return Err(DesktopError::new(
                "server_unreachable",
                "The existing WebCodex Server is not reachable",
                "Check the Server URL and network path, then retry.",
            ));
        }
        let runner_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
        let replacing_owned_runner = identity_replaced
            && process_is_active(self.process_snapshot(ProcessKey::LocalRunner).await);
        if replacing_owned_runner {
            self.stop_process_until(ProcessKey::LocalRunner, runner_deadline)
                .await;
            cancellation.check()?;
        }
        let runner_ready = if identity_replaced {
            false
        } else {
            self.adapter
                .runner_ready_until(&identity, cancellation, runner_deadline)
                .await
                .unwrap_or(false)
        };
        cancellation.check()?;
        let runner_started = if !runner_ready {
            if runner_deadline.is_elapsed() {
                return Err(readiness_timeout_error(
                    "runner_offline",
                    "Runner did not become connected",
                    "Check Server reachability and Runner diagnostics, then retry.",
                ));
            }
            self.spawn_configured_runner(&identity, cancellation)
                .await?;
            true
        } else {
            false
        };
        self.wait_for_runner(&identity, cancellation, runner_deadline, runner_started)
            .await?;
        let identity = match self
            .adapter
            .activate_project(&identity, &runner_client_id, &project, cancellation)
            .await
        {
            Ok(identity) => identity,
            Err(error)
                if matches!(
                    error.code.as_str(),
                    "project_activation_capability_unavailable"
                        | "project_activation_restart_required"
                ) =>
            {
                if !can_refresh_legacy_runner(self.process_snapshot(ProcessKey::LocalRunner).await)
                {
                    return Err(DesktopError::new(
                        "project_activation_legacy_runner",
                        "This Runner needs to be refreshed before the new project can be activated",
                        "Refresh the Runner, then select this project again.",
                    ));
                }
                let legacy_identity = self
                    .adapter
                    .legacy_register_project(&identity, &runner_client_id, &project, cancellation)
                    .await?;
                let legacy_deadline = Deadline::after(RUNNER_READY_TIMEOUT);
                self.stop_process_until(ProcessKey::LocalRunner, legacy_deadline)
                    .await;
                if legacy_deadline.is_elapsed() {
                    return Err(readiness_timeout_error(
                        "runner_offline",
                        "Desktop could not refresh its legacy Runner",
                        "Retry project setup after checking Runner diagnostics.",
                    ));
                }
                self.spawn_configured_runner(&legacy_identity, cancellation)
                    .await?;
                self.wait_for_runner(&legacy_identity, cancellation, legacy_deadline, true)
                    .await?;
                legacy_identity
            }
            Err(error) => return Err(error),
        };
        self.wait_for_project(&identity, cancellation).await?;
        cancellation.check()?;
        self.config.topology = Some(topology);
        self.store_identity(&project, &identity, None, Some(runner_client_id))
            .await?;
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            if server_url.starts_with("https://") {
                // HTTPS proves transport shape, not that ChatGPT can reach and
                // authenticate to the MCP endpoint. D1 has no canonical remote
                // MCP/handoff probe, so keep this explicitly unverified.
                ExposureReadiness::Unknown
            } else {
                ExposureReadiness::Degraded
            },
            ProjectReadiness::Ready,
        );
        self.activity.push(
            ActivityEventKind::RemoteConnected,
            "desktop",
            ActivityLevel::Info,
            "This computer is connected to the existing WebCodex Server",
        );
        self.get_state().await
    }

    pub async fn start_quick_share(
        &mut self,
        project_path: &str,
        provider: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let project = self.adapter.inspect_project(project_path).await?;
        cancellation.check()?;
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        crate::runtime_selection::verify_resolved_files(&binaries).await?;
        if self
            .process_snapshot(ProcessKey::QuickShare)
            .await
            .is_some_and(|process| {
                matches!(
                    process.phase,
                    ProcessPhase::Starting | ProcessPhase::Running
                )
            })
        {
            return Err(DesktopError::new(
                "quick_share_already_running",
                "Quick Share is already running",
                "Stop the current share before starting another one.",
            ));
        }
        let deadline = Deadline::after(QUICK_SHARE_READY_TIMEOUT);
        let tunnel_proxy = effective_tunnel_proxy(&self.config.tunnel_proxy)?;
        let mut command = self.adapter.quick_share_command(
            Path::new(&project.path),
            provider,
            tunnel_proxy.url.as_deref(),
        )?;
        if provider == "openai" {
            self.tunnel_config.apply_to_command(&mut command)?;
        }
        if deadline.is_elapsed() {
            return Err(readiness_timeout_error(
                "quick_share_not_ready",
                "Quick Share did not reach verified readiness",
                "Check Activity and Tunnel prerequisites, then retry.",
            ));
        }
        let mut events = self
            .spawn_owned(ProcessKey::QuickShare, command, true, cancellation)
            .await?
            .expect("machine stdout requested");
        self.snapshot.topology = Some(RuntimeTopology {
            experience: Experience::QuickShare,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: match provider {
                "cloudflare" => Exposure::Cloudflare,
                "openai" => Exposure::OpenAiTunnel,
                _ => Exposure::None,
            },
            enrollment: Enrollment::ExistingProfile {
                profile: "temporary_share".to_string(),
            },
        });
        self.stage_project_scope(project.clone());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            ExposureReadiness::Starting,
            ProjectReadiness::Configured,
        );
        self.activity.push(
            ActivityEventKind::QuickShareStarting,
            "quick_share",
            ActivityLevel::Info,
            "Starting the temporary Quick Share runtime",
        );
        self.publish_snapshot();
        let event_wait = async {
            while let Some(value) = events.recv().await {
                match value.get("event").and_then(Value::as_str) {
                    Some("ready") => return Ok(Some(value)),
                    Some("machine_event_overflow") => return Err(value),
                    _ => {}
                }
            }
            Ok(None)
        };
        let event_result = tokio::select! {
            biased;
            _ = cancellation.cancelled() => {
                self.stop_process_until(
                    ProcessKey::QuickShare,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                ).await;
                return Err(cancelled_error());
            }
            result = tokio::time::timeout_at(deadline.instant(), event_wait) => {
                result
            }
        };
        let event_value = match event_result {
            Ok(Ok(Some(value))) => value,
            Ok(Err(overflow)) => {
                self.stop_process_until(
                    ProcessKey::QuickShare,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                )
                .await;
                return Err(machine_event_overflow_error(&overflow));
            }
            Ok(Ok(None)) | Err(_) => {
                self.stop_process_until(
                    ProcessKey::QuickShare,
                    Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
                )
                .await;
                return Err(DesktopError::new(
                    "quick_share_not_ready",
                    "Quick Share did not reach verified readiness",
                    "Check Activity and Tunnel prerequisites, then retry.",
                )
                .with_details(serde_json::json!({
                    "category": "readiness_timeout",
                })));
            }
        };
        let event: QuickShareReadyEvent = match serde_json::from_value(event_value) {
            Ok(event) => event,
            Err(_) => {
                self.stop_process(ProcessKey::QuickShare).await;
                return Err(DesktopError::new(
                    "webcodex_contract_invalid",
                    "Quick Share returned an invalid readiness event",
                    "Verify that Desktop and WebCodex binaries come from the same source baseline.",
                ));
            }
        };
        if event.event != "ready"
            || event.schema_version != 1
            || event.experience != "quick_share"
            || event.project.trim().is_empty()
            || event.exposure.kind.trim().is_empty()
        {
            self.stop_process(ProcessKey::QuickShare).await;
            return Err(DesktopError::new(
                "webcodex_contract_invalid",
                "Quick Share readiness identity is incomplete",
                "Update Desktop and WebCodex together.",
            ));
        }
        cancellation.check()?;
        let clipboard_required = event.connection.clipboard_contains != "none";
        let handoff_available = !clipboard_required || event.connection.clipboard_state == "copied";
        let ready_for_chatgpt = event.ready_for_chatgpt && handoff_available;
        self.snapshot.quick_share = Some(QuickShareState {
            provider: provider.to_string(),
            project: project.path.clone(),
            mcp_url: event.connection.mcp_url,
            clipboard_state: event.connection.clipboard_state,
            clipboard_contains: event.connection.clipboard_contains,
            ready_for_chatgpt,
        });
        let exposure_readiness = match event.exposure.state.as_str() {
            "remote_ready" if handoff_available => ExposureReadiness::RemoteReady,
            "remote_ready" => ExposureReadiness::Degraded,
            "local_ready" => ExposureReadiness::LocalReady,
            _ => ExposureReadiness::Unknown,
        };
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            exposure_readiness,
            ProjectReadiness::Ready,
        );
        if !handoff_available {
            self.snapshot.readiness.next_action_kind =
                Some(ReadinessNextActionKind::RestoreClipboardHandoff);
            self.snapshot.readiness.next_action =
                Some("Clipboard handoff is unavailable; restart Quick Share after clipboard access is restored.".to_string());
        }
        self.activity.push(
            ActivityEventKind::QuickShareReady,
            "quick_share",
            ActivityLevel::Info,
            "Quick Share reached verified readiness",
        );
        self.get_state().await
    }

    pub async fn stop_quick_share(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.stop_process(ProcessKey::QuickShare).await;
        self.snapshot.quick_share = None;
        self.snapshot.topology = self.config.topology.clone();
        self.snapshot.project = self.config.project.clone();
        self.activity.push(
            ActivityEventKind::QuickShareStopped,
            "quick_share",
            ActivityLevel::Info,
            "Quick Share stopped",
        );
        if self.config.runtime.is_some() {
            self.refresh_runtime_status(cancellation).await
        } else {
            self.snapshot.readiness = DesktopStateSnapshot::default().readiness;
            self.get_state().await
        }
    }

    pub async fn stop_local_runtime(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if self.config.persistent_environment.is_some() {
            return self.stop_environment(cancellation).await;
        }
        self.stop_all_connection_processes().await?;
        self.stop_process(ProcessKey::LocalRunner).await;
        self.config.runtime_autostart = Some(false);
        self.save_config().await?;
        self.stop_process(ProcessKey::LocalServer).await;
        self.snapshot.topology = self.config.topology.clone();
        let exposure = exposure_readiness(self.config.topology.as_ref());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Stopped,
            RunnerReadiness::Stopped,
            exposure,
            self.config
                .project
                .as_ref()
                .map(|_| ProjectReadiness::Configured)
                .unwrap_or(ProjectReadiness::None),
        );
        self.activity.push(
            ActivityEventKind::RuntimeStopped,
            "desktop",
            ActivityLevel::Info,
            "Desktop-managed local runtime stopped",
        );
        self.get_state().await
    }

    async fn process_snapshot(&self, kind: ProcessKey) -> Option<crate::process::ProcessSnapshot> {
        self.supervisor.lock().await.snapshot(kind)
    }

    async fn spawn_owned(
        &self,
        kind: ProcessKey,
        command: std::process::Command,
        machine_stdout: bool,
        cancellation: &CancellationContext,
    ) -> DesktopResult<Option<MachineEventReceiver>> {
        cancellation.check()?;
        let mut supervisor = self.supervisor.lock().await;
        cancellation.check()?;
        supervisor.spawn_owned(kind, command, machine_stdout).await
    }

    async fn stop_process(&self, kind: ProcessKey) {
        self.supervisor.lock().await.stop(kind).await;
    }

    async fn stop_process_until(&self, kind: ProcessKey, deadline: Deadline) {
        self.supervisor
            .lock()
            .await
            .stop_until(kind, deadline)
            .await;
    }

    async fn wait_for_server(
        &mut self,
        server_url: &str,
        env_file: Option<&Path>,
        token_file: Option<&Path>,
        cancellation: &CancellationContext,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) -> DesktopResult<()> {
        loop {
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKey::LocalServer,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "server_unreachable",
                    "WebCodex Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            if let Some(process) = self.process_snapshot(ProcessKey::LocalServer).await {
                if matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed) {
                    let diagnostics = self.supervisor.lock().await.server_startup_diagnostics();
                    self.cleanup_readiness_process(
                        ProcessKey::LocalServer,
                        deadline,
                        cleanup_owned_process,
                    )
                    .await;
                    return Err(crate::process::startup::server_start_error(
                        process.exit_code,
                        diagnostics.as_ref(),
                    ));
                }
            }
            if self
                .adapter
                .server_status_until(
                    Some(server_url),
                    env_file,
                    token_file,
                    cancellation,
                    deadline,
                )
                .await
                .is_ok_and(|status| status.http_reachable)
            {
                return Ok(());
            }
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKey::LocalServer,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "server_unreachable",
                    "WebCodex Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            sleep_or_cancel_until(POLL_INTERVAL, cancellation, deadline).await?;
        }
    }

    async fn wait_for_runner(
        &mut self,
        identity: &RunnerRuntimeIdentity,
        cancellation: &CancellationContext,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) -> DesktopResult<()> {
        loop {
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKey::LocalRunner,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "runner_offline",
                    "Runner did not become connected",
                    "Check Server reachability and Runner diagnostics, then retry.",
                ));
            }
            if let Some(process) = self.process_snapshot(ProcessKey::LocalRunner).await {
                if matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed) {
                    self.cleanup_readiness_process(
                        ProcessKey::LocalRunner,
                        deadline,
                        cleanup_owned_process,
                    )
                    .await;
                    return Err(DesktopError::new(
                        "runner_offline",
                        "The Desktop-owned Runner exited while connecting",
                        "Open Activity for safe diagnostics and retry.",
                    ));
                }
            }
            if self
                .adapter
                .runner_ready_until(identity, cancellation, deadline)
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }
            cancellation.check()?;
            if deadline.is_elapsed() {
                self.cleanup_readiness_process(
                    ProcessKey::LocalRunner,
                    deadline,
                    cleanup_owned_process,
                )
                .await;
                return Err(readiness_timeout_error(
                    "runner_offline",
                    "Runner did not become connected",
                    "Check Server reachability and Runner diagnostics, then retry.",
                ));
            }
            sleep_or_cancel_until(POLL_INTERVAL, cancellation, deadline).await?;
        }
    }

    async fn wait_for_project(
        &mut self,
        identity: &ProjectRuntimeIdentity,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        let deadline = Deadline::after(PROJECT_READY_TIMEOUT);
        loop {
            cancellation.check()?;
            if deadline.is_elapsed() {
                return Err(project_not_loaded_error());
            }
            if self
                .adapter
                .project_ready_until(identity, cancellation, deadline)
                .await
                .unwrap_or(false)
            {
                return Ok(());
            }
            cancellation.check()?;
            if deadline.is_elapsed() {
                return Err(project_not_loaded_error());
            }
            sleep_or_cancel_until(POLL_INTERVAL, cancellation, deadline).await?;
        }
    }

    async fn cleanup_readiness_process(
        &self,
        kind: ProcessKey,
        deadline: Deadline,
        cleanup_owned_process: bool,
    ) {
        if cleanup_owned_process {
            self.stop_process_until(
                kind,
                Deadline::at(deadline.cleanup_deadline(READINESS_CLEANUP_SLACK)),
            )
            .await;
        }
    }

    async fn store_identity(
        &mut self,
        project: &ProjectSelection,
        identity: &ProjectRuntimeIdentity,
        server_env_file: Option<PathBuf>,
        runner_client_id: Option<String>,
    ) -> DesktopResult<()> {
        let mut project = project.clone();
        project.runtime_project_id = Some(identity.runtime_project_id.clone());
        self.config.project = Some(project.clone());
        self.config.runtime = Some(StoredRuntime {
            server_url: identity.server_url.clone(),
            server_env_file,
            runner_config: Some(identity.runner_config.clone()),
            user_token_file: Some(identity.user_token_file.clone()),
            runner_client_id,
            project_id: Some(identity.project_id.clone()),
            runtime_project_id: Some(identity.runtime_project_id.clone()),
        });
        self.snapshot.project = Some(project);
        self.save_config().await
    }

    async fn save_config(&mut self) -> DesktopResult<()> {
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|_| {
                DesktopError::new(
                    "desktop_state_unavailable",
                    "Desktop cannot create its app-data directory",
                    "Check local filesystem permissions and retry.",
                )
            })?;
        if let (Some(project), Some(path)) = (
            &self.config.project,
            self.config
                .runtime
                .as_ref()
                .and_then(|r| r.runner_config.as_ref()),
        ) {
            if webcodex_runner_config::paths::paths_equal(
                Path::new(&project.path),
                Path::new(&project.allowed_root),
            ) {
                self.config.saved_projects.retain(|entry| {
                    !(entry.runner_config == *path
                        && webcodex_runner_config::paths::paths_equal(
                            Path::new(&entry.project.path),
                            Path::new(&project.path),
                        ))
                });
                self.config
                    .saved_projects
                    .push(crate::models::SavedProject {
                        project: project.clone(),
                        runner_config: path.clone(),
                    });
                if self.config.saved_projects.len() > 64 {
                    self.config.saved_projects.remove(0);
                }
            }
        }
        let encoded = serde_json::to_vec_pretty(&self.config).map_err(|_| {
            DesktopError::new(
                "desktop_state_invalid",
                "Desktop could not encode its non-secret runtime state",
                "Retry the setup operation.",
            )
        })?;
        let config_path = self.config_path.clone();
        tokio::task::spawn_blocking(move || save_config_atomically(&config_path, &encoded))
            .await
            .map_err(|_| desktop_state_unavailable("Desktop state persistence worker stopped"))??;
        Ok(())
    }
}

