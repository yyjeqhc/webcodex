//! Native, read-only local observations. Remote fleet rows never identify local programs.
use super::*;
use crate::deadline::Deadline;
use crate::operation::CancellationSignal;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use webcodex_core::desktop_runtime_contract::MachineBuildInfo;
use webcodex_environment::unified_update::{
    CandidateIdentity, DownloadPhase, UpdateBlocker, UpdateView,
};
use webcodex_environment::{EnvironmentMode, UpgradeTarget};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfirmation {
    pub candidate: CandidateIdentity,
    pub target: UpgradeTarget,
    pub selection_revision: u64,
    pub services: Vec<webcodex_environment::UpgradeServiceComponent>,
    pub service_inventory_complete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunningComponent {
    pub binary: &'static str,
    pub version: Option<String>,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub state: &'static str,
    pub service_state: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalUpdateStatus {
    pub view: UpdateView,
    pub environment_id: Option<String>,
    pub selection_revision: u64,
    pub installed_observed: bool,
    pub installed_checked_at_ms: Option<u64>,
    pub running: Vec<RunningComponent>,
    pub confirmation: Option<UpdateConfirmation>,
    pub observation_error: bool,
}

fn bounded_local_status(status: LocalUpdateStatus) -> DesktopResult<LocalUpdateStatus> {
    if serde_json::to_vec(&status)
        .map_err(|_| action_error())?
        .len()
        > webcodex_environment::unified_update::MAX_UPDATE_VIEW_BYTES
    {
        return Err(action_error());
    }
    Ok(status)
}

pub(super) fn confirmation_matches(
    confirmation: &UpdateConfirmation,
    environment_id: Option<&str>,
    revision: u64,
    version: &str,
) -> bool {
    environment_id == Some(confirmation.target.environment_id.as_str())
        && revision == confirmation.selection_revision
        && version == confirmation.candidate.version
        && confirmation.target.manifest_sha256 == confirmation.candidate.manifest_sha256
}

fn confirmation_operation(upgrade: Option<&webcodex_environment::UpgradeStatus>) -> Option<String> {
    upgrade
        .filter(|upgrade| {
            !matches!(
                upgrade.phase,
                webcodex_environment::UpgradePhase::Committed
                    | webcodex_environment::UpgradePhase::RolledBack
            )
        })
        .map(|upgrade| upgrade.operation_id.clone())
}

fn unknown(binary: &'static str, state: &'static str) -> RunningComponent {
    RunningComponent {
        binary,
        version: None,
        git_commit: None,
        git_dirty: None,
        state,
        service_state: if binary == "webcodex" || binary == "webcodex-desktop" {
            "not_applicable"
        } else {
            "unknown"
        },
    }
}

fn safe_text(value: Option<&Value>, limit: usize) -> Option<String> {
    value?
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= limit && !s.chars().any(char::is_control))
        .map(str::to_owned)
}

fn observed(binary: &'static str, value: &Value) -> RunningComponent {
    RunningComponent {
        binary,
        version: safe_text(value.get("version"), 96).filter(|value| {
            semver::Version::parse(value).is_ok_and(|version| {
                version.pre.is_empty() && version.build.is_empty() && version.to_string() == *value
            })
        }),
        git_commit: safe_text(value.get("build_git_commit"), 64).filter(|value| {
            matches!(value.len(), 40 | 64)
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }),
        git_dirty: value.get("build_git_dirty").and_then(Value::as_bool),
        state: "observed",
        service_state: "unknown",
    }
}

fn local_record(config: &StoredDesktopConfig) -> Option<webcodex_environment::EnvironmentRecord> {
    let root = webcodex_environment::default_environment_dir().ok()?;
    if !root.is_dir() {
        return None;
    }
    let record = webcodex_environment::EnvironmentStore::open_existing(root)
        .ok()??
        .load_environment()
        .ok()??;
    (Some(record.environment_id.as_str()) == config.persistent_environment.as_deref()
        && record.request.account.identity
            == webcodex_environment::current_account().ok()?.identity)
        .then_some(record)
}

fn verified_file_build(probe: runtime_selection::BinaryProbe) -> Option<MachineBuildInfo> {
    probe
        .error_code
        .as_deref()
        .is_none_or(|code| {
            matches!(
                code,
                "runtime_contract_incompatible" | "binary_architecture_mismatch"
            )
        })
        .then_some(probe.metadata)
        .flatten()
}

fn saved_environment_unchanged(
    before: Option<&webcodex_environment::EnvironmentRecord>,
    after: Option<&webcodex_environment::EnvironmentRecord>,
) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            before.schema_version == after.schema_version
                && before.environment_id == after.environment_id
                && before.request == after.request
                && before.username == after.username
                && before.runner_client_id == after.runner_client_id
                && before.projects == after.projects
                && before.configured == after.configured
        }
        _ => false,
    }
}

