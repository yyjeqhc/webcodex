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
        let can_restart = if core.config.persistent_environment.is_some() {
            runtime.runner_config.is_some() && runtime.runner_client_id.is_some()
        } else {
            core.process_snapshot(ProcessKey::LocalRunner)
                .await
                .is_some_and(|p| p.owned_by_desktop && p.phase == ProcessPhase::Running)
        };
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
            let edit = tokio::task::spawn_blocking({
                let runtime = runtime.clone();
                move || crate::webcodex::settings::stage_paths_update(&runtime, request)
            })
            .await
            .map_err(|_| desktop_state_unavailable("Settings worker stopped"))??;
            apply_staged_settings(&runtime, &edit).await?;
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn update_runner_allowed_roots(
        &self,
        request: crate::webcodex::settings::AllowedRootsUpdate,
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
            let edit = tokio::task::spawn_blocking({
                let runtime = runtime.clone();
                move || crate::webcodex::settings::stage_allowed_roots_update(&runtime, request)
            })
            .await
            .map_err(|_| desktop_state_unavailable("Settings worker stopped"))??;

            apply_staged_settings(&runtime, &edit).await?;
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
            if core.config.persistent_environment.is_some() {
                let runtime = core
                    .config
                    .runtime
                    .as_ref()
                    .ok_or_else(|| desktop_state_unavailable("Runner identity unavailable"))?;
                crate::webcodex::settings::verify_target(runtime, &expected)?;
                return core.restart_environment_runner(&cancellation).await;
            }
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

/// Shared settings transaction: an unavailable check proves reload was not
/// submitted; an indeterminate reload does not authorize rollback or replay.
/// This same path serves allowed roots, instruction files and Skill roots.
pub(super) async fn apply_staged_settings(
    runtime: &StoredRuntime,
    edit: &crate::webcodex::settings::PendingSettingsEdit,
) -> DesktopResult<()> {
    let client_id = edit_target_client_id(runtime)?;
    apply_with_control(&client_id, edit, |tool, params| {
        runner_config_call(runtime, tool, params)
    })
    .await
}

async fn apply_with_control<F, Fut>(
    client_id: &str,
    edit: &crate::webcodex::settings::PendingSettingsEdit,
    mut call: F,
) -> DesktopResult<()>
where
    F: FnMut(&'static str, Value) -> Fut,
    Fut: std::future::Future<Output = DesktopResult<Value>>,
{
    let checked = call(
        "check_runner_config",
        serde_json::json!({"client_id": client_id}),
    )
    .await;
    let generation = match checked.and_then(|value| checked_config_generation(&value)) {
        Ok(generation) => generation,
        Err(error) => {
            rollback_staged_settings(edit)?;
            return Err(error);
        }
    };
    if !edit.candidate_unchanged()? {
        return Err(settings_error(
            "runner_config_concurrent_change",
            "Runner configuration changed during validation",
            "Reload settings before making another change.",
        ));
    }
    let reload = call(
        "reload_runner_config",
        serde_json::json!({
            "client_id": client_id, "expected_generation": generation
        }),
    )
    .await;
    let applied = match reload {
        Ok(value) if value.get("success").and_then(Value::as_bool) == Some(true) => reload_generation_applied(&value, generation),
        Ok(value) if reload_outcome_unknown(&value) => {
            let observed = call("check_runner_config", serde_json::json!({"client_id":client_id})).await;
            Err(uncertain_reload(observed))
        }
        Err(_) => {
            let observed = call("check_runner_config", serde_json::json!({"client_id":client_id})).await;
            Err(uncertain_reload(observed))
        }
        Ok(_) => Err(settings_error("runner_config_reload_failed", "Runner rejected the settings reload", "The unchanged on-disk candidate will be restored. Reconnect and reload settings before trying again.")),
    };
    match applied {
        Ok(()) if edit.candidate_unchanged()? => Ok(()),
        Ok(()) => Err(settings_error(
            "runner_config_reconcile_required",
            "Configuration changed after reload",
            "Recheck the active Runner and current settings; no automatic rollback was attempted.",
        )),
        Err(error) if error.code == "runner_config_reload_failed" => {
            rollback_staged_settings(edit)?;
            Err(error)
        }
        Err(error) => Err(error),
    }
}

fn settings_error(
    code: &'static str,
    message: &'static str,
    next_action: &'static str,
) -> DesktopError {
    DesktopError::new(code, message, next_action)
}

fn edit_target_client_id(runtime: &StoredRuntime) -> DesktopResult<String> {
    runtime
        .runner_client_id
        .clone()
        .filter(|client_id| !client_id.trim().is_empty())
        .ok_or_else(|| desktop_state_unavailable("Runner identity unavailable"))
}

async fn runner_config_call(
    runtime: &StoredRuntime,
    tool: &'static str,
    params: Value,
) -> DesktopResult<Value> {
    crate::workspace::post(
        runtime,
        "/api/tools/call",
        serde_json::json!({"tool": tool, "params": params}),
        Duration::from_secs(12),
    )
    .await
    .map_err(|_| {
        settings_error(
            "runner_config_control_unavailable",
            "Runner configuration could not be verified online",
            "The on-disk change was not reported as applied. Reconnect the Runner and recheck settings.",
        )
    })
}

fn checked_config_generation(value: &Value) -> DesktopResult<u64> {
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(settings_error(
            "runner_config_check_failed",
            "Runner rejected the settings candidate",
            "The previous on-disk settings configuration was restored. Correct the configuration and try again.",
        ));
    }
    let output = value.get("output").ok_or_else(|| {
        settings_error(
            "runner_config_check_failed",
            "Runner returned an incomplete configuration check",
            "The previous on-disk settings configuration was restored. Recheck the Runner and try again.",
        )
    })?;
    if output.get("valid").and_then(Value::as_bool) != Some(true)
        || output
            .get("restart_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(settings_error(
            "runner_config_check_failed",
            "Runner could not safely hot-reload the settings candidate",
            "The previous on-disk settings configuration was restored. Resolve other pending Runner config changes and try again.",
        ));
    }
    output
        .get("current_generation")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            settings_error(
                "runner_config_check_failed",
                "Runner omitted the active configuration generation",
                "The previous on-disk settings configuration was restored. Recheck the Runner and try again.",
            )
        })
}

