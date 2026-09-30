use super::*;
use crate::diagnostics::{
    self, ConfigurationRecovery, DiagnosticSnapshot, ResourceKind, TraceMode, TraceSettings,
    TraceUpdate,
};
use crate::runtime_selection;
use sha2::{Digest, Sha256};
mod tracing;
use tracing::effective_trace_mode;

fn runtime_fence(runtime: &StoredRuntime) -> String {
    // Correlation only, not an authority token or a copy of credential bytes.
    let value = serde_json::to_vec(runtime).unwrap_or_default();
    format!("{:x}", Sha256::digest(value))
}

fn valid_console_user_credential(value: &str) -> bool {
    value.starts_with("wc_pat_")
        && value.len() <= 16384
        && value.len() > "wc_pat_".len()
        && !value.chars().any(char::is_whitespace)
}

impl AppState {
    pub async fn diagnostics(&self) -> DesktopResult<DiagnosticSnapshot> {
        let (settings, runtime, trace_target, recovery, mut resources, identity_fence) = {
            let slot = self.core.lock().await;
            let core = slot
                .as_ref()
                .ok_or_else(|| diagnostics::diagnostic_error("desktop_operation_busy"))?;
            let settings = core.runtime_settings_snapshot().await;
            let runtime = core.config.runtime.clone();
            let trace_target = core.managed_trace_target().await;
            let backup = matches!(
                read_stored_config(&desktop_state_backup_path(&core.config_path)),
                Ok(StoredConfigFile::Valid { .. })
            );
            let recovery = ConfigurationRecovery {
                reason_code: core.configuration_issue.clone(),
                backup_available: backup,
                primary_fingerprint: runtime_shell::configuration_file_fingerprint(
                    &core.config_path,
                )
                .ok(),
            };
            let mut resources = vec![
                ResourceKind::AppData,
                ResourceKind::Documentation,
                ResourceKind::Github,
                ResourceKind::ReportIssue,
            ];
            if trace_target.is_ok() {
                resources.push(ResourceKind::ServerConfiguration);
            }
            if core.adapter.binaries().is_ok() {
                resources.push(ResourceKind::RuntimeDirectory);
            }
            let fence = runtime.as_ref().map(runtime_fence);
            (settings, runtime, trace_target, recovery, resources, fence)
        };
        let mut trace = trace_target
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|target| target.inspect())
            .unwrap_or_else(|error| TraceSettings {
                mode: TraceMode::Off,
                effective_mode: None,
                revision: String::new(),
                available: false,
                restart_required: false,
                can_restart: false,
                error_code: Some(error.code),
            });
        let trace_path = trace_target.ok().map(|target| target.path);
        let (runner, windows) = if let Some(runtime) = &runtime {
            let (runner, windows) = tokio::join!(
                crate::workspace::query(runtime, crate::workspace::WorkspaceRequest::Overview {}),
                crate::workspace::query(runtime, crate::workspace::WorkspaceRequest::Windows {})
            );
            (runner.ok(), windows.ok())
        } else {
            (None, None)
        };
        trace.effective_mode = runner.as_ref().and_then(effective_trace_mode);
        trace.restart_required =
            trace.available && trace.effective_mode.is_some_and(|mode| mode != trace.mode);
        if trace_path
            .as_deref()
            .and_then(diagnostics::trace_directory)
            .is_some()
        {
            resources.push(ResourceKind::TraceDirectory);
        }
        let mut last_call = None;
        if let (Some(runtime), Some(windows)) = (&runtime, windows.as_ref()) {
            // The UI labels this as the latest authorized observed Window, not
            // "the current ChatGPT turn" or a complete global activity history.
            if let Some(key) = windows
                .get("windows")
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter().max_by_key(|w| {
                        w.get("last_meaningful_activity_at_ms")
                            .and_then(Value::as_i64)
                            .unwrap_or(0)
                    })
                })
                .and_then(|w| w.get("client_window_key"))
                .and_then(Value::as_str)
            {
                if let Ok(detail) = crate::workspace::query(
                    runtime,
                    crate::workspace::WorkspaceRequest::Window {
                        client_window_key: key.to_string(),
                    },
                )
                .await
                {
                    last_call = diagnostics::continuation(&detail, runtime_selection::now_ms());
                }
            }
        }
        let copy_available = runtime.as_ref().is_some_and(|runtime| {
            runtime
                .user_token_file
                .as_ref()
                .is_some_and(|p| p.is_file())
                && diagnostics::console_url(&runtime.server_url).is_ok()
        });
        if runtime
            .as_ref()
            .is_some_and(|runtime| diagnostics::console_url(&runtime.server_url).is_ok())
        {
            resources.push(ResourceKind::RuntimeConsole);
        }
        {
            let slot = self.core.lock().await;
            let core = slot
                .as_ref()
                .ok_or_else(|| diagnostics::diagnostic_error("desktop_operation_busy"))?;
            if core.config.runtime.as_ref().map(runtime_fence) != identity_fence
                || core.config.runtime_selection_revision != settings.selection_revision
            {
                return Err(diagnostics::diagnostic_error("diagnostic_identity_changed"));
            }
        }
        let snapshot = self.get_state();
        let permissions = crate::platform::permissions::probe();
        let permissions = serde_json::json!({"supported":permissions.supported,"desktop_accessibility":permissions.desktop_accessibility,"desktop_screen_recording":permissions.desktop_screen_recording});
        let mut report = diagnostics::report(
            &snapshot,
            &settings,
            runner.as_ref(),
            last_call,
            &trace,
            &self.activity.snapshot(),
            permissions,
        );
        if let Some(report) = report.as_object_mut() {
            report.insert(
                "desktop_data_dir".into(),
                serde_json::json!({
                    "source": self.desktop_data_dir.source.label(),
                    "physical_resolution": if self.desktop_data_dir.physical_resolution_changed {
                        "changed"
                    } else {
                        "unchanged"
                    }
                }),
            );
        }
        let markdown = diagnostics::report_markdown(&report);
        Ok(DiagnosticSnapshot {
            schema_version: 1,
            observed_at_ms: runtime_selection::now_ms(),
            trace,
            configuration: recovery,
            resources,
            can_copy_console_credential: copy_available,
            credential_copy_fence: identity_fence,
            report,
            markdown,
        })
    }

    pub async fn set_tool_request_tracing(
        &self,
        request: TraceUpdate,
    ) -> DesktopResult<TraceSettings> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::TraceUpdate, false)
            .await?;
        let mut saved = None;
        let result = async {
            cancellation.check()?;
            let target = core.managed_trace_target().await?;
            if request.restart {
                if !target.can_restart {
                    return Err(diagnostics::diagnostic_error("server_not_owned"));
                }
                if let Some(runtime) = &core.config.runtime {
                    let observed = crate::workspace::query(
                        runtime,
                        crate::workspace::WorkspaceRequest::Overview {},
                    )
                    .await
                    .ok();
                    let active = observed
                        .as_ref()
                        .and_then(runtime_shell::observed_active_jobs);
                    if active != Some(0) && !request.confirm_interrupt {
                        return Err(diagnostics::diagnostic_error(
                            "runtime_switch_jobs_confirmation_required",
                        )
                        .with_details(serde_json::json!({"active_jobs":active})));
                    }
                }
            }
            let restart = request.restart;
            let requested_mode = request.mode;
            cancellation.check()?;
            let mut trace = tokio::task::spawn_blocking(move || {
                let target = target;
                target.update(
                    request.mode,
                    &request.expected_revision,
                    request.confirm_full,
                )
            })
            .await
            .map_err(|_| diagnostics::diagnostic_error("server_environment_write_unconfirmed"))??;
            if restart {
                if let Err(error) = core.restart_owned_server_for_trace(&cancellation).await {
                    // The file save is confirmed but restart/reconnect is not.
                    // Preserve existing healthy components; never feed this
                    // partial outcome to generic operation cleanup as a fresh
                    // failed admission or retry the side effect automatically.
                    trace.restart_required = true;
                    trace.error_code = Some(error.code.clone());
                    trace.can_restart = core
                        .managed_trace_target()
                        .await
                        .is_ok_and(|target| target.can_restart);
                    saved = Some(trace);
                    core.snapshot.readiness.runtime_ready = false;
                    core.snapshot.runtime_error = Some(error);
                    core.terminalize_failed_start(false);
                    return core.get_state().await;
                }
            }
            let runtime = core
                .config
                .runtime
                .as_ref()
                .ok_or_else(|| diagnostics::diagnostic_error("runtime_identity_unavailable"))?;
            let observed =
                crate::workspace::query(runtime, crate::workspace::WorkspaceRequest::Overview {})
                    .await
                    .ok();
            trace.effective_mode = observed.as_ref().and_then(effective_trace_mode);
            trace.restart_required = trace.effective_mode != Some(requested_mode);
            if restart && trace.restart_required {
                trace.error_code = Some("trace_effective_mode_unconfirmed_or_overridden".into());
            }
            trace.can_restart = core
                .managed_trace_target()
                .await
                .is_ok_and(|target| target.can_restart);
            saved = Some(trace);
            core.get_state().await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await?;
        saved.ok_or_else(|| diagnostics::diagnostic_error("trace_update_unconfirmed"))
    }

    pub async fn open_diagnostic_resource(&self, kind: ResourceKind) -> DesktopResult<()> {
        let slot = self.core.lock().await;
        let core = slot
            .as_ref()
            .ok_or_else(|| diagnostics::diagnostic_error("desktop_operation_busy"))?;
        let location = match kind {
            ResourceKind::RuntimeConsole => {
                let runtime =
                    core.config.runtime.as_ref().ok_or_else(|| {
                        diagnostics::diagnostic_error("runtime_console_unavailable")
                    })?;
                let url = diagnostics::console_url(&runtime.server_url)?;
                drop(slot);
                return crate::platform::opener::url(&url);
            }
            ResourceKind::Documentation => {
                drop(slot);
                return crate::platform::opener::url("https://github.com/yyjeqhc/webcodex/blob/main/docs/DESKTOP_RUNTIME_COMPATIBILITY.md");
            }
            ResourceKind::Github => {
                drop(slot);
                return crate::platform::opener::url("https://github.com/yyjeqhc/webcodex");
            }
            ResourceKind::ReportIssue => {
                drop(slot);
                return crate::platform::opener::url(
                    "https://github.com/yyjeqhc/webcodex/issues/new",
                );
            }
            ResourceKind::Contributing => {
                drop(slot);
                return crate::platform::opener::url(
                    "https://github.com/yyjeqhc/webcodex/blob/main/CONTRIBUTING.md",
                );
            }
            ResourceKind::DesktopDevelopment => {
                drop(slot);
                return crate::platform::opener::url(
                    "https://github.com/yyjeqhc/webcodex/blob/main/docs/DESKTOP_DEVELOPMENT.md",
                );
            }
            ResourceKind::AppData => core.data_dir.clone(),
            ResourceKind::ServerConfiguration => core
                .managed_trace_target()
                .await?
                .path
                .parent()
                .ok_or_else(|| diagnostics::diagnostic_error("server_environment_unavailable"))?
                .to_path_buf(),
            ResourceKind::TraceDirectory => {
                diagnostics::trace_directory(&core.managed_trace_target().await?.path)
                    .ok_or_else(|| diagnostics::diagnostic_error("trace_directory_unavailable"))?
            }
            ResourceKind::RuntimeDirectory => core.adapter.binaries()?.directory.clone(),
        };
        drop(slot);
        crate::platform::opener::directory(&location)
    }

    pub async fn copy_console_credential(
        &self,
        app: &tauri::AppHandle,
        expected_fence: &str,
    ) -> DesktopResult<()> {
        use std::io::Read;
        use tauri_plugin_clipboard_manager::ClipboardExt;
        // Hold identity admission across this bounded local read/copy. The token
        // never appears in an IPC response, activity message or diagnostics value.
        let slot = self.core.lock().await;
        let core = slot
            .as_ref()
            .ok_or_else(|| diagnostics::diagnostic_error("desktop_operation_busy"))?;
        if self.operations.current().is_some() {
            return Err(diagnostics::diagnostic_error("desktop_operation_busy"));
        }
        let runtime = core
            .config
            .runtime
            .as_ref()
            .ok_or_else(|| diagnostics::diagnostic_error("runtime_console_unavailable"))?;
        if runtime_fence(runtime) != expected_fence {
            return Err(diagnostics::diagnostic_error("diagnostic_identity_changed"));
        }
        diagnostics::console_url(&runtime.server_url)?;
        let file = runtime
            .user_token_file
            .as_ref()
            .ok_or_else(|| diagnostics::diagnostic_error("runtime_console_unavailable"))?;
        let mut token = String::new();
        std::fs::File::open(file)
            .map_err(|_| diagnostics::diagnostic_error("console_copy_unavailable"))?
            .take(16385)
            .read_to_string(&mut token)
            .map_err(|_| diagnostics::diagnostic_error("console_copy_unavailable"))?;
        let value = token.trim();
        if !valid_console_user_credential(value) {
            return Err(diagnostics::diagnostic_error("console_copy_unavailable"));
        }
        app.clipboard()
            .write_text(value)
            .map_err(|_| diagnostics::diagnostic_error("clipboard_unavailable"))
    }
}

