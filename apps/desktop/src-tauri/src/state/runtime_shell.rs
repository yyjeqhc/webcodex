use super::*;
use crate::runtime_selection::{
    self, RuntimeSettings, RuntimeSource, RuntimeSwitchRequest, RuntimeSwitchResult,
};
use crate::webcodex::cli::ResolvedBinaries;
use webcodex_core::desktop_runtime_contract::{ProtocolCompatibility, DESKTOP_RUNTIME_CONTRACT};

impl AppState {
    pub async fn runtime_settings(&self) -> DesktopResult<RuntimeSettings> {
        let (mut settings, runtime, identity) = {
            let slot = self.core.lock().await;
            let core = slot
                .as_ref()
                .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?;
            (
                core.runtime_settings_snapshot().await,
                core.config.runtime.clone(),
                runner_identity_from_config(&core.config),
            )
        };
        if let Some(runtime) = runtime {
            settings.active_jobs =
                crate::workspace::query(&runtime, crate::workspace::WorkspaceRequest::Overview {})
                    .await
                    .ok()
                    .filter(|v| v.get("connected").and_then(Value::as_bool) == Some(true))
                    .and_then(|v| observed_active_jobs(&v));
        } else {
            settings.active_jobs = Some(0);
        }
        let slot = self.core.lock().await;
        if slot.as_ref().is_none_or(|core| {
            core.config.runtime_selection_revision != settings.selection_revision
                || runner_identity_from_config(&core.config) != identity
        }) {
            return Err(runtime_selection::error("runtime_selection_changed"));
        }
        Ok(settings)
    }

    pub async fn probe_runtime(&self, source: RuntimeSource) -> DesktopResult<RuntimeSettings> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeProbe, true)
            .await?;
        let result = async {
            let (candidate, _) = runtime_selection::probe(
                source,
                core.adapter.bundled_runtime_dir(),
                core.config.runtime_selection_revision,
                &cancellation,
                Deadline::after(Duration::from_secs(30)),
            )
            .await?;
            core.runtime_candidate_context = Some(selection_context(&core.config));
            core.runtime_candidate = Some(candidate);
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await?;
        self.runtime_settings().await
    }

    pub async fn recheck_runtime(&self) -> DesktopResult<RuntimeSettings> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeProbe, true)
            .await?;
        let result = async {
            let (mut candidate, resolved) = runtime_selection::probe(
                core.config.runtime_binary_source.clone(),
                core.adapter.bundled_runtime_dir(),
                core.config.runtime_selection_revision,
                &cancellation,
                Deadline::after(Duration::from_secs(30)),
            )
            .await?;
            if let (Ok(active), Some(disk)) = (core.adapter.binaries(), resolved.as_ref()) {
                if active.fingerprint != disk.fingerprint {
                    candidate
                        .advisories
                        .push("runtime_files_changed_restart_required".into());
                }
            }
            // Observation alone does not replace a running executable set.
            core.runtime_selected_probe = Some(candidate);
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await?;
        self.runtime_settings().await
    }

    pub async fn switch_runtime(
        &self,
        request: RuntimeSwitchRequest,
    ) -> DesktopResult<RuntimeSwitchResult> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RuntimeSwitch, false)
            .await?;
        let result = core
            .switch_runtime_transaction(request, &cancellation)
            .await;
        // A controlled rollback is an explicit observation, not a new failed
        // admission. Generic error cleanup must not kill the restored generation.
        let state_result = match &result {
            Ok(_) => core.get_state().await,
            Err(error) => Err(error.clone()),
        };
        self.finish_operation(operation, cancellation, core, baseline, state_result)
            .await?;
        result
    }

    pub async fn restore_previous_configuration(
        &self,
        expected_primary_sha256: String,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::ConfigurationRestore, false)
            .await?;
        let result = async {
            if core.configuration_issue.is_none() { return Err(runtime_selection::error("configuration_restore_not_required")); }
            let path = core.config_path.clone();
            let config = tokio::task::spawn_blocking(move || {
                if configuration_file_fingerprint(&path)? != expected_primary_sha256 { return Err(runtime_selection::error("configuration_changed")); }
                let StoredConfigFile::Valid { config, bytes } = read_stored_config(&desktop_state_backup_path(&path))? else {
                    return Err(runtime_selection::error("configuration_backup_unavailable"));
                };
                write_atomic_file(&path, &bytes).map_err(|_| runtime_selection::error("configuration_restore_failed"))?;
                Ok::<_, DesktopError>(config)
            }).await.map_err(|_| runtime_selection::error("configuration_restore_failed"))??;
            core.config = config;
            core.configuration_issue = None;
            core.snapshot.configuration_issue = None;
            core.adapter.set_runtime_source(core.config.runtime_binary_source.clone());
            core.adapter.set_runtime_approval(core.config.runtime_binary_fingerprint.clone());
            core.snapshot.topology = core.config.topology.clone();
            core.snapshot.project = project_snapshot(&core.config);
            core.snapshot.readiness = aggregate_readiness(ServerReadiness::Stopped, RunnerReadiness::Stopped, ExposureReadiness::Disabled, ProjectReadiness::Configured);
            core.activity.push(ActivityEventKind::StateRecovered, "desktop_state", ActivityLevel::Warning, "Previous known-good configuration restored by the local operator; Runtime has not been started");
            core.get_state().await
        }.await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

