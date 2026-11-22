//! Completion policy may change only the published-state model. Process cleanup
//! stays with the AppState supervisor, while migration/update coordinators own
//! their durable restoration. No callback receives DesktopCore or its providers.

use super::ProcessCleanup;
use crate::error::{DesktopError, ErrorComponent};
use crate::models::{
    aggregate_readiness, DesktopOperationKind, DesktopStateSnapshot, ExposureReadiness,
    ProjectReadiness, ReadinessSnapshot, RunnerReadiness, ServerReadiness,
};
use crate::runtime_selection::RuntimeSwitchResult;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailureOwner {
    NewProcessDelta,
    DurableCoordinator,
    NoProcessChanges,
}

#[derive(Clone, Copy)]
enum RuntimeErrorPolicy {
    Unchanged,
    ReflectResult,
    PreserveSwitchRecovery,
}

struct CompletionPolicy {
    failure_owner: FailureOwner,
    runtime_error: RuntimeErrorPolicy,
}

impl CompletionPolicy {
    fn for_kind(kind: DesktopOperationKind) -> Self {
        use DesktopOperationKind::*;
        use FailureOwner::*;
        use RuntimeErrorPolicy::*;
        // Exhaustive: a new operation must explicitly select its completion
        // owner instead of silently inheriting generic cleanup.
        let (failure_owner, runtime_error) = match kind {
            EnvironmentInvite => (NoProcessChanges, Unchanged),
            EnvironmentMigration => (DurableCoordinator, ReflectResult),
            DesktopUpdate => (DurableCoordinator, Unchanged),
            RuntimeSwitch => (NewProcessDelta, PreserveSwitchRecovery),
            LocalSetup | RemoteSetup | RuntimeResume | RunnerRestart | EnvironmentService => {
                (NewProcessDelta, ReflectResult)
            }
            LocalProjectActivate | ProjectUnregister | QuickShareStart | QuickShareStop
            | RegularTunnelStart | RegularTunnelStop | LocalRuntimeStop | RuntimeRefresh
            | TunnelProxyUpdate | TunnelConfigUpdate | RunnerSettingsUpdate | RuntimeProbe
            | TraceUpdate | ConfigurationRestore => (NewProcessDelta, Unchanged),
        };
        Self {
            failure_owner,
            runtime_error,
        }
    }
}

pub(super) struct OperationCompletion<'a> {
    kind: DesktopOperationKind,
    error: Option<&'a DesktopError>,
    policy: CompletionPolicy,
}

impl<'a> OperationCompletion<'a> {
    pub(super) fn new(kind: DesktopOperationKind, error: Option<&'a DesktopError>) -> Self {
        Self {
            kind,
            error,
            policy: CompletionPolicy::for_kind(kind),
        }
    }

    pub(super) fn requires_process_cleanup(&self) -> bool {
        self.error.is_some() && self.policy.failure_owner == FailureOwner::NewProcessDelta
    }

    pub(super) fn apply_failure(
        &self,
        snapshot: &mut DesktopStateSnapshot,
        baseline: &DesktopStateSnapshot,
        cleanup: Option<ProcessCleanup>,
    ) -> bool {
        if self.policy.failure_owner == FailureOwner::NoProcessChanges {
            return false;
        }
        debug_assert_eq!(cleanup.is_some(), self.requires_process_cleanup());
        let Some(error) = self.error else {
            return false;
        };
        let cancelled = error.code == "desktop_operation_cancelled";
        if let Some(cleanup) = cleanup {
            reconcile_after_operation_failure(snapshot, self.kind, baseline, cleanup, cancelled);
        }
        // A durable coordinator has already restored the original process or
        // published an unknown-result state. Never reclaim that generation here.
        terminalize_failed_start(snapshot, cancelled);
        true
    }