#[cfg(test)]
mod credential_tests {
    use super::valid_console_user_credential;

    #[test]
    fn runtime_console_copy_accepts_only_managed_user_pat_shape() {
        assert!(valid_console_user_credential("wc_pat_user_token"));
        for rejected in [
            "",
            "wc_pat_",
            "wc_agent_runner_token",
            "wc_pair_pairing_code",
            "webcodex_temporary_secret",
            "arbitrary-bearer-token",
            "wc_pat_has whitespace",
        ] {
            assert!(!valid_console_user_credential(rejected), "{rejected}");
        }
    }
}

impl DesktopCore {
    pub(super) fn managed_server_environment(&self) -> DesktopResult<PathBuf> {
        if !matches!(
            self.config.topology.as_ref().map(|t| &t.server),
            Some(ServerTopology::Local)
        ) {
            return Err(diagnostics::diagnostic_error("server_not_owned"));
        }
        let runtime = self
            .config
            .runtime
            .as_ref()
            .ok_or_else(|| diagnostics::diagnostic_error("server_environment_unavailable"))?;
        let path = runtime
            .server_env_file
            .as_ref()
            .ok_or_else(|| diagnostics::diagnostic_error("server_environment_unavailable"))?;
        let expected = self.data_dir.join("runtime/local/webcodex.env");
        if path != &expected || !path.is_file() {
            return Err(diagnostics::diagnostic_error(
                "server_environment_not_managed",
            ));
        }
        Ok(path.clone())
    }