pub(super) fn observed_active_jobs(value: &Value) -> Option<u64> {
    if value.get("connected").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    value
        .get("jobs_running")?
        .as_u64()?
        .checked_add(value.get("jobs_queued")?.as_u64()?)
}

fn selection_context(config: &StoredDesktopConfig) -> String {
    use sha2::{Digest, Sha256};
    // Update-check cache and UI preferences do not retarget execution. Runtime,
    // Project and connection identity do, even if the binary-source revision did
    // not change while a candidate preview was open.
    let value = serde_json::json!({"runtime":config.runtime,"project":config.project,"topology":config.topology,
        "source":config.runtime_binary_source,"fingerprint":config.runtime_binary_fingerprint,
        "revision":config.runtime_selection_revision});
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&value).expect("stored identity is serializable"))
    )
}

pub(super) fn configuration_file_fingerprint(path: &Path) -> DesktopResult<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    match std::fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(DESKTOP_STATE_MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| runtime_selection::error("configuration_unreadable"))?;
            if bytes.len() > DESKTOP_STATE_MAX_BYTES as usize {
                return Err(runtime_selection::error("configuration_unreadable"));
            }
            Ok(format!("{:x}", Sha256::digest(bytes)))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok("missing".into()),
        Err(_) => Err(runtime_selection::error("configuration_unreadable")),
    }
}

impl DesktopCore {
    pub(super) async fn runtime_settings_snapshot(&self) -> RuntimeSettings {
        let selected = self.runtime_selected_probe.clone().or_else(|| {
            self.adapter.binaries().ok().map(|binaries| {
                runtime_selection::candidate_from_resolved(
                    binaries,
                    self.config.runtime_binary_source.clone(),
                    self.config.runtime_selection_revision,
                )
            })
        });
        let switch_reason = self
            .runtime_switch_authority()
            .await
            .err()
            .map(|error| error.code);
        RuntimeSettings {
            source: self.config.runtime_binary_source.clone(),
            selection_revision: self.config.runtime_selection_revision,
            desktop_contract: DESKTOP_RUNTIME_CONTRACT,
            selected,
            candidate: self.runtime_candidate.clone(),
            previous_source: self.config.previous_runtime_source.clone(),
            last_switch: self.runtime_last_switch.clone(),
            unavailable_code: self.configuration_issue.clone().or_else(|| {
                self.runtime_selected_probe
                    .as_ref()
                    .and_then(|p| p.error_code.clone())
            }),
            active_jobs: None,
            can_switch: switch_reason.is_none(),
            switch_unavailable_reason: switch_reason,
        }
    }

    pub(super) async fn runtime_switch_authority(&self) -> DesktopResult<()> {
        if self.configuration_issue.is_some() {
            return Err(runtime_selection::error("configuration_migration_failed"));
        }
        if self.snapshot.quick_share.is_some()
            || self
                .config
                .topology
                .as_ref()
                .is_some_and(|t| t.experience != Experience::Full)
        {
            return Err(runtime_selection::error("quick_share_must_be_stopped"));
        }
        if self.config.runtime.is_none() {
            return Ok(());
        }
        if matches!(
            self.config.topology.as_ref().map(|t| &t.server),
            Some(ServerTopology::Local)
        ) && self
            .process_snapshot(ProcessKey::LocalServer)
            .await
            .is_none()
        {
            let runtime = self.config.runtime.as_ref().expect("checked");
            let address = loopback_socket_from_server_url(&runtime.server_url)
                .ok_or_else(|| runtime_selection::error("server_not_owned"))?;
            if TcpListener::bind(address).is_err() {
                return Err(runtime_selection::error("server_not_owned"));
            }
        }
        for key in [ProcessKey::LocalRunner, ProcessKey::LocalServer] {
            if self
                .process_snapshot(key)
                .await
                .is_some_and(|p| !p.owned_by_desktop)
            {
                return Err(runtime_selection::error("runtime_not_owned"));
            }
        }
        Ok(())
    }

