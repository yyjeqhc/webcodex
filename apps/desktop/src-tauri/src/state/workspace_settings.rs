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

            let checked = match runner_config_call(&runtime, "runner_config_check", serde_json::json!({
                "client_id": edit_target_client_id(&runtime)?
            }))
            .await
            {
                Ok(value) => value,
                Err(error) => {
                    rollback_staged_file_access(&edit)?;
                    return Err(error);
                }
            };
            let generation = match checked_config_generation(&checked) {
                Ok(generation) => generation,
                Err(error) => {
                    rollback_staged_file_access(&edit)?;
                    return Err(error);
                }
            };
            if !edit.candidate_unchanged()? {
                return Err(file_access_error(
                    "runner_config_concurrent_change",
                    "Runner configuration changed while file access was being validated",
                    "Reload File access and retry from the current Runner configuration.",
                ));
            }

            let reload = runner_config_call(
                &runtime,
                "runner_config_reload",
                serde_json::json!({
                    "client_id": edit_target_client_id(&runtime)?,
                    "expected_generation": generation,
                }),
            )
            .await;
            let applied = match reload {
                Ok(value) if value.get("success").and_then(Value::as_bool) == Some(true) => {
                    reload_generation_applied(&value, generation)
                }
                Ok(value) if reload_outcome_unknown(&value) => {
                    reconcile_unknown_reload(&runtime, &edit, generation).await
                }
                Ok(_) => Err(file_access_error(
                    "runner_config_reload_failed",
                    "Runner rejected the file access reload",
                    "The previous on-disk file access configuration was restored. Recheck the Runner and try again.",
                )),
                Err(_) => reconcile_unknown_reload(&runtime, &edit, generation).await,
            };
            match applied {
                Ok(()) => core.get_state().await,
                Err(error) => {
                    rollback_staged_file_access(&edit)?;
                    Err(error)
                }
            }
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

fn file_access_error(
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
        file_access_error(
            "runner_config_control_unavailable",
            "Runner configuration could not be verified online",
            "The on-disk change was not reported as applied. Reconnect the Runner and recheck File access.",
        )
    })
}

fn checked_config_generation(value: &Value) -> DesktopResult<u64> {
    if value.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(file_access_error(
            "runner_config_check_failed",
            "Runner rejected the file access candidate",
            "The previous on-disk file access configuration was restored. Correct the configuration and try again.",
        ));
    }
    let output = value.get("output").ok_or_else(|| {
        file_access_error(
            "runner_config_check_failed",
            "Runner returned an incomplete configuration check",
            "The previous on-disk file access configuration was restored. Recheck the Runner and try again.",
        )
    })?;
    if output.get("valid").and_then(Value::as_bool) != Some(true)
        || output
            .get("restart_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        return Err(file_access_error(
            "runner_config_check_failed",
            "Runner could not safely hot-reload the file access candidate",
            "The previous on-disk file access configuration was restored. Resolve other pending Runner config changes and try again.",
        ));
    }
    output
        .get("current_generation")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            file_access_error(
                "runner_config_check_failed",
                "Runner omitted the active configuration generation",
                "The previous on-disk file access configuration was restored. Recheck the Runner and try again.",
            )
        })
}

fn reload_generation_applied(value: &Value, before: u64) -> DesktopResult<()> {
    let output = value.get("output").ok_or_else(|| {
        file_access_error(
            "runner_config_reconcile_required",
            "Runner reload completed without enough state to confirm file access",
            "Recheck File access before relying on the new folders.",
        )
    })?;
    if output
        .get("restart_required")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(file_access_error(
            "runner_config_reload_failed",
            "Runner reported a restart-only configuration change",
            "The previous on-disk file access configuration was restored. Resolve other Runner config changes first.",
        ));
    }
    let expected = before.checked_add(1).ok_or_else(|| {
        file_access_error(
            "runner_config_reconcile_required",
            "Runner configuration generation could not be reconciled",
            "Recheck File access before relying on the new folders.",
        )
    })?;
    match output.get("current_generation").and_then(Value::as_u64) {
        Some(current) if current == expected => Ok(()),
        _ => Err(file_access_error(
            "runner_config_reconcile_required",
            "Runner reload generation did not match the checked candidate",
            "Recheck File access before relying on the new folders.",
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

async fn reconcile_unknown_reload(
    runtime: &StoredRuntime,
    edit: &crate::webcodex::settings::PendingAllowedRootsEdit,
    before: u64,
) -> DesktopResult<()> {
    if !edit.candidate_unchanged()? {
        return Err(file_access_error(
            "runner_config_reconcile_required",
            "Runner configuration changed while reload status was uncertain",
            "Reload File access and inspect the current Runner configuration before making another change.",
        ));
    }
    let checked = runner_config_call(
        runtime,
        "runner_config_check",
        serde_json::json!({"client_id": edit_target_client_id(runtime)?}),
    )
    .await?;
    let current = checked_config_generation(&checked)?;
    let expected = before.checked_add(1).ok_or_else(|| {
        file_access_error(
            "runner_config_reconcile_required",
            "Runner configuration generation could not be reconciled",
            "Recheck File access before relying on the new folders.",
        )
    })?;
    if current == expected {
        return Ok(());
    }
    if current == before {
        return Err(file_access_error(
            "runner_config_reload_failed",
            "Runner did not activate the file access candidate",
            "The previous on-disk file access configuration was restored. Recheck the Runner and try again.",
        ));
    }
    Err(file_access_error(
        "runner_config_reconcile_required",
        "Runner configuration advanced unexpectedly while reload status was uncertain",
        "Recheck File access before relying on either the old or new folder list.",
    ))
}

fn rollback_staged_file_access(
    edit: &crate::webcodex::settings::PendingAllowedRootsEdit,
) -> DesktopResult<()> {
    match edit.rollback_if_unchanged()? {
        true => Ok(()),
        false => Err(file_access_error(
            "runner_config_reconcile_required",
            "Runner configuration changed before Desktop could restore the previous file access list",
            "Reload File access and inspect the current Runner configuration before making another change.",
        )),
    }
}

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