    async fn restart_owned_server_for_trace(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<()> {
        self.preflight_providers()?;
        if let Some(environment_id) = self.config.persistent_environment.clone() {
            use webcodex_environment::{service::Component, NativeEnvironment, ServiceOperation};
            let runtime = self
                .config
                .runtime
                .clone()
                .ok_or_else(|| diagnostics::diagnostic_error("runtime_identity_unavailable"))?;
            cancellation.check()?;
            NativeEnvironment::new()
                .map_err(environment::desktop_error)?
                .control_service_for_environment(
                    &environment::store()?,
                    Some(&environment_id),
                    Component::Server,
                    ServiceOperation::Restart,
                )
                .await
                .map_err(environment::desktop_error)?;
            self.snapshot.readiness.server = ServerReadiness::Starting;
            self.snapshot.readiness.runtime_ready = false;
            self.publish_snapshot();
            let deadline = Deadline::after(Duration::from_secs(50));
            self.wait_for_server(
                &runtime.server_url,
                runtime.server_env_file.as_deref(),
                runtime.user_token_file.as_deref(),
                cancellation,
                deadline,
                false,
            )
            .await?;
            if let Some(identity) = runner_identity_from_config(&self.config) {
                self.wait_for_runner(&identity, cancellation, deadline, false)
                    .await?;
            }
            self.refresh_environment_status(cancellation).await?;
            self.snapshot.runtime_error = None;
            return Ok(());
        }
        let identity = runner_identity_from_config(&self.config)
            .ok_or_else(|| diagnostics::diagnostic_error("runtime_identity_unavailable"))?;
        self.adapter.ensure_binaries(cancellation).await?;
        runtime_selection::verify_resolved_files(self.adapter.binaries()?).await?;
        let env = self.managed_server_environment()?;
        let command = self.adapter.local_server_command(&env)?;
        self.supervisor
            .lock()
            .await
            .stop_checked(ProcessKey::LocalServer)
            .await?;
        self.snapshot.readiness.server = ServerReadiness::Starting;
        self.snapshot.readiness.runtime_ready = false;
        self.publish_snapshot();
        self.spawn_owned(ProcessKey::LocalServer, command, false, cancellation)
            .await?;
        let deadline = Deadline::after(Duration::from_secs(50));
        self.wait_for_server(
            &identity.server_url,
            Some(&env),
            Some(&identity.user_token_file),
            cancellation,
            deadline,
            true,
        )
        .await?;
        // The healthy owned Runner is not killed; wait for its existing bounded
        // reconnect path. Failed reconnect is reported, not a reason to broad-kill.
        self.wait_for_runner(&identity, cancellation, deadline, false)
            .await?;
        self.refresh_runtime_status(cancellation).await?;
        self.snapshot.runtime_error = None;
        Ok(())
    }
}