    pub(super) async fn persist_runtime_preferences(&self) -> DesktopResult<()> {
        let encoded = serde_json::to_vec_pretty(&self.config)
            .map_err(|_| runtime_selection::error("desktop_state_invalid"))?;
        let path = self.config_path.clone();
        tokio::task::spawn_blocking(move || save_config_atomically(&path, &encoded))
            .await
            .map_err(|_| runtime_selection::error("desktop_state_unavailable"))?
    }

    async fn switch_runtime_transaction(
        &mut self,
        request: RuntimeSwitchRequest,
        cancellation: &CancellationContext,
    ) -> DesktopResult<RuntimeSwitchResult> {
        self.runtime_switch_authority().await?;
        if self.runtime_candidate_context.as_deref()
            != Some(selection_context(&self.config).as_str())
        {
            return Err(runtime_selection::error(
                "runtime_selection_context_changed",
            ));
        }
        if request.expected_selection_revision != self.config.runtime_selection_revision {
            return Err(runtime_selection::error("runtime_selection_changed"));
        }
        let candidate = self
            .runtime_candidate
            .clone()
            .filter(|p| {
                p.candidate_id == request.candidate_id
                    && p.selection_revision == request.expected_selection_revision
            })
            .ok_or_else(|| runtime_selection::error("runtime_candidate_stale"))?;
        if candidate.compatibility != ProtocolCompatibility::Compatible {
            return Err(runtime_selection::error("runtime_candidate_incompatible"));
        }
        let (fresh, resolved) = runtime_selection::probe(
            candidate.source.clone(),
            self.adapter.bundled_runtime_dir(),
            self.config.runtime_selection_revision,
            cancellation,
            Deadline::after(Duration::from_secs(30)),
        )
        .await?;
        let resolved = resolved.ok_or_else(|| {
            runtime_selection::error(
                fresh
                    .error_code
                    .as_deref()
                    .unwrap_or("runtime_candidate_incompatible"),
            )
        })?;
        if candidate.fingerprint != fresh.fingerprint || candidate.directory != fresh.directory {
            return Err(runtime_selection::error("runtime_candidate_changed"));
        }
        runtime_selection::verify_resolved_files(&resolved).await?;
        let previous_config = self.config.clone();
        let previous_binaries = self.adapter.binaries().ok().cloned();
        let local = matches!(
            self.config.topology.as_ref().map(|t| &t.server),
            Some(ServerTopology::Local)
        );
        let identity = runner_identity_from_config(&self.config);
        if self.config.runtime.is_some() && identity.is_none() {
            return Err(runtime_selection::error("runtime_identity_unavailable"));
        }
        if let Some(runtime) = self.config.runtime.as_ref() {
            let own_runner = self
                .process_snapshot(ProcessKey::LocalRunner)
                .await
                .is_some();
            let observed =
                crate::workspace::query(runtime, crate::workspace::WorkspaceRequest::Overview {})
                    .await
                    .ok();
            if !own_runner
                && observed
                    .as_ref()
                    .and_then(|v| v.get("connected"))
                    .and_then(Value::as_bool)
                    == Some(true)
            {
                return Err(runtime_selection::error("runner_not_owned"));
            }
            let active = observed.as_ref().and_then(observed_active_jobs);
            if own_runner && active != Some(0) && !request.confirm_interrupt {
                return Err(
                    runtime_selection::error("runtime_switch_jobs_confirmation_required")
                        .with_details(serde_json::json!({"active_jobs": active})),
                );
            }
        }
        cancellation.check()?;
        let mut transaction = RuntimeSwitchExecution {
            core: self,
            candidate_source: candidate.source.clone(),
            candidate_binaries: resolved,
            previous_config: previous_config.clone(),
            previous_binaries,
            identity: identity.clone(),
            local,
            cancellation,
        };
        let observed = runtime_selection::lifecycle::execute(&mut transaction).await;
        drop(transaction);
        let outcome = match observed {
            runtime_selection::lifecycle::Outcome::Activated => {
                self.runtime_selected_probe = Some(fresh);
                self.runtime_candidate = None;
                self.runtime_candidate_context = None;
                RuntimeSwitchResult {
                    outcome: if identity.is_some() {
                        "activated"
                    } else {
                        "selected"
                    }
                    .into(),
                    reason_code: None,
                    rollback_reason_code: None,
                    selection_revision: self.config.runtime_selection_revision,
                    restart_required: identity.is_none(),
                }
            }
            runtime_selection::lifecycle::Outcome::RolledBack { reason } => RuntimeSwitchResult {
                outcome: "rolled_back".into(),
                reason_code: Some(reason),
                rollback_reason_code: None,
                selection_revision: self.config.runtime_selection_revision,
                restart_required: false,
            },
            runtime_selection::lifecycle::Outcome::RecoveryRequired {
                reason,
                rollback_reason,
            } => {
                self.config = previous_config.clone();
                self.adapter
                    .set_runtime_source(previous_config.runtime_binary_source.clone());
                self.adapter
                    .set_runtime_approval(previous_config.runtime_binary_fingerprint.clone());
                self.snapshot.readiness.runtime_ready = false;
                if local {
                    self.snapshot.readiness.server = ServerReadiness::Error;
                }
                self.snapshot.readiness.runner = RunnerReadiness::Error;
                RuntimeSwitchResult {
                    outcome: "recovery_required".into(),
                    reason_code: Some(reason),
                    rollback_reason_code: Some(rollback_reason),
                    selection_revision: self.config.runtime_selection_revision,
                    restart_required: true,
                }
            }
        };
        self.runtime_last_switch = Some(outcome.clone());
        self.snapshot.binaries = self.adapter.binaries().ok().map(ResolvedBinaries::info);
        self.publish_snapshot();
        Ok(outcome)
    }