fn reload_generation_applied(value: &Value, before: u64) -> DesktopResult<()> {
    let output = value.get("output").ok_or_else(|| {
        settings_error(
            "runner_config_reconcile_required",
            "Runner reload completed without enough state to confirm settings",
            "Recheck settings before relying on the new settings.",
        )
    })?;
    if output
        .get("restart_required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(settings_error(
            "runner_config_reconcile_required",
            "Runner reported startup-only changes after accepting the reload",
            "Hot-reloadable values may already be active. Inspect the current configuration; Desktop did not roll it back or restart the Runner.",
        ));
    }
    let expected = before.checked_add(1).ok_or_else(|| {
        settings_error(
            "runner_config_reconcile_required",
            "Runner configuration generation could not be reconciled",
            "Recheck settings before relying on the new settings.",
        )
    })?;
    match output.get("current_generation").and_then(Value::as_u64) {
        Some(current) if current == expected => Ok(()),
        _ => Err(settings_error(
            "runner_config_reconcile_required",
            "Runner reload generation did not match the checked candidate",
            "Recheck settings before relying on the new settings.",
        )),
    }
}

fn reload_outcome_unknown(value: &Value) -> bool {
    value
        .get("output")
        .and_then(|output| output.get("execution_state"))
        .and_then(Value::as_str)
        == Some("outcome_unknown")
        || value
            .get("output")
            .and_then(|output| output.get("error_code"))
            .and_then(Value::as_str)
            .is_some_and(|code| {
                matches!(
                    code,
                    "operation_indeterminate" | "config_generation_conflict"
                )
            })
}

fn uncertain_reload(observed: DesktopResult<Value>) -> DesktopError {
    // A check observes the current generation, not which request changed it.
    // Even an unchanged generation cannot prove a timed-out reload will not
    // execute later. Never replay or restore the candidate on this evidence.
    settings_error("runner_config_reconcile_required", "Runner settings reload outcome is uncertain", "The candidate remains on disk. Reconnect and inspect settings before retrying; no restart, repeated reload or automatic rollback was attempted.")
        .with_details(serde_json::json!({"observed_generation": observed.ok().and_then(|value| value.get("output")?.get("current_generation")?.as_u64())}))
}

fn rollback_staged_settings(
    edit: &crate::webcodex::settings::PendingSettingsEdit,
) -> DesktopResult<()> {
    match edit.rollback_if_unchanged()? {
        true => Ok(()),
        false => Err(settings_error(
            "runner_config_reconcile_required",
            "Runner configuration changed before Desktop could restore the previous settings list",
            "Reload settings and inspect the current Runner configuration before making another change.",
        )),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod file_access_contract_tests {
    use super::*;

    #[test]
    fn check_success_and_reload_success_require_the_same_generation_transition() {
        let check = serde_json::json!({"success": true, "output": {"valid": true, "restart_required": false, "current_generation": 7}});
        let generation = checked_config_generation(&check).unwrap();
        assert_eq!(generation, 7);
        let reload = serde_json::json!({"success": true, "output": {"restart_required": false, "current_generation": 8}});
        reload_generation_applied(&reload, generation).unwrap();
    }

    #[test]
    fn check_failure_and_restart_only_candidate_fail_closed() {
        for check in [
            serde_json::json!({"success": false, "output": {"valid": false, "current_generation": 7}}),
            serde_json::json!({"success": true, "output": {"valid": true, "restart_required": true, "current_generation": 7}}),
        ] {
            assert!(checked_config_generation(&check).is_err());
        }
    }

    #[test]
    fn reload_failure_or_generation_mismatch_is_never_reported_as_applied() {
        let rejected = serde_json::json!({"success": false, "output": {"current_generation": 7}});
        assert_ne!(rejected.get("success").and_then(Value::as_bool), Some(true));
        let wrong_generation = serde_json::json!({"success": true, "output": {"restart_required": false, "current_generation": 9}});
        assert!(reload_generation_applied(&wrong_generation, 7).is_err());
    }
}