    pub(super) fn apply_runtime_error(
        &self,
        snapshot: &mut DesktopStateSnapshot,
        last_switch: Option<&RuntimeSwitchResult>,
    ) -> bool {
        // A successful RuntimeRefresh is fresh native evidence that one or more
        // components recovered. Clear only resolved transient component errors;
        // unrelated components, uncertain outcomes, and ownership conflicts survive.
        if self.kind == DesktopOperationKind::RuntimeRefresh && self.error.is_none() {
            return reconcile_runtime_errors_after_observation(snapshot);
        }
        match self.policy.runtime_error {
            RuntimeErrorPolicy::Unchanged => return false,
            RuntimeErrorPolicy::ReflectResult | RuntimeErrorPolicy::PreserveSwitchRecovery => {}
        }
        if let Some(error) = self.error {
            if error.code == "desktop_operation_cancelled" {
                // Cancellation is not a new component failure and cannot
                // invalidate unrelated errors restored from the baseline.
                return false;
            }
            snapshot
                .component_errors
                .insert(error.component(), error.clone());
            snapshot.runtime_error = Some(error.clone());
        } else {
            let preserve = matches!(
                self.policy.runtime_error,
                RuntimeErrorPolicy::PreserveSwitchRecovery
            ) && last_switch.is_some_and(|switch| {
                matches!(switch.outcome.as_str(), "rolled_back" | "recovery_required")
            });
            if preserve {
                return false;
            }
            return match self.kind {
                // An explicit setup, migration or successful environment switch
                // establishes a different owner/identity. Prior errors belong to
                // the replaced environment, not the new one.
                DesktopOperationKind::LocalSetup
                | DesktopOperationKind::RemoteSetup
                | DesktopOperationKind::EnvironmentMigration
                | DesktopOperationKind::RuntimeSwitch => {
                    let changed =
                        snapshot.runtime_error.is_some() || !snapshot.component_errors.is_empty();
                    snapshot.component_errors.clear();
                    snapshot.runtime_error = None;
                    changed
                }
                // Starting/restarting a component is not proof that all other
                // components recovered. Only the observed readiness can retire
                // each transient failure, including after a successful restart.
                DesktopOperationKind::RuntimeResume
                | DesktopOperationKind::RunnerRestart
                | DesktopOperationKind::EnvironmentService => {
                    reconcile_runtime_errors_after_observation(snapshot)
                }
                _ => false,
            };
        }
        true
    }
}

fn is_auto_clearable_component_error(error: &DesktopError, readiness: &ReadinessSnapshot) -> bool {
    // Whether the *failed operation* may be retried is distinct from whether a
    // fresh, authoritative health observation proves its failure has recovered.
    // For example, a port-conflict start must not auto-retry, but Server=Ready
    // resolves the stale alert after the port conflict has been fixed.
    match error.code.as_str() {
        "runner_offline" | "runner_not_ready" | "runner_start_failed" | "runner_exited" => {
            readiness.runner == RunnerReadiness::Ready
        }
        "server_unreachable"
        | "server_not_ready"
        | "server_start_failed"
        | "server_exited"
        | "workspace_server_unreachable" => readiness.server == ServerReadiness::Ready,
        "project_not_loaded" => readiness.project == ProjectReadiness::Ready,
        "runtime_not_ready" => readiness.runtime_ready,
        _ => false,
    }
}

fn primary_component_error(snapshot: &DesktopStateSnapshot) -> Option<DesktopError> {
    const PRIORITY: [ErrorComponent; 12] = [
        ErrorComponent::Config,
        ErrorComponent::Migration,
        ErrorComponent::Runtime,
        ErrorComponent::Service,
        ErrorComponent::Server,
        ErrorComponent::Auth,
        ErrorComponent::Runner,
        ErrorComponent::Project,
        ErrorComponent::Tunnel,
        ErrorComponent::Environment,
        ErrorComponent::Desktop,
        ErrorComponent::Unknown,
    ];
    for component in PRIORITY {
        if let Some(error) = snapshot.component_errors.get(&component) {
            return Some(error.clone());
        }
    }
    None
}

pub(super) fn reconcile_runtime_errors_after_observation(
    snapshot: &mut DesktopStateSnapshot,
) -> bool {
    let mut changed = false;
    let readiness = snapshot.readiness.clone();
    let stale_components: Vec<ErrorComponent> = snapshot
        .component_errors
        .iter()
        .filter(|(_, error)| is_auto_clearable_component_error(error, &readiness))
        .map(|(component, _)| *component)
        .collect();
    for component in stale_components {
        snapshot.component_errors.remove(&component);
        changed = true;
    }
    if snapshot
        .runtime_error
        .as_ref()
        .is_some_and(|error| is_auto_clearable_component_error(error, &readiness))
    {
        snapshot.runtime_error = primary_component_error(snapshot);
        changed = true;
    }
    changed
}