fn restart_required(
    upgrade: Option<&webcodex_environment::UpgradeStatus>,
    environment_id: Option<&str>,
    installed: &[MachineBuildInfo],
    desktop: &RunningComponent,
) -> bool {
    desktop.state == "observed"
        && desktop.version.is_some()
        && desktop.git_commit.is_some()
        && upgrade.is_some_and(|upgrade| {
            upgrade.phase == webcodex_environment::UpgradePhase::Committed
                && Some(upgrade.environment_id.as_str()) == environment_id
                && installed.iter().any(|build| {
                    build.binary == "webcodex-desktop"
                        && build.version == upgrade.version
                        && build.git_commit.as_deref() == Some(upgrade.source_sha.as_str())
                        && build.git_dirty == Some(false)
                })
                && (desktop.version.as_deref() != Some(upgrade.version.as_str())
                    || desktop.git_commit.as_deref() != Some(upgrade.source_sha.as_str()))
        })
}

fn service_state(status: &webcodex_environment::service::ServiceStatus) -> &'static str {
    use webcodex_environment::service::Ownership;
    match (status.ownership, status.installed, status.running) {
        (Ownership::Absent, false, _) => "absent",
        (Ownership::Owned, true, Some(true)) => "running",
        (Ownership::Owned, true, Some(false)) => "stopped",
        _ => "unknown",
    }
}

fn install_eligible(
    download: &DownloadStatus,
    context_available: bool,
    service_inventory_complete: bool,
    blockers: &[UpdateBlocker],
) -> bool {
    download.can_install
        && download.installation == InstallationKind::Managed
        && context_available
        && service_inventory_complete
        && download.phase == DownloadPhase::ReadyToInstall
        && blockers.is_empty()
}

fn include_service(
    services: &mut Vec<webcodex_environment::UpgradeServiceComponent>,
    component: webcodex_environment::UpgradeServiceKind,
    scope: webcodex_environment::service::ServiceScope,
    status: &webcodex_environment::service::ServiceStatus,
) -> bool {
    if status.ownership != webcodex_environment::service::Ownership::Owned
        || !status.installed
        || status.running.is_none()
    {
        return false;
    }
    let projected = webcodex_environment::UpgradeServiceComponent { component, scope };
    if !services.contains(&projected) {
        services.push(projected);
    }
    true
}

fn loopback(url: &str) -> bool {
    url::Url::parse(url).ok().is_some_and(|url| {
        matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        )
    })
}

fn standalone_tunnel_service(profile: &webcodex_environment::TunnelRecord) -> bool {
    profile.installed && profile.host_mode == webcodex_environment::TunnelHostMode::Standalone
}