    pub(super) async fn stop_owned_runtime_for_switch(
        &mut self,
        local_server: bool,
    ) -> DesktopResult<()> {
        self.supervisor
            .lock()
            .await
            .stop_checked(ProcessKey::LocalRunner)
            .await?;
        if local_server {
            self.supervisor
                .lock()
                .await
                .stop_checked(ProcessKey::LocalServer)
                .await?;
        }
        Ok(())
    }

    /// Existing identity only: no pairing, enrollment, registry mutation, project
    /// activation, port substitution, or credential regeneration on binary switch.
    pub(super) async fn start_existing_runtime(
        &mut self,
        identity: &RunnerRuntimeIdentity,
        local_server: bool,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        let deadline = Deadline::after(Duration::from_secs(50));
        let runtime = self
            .config
            .runtime
            .clone()
            .ok_or_else(|| runtime_selection::error("runtime_identity_unavailable"))?;
        self.snapshot.readiness.runtime_ready = false;
        self.snapshot.readiness.runner = RunnerReadiness::Connecting;
        if local_server {
            let env = runtime
                .server_env_file
                .as_ref()
                .ok_or_else(|| runtime_selection::error("server_configuration_unavailable"))?;
            let address = loopback_socket_from_server_url(&runtime.server_url)
                .ok_or_else(|| runtime_selection::error("server_not_owned"))?;
            let port = TcpListener::bind(address)
                .map_err(|_| runtime_selection::error("server_port_in_use"))?;
            drop(port);
            self.snapshot.readiness.server = ServerReadiness::Starting;
            self.publish_snapshot();
            let command = self.adapter.local_server_command(env)?;
            self.spawn_owned(ProcessKey::LocalServer, command, false, cancellation)
                .await?;
        }
        self.wait_for_server(
            &identity.server_url,
            runtime.server_env_file.as_deref(),
            Some(&identity.user_token_file),
            cancellation,
            deadline,
            local_server,
        )
        .await?;
        let mut command = self.adapter.local_runner_command(&identity.runner_config)?;
        self.mcp_providers.apply_to_command(&mut command)?;
        self.spawn_owned(ProcessKey::LocalRunner, command, false, cancellation)
            .await?;
        self.wait_for_runner(identity, cancellation, deadline, true)
            .await?;
        runtime_selection::verify_resolved_files(self.adapter.binaries()?).await?;
        let observed = self
            .adapter
            .server_status_until(
                Some(&identity.server_url),
                runtime.server_env_file.as_deref(),
                Some(&identity.user_token_file),
                cancellation,
                deadline,
            )
            .await?;
        if local_server {
            let process = self
                .process_snapshot(ProcessKey::LocalServer)
                .await
                .ok_or_else(|| runtime_selection::error("selected_runtime_not_active"))?;
            if process.pid != observed.server_pid {
                return Err(runtime_selection::error("selected_runtime_not_active"));
            }
            let expected = self
                .adapter
                .binaries()?
                .builds
                .iter()
                .find(|b| b.binary == "webcodex-server")
                .ok_or_else(|| runtime_selection::error("build_info_unverifiable"))?;
            if observed
                .server_build
                .as_ref()
                .and_then(|v| v.get("git_commit"))
                .and_then(Value::as_str)
                != expected.git_commit.as_deref()
            {
                return Err(runtime_selection::error("selected_runtime_not_active"));
            }
        }
        let runner =
            crate::workspace::query(&runtime, crate::workspace::WorkspaceRequest::Overview {})
                .await?;
        let runner_client_id = stored_runner_client_id(&self.config)
            .ok_or_else(|| runtime_selection::error("runtime_identity_unavailable"))?;
        let expected = self
            .adapter
            .binaries()?
            .builds
            .iter()
            .find(|b| b.binary == "webcodex-runner")
            .ok_or_else(|| runtime_selection::error("build_info_unverifiable"))?;
        if runner.get("client_id").and_then(Value::as_str) != Some(runner_client_id.as_str())
            || runner.get("version").and_then(Value::as_str) != Some(expected.version.as_str())
            || runner.get("build_git_commit").and_then(Value::as_str)
                != expected.git_commit.as_deref()
            || runner.get("build_git_dirty").and_then(Value::as_bool) != expected.git_dirty
        {
            return Err(runtime_selection::error("selected_runtime_not_active"));
        }
        self.refresh_runtime_status(cancellation).await?;
        Ok(())
    }
}