pub(super) fn terminalize_failed_start(snapshot: &mut DesktopStateSnapshot, cancelled: bool) {
    let server = if snapshot.readiness.server == ServerReadiness::Starting {
        if cancelled {
            ServerReadiness::Stopped
        } else {
            ServerReadiness::Error
        }
    } else {
        snapshot.readiness.server.clone()
    };
    let runner = if snapshot.readiness.runner == RunnerReadiness::Connecting {
        if cancelled {
            RunnerReadiness::Stopped
        } else {
            RunnerReadiness::Error
        }
    } else {
        snapshot.readiness.runner.clone()
    };
    let exposure = if snapshot.readiness.exposure == ExposureReadiness::Starting {
        if cancelled {
            ExposureReadiness::Disabled
        } else {
            ExposureReadiness::Error
        }
    } else {
        snapshot.readiness.exposure.clone()
    };
    snapshot.readiness =
        aggregate_readiness(server, runner, exposure, snapshot.readiness.project.clone());
}

pub(super) fn reconcile_after_operation_failure(
    snapshot: &mut DesktopStateSnapshot,
    kind: DesktopOperationKind,
    baseline: &DesktopStateSnapshot,
    cleanup: ProcessCleanup,
    cancelled: bool,
) {
    let observed_binaries = snapshot.binaries.clone();
    match kind {
        DesktopOperationKind::RuntimeResume => {
            // Resume is a desired-state replay of an already committed setup.
            // If any step fails or is cancelled, newly owned processes have
            // already been reclaimed above; restore the last published view
            // rather than leaving a synthetic "starting" state behind.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::QuickShareStart => {
            // Quick Share is intentionally ephemeral. Any failed start has
            // already stopped (or will have cleanup stop) the newly owned
            // foreground process, so return the public topology to the
            // last committed runtime instead of leaving "starting" behind.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::RegularTunnelStart if cancelled => {
            // User cancellation is not a tunnel failure. Restore the last
            // observed full-runtime state after the exact owned tunnel is
            // reclaimed rather than publishing a synthetic tunnel error.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::RuntimeRefresh if cancelled => {
            // A cancelled observation must not partially overwrite the
            // last published control-plane state.
            *snapshot = baseline.clone();
        }
        DesktopOperationKind::LocalSetup => {
            let server = if cleanup.local_server {
                ServerReadiness::Stopped
            } else if snapshot.readiness.server == ServerReadiness::Starting {
                baseline.readiness.server.clone()
            } else {
                snapshot.readiness.server.clone()
            };
            let runner = if cleanup.local_runner {
                RunnerReadiness::Stopped
            } else if snapshot.readiness.runner == RunnerReadiness::Connecting {
                baseline.readiness.runner.clone()
            } else {
                snapshot.readiness.runner.clone()
            };
            let project = if cleanup.local_server || cleanup.local_runner {
                snapshot
                    .project
                    .as_ref()
                    .map(|_| ProjectReadiness::Configured)
                    .unwrap_or(ProjectReadiness::None)
            } else {
                snapshot.readiness.project.clone()
            };
            snapshot.readiness =
                aggregate_readiness(server, runner, snapshot.readiness.exposure.clone(), project);
        }
        DesktopOperationKind::RemoteSetup => {
            let runner = if cleanup.local_runner {
                RunnerReadiness::Stopped
            } else if snapshot.readiness.runner == RunnerReadiness::Connecting {
                baseline.readiness.runner.clone()
            } else {
                snapshot.readiness.runner.clone()
            };
            let project = if cleanup.local_runner {
                snapshot
                    .project
                    .as_ref()
                    .map(|_| ProjectReadiness::Configured)
                    .unwrap_or(ProjectReadiness::None)
            } else {
                snapshot.readiness.project.clone()
            };
            snapshot.readiness = aggregate_readiness(
                snapshot.readiness.server.clone(),
                runner,
                snapshot.readiness.exposure.clone(),
                project,
            );
        }
        _ => {}
    }
    if observed_binaries.is_some() {
        snapshot.binaries = observed_binaries;
    }
}

#[cfg(test)]
mod tests;
