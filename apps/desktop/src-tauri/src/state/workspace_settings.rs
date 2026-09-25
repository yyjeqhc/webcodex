use super::*;

impl AppState {
    pub async fn runner_settings(
        &self,
    ) -> DesktopResult<crate::webcodex::settings::RunnerSettings> {
        let mut slot = self.core.lock().await;
        let core = slot
            .as_mut()
            .ok_or_else(|| desktop_state_unavailable("Desktop is busy"))?;
        let runtime = core
            .config
            .runtime
            .clone()
            .ok_or_else(|| desktop_state_unavailable("Configure a Runner first"))?;
        let can_restart = core
            .process_snapshot(ProcessKey::LocalRunner)
            .await
            .is_some_and(|p| p.owned_by_desktop && p.phase == ProcessPhase::Running);
        tokio::task::spawn_blocking(move || {
            crate::webcodex::settings::inspect(&runtime, can_restart)
        })
        .await
        .map_err(|_| desktop_state_unavailable("Settings worker stopped"))?
    }

    pub async fn update_runner_settings(
        &self,
        request: crate::webcodex::settings::SettingsUpdate,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(|| desktop_state_unavailable("Configure a Runner first"))?;
            tokio::task::spawn_blocking(move || {
                crate::webcodex::settings::update(&runtime, request)
            })
            .await
            .map_err(|_| desktop_state_unavailable("Settings worker stopped"))??;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn restart_owned_runner(
        &self,
        expected: crate::webcodex::settings::SettingsTarget,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerRestart, false)
            .await?;
        let result = async {
            if !core
                .process_snapshot(ProcessKey::LocalRunner)
                .await
                .is_some_and(|p| p.owned_by_desktop && p.phase == ProcessPhase::Running)
            {
                return Err(DesktopError::new(
                    "runner_not_owned",
                    "Desktop does not own a running Runner",
                    "Restart the Runner using its actual process owner.",
                ));
            }
            let identity = runner_identity_from_config(&core.config)
                .ok_or_else(|| desktop_state_unavailable("Runner identity unavailable"))?;
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(|| desktop_state_unavailable("Runner identity unavailable"))?;
            crate::webcodex::settings::verify_target(&runtime, &expected)?;
            tokio::task::spawn_blocking(move || crate::webcodex::settings::inspect(&runtime, true))
                .await
                .map_err(|_| desktop_state_unavailable("Settings worker stopped"))??;
            core.adapter.ensure_binaries(&cancellation).await?;
            let command = core.prepare_runner_command(&identity).await?;
            core.supervisor
                .lock()
                .await
                .stop_checked(ProcessKey::LocalRunner)
                .await?;
            core.snapshot.readiness.runner = RunnerReadiness::Connecting;
            core.snapshot.readiness.runtime_ready = false;
            core.publish_snapshot();
            core.spawn_owned(ProcessKey::LocalRunner, command, false, &cancellation)
                .await?;
            core.mcp_applied_revision = Some(core.mcp_providers.revision());
            core.coding_agents_applied_revision = Some(core.coding_agents.revision());
            core.wait_for_runner(
                &identity,
                &cancellation,
                Deadline::after(RUNNER_READY_TIMEOUT),
                true,
            )
            .await?;
            core.refresh_runtime_status(&cancellation).await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn add_runner_plugin(
        &self,
        request: crate::webcodex::settings::PluginAddRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::RunnerSettingsUpdate, false)
            .await?;
        let result = async {
            let runtime = core
                .config
                .runtime
                .clone()
                .ok_or_else(|| desktop_state_unavailable("Configure a Runner first"))?;
            tokio::task::spawn_blocking(move || {
                crate::webcodex::settings::add_plugin(&runtime, request)
            })
            .await
            .map_err(|_| desktop_state_unavailable("Settings worker stopped"))??;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}