/// Process authority and byte/identity admission have already been checked by
/// switch_runtime_transaction. The driver cannot choose another Project, token,
/// connection or fallback source while the state-machine runs.
struct RuntimeSwitchExecution<'a> {
    core: &'a mut DesktopCore,
    candidate_source: RuntimeSource,
    candidate_binaries: ResolvedBinaries,
    previous_config: StoredDesktopConfig,
    previous_binaries: Option<ResolvedBinaries>,
    identity: Option<RunnerRuntimeIdentity>,
    local: bool,
    cancellation: &'a CancellationContext,
}
impl runtime_selection::lifecycle::SwitchDriver for RuntimeSwitchExecution<'_> {
    async fn stop(&mut self) -> DesktopResult<()> {
        self.core.stop_owned_runtime_for_switch(self.local).await
    }
    async fn activate_candidate(&mut self) -> DesktopResult<()> {
        self.core.adapter.activate_binaries(
            self.candidate_source.clone(),
            self.candidate_binaries.clone(),
        );
        if let Some(identity) = &self.identity {
            self.core
                .start_existing_runtime(identity, self.local, self.cancellation)
                .await?;
        }
        Ok(())
    }
    async fn commit_candidate(&mut self) -> DesktopResult<()> {
        self.core.config.previous_runtime_source =
            Some(self.previous_config.runtime_binary_source.clone());
        self.core.config.runtime_binary_source = self.candidate_source.clone();
        self.core.config.runtime_binary_fingerprint = match self.candidate_source {
            RuntimeSource::Custom { .. } => Some(self.candidate_binaries.fingerprint.clone()),
            RuntimeSource::Bundled => None,
        };
        self.core.config.runtime_selection_revision = self
            .previous_config
            .runtime_selection_revision
            .checked_add(1)
            .ok_or_else(|| runtime_selection::error("runtime_revision_exhausted"))?;
        self.core.persist_runtime_preferences().await
    }
    async fn restore_previous(&mut self) -> DesktopResult<()> {
        self.core.config = self.previous_config.clone();
        self.core.persist_runtime_preferences().await?;
        if self.cancellation.is_cancelled() {
            return Err(runtime_selection::error("runtime_switch_interrupted"));
        }
        let previous = self
            .previous_binaries
            .as_ref()
            .ok_or_else(|| runtime_selection::error("rollback_runtime_unverifiable"))?;
        runtime_selection::verify_resolved_files(previous).await?;
        self.core.adapter.activate_binaries(
            self.previous_config.runtime_binary_source.clone(),
            previous.clone(),
        );
        if let Some(identity) = &self.identity {
            self.core
                .start_existing_runtime(identity, self.local, self.cancellation)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_connection_change_invalidates_a_pending_candidate_without_a_binary_revision_change() {
        let initial = StoredDesktopConfig::default();
        let mut changed = initial.clone();
        changed.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        assert_ne!(selection_context(&initial), selection_context(&changed));
    }
    #[test]
    fn an_update_check_does_not_invalidate_runtime_selection_authority() {
        let initial = StoredDesktopConfig::default();
        let mut changed = initial.clone();
        changed.update_cache.last_check_at_ms = Some(42);
        assert_eq!(selection_context(&initial), selection_context(&changed));
    }
}
