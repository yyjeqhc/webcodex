impl AppState {
    pub fn new(data_dir: PathBuf, resource_dir: PathBuf) -> DesktopResult<Self> {
        let desktop_data_dir = crate::desktop_data_dir::DesktopDataDir {
            effective: data_dir,
            source: crate::desktop_data_dir::DesktopDataDirSource::Tauri,
            physical_resolution_changed: false,
        };
        Self::new_resolved(desktop_data_dir, resource_dir)
    }

    pub fn new_resolved(
        desktop_data_dir: crate::desktop_data_dir::DesktopDataDir,
        resource_dir: PathBuf,
    ) -> DesktopResult<Self> {
        let data_dir = desktop_data_dir.effective.clone();
        let managed_instructions = Arc::new(crate::managed_instructions::ManagedInstructions::new(
            data_dir.clone(),
        ));
        let updates = crate::updates::UpdateManager::new(data_dir.clone());
        let core = DesktopCore::new(data_dir, resource_dir)?;
        let published = Arc::clone(&core.published);
        let supervisor = Arc::clone(&core.supervisor);
        let activity = core.activity.clone();
        let connections = core.connections.clone();
        Ok(Self {
            core: Mutex::new(Some(core)),
            desktop_data_dir,
            managed_instructions,
            ssh_resources: Mutex::new(crate::ssh_resources::SshResourcesManager::default()),
            published,
            supervisor,
            connections,
            operations: OperationController::new(activity.clone()),
            activity,
            shutdown_signal: CancellationSignal::new(),
            shutdown_started: AtomicBool::new(false),
            update_check: tokio::sync::Mutex::new(()),
            updates,
        })
    }

    pub fn get_state(&self) -> DesktopStateSnapshot {
        let mut snapshot = self
            .published
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        // Native services own persistent runtime observations. Desktop's process
        // registry must not overwrite them with absent or historical processes.
        if snapshot.persistent_environment.is_none() {
            if let Ok(mut supervisor) = self.supervisor.try_lock() {
                let mut server = snapshot.readiness.server.clone();
                let mut runner = snapshot.readiness.runner.clone();
                if supervisor
                    .snapshot(ProcessKey::LocalServer)
                    .is_some_and(|p| matches!(p.phase, ProcessPhase::Exited | ProcessPhase::Failed))
                {
                    server = ServerReadiness::Error;
                }
                if supervisor
                    .snapshot(ProcessKey::LocalRunner)
                    .is_some_and(|p| matches!(p.phase, ProcessPhase::Exited | ProcessPhase::Failed))
                {
                    runner = RunnerReadiness::Error;
                }
                if server != snapshot.readiness.server || runner != snapshot.readiness.runner {
                    snapshot.readiness = aggregate_readiness(
                        server,
                        runner,
                        snapshot.readiness.exposure.clone(),
                        snapshot.readiness.project.clone(),
                    );
                }
            }
            self.connections
                .project(&mut snapshot.connections, snapshot.readiness.runtime_ready);
        }
        snapshot.current_operation = self.operations.current();
        snapshot.activity_sequence = self.activity.latest_sequence();
        if snapshot.openai_tunnel_config.source == crate::models::TunnelConfigSource::Environment {
            snapshot.openai_tunnel_config = crate::tunnel_config::environment_snapshot();
            snapshot.openai_tunnel_configured = snapshot.openai_tunnel_config.is_configured();
        }
        snapshot.regular_tunnel_available = true;
        snapshot.powershell_runtime = crate::platform::powershell_runtime_snapshot();
        snapshot
    }

    pub fn activity(&self) -> Vec<crate::activity::ActivityEntry> {
        self.activity.snapshot()
    }

    pub async fn inspect_project(&self, path: &str) -> DesktopResult<ProjectSelection> {
        inspect_project_path(path).await
    }

    pub async fn refresh_runtime_status(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeRefresh, true)
            .await?;
        let result = core.refresh_runtime_status(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn observe_chatgpt_activity(&self) -> DesktopResult<DesktopStateSnapshot> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        if self.operations.current().is_some() {
            return Ok(self.get_state());
        }
        let cancellation =
            CancellationContext::new(CancellationSignal::new(), self.shutdown_signal.clone());
        let probe = {
            let slot = self.core.lock().await;
            let Some(core) = slot.as_ref() else {
                return Ok(self.get_state());
            };
            let Some(probe) = core.chatgpt_activity_probe() else {
                return Ok(self.get_state());
            };
            probe
        };
        let observation = WebCodexAdapter::chatgpt_activity_with_binary(
            &probe.webcodex,
            &probe.identity,
            &cancellation,
        )
        .await;
        cancellation.check()?;
        let Ok(last_meaningful_activity_at_ms) = observation else {
            return Ok(self.get_state());
        };
        if self.operations.current().is_some() {
            return Ok(self.get_state());
        }
        let mut slot = self.core.lock().await;
        let Some(core) = slot.as_mut() else {
            return Ok(self.get_state());
        };
        core.apply_chatgpt_activity_observation(&probe.identity, last_meaningful_activity_at_ms)
    }

    pub async fn resume_saved_runtime(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeResume, true)
            .await?;
        let result = core.resume_saved_runtime(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn update_tunnel_config(
        &self,
        request: TunnelConfigRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelConfigUpdate, false)
            .await?;
        let result = async {
            cancellation.check()?;
            let previous = core.tunnel_config.clone();
            core.mutate_tunnel_config(move |config, path| config.update(path, request))
                .await?;
            let id = crate::connection_id::TunnelProfileId::DEFAULT;
            let active =
                process_is_active(core.process_snapshot(ProcessKey::RegularTunnel(id)).await);
            if active && !core.tunnel_config.same_launch_as(&previous, id) {
                core.stop_connection_process(id)
                    .await
                    .map_err(tunnel_apply_error)?;
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

    pub async fn update_tunnel_proxy(
        &self,
        mode: TunnelProxyMode,
        custom_url: Option<&str>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TunnelProxyUpdate, false)
            .await?;
        let result = core
            .update_tunnel_proxy(mode, custom_url, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn configure_local_setup(
        &self,
        project_path: Option<&str>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.configure_environment(crate::models::EnvironmentInput {
            service_scope: None,
            mode: "create".into(),
            server_url: None,
            project_path: project_path.map(str::to_owned),
            runner: Some(true),
            pairing_code: None,
            user_token: None,
            replace_pairing_code: false,
        })
        .await
    }

    pub async fn configure_environment(
        &self,
        input: crate::models::EnvironmentInput,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let migration = self
            .get_state()
            .topology
            .as_ref()
            .is_some_and(|topology| topology.experience == Experience::Full)
            && self.get_state().persistent_environment.is_none();
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(
                if migration {
                    DesktopOperationKind::EnvironmentMigration
                } else if input.mode == "create" {
                    DesktopOperationKind::LocalSetup
                } else {
                    DesktopOperationKind::RemoteSetup
                },
                false,
            )
            .await?;
        let result = core.configure_environment(input, &cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn activate_local_project(
        &self,
        project_path: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalProjectActivate, true)
            .await?;
        let result = if core.config.persistent_environment.is_some() {
            core.add_environment_project(project_path, &cancellation)
                .await
        } else {
            core.activate_local_project(project_path, &cancellation)
                .await
        };
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn configure_remote_setup(
        &self,
        server_url: &str,
        pairing_code: &str,
        project_path: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.configure_environment(crate::models::EnvironmentInput {
            service_scope: None,
            mode: "join".into(),
            server_url: Some(server_url.to_owned()),
            project_path: Some(project_path.to_owned()),
            runner: Some(true),
            pairing_code: Some(pairing_code.to_owned()),
            user_token: None,
            replace_pairing_code: false,
        })
        .await
    }

    pub async fn start_quick_share(
        &self,
        project_path: &str,
        provider: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::QuickShareStart, true)
            .await?;
        let result = core
            .start_quick_share(project_path, provider, &cancellation)
            .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn stop_quick_share(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::QuickShareStop, false)
            .await?;
        let result = core.stop_quick_share(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn start_regular_tunnel(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RegularTunnelStart, true)
            .await?;
        let result = core.start_regular_tunnel(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn stop_regular_tunnel(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RegularTunnelStop, false)
            .await?;
        let result = core.stop_regular_tunnel(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn stop_local_runtime(&self) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalRuntimeStop, false)
            .await?;
        let result = core.stop_local_runtime(&cancellation).await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub fn cancel_operation(&self, operation_id: &str) -> DesktopResult<DesktopStateSnapshot> {
        self.operations.cancel(operation_id)?;
        Ok(self.get_state())
    }

    pub async fn shutdown(&self) {
        if self.shutdown_started.swap(true, Ordering::SeqCst) {
            return;
        }
        self.shutdown_signal.cancel();
        self.updates.cancel_download(false);
        self.operations.cancel_active_for_shutdown();
        self.connections.cancel_all();
        self.supervisor.lock().await.stop_all().await;
        let _ = self
            .operations
            .wait_until_idle(tokio::time::Instant::now() + SHUTDOWN_OPERATION_WAIT)
            .await;
    }

    async fn begin_operation(
        &self,
        kind: DesktopOperationKind,
        cancellable: bool,
    ) -> DesktopResult<(
        OperationAdmission,
        CancellationContext,
        DesktopCore,
        ProcessBaseline,
    )> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        if kind != DesktopOperationKind::ConfigurationRestore
            && self.get_state().configuration_issue.is_some()
        {
            return Err(DesktopError::new(
                "configuration_migration_failed",
                "Configuration could not be migrated",
                "Open Diagnostics or explicitly restore the previous known-good configuration.",
            ));
        }
        let operation = self.operations.admit(kind, cancellable)?;
        let cancellation =
            CancellationContext::new(operation.cancellation.clone(), self.shutdown_signal.clone());
        let baseline = self.capture_process_baseline().await;
        let core = {
            let mut slot = self.core.lock().await;
            slot.take()
        };
        let Some(core) = core else {
            let error = DesktopError::new(
                "desktop_operation_busy",
                "Desktop mutation state is already in use",
                "Wait for the current operation to finish.",
            );
            let result: DesktopResult<()> = Err(error.clone());
            self.operations.finish(&operation.id, &result);
            return Err(error);
        };
        Ok((operation, cancellation, core, baseline))
    }

    async fn finish_operation(
        &self,
        operation: OperationAdmission,
        cancellation: CancellationContext,
        mut core: DesktopCore,
        baseline: ProcessBaseline,
        mut result: DesktopResult<DesktopStateSnapshot>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        if result.is_ok() && cancellation.is_cancelled() {
            result = Err(cancelled_error());
        }
        let completion = operation_completion::OperationCompletion::new(
            operation.kind, result.as_ref().err(),
        );
        let cleanup = if completion.requires_process_cleanup() {
            Some(self.cleanup_new_owned_processes(&baseline).await)
        } else {
            None
        };
        if completion.apply_failure(&mut core.snapshot, &baseline.snapshot, cleanup) {
            core.publish_snapshot();
        }
        if completion.apply_runtime_error(&mut core.snapshot, core.runtime_last_switch.as_ref()) {
            core.publish_snapshot();
        }
        {
            let mut slot = self.core.lock().await;
            *slot = Some(core);
        }
        self.operations.finish(&operation.id, &result);
        match result {
            Ok(_) => Ok(self.get_state()),
            Err(error) => Err(error),
        }
    }

    async fn capture_process_baseline(&self) -> ProcessBaseline {
        let snapshot = self
            .published
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let mut supervisor = self.supervisor.lock().await;
        let generations = supervisor
            .keys()
            .into_iter()
            .filter_map(|key| {
                supervisor
                    .snapshot(key)
                    .map(|process| (key, process.generation))
            })
            .collect();
        ProcessBaseline {
            generations,
            snapshot,
        }
    }

    async fn cleanup_new_owned_processes(&self, baseline: &ProcessBaseline) -> ProcessCleanup {
        let mut supervisor = self.supervisor.lock().await;
        let mut cleanup = ProcessCleanup::default();
        let mut keys = supervisor.keys();
        keys.sort_by_key(|key| match key {
            ProcessKey::QuickShare => 0,
            ProcessKey::RegularTunnel(_) => 1,
            ProcessKey::LocalRunner => 2,
            ProcessKey::LocalServer => 3,
        });
        for key in keys {
            let Some(process) = supervisor.snapshot(key) else {
                continue;
            };
            if baseline.generations.get(&key) != Some(&process.generation) {
                if let ProcessKey::RegularTunnel(id) = key {
                    self.connections.stopping(id);
                }
                supervisor.stop_generation(key, process.generation).await;
                if let ProcessKey::RegularTunnel(id) = key {
                    self.connections
                        .stopped(id, supervisor.snapshot(key).is_none());
                }
                cleanup.mark_stopped(key);
            }
        }
        cleanup
    }

    #[cfg(test)]
    async fn hold_test_operation(
        &self,
        started: tokio::sync::oneshot::Sender<String>,
        cleanup_release: tokio::sync::oneshot::Receiver<()>,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, core, baseline) = self
            .begin_operation(DesktopOperationKind::LocalSetup, true)
            .await?;
        let _ = started.send(operation.id.clone());
        cancellation.cancelled().await;
        let _ = cleanup_release.await;
        let result: DesktopResult<DesktopStateSnapshot> = Err(cancelled_error());
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    #[cfg(all(test, target_os = "macos"))]
    async fn run_test_one_shot_operation(
        &self,
        executable: PathBuf,
        args: Vec<String>,
        payload: Vec<u8>,
        timeout: Duration,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeRefresh, true)
            .await?;
        let result = crate::webcodex::run_test_bounded(
            &executable,
            &args,
            Some(&payload),
            &cancellation,
            timeout,
        )
        .await
        .map(|_| core.publish_snapshot());
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

#[derive(Clone)]
struct ProcessBaseline {
    generations: std::collections::HashMap<ProcessKey, u64>,
    snapshot: DesktopStateSnapshot,
}

#[derive(Clone, Copy, Default)]
struct ProcessCleanup {
    local_server: bool,
    local_runner: bool,
    quick_share: bool,
    regular_tunnel: bool,
}

impl ProcessCleanup {
    fn mark_stopped(&mut self, kind: ProcessKey) {
        match kind {
            ProcessKey::LocalServer => self.local_server = true,
            ProcessKey::LocalRunner => self.local_runner = true,
            ProcessKey::QuickShare => self.quick_share = true,
            ProcessKey::RegularTunnel(_) => self.regular_tunnel = true,
        }
    }
}

fn project_not_loaded_error() -> DesktopError {
    readiness_timeout_error(
        "project_not_loaded",
        "The selected project did not become ready on the current Runner",
        "Retry project activation after checking Runner and project diagnostics.",
    )
}

fn process_is_active(snapshot: Option<crate::process::ProcessSnapshot>) -> bool {
    snapshot.is_some_and(|process| {
        matches!(
            process.phase,
            ProcessPhase::Starting | ProcessPhase::Running | ProcessPhase::Stopping
        )
    })
}

fn can_refresh_legacy_runner(snapshot: Option<crate::process::ProcessSnapshot>) -> bool {
    snapshot.is_some_and(|process| {
        process.owned_by_desktop
            && matches!(
                process.phase,
                ProcessPhase::Starting | ProcessPhase::Running | ProcessPhase::Stopping
            )
    })
}