impl AppState {
    pub async fn local_update_status(
        &self,
        inspect_files: bool,
    ) -> DesktopResult<LocalUpdateStatus> {
        let (config, bundled) = {
            let slot = self.core.lock().await;
            let core = slot.as_ref().ok_or_else(action_error)?;
            (
                core.config.clone(),
                core.adapter
                    .bundled_runtime_dir()
                    .map(std::path::Path::to_path_buf),
            )
        };
        let record = local_record(&config);
        let environment_id = record.as_ref().map(|record| record.environment_id.clone());
        let mut installed: Vec<MachineBuildInfo> = Vec::new();
        if inspect_files {
            let cancellation =
                CancellationContext::new(CancellationSignal::new(), self.shutdown_signal.clone());
            let deadline = Deadline::after(Duration::from_secs(10));
            if let Ok((files, _)) = runtime_selection::probe(
                config.runtime_binary_source.clone(),
                bundled.as_deref(),
                config.runtime_selection_revision,
                &cancellation,
                deadline,
            )
            .await
            {
                installed.extend(files.binaries.into_iter().filter_map(verified_file_build));
            }
            if let Ok(desktop) = std::env::current_exe() {
                if let Ok(probe) = runtime_selection::probe_binary(
                    &desktop,
                    "webcodex-desktop",
                    &cancellation,
                    deadline,
                )
                .await
                {
                    if let Some(build) = verified_file_build(probe) {
                        installed.push(build);
                    }
                }
            }
        }

        let installed_checked_at_ms = inspect_files.then(runtime_selection::now_ms);
        let mut running = vec![
            unknown("webcodex", "not_applicable"),
            unknown("webcodex-server", "unknown"),
            unknown("webcodex-runner", "unknown"),
            unknown("webcodex-desktop", "unknown"),
        ];
        let desktop = crate::commands::get_desktop_build_info();
        running[3] = RunningComponent {
            binary: "webcodex-desktop",
            version: Some(desktop.version),
            git_commit: desktop.git_commit,
            git_dirty: desktop.git_dirty,
            state: "observed",
            service_state: "not_applicable",
        };
        let mut observation_error = false;
        let mut active_tasks = None;
        let mut services = Vec::new();
        let mut service_inventory_complete = false;
        if let (Some(record), Some(runtime)) = (&record, &config.runtime) {
            let same_server = webcodex_environment::canonical_server_url(&runtime.server_url)
                .ok()
                .zip(webcodex_environment::canonical_server_url(&record.request.server_url).ok())
                .is_some_and(|(saved, observed)| saved == observed);
            if same_server
                && matches!(record.request.mode, EnvironmentMode::Create { .. })
                && loopback(&runtime.server_url)
            {
                match crate::workspace::query(
                    runtime,
                    crate::workspace::WorkspaceRequest::Overview {},
                )
                .await
                {
                    Ok(value) => {
                        running[1] = observed("webcodex-server", &value);
                    }
                    Err(_) => observation_error = true,
                }
            } else if !record.request.local_server() {
                running[1].state = "not_local";
            }
            if same_server && record.request.local_runner() {
                if let Some(id) = record
                    .runner_client_id
                    .as_ref()
                    .filter(|id| Some(id.as_str()) == stored_runner_client_id(&config).as_deref())
                {
                    let mut exact = runtime.clone();
                    exact.runner_client_id = Some(id.clone());
                    match crate::workspace::query(
                        &exact,
                        crate::workspace::WorkspaceRequest::RunnerDetails {},
                    )
                    .await
                    {
                        Ok(value)
                            if value.get("client_id").and_then(Value::as_str)
                                == Some(id.as_str())
                                && value.get("connected").and_then(Value::as_bool)
                                    == Some(true) =>
                        {
                            running[2] = observed("webcodex-runner", &value);
                        }
                        _ => observation_error = true,
                    }
                }
            } else if !record.request.local_runner() {
                running[2].state = "not_local";
            }
        }
        if let (Some(record), Some(root)) = (
            &record,
            webcodex_environment::default_environment_dir().ok(),
        ) {
            if let Ok(Some(store)) = webcodex_environment::EnvironmentStore::open_existing(root) {
                service_inventory_complete = true;
                if let Ok(native) = webcodex_environment::NativeEnvironment::new() {
                    active_tasks = native
                        .upgrade_task_count(&store, &record.environment_id)
                        .await
                        .ok();
                }
                for (index, component, local_role) in [
                    (
                        1,
                        webcodex_environment::service::Component::Server,
                        record.request.local_server(),
                    ),
                    (
                        2,
                        webcodex_environment::service::Component::Runner,
                        record.request.local_runner(),
                    ),
                ] {
                    running[index].service_state = if !local_role {
                        "not_local"
                    } else {
                        match webcodex_environment::service_spec(&store, record, component)
                            .ok()
                            .and_then(|spec| {
                                webcodex_environment::service::ServiceManager::inspect(&spec)
                                    .ok()
                                    .map(|status| (spec.scope, status))
                            }) {
                            Some((scope, status)) => {
                                let kind = if index == 1 {
                                    webcodex_environment::UpgradeServiceKind::Server
                                } else {
                                    webcodex_environment::UpgradeServiceKind::Runner
                                };
                                service_inventory_complete &=
                                    include_service(&mut services, kind, scope, &status);
                                service_state(&status)
                            }
                            None => {
                                service_inventory_complete = false;
                                "unknown"
                            }
                        }
                    };
                }
                if record.request.local_server() {
                    match webcodex_environment::tunnel_profiles(&store) {
                        Ok(profiles) => {
                            service_inventory_complete &= profiles.len() <= 16;
                            for profile in profiles
                                .into_iter()
                                .take(16)
                                .filter(standalone_tunnel_service)
                            {
                                match webcodex_environment::tunnel_service_spec(
                                    &store,
                                    record,
                                    &profile.profile_id,
                                )
                                .ok()
                                .and_then(|spec| {
                                    webcodex_environment::service::ServiceManager::inspect(&spec)
                                        .ok()
                                        .map(|status| (spec.scope, status))
                                }) {
                                    Some((scope, status)) => {
                                        service_inventory_complete &= include_service(
                                            &mut services,
                                            webcodex_environment::UpgradeServiceKind::Tunnel,
                                            scope,
                                            &status,
                                        )
                                    }
                                    None => service_inventory_complete = false,
                                }
                            }
                        }
                        Err(_) => service_inventory_complete = false,
                    }
                }
            }
        }
        let mut view = self
            .updates
            .status_view(&installed)
            .map_err(|_| action_error())?;
        let (kind, context) = self.update_installation_context().await?;
        view.download.installation = kind;
        view.blockers.retain(|blocker| {
            !matches!(
                blocker,
                UpdateBlocker::UnsupportedInstallation
                    | UpdateBlocker::UnsupportedPlatform
                    | UpdateBlocker::EnvironmentNotConfigured
            )
        });
        match kind {
            InstallationKind::Managed => {}
            InstallationKind::EnvironmentNotConfigured => {
                view.blockers.push(UpdateBlocker::EnvironmentNotConfigured)
            }
            InstallationKind::UnsupportedPlatform => {
                view.blockers.push(UpdateBlocker::UnsupportedPlatform)
            }
            _ => view.blockers.push(UpdateBlocker::UnsupportedInstallation),
        }
        if active_tasks.is_some_and(|count| count > 0) {
            view.blockers.push(UpdateBlocker::ActiveTasks);
        }
        if record.is_some() && active_tasks.is_none() {
            view.blockers
                .push(UpdateBlocker::TaskObservationUnavailable);
        }
        view.download.can_install = install_eligible(
            &view.download,
            context.is_some(),
            service_inventory_complete,
            &view.blockers,
        );
        view.restart_required = restart_required(
            view.upgrade.as_ref(),
            environment_id.as_deref(),
            &installed,
            &running[3],
        );
        let confirmation = view
            .candidate
            .as_ref()
            .filter(|_| {
                view.download.can_install
                    && !view.download.pending_install
                    && view.blockers.is_empty()
            })
            .and_then(|candidate| {
                Some(UpdateConfirmation {
                    candidate: candidate.clone(),
                    target: UpgradeTarget {
                        environment_id: environment_id.clone()?,
                        manifest_sha256: candidate.manifest_sha256.clone(),
                        operation_id: confirmation_operation(view.upgrade.as_ref()),
                    },
                    selection_revision: config.runtime_selection_revision,
                    services: services.clone(),
                    service_inventory_complete,
                })
            });
        // Network observations cannot survive a local selection or identity change.
        let slot = self.core.lock().await;
        if slot.as_ref().is_none_or(|core| {
            core.config.persistent_environment != config.persistent_environment
                || core.config.runtime_selection_revision != config.runtime_selection_revision
                || runner_identity_from_config(&core.config) != runner_identity_from_config(&config)
        }) {
            return Err(action_error());
        }
        // Core task observation fences its own record. This response must also
        // retain the original role/account/service inventory we observed above.
        if !saved_environment_unchanged(record.as_ref(), local_record(&config).as_ref()) {
            return Err(action_error());
        }
        bounded_local_status(LocalUpdateStatus {
            view,
            environment_id,
            selection_revision: config.runtime_selection_revision,
            installed_observed: inspect_files,
            installed_checked_at_ms,
            running,
            confirmation,
            observation_error,
        })
    }
}

#[cfg(test)]
mod tests;
